// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_RUNQ_H
#define NOVA_RT_RUNQ_H

/* Plan 83-study-go-c-mn Ф.1 — fixed-size per-worker run queue (Go-1.4 port).
 *
 * Replaces the growable Chase-Lev deque (deque.h) and the growable
 * NovaSchedState parked[]/pending_wake[] arrays whose reallocation
 * (nova_sched_grow_state, plain non-atomic pointer-swap) is the root cause
 * of [M-83.11-grow-vs-wake-race]: a peer/driver thread loads the OLD array
 * base, CASes into the orphaned array, while the parked fiber's real state
 * now lives in the NEW array → lost wake → deterministic hang.
 *
 * Structural fix (Go P.runq): a FIXED-SIZE, NEVER-REALLOCATED inline ring of
 * `mco_coro*`. Base address is stable for the worker's whole life; slots are
 * never copied/moved/freed. Overflow is NEVER handled by reallocation — it
 * spills HALF the ring to a single global overflow queue (runqputslow). The
 * "reader holds a pointer into the old backing array while the writer
 * reallocates" bug class is therefore structurally impossible.
 *
 * ── Attribution ──────────────────────────────────────────────────────────
 * Algorithm adapted from the Go runtime (`src/pkg/runtime/proc.c`,
 * go1.4 — runqput / runqget / runqgrab / runqputslow), Copyright 2009 The Go
 * Authors, BSD-3-Clause (see THIRD_PARTY/go-LICENSE). Re-implemented for
 * Nova's mco_coro fibers + Boehm GC; not a verbatim copy. Memory-ordering
 * discipline (store-release tail / load-acquire head+tail / CAS head) is the
 * same as the original and as deque.h (PPoPP-2013); the bug was the
 * reallocation, not the fences.
 *
 * Concurrency contract:
 *   - tail: SINGLE producer (the owning worker). Plain slot store, then
 *     store-release(tail). No CAS.
 *   - head: MULTIPLE consumers (owner via nova_runq_get + thieves via
 *     nova_runq_grab). EVERY head advance is a CAS.
 *   - A value read from a slot before the head-CAS is stable until the slot
 *     is reused, and reuse needs CAP more puts (which need head to advance),
 *     so the pre-CAS read is race-free (Go's exact invariant).
 *
 * Diagnostics: counters are RELAXED atomics, post-hoc dump only (env
 * NOVA_DIAG_RUNQ) — NEVER a hot-path fprintf (debugging-races.md Lesson #1:
 * an fprintf mfence masks the very race we are closing).
 */

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* mco_coro is the fiber handle (minicoro). Forward-declared so this header is
 * standalone-testable (test_runq.c provides its own definition). */
struct mco_coro;
typedef struct mco_coro mco_coro;

/* ── Tuning ───────────────────────────────────────────────────────────── */

#ifndef NOVA_RUNQ_CAP
#define NOVA_RUNQ_CAP 256u            /* Go P.runq is [256]; power-of-two. */
#endif
#define NOVA_RUNQ_MASK (NOVA_RUNQ_CAP - 1u)

#if (NOVA_RUNQ_CAP & NOVA_RUNQ_MASK) != 0
#error "NOVA_RUNQ_CAP must be a power of two"
#endif

/* schedlink accessor: returns an lvalue-pointer to the intrusive overflow
 * link field on the fiber's owning descriptor (NovaSpawnCtxBase.schedlink in
 * the runtime; a test field in test_runq.c). The runtime defines this; the
 * unit test defines its own. Declared (not defined) here. */
mco_coro** nova_co_schedlink(mco_coro* co);

/* ── Per-worker fixed ring ────────────────────────────────────────────── */

typedef struct NovaRunq {
    /* Monotonic, masked only at slot access. length = (uint32)(tail - head);
     * empty = tail==head; full = (uint32)(tail-head) >= CAP. Wraparound-safe
     * for CAP <= 2^31. */
    volatile uint32_t head;   /* multi-consumer: CAS on every advance      */
    volatile uint32_t tail;   /* single-producer: store-release by owner    */
    mco_coro* slots[NOVA_RUNQ_CAP];   /* inline, never reallocated           */
} NovaRunq;

/* ── Global overflow queue (intrusive singly-linked via schedlink) ─────── */

typedef struct NovaGlobalRunq {
    mco_coro*           head;     /* oldest                                  */
    mco_coro*           tail;     /* newest                                  */
    volatile int32_t    size;     /* approximate count (under lock)          */
    volatile int32_t    lock;     /* spinlock (overflow is rare → cheap)     */
} NovaGlobalRunq;

/* ── Diagnostic counters (RELAXED; dump only when NOVA_DIAG_RUNQ set) ───── */

typedef struct NovaRunqDiag {
    volatile uint64_t overflow_spills;   /* runqputslow invocations          */
    volatile uint64_t grab_batches;      /* successful steal-half grabs       */
    volatile uint64_t grab_retries;      /* inconsistent-snapshot retries     */
    volatile uint64_t global_puts;       /* batches pushed to global queue    */
    volatile uint64_t global_pulls;      /* fibers pulled from global queue    */
} NovaRunqDiag;

extern NovaRunqDiag nova_runq_diag;

static inline void _nova_runq_diag_inc(volatile uint64_t* p) {
    __atomic_fetch_add(p, 1u, __ATOMIC_RELAXED);
}

/* ── Init ─────────────────────────────────────────────────────────────── */

static inline void nova_runq_init(NovaRunq* q) {
    q->head = 0;
    q->tail = 0;
    for (uint32_t i = 0; i < NOVA_RUNQ_CAP; i++) q->slots[i] = NULL;
}

static inline void nova_globrunq_init(NovaGlobalRunq* g) {
    g->head = NULL;
    g->tail = NULL;
    g->size = 0;
    g->lock = 0;
}

/* ── Global-queue spinlock (overflow path only) ───────────────────────── */

static inline void _nova_globrunq_lock(NovaGlobalRunq* g) {
    for (;;) {
        int32_t expected = 0;
        if (__atomic_compare_exchange_n(&g->lock, &expected, 1, false,
                                        __ATOMIC_ACQUIRE, __ATOMIC_RELAXED))
            return;
        /* spin; overflow is rare so contention window is tiny */
        while (__atomic_load_n(&g->lock, __ATOMIC_RELAXED) != 0) { /* pause */ }
    }
}
static inline void _nova_globrunq_unlock(NovaGlobalRunq* g) {
    __atomic_store_n(&g->lock, 0, __ATOMIC_RELEASE);
}

/* Push a pre-linked batch [batch_head .. batch_tail] (n fibers, linked via
 * schedlink, batch_tail->schedlink == NULL) onto the global overflow queue. */
static inline void nova_globrunq_put_batch(NovaGlobalRunq* g,
                                           mco_coro* batch_head,
                                           mco_coro* batch_tail,
                                           int32_t n) {
    if (!batch_head || n <= 0) return;
    *nova_co_schedlink(batch_tail) = NULL;
    _nova_globrunq_lock(g);
    if (g->tail) {
        *nova_co_schedlink(g->tail) = batch_head;
    } else {
        g->head = batch_head;
    }
    g->tail = batch_tail;
    g->size += n;
    _nova_globrunq_unlock(g);
    _nova_runq_diag_inc(&nova_runq_diag.global_puts);
}

/* Pop one fiber from the global overflow queue (NULL if empty). */
static inline mco_coro* nova_globrunq_get_one(NovaGlobalRunq* g) {
    if (__atomic_load_n(&g->size, __ATOMIC_RELAXED) == 0) return NULL;
    _nova_globrunq_lock(g);
    mco_coro* co = g->head;
    if (co) {
        g->head = *nova_co_schedlink(co);
        if (!g->head) g->tail = NULL;
        g->size -= 1;
        *nova_co_schedlink(co) = NULL;
    }
    _nova_globrunq_unlock(g);
    if (co) _nova_runq_diag_inc(&nova_runq_diag.global_pulls);
    return co;
}

/* nova_runq_put_slow — port of go1.4 src/runtime/proc.c::runqputslow.
 * Put gp and a batch of work from the local runnable queue on the global
 * queue. Executed only by the owner worker. Amortizes the global-lock cost
 * by spilling HALF the ring at once (Go's design — throughput parity with
 * the old growable deque, plan §8 risk 2). Returns false if the head moved
 * under us, in which case the caller retries the fast path. */
static inline bool nova_runq_put_slow(NovaRunq* q, NovaGlobalRunq* g,
                                      mco_coro* gp, uint32_t h, uint32_t t) {
    mco_coro* batch[NOVA_RUNQ_CAP / 2 + 1];
    uint32_t n, i;
    /* First, grab a batch from the local queue. */
    n = t - h;
    n = n / 2;
    if (n != NOVA_RUNQ_CAP / 2) {
        /* go1.4 throws "runqputslow: queue is not full" here (the snapshot
         * guarantees t-h==CAP). We instead return false defensively so a
         * stale snapshot can never abort the process; caller retries. */
        return false;
    }
    for (i = 0; i < n; i++)
        batch[i] = q->slots[(h + i) & NOVA_RUNQ_MASK];
    if (!__atomic_compare_exchange_n(&q->head, &h, h + n, false,
                                     __ATOMIC_RELEASE, __ATOMIC_RELAXED)) /* cas-release, commits consume */
        return false;
    batch[n] = gp;
    /* Link the fibers. */
    for (i = 0; i < n; i++)
        *nova_co_schedlink(batch[i]) = batch[i + 1];
    /* Now put the batch on the global queue. */
    nova_globrunq_put_batch(g, batch[0], batch[n], (int32_t)(n + 1));
    _nova_runq_diag_inc(&nova_runq_diag.overflow_spills);
    return true;
}

/* nova_runq_put — port of go1.4 src/runtime/proc.c::runqput.
 * Put gp on the local runnable queue. Executed only by the owner worker.
 * (Nova keeps its LIFO runnext slot separate, as go1.4 does — runnext was
 * added to runqput only in go1.5.) */
static inline void nova_runq_put(NovaRunq* q, NovaGlobalRunq* g, mco_coro* gp) {
retry:; {
        uint32_t h = __atomic_load_n(&q->head, __ATOMIC_ACQUIRE); /* load-acquire, synchronize with consumers */
        uint32_t t = __atomic_load_n(&q->tail, __ATOMIC_RELAXED); /* owner is sole writer (go1.4 reads plain) */
        if ((uint32_t)(t - h) < NOVA_RUNQ_CAP) {
            q->slots[t & NOVA_RUNQ_MASK] = gp;
            __atomic_store_n(&q->tail, t + 1, __ATOMIC_RELEASE);  /* store-release, makes the item available for consumption */
            return;
        }
        if (nova_runq_put_slow(q, g, gp, h, t)) return;
        /* the queue is not full, now the put above must succeed */
        goto retry;
    }
}

/* nova_runq_get — port of go1.4 src/runtime/proc.c::runqget.
 * Get a fiber from the local runnable queue. The owner pops from the head and
 * therefore competes with thieves (every head advance is a CAS). The slot
 * value read before the CAS is stable until the slot is reused, and reuse
 * needs CAP more puts — Go's invariant making the pre-CAS read race-free. */
static inline mco_coro* nova_runq_get(NovaRunq* q) {
    for (;;) {
        uint32_t h = __atomic_load_n(&q->head, __ATOMIC_ACQUIRE); /* load-acquire, synchronize with other consumers */
        uint32_t t = __atomic_load_n(&q->tail, __ATOMIC_RELAXED); /* owner is sole writer (go1.4 reads plain) */
        if (t == h) return NULL;
        mco_coro* gp = q->slots[h & NOVA_RUNQ_MASK];
        if (__atomic_compare_exchange_n(&q->head, &h, h + 1, false,
                                        __ATOMIC_ACQ_REL, __ATOMIC_RELAXED)) /* cas-release, commits consume */
            return gp;
    }
}

/* nova_runq_grab — port of go1.4 src/runtime/proc.c::runqgrab.
 * Grab a batch of fibers from the victim's local runnable queue. batch[] must
 * be of size >= NOVA_RUNQ_CAP/2; returns the number grabbed. May be executed
 * by any worker (the thief). Stolen items are returned in FIFO order
 * (batch[0] oldest). */
static inline uint32_t nova_runq_grab(NovaRunq* victim, mco_coro** batch,
                                      uint32_t batch_cap) {
    for (;;) {
        uint32_t h = __atomic_load_n(&victim->head, __ATOMIC_ACQUIRE); /* load-acquire, synchronize with other consumers */
        uint32_t t = __atomic_load_n(&victim->tail, __ATOMIC_ACQUIRE); /* load-acquire, synchronize with the producer */
        uint32_t n = t - h;
        n = n - n / 2;
        if (n == 0) return 0;
        if (n > NOVA_RUNQ_CAP / 2) {           /* read inconsistent h and t */
            _nova_runq_diag_inc(&nova_runq_diag.grab_retries);
            continue;
        }
        if (n > batch_cap) n = batch_cap;
        for (uint32_t i = 0; i < n; i++)
            batch[i] = victim->slots[(h + i) & NOVA_RUNQ_MASK];
        if (__atomic_compare_exchange_n(&victim->head, &h, h + n, false,
                                        __ATOMIC_ACQ_REL, __ATOMIC_RELAXED)) { /* cas-release, commits consume */
            _nova_runq_diag_inc(&nova_runq_diag.grab_batches);
            return n;
        }
    }
}

/* nova_runq_steal — port of go1.4 src/runtime/proc.c::runqsteal.
 * Steal half of the elements from the victim's local runnable queue and put
 * them onto the thief's (self's) local runnable queue. Returns one of the
 * stolen elements (or NULL if the steal failed / victim empty). The thief
 * owns `self`, so the batch-into-self puts use plain owner stores. */
static inline mco_coro* nova_runq_steal(NovaRunq* self, NovaRunq* victim) {
    mco_coro* batch[NOVA_RUNQ_CAP / 2];
    uint32_t n = nova_runq_grab(victim, batch, NOVA_RUNQ_CAP / 2);
    if (n == 0) return NULL;
    n--;
    mco_coro* gp = batch[n];          /* return the last stolen fiber */
    if (n == 0) return gp;            /* only one stolen → nothing to enqueue */
    uint32_t h = __atomic_load_n(&self->head, __ATOMIC_ACQUIRE); /* load-acquire, synchronize with consumers */
    uint32_t t = self->tail;
    /* self is empty/near-empty when stealing, so this never overflows; assert
     * the Go invariant (go1.4 throws "runqsteal: runq overflow"). */
    if ((uint32_t)(t - h + n) >= NOVA_RUNQ_CAP) return gp; /* defensive: drop-into-caller instead of abort */
    for (uint32_t i = 0; i < n; i++, t++)
        self->slots[t & NOVA_RUNQ_MASK] = batch[i];
    __atomic_store_n(&self->tail, t, __ATOMIC_RELEASE); /* store-release, makes the item available for consumption */
    return gp;
}

/* ── Introspection (owner-side, approximate for foreign readers) ───────── */

static inline uint32_t nova_runq_len(const NovaRunq* q) {
    uint32_t h = __atomic_load_n(&q->head, __ATOMIC_ACQUIRE);
    uint32_t t = __atomic_load_n(&q->tail, __ATOMIC_ACQUIRE);
    return (uint32_t)(t - h);
}

static inline bool nova_runq_empty(const NovaRunq* q) {
    return nova_runq_len(q) == 0;
}

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_RUNQ_H */
