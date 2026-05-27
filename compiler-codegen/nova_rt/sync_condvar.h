/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * sync_condvar.h — Nova_Condvar: fiber-aware condition variable.
 * Plan 103.4 (D170 §Ф.4): Condvar tied to Mutex (M10).
 *
 * MUST be included AFTER Nova_Mutex and Nova_ReentrantMutex are declared
 * (i.e., from sync_primitives.h after those definitions).
 *
 * ── Design ─────────────────────────────────────────────────────────────
 *
 *  wait(m): register waiter in cv's FIFO list (under cv->mu), then park
 *    fiber while holding cv->mu. The combined deferred-unlock callback
 *    releases BOTH cv->mu AND the user mutex atomically after yield
 *    (Plan 44.5 Layer 5). This eliminates the lost-wakeup race where
 *    notify could fire between cv->mu release and park.
 *
 *  wait_for(m, timeout): same + libuv timer (Plan 22). Timer armed
 *    inside cv->mu critical section before park. Timer fires →
 *    dequeue waiter, set timed_out=true, wake fiber. notify fires first →
 *    dequeue waiter, nullify tlf_handle->waiter (timer becomes no-op).
 *    Returns WaitResult.Notified | WaitResult.TimedOut after re-acquiring m.
 *
 *  wait_until(m, pred): predicate loop — calls wait_Mutex until pred
 *    returns true. C implementation accepts NovaClosBase* for fn()->bool
 *    closure (Plan 103.4 §Ф.4, same pattern as Once.call_once).
 *
 *  notify_one(): dequeue FIFO head, wake its fiber.
 *  notify_all(): dequeue all, wake all fibers (FIFO order).
 *
 * ── Lost-wakeup prevention ──────────────────────────────────────────────
 *  park_with_unlock sets parked[slot]=true BEFORE yielding (Plan 44.5).
 *  By holding cv->mu until AFTER parked[slot]=true (via deferred callback),
 *  notify cannot dequeue+wake the fiber while parked[slot] is false.
 *  Without this, notify fires as a no-op and the fiber parks forever.
 *
 * ── Spurious wakeup ─────────────────────────────────────────────────────
 *  wait() may return without notify (M:N rebalance).
 *  Must wrap in predicate loop. Use Nova-body wait_until(m, pred) helper.
 *
 * ── ReentrantMutex interaction ──────────────────────────────────────────
 *  wait(rm): saves lock_count, releases ALL levels (count→0), re-acquires
 *  as count=1 on wake. Java-pitfall-aware: does NOT restore original count.
 *  emit_c.rs emits W_REENTRANT_CONDVAR_RECOMMEND when ReentrantMutex used.
 *
 * ── Preconditions ───────────────────────────────────────────────────────
 *  mutex must be locked before calling wait() / wait_for().
 *  Unconditional runtime panic if not (fires in Dev AND Release — Plan 103.3 pattern).
 */

#ifndef NOVA_RT_SYNC_CONDVAR_H
#define NOVA_RT_SYNC_CONDVAR_H

/* ── WaitResult (Plan 103.4, D170) ──────────────────────────────────────
 *
 * Return type for Condvar.wait_for().
 *   Notified = 0  — woken by notify_one() or notify_all()
 *   TimedOut  = 1 — timeout expired without notification
 *
 * Tag values coordinated with emit_c.rs (RUNTIME_DEFINED_TYPES "WaitResult").
 * Declared with named-typedef pattern (same as Nova_OnceState in Plan 103.5)
 * to avoid conflict with emit_c.rs forward-decl path. */

typedef enum {
    NOVA_TAG_WaitResult_Notified = 0,
    NOVA_TAG_WaitResult_TimedOut = 1,
} Nova_WaitResult_Tag;

typedef struct Nova_WaitResult Nova_WaitResult;
struct Nova_WaitResult {
    Nova_WaitResult_Tag tag;
};

static inline Nova_WaitResult* nova_make_WaitResult_Notified(void) {
    Nova_WaitResult* r = (Nova_WaitResult*)nova_alloc(sizeof(Nova_WaitResult));
    r->tag = NOVA_TAG_WaitResult_Notified;
    return r;
}
static inline Nova_WaitResult* nova_make_WaitResult_TimedOut(void) {
    Nova_WaitResult* r = (Nova_WaitResult*)nova_alloc(sizeof(Nova_WaitResult));
    r->tag = NOVA_TAG_WaitResult_TimedOut;
    return r;
}

/* ── Condvar TLF handle (wait_for timer state) ──────────────────────────
 *
 * Same lifecycle pattern as NovaMutexTLFHandle (Plan 103.3):
 *   - Allocated on heap (malloc) in wait_for() before park.
 *   - timer.data = handle (uv_timer callback uses this).
 *   - handle->waiter set to NULL by notify path so timer_cb becomes no-op.
 *   - Freed by close_cb (called from uv_close in timer_cb). */

typedef struct {
    uv_timer_t  timer;    /* embedded; must be first (timer.data = handle) */
    void*       condvar;  /* Nova_Condvar* — forward-compatible (struct below) */
    void*       waiter;   /* NovaCondvarWaiter* or NULL */
} NovaCondvarTLFHandle;

static void _nova_condvar_tlf_close_cb(uv_handle_t* h) {
    free(h->data);   /* free NovaCondvarTLFHandle (raw malloc) */
}

/* ── Condvar waiter ──────────────────────────────────────────────────────
 *
 * Stack-allocated (valid while fiber is in wait/wait_for).
 * Protected by cv->mu when accessing/modifying the list. */

typedef struct NovaCondvarWaiter {
    NovaFiberQueue*           scope;
    int                       slot;
    struct NovaCondvarWaiter* next;
    struct NovaCondvarWaiter* prev;
    bool                      timed_out;    /* set by timer_cb before wake */
    NovaCondvarTLFHandle*     tlf_handle;   /* NULL for plain wait() waiters */
} NovaCondvarWaiter;

/* ── Condvar struct ──────────────────────────────────────────────────────
 *
 * FIFO waiter queue (Plan 103.3 M6 consistency).
 * Nova_Condvar allocated uncollectable (same GC-race fix as Nova_Mutex). */

typedef struct {
    nova_mutex_t       mu;    /* guards head/tail waiter list */
    NovaCondvarWaiter* head;
    NovaCondvarWaiter* tail;
} Nova_Condvar;

/* Forward-declare timer callback (defined after Nova_Condvar and its deps). */
static void _nova_condvar_tlf_timer_cb(uv_timer_t* h);

/* ── Constructor ─────────────────────────────────────────────────────── */

static inline Nova_Condvar* Nova_Condvar_static_new(void) {
    /* Uncollectable for same GC-race reason as Nova_Mutex (Plan 103.3):
     * Boehm may miss the pointer on the main thread stack during M:N
     * worker materialization, causing premature collection. */
    Nova_Condvar* cv = (Nova_Condvar*)nova_alloc_uncollectable(sizeof(Nova_Condvar));
    nova_mutex_init(&cv->mu);
    cv->head = NULL;
    cv->tail = NULL;
    return cv;
}

/* ── Timer callback (fires when wait_for timeout expires) ────────────── */

static void _nova_condvar_tlf_timer_cb(uv_timer_t* h) {
    NovaCondvarTLFHandle* handle = (NovaCondvarTLFHandle*)h->data;
    Nova_Condvar* cv = (Nova_Condvar*)handle->condvar;
    nova_mutex_lock(&cv->mu);
    NovaCondvarWaiter* w = (NovaCondvarWaiter*)handle->waiter;
    if (w != NULL) {
        /* Timer won the race — dequeue waiter from cv's list. */
        if (w->prev) w->prev->next = w->next;
        else         cv->head = w->next;
        if (w->next) w->next->prev = w->prev;
        else         cv->tail = w->prev;
        w->timed_out   = true;
        handle->waiter = NULL;
        NovaFiberQueue* scope = w->scope;
        int slot = w->slot;
        nova_mutex_unlock(&cv->mu);
        nova_sched_wake(scope, slot);
    } else {
        /* notify_one/all already woke this waiter — no-op. */
        nova_mutex_unlock(&cv->mu);
    }
    uv_close((uv_handle_t*)h, _nova_condvar_tlf_close_cb);
}

/* ── Combined park_with_unlock callbacks ─────────────────────────────────
 *
 * Plan 103.4 §Ф.4 lost-wakeup fix: hold cv->mu THROUGH park_with_unlock.
 * park_with_unlock (Plan 44.5 Layer 5):
 *   1. Sets parked[slot]=true.
 *   2. Stores callback in TLS.
 *   3. Calls mco_yield — control returns to scheduler.
 *   4. Scheduler calls callback AFTER yield (cv->mu still held at this point).
 *   5. Callback releases cv->mu + user mutex.
 *
 * This guarantees notify cannot dequeue + wake the fiber while
 * parked[slot]==false — eliminating the lost-wakeup race entirely. */

/* Context for combined Condvar-internal + Mutex unlock. */
typedef struct {
    Nova_Condvar* cv;      /* release cv->mu after yield */
    Nova_Mutex*   user_m;  /* release user mutex after yield */
} _CondvarWaitMuCtx;

/* Callback: release cv->mu then user Nova_Mutex. */
static void _condvar_unlock_both_mu_cb(void* ctx_ptr) {
    _CondvarWaitMuCtx* ctx = (_CondvarWaitMuCtx*)ctx_ptr;
    nova_mutex_unlock(&ctx->cv->mu);
    Nova_Mutex_method_unlock(ctx->user_m);
}

/* Context for combined Condvar-internal + ReentrantMutex full-release. */
typedef struct {
    Nova_Condvar*        cv;   /* release cv->mu after yield */
    Nova_ReentrantMutex* rm;   /* force-release ALL lock levels after yield */
} _CondvarWaitRMCtx;

/* Callback: release cv->mu then force-release ALL ReentrantMutex lock levels.
 * Design (Plan 103.4 §Ф.4): on wake, caller re-acquires as count=1.
 * Does NOT restore original lock_count (Java-pitfall-aware). */
static void _condvar_unlock_both_rm_cb(void* ctx_ptr) {
    _CondvarWaitRMCtx* ctx = (_CondvarWaitRMCtx*)ctx_ptr;
    nova_mutex_unlock(&ctx->cv->mu);
    Nova_ReentrantMutex* rm = ctx->rm;
    /* Force lock_count = 1 so the next Nova_ReentrantMutex_method_unlock
     * fully releases ownership (sets locked=false, wakes next waiter). */
    nova_mutex_lock(&rm->mu);
    rm->lock_count = 1;
    nova_mutex_unlock(&rm->mu);
    Nova_ReentrantMutex_method_unlock(rm);
}

/* ── wait(m mut Mutex) ────────────────────────────────────────────────── */

static inline nova_unit Nova_Condvar_method_wait_Mutex(Nova_Condvar* cv,
                                                        Nova_Mutex* m) {
    /* Precondition: m must be locked (Plan 103.4 §«Discovery 103.5»:
     * unconditional check — fires in Dev AND Release). */
    if (!Nova_Mutex_method_is_locked(m)) {
        Nova_Fail_fail(nova_str_from_cstr(
            "Condvar.wait(): mutex not locked — "
            "caller must hold mutex before calling wait()"));
        nova_throw(nova_str_from_cstr(
            "Condvar.wait(): mutex not locked — "
            "caller must hold mutex before calling wait()"));
    }

    /* Register self as FIFO waiter (under cv->mu). */
    NovaCondvarWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.next       = NULL;
    w.timed_out  = false;
    w.tlf_handle = NULL;

    nova_mutex_lock(&cv->mu);
    w.prev = cv->tail;
    if (cv->tail) cv->tail->next = &w;
    else          cv->head = &w;
    cv->tail = &w;

    /* Hold cv->mu through park (lost-wakeup fix, Plan 103.4 §Ф.4):
     * Combined callback releases cv->mu AND user mutex atomically after yield.
     * notify_one/all cannot dequeue+wake this fiber while parked[slot]==false
     * because they must lock cv->mu first (which we still hold). */
    _CondvarWaitMuCtx both_ctx = { cv, m };
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 _condvar_unlock_both_mu_cb, &both_ctx);

    /* Fiber resumed (either notify or spurious wake).
     * Spurious wakeup contract: caller must use predicate loop or wait_until. */
    Nova_Mutex_method_lock(m);
    return NOVA_UNIT;
}

/* ── wait(m mut ReentrantMutex) — ReentrantMutex overload ───────────────
 *
 * Releases ENTIRE recursive lock count (→0); re-acquires as count=1.
 * Design: intentionally does NOT restore original lock_count on wake.
 * Java-pitfall-aware (Plan 103.4 §Ф.4).
 * emit_c.rs emits W_REENTRANT_CONDVAR_RECOMMEND for calls with this type. */

static inline nova_unit Nova_Condvar_method_wait_ReentrantMutex(
        Nova_Condvar* cv, Nova_ReentrantMutex* rm) {
    /* Precondition: rm locked by current fiber. */
    nova_mutex_lock(&rm->mu);
    bool owned = rm->locked && (rm->owner_coro == mco_running());
    nova_mutex_unlock(&rm->mu);
    if (!owned) {
        Nova_Fail_fail(nova_str_from_cstr(
            "Condvar.wait(): ReentrantMutex not locked by current fiber — "
            "caller must hold ReentrantMutex before calling wait()"));
        nova_throw(nova_str_from_cstr(
            "Condvar.wait(): ReentrantMutex not locked by current fiber — "
            "caller must hold ReentrantMutex before calling wait()"));
    }

    /* Register self as FIFO waiter. */
    NovaCondvarWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.next       = NULL;
    w.timed_out  = false;
    w.tlf_handle = NULL;

    nova_mutex_lock(&cv->mu);
    w.prev = cv->tail;
    if (cv->tail) cv->tail->next = &w;
    else          cv->head = &w;
    cv->tail = &w;

    /* Hold cv->mu through park — combined callback releases cv->mu AND rm
     * (all lock levels). Context lives on fiber's stack, valid while suspended. */
    _CondvarWaitRMCtx both_ctx = { cv, rm };
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 _condvar_unlock_both_rm_cb, &both_ctx);

    /* Re-acquire as count=1 (intentional: not restoring original count). */
    Nova_ReentrantMutex_method_lock(rm);
    return NOVA_UNIT;
}

/* ── wait_for(m mut Mutex, timeout Duration) → WaitResult ───────────────
 *
 * Same as wait_Mutex() but arms a libuv timer for timeout.
 * Timer is armed INSIDE cv->mu critical section (before park) to prevent
 * lost-wakeup: notify cannot fire between enqueue and park.
 * Race: notify vs timer resolved by handle->waiter NULL-check.
 * In all cases (Notified or TimedOut), re-acquires m before returning. */

static inline Nova_WaitResult* Nova_Condvar_method_wait_for(
        Nova_Condvar* cv, Nova_Mutex* m, void* timeout) {
    /* timeout is Nova_Duration* — first field is int64_t nanos. */
    int64_t nanos = *(int64_t*)timeout;

    /* Precondition: m locked (unconditional). */
    if (!Nova_Mutex_method_is_locked(m)) {
        Nova_Fail_fail(nova_str_from_cstr(
            "Condvar.wait_for(): mutex not locked — "
            "caller must hold mutex before calling wait_for()"));
        nova_throw(nova_str_from_cstr(
            "Condvar.wait_for(): mutex not locked — "
            "caller must hold mutex before calling wait_for()"));
    }

    /* Zero/negative timeout: non-blocking check. Release and re-acquire m. */
    if (nanos <= 0) {
        Nova_Mutex_method_unlock(m);
        Nova_Mutex_method_lock(m);
        return nova_make_WaitResult_TimedOut();
    }

    if (_nova_active_slot < 0) {
        /* Non-fiber path: spin-poll until deadline. */
        Nova_Mutex_method_unlock(m);
        int64_t deadline = _nova_monotonic_ns() + nanos;
        while (_nova_monotonic_ns() < deadline) _nova_cpu_yield();
        Nova_Mutex_method_lock(m);
        return nova_make_WaitResult_TimedOut();
    }

    uint64_t delay_ms = (uint64_t)((nanos + 999999LL) / 1000000LL);
    if (delay_ms == 0) delay_ms = 1;

    /* Allocate TLF handle on heap (freed by close_cb). */
    NovaCondvarTLFHandle* handle =
        (NovaCondvarTLFHandle*)malloc(sizeof(NovaCondvarTLFHandle));
    if (!handle) {
        fprintf(stderr, "nova: Condvar.wait_for: malloc failed\n");
        abort();
    }
    handle->condvar    = (void*)cv;
    handle->timer.data = handle;

    /* Stack waiter (valid until fiber returns from this function). */
    NovaCondvarWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.timed_out  = false;
    w.tlf_handle = handle;
    handle->waiter = &w;

    /* Hold cv->mu for enqueue + timer arm + through park.
     * Lost-wakeup fix (Plan 103.4 §Ф.4): notify and timer_cb both lock
     * cv->mu before dequeuing — they cannot fire while we hold it.
     * uv_timer_init/start are non-blocking registration calls, safe under lock. */
    nova_mutex_lock(&cv->mu);
    w.next = NULL;
    w.prev = cv->tail;
    if (cv->tail) cv->tail->next = &w;
    else          cv->head = &w;
    cv->tail = &w;

    /* Arm timer (inside cv->mu — prevents race with immediate timeout). */
    int rc = uv_timer_init(nova_current_loop(), &handle->timer);
    if (rc != 0) {
        /* Timer init failed — dequeue waiter under cv->mu and return TimedOut. */
        if (w.prev) w.prev->next = w.next; else cv->head = w.next;
        if (w.next) w.next->prev = w.prev; else cv->tail = w.prev;
        nova_mutex_unlock(&cv->mu);
        free(handle);
        Nova_Mutex_method_lock(m);
        return nova_make_WaitResult_TimedOut();
    }
    rc = uv_timer_start(&handle->timer, _nova_condvar_tlf_timer_cb, delay_ms, 0);
    if (rc != 0) {
        if (w.prev) w.prev->next = w.next; else cv->head = w.next;
        if (w.next) w.next->prev = w.prev; else cv->tail = w.prev;
        nova_mutex_unlock(&cv->mu);
        uv_close((uv_handle_t*)&handle->timer, _nova_condvar_tlf_close_cb);
        Nova_Mutex_method_lock(m);
        return nova_make_WaitResult_TimedOut();
    }

    /* Park atomically — combined callback releases cv->mu AND user mutex. */
    _CondvarWaitMuCtx both_ctx = { cv, m };
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 _condvar_unlock_both_mu_cb, &both_ctx);

    /* Resumed: check which path fired (timer vs notify). Re-acquire m. */
    Nova_Mutex_method_lock(m);
    return w.timed_out
        ? nova_make_WaitResult_TimedOut()
        : nova_make_WaitResult_Notified();
}

/* ── wait_until(m mut Mutex, predicate fn()->bool) ──────────────────────
 *
 * Predicate loop: calls wait_Mutex() in a loop until pred() returns true.
 * Handles spurious wakeup automatically.
 *
 * Plan 103.4 §Ф.4: pred is a fn()->bool closure (NovaClosBase layout).
 * C calling convention: NovaClosBase = { void* fn; void* env }.
 * Call: ((nova_bool(*)(void*))(pred->fn))(pred->env).
 *
 * Same closure pattern as Once.call_once (NovaClosBase*, Plan 103.5). */

static inline nova_unit Nova_Condvar_method_wait_until(
        Nova_Condvar* cv, Nova_Mutex* m, NovaClosBase* pred) {
    while (!((nova_bool (*)(void*))(pred->fn))(pred->env)) {
        Nova_Condvar_method_wait_Mutex(cv, m);
    }
    return NOVA_UNIT;
}

/* ── notify_one() ────────────────────────────────────────────────────── */

static inline nova_unit Nova_Condvar_method_notify_one(Nova_Condvar* cv) {
    nova_mutex_lock(&cv->mu);
    NovaCondvarWaiter* w = cv->head;
    if (w != NULL) {
        /* Dequeue FIFO head (fair: oldest waiter woken first). */
        cv->head = w->next;
        if (cv->head) cv->head->prev = NULL;
        else          cv->tail = NULL;
        /* Nullify handle->waiter so timer_cb fires as no-op (Plan 103.3 pattern). */
        if (w->tlf_handle) w->tlf_handle->waiter = NULL;
        NovaFiberQueue* scope = w->scope;
        int slot = w->slot;
        nova_mutex_unlock(&cv->mu);
        nova_sched_wake(scope, slot);
    } else {
        nova_mutex_unlock(&cv->mu);
    }
    return NOVA_UNIT;
}

/* ── notify_all() ───────────────────────────────────────────────────── */

static inline nova_unit Nova_Condvar_method_notify_all(Nova_Condvar* cv) {
    nova_mutex_lock(&cv->mu);
    /* Snapshot entire list under lock, then wake all outside lock. */
    NovaCondvarWaiter* head = cv->head;
    cv->head = NULL;
    cv->tail = NULL;
    nova_mutex_unlock(&cv->mu);
    /* Wake all in FIFO order (head = oldest = first to wake). */
    while (head != NULL) {
        NovaCondvarWaiter* next = head->next;
        if (head->tlf_handle) head->tlf_handle->waiter = NULL;
        nova_sched_wake(head->scope, head->slot);
        head = next;
    }
    return NOVA_UNIT;
}

#endif /* NOVA_RT_SYNC_CONDVAR_H */
