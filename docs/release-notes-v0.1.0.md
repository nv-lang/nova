<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova v0.1.0 — release notes (draft)

**Nova v0.1.0 is the first public release of the language.** Nova compiles
to C and then to a native binary — there is no interpreter. Every
function's side effects (`Db`, `Net`, `Io`, `Time`, `Fail`, ...) are part of
its type signature and checked by the compiler. Memory is managed by a
Boehm GC by default; for resources that need deterministic cleanup,
`consume`-typed ownership guarantees an exit-time callback with no GC in
the loop. Concurrency is structured (`spawn`, `parallel for`, `supervised`)
on an M:N work-stealing fiber scheduler, with no `async`/`await` split.

This is a snapshot of an early, working compiler and standard library —
not a finished 1.0. See "Known limitations" below before depending on it
for anything beyond experimentation.

## Highlights

### Language

- **Effects in function signatures** (`fn f(...) Db Net Fail -> T`):
  side effects a function performs are visible in its type; a handler is
  substituted via `with Handler = ... { body }`, which is also Nova's
  answer to mocking in tests — swap a handler, no mocking framework.
- **`consume`/ownership with `defer`**: `defer { ... }` runs at every scope
  exit (including `throw`/`panic`), LIFO across multiple `defer`s in the
  same scope. A `consume`-typed binding is ownership-tracked; historically
  it had to be consumed exactly once or the compiler rejected the program
  (strict-linear). **New in this release (D432):** a `consume` type can
  opt into an affine discipline instead — if it declares an effect-pure
  `@cleanup(outcome ScopeOutcome) -> ()`, the compiler auto-inserts a call
  to it on any exit path where the value is still live, so forgetting to
  consume it is no longer a compile error. Types without `@cleanup` keep
  the original strict-linear behavior.
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

### Standard library

`std` ships with collections (`Vec`/`[]T` alias, `HashMap`, iterators),
IO, filesystem, path, OS, time, JSON-capable encoding, checksums,
cryptography primitives, identifiers, Unicode, text utilities, a testing
framework with deterministic handlers (e.g. a mockable clock and `Random`
seed), and the concurrency/runtime layer that backs the fiber scheduler.
Networking, TLS, HTTP, and compression are separately versioned packages
(`nova-net`/`nova-tls`/`nova-http`/`nova-compress`), pulled in via
`nova.lock` the same way any external Nova package is.

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
- **Docker image** (`docker/release/`) — a ~1 GB Linux image with the
  `nova` compiler, `std/`, and the C runtime (libuv-backed), built from
  the same recipe as CI; mount a project directory and run
  `nova build`/`nova test` inside the container with no local Rust
  toolchain.
- Optional **Z3-backed contract verification** (`--features z3-backend`,
  `NOVA_SMT_BACKEND=z3`); a dependency-free `TrivialBackend` (reflexive
  tautologies, constant folding) is the default and needs no external
  solver.

## Distribution

- **Windows x64**: a prebuilt zip (`nova-v0.1.0-windows-x64.zip`) with
  `nova.exe`, `nova-lsp.exe`, the `std/` sources, a trimmed C runtime
  (headers plus the subset of libuv actually compiled), a trimmed Boehm
  GC lib/headers subset, a `setup-env.ps1` that points the compiler at
  this bundle from any working directory, `README-INSTALL.md`, and the
  license files. A C compiler (MSVC via `vcvars64.bat`, or Clang/GCC)
  must be installed separately — Nova compiles to C, not directly to
  machine code. See [docs/quickstart.md](quickstart.md).
- **Linux**: built from source; there is no prebuilt Linux archive for
  v0.1.0 yet. Follow [docs/linux-build.md](linux-build.md) (Debian/Ubuntu
  packages, Rust toolchain, `git submodule update` for the libuv
  submodule, build, smoke test). This is the same recipe the CI gate
  runs, and it is green.
- **Docker**: `docker/release/Dockerfile`, a two-stage build (Ubuntu
  22.04 builder with the Rust toolchain, then a slim runtime image with
  the compiled `nova` binary, `std/`, and the C runtime). Build context
  must be the repository root. See [docker/release/README.md](../docker/release/README.md).

## Known limitations

This is an early release; treat it accordingly.

- **API and syntax are not yet frozen.** The core language surface
  (effects, handlers, syntax, memory model, concurrency primitives) is
  stable in practice, but corners of the standard library and CLI can
  still change before a 1.0.
- **Windows is the primary, most-tested platform** for this release —
  the prebuilt zip only targets Windows x64. Linux works and is
  CI-gated, but only from source; there is no prebuilt Linux binary yet.
- **Contract verification beyond trivial cases needs Z3**, an optional
  external dependency; without it, only reflexive/constant-foldable
  contracts are statically proven, and the rest fall back to runtime
  checks (stripped in release builds unless proven false).
- **The garbage collector is stop-the-world** (Boehm GC); a concurrent,
  incremental collector is on the post-1.0 roadmap, not in this release.
- **The language specification is authoritative but written in Russian**
  (`spec/decisions/`); this release's English-facing documentation
  (README, quickstart, language tour) is a curated subset, not a full
  translation.
- The VSCode extension currently ships syntax highlighting plus LSP
  wiring; a packaged `.vsix` artifact for this release is tracked
  separately from the core language/compiler release.
- Some standard-library corners and example programs carry documented,
  narrow-scope simplifications (see `docs/simplifications.md` in the
  repository) — these are tracked, not silent.

## Links

- [Quickstart](quickstart.md) — install, build, and run your first Nova
  program, including the effects/concurrency example.
- [Language tour](language-tour.md) — a 12-section, example-by-example
  tour of the language, every snippet a real compiling/running file.
- [spec/decisions/](../spec/decisions/) — the D-numbered design decision
  log; the authoritative source for Nova syntax and semantics.
- [Repository](https://github.com/nv-lang/nova)
- [docs/linux-build.md](linux-build.md) — building from source on
  Linux/WSL2.
