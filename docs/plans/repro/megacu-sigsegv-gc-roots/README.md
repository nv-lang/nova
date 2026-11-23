# Repro: merged-CU intermittent SIGSEGV — lost GC roots via unchained `GC_push_other_roots` (№TBD-1) + illegal `GC_unregister_my_thread` after `GC_DUPLICATE` (№TBD-2)

Base: `ebbc2d8cd` (`origin/integrate/wake-loss-320-b`), Windows, MSVC/clang,
vcpkg `gc.lib` (GC_THREADS, GWW_VDB, THREAD_LOCAL_ALLOC, PARALLEL_MARK).
Date: 2026-11-23, worktree `codex-segv`, branch `codex/segv-gc-register-dup`.

## Defect №TBD-2 — deterministic C harness (RED/GREEN both ways)

`repro_gc_dup.c.txt` (buggy) / `repro_gc_dup_fixed.c.txt` (guarded unregister) — `.c`-суффиксы спрятаны в `.txt` по правилу улик №695 (глобальный `.gitignore` игнорирует `*.c`).

- 16 `CreateThread` threads (gc.h maps `CreateThread` to `GC_CreateThread`,
  so every thread is ALREADY registered implicitly); each calls
  `GC_register_my_thread()` → prints rc (observed: `GC_DUPLICATE` = 1 for
  all 16), allocates, then calls `GC_unregister_my_thread()`.
- `GC_enable_incremental()` on.

Build (MSVC, same `gc.lib` as Nova test binaries):

```bat
call "<vcvars64.bat>"
set GCLIB=<NOVA_GC_LIB_DIR>
set GCINC=%GCLIB%\..\include
cl /nologo /W0 /MT /O2 /DGC_THREADS /I%GCINC% repro_gc_dup.c.txt ^
   /Fe:repro_gc_dup.exe /link /LIBPATH:%GCLIB% gc.lib
```

Measured (10 runs each, same machine):

| variant | result |
|---|---|
| buggy (unconditional unregister) | **10/10 SIGSEGV** |
| guarded (`if (rc == GC_SUCCESS)`) | **10/10 exit 0** |

In the current Nova runtime the raw register returns `GC_SUCCESS` for all
worker/driver threads (measured 17/17 rc=0: they are created via
libuv's own `CreateThread`, which the gc.h macro does NOT intercept),
so this defect is latent for Nova today but activates silently if
nova_rt ever creates threads through the intercepted path.

## Defect №TBD-1 — lost roots (mechanism, static + stress)

`fiber_arena_win.c` published `_nova_fw_gc_push_other_roots` through
`GC_set_push_other_roots` without chaining. The default callback on
`GC_WIN32_THREADS` is `GC_push_all_stacks()` — the only scanner of the
native stacks of every GC-registered thread. The driver thread never
creates a fiber arena and a worker has none until its first `mco_create`
(lazy arenas, D137), so their native stacks were never pushed →
premature collect under `NOVA_MAXPROCS=16` → torn live objects
(SIGSEGV in free-list walk, null HashMap bucket, torn HTTP response).

Fix: `GC_get_push_other_roots()` before `GC_set_push_other_roots`, call
the predecessor first inside the callback.

## Stress protocol (`stress.sh`)

Merged CU: `nova test spec_tests --filter a_q3_println_debug_record
--keep-artifacts` (folder-module peer merge — the exe contains the whole
`spec_tests/conformance` corpus, 3298 embedded test blocks), then run the
kept exe directly:

```sh
NOVA_MAXPROCS=16 NOVA_GC_INCREMENTAL=1 NOVA_DIAG_SEGV=1 ./a_q3_println_debug_record.exe
```

A/B discipline: SHA-256 every exe variant (old runtime vs conditional
unregister vs chained callback) before comparing; ≥8 concurrent copies ×
12 rounds reproduces the load-sensitive modes; solo sequential runs are
the clean-lane criterion (5 × 60 = 300 target).

## Known separate defect found while stress-testing (№TBD-3, [M-net-stream-cross-worker-reset])

`m_boxcap_detach_cancel_token_pos` fails ~1/300 solo, ~11/96 under 8-way
load, identically on old/conditional-unregister/chained runtimes (not the
GC defects) and on a solo-file twin; raising the supervised timeout
5s→60s does not help. Instrumented twin shows connect SUCCEEDS, the
client writes, then read fails with **"connection reset by peer"** (the
server closes the connection before responding). At `NOVA_MAXPROCS=1`
the twin is 96/96 green under the same load → M:N-scheduler dependent.
Hypothesis: the detached response-writing fiber runs on a worker whose
libuv loop does not own the accepted TcpStream handle (D98 per-worker
loops; libuv handle ops are not thread-safe) → RST. Dedicated window
required; tracked as №TBD-3 in 221.1-bug-sweep.md.

## Final series (chained runtime, exe
SHA-256 3130aedbacc502181dd260f2ac8fdce64d71af6d8d58f32e414a707817fc64cc)

300 sequential solo runs, `NOVA_MAXPROCS=16 NOVA_GC_INCREMENTAL=1`:
**0 SIGSEGV**, 299×exit 0, 1×exit 1 = the m_boxcap fixture flake above
(run 298), i.e. no SIGSEGV-mode run in the series; the residual exit-1
is the separately-filed fixture race, not a GC/M:N defect.


## Follow-up window (codex/tcp-close-reset, base c72fea17c): m_boxcap RST root cause (№TBD-3)

Single-diff A/B on the merged-conformance-base build; the ONLY difference
between the two builds is `compiler-codegen/nova_rt/net.c` (graceful TCP
close: `uv_shutdown` on the owner loop, `uv_close` in its callback).

| exe | net.c | SHA-256 |
|---|---|---|
| mini-A-base.exe | base (immediate close) | `7578d84804ac25dbe672f87455b13d8efae5a8e3c877f793bec3ab2615d63dda` |
| mini-B-graceful.exe | graceful close | `65807a90abae1ef9757e86346dda77bd16731b7f2f08d0d355b984ae4dd3a9b1` |

Failure classification (instrumented twin, `NOVA_GC_INCREMENTAL=1`,
`NOVA_MAXPROCS=16`): EVERY failure in every series is
**connect OK / write OK / read fails "connection reset by peer" /
got.len=0** — never a connect error, never a write error, never clean
EOF, never partial response.

| series | base (immediate close) | graceful close |
|---|---|---|
| 8 processes x 12 rounds | 7/96 fail (all RST-on-read) | **96/96 pass** |
| 300 solo runs | 4/300 fail | **300/300 pass** |

Root cause: `net_tcp_close` issued `uv_close` immediately after the
response write completed. On Windows, `closesocket` with unread inbound
bytes (the fixture server never reads the client's GET) sends RST,
destroying the still-undelivered outbound response. The
`NOVA_MAXPROCS` dependence is timing (when close lands relative to
response delivery), not handle ownership — the cross-worker hypothesis
in the original marker is refuted.


## Residual after the shutdown-first close: drain-before-close (final fix)

The acceptor's independent run found 1/300 residual failure (LENOVO-RYZEN);
reproduced locally as 3/500 under 20-way load — ALL still "connection reset
by peer". Reason: `uv_shutdown` sends the FIN, but `closesocket` with unread
inbound bytes still RSTs and the RST aborts the outbound path — the
not-yet-delivered tail of the response dies. Final fix: after the FIN,
DRAIN inbound to EOF (bounded by a 300 ms idle timer and a 1 MiB cap; a
busy user read falls back to an immediate close) before `uv_close`. With
no unread inbound bytes there is no RST, and TCP guarantees delivery of
everything already sent.

| series (mini-C-drain, SHA-256 `a3adb3aa2434…b4`) | result |
|---|---|
| 500 solo (`NOVA_GC_INCREMENTAL=1 NOVA_MAXPROCS=16`) | **0 failures** |
| 300 under 20-way concurrent load | **0 failures** |
| smoke, 20-way × 12 | 240/240 |

Reference points on the same protocol: base (immediate close) 11/396,
graceful-close-only 3/500 under load.


## Lifetime follow-up (041a1f6c3 -> redesign): embedded drain timer UAF

The first drain implementation EMBEDDED the drain `uv_timer_t` in
`NovaNet2Stream`. libuv's endgame queue PREPENDS, so closing the stream
after the timer makes the stream's close callback (which frees the
struct) run BEFORE the timer's own endgame completes -- a use-after-free
the acceptor flagged. A poison-on-free C harness
(`close_order_test.c.txt`, builds by #include-ing net.c; sources stored
as .c.txt per the evidence-file rule) confirmed it deterministically:

- RED (041a1f6c3): AV inside `uv_run`'s endgame, faulting instruction
  resolved via the linker map; the freed stream page is read after the
  free while the timer's endgame unwinds.
- GREEN (redesign): the drain timer moved into its OWN heap allocation
  (`NovaDrainCtx`); each close callback frees only its own allocation;
  the stream is held by one extra reference until the timer's close
  callback releases it; `uv_timer_init`/`uv_timer_start`/`uv_read_start`
  failures complete the close without waiting for EOF and release
  everything already created. Harness: stream freed exactly once, after
  both close callbacks, no post-free fault.

Nova-level stress on the redesigned runtime (mini-D, SHA-256
`eb7aa386585c00184e44abd8ed5e83d2640b6ab3d79400786f869804de25eac6`):
**500 solo + 300 under 20-way load, 0 failures**, same protocol as above.


## Acceptance-review fix: restore stream handle.data before the drain close

The acceptor's diff audit caught it: the drain pointed the stream's
`uv handle.data` at `NovaDrainCtx` (so drain read callbacks find their
context), but `_nn2_drain_finish` queued `uv_close` on the stream
WITHOUT restoring it -- and `_nn2_stream_close_cb` reads `h->data` as
`NovaNet2Stream*`. The first harness run MISSED this because its free
counter did not distinguish the stream allocation from the context
allocation (false GREEN); after switching the counter to
pointer-identity of the stream allocation the defect shows as
`stream_frees = 0` (the stream is never freed, the context refcount is
corrupted instead).

Fix: `_nn2_drain_finish` restores `s->handle.data = s` BEFORE `uv_close`
is queued on the stream. All other stream-close paths keep
`handle.data == s` throughout (audited).

- RED unchanged: same harness on 041a1f6c3 reproduces the AV inside
  `uv_run`'s endgame.
- GREEN: all three scenarios (normal graceful close, uv_timer_init
  failure, uv_timer_start failure) report `stream_frees == 1`.
- Nova-level stress after the fix (mini-E, SHA-256
  `e112a05637b2723cc4805aac156ab8221491516cc5df0dcf063b171f1b8728d1`):
  500 solo + 300 under 20-way load, 0 failures.
