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

/* ─── 221.1 №474 (родитель №470): Boehm STW-пауза — гистограмма длительностей ───
 *
 * getenv-gated (`NOVA_GC_PAUSE_DIAG=1`) диагностика: колбэк
 * `GC_set_on_collection_event` РЕГИСТРИРУЕТСЯ только когда флаг взведён (не
 * getenv() на каждое событие — сам колбэк тоже холодный путь, STW-паузы редки
 * по определению) — обычная сборка платит ровно ОДИН `getenv()` при
 * `nova_gc_init()` и больше ничего. Бракетируем `GC_EVENT_PRE_STOP_WORLD`
 * (мир вот-вот встанет) .. `GC_EVENT_POST_START_WORLD` (мир снова бежит) —
 * это и есть интервал, в течение которого ВСЕ файберы/воркеры физически не
 * могли продвинуться, включая cooperative-планировщик Vela и join-loop
 * дедлайн-гейт `nova_supervised_run_impl` (см. реестр №470: дедлайн
 * вычисляется и сравнивается корректно, но между arm и check реально
 * проходит больше времени, чем запланировано — эта гистограмма даёт числа
 * ДЛЯ ЭТОГО расхождения, а не гипотезу). Один активный STW за раз
 * (Boehm сериализует запуск коллектора), поэтому `_stw_start_ns` — простая
 * не-atomic переменная, без гонки писателей. */
#define NOVA_GC_PAUSE_BUCKETS 10
static const double _nova_gc_pause_bucket_ms[NOVA_GC_PAUSE_BUCKETS - 1] = {
    1, 2, 5, 10, 25, 50, 100, 250, 500
};
static uint64_t _nova_gc_pause_hist[NOVA_GC_PAUSE_BUCKETS];
static uint64_t _nova_gc_pause_count = 0;
static uint64_t _nova_gc_pause_total_ns = 0;
static uint64_t _nova_gc_pause_max_ns = 0;
static uint64_t _nova_gc_pause_stw_start_ns = 0;

static void _nova_gc_pause_event_cb(GC_EventType ev) {
    if (ev == GC_EVENT_PRE_STOP_WORLD) {
        _nova_gc_pause_stw_start_ns = _now_ns();
    } else if (ev == GC_EVENT_POST_START_WORLD) {
        uint64_t start = _nova_gc_pause_stw_start_ns;
        if (start == 0) return;  /* defensive: mismatched event pair */
        uint64_t dur_ns = _now_ns() - start;
        double dur_ms = (double)dur_ns / 1e6;
        int bucket = NOVA_GC_PAUSE_BUCKETS - 1;
        for (int i = 0; i < NOVA_GC_PAUSE_BUCKETS - 1; i++) {
            if (dur_ms < _nova_gc_pause_bucket_ms[i]) { bucket = i; break; }
        }
        __atomic_fetch_add(&_nova_gc_pause_hist[bucket], 1, __ATOMIC_RELAXED);
        __atomic_fetch_add(&_nova_gc_pause_count, 1, __ATOMIC_RELAXED);
        __atomic_fetch_add(&_nova_gc_pause_total_ns, dur_ns, __ATOMIC_RELAXED);
        uint64_t cur_max = __atomic_load_n(&_nova_gc_pause_max_ns, __ATOMIC_RELAXED);
        while (dur_ns > cur_max &&
               !__atomic_compare_exchange_n(&_nova_gc_pause_max_ns, &cur_max, dur_ns,
                                             0, __ATOMIC_RELAXED, __ATOMIC_RELAXED)) {}
        _nova_gc_pause_stw_start_ns = 0;
    }
}

static void _nova_gc_pause_diag_dump(void) {
    if (_nova_gc_pause_count == 0) {
        fprintf(stderr, "[gc-pause-diag] no STW pauses recorded\n");
        return;
    }
    fprintf(stderr,
        "[gc-pause-diag] count=%llu total_ms=%.2f max_ms=%.2f mean_ms=%.3f\n",
        (unsigned long long)_nova_gc_pause_count,
        (double)_nova_gc_pause_total_ns / 1e6,
        (double)_nova_gc_pause_max_ns / 1e6,
        (double)_nova_gc_pause_total_ns / 1e6 / (double)_nova_gc_pause_count);
    static const char* labels[NOVA_GC_PAUSE_BUCKETS] = {
        "<1ms", "<2ms", "<5ms", "<10ms", "<25ms", "<50ms", "<100ms", "<250ms",
        "<500ms", ">=500ms"
    };
    for (int i = 0; i < NOVA_GC_PAUSE_BUCKETS; i++) {
        if (_nova_gc_pause_hist[i] > 0) {
            fprintf(stderr, "[gc-pause-diag]   %-8s %llu\n", labels[i],
                    (unsigned long long)_nova_gc_pause_hist[i]);
        }
    }
    fflush(stderr);
}

/* Cross-TU snapshot for correlating a specific event (e.g. a supervised
 * deadline firing, fibers.h) against cumulative GC-pause stats AT THAT
 * MOMENT. Cheap relaxed loads — safe to call even when NOVA_GC_PAUSE_DIAG
 * was never set (all-zero in that case, since the callback never runs). */
void nova_gc_pause_diag_snapshot(uint64_t* count, uint64_t* total_ns, uint64_t* max_ns) {
    if (count)    *count    = __atomic_load_n(&_nova_gc_pause_count, __ATOMIC_RELAXED);
    if (total_ns) *total_ns = __atomic_load_n(&_nova_gc_pause_total_ns, __ATOMIC_RELAXED);
    if (max_ns)   *max_ns   = __atomic_load_n(&_nova_gc_pause_max_ns, __ATOMIC_RELAXED);
}

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
 * поэтому решение принимается ДО вызова, своей пробой ядра (та же, что
 * `detect_soft_dirty_supported` в libgc: очистить биты через clear_refs,
 * записать страницу, прочитать бит 55 в pagemap).
 *
 * `NOVA_GC_INCREMENTAL=0` — выключить всегда (как раньше); `=1` — включить
 * БЕЗУСЛОВНО, в том числе поверх защиты страниц: это ключ пробы в обе
 * стороны на одном бинаре, не режим для работы. */
#if defined(__linux__)
#  include <fcntl.h>
#  include <unistd.h>
#  include <sys/mman.h>
static int _nova_kernel_soft_dirty_works(void) {
    int ok = 0;
    long pg = sysconf(_SC_PAGESIZE);
    if (pg <= 0) return 0;
    volatile char* page = (volatile char*)mmap(NULL, (size_t)pg, PROT_READ | PROT_WRITE,
                                               MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == (volatile char*)MAP_FAILED) return 0;
    page[0] = 1;  /* страница присутствует */
    int cfd = open("/proc/self/clear_refs", O_WRONLY | O_CLOEXEC);
    int pfd = open("/proc/self/pagemap", O_RDONLY | O_CLOEXEC);
    if (cfd >= 0 && pfd >= 0 && write(cfd, "4", 1) == 1) {
        uint64_t e = 0;
        off_t off = (off_t)(((uintptr_t)page / (uintptr_t)pg) * sizeof e);
        int clean = pread(pfd, &e, sizeof e, off) == (ssize_t)sizeof e && !((e >> 55) & 1);
        page[0] = 2;
        e = 0;
        ok = clean && pread(pfd, &e, sizeof e, off) == (ssize_t)sizeof e && ((e >> 55) & 1);
    }
    if (cfd >= 0) close(cfd);
    if (pfd >= 0) close(pfd);
    munmap((void*)page, (size_t)pg);
    return ok;
}
#endif

static int _nova_gc_incremental_wanted(void) {
    const char* e = getenv("NOVA_GC_INCREMENTAL");
    if (e && strcmp(e, "0") == 0) return 0;
    if (e && strcmp(e, "1") == 0) return 1;
#if defined(_WIN32)
    return 1;   /* GWW_VDB: ядро ведёт журнал записи, страницы не защищаются */
#elif defined(__linux__)
    /* SOFT_VDB появился в libgc 8.2; старее — только защита страниц. */
    return GC_get_version() >= ((8u << 16) | (2u << 8)) && _nova_kernel_soft_dirty_works();
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
     * больше числом. Замер (getenv A/B, `NOVA_GC_PAUSE_DIAG=1`) на этой же
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

    /* 221.1 №474: STW pause histogram — see comment above the callback.
     * Registered ONLY when the env var is set; dump wired via atexit() so
     * callers don't need a matching public shutdown hook. */
    if (getenv("NOVA_GC_PAUSE_DIAG")) {
        GC_set_on_collection_event(_nova_gc_pause_event_cb);
        atexit(_nova_gc_pause_diag_dump);
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
