// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_SYNC_PRIMITIVES_H
#define NOVA_RT_SYNC_PRIMITIVES_H

/* Plan 18 std.sync: fiber-aware AtomicInt / Mutex / WaitGroup.
 *
 * Included from nova_rt.h AFTER nova_sched.h (needs nova_sched_park_with_unlock,
 * nova_sched_wake) and fibers.h (_nova_active_scope, _nova_active_slot TLS).
 *
 * Design:
 *  - AtomicInt: thin wrapper around nova_atomic_int (sync.h). No park/wake.
 *  - Mutex: nova_mutex_t guards `locked` + waiter list. Fiber waiters park;
 *    non-fiber callers spin (rare for fiber-based code).
 *  - WaitGroup: nova_mutex_t guards `count` + waiter list. wait() parks when
 *    count > 0; done() wakes all waiters when count reaches 0.
 *
 * Waiter structs are stack-allocated in the parking fiber's call frame —
 * identical to ChannelWaiter pattern in channels.h. Safe because the fiber
 * stack is fixed (8 MB) and persists until the fiber resumes.
 *
 * Non-fiber path (OS thread, _nova_active_slot < 0): spin on unlock. This is
 * acceptable for init/teardown scenarios where M:N is not active.
 */

/* ── AtomicInt ─────────────────────────────────────────────────── */

typedef struct {
    volatile nova_atomic_int value;
} Nova_AtomicInt;

static inline Nova_AtomicInt* Nova_AtomicInt_static_new(nova_int v) {
    Nova_AtomicInt* a = (Nova_AtomicInt*)nova_alloc(sizeof(Nova_AtomicInt));
    nova_aint_init(&a->value, (int32_t)v);
    return a;
}

static inline nova_int Nova_AtomicInt_method_load(const Nova_AtomicInt* a) {
    return (nova_int)nova_aint_load(&a->value);
}

static inline nova_unit Nova_AtomicInt_method_store(Nova_AtomicInt* a, nova_int v) {
    nova_aint_store(&a->value, (int32_t)v);
    return NOVA_UNIT;
}

static inline nova_int Nova_AtomicInt_method_fetch_add(Nova_AtomicInt* a, nova_int delta) {
    return (nova_int)__atomic_fetch_add(&a->value, (int32_t)delta, __ATOMIC_ACQ_REL);
}

static inline nova_int Nova_AtomicInt_method_fetch_sub(Nova_AtomicInt* a, nova_int delta) {
    return (nova_int)__atomic_fetch_sub(&a->value, (int32_t)delta, __ATOMIC_ACQ_REL);
}

static inline nova_bool Nova_AtomicInt_method_compare_exchange(
        Nova_AtomicInt* a, nova_int expected_val, nova_int desired) {
    int32_t exp = (int32_t)expected_val;
    return nova_aint_cas(&a->value, &exp, (int32_t)desired);
}

/* ── Mutex waiter ──────────────────────────────────────────────── */

typedef struct NovaMutexWaiter {
    NovaFiberQueue*         scope;
    int                     slot;
    struct NovaMutexWaiter* next;
    struct NovaMutexWaiter* prev;
} NovaMutexWaiter;

/* ── Mutex ─────────────────────────────────────────────────────── */

typedef struct {
    nova_mutex_t      mu;      /* guards locked + waiter list */
    bool              locked;
    NovaMutexWaiter*  head;
    NovaMutexWaiter*  tail;
} Nova_Mutex;

static inline Nova_Mutex* Nova_Mutex_static_new(void) {
    Nova_Mutex* m = (Nova_Mutex*)nova_alloc(sizeof(Nova_Mutex));
    nova_mutex_init(&m->mu);
    m->locked = false;
    m->head   = NULL;
    m->tail   = NULL;
    return m;
}

static inline nova_bool Nova_Mutex_method_try_lock(Nova_Mutex* m) {
    nova_mutex_lock(&m->mu);
    if (!m->locked) {
        m->locked = true;
        nova_mutex_unlock(&m->mu);
        return true;
    }
    nova_mutex_unlock(&m->mu);
    return false;
}

static inline nova_unit Nova_Mutex_method_lock(Nova_Mutex* m) {
    nova_mutex_lock(&m->mu);
    if (!m->locked) {
        m->locked = true;
        nova_mutex_unlock(&m->mu);
        return NOVA_UNIT;
    }
    if (_nova_active_slot < 0) {
        /* Non-fiber: spin until the lock becomes available. */
        nova_mutex_unlock(&m->mu);
        for (;;) {
            nova_mutex_lock(&m->mu);
            if (!m->locked) {
                m->locked = true;
                nova_mutex_unlock(&m->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&m->mu);
        }
    }
    /* Fiber path: register as waiter and park. */
    NovaMutexWaiter w;
    w.scope = _nova_active_scope;
    w.slot  = _nova_active_slot;
    w.next  = NULL;
    w.prev  = m->tail;
    if (m->tail) m->tail->next = &w;
    else         m->head = &w;
    m->tail = &w;
    /* park_with_unlock: releases mu atomically with park transition (M:N safe). */
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &m->mu);
    /* Resumed: lock ownership transferred from unlock() — no re-check needed. */
    return NOVA_UNIT;
}

static inline nova_unit Nova_Mutex_method_unlock(Nova_Mutex* m) {
    nova_mutex_lock(&m->mu);
    if (m->head) {
        /* Transfer ownership to first waiter: lock stays true. */
        NovaMutexWaiter* w = m->head;
        m->head = w->next;
        if (m->head) m->head->prev = NULL;
        else         m->tail = NULL;
        NovaFiberQueue* scope = w->scope;
        int slot = w->slot;
        nova_mutex_unlock(&m->mu);
        nova_sched_wake(scope, slot);
    } else {
        m->locked = false;
        nova_mutex_unlock(&m->mu);
    }
    return NOVA_UNIT;
}

/* ── WaitGroup waiter ──────────────────────────────────────────── */

typedef struct NovaWGWaiter {
    NovaFiberQueue*      scope;
    int                  slot;
    struct NovaWGWaiter* next;
    struct NovaWGWaiter* prev;
} NovaWGWaiter;

/* ── WaitGroup ─────────────────────────────────────────────────── */

typedef struct {
    nova_mutex_t    mu;    /* guards count + waiter list */
    int             count;
    NovaWGWaiter*   head;
    NovaWGWaiter*   tail;
} Nova_WaitGroup;

static inline Nova_WaitGroup* Nova_WaitGroup_static_new(void) {
    Nova_WaitGroup* wg = (Nova_WaitGroup*)nova_alloc(sizeof(Nova_WaitGroup));
    nova_mutex_init(&wg->mu);
    wg->count = 0;
    wg->head  = NULL;
    wg->tail  = NULL;
    return wg;
}

static inline nova_unit Nova_WaitGroup_method_add(Nova_WaitGroup* wg, nova_int delta) {
    nova_mutex_lock(&wg->mu);
    wg->count += (int)delta;
    nova_mutex_unlock(&wg->mu);
    return NOVA_UNIT;
}

static inline nova_unit Nova_WaitGroup_method_done(Nova_WaitGroup* wg) {
    nova_mutex_lock(&wg->mu);
    wg->count -= 1;
    if (wg->count <= 0) {
        wg->count = 0;
        /* Detach the whole waiter list under lock, then wake outside. */
        NovaWGWaiter* w = wg->head;
        wg->head = NULL;
        wg->tail = NULL;
        nova_mutex_unlock(&wg->mu);
        while (w) {
            NovaWGWaiter* next = w->next;
            nova_sched_wake(w->scope, w->slot);
            w = next;
        }
    } else {
        nova_mutex_unlock(&wg->mu);
    }
    return NOVA_UNIT;
}

static inline nova_unit Nova_WaitGroup_method_wait(Nova_WaitGroup* wg) {
    nova_mutex_lock(&wg->mu);
    if (wg->count <= 0) {
        nova_mutex_unlock(&wg->mu);
        return NOVA_UNIT;
    }
    if (_nova_active_slot < 0) {
        /* Non-fiber: spin until count reaches 0. */
        nova_mutex_unlock(&wg->mu);
        for (;;) {
            nova_mutex_lock(&wg->mu);
            if (wg->count <= 0) {
                nova_mutex_unlock(&wg->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&wg->mu);
        }
    }
    /* Fiber path: register as waiter and park. */
    NovaWGWaiter w;
    w.scope = _nova_active_scope;
    w.slot  = _nova_active_slot;
    w.next  = NULL;
    w.prev  = wg->tail;
    if (wg->tail) wg->tail->next = &w;
    else          wg->head = &w;
    wg->tail = &w;
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &wg->mu);
    return NOVA_UNIT;
}

#endif /* NOVA_RT_SYNC_PRIMITIVES_H */
