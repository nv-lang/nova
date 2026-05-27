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

/* ─── Nova_SocketAddr ──────────────────────────────────────────────── */

static Nova_SocketAddr* _nova_alloc_addr(void) {
    Nova_SocketAddr* a = (Nova_SocketAddr*)nova_alloc(sizeof(Nova_SocketAddr));
    memset(a, 0, sizeof(*a));
    return a;
}

static Nova_SocketAddr* _nova_addr_from_storage(const struct sockaddr_storage* ss) {
    Nova_SocketAddr* a = _nova_alloc_addr();
    memcpy(&a->storage, ss, sizeof(*ss));
    return a;
}

Nova_SocketAddr* Nova_SocketAddr_static_loopback(uint16_t port) {
    Nova_SocketAddr* a = _nova_alloc_addr();
    struct sockaddr_in* in4 = (struct sockaddr_in*)&a->storage;
    uv_ip4_addr("127.0.0.1", port, in4);
    return a;
}

Nova_SocketAddr* Nova_SocketAddr_static_loopback_v6(uint16_t port) {
    Nova_SocketAddr* a = _nova_alloc_addr();
    struct sockaddr_in6* in6 = (struct sockaddr_in6*)&a->storage;
    uv_ip6_addr("::1", port, in6);
    return a;
}

Nova_SocketAddr* Nova_SocketAddr_static_v4(uint8_t a, uint8_t b,
                                            uint8_t c, uint8_t d,
                                            uint16_t port) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%u.%u.%u.%u", a, b, c, d);
    Nova_SocketAddr* addr = _nova_alloc_addr();
    struct sockaddr_in* in4 = (struct sockaddr_in*)&addr->storage;
    uv_ip4_addr(buf, port, in4);
    return addr;
}

NovaRes_nova_int_nova_str* Nova_SocketAddr_static_parse(nova_str s) {
    /* Copy to NUL-terminated buffer. */
    char* buf = (char*)malloc(s.len + 1);
    if (!buf) return _NET_ERR("OOM");
    memcpy(buf, s.ptr, s.len);
    buf[s.len] = '\0';

    /* Try IPv4 first: "host:port". Find last ':'. */
    char* colon = strrchr(buf, ':');
    if (!colon) { free(buf); return _NET_ERR("invalid addr: no port"); }

    int port_n = atoi(colon + 1);
    if (port_n <= 0 || port_n > 65535) {
        free(buf); return _NET_ERR("invalid port");
    }
    *colon = '\0';

    Nova_SocketAddr* addr = _nova_alloc_addr();
    /* Try IPv4. */
    if (uv_ip4_addr(buf, port_n, (struct sockaddr_in*)&addr->storage) == 0) {
        free(buf); return _NET_OK(addr);
    }
    /* Try IPv6 (strip brackets if "[::1]"). */
    char* host = buf;
    if (host[0] == '[') {
        host++;
        char* rbrace = strchr(host, ']');
        if (rbrace) *rbrace = '\0';
    }
    if (uv_ip6_addr(host, port_n, (struct sockaddr_in6*)&addr->storage) == 0) {
        free(buf); return _NET_OK(addr);
    }
    free(buf);
    return _NET_ERR("invalid address");
}

uint16_t Nova_SocketAddr_method_port(Nova_SocketAddr* addr) {
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

static void _populate_host_cache(Nova_SocketAddr* addr) {
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

nova_str Nova_SocketAddr_method_host_str(Nova_SocketAddr* addr) {
    _populate_host_cache(addr);
    return _nova_net_cstr(addr->host_cache);
}

nova_bool Nova_SocketAddr_method_is_v4(Nova_SocketAddr* addr) {
    return addr->storage.ss_family == AF_INET;
}

nova_bool Nova_SocketAddr_method_is_v6(Nova_SocketAddr* addr) {
    return addr->storage.ss_family == AF_INET6;
}

nova_str Nova_SocketAddr_method_to_str(Nova_SocketAddr* addr) {
    char buf[128];
    _populate_host_cache(addr);
    uint16_t port = Nova_SocketAddr_method_port(addr);
    if (addr->storage.ss_family == AF_INET6) {
        snprintf(buf, sizeof(buf), "[%s]:%u", addr->host_cache, port);
    } else {
        snprintf(buf, sizeof(buf), "%s:%u", addr->host_cache, port);
    }
    return _nova_net_cstr(buf);
}

/* ─── Nova_TcpListener ─────────────────────────────────────────────── */

/* Forward decls. */
static void _tcp_listener_close_cb(uv_handle_t* h);
static NovaStopMode _tcp_listener_accept_stop_cb(void* handle);
static void _tcp_connection_cb(uv_stream_t* srv, int status);

static Nova_TcpListener* _nova_alloc_listener(void) {
    Nova_TcpListener* lst = (Nova_TcpListener*)
        nova_alloc_uncollectable(sizeof(Nova_TcpListener));
    memset(lst, 0, sizeof(*lst));
    nova_aint_init(&lst->stage, NOVA_NET_STAGE_IDLE);
    return lst;
}

NovaRes_nova_int_nova_str* Nova_TcpListener_static_bind(Nova_SocketAddr* addr) {
    uv_loop_t* loop = nova_current_loop();
    Nova_TcpListener* lst = _nova_alloc_listener();
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
    Nova_TcpListener* lst = (Nova_TcpListener*)srv->data;
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
    Nova_TcpListener* lst = (Nova_TcpListener*)handle;
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
    Nova_TcpListener* lst = (Nova_TcpListener*)h->data;
    nova_aint_store(&lst->stage, NOVA_NET_STAGE_CLOSED);
    if (lst->accept_scope) {
        NovaFiberQueue* sc = lst->accept_scope;
        int sl             = lst->accept_slot;
        lst->accept_scope  = NULL;
        nova_sched_wake(sc, sl);
    }
}

NovaRes_nova_int_nova_str* Nova_TcpListener_method_accept(Nova_TcpListener* lst) {
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
    Nova_TcpStream* st = (Nova_TcpStream*)
        nova_alloc_uncollectable(sizeof(Nova_TcpStream));
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

uint16_t Nova_TcpListener_method_local_port(Nova_TcpListener* lst) {
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

Nova_SocketAddr* Nova_TcpListener_method_local_addr(Nova_TcpListener* lst) {
    Nova_SocketAddr* a = _nova_alloc_addr();
    int namelen = sizeof(a->storage);
    uv_tcp_getsockname(&lst->handle, (struct sockaddr*)&a->storage, &namelen);
    return a;
}

nova_unit Nova_TcpListener_method_close(Nova_TcpListener* lst) {
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

/* ─── Nova_TcpStream ───────────────────────────────────────────────── */

/* Forward decls. */
static void _tcp_stream_close_cb(uv_handle_t* h);
static NovaStopMode _tcp_stream_op_stop_cb(void* handle);
static void _tcp_connect_cb(uv_connect_t* req, int status);
static void _tcp_read_cb(uv_stream_t* stream, ssize_t nread, const uv_buf_t* buf);
static void _tcp_alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf);
static void _tcp_write_cb(uv_write_t* req, int status);

static Nova_TcpStream* _nova_alloc_stream(void) {
    Nova_TcpStream* s = (Nova_TcpStream*)
        nova_alloc_uncollectable(sizeof(Nova_TcpStream));
    memset(s, 0, sizeof(*s));
    nova_aint_init(&s->stage, NOVA_NET_STAGE_IDLE);
    return s;
}

NovaRes_nova_int_nova_str* Nova_TcpStream_static_connect(Nova_SocketAddr* addr) {
    uv_loop_t* loop = nova_current_loop();
    Nova_TcpStream* s = _nova_alloc_stream();
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
    Nova_TcpStream* s = (Nova_TcpStream*)req->data;
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
    Nova_TcpStream* s = (Nova_TcpStream*)handle;
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
    Nova_TcpStream* s = (Nova_TcpStream*)h->data;
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
    Nova_TcpStream* s = (Nova_TcpStream*)h->data;
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
    Nova_TcpStream* s = (Nova_TcpStream*)stream->data;
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

NovaRes_nova_int_nova_str* Nova_TcpStream_method_read_bytes(
        Nova_TcpStream* s, nova_int max_bytes) {
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
    Nova_TcpStream* s = (Nova_TcpStream*)req->data;
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

NovaRes_nova_int_nova_str* Nova_TcpStream_method_write(
        Nova_TcpStream* s, nova_str data) {
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

uint16_t Nova_TcpStream_method_local_port(Nova_TcpStream* s) {
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getsockname(&s->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

uint16_t Nova_TcpStream_method_peer_port(Nova_TcpStream* s) {
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_tcp_getpeername(&s->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

Nova_SocketAddr* Nova_TcpStream_method_local_addr(Nova_TcpStream* s) {
    Nova_SocketAddr* a = _nova_alloc_addr();
    int n = sizeof(a->storage);
    uv_tcp_getsockname(&s->handle, (struct sockaddr*)&a->storage, &n);
    return a;
}

Nova_SocketAddr* Nova_TcpStream_method_peer_addr(Nova_TcpStream* s) {
    Nova_SocketAddr* a = _nova_alloc_addr();
    int n = sizeof(a->storage);
    uv_tcp_getpeername(&s->handle, (struct sockaddr*)&a->storage, &n);
    return a;
}

nova_unit Nova_TcpStream_method_close(Nova_TcpStream* s) {
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

/* ─── Nova_UdpSocket ───────────────────────────────────────────────── */

/* Forward decls. */
static void _udp_close_cb(uv_handle_t* h);
static NovaStopMode _udp_recv_stop_cb(void* handle);
static void _udp_alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf);
static void _udp_recv_cb(uv_udp_t* handle, ssize_t nread,
                          const uv_buf_t* buf,
                          const struct sockaddr* sender,
                          unsigned int flags);
static void _udp_send_cb(uv_udp_send_t* req, int status);

typedef struct { Nova_UdpSocket* sock; uv_udp_send_t req; char* buf; } _NovaUdpSendCtx;

NovaRes_nova_int_nova_str* Nova_UdpSocket_static_bind(Nova_SocketAddr* addr) {
    uv_loop_t* loop = nova_current_loop();
    Nova_UdpSocket* sock = (Nova_UdpSocket*)
        nova_alloc_uncollectable(sizeof(Nova_UdpSocket));
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

NovaRes_nova_int_nova_str* Nova_UdpSocket_method_send_to(
        Nova_UdpSocket* sock, nova_str data, Nova_SocketAddr* addr) {
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
    Nova_UdpSocket* sock = ctx->sock;
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
    Nova_UdpSocket* sock = (Nova_UdpSocket*)h->data;
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
    Nova_UdpSocket* sock = (Nova_UdpSocket*)handle->data;
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
    Nova_UdpSocket* sock = (Nova_UdpSocket*)handle;
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
    Nova_UdpSocket* sock = (Nova_UdpSocket*)h->data;
    nova_aint_store(&sock->stage, NOVA_NET_STAGE_CLOSED);
    if (sock->recv_scope) {
        NovaFiberQueue* sc = sock->recv_scope;
        int sl = sock->recv_slot;
        sock->recv_scope = NULL;
        nova_sched_wake(sc, sl);
    }
}

NovaRes_nova_int_nova_str* Nova_UdpSocket_method_recv_from(
        Nova_UdpSocket* sock, nova_int max_bytes) {
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

Nova_SocketAddr* Nova_UdpSocket_method_last_sender(Nova_UdpSocket* sock) {
    if (!sock->last_sender_valid) {
        return Nova_SocketAddr_static_loopback(0);
    }
    Nova_SocketAddr* a = _nova_alloc_addr();
    memcpy(&a->storage, &sock->last_sender_storage, sizeof(struct sockaddr_storage));
    return a;
}

uint16_t Nova_UdpSocket_method_local_port(Nova_UdpSocket* sock) {
    struct sockaddr_storage ss; int n = sizeof(ss);
    if (uv_udp_getsockname(&sock->handle, (struct sockaddr*)&ss, &n) != 0) return 0;
    if (ss.ss_family == AF_INET)  return ntohs(((struct sockaddr_in*)&ss)->sin_port);
    if (ss.ss_family == AF_INET6) return ntohs(((struct sockaddr_in6*)&ss)->sin6_port);
    return 0;
}

Nova_SocketAddr* Nova_UdpSocket_method_local_addr(Nova_UdpSocket* sock) {
    Nova_SocketAddr* a = _nova_alloc_addr();
    int n = sizeof(a->storage);
    uv_udp_getsockname(&sock->handle, (struct sockaddr*)&a->storage, &n);
    return a;
}

nova_unit Nova_UdpSocket_method_close(Nova_UdpSocket* sock) {
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
