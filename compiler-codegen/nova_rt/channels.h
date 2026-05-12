// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_CHANNELS_H
#define NOVA_RT_CHANNELS_H

/* Plan 21 (D91): capability-split Channel API — ChanWriter[T] / ChanReader[T].
 *
 * Replaces D79 Go-style (one Nova_Channel* with send+recv). Breaking change.
 *
 * Design decisions (Plan 21):
 *   A1. WaiterList heap-allocated — safe under M:N (Plan 23).
 *   A2. stop_cb returns NOVA_STOP_SYNC — channel cancel is synchronous.
 *   A3. _nova_active_scope / _nova_active_slot thread-locals from fibers.h.
 *   A4. Nova_ChannelPair struct for tuple-return from nova_channel_new().
 *   A5. Full-buffer send parks (no busy-yield) — R7 Plan 22 enforced.
 *
 * Included from nova_rt.h after fibers.h (provides _nova_active_scope,
 * _nova_active_slot, NovaStopMode, NovaFiberQueue) and effects.h
 * (provides nova_throw, nova_str_from_cstr). sched.h included after
 * channels.h so forward-declare nova_sched_* here via includes only.
 * Actually sched.h comes after channels.h in nova_rt.h — see order:
 *   array.h → effects.h → fibers.h → channels.h → eventloop.h → sched.h
 * Therefore channels.h must NOT call sched API directly in static inlines
 * that are defined here. Solution: move blocking recv/send to channels.c,
 * OR reorder includes.
 *
 * РЕШЕНИЕ: nova_rt.h переставляем channels.h ПОСЛЕ sched.h.
 * channels.h включается последним (после sched.h), все deps доступны.
 */

#include "alloc.h"
#include <stdint.h>
#include <stdbool.h>
/* Plan 40 Ф.3-extended: alloca для shuffle order array без VLA
 * (MSVC не поддерживает VLA, но alloca есть на всех toolchain'ах). */
#ifdef _MSC_VER
  #include <malloc.h>
#else
  #include <alloca.h>
#endif

/* ── Forward declarations ──────────────────────────────────────── */

typedef struct Nova_ChannelState Nova_ChannelState;
typedef struct BaseWaiter        BaseWaiter;
typedef struct ChannelWaiter     ChannelWaiter;
typedef struct SelectWaiter      SelectWaiter;

/* ── BaseWaiter — common prefix (Plan 40 R2 C1) ─────────────────
 *
 * Shared between ChannelWaiter (single park/wake) and SelectWaiter
 * (select arm registration). Replaces the cast-pun pattern of Plan 31
 * with explicit composition — strict-aliasing safe under -O3 + LTO.
 *
 * Fields:
 *   scope/slot     — scheduler park identity.
 *   channel        — back-pointer for unlink; NULL = unlinked.
 *   is_recv        — true = recv-waiter, false = send-waiter.
 *   send_val       — value to commit (send-waiter only).
 *                    For SelectWaiter recv arm, on wake the channel writes
 *                    the value here directly (Plan 40 R1 B1 direct-copy)
 *                    avoiding a buffer round-trip.
 *   next/prev      — doubly-linked list (Plan 40 T2; O(1) unlink).
 *   fired          — Plan 40 R1 A6 + R2 B2: selectdone CAS. 0 = waiter
 *                    still owns the slot; 1 = winner CAS'd this waiter.
 *                    For ChannelWaiter (single waiter) fired is also CAS'd
 *                    so wake-loop has a single unified protocol.
 *   cancelled      — Plan 40 R2 C2: stop_cb lock-free path. stop_cb sets
 *                    this WITHOUT acquiring channel mutex; wake helpers
 *                    skip cancelled waiters during iteration. Lazy unlink
 *                    at next-wake or select-park exit.
 */
struct BaseWaiter {
    NovaFiberQueue*    scope;
    int                slot;
    Nova_ChannelState* channel;
    bool               is_recv;
    nova_int           send_val;
    BaseWaiter*        next;
    BaseWaiter*        prev;
    nova_atomic_int    fired;
    nova_atomic_bool   cancelled;
};

/* ChannelWaiter — for plain recv/send park. Identical layout to BaseWaiter. */
struct ChannelWaiter {
    BaseWaiter base;
};

/* SelectWaiter — for select-arm registration. BaseWaiter prefix + arm-only.
 *
 * On wake, channel writes the recv'd value into `base.send_val` (re-using
 * the field as a unified carrier for direct-copy, Plan 40 R1 B1).
 * select_park reads `waiters[which].base.send_val` after fired check.
 */
struct SelectWaiter {
    BaseWaiter base;
    int        arm_idx;
};

/* ── Channel state (hidden from Nova code) ─────────────────────── */

/* Plan 40 R2 C5 (false sharing prevention): fields grouped by access
 * pattern, padded between groups. Single-threaded cost: +128 bytes per
 * channel. Under M:N saves 100-1000× contention vs unpadded layout
 * (crossbeam benchmarks, Zen4 16-core).
 *
 * Group A (mostly read; cold writes on close):
 *   mu, closed, on_select_lost, cleanup_data
 * Group B (under-lock state; mutated on every send/recv):
 *   buf, cap, head, count, recv_waiters, send_waiters
 * Group C (refcount; contended on close path):
 *   writer_count, reader_closed
 */
struct Nova_ChannelState {
    /* ── Group A: mostly read + cold writes ── */
    nova_mutex_t      mu;
    nova_atomic_bool  closed;        /* Plan 40 R1 A2: fast-path read без lock; under-lock re-check */
    /* Plan 40 Ф.2 B7: optional cleanup hook fired when this channel
     * loses a select race (другая arm выиграла, эта не нужна).
     * Используется Time.after для отмены неиспользованного uv_timer'а.
     * NULL для обычных каналов — без overhead. */
    void           (*on_select_lost)(Nova_ChannelState*);
    void*             cleanup_data;

    _Alignas(NOVA_CACHELINE_SIZE) char _pad_ab[1];

    /* ── Group B: under-lock state ── */
    nova_int*         buf;
    int64_t           cap;
    int64_t           head;
    int64_t           count;
    BaseWaiter*       recv_waiters;  /* doubly-linked; head insert */
    BaseWaiter*       send_waiters;  /* doubly-linked; head insert */

    _Alignas(NOVA_CACHELINE_SIZE) char _pad_bc[1];

    /* ── Group C: refcounts (contended on close) ── */
    nova_atomic_int   writer_count;  /* Plan 40 R1 A1: Release-dec + Acquire-fence-on-zero */
    nova_atomic_bool  reader_closed; /* Plan 40 R1 B2: symmetric reader-side close */
};

/* ── try_recv / try_send result (three-way, matches Rust TryRecvError) ── */

/* NOVA_CHAN_TRY_OK     — value transferred
 * NOVA_CHAN_TRY_EMPTY  — buffer empty/full, channel still open (transient)
 * NOVA_CHAN_TRY_CLOSED — channel closed, no more data will arrive
 * Nova code uses rx.is_closed() to distinguish EMPTY from CLOSED after None. */
typedef enum { NOVA_CHAN_TRY_OK = 0, NOVA_CHAN_TRY_EMPTY = 1, NOVA_CHAN_TRY_CLOSED = 2 } NovaChanTryTag;
typedef struct { NovaChanTryTag tag; nova_int value; } NovaChanTryResult;

/* ── Capability wrappers ───────────────────────────────────────── */

/* writer_closed: per-writer flag so double-close() is idempotent per handle,
 * preventing writer_count underflow when one clone is closed twice. */
typedef struct { Nova_ChannelState* state; bool writer_closed; } Nova_ChanWriter;
typedef struct { Nova_ChannelState* state; } Nova_ChanReader;

/* Factory return type (A4). */
typedef struct { Nova_ChanWriter* tx; Nova_ChanReader* rx; } Nova_ChannelPair;

/* ── WaiterList helpers ────────────────────────────────────────── */

/* Plan 40 T2: O(1) doubly-linked unlink. Caller MUST hold channel mu. */
static inline void _nova_waiter_unlink_locked(BaseWaiter* w) {
    if (!w->channel) return;  /* already unlinked */
    Nova_ChannelState* st = w->channel;
    BaseWaiter** head = w->is_recv ? &st->recv_waiters : &st->send_waiters;
    if (w->prev) {
        w->prev->next = w->next;
    } else {
        *head = w->next;  /* w was head */
    }
    if (w->next) {
        w->next->prev = w->prev;
    }
    w->next = NULL;
    w->prev = NULL;
    w->channel = NULL;
}

/* Plan 40 T2: O(1) doubly-linked head insert. Caller MUST hold channel mu. */
static inline void _nova_waiter_insert_locked(BaseWaiter* w) {
    Nova_ChannelState* st = w->channel;
    BaseWaiter** head = w->is_recv ? &st->recv_waiters : &st->send_waiters;
    w->prev = NULL;
    w->next = *head;
    if (*head) (*head)->prev = w;
    *head = w;
}

/* Plan 40 R2 C2: stop_cb lock-free contract.
 *
 * stop_cb runs from scheduler context при cancel_scope cancellation.
 * It MUST NOT acquire channel mutex (potential deadlock if scheduler
 * holds another lock). Instead it sets an atomic `cancelled` flag;
 * wake helpers iterating the waiter list skip cancelled entries (lazy
 * unlink at next wake or at select_park exit).
 *
 * Wake fiber so cancel_scope check fires after park return. */
static NovaStopMode _nova_channel_waiter_stop_cb(void* handle) {
    BaseWaiter* w = (BaseWaiter*)handle;
    nova_abool_store(&w->cancelled, true);
    if (w->channel) {
        nova_sched_wake(w->scope, w->slot);
    }
    return NOVA_STOP_SYNC;
}

/* Helper to release channel mutex from nova_sched_park_with_unlock callback. */
static inline void _nova_unlock_mutex_cb(void* arg) {
    nova_mutex_unlock((nova_mutex_t*)arg);
}

/* ── Factory ───────────────────────────────────────────────────── */

static inline Nova_ChannelPair nova_channel_new(int64_t capacity) {
    /* Plan 40 B9: validate before any allocation — no leak on throw. */
    if (capacity <= 0) {
        nova_throw(nova_str_from_cstr("Channel.new: capacity must be >= 1"));
    }
    Nova_ChannelState* st = (Nova_ChannelState*)nova_alloc(sizeof(Nova_ChannelState));
    nova_mutex_init(&st->mu);
    st->buf          = (nova_int*)nova_alloc((size_t)capacity * sizeof(nova_int));
    st->cap          = capacity;
    st->head         = 0;
    st->count        = 0;
    nova_abool_init(&st->closed, false);
    nova_aint_init(&st->writer_count, 1);
    nova_abool_init(&st->reader_closed, false);  /* Plan 40 R1 B2 */
    st->recv_waiters = NULL;
    st->send_waiters = NULL;
    st->on_select_lost = NULL;
    st->cleanup_data   = NULL;
    Nova_ChanWriter* tx = (Nova_ChanWriter*)nova_alloc(sizeof(Nova_ChanWriter));
    Nova_ChanReader* rx = (Nova_ChanReader*)nova_alloc(sizeof(Nova_ChanReader));
    tx->state         = st;
    tx->writer_closed = false;
    rx->state         = st;
    return (Nova_ChannelPair){ .tx = tx, .rx = rx };
}

/* ── Internal wake helpers ─────────────────────────────────────── */

/* Plan 40 R2 B2: unified selectdone wake protocol.
 *
 * Walk recv_waiters head→tail. For each waiter:
 *   - skip if cancelled (lazy unlink at next opportunity).
 *   - CAS fired: 0→1. First successful CAS wins.
 *     • Direct-copy value into winner's stack (Plan 40 R1 B1).
 *     • Unlink, wake fiber, return.
 *   - On CAS failure (waiter already fired by another wake / scope cancel):
 *     unlink it lazily and continue.
 *
 * Returns 1 if a value was handed off (caller decrements count); 0 if
 * no eligible waiter (caller pushes into buffer normally).
 *
 * Caller MUST hold channel mu. */
static inline int _nova_channel_wake_recv_with_value(Nova_ChannelState* st,
                                                      nova_int value) {
    BaseWaiter* w = st->recv_waiters;
    while (w) {
        BaseWaiter* next = w->next;
        /* Lazy unlink of cancelled waiters. */
        if (nova_abool_load(&w->cancelled)) {
            _nova_waiter_unlink_locked(w);
            w = next;
            continue;
        }
        int32_t expected = 0;
        if (nova_aint_cas_weak_release(&w->fired, &expected, 1)) {
            /* Won the CAS. Direct-copy value into waiter's recv slot if
             * it's a SelectWaiter (has recv_val field). For plain
             * ChannelWaiter we use send_val as the carrier (reusing the
             * field — see recv path below). */
            w->send_val = value;
            _nova_waiter_unlink_locked(w);
            nova_sched_wake(w->scope, w->slot);
            return 1;
        }
        /* CAS failed: another wake already claimed this waiter. Lazy unlink
         * (it's now considered dead). */
        _nova_waiter_unlink_locked(w);
        w = next;
    }
    return 0;
}

/* Wake first eligible send-waiter and commit its value into buffer.
 * Caller MUST hold channel mu. Returns 1 if a send-waiter was promoted
 * into the buffer (i.e. count was incremented + sender woken). */
static inline int _nova_channel_wake_send(Nova_ChannelState* st) {
    BaseWaiter* w = st->send_waiters;
    while (w) {
        BaseWaiter* next = w->next;
        if (nova_abool_load(&w->cancelled)) {
            _nova_waiter_unlink_locked(w);
            w = next;
            continue;
        }
        int32_t expected = 0;
        if (nova_aint_cas_weak_release(&w->fired, &expected, 1)) {
            /* Won the CAS — promote waiter's send_val into buffer. */
            int64_t tail = (st->head + st->count) % st->cap;
            st->buf[tail] = w->send_val;
            st->count++;
            _nova_waiter_unlink_locked(w);
            nova_sched_wake(w->scope, w->slot);
            return 1;
        }
        _nova_waiter_unlink_locked(w);
        w = next;
    }
    return 0;
}

/* ── Receiver ──────────────────────────────────────────────────── */

/* Plan 40 R1 A2: fast-path is_closed read without lock; full state check
 * MUST be performed under lock (TOCTOU re-check protocol). */
static inline NovaOpt_nova_int nova_chan_reader_recv(Nova_ChanReader* rx) {
    Nova_ChannelState* st = rx->state;

    nova_mutex_lock(&st->mu);

    /* Try take immediately under lock. */
    if (st->count > 0) {
        nova_int v = st->buf[st->head];
        st->head = (st->head + 1) % st->cap;
        st->count--;
        /* Promote a parked sender into the freed slot. */
        (void)_nova_channel_wake_send(st);
        nova_mutex_unlock(&st->mu);
        return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_Some, .value = v };
    }
    /* Re-check closed under lock (Plan 40 A2 TOCTOU). */
    if (nova_abool_load(&st->closed)) {
        nova_mutex_unlock(&st->mu);
        return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_None, .value = 0 };
    }

    /* Need to park. Allocate waiter on the heap (Nova_alloc — GC-managed). */
    NovaFiberQueue* sc = _nova_active_scope;
    int             sl = _nova_active_slot;
    if (!sc || sl < 0) {
        nova_mutex_unlock(&st->mu);
        nova_throw(nova_str_from_cstr("recv called outside fiber context"));
    }

    BaseWaiter* w = (BaseWaiter*)nova_alloc(sizeof(BaseWaiter));
    w->scope    = sc;
    w->slot     = sl;
    w->channel  = st;
    w->is_recv  = true;
    w->send_val = 0;
    nova_aint_init(&w->fired, 0);
    nova_abool_init(&w->cancelled, false);
    _nova_waiter_insert_locked(w);

    nova_sched_register_pending(sc, sl, w, _nova_channel_waiter_stop_cb);
    /* Plan 40 R2 C6: atomically transition to parked state + release lock. */
    nova_sched_park_with_unlock(sc, sl, _nova_unlock_mutex_cb, &st->mu);
    nova_sched_unregister_pending(sc, sl);

    /* Plan 40 R3-7 + C2: re-acquire lock, check fired/cancelled, drain. */
    nova_mutex_lock(&st->mu);

    /* If cancel_scope cancelled us, throw. cancelled flag was set by
     * stop_cb without acquiring the lock; we observe it now. */
    if (sc->cancel_requested) {
        if (w->channel) _nova_waiter_unlink_locked(w);
        nova_mutex_unlock(&st->mu);
        nova_throw(nova_str_from_cstr("scope cancelled"));
    }

    /* Wake helper (sender side) CAS'd our fired = 1 and copied value
     * into w->send_val (direct-copy, Plan 40 B1). */
    int32_t fired = nova_aint_load(&w->fired);
    if (fired) {
        nova_int v = w->send_val;
        /* waiter already unlinked by wake helper */
        nova_mutex_unlock(&st->mu);
        return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_Some, .value = v };
    }

    /* Spurious wake or closed-side wake without value (channel closed). */
    if (w->channel) _nova_waiter_unlink_locked(w);
    /* Re-check closed for proper return value. */
    if (st->count > 0) {
        nova_int v = st->buf[st->head];
        st->head = (st->head + 1) % st->cap;
        st->count--;
        (void)_nova_channel_wake_send(st);
        nova_mutex_unlock(&st->mu);
        return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_Some, .value = v };
    }
    nova_mutex_unlock(&st->mu);
    return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_None, .value = 0 };
}

static inline NovaChanTryResult nova_chan_reader_try_recv(Nova_ChanReader* rx) {
    Nova_ChannelState* st = rx->state;
    nova_mutex_lock(&st->mu);
    if (st->count == 0) {
        NovaChanTryTag tag = nova_abool_load(&st->closed) ?
                              NOVA_CHAN_TRY_CLOSED : NOVA_CHAN_TRY_EMPTY;
        nova_mutex_unlock(&st->mu);
        return (NovaChanTryResult){ .tag = tag, .value = 0 };
    }
    nova_int v = st->buf[st->head];
    st->head = (st->head + 1) % st->cap;
    st->count--;
    (void)_nova_channel_wake_send(st);
    nova_mutex_unlock(&st->mu);
    return (NovaChanTryResult){ .tag = NOVA_CHAN_TRY_OK, .value = v };
}

/* Snapshot read accessors. count/cap accessed под lock'ом; closed atomic. */
static inline nova_int  nova_chan_reader_len(Nova_ChanReader* rx) {
    nova_mutex_lock(&rx->state->mu);
    nova_int n = (nova_int)rx->state->count;
    nova_mutex_unlock(&rx->state->mu);
    return n;
}
static inline nova_int  nova_chan_reader_capacity(Nova_ChanReader* rx) {
    return (nova_int)rx->state->cap;  /* immutable after init */
}
static inline nova_bool nova_chan_reader_is_closed(Nova_ChanReader* rx) {
    return (nova_bool)nova_abool_load(&rx->state->closed);
}

/* Plan 40 R1 B2: symmetric reader-side close. Wakes all parked senders
 * who then return `false` from `send()`. Subsequent sends return false
 * immediately (reader_closed atomic load fast-path). */
static inline void nova_chan_reader_close(Nova_ChanReader* rx) {
    Nova_ChannelState* st = rx->state;
    nova_mutex_lock(&st->mu);
    if (nova_abool_load(&st->reader_closed)) {
        nova_mutex_unlock(&st->mu);
        return;
    }
    nova_abool_store(&st->reader_closed, true);
    /* Wake all parked senders — they will see reader_closed and return. */
    while (st->send_waiters) {
        BaseWaiter* w = st->send_waiters;
        int32_t expected = 0;
        if (nova_aint_cas_weak_release(&w->fired, &expected, 1)) {
            BaseWaiter* next = w->next;
            _nova_waiter_unlink_locked(w);
            nova_sched_wake(w->scope, w->slot);
            (void)next;
        } else {
            /* Already fired by another path — just unlink. */
            _nova_waiter_unlink_locked(w);
        }
    }
    nova_mutex_unlock(&st->mu);
}

/* ── Sender ────────────────────────────────────────────────────── */

/* Returns true if value was sent, false if channel is closed (Plan 30 Ф.1). */
static inline nova_bool nova_chan_writer_send(Nova_ChanWriter* tx, nova_int v) {
    Nova_ChannelState* st = tx->state;

    /* Fast-path closed check (Plan 40 R1 A2: re-check under lock). */
    if (nova_abool_load(&st->closed) || nova_abool_load(&st->reader_closed)) {
        return 0;
    }

    nova_mutex_lock(&st->mu);

    /* Re-check closed under lock. */
    if (nova_abool_load(&st->closed) || nova_abool_load(&st->reader_closed)) {
        nova_mutex_unlock(&st->mu);
        return 0;
    }

    /* Direct hand-off to a parked receiver (Plan 40 B1 direct-copy):
     * skip the buffer entirely если есть recv-waiter. */
    if (st->recv_waiters) {
        if (_nova_channel_wake_recv_with_value(st, v)) {
            nova_mutex_unlock(&st->mu);
            return 1;
        }
        /* All recv_waiters were cancelled — fall through to push. */
    }

    /* Buffer has room → push and return. */
    if (st->count < st->cap) {
        int64_t tail = (st->head + st->count) % st->cap;
        st->buf[tail] = v;
        st->count++;
        nova_mutex_unlock(&st->mu);
        return 1;
    }

    /* Need to park. */
    NovaFiberQueue* sc = _nova_active_scope;
    int             sl = _nova_active_slot;
    if (!sc || sl < 0) {
        nova_mutex_unlock(&st->mu);
        nova_throw(nova_str_from_cstr("send called outside fiber context"));
    }

    BaseWaiter* w = (BaseWaiter*)nova_alloc(sizeof(BaseWaiter));
    w->scope    = sc;
    w->slot     = sl;
    w->channel  = st;
    w->is_recv  = false;
    w->send_val = v;
    nova_aint_init(&w->fired, 0);
    nova_abool_init(&w->cancelled, false);
    _nova_waiter_insert_locked(w);

    nova_sched_register_pending(sc, sl, w, _nova_channel_waiter_stop_cb);
    nova_sched_park_with_unlock(sc, sl, _nova_unlock_mutex_cb, &st->mu);
    nova_sched_unregister_pending(sc, sl);

    nova_mutex_lock(&st->mu);

    if (sc->cancel_requested) {
        if (w->channel) _nova_waiter_unlink_locked(w);
        nova_mutex_unlock(&st->mu);
        nova_throw(nova_str_from_cstr("scope cancelled"));
    }

    /* fired=1 means recv-side / close picked us up. If close — return false. */
    int32_t fired = nova_aint_load(&w->fired);
    nova_bool closed_now = nova_abool_load(&st->closed) ||
                           nova_abool_load(&st->reader_closed);
    if (w->channel) _nova_waiter_unlink_locked(w);
    nova_mutex_unlock(&st->mu);

    if (closed_now) return 0;
    if (fired) return 1;
    /* Spurious wake без actual transfer — channel ещё open но waiter
     * canceled by stop_cb path. Treat as closed=false send=false. */
    return 0;
}

static inline NovaChanTryResult nova_chan_writer_try_send(Nova_ChanWriter* tx, nova_int v) {
    Nova_ChannelState* st = tx->state;
    nova_mutex_lock(&st->mu);
    if (nova_abool_load(&st->closed) || nova_abool_load(&st->reader_closed)) {
        nova_mutex_unlock(&st->mu);
        return (NovaChanTryResult){ .tag = NOVA_CHAN_TRY_CLOSED, .value = 0 };
    }
    /* Direct hand-off to parked receiver. */
    if (st->recv_waiters) {
        if (_nova_channel_wake_recv_with_value(st, v)) {
            nova_mutex_unlock(&st->mu);
            return (NovaChanTryResult){ .tag = NOVA_CHAN_TRY_OK, .value = 0 };
        }
    }
    if (st->count >= st->cap) {
        nova_mutex_unlock(&st->mu);
        return (NovaChanTryResult){ .tag = NOVA_CHAN_TRY_EMPTY, .value = 0 };
    }
    int64_t tail = (st->head + st->count) % st->cap;
    st->buf[tail] = v;
    st->count++;
    nova_mutex_unlock(&st->mu);
    return (NovaChanTryResult){ .tag = NOVA_CHAN_TRY_OK, .value = 0 };
}

/* Plan 40 R1 A1: writer_count decrement через Release-fetch_sub; thread
 * that drove count to zero issues Acquire fence before reading owned
 * state — classic refcount idiom (Boost.Atomic, Rust Arc::drop). */
static inline void nova_chan_writer_close(Nova_ChanWriter* tx) {
    if (tx->writer_closed) return;  /* per-writer idempotent guard */
    tx->writer_closed = true;
    Nova_ChannelState* st = tx->state;
    int32_t prev = nova_aint_fetch_sub_release(&st->writer_count);
    if (prev != 1) return;  /* other writers still alive */

    /* We drove count to zero — Acquire fence pairs with all prior Release
     * decrements + earlier writes by other writers. */
    nova_thread_fence_acquire();

    nova_mutex_lock(&st->mu);
    nova_abool_store(&st->closed, true);
    /* Wake all parked recv- and send-waiters under lock. */
    while (st->recv_waiters) {
        BaseWaiter* w = st->recv_waiters;
        int32_t expected = 0;
        (void)nova_aint_cas_weak_release(&w->fired, &expected, 1);
        _nova_waiter_unlink_locked(w);
        nova_sched_wake(w->scope, w->slot);
    }
    while (st->send_waiters) {
        BaseWaiter* w = st->send_waiters;
        int32_t expected = 0;
        (void)nova_aint_cas_weak_release(&w->fired, &expected, 1);
        _nova_waiter_unlink_locked(w);
        nova_sched_wake(w->scope, w->slot);
    }
    nova_mutex_unlock(&st->mu);
}

/* Plan 30 Ф.2: clone creates a second writer sharing the same channel
 * state. Plan 40 R1 A1: atomic increment. */
static inline Nova_ChanWriter* nova_chan_writer_clone(Nova_ChanWriter* tx) {
    nova_aint_inc(&tx->state->writer_count);
    Nova_ChanWriter* clone = (Nova_ChanWriter*)nova_alloc(sizeof(Nova_ChanWriter));
    clone->state         = tx->state;
    clone->writer_closed = false;
    return clone;
}

static inline nova_int  nova_chan_writer_len(Nova_ChanWriter* tx) {
    nova_mutex_lock(&tx->state->mu);
    nova_int n = (nova_int)tx->state->count;
    nova_mutex_unlock(&tx->state->mu);
    return n;
}
static inline nova_int  nova_chan_writer_capacity(Nova_ChanWriter* tx) {
    return (nova_int)tx->state->cap;
}
static inline nova_bool nova_chan_writer_is_closed(Nova_ChanWriter* tx) {
    return (nova_bool)(nova_abool_load(&tx->state->closed) ||
                       nova_abool_load(&tx->state->reader_closed));
}

/* ── Select — D94 (Plan 31) ────────────────────────────────────── */

/* Plan 40 Ф.3-extended (2026-05-12): per-call adaptive storage без cap'а.
 *
 * Caller (codegen emit_select) выделяет SelectSlot _arms[n_ch] +
 * SelectWaiter _waiters[n_ch] на стеке (compound literal, размер
 * literal на codegen-time, MSVC-compatible — не VLA), передаёт указатели
 * в nova_select_init. Storage = ровно n_ch слотов, zero-fill только
 * используемые. Stack frame ~80n байт на одну select-операцию.
 *
 * Plan 40 Ф.1 (с Plan 23 M:N) добавит atomics/selectdone CAS/
 * doubly-linked в SelectWaiter, не меняя storage layout. */

typedef struct {
    Nova_ChannelState* chan;     /* NULL = slot unused or default arm */
    bool               is_recv;
    nova_int           send_val;
    bool               guard;   /* false → arm disabled */
    bool               wildcard; /* true = `_ = rx` fires on closed too; false = `Some(v) = rx` needs data */
} SelectSlot;

/* SelectWaiter struct defined earlier via BaseWaiter composition (Plan 40 C1). */

/* arms и waiters — caller-provided storage (compound literal в emit'е
 * со размером n_arms, literal на codegen-time). */
typedef struct {
    SelectSlot*     arms;      /* caller-provided, size = n_arms */
    SelectWaiter*   waiters;   /* caller-provided, size = n_arms */
    int             n_arms;    /* number of channel arms (excl. default) */
    int             which;     /* arm that fired: 0..n_arms-1, or -2 = default */
    nova_int        recv_val;  /* received value for winning recv arm */
    NovaFiberQueue* scope;     /* filled by generated code before park */
    int             slot;      /* filled by generated code before park */
} SelectCtx;

static inline SelectCtx nova_select_init(int n_arms,
                                           SelectSlot* arms_storage,
                                           SelectWaiter* waiters_storage) {
    SelectCtx ctx;
    int i;
    ctx.arms     = arms_storage;
    ctx.waiters  = waiters_storage;
    ctx.n_arms   = n_arms;
    ctx.which    = -1;
    ctx.recv_val = 0;
    ctx.scope    = NULL;
    ctx.slot     = -1;
    /* Zero-fill ровно n_arms слотов. */
    for (i = 0; i < n_arms; i++) {
        ctx.arms[i].chan      = NULL;
        ctx.arms[i].is_recv   = false;
        ctx.arms[i].send_val  = 0;
        ctx.arms[i].guard     = false;
        ctx.arms[i].wildcard  = false;
        /* BaseWaiter prefix — обнуляем поля чтобы valid state. */
        ctx.waiters[i].base.scope    = NULL;
        ctx.waiters[i].base.slot     = -1;
        ctx.waiters[i].base.channel  = NULL;
        ctx.waiters[i].base.is_recv  = false;
        ctx.waiters[i].base.send_val = 0;
        ctx.waiters[i].base.next     = NULL;
        ctx.waiters[i].base.prev     = NULL;
        nova_aint_init(&ctx.waiters[i].base.fired, 0);
        nova_abool_init(&ctx.waiters[i].base.cancelled, false);
        ctx.waiters[i].arm_idx       = i;
    }
    return ctx;
}

static inline void nova_select_set_recv(SelectCtx* ctx, int n,
                                         Nova_ChanReader* rx, int guard,
                                         int wildcard) {
    if (n < 0 || n >= ctx->n_arms) return;
    ctx->arms[n].chan     = rx ? rx->state : NULL;
    ctx->arms[n].is_recv  = true;
    ctx->arms[n].guard    = (bool)guard;
    ctx->arms[n].wildcard = (bool)wildcard;
}

static inline void nova_select_set_send(SelectCtx* ctx, int n,
                                         Nova_ChanWriter* tx, nova_int val,
                                         int guard) {
    if (n < 0 || n >= ctx->n_arms) return;
    ctx->arms[n].chan      = tx ? tx->state : NULL;
    ctx->arms[n].is_recv   = false;
    ctx->arms[n].send_val  = val;
    ctx->arms[n].guard     = (bool)guard;
    ctx->arms[n].wildcard  = false;
}

/* Xorshift32 — fairness shuffle RNG seeded by ctx address. */
static inline uint32_t _nova_sel_rng(uint32_t* s) {
    *s ^= *s << 13; *s ^= *s >> 17; *s ^= *s << 5;
    return *s;
}

/* Try all enabled arms once in random order. Returns 1 if an arm fired.
 * Sets ctx->which and ctx->recv_val on success.
 *
 * Plan 40 R2: each channel locked individually around its mutation.
 * Plan 40 R2 §6: no need to hold multiple locks (optimistic re-scan
 * via post-park retry replaces "hold-all" Go pattern). */
static inline int nova_select_try_immediate(SelectCtx* ctx) {
    int n = ctx->n_arms, i, j;
    /* Fisher-Yates shuffle (Plan 40 Ф.3 final): alloca = ровно n. */
    int* order = (int*)alloca((size_t)n * sizeof(int));
    for (i = 0; i < n; i++) order[i] = i;

    uint32_t rng = (uint32_t)(uintptr_t)ctx ^ 0xdeadbeef;
    if (!rng) rng = 1;
    for (i = n - 1; i > 0; i--) {
        j = (int)(_nova_sel_rng(&rng) % (uint32_t)(i + 1));
        int tmp = order[i]; order[i] = order[j]; order[j] = tmp;
    }

    for (i = 0; i < n; i++) {
        int idx = order[i];
        SelectSlot* arm = &ctx->arms[idx];
        if (!arm->chan || !arm->guard) continue;
        Nova_ChannelState* st = arm->chan;

        nova_mutex_lock(&st->mu);
        if (arm->is_recv) {
            if (st->count > 0) {
                nova_int v = st->buf[st->head];
                st->head = (st->head + 1) % st->cap;
                st->count--;
                (void)_nova_channel_wake_send(st);
                nova_mutex_unlock(&st->mu);
                ctx->which = idx; ctx->recv_val = v;
                return 1;
            }
            /* wildcard `_ = rx` fires on closed channel; bound `Some(v) = rx` не fires. */
            if (nova_abool_load(&st->closed) && arm->wildcard) {
                nova_mutex_unlock(&st->mu);
                ctx->which = idx; ctx->recv_val = 0;
                return 1;
            }
        } else {
            if (!nova_abool_load(&st->closed) &&
                !nova_abool_load(&st->reader_closed) &&
                st->count < st->cap) {
                /* Hand-off to parked receiver if any (direct-copy). */
                if (st->recv_waiters &&
                    _nova_channel_wake_recv_with_value(st, arm->send_val)) {
                    nova_mutex_unlock(&st->mu);
                    ctx->which = idx;
                    return 1;
                }
                /* Push into buffer. */
                int64_t tail = (st->head + st->count) % st->cap;
                st->buf[tail] = arm->send_val;
                st->count++;
                nova_mutex_unlock(&st->mu);
                ctx->which = idx;
                return 1;
            }
        }
        nova_mutex_unlock(&st->mu);
    }
    return 0;
}

/* stop_cb for select-arm cancel-during-park. Plan 40 R2 C2: lock-free —
 * just set the atomic cancelled flag; wake helpers skip lazily. Wake
 * fiber so cancel_scope check fires after park. */
static NovaStopMode _nova_select_waiter_stop_cb(void* handle) {
    SelectWaiter* w = (SelectWaiter*)handle;
    nova_abool_store(&w->base.cancelled, true);
    if (w->base.channel) {
        nova_sched_wake(w->base.scope, w->base.slot);
    }
    return NOVA_STOP_SYNC;
}

/* Unlink select waiter under channel lock. Caller MUST hold w->base.channel->mu. */
static inline void _nova_sel_waiter_unlink_locked(SelectWaiter* w) {
    _nova_waiter_unlink_locked(&w->base);
}

/* Park until one select arm becomes ready. ctx->scope and ctx->slot must
 * be set before calling. On return ctx->which / ctx->recv_val are filled.
 *
 * Plan 40 R1/R2: production-grade protocol.
 *   - Plan 40 R2 §6: each channel locked individually around its
 *     waiter registration (no hold-all-locks). Post-park retry via
 *     try_immediate replaces Go's hold-all pattern — equivalent
 *     correctness, less lock traffic.
 *   - Plan 40 R3-1: BaseWaiter chain lives on fiber stack via compound
 *     literal; Boehm scans parked fiber stacks → safe.
 *   - Plan 40 R2 C3: panic if no enabled arms (silent forever-park is bug).
 *   - Plan 40 R3-7: post-park MUST re-check fired/try_immediate (spurious
 *     wakes allowed; correctness mechanism not optimization).
 */
static inline void nova_select_park(SelectCtx* ctx) {
    int n = ctx->n_arms, i;

    /* Plan 40 R2 C3: panic on no-enabled-arm. */
    int n_enabled = 0;
    for (i = 0; i < n; i++) {
        if (ctx->arms[i].chan && ctx->arms[i].guard) n_enabled++;
    }
    if (n_enabled == 0) {
        nova_throw(nova_str_from_cstr("select: no enabled arm"));
    }

    /* D94 Ф.6 (pre-check): count arms that could ever unblock us. */
    int can_unblock = 0;
    for (i = 0; i < n; i++) {
        SelectSlot* arm = &ctx->arms[i];
        if (!arm->chan || !arm->guard) continue;
        Nova_ChannelState* st = arm->chan;
        nova_bool cl = nova_abool_load(&st->closed);
        nova_bool rcl = nova_abool_load(&st->reader_closed);
        if (arm->is_recv && cl && st->count == 0) continue;
        if (!arm->is_recv && (cl || rcl)) continue;
        can_unblock++;
    }
    if (can_unblock == 0) {
        nova_throw(nova_str_from_cstr("select: all channels closed"));
    }

    NovaFiberQueue* scope = ctx->scope;
    int             slot  = ctx->slot;
    if (!scope || slot < 0) {
        fprintf(stderr, "nova: nova_select_park: scope/slot not set\n");
        abort();
    }

    /* Register a SelectWaiter on every enabled arm's channel waiter-list.
     * Each channel is locked individually for registration.
     *
     * Race window between two registrations is fine: a producer that
     * fires waiter i fires its CAS; post-park retry of try_immediate
     * sees the consumed state and picks the winner. */
    for (i = 0; i < n; i++) {
        SelectSlot*   arm = &ctx->arms[i];
        SelectWaiter*   w = &ctx->waiters[i];
        w->base.channel = NULL;
        if (!arm->chan || !arm->guard) continue;
        Nova_ChannelState* st = arm->chan;
        nova_bool cl  = nova_abool_load(&st->closed);
        nova_bool rcl = nova_abool_load(&st->reader_closed);
        if (arm->is_recv && cl && st->count == 0) continue;
        if (!arm->is_recv && (cl || rcl)) continue;
        w->base.scope    = scope;
        w->base.slot     = slot;
        w->base.channel  = st;
        w->base.is_recv  = arm->is_recv;
        w->base.send_val = arm->send_val;
        w->base.next     = NULL;
        w->base.prev     = NULL;
        nova_aint_init(&w->base.fired, 0);
        nova_abool_init(&w->base.cancelled, false);
        w->arm_idx       = i;
        nova_mutex_lock(&st->mu);
        _nova_waiter_insert_locked(&w->base);
        nova_mutex_unlock(&st->mu);
        nova_sched_register_pending(scope, slot, w, _nova_select_waiter_stop_cb);
    }

    /* Park. Plan 40 R3-7: spurious wakes allowed — post-park always re-checks. */
    nova_sched_park(scope, slot);
    nova_sched_unregister_pending(scope, slot);

    /* Unlink remaining waiters under their channel's lock. */
    for (i = 0; i < n; i++) {
        SelectWaiter* w = &ctx->waiters[i];
        if (!w->base.channel) continue;
        Nova_ChannelState* st = w->base.channel;
        nova_mutex_lock(&st->mu);
        if (w->base.channel) {
            _nova_waiter_unlink_locked(&w->base);
        }
        nova_mutex_unlock(&st->mu);
    }

    if (scope->cancel_requested) {
        nova_throw(nova_str_from_cstr("scope cancelled"));
    }

    /* Identify the winning arm. First check fired flags (a producer's wake
     * helper already CAS'd one of our waiters and copied value into its
     * send_val via direct-copy). */
    ctx->which = -1;
    for (i = 0; i < n; i++) {
        SelectWaiter* w = &ctx->waiters[i];
        if (nova_aint_load(&w->base.fired)) {
            ctx->which = w->arm_idx;
            ctx->recv_val = w->base.send_val;  /* direct-copy carrier */
            break;
        }
    }
    /* No waiter fired — try_immediate retry handles closed-channels case
     * (wildcard) and any state that changed between registration unlinks. */
    if (ctx->which < 0) {
        nova_select_try_immediate(ctx);
    }

    /* Plan 40 Ф.2 B7: fire on_select_lost callbacks for arms that did not win. */
    for (i = 0; i < n; i++) {
        if (i == ctx->which) continue;
        SelectSlot* arm = &ctx->arms[i];
        if (!arm->chan || !arm->guard) continue;
        if (arm->chan->on_select_lost) {
            arm->chan->on_select_lost(arm->chan);
        }
    }
}

/* ── Time.after — D94 timeout channel (Plan 31 Ф.5) ───────────── */

/* Heap-allocated timer state: lives until close_cb fires.
 * Plan 40 Ф.2 B7: `cancelled` flag для idempotent cancel из select wake. */
typedef struct {
    uv_timer_t       timer;
    Nova_ChanWriter* tx;
    bool             cancelled;  /* set once timer is stopped/closed early */
} NovaAfterState;

static void _nova_after_close_cb(uv_handle_t* h) {
    NovaAfterState* st = (NovaAfterState*)h->data;
    (void)st;
    /* state is heap-allocated; GC will collect it. tx already closed. */
}

static void _nova_after_timer_cb(uv_timer_t* h) {
    NovaAfterState* st = (NovaAfterState*)h->data;
    if (st->cancelled) return;  /* select wake cancelled us; do nothing */
    /* Non-blocking send: channel cap=1, always has room at this point. */
    nova_chan_writer_try_send(st->tx, 1);
    /* Close writer so reader sees channel as closed after consuming the value. */
    nova_chan_writer_close(st->tx);
    uv_close((uv_handle_t*)h, _nova_after_close_cb);
}

/* Plan 40 Ф.2 B7: on_select_lost callback — invoked from nova_select_park
 * when Time.after-arm did NOT win. Stops timer + closes uv handle so
 * background event-loop no longer dispatches us. Idempotent via `cancelled`. */
static void _nova_after_on_select_lost(Nova_ChannelState* st) {
    NovaAfterState* after = (NovaAfterState*)st->cleanup_data;
    if (!after || after->cancelled) return;
    after->cancelled = true;
    uv_timer_stop(&after->timer);
    /* Close writer so reader gets closed-state if it's reused outside select.
     * Idempotent through writer_closed guard. */
    nova_chan_writer_close(after->tx);
    uv_close((uv_handle_t*)&after->timer, _nova_after_close_cb);
}

/* Create a channel that receives one value after `ms` milliseconds.
 * Returns the reader end.  The timer fires in the event-loop background;
 * no fiber is parked.  Use in a select arm:
 *   Some(_) = Time.after(100) => { ... }  // timeout branch
 *
 * Plan 40 Ф.2 B7: if reader is used in select and another arm wins, the
 * select_lost callback stops the timer; otherwise timer fires normally. */
static inline Nova_ChanReader* Nova_Time_after(nova_int ms) {
    Nova_ChannelPair pair = nova_channel_new(1);
    NovaAfterState* st = (NovaAfterState*)nova_alloc(sizeof(NovaAfterState));
    st->tx = pair.tx;
    st->cancelled = false;
    int rc = uv_timer_init(nova_evloop(), &st->timer);
    if (rc != 0) {
        fprintf(stderr, "nova: Nova_Time_after: uv_timer_init failed: %s\n",
                uv_strerror(rc));
        abort();
    }
    st->timer.data = st;
    /* Plan 40 Ф.2 B7: register cleanup hook on the channel state. */
    pair.rx->state->on_select_lost = _nova_after_on_select_lost;
    pair.rx->state->cleanup_data   = st;
    uint64_t delay = ms > 0 ? (uint64_t)ms : 1;
    rc = uv_timer_start(&st->timer, _nova_after_timer_cb, delay, 0);
    if (rc != 0) {
        fprintf(stderr, "nova: Nova_Time_after: uv_timer_start failed: %s\n",
                uv_strerror(rc));
        uv_close((uv_handle_t*)&st->timer, NULL);
        abort();
    }
    return pair.rx;
}

#endif /* NOVA_RT_CHANNELS_H */
