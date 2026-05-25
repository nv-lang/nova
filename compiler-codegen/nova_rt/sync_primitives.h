// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_SYNC_PRIMITIVES_H
#define NOVA_RT_SYNC_PRIMITIVES_H

#include <stdio.h>
#include <stdlib.h>

/* Plan 18 std.sync: fiber-aware AtomicInt / AtomicBool / Mutex / WaitGroup / Once.
 *
 * Included from nova_rt.h AFTER nova_sched.h (needs nova_sched_park_with_unlock,
 * nova_sched_wake) and fibers.h (_nova_active_scope, _nova_active_slot TLS).
 *
 * Design:
 *  - AtomicInt / AtomicBool: thin wrappers around __atomic_* builtins. No park/wake.
 *  - Mutex: nova_mutex_t guards `locked` + waiter list. Fiber waiters park;
 *    non-fiber callers spin with CPU yield hint.
 *  - WaitGroup: nova_mutex_t guards `count` + waiter list. wait() parks when
 *    count > 0; done() wakes all waiters when count reaches 0.
 *  - Once: state machine NEW→RUNNING→DONE. First caller transitions NEW→RUNNING
 *    and returns true (should do the work). Concurrent callers park until DONE.
 *    All callers other than the first return false only after DONE is set.
 *
 * Waiter structs are stack-allocated in the parking fiber's call frame —
 * identical to ChannelWaiter pattern in channels.h. Safe because the fiber
 * stack is fixed (8 MB) and persists until the fiber resumes.
 *
 * Non-fiber path (_nova_active_slot < 0): spin with _nova_cpu_yield() hint.
 * This covers init/teardown and test scenarios that call sync primitives
 * outside a supervised scope.
 *
 * INVARIANTS (checked via NOVA_SYNC_ASSERT in debug builds):
 *  - Mutex.unlock() must be called only when the mutex is locked.
 *  - WaitGroup.done() must not decrement below zero.
 *  - Once.done() must be called exactly once, by the fiber whose run() returned true.
 *
 * NOT SUPPORTED (by design, same as Go/parking_lot):
 *  - Mutex is NOT reentrant. Calling lock() twice from the same fiber deadlocks.
 *  - WaitGroup.add() after wait() has started is undefined (same as Go).
 */

/* ── Debug assertions ──────────────────────────────────────────── */

#ifdef NOVA_DEBUG
#  define NOVA_SYNC_ASSERT(cond, msg)                                   \
     do {                                                                \
         if (!(cond)) {                                                  \
             fprintf(stderr, "[nova sync] FATAL: " msg "\n");           \
             abort();                                                    \
         }                                                               \
     } while (0)
#else
#  define NOVA_SYNC_ASSERT(cond, msg) ((void)0)
#endif

/* ── CPU yield hint ────────────────────────────────────────────── */

/* Used in OS-thread spin loops. Reduces bus traffic and gives the OS
 * scheduler a hint that this thread is busy-waiting.
 * x86: PAUSE reduces pipeline pressure (1 instruction vs tight CAS loop).
 * ARM: YIELD is the equivalent hint.
 * Windows: YieldProcessor() wraps the PAUSE/YIELD intrinsic. */
static inline void _nova_cpu_yield(void) {
#if defined(_WIN32)
    YieldProcessor();
#elif defined(__aarch64__) || defined(__arm64__)
    __asm__ volatile("yield" ::: "memory");
#elif defined(__x86_64__) || defined(__i386__)
    __asm__ volatile("pause" ::: "memory");
#endif
    /* On other POSIX platforms: fall through — the nova_mutex_lock/unlock
     * pair in the spin loop already implies OS scheduler interaction. */
}

/* ── AtomicInt ─────────────────────────────────────────────────── */

/* AtomicInt wraps nova_atomic_int (int32_t). All accesses go through
 * __atomic_* builtins in sync.h — the 'volatile' qualifier is NOT needed
 * (and would be misleading: volatile ≠ atomic in C11). */
typedef struct {
    nova_atomic_int value;
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

/* ── AtomicBool ────────────────────────────────────────────────── */

/* AtomicBool wraps nova_atomic_bool (bool). Useful for flags that are set
 * once (e.g., cancel sentinels) or toggled atomically. The swap() operation
 * is particularly useful for "take ownership" patterns: if swap(true) returns
 * false, the caller is the first to set the flag. */
typedef struct {
    nova_atomic_bool value;
} Nova_AtomicBool;

static inline Nova_AtomicBool* Nova_AtomicBool_static_new(nova_bool v) {
    Nova_AtomicBool* a = (Nova_AtomicBool*)nova_alloc(sizeof(Nova_AtomicBool));
    nova_abool_init(&a->value, (bool)v);
    return a;
}

static inline nova_bool Nova_AtomicBool_method_load(const Nova_AtomicBool* a) {
    return (nova_bool)nova_abool_load(&a->value);
}

static inline nova_unit Nova_AtomicBool_method_store(Nova_AtomicBool* a, nova_bool v) {
    nova_abool_store(&a->value, (bool)v);
    return NOVA_UNIT;
}

static inline nova_bool Nova_AtomicBool_method_compare_exchange(
        Nova_AtomicBool* a, nova_bool expected_val, nova_bool desired) {
    bool exp = (bool)expected_val;
    return (nova_bool)__atomic_compare_exchange_n(
        &a->value, &exp, (bool)desired,
        false,              /* strong */
        __ATOMIC_ACQ_REL,   /* success ordering */
        __ATOMIC_ACQUIRE);  /* failure ordering */
}

/* Atomically set to v, return previous value. Useful for "take ownership"
 * patterns: `if !flag.swap(true)` — only first caller wins. */
static inline nova_bool Nova_AtomicBool_method_swap(Nova_AtomicBool* a, nova_bool v) {
    return (nova_bool)__atomic_exchange_n(&a->value, (bool)v, __ATOMIC_ACQ_REL);
}

/* ── Mutex waiter ──────────────────────────────────────────────── */

typedef struct NovaMutexWaiter {
    NovaFiberQueue*         scope;
    int                     slot;
    struct NovaMutexWaiter* next;
    struct NovaMutexWaiter* prev;
} NovaMutexWaiter;

/* ── Mutex ─────────────────────────────────────────────────────── */

/* Fair FIFO Mutex. Fiber waiters are queued in arrival order. The lock is
 * transferred directly to the next waiter on unlock (no thundering herd).
 *
 * NOT reentrant: lock() from the same fiber that holds the lock deadlocks.
 * unlock() without a matching lock() is a debug-assert violation. */
typedef struct {
    nova_mutex_t      mu;       /* guards locked + waiter list */
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
        /* Non-fiber: spin with CPU yield to avoid burning the bus. */
        nova_mutex_unlock(&m->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&m->mu);
            if (!m->locked) {
                m->locked = true;
                nova_mutex_unlock(&m->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&m->mu);
        }
    }
    /* Fiber path: register as waiter and park atomically with unlock. */
    NovaMutexWaiter w;
    w.scope = _nova_active_scope;
    w.slot  = _nova_active_slot;
    w.next  = NULL;
    w.prev  = m->tail;
    if (m->tail) m->tail->next = &w;
    else         m->head = &w;
    m->tail = &w;
    /* park_with_unlock: parks fiber first, then releases mu. Prevents
     * lost-wakeup race (unlock cannot fire before park is registered). */
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &m->mu);
    /* Resumed: lock ownership transferred from unlock() — no re-check needed. */
    return NOVA_UNIT;
}

static inline nova_unit Nova_Mutex_method_unlock(Nova_Mutex* m) {
    nova_mutex_lock(&m->mu);
    NOVA_SYNC_ASSERT(m->locked, "Mutex.unlock() called on an unlocked mutex");
    if (m->head) {
        /* Transfer lock ownership to first waiter: locked stays true. */
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

/* Counter-based barrier. add(n) before spawning n workers; each worker
 * calls done() when finished; wait() parks until count reaches zero.
 *
 * Multiple callers may wait() concurrently — all are woken when done()
 * drives count to zero (WakeAll semantics).
 *
 * add() after wait() has started is undefined (same behavior as Go's
 * sync.WaitGroup — add must complete-happens-before any wait). */
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
    NOVA_SYNC_ASSERT(wg->count > 0,
                     "WaitGroup.done() called more times than add() — counter underflow");
    wg->count -= 1;
    if (wg->count == 0) {
        /* Detach the whole waiter list under lock, then wake outside.
         * Waking under the lock would cause the woken fiber to immediately
         * contend for the lock again — releasing first is more efficient. */
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
        /* Non-fiber: spin with CPU yield. */
        nova_mutex_unlock(&wg->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&wg->mu);
            if (wg->count <= 0) {
                nova_mutex_unlock(&wg->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&wg->mu);
        }
    }
    /* Fiber path: register as waiter and park atomically with unlock. */
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

/* ── Once waiter ───────────────────────────────────────────────── */

typedef struct NovaOnceWaiter {
    NovaFiberQueue*        scope;
    int                    slot;
    struct NovaOnceWaiter* next;  /* singly-linked: LIFO order, but once is
                                   * always just one burst of wakeups */
} NovaOnceWaiter;

/* ── Once ──────────────────────────────────────────────────────── */

#define NOVA_ONCE_NEW     0   /* not yet started */
#define NOVA_ONCE_RUNNING 1   /* one fiber is executing the once-body */
#define NOVA_ONCE_DONE    2   /* body complete, state is permanent */

/* Once guarantees that a body executes exactly once even under concurrency.
 *
 * Usage pattern:
 *
 *   if once.run() {
 *       // executed by exactly one fiber
 *       expensive_init()
 *       once.done()   // MUST call — releases waiting fibers
 *   }
 *   // all fibers reach here after init is complete
 *
 * run() returns true for the first caller (which must call done()).
 * Concurrent callers that arrive while state=RUNNING park until done() fires.
 * All callers that arrive after state=DONE return false immediately.
 *
 * CONTRACT: if run() returns true, the caller MUST call done() exactly once.
 * Failing to call done() leaves all waiting fibers permanently parked. */
typedef struct {
    nova_mutex_t     mu;
    int              state;    /* NOVA_ONCE_* constants */
    NovaOnceWaiter*  waiters;
} Nova_Once;

static inline Nova_Once* Nova_Once_static_new(void) {
    Nova_Once* o = (Nova_Once*)nova_alloc(sizeof(Nova_Once));
    nova_mutex_init(&o->mu);
    o->state   = NOVA_ONCE_NEW;
    o->waiters = NULL;
    return o;
}

/* run(): transitions state NEW→RUNNING for the first caller (returns true).
 * Subsequent callers park (fiber) or spin (OS thread) until DONE, then
 * return false. Callers arriving after DONE return false immediately. */
static inline nova_bool Nova_Once_method_run(Nova_Once* o) {
    /* Fast path: acquire-load without mutex. Safe because DONE is terminal
     * and the release-store in done() synchronizes with this acquire-load. */
    if (__atomic_load_n(&o->state, __ATOMIC_ACQUIRE) == NOVA_ONCE_DONE)
        return false;

    nova_mutex_lock(&o->mu);

    if (o->state == NOVA_ONCE_DONE) {
        nova_mutex_unlock(&o->mu);
        return false;
    }
    if (o->state == NOVA_ONCE_NEW) {
        o->state = NOVA_ONCE_RUNNING;
        nova_mutex_unlock(&o->mu);
        return true;   /* this fiber is the runner */
    }

    /* state == RUNNING: another fiber is executing the once-body. */
    if (_nova_active_slot < 0) {
        /* Non-fiber: spin with CPU yield until DONE. */
        nova_mutex_unlock(&o->mu);
        for (;;) {
            _nova_cpu_yield();
            if (__atomic_load_n(&o->state, __ATOMIC_ACQUIRE) == NOVA_ONCE_DONE)
                return false;
        }
    }

    /* Fiber: park until done() sets state=DONE and wakes us. */
    NovaOnceWaiter w;
    w.scope    = _nova_active_scope;
    w.slot     = _nova_active_slot;
    w.next     = o->waiters;
    o->waiters = &w;
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &o->mu);
    /* Woken by done() — state is NOVA_ONCE_DONE. */
    return false;
}

/* done(): marks the once-body as complete. Wakes all parked waiters.
 * Must be called exactly once, by the fiber that received true from run(). */
static inline nova_unit Nova_Once_method_done(Nova_Once* o) {
    nova_mutex_lock(&o->mu);
    NOVA_SYNC_ASSERT(o->state == NOVA_ONCE_RUNNING,
                     "Once.done() called without a matching run() returning true");
    /* Release-store: makes the body's side-effects visible to all callers
     * that observe DONE via the acquire-load fast path in run(). */
    __atomic_store_n(&o->state, NOVA_ONCE_DONE, __ATOMIC_RELEASE);
    NovaOnceWaiter* w = o->waiters;
    o->waiters = NULL;
    nova_mutex_unlock(&o->mu);
    while (w) {
        NovaOnceWaiter* next = w->next;
        nova_sched_wake(w->scope, w->slot);
        w = next;
    }
    return NOVA_UNIT;
}

/* ── MemOrdering (Plan 103.1) ──────────────────────────────────────────
 *
 * Pre-declared here so nova_fn_fence (below) can reference Nova_MemOrdering*
 * before the generated code defines it. The codegen skips re-emitting
 * MemOrdering struct/constructors (RUNTIME_DEFINED_TYPES in emit_c.rs)
 * because this pre-declaration IS the canonical definition.
 *
 * Tag values must match the variant ORDER declared in std/runtime/sync.nv:
 *   Relaxed=0  Acquire=1  Release=2  AcqRel=3  SeqCst=4
 * This is coordinated with the codegen helper nova_mem_ordering_to_atomic
 * (emit_c.rs Plan 103.1 Ф.3) and is documented in D167.
 */
typedef enum {
    NOVA_TAG_MemOrdering_Relaxed = 0,
    NOVA_TAG_MemOrdering_Acquire = 1,
    NOVA_TAG_MemOrdering_Release = 2,
    NOVA_TAG_MemOrdering_AcqRel  = 3,
    NOVA_TAG_MemOrdering_SeqCst  = 4,
} Nova_MemOrdering_Tag;

typedef struct Nova_MemOrdering Nova_MemOrdering;
struct Nova_MemOrdering {
    Nova_MemOrdering_Tag tag;
    union { char _dummy; } payload;   /* unit-only variants — MSVC requires ≥1 member */
};

/* Constructors — normally emitted by emit_sum_type; here because MemOrdering
 * is in RUNTIME_DEFINED_TYPES (emit_sum_type is skipped). */
static inline Nova_MemOrdering* nova_make_MemOrdering_Relaxed(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_Relaxed;
    return _r;
}
static inline Nova_MemOrdering* nova_make_MemOrdering_Acquire(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_Acquire;
    return _r;
}
static inline Nova_MemOrdering* nova_make_MemOrdering_Release(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_Release;
    return _r;
}
static inline Nova_MemOrdering* nova_make_MemOrdering_AcqRel(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_AcqRel;
    return _r;
}
static inline Nova_MemOrdering* nova_make_MemOrdering_SeqCst(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_SeqCst;
    return _r;
}

/* nova_fn_fence — implements `export external fn fence(ord MemOrdering)`.
 *
 * C name derived by ExternalRegistry: free function → nova_fn_fence.
 * Parameter type MemOrdering maps to Nova_MemOrdering* (heap-pointer ABI).
 *
 * Semantics (D167):
 *  Relaxed — no-op (fence is valid syntactically; has no ordering effect)
 *  Acquire — all subsequent reads/writes happen-after all prior Release stores
 *  Release — all prior reads/writes happen-before all subsequent Acquire loads
 *  AcqRel  — combination Acquire + Release
 *  SeqCst  — total order participation; sequenced relative to all SeqCst ops
 */
static inline nova_unit nova_fn_fence(Nova_MemOrdering* ord) {
    switch (ord->tag) {
        case NOVA_TAG_MemOrdering_Relaxed: /* no-op: valid syntactically */ break;
        case NOVA_TAG_MemOrdering_Acquire: nova_thread_fence_acquire(); break;
        case NOVA_TAG_MemOrdering_Release: nova_thread_fence_release(); break;
        case NOVA_TAG_MemOrdering_AcqRel:  nova_thread_fence_acq_rel(); break;
        case NOVA_TAG_MemOrdering_SeqCst:  nova_thread_fence_seq_cst(); break;
    }
    return NOVA_UNIT;
}

#endif /* NOVA_RT_SYNC_PRIMITIVES_H */
