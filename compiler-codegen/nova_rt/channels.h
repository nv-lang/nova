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
#include <stdio.h>
#include <stdlib.h>

/* ── Forward declarations ──────────────────────────────────────── */

typedef struct Nova_ChannelState Nova_ChannelState;
typedef struct ChannelWaiter     ChannelWaiter;

/* ── ChannelWaiter — heap-allocated (A1) ───────────────────────── */

struct ChannelWaiter {
    NovaFiberQueue*    scope;
    int                slot;
    Nova_ChannelState* channel;   /* back-pointer for unlink; NULL = unlinked */
    bool               is_recv;   /* true = recv-waiter, false = send-waiter */
    nova_int           send_val;  /* value to commit (send-waiter only) */
    ChannelWaiter*     next;
};

/* ── Channel state (hidden from Nova code) ─────────────────────── */

struct Nova_ChannelState {
    nova_int*      buf;
    int64_t        cap;
    int64_t        head;
    int64_t        count;
    bool           closed;
    int32_t        writer_count;  /* ref-count: channel closes when all writers call close() */
    ChannelWaiter* recv_waiters;  /* fibers parked waiting for data */
    ChannelWaiter* send_waiters;  /* fibers parked waiting for space */
};

/* ── Capability wrappers ───────────────────────────────────────── */

typedef struct { Nova_ChannelState* state; } Nova_ChanWriter;
typedef struct { Nova_ChannelState* state; } Nova_ChanReader;

/* Factory return type (A4). */
typedef struct { Nova_ChanWriter* tx; Nova_ChanReader* rx; } Nova_ChannelPair;

/* ── WaiterList helpers ────────────────────────────────────────── */

static inline void _nova_channel_waiter_unlink(ChannelWaiter* w) {
    if (!w->channel) return;
    Nova_ChannelState* st = w->channel;
    ChannelWaiter** head = w->is_recv ? &st->recv_waiters : &st->send_waiters;
    ChannelWaiter* prev = NULL;
    ChannelWaiter* cur  = *head;
    while (cur) {
        if (cur == w) {
            if (prev) prev->next = cur->next;
            else      *head      = cur->next;
            w->channel = NULL;
            return;
        }
        prev = cur;
        cur  = cur->next;
    }
    w->channel = NULL;
}

/* stop_cb for cancel-during-park (D93 Ф.8). SYNC — no async backend. */
static NovaStopMode _nova_channel_waiter_stop_cb(void* handle) {
    ChannelWaiter* w = (ChannelWaiter*)handle;
    _nova_channel_waiter_unlink(w);
    return NOVA_STOP_SYNC;
}

/* ── Factory ───────────────────────────────────────────────────── */

static inline Nova_ChannelPair nova_channel_new(int64_t capacity) {
    Nova_ChannelState* st = (Nova_ChannelState*)nova_alloc(sizeof(Nova_ChannelState));
    int64_t actual = capacity > 0 ? capacity : 1;
    st->buf          = (nova_int*)nova_alloc((size_t)actual * sizeof(nova_int));
    st->cap          = actual;
    st->head         = 0;
    st->count        = 0;
    st->closed       = false;
    st->writer_count = 1;
    st->recv_waiters = NULL;
    st->send_waiters = NULL;
    Nova_ChanWriter*   tx = (Nova_ChanWriter*)nova_alloc(sizeof(Nova_ChanWriter));
    Nova_ChanReader* rx = (Nova_ChanReader*)nova_alloc(sizeof(Nova_ChanReader));
    tx->state = st;
    rx->state = st;
    return (Nova_ChannelPair){ .tx = tx, .rx = rx };
}

/* ── Internal wake helpers ─────────────────────────────────────── */

static inline void _nova_channel_wake_recv(Nova_ChannelState* st) {
    if (!st->recv_waiters) return;
    ChannelWaiter* w = st->recv_waiters;
    st->recv_waiters = w->next;
    w->channel = NULL;
    nova_sched_wake(w->scope, w->slot);
}

/* Wake first send-waiter and commit its value into the buffer. */
static inline void _nova_channel_wake_send(Nova_ChannelState* st) {
    if (!st->send_waiters) return;
    ChannelWaiter* w = st->send_waiters;
    st->send_waiters = w->next;
    w->channel = NULL;
    int64_t tail = (st->head + st->count) % st->cap;
    st->buf[tail] = w->send_val;
    st->count++;
    nova_sched_wake(w->scope, w->slot);
}

/* ── Receiver ──────────────────────────────────────────────────── */

static inline NovaOpt_nova_int nova_chan_reader_recv(Nova_ChanReader* rx) {
    Nova_ChannelState* st = rx->state;

    if (st->count > 0) goto _take;
    if (st->closed)    goto _closed;

    {
        NovaFiberQueue* sc = _nova_active_scope;
        int             sl = _nova_active_slot;
        if (!sc || sl < 0) {
            fprintf(stderr,
                "nova: FATAL: nova_chan_reader_recv called outside fiber context\n");
            abort();
        }
        while (st->count == 0 && !st->closed) {
            ChannelWaiter* w = (ChannelWaiter*)nova_alloc(sizeof(ChannelWaiter));
            w->scope    = sc;
            w->slot     = sl;
            w->channel  = st;
            w->is_recv  = true;
            w->send_val = 0;
            w->next     = st->recv_waiters;
            st->recv_waiters = w;

            nova_sched_register_pending(sc, sl, w, _nova_channel_waiter_stop_cb);
            nova_sched_park(sc, sl);
            nova_sched_unregister_pending(sc, sl);

            if (sc->cancel_requested) {
                nova_throw(nova_str_from_cstr("scope cancelled"));
            }
        }
    }

    if (st->count == 0) goto _closed;

_take: {
        nova_int v = st->buf[st->head];
        st->head = (st->head + 1) % st->cap;
        st->count--;
        _nova_channel_wake_send(st);
        return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_Some, .value = v };
    }

_closed:
    return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_None, .value = 0 };
}

static inline NovaOpt_nova_int nova_chan_reader_try_recv(Nova_ChanReader* rx) {
    Nova_ChannelState* st = rx->state;
    if (st->count == 0) {
        return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_None, .value = 0 };
    }
    nova_int v = st->buf[st->head];
    st->head = (st->head + 1) % st->cap;
    st->count--;
    _nova_channel_wake_send(st);
    return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_Some, .value = v };
}

static inline nova_int   nova_chan_reader_len(Nova_ChanReader* rx)       { return (nova_int)rx->state->count;  }
static inline nova_int   nova_chan_reader_capacity(Nova_ChanReader* rx)  { return (nova_int)rx->state->cap;    }
static inline nova_bool  nova_chan_reader_is_closed(Nova_ChanReader* rx) { return (nova_bool)rx->state->closed;}

/* ── Sender ────────────────────────────────────────────────────── */

/* Returns true if value was sent, false if channel is closed (Plan 30 Ф.1). */
static inline nova_bool nova_chan_writer_send(Nova_ChanWriter* tx, nova_int v) {
    Nova_ChannelState* st = tx->state;

    if (st->closed) return 0;

    if (st->count < st->cap) goto _push;

    {
        NovaFiberQueue* sc = _nova_active_scope;
        int             sl = _nova_active_slot;
        if (!sc || sl < 0) {
            fprintf(stderr,
                "nova: FATAL: nova_chan_writer_send called outside fiber context\n");
            abort();
        }
        while (st->count >= st->cap && !st->closed) {
            ChannelWaiter* w = (ChannelWaiter*)nova_alloc(sizeof(ChannelWaiter));
            w->scope    = sc;
            w->slot     = sl;
            w->channel  = st;
            w->is_recv  = false;
            w->send_val = v;
            w->next     = st->send_waiters;
            st->send_waiters = w;

            nova_sched_register_pending(sc, sl, w, _nova_channel_waiter_stop_cb);
            nova_sched_park(sc, sl);
            nova_sched_unregister_pending(sc, sl);

            if (sc->cancel_requested) {
                nova_throw(nova_str_from_cstr("scope cancelled"));
            }
        }
        if (st->closed) return 0;
        /* recv-side committed our value into buffer already (A5) */
        return 1;
    }

_push: {
        int64_t tail = (st->head + st->count) % st->cap;
        st->buf[tail] = v;
        st->count++;
        _nova_channel_wake_recv(st);
        return 1;
    }
}

static inline nova_bool nova_chan_writer_try_send(Nova_ChanWriter* tx, nova_int v) {
    Nova_ChannelState* st = tx->state;
    if (st->closed || st->count >= st->cap) return 0;
    int64_t tail = (st->head + st->count) % st->cap;
    st->buf[tail] = v;
    st->count++;
    _nova_channel_wake_recv(st);
    return 1;
}

static inline void nova_chan_writer_close(Nova_ChanWriter* tx) {
    Nova_ChannelState* st = tx->state;
    if (st->closed) return;
    st->writer_count--;
    if (st->writer_count > 0) return;  /* other writers still alive */
    st->closed = true;
    while (st->recv_waiters) {
        ChannelWaiter* w = st->recv_waiters;
        st->recv_waiters = w->next;
        w->channel = NULL;
        nova_sched_wake(w->scope, w->slot);
    }
    while (st->send_waiters) {
        ChannelWaiter* w = st->send_waiters;
        st->send_waiters = w->next;
        w->channel = NULL;
        nova_sched_wake(w->scope, w->slot);
    }
}

/* Plan 30 Ф.2: clone creates a second writer sharing the same channel state. */
static inline Nova_ChanWriter* nova_chan_writer_clone(Nova_ChanWriter* tx) {
    tx->state->writer_count++;
    Nova_ChanWriter* clone = (Nova_ChanWriter*)nova_alloc(sizeof(Nova_ChanWriter));
    clone->state = tx->state;
    return clone;
}

static inline nova_int   nova_chan_writer_len(Nova_ChanWriter* tx)       { return (nova_int)tx->state->count;  }
static inline nova_int   nova_chan_writer_capacity(Nova_ChanWriter* tx)  { return (nova_int)tx->state->cap;    }
static inline nova_bool  nova_chan_writer_is_closed(Nova_ChanWriter* tx) { return (nova_bool)tx->state->closed;}

#endif /* NOVA_RT_CHANNELS_H */
