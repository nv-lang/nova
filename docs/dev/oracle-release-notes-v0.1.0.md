<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova Oracle 0.1.0 — release notes (alpha draft)

**Nova Oracle 0.1.0** is an early alpha release. Oracle compiles
to C and then to a native binary — there is no interpreter. Every
function's side effects (`Db`, `Net`, `Io`, `Time`, `Fail`, ...) are part of
its type signature and checked by the compiler. Memory is managed by a
Boehm GC by default; for resources that need deterministic cleanup,
`consume`-typed ownership guarantees an exit-time callback with no GC in
the loop. Concurrency is structured (`spawn`, `parallel for`, `supervised`)
on an M:N work-stealing fiber scheduler, with no `async`/`await` split.

This is an early alpha snapshot of Oracle, not a finished 1.0. The language
surface and APIs may change, and compatibility is not guaranteed. See
"Known limitations" below before depending on it for anything beyond
experimentation.

## Highlights

### Language

- **Effects in function signatures** (`fn f(...) Db Net Fail -> T`):
  side effects a function performs are visible in its type; a handler is
  substituted via `with Handler = ... { body }`, which is also Nova's
  answer to mocking in tests — swap a handler, no mocking framework.
- **`consume`/ownership with `defer`**: `defer { ... }` runs at every scope
  exit (including `throw`/`panic`), LIFO across multiple `defer`s in the
  same scope. A `consume` type with `@cleanup` is affine for a named bare
  binding (`consume x = ...`): if still live at scope exit, the compiler
  inserts cleanup (D432). Cleanup effects propagate to the containing
  function; a `Fail` effect must be declared where required. Other binding
  forms and types without `@cleanup` retain explicit-consumption rules;
  cleanup is not recursively synthesized for arbitrary aggregates.
- **`protocol`s** — structural interfaces, opted into explicitly via
  `#impl(...)`, distinct from effects (an effect is a swappable
  implementation of "how"; a protocol is a fixed contract of "what a value
  can do").
- **Generics**, **sum types** (`type X enum A | B | C`), **pattern
  matching** (`match`, guards, the `if <Pattern> = expr { } else { }`
  if-let form), **records** with property-methods-by-arity
  (`@x() -> T` reads, `mut @x(v T) -> @` writes and returns the receiver).
- **Structured concurrency**: `spawn`, `supervised`, `supervised(deadline:
  ...)`, `parallel for`, channels, `select` — on an M:N work-stealing
  scheduler with a per-worker libuv event loop and preemption. No function
  colour: the same function works in a sequential loop or in `parallel
  for` without a signature change.
- **Contracts** (`requires`/`ensures`/`old`/`result`/`invariant`/
  `reads`/`modifies`/`decreases`/`ghost let`/`assume`/`assert_static`),
  optional and gradual: without them the code behaves like an ordinary
  imperative language; with them the compiler attempts static proof and
  falls back to a stripped-in-release runtime check for what it can't
  prove.
- Folder-modules (a module is a single file or a folder of peer files
  sharing one namespace, Go-style), cross-file imports with cycle
  detection, file-level `#forbid Net, Fs` capability attributes.
- **Typed effect operations, no ambient special-casing.** A handler's
  operations must now declare their full signature (`now() -> Timestamp
  =>`, ...; D434) instead of leaving it inferred. The built-in `Time`
  effect is fully typed end to end (`sleep(d Duration)`, `now() ->
  Timestamp`, `now_monotonic() -> Monotonic`), and — like `Fs`/`Net` — it
  must appear explicitly in a function's effect row; the previous
  "ambient" carve-out for `Time` is gone (D62 retraction). An effect
  with one obvious default implementation can declare it once with
  `#default_handler(...)` (D431) for an opted-in effect instead of every
  call site wiring a handler by hand. Under strict effects, callers still
  declare the effect explicitly; Fs/Net/Os have not been migrated to this
  default-handler mechanism.
- **Better diagnostics for uncaught `throw`/panics (D437)**: the error
  reports the throw site plus a propagation trace through the `?`-sites it
  passed through. This is not a full call stack: the trace is bounded to 16
  entries, and the default human-readable output also has an opt-in JSON
  form (`NOVA_PANIC_FORMAT=json`, D462).
- **Consistent numeric `match` arms (D433, as amended by D491)**: without an
  expected result type, numeric arms must agree; incompatible widths or
  signedness are rejected instead of silently selecting one arm's type.
  The earlier safe-widening rule was withdrawn. An in-range unsuffixed
  integer literal can still adopt its sibling arm's type.

### Standard library

Oracle's `std` includes collections (`Vec`/`[]T` alias, `HashMap`, iterators),
IO, filesystem, path, OS, time, JSON-capable encoding, checksums,
cryptography primitives, identifiers, Unicode, text utilities, a testing
framework with deterministic handlers (e.g. a mockable clock and `Random`
seed), and the concurrency/runtime layer that backs the fiber scheduler.
Networking, TLS, HTTP, and compression are separately versioned packages.
Their availability, exact versions, and release readiness are separate from
this Oracle compiler release and must be checked in their own package sources.

- **`serde`-style field attributes** for the JSON derive: `rename`,
  `rename_all` (container-level, typo-checked at compile time rather
  than a silently-ignored magic string), `skip`, `skip_serializing_if`,
  `default` (including `default = "fn"`), and `alias` (D435) — plus
  strict-by-default rejection of unknown JSON fields, with an explicit
  opt-out (`#serde(allow_unknown)`, D436). `flatten` is designed but
  not synthesized yet and is rejected with a diagnostic. These field
  attributes currently apply to records; rich attributes on sum-variant
  record payloads remain outside this scope (D435).
- **Runtime hardening**: an intermittent, load-dependent crash in
  orphaned `detach` fibers (a use-after-return on the parent's stack)
  and a use-after-free in listener refcounting on a cancelled-then-
  retried `accept()` are both fixed. `Semaphore` gained a non-blocking
  `try_acquire_permit() -> Option[Permit]` so admission-control code can
  use a `@cleanup`-guarded `Permit` instead of a bare boolean plus a
  manual `release()` in `defer`.
- **Affine `@cleanup` (D432) is used by selected resource types**, including
  `File`, `BufWriter`, TCP listener/stream/split halves, `UdpSocket`, and
  `Permit`. It applies to the supported bare consume binding form, not to
  every pattern or aggregate; types without this protocol keep their prior
  ownership rules.
- HTTP/router and typed-extractor claims in the earlier shared draft are not
  treated as Oracle release guarantees. The current Carina release checklist
  still records extractor arities and end-to-end acceptance as open package
  work; verify the separately versioned package at its own release before
  advertising those capabilities.

### Tooling

- **`nova` CLI**: `nova build` (Nova → C → native binary), `nova check`
  (type-check only), `nova test` (runs in-file `test { ... }` blocks,
  JSON/JUnit output, retries, filtering), `nova doc` (Markdown/JSON
  documentation generation from `///`/`//!` doc-comments, with doc-tests
  and a `--check` mode for broken links/missing summaries).
  `nova run` (an interpreter entry point) is intentionally unsupported —
  Nova is compile-only.
- **`nova-lsp`** — a language server (hover, diagnostics, symbol
  lookup) with a **VSCode extension** (TextMate grammar plus LSP wiring;
  Sublime/Vim/Emacs get syntax-highlighting-only grammars under
  `editors/`).
- **Docker recipe** (`docker/release/`) exists. This plan proposes
  `nova-oracle:0.1.0`; no Docker image has been built or published for this
  draft.
- Optional **Z3-backed contract verification** (`--features z3-backend`,
  `NOVA_SMT_BACKEND=z3`); a dependency-free `TrivialBackend` (reflexive
  tautologies, constant folding) is the default and needs no external
  solver.

## Distribution

- **Planned release assets (not produced by current CI):**
  `nova-oracle-0.1.0-linux-x86_64.tar.gz`,
  `nova-oracle-0.1.0-windows-x86_64.zip`,
  `nova-oracle-0.1.0-src.tar.gz`, and `SHA256SUMS`. Current CI builds
  `nova-cli` in release mode on Ubuntu and Windows x86_64, but does not
  upload releasable compiler binaries. Its artifact uploads are benchmark
  JSON/Markdown results and the full test report; those are not release
  assets.
- The intended binary platforms are Linux and Windows x86_64. The archive
  contents and installation recipe still need release-build verification;
  a C compiler is required because Nova compiles to C. See
  [docs/guide/quickstart.md](../guide/quickstart.md).
- **Linux source build**: follow [docs/guide/linux-build.md](../guide/linux-build.md)
  (Debian/Ubuntu packages, Rust toolchain, libuv submodule, build, smoke
  test). A CI release-mode build is not a releasable Linux archive.
- **Docker**: `docker/release/Dockerfile`, a two-stage build (Ubuntu
  22.04 builder with the Rust toolchain, then a slim runtime image with
  the compiled `nova` binary, `std/`, and the C runtime). Build context
  must be the repository root. The planned Oracle image name is
  `nova-oracle:0.1.0`; no image has been built or published for this draft.
  See [docker/release/README.md](../../docker/release/README.md).

## Known limitations

This is an early release; treat it accordingly.

- **API and syntax are not frozen.** The language surface, standard library,
  and CLI may change; compatibility is not guaranteed before 1.0.
- **Release assets are not CI outputs.** CI currently builds Oracle on
  Ubuntu and Windows x86_64, but uploads no releasable binaries. Building on
  a runner does not by itself prove that a distributable archive is ready.
- **Contract verification beyond trivial cases needs Z3**, an optional
  external dependency; without it, only reflexive/constant-foldable
  contracts are statically proven, and the rest fall back to runtime
  checks (stripped in release builds unless proven false).
- **A vector literal reserves the default growth capacity, not the
  element count.** `[7, 8]` gives `len=2 cap=8` today; the spec (D239)
  pins the capacity to the element count, and the self-hosted compiler
  (Carina, in development) implements it that way. Observable only through `.cap()`; correctness
  and `len` are unaffected.
- **The garbage collector is stop-the-world** (Boehm GC); a concurrent,
  incremental collector is on the post-1.0 roadmap, not in this release.
- **Sharing mutable state across fibers is checked, including the
  transitive paths.** The compiler has always rejected a direct `mut`
  capture inside a `spawn`/`detach`/`parallel for` body
  (`E_CONCURRENT_MUT_CAPTURE`). This release closes the two gaps measured
  under entry 150 in `docs/plans/221.1-bug-sweep.md` (D441,
  `spec/decisions/06-concurrency.md`): a closure that captures `mut` state
  created **outside** a fiber boundary and is then handed in — as a
  parameter to a function that itself spawns it, or sent down a channel —
  is now flagged at the crossing point, same as a direct capture (measured:
  a shared `Vec` written from 8 fibers through such a closure produced a
  wrong length or crashed in 60 out of 60 runs before the fix; the same
  program with an `AtomicInt` was clean 20/20 — both are pinned as
  conformance fixtures). And an effect handler installed with `with X =
  … { … spawn … }` around a fiber-launching body — which actually runs
  *in the fiber of the failing operation*, not the installing scope's
  fiber (measured: an unsynchronised counter in such a handler lost
  updates in 2 of 5 batches of 64×20 concurrent child failures) — now gets
  the same check, under a dedicated diagnostic
  (`E_HANDLER_MUT_CAPTURE_IN_FIBER`). The one deliberate exception is
  `Supervisor.on_child_fail`, which the runtime genuinely serialises on
  the scope's drive fiber (D416 §2) — pinned by its own fixture proving
  an unsynchronised counter stays exact across the same 64×20 load.
  Share mutable state across fibers through internally synchronised types
  (`Atomic*`, `Mutex`, channel ends, `#share` types); `ro` (immutable)
  captures remain always safe. Two structural risks remain honestly
  un-enforced because no live call site exercises them yet: a closure
  stored as a struct/collection field and called back out later, and a
  named-function call graph deeper than one hop between the closure's
  origin and the `spawn` that invokes it — see D441 §5 for the precise
  boundary.
- **The language specification is authoritative but written in Russian**
  (`spec/decisions/`); this release's English-facing documentation
  (README, quickstart, language tour) is a curated subset, not a full
  translation.
- The VSCode extension is not included in the owner-approved asset list for
  this draft; packaging and release attachment are not asserted here.
- Some standard-library corners and example programs carry documented,
  narrow-scope simplifications (see `docs/dev/simplifications.md` in the
  repository) — these are tracked, not silent.
- **Serde attributes have scope limits.** `flatten` is parsed but not
  synthesized; record-field attributes do not yet cover rich attributes on
  sum-variant record payloads (D435). Unknown JSON fields are rejected by
  default; `#serde(allow_unknown)` is the explicit opt-out (D436).
- **Separately versioned packages are not covered by Oracle's acceptance.**
  In particular, current release-plan evidence keeps typed-extractor
  arities/end-to-end acceptance and some Polaris HTTP work open; do not infer
  that package APIs are fully validated from an Oracle compiler release.

## Links

- [Quickstart](../guide/quickstart.md) — install, build, and run your first Nova
  program, including the effects/concurrency example.
- [Language tour](../guide/language-tour.md) — a 12-section, example-by-example
  tour of the language, every snippet a real compiling/running file.
- [spec/decisions/](../../spec/decisions/) — the D-numbered design decision
  log; the authoritative source for Nova syntax and semantics.
- [Repository](https://github.com/nv-lang/nova)
- [docs/guide/linux-build.md](../guide/linux-build.md) — building from source on
  Linux/WSL2.
