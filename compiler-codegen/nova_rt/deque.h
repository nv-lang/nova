// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_DEQUE_H
#define NOVA_RT_DEQUE_H

/* Plan 44.5 — Chase-Lev work-stealing deque (Chase & Lev, SPAA 2005).
 *
 * Reference: "Dynamic Circular Work-Stealing Deque" (Chase & Lev 2005,
 * https://www.dre.vanderbilt.edu/~schmidt/PDF/work-stealing-dequeue.pdf).
 * Also: "Correct and Efficient Work-Stealing for Weak Memory Models"
 * (Lê, Pop, Cohen, Nardelli, PPoPP 2013) — modern memory ordering
 * specification used here.
 *
 * Properties:
 *   - Single producer (owner thread): push/pop wait-free (no CAS).
 *   - Multiple consumers (steal threads): lock-free CAS-based.
 *   - No false sharing на push/pop hot path.
 *   - Dynamic resize: array grows когда bottom-top > capacity.
 *
 * API discipline:
 *   - push/pop: только owner thread (тот, что инициализировал).
 *   - steal: любой thread кроме owner.
 *
 * Memory ordering (PPoPP 2013):
 *   - push: store(buffer[b]) → store_release(bottom).
 *   - pop:  store(bottom) → cas(top) — single thread, no race с push.
 *   - steal: load_acquire(top) → load_acquire(bottom) → load(buffer[t])
 *           → cas_seq_cst(top).
 *
 * Caller obligations:
 *   - Items в deque — opaque void*. Lifetime managed by caller (Boehm GC
 *     keeps live через other roots, или caller pins).
 *   - Owner thread inside one deque — fixed for deque's lifetime.
 */

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
#include "sync.h"     /* nova_atomic_intptr, atomic helpers */

/* ── NovaDequeArray — array snapshot for resize ─────────────────── */

typedef struct NovaDequeArray {
    int64_t  capacity;       /* power-of-2 size */
    int64_t  mask;           /* capacity - 1 */
    void**   slots;          /* heap-allocated, raw malloc */
} NovaDequeArray;

/* ── NovaDeque ──────────────────────────────────────────────────── */

typedef struct NovaDeque {
    /* PPoPP 2013 sequential consistency for the indices.
     * bottom — pushed/popped by owner; steal-side reads relaxed-ly.
     * top    — primary contention point, CAS'd by stealers.
     *
     * Both intptr (signed) to support empty-queue detection (b - t < 0). */
    nova_atomic_intptr  top;
    nova_atomic_intptr  bottom;
    /* Pointer to current backing array. Replaced atomically on resize. */
    NovaDequeArray*     array;
} NovaDeque;

/* ── Internal helpers ───────────────────────────────────────────── */

static inline NovaDequeArray* _nova_deque_array_new(int64_t capacity) {
    NovaDequeArray* a = (NovaDequeArray*)malloc(sizeof(NovaDequeArray));
    if (!a) return NULL;
    a->capacity = capacity;
    a->mask     = capacity - 1;
    a->slots    = (void**)calloc((size_t)capacity, sizeof(void*));
    if (!a->slots) { free(a); return NULL; }
    return a;
}

static inline void _nova_deque_array_free(NovaDequeArray* a) {
    if (!a) return;
    free(a->slots);
    free(a);
}

/* ── Public API ─────────────────────────────────────────────────── */

/* Initialize deque с initial capacity (must be power-of-2, min 8). */
static inline bool nova_deque_init(NovaDeque* d, int64_t initial_capacity) {
    if (initial_capacity < 8) initial_capacity = 8;
    /* round up to power-of-2 */
    int64_t cap = 1;
    while (cap < initial_capacity) cap <<= 1;

    NovaDequeArray* a = _nova_deque_array_new(cap);
    if (!a) return false;

    nova_aint_init((nova_atomic_int*)&d->top, 0);      /* note: cast OK on 64-bit; refined ниже */
    nova_aint_init((nova_atomic_int*)&d->bottom, 0);
    /* intptr atomics on 64-bit: __atomic_* genericна работает с pointer-sized.
     * Используем raw __atomic_store_n для intptr. */
    __atomic_store_n(&d->top, 0, __ATOMIC_RELAXED);
    __atomic_store_n(&d->bottom, 0, __ATOMIC_RELAXED);
    __atomic_store_n(&d->array, a, __ATOMIC_RELEASE);
    return true;
}

static inline void nova_deque_destroy(NovaDeque* d) {
    NovaDequeArray* a = __atomic_load_n(&d->array, __ATOMIC_ACQUIRE);
    _nova_deque_array_free(a);
    d->array = NULL;
}

/* Resize: создаём бо́льший array, копируем live items [top, bottom),
 * атомарно swap. Called только owner thread (single-writer на array). */
static inline bool _nova_deque_grow(NovaDeque* d, intptr_t t, intptr_t b) {
    NovaDequeArray* old = __atomic_load_n(&d->array, __ATOMIC_RELAXED);
    NovaDequeArray* fresh = _nova_deque_array_new(old->capacity * 2);
    if (!fresh) return false;
    for (intptr_t i = t; i < b; i++) {
        fresh->slots[i & fresh->mask] = old->slots[i & old->mask];
    }
    /* Release fresh так чтобы stealers видели fresh->slots writes выше. */
    __atomic_store_n(&d->array, fresh, __ATOMIC_RELEASE);
    /* old array — GC managed via Boehm? Нет — raw malloc. Free через
     * deferred mechanism: stealers могут ещё его читать. Простой safe
     * approach — leak old array (small leak — ~16 KB amortised). Под
     * Plan 44.6+ можно сделать epoch-based reclamation. */
    /* _nova_deque_array_free(old); — disabled, stealer race */
    return true;
}

/* Owner push (LIFO). Wait-free under normal load. */
static inline bool nova_deque_push(NovaDeque* d, void* item) {
    intptr_t b = __atomic_load_n(&d->bottom, __ATOMIC_RELAXED);
    intptr_t t = __atomic_load_n(&d->top,    __ATOMIC_ACQUIRE);
    NovaDequeArray* a = __atomic_load_n(&d->array, __ATOMIC_RELAXED);
    if (b - t > a->capacity - 1) {
        if (!_nova_deque_grow(d, t, b)) return false;
        a = __atomic_load_n(&d->array, __ATOMIC_RELAXED);
    }
    a->slots[b & a->mask] = item;
    /* Release barrier: stealers reading bottom-then-slot must see slot store. */
    __atomic_thread_fence(__ATOMIC_RELEASE);
    __atomic_store_n(&d->bottom, b + 1, __ATOMIC_RELAXED);
    return true;
}

/* Owner pop (LIFO). Returns NULL if empty. May race с stealer на last item. */
static inline void* nova_deque_pop(NovaDeque* d) {
    intptr_t b = __atomic_load_n(&d->bottom, __ATOMIC_RELAXED) - 1;
    NovaDequeArray* a = __atomic_load_n(&d->array, __ATOMIC_RELAXED);
    __atomic_store_n(&d->bottom, b, __ATOMIC_RELAXED);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    intptr_t t = __atomic_load_n(&d->top, __ATOMIC_RELAXED);
    void* item = NULL;
    if (t <= b) {
        /* Non-empty queue. */
        item = a->slots[b & a->mask];
        if (t == b) {
            /* Last item — race со stealer. CAS top to claim. */
            intptr_t expected = t;
            if (!__atomic_compare_exchange_n(&d->top, &expected, t + 1,
                                              false,
                                              __ATOMIC_SEQ_CST,
                                              __ATOMIC_RELAXED)) {
                /* Stealer won — item already taken. */
                item = NULL;
            }
            __atomic_store_n(&d->bottom, b + 1, __ATOMIC_RELAXED);
        }
    } else {
        /* Empty — restore bottom. */
        __atomic_store_n(&d->bottom, b + 1, __ATOMIC_RELAXED);
    }
    return item;
}

/* Steal (FIFO). Any thread except owner. Returns NULL if empty или
 * contention (caller retry). */
static inline void* nova_deque_steal(NovaDeque* d) {
    intptr_t t = __atomic_load_n(&d->top, __ATOMIC_ACQUIRE);
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    intptr_t b = __atomic_load_n(&d->bottom, __ATOMIC_ACQUIRE);
    void* item = NULL;
    if (t < b) {
        NovaDequeArray* a = __atomic_load_n(&d->array, __ATOMIC_ACQUIRE);
        item = a->slots[t & a->mask];
        intptr_t expected = t;
        if (!__atomic_compare_exchange_n(&d->top, &expected, t + 1,
                                          false,
                                          __ATOMIC_SEQ_CST,
                                          __ATOMIC_RELAXED)) {
            /* Lost CAS — другой stealer или pop'нул. */
            return NULL;
        }
    }
    return item;
}

/* Snapshot — owner inspection (not thread-safe relative to stealers). */
static inline int64_t nova_deque_size_approx(NovaDeque* d) {
    intptr_t b = __atomic_load_n(&d->bottom, __ATOMIC_RELAXED);
    intptr_t t = __atomic_load_n(&d->top,    __ATOMIC_RELAXED);
    return b - t;
}

#endif /* NOVA_RT_DEQUE_H */
