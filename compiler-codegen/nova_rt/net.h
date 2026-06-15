#ifndef NOVA_RT_NET_H
#define NOVA_RT_NET_H

/* Plan 83.12: async net/socket stdlib — TCP + UDP via libuv.
 *
 * Implements std/net/{tcp,udp,addr} using the Plan 22 park/wake pattern
 * (D93) and the Plan 83.10.2 NovaDeferredCloseQueue for cross-thread
 * uv_close safety.
 *
 * Types:
 *   NovaRt_SocketAddr — opaque sockaddr_storage wrapper
 *   NovaRt_TcpListener — uv_tcp_t server-side listener
 *   NovaRt_TcpStream   — uv_tcp_t connected stream (client or accepted)
 *   NovaRt_UdpSocket   — uv_udp_t datagram socket
 *
 * Park/wake lifecycle (per operation, follows sleep pattern):
 *   1. Caller fiber: set up request, register stop_cb, park(scope, slot).
 *   2. libuv callback fires on owning loop thread: store result, wake(scope, slot).
 *   3. Fiber resumes: unregister stop_cb, check cancel_requested.
 *
 * Thread-affinity invariant (Plan 83.10.2):
 *   All uv_* operations on a handle MUST run on the thread that owns the
 *   handle's loop. Cross-thread close (from cancel stop_cb) is routed via
 *   nova_loop_defer_close so the loop's thread performs the actual uv_close.
 *
 * Allocation: all net handles use nova_alloc_uncollectable() to prevent GC
 * collection while a live libuv handle references the struct.
 */

#ifndef NOVA_USE_LIBUV
#  error "Plan 83.12: NOVA_USE_LIBUV required for std/net."
#endif

#include <uv.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include "nova_rt.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ─── Stage enum (shared by all net handle types) ─────────────────── */

typedef enum {
    NOVA_NET_STAGE_IDLE    = 0,  /* handle alive, no pending operation */
    NOVA_NET_STAGE_PENDING = 1,  /* async operation in flight, fiber parked */
    NOVA_NET_STAGE_CLOSING = 2,  /* uv_close issued */
    NOVA_NET_STAGE_CLOSED  = 3,  /* close_cb has fired */
} NovaNetStage;

/* ─── NovaRt_SocketAddr ──────────────────────────────────────────────── */

/* Opaque IPv4/IPv6 socket address. Large enough for both families.
 * host_cache is populated lazily by host_str(). */
typedef struct NovaRt_SocketAddr {
    struct sockaddr_storage storage;   /* actual address data */
    char    host_cache[64];            /* cached host string (NULL-term) */
    int     host_cached;               /* 1 once host_cache is valid */
} NovaRt_SocketAddr;

/* ─── NovaRt_TcpListener ─────────────────────────────────────────────── */

/* The connection_cb state is stored here; at most one pending accept at
 * a time (V1). A separate pending_accepts counter tracks backlogged
 * connections so a fast client doesn't get missed. */
typedef struct NovaRt_TcpListener {
    uv_tcp_t        handle;            /* must be first (uv_close compat) */
    uv_loop_t*      loop;              /* owning loop */
    nova_atomic_int stage;             /* NovaNetStage */

    /* One-slot pending-accept queue: */
    NovaFiberQueue* accept_scope;      /* NULL when no waiter */
    int             accept_slot;
    void*           accept_result;     /* NovaRt_TcpStream* on success */
    nova_str        accept_error;      /* error msg if accept_result==NULL */
    int             pending_conns;     /* # connections queued by OS */
} NovaRt_TcpListener;

/* ─── NovaRt_TcpStream ───────────────────────────────────────────────── */

/* One pending operation at a time (connect/read_bytes/write). The
 * same scope/slot fields are reused across operations; callers must
 * serialise. */
typedef struct NovaRt_TcpStream {
    uv_tcp_t        handle;            /* must be first */
    uv_loop_t*      loop;              /* owning loop */
    nova_atomic_int stage;             /* NovaNetStage */

    /* Pending operation (connect / read_bytes / write): */
    NovaFiberQueue* op_scope;          /* NULL when idle */
    int             op_slot;
    nova_str        op_error;          /* set on failure */

    /* connect_req (reusable for single connect): */
    uv_connect_t    connect_req;

    /* read state: */
    char*           read_buf;          /* malloc'd, freed after read */
    ssize_t         read_len;          /* bytes received (≥0) or UV error (<0) */
    int             read_max;          /* max_bytes requested */
    int             is_eof;            /* 1 if UV_EOF received */

    /* write state: */
    uv_write_t      write_req;
    char*           write_buf;         /* copy of user data (malloc'd) */
    ssize_t         write_len;         /* bytes written on success */
} NovaRt_TcpStream;

/* ─── NovaRt_UdpSocket ───────────────────────────────────────────────── */

typedef struct NovaRt_UdpSocket {
    uv_udp_t        handle;            /* must be first */
    uv_loop_t*      loop;              /* owning loop */
    nova_atomic_int stage;             /* NovaNetStage */

    /* Pending recv_from: */
    NovaFiberQueue* recv_scope;        /* NULL when idle */
    int             recv_slot;
    nova_str        recv_error;

    char*           recv_buf;          /* malloc'd, freed after recv */
    ssize_t         recv_len;          /* bytes received */
    int             recv_max;          /* max_bytes requested */

    /* Last sender (set by alloc_cb/recv_cb): */
    struct sockaddr_storage last_sender_storage;
    int             last_sender_valid; /* 1 once populated */
} NovaRt_UdpSocket;

/* ─── SocketAddr constructors ──────────────────────────────────────── */

NovaRt_SocketAddr* NovaRt_SocketAddr_static_loopback(uint16_t port);
NovaRt_SocketAddr* NovaRt_SocketAddr_static_loopback_v6(uint16_t port);
NovaRt_SocketAddr* NovaRt_SocketAddr_static_v4(uint8_t a, uint8_t b,
                                            uint8_t c, uint8_t d,
                                            uint16_t port);
NovaRes_nova_int_nova_str*
    NovaRt_SocketAddr_static_parse(nova_str s);
uint16_t  NovaRt_SocketAddr_method_port(NovaRt_SocketAddr* addr);
nova_str  NovaRt_SocketAddr_method_host_str(NovaRt_SocketAddr* addr);
nova_bool NovaRt_SocketAddr_method_is_v4(NovaRt_SocketAddr* addr);
nova_bool NovaRt_SocketAddr_method_is_v6(NovaRt_SocketAddr* addr);
nova_str  NovaRt_SocketAddr_method_to_str(NovaRt_SocketAddr* addr);

/* ─── TcpListener methods ──────────────────────────────────────────── */

NovaRes_nova_int_nova_str*
    NovaRt_TcpListener_static_bind(NovaRt_SocketAddr* addr);
NovaRes_nova_int_nova_str*
    NovaRt_TcpListener_method_accept(NovaRt_TcpListener* lst);
uint16_t         NovaRt_TcpListener_method_local_port(NovaRt_TcpListener* lst);
NovaRt_SocketAddr* NovaRt_TcpListener_method_local_addr(NovaRt_TcpListener* lst);
nova_unit        NovaRt_TcpListener_method_close(NovaRt_TcpListener* lst);

/* ─── TcpStream methods ────────────────────────────────────────────── */

NovaRes_nova_int_nova_str*
    NovaRt_TcpStream_static_connect(NovaRt_SocketAddr* addr);
NovaRes_nova_int_nova_str*
    NovaRt_TcpStream_method_read_bytes(NovaRt_TcpStream* s, nova_int max_bytes);
NovaRes_nova_int_nova_str*
    NovaRt_TcpStream_method_write(NovaRt_TcpStream* s, nova_str data);
uint16_t         NovaRt_TcpStream_method_local_port(NovaRt_TcpStream* s);
uint16_t         NovaRt_TcpStream_method_peer_port(NovaRt_TcpStream* s);
NovaRt_SocketAddr* NovaRt_TcpStream_method_local_addr(NovaRt_TcpStream* s);
NovaRt_SocketAddr* NovaRt_TcpStream_method_peer_addr(NovaRt_TcpStream* s);
nova_unit        NovaRt_TcpStream_method_close(NovaRt_TcpStream* s);

/* ─── UdpSocket methods ────────────────────────────────────────────── */

NovaRes_nova_int_nova_str*
    NovaRt_UdpSocket_static_bind(NovaRt_SocketAddr* addr);
NovaRes_nova_int_nova_str*
    NovaRt_UdpSocket_method_send_to(NovaRt_UdpSocket* sock,
                                   nova_str data, NovaRt_SocketAddr* addr);
NovaRes_nova_int_nova_str*
    NovaRt_UdpSocket_method_recv_from(NovaRt_UdpSocket* sock, nova_int max_bytes);
NovaRt_SocketAddr* NovaRt_UdpSocket_method_last_sender(NovaRt_UdpSocket* sock);
uint16_t         NovaRt_UdpSocket_method_local_port(NovaRt_UdpSocket* sock);
NovaRt_SocketAddr* NovaRt_UdpSocket_method_local_addr(NovaRt_UdpSocket* sock);
nova_unit        NovaRt_UdpSocket_method_close(NovaRt_UdpSocket* sock);

/* ─── Plan 91.12 Ф.0: literal-name entry-points (Nova extern "C" fn) ──── */
/*
 * Handle ABI: all C handles (NovaRt_SocketAddr*, etc.) are passed and returned
 * as nova_int (= intptr_t).  Constructors return (nova_int)ptr or -1 on error.
 * Error message: call net_last_error() after any -1 return.
 *
 * udp_socket_recv_from uses TLS: stores data+sender in thread-local buffers
 * for Nova to read via udp_socket_recv_data() / udp_socket_recv_sender()
 * immediately after (no intervening Blocking call → cooperative-safe).
 */

nova_int         socket_addr_loopback(uint16_t port);
nova_int         socket_addr_loopback_v6(uint16_t port);
nova_int         socket_addr_v4(uint8_t a, uint8_t b, uint8_t c, uint8_t d, uint16_t port);
nova_int         socket_addr_parse(nova_str s);    /* -1 on parse error */
uint16_t         socket_addr_port(nova_int addr);
nova_str         socket_addr_host_str(nova_int addr);
nova_bool        socket_addr_is_v4(nova_int addr);
nova_bool        socket_addr_is_v6(nova_int addr);
nova_str         socket_addr_to_str(nova_int addr);

nova_int         tcp_listener_bind(nova_int addr);   /* -1 on error */
nova_int         tcp_listener_accept(nova_int lst);  /* -1 on error */
uint16_t         tcp_listener_local_port(nova_int lst);
nova_int         tcp_listener_local_addr(nova_int lst);
nova_unit        tcp_listener_close(nova_int lst);

nova_int         tcp_stream_connect(nova_int addr);  /* -1 on error */
nova_int         tcp_stream_write(nova_int s, nova_str data);  /* bytes or -1 */
uint16_t         tcp_stream_local_port(nova_int s);
uint16_t         tcp_stream_peer_port(nova_int s);
nova_int         tcp_stream_local_addr(nova_int s);
nova_int         tcp_stream_peer_addr(nova_int s);
nova_unit        tcp_stream_close(nova_int s);
nova_str         net_last_error(void);  /* thread-local; valid after -1 return */

nova_int         udp_socket_bind(nova_int addr);     /* -1 on error */
nova_int         udp_socket_send_to(nova_int s, nova_str data, nova_int addr); /* 0 or -1 */
nova_int         udp_socket_recv_from(nova_int s, nova_int max);               /* 0 or -1 */
nova_str         udp_socket_recv_data(void);    /* TLS: data from last recv_from */
nova_int         udp_socket_recv_sender(void);  /* TLS: sender from last recv_from */
uint16_t         udp_socket_local_port(nova_int s);
nova_int         udp_socket_local_addr(nova_int s);
nova_unit        udp_socket_close(nova_int s);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_NET_H */
