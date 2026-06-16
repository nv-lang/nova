/* Plan 91.12: V2 net guard + literal-name FFI shims.
 *
 * Force-included BEFORE nova_rt.h via nova.toml [ffi] c_shims.
 * nova_rt.h already includes net.h with V2 entry-points (post-merge),
 * so this file only provides:
 *   1. NOVA_NET_V2=1 signal (V2 algebraic-effect API in use).
 *   2. Forward declarations for NovaValue_* proxy structs.
 *   3. Macro aliases bridging codegen call-site names to definition names. */

#pragma once

/* V2 API signal — consumed by any code that still guards on this. */
#ifndef NOVA_NET_V2
#define NOVA_NET_V2 1
#endif

/* V2 net value-record proxy structs.
 * Codegen emits NovaValue_<T> for value-records and defines them in the TU.
 * We only need forward declarations here so that net.h entry-point signatures
 * using void* (CSocketAddr etc.) compile before codegen defines the structs. */
struct NovaValue_SocketAddr;
struct NovaValue_TcpListener;
struct NovaValue_TcpStream;
struct NovaValue_UdpSocket;

/* Alias Nova_T_static_Variant(data) → nova_make_T_Variant(data).
 * Codegen emits Nova_NetError_static_IoError/InvalidAddr calls but does
 * not emit their definitions (emits nova_make_* instead).  These macros
 * redirect to the definitions that codegen does emit in the same TU. */
#define Nova_NetError_static_IoError     nova_make_NetError_IoError
#define Nova_NetError_static_InvalidAddr nova_make_NetError_InvalidAddr

/* Method overload name aliases.
 * Call sites emit `Nova_T_method_name(...)` (no param-type suffix),
 * but codegen definitions use `Nova_T_method_name__param_types(...)`.
 * These macros bridge the mismatch for methods with parameters. */
#define Nova_UdpSocket_method_send_to \
    Nova_UdpSocket_method_send_to__nova_str_Nova_SocketAddr_p
#define Nova_UdpSocket_method_recv_from \
    Nova_UdpSocket_method_recv_from__nova_int
#define Nova_TcpStream_method_write \
    Nova_TcpStream_method_write__nova_str
