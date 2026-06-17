# Plan 91.15 Acceptance Criteria — std/net API Polish

Spec: **D302** in `spec/decisions/04-effects.md` (net-polish + error contract +
effect-naming convention). Blocking retraction recorded in D50 banner,
`spec/decisions/06-concurrency.md`.

## P0: Blocking removal
- [x] `blocking{}` expression is a compile error (`[D172-block-form-removed]`,
      from Plan 113; the checker arm was collapsed to a plain block walk).
- [x] `Blocking` effect removed from all std/net function signatures
      (`tcp.nv`, `udp.nv`: `accept`/`connect`/`write`/`read`/`write_all`/
      `send_to`/`recv_from` + split-half methods). I/O now carries only
      `TcpNet`/`UdpNet` and parks inside the effect handler.
- [x] `Blocking` removed as a recognized effect/type name in the compiler
      (`realtime_suspend_effect()` + builtin effect-name set,
      `compiler-codegen/src/types/mod.rs`).
- [x] `#blocking fn` attribute (on functions) retained — untouched in the parser.
- [x] Negative coverage kept (`negative_capability/blocking_*.nv`): expect
      `D172-block-form-removed`; all PASS.

## P1: API improvements
- [x] `NetError.Eof` returned when a TCP peer closes the connection
      (covered by `net_eof_semantics_slow`).
- [x] `NetError @to_str()` returns a human-readable string for all variants
      (covered by `net_error_to_str`).
- [x] `SocketAddr.ip()` works; `host_str()` is gone (positive `net_ip_method`;
      negative `net_host_str_removed_neg` proves `host_str` is removed).
- [x] `TcpStream.write_all()` writes the whole buffer or errors
      (C-backed `uv_write`; covered by `net_write_all_mock`).

## P2: New errors + spec
- [x] `NetError.PermissionDenied` for `UV_EACCES` — `_nova_net_uv_err` switch
      maps to canonical `NOVA_NET_MSG_PERMISSION_DENIED`; classified by
      `net_error()` in `tcp.nv` (covered by `net_permission_denied`).
- [x] `NetError.ConnectionReset` for `UV_ECONNRESET` — canonical
      `NOVA_NET_MSG_CONNECTION_RESET`; distinct from `BrokenPipe`
      (covered by `net_connection_reset`).
- [x] TCP-split spec documented — **D301** (not D297, which is taken by LSP Rename).
- [x] API-polish spec documented — **D302** (not D298, which is taken by UDP split).

Architecture note: net errors reach Nova as `str` (V1 string-erased), classified
in `net_error()`. The UV→variant mapping is a string-normalization switch in
`net.c`, not a numeric error-code path. Bare `NetError.X` literals erase to
`nova_int` under V2-net codegen, so the new tests inspect variants via the proven
`to_str()` path rather than `match` on a bare literal (pre-existing V2 limitation).

## Tests (`nova_tests/plan91_15`, `--include-slow`) — 7/7 PASS
- [x] `net_error_to_str`, `net_ip_method`, `net_host_str_removed_neg`,
      `net_write_all_mock`, `net_eof_semantics_slow`, `net_permission_denied`,
      `net_connection_reset`.
