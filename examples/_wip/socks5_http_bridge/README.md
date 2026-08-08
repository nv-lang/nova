<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# `socks5_http_bridge` — blocked, not wired into the build (Plan 249 Ф.2)

`main.nv` here is the CONNECT-path implementation of Plan 249's
HTTP-to-SOCKS5 bridge (`docs/plans/249-socks5-http-bridge-example.md`),
written and manually smoke-tested (config validation, accept loop, `501`,
`502`, `431` all confirmed against a real running binary). It is **not**
wired into `examples/nova.toml`'s `[dependencies]` and does not compile as
part of the examples build. Two blockers, found and characterized during
this window (2026-08-08):

1. **`nova-socks` has no published tag.** `socks = { git = "...",
   version = "0.1" }` cannot resolve — `nova.lock.toml`'s version
   resolution reads the DECLARED `[dependencies]` form regardless of any
   active `nova.override.toml` `[replace]` (D420 §4: override only swaps
   the source used to load code, AFTER lock sync; it does not exempt a
   dependency from needing a real matching git tag to lock at all). Adding
   the dependency to the committed `examples/nova.toml` before a tag
   exists breaks the WHOLE examples package's build/lock — exactly the
   №444/CI incident (`polaris`, fixed by cutting `v0.1.0`) repeated.
   Fix: cut `nova-socks` `v0.1.0` (tests green, already pushed to all 3
   mirrors) — an owner decision (plan §3.1 names a FUTURE `HttpClient`
   consumer as the anticipated trigger, but this example is arguably that
   "first stable consumer" already).

2. **`pipe_bidirectional`'s exact pattern (as designed — into_split() on
   both streams, two sibling `spawn consume` fibers cross-pumping, a
   shared `CancelToken` woken on EOF/error) crashes the native runtime.**
   Repro (isolated `nova test`, no bridge/SOCKS5 involved — two real TCP
   loopback connections, split+cross-pump+one real echo+one real close):
   `Assertion failed: 0, file
   compiler-codegen/nova_rt/libuv/src/win/core.c, line 694` (the endgame
   handler's `default: assert(0)` in `uv__finish_close`/its
   dispatch — an unrecognized/corrupted `uv_handle_t.type` reaching
   handle-endgame processing). Reproduced 4/4 runs of one isolated
   variant; a second isolated variant crashed the SAME way. Reproduces
   BOTH with and without the Ф.0-а `TcpWriteHalf.close()` FIN fix (commit
   `8ea6472b9`) — NOT introduced by that fix, though the exact failure
   presentation differed slightly without it (plain `RUN-FAIL` with no
   captured assertion text vs. the assertion above with it — inconclusive
   whether that's the same crash surfacing differently or two distinct
   issues; not chased further this window). Not something this window
   should attempt to fix — Vela/M:N runtime territory
   (`docs/dev/mn-coding-conventions.md`), the class of bug that took
   sustained, dedicated investigation for №390.

Until both clear, this file stays here (unbuilt, per the `_wip/`
convention — see `examples/_wip/README.md`). When it moves back to
`examples/net/socks5_http_bridge/`: also add `socks = { git = "...",
version = "x.y" }` to `examples/nova.toml`, a `nova.override.toml`
`[replace]` entry for local dev, `README.md`/`README.ru.md` for the
example itself (plan 249 §4 Ф.2 checklist), and Ф.3 (plain-HTTP path —
currently an honest `501 Not Implemented` stub).
