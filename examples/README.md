<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# examples/ — showcase

Eight programs, ordered by growing scope. Each builds standalone with
`nova build <file> --strict-effects` (server/client pairs: build-only, see
note below). Full audit — [`docs/plans/wip/197-audit-progress.md`](../docs/plans/wip/197-audit-progress.md).

| # | Path | What it shows |
|---|------|----------------|
| 1 | [`basics/hello.nv`](basics/hello.nv) | The absolute minimum: `fn main`, `println`. |
| 2 | [`getting_started.nv`](getting_started.nv) | One self-contained file — records, a sum type + `match`, a for-loop, and an algebraic effect swapped for a test handler with zero code changes. |
| 3 | [`mini_aggregator.nv`](mini_aggregator.nv) | The flagship pattern in ~30 lines: `parallel for` + `supervised(deadline:)` — concurrent fan-out with a shared time budget and honest cancellation of stragglers. |
| 4 | [`tour/`](tour/) | A guided walk through the language: [`hello.nv`](tour/hello.nv) → [`types.nv`](tour/types.nv) → [`methods.nv`](tour/methods.nv) → [`patterns.nv`](tour/patterns.nv) → [`collections_tour.nv`](tour/collections_tour.nv) → [`strings_tour.nv`](tour/strings_tour.nv) → [`errors.nv`](tour/errors.nv) → [`consume_tour.nv`](tour/consume_tour.nv) → [`modules_tour.nv`](tour/modules_tour.nv) (folder-modules, [`greeter/`](tour/greeter/)) → [`effects_tour.nv`](tour/effects_tour.nv) → [`concurrency.nv`](tour/concurrency.nv) → [`ffi_tour.nv`](tour/ffi_tour.nv). |
| 5 | [`effects/`](effects/) | The effect system beyond the tour basics: [`effects.nv`](effects/effects.nv)/[`effects_d61.nv`](effects/effects_d61.nv) (handler substitution, D61), [`spawn_demo.nv`](effects/spawn_demo.nv) (structured concurrency primitives). |
| 6 | [`net/`](net/) / [`tls/`](tls/) | Matched echo server/client pairs demonstrating the native-module dependency pattern (Plan 193/195) — plain TCP vs TLS, same shape. **`tls/echo_server.nv`/`echo_client.nv` build clean; `net/echo_server.nv`/`echo_client.nv` are currently blocked by a confirmed compiler bug** — see [Known gaps](#known-gaps) below. |
| 7 | [`ffi/`](ffi/) / [`typed_pointers/`](typed_pointers/) | Native interop: opaque pointers, typed handles, `unsafe fn`, `external fn` — the Plan 195 FFI pattern ([`ffi/ptr_basics.nv`](ffi/ptr_basics.nv), [`typed_pointers/basic_pointer.nv`](typed_pointers/basic_pointer.nv), [`typed_pointers/unsafe_fn_keyword.nv`](typed_pointers/unsafe_fn_keyword.nv)). |
| 8 | [`flagship/aggregator/`](flagship/aggregator/) | The full flagship demo (Plan 187) — the pattern from #3, at production scale: real `std.net`/`std.http`, a JSON API, load-test harness, regression corpus. Server — build-only, doesn't run to completion in a showcase check. |

## Build-only vs run

Everything above is `nova build`-clean. Programs 1-5 and 7 print
deterministic output and are safe to run in a showcase check. `net`/`tls`
pairs are a server + a client — `echo_server.nv` listens forever, so CI and
this showcase only **build** it (never run it to completion); `flagship/
aggregator/src/main.nv` is the same shape (a live HTTP server). Their build
success alone is the signal.

## Known gaps

Two files are content-clean but blocked by confirmed compiler bugs, not
authorial mistakes — documented in full in
[`docs/plans/wip/197-audit-progress.md`](../docs/plans/wip/197-audit-progress.md):

- **`net/echo_client.nv` / `net/echo_server.nv`** — link error `undefined
  symbol: Nova_TcpStream_consume_cleanup`. The generated C calls this
  symbol (from `consume stream = … ` inside a `spawn { }` block) but never
  emits a definition anywhere in the translation unit — a codegen gap in
  lowering `TcpStream`'s `@cleanup` method when the consume happens inside
  a spawned closure. Reproduces identically against the current release
  `nova.exe` in the main repo (not a worktree artifact). **This is the
  exact pattern `nova-gate.yml`'s flagship-build job targets** — the gate
  has reportedly never been verified on a live push (see Plan 197 status);
  this local repro suggests it would currently fail there too.
- **`real_world/orm_decorators.nv` / `real_world/orm_demo.nv`** — both
  content-clean (all authorial dead-syntax removed), blocked by two
  separate confirmed issues: `SyncDetach` is not implemented in the std
  runtime bootstrap (`orm_decorators.nv`), and `Repo[T].bulk_load[K]` +
  `Vec[K].map(function_value)` can't infer a C type for the closure return
  (`E7001`, class `M-196.5-b3-closure-param-bind`, `orm_demo.nv`). Neither
  file was quarantined; both are finished content waiting on a compiler/std
  fix.

There is no quarantine directory any more. `_wip/` held six files whose
concepts were worth keeping but whose content needed a clean rewrite; it
was removed on 2026-08-10 (registry #533). An exclusion with no expiry
and no owner does not preserve a concept — it hides rot behind a name.
The two concepts it held (effect density across a service's signatures,
and an unsafe-block demo) are described in the commit that removed them
and can be rewritten from that description whenever they earn the work.
Every `.nv` under `examples/` is now inside the compile gate: there is no
place left to put code that is exempt from compiling.
