// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 44.2 Etap 1 — per-thread fiber stack arena (Linux/macOS).
 * See fiber_arena.h for design notes.
 *
 * Compiled into binary as separate TU (linked alongside alloc_boehm.c /
 * effects.c / fibers.c). Windows: this TU compiles но не используется —
 * NOVA_FIBER_ARENA_ENABLED == 0 makes everything no-op.
 *
 * Plan 82.2 (2026-05-26): cross-thread dealloc support через глобальный
 * реестр арен — порт механизма из fiber_arena_win.c. Под M:N work-
 * stealing fiber может быть allocated на thread A (через mco_create в
 * nova_runtime_spawn_global), а deallocated на thread B (mco_destroy в
 * worker B'е). Раньше: TLS-based bounds check видел чужой ptr → warning +
 * slot leak (Plan 44.2 явно отложил P41-15 «cross-thread dealloc atomic
 * bitmap» до Plan 23). Теперь: append-only глобальный список арен;
 * nova_fiber_dealloc fast-path = TLS match, slow-path =
 * _nova_find_arena_for(ptr); bitmap clear через __atomic_fetch_and
 * для cross-thread safety. Паритет с Windows fiber_arena_win.c. */

#include "fiber_arena.h"

/* Plan 82 Ф.1: внутренний guard сужен с NOVA_FIBER_ARENA_ENABLED до
 * явного POSIX-условия. NOVA_FIBER_ARENA_ENABLED теперь true и на
 * Windows (Windows-реализация — fiber_arena_win.c); этот файл — строго
 * POSIX-путь, на Windows компилируется в пустой TU. */
#if defined(__linux__) || defined(__APPLE__)

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <unistd.h>
#include <sys/mman.h>
#include <pthread.h>
#include <signal.h>     /* P41-6 SIGSEGV handler */
#include <ucontext.h>   /* для siginfo_t.si_addr */

#ifdef NOVA_GC_BOEHM
#include <gc.h>
#endif

/* ── Per-thread arena state ────────────────────────────────────── */

/* Bitmap word count = ceil(NOVA_FIBER_SLOT_COUNT / 64).
 * 4096 slots = 64 uint64_t words = 512 bytes bitmap. Acceptable cost. */
#define NOVA_FIBER_BITMAP_WORDS ((NOVA_FIBER_SLOT_COUNT + 63) / 64)

/* Plan 82.2: arena struct — heap-allocated (раньше __thread embedded).
 *
 * Структура переживает thread exit — живёт в глобальном append-only
 * списке для cross-thread dealloc routing. Только на retire (thread
 * exit) `base` атомарно зануляется; munmap освобождает виртуальную
 * память, но struct sам не free'ится (другие потоки могут быть в
 * середине list traversal).
 *
 * Field-level concurrency contract:
 *  - base               : atomic store/load. NULL после retire.
 *  - virtual_size       : write-once в init под release-store base'а;
 *                         read-only после init.
 *  - slot_size, slot_count : immutable после init.
 *  - slots_active       : atomic add/sub. Owner increments на alloc,
 *                         любой поток decrements на dealloc. Read для
 *                         MADV gate — atomic load (best-effort).
 *  - high_water         : owner-only write (alloc bumps); plain read OK.
 *  - free_bits[]        : owner OR-set на alloc (RELAXED — single owner,
 *                         no concurrent SETs), любой поток AND-clear
 *                         на dealloc (RELEASE). Read через ACQUIRE-load.
 *  - next_arena         : write-once при list add; read-only после. */
struct NovaFiberArena {
    char*    base;             /* atomic; NULL after retire */
    size_t   virtual_size;
    size_t   slot_size;
    size_t   slot_count;
    size_t   slots_active;     /* atomic add/sub */
    size_t   high_water;       /* owner-only mutation */
    uint64_t free_bits[NOVA_FIBER_BITMAP_WORDS];  /* atomic ops */
    /* Plan 82.2: link в глобальный append-only список арен. */
    struct NovaFiberArena* next_arena;
};

/* TLS: указатель на heap-allocated арену этого потока. NULL до
 * nova_fiber_arena_init; никогда не free'ится после init (struct живёт
 * в global list до конца процесса). */
static __thread struct NovaFiberArena* _t_arena = NULL;

/* Plan 82.2: global append-only arena registry для cross-thread dealloc
 * dispatch. Чтение lock-free через ACQUIRE-load на head + next_arena
 * pointers. Запись (per-thread init) под mutex'ом. */
static struct NovaFiberArena* _nova_arena_list_head = NULL;
static pthread_mutex_t _nova_arena_list_mu = PTHREAD_MUTEX_INITIALIZER;

static pthread_key_t _arena_cleanup_key;
static pthread_once_t _arena_key_once = PTHREAD_ONCE_INIT;

/* Plan 82.2: find arena owning ptr — address-based dispatch. O(N_arenas)
 * linear scan; N <= N_workers + 1 (main), typically 4-16. Lock-free
 * read (append-only list semantics — никто не удаляет ноды). Skips
 * retired arenas (base == NULL после _arena_thread_exit_cleanup). */
static struct NovaFiberArena* _nova_find_arena_for(const char* p) {
    struct NovaFiberArena* a =
        __atomic_load_n(&_nova_arena_list_head, __ATOMIC_ACQUIRE);
    while (a) {
        char* base = __atomic_load_n(&a->base, __ATOMIC_ACQUIRE);
        if (base &&
            p >= base + NOVA_FIBER_GUARD_SIZE &&
            p <  base + a->virtual_size) {
            return a;
        }
        a = a->next_arena;
    }
    return NULL;
}

/* Plan 82.2: append arena в глобальный список. Mutex-guarded на запись;
 * RELEASE-store на head гарантирует readers видят все a->* fields
 * установленными до того, как видят `a` в списке. */
static void _nova_arena_list_add(struct NovaFiberArena* a) {
    pthread_mutex_lock(&_nova_arena_list_mu);
    a->next_arena = _nova_arena_list_head;
    __atomic_store_n(&_nova_arena_list_head, a, __ATOMIC_RELEASE);
    pthread_mutex_unlock(&_nova_arena_list_mu);
}

/* ── Cleanup at thread exit (P41-12) ───────────────────────────── */

static void _arena_thread_exit_cleanup(void* arg) {
    struct NovaFiberArena* a = (struct NovaFiberArena*)arg;
    if (!a || !a->base) return;

#ifdef NOVA_GC_BOEHM
    /* Unregister GC roots для этой arena before unmapping. Boehm
     * GC_remove_roots takes (start, end). Safe to call даже если
     * range never registered (no-op then). */
    if (a->high_water > 0) {
        GC_remove_roots(a->base, a->base + a->high_water * a->slot_size);
    }
#endif

    munmap(a->base, a->virtual_size);

    /* Plan 82.2: atomic NULL base — marker retired для _nova_find_arena_for.
     * Структура НЕ free'ится — остаётся в глобальном списке (другие
     * потоки могут быть в середине list traversal). Memset selective —
     * НЕ трогать next_arena (link в живой список). */
    __atomic_store_n(&a->base, NULL, __ATOMIC_RELEASE);
    a->virtual_size = 0;
    a->slots_active = 0;
    a->high_water = 0;
    memset(a->free_bits, 0, sizeof(a->free_bits));
    /* slot_size / slot_count / next_arena — оставлены: первые два immutable
     * post-init и больше не читаются (base==NULL), next_arena — link. */
}

static void _arena_register_pthread_key(void) {
    pthread_key_create(&_arena_cleanup_key, _arena_thread_exit_cleanup);
}

/* ── SIGSEGV pretty handler (P41-6, 2026-05-13) ───────────────────
 *
 * Перехватывает SIGSEGV для guard-page hits в нашей arena и печатает
 * понятную диагностику ("Fiber stack overflow in slot N") вместо
 * generic "Segmentation fault".
 *
 * Trade-off: SIGSEGV — process-wide signal, наш handler applies ко
 * всем threads. Для не-arena SIGSEGV (например null deref в user code)
 * мы делегируем обратно default action через sigaction restore.
 *
 * Plan 82.2: cross-thread fiber overflow (work-stolen fiber overflows
 * на worker B, но stack принадлежит worker A's arena) теперь корректно
 * диагностируется — handler ищет owner arena в глобальном списке если
 * TLS arena не содержит fault. */

static struct sigaction _prev_sigsegv;
static bool _sigsegv_installed = false;

static void _arena_sigsegv_handler(int sig, siginfo_t* info, void* uctx) {
    void* fault_addr = info ? info->si_addr : NULL;
    struct NovaFiberArena* a = _t_arena;
    bool in_our_range = false;

    /* Plan 82.2: сначала проверяем TLS arena (fast path); если fault
     * не сюда — пытаемся найти owner globally (cross-thread fiber). */
    if (a && a->base && fault_addr &&
        (char*)fault_addr >= a->base &&
        (char*)fault_addr <  a->base + a->virtual_size) {
        in_our_range = true;
    } else if (fault_addr) {
        struct NovaFiberArena* owner =
            _nova_find_arena_for((const char*)fault_addr);
        if (owner) {
            a = owner;
            in_our_range = true;
        }
    }

    if (!in_our_range) {
        /* Не наш диапазон. Делегируем previous handler или default. */
        if (_prev_sigsegv.sa_flags & SA_SIGINFO) {
            if (_prev_sigsegv.sa_sigaction &&
                _prev_sigsegv.sa_sigaction != (void*)SIG_DFL &&
                _prev_sigsegv.sa_sigaction != (void*)SIG_IGN) {
                _prev_sigsegv.sa_sigaction(sig, info, uctx);
                return;
            }
        } else if (_prev_sigsegv.sa_handler &&
                   _prev_sigsegv.sa_handler != SIG_DFL &&
                   _prev_sigsegv.sa_handler != SIG_IGN) {
            _prev_sigsegv.sa_handler(sig);
            return;
        }
        signal(sig, SIG_DFL);
        raise(sig);
        return;
    }

    /* В arena `a`. Какой slot? guard или usable? */
    size_t offset      = (size_t)((char*)fault_addr - a->base);
    size_t slot_idx    = offset / a->slot_size;
    size_t slot_offset = offset % a->slot_size;

    if (slot_offset < NOVA_FIBER_GUARD_SIZE) {
        fprintf(stderr,
                "\nnova: fiber stack overflow in slot %zu "
                "(fault @ %p, guard @ [%p, %p))\n"
                "Hint: increase NOVA_FIBER_STACK_SIZE or reduce recursion depth.\n",
                slot_idx, fault_addr,
                a->base + slot_idx * a->slot_size,
                a->base + slot_idx * a->slot_size + NOVA_FIBER_GUARD_SIZE);
    } else {
        fprintf(stderr,
                "\nnova: SIGSEGV in fiber arena slot %zu, offset %zu "
                "(fault @ %p)\n"
                "Hint: heap corruption or use-after-free affecting fiber memory.\n",
                slot_idx, slot_offset, fault_addr);
    }
    fflush(stderr);

    signal(sig, SIG_DFL);
    raise(sig);
}

static void _arena_install_sigsegv_handler(void) {
    if (_sigsegv_installed) return;
    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_sigaction = _arena_sigsegv_handler;
    sa.sa_flags     = SA_SIGINFO | SA_NODEFER;  /* allow re-entry для re-raise */
    sigemptyset(&sa.sa_mask);
    sigaction(SIGSEGV, &sa, &_prev_sigsegv);
    _sigsegv_installed = true;
}

/* ── Init ──────────────────────────────────────────────────────── */

void nova_fiber_arena_init(void) {
    /* Already initialized? (idempotent — safe to call multiple times.) */
    if (_t_arena && _t_arena->base) return;

    pthread_once(&_arena_key_once, _arena_register_pthread_key);
    /* P41-6: pretty stack overflow diagnostic. Idempotent — выполнится
     * один раз для процесса (не per-thread). */
    _arena_install_sigsegv_handler();

    size_t slot_size = NOVA_FIBER_STACK_SIZE;
    size_t slot_count = NOVA_FIBER_SLOT_COUNT;

    if (slot_count > NOVA_FIBER_BITMAP_WORDS * 64) {
        fprintf(stderr, "nova: fiber_arena slot_count exceeds bitmap capacity\n");
        abort();
    }

    size_t virtual_size = slot_size * slot_count;

    int prot = PROT_READ | PROT_WRITE;
    int flags = MAP_PRIVATE | MAP_ANONYMOUS;
#ifdef MAP_NORESERVE
    flags |= MAP_NORESERVE;
#endif

    void* p = mmap(NULL, virtual_size, prot, flags, -1, 0);
    if (p == MAP_FAILED) {
        fprintf(stderr, "nova: fiber_arena mmap failed (%zu bytes)\n",
                virtual_size);
        abort();
    }

    /* Plan 44.2 P41-14: disable Transparent Huge Pages для arena. */
#if defined(MADV_NOHUGEPAGE)
    madvise(p, virtual_size, MADV_NOHUGEPAGE);
#endif

    /* Plan 44.2 P41-5: guard page (PROT_NONE) at bottom of every slot. */
    for (size_t i = 0; i < slot_count; i++) {
        char* slot_base = (char*)p + i * slot_size;
        if (mprotect(slot_base, NOVA_FIBER_GUARD_SIZE, PROT_NONE) != 0) {
            fprintf(stderr, "nova: fiber_arena guard page mprotect failed\n");
            abort();
        }
    }

    /* Plan 82.2: heap-allocate arena struct. calloc zero-инициализирует;
     * никогда не free'ится — живёт в global list до конца процесса. */
    struct NovaFiberArena* a =
        (struct NovaFiberArena*)calloc(1, sizeof(struct NovaFiberArena));
    if (!a) {
        fprintf(stderr, "nova: fiber_arena state alloc failed\n");
        abort();
    }
    a->virtual_size = virtual_size;
    a->slot_size = slot_size;
    a->slot_count = slot_count;
    a->slots_active = 0;
    a->high_water = 0;
    /* free_bits, next_arena уже zero'd calloc'ом. */

    /* base устанавливается RELEASE-store последним: до этого момента
     * _nova_find_arena_for видит arena с base==NULL → skip. После store —
     * ACQUIRE-readers видят all остальные fields. */
    __atomic_store_n(&a->base, (char*)p, __ATOMIC_RELEASE);

    /* Append в глобальный список ПОСЛЕ полной инициализации. */
    _nova_arena_list_add(a);

    _t_arena = a;

    /* Plan 44.2 P41-11: НЕ register full arena как GC root now —
     * active-range registration: lazy, on first slot alloc bumping
     * high_water. См. _arena_register_active_range. */

    /* Plan 82.2: pthread_setspecific принимает heap pointer (не &_t_arena).
     * Cleanup-callback получит указатель на heap struct — корректно
     * munmap + NULL base + сохранение next_arena в list. */
    pthread_setspecific(_arena_cleanup_key, a);
}

/* ── Active-range GC root management (P41-11) ──────────────────── */

#ifdef NOVA_GC_BOEHM
/* Plan 44.2 audit R8 P0 (2026-05-13): __thread — per-thread tracker. */
static __thread size_t _registered_high_water = 0;

static void _arena_register_active_range(struct NovaFiberArena* a, size_t new_high) {
    if (new_high <= _registered_high_water) return;

    if (_registered_high_water > 0) {
        GC_remove_roots(a->base, a->base + _registered_high_water * a->slot_size);
    }
    GC_add_roots(a->base, a->base + new_high * a->slot_size);
    _registered_high_water = new_high;
}
#else
static inline void _arena_register_active_range(struct NovaFiberArena* a, size_t h) {
    (void)a; (void)h;
}
#endif

/* ── Bitmap allocate / free ─────────────────────────────────────── */

/* Find first free slot (bit 0 in free_bits). Returns slot index or
 * SIZE_MAX if none.
 *
 * Plan 82.2: ACQUIRE-load на каждое слово — гарантирует видимость
 * cross-thread released slots до того, как owner ищет free slot. */
static size_t _arena_find_free_slot(struct NovaFiberArena* a) {
    size_t total_words = (a->slot_count + 63) / 64;
    for (size_t w = 0; w < total_words; w++) {
        uint64_t word = __atomic_load_n(&a->free_bits[w], __ATOMIC_ACQUIRE);
        uint64_t inv = ~word;
        if (inv == 0) continue;  /* word fully used */
        size_t bit = (size_t)__builtin_ctzll(inv);
        size_t slot = w * 64 + bit;
        if (slot >= a->slot_count) continue;  /* past end */
        return slot;
    }
    return SIZE_MAX;
}

/* Plan 82.2: atomic OR — owner-only path (alloc на owning thread).
 * Никто другой не делает SET одновременно (cross-thread только AND-clears
 * другие слоты). RELAXED достаточно — happens-before гарантируется
 * single-owner store-order. */
static void _arena_mark_slot_used(struct NovaFiberArena* a, size_t slot) {
    size_t w = slot / 64;
    size_t b = slot % 64;
    __atomic_fetch_or(&a->free_bits[w], (1ULL << b), __ATOMIC_RELAXED);
}

/* Plan 82.2: atomic AND — cross-thread safe (owner thread И любой
 * worker, выполнивший mco_destroy для work-stolen fiber'а).
 * RELEASE — clear visible перед slots_active decrement. */
static void _arena_mark_slot_free(struct NovaFiberArena* a, size_t slot) {
    size_t w = slot / 64;
    size_t b = slot % 64;
    __atomic_fetch_and(&a->free_bits[w], ~(1ULL << b), __ATOMIC_RELEASE);
}

/* ── minicoro alloc callbacks ──────────────────────────────────── */

void* nova_fiber_alloc(size_t size, void* allocator_data) {
    (void)allocator_data;
    if (!_t_arena || !_t_arena->base) {
        nova_fiber_arena_init();
    }
    struct NovaFiberArena* a = _t_arena;

    /* Caller (minicoro) запросит конкретный size; мы ignore — slot_size
     * фиксирован. Verify что requested ≤ usable region (slot - guard). */
    size_t usable = a->slot_size - NOVA_FIBER_GUARD_SIZE;
    if (size > usable) {
        fprintf(stderr, "nova: fiber_alloc requested %zu > usable %zu (slot %zu - guard %d)\n",
                size, usable, a->slot_size, NOVA_FIBER_GUARD_SIZE);
        return NULL;  /* minicoro will handle as failure */
    }

    size_t slot = _arena_find_free_slot(a);
    if (slot == SIZE_MAX) {
        fprintf(stderr, "nova: fiber_arena exhausted (%zu slots used)\n",
                __atomic_load_n(&a->slots_active, __ATOMIC_RELAXED));
        abort();
    }

    _arena_mark_slot_used(a, slot);
    __atomic_add_fetch(&a->slots_active, 1, __ATOMIC_RELAXED);
    if (slot + 1 > a->high_water) {
        a->high_water = slot + 1;
        _arena_register_active_range(a, a->high_water);
    }

    /* Usable region: slot_base + guard_size .. slot_base + slot_size.
     * Stack starts at slot_top (grows down). minicoro caller treats
     * returned pointer as base of stack region. */
    return a->base + slot * a->slot_size + NOVA_FIBER_GUARD_SIZE;
}

/* Plan 82.2: address-based dealloc — fast path TLS arena, slow path
 * global lookup. Под M:N work-stealing fiber может быть allocated на
 * thread A (mco_create в nova_runtime_spawn_global on calling thread),
 * deallocated на worker B (mco_destroy в worker B'е после fiber dies).
 *
 * Раньше: только TLS check → cross-thread ptr вне range → warning +
 * skip → slot leak в A's arena (bitmap bit never cleared).
 *
 * Теперь: fast path = TLS bounds match (typical same-thread case);
 * slow path = _nova_find_arena_for(p) — address-based owner lookup в
 * глобальном списке арен. Atomic bitmap clear работает корректно
 * cross-thread. */
void nova_fiber_dealloc(void* ptr, size_t size, void* allocator_data) {
    (void)size; (void)allocator_data;
    if (!ptr) return;

    char* p = (char*)ptr;
    struct NovaFiberArena* a = _t_arena;

    /* Fast path: ptr в текущей TLS arena (typical case без миграции). */
    if (a && a->base &&
        p >= a->base + NOVA_FIBER_GUARD_SIZE &&
        p <  a->base + a->virtual_size) {
        /* in current arena — fall through */
    } else {
        /* Slow path: cross-thread dealloc — найти owner по адресу. */
        a = _nova_find_arena_for(p);
        if (!a) {
            fprintf(stderr, "nova: fiber_dealloc ptr outside all arenas (%p)\n", ptr);
            return;
        }
    }

    /* Reverse usable_ptr → slot index using owning arena's layout. */
    size_t offset = (size_t)(p - a->base - NOVA_FIBER_GUARD_SIZE);
    size_t slot = offset / a->slot_size;
    if (slot >= a->slot_count) {
        fprintf(stderr, "nova: fiber_dealloc slot index out of range\n");
        return;
    }

    _arena_mark_slot_free(a, slot);
    __atomic_sub_fetch(&a->slots_active, 1, __ATOMIC_RELAXED);

    /* Plan 44.2 P41-3 (R8, 2026-05-13): MADV_DONTNEED только на idle.
     *
     * Раньше: per-dealloc madvise → каждый syscall takes mmap_sem write
     * lock → serialize все VM ops в процессе. Под 100k fiber/sec churn —
     * deadlock-grade.
     *
     * Теперь: при `slots_active == 0` (idle = весь scope завершился)
     * выполняем ОДИН madvise на весь used range [base+guard, high_water*slot].
     *
     * Plan 82.2: MADV_DONTNEED только когда dealloc на own thread
     * (a == _t_arena). Cross-thread dealloc skip'ает MADV — owning thread
     * сам сделает на следующем idle (free_bits cleared cross-thread'ом
     * виден ACQUIRE-load'у в _arena_find_free_slot). */
    if (a == _t_arena &&
        __atomic_load_n(&a->slots_active, __ATOMIC_ACQUIRE) == 0 &&
        a->high_water > 0) {
#ifdef MADV_DONTNEED
        char* range_base = a->base + NOVA_FIBER_GUARD_SIZE;
        size_t range_size = a->high_water * a->slot_size
                          - NOVA_FIBER_GUARD_SIZE;
        madvise(range_base, range_size, MADV_DONTNEED);
#endif
    }
}

/* Plan 44.2 P41-3 (2026-05-13): explicit compact API для long-running
 * workloads без natural idle. Released все free slots' physical pages
 * одним syscall. Exposed через std.runtime.fibers.compact(). */
void nova_fiber_arena_compact(void) {
    if (!_t_arena || !_t_arena->base || _t_arena->high_water == 0) return;
    struct NovaFiberArena* a = _t_arena;
#ifdef MADV_DONTNEED
    /* Iterate bitmap, find contiguous free runs, batch MADV. */
    size_t total_words = (a->slot_count + 63) / 64;
    size_t run_start = SIZE_MAX;  /* sentinel — no run in progress */
    for (size_t w = 0; w < total_words; w++) {
        uint64_t bits = __atomic_load_n(&a->free_bits[w], __ATOMIC_ACQUIRE);
        for (size_t b = 0; b < 64; b++) {
            size_t slot = w * 64 + b;
            if (slot >= a->high_water) goto end_scan;
            bool used = (bits >> b) & 1;
            if (!used) {
                if (run_start == SIZE_MAX) run_start = slot;
            } else {
                if (run_start != SIZE_MAX) {
                    /* Flush run [run_start, slot). */
                    char* rbase = a->base + run_start * a->slot_size
                                + NOVA_FIBER_GUARD_SIZE;
                    size_t rsize = (slot - run_start) * a->slot_size
                                 - NOVA_FIBER_GUARD_SIZE;
                    madvise(rbase, rsize, MADV_DONTNEED);
                    run_start = SIZE_MAX;
                }
            }
        }
    }
end_scan:
    if (run_start != SIZE_MAX) {
        char* rbase = a->base + run_start * a->slot_size
                    + NOVA_FIBER_GUARD_SIZE;
        size_t rsize = (a->high_water - run_start) * a->slot_size
                     - NOVA_FIBER_GUARD_SIZE;
        madvise(rbase, rsize, MADV_DONTNEED);
    }
#endif
}

bool nova_fiber_arena_contains(const void* ptr) {
    if (!_t_arena || !_t_arena->base) return false;
    return (const char*)ptr >= _t_arena->base &&
           (const char*)ptr <  _t_arena->base + _t_arena->virtual_size;
}

/* Plan 82 Ф.1: POSIX не нуждается в патче ctx.stack_limit — mmap
 * MAP_NORESERVE даёт kernel demand-paging без __chkstk-проблемы. NULL
 * → nova_fiber_post_create (fibers.c) пропускает патч. */
void* nova_fiber_committed_low(const void* block_ptr) {
    (void)block_ptr;
    return NULL;
}

/* Plan 82 Ф.3 — M:N lifecycle. POSIX-арена живёт в TLS pointer + heap
 * struct в глобальном списке; cleanup идёт через pthread_key при выходе
 * потока (Plan 82.2: munmap + NULL base + сохранение struct в list для
 * cross-thread dealloc traversal). Явные thread_exit / release_retired
 * не нужны — no-op. */
void nova_fiber_arena_thread_exit(void) { }
void nova_fiber_arena_release_retired(void) { }

NovaFiberArenaStats nova_fiber_arena_stats(void) {
    NovaFiberArenaStats s = { 0 };
    if (_t_arena && _t_arena->base) {
        s.virtual_reserved = _t_arena->virtual_size;
        s.slot_count       = _t_arena->slot_count;
        s.slots_active     = __atomic_load_n(&_t_arena->slots_active,
                                              __ATOMIC_RELAXED);
        s.high_water       = _t_arena->high_water;
    }
    return s;
}

#else /* не POSIX — Windows (fiber_arena_win.c) или unsupported */

/* Пустой TU. На Windows arena-реализацию несёт fiber_arena_win.c; на
 * unsupported-платформах NOVA_FIBER_ARENA_ENABLED == 0 и API не
 * объявлен. Файл всегда в списке линковки — отдельный маркер-тип. */
typedef int _nova_fiber_arena_disabled_marker;

#endif /* POSIX */
