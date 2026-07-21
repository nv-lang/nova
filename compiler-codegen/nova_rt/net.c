/* Plan 183 Ф.1 / Plan 182 Ф.1: nova_rt/net.c — std/net C substrate.
 * (Renamed from net2.c once the legacy pre-D407 net.c/net.h was removed —
 * this file IS the std/net substrate now, not a parallel generation.)
 *
 * See net.h for the design contract (one FFI layer, byte transport, zero-copy,
 * no static result slots, M:N-safe park/wake). ALL internal helpers are
 * `static`.
 *
 * Park/wake/cancel follow the classic net.c mechanism (Plan 22 / D93) with one
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
 * silently dropped → the fiber parks forever. The legacy net.c's naive
 * single-shot `nova_sched_park` had this hole on every op. The lost-wake-free
 * pattern
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
 * on the loop's own thread. Callers (std/net tests, user code) must
 * therefore create a socket INSIDE the fiber that operates it. Lifting the
 * constraint = marshalling op issue to the owning loop thread via a defer-op
 * queue (generalisation of nova_loop_defer_close) — backlog
 * [M-183-net2-loop-affinity-cross-thread-op].
 */

#ifndef NOVA_USE_LIBUV
#  error "Plan 183: NOVA_USE_LIBUV required."
#endif

#include "net.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Canonical error strings for the two codes std/net classifies into typed
 * NetError variants (kept byte-identical to net.c so the Nova match is stable). */
#define NN2_MSG_PERMISSION_DENIED "permission denied"
#define NN2_MSG_CONNECTION_RESET  "connection reset by peer"

/* ─── Stage enum (shared by all net handle types) ─────────────────────────── */

enum {
    NN2_IDLE    = 0,
    NN2_PENDING = 1,
    NN2_CLOSING = 2,
    NN2_CLOSED  = 3,
};

/* ─── [M-183-net2-loop-affinity-cross-thread-op] fix: cross-thread uv-op
 * issue marshal ────────────────────────────────────────────────────────────
 *
 * Root cause (backlog-followups.md, filed OPEN before this fix): a fiber can
 * be moved to a different worker by work-stealing while parked waiting on a
 * socket op. Its NEXT libuv call against a handle created earlier (bound to
 * the ORIGINAL worker's loop via nova_current_loop() at create time) then
 * runs on the WRONG OS thread — concurrent, unsynchronized mutation of that
 * loop's internal bookkeeping (libuv loops are single-thread-owned; the only
 * cross-thread-safe entry point is uv_async_send). The issued op's
 * completion callback is silently mis-queued/lost — the parked fiber never
 * wakes. Measured ~1/300 on the std/tls handshake smoke
 * ([M-116-handshake-socket-deadlock]): two fibers doing back-to-back
 * write-then-read on their own socket are exactly the pattern that exposes
 * the window (a park between two ops on the SAME handle is exactly the
 * work-steal opportunity).
 *
 * Fix: every ISSUE call that touches a handle created in a PRIOR park cycle
 * (read/write/accept/udp send/recv) checks `nova_current_loop() ==
 * <handle-owning-loop>` at each call site. Fast path (same thread — the
 * overwhelming common case) calls the raw uv_* function DIRECTLY and
 * handles its return code SYNCHRONOUSLY, byte-for-byte like the pre-fix
 * code — no latch, no wake. Slow path (genuine cross-thread mismatch)
 * marshals a small "_deferred" wrapper onto the owning thread via
 * nova_loop_defer_call (mutex-queue + uv_async_send, same shape as
 * nova_loop_defer_close/Plan 83.10.2); that wrapper — and ONLY that
 * wrapper — publishes the op's completion latch and calls nova_sched_wake,
 * because in that path the call genuinely runs on a different thread than
 * the parked caller.
 *
 * Why the same-thread path must NEVER call nova_sched_wake (real regression,
 * not a theoretical concern): an earlier version of this fix routed BOTH
 * paths through one unconditional "issue-then-latch-then-wake" helper. For
 * ops with no callback of their own (uv_tcp_init+uv_accept complete
 * synchronously, no async completion) this fired nova_sched_wake on the
 * CALLING fiber's own (scope, slot) BEFORE that fiber had parked for this
 * wait — a reentrant self-wake racing the by-co gopark/goready park-state
 * machine (nova_sched.h). It surfaced as ~50% hangs/crashes in
 * std/net/split_test.nv (accept-heavy) despite the std/tls handshake smoke
 * passing 720/720 (read/write rarely hit their synchronous-failure branch,
 * so the same reentrancy window was almost never exercised there). Splitting
 * "raw direct call" from "deferred latch+wake wrapper" per call site removes
 * the reentrancy entirely: the wake-capable path only ever runs on a thread
 * OTHER than the one doing the parking. */

/* ─── Cancel-scope helper (same pattern as net.c _net_cancel_scope) ─────── */

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

NovaNetAddr* net_addr_loopback(uint16_t port) {
    NovaNetAddr* a = _nn2_alloc_addr();
    struct sockaddr_in in4;
    uv_ip4_addr("127.0.0.1", port, &in4);
    struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
    memcpy(&ss, &in4, sizeof(in4));
    _nn2_addr_from_ss(&ss, a);
    return a;
}

NovaNetAddr* net_addr_loopback_v6(uint16_t port) {
    NovaNetAddr* a = _nn2_alloc_addr();
    struct sockaddr_in6 in6;
    uv_ip6_addr("::1", port, &in6);
    struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
    memcpy(&ss, &in6, sizeof(in6));
    _nn2_addr_from_ss(&ss, a);
    return a;
}

NovaNetAddr* net_addr_v4(uint8_t a, uint8_t b, uint8_t c, uint8_t d,
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
void net_addr_loopback_into(uint16_t port, uint8_t* out) {
    NovaNetAddr* a = (NovaNetAddr*)out;
    struct sockaddr_in in4;
    uv_ip4_addr("127.0.0.1", port, &in4);
    struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
    memcpy(&ss, &in4, sizeof(in4));
    _nn2_addr_from_ss(&ss, a);
}

void net_addr_loopback_v6_into(uint16_t port, uint8_t* out) {
    NovaNetAddr* a = (NovaNetAddr*)out;
    struct sockaddr_in6 in6;
    uv_ip6_addr("::1", port, &in6);
    struct sockaddr_storage ss; memset(&ss, 0, sizeof(ss));
    memcpy(&ss, &in6, sizeof(in6));
    _nn2_addr_from_ss(&ss, a);
}

void net_addr_v4_into(uint8_t a, uint8_t b, uint8_t c, uint8_t d,
                           uint16_t port, uint8_t* out) {
    NovaNetAddr* r = (NovaNetAddr*)out;
    memset(r, 0, sizeof(*r));
    r->family = 4;
    r->port   = port;
    r->bytes[0] = a; r->bytes[1] = b; r->bytes[2] = c; r->bytes[3] = d;
}

nova_int net_addr_parse(const uint8_t* s, nova_int len, NovaNetAddr* out) {
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

uint16_t net_addr_port(const NovaNetAddr* a) { return a->port; }
nova_bool net_addr_is_v4(const NovaNetAddr* a) { return a->family == 4; }
nova_bool net_addr_is_v6(const NovaNetAddr* a) { return a->family == 6; }

nova_int net_addr_ip(const NovaNetAddr* a, uint8_t* buf, nova_int cap) {
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

nova_int net_addr_to_str(const NovaNetAddr* a, uint8_t* buf, nova_int cap) {
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

nova_int net_strerror(nova_int code, uint8_t* buf, nova_int cap) {
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
    nova_atomic_int refcount;      /* [M-boehm-...] variant (b), see below */
} NovaNet2Listener;

typedef struct NovaNet2Stream {
    uv_tcp_t        handle;        /* must be first */
    uv_loop_t*      loop;
    nova_atomic_int stage;
    nova_atomic_int refcount;      /* [M-boehm-...] variant (b), see below */

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

/* ─── [M-boehm-large-buffer-retention-fiber-reuse] variant (b): free-on-close
 * refcount protocol ───────────────────────────────────────────────────────
 *
 * Variant (a) (merged) stopped the buffer-scaling leak (read_ptr/write_req
 * cleared after each op). The RESIDUAL leak variant (b) fixes: the
 * NovaNet2Listener/Stream/Udp structs themselves are nova_alloc_uncollectable
 * (a permanent GC root — see the file-header rationale) and were NEVER
 * nova_free_uncollectable'd — a fixed-size leak per accepted/connected
 * socket. `_nn2_*_close_cb` only flipped stage=CLOSED and woke waiters.
 *
 * Naive `nova_free_uncollectable(s)` inside close_cb is a use-after-free
 * (mn-coding-conventions.md §9 class): a fiber parked in net_tcp_read/write/
 * connect/accept/udp_send_to/udp_recv_from is woken BY close_cb (the CLOSING
 * predicate) but resumes on its OWN schedule — it dereferences `s`/`lst`/
 * `sock` (stage, op_err, read_err, …) strictly AFTER close_cb has already
 * run. A concurrent net_tcp_close()/net_listener_close()/net_udp_close()
 * from a DIFFERENT fiber than the one parked reading/writing (the supported
 * "close from elsewhere to unblock a park" pattern — split TcpReadHalf/
 * TcpWriteHalf, or a supervisor closing a listener during shutdown) makes
 * this a genuine cross-fiber race, not just a same-fiber ordering question.
 *
 * Fix — classic intrusive atomic refcount (same idiom as
 * `nova_chan_writer_close` in channels.h / R1 A1 in sync.h:
 * nova_aint_fetch_sub_release + acquire-fence-before-destroy on the
 * thread that drives the count to zero):
 *   - `refcount` starts at 1 ("existence" unit — the handle/struct is alive,
 *     libuv references it, the Nova-side handle value may still be used).
 *   - Every function that may touch struct fields AFTER a park (read/write/
 *     connect/accept/udp send/recv — exactly the ops with a CLOSING-aware
 *     predicate) acquires ONE unit for its entire body (from entry to every
 *     return, via a single acquire()/goto-release()/return exit) — this
 *     covers the whole "issue → maybe-park → read result fields" window,
 *     including any cross-thread `nova_loop_defer_call` completion running
 *     concurrently while this fiber is parked.
 *   - `_nn2_*_close_cb` (runs once uv_close completes — libuv guarantees no
 *     other callback fires on the handle afterward) releases the "existence"
 *     unit AFTER doing its wake-work.
 *   - The struct is freed by whichever release drives refcount to 0 — could
 *     be an in-flight op finishing after close_cb already ran, or close_cb
 *     itself if no op is in flight when it runs. Both orders are safe: the
 *     side that observes the OTHER side's contribution already retired sees
 *     count 0 and frees exactly once (single atomic fetch_sub per release,
 *     no double-free).
 *   - `net_tcp_shutdown`'s cross-thread path is fire-and-forget (no park) —
 *     the ISSUING call cannot release after itself. It acquires before
 *     queuing the deferred job; the deferred job itself releases after
 *     calling uv_shutdown (mirrors mn-conventions §9: a pointer crossing a
 *     thread/queue boundary needs the crossing to be covered by an explicit
 *     lifetime unit, not by the issuing frame's already-returned stack).
 *   - Scope: only nova_alloc_uncollectable'd net.c objects. Never-freed DNS
 *     `NovaNet2DnsReq` is nova_alloc (regular GC memory) — untouched. */

static inline void _nn2_stream_acquire(NovaNet2Stream* s) {
    (void)nova_aint_inc(&s->refcount);
}
static inline void _nn2_stream_release(NovaNet2Stream* s) {
    if (nova_aint_fetch_sub_release(&s->refcount) == 1) {
        nova_thread_fence_acquire();
        nova_free_uncollectable(s);
    }
}
static inline void _nn2_listener_acquire(NovaNet2Listener* lst) {
    (void)nova_aint_inc(&lst->refcount);
}
static inline void _nn2_listener_release(NovaNet2Listener* lst) {
    if (nova_aint_fetch_sub_release(&lst->refcount) == 1) {
        nova_thread_fence_acquire();
        nova_free_uncollectable(lst);
    }
}

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

void* net_tcp_listen(const NovaNetAddr* addr, nova_int backlog, nova_int* out_err) {
    uv_loop_t* loop = nova_current_loop();
    NovaNet2Listener* lst =
        (NovaNet2Listener*)nova_alloc_uncollectable(sizeof(NovaNet2Listener));
    memset(lst, 0, sizeof(*lst));
    nova_aint_init(&lst->stage, NN2_IDLE);
    nova_aint_init(&lst->refcount, 1);   /* [M-boehm-...] variant (b): existence unit */
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
    /* [M-boehm-...] variant (b): libuv is done with `lst` (uv_close
     * completed — no further callback will touch it). Release the
     * "existence" unit; frees now iff no net_tcp_accept() call is
     * currently in flight (holding its own acquire). */
    _nn2_listener_release(lst);
}

/* [M-183-net2-loop-affinity-cross-thread-op] fix: the accept-issue step
 * (uv_tcp_init + uv_accept) MUST run on the loop that owns `lst` — uv_accept
 * mutates the listener's pending-accept queue. The accepted stream inherits
 * lst->loop (NOT nova_current_loop(): the accepting fiber may have been
 * work-stolen since the wait-for-pending_conns park started).
 *
 * IMPORTANT (lesson from a real regression caught by split_test.nv stress):
 * uv_tcp_init/uv_accept are SYNCHRONOUS libuv calls (no completion callback
 * of their own) — unlike read/write/connect they never had a "wake the
 * parked fiber" step in the original code. The fast (same-thread) path below
 * therefore must NOT call nova_sched_wake either: doing so unconditionally
 * (as an earlier version of this fix did) reentrantly wakes the CALLING
 * fiber's own slot BEFORE it has parked for this op, racing the by-co
 * gopark/goready park-state machine (nova_sched.h) against itself. The
 * latch+wake protocol is only correct — and only used — on the genuine
 * cross-thread path, where the job runs on a DIFFERENT thread than the
 * caller and the caller unconditionally parks waiting for it. */
static void _nn2_accept_issue_raw(NovaNet2Listener* lst, NovaNet2Stream* st,
                                   int* out_rc) {
    int rc = uv_tcp_init(lst->loop, &st->handle);
    if (rc == 0) {
        rc = uv_accept((uv_stream_t*)&lst->handle, (uv_stream_t*)&st->handle);
        if (rc != 0) {
            uv_close((uv_handle_t*)&st->handle, NULL);
        }
    }
    *out_rc = rc;
}

typedef struct {
    NovaNet2Listener* lst;
    NovaNet2Stream*   st;
    int               rc;          /* out: 0 or a UV error code */
    NovaFiberQueue*   scope;
    int               slot;
    nova_atomic_int   done;         /* completion latch (cross-thread path only) */
} Nn2AcceptIssueCtx;

/* Cross-thread-only: runs on lst->loop's owning thread via defer_call. The
 * calling fiber is guaranteed to be parked (or about to park) on `done` by
 * the time this fires, so publish-then-wake here is the standard
 * lost-wake-free pattern — safe precisely BECAUSE this executes on a
 * different thread than the parking fiber. */
static void _nn2_do_accept_issue_deferred(void* argp) {
    Nn2AcceptIssueCtx* ctx = (Nn2AcceptIssueCtx*)argp;
    _nn2_accept_issue_raw(ctx->lst, ctx->st, &ctx->rc);
    nova_aint_store(&ctx->done, 1);
    NovaFiberQueue* sc = ctx->scope; int sl = ctx->slot;
    if (sc) nova_sched_wake(sc, sl);
}
static nova_bool _nn2_accept_issue_ready(void* ctx) {
    return nova_aint_load(&((Nn2AcceptIssueCtx*)ctx)->done) != 0;
}

void* net_tcp_accept(void* lstv, nova_int* out_err) {
    NovaNet2Listener* lst = (NovaNet2Listener*)lstv;
    void* result;
    /* [M-boehm-large-buffer-retention-fiber-reuse] variant (b): op-in-flight
     * unit held for the WHOLE call (covers the park + every post-park read
     * of `lst` below) so a concurrent net_listener_close()/scope-cancel
     * cannot free `lst` out from under us. Released exactly once at every
     * exit via the `out:` label. */
    _nn2_listener_acquire(lst);

    int32_t s = nova_aint_load(&lst->stage);
    if (s >= NN2_CLOSING) { if (out_err) *out_err = UV_ECANCELED; result = NULL; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: accept outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        if (out_err) *out_err = UV_ECANCELED; result = NULL; goto out;
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
            if (out_err) *out_err = UV_ECANCELED; result = NULL; goto out;
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
            if (out_err) *out_err = UV_ECANCELED; result = NULL; goto out;
        }
        if (nova_aint_load(&lst->stage) >= NN2_CLOSING) {
            if (out_err) *out_err = UV_ECANCELED; result = NULL; goto out;
        }
    }

    NovaNet2Stream* st =
        (NovaNet2Stream*)nova_alloc_uncollectable(sizeof(NovaNet2Stream));
    memset(st, 0, sizeof(*st));
    nova_aint_init(&st->stage, NN2_IDLE);
    nova_aint_init(&st->refcount, 1);   /* [M-boehm-...] variant (b): existence unit */
    /* Accepted stream inherits the LISTENER's loop (see Nn2AcceptIssueCtx
     * comment) — not nova_current_loop(), which may now be a different
     * worker than the one lst was created/bound on. */
    st->loop = lst->loop;
    st->handle.data = st;

    int rc;
    if (nova_current_loop() == lst->loop) {
        /* Common case: same thread as the listener — direct call, exactly
         * like the pre-fix code, no latch/wake involved at all. */
        _nn2_accept_issue_raw(lst, st, &rc);
    } else {
        /* Genuine cross-thread case (work-steal moved this fiber off the
         * listener's worker between parks): marshal to lst->loop's thread
         * and park for the result. Bare park (no register_pending): this
         * window is bounded by how fast the deferred call runs — a scope
         * cancel during it is picked up by nova_sched_cancel_all_pending's
         * bare-park fallback (wakes unconditionally; the predicate still
         * requires actx.done, so a cancel-wake alone just re-parks until the
         * real completion lands). */
        Nn2AcceptIssueCtx actx;
        actx.lst = lst;
        actx.st  = st;
        actx.rc  = 0;
        actx.scope = scope;
        actx.slot  = slot;
        nova_aint_init(&actx.done, 0);

        nova_loop_defer_call(lst->loop, _nn2_do_accept_issue_deferred, &actx);
        nova_sched_park_until(scope, slot, _nn2_accept_issue_ready, &actx);
        rc = actx.rc;
    }

    if (rc != 0) { if (out_err) *out_err = rc; result = NULL; goto out; }
    if (out_err) *out_err = 0;
    result = st;

out:
    _nn2_listener_release(lst);
    return result;
}

uint16_t net_listener_local_port(void* lstv) {
    NovaNet2Listener* lst = (NovaNet2Listener*)lstv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getsockname(&lst->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

void net_listener_local_addr(void* lstv, NovaNetAddr* out) {
    NovaNet2Listener* lst = (NovaNet2Listener*)lstv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    memset(&ss, 0, sizeof(ss));
    uv_tcp_getsockname(&lst->handle, (struct sockaddr*)&ss, &n);
    _nn2_addr_from_ss(&ss, out);
}

void net_listener_set_reuse_address(void* lstv, nova_bool on) {
    (void)lstv; (void)on;  /* libuv sets SO_REUSEADDR by default at bind */
}

void net_listener_close(void* lstv) {
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
    /* [M-boehm-...] variant (b): libuv is done with `s` (uv_close completed —
     * no further read_cb/write_cb/connect_cb will fire). Release the
     * "existence" unit; frees now iff no read/write/connect call is
     * currently in flight (each holds its own acquire across park+wake). */
    _nn2_stream_release(s);
}

void* net_tcp_connect(const NovaNetAddr* addr, nova_int* out_err) {
    uv_loop_t* loop = nova_current_loop();
    NovaNet2Stream* s =
        (NovaNet2Stream*)nova_alloc_uncollectable(sizeof(NovaNet2Stream));
    memset(s, 0, sizeof(*s));
    nova_aint_init(&s->stage, NN2_IDLE);
    nova_aint_init(&s->refcount, 1);   /* [M-boehm-...] variant (b): existence unit */
    s->loop = loop;
    s->handle.data = s;
    s->connect_req.data = s;

    void* result;
    /* [M-boehm-large-buffer-retention-fiber-reuse] variant (b): op-in-flight
     * unit for the WHOLE call — a scope-cancel can invoke _nn2_stream_stop_cb
     * (registered below) concurrently on another thread once `s` is
     * registered pending; without this acquire that could drive the
     * existence unit to 0 and free `s` while we are still parked / reading
     * its fields. Released exactly once at every exit via `out:`. */
    _nn2_stream_acquire(s);

    int rc = uv_tcp_init(loop, &s->handle);
    if (rc != 0) { if (out_err) *out_err = rc; result = NULL; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: connect outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        if (out_err) *out_err = UV_ECANCELED;
        uv_close((uv_handle_t*)&s->handle, _nn2_stream_close_cb);
        result = NULL; goto out;
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
        result = NULL; goto out;
    }

    nova_sched_park_until(scope, slot, _nn2_stream_op_ready, s);
    nova_sched_unregister_pending(scope, slot);

    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        if (out_err) *out_err = UV_ECANCELED; result = NULL; goto out;
    }
    if (nova_aint_load(&s->stage) == NN2_CLOSED) {
        if (out_err) *out_err = UV_ECANCELED; result = NULL; goto out;
    }
    if (s->op_err != 0) { if (out_err) *out_err = s->op_err; result = NULL; goto out; }

    nova_aint_store(&s->stage, NN2_IDLE);
    if (out_err) *out_err = 0;
    result = s;

out:
    _nn2_stream_release(s);
    return result;
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

/* [M-183-net2-loop-affinity-cross-thread-op] fix: issue uv_read_start on the
 * handle's owning thread. Same-thread path (the overwhelming common case)
 * is a direct call with rc handled synchronously by the caller, byte-for-
 * byte like the pre-fix code — NO latch/wake here (see the accept-fix
 * comment above for why a same-thread reentrant wake is a real bug, not
 * just theoretical: it raced this exact fiber's own not-yet-parked
 * park-state and was caught by split_test.nv stress). The deferred
 * (cross-thread) wrapper below is the only place that publishes read_done +
 * wakes, and only because in that case the call runs on a different thread
 * than the parked caller. */
static void _nn2_read_start_raw(NovaNet2Stream* s, int* out_rc) {
    *out_rc = uv_read_start((uv_stream_t*)&s->handle, _nn2_read_alloc_cb, _nn2_read_cb);
}

static void _nn2_do_read_start_deferred(void* argp) {
    NovaNet2Stream* s = (NovaNet2Stream*)argp;
    int rc;
    _nn2_read_start_raw(s, &rc);
    if (rc != 0) {
        s->read_n   = 0;
        s->read_err = rc;
        nova_aint_store(&s->read_done, 1);
        NovaFiberQueue* sc = s->read_scope; int sl = s->read_slot;
        s->read_scope = NULL;
        if (sc) nova_sched_wake(sc, sl);
    }
}

nova_int net_tcp_read(void* sv, uint8_t* buf, nova_int cap) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    nova_int result;
    /* [M-boehm-large-buffer-retention-fiber-reuse] variant (b): op-in-flight
     * unit for the WHOLE call (covers the park + every post-park read of `s`
     * below) — a concurrent net_tcp_close()/scope-cancel from a DIFFERENT
     * fiber (the supported "close from elsewhere to unblock a parked read"
     * pattern) must not free `s` while we are still using it. Released
     * exactly once at every exit via `out:`. */
    _nn2_stream_acquire(s);

    int32_t st = nova_aint_load(&s->stage);
    if (st >= NN2_CLOSING) { result = UV_ECANCELED; goto out; }
    if (cap <= 0) { result = 0; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: read outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = UV_ECANCELED; goto out; }

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

    if (nova_current_loop() == s->loop) {
        /* Common case: direct call, exactly like the pre-fix code. */
        int rc;
        _nn2_read_start_raw(s, &rc);
        if (rc != 0) {
            nova_sched_unregister_pending(scope, slot);
            s->read_scope = NULL;
            result = rc; goto out;
        }
    } else {
        /* Genuine cross-thread case: marshal to s->loop's thread. */
        nova_loop_defer_call(s->loop, _nn2_do_read_start_deferred, s);
    }

    nova_sched_park_until(scope, slot, _nn2_stream_read_ready, s);
    nova_sched_unregister_pending(scope, slot);

    /* [M-boehm-large-buffer-retention-fiber-reuse]: the read is complete and
     * the bytes are already in the caller's buffer. `s` is an uncollectable
     * (permanently GC-scanned) stream that is never freed, so leaving
     * `s->read_ptr` pointing at the caller's []u8 backing ROOTS that buffer
     * forever — a real leak that grows ∝ buffer size per connection. Drop the
     * reference now that libuv is done with it (the next read re-sets it). */
    s->read_ptr = NULL;

    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = UV_ECANCELED; goto out; }
    if (nova_aint_load(&s->stage) >= NN2_CLOSING)       { result = UV_ECANCELED; goto out; }

    if (s->read_err != 0) { result = s->read_err; goto out; }
    if (s->read_eof)      { result = 0; goto out; }   /* clean EOF: 0 bytes */
    result = s->read_n;

out:
    _nn2_stream_release(s);
    return result;
}

static void _nn2_write_cb(uv_write_t* req, int status) {
    NovaNet2Stream* s = (NovaNet2Stream*)req->data;
    s->write_err = status;
    nova_aint_store(&s->write_done, 1);  /* results published → latch → wake */
    NovaFiberQueue* sc = s->write_scope; int sl = s->write_slot;
    s->write_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

/* [M-183-net2-loop-affinity-cross-thread-op] fix: issue uv_write on the
 * handle's owning thread. Same-thread path: direct call, rc handled
 * synchronously by the caller — NO latch/wake (see the read-fix comment for
 * why this matters: a same-thread reentrant wake here would race THIS
 * fiber's own not-yet-parked park-state). `ctx` for the deferred path lives
 * on the ISSUING fiber's own coroutine stack (safe: net_tcp_write does not
 * return past nova_sched_park_until until write_done is published, so the
 * deferred job — which runs strictly before that — always sees a live
 * `ctx`). */
typedef struct {
    NovaNet2Stream* s;
    uv_buf_t        ubuf;
} Nn2WriteIssueCtx;

static void _nn2_write_issue_raw(Nn2WriteIssueCtx* ctx, int* out_rc) {
    NovaNet2Stream* s = ctx->s;
    *out_rc = uv_write(&s->write_req, (uv_stream_t*)&s->handle, &ctx->ubuf, 1,
                       _nn2_write_cb);
}

static void _nn2_do_write_issue_deferred(void* argp) {
    Nn2WriteIssueCtx* ctx = (Nn2WriteIssueCtx*)argp;
    NovaNet2Stream* s = ctx->s;
    int rc;
    _nn2_write_issue_raw(ctx, &rc);
    if (rc != 0) {
        s->write_err = rc;
        nova_aint_store(&s->write_done, 1);
        NovaFiberQueue* sc = s->write_scope; int sl = s->write_slot;
        s->write_scope = NULL;
        if (sc) nova_sched_wake(sc, sl);
    }
}

nova_int net_tcp_write(void* sv, const uint8_t* buf, nova_int len) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    nova_int result;
    /* [M-boehm-large-buffer-retention-fiber-reuse] variant (b): op-in-flight
     * unit for the WHOLE call — same rationale as net_tcp_read above.
     * Released exactly once at every exit via `out:`. */
    _nn2_stream_acquire(s);

    int32_t st = nova_aint_load(&s->stage);
    if (st >= NN2_CLOSING) { result = UV_ECANCELED; goto out; }
    if (len == 0) { result = 0; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: write outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = UV_ECANCELED; goto out; }

    /* Zero-copy: uv_write points straight at the caller's []u8 memory, which the
     * Nova caller keeps alive on its fiber stack across this parked call.
     * Publish waiter + latch BEFORE uv_write (lost-wake-free); no stage
     * transition (full-duplex with a concurrent read, see read comment). */
    uv_buf_t ubuf = uv_buf_init((char*)(uintptr_t)buf, (unsigned int)len);
    Nn2WriteIssueCtx wctx_defer;  /* only populated/used on the cross-thread path */
    s->write_req.data = s;
    s->write_n   = len;
    s->write_err = 0;
    nova_aint_store(&s->write_done, 0);
    s->write_scope = scope;
    s->write_slot  = slot;
    nova_sched_register_pending(scope, slot, s, _nn2_stream_stop_cb);

    if (nova_current_loop() == s->loop) {
        /* Common case: direct call, exactly like the pre-fix code. */
        Nn2WriteIssueCtx wctx = { s, ubuf };
        int rc;
        _nn2_write_issue_raw(&wctx, &rc);
        if (rc != 0) {
            nova_sched_unregister_pending(scope, slot);
            s->write_scope = NULL;
            result = rc; goto out;
        }
    } else {
        /* Genuine cross-thread case: `wctx` is a local of THIS function call
         * (not the else-block) — its storage lives until net_tcp_write
         * returns, which only happens after the unconditional park below, so
         * the deferred job always sees a live pointer. */
        wctx_defer.s = s;
        wctx_defer.ubuf = ubuf;
        nova_loop_defer_call(s->loop, _nn2_do_write_issue_deferred, &wctx_defer);
    }

    nova_sched_park_until(scope, slot, _nn2_stream_write_ready, s);
    nova_sched_unregister_pending(scope, slot);

    /* [M-boehm-large-buffer-retention-fiber-reuse]: uv_write copied the
     * caller's buffer pointer into s->write_req.bufsml[]. The write is now
     * complete (write_cb has fired) so libuv no longer needs the req, but the
     * stale pointer would keep the caller's []u8 alive forever via the
     * uncollectable, never-freed `s`. Clear the whole req (re-inited on the
     * next write) to release it. */
    memset(&s->write_req, 0, sizeof(s->write_req));

    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = UV_ECANCELED; goto out; }
    if (nova_aint_load(&s->stage) >= NN2_CLOSING)       { result = UV_ECANCELED; goto out; }

    if (s->write_err != 0) { result = s->write_err; goto out; }
    result = s->write_n;

out:
    _nn2_stream_release(s);
    return result;
}

/* [M-183-net2-loop-affinity-cross-thread-op] fix: uv_shutdown is fire-and-
 * forget here (close_cb NULL, no park) — a cross-thread call would race the
 * owning loop's own uv_run same as read/write/accept. Unlike those, this
 * function does not park, so it cannot safely wait for the (possibly
 * deferred) issue to publish a real return code without introducing a new
 * blocking point; we accept a best-effort `0` on the cross-thread path —
 * callers of shutdown() do not depend on this rc for correctness (half-close
 * is already advisory).
 *
 * [M-boehm-large-buffer-retention-fiber-reuse] variant (b): `s` is no longer
 * "safe to touch whenever this job runs" for free — it is only alive while
 * refcount > 0. Since net_tcp_shutdown() does NOT park (its own frame
 * returns to the caller before this job necessarily runs — mn-conventions
 * §9 class: a pointer crossing a thread/queue boundary needs the crossing
 * itself covered by a lifetime unit, not by the issuing frame's stack,
 * which may already be gone), net_tcp_shutdown() acquires ONE unit before
 * queuing and THIS job releases it after touching `s` — mirroring the
 * park-based ops' acquire/release but shifted to bracket the deferred call
 * itself rather than net_tcp_shutdown()'s (already-returned) frame. */
static void _nn2_do_shutdown_issue(void* argp) {
    NovaNet2Stream* s = (NovaNet2Stream*)argp;
    uv_shutdown(&s->shutdown_req, (uv_stream_t*)&s->handle, NULL);
    _nn2_stream_release(s);
}

nova_int net_tcp_shutdown(void* sv) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    s->shutdown_req.data = s;
    if (nova_current_loop() == s->loop) {
        return uv_shutdown(&s->shutdown_req, (uv_stream_t*)&s->handle, NULL);
    }
    _nn2_stream_acquire(s);   /* released by _nn2_do_shutdown_issue itself */
    nova_loop_defer_call(s->loop, _nn2_do_shutdown_issue, s);
    return 0;
}

uint16_t net_tcp_local_port(void* sv) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getsockname(&s->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

uint16_t net_tcp_peer_port(void* sv) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getpeername(&s->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

void net_tcp_local_addr(void* sv, NovaNetAddr* out) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    memset(&ss, 0, sizeof(ss));
    uv_tcp_getsockname(&s->handle, (struct sockaddr*)&ss, &n);
    _nn2_addr_from_ss(&ss, out);
}

void net_tcp_peer_addr(void* sv, NovaNetAddr* out) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    memset(&ss, 0, sizeof(ss));
    uv_tcp_getpeername(&s->handle, (struct sockaddr*)&ss, &n);
    _nn2_addr_from_ss(&ss, out);
}

void net_tcp_set_nodelay(void* sv, nova_bool on) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    uv_tcp_nodelay(&s->handle, on ? 1 : 0);
}

void net_tcp_set_keepalive(void* sv, nova_bool on) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    uv_tcp_keepalive(&s->handle, on ? 1 : 0, 60);
}

void net_tcp_mark_split(void* sv) {
    NovaNet2Stream* s = (NovaNet2Stream*)sv;
    __atomic_store_n(&s->split_refcount, 2, __ATOMIC_RELEASE);
}

void net_tcp_close(void* sv) {
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
    nova_atomic_int refcount;      /* [M-boehm-...] variant (b), see below */

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

/* [M-boehm-...] variant (b): same refcount protocol as stream/listener above. */
static inline void _nn2_udp_acquire(NovaNet2Udp* sock) {
    (void)nova_aint_inc(&sock->refcount);
}
static inline void _nn2_udp_release(NovaNet2Udp* sock) {
    if (nova_aint_fetch_sub_release(&sock->refcount) == 1) {
        nova_thread_fence_acquire();
        nova_free_uncollectable(sock);
    }
}

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

void* net_udp_bind(const NovaNetAddr* addr, nova_int* out_err) {
    uv_loop_t* loop = nova_current_loop();
    NovaNet2Udp* sock = (NovaNet2Udp*)nova_alloc_uncollectable(sizeof(NovaNet2Udp));
    memset(sock, 0, sizeof(*sock));
    nova_aint_init(&sock->stage, NN2_IDLE);
    nova_aint_init(&sock->refcount, 1);   /* [M-boehm-...] variant (b): existence unit */
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

/* [M-183-net2-loop-affinity-cross-thread-op] fix: issue uv_udp_send on the
 * socket's owning thread. Same-thread path: direct call, rc handled
 * synchronously — NO latch/wake (see the TCP read-fix comment for why: a
 * same-thread reentrant wake races THIS fiber's own not-yet-parked
 * park-state). Deferred wrapper below is cross-thread-only. */
typedef struct {
    NovaNet2Udp*            sock;
    uv_buf_t                ubuf;
    struct sockaddr_storage ss;
} Nn2UdpSendIssueCtx;

static void _nn2_udp_send_issue_raw(Nn2UdpSendIssueCtx* ctx, int* out_rc) {
    NovaNet2Udp* sock = ctx->sock;
    *out_rc = uv_udp_send(&sock->send_req, &sock->handle, &ctx->ubuf, 1,
                          (const struct sockaddr*)&ctx->ss, _nn2_udp_send_cb);
}

static void _nn2_do_udp_send_issue_deferred(void* argp) {
    Nn2UdpSendIssueCtx* ctx = (Nn2UdpSendIssueCtx*)argp;
    NovaNet2Udp* sock = ctx->sock;
    int rc;
    _nn2_udp_send_issue_raw(ctx, &rc);
    if (rc != 0) {
        sock->send_err = rc;
        nova_aint_store(&sock->send_done, 1);
        NovaFiberQueue* sc = sock->send_scope; int sl = sock->send_slot;
        sock->send_scope = NULL;
        if (sc) nova_sched_wake(sc, sl);
    }
}

nova_int net_udp_send_to(void* sockv, const uint8_t* buf, nova_int len,
                             const NovaNetAddr* addr) {
    NovaNet2Udp* sock = (NovaNet2Udp*)sockv;
    nova_int result;
    /* [M-boehm-large-buffer-retention-fiber-reuse] variant (b): op-in-flight
     * unit for the WHOLE call — same rationale as the TCP ops above (a
     * concurrent net_udp_close() from another fiber must not free `sock`
     * while this call is parked / reading its fields). Released exactly
     * once at every exit via `out:`. */
    _nn2_udp_acquire(sock);

    if (len == 0) { result = 0; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: send_to outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = UV_ECANCELED; goto out; }

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

    Nn2UdpSendIssueCtx sctx;
    sctx.sock = sock;
    sctx.ubuf = ubuf;
    sctx.ss   = ss;

    if (nova_current_loop() == sock->loop) {
        int rc;
        _nn2_udp_send_issue_raw(&sctx, &rc);
        if (rc != 0) { sock->send_scope = NULL; result = rc; goto out; }
    } else {
        nova_loop_defer_call(sock->loop, _nn2_do_udp_send_issue_deferred, &sctx);
    }

    nova_sched_park_until(scope, slot, _nn2_udp_send_ready, sock);
    sock->send_scope = NULL;

    if (sock->send_err != 0) { result = sock->send_err; goto out; }
    result = len;

out:
    _nn2_udp_release(sock);
    return result;
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
    /* [M-boehm-...] variant (b): libuv is done with `sock`. Release the
     * "existence" unit; frees now iff no send/recv call is currently in
     * flight (each holds its own acquire across park+wake). */
    _nn2_udp_release(sock);
}

/* [M-183-net2-loop-affinity-cross-thread-op] fix: issue uv_udp_recv_start on
 * the socket's owning thread (mirrors TCP read's split above: same-thread =
 * direct call, no latch/wake; deferred wrapper is cross-thread-only). */
static void _nn2_udp_recv_start_raw(NovaNet2Udp* sock, int* out_rc) {
    *out_rc = uv_udp_recv_start(&sock->handle, _nn2_udp_alloc_cb, _nn2_udp_recv_cb);
}

static void _nn2_do_udp_recv_start_deferred(void* argp) {
    NovaNet2Udp* sock = (NovaNet2Udp*)argp;
    int rc;
    _nn2_udp_recv_start_raw(sock, &rc);
    if (rc != 0) {
        sock->recv_err = rc;
        nova_aint_store(&sock->recv_done, 1);
        NovaFiberQueue* sc = sock->recv_scope; int sl = sock->recv_slot;
        sock->recv_scope = NULL;
        if (sc) nova_sched_wake(sc, sl);
    }
}

nova_int net_udp_recv_from(void* sockv, uint8_t* buf, nova_int cap,
                               NovaNetAddr* sender) {
    NovaNet2Udp* sock = (NovaNet2Udp*)sockv;
    nova_int result;
    /* [M-boehm-large-buffer-retention-fiber-reuse] variant (b): op-in-flight
     * unit for the WHOLE call — same rationale as net_tcp_read above.
     * Released exactly once at every exit via `out:`. */
    _nn2_udp_acquire(sock);

    int32_t s = nova_aint_load(&sock->stage);
    if (s >= NN2_CLOSING) { result = UV_ECANCELED; goto out; }
    if (cap <= 0) { result = 0; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: recv_from outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nn2_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = UV_ECANCELED; goto out; }

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

    if (nova_current_loop() == sock->loop) {
        int rc;
        _nn2_udp_recv_start_raw(sock, &rc);
        if (rc != 0) {
            nova_sched_unregister_pending(scope, slot);
            sock->recv_scope = NULL;
            nova_aint_store(&sock->stage, NN2_IDLE);
            result = rc; goto out;
        }
    } else {
        nova_loop_defer_call(sock->loop, _nn2_do_udp_recv_start_deferred, sock);
    }

    nova_sched_park_until(scope, slot, _nn2_udp_recv_ready, sock);
    nova_sched_unregister_pending(scope, slot);

    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = UV_ECANCELED; goto out; }
    if (nova_aint_load(&sock->stage) >= NN2_CLOSING)    { result = UV_ECANCELED; goto out; }
    nova_aint_store(&sock->stage, NN2_IDLE);

    if (sock->recv_err != 0) { result = sock->recv_err; goto out; }
    if (sender) {
        if (sock->recv_sender_valid) _nn2_addr_from_ss(&sock->recv_sender, sender);
        else memset(sender, 0, sizeof(*sender));
    }
    result = sock->recv_n;

out:
    _nn2_udp_release(sock);
    return result;
}

uint16_t net_udp_local_port(void* sockv) {
    NovaNet2Udp* sock = (NovaNet2Udp*)sockv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_udp_getsockname(&sock->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

void net_udp_local_addr(void* sockv, NovaNetAddr* out) {
    NovaNet2Udp* sock = (NovaNet2Udp*)sockv;
    struct sockaddr_storage ss; int n = sizeof(ss);
    memset(&ss, 0, sizeof(ss));
    uv_udp_getsockname(&sock->handle, (struct sockaddr*)&ss, &n);
    _nn2_addr_from_ss(&ss, out);
}

void net_udp_close(void* sockv) {
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
 * (used by std/net/dns.nv to build value SocketAddrs from the DNS result). */
void net_addr_copy_at(const NovaNetAddr* arr, nova_int i, uint8_t* out) {
    memcpy(out, &arr[i], sizeof(NovaNetAddr));
}

/* DNS (D407 §5 / Ф.0-map form): ONE getaddrinfo call. libuv's callback already
 * holds the whole addrinfo list, so the C layer allocates a GC array of exactly
 * `count` value-address images and hands ownership to the caller via *out_arr
 * (the single named addrinfo→array OS-transfer, §2а) — no flat pre-guess, no
 * second lookup. Returns the count (>=1), or -1 on error (UV code → *out_err). */
nova_int net_dns_lookup(const uint8_t* host, nova_int host_len, uint16_t port,
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
    if (!scope) { free(hostz); fprintf(stderr, "nova/net: dns outside scope\n"); abort(); }

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
