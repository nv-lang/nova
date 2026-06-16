/* Plan 83.12: nova_rt/net.c — async TCP/UDP stdlib via libuv.
 *
 * Park/wake pattern follows Plan 22 (_nova_sleep_via_libuv):
 *   1. Allocate request state on GC-heap.
 *   2. Register stop_cb for cancel integration (D93).
 *   3. Park current fiber via nova_sched_park.
 *   4. libuv callback (on owning loop thread): store result, call
 *      nova_sched_wake.
 *   5. Fiber resumes: unregister, check cancel_requested.
 *
 * Thread-affinity (Plan 83.10.2):
 *   Handles are initialised on nova_current_loop() at creation time.
 *   Cross-thread uv_close (cancel stop_cb) routes through
 *   nova_loop_defer_close so the actual uv_close runs on the correct thread.
 *
 * Error encoding for erased Result[T, str]:
 *   Ok:  nova_make_Result_Ok((nova_int)(intptr_t)ptr)
 *   Err: nova_make_Result_Err(nova_str_from_cstr(msg))
 *
 * Helper macros:
 *   _NET_OK(ptr)       → Result wrapping pointer
 *   _NET_ERR(msg)      → Result wrapping error string
 *   _NET_ERR_UV(rc)    → Result wrapping libuv error string
 */

#ifndef NOVA_USE_LIBUV
#  error "Plan 83.12: NOVA_USE_LIBUV required."
#endif

#include "net.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ─── Helper macros ────────────────────────────────────────────────── */

static inline nova_str _nova_net_cstr(const char* s) {
    size_t n = strlen(s);
    char*  p = (char*)malloc(n + 1);
    if (!p) { fprintf(stderr, "nova/net: OOM\n"); abort(); }
    memcpy(p, s, n + 1);
    return (nova_str){ .ptr = p, .len = n };
}

static inline nova_str _nova_net_uv_err(int rc) {
    return _nova_net_cstr(uv_strerror(rc));
}

#define _NET_OK(ptr)    nova_make_Result_Ok((nova_int)(intptr_t)(ptr))
#define _NET_ERR(msg)   nova_make_Result_Err(_nova_net_cstr(msg))
#define _NET_ERR_UV(rc) nova_make_Result_Err(_nova_net_uv_err(rc))

/* ─── Park/wake helpers ────────────────────────────────────────────── */

/* Get the parent supervised scope for cancel_requested checks.
 * (Same pattern as _nova_sleep_via_libuv FIX 83.10.2.) */
static inline NovaFiberQueue* _nova_net_cancel_scope(NovaFiberQueue* scope) {
    mco_coro* rc = mco_running();
    if (rc) {
        NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(rc);
        if (base && base->_nova_parent_scope) {
            return (NovaFiberQueue*)base->_nova_parent_scope;
        }
    }
    return scope;
}

/* ─── NovaRt_SocketAddr ──────────────────────────────────────────────── */

static NovaRt_SocketAddr* _nova_alloc_addr(void) {
    NovaRt_SocketAddr* a = (NovaRt_SocketAddr*)nova_alloc(sizeof(NovaRt_SocketAddr));
    memset(a, 0, sizeof(*a));
    return a;
}

static NovaRt_SocketAddr* _nova_addr_from_storage(const struct sockaddr_storage* ss) {
    NovaRt_SocketAddr* a = _nova_alloc_addr();
    memcpy(&a->storage, ss, sizeof(*ss));
    return a;
}

NovaRt_SocketAddr* NovaRt_SocketAddr_static_loopback(uint16_t port) {
    NovaRt_SocketAddr* a = _nova_alloc_addr();
    struct sockaddr_in* in4 = (struct sockaddr_in*)&a->storage;
    uv_ip4_addr("127.0.0.1", port, in4);
    return a;
}

NovaRt_SocketAddr* NovaRt_SocketAddr_static_loopback_v6(uint16_t port) {
    NovaRt_SocketAddr* a = _nova_alloc_addr();
    struct sockaddr_in6* in6 = (struct sockaddr_in6*)&a->storage;
    uv_ip6_addr("::1", port, in6);
    return a;
}

NovaRt_SocketAddr* NovaRt_SocketAddr_static_v4(uint8_t a, uint8_t b,
                                            uint8_t c, uint8_t d,
                                            uint16_t port) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%u.%u.%u.%u", a, b, c, d);
    NovaRt_SocketAddr* addr = _nova_alloc_addr();
    struct sockaddr_in* in4 = (struct sockaddr_in*)&addr->storage;
    uv_ip4_addr(buf, port, in4);
    return addr;
}

NetAddrResult NovaRt_SocketAddr_static_parse(const char* s, NovaRt_SocketAddr* addr) {
    char* buf = (char*)alloca(strlen(s) + 1);
    strcpy(buf, s);

    char* colon = strrchr(buf, ':');
    if (!colon) return NET_ADDR_INVALID_ADDR;

    int port_n = atoi(colon + 1);
    if (port_n <= 0 || port_n > 65535) return NET_ADDR_INVALID_PORT;
    *colon = '\0';

    if (uv_ip4_addr(buf, port_n, (struct sockaddr_in*)&addr->storage) == 0)
        return NET_ADDR_OK;

    char* host = buf;
    if (host[0] == '[') {
        host++;
        char* rbrace = strchr(host, ']');
        if (rbrace) *rbrace = '\0';
    }
    if (uv_ip6_addr(host, port_n, (struct sockaddr_in6*)&addr->storage) == 0)
        return NET_ADDR_OK;

    return NET_ADDR_INVALID_ADDR;
}

static const char* _net_addr_result_msg(NetAddrResult r) {
    switch (r) {
        case NET_ADDR_INVALID_PORT: return "invalid port";
        default:                    return "invalid address";
    }
}

uint16_t NovaRt_SocketAddr_method_port(NovaRt_SocketAddr* addr) {
    int family = addr->storage.ss_family;
    if (family == AF_INET) {
        struct sockaddr_in* in4 = (struct sockaddr_in*)&addr->storage;
        return ntohs(in4->sin_port);
    } else if (family == AF_INET6) {
        struct sockaddr_in6* in6 = (struct sockaddr_in6*)&addr->storage;
        return ntohs(in6->sin6_port);
    }
    return 0;
}

static void _populate_host_cache(NovaRt_SocketAddr* addr) {
    if (addr->host_cached) return;
    int family = addr->storage.ss_family;
    if (family == AF_INET) {
        uv_ip4_name((const struct sockaddr_in*)&addr->storage,
                    addr->host_cache, sizeof(addr->host_cache));
    } else if (family == AF_INET6) {
        uv_ip6_name((const struct sockaddr_in6*)&addr->storage,
                    addr->host_cache, sizeof(addr->host_cache));
    } else {
        strncpy(addr->host_cache, "(unknown)", sizeof(addr->host_cache) - 1);
    }
    addr->host_cached = 1;
}

nova_str NovaRt_SocketAddr_method_host_str(NovaRt_SocketAddr* addr) {
    _populate_host_cache(addr);
    return _nova_net_cstr(addr->host_cache);
}

nova_bool NovaRt_SocketAddr_method_is_v4(NovaRt_SocketAddr* addr) {
    return addr->storage.ss_family == AF_INET;
}

nova_bool NovaRt_SocketAddr_method_is_v6(NovaRt_SocketAddr* addr) {
    return addr->storage.ss_family == AF_INET6;
}

nova_str NovaRt_SocketAddr_method_to_str(NovaRt_SocketAddr* addr) {
    char buf[128];
    _populate_host_cache(addr);
    uint16_t port = NovaRt_SocketAddr_method_port(addr);
    if (addr->storage.ss_family == AF_INET6) {
        snprintf(buf, sizeof(buf), "[%s]:%u", addr->host_cache, port);
    } else {
        snprintf(buf, sizeof(buf), "%s:%u", addr->host_cache, port);
    }
    return _nova_net_cstr(buf);
}

/* ─── NovaRt_TcpListener ─────────────────────────────────────────────── */

/* Forward decls. */
static void _tcp_listener_close_cb(uv_handle_t* h);
static NovaStopMode _tcp_listener_accept_stop_cb(void* handle);
static void _tcp_connection_cb(uv_stream_t* srv, int status);

static NovaRt_TcpListener* _nova_alloc_listener(void) {
    NovaRt_TcpListener* lst = (NovaRt_TcpListener*)
        nova_alloc_uncollectable(sizeof(NovaRt_TcpListener));
    memset(lst, 0, sizeof(*lst));
    nova_aint_init(&lst->stage, NOVA_NET_STAGE_IDLE);
    return lst;
}

NovaRes_nova_int_nova_str* NovaRt_TcpListener_static_bind(NovaRt_SocketAddr* addr) {
    uv_loop_t* loop = nova_current_loop();
    NovaRt_TcpListener* lst = _nova_alloc_listener();
    lst->loop = loop;
    lst->handle.data = lst;

    int rc = uv_tcp_init(loop, &lst->handle);
    if (rc != 0) return _NET_ERR_UV(rc);

    /* Allow address reuse (avoids TIME_WAIT issues in tests). */
    uv_tcp_simultaneous_accepts(&lst->handle, 1);

    rc = uv_tcp_bind(&lst->handle, (const struct sockaddr*)&addr->storage, 0);
    if (rc != 0) {
        uv_close((uv_handle_t*)&lst->handle, _tcp_listener_close_cb);
        return _NET_ERR_UV(rc);
    }

    /* Start listening (backlog = 128). */
    rc = uv_listen((uv_stream_t*)&lst->handle, 128, _tcp_connection_cb);
    if (rc != 0) {
        uv_close((uv_handle_t*)&lst->handle, _tcp_listener_close_cb);
        return _NET_ERR_UV(rc);
    }

    return _NET_OK(lst);
}

/* connection_cb: OS signalled a new connection is ready.
 * If there's a parked accept()-waiter: wake it.
 * Otherwise: increment pending_conns counter. */
static void _tcp_connection_cb(uv_stream_t* srv, int status) {
    NovaRt_TcpListener* lst = (NovaRt_TcpListener*)srv->data;
    if (status < 0) {
        /* Error from listen. If there's a waiter, wake with error. */
        if (lst->accept_scope) {
            lst->accept_result = NULL;
            lst->accept_error  = _nova_net_uv_err(status);
            NovaFiberQueue* sc = lst->accept_scope;
            int sl             = lst->accept_slot;
            lst->accept_scope  = NULL;
            nova_sched_wake(sc, sl);
        }
        return;
    }
    /* Increment pending connection count. */
    lst->pending_conns++;

    /* Wake a parked accept() caller immediately if one exists. */
    if (lst->accept_scope) {
        NovaFiberQueue* sc = lst->accept_scope;
        int sl             = lst->accept_slot;
        lst->accept_scope  = NULL;
        nova_sched_wake(sc, sl);
    }
}

/* accept_stop_cb: cancel fires while accept() is parked.
 * We close the listener handle via defer_close (thread-safe). */
static NovaStopMode _tcp_listener_accept_stop_cb(void* handle) {
    NovaRt_TcpListener* lst = (NovaRt_TcpListener*)handle;
    int32_t expected = NOVA_NET_STAGE_IDLE;
    /* CAS IDLE → CLOSING. Only winner does uv_close. */
    if (__atomic_compare_exchange_n(
            (volatile int32_t*)&lst->stage,
            &expected, NOVA_NET_STAGE_CLOSING,
            0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE))
    {
        lst->accept_error  = _nova_net_cstr("cancelled");
        lst->accept_result = NULL;
        nova_loop_defer_close(lst->loop,
                              (uv_handle_t*)&lst->handle,
                              _tcp_listener_close_cb);
    }
    return NOVA_STOP_ASYNC;
}

/* close_cb: handle fully released. Wake any parked accept()-waiter. */
static void _tcp_listener_close_cb(uv_handle_t* h) {
    NovaRt_TcpListener* lst = (NovaRt_TcpListener*)h->data;
    nova_aint_store(&lst->stage, NOVA_NET_STAGE_CLOSED);
    if (lst->accept_scope) {
        NovaFiberQueue* sc = lst->accept_scope;
        int sl             = lst->accept_slot;
        lst->accept_scope  = NULL;
        nova_sched_wake(sc, sl);
    }
}

NovaRes_nova_int_nova_str* NovaRt_TcpListener_method_accept(NovaRt_TcpListener* lst) {
    /* Check stage. */
    int32_t s = nova_aint_load(&lst->stage);
    if (s == NOVA_NET_STAGE_CLOSED) return _NET_ERR("listener closed");
    if (s == NOVA_NET_STAGE_CLOSING) return _NET_ERR("listener closing");

    NovaFiberQueue* scope = _nova_active_scope;
    int slot  = _nova_active_slot;
    if (!scope) {
        fprintf(stderr, "nova/net: TcpListener.accept outside scope\n");
        abort();
    }

    NovaFiberQueue* cancel_sc = _nova_net_cancel_scope(scope);
    /* Early-exit if already cancelled. */
    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        return _NET_ERR("cancelled");
    }

    /* Wait in a predicate-loop so we re-park if connection_cb hasn't
     * fired yet after a spurious wake. */
    for (;;) {
        if (lst->pending_conns > 0) {
            lst->pending_conns--;
            break; /* connection is ready */
        }
        if (nova_aint_load(&lst->stage) >= NOVA_NET_STAGE_CLOSING) {
            /* Listener closed, not by us — accept error already in lst. */
            if (lst->accept_error.len > 0) return _NET_ERR("listener closed");
            return _NET_ERR("listener closed");
        }

        /* Park, waiting for connection_cb or close_cb. */
        lst->accept_scope = scope;
        lst->accept_slot  = slot;
        nova_sched_register_pending(scope, slot, lst,
                                    _tcp_listener_accept_stop_cb);
        nova_sched_park(scope, slot);
        nova_sched_unregister_pending(scope, slot);

        /* Check cancel. */
        if (nova_abool_load(&cancel_sc->cancel_requested)) {
            return _NET_ERR("cancelled");
        }
        /* Check if close_cb fired (accept_result/error set). */
        if (nova_aint_load(&lst->stage) == NOVA_NET_STAGE_CLOSED) {
            return _NET_ERR("listener closed");
        }
        /* loop: re-check pending_conns */
    }

    /* We have a pending connection: accept it. */
    uv_loop_t* loop = nova_current_loop();
    NovaRt_TcpStream* st = (NovaRt_TcpStream*)
        nova_alloc_uncollectable(sizeof(NovaRt_TcpStream));
    memset(st, 0, sizeof(*st));
    nova_aint_init(&st->stage, NOVA_NET_STAGE_IDLE);
    st->loop = loop;
    st->handle.data = st;

    int rc = uv_tcp_init(loop, &st->handle);
    if (rc != 0) return _NET_ERR_UV(rc);

    rc = uv_accept((uv_stream_t*)&lst->handle, (uv_stream_t*)&st->handle);
    if (rc != 0) {
        uv_close((uv_handle_t*)&st->handle, NULL);
        return _NET_ERR_UV(rc);
    }
    return _NET_OK(st);
}

uint16_t NovaRt_TcpListener_method_local_port(NovaRt_TcpListener* lst) {
    struct sockaddr_storage ss;
    int namelen = sizeof(ss);
    if (uv_tcp_getsockname(&lst->handle, (struct sockaddr*)&ss, &namelen) != 0)
        return 0;
    if (ss.ss_family == AF_INET)
        return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6)
        return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

NovaRt_SocketAddr* NovaRt_TcpListener_method_local_addr(NovaRt_TcpListener* lst) {
    NovaRt_SocketAddr* a = _nova_alloc_addr();
    int namelen = sizeof(a->storage);
    uv_tcp_getsockname(&lst->handle, (struct sockaddr*)&a->storage, &namelen);
    return a;
}

nova_unit NovaRt_TcpListener_method_close(NovaRt_TcpListener* lst) {
    int32_t expected = NOVA_NET_STAGE_IDLE;
    if (__atomic_compare_exchange_n(
            (volatile int32_t*)&lst->stage,
            &expected, NOVA_NET_STAGE_CLOSING,
            0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE))
    {
        nova_loop_defer_close(lst->loop,
                              (uv_handle_t*)&lst->handle,
                              _tcp_listener_close_cb);
    }
    return NOVA_UNIT;
}

/* ─── NovaRt_TcpStream ───────────────────────────────────────────────── */

/* Forward decls. */
static void _tcp_stream_close_cb(uv_handle_t* h);
static NovaStopMode _tcp_stream_op_stop_cb(void* handle);
static void _tcp_connect_cb(uv_connect_t* req, int status);
static void _tcp_read_cb(uv_stream_t* stream, ssize_t nread, const uv_buf_t* buf);
static void _tcp_alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf);
static void _tcp_write_cb(uv_write_t* req, int status);

static NovaRt_TcpStream* _nova_alloc_stream(void) {
    NovaRt_TcpStream* s = (NovaRt_TcpStream*)
        nova_alloc_uncollectable(sizeof(NovaRt_TcpStream));
    memset(s, 0, sizeof(*s));
    nova_aint_init(&s->stage, NOVA_NET_STAGE_IDLE);
    return s;
}

NovaRes_nova_int_nova_str* NovaRt_TcpStream_static_connect(NovaRt_SocketAddr* addr) {
    uv_loop_t* loop = nova_current_loop();
    NovaRt_TcpStream* s = _nova_alloc_stream();
    s->loop = loop;
    s->handle.data = s;
    s->connect_req.data = s;

    int rc = uv_tcp_init(loop, &s->handle);
    if (rc != 0) return _NET_ERR_UV(rc);

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: connect outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nova_net_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        uv_close((uv_handle_t*)&s->handle, _tcp_stream_close_cb);
        return _NET_ERR("cancelled");
    }

    rc = uv_tcp_connect(&s->connect_req, &s->handle,
                        (const struct sockaddr*)&addr->storage,
                        _tcp_connect_cb);
    if (rc != 0) {
        uv_close((uv_handle_t*)&s->handle, _tcp_stream_close_cb);
        return _NET_ERR_UV(rc);
    }

    /* Park until connect_cb fires. */
    nova_aint_store(&s->stage, NOVA_NET_STAGE_PENDING);
    s->op_scope = scope;
    s->op_slot  = slot;
    nova_sched_register_pending(scope, slot, s, _tcp_stream_op_stop_cb);
    nova_sched_park(scope, slot);
    nova_sched_unregister_pending(scope, slot);

    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        return _NET_ERR("cancelled");
    }
    if (s->op_error.len > 0) return _NET_ERR(s->op_error.ptr);

    nova_aint_store(&s->stage, NOVA_NET_STAGE_IDLE);
    return _NET_OK(s);
}

static void _tcp_connect_cb(uv_connect_t* req, int status) {
    NovaRt_TcpStream* s = (NovaRt_TcpStream*)req->data;
    if (status < 0) {
        s->op_error = _nova_net_uv_err(status);
    } else {
        /* Zero out error on success. */
        s->op_error = (nova_str){ .ptr = NULL, .len = 0 };
    }
    NovaFiberQueue* sc = s->op_scope;
    int sl = s->op_slot;
    s->op_scope = NULL;
    nova_sched_wake(sc, sl);
}

static NovaStopMode _tcp_stream_op_stop_cb(void* handle) {
    NovaRt_TcpStream* s = (NovaRt_TcpStream*)handle;
    int32_t expected = NOVA_NET_STAGE_PENDING;
    if (__atomic_compare_exchange_n(
            (volatile int32_t*)&s->stage,
            &expected, NOVA_NET_STAGE_CLOSING,
            0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE))
    {
        s->op_error = _nova_net_cstr("cancelled");
        /* Stop active read if any. */
        /* uv_read_stop and uv_close must run on loop thread. */
        nova_loop_defer_close(s->loop,
                              (uv_handle_t*)&s->handle,
                              _tcp_stream_close_cb);
    }
    return NOVA_STOP_ASYNC;
}

static void _tcp_stream_close_cb(uv_handle_t* h) {
    NovaRt_TcpStream* s = (NovaRt_TcpStream*)h->data;
    nova_aint_store(&s->stage, NOVA_NET_STAGE_CLOSED);
    if (s->op_scope) {
        NovaFiberQueue* sc = s->op_scope;
        int sl = s->op_slot;
        s->op_scope = NULL;
        nova_sched_wake(sc, sl);
    }
}

/* alloc_cb: libuv asks us for a buffer to read into.
 * We allocate a heap buffer; read_cb frees it (or we own it). */
static void _tcp_alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf) {
    NovaRt_TcpStream* s = (NovaRt_TcpStream*)h->data;
    size_t cap = s->read_max > 0 ? (size_t)s->read_max : suggested;
    if (cap > 65536) cap = 65536;  /* sanity cap */
    char* mem = (char*)malloc(cap);
    if (!mem) { buf->base = NULL; buf->len = 0; return; }
    s->read_buf = mem;
    buf->base = mem;
    buf->len  = cap;
}

/* read_cb: data arrived or EOF/error. Stop reading, store result, wake. */
static void _tcp_read_cb(uv_stream_t* stream, ssize_t nread,
                          const uv_buf_t* buf_unused) {
    (void)buf_unused;
    NovaRt_TcpStream* s = (NovaRt_TcpStream*)stream->data;
    /* Stop reading — we do one-shot reads. */
    uv_read_stop(stream);

    if (nread == UV_EOF) {
        s->read_len = 0;
        s->is_eof   = 1;
        s->op_error = (nova_str){ .ptr = NULL, .len = 0 };
    } else if (nread < 0) {
        s->read_len = 0;
        s->op_error = _nova_net_uv_err((int)nread);
    } else {
        s->read_len = nread;
        s->op_error = (nova_str){ .ptr = NULL, .len = 0 };
    }

    NovaFiberQueue* sc = s->op_scope;
    int sl = s->op_slot;
    s->op_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

NovaRes_nova_int_nova_str* NovaRt_TcpStream_method_read_bytes(
        NovaRt_TcpStream* s, nova_int max_bytes) {
    int32_t st = nova_aint_load(&s->stage);
    if (st == NOVA_NET_STAGE_CLOSED) return _NET_ERR("stream closed");
    if (st == NOVA_NET_STAGE_CLOSING) return _NET_ERR("stream closing");

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: read_bytes outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nova_net_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) return _NET_ERR("cancelled");

    s->read_max = (int)(max_bytes > 0 ? max_bytes : 4096);
    s->read_len = 0;
    s->is_eof   = 0;
    if (s->read_buf) { free(s->read_buf); s->read_buf = NULL; }
    s->op_error = (nova_str){ .ptr = NULL, .len = 0 };

    int rc = uv_read_start((uv_stream_t*)&s->handle, _tcp_alloc_cb, _tcp_read_cb);
    if (rc != 0) return _NET_ERR_UV(rc);

    nova_aint_store(&s->stage, NOVA_NET_STAGE_PENDING);
    s->op_scope = scope;
    s->op_slot  = slot;
    nova_sched_register_pending(scope, slot, s, _tcp_stream_op_stop_cb);
    nova_sched_park(scope, slot);
    nova_sched_unregister_pending(scope, slot);

    /* After wake: check cancel and error. */
    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        if (s->read_buf) { free(s->read_buf); s->read_buf = NULL; }
        return _NET_ERR("cancelled");
    }
    if (nova_aint_load(&s->stage) == NOVA_NET_STAGE_CLOSED) {
        if (s->read_buf) { free(s->read_buf); s->read_buf = NULL; }
        return _NET_ERR("stream closed");
    }
    nova_aint_store(&s->stage, NOVA_NET_STAGE_IDLE);

    if (s->op_error.len > 0) {
        if (s->read_buf) { free(s->read_buf); s->read_buf = NULL; }
        return nova_make_Result_Err(s->op_error);
    }

    /* Build nova_str from read_buf (we own this memory; use it directly). */
    if (s->read_len == 0) {
        /* EOF: return Ok(""). */
        if (s->read_buf) { free(s->read_buf); s->read_buf = NULL; }
        return nova_make_Result_Ok((nova_int)(intptr_t)(nova_alloc(sizeof(nova_str))));
        /* Actually we want to return Ok("") as a nova_str ... */
        /* But Ok wraps nova_int. The str value needs to be returned as pointer. */
        /* For empty string, return Ok(0) — caller checks r.is_ok() == true. */
        /* In C, Ok(0) is a valid empty-string sentinel. */
    }
    /* Copy data into GC-managed string. */
    char* heap = (char*)nova_alloc(s->read_len + 1);
    memcpy(heap, s->read_buf, s->read_len);
    heap[s->read_len] = '\0';
    free(s->read_buf);
    s->read_buf = NULL;

    /* Pack nova_str into nova_int for Ok payload.
     * The str struct is: { ptr: char*, len: size_t }.
     * We need to return a nova_str as the Ok value.
     * Since Ok(nova_int) is the only slot, we heap-allocate a nova_str
     * and return its pointer as nova_int. */
    nova_str* res_str = (nova_str*)nova_alloc(sizeof(nova_str));
    res_str->ptr = heap;
    res_str->len = s->read_len;
    return nova_make_Result_Ok((nova_int)(intptr_t)res_str);
}

static void _tcp_write_cb(uv_write_t* req, int status) {
    NovaRt_TcpStream* s = (NovaRt_TcpStream*)req->data;
    if (status < 0) {
        s->op_error  = _nova_net_uv_err(status);
        s->write_len = 0;
    } else {
        s->op_error  = (nova_str){ .ptr = NULL, .len = 0 };
        /* write_len set before park. */
    }
    NovaFiberQueue* sc = s->op_scope;
    int sl = s->op_slot;
    s->op_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

NovaRes_nova_int_nova_str* NovaRt_TcpStream_method_write(
        NovaRt_TcpStream* s, nova_str data) {
    int32_t st = nova_aint_load(&s->stage);
    if (st == NOVA_NET_STAGE_CLOSED) return _NET_ERR("stream closed");
    if (st == NOVA_NET_STAGE_CLOSING) return _NET_ERR("stream closing");

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: write outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nova_net_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) return _NET_ERR("cancelled");

    if (data.len == 0) return nova_make_Result_Ok(0);

    /* Copy data: libuv keeps a reference until write_cb. */
    if (s->write_buf) { free(s->write_buf); s->write_buf = NULL; }
    s->write_buf = (char*)malloc(data.len);
    if (!s->write_buf) return _NET_ERR("OOM");
    memcpy(s->write_buf, data.ptr, data.len);
    s->write_len = (ssize_t)data.len;

    uv_buf_t ubuf = uv_buf_init(s->write_buf, (unsigned int)data.len);
    s->write_req.data = s;
    s->op_error = (nova_str){ .ptr = NULL, .len = 0 };

    int rc = uv_write(&s->write_req, (uv_stream_t*)&s->handle, &ubuf, 1,
                      _tcp_write_cb);
    if (rc != 0) {
        free(s->write_buf); s->write_buf = NULL;
        return _NET_ERR_UV(rc);
    }

    nova_aint_store(&s->stage, NOVA_NET_STAGE_PENDING);
    s->op_scope = scope;
    s->op_slot  = slot;
    nova_sched_register_pending(scope, slot, s, _tcp_stream_op_stop_cb);
    nova_sched_park(scope, slot);
    nova_sched_unregister_pending(scope, slot);

    if (s->write_buf) { free(s->write_buf); s->write_buf = NULL; }

    if (nova_abool_load(&cancel_sc->cancel_requested)) return _NET_ERR("cancelled");
    if (nova_aint_load(&s->stage) == NOVA_NET_STAGE_CLOSED)
        return _NET_ERR("stream closed");
    nova_aint_store(&s->stage, NOVA_NET_STAGE_IDLE);
    if (s->op_error.len > 0) return nova_make_Result_Err(s->op_error);
    return nova_make_Result_Ok((nova_int)s->write_len);
}

uint16_t NovaRt_TcpStream_method_local_port(NovaRt_TcpStream* s) {
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getsockname(&s->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

uint16_t NovaRt_TcpStream_method_peer_port(NovaRt_TcpStream* s) {
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getpeername(&s->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

NovaRt_SocketAddr* NovaRt_TcpStream_method_local_addr(NovaRt_TcpStream* s) {
    NovaRt_SocketAddr* a = _nova_alloc_addr();
    int n = sizeof(a->storage);
    uv_tcp_getsockname(&s->handle, (struct sockaddr*)&a->storage, &n);
    return a;
}

NovaRt_SocketAddr* NovaRt_TcpStream_method_peer_addr(NovaRt_TcpStream* s) {
    NovaRt_SocketAddr* a = _nova_alloc_addr();
    int n = sizeof(a->storage);
    uv_tcp_getpeername(&s->handle, (struct sockaddr*)&a->storage, &n);
    return a;
}

nova_unit NovaRt_TcpStream_method_close(NovaRt_TcpStream* s) {
    int32_t expected = NOVA_NET_STAGE_IDLE;
    if (__atomic_compare_exchange_n(
            (volatile int32_t*)&s->stage,
            &expected, NOVA_NET_STAGE_CLOSING,
            0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE))
    {
        nova_loop_defer_close(s->loop,
                              (uv_handle_t*)&s->handle,
                              _tcp_stream_close_cb);
    }
    return NOVA_UNIT;
}

/* ─── NovaRt_UdpSocket ───────────────────────────────────────────────── */

/* Forward decls. */
static void _udp_close_cb(uv_handle_t* h);
static NovaStopMode _udp_recv_stop_cb(void* handle);
static void _udp_alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf);
static void _udp_recv_cb(uv_udp_t* handle, ssize_t nread,
                          const uv_buf_t* buf,
                          const struct sockaddr* sender,
                          unsigned int flags);
static void _udp_send_cb(uv_udp_send_t* req, int status);

typedef struct { NovaRt_UdpSocket* sock; uv_udp_send_t req; char* buf; } _NovaUdpSendCtx;

NovaRes_nova_int_nova_str* NovaRt_UdpSocket_static_bind(NovaRt_SocketAddr* addr) {
    uv_loop_t* loop = nova_current_loop();
    NovaRt_UdpSocket* sock = (NovaRt_UdpSocket*)
        nova_alloc_uncollectable(sizeof(NovaRt_UdpSocket));
    memset(sock, 0, sizeof(*sock));
    nova_aint_init(&sock->stage, NOVA_NET_STAGE_IDLE);
    sock->loop = loop;
    sock->handle.data = sock;

    int rc = uv_udp_init(loop, &sock->handle);
    if (rc != 0) return _NET_ERR_UV(rc);

    rc = uv_udp_bind(&sock->handle,
                     (const struct sockaddr*)&addr->storage, 0);
    if (rc != 0) {
        uv_close((uv_handle_t*)&sock->handle, _udp_close_cb);
        return _NET_ERR_UV(rc);
    }
    return _NET_OK(sock);
}

NovaRes_nova_int_nova_str* NovaRt_UdpSocket_method_send_to(
        NovaRt_UdpSocket* sock, nova_str data, NovaRt_SocketAddr* addr) {
    if (data.len == 0) return nova_make_Result_Ok(0);

    /* We do a synchronous uv_udp_send + park pattern. */
    _NovaUdpSendCtx* ctx = (_NovaUdpSendCtx*)nova_alloc(sizeof(_NovaUdpSendCtx));
    ctx->sock = sock;
    ctx->buf  = (char*)malloc(data.len);
    if (!ctx->buf) return _NET_ERR("OOM");
    memcpy(ctx->buf, data.ptr, data.len);
    ctx->req.data = ctx;

    uv_buf_t ubuf = uv_buf_init(ctx->buf, (unsigned int)data.len);
    int rc = uv_udp_send(&ctx->req, &sock->handle, &ubuf, 1,
                         (const struct sockaddr*)&addr->storage,
                         _udp_send_cb);
    if (rc != 0) { free(ctx->buf); return _NET_ERR_UV(rc); }

    /* For send_to we use a simple park-and-wake. */
    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: send_to outside scope\n"); abort(); }

    sock->recv_scope = scope;
    sock->recv_slot  = slot;
    nova_sched_park(scope, slot);
    sock->recv_scope = NULL;

    nova_str err = sock->recv_error;
    sock->recv_error = (nova_str){ .ptr = NULL, .len = 0 };
    if (err.len > 0) { free(ctx->buf); return nova_make_Result_Err(err); }
    free(ctx->buf);
    return nova_make_Result_Ok((nova_int)data.len);
}

/* send_cb: wake parked send_to caller. */
static void _udp_send_cb(uv_udp_send_t* req, int status) {
    _NovaUdpSendCtx* ctx = (_NovaUdpSendCtx*)req->data;
    NovaRt_UdpSocket* sock = ctx->sock;
    if (status < 0) {
        sock->recv_error = _nova_net_uv_err(status);
    } else {
        sock->recv_error = (nova_str){ .ptr = NULL, .len = 0 };
    }
    NovaFiberQueue* sc = sock->recv_scope;
    int sl = sock->recv_slot;
    if (sc) { sock->recv_scope = NULL; nova_sched_wake(sc, sl); }
}

static void _udp_alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf) {
    NovaRt_UdpSocket* sock = (NovaRt_UdpSocket*)h->data;
    size_t cap = sock->recv_max > 0 ? (size_t)sock->recv_max : suggested;
    if (cap > 65536) cap = 65536;
    char* mem = (char*)malloc(cap);
    if (!mem) { buf->base = NULL; buf->len = 0; return; }
    sock->recv_buf = mem;
    buf->base = mem;
    buf->len  = cap;
}

static void _udp_recv_cb(uv_udp_t* handle, ssize_t nread,
                          const uv_buf_t* buf_unused,
                          const struct sockaddr* sender,
                          unsigned int flags) {
    (void)buf_unused; (void)flags;
    NovaRt_UdpSocket* sock = (NovaRt_UdpSocket*)handle->data;
    /* Stop receiving — one-shot. */
    uv_udp_recv_stop(handle);

    if (nread < 0) {
        sock->recv_len   = 0;
        sock->recv_error = _nova_net_uv_err((int)nread);
    } else {
        sock->recv_len   = nread;
        sock->recv_error = (nova_str){ .ptr = NULL, .len = 0 };
        if (sender) {
            memcpy(&sock->last_sender_storage, sender, sizeof(struct sockaddr_storage));
            sock->last_sender_valid = 1;
        }
    }
    NovaFiberQueue* sc = sock->recv_scope;
    int sl = sock->recv_slot;
    sock->recv_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

static NovaStopMode _udp_recv_stop_cb(void* handle) {
    NovaRt_UdpSocket* sock = (NovaRt_UdpSocket*)handle;
    int32_t expected = NOVA_NET_STAGE_PENDING;
    if (__atomic_compare_exchange_n(
            (volatile int32_t*)&sock->stage,
            &expected, NOVA_NET_STAGE_CLOSING,
            0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE))
    {
        sock->recv_error = _nova_net_cstr("cancelled");
        nova_loop_defer_close(sock->loop,
                              (uv_handle_t*)&sock->handle,
                              _udp_close_cb);
    }
    return NOVA_STOP_ASYNC;
}

static void _udp_close_cb(uv_handle_t* h) {
    NovaRt_UdpSocket* sock = (NovaRt_UdpSocket*)h->data;
    nova_aint_store(&sock->stage, NOVA_NET_STAGE_CLOSED);
    if (sock->recv_scope) {
        NovaFiberQueue* sc = sock->recv_scope;
        int sl = sock->recv_slot;
        sock->recv_scope = NULL;
        nova_sched_wake(sc, sl);
    }
}

NovaRes_nova_int_nova_str* NovaRt_UdpSocket_method_recv_from(
        NovaRt_UdpSocket* sock, nova_int max_bytes) {
    int32_t s = nova_aint_load(&sock->stage);
    if (s == NOVA_NET_STAGE_CLOSED)  return _NET_ERR("socket closed");
    if (s == NOVA_NET_STAGE_CLOSING) return _NET_ERR("socket closing");

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/net: recv_from outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _nova_net_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) return _NET_ERR("cancelled");

    sock->recv_max = (int)(max_bytes > 0 ? max_bytes : 65536);
    sock->recv_len = 0;
    if (sock->recv_buf) { free(sock->recv_buf); sock->recv_buf = NULL; }
    sock->recv_error = (nova_str){ .ptr = NULL, .len = 0 };

    int rc = uv_udp_recv_start(&sock->handle, _udp_alloc_cb, _udp_recv_cb);
    if (rc != 0) return _NET_ERR_UV(rc);

    nova_aint_store(&sock->stage, NOVA_NET_STAGE_PENDING);
    sock->recv_scope = scope;
    sock->recv_slot  = slot;
    nova_sched_register_pending(scope, slot, sock, _udp_recv_stop_cb);
    nova_sched_park(scope, slot);
    nova_sched_unregister_pending(scope, slot);

    if (sock->recv_buf) { free(sock->recv_buf); sock->recv_buf = NULL; }

    if (nova_abool_load(&cancel_sc->cancel_requested)) return _NET_ERR("cancelled");
    if (nova_aint_load(&sock->stage) == NOVA_NET_STAGE_CLOSED)
        return _NET_ERR("socket closed");
    nova_aint_store(&sock->stage, NOVA_NET_STAGE_IDLE);
    if (sock->recv_error.len > 0) return nova_make_Result_Err(sock->recv_error);

    /* Build result string. */
    char* heap = (char*)nova_alloc(sock->recv_len + 1);
    if (sock->recv_buf) memcpy(heap, sock->recv_buf, sock->recv_len);
    heap[sock->recv_len] = '\0';
    if (sock->recv_buf) { free(sock->recv_buf); sock->recv_buf = NULL; }
    nova_str* res = (nova_str*)nova_alloc(sizeof(nova_str));
    res->ptr = heap;
    res->len = sock->recv_len;
    return nova_make_Result_Ok((nova_int)(intptr_t)res);
}

NovaRt_SocketAddr* NovaRt_UdpSocket_method_last_sender(NovaRt_UdpSocket* sock) {
    if (!sock->last_sender_valid) {
        return NovaRt_SocketAddr_static_loopback(0);
    }
    NovaRt_SocketAddr* a = _nova_alloc_addr();
    memcpy(&a->storage, &sock->last_sender_storage, sizeof(struct sockaddr_storage));
    return a;
}

uint16_t NovaRt_UdpSocket_method_local_port(NovaRt_UdpSocket* sock) {
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_udp_getsockname(&sock->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

NovaRt_SocketAddr* NovaRt_UdpSocket_method_local_addr(NovaRt_UdpSocket* sock) {
    NovaRt_SocketAddr* a = _nova_alloc_addr();
    int n = sizeof(a->storage);
    uv_udp_getsockname(&sock->handle, (struct sockaddr*)&a->storage, &n);
    return a;
}

nova_unit NovaRt_UdpSocket_method_close(NovaRt_UdpSocket* sock) {
    int32_t expected = NOVA_NET_STAGE_IDLE;
    if (__atomic_compare_exchange_n(
            (volatile int32_t*)&sock->stage,
            &expected, NOVA_NET_STAGE_CLOSING,
            0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE))
    {
        nova_loop_defer_close(sock->loop,
                              (uv_handle_t*)&sock->handle,
                              _udp_close_cb);
    }
    return NOVA_UNIT;
}

/* ═══════════════════════════════════════════════════════════════════════════
 * Plan 91.12 Ф.0: literal-name entry-points for Nova `extern "C" fn`.
 *
 * Handle ABI: all NovaRt_*  pointers are passed and returned as nova_int
 * (= intptr_t). Constructors return (nova_int)ptr or -1 on error.
 * Error message: call net_last_error() after any -1 return.
 *
 * udp_socket_recv_from() stores result in thread-local buffers; read via
 * udp_socket_recv_data() / udp_socket_recv_sender() immediately after
 * (cooperative fibers guarantee no intervening writes to TLS).
 * ═══════════════════════════════════════════════════════════════════════════ */

/* ─── Thread-local last error ──────────────────────────────────────────── */

#if defined(_MSC_VER)
  static __declspec(thread) char _net_tls_last_error[4096];
#else
  static __thread char _net_tls_last_error[4096];
#endif

static void _net_store_err(nova_str s) {
    size_t n = (size_t)s.len < sizeof(_net_tls_last_error) - 1
               ? (size_t)s.len : sizeof(_net_tls_last_error) - 1;
    memcpy(_net_tls_last_error, s.ptr, n);
    _net_tls_last_error[n] = '\0';
}

nova_str net_last_error(void) {
    return _nova_net_cstr(_net_tls_last_error);
}

/* ─── SocketAddr ───────────────────────────────────────────────────────── */

NovaRt_SocketAddr* socket_addr_loopback(uint16_t port) {
    return NovaRt_SocketAddr_static_loopback(port);
}
NovaRt_SocketAddr* socket_addr_loopback_v6(uint16_t port) {
    return NovaRt_SocketAddr_static_loopback_v6(port);
}
NovaRt_SocketAddr* socket_addr_v4(uint8_t a, uint8_t b, uint8_t c, uint8_t d, uint16_t port) {
    return NovaRt_SocketAddr_static_v4(a, b, c, d, port);
}
_NovaTuple2 socket_addr_parse(nova_str s) {
    char* buf = (char*)alloca(s.len + 1);
    memcpy(buf, s.ptr, s.len);
    buf[s.len] = '\0';

    _NovaTuple2 r;
    NovaRt_SocketAddr* addr = _nova_alloc_addr();
    NetAddrResult code = NovaRt_SocketAddr_static_parse(buf, addr);
    r.f0 = (nova_int)code;
    r.f1 = (nova_int)(intptr_t)((code == NET_ADDR_OK) ? addr : NULL);
    return r;
}
uint16_t socket_addr_port(NovaRt_SocketAddr* addr) {
    return NovaRt_SocketAddr_method_port(addr);
}
nova_str socket_addr_host_str(NovaRt_SocketAddr* addr) {
    return NovaRt_SocketAddr_method_host_str(addr);
}
nova_bool socket_addr_is_v4(NovaRt_SocketAddr* addr) {
    return NovaRt_SocketAddr_method_is_v4(addr);
}
nova_bool socket_addr_is_v6(NovaRt_SocketAddr* addr) {
    return NovaRt_SocketAddr_method_is_v6(addr);
}
nova_str socket_addr_to_str(NovaRt_SocketAddr* addr) {
    return NovaRt_SocketAddr_method_to_str(addr);
}

/* ─── TcpListener ──────────────────────────────────────────────────────── */

NovaRt_TcpListener* tcp_listener_bind(NovaRt_SocketAddr* addr) {
    NovaRes_nova_int_nova_str* r = NovaRt_TcpListener_static_bind(addr);
    if (r->tag == NOVA_TAG_Result_Ok) return (NovaRt_TcpListener*)(intptr_t)r->payload.Ok._0;
    _net_store_err(r->payload.Err._0);
    return NULL;
}
NovaRt_TcpStream* tcp_listener_accept(NovaRt_TcpListener* lst) {
    NovaRes_nova_int_nova_str* r = NovaRt_TcpListener_method_accept(lst);
    if (r->tag == NOVA_TAG_Result_Ok) return (NovaRt_TcpStream*)(intptr_t)r->payload.Ok._0;
    _net_store_err(r->payload.Err._0);
    return NULL;
}
uint16_t tcp_listener_local_port(NovaRt_TcpListener* lst) {
    return NovaRt_TcpListener_method_local_port(lst);
}
NovaRt_SocketAddr* tcp_listener_local_addr(NovaRt_TcpListener* lst) {
    return NovaRt_TcpListener_method_local_addr(lst);
}
nova_unit tcp_listener_close(NovaRt_TcpListener* lst) {
    NovaRt_TcpListener_method_close(lst);
    return NOVA_UNIT;
}

/* ─── TcpStream ────────────────────────────────────────────────────────── */

/* TLS buffer for tcp_stream_read_bytes result.
 * Safe: Nova fibers are cooperative — no other fiber runs between
 * tcp_stream_read_bytes() return and the tcp_stream_read_data() read. */
#if defined(_MSC_VER)
  static __declspec(thread) nova_str _net_tcp_read_data;
#else
  static __thread nova_str _net_tcp_read_data;
#endif

NovaRt_TcpStream* tcp_stream_connect(NovaRt_SocketAddr* addr) {
    NovaRes_nova_int_nova_str* r = NovaRt_TcpStream_static_connect(addr);
    if (r->tag == NOVA_TAG_Result_Ok) return (NovaRt_TcpStream*)(intptr_t)r->payload.Ok._0;
    _net_store_err(r->payload.Err._0);
    return NULL;
}
nova_int tcp_stream_write(NovaRt_TcpStream* s, nova_str data) {
    NovaRes_nova_int_nova_str* r = NovaRt_TcpStream_method_write(s, data);
    if (r->tag == NOVA_TAG_Result_Ok) return r->payload.Ok._0;
    _net_store_err(r->payload.Err._0);
    return -1;
}
uint16_t tcp_stream_local_port(NovaRt_TcpStream* s) {
    return NovaRt_TcpStream_method_local_port(s);
}
uint16_t tcp_stream_peer_port(NovaRt_TcpStream* s) {
    return NovaRt_TcpStream_method_peer_port(s);
}
NovaRt_SocketAddr* tcp_stream_local_addr(NovaRt_TcpStream* s) {
    return NovaRt_TcpStream_method_local_addr(s);
}
NovaRt_SocketAddr* tcp_stream_peer_addr(NovaRt_TcpStream* s) {
    return NovaRt_TcpStream_method_peer_addr(s);
}
nova_unit tcp_stream_close(NovaRt_TcpStream* s) {
    NovaRt_TcpStream_method_close(s);
    return NOVA_UNIT;
}
/* Read up to max_bytes from stream. Returns bytes read (0 = EOF, -1 = error).
 * On success the data is in tcp_stream_read_data() TLS slot. */
nova_int tcp_stream_read_bytes(NovaRt_TcpStream* s, nova_int max) {
    NovaRes_nova_int_nova_str* r = NovaRt_TcpStream_method_read_bytes(s, max);
    if (r->tag != NOVA_TAG_Result_Ok) {
        _net_store_err(r->payload.Err._0);
        return -1;
    }
    nova_int payload = r->payload.Ok._0;
    if (payload == 0) {
        /* EOF: empty string. */
        _net_tcp_read_data = (nova_str){ .ptr = NULL, .len = 0 };
    } else {
        _net_tcp_read_data = *(nova_str*)(intptr_t)payload;
    }
    return (nova_int)_net_tcp_read_data.len;
}
nova_str tcp_stream_read_data(void) { return _net_tcp_read_data; }
nova_unit tcp_stream_set_nodelay(NovaRt_TcpStream* s, nova_bool on) {
    uv_tcp_nodelay(&s->handle, on ? 1 : 0);
    return NOVA_UNIT;
}
nova_unit tcp_stream_set_keepalive(NovaRt_TcpStream* s, nova_bool on) {
    uv_tcp_keepalive(&s->handle, on ? 1 : 0, 60);
    return NOVA_UNIT;
}
nova_unit tcp_listener_set_reuse_address(NovaRt_TcpListener* lst, nova_bool on) {
    (void)lst; (void)on;  /* libuv sets SO_REUSEADDR by default at bind */
    return NOVA_UNIT;
}

/* ─── UdpSocket ────────────────────────────────────────────────────────── */

/* TLS buffers for udp_socket_recv_from results.
 * Safe: Nova fibers are cooperative — no other fiber runs between
 * udp_socket_recv_from() return and the recv_data/recv_sender reads. */
#if defined(_MSC_VER)
  static __declspec(thread) nova_str        _net_recv_data;
  static __declspec(thread) NovaRt_SocketAddr* _net_recv_sender;
#else
  static __thread nova_str        _net_recv_data;
  static __thread NovaRt_SocketAddr* _net_recv_sender;
#endif

NovaRt_UdpSocket* udp_socket_bind(NovaRt_SocketAddr* addr) {
    NovaRes_nova_int_nova_str* r = NovaRt_UdpSocket_static_bind(addr);
    if (r->tag == NOVA_TAG_Result_Ok) return (NovaRt_UdpSocket*)(intptr_t)r->payload.Ok._0;
    _net_store_err(r->payload.Err._0);
    return NULL;
}
nova_int udp_socket_send_to(NovaRt_UdpSocket* s, nova_str data, NovaRt_SocketAddr* addr) {
    NovaRes_nova_int_nova_str* r = NovaRt_UdpSocket_method_send_to(s, data, addr);
    if (r->tag == NOVA_TAG_Result_Ok) return 0;
    _net_store_err(r->payload.Err._0);
    return -1;
}
nova_int udp_socket_recv_from(NovaRt_UdpSocket* s, nova_int max) {
    NovaRes_nova_int_nova_str* r = NovaRt_UdpSocket_method_recv_from(s, max);
    if (r->tag != NOVA_TAG_Result_Ok) {
        _net_store_err(r->payload.Err._0);
        return -1;
    }
    _net_recv_data = *(nova_str*)(intptr_t)r->payload.Ok._0;
    _net_recv_sender = s->last_sender_valid
        ? _nova_addr_from_storage(&s->last_sender_storage)
        : NovaRt_SocketAddr_static_loopback(0);
    return 0;
}
nova_str           udp_socket_recv_data(void)   { return _net_recv_data; }
NovaRt_SocketAddr* udp_socket_recv_sender(void) { return _net_recv_sender; }
uint16_t udp_socket_local_port(NovaRt_UdpSocket* s) {
    return NovaRt_UdpSocket_method_local_port(s);
}
NovaRt_SocketAddr* udp_socket_local_addr(NovaRt_UdpSocket* s) {
    return NovaRt_UdpSocket_method_local_addr(s);
}
nova_unit udp_socket_close(NovaRt_UdpSocket* s) {
    NovaRt_UdpSocket_method_close(s);
    return NOVA_UNIT;
}

/* ─── DNS ─────────────────────────────────────────────────────────────── */

typedef struct {
    uv_getaddrinfo_t    req;
    NovaFiberQueue*     scope;
    int                 slot;
    int                 status;      /* uv error code or 0 */
    struct addrinfo*    res;         /* libuv-owned result list */
} NovaDnsReq;

static void _dns_getaddrinfo_cb(uv_getaddrinfo_t* req, int status,
                                struct addrinfo* res) {
    NovaDnsReq* dr = (NovaDnsReq*)req->data;
    dr->status = status;
    dr->res    = res;
    NovaFiberQueue* sc = dr->scope;
    int             sl = dr->slot;
    dr->scope = NULL;
    nova_sched_wake(sc, sl);
}

static NovaStopMode _dns_stop_cb(void* handle) {
    NovaDnsReq* dr = (NovaDnsReq*)handle;
    /* uv_getaddrinfo can't be cancelled mid-flight without closing the loop.
     * We set a sentinel and the fiber detects cancel on resume. */
    (void)dr;
    return NOVA_STOP_ASYNC;
}

/* TLS: last dns_lookup result array — cooperative-safe (read immediately after call). */
#if defined(_MSC_VER)
  static __declspec(thread) NovaRt_SocketAddr** _net_dns_addrs;
#else
  static __thread NovaRt_SocketAddr** _net_dns_addrs;
#endif

nova_int dns_lookup(const uint8_t* host_ptr, nova_int host_len, uint16_t port) {
    NovaRt_SocketAddr** out_addrs = NULL;
    /* Build NUL-terminated host string. */
    char* host = (char*)malloc((size_t)host_len + 1);
    if (!host) { _net_store_err(_nova_net_cstr("OOM")); return -1; }
    memcpy(host, host_ptr, (size_t)host_len);
    host[host_len] = '\0';

    char port_str[8];
    snprintf(port_str, sizeof(port_str), "%u", (unsigned)port);

    uv_loop_t* loop = nova_current_loop();
    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) {
        free(host);
        fprintf(stderr, "nova/net: dns_lookup outside scope\n");
        abort();
    }

    NovaFiberQueue* cancel_sc = _nova_net_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        free(host);
        _net_store_err(_nova_net_cstr("cancelled"));
        return -1;
    }

    NovaDnsReq* dr = (NovaDnsReq*)nova_alloc(sizeof(NovaDnsReq));
    memset(dr, 0, sizeof(*dr));
    dr->req.data = dr;
    dr->scope    = scope;
    dr->slot     = slot;

    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family   = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    int rc = uv_getaddrinfo(loop, &dr->req, _dns_getaddrinfo_cb,
                            host, port_str, &hints);
    free(host);
    if (rc != 0) {
        _net_store_err(_nova_net_uv_err(rc));
        return -1;
    }

    nova_sched_register_pending(scope, slot, dr, _dns_stop_cb);
    nova_sched_park(scope, slot);
    nova_sched_unregister_pending(scope, slot);

    if (nova_abool_load(&cancel_sc->cancel_requested)) {
        if (dr->res) uv_freeaddrinfo(dr->res);
        _net_store_err(_nova_net_cstr("cancelled"));
        return -1;
    }

    if (dr->status != 0) {
        if (dr->res) uv_freeaddrinfo(dr->res);
        _net_store_err(_nova_net_uv_err(dr->status));
        return -1;
    }

    /* Count results. */
    nova_int count = 0;
    for (struct addrinfo* ai = dr->res; ai != NULL; ai = ai->ai_next) {
        if (ai->ai_family == AF_INET || ai->ai_family == AF_INET6) count++;
    }
    if (count == 0) {
        uv_freeaddrinfo(dr->res);
        _net_store_err(_nova_net_cstr("no addresses"));
        return -1;
    }

    /* Allocate array of SocketAddr* on the GC heap. */
    NovaRt_SocketAddr** arr = (NovaRt_SocketAddr**)
        nova_alloc(sizeof(NovaRt_SocketAddr*) * (size_t)count);
    nova_int i = 0;
    for (struct addrinfo* ai = dr->res; ai != NULL; ai = ai->ai_next) {
        if (ai->ai_family != AF_INET && ai->ai_family != AF_INET6) continue;
        struct sockaddr_storage ss;
        memset(&ss, 0, sizeof(ss));
        memcpy(&ss, ai->ai_addr, ai->ai_addrlen);
        arr[i++] = _nova_addr_from_storage(&ss);
    }
    uv_freeaddrinfo(dr->res);

    _net_dns_addrs = arr;
    return count;
}

nova_int dns_addr_at(nova_int i) {
    return (nova_int)(intptr_t)_net_dns_addrs[i];
}
