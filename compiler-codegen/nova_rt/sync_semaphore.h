// SPDX-License-Identifier: MIT OR Apache-2.0
// Plan 103.4 — Semaphore (Agent A)
//
// Fiber-aware counting semaphore. Bounded permits (M11).
// Fair FIFO waiter queue (Plan 103.3 M6 consistency).
//
// Included from sync_primitives.h after all other sync headers.
// Depends on: nova_sched_{park_with_unlock,wake}, nova_alloc_uncollectable,
//             Nova_Fail_fail, nova_throw, uv_timer_*, _nova_active_scope/slot,
//             _nova_cpu_yield, _nova_monotonic_ns, NovaMutexTLFHandle pattern.
//
// Naming convention: Nova_Semaphore_{static|method}_<name>[_<suffix>].
// All runtime invariants are unconditional (fire in Dev AND Release — per
// Plan 103.5 Discovery: NOVA_SYNC_ASSERT is a no-op in Dev mode, avoided).

#ifndef NOVA_RT_SYNC_SEMAPHORE_H
#define NOVA_RT_SYNC_SEMAPHORE_H

/* ── Semaphore TLF (timer) handle ─────────────────────────────────────────
 *
 * Used by try_acquire_for(Duration): raw-malloc'd (NOT GC-managed).
 * Lifecycle: allocated in try_acquire_for before park; freed in close_cb.
 * Protocol (under sem->mu):
 *  - try_acquire_for: alloc handle, enqueue timed waiter, start timer, park.
 *  - On acquire (release transfers permits): set handle->waiter=NULL, wake.
 *    Timer fires later, sees waiter==NULL, calls uv_close.
 *  - On timeout (timer fires first): remove waiter from queue,
 *    set waiter->timed_out=true, set handle->waiter=NULL, wake fiber. */
typedef struct NovaSemaphoreTLFHandle {
    uv_timer_t  timer;    /* embedded; must be first (timer.data = handle) */
    void*       sem;      /* Nova_Semaphore* — void* avoids forward-decl dep */
    void*       waiter;   /* NovaSemaphoreWaiter* or NULL */
} NovaSemaphoreTLFHandle;

static void _nova_sem_tlf_close_cb(uv_handle_t* h) {
    free(h->data);  /* free NovaSemaphoreTLFHandle (raw malloc) */
}

/* ── Semaphore waiter ──────────────────────────────────────────────────────
 *
 * Stack-allocated in the parking fiber's call frame — same pattern as
 * NovaMutexWaiter. Safe because the fiber stack (8 MB) persists until resume.
 * Zero-init: timed_out=false, tlf_handle=NULL for plain acquire() callers. */
typedef struct NovaSemaphoreWaiter {
    NovaFiberQueue*             scope;
    int                         slot;
    struct NovaSemaphoreWaiter* next;
    struct NovaSemaphoreWaiter* prev;
    nova_int                    permits_needed;  /* 1 for acquire(); n for acquire_n(n) */
    bool                        timed_out;       /* set by timer_cb before wake */
    NovaSemaphoreTLFHandle*     tlf_handle;      /* NULL for plain waiters */
} NovaSemaphoreWaiter;

/* ── Nova_Semaphore ────────────────────────────────────────────────────────
 *
 * Fiber-aware counting semaphore. Bounded permits (M11): `new(permits)` sets
 * the initial count. Permits may exceed initial via over-release (D170 §2:
 * Java-compatible — no over-release panic in V1; W_SEMAPHORE_OVER_RELEASE
 * warning is a V2 optional addition).
 *
 * Fair FIFO: waiters queued in FIFO order. FIFO fairness invariant:
 *   if head waiter needs k permits and available < k → stop waking (even if
 *   later waiters could be satisfied). Prevents starvation of large acquire_n.
 *
 * NOT reentrant acquire: calling acquire() while already holding a permit is
 * not tracked. Deadlock possible if permits exhausted and caller re-acquires. */
typedef struct {
    nova_mutex_t          mu;       /* guards permits + waiter list */
    nova_int              permits;  /* current available permits (int64_t) */
    NovaSemaphoreWaiter*  head;     /* FIFO front */
    NovaSemaphoreWaiter*  tail;     /* FIFO back */
} Nova_Semaphore;

/* Forward-declare timer callback (defined after Nova_Semaphore). */
static void _nova_sem_tlf_timer_cb(uv_timer_t* h);

/* ── Constructor ───────────────────────────────────────────────────────────
 *
 * Plan 103.3 GC-race fix: uncollectable allocation (same as Mutex/RwLock/
 * ReentrantMutex). Windows M:N armed mode may trigger GC during
 * _ensure_materialized before the Nova GC roots capture this pointer. */
static inline Nova_Semaphore* Nova_Semaphore_static_new(nova_int permits) {
    /* Unconditional: permits must be >= 0 (fires in Dev AND Release). */
    if (permits < 0) {
        Nova_Fail_fail(nova_str_from_cstr(
            "Semaphore.new: initial permits must be >= 0"));
        nova_throw(nova_str_from_cstr(
            "Semaphore.new: initial permits must be >= 0"));
    }
    Nova_Semaphore* s = (Nova_Semaphore*)nova_alloc_uncollectable(sizeof(Nova_Semaphore));
    nova_mutex_init(&s->mu);
    s->permits = permits;
    s->head    = NULL;
    s->tail    = NULL;
    return s;
}

/* ── Internal: wake front-of-queue waiters (under s->mu held) ─────────────
 *
 * Pops head waiters while permits >= head->permits_needed (FIFO fairness).
 * Skips timed-out waiters (tlf_handle->waiter == NULL). On each wake,
 * permits are decremented BEFORE the fiber resumes — no re-check needed. */
static inline void _nova_sem_wake_waiters(Nova_Semaphore* s) {
    while (s->head) {
        NovaSemaphoreWaiter* w = s->head;
        /* Skip timed-out waiters that timer_cb already removed from time-slot. */
        if (w->tlf_handle && w->tlf_handle->waiter == NULL) {
            s->head = w->next;
            if (s->head) s->head->prev = NULL;
            else         s->tail = NULL;
            continue;
        }
        /* FIFO: stop if head needs more permits than available. */
        if (s->permits < w->permits_needed) break;
        /* Transfer permits — deducted before wake (waiter does NOT re-check). */
        s->permits -= w->permits_needed;
        s->head = w->next;
        if (s->head) s->head->prev = NULL;
        else         s->tail = NULL;
        /* Nullify tlf_handle->waiter so timer_cb becomes no-op on timeout race. */
        if (w->tlf_handle) w->tlf_handle->waiter = NULL;
        NovaFiberQueue* scope = w->scope;
        int             slot  = w->slot;
        nova_sched_wake(scope, slot);
    }
}

/* ── acquire_n(n) ──────────────────────────────────────────────────────────
 *
 * Acquire n permits atomically. Parks until n permits available.
 * Fair FIFO: if waiters are queued OR permits < n, enqueue self.
 * Unconditional: n must be >= 1. */
static inline nova_unit Nova_Semaphore_method_acquire_n(Nova_Semaphore* s, nova_int n) {
    if (n <= 0) {
        Nova_Fail_fail(nova_str_from_cstr(
            "Semaphore.acquire_n: n must be >= 1"));
        nova_throw(nova_str_from_cstr(
            "Semaphore.acquire_n: n must be >= 1"));
    }
    nova_mutex_lock(&s->mu);
    /* Fast path: no waiters AND enough permits (maintains FIFO — don't jump queue). */
    if (s->head == NULL && s->permits >= n) {
        s->permits -= n;
        nova_mutex_unlock(&s->mu);
        return NOVA_UNIT;
    }
    if (_nova_active_slot < 0) {
        /* Non-fiber: spin until permits available. */
        nova_mutex_unlock(&s->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&s->mu);
            if (s->permits >= n) {
                s->permits -= n;
                nova_mutex_unlock(&s->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&s->mu);
        }
    }
    /* Fiber path: enqueue at tail and park. */
    NovaSemaphoreWaiter w;
    w.scope          = _nova_active_scope;
    w.slot           = _nova_active_slot;
    w.next           = NULL;
    w.prev           = s->tail;
    w.permits_needed = n;
    w.timed_out      = false;
    w.tlf_handle     = NULL;
    if (s->tail) s->tail->next = &w;
    else         s->head = &w;
    s->tail = &w;
    /* park_with_unlock: parks fiber first, then releases mu atomically.
     * Prevents lost-wakeup race (release_n cannot fire before park registered). */
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &s->mu);
    /* Resumed: permits already decremented by _nova_sem_wake_waiters. */
    return NOVA_UNIT;
}

/* ── acquire() ─────────────────────────────────────────────────────────── */
/* Plan 103.9: Nova_Semaphore_method_acquire returns Nova_Permit* (V2 guard API).
 * Old callers discarding the result still compile — C ignores non-void returns. */
static inline Nova_Permit* Nova_Semaphore_method_acquire(Nova_Semaphore* s) {
    Nova_Semaphore_method_acquire_n(s, 1);
    Nova_Permit* _p = (Nova_Permit*)nova_alloc(sizeof(Nova_Permit));
    _p->ptr = (nova_int)(uintptr_t)s;
    return _p;
}

/* ── release_n(n) ─────────────────────────────────────────────────────────
 *
 * Release n permits. Increments permits then wakes front-of-queue waiters.
 * D170 §2: over-release (permits > initial) allowed (Java semantics, V1).
 * Unconditional: n must be >= 1. */
static inline nova_unit Nova_Semaphore_method_release_n(Nova_Semaphore* s, nova_int n) {
    if (n <= 0) {
        Nova_Fail_fail(nova_str_from_cstr(
            "Semaphore.release_n: n must be >= 1"));
        nova_throw(nova_str_from_cstr(
            "Semaphore.release_n: n must be >= 1"));
    }
    nova_mutex_lock(&s->mu);
    s->permits += n;
    _nova_sem_wake_waiters(s);
    nova_mutex_unlock(&s->mu);
    return NOVA_UNIT;
}

/* ── release() ─────────────────────────────────────────────────────────── */
static inline nova_unit Nova_Semaphore_method_release(Nova_Semaphore* s) {
    return Nova_Semaphore_method_release_n(s, 1);
}

/* ── try_acquire() ─────────────────────────────────────────────────────── */
static inline nova_bool Nova_Semaphore_method_try_acquire(Nova_Semaphore* s) {
    nova_mutex_lock(&s->mu);
    /* Fair: only fast-path if no waiters (don't jump the queue). */
    if (s->head == NULL && s->permits > 0) {
        s->permits--;
        nova_mutex_unlock(&s->mu);
        return true;
    }
    nova_mutex_unlock(&s->mu);
    return false;
}

/* ── try_acquire_for(Duration) ─────────────────────────────────────────────
 *
 * Attempt to acquire within timeout. Returns true if acquired, false if
 * timeout expired. timeout <= 0 → behaves as try_acquire().
 * Fiber path: arms a libuv timer; parks until permit acquired or timer fires.
 * Non-fiber path: spin-poll until deadline.
 *
 * timeout is Nova_Duration* — void* avoids include-order dependency;
 * first field is int64_t nanos. */
static inline nova_bool Nova_Semaphore_method_try_acquire_for(Nova_Semaphore* s,
                                                               void* timeout) {
    int64_t nanos = *(int64_t*)timeout;
    if (nanos <= 0) return Nova_Semaphore_method_try_acquire(s);

    /* Fast path: check immediately before any timer setup. */
    nova_mutex_lock(&s->mu);
    if (s->head == NULL && s->permits > 0) {
        s->permits--;
        nova_mutex_unlock(&s->mu);
        return true;
    }

    if (_nova_active_slot < 0) {
        /* Non-fiber: spin-poll with deadline. */
        nova_mutex_unlock(&s->mu);
        int64_t deadline = _nova_monotonic_ns() + nanos;
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&s->mu);
            if (s->permits > 0) {
                s->permits--;
                nova_mutex_unlock(&s->mu);
                return true;
            }
            nova_mutex_unlock(&s->mu);
            if (_nova_monotonic_ns() >= deadline) return false;
        }
    }

    /* Fiber path: set up timer + register as timed waiter. */
    uint64_t delay_ms = (uint64_t)((nanos + 999999LL) / 1000000LL);
    if (delay_ms == 0) delay_ms = 1;

    /* Allocate timer state on heap (libuv owns until close_cb frees). */
    NovaSemaphoreTLFHandle* handle = (NovaSemaphoreTLFHandle*)malloc(sizeof(NovaSemaphoreTLFHandle));
    if (!handle) {
        nova_mutex_unlock(&s->mu);
        fprintf(stderr, "nova: Semaphore.try_acquire_for: malloc failed\n");
        abort();
    }
    handle->sem        = (void*)s;
    handle->timer.data = handle;

    /* Stack waiter (valid until fiber returns from this function). */
    NovaSemaphoreWaiter w;
    w.scope          = _nova_active_scope;
    w.slot           = _nova_active_slot;
    w.next           = NULL;
    w.prev           = s->tail;
    w.permits_needed = 1;
    w.timed_out      = false;
    w.tlf_handle     = handle;
    handle->waiter   = &w;

    /* Enqueue at tail (under mu held since fast-path check above). */
    if (s->tail) s->tail->next = &w;
    else         s->head = &w;
    s->tail = &w;

    /* Start timer. */
    int rc = uv_timer_init(nova_current_loop(), &handle->timer);
    if (rc != 0) {
        /* Remove waiter and bail. */
        if (w.prev) w.prev->next = NULL; else s->head = NULL;
        s->tail = w.prev;
        nova_mutex_unlock(&s->mu);
        free(handle);
        return false;
    }
    rc = uv_timer_start(&handle->timer, _nova_sem_tlf_timer_cb, delay_ms, 0);
    if (rc != 0) {
        if (w.prev) w.prev->next = NULL; else s->head = NULL;
        s->tail = w.prev;
        nova_mutex_unlock(&s->mu);
        uv_close((uv_handle_t*)&handle->timer, _nova_sem_tlf_close_cb);
        return false;
    }

    /* Park atomically with mu release. */
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &s->mu);
    /* Resumed: timed_out=false → permit acquired; timed_out=true → timeout. */
    return !w.timed_out;
}

/* ── available_permits() ───────────────────────────────────────────────────
 *
 * Best-effort observability. NOT for synchronization decisions.
 * Relaxed load: consistent with Plan 103.3 is_locked() pattern. */
static inline nova_int Nova_Semaphore_method_available_permits(const Nova_Semaphore* s) {
    return (nova_int)__atomic_load_n(&s->permits, __ATOMIC_RELAXED);
}

/* ── Timer callback (fires when try_acquire_for timeout expires) ───────────
 *
 * Defined after Nova_Semaphore (used Nova_Semaphore* via handle->sem void*). */
static void _nova_sem_tlf_timer_cb(uv_timer_t* h) {
    NovaSemaphoreTLFHandle* handle = (NovaSemaphoreTLFHandle*)h->data;
    Nova_Semaphore* s = (Nova_Semaphore*)handle->sem;
    nova_mutex_lock(&s->mu);
    NovaSemaphoreWaiter* w = (NovaSemaphoreWaiter*)handle->waiter;
    if (w != NULL) {
        /* Timer won the race — remove waiter from FIFO queue. */
        if (w->prev) w->prev->next = w->next;
        else         s->head = w->next;
        if (w->next) w->next->prev = w->prev;
        else         s->tail = w->prev;
        w->timed_out   = true;
        handle->waiter = NULL;
        NovaFiberQueue* scope = w->scope;
        int slot = w->slot;
        nova_mutex_unlock(&s->mu);
        nova_sched_wake(scope, slot);
    } else {
        /* release_n already transferred permits to this waiter: no-op. */
        nova_mutex_unlock(&s->mu);
    }
    uv_close((uv_handle_t*)h, _nova_sem_tlf_close_cb);
}

#endif /* NOVA_RT_SYNC_SEMAPHORE_H */
