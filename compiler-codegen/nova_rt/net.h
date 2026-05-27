#ifndef NOVA_RT_NET_H
#define NOVA_RT_NET_H

/* Plan 83.12: async net/socket stdlib — TCP + UDP via libuv.
 *
 * Implements std/net/{tcp,udp,addr} using the Plan 22 park/wake pattern
 * (D93) and the Plan 83.10.2 NovaDeferredCloseQueue for cross-thread
 * uv_close safety.
 *
 * Types:
 *   Nova_SocketAddr — opaque sockaddr_storage wrapper
 *   Nova_TcpListener — uv_tcp_t server-side listener
 *   Nova_TcpStream   — uv_tcp_t connected stream (client or accepted)
 *   Nova_UdpSocket   — uv_udp_t datagram socket
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

/* ─── Nova_SocketAddr ──────────────────────────────────────────────── */

/* Opaque IPv4/IPv6 socket address. Large enough for both families.
 * host_cache is populated lazily by host_str(). */
typedef struct Nova_SocketAddr {
    struct sockaddr_storage storage;   /* actual address data */
    char    host_cache[64];            /* cached host string (NULL-term) */
    int     host_cached;               /* 1 once host_cache is valid */
} Nova_SocketAddr;

/* ─── Nova_TcpListener ─────────────────────────────────────────────── */

/* The connection_cb state is stored here; at most one pending accept at
 * a time (V1). A separate pending_accepts counter tracks backlogged
 * connections so a fast client doesn't get missed. */
typedef struct Nova_TcpListener {
    uv_tcp_t        handle;            /* must be first (uv_close compat) */
    uv_loop_t*      loop;              /* owning loop */
    nova_atomic_int stage;             /* NovaNetStage */

    /* One-slot pending-accept queue: */
    NovaFiberQueue* accept_scope;      /* NULL when no waiter */
    int             accept_slot;
    void*           accept_result;     /* Nova_TcpStream* on success */
    nova_str        accept_error;      /* error msg if accept_result==NULL */
    int             pending_conns;     /* # connections queued by OS */
} Nova_TcpListener;

/* ─── Nova_TcpStream ───────────────────────────────────────────────── */

/* One pending operation at a time (connect/read_bytes/write). The
 * same scope/slot fields are reused across operations; callers must
 * serialise. */
typedef struct Nova_TcpStream {
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
} Nova_TcpStream;

/* ─── Nova_UdpSocket ───────────────────────────────────────────────── */

typedef struct Nova_UdpSocket {
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
} Nova_UdpSocket;

/* ─── SocketAddr constructors ──────────────────────────────────────── */

Nova_SocketAddr* Nova_SocketAddr_static_loopback(uint16_t port);
Nova_SocketAddr* Nova_SocketAddr_static_loopback_v6(uint16_t port);
Nova_SocketAddr* Nova_SocketAddr_static_v4(uint8_t a, uint8_t b,
                                            uint8_t c, uint8_t d,
                                            uint16_t port);
NovaRes_nova_int_nova_str*
    Nova_SocketAddr_static_parse(nova_str s);
uint16_t  Nova_SocketAddr_method_port(Nova_SocketAddr* addr);
nova_str  Nova_SocketAddr_method_host_str(Nova_SocketAddr* addr);
nova_bool Nova_SocketAddr_method_is_v4(Nova_SocketAddr* addr);
nova_bool Nova_SocketAddr_method_is_v6(Nova_SocketAddr* addr);
nova_str  Nova_SocketAddr_method_to_str(Nova_SocketAddr* addr);

/* ─── TcpListener methods ──────────────────────────────────────────── */

NovaRes_nova_int_nova_str*
    Nova_TcpListener_static_bind(Nova_SocketAddr* addr);
NovaRes_nova_int_nova_str*
    Nova_TcpListener_method_accept(Nova_TcpListener* lst);
uint16_t         Nova_TcpListener_method_local_port(Nova_TcpListener* lst);
Nova_SocketAddr* Nova_TcpListener_method_local_addr(Nova_TcpListener* lst);
nova_unit        Nova_TcpListener_method_close(Nova_TcpListener* lst);

/* ─── TcpStream methods ────────────────────────────────────────────── */

NovaRes_nova_int_nova_str*
    Nova_TcpStream_static_connect(Nova_SocketAddr* addr);
NovaRes_nova_int_nova_str*
    Nova_TcpStream_method_read_bytes(Nova_TcpStream* s, nova_int max_bytes);
NovaRes_nova_int_nova_str*
    Nova_TcpStream_method_write(Nova_TcpStream* s, nova_str data);
uint16_t         Nova_TcpStream_method_local_port(Nova_TcpStream* s);
uint16_t         Nova_TcpStream_method_peer_port(Nova_TcpStream* s);
Nova_SocketAddr* Nova_TcpStream_method_local_addr(Nova_TcpStream* s);
Nova_SocketAddr* Nova_TcpStream_method_peer_addr(Nova_TcpStream* s);
nova_unit        Nova_TcpStream_method_close(Nova_TcpStream* s);

/* ─── UdpSocket methods ────────────────────────────────────────────── */

NovaRes_nova_int_nova_str*
    Nova_UdpSocket_static_bind(Nova_SocketAddr* addr);
NovaRes_nova_int_nova_str*
    Nova_UdpSocket_method_send_to(Nova_UdpSocket* sock,
                                   nova_str data, Nova_SocketAddr* addr);
NovaRes_nova_int_nova_str*
    Nova_UdpSocket_method_recv_from(Nova_UdpSocket* sock, nova_int max_bytes);
Nova_SocketAddr* Nova_UdpSocket_method_last_sender(Nova_UdpSocket* sock);
uint16_t         Nova_UdpSocket_method_local_port(Nova_UdpSocket* sock);
Nova_SocketAddr* Nova_UdpSocket_method_local_addr(Nova_UdpSocket* sock);
nova_unit        Nova_UdpSocket_method_close(Nova_UdpSocket* sock);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_NET_H */
