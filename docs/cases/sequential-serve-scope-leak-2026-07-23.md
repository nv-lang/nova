<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Case study — `[M-sequential-serve-instances-stale-state]` (221.1 #38)

**Status: ROOT CAUSE CONFIRMED (hard evidence), FIX NOT LANDED.** Session:
ОКНО-4 (sonnet), 2026-07-23, worktree `nova-okno4`.

## Symptom (nova-http)

Two sequential `serve_router()`/`serve()` test instances in ONE process
(`nova-http` package, `rt/serve_policy_smoke.nv`): the first test passes,
every later one gets an empty/reset response. Isolated in separate
processes, both are fine.

## Minimal standalone repro (no nova-http needed)

`scratch38/repro38g2.nv` in this worktree (NOT committed to
`spec_tests/conformance` — it is RED and conformance must stay green).
Two sequential top-level tests, pure `std.net` + `std.time`:

- **Test A**: `with Fail[TimeoutError] { supervised(timeout: 3s) { spawn {
  bind; while(true) { match accept() { Ok(s) => detach{ read_attempt(s,
  now+60s) }  Err(_) => sleep(20ms) } } }  spawn { connect; write; read } } }`
  — the accept loop's SECOND `accept()` call genuinely blocks until the
  outer 3s timeout fires and cancels it. PASSES.
- **Test C** (runs second, in the SAME process): trivial single-shot
  `supervised { spawn { bind; accept(); read_attempt(stream, now+60s);
  close }  spawn { connect; write; read } }` — no loop, no `detach`, no
  cancellation of its own. **FAILS**: `read_attempt`'s inner
  `supervised(deadline: now+60s)` times out in ~10-14ms instead of running
  for up to 60s.

Confirmed (black-box, by construction) that test A's OUTER
`supervised(timeout:)` genuinely undergoing real cancellation is
**required** to poison test C — two back-to-back tests with NO real
cancellation in either (`repro38g.nv`) pass cleanly, as does a version with
`with Net = real_net()` active but no actual sockets (`repro38d.nv`), and a
version with real accept()+Time-only inner spawn (no `.share()`/read)
(`repro38e.nv`).

## Instrumented proof

Added a temporary `fprintf` at the top of `nova_supervised_run_impl`
(fibers.h, ~line 2952, since reverted — not committed) printing `q`,
`q->deadline_ns`, and `time_monotonic_ns()`. Output for the A→C run:

```
run_impl[1]: q=...D410 _dl_ns=69257480476600 now=69254492104800 remain_ms=2988   <- test A's OUTER supervised(timeout: 3s)
run_impl[2]: q=...FF150 _dl_ns=0                                                 <- test A's read_attempt (inside detach — severed, no ambient deadline, correct/benign)
run_impl[3]: q=...D210 _dl_ns=69257480476600 now=69257490462700 remain_ms=-9     <- test C's OUTER plain `supervised {}` — ALREADY EXPIRED
```

Line 3 (test C's own top-level, PLAIN `supervised {}` block, which has no
`deadline:`/`timeout:` of its own and should just inherit `_nova_main_scope`'s
`deadline_ns` — always `0`, `_nova_main_scope` never gets a `deadline:`/
`timeout:`) instead shows the **byte-identical** `_dl_ns` as test A's own
outer scope, ALREADY IN THE PAST by the time test C runs. This is only
possible if, at the moment test C's own `nova_scope_init` ran,
`_nova_active_scope` pointed directly at test A's own (by then C-stack-freed)
`NovaFiberQueue` — i.e. a **dangling scope pointer surviving across the two
test bodies**, corrupting `nova_scope_init`'s deadline-inheritance
(`q->deadline_ns = _nova_active_scope ? _nova_active_scope->deadline_ns : 0`,
fibers.h ~line 859) for the NEXT unrelated `supervised{}`/`(deadline:)` block.

## Relation to the known bug class (already partially fixed)

This is the SAME class already named and partially fixed in
`compiler-codegen/src/codegen/emit_c.rs` ("race-198 / 196.6", ~line
26970-26999, 2026-07-13): *"a dangling pointer... a dangling stack address
of the completed test's own C frame... [leaks] into a LATER test"*. The
existing fix calls `nova_runtime_reset()` (fibers.h:4295) after every test
block, which explicitly sets `_nova_active_scope = NULL` on the **calling
(driving) thread**, then restores it to `_chunk_main_scope`
(`&_nova_main_scope`).

**That fix does not close this specific repro.** `nova_runtime_reset()`
only resets the thread it runs on (the main/driving thread dispatching test
bodies). The repro's accept-loop fiber (the one that gets genuinely
cancelled) runs under the "armed" M:N runtime, quite possibly on a
**worker OS thread** — `_nova_active_scope` is `__thread`-qualified
(per-OS-thread, not a single process-wide global, not per-fiber
intrinsically — fibers.h ~line 809/2147). Nothing in `nova_runtime_reset()`
touches worker threads' own copies of this TLS slot, nor any other
worker-local bookkeeping that might route through it after a fiber that
observed real scope cancellation dies.

The exact write site that leaves a worker thread's `_nova_active_scope` (or
some other TLS this ends up threading through) pointing at the dead scope
was **not pinned down** in this session — every code path read
(`nova_supervised_run_impl`'s own internal restore at line 3155,
`_worker_run_one_fiber`'s save/restore around `mco_resume`, the codegen
emitted `emit_supervised` restore at `emit_c.rs:13375`) looks, on static
reading, like it *should* already be correct; the corruption must come from
either a genuine race (the cancelled fiber's own worker-thread cleanup not
fully synchronized with the driving thread's `nova_runtime_reset()`/next-test
start) or a path not covered by this read-through. Needs either a debugger
session or bracketed instrumentation on `_worker_run_one_fiber`'s own
TLS save/restore plus the worker pool's idle/reuse path — see
`docs/debugging-races.md` for the state-dump method used previously for this
class (`reference-mn-race-case-study` precedent).

## What's NOT the cause (ruled out empirically this session)

- Semaphore/admission-gate state (`repro38f.nv`: real accept+share+read
  racing `supervised(deadline:)`, WITHOUT the loop/timeout/detach shape —
  passes twice in a row).
- Router/`handler_fn` dispatch (bypassed entirely with a raw
  `fn([]u8)->ServerResponse` handler against nova-http's own `serve()` —
  bug reproduces identically).
- An "orphan fiber spins forever" leak: the accept-loop's own retry-fiber
  was confirmed (via a print in the `Err(_)` arm) to die cleanly, ONE retry
  after the outer cancellation — it does not survive into the next test.
- A literally-expired deadline being computed fresh and wrong (e.g. a stale
  mocked clock): no mock clock is used anywhere in the repro; `Monotonic.now()`
  is the real `uv_hrtime()`-backed clock throughout.
- `detach`'s own deadline-severing (`inherited=0` for a detached
  connection handler) — that is the CORRECT/benign behaviour (matches test
  A's own successful read), not the bug.

## Next steps for whoever picks this up

1. Reproduce with `scratch38/repro38g2.nv` (or regenerate from this file's
   description — 2 tests, ~90 lines, no nova-http needed).
2. Bracket `_worker_run_one_fiber` (runtime.c ~2174-2260)'s own
   save/restore of `_nova_active_scope` with per-worker-thread-id
   instrumentation, plus a print at the exact `nova_scope_init` call for
   test C's outer scope, to catch which thread's TLS is actually being read
   and by which code path it got the stale value.
3. Once pinned down, the fix is very likely either (a) extend
   `nova_runtime_reset()` to broadcast a reset to all worker threads
   between test-chunk-boundaries (heavier, but matches the existing
   single-thread-reset's own stated intent), or (b) close whatever specific
   worker-thread code path fails to overwrite `_nova_active_scope` with the
   fiber's own bound `_nova_fiber_scope` before touching it.
4. Independently — `serve()`'s own accept-loop `Err(_) => { …; sleep(20ms)
   }` arm (nova-http, `servernet/serve.nv`) does not distinguish
   `NetError.Cancelled` from a transient accept error; on real cancellation
   it retries once more before the cooperative-cancel throw kills the
   fiber. Not the root cause of #38 (ruled out — the fiber dies either way,
   see above), but worth tightening in nova-http anyway (`Err(NetError.
   Cancelled) => running = false` instead of blind retry) as defense in
   depth / to avoid the one extra 20ms retry cycle.
