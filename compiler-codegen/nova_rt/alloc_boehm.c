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

/* [M-gc-thread-register-duplicate-unregister]: per-thread flag — did THIS
 * thread succeed at an EXPLICIT GC_register_my_thread? Only such a thread
 * may call GC_unregister_my_thread (gc.h contract). A thread bdwgc already
 * registered implicitly (Windows CreateThread under GC_THREADS) reports
 * GC_DUPLICATE from register and MUST NOT be explicitly unregistered:
 * the unregister then removes a thread the collector still owns, corrupting
 * its thread table / free lists — observed as SIGSEGV in
 * GC_print_free_list <- GC_generic_malloc_many <- GC_malloc_kind <- nova_alloc
 * on the merged conformance CU (0xC0000005). */
#if defined(_WIN32)
static __declspec(thread) int _nova_gc_explicitly_registered = 0;
#else
static __thread int _nova_gc_explicitly_registered = 0;
#endif

int nova_gc_register_my_thread(void) {
    struct GC_stack_base sb;
    if (GC_get_stack_base(&sb) != GC_SUCCESS) return 0;
    int rc = GC_register_my_thread(&sb);
    _nova_gc_explicitly_registered = (rc == GC_SUCCESS);
    return _nova_gc_explicitly_registered;
}

void nova_gc_unregister_my_thread(void) {
    if (_nova_gc_explicitly_registered) {
        GC_unregister_my_thread();
        _nova_gc_explicitly_registered = 0;
    }
}

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

/* 221.1 №1461: инкрементальный режим — ТОЛЬКО там, где ядро само следит за
 * записью в страницы кучи.
 *
 * Инкрементальный Boehm узнаёт, какие страницы кучи изменились с прошлого
 * марка, одним из трёх способов: GetWriteWatch (Windows, GWW_VDB), биты
 * soft-dirty ядра Linux (SOFT_VDB, libgc >= 8.2) или ЗАЩИТА СТРАНИЦ: куча
 * помечается PROT_READ, первая запись ловится SIGSEGV-обработчиком, который
 * снимает защиту и ставит бит (MPROTECT_VDB). Третий способ видит только
 * записи ИЗ ПОЛЬЗОВАТЕЛЬСКОГО КОДА. Запись в защищённую страницу из ЯДРА —
 * `read`/`pread`/`recv` в буфер, выделенный `nova_alloc`, — сигнала не
 * порождает: системный вызов возвращает EFAULT. А `nova_alloc` = `GC_malloc`
 * без `_atomic`, то есть ВСЕ байтовые буферы рантайма лежат в защищаемых
 * страницах.
 *
 * Замер (облачная сессия №1461, Linux 6.18 без CONFIG_MEM_SOFT_DIRTY, libgc
 * 8.2.6): `novac check display_generic/pos_1.nv` под `setarch -R` 19/20
 * неверно; strace — `pread64(…, 0x7ffff7898000, 8192, 8192) = -1 EFAULT`
 * сразу после марка (чтение /proc/self/maps колбэком корней), файл
 * `std/src/collections/vec/core.nv` недочитан, и Карина отвечает «`Vec` is
 * not among the declarations». `GC_incremental_protection_needs()` на этой
 * машине = GC_PROTECTS_POINTER_HEAP. Живые объекты НЕ терялись — терялись
 * байты, которые ядро не смогло записать.
 *
 * Где способ третий: Linux без soft-dirty (arm64 его не имеет вовсе, часть
 * облачных ядер собрана без него), libgc < 8.2 (Ubuntu 22.04 — 8.0), macOS
 * (mach-исключения поверх mprotect). Там включать инкрементальный режим
 * НЕЛЬЗЯ, а выключить его после `GC_enable_incremental` Boehm не умеет —
 * поэтому решение принимается ДО вызова.
 *
 * Задача #21 (2026-10-06): решение принимает САМА libgc, а не наша догадка о
 * ней. Первый фикс угадывал её выбор пробой ядра (clear_refs + бит 55
 * pagemap), но способ слежения выбирает не ядро, а libgc, и её выбор зависит
 * ещё от СБОРКИ (gcconfig.h включает SOFT_VDB только при glibc — под musl,
 * то есть Alpine, его нет; NO_SOFT_VDB, GC_PREFER_MPROTECT_VDB) и от
 * окружения (`GC_USE_GETWRITEWATCH=0`). Замер (WSL, ядро 6.6 С soft-dirty,
 * libgc 8.2.8, собранная с -DNO_SOFT_VDB): проба ядра говорила «да», libgc
 * брала mprotect, и `novac check display_generic/pos_1.nv` под `setarch -R`
 * был верен 9 раз из 20. Поэтому на Linux вопрос задаётся libgc в
 * дочернем процессе: fork, там `GC_enable_incremental` при выключенной сборке
 * (`GC_disable` — ни одного марка, только выбор способа) и ответ
 * `GC_incremental_protection_needs()`; родитель включает режим, только если
 * ребёнок сказал GC_PROTECTS_NONE. На старте процесс однопоточный (марк-
 * потоки libgc спят), ребёнок выходит через `_exit` и снимается `alarm`,
 * если зависнет; любой сбой пробы — полный сборщик.
 *
 * Windows: libgc наша (nova_rt/gc, GWW_VDB + MPROTECT_VDB, без
 * GC_PREFER_MPROTECT_VDB), значит выбор зависит только от
 * `GC_USE_GETWRITEWATCH` — его и читаем, как читает libgc.
 *
 * `NOVA_GC_INCREMENTAL=0` — выключить всегда (как раньше); `=1` — включить
 * БЕЗУСЛОВНО, в том числе поверх защиты страниц: это ключ пробы в обе
 * стороны на одном бинаре, не режим для работы. */
#if defined(__linux__)
#  include <signal.h>
#  include <unistd.h>
#  include <sys/wait.h>
#  include <errno.h>
static int _nova_gc_incremental_needs_no_protection(void) {
    /* Унаследованный SIG_IGN для SIGCHLD заставил бы ядро само убрать
     * ребёнка, и waitpid не узнал бы ответа — на время пробы SIG_DFL. */
    struct sigaction dfl, old_chld;
    memset(&dfl, 0, sizeof dfl);
    dfl.sa_handler = SIG_DFL;
    sigemptyset(&dfl.sa_mask);
    sigaction(SIGCHLD, &dfl, &old_chld);
    int ok = 0;
    pid_t pid = fork();
    if (pid == 0) {
        sigaction(SIGALRM, &dfl, NULL);
        alarm(5);
        GC_disable();
        GC_enable_incremental();
        _exit(!GC_is_incremental_mode() ? 2
              : GC_incremental_protection_needs() == GC_PROTECTS_NONE ? 0 : 1);
    }
    if (pid > 0) {
        int st = 0;
        pid_t r;
        while ((r = waitpid(pid, &st, 0)) < 0 && errno == EINTR) {}
        ok = r == pid && WIFEXITED(st) && WEXITSTATUS(st) == 0;
    }
    sigaction(SIGCHLD, &old_chld, NULL);
    return ok;
}
#endif

static int _nova_gc_incremental_wanted(void) {
    const char* e = getenv("NOVA_GC_INCREMENTAL");
    if (e && strcmp(e, "0") == 0) return 0;
    if (e && strcmp(e, "1") == 0) return 1;
#if defined(_WIN32)
    /* GWW_VDB: ядро ведёт журнал записи, страницы не защищаются — если libgc
     * не велели обойтись без него (тот же ключ, что она читает сама). */
    const char* g = getenv("GC_USE_GETWRITEWATCH");
    return !(g && strcmp(g, "0") == 0);
#elif defined(__linux__)
    return _nova_gc_incremental_needs_no_protection();
#else
    return 0;   /* macOS и прочие: только защита страниц */
#endif
}

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

    /* 221.1 №474 (родитель №470): сократить сами STW-паузы, не подделывать
     * дедлайн-часы (запрет интегратора — компенсация паузы в сравнении с
     * `_dl_ns` прячет симптом, а не чинит его).
     *
     * (a) Инкрементальный сборщик — переключает Boehm на generational
     * write-barrier mark вместо полного stop-the-world марка; каждая
     * отдельная пауза короче (мельче инкременты), хотя пауз может стать
     * больше числом. Замер (getenv A/B, гистограммой пауз) на этой же
     * машине шумный (см. реестр №474 — параллельные cargo/rustc из других
     * окон эту же машину постоянно грузят, `scripts/tools/measure.sh`
     * отказывает мерить бОльшую часть сессии), но НЕ показал регресса ни на
     * одной чистой выборке — включаем ПО УМОЛЧАНИЮ (не только для замера);
     * `NOVA_GC_INCREMENTAL=0` — явный откат на полный сборщик, если для
     * какой-то нагрузки инкрементальный вдруг окажется хуже. С №1461 —
     * не везде: только где Boehm следит за записью без защиты страниц
     * (`_nova_gc_incremental_wanted` выше и его комментарий).
     *
     * (b) Ранний прогрев кучи (`GC_expand_hp`) — избегает СЕРИИ мелких
     * grow-and-collect циклов в начале процесса (типичны для supervised-
     * timeout-тяжёлых тестов/серверов: Channel.new/AtomicBool.new/
     * NovaEarlyDl на каждый scope). 4 МиБ — на два порядка больше типичной
     * кучи маленькой тестовой программы, но пренебрежимо мало против
     * серверного бюджета памяти; безопасный, обратимый параметр, НЕ меняет
     * ничьё наблюдаемое поведение, кроме частоты триггера коллектора. */
    if (_nova_gc_incremental_wanted()) {
        GC_enable_incremental();
    }
    GC_expand_hp(4 * 1024 * 1024);

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
    GC_gcollect();
#endif
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

/* 221.1 №1420: pin `p` (and everything reachable from it) for the rest of the
 * process. The pin is a one-pointer GC_malloc_uncollectable cell that is never
 * freed: Boehm always marks uncollectable objects and scans them for
 * pointers, so the cell is a root on its own -- no list, no lock, no
 * per-thread bookkeeping, and it does not matter which thread's slot, fiber
 * snapshot or register the pointer is copied into later.
 *
 * Used for objects whose only other reference is a thread-local slot: under
 * GC_set_no_dls(1) (nova_gc_init) a TLS block is not a root -- measured on
 * Linux for the main thread (a worker's static TLS happens to sit inside its
 * scanned pthread stack block), reported on Windows for every thread -- so
 * such an object is collected by the first sweep. The one user today is the lazy
 * `#default_handler` install in the generated effect dispatchers
 * (`_nova_handler_<E> = nova_gc_pin(<ctor>())`, emit_effect_type): it runs
 * once per (thread, effect), so the retained memory is bounded by
 * threads x effects, and a default handler lives as long as its thread
 * anyway. Not for per-call objects -- the cell is never released. */
void* nova_gc_pin(void* p) {
    if (!p) return p;
    void** cell = (void**)GC_malloc_uncollectable(sizeof(void*));
    if (!cell) {
        fprintf(stderr, "nova: out of memory (gc pin)\n");
        fflush(stdout);
        fflush(stderr);
        abort();
    }
    *cell = p;
    return p;
}

void nova_free_uncollectable(void* ptr) {
    if (!ptr) return;
    /* [M-mn-spawnctx-corruption-cancel-wake] дискриминатор (opt-in,
     * NOVA_UNCOLL_QUAR=1): вместо GC_free — poison 0xDD + осознанная утечка.
     * Если краш исчезает под этим флагом без иных изменений — порча течёт
     * через реюз какого-то released-uncollectable блока (сюда попадают
     * ctx_pins[], effect-snapshots, sync-примитивы и т.д.).
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
