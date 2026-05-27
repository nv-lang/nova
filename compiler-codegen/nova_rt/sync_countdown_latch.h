// SPDX-License-Identifier: MIT OR Apache-2.0
// Plan 103.4 Agent-C — CountDownLatch: one-shot count-down rendezvous.
//
// Java-style: immutable initial count, only count_down() decreases.
// await() parks fiber until count reaches 0.
// count_down() at count==0 is a no-op (saturating semantic, Java parity).
// new(count) panics unconditionally if count <= 0.
//
// Design mirrors WaitGroup waiter model:
//  - doubly-linked list of parked fibers (NovaCDLWaiter, stack-allocated)
//  - all waiters woken when count reaches 0 (WakeAll semantics)
//  - try_await_for: libuv timer + NovaCDLTLFHandle (malloc'd, freed in close_cb)
//
// GC-race fix (M:N Windows): Nova_CountDownLatch allocated via
// nova_alloc_uncollectable (same as Mutex — Plan 103.3 Discovery).
//
// Runtime invariants: ALL checked unconditionally (not NOVA_SYNC_ASSERT),
// per Plan 103.5 Discovery "unconditional-invariant" pattern.
//
// Included from sync_primitives.h (which is included from nova_rt.h).
// Prerequisite headers (included first in nova_rt.h):
//   effects.h     — Nova_Fail_fail, nova_throw, nova_str_from_cstr
//   fibers.h      — NovaFiberQueue, _nova_active_scope, _nova_active_slot
//   sched.h       — nova_sched_park_with_unlock, nova_sched_wake
//   sync.h        — nova_mutex_t, nova_mutex_init, nova_mutex_lock/unlock
//   libuv          — uv_timer_t, uv_timer_init/start, uv_close
//
// C name mangling (ExternalRegistry auto-derives from sync.nv):
//   CountDownLatch.new(count)        → Nova_CountDownLatch_static_new
//   CountDownLatch @count_down()     → Nova_CountDownLatch_method_count_down
//   CountDownLatch @count_down_n(n)  → Nova_CountDownLatch_method_count_down_n
//   CountDownLatch @await()          → Nova_CountDownLatch_method_await
//   CountDownLatch @try_await()      → Nova_CountDownLatch_method_try_await
//   CountDownLatch @try_await_for(t) → Nova_CountDownLatch_method_try_await_for
//   CountDownLatch @current_count()  → Nova_CountDownLatch_method_current_count

#ifndef NOVA_RT_SYNC_COUNTDOWN_LATCH_H
#define NOVA_RT_SYNC_COUNTDOWN_LATCH_H

/* ── TLF handle (try_await_for timer state) ─────────────────────── */

/* NovaCDLTLFHandle is raw-malloc'd (NOT GC-managed). Lifecycle:
 *   allocated in try_await_for() before park.
 *   timer_cb or close_cb frees it (via _nova_cdl_tlf_close_cb).
 *
 * Protocol (all under cdl->mu for serialization):
 *   - try_await_for(): alloc handle, enqueue timed waiter, start timer, park.
 *   - count reaches 0 (wake path): nullify handle->waiter, wake fiber.
 *     Timer fires next but sees waiter==NULL → no-op.
 *   - timer fires first: remove waiter from queue, set timed_out=true,
 *     wake fiber, nullify handle->waiter, call uv_close.
 *   - close_cb: frees handle (raw malloc). */

typedef struct NovaCDLTLFHandle {
    uv_timer_t  timer;    /* embedded; must be first (timer.data = handle) */
    void*       cdl;      /* Nova_CountDownLatch* — void* for forward-compat */
    void*       waiter;   /* NovaCDLWaiter* or NULL when race is resolved */
} NovaCDLTLFHandle;

static void _nova_cdl_tlf_close_cb(uv_handle_t* h) {
    free(h->data);   /* free NovaCDLTLFHandle (raw malloc) */
}

/* ── CDL waiter (stack-allocated in parking fiber's frame) ─────── */

typedef struct NovaCDLWaiter {
    NovaFiberQueue*        scope;
    int                    slot;
    struct NovaCDLWaiter*  next;
    struct NovaCDLWaiter*  prev;
    bool                   timed_out;   /* set by timer_cb before wake */
    NovaCDLTLFHandle*      tlf_handle;  /* NULL for plain await() waiters */
} NovaCDLWaiter;

/* ── CountDownLatch ─────────────────────────────────────────────── */

/* One-shot count-down rendezvous.
 *
 * new(count > 0): initialize with immutable count.
 * count_down(): decrement; if reaches 0 → wake all waiters.
 * await(): park until count == 0.
 *
 * Key difference vs WaitGroup:
 *  - initial count is fixed; only count_down() (no add()).
 *  - count_down at count==0 is a no-op (saturating), never panics.
 *  - new(0) panics (must have at least one thing to count down). */
typedef struct {
    nova_mutex_t   mu;    /* guards count + waiter list */
    int            count; /* current count; monotonically non-increasing */
    NovaCDLWaiter* head;
    NovaCDLWaiter* tail;
} Nova_CountDownLatch;

/* ── Timer callback (defined after Nova_CountDownLatch) ─────────── */

static void _nova_cdl_tlf_timer_cb(uv_timer_t* h) {
    NovaCDLTLFHandle* handle = (NovaCDLTLFHandle*)h->data;
    Nova_CountDownLatch* cdl = (Nova_CountDownLatch*)handle->cdl;
    nova_mutex_lock(&cdl->mu);
    NovaCDLWaiter* w = (NovaCDLWaiter*)handle->waiter;
    if (w != NULL) {
        /* Timer won the race — remove waiter from queue. */
        if (w->prev) w->prev->next = w->next;
        else         cdl->head = w->next;
        if (w->next) w->next->prev = w->prev;
        else         cdl->tail = w->prev;
        w->timed_out   = true;
        handle->waiter = NULL;
        NovaFiberQueue* scope = w->scope;
        int slot = w->slot;
        nova_mutex_unlock(&cdl->mu);
        nova_sched_wake(scope, slot);
    } else {
        /* count reached 0 first: waiter already woken. Timer fires as no-op. */
        nova_mutex_unlock(&cdl->mu);
    }
    uv_close((uv_handle_t*)h, _nova_cdl_tlf_close_cb);
}

/* ── Internal helper: wake all waiters (called with mu held, unlocks inside) */

static inline void _nova_cdl_wake_all(Nova_CountDownLatch* cdl) {
    NovaCDLWaiter* w = cdl->head;
    cdl->head = NULL;
    cdl->tail = NULL;
    nova_mutex_unlock(&cdl->mu);
    /* Walk waiter list and wake each. For try_await_for waiters: nullify
     * handle->waiter so timer_cb becomes a no-op when it eventually fires. */
    while (w) {
        NovaCDLWaiter* nxt = w->next;
        if (w->tlf_handle) w->tlf_handle->waiter = NULL;
        nova_sched_wake(w->scope, w->slot);
        w = nxt;
    }
}

/* ── Constructor ────────────────────────────────────────────────── */

static inline Nova_CountDownLatch* Nova_CountDownLatch_static_new(nova_int count) {
    /* Unconditional invariant check — not NOVA_SYNC_ASSERT (Plan 103.5 pattern).
     * count == 0 would mean "already done" with no way to count down, which is
     * always a programmer error. */
    if (count <= 0) {
        Nova_Fail_fail(nova_str_from_cstr("CountDownLatch.new: count must be > 0"));
        nova_throw(nova_str_from_cstr("CountDownLatch.new: count must be > 0"));
    }
    /* Plan 103.3 GC-race fix: alloc uncollectable prevents Boehm GC from
     * freeing the latch while a worker thread holds it on a shadow stack
     * during M:N scheduler materialization on Windows. */
    Nova_CountDownLatch* cdl =
        (Nova_CountDownLatch*)nova_alloc_uncollectable(sizeof(Nova_CountDownLatch));
    nova_mutex_init(&cdl->mu);
    cdl->count = (int)count;
    cdl->head  = NULL;
    cdl->tail  = NULL;
    return cdl;
}

/* ── count_down() ───────────────────────────────────────────────── */

static inline nova_unit Nova_CountDownLatch_method_count_down(Nova_CountDownLatch* cdl) {
    nova_mutex_lock(&cdl->mu);
    if (cdl->count == 0) {
        /* Saturating at 0: no-op (Java parity). Not a panic. */
        nova_mutex_unlock(&cdl->mu);
        return NOVA_UNIT;
    }
    cdl->count -= 1;
    if (cdl->count == 0) {
        _nova_cdl_wake_all(cdl);   /* unlocks inside */
    } else {
        nova_mutex_unlock(&cdl->mu);
    }
    return NOVA_UNIT;
}

/* ── count_down_n(n) ────────────────────────────────────────────── */

static inline nova_unit Nova_CountDownLatch_method_count_down_n(Nova_CountDownLatch* cdl,
                                                                  nova_int n) {
    nova_mutex_lock(&cdl->mu);
    if (cdl->count == 0) {
        /* Already at 0: saturating no-op. */
        nova_mutex_unlock(&cdl->mu);
        return NOVA_UNIT;
    }
    if (n <= 0) {
        /* Negative or zero n: no-op. */
        nova_mutex_unlock(&cdl->mu);
        return NOVA_UNIT;
    }
    /* Saturating subtract: clamp to 0 instead of going negative. */
    if (n >= (nova_int)cdl->count) {
        cdl->count = 0;
    } else {
        cdl->count -= (int)n;
    }
    if (cdl->count == 0) {
        _nova_cdl_wake_all(cdl);   /* unlocks inside */
    } else {
        nova_mutex_unlock(&cdl->mu);
    }
    return NOVA_UNIT;
}

/* ── await() ────────────────────────────────────────────────────── */

static inline nova_unit Nova_CountDownLatch_method_await(Nova_CountDownLatch* cdl) {
    nova_mutex_lock(&cdl->mu);
    if (cdl->count == 0) {
        /* Fast path: already done. */
        nova_mutex_unlock(&cdl->mu);
        return NOVA_UNIT;
    }
    if (_nova_active_slot < 0) {
        /* Non-fiber path: spin with CPU yield (covers test / teardown callers). */
        nova_mutex_unlock(&cdl->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&cdl->mu);
            if (cdl->count == 0) {
                nova_mutex_unlock(&cdl->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&cdl->mu);
        }
    }
    /* Fiber path: register as waiter and park atomically with mu release.
     * park_with_unlock parks the fiber first, then releases the mutex.
     * This prevents lost-wakeup: count_down cannot fire the wakeup until
     * after the fiber is registered in the scheduler's park queue. */
    NovaCDLWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.next       = NULL;
    w.prev       = cdl->tail;
    w.timed_out  = false;
    w.tlf_handle = NULL;
    if (cdl->tail) cdl->tail->next = &w;
    else           cdl->head = &w;
    cdl->tail = &w;
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &cdl->mu);
    /* Resumed: count reached 0 — all waiters woken. */
    return NOVA_UNIT;
}

/* ── try_await() ────────────────────────────────────────────────── */

static inline nova_bool Nova_CountDownLatch_method_try_await(Nova_CountDownLatch* cdl) {
    nova_mutex_lock(&cdl->mu);
    nova_bool done = (nova_bool)(cdl->count == 0);
    nova_mutex_unlock(&cdl->mu);
    return done;
}

/* ── try_await_for(Duration) ────────────────────────────────────── */

/* Park up to timeout waiting for count to reach 0.
 * Returns true if count reached 0, false if timeout expired.
 * timeout <= 0: behaves as try_await() (non-blocking).
 * Fiber path: arms a libuv timer; parks until count==0 or timer fires.
 * Non-fiber path: spin-poll until deadline. */
static inline nova_bool Nova_CountDownLatch_method_try_await_for(Nova_CountDownLatch* cdl,
                                                                   void* timeout) {
    /* timeout is Nova_Duration* — void* avoids include-order dep;
     * first field is int64_t nanos. */
    int64_t nanos = *(int64_t*)timeout;
    if (nanos <= 0) return Nova_CountDownLatch_method_try_await(cdl);

    /* Fast path: already done? */
    nova_mutex_lock(&cdl->mu);
    if (cdl->count == 0) {
        nova_mutex_unlock(&cdl->mu);
        return true;
    }

    if (_nova_active_slot < 0) {
        /* Non-fiber: spin-poll with deadline. */
        nova_mutex_unlock(&cdl->mu);
        int64_t deadline = _nova_monotonic_ns() + nanos;
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&cdl->mu);
            if (cdl->count == 0) {
                nova_mutex_unlock(&cdl->mu);
                return true;
            }
            nova_mutex_unlock(&cdl->mu);
            if (_nova_monotonic_ns() >= deadline) return false;
        }
    }

    /* Fiber path: set up libuv timer + register as timed waiter. */
    uint64_t delay_ms = (uint64_t)((nanos + 999999LL) / 1000000LL);
    if (delay_ms == 0) delay_ms = 1;

    /* Allocate timer handle on heap (libuv owns until close_cb frees it). */
    NovaCDLTLFHandle* handle =
        (NovaCDLTLFHandle*)malloc(sizeof(NovaCDLTLFHandle));
    if (!handle) {
        nova_mutex_unlock(&cdl->mu);
        fprintf(stderr, "nova: CountDownLatch.try_await_for: malloc failed\n");
        abort();
    }
    handle->cdl        = (void*)cdl;
    handle->timer.data = handle;

    /* Waiter struct is on the fiber's stack — valid until the fiber returns. */
    NovaCDLWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.timed_out  = false;
    w.tlf_handle = handle;
    handle->waiter = &w;

    /* Enqueue waiter (under mu held since fast-path check above). */
    w.next = NULL;
    w.prev = cdl->tail;
    if (cdl->tail) cdl->tail->next = &w;
    else           cdl->head = &w;
    cdl->tail = &w;

    /* Start timer (safe to call under mu — doesn't block). */
    int rc = uv_timer_init(nova_current_loop(), &handle->timer);
    if (rc != 0) {
        /* Remove waiter from queue and bail. */
        if (w.prev) w.prev->next = NULL; else cdl->head = NULL;
        cdl->tail = w.prev;
        nova_mutex_unlock(&cdl->mu);
        free(handle);
        return false;
    }
    rc = uv_timer_start(&handle->timer, _nova_cdl_tlf_timer_cb, delay_ms, 0);
    if (rc != 0) {
        if (w.prev) w.prev->next = NULL; else cdl->head = NULL;
        cdl->tail = w.prev;
        nova_mutex_unlock(&cdl->mu);
        uv_close((uv_handle_t*)&handle->timer, _nova_cdl_tlf_close_cb);
        return false;
    }

    /* Park atomically with mu release. */
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &cdl->mu);
    /* Resumed: either count==0 (timed_out=false) or timer fired (timed_out=true). */
    return !w.timed_out;
}

/* ── current_count() ────────────────────────────────────────────── */

/* Best-effort snapshot of current count. May be stale by the time
 * the caller reads the result. Not for synchronization — only for
 * observability (debugging, metrics). */
static inline nova_int Nova_CountDownLatch_method_current_count(Nova_CountDownLatch* cdl) {
    nova_mutex_lock(&cdl->mu);
    nova_int c = (nova_int)cdl->count;
    nova_mutex_unlock(&cdl->mu);
    return c;
}

#endif /* NOVA_RT_SYNC_COUNTDOWN_LATCH_H */
