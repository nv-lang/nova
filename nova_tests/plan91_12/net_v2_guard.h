/* Plan 91.12: V2 net guard + literal-name FFI forward declarations.
 *
 * Force-included BEFORE nova_rt/nova_rt.h via nova.toml [ffi] c_shims.
 *
 * Two jobs:
 *   1. Pull in worktree net.h FIRST so its include-guard (NOVA_RT_NET_H)
 *      fires and main repo net.h is skipped when nova_rt.h later tries
 *      to #include "net.h".  Worktree net.h has NovaRt_* (not Nova_*)
 *      struct names — no conflict with V2 codegen output.
 *   2. Define NOVA_NET_V2=1 so the remaining #ifndef NOVA_NET_V2 guard
 *      in main net.h also suppresses any V1 declarations that slip through
 *      (belt + suspenders).
 *
 * V2 value-record proxy structs (Nova_T { intptr_t handle; }) are defined
 * below so codegen-emitted `->handle` accesses compile correctly with the
 * handle-ABI types defined here instead of the uv-heavy V1 definitions. */

#pragma once

/* Belt: force worktree net.h first — sets NOVA_RT_NET_H guard,
 * blocking main repo net.h later when nova_rt.h runs #include "net.h". */
#include "D:/Sources/nv-lang/nova-p91-12/compiler-codegen/nova_rt/net.h"

/* Suspenders: also define NOVA_NET_V2 to suppress any residual V1 decls. */
#ifndef NOVA_NET_V2
#define NOVA_NET_V2 1
#endif

/* V2 net value-record proxy structs.
 * Layout: single intptr_t field `handle` = cast-to-int C pointer.
 * These replace the V1 uv-heavy structs from main net.h. */
typedef struct Nova_SocketAddr  { intptr_t handle; } Nova_SocketAddr;
typedef struct Nova_TcpListener { intptr_t handle; } Nova_TcpListener;
typedef struct Nova_TcpStream   { intptr_t handle; } Nova_TcpStream;
typedef struct Nova_UdpSocket   { intptr_t handle; } Nova_UdpSocket;

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
