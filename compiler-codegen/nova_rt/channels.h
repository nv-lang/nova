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
    if (capacity <= 0) {
        nova_throw(nova_str_from_cstr("Channel.new: capacity must be >= 1"));
    }
    Nova_ChanWriter*   tx = (Nova_ChanWriter*)nova_alloc(sizeof(Nova_ChanWriter));
    Nova_ChanReader* rx = (Nova_ChanReader*)nova_alloc(sizeof(Nova_ChanReader));
    tx->state        = st;
    tx->writer_closed = false;
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
            nova_throw(nova_str_from_cstr("recv called outside fiber context"));
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

static inline NovaChanTryResult nova_chan_reader_try_recv(Nova_ChanReader* rx) {
    Nova_ChannelState* st = rx->state;
    if (st->count == 0) {
        NovaChanTryTag tag = st->closed ? NOVA_CHAN_TRY_CLOSED : NOVA_CHAN_TRY_EMPTY;
        return (NovaChanTryResult){ .tag = tag, .value = 0 };
    }
    nova_int v = st->buf[st->head];
    st->head = (st->head + 1) % st->cap;
    st->count--;
    _nova_channel_wake_send(st);
    return (NovaChanTryResult){ .tag = NOVA_CHAN_TRY_OK, .value = v };
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
            nova_throw(nova_str_from_cstr("send called outside fiber context"));
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

static inline NovaChanTryResult nova_chan_writer_try_send(Nova_ChanWriter* tx, nova_int v) {
    Nova_ChannelState* st = tx->state;
    if (st->closed)           return (NovaChanTryResult){ .tag = NOVA_CHAN_TRY_CLOSED, .value = 0 };
    if (st->count >= st->cap) return (NovaChanTryResult){ .tag = NOVA_CHAN_TRY_EMPTY,  .value = 0 };
    int64_t tail = (st->head + st->count) % st->cap;
    st->buf[tail] = v;
    st->count++;
    _nova_channel_wake_recv(st);
    return (NovaChanTryResult){ .tag = NOVA_CHAN_TRY_OK, .value = 0 };
}

static inline void nova_chan_writer_close(Nova_ChanWriter* tx) {
    if (tx->writer_closed) return;  /* per-writer idempotent guard */
    tx->writer_closed = true;
    Nova_ChannelState* st = tx->state;
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
    clone->state        = tx->state;
    clone->writer_closed = false;
    return clone;
}

static inline nova_int   nova_chan_writer_len(Nova_ChanWriter* tx)       { return (nova_int)tx->state->count;  }
static inline nova_int   nova_chan_writer_capacity(Nova_ChanWriter* tx)  { return (nova_int)tx->state->cap;    }
static inline nova_bool  nova_chan_writer_is_closed(Nova_ChanWriter* tx) { return (nova_bool)tx->state->closed;}

/* ── Select — D94 (Plan 31) ────────────────────────────────────── */

/* Maximum channel arms per select (stack-allocated). */
#define NOVA_SELECT_MAX_ARMS 16

typedef struct {
    Nova_ChannelState* chan;     /* NULL = slot unused or default arm */
    bool               is_recv;
    nova_int           send_val;
    bool               guard;   /* false → arm disabled */
    bool               wildcard; /* true = `_ = rx` fires on closed too; false = `Some(v) = rx` needs data */
} SelectSlot;

/* SelectWaiter: registered on channel's waiter-list while select is parked.
 *
 * Layout MUST match ChannelWaiter for the first 6 fields — channel wake
 * helpers cast ChannelWaiter* → call scope/slot → nova_sched_wake works.
 * arm_idx is select-only extra field. */
typedef struct SelectWaiter {
    /* ── Must match ChannelWaiter (first 6 fields) ── */
    NovaFiberQueue*      scope;
    int                  slot;
    Nova_ChannelState*   channel;   /* NULL when unlinked */
    bool                 is_recv;
    nova_int             send_val;
    struct SelectWaiter* next;
    /* ── select-only ── */
    int                  arm_idx;
} SelectWaiter;

typedef struct {
    SelectSlot      arms[NOVA_SELECT_MAX_ARMS];
    int             n_arms;    /* number of channel arms (excl. default) */
    int             which;     /* arm that fired: 0..n_arms-1, or -2 = default */
    nova_int        recv_val;  /* received value for winning recv arm */
    NovaFiberQueue* scope;     /* filled by generated code before park */
    int             slot;      /* filled by generated code before park */
    SelectWaiter    waiters[NOVA_SELECT_MAX_ARMS];
} SelectCtx;

static inline SelectCtx nova_select_init(int n_arms) {
    SelectCtx ctx;
    int i;
    for (i = 0; i < NOVA_SELECT_MAX_ARMS; i++) {
        ctx.arms[i].chan      = NULL;
        ctx.arms[i].is_recv   = false;
        ctx.arms[i].send_val  = 0;
        ctx.arms[i].guard     = false;
        ctx.arms[i].wildcard  = false;
    }
    ctx.n_arms   = n_arms;
    ctx.which    = -1;
    ctx.recv_val = 0;
    ctx.scope    = NULL;
    ctx.slot     = -1;
    return ctx;
}

static inline void nova_select_set_recv(SelectCtx* ctx, int n,
                                         Nova_ChanReader* rx, int guard,
                                         int wildcard) {
    if (n < 0 || n >= NOVA_SELECT_MAX_ARMS) return;
    ctx->arms[n].chan     = rx ? rx->state : NULL;
    ctx->arms[n].is_recv  = true;
    ctx->arms[n].guard    = (bool)guard;
    ctx->arms[n].wildcard = (bool)wildcard;
}

static inline void nova_select_set_send(SelectCtx* ctx, int n,
                                         Nova_ChanWriter* tx, nova_int val,
                                         int guard) {
    if (n < 0 || n >= NOVA_SELECT_MAX_ARMS) return;
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
 * Sets ctx->which and ctx->recv_val on success. */
static inline int nova_select_try_immediate(SelectCtx* ctx) {
    int order[NOVA_SELECT_MAX_ARMS];
    int n = ctx->n_arms, i, j;
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
        if (arm->is_recv) {
            if (st->count > 0) {
                nova_int v = st->buf[st->head];
                st->head = (st->head + 1) % st->cap;
                st->count--;
                _nova_channel_wake_send(st);
                ctx->which = idx; ctx->recv_val = v;
                return 1;
            }
            /* `_ = rx` (wildcard) fires on closed channel; `Some(v) = rx` does not */
            if (st->closed && arm->wildcard) {
                ctx->which = idx; ctx->recv_val = 0;
                return 1;
            }
        } else {
            if (!st->closed && st->count < st->cap) {
                int64_t tail = (st->head + st->count) % st->cap;
                st->buf[tail] = arm->send_val;
                st->count++;
                _nova_channel_wake_recv(st);
                ctx->which = idx;
                return 1;
            }
        }
    }
    return 0;
}

/* stop_cb: cancel during select park — unlink our waiter. SYNC. */
static NovaStopMode _nova_select_waiter_stop_cb(void* handle) {
    SelectWaiter* w = (SelectWaiter*)handle;
    if (!w->channel) return NOVA_STOP_SYNC;
    Nova_ChannelState* st = w->channel;
    ChannelWaiter** head = w->is_recv ? &st->recv_waiters : &st->send_waiters;
    ChannelWaiter* prev = NULL;
    ChannelWaiter* cur  = *head;
    while (cur) {
        if ((void*)cur == (void*)w) {
            if (prev) prev->next = cur->next;
            else      *head = cur->next;
            w->channel = NULL;
            break;
        }
        prev = cur; cur = cur->next;
    }
    return NOVA_STOP_SYNC;
}

static inline void _nova_sel_waiter_unlink(SelectWaiter* w) {
    if (!w->channel) return;
    Nova_ChannelState* st = w->channel;
    ChannelWaiter** head = w->is_recv ? &st->recv_waiters : &st->send_waiters;
    ChannelWaiter* prev = NULL;
    ChannelWaiter* cur  = *head;
    while (cur) {
        if ((void*)cur == (void*)w) {
            if (prev) prev->next = cur->next;
            else      *head = cur->next;
            w->channel = NULL;
            return;
        }
        prev = cur; cur = cur->next;
    }
    w->channel = NULL;
}

/* Park until one select arm becomes ready. ctx->scope and ctx->slot must
 * be set before calling. On return ctx->which / ctx->recv_val are filled. */
static inline void nova_select_park(SelectCtx* ctx) {
    int n = ctx->n_arms, i;

    /* D94 Ф.6 (pre-check): count arms that could ever unblock us.
     * Do this before checking scope/slot so the all-closed error fires
     * even outside a fiber (e.g. in main() or test code). */
    int can_unblock = 0;
    for (i = 0; i < n; i++) {
        SelectSlot* arm = &ctx->arms[i];
        if (!arm->chan || !arm->guard) continue;
        Nova_ChannelState* st = arm->chan;
        if (arm->is_recv && st->closed && st->count == 0) continue;
        if (!arm->is_recv && st->closed) continue;
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

    /* Register a SelectWaiter (layout-compatible with ChannelWaiter) on
     * every enabled arm's channel waiter-list.  When any channel operation
     * makes an arm ready it calls nova_sched_wake(w->scope, w->slot) through
     * the normal _nova_channel_wake_recv / _nova_channel_wake_send paths,
     * which wakes our fiber.  We then retry try_immediate to pick the winner. */
    for (i = 0; i < n; i++) {
        SelectSlot*   arm = &ctx->arms[i];
        SelectWaiter*   w = &ctx->waiters[i];
        w->channel = NULL;
        if (!arm->chan || !arm->guard) continue;
        Nova_ChannelState* st = arm->chan;
        /* Skip channels that are already closed — they can never unblock us.
         * (pre-check above already ensured at least one live arm exists.) */
        if (arm->is_recv && st->closed && st->count == 0) continue;
        if (!arm->is_recv && st->closed) continue;
        w->scope    = scope;
        w->slot     = slot;
        w->channel  = st;
        w->is_recv  = arm->is_recv;
        w->send_val = arm->send_val;
        w->next     = NULL;
        w->arm_idx  = i;
        if (arm->is_recv) {
            w->next = (SelectWaiter*)st->recv_waiters;
            st->recv_waiters = (ChannelWaiter*)w;
        } else {
            w->next = (SelectWaiter*)st->send_waiters;
            st->send_waiters = (ChannelWaiter*)w;
        }
        nova_sched_register_pending(scope, slot, w, _nova_select_waiter_stop_cb);
    }

    nova_sched_park(scope, slot);
    nova_sched_unregister_pending(scope, slot);

    /* Unlink remaining waiters (the winner was already popped by channel code). */
    for (i = 0; i < n; i++) {
        _nova_sel_waiter_unlink(&ctx->waiters[i]);
    }

    if (scope->cancel_requested) {
        nova_throw(nova_str_from_cstr("scope cancelled"));
    }

    /* Identify the winning arm. The channel that woke us already updated its
     * buffer; try_immediate reads the value atomically. */
    nova_select_try_immediate(ctx);
}

/* ── Time.after — D94 timeout channel (Plan 31 Ф.5) ───────────── */

/* Heap-allocated timer state: lives until close_cb fires. */
typedef struct {
    uv_timer_t       timer;
    Nova_ChanWriter* tx;
} NovaAfterState;

static void _nova_after_close_cb(uv_handle_t* h) {
    NovaAfterState* st = (NovaAfterState*)h->data;
    (void)st;
    /* state is heap-allocated; GC will collect it.  tx already closed. */
}

static void _nova_after_timer_cb(uv_timer_t* h) {
    NovaAfterState* st = (NovaAfterState*)h->data;
    /* Non-blocking send: channel cap=1, always has room at this point. */
    nova_chan_writer_try_send(st->tx, 1);
    /* Close writer so reader sees channel as closed after consuming the value. */
    nova_chan_writer_close(st->tx);
    uv_close((uv_handle_t*)h, _nova_after_close_cb);
}

/* Create a channel that receives one value after `ms` milliseconds.
 * Returns the reader end.  The timer fires in the event-loop background;
 * no fiber is parked.  Use in a select arm:
 *   Some(_) = Time.after(100) => { ... }  // timeout branch */
static inline Nova_ChanReader* Nova_Time_after(nova_int ms) {
    Nova_ChannelPair pair = nova_channel_new(1);
    NovaAfterState* st = (NovaAfterState*)nova_alloc(sizeof(NovaAfterState));
    st->tx = pair.tx;
    int rc = uv_timer_init(nova_evloop(), &st->timer);
    if (rc != 0) {
        fprintf(stderr, "nova: Nova_Time_after: uv_timer_init failed: %s\n",
                uv_strerror(rc));
        abort();
    }
    st->timer.data = st;
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
