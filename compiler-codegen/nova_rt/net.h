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

/* ─── NetAddrResult: error codes for address parsing ──────────────── */

typedef enum {
    NET_ADDR_OK           = 0,
    NET_ADDR_INVALID_ADDR = 1,  /* malformed host or missing port separator */
    NET_ADDR_INVALID_PORT = 2,  /* port out of range 1-65535 */
} NetAddrResult;

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
/* Parse NUL-terminated "host:port" string into addr (must be pre-allocated).
 * Returns NET_ADDR_OK on success; addr->storage is populated.
 * On error the storage is undefined; caller must not use addr. */
NetAddrResult
    NovaRt_SocketAddr_static_parse(const char* s, NovaRt_SocketAddr* addr);
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
 * Handle ABI: all C handles are passed and returned as their typed pointer
 * (NovaRt_SocketAddr*, NovaRt_TcpListener*, etc.). Constructors return the
 * pointer or NULL on error. Error message: call net_last_error() after any
 * NULL return. Numeric results (bytes written, recv status) use nova_int.
 *
 * Nova sees these as CSocketAddr(*()) / CTcpListener(*()) etc. — opaque
 * newtypes over void* — which is ABI-compatible with typed C pointers.
 *
 * udp_socket_recv_from uses TLS: stores data+sender in thread-local buffers
 * for Nova to read via udp_socket_recv_data() / udp_socket_recv_sender()
 * immediately after (no intervening Blocking call → cooperative-safe).
 */

/* Tuple return type for socket_addr_parse: Nova (int, CSocketAddr).
 * CSocketAddr is a newtype over *() — codegen erases it to nova_int.
 * So the ABI tuple is { nova_int f0 (code); nova_int f1 (handle as intptr) }.
 * This matches the codegen-emitted _NovaTuple2 typedef exactly. */
#ifndef NOVA_TUPLE_TYPEDEF__NovaTuple2
#define NOVA_TUPLE_TYPEDEF__NovaTuple2
typedef struct { nova_int f0; nova_int f1; } _NovaTuple2;
#endif

NovaRt_SocketAddr*  socket_addr_loopback(uint16_t port);
NovaRt_SocketAddr*  socket_addr_loopback_v6(uint16_t port);
NovaRt_SocketAddr*  socket_addr_v4(uint8_t a, uint8_t b, uint8_t c, uint8_t d, uint16_t port);
/* Parse "host:port". Returns (code, handle-as-intptr):
 * f0=code (0=OK, 1=INVALID_ADDR, 2=INVALID_PORT), f1=NovaRt_SocketAddr* cast to nova_int. */
_NovaTuple2         socket_addr_parse(nova_str s);
uint16_t            socket_addr_port(NovaRt_SocketAddr* addr);
nova_str            socket_addr_host_str(NovaRt_SocketAddr* addr);
nova_bool           socket_addr_is_v4(NovaRt_SocketAddr* addr);
nova_bool           socket_addr_is_v6(NovaRt_SocketAddr* addr);
nova_str            socket_addr_to_str(NovaRt_SocketAddr* addr);

NovaRt_TcpListener* tcp_listener_bind(NovaRt_SocketAddr* addr);   /* NULL on error */
NovaRt_TcpStream*   tcp_listener_accept(NovaRt_TcpListener* lst); /* NULL on error */
uint16_t            tcp_listener_local_port(NovaRt_TcpListener* lst);
NovaRt_SocketAddr*  tcp_listener_local_addr(NovaRt_TcpListener* lst);
nova_unit           tcp_listener_close(NovaRt_TcpListener* lst);

NovaRt_TcpStream*   tcp_stream_connect(NovaRt_SocketAddr* addr);  /* NULL on error */
nova_int            tcp_stream_write(NovaRt_TcpStream* s, nova_str data);  /* bytes or -1 */
nova_int            tcp_stream_read_bytes(NovaRt_TcpStream* s, nova_int max); /* bytes (0=EOF), -1=error */
nova_str            tcp_stream_read_data(void);  /* TLS: data from last tcp_stream_read_bytes */
uint16_t            tcp_stream_local_port(NovaRt_TcpStream* s);
uint16_t            tcp_stream_peer_port(NovaRt_TcpStream* s);
NovaRt_SocketAddr*  tcp_stream_local_addr(NovaRt_TcpStream* s);
NovaRt_SocketAddr*  tcp_stream_peer_addr(NovaRt_TcpStream* s);
nova_unit           tcp_stream_set_nodelay(NovaRt_TcpStream* s, nova_bool on);    /* TCP_NODELAY */
nova_unit           tcp_stream_set_keepalive(NovaRt_TcpStream* s, nova_bool on);  /* SO_KEEPALIVE */
nova_unit           tcp_stream_close(NovaRt_TcpStream* s);
nova_unit           tcp_listener_set_reuse_address(NovaRt_TcpListener* lst, nova_bool on); /* SO_REUSEADDR */
nova_str            net_last_error(void);  /* thread-local; valid after NULL/-1 return */

NovaRt_UdpSocket*   udp_socket_bind(NovaRt_SocketAddr* addr);     /* NULL on error */
nova_int            udp_socket_send_to(NovaRt_UdpSocket* s, nova_str data, NovaRt_SocketAddr* addr);
nova_int            udp_socket_recv_from(NovaRt_UdpSocket* s, nova_int max); /* 0 or -1 */
nova_str            udp_socket_recv_data(void);    /* TLS: data from last recv_from */
NovaRt_SocketAddr*  udp_socket_recv_sender(void);  /* TLS: sender from last recv_from */
uint16_t            udp_socket_local_port(NovaRt_UdpSocket* s);
NovaRt_SocketAddr*  udp_socket_local_addr(NovaRt_UdpSocket* s);
nova_unit           udp_socket_close(NovaRt_UdpSocket* s);

/* ─── DNS ─────────────────────────────────────────────────────────── */

/* dns_lookup: resolve "host" to a list of SocketAddr for the given port.
 * Blocking (parks fiber via uv_getaddrinfo callback).
 * Returns count of addresses written into *out_addrs (GC-heap array).
 * Returns -1 on error; call net_last_error() for the message.
 *
 * Nova ffi.nv wraps this as:
 *   extern "C" fn dns_lookup(host *u8, host_len int, port u16, out_addrs *()) -> int
 * On success Nova reads out_addrs[0..count-1] as CSocketAddr handles. */
/* dns_lookup: park fiber, call uv_getaddrinfo.
 * Returns count (≥1) on success; dns_last_addrs() returns the GC-heap array.
 * Returns -1 on error; net_last_error() returns the message.
 * The two TLS accessors are cooperative-safe: no Blocking call may interleave
 * between dns_lookup() returning and dns_last_addrs()/dns_addr_at() reads. */
nova_int            dns_lookup(const uint8_t* host, nova_int host_len,
                               uint16_t port);

/* dns_addr_at: read the i-th SocketAddr from the last dns_lookup result.
 * Returns the pointer cast to nova_int (intptr_t) — matches CSocketAddr ABI. */
nova_int            dns_addr_at(nova_int i);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_NET_H */
