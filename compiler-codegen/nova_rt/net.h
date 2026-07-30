#ifndef NOVA_RT_NET_H
#define NOVA_RT_NET_H

/* Plan 183 Ф.1 / Plan 182 Ф.1: nova_rt/net.c — async TCP/UDP/DNS stdlib substrate
 * (file renamed from net2.c/net2.h once the legacy std/net substrate was removed).
 *
 * ONE layer of FFI (D407): public C functions are plain `net_*` with
 * C-ABI signatures (D282 rule 2) — scalars, pointer+length, out-parameters,
 * return codes. NO `nova_str` and NO `NovaRt_*_method_*` mangling-imitators in
 * the transport (that was Д1 in Plan 183). The Nova types (TcpStream,
 * SocketAddr, …) and all logic live in `.nv` on top of `extern "C"` (model:
 * std/fs on uv_fs_*).
 *
 * ZERO-COPY transport (D407 §2а, model std/fs `fs_read`/`fs_write`):
 *   - read:  net_tcp_read(h, buf, cap) — libuv's alloc_cb hands the caller's
 *            buffer slice straight to the kernel; read_cb reports n. No malloc,
 *            no memcpy, no nova_alloc in the hot path.
 *   - write: net_tcp_write(h, buf, len) — uv_write points directly at the
 *            caller's []u8 memory (kept alive on the fiber stack; the
 *            conservative GC sees it). No copy.
 *
 * NO static result slots (Д2 fix): results are returned by value — return code
 * (int/int64, <0 = -UV code) + out-parameters (NovaNetAddr* sender, dns array).
 * Error text is built on the Nova side from the code via net_strerror().
 * Invariant: grep -E "__thread|__declspec\(thread\)" net.c == 0.
 *
 * Park/wake/cancel: park slot lives in the supervised scope, not in the OS
 * thread → M:N-safe. Cross-thread uv_close routed via nova_loop_defer_close
 * (Plan 83.10.2).
 *
 * SocketAddr = value-record (NovaNetAddr): address is DATA (<=16 bytes + port +
 * family kind), not a handle. Handles (listener/stream/udp) are opaque `void*`.
 *
 * Plan 182 Ф.1: the pre-D407 legacy std/net substrate (old net.c/net.h, one
 * FFI generation back) was removed and this file promoted to net.c/net.h in
 * its place. All internal helpers here are `static`.
 */

#ifndef NOVA_USE_LIBUV
#  error "Plan 183: NOVA_USE_LIBUV required for std/net."
#endif

#include <uv.h>
#include <stdint.h>
#include "nova_rt.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ─── NovaNetAddr — value-record socket address (mirrors SocketAddr value) ────
 * `bytes` holds the raw network-order address (IPv4 in the first 4 bytes, IPv6
 * across all 16). `family` is 4 (IPv4) / 6 (IPv6) / 0 (unspecified). Passed
 * across the FFI by pointer to a caller-owned / nova_alloc'd struct. */
typedef struct NovaNetAddr {
    uint8_t  bytes[16];
    uint16_t port;      /* host byte order */
    uint8_t  family;    /* 4 | 6 | 0 */
    uint8_t  _pad;
} NovaNetAddr;

static inline nova_int net_addr_size(void) { return (nova_int)sizeof(NovaNetAddr); }

/* ─── Addresses (no I/O — pure data construction / inspection) ─────────────── */

/* Constructors return a nova_alloc'd NovaNetAddr* (Nova sees it as an opaque
 * pointer in Ф.1; becomes an inline value-record in Ф.2). */
NovaNetAddr* net_addr_loopback(uint16_t port);     /* 127.0.0.1:port */
NovaNetAddr* net_addr_loopback_v6(uint16_t port);  /* [::1]:port */
NovaNetAddr* net_addr_v4(uint8_t a, uint8_t b, uint8_t c, uint8_t d,
                              uint16_t port);
/* Ф.2: construct the value-record straight into the caller's 20-byte []u8 image
 * (Nova SocketAddr owns the bytes; no nova_alloc, no C-owned handle). */
void net_addr_loopback_into(uint16_t port, uint8_t* out);
void net_addr_loopback_v6_into(uint16_t port, uint8_t* out);
void net_addr_v4_into(uint8_t a, uint8_t b, uint8_t c, uint8_t d,
                           uint16_t port, uint8_t* out);
/* [M-socket-addr-port-only-form]: "any interface" wildcard addresses
 * (0.0.0.0:port / [::]:port — Go/nginx `:port` convention), NOT loopback.
 * v4 wildcard needs no dedicated C entry point (`net_addr_v4_into(0,0,0,0,
 * port, out)` already builds it structurally); v6 wildcard needs `uv_ip6_addr`
 * on "::", mirroring `net_addr_loopback_v6_into`. */
void net_addr_any_v6_into(uint16_t port, uint8_t* out);
/* Parse "host:port" from (s,len). Fills *out on success. Returns 0=OK,
 * 1=invalid address, 2=invalid port. No TLS. */
nova_int          net_addr_parse(const uint8_t* s, nova_int len, NovaNetAddr* out);
uint16_t     net_addr_port(const NovaNetAddr* a);
nova_bool          net_addr_is_v4(const NovaNetAddr* a);
nova_bool          net_addr_is_v6(const NovaNetAddr* a);
/* Format the host IP text into the caller's buffer; returns the text length
 * (bytes) written (<= cap). Zero intermediate allocation. */
nova_int      net_addr_ip(const NovaNetAddr* a, uint8_t* buf, nova_int cap);
/* Format "host:port" (v6: "[host]:port") into the caller's buffer. */
nova_int      net_addr_to_str(const NovaNetAddr* a, uint8_t* buf, nova_int cap);

/* Format the human-readable text for a UV error code into the caller's buffer;
 * returns the length written. Replaces the net.c net_last_error() TLS slot. */
nova_int      net_strerror(nova_int code, uint8_t* buf, nova_int cap);

/* ─── TCP ─────────────────────────────────────────────────────────────────────
 * Handles are opaque `void*`. Hot-path read/write return int64 (>=0 bytes,
 * 0 = EOF on read, <0 = -UV error code). Cold-path constructors return the
 * handle or NULL; on NULL the UV error code is written to *out_err (if non-NULL). */

void*    net_tcp_listen(const NovaNetAddr* addr, nova_int backlog, nova_int* out_err);
void*    net_tcp_accept(void* lst, nova_int* out_err);   /* parks fiber */
void*    net_tcp_connect(const NovaNetAddr* addr, nova_int* out_err); /* parks */
nova_int  net_tcp_read(void* s, uint8_t* buf, nova_int cap);   /* parks; ZERO-COPY */
nova_int  net_tcp_write(void* s, const uint8_t* buf, nova_int len); /* parks; ZERO-COPY */
nova_int      net_tcp_shutdown(void* s);                 /* half-close write side */
void     net_tcp_close(void* s);                    /* refcount-aware */
void     net_tcp_mark_split(void* s);               /* refcount = 2 (split) */
uint16_t net_tcp_local_port(void* s);
uint16_t net_tcp_peer_port(void* s);
void     net_tcp_local_addr(void* s, NovaNetAddr* out);
void     net_tcp_peer_addr(void* s, NovaNetAddr* out);
void     net_tcp_set_nodelay(void* s, nova_bool on);
void     net_tcp_set_keepalive(void* s, nova_bool on);

uint16_t net_listener_local_port(void* lst);
void     net_listener_local_addr(void* lst, NovaNetAddr* out);
void     net_listener_set_reuse_address(void* lstv, nova_bool on);
void     net_listener_close(void* lst);

/* ─── UDP ─────────────────────────────────────────────────────────────────── */

void*    net_udp_bind(const NovaNetAddr* addr, nova_int* out_err);
/* send_to: bytes straight from caller's []u8; parks until sent. n or <0. */
nova_int  net_udp_send_to(void* sock, const uint8_t* buf, nova_int len,
                              const NovaNetAddr* addr);
/* recv_from: datagram written straight into caller's buf (ZERO-COPY); sender
 * address filled into *sender (the one named OS-transfer). Returns n or <0. */
nova_int  net_udp_recv_from(void* sock, uint8_t* buf, nova_int cap,
                                NovaNetAddr* sender);
uint16_t net_udp_local_port(void* sock);
void     net_udp_local_addr(void* sock, NovaNetAddr* out);
void     net_udp_close(void* sock);

/* ─── DNS ─────────────────────────────────────────────────────────────────────
 * Resolve (host,len):port in ONE getaddrinfo call. Parks the fiber. On success
 * sets *out_arr to a nova_alloc'd array of exactly `count` NovaNetAddr images
 * (ownership to the caller — the single named addrinfo→array OS-transfer) and
 * returns the count (>=1). Returns -1 on error (UV code → *out_err). No TLS. */
nova_int  net_dns_lookup(const uint8_t* host, nova_int host_len, uint16_t port,
                             NovaNetAddr** out_arr, nova_int* out_err);
/* Copy the i-th image out of a DNS result array into a caller []u8 image. */
void      net_addr_copy_at(const NovaNetAddr* arr, nova_int i, uint8_t* out);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_NET2_H */
