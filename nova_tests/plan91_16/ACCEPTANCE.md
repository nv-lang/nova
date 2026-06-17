# Plan 91.16 Acceptance Criteria — TcpReadHalf / TcpWriteHalf

`TcpStream.split() -> (TcpReadHalf, TcpWriteHalf)` — the TCP analogue of the
UDP split (Plan 166 / D298). Spec: **D301** in `spec/decisions/04-effects.md`.

## Functional
- [x] `TcpStream.split()` returns `(TcpReadHalf, TcpWriteHalf)` consume values.
- [x] Concurrent read + write from two separate fibers works correctly
      (independent park slots: `read_scope`/`read_slot` vs `write_scope`/`write_slot`
      in `NovaRt_TcpStream`; the un-split stream shared one `op_scope`/`op_slot`,
      which corrupts under concurrent r/w — this is the core correctness fix).
- [x] `TcpReadHalf.close()` and `TcpWriteHalf.close()` each decrement an atomic
      `split_refcount` (`__atomic_sub_fetch`).
- [x] Underlying socket `uv_close` only fires when refcount reaches 0
      (after BOTH halves close).
- [x] `write_all()` writes the whole buffer or errors (C-backed: `uv_write`
      queues the entire buffer atomically — closes `[M-91.15-write-all]`).
- [x] `read()` returns `Err(NetError.Eof)` when the peer closes the connection.

## Type Safety
- [x] `TcpStream` is consumed by `split()` — cannot be used after split
      (D131 use-after-consume fires; covered by negative test).
- [x] `TcpReadHalf` exposes only read-side methods (no `write`).
- [x] `TcpWriteHalf` exposes only write-side methods (no `read`).
- [ ] Double-close / use-after-close of a half as a **compile** error —
      NOT expressible in V1: the parser rejects `consume (rd, wr) = s.split()`
      ("unexpected `consume`"), and `mut`-bound tuple elements aren't
      double-consume tracked. Runtime atomic refcount protects these paths
      instead. Filed `[M-91.16-tuple-consume-binding]`; documented in D301.

## Effect System
- [x] `TcpNet` effect extended with `split_stream`, `write_all`,
      `read_half_*`, `write_half_*` ops (`std/net/effect.nv`).
- [x] Mock handler (`mock_tcp_net`) covers all new operations (`std/net/mock.nv`).
- [x] `real_tcp_net()` handler covers all new operations (`std/net/tcp.nv`).

## Tests (`nova_tests/plan91_16`, `--include-slow`) — 3/3 PASS
- [x] `tcp_split_mock` — positive: mock dispatch through the effect vtable.
- [x] `tcp_split_echo_slow` — positive: real loopback server+client exchange via
      read/write halves (cross-fiber full-duplex).
- [x] `tcp_split_stream_after_split_neg` — negative: use-after-split fires D131.

## Regression
- `plan91_12` 26/2 — the 2 non-PASS are pre-existing and not regressions:
  `net_v2_tcp_multi_client_slow` flakes under 16-job parallel load (PASS isolated);
  `net_v2_udp_two_fiber_slow` intermittently hangs (~2/3 of runs timeout, ~1/3 PASS,
  even at `--jobs 1`) — a UDP-loopback park/wake race. UDP runtime in
  `compiler-codegen/nova_rt/net.c` is byte-identical to base `ccca04f6` (0 UDP lines
  changed) and `std/net/udp.nv` only drops the `Blocking` annotation, so this is
  unrelated to the TCP split. Tracked as `[M-net-udp-two-fiber-race]`.
- `plan83_10` 20/0, `plan100_4_2` 9/0 — shared runtime / consume-checker unaffected.
