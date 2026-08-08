/* nova_rt/alloc_boehm.c — Boehm GC implementation.
 *
 * Full tracing GC: collects cycles, concurrent mark (on platforms that support it).
 * Matches Nova spec D6: managed heap, programmer never calls free.
 *
 * To use: compile with this file instead of alloc.c or alloc_rc.c, and link gc.lib.
 *   cl.exe ... nova_rt\alloc_boehm.c /I<vcpkg_installed\x64-windows-static\include>
 *             /link <vcpkg_installed\x64-windows-static\lib\gc.lib>
 *                   <vcpkg_installed\x64-windows-static\lib\atomic_ops.lib>
 *
 * nova_retain / nova_release are no-ops — GC handles everything automatically.
 *
 * Contract: nova_alloc MUST return zeroed memory. GC_malloc already satisfies
 * this (Boehm API guarantee). No memset needed.
 *
 * Stat functions: nova_gc_live_count / nova_gc_free_count are approximations —
 * exact live count requires finalizer cooperation which Boehm does not provide.
 * _alloc_count is an upper bound; GC may have freed some objects since. */

#include "alloc.h"

/* GC_THREADS defined by -DGC_THREADS compile flag (Plan 44.5): exposes
 * GC_register_my_thread / GC_allow_register_threads for M:N workers. */
#include <gc.h>

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>  /* getenv — NOVA_UNCOLL_QUAR дискриминатор */
#include <string.h>  /* memset — poison */

/* Monotonic alloc counter — incremented on every nova_alloc call.
 * Used by nova_gc_alloc_count() and nova_gc_reset_stats().
 *
 * [M-211-alloc-count-rmw-race] (2026-07-17, TSan-confirmed via Plan 211
 * mn_smoke): armed M:N spawns nova_alloc concurrently from multiple worker
 * threads (nova_scope_alloc_slot on fiber preamble) — a plain `_alloc_count++`
 * is a non-atomic read-modify-write raced by every concurrent allocator
 * thread. Not GC-correctness-affecting (Boehm's own bookkeeping is unrelated
 * to this stat), but formally UB and lost increments are possible under
 * contention. Fixed with relaxed atomics — same discipline as
 * `_nova_runq_diag_inc` (runq.h): counter value ordering doesn't matter,
 * only that the RMW itself is atomic. Zero cost (single instruction on
 * x86/ARM, no barrier needed for RELAXED). */
static size_t _alloc_count = 0;

/* Plan 57.C.2: last GC pause длительность в наносекундах (monotonic timer
 * wraps GC_gcollect). Updated в nova_gc_collect; consumers (bench, gc.last_pause_ns)
 * читают через nova_gc_last_pause_ns(). */
static uint64_t _last_pause_ns = 0;

/* High-res timer для pause measurement. На Windows — QueryPerformanceCounter,
 * на Linux/macOS — clock_gettime(CLOCK_MONOTONIC). */
#if defined(_WIN32)
#  define WIN32_LEAN_AND_MEAN
#  include <windows.h>
static uint64_t _now_ns(void) {
    static LARGE_INTEGER freq = {0};
    LARGE_INTEGER c;
    if (freq.QuadPart == 0) QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&c);
    uint64_t secs = (uint64_t)c.QuadPart / (uint64_t)freq.QuadPart;
    uint64_t rem  = (uint64_t)c.QuadPart % (uint64_t)freq.QuadPart;
    return secs * 1000000000ULL + rem * 1000000000ULL / (uint64_t)freq.QuadPart;
}
#else
#  include <time.h>
static uint64_t _now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}
#endif

extern void _nova_install_segv_handler(void);  /* Plan 83.11 §12.31 — segv_diag.c */

void nova_gc_init(void) {
    /* Plan 83.11 §12.31: install in-process SEGV localizer FIRST (before any
     * potentially-faulting init). Gated by NOVA_DIAG_SEGV env. No-op on Linux. */
    _nova_install_segv_handler();

    /* Plan 44.2 Etap 1 wire-up fix (2026-05-12): Boehm/Docker hardening
     * before GC_INIT().
     *
     * GC_set_no_dls(1) — skip dynamic-libraries data-segment scan.
     * Without this Boehm's GC_init_linux_data_start() walks /proc/self/maps
     * to detect data segment; under Docker restricted permissions /proc walk
     * returns inconsistent results → SEGV в GC_find_limit_with_bound during
     * GC_init.
     *
     * Nova statically links its runtime — dynamic library roots не нужны.
     * Только main binary data segment + Plan 44.2 fiber arena ranges + heap. */
    GC_set_no_dls(1);

    GC_INIT();
    /* Allow GC to run finalisers / collect aggressively.
     *
     * [M-boehm-large-buffer-retention-fiber-reuse] DISCRIMINATOR (env-gated,
     * default = historical behavior, zero overhead): interior pointers make
     * ANY conservatively-scanned word that points ANYWHERE inside a heap
     * object retain the whole object — so a stale stack word landing inside a
     * KB-scale buffer retains it (retention ∝ buffer size). NOVA_GC_NO_INTERIOR=1
     * turns them off to measure how much of the residual leak is interior-
     * pointer amplification vs. base-pointer hits. DIAGNOSTIC ONLY — Nova `[]T`
     * slice views point into the middle of a Vec backing, so turning interior
     * pointers off is NOT correctness-preserving in general. */
    {
        const char* e = getenv("NOVA_GC_NO_INTERIOR");
        GC_set_all_interior_pointers((e && e[0] == '1') ? 0 : 1);
    }
}

void nova_gc_shutdown(void) {
    int _nv456_diag = getenv("NOVA_DIAG_M456") != NULL;
    if (_nv456_diag) { fprintf(stderr, "[m456] gc_shutdown ENTER\n"); fflush(stderr); }
    /* Plan 44.2 Etap 1 (2026-05-12): skip final GC_gcollect on Linux only.
     *
     * Under Ubuntu 22.04 system libgc (built с PARALLEL_MARK), GC_gcollect
     * на shutdown триггерит parallel marker threads. Под Docker thread
     * stack walks могут fail → SEGV в GC_do_local_mark / GC_do_parallel_mark.
     *
     * На Windows/macOS наш vcpkg-собранный libgc не использует PARALLEL_MARK
     * и финальный collect нужен для корректного teardown background handles
     * (libuv timers, channels). Без него ASAN/Valgrind видят утечки и
     * некоторые tests падают на shutdown с access violation. */
#if defined(__linux__)
    /* GC_gcollect(); — disabled под Linux Docker */
#else
    if (_nv456_diag) { fprintf(stderr, "[m456] gc_shutdown before GC_gcollect\n"); fflush(stderr); }
    GC_gcollect();
    if (_nv456_diag) { fprintf(stderr, "[m456] gc_shutdown after GC_gcollect\n"); fflush(stderr); }
#endif
    if (_nv456_diag) { fprintf(stderr, "[m456] gc_shutdown RETURNING\n"); fflush(stderr); }
}

void* nova_alloc(size_t size) {
    void* p = GC_malloc(size);
    if (!p) {
        fprintf(stderr, "nova: out of memory\n");
        /* #278 [M-nova-alloc-abort-no-fflush]: flush BOTH streams before
         * abort() — see the matching comment in alloc.c's nova_alloc for
         * the full rationale (buffered stdout output lost on crash). */
        fflush(stdout);
        fflush(stderr);
        abort();
    }
    __atomic_fetch_add(&_alloc_count, 1, __ATOMIC_RELAXED);
    return p;
}

/* Plan 83.4.5.8 (2026-05-24): uncollectable allocation — backing
 * via GC_malloc_uncollectable. Под Boehm с GC_THREADS такая память
 * никогда не reclaimed sweep'ом + автоматически scanned for pointers
 * (поведение GC_malloc, но с persisted lifetime).
 *
 * Use case — SpawnCtx под armed M:N: main thread alloc + write
 * fields; worker thread reads через mco_get_user_data. GC race
 * между write и read (даже с ctx_pins) на Windows fiber arena
 * приводит к worker-side reading zeros. Uncollectable полностью
 * обходит проблему — memory гарантированно сохраняется до явного
 * free.
 *
 * Contract: zero-initialized (GC_malloc_uncollectable returns
 * zero-init memory per Boehm API). Caller MUST nova_free_uncollectable
 * для избежания leak. */
void* nova_alloc_uncollectable(size_t size) {
    void* p = GC_malloc_uncollectable(size);
    if (!p) {
        fprintf(stderr, "nova: out of memory (uncollectable)\n");
        /* #278: see nova_alloc's matching comment above. */
        fflush(stdout);
        fflush(stderr);
        abort();
    }
    __atomic_fetch_add(&_alloc_count, 1, __ATOMIC_RELAXED);
    return p;
}

/* Plan 152.4: register [lo, hi) as a GC root. Needed because GC_set_no_dls(1)
 * (see nova_gc_init) leaves the program's static/BSS data unscanned, so a
 * module-level lazy-static `static T* _value;` would otherwise not be a root
 * and its (possibly large) object graph would be collected under pressure. */
void nova_gc_add_root(void* lo, void* hi) {
    GC_add_roots((char*)lo, (char*)hi);
}

void nova_free_uncollectable(void* ptr) {
    if (!ptr) return;
    /* [M-mn-spawnctx-corruption-cancel-wake] дискриминатор (opt-in,
     * NOVA_UNCOLL_QUAR=1): вместо GC_free — poison 0xDD + осознанная утечка.
     * Если краш исчезает под этим флагом без иных изменений — порча течёт
     * через реюз какого-то released-uncollectable блока (SpawnCtx-пул уже
     * закрыт отдельным NOVA_SPAWN_POOL_DIAG-карантином; сюда попадают
     * остальные: ctx_pins[], effect-snapshots, sync-примитивы и т.д.).
     * Читатель stale-указателя получает детерминированный 0xDD-паттерн
     * вместо случайного мусора. Ноль оверхеда без env (кеш-бранч). */
    {
        static int _quar = -1;
        int q = __atomic_load_n(&_quar, __ATOMIC_RELAXED);
        if (q < 0) {
            const char* e = getenv("NOVA_UNCOLL_QUAR");
            q = (e && e[0] == '1') ? 1 : 0;
            __atomic_store_n(&_quar, q, __ATOMIC_RELAXED);
        }
        if (q) {
            size_t sz = GC_size(ptr);
            if (sz > 0) memset(ptr, 0xDD, sz);
            return;
        }
    }
    GC_free(ptr);
}

/* RC ops are no-ops under Boehm — GC traces references automatically */
void nova_retain(void* ptr)  { (void)ptr; }
void nova_release(void* ptr) { (void)ptr; }

/* Stat functions required by alloc.h. Boehm does not expose per-object
 * freed/live counts without finalizers; we use heap_size as a proxy.
 * Conservative: nova_gc_free_count returns 0 (never overclaims). */
size_t nova_gc_alloc_count(void) { return __atomic_load_n(&_alloc_count, __ATOMIC_RELAXED); }
size_t nova_gc_free_count(void)  { return 0; /* conservative: GC freed count unavailable */ }
size_t nova_gc_live_count(void)  { return __atomic_load_n(&_alloc_count, __ATOMIC_RELAXED); /* upper bound; GC may have freed some */ }
void   nova_gc_reset_stats(void) { __atomic_store_n(&_alloc_count, 0, __ATOMIC_RELAXED); }

/* Plan 32: introspection — under Boehm full GC support.
 * Plan 57.C.2: nova_gc_collect timed; last_pause_ns updated. */
size_t   nova_gc_heap_size(void) { return GC_get_heap_size(); }
void     nova_gc_collect(void)   {
    uint64_t t0 = _now_ns();
    GC_gcollect();
    _last_pause_ns = _now_ns() - t0;
}
uint64_t nova_gc_last_pause_ns(void) { return _last_pause_ns; }
