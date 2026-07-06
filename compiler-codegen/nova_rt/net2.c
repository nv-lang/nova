/* Plan 183 Ф.1: nova_rt/net2.c — reworked std/net C substrate.
 *
 * See net2.h for the design contract (one FFI layer, byte transport, zero-copy,
 * no static result slots, M:N-safe park/wake). This file lives alongside the
 * legacy net.c during migration; ALL internal helpers are `static` so there is
 * no link collision with net.c's symbols.
 *
 * Park/wake/cancel follow the net.c mechanism (Plan 22 / D93) with one
 * hard-won correction (lost-wake, see below):
 *   1. Allocate the handle on the GC heap (nova_alloc_uncollectable — a live uv
 *      handle references the struct, so GC must not move/collect it).
 *   2. Register a stop_cb for cancel integration (D93).
 *   3. Park the current fiber via nova_sched_park_until (predicate!).
 *   4. The libuv callback (on the owning loop thread) stores the result INTO the
 *      per-operation fields of the handle, sets the op's atomic `done` flag,
 *      and calls nova_sched_wake.
 *   5. The fiber resumes, unregisters, checks cancel_requested.
 * The result never transits a __thread slot — it lands in the parked fiber's own
 * handle struct, which is exactly what fixes the M:N data race (Д2).
 *
 * LOST-WAKE CORRECTION (Plan 183 Ф.1, found by UDP smoke TIMEOUT ~3/10):
 * nova_sched_wake resolves the target fiber via parked_co[slot], which is set
 * only INSIDE nova_sched_park. libuv callbacks run on the loop-owning thread,
 * concurrently with the issuing fiber's worker under M:N — a callback firing
 * between the uv-op issue and the park finds parked_co==NULL and the wake is
 * silently dropped → the fiber parks forever. net.c's naive single-shot
 * `nova_sched_park` has this hole on every op. The lost-wake-free pattern
 * (same as channels/sync): publish scope/slot + reset the atomic `done` flag
 * BEFORE issuing the op; the callback stores results, then `done=1`, then
 * wake; the fiber parks with nova_sched_park_until(pred) — the pred fast-path
 * consumes a completion that beat the park, re-park absorbs spurious wakes.
 * Predicates also treat stage >= CLOSING as completion so close/cancel wakes
 * are never waited out.
 *
 * LOOP-AFFINITY CONTRACT (Plan 183 Ф.4, found by UDP flake ~2/10 TIMEOUT):
 * a uv handle is pinned to the loop it was created on (nova_current_loop() at
 * bind/connect/accept time), and libuv loops are NOT thread-safe. Under M:N
 * every worker owns its own loop, so an op issued on a handle from a different
 * worker (or from a worker fiber on a main-thread-bound handle while the main
 * thread pumps _evloop in the supervised drain) is concurrent cross-thread
 * loop mutation: the req is mis-queued and its completion callback never
 * fires — the parked fiber hangs. This is NOT a lost-wake (the latch is
 * sound) and NOT datagram loss. The only cross-thread-safe libuv entry point
 * is uv_async_send (used by nova_loop_defer_close) — everything else must run
 * on the loop's own thread. Callers (std/net2 tests, user code) must
 * therefore create a socket INSIDE the fiber that operates it. Lifting the
 * constraint = marshalling op issue to the owning loop thread via a defer-op
 * queue (generalisation of nova_loop_defer_close) — backlog
 * [M-183-net2-loop-affinity-cross-thread-op].
 */

#ifndef NOVA_USE_LIBUV
#  error "Plan 183: NOVA_USE_LIBUV required."
#endif

#include "net2.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Canonical error strings for the two codes std/net classifies into typed
 * NetError variants (kept byte-identical to net.c so the Nova match is stable). */
#define NN2_MSG_PERMISSION_DENIED "permission denied"
#define NN2_MSG_CONNECTION_RESET  "connection reset by peer"

/* ─── Stage enum (shared by all net2 handle types) ─────────────────────────── */

enum {
    NN2_IDLE    = 0,
    NN2_PENDING = 1,
    NN2_CLOSING = 2,
    NN2_CLOSED  = 3,
};

/* ─── Cancel-scope helper (same pattern as net.c _nova_net_cancel_scope) ─────── */

static inline NovaFiberQueue* _nn2_cancel_scope(NovaFiberQueue* scope) {
    mco_coro* rc = mco_running();
    if (rc) {
        NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(rc);
        if (base && base->_nova_parent_scope) {
            return (NovaFiberQueue*)base->_nova_parent_scope;
        }
    }
    return scope;
}

/* ─── Address helpers ──────────────────────────────────────────────────────── */

static NovaNetAddr* _nn2_alloc_addr(void) {
    NovaNetAddr* a = (NovaNetAddr*)nova_alloc(sizeof(NovaNetAddr));
    memset(a, 0, sizeof(*a));
    return a;
}

static void _nn2_addr_to_ss(const NovaNetAddr* a, struct sockaddr_storage* ss) {
    memset(ss, 0, sizeof(*ss));
    if (a->family == 6) {
        struct sockaddr_in6* in6 = (struct sockaddr_in6*)ss;
        in6->sin6_family = AF_INET6;
        in6->sin6_port   = htons(a->port);
        memcpy(&in6->sin6_addr, a->bytes, 16);
    } else {
        struct sockaddr_in* in4 = (struct sockaddr_in*)ss;
        in4->sin_family = AF_INET;
        in4->sin_port   = htons(a->port);
        memcpy(&in4->sin_addr, a->bytes, 4);
    }
}

/* One of the named, unavoidable OS-transfers (D407 §2а): sockaddr_storage → the
 * 16-byte value address. NOT in the read/write hot path. */
static void _nn2_addr_from_ss(const struct sockaddr_storage* ss, NovaNetAddr* a) {
    memset(a, 0, sizeof(*a));
    if (ss->ss_family == AF_INET6) {
        const struct sockaddr_in6* in6 = (const struct sockaddr_in6*)ss;
        a->family = 6;
        a->port   = ntohs(in6->sin6_port);
        memcpy(a->bytes, &in6->sin6_addr, 16);
    } else {
        const struct sockaddr_in* in4 = (const struct sockaddr_in*)ss;
        a->family = 4;
        a->port   = ntohs(in4->sin_port);
        memcpy(a->bytes, &in4->sin_addr, 4);
    }
}

NovaNetAddr* nova_net_addr_loopback(uint16_t port) {
    NovaNetAddr* a = _nn2_alloc_addr();
    struct sockaddr_in in4;
    uv_ip4_addr("127.0.0.1", port, &in4);
    struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
    memcpy(&ss, &in4, sizeof(in4));
    _nn2_addr_from_ss(&ss, a);
    return a;
}

NovaNetAddr* nova_net_addr_loopback_v6(uint16_t port) {
    NovaNetAddr* a = _nn2_alloc_addr();
    struct sockaddr_in6 in6;
    uv_ip6_addr("::1", port, &in6);
    struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
    memcpy(&ss, &in6, sizeof(in6));
    _nn2_addr_from_ss(&ss, a);
    return a;
}

NovaNetAddr* nova_net_addr_v4(uint8_t a, uint8_t b, uint8_t c, uint8_t d,
                              uint16_t port) {
    NovaNetAddr* out = _nn2_alloc_addr();
    out->family  = 4;
    out->port    = port;
    out->bytes[0] = a; out->bytes[1] = b; out->bytes[2] = c; out->bytes[3] = d;
    return out;
}

/* Ф.2: value-record construction straight into the caller's 20-byte []u8 image
 * (NovaNetAddr POD) — no nova_alloc, no C-owned handle. The Nova SocketAddr
 * value owns these bytes; layout stays C-owned here so the .nv side never bakes
 * struct offsets / endianness (D407 §5). */
void nova_net_addr_loopback_into(uint16_t port, uint8_t* out) {
    NovaNetAddr* a = (NovaNetAddr*)out;
    struct sockaddr_in in4;
    uv_ip4_addr("127.0.0.1", port, &in4);
    struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
    memcpy(&ss, &in4, sizeof(in4));
    _nn2_addr_from_ss(&ss, a);
}

void nova_net_addr_loopback_v6_into(uint16_t port, uint8_t* out) {
    NovaNetAddr* a = (NovaNetAddr*)out;
    struct sockaddr_in6 in6;
    uv_ip6_addr("::1", port, &in6);
    struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
    memcpy(&ss, &in6, sizeof(in6));
    _nn2_addr_from_ss(&ss, a);
}

void nova_net_addr_v4_into(uint8_t a, uint8_t b, uint8_t c, uint8_t d,
                           uint16_t port, uint8_t* out) {
    NovaNetAddr* r = (NovaNetAddr*)out;
    memset(r, 0, sizeof(*r));
    r->family = 4;
    r->port   = port;
    r->bytes[0] = a; r->bytes[1] = b; r->bytes[2] = c; r->bytes[3] = d;
}

nova_int nova_net_addr_parse(const uint8_t* s, nova_int len, NovaNetAddr* out) {
    char* buf = (char*)alloca((size_t)len + 1);
    memcpy(buf, s, (size_t)len);
    buf[len] = '\0';

    char* colon = strrchr(buf, ':');
    if (!colon) return 1;

    int port_n = atoi(colon + 1);
    if (port_n <= 0 || port_n > 65535) return 2;
    *colon = '\0';

    struct sockaddr_in in4;
    if (uv_ip4_addr(buf, port_n, &in4) == 0) {
        struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
        memcpy(&ss, &in4, sizeof(in4));
        _nn2_addr_from_ss(&ss, out);
        return 0;
    }

    char* host = buf;
    if (host[0] == '[') {
        host++;
        char* rbrace = strchr(host, ']');
        if (rbrace) *rbrace = '\0';
    }
    struct sockaddr_in6 in6;
    if (uv_ip6_addr(host, port_n, &in6) == 0) {
        struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
        memcpy(&ss, &in6, sizeof(in6));
        _nn2_addr_from_ss(&ss, out);
        return 0;
    }
    return 1;
}

uint16_t nova_net_addr_port(const NovaNetAddr* a) { return a->port; }
nova_bool nova_net_addr_is_v4(const NovaNetAddr* a) { return a->family == 4; }
nova_bool nova_net_addr_is_v6(const NovaNetAddr* a) { return a->family == 6; }

nova_int nova_net_addr_ip(const NovaNetAddr* a, uint8_t* buf, nova_int cap) {
    char tmp[64];
    struct sockaddr_storage ss;
    _nn2_addr_to_ss(a, &ss);
    if (a->family == 6) {
        uv_ip6_name((const struct sockaddr_in6*)&ss, tmp, sizeof(tmp));
    } else {
        uv_ip4_name((const struct sockaddr_in*)&ss, tmp, sizeof(tmp));
    }
    nova_int n = (nova_int)strlen(tmp);
    if (n > cap) n = cap;
    memcpy(buf, tmp, (size_t)n);
    return n;
}

nova_int nova_net_addr_to_str(const NovaNetAddr* a, uint8_t* buf, nova_int cap) {
    char host[64];
    char tmp[128];
    struct sockaddr_storage ss;
    _nn2_addr_to_ss(a, &ss);
    if (a->family == 6) {
        uv_ip6_name((const struct sockaddr_in6*)&ss, host, sizeof(host));
        snprintf(tmp, sizeof(tmp), "[%s]:%u", host, (unsigned)a->port);
    } else {
        uv_ip4_name((const struct sockaddr_in*)&ss, host, sizeof(host));
        snprintf(tmp, sizeof(tmp), "%s:%u", host, (unsigned)a->port);
    }
    nova_int n = (nova_int)strlen(tmp);
    if (n > cap) n = cap;
    memcpy(buf, tmp, (size_t)n);
    return n;
}

nova_int nova_net_strerror(nova_int code, uint8_t* buf, nova_int cap) {
    const char* msg;
    switch (code) {
        case UV_EACCES:     msg = NN2_MSG_PERMISSION_DENIED; break;
        case UV_ECONNRESET: msg = NN2_MSG_CONNECTION_RESET;  break;
        default:            msg = uv_strerror(code);         break;
    }
    nova_int n = (nova_int)strlen(msg);
    if (n > cap) n = cap;
    memcpy(buf, msg, (size_t)n);
    return n;
}

/* ═══════════════════════════════════════════════════════════════════════════
 * TCP
 * ═══════════════════════════════════════════════════════════════════════════ */

typedef struct NovaNet2Listener {
    uv_tcp_t        handle;        /* must be first (uv_close compat) */
    uv_loop_t*      loop;
    nova_atomic_int stage;
    NovaFiberQueue* accept_scope;  /* NULL when no waiter */
    int             accept_slot;
    nova_atomic_int pending_conns; /* incremented by connection_cb (loop thread) */
} NovaNet2Listener;

typedef struct NovaNet2Stream {
    uv_tcp_t        handle;        /* must be first */
    uv_loop_t*      loop;
    nova_atomic_int stage;

    /* connect path */
    uv_connect_t    connect_req;
    NovaFiberQueue* op_scope;      /* connect waiter (NULL = none) */
    int             op_slot;
    int             op_err;        /* UV code (0 = ok) */
    nova_atomic_int op_done;       /* completion latch (lost-wake fix) */

    /* read path (independent slot → full-duplex without a split C-API) */
    NovaFiberQueue* read_scope;
    int             read_slot;
    uint8_t*        read_ptr;      /* caller's buffer — libuv reads straight in */
    nova_int        read_cap;
    nova_int        read_n;        /* >0 bytes / 0 EOF (via read_eof) */
    int             read_err;      /* UV code (0 = ok) */
    int             read_eof;
    nova_atomic_int read_done;     /* completion latch */

    /* write path (independent slot) */
    uv_write_t      write_req;
    NovaFiberQueue* write_scope;
    int             write_slot;
    nova_int        write_n;
    int             write_err;
    nova_atomic_int write_done;    /* completion latch */

    /* shutdown (half-close) */
    uv_shutdown_t   shutdown_req;

    volatile int32_t split_refcount; /* 0 = unsplit / 2 / 1 */
} NovaNet2Stream;

/* ─── Park predicates (lost-wake-free, see file header) ──────────────────────
 * Each returns true when the op's completion latch is set OR the handle is
 * closing/closed (close_cb / cancel path). Reads are SEQ_CST via nova_aint_load;
 * callbacks publish results BEFORE the latch store. */

static nova_bool _nn2_accept_ready(void* ctx) {
    NovaNet2Listener* lst = (NovaNet2Listener*)ctx;
    return nova_aint_load(&lst->pending_conns) > 0
        || nova_aint_load(&lst->stage) >= NN2_CLOSING;
}
static nova_bool _nn2_stream_op_ready(void* ctx) {
    NovaNet2Stream* s = (NovaNet2Stream*)ctx;
    return nova_aint_load(&s->op_done) != 0
        || nova_aint_load(&s->stage) >= NN2_CLOSING;
}
static nova_bool _nn2_stream_read_ready(void* ctx) {
    NovaNet2Stream* s = (NovaNet2Stream*)ctx;
    return nova_aint_load(&s->read_done) != 0
        || nova_aint_load(&s->stage) >= NN2_CLOSING;
}
static nova_bool _nn2_stream_write_ready(void* ctx) {
    NovaNet2Stream* s = (NovaNet2Stream*)ctx;
    return nova_aint_load(&s->write_done) != 0
        || nova_aint_load(&s->stage) >= NN2_CLOSING;
}

/* forward decls */
static void         _nn2_stream_close_cb(uv_handle_t* h);
static NovaStopMode _nn2_stream_stop_cb(void* handle);
static void         _nn2_listener_close_cb(uv_handle_t* h);
static NovaStopMode _nn2_listener_stop_cb(void* handle);

/* ─── TcpListener ──────────────────────────────────────────────────────────── */

static void _nn2_connection_cb(uv_stream_t* srv, int status) {
    NovaNet2Listener* lst = (NovaNet2Listener*)srv->data;
    if (status >= 0) {
        /* Publish BEFORE the wake attempt — the accept predicate reads it. */
        __atomic_fetch_add((volatile int32_t*)&lst->pending_conns, 1,
                           __ATOMIC_ACQ_REL);
    }
    if (lst->accept_scope) {
        NovaFiberQueue* sc = lst->accept_scope; int sl = lst->accept_slot;
        lst->accept_scope = NULL;
        nova_sched_wake(sc, sl);
    }
}

void* nova_net_tcp_listen(const NovaNetAddr* addr, nova_int backlog, nova_int* out_err) {
    uv_loop_t* loop = nova_current_loop();
    NovaNet2Listener* lst =
        (NovaNet2Listener*)nova_alloc_uncollectable(sizeof(NovaNet2Listener));
    memset(lst, 0, sizeof(*lst));
    nova_aint_init(&lst->stage, NN2_IDLE);
    lst->loop = loop;
    lst->handle.data = lst;

    int rc = uv_tcp_init(loop, &lst->handle);
    if (rc != 0) { if (out_err) *out_err = rc; return NULL; }
    uv_tcp_simultaneous_accepts(&lst->handle, 1);

    struct sockaddr_storage ss;
    _nn2_addr_to_ss(addr, &ss);
    rc = uv_tcp_bind(&lst->handle, (const struct sockaddr*)&ss, 0);
    if (rc != 0) {
        if (out_err) *out_err = rc;
        uv_close((uv_handle_t*)&lst->handle, _nn2_listener_close_cb);
        return NULL;
    }
    rc = uv_listen((uv_stream_t*)&lst->handle,
                   backlog > 0 ? backlog : 128, _nn2_connection_cb);
    if (rc != 0) {
        if (out_err) *out_err = rc;
        uv_close((uv_handle_t*)&lst->handle, _nn2_listener_close_cb);
        return NULL;
    }
    if (out_err) *out_err = 0;
    return lst;
}

static NovaStopMode _nn2_listener_stop_cb(void* handle) {
    NovaNet2Listener* lst = (NovaNet2Listener*)handle;
    int32_t expected = NN2_IDLE;
    if (__atomic_compare_exchange_n((volatile int32_t*)&lst->stage,
            &expected, NN2_CLOSING, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        nova_loop_defer_close(lst->loop, (uv_handle_t*)&lst->handle,
                              _nn2_listener_close_cb);
    }
    return NOVA_STOP_ASYNC;
}

static void _nn2_listener_close_cb(uv_handle_t* h) {
    NovaNet2Listener* lst = (NovaNet2Listener*)h->data;
    nova_aint_store(&lst->stage, NN2_CLOSED);
    if (lst->accept_scope) {
        NovaFiberQueue* sc = lst->accept_scope; int sl = lst->accept_slot;
        lst->accept_scope = NULL;
        nova_sched_wake(sc, sl);
    }
}

void* nova_net_tcp_accept(void* lstv, nova_int* out_err) {
    NovaNet2Listener* lst = (NovaNet2Listener*)lstv;
    int32_t s = nova_aint_load(&lst->stage);
    if (s >= NN2_CLOSING) { if (out_err) *out_err = UV_ECANCELED; return NULL; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net2: accept outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        if (out_err) *out_err = UV_ECANCELED; return NULL;
    }

    for (;;) {
        /* CAS-claim one pending connection (connection_cb increments on the
         * loop thread — plain -- would race). */
        int32_t pc = nova_aint_load(&lst->pending_conns);
        if (pc > 0) {
            if (__atomic_compare_exchange_n(
                    (volatile int32_t*)&lst->pending_conns, &pc, pc - 1,
                    0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
                break; /* claimed */
            }
            continue;  /* lost the CAS — retry */
        }
        if (nova_aint_load(&lst->stage) >= NN2_CLOSING) {
            if (out_err) *out_err = UV_ECANCELED; return NULL;
        }
        /* Publish the waiter BEFORE parking; the predicate absorbs a
         * connection_cb that fires in the gap (lost-wake-free). */
        lst->accept_scope = scope;
        lst->accept_slot  = slot;
        nova_sched_register_pending(scope, slot, lst, _nn2_listener_stop_cb);
        nova_sched_park_until(scope, slot, _nn2_accept_ready, lst);
        nova_sched_unregister_pending(scope, slot);
        lst->accept_scope = NULL;

        if (nova_abool_load(&cancel_sc->cancel_requested)) {
            if (out_err) *out_err = UV_ECANCELED; return NULL;
        }
        if (nova_aint_load(&lst->stage) >= NN2_CLOSING) {
            if (out_err) *out_err = UV_ECANCELED; return NULL;
        }
    }

    uv_loop_t* loop = nova_current_loop();
    NovaNet2Stream* st =
        (NovaNet2Stream*)nova_alloc_uncollectable(sizeof(NovaNet2Stream));
    memset(st, 0, sizeof(*st));
    nova_aint_init(&st->stage, NN2_IDLE);
    st->loop = loop;
    st->handle.data = st;

    int rc = uv_tcp_init(loop, &st->handle);
    if (rc != 0) { if (out_err) *out_err = rc; return NULL; }
    rc = uv_accept((uv_stream_t*)&lst->handle, (uv_stream_t*)&st->handle);
    if (rc != 0) {
        if (out_err) *out_err = rc;
        uv_close((uv_handle_t*)&st->handle, NULL);
        return NULL;
    }
    if (out_err) *out_err = 0;
    return st;
}

uint16_t nova_net_listener_local_port(void* lstv) {
    NovaNet2Listener* lst = (NovaNet2Listener*)lstv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getsockname(&lst->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

void nova_net_listener_local_addr(void* lstv, NovaNetAddr* out) {
    NovaNet2Listener* lst = (NovaNet2Listener*)lstv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    memset(&ss, 0, sizeof(ss));
    uv_tcp_getsockname(&lst->handle, (struct sockaddr*)&ss, &n);
    _nn2_addr_from_ss(&ss, out);
}

void nova_net_listener_set_reuse_address(void* lstv, nova_bool on) {
    (void)lstv; (void)on;  /* libuv sets SO_REUSEADDR by default at bind */
}

void nova_net_listener_close(void* lstv) {
    NovaNet2Listener* lst = (NovaNet2Listener*)lstv;
    int32_t expected = NN2_IDLE;
    if (__atomic_compare_exchange_n((volatile int32_t*)&lst->stage,
            &expected, NN2_CLOSING, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        nova_loop_defer_close(lst->loop, (uv_handle_t*)&lst->handle,
                              _nn2_listener_close_cb);
    }
}

/* ─── TcpStream ────────────────────────────────────────────────────────────── */

static void _nn2_connect_cb(uv_connect_t* req, int status) {
    NovaNet2Stream* s = (NovaNet2Stream*)req->data;
    s->op_err = status;
    nova_aint_store(&s->op_done, 1);   /* results published → latch → wake */
    NovaFiberQueue* sc = s->op_scope; int sl = s->op_slot;
    s->op_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

static NovaStopMode _nn2_stream_stop_cb(void* handle) {
    NovaNet2Stream* s = (NovaNet2Stream*)handle;
    int32_t expected = NN2_IDLE;
    /* Try IDLE→CLOSING; if a read/write is PENDING, transition that too. */
    if (!__atomic_compare_exchange_n((volatile int32_t*)&s->stage,
            &expected, NN2_CLOSING, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        expected = NN2_PENDING;
        if (!__atomic_compare_exchange_n((volatile int32_t*)&s->stage,
                &expected, NN2_CLOSING, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
            return NOVA_STOP_ASYNC;  /* already closing/closed */
        }
    }
    nova_loop_defer_close(s->loop, (uv_handle_t*)&s->handle, _nn2_stream_close_cb);
    return NOVA_STOP_ASYNC;
}

static void _nn2_stream_close_cb(uv_handle_t* h) {
    NovaNet2Stream* s = (NovaNet2Stream*)h->data;
    nova_aint_store(&s->stage, NN2_CLOSED);
    /* Wake every parked waiter so each unwinds with "closed". */
    if (s->op_scope)    { NovaFiberQueue* sc = s->op_scope;    int sl = s->op_slot;    s->op_scope = NULL;    nova_sched_wake(sc, sl); }
    if (s->read_scope)  { NovaFiberQueue* sc = s->read_scope;  int sl = s->read_slot;  s->read_scope = NULL;  nova_sched_wake(sc, sl); }
    if (s->write_scope) { NovaFiberQueue* sc = s->write_scope; int sl = s->write_slot; s->write_scope = NULL; nova_sched_wake(sc, sl); }
}

void* nova_net_tcp_connect(const NovaNetAddr* addr, nova_int* out_err) {
    uv_loop_t* loop = nova_current_loop();
    NovaNet2Stream* s =
        (NovaNet2Stream*)nova_alloc_uncollectable(sizeof(NovaNet2Stream));
    memset(s, 0, sizeof(*s));
    nova_aint_init(&s->stage, NN2_IDLE);
    s->loop = loop;
    s->handle.data = s;
    s->connect_req.data = s;

    int rc = uv_tcp_init(loop, &s->handle);
    if (rc != 0) { if (out_err) *out_err = rc; return NULL; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net2: connect outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        if (out_err) *out_err = UV_ECANCELED;
        uv_close((uv_handle_t*)&s->handle, _nn2_stream_close_cb);
        return NULL;
    }

    /* Publish waiter + latch BEFORE issuing the op (lost-wake-free): the
     * connect_cb may fire on the loop thread before we park. */
    nova_aint_store(&s->stage, NN2_PENDING);
    nova_aint_init(&s->op_done, 0);
    s->op_scope = scope;
    s->op_slot  = slot;
    nova_sched_register_pending(scope, slot, s, _nn2_stream_stop_cb);

    struct sockaddr_storage ss;
    _nn2_addr_to_ss(addr, &ss);
    rc = uv_tcp_connect(&s->connect_req, &s->handle,
                        (const struct sockaddr*)&ss, _nn2_connect_cb);
    if (rc != 0) {
        nova_sched_unregister_pending(scope, slot);
        s->op_scope = NULL;
        if (out_err) *out_err = rc;
        uv_close((uv_handle_t*)&s->handle, _nn2_stream_close_cb);
        return NULL;
    }

    nova_sched_park_until(scope, slot, _nn2_stream_op_ready, s);
    nova_sched_unregister_pending(scope, slot);

    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        if (out_err) *out_err = UV_ECANCELED; return NULL;
    }
    if (nova_aint_load(&s->stage) == NN2_CLOSED) {
        if (out_err) *out_err = UV_ECANCELED; return NULL;
    }
    if (s->op_err != 0) { if (out_err) *out_err = s->op_err; return NULL; }

    nova_aint_store(&s->stage, NN2_IDLE);
    if (out_err) *out_err = 0;
    return s;
}

/* alloc_cb: hand libuv the CALLER's buffer slice (zero-copy read). */
static void _nn2_read_alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf) {
    (void)suggested;
    NovaNet2Stream* s = (NovaNet2Stream*)h->data;
    buf->base = (char*)s->read_ptr;
    buf->len  = (unsigned long)s->read_cap;
}

static void _nn2_read_cb(uv_stream_t* stream, ssize_t nread,
                         const uv_buf_t* buf) {
    (void)buf;
    NovaNet2Stream* s = (NovaNet2Stream*)stream->data;
    if (nread == 0) return;  /* EAGAIN: no data yet — stay parked */
    uv_read_stop(stream);
    if (nread == UV_EOF) {
        s->read_n   = 0;
        s->read_eof = 1;
        s->read_err = 0;
    } else if (nread < 0) {
        s->read_n   = 0;
        s->read_err = (int)nread;
    } else {
        s->read_n   = nread;   /* data already in the caller's buffer */
        s->read_err = 0;
    }
    nova_aint_store(&s->read_done, 1);  /* results published → latch → wake */
    NovaFiberQueue* sc = s->read_scope; int sl = s->read_slot;
    s->read_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

nova_int nova_net_tcp_read(void* sv, uint8_t* buf, nova_int cap) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    int32_t st = nova_aint_load(&s->stage);
    if (st >= NN2_CLOSING) return UV_ECANCELED;
    if (cap <= 0) return 0;

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net2: read outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) return UV_ECANCELED;

    /* Publish waiter + buffer + latch BEFORE uv_read_start (lost-wake-free).
     * NOTE: no stage transition here — read and write halves run full-duplex
     * concurrently on the same handle (independent scopes + latches); stage is
     * only IDLE/CLOSING/CLOSED for streams after connect. */
    s->read_ptr = buf;
    s->read_cap = cap;
    s->read_n   = 0;
    s->read_eof = 0;
    s->read_err = 0;
    nova_aint_store(&s->read_done, 0);
    s->read_scope = scope;
    s->read_slot  = slot;
    nova_sched_register_pending(scope, slot, s, _nn2_stream_stop_cb);

    int rc = uv_read_start((uv_stream_t*)&s->handle,
                           _nn2_read_alloc_cb, _nn2_read_cb);
    if (rc != 0) {
        nova_sched_unregister_pending(scope, slot);
        s->read_scope = NULL;
        return rc;
    }

    nova_sched_park_until(scope, slot, _nn2_stream_read_ready, s);
    nova_sched_unregister_pending(scope, slot);

    if (nova_abool_load(&cancel_sc->cancel_requested)) return UV_ECANCELED;
    if (nova_aint_load(&s->stage) >= NN2_CLOSING)       return UV_ECANCELED;

    if (s->read_err != 0) return s->read_err;
    if (s->read_eof)      return 0;   /* clean EOF: 0 bytes */
    return s->read_n;
}

static void _nn2_write_cb(uv_write_t* req, int status) {
    NovaNet2Stream* s = (NovaNet2Stream*)req->data;
    s->write_err = status;
    nova_aint_store(&s->write_done, 1);  /* results published → latch → wake */
    NovaFiberQueue* sc = s->write_scope; int sl = s->write_slot;
    s->write_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

nova_int nova_net_tcp_write(void* sv, const uint8_t* buf, nova_int len) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    int32_t st = nova_aint_load(&s->stage);
    if (st >= NN2_CLOSING) return UV_ECANCELED;
    if (len == 0) return 0;

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net2: write outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) return UV_ECANCELED;

    /* Zero-copy: uv_write points straight at the caller's []u8 memory, which the
     * Nova caller keeps alive on its fiber stack across this parked call.
     * Publish waiter + latch BEFORE uv_write (lost-wake-free); no stage
     * transition (full-duplex with a concurrent read, see read comment). */
    uv_buf_t ubuf = uv_buf_init((char*)(uintptr_t)buf, (unsigned int)len);
    s->write_req.data = s;
    s->write_n   = len;
    s->write_err = 0;
    nova_aint_store(&s->write_done, 0);
    s->write_scope = scope;
    s->write_slot  = slot;
    nova_sched_register_pending(scope, slot, s, _nn2_stream_stop_cb);

    int rc = uv_write(&s->write_req, (uv_stream_t*)&s->handle, &ubuf, 1,
                      _nn2_write_cb);
    if (rc != 0) {
        nova_sched_unregister_pending(scope, slot);
        s->write_scope = NULL;
        return rc;
    }

    nova_sched_park_until(scope, slot, _nn2_stream_write_ready, s);
    nova_sched_unregister_pending(scope, slot);

    if (nova_abool_load(&cancel_sc->cancel_requested)) return UV_ECANCELED;
    if (nova_aint_load(&s->stage) >= NN2_CLOSING)       return UV_ECANCELED;

    if (s->write_err != 0) return s->write_err;
    return s->write_n;
}

nova_int nova_net_tcp_shutdown(void* sv) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    s->shutdown_req.data = s;
    return uv_shutdown(&s->shutdown_req, (uv_stream_t*)&s->handle, NULL);
}

uint16_t nova_net_tcp_local_port(void* sv) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getsockname(&s->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

uint16_t nova_net_tcp_peer_port(void* sv) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getpeername(&s->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

void nova_net_tcp_local_addr(void* sv, NovaNetAddr* out) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    memset(&ss, 0, sizeof(ss));
    uv_tcp_getsockname(&s->handle, (struct sockaddr*)&ss, &n);
    _nn2_addr_from_ss(&ss, out);
}

void nova_net_tcp_peer_addr(void* sv, NovaNetAddr* out) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    memset(&ss, 0, sizeof(ss));
    uv_tcp_getpeername(&s->handle, (struct sockaddr*)&ss, &n);
    _nn2_addr_from_ss(&ss, out);
}

void nova_net_tcp_set_nodelay(void* sv, nova_bool on) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    uv_tcp_nodelay(&s->handle, on ? 1 : 0);
}

void nova_net_tcp_set_keepalive(void* sv, nova_bool on) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    uv_tcp_keepalive(&s->handle, on ? 1 : 0, 60);
}

void nova_net_tcp_mark_split(void* sv) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    __atomic_store_n(&s->split_refcount, 2, __ATOMIC_RELEASE);
}

void nova_net_tcp_close(void* sv) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    /* Split streams: only the last half actually closes the handle. */
    if (__atomic_load_n(&s->split_refcount, __ATOMIC_ACQUIRE) > 0) {
        int32_t left = __atomic_sub_fetch(&s->split_refcount, 1, __ATOMIC_ACQ_REL);
        if (left > 0) return;
    }
    int32_t expected = NN2_IDLE;
    if (__atomic_compare_exchange_n((volatile int32_t*)&s->stage,
            &expected, NN2_CLOSING, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        nova_loop_defer_close(s->loop, (uv_handle_t*)&s->handle,
                              _nn2_stream_close_cb);
    }
}

/* ═══════════════════════════════════════════════════════════════════════════
 * UDP
 * ═══════════════════════════════════════════════════════════════════════════ */

typedef struct NovaNet2Udp {
    uv_udp_t        handle;        /* must be first */
    uv_loop_t*      loop;
    nova_atomic_int stage;

    /* recv path */
    NovaFiberQueue* recv_scope;
    int             recv_slot;
    uint8_t*        recv_ptr;      /* caller's buffer (zero-copy) */
    nova_int        recv_cap;
    nova_int        recv_n;
    int             recv_err;
    struct sockaddr_storage recv_sender;
    int             recv_sender_valid;
    nova_atomic_int recv_done;     /* completion latch (lost-wake fix) */

    /* send path */
    uv_udp_send_t   send_req;
    NovaFiberQueue* send_scope;
    int             send_slot;
    int             send_err;
    nova_atomic_int send_done;     /* completion latch */
} NovaNet2Udp;

static void         _nn2_udp_close_cb(uv_handle_t* h);
static NovaStopMode _nn2_udp_stop_cb(void* handle);

static nova_bool _nn2_udp_recv_ready(void* ctx) {
    NovaNet2Udp* sock = (NovaNet2Udp*)ctx;
    return nova_aint_load(&sock->recv_done) != 0
        || nova_aint_load(&sock->stage) >= NN2_CLOSING;
}
static nova_bool _nn2_udp_send_ready(void* ctx) {
    NovaNet2Udp* sock = (NovaNet2Udp*)ctx;
    return nova_aint_load(&sock->send_done) != 0
        || nova_aint_load(&sock->stage) >= NN2_CLOSING;
}

void* nova_net_udp_bind(const NovaNetAddr* addr, nova_int* out_err) {
    uv_loop_t* loop = nova_current_loop();
    NovaNet2Udp* sock = (NovaNet2Udp*)nova_alloc_uncollectable(sizeof(NovaNet2Udp));
    memset(sock, 0, sizeof(*sock));
    nova_aint_init(&sock->stage, NN2_IDLE);
    sock->loop = loop;
    sock->handle.data = sock;

    int rc = uv_udp_init(loop, &sock->handle);
    if (rc != 0) { if (out_err) *out_err = rc; return NULL; }

    struct sockaddr_storage ss;
    _nn2_addr_to_ss(addr, &ss);
    rc = uv_udp_bind(&sock->handle, (const struct sockaddr*)&ss, 0);
    if (rc != 0) {
        if (out_err) *out_err = rc;
        uv_close((uv_handle_t*)&sock->handle, _nn2_udp_close_cb);
        return NULL;
    }
    if (out_err) *out_err = 0;
    return sock;
}

static void _nn2_udp_send_cb(uv_udp_send_t* req, int status) {
    NovaNet2Udp* sock = (NovaNet2Udp*)req->data;
    sock->send_err = status;
    nova_aint_store(&sock->send_done, 1);  /* latch → wake */
    NovaFiberQueue* sc = sock->send_scope; int sl = sock->send_slot;
    sock->send_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

nova_int nova_net_udp_send_to(void* sockv, const uint8_t* buf, nova_int len,
                             const NovaNetAddr* addr) {
    NovaNet2Udp* sock = (NovaNet2Udp*)sockv;
    if (len == 0) return 0;

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net2: send_to outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) return UV_ECANCELED;

    /* Zero-copy: send straight from the caller's []u8. Publish waiter + latch
     * BEFORE uv_udp_send (lost-wake-free: this exact gap timed out ~3/10). */
    uv_buf_t ubuf = uv_buf_init((char*)(uintptr_t)buf, (unsigned int)len);
    struct sockaddr_storage ss;
    _nn2_addr_to_ss(addr, &ss);
    sock->send_req.data = sock;
    sock->send_err = 0;
    nova_aint_store(&sock->send_done, 0);
    sock->send_scope = scope;
    sock->send_slot  = slot;

    int rc = uv_udp_send(&sock->send_req, &sock->handle, &ubuf, 1,
                         (const struct sockaddr*)&ss, _nn2_udp_send_cb);
    if (rc != 0) { sock->send_scope = NULL; return rc; }

    nova_sched_park_until(scope, slot, _nn2_udp_send_ready, sock);
    sock->send_scope = NULL;

    if (sock->send_err != 0) return sock->send_err;
    return len;
}

static void _nn2_udp_alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf) {
    (void)suggested;
    NovaNet2Udp* sock = (NovaNet2Udp*)h->data;
    buf->base = (char*)sock->recv_ptr;
    buf->len  = (unsigned long)sock->recv_cap;
}

static void _nn2_udp_recv_cb(uv_udp_t* handle, ssize_t nread,
                             const uv_buf_t* buf, const struct sockaddr* sender,
                             unsigned int flags) {
    (void)buf; (void)flags;
    NovaNet2Udp* sock = (NovaNet2Udp*)handle->data;
    if (nread == 0 && sender == NULL) return;  /* empty datagram / EAGAIN */
    uv_udp_recv_stop(handle);
    if (nread < 0) {
        sock->recv_n   = 0;
        sock->recv_err = (int)nread;
    } else {
        sock->recv_n   = nread;   /* data already in caller's buffer */
        sock->recv_err = 0;
        if (sender) {
            memcpy(&sock->recv_sender, sender, sizeof(struct sockaddr_storage));
            sock->recv_sender_valid = 1;
        }
    }
    nova_aint_store(&sock->recv_done, 1);  /* results published → latch → wake */
    NovaFiberQueue* sc = sock->recv_scope; int sl = sock->recv_slot;
    sock->recv_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

static NovaStopMode _nn2_udp_stop_cb(void* handle) {
    NovaNet2Udp* sock = (NovaNet2Udp*)handle;
    int32_t expected = NN2_PENDING;
    if (__atomic_compare_exchange_n((volatile int32_t*)&sock->stage,
            &expected, NN2_CLOSING, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        nova_loop_defer_close(sock->loop, (uv_handle_t*)&sock->handle,
                              _nn2_udp_close_cb);
    }
    return NOVA_STOP_ASYNC;
}

static void _nn2_udp_close_cb(uv_handle_t* h) {
    NovaNet2Udp* sock = (NovaNet2Udp*)h->data;
    nova_aint_store(&sock->stage, NN2_CLOSED);
    if (sock->recv_scope) {
        NovaFiberQueue* sc = sock->recv_scope; int sl = sock->recv_slot;
        sock->recv_scope = NULL;
        nova_sched_wake(sc, sl);
    }
}

nova_int nova_net_udp_recv_from(void* sockv, uint8_t* buf, nova_int cap,
                               NovaNetAddr* sender) {
    NovaNet2Udp* sock = (NovaNet2Udp*)sockv;
    int32_t s = nova_aint_load(&sock->stage);
    if (s >= NN2_CLOSING) return UV_ECANCELED;
    if (cap <= 0) return 0;

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net2: recv_from outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) return UV_ECANCELED;

    /* Publish waiter + buffer + latch BEFORE uv_udp_recv_start (lost-wake-free). */
    sock->recv_ptr = buf;
    sock->recv_cap = cap;
    sock->recv_n   = 0;
    sock->recv_err = 0;
    sock->recv_sender_valid = 0;
    nova_aint_store(&sock->recv_done, 0);
    nova_aint_store(&sock->stage, NN2_PENDING);
    sock->recv_scope = scope;
    sock->recv_slot  = slot;
    nova_sched_register_pending(scope, slot, sock, _nn2_udp_stop_cb);

    int rc = uv_udp_recv_start(&sock->handle, _nn2_udp_alloc_cb, _nn2_udp_recv_cb);
    if (rc != 0) {
        nova_sched_unregister_pending(scope, slot);
        sock->recv_scope = NULL;
        nova_aint_store(&sock->stage, NN2_IDLE);
        return rc;
    }

    nova_sched_park_until(scope, slot, _nn2_udp_recv_ready, sock);
    nova_sched_unregister_pending(scope, slot);

    if (nova_abool_load(&cancel_sc->cancel_requested)) return UV_ECANCELED;
    if (nova_aint_load(&sock->stage) >= NN2_CLOSING)    return UV_ECANCELED;
    nova_aint_store(&sock->stage, NN2_IDLE);

    if (sock->recv_err != 0) return sock->recv_err;
    if (sender) {
        if (sock->recv_sender_valid) _nn2_addr_from_ss(&sock->recv_sender, sender);
        else memset(sender, 0, sizeof(*sender));
    }
    return sock->recv_n;
}

uint16_t nova_net_udp_local_port(void* sockv) {
    NovaNet2Udp* sock = (NovaNet2Udp*)sockv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_udp_getsockname(&sock->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

void nova_net_udp_local_addr(void* sockv, NovaNetAddr* out) {
    NovaNet2Udp* sock = (NovaNet2Udp*)sockv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    memset(&ss, 0, sizeof(ss));
    uv_udp_getsockname(&sock->handle, (struct sockaddr*)&ss, &n);
    _nn2_addr_from_ss(&ss, out);
}

void nova_net_udp_close(void* sockv) {
    NovaNet2Udp* sock = (NovaNet2Udp*)sockv;
    int32_t expected = NN2_IDLE;
    if (__atomic_compare_exchange_n((volatile int32_t*)&sock->stage,
            &expected, NN2_CLOSING, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        nova_loop_defer_close(sock->loop, (uv_handle_t*)&sock->handle,
                              _nn2_udp_close_cb);
    }
}

/* ═══════════════════════════════════════════════════════════════════════════
 * DNS
 * ═══════════════════════════════════════════════════════════════════════════ */

typedef struct {
    uv_getaddrinfo_t req;
    NovaFiberQueue*  scope;
    int              slot;
    int              status;
    struct addrinfo* res;
    nova_atomic_int  done;   /* completion latch (lost-wake fix) */
} NovaNet2DnsReq;

static nova_bool _nn2_dns_ready(void* ctx) {
    return nova_aint_load(&((NovaNet2DnsReq*)ctx)->done) != 0;
}

static void _nn2_dns_cb(uv_getaddrinfo_t* req, int status,
                        struct addrinfo* res) {
    NovaNet2DnsReq* dr = (NovaNet2DnsReq*)req->data;
    dr->status = status;
    dr->res    = res;
    nova_aint_store(&dr->done, 1);  /* results published → latch → wake */
    NovaFiberQueue* sc = dr->scope; int sl = dr->slot;
    dr->scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

static NovaStopMode _nn2_dns_stop_cb(void* handle) {
    (void)handle;  /* uv_getaddrinfo can't be cancelled mid-flight */
    return NOVA_STOP_ASYNC;
}

/* Copy the i-th NovaNetAddr image out of a GC array into a caller []u8 image
 * (used by std/net2/dns.nv to build value SocketAddrs from the DNS result). */
void nova_net_addr_copy_at(const NovaNetAddr* arr, nova_int i, uint8_t* out) {
    memcpy(out, &arr[i], sizeof(NovaNetAddr));
}

/* DNS (D407 §5 / Ф.0-map form): ONE getaddrinfo call. libuv's callback already
 * holds the whole addrinfo list, so the C layer allocates a GC array of exactly
 * `count` value-address images and hands ownership to the caller via *out_arr
 * (the single named addrinfo→array OS-transfer, §2а) — no flat pre-guess, no
 * second lookup. Returns the count (>=1), or -1 on error (UV code → *out_err). */
nova_int nova_net_dns_lookup(const uint8_t* host, nova_int host_len, uint16_t port,
                            NovaNetAddr** out_arr, nova_int* out_err) {
    char* hostz = (char*)malloc((size_t)host_len + 1);
    if (!hostz) { if (out_err) *out_err = UV_ENOMEM; return -1; }
    memcpy(hostz, host, (size_t)host_len);
    hostz[host_len] = '\0';

    char port_str[8];
    snprintf(port_str, sizeof(port_str), "%u", (unsigned)port);

    uv_loop_t* loop = nova_current_loop();
    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { free(hostz); fprintf(stderr, "nova/net2: dns outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        free(hostz); if (out_err) *out_err = UV_ECANCELED; return -1;
    }

    NovaNet2DnsReq* dr = (NovaNet2DnsReq*)nova_alloc(sizeof(NovaNet2DnsReq));
    memset(dr, 0, sizeof(*dr));
    dr->req.data = dr;
    dr->scope    = scope;
    dr->slot     = slot;

    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family   = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    int rc = uv_getaddrinfo(loop, &dr->req, _nn2_dns_cb, hostz, port_str, &hints);
    free(hostz);
    if (rc != 0) { if (out_err) *out_err = rc; return -1; }

    nova_sched_register_pending(scope, slot, dr, _nn2_dns_stop_cb);
    nova_sched_park_until(scope, slot, _nn2_dns_ready, dr);
    nova_sched_unregister_pending(scope, slot);

    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        if (dr->res) uv_freeaddrinfo(dr->res);
        if (out_err) *out_err = UV_ECANCELED; return -1;
    }
    if (dr->status != 0) {
        if (dr->res) uv_freeaddrinfo(dr->res);
        if (out_err) *out_err = dr->status; return -1;
    }

    nova_int count = 0;
    for (struct addrinfo* ai = dr->res; ai; ai = ai->ai_next)
        if (ai->ai_family == AF_INET || ai->ai_family == AF_INET6) count++;
    if (count == 0) {
        uv_freeaddrinfo(dr->res);
        if (out_err) *out_err = UV_EAI_NONAME; return -1;
    }

    /* Named OS-transfer (D407 §2а): addrinfo list → GC array of exactly `count`
     * value images; ownership handed to the caller via *out_arr. One pass. */
    NovaNetAddr* arr =
        (NovaNetAddr*)nova_alloc(sizeof(NovaNetAddr) * (size_t)count);
    nova_int i = 0;
    for (struct addrinfo* ai = dr->res; ai; ai = ai->ai_next) {
        if (ai->ai_family != AF_INET && ai->ai_family != AF_INET6) continue;
        struct sockaddr_storage ss;
        memset(&ss, 0, sizeof(ss));
        memcpy(&ss, ai->ai_addr, ai->ai_addrlen);
        _nn2_addr_from_ss(&ss, &arr[i++]);
    }
    uv_freeaddrinfo(dr->res);

    if (out_arr) *out_arr = arr;
    if (out_err) *out_err = 0;
    return count;
}
