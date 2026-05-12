// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 41 Etap 1 — per-thread fiber stack arena (Linux/macOS).
 * See fiber_arena.h for design notes.
 *
 * Compiled into binary as separate TU (linked alongside alloc_boehm.c /
 * effects.c / fibers.c). Windows: this TU compiles но не используется —
 * NOVA_FIBER_ARENA_ENABLED == 0 makes everything no-op. */

#include "fiber_arena.h"

#if NOVA_FIBER_ARENA_ENABLED

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <unistd.h>
#include <sys/mman.h>
#include <pthread.h>

#ifdef NOVA_GC_BOEHM
#include <gc.h>
#endif

/* ── Per-thread arena state ────────────────────────────────────── */

struct NovaFiberArena {
    char*    base;             /* mmap'd base address */
    size_t   virtual_size;     /* total bytes reserved */
    size_t   slot_size;        /* per-slot, including guard page */
    size_t   slot_count;
    size_t   slots_active;
    size_t   high_water;       /* highest slot index ever used */
    /* Free-list bitmap: 1 bit per slot. 256 slots = 4 uint64_t words.
     * Bit 1 = used, bit 0 = free.
     * Plan 23 prep: будет atomic под M:N (P41-15). Сейчас plain. */
    uint64_t free_bits[4];     /* 256 bits = 4 words; need bump if > 256 slots */
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

/* ── Init ──────────────────────────────────────────────────────── */

void nova_fiber_arena_init(void) {
    if (_t_arena.base) return;  /* already initialized */

    pthread_once(&_arena_key_once, _arena_register_pthread_key);

    size_t slot_size = NOVA_FIBER_STACK_SIZE;
    size_t slot_count = NOVA_FIBER_SLOT_COUNT;

    /* sizeof bitmap supports 256 slots; bump if config grows. */
    if (slot_count > 256) {
        fprintf(stderr, "nova: fiber_arena slot_count > 256 needs bigger bitmap\n");
        abort();
    }

    size_t virtual_size = slot_size * slot_count;

    /* Plan 41 P41-3 TODO: detect vm.overcommit_memory=2 and downgrade.
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

    /* Plan 41 P41-14: disable Transparent Huge Pages для arena.
     * THP upgrades 2MB-aligned VMAs to 2MB huge pages — конфликтует с
     * lazy commit precision (entire slot would commit at once). */
#if defined(MADV_NOHUGEPAGE)
    madvise(p, virtual_size, MADV_NOHUGEPAGE);
#endif

    /* Plan 41 P41-5: guard page (PROT_NONE, 4KB) at bottom of every slot.
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

    /* Plan 41 P41-11: НЕ register full arena как GC root now —
     * defeats lazy commit (Boehm scan reads every page → COW to zero,
     * RSS grows to full 512MB).
     * Active-range registration: lazy, on first slot alloc bumping
     * high_water. См. _arena_register_active_range. */

    /* Register для thread-exit cleanup. */
    pthread_setspecific(_arena_cleanup_key, &_t_arena);
}

/* ── Active-range GC root management (P41-11) ──────────────────── */

#ifdef NOVA_GC_BOEHM
static size_t _registered_high_water = 0;

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
        /* Plan 41 P41-2: abort instead of calloc fallback (which would
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

    /* Plan 41 P41-4 P1: should batch madvise через decay queue.
     * Bootstrap: per-dealloc syscall. Известный regression под churn —
     * fix to Plan 41 Этап 3. */
    char* slot_top = _t_arena.base + (slot + 1) * _t_arena.slot_size;
    char* usable_base = _t_arena.base + slot * _t_arena.slot_size + NOVA_FIBER_GUARD_SIZE;
    size_t usable_size = (size_t)(slot_top - usable_base);
#ifdef MADV_DONTNEED
    madvise(usable_base, usable_size, MADV_DONTNEED);
#else
    (void)usable_size;
#endif
}

bool nova_fiber_arena_contains(const void* ptr) {
    if (!_t_arena.base) return false;
    return (const char*)ptr >= _t_arena.base &&
           (const char*)ptr <  _t_arena.base + _t_arena.virtual_size;
}

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

#else /* NOVA_FIBER_ARENA_ENABLED == 0 — Windows or unsupported */

/* No-op stub for unsupported platforms. fiber_arena.h declarations
 * absent под этой ветке, but we keep a translation unit to satisfy
 * the build (no need to conditionally compile .c file inclusion). */
typedef int _nova_fiber_arena_disabled_marker;

#endif /* NOVA_FIBER_ARENA_ENABLED */
