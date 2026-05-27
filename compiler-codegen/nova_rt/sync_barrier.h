// SPDX-License-Identifier: MIT OR Apache-2.0
// Plan 103.4 — Barrier (CyclicBarrier-style, reusable N-party rendezvous).
// Included from sync_primitives.h via AGENT-B marker after array.h + effects.h.
//
// Design (Ф.2):
//  - Nova_Barrier holds mu + arrived_count + generation + broken flag + FIFO waiter list.
//  - wait(): last arrival (index = parties_total-1) completes the round — increments
//    generation, resets arrived_count, wakes all parked waiters.
//  - wait_with_action(): same as wait(), but last arrival executes action BEFORE waking
//    other waiters (mutex released during action to avoid re-entrancy deadlock).
//  - wait_for(timeout Duration): libuv timer pattern (Plan 103.3 M6); on timeout the
//    timer callback marks ALL current waiters as broken and leaves barrier broken.
//  - reset(): force-break; marks all current waiters broken, wakes them; resets
//    arrived_count=0, generation++, broken=false (ready for fresh use).
//  - is_broken(): atomic-load of broken flag (best-effort; no mutex needed).
//
// GC: nova_alloc_uncollectable — identical to Mutex (Plan 103.3 GC-race fix).
// Waiter structs: stack-allocated in parking fiber's call frame (same as WaitGroup/Once).
//
// Non-fiber path: spin-poll generation/broken. Real barrier usage requires fibers;
// the non-fiber path exists for test scaffolding consistency.
//
// INVARIANTS (unconditional — fire in Dev AND Release, per §Discovery from 103.5):
//  - Nova_Barrier_static_new: parties <= 0 → runtime panic.
//  - wait/wait_with_action/wait_for: broken barrier → panic (must call reset() first).

#ifndef NOVA_RT_SYNC_BARRIER_H
#define NOVA_RT_SYNC_BARRIER_H

/* ── Barrier TLF handle (timer-for-wait_for) ─────────────────────────
 *
 * Raw-malloc'd (NOT GC-managed). Lifecycle:
 *   Allocated in wait_for() before park.
 *   Timer_cb or close_cb frees it (via _nova_barrier_tlf_close_cb).
 *
 * Protocol (all under b->mu):
 *   - wait_for(): alloc handle, enqueue timed waiter, start timer, park.
 *   - On normal completion (last arrival wakes everyone): set handle->waiter=NULL
 *     before wake. Timer fires later as no-op.
 *   - On timeout (timer fires first): remove waiter, break barrier, wake all,
 *     set handle->waiter=NULL, uv_close.
 *   - close_cb: frees handle (raw malloc).
 */

typedef struct NovaBarrierTLFHandle {
    uv_timer_t  timer;    /* embedded; MUST be first (timer.data = handle) */
    void*       barrier;  /* Nova_Barrier* — void* avoids forward-decl order issues */
    void*       waiter;   /* NovaBarrierWaiter* or NULL when no longer needed */
} NovaBarrierTLFHandle;

static void _nova_barrier_tlf_close_cb(uv_handle_t* h) {
    free(h->data);   /* free NovaBarrierTLFHandle (raw malloc'd) */
}

/* ── Barrier waiter ──────────────────────────────────────────────── */

typedef struct NovaBarrierWaiter {
    NovaFiberQueue*           scope;
    int                       slot;
    struct NovaBarrierWaiter* next;
    struct NovaBarrierWaiter* prev;
    nova_int                  my_index;   /* arrival index captured before park */
    bool                      broken;     /* set before wake by timer_cb or reset() */
    bool                      timed_out;  /* set before wake by timer_cb (own fiber) */
    NovaBarrierTLFHandle*     tlf_handle; /* NULL for wait() / wait_with_action() */
} NovaBarrierWaiter;

/* ── Barrier ─────────────────────────────────────────────────────── */

/*
 * Nova_Barrier — reusable N-party rendezvous (CyclicBarrier-style).
 *
 * Lifecycle per round:
 *   1. Each of the N fibers calls wait() / wait_with_action() / wait_for().
 *   2. The last arrival (arrived_count == parties_total) resets arrived_count=0,
 *      increments generation, and wakes all parked waiters.
 *   3. A new round can begin immediately.
 *
 * Broken state: set by reset() or by a wait_for() timeout. Any wait() on a
 * broken barrier panics immediately; call reset() to repair.
 *
 * Thread-safety: all mutable state under mu. is_broken() uses atomic read.
 * GC: nova_alloc_uncollectable (Plan 103.3 M6 GC-race fix for M:N Windows).
 */
typedef struct {
    nova_mutex_t        mu;
    nova_int            parties_total;
    nova_int            arrived_count;
    nova_int            generation;    /* monotonically increasing; wraps on int overflow */
    bool                broken;        /* set by timer_cb (wait_for timeout) or reset() */
    NovaBarrierWaiter*  head;          /* FIFO waiter queue head */
    NovaBarrierWaiter*  tail;          /* FIFO waiter queue tail */
} Nova_Barrier;

/* ── Forward declaration for timer callback ──────────────────────── */
static void _nova_barrier_tlf_timer_cb(uv_timer_t* h);

/* ── Internal helpers (all called under b->mu) ───────────────────── */

/* Enqueue a stack-allocated waiter at the FIFO tail. */
static void _nova_barrier_enqueue(Nova_Barrier* b, NovaBarrierWaiter* w) {
    w->next = NULL;
    w->prev = b->tail;
    if (b->tail) b->tail->next = w;
    else         b->head = w;
    b->tail = w;
}

/* Remove a waiter from the queue (used by timer_cb when timer fires first). */
static void _nova_barrier_dequeue(Nova_Barrier* b, NovaBarrierWaiter* w) {
    if (w->prev) w->prev->next = w->next;
    else         b->head = w->next;
    if (w->next) w->next->prev = w->prev;
    else         b->tail = w->prev;
}

/* Mark all current waiters as broken, null out their TLF handles, and detach
 * the waiter list. Does NOT modify b->broken (caller decides).
 * Returns the detached head for the caller to wake. */
static NovaBarrierWaiter* _nova_barrier_mark_and_detach(Nova_Barrier* b) {
    NovaBarrierWaiter* cur = b->head;
    while (cur) {
        cur->broken = true;
        if (cur->tlf_handle) cur->tlf_handle->waiter = NULL;
        cur = cur->next;
    }
    NovaBarrierWaiter* head = b->head;
    b->head = b->tail = NULL;
    return head;
}

/* Complete a barrier round: reset arrived_count, increment generation, null out
 * TLF handles (so their timers become no-ops when they eventually fire), and
 * detach the waiter list. Returns the detached head for the caller to wake. */
static NovaBarrierWaiter* _nova_barrier_complete_round(Nova_Barrier* b) {
    b->arrived_count = 0;
    b->generation++;
    NovaBarrierWaiter* cur = b->head;
    while (cur) {
        if (cur->tlf_handle) cur->tlf_handle->waiter = NULL;
        cur = cur->next;
    }
    NovaBarrierWaiter* head = b->head;
    b->head = b->tail = NULL;
    return head;
}

/* Wake a detached chain of waiters (must NOT hold b->mu). */
static void _nova_barrier_wake_chain(NovaBarrierWaiter* head) {
    while (head) {
        NovaBarrierWaiter* next = head->next;
        nova_sched_wake(head->scope, head->slot);
        head = next;
    }
}

/* ── Constructor ─────────────────────────────────────────────────── */

static inline Nova_Barrier* Nova_Barrier_static_new(nova_int parties) {
    /* Unconditional invariant check (fires in Dev AND Release — §Discovery from 103.5). */
    if (parties <= 0) {
        Nova_Fail_fail(nova_str_from_cstr("Barrier.new(): parties must be >= 1"));
        nova_throw(nova_str_from_cstr("Barrier.new(): parties must be >= 1"));
    }
    /* nova_alloc_uncollectable: prevents GC race in M:N scheduler (Plan 103.3 M6 fix).
     * Returns zeroed memory. */
    Nova_Barrier* b = (Nova_Barrier*)nova_alloc_uncollectable(sizeof(Nova_Barrier));
    nova_mutex_init(&b->mu);
    b->parties_total = parties;
    b->arrived_count = 0;
    b->generation    = 0;
    b->broken        = false;
    b->head          = NULL;
    b->tail          = NULL;
    return b;
}

/* ── Timer callback (fires when wait_for timeout expires) ──────────
 *
 * If handle->waiter != NULL: this fiber timed out.
 *   - Remove the timed-out waiter from the queue.
 *   - Mark all other current waiters as broken.
 *   - Set b->broken = true (stays until reset()).
 *   - Reset arrived_count=0, generation++.
 *   - Wake all: timed-out fiber (timed_out=true) + all others (broken=true).
 *
 * If handle->waiter == NULL: normal completion already handled; no-op.
 */
static void _nova_barrier_tlf_timer_cb(uv_timer_t* h) {
    NovaBarrierTLFHandle* handle = (NovaBarrierTLFHandle*)h->data;
    Nova_Barrier* b = (Nova_Barrier*)handle->barrier;
    nova_mutex_lock(&b->mu);
    NovaBarrierWaiter* w = (NovaBarrierWaiter*)handle->waiter;
    if (w != NULL) {
        /* Timer won the race — this fiber timed out. */
        _nova_barrier_dequeue(b, w);
        handle->waiter = NULL;
        w->timed_out   = true;
        /* Mark all remaining waiters in this generation as broken. */
        NovaBarrierWaiter* others = _nova_barrier_mark_and_detach(b);
        /* Break the barrier permanently (until reset()). */
        b->broken        = true;
        b->arrived_count = 0;
        b->generation++;
        nova_mutex_unlock(&b->mu);
        /* Wake the timed-out fiber, then all broken others. */
        nova_sched_wake(w->scope, w->slot);
        _nova_barrier_wake_chain(others);
    } else {
        /* Normal completion already transferred this waiter's slot; no-op. */
        nova_mutex_unlock(&b->mu);
    }
    uv_close((uv_handle_t*)h, _nova_barrier_tlf_close_cb);
}

/* ── wait() -> int ───────────────────────────────────────────────── */

static inline nova_int Nova_Barrier_method_wait(Nova_Barrier* b) {
    nova_mutex_lock(&b->mu);
    /* Unconditional broken-check (fires Dev AND Release). */
    if (b->broken) {
        nova_mutex_unlock(&b->mu);
        Nova_Fail_fail(nova_str_from_cstr("Barrier.wait(): barrier is broken (call reset() to repair)"));
        nova_throw(nova_str_from_cstr("Barrier.wait(): barrier is broken (call reset() to repair)"));
    }
    b->arrived_count++;
    nova_int my_index = b->arrived_count - 1;

    if (b->arrived_count == b->parties_total) {
        /* Last arrival — complete the round. */
        NovaBarrierWaiter* chain = _nova_barrier_complete_round(b);
        nova_mutex_unlock(&b->mu);
        _nova_barrier_wake_chain(chain);
        return my_index;
    }

    if (_nova_active_slot < 0) {
        /* Non-fiber path: spin-poll generation/broken. */
        nova_int my_gen = b->generation;
        nova_mutex_unlock(&b->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&b->mu);
            if (b->broken) {
                nova_mutex_unlock(&b->mu);
                Nova_Fail_fail(nova_str_from_cstr("Barrier.wait(): barrier is broken"));
                nova_throw(nova_str_from_cstr("Barrier.wait(): barrier is broken"));
            }
            if (b->generation != my_gen) {
                nova_mutex_unlock(&b->mu);
                return my_index;
            }
            nova_mutex_unlock(&b->mu);
        }
    }

    /* Fiber path: register as waiter and park atomically with mutex release.
     * park_with_unlock prevents lost-wakeup race. */
    NovaBarrierWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.my_index   = my_index;
    w.broken     = false;
    w.timed_out  = false;
    w.tlf_handle = NULL;
    _nova_barrier_enqueue(b, &w);
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &b->mu);
    /* Resumed. Reason: broken (reset/timeout) or normal completion. */
    if (w.broken) {
        Nova_Fail_fail(nova_str_from_cstr("Barrier.wait(): barrier was reset or timed out by another party"));
        nova_throw(nova_str_from_cstr("Barrier.wait(): barrier was reset or timed out by another party"));
    }
    return my_index;
}

/* ── wait_with_action(action fn() -> ()) -> int ─────────────────── */
/*
 * Identical to wait() except: the last-arrival fiber executes `action`
 * AFTER releasing b->mu but BEFORE waking other waiters.
 *
 * This ensures:
 *   1. action cannot deadlock by calling barrier methods (mutex is released).
 *   2. Other waiters see the effect of action before they proceed.
 *
 * action: void* cast to NovaClosBase* — fn() -> () closure.
 */
static inline nova_int Nova_Barrier_method_wait_with_action(Nova_Barrier* b, void* action) {
    nova_mutex_lock(&b->mu);
    if (b->broken) {
        nova_mutex_unlock(&b->mu);
        Nova_Fail_fail(nova_str_from_cstr("Barrier.wait_with_action(): barrier is broken"));
        nova_throw(nova_str_from_cstr("Barrier.wait_with_action(): barrier is broken"));
    }
    b->arrived_count++;
    nova_int my_index = b->arrived_count - 1;

    if (b->arrived_count == b->parties_total) {
        /* Last arrival — complete round, execute action, then wake. */
        NovaBarrierWaiter* chain = _nova_barrier_complete_round(b);
        nova_mutex_unlock(&b->mu);
        /* Execute action BEFORE waking waiters ("last-arrival fiber executes action
         * inside barrier before wake" — §Ф.2). Mutex is released, so re-entry is safe. */
        if (action != NULL) {
            NovaClosBase* clos = (NovaClosBase*)action;
            ((nova_unit(*)(void*))clos->fn)(clos->env);
        }
        _nova_barrier_wake_chain(chain);
        return my_index;
    }

    if (_nova_active_slot < 0) {
        /* Non-fiber: spin. */
        nova_int my_gen = b->generation;
        nova_mutex_unlock(&b->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&b->mu);
            if (b->broken) {
                nova_mutex_unlock(&b->mu);
                Nova_Fail_fail(nova_str_from_cstr("Barrier.wait_with_action(): barrier is broken"));
                nova_throw(nova_str_from_cstr("Barrier.wait_with_action(): barrier is broken"));
            }
            if (b->generation != my_gen) {
                nova_mutex_unlock(&b->mu);
                return my_index;
            }
            nova_mutex_unlock(&b->mu);
        }
    }

    NovaBarrierWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.my_index   = my_index;
    w.broken     = false;
    w.timed_out  = false;
    w.tlf_handle = NULL;
    _nova_barrier_enqueue(b, &w);
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &b->mu);
    if (w.broken) {
        Nova_Fail_fail(nova_str_from_cstr("Barrier.wait_with_action(): barrier was reset or timed out"));
        nova_throw(nova_str_from_cstr("Barrier.wait_with_action(): barrier was reset or timed out"));
    }
    return my_index;
}

/* ── wait_for(timeout Duration) -> Option[int] ───────────────────── */
/*
 * Like wait() but with a timeout. Returns:
 *   Option.Some(arrival_index) — barrier completed normally within timeout.
 *   Option.None — timeout expired OR barrier already broken.
 *
 * Timeout semantics: if this fiber times out, the timer callback breaks the
 * barrier (b->broken=true) and wakes ALL other current waiters with broken=true.
 * Those waiters' wait() / wait_with_action() will panic on return. This matches
 * Java CyclicBarrier.await(time, unit) semantics: one timeout → all wake broken.
 *
 * timeout: Nova_Duration* cast to void*; first field is int64_t nanos.
 */
static inline NovaOpt_nova_int Nova_Barrier_method_wait_for(Nova_Barrier* b, void* timeout) {
    int64_t nanos = *(int64_t*)timeout;
    if (nanos <= 0) {
        /* Non-blocking: check immediately without parking. */
        nova_mutex_lock(&b->mu);
        nova_bool is_broken = b->broken;
        nova_mutex_unlock(&b->mu);
        (void)is_broken;
        /* With zero/negative timeout we cannot wait for other parties. */
        return nova_make_Option_None();
    }

    nova_mutex_lock(&b->mu);
    if (b->broken) {
        nova_mutex_unlock(&b->mu);
        return nova_make_Option_None();  /* broken → caller may check is_broken() */
    }
    b->arrived_count++;
    nova_int my_index = b->arrived_count - 1;

    if (b->arrived_count == b->parties_total) {
        /* Last arrival — complete normally. */
        NovaBarrierWaiter* chain = _nova_barrier_complete_round(b);
        nova_mutex_unlock(&b->mu);
        _nova_barrier_wake_chain(chain);
        return nova_make_Option_Some(my_index);
    }

    if (_nova_active_slot < 0) {
        /* Non-fiber: spin-poll with deadline. */
        nova_int my_gen = b->generation;
        nova_mutex_unlock(&b->mu);
        int64_t deadline = _nova_monotonic_ns() + nanos;
        for (;;) {
            _nova_cpu_yield();
            if (_nova_monotonic_ns() >= deadline) {
                /* Timed out: undo arrived_count if still in same generation. */
                nova_mutex_lock(&b->mu);
                if (b->generation == my_gen) {
                    b->arrived_count--;
                }
                nova_mutex_unlock(&b->mu);
                return nova_make_Option_None();
            }
            nova_mutex_lock(&b->mu);
            if (b->broken) {
                /* Broken by another timeout or reset(). */
                nova_mutex_unlock(&b->mu);
                return nova_make_Option_None();
            }
            if (b->generation != my_gen) {
                /* Generation changed — barrier completed (or reset). */
                nova_mutex_unlock(&b->mu);
                return nova_make_Option_Some(my_index);
            }
            nova_mutex_unlock(&b->mu);
        }
    }

    /* Fiber path: arm libuv timer + enqueue timed waiter. */
    uint64_t delay_ms = (uint64_t)((nanos + 999999LL) / 1000000LL);
    if (delay_ms == 0) delay_ms = 1;

    NovaBarrierTLFHandle* handle = (NovaBarrierTLFHandle*)malloc(sizeof(NovaBarrierTLFHandle));
    if (!handle) {
        /* malloc failed: undo arrived_count and bail without parking. */
        b->arrived_count--;
        nova_mutex_unlock(&b->mu);
        fprintf(stderr, "nova: Barrier.wait_for: malloc failed\n");
        return nova_make_Option_None();
    }
    handle->barrier    = (void*)b;
    handle->timer.data = handle;

    /* Stack waiter (valid until fiber returns from this function). */
    NovaBarrierWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.my_index   = my_index;
    w.broken     = false;
    w.timed_out  = false;
    w.tlf_handle = handle;
    handle->waiter = &w;

    _nova_barrier_enqueue(b, &w);

    int rc = uv_timer_init(nova_current_loop(), &handle->timer);
    if (rc != 0) {
        _nova_barrier_dequeue(b, &w);
        b->arrived_count--;
        nova_mutex_unlock(&b->mu);
        free(handle);
        return nova_make_Option_None();
    }
    rc = uv_timer_start(&handle->timer, _nova_barrier_tlf_timer_cb, delay_ms, 0);
    if (rc != 0) {
        _nova_barrier_dequeue(b, &w);
        b->arrived_count--;
        nova_mutex_unlock(&b->mu);
        uv_close((uv_handle_t*)&handle->timer, _nova_barrier_tlf_close_cb);
        return nova_make_Option_None();
    }

    /* Park atomically with mutex release (prevents lost-wakeup race). */
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &b->mu);
    /* Resumed: timed_out=true (timer fired for us), broken=true (other timeout/reset),
     * or neither (normal completion — _nova_barrier_complete_round nulled handle->waiter). */
    if (w.timed_out || w.broken) {
        return nova_make_Option_None();
    }
    return nova_make_Option_Some(my_index);
}

/* ── is_broken() -> bool ─────────────────────────────────────────── */

static inline nova_bool Nova_Barrier_method_is_broken(const Nova_Barrier* b) {
    /* Acquire-load: observability without mutex (best-effort). */
    return (nova_bool)__atomic_load_n((const bool*)&b->broken, __ATOMIC_ACQUIRE);
}

/* ── reset() ─────────────────────────────────────────────────────── */
/*
 * Force-reset: mark all current waiters as broken and wake them; reset
 * arrived_count=0, generation++, broken=false. After reset() the barrier
 * is ready for a fresh generation. Waiters woken by reset() will panic
 * (w.broken=true → wait() / wait_with_action() throw).
 */
static inline nova_unit Nova_Barrier_method_reset(Nova_Barrier* b) {
    nova_mutex_lock(&b->mu);
    /* Mark all current waiters as broken and detach them. */
    NovaBarrierWaiter* chain = _nova_barrier_mark_and_detach(b);
    /* Reset state: clear broken, reset counters, increment generation. */
    b->arrived_count = 0;
    b->generation++;
    b->broken = false;  /* clear for fresh use */
    nova_mutex_unlock(&b->mu);
    /* Wake the detached chain — each waiter will see w.broken=true on resume. */
    _nova_barrier_wake_chain(chain);
    return NOVA_UNIT;
}

#endif /* NOVA_RT_SYNC_BARRIER_H */
