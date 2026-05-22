// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 44.2 Etap 1 — per-thread fiber stack arena (Linux/macOS).
 * See fiber_arena.h for design notes.
 *
 * Compiled into binary as separate TU (linked alongside alloc_boehm.c /
 * effects.c / fibers.c). Windows: this TU compiles но не используется —
 * NOVA_FIBER_ARENA_ENABLED == 0 makes everything no-op. */

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

struct NovaFiberArena {
    char*    base;             /* mmap'd base address */
    size_t   virtual_size;     /* total bytes reserved */
    size_t   slot_size;        /* per-slot, including guard page */
    size_t   slot_count;
    size_t   slots_active;
    size_t   high_water;       /* highest slot index ever used */
    /* Free-list bitmap: 1 bit per slot.
     * Bit 1 = used, bit 0 = free.
     * Plan 23 prep: будет atomic под M:N (P41-15). Сейчас plain. */
    uint64_t free_bits[NOVA_FIBER_BITMAP_WORDS];
};

static __thread NovaFiberArena _t_arena = { 0 };
static pthread_key_t _arena_cleanup_key;
static pthread_once_t _arena_key_once = PTHREAD_ONCE_INIT;

/* ── Cleanup at thread exit (P41-12) ───────────────────────────── */

static void _arena_thread_exit_cleanup(void* arg) {
    NovaFiberArena* a = (NovaFiberArena*)arg;
    if (!a || !a->base) return;

#ifdef NOVA_GC_BOEHM
    /* Unregister GC roots для этой arena before unmapping. Boehm
     * GC_remove_roots takes (start, end). Safe to call даже если
     * range never registered (no-op then). */
    if (a->high_water > 0) {
        GC_remove_roots(a->base, a->base + a->high_water * a->slot_size);
    }
    /* If no slots ever used, range was never registered — skip. */
#endif

    munmap(a->base, a->virtual_size);
    memset(a, 0, sizeof(*a));
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
 * Chaining: сохраняем previous SIGSEGV handler в `_prev_sigsegv` и
 * вызываем его если fault не в нашей arena (или в usable region).
 *
 * Безопасность: handler работает в signal context (async-signal-safe
 * functions only). fprintf к stderr — НЕ async-safe строго, но в
 * single-threaded crash context это commonly acceptable practice
 * (Boehm, libuv, Go runtime все делают похожее). */

static struct sigaction _prev_sigsegv;
static bool _sigsegv_installed = false;

static void _arena_sigsegv_handler(int sig, siginfo_t* info, void* uctx) {
    void* fault_addr = info ? info->si_addr : NULL;

    /* Не наш диапазон? Восстановим default или previous handler и re-raise. */
    if (!_t_arena.base || !fault_addr ||
        (char*)fault_addr <  _t_arena.base ||
        (char*)fault_addr >= _t_arena.base + _t_arena.virtual_size) {
        /* Delegate. Если previous был SIG_DFL — restore default и re-raise.
         * Если был user handler — invoke его. */
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
        /* No handler — restore default and re-raise. Default = process abort. */
        signal(sig, SIG_DFL);
        raise(sig);
        return;
    }

    /* В arena. Какой slot? guard или usable? */
    size_t offset      = (size_t)((char*)fault_addr - _t_arena.base);
    size_t slot_idx    = offset / _t_arena.slot_size;
    size_t slot_offset = offset % _t_arena.slot_size;

    /* fprintf не строго async-safe — но в crash context приемлемо. */
    if (slot_offset < NOVA_FIBER_GUARD_SIZE) {
        fprintf(stderr,
                "\nnova: fiber stack overflow in slot %zu "
                "(fault @ %p, guard @ [%p, %p))\n"
                "Hint: increase NOVA_FIBER_STACK_SIZE or reduce recursion depth.\n",
                slot_idx, fault_addr,
                _t_arena.base + slot_idx * _t_arena.slot_size,
                _t_arena.base + slot_idx * _t_arena.slot_size + NOVA_FIBER_GUARD_SIZE);
    } else {
        fprintf(stderr,
                "\nnova: SIGSEGV in fiber arena slot %zu, offset %zu "
                "(fault @ %p)\n"
                "Hint: heap corruption or use-after-free affecting fiber memory.\n",
                slot_idx, slot_offset, fault_addr);
    }
    fflush(stderr);

    /* Restore default and re-raise so core dump / debugger attach работает. */
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
    if (_t_arena.base) return;  /* already initialized */

    pthread_once(&_arena_key_once, _arena_register_pthread_key);
    /* P41-6: pretty stack overflow diagnostic. Idempotent — выполнится
     * один раз для процесса (не per-thread). */
    _arena_install_sigsegv_handler();

    size_t slot_size = NOVA_FIBER_STACK_SIZE;
    size_t slot_count = NOVA_FIBER_SLOT_COUNT;

    /* Bitmap sized via NOVA_FIBER_BITMAP_WORDS — supports up to
     * NOVA_FIBER_SLOT_COUNT. Sanity check: slot_count must fit bitmap. */
    if (slot_count > NOVA_FIBER_BITMAP_WORDS * 64) {
        fprintf(stderr, "nova: fiber_arena slot_count exceeds bitmap capacity\n");
        abort();
    }

    size_t virtual_size = slot_size * slot_count;

    /* Plan 44.2 P41-3 TODO: detect vm.overcommit_memory=2 and downgrade.
     * For now (bootstrap) assume default Linux overcommit_memory ∈ {0,1}. */

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

    /* Plan 44.2 P41-14: disable Transparent Huge Pages для arena.
     * THP upgrades 2MB-aligned VMAs to 2MB huge pages — конфликтует с
     * lazy commit precision (entire slot would commit at once). */
#if defined(MADV_NOHUGEPAGE)
    madvise(p, virtual_size, MADV_NOHUGEPAGE);
#endif

    /* Plan 44.2 P41-5: guard page (PROT_NONE, 4KB) at bottom of every slot.
     * Stack grows DOWN on x86/ARM, so "bottom" = lowest address = first
     * page of each slot. Overflow → page fault → SIGSEGV. */
    for (size_t i = 0; i < slot_count; i++) {
        char* slot_base = (char*)p + i * slot_size;
        if (mprotect(slot_base, NOVA_FIBER_GUARD_SIZE, PROT_NONE) != 0) {
            fprintf(stderr, "nova: fiber_arena guard page mprotect failed\n");
            abort();
        }
    }

    _t_arena.base = (char*)p;
    _t_arena.virtual_size = virtual_size;
    _t_arena.slot_size = slot_size;
    _t_arena.slot_count = slot_count;
    _t_arena.slots_active = 0;
    _t_arena.high_water = 0;
    memset(_t_arena.free_bits, 0, sizeof(_t_arena.free_bits));

    /* Plan 44.2 P41-11: НЕ register full arena как GC root now —
     * defeats lazy commit (Boehm scan reads every page → COW to zero,
     * RSS grows to full 512MB).
     * Active-range registration: lazy, on first slot alloc bumping
     * high_water. См. _arena_register_active_range. */

    /* Register для thread-exit cleanup. */
    pthread_setspecific(_arena_cleanup_key, &_t_arena);
}

/* ── Active-range GC root management (P41-11) ──────────────────── */

#ifdef NOVA_GC_BOEHM
/* Plan 44.2 audit R8 P0 (2026-05-13): __thread — per-thread tracker.
 * Без __thread thread B's `_arena_register_active_range` видит
 * `_registered_high_water = 100` от Thread A → skip'ит свою регистрацию
 * → Thread B fiber stacks НЕ зарегистрированы как Boehm root →
 * conservative scan miss → UAF под M:N (Plan 23). */
static __thread size_t _registered_high_water = 0;

static void _arena_register_active_range(NovaFiberArena* a, size_t new_high) {
    if (new_high <= _registered_high_water) return;  /* already covered */

    /* Remove old range, add new. Boehm GC_remove_roots / GC_add_roots
     * are safe to call repeatedly. */
    if (_registered_high_water > 0) {
        GC_remove_roots(a->base, a->base + _registered_high_water * a->slot_size);
    }
    GC_add_roots(a->base, a->base + new_high * a->slot_size);
    _registered_high_water = new_high;
}
#else
static inline void _arena_register_active_range(NovaFiberArena* a, size_t h) {
    (void)a; (void)h;
}
#endif

/* ── Bitmap allocate / free ─────────────────────────────────────── */

/* Find first free slot (bit 0 in free_bits). Returns slot index or
 * SIZE_MAX if none. */
static size_t _arena_find_free_slot(NovaFiberArena* a) {
    size_t total_words = (a->slot_count + 63) / 64;
    for (size_t w = 0; w < total_words; w++) {
        uint64_t inv = ~a->free_bits[w];
        if (inv == 0) continue;  /* word fully used */
        /* __builtin_ctzll: count trailing zeros (Linux/macOS clang/gcc). */
        size_t bit = (size_t)__builtin_ctzll(inv);
        size_t slot = w * 64 + bit;
        if (slot >= a->slot_count) continue;  /* past end */
        return slot;
    }
    return SIZE_MAX;
}

static void _arena_mark_slot_used(NovaFiberArena* a, size_t slot) {
    size_t w = slot / 64;
    size_t b = slot % 64;
    a->free_bits[w] |= (1ULL << b);
}

static void _arena_mark_slot_free(NovaFiberArena* a, size_t slot) {
    size_t w = slot / 64;
    size_t b = slot % 64;
    a->free_bits[w] &= ~(1ULL << b);
}

/* ── minicoro alloc callbacks ──────────────────────────────────── */

void* nova_fiber_alloc(size_t size, void* allocator_data) {
    (void)allocator_data;
    if (!_t_arena.base) {
        nova_fiber_arena_init();
    }

    /* Caller (minicoro) запросит конкретный size; мы ignore — slot_size
     * фиксирован. Verify что requested ≤ usable region (slot - guard). */
    size_t usable = _t_arena.slot_size - NOVA_FIBER_GUARD_SIZE;
    if (size > usable) {
        fprintf(stderr, "nova: fiber_alloc requested %zu > usable %zu (slot %zu - guard %d)\n",
                size, usable, _t_arena.slot_size, NOVA_FIBER_GUARD_SIZE);
        return NULL;  /* minicoro will handle as failure */
    }

    size_t slot = _arena_find_free_slot(&_t_arena);
    if (slot == SIZE_MAX) {
        /* Plan 44.2 P41-2: abort instead of calloc fallback (which would
         * regress to _NOVA_GC_DISABLE UAF risk). Arena exhaustion is
         * production error — log + abort. Production sizing: 256 slots.
         * Plan 23 prep: implement arena chaining instead of abort. */
        fprintf(stderr, "nova: fiber_arena exhausted (%zu slots used)\n",
                _t_arena.slots_active);
        abort();
    }

    _arena_mark_slot_used(&_t_arena, slot);
    _t_arena.slots_active++;
    if (slot + 1 > _t_arena.high_water) {
        _t_arena.high_water = slot + 1;
        _arena_register_active_range(&_t_arena, _t_arena.high_water);
    }

    /* Usable region: slot_base + guard_size .. slot_base + slot_size.
     * Stack starts at slot_top (grows down). minicoro caller treats
     * returned pointer as base of stack region. */
    return _t_arena.base + slot * _t_arena.slot_size + NOVA_FIBER_GUARD_SIZE;
}

void nova_fiber_dealloc(void* ptr, size_t size, void* allocator_data) {
    (void)size; (void)allocator_data;
    if (!ptr || !_t_arena.base) return;

    /* Reverse usable_ptr → slot index.
     * usable_ptr = base + slot * slot_size + guard_size
     * → slot = (usable_ptr - base - guard_size) / slot_size */
    char* p = (char*)ptr;
    if (p < _t_arena.base + NOVA_FIBER_GUARD_SIZE ||
        p >= _t_arena.base + _t_arena.virtual_size) {
        fprintf(stderr, "nova: fiber_dealloc ptr outside arena (%p)\n", ptr);
        return;
    }
    size_t offset = (size_t)(p - _t_arena.base - NOVA_FIBER_GUARD_SIZE);
    size_t slot = offset / _t_arena.slot_size;
    if (slot >= _t_arena.slot_count) {
        fprintf(stderr, "nova: fiber_dealloc slot index out of range\n");
        return;
    }

    _arena_mark_slot_free(&_t_arena, slot);
    _t_arena.slots_active--;

    /* Plan 44.2 P41-3 (R8, 2026-05-13): MADV_DONTNEED только на idle.
     *
     * Раньше: per-dealloc madvise → каждый syscall takes mmap_sem write
     * lock → serialize все VM ops в процессе. Под 100k fiber/sec churn —
     * deadlock-grade.
     *
     * Теперь: при `slots_active == 0` (idle = весь scope завершился)
     * выполняем ОДИН madvise на весь used range [base+guard, high_water*slot].
     * Pages этого range возвращаются ОС батчем, mmap_sem locked один раз.
     *
     * Trade-off: между peak и idle physical pages держатся (slot pages
     * cached в OS). Это **win**: следующий burst переиспользует pages
     * без zero-page COW. Workload pattern Nova — supervised scope
     * spawns burst → quiescence → next scope — идеально fits.
     *
     * Если pattern long-running без idle (например server с постоянно
     * активными fibers) — manual flush через nova_fibers_compact()
     * (см. std.runtime.fibers). */
    if (_t_arena.slots_active == 0 && _t_arena.high_water > 0) {
#ifdef MADV_DONTNEED
        char* range_base = _t_arena.base + NOVA_FIBER_GUARD_SIZE;
        size_t range_size = _t_arena.high_water * _t_arena.slot_size
                          - NOVA_FIBER_GUARD_SIZE;
        madvise(range_base, range_size, MADV_DONTNEED);
#endif
    }
}

/* Plan 44.2 P41-3 (2026-05-13): explicit compact API для long-running
 * workloads без natural idle. Released все free slots' physical pages
 * одним syscall. Exposed через std.runtime.fibers.compact(). */
void nova_fiber_arena_compact(void) {
    if (!_t_arena.base || _t_arena.high_water == 0) return;
#ifdef MADV_DONTNEED
    /* Iterate bitmap, find contiguous free runs, batch MADV. */
    size_t total_words = (_t_arena.slot_count + 63) / 64;
    size_t run_start = SIZE_MAX;  /* sentinel — no run in progress */
    for (size_t w = 0; w < total_words; w++) {
        uint64_t bits = _t_arena.free_bits[w];
        for (size_t b = 0; b < 64; b++) {
            size_t slot = w * 64 + b;
            if (slot >= _t_arena.high_water) goto end_scan;
            bool used = (bits >> b) & 1;
            if (!used) {
                if (run_start == SIZE_MAX) run_start = slot;
            } else {
                if (run_start != SIZE_MAX) {
                    /* Flush run [run_start, slot). */
                    char* rbase = _t_arena.base + run_start * _t_arena.slot_size
                                + NOVA_FIBER_GUARD_SIZE;
                    size_t rsize = (slot - run_start) * _t_arena.slot_size
                                 - NOVA_FIBER_GUARD_SIZE;
                    madvise(rbase, rsize, MADV_DONTNEED);
                    run_start = SIZE_MAX;
                }
            }
        }
    }
end_scan:
    if (run_start != SIZE_MAX) {
        char* rbase = _t_arena.base + run_start * _t_arena.slot_size
                    + NOVA_FIBER_GUARD_SIZE;
        size_t rsize = (_t_arena.high_water - run_start) * _t_arena.slot_size
                     - NOVA_FIBER_GUARD_SIZE;
        madvise(rbase, rsize, MADV_DONTNEED);
    }
#endif
}

bool nova_fiber_arena_contains(const void* ptr) {
    if (!_t_arena.base) return false;
    return (const char*)ptr >= _t_arena.base &&
           (const char*)ptr <  _t_arena.base + _t_arena.virtual_size;
}

/* Plan 82 Ф.1: POSIX не нуждается в патче ctx.stack_limit — mmap
 * MAP_NORESERVE даёт kernel demand-paging без __chkstk-проблемы. NULL
 * → nova_fiber_post_create (fibers.c) пропускает патч. */
void* nova_fiber_committed_low(const void* block_ptr) {
    (void)block_ptr;
    return NULL;
}

/* Plan 82 Ф.3 — M:N lifecycle. POSIX-арена живёт в TLS (__thread) и
 * освобождается _arena_thread_exit_cleanup через pthread_key при выходе
 * потока; явные thread_exit / release_retired не нужны — no-op. */
void nova_fiber_arena_thread_exit(void) { }
void nova_fiber_arena_release_retired(void) { }

NovaFiberArenaStats nova_fiber_arena_stats(void) {
    NovaFiberArenaStats s = { 0 };
    if (_t_arena.base) {
        s.virtual_reserved = _t_arena.virtual_size;
        s.slot_count       = _t_arena.slot_count;
        s.slots_active     = _t_arena.slots_active;
        s.high_water       = _t_arena.high_water;
    }
    return s;
}

#else /* не POSIX — Windows (fiber_arena_win.c) или unsupported */

/* Пустой TU. На Windows arena-реализацию несёт fiber_arena_win.c; на
 * unsupported-платформах NOVA_FIBER_ARENA_ENABLED == 0 и API не
 * объявлен. Файл всегда в списке линковки — отдельный маркер-тип. */
typedef int _nova_fiber_arena_disabled_marker;

#endif /* POSIX */
