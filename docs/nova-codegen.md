# nova-codegen

**English** | [Русский](nova-codegen.ru.md)

`nova-codegen` is the internal Nova compiler: parser + type checker +
treewalk interpreter + C backend + cross-file resolver + SMT contract
verifier.

> **Internal component.** For day-to-day use, prefer the `nova` CLI
> ([docs/nova-cli.md](nova-cli.md)). `nova-codegen` remains the entry
> point for IDE integration, CI, direct codegen debugging, and is
> consumed by `nova-cli` as a path dependency.

Version: `0.1.0` (bootstrap). Cargo package `nova-codegen`, crate
`nova_codegen`, binary `nova-codegen`.

---

## Contents

- [Installation and build](#installation-and-build)
- [Exit codes](#exit-codes)
- [Commands](#commands)
  - [`nova-codegen check`](#nova-codegen-check)
  - [`nova-codegen run`](#nova-codegen-run)
  - [`nova-codegen test-interp`](#nova-codegen-test-interp)
  - [`nova-codegen compile`](#nova-codegen-compile)
  - [`nova-codegen emit-runtime-stubs`](#nova-codegen-emit-runtime-stubs)
  - [`nova-codegen dump-runtime`](#nova-codegen-dump-runtime)
  - [`nova-codegen test-build`](#nova-codegen-test-build)
  - [`nova-codegen test-all`](#nova-codegen-test-all)
- [Environment variables](#environment-variables)
- [Cargo features](#cargo-features)
- [Library API (`nova_codegen`)](#library-api-nova_codegen)
- [Internal architecture](#internal-architecture)
- [Runtime (`nova_rt/`)](#runtime-nova_rt)
- [Related documents](#related-documents)

---

## Installation and build

```bash
# Debug build
cargo build --manifest-path compiler-codegen/Cargo.toml

# Release (bootstrap keeps opt-level=0, no size optimization)
cargo build --release --manifest-path compiler-codegen/Cargo.toml

# With the Z3 backend for contracts (Plan 33.1)
cargo build --release --manifest-path compiler-codegen/Cargo.toml --features z3-backend

# Rust-level unit tests for the compiler
cargo test --manifest-path compiler-codegen/Cargo.toml
```

You get `compiler-codegen/target/{debug,release}/nova-codegen[.exe]`.

**The main thread** is spawned with a **64 MiB** stack
(`std::thread::Builder` + `stack_size`) — AST traversals are mutually
recursive (expr ↔ block ↔ stmt), and the type checker / SCC purity
inference / SMT encoder need deep stacks on large modules. The default
1 MiB Windows stack is insufficient.

**Minimal dependencies:** `clap`, `anyhow`. The bootstrap must build
with an empty lockfile on any stable Rust 1.85+.

---

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Failure (parse fail, type-check fail, codegen fail, runtime fail, tests failed) |

Unlike [`nova`](nova-cli.md), `nova-codegen` uses **binary** exit
codes (0/1) — no separation between usage error and diagnostic failure.

---

## Commands

### `nova-codegen check`

Type-check a file without running it.

```
nova-codegen check FILE
```

| Argument | Description |
|---|---|
| `FILE` | Path to a `.nv` file |

**Pipeline:**

1. `read_file(path)`
2. `parser::parse(&src)` → AST
3. `check_module_path` — D78 path/module enforcement
4. `types::check_module` → type-check + effect inference
5. Prints `ok: {} parsed and checked`

On error — rendered via `Diagnostic::render` with line:col and source
underline.

---

### `nova-codegen run`

Type-check and interpret (calls `fn main`).

```
nova-codegen run FILE
```

**Pipeline:**

1. parse + path-check + type-check
2. `types::annotate_map_literals` (Plan 52 Ф.7) — annotate
   `[k: v]` literals with inferred K/V
3. `desugar::desugar_module` (Plan 52 Ф.5) — desugar map literals
   into `with_capacity` + `@insert`
4. `interp::Interpreter::new()` → `load_module` → `run_main`

The treewalk interpreter has parity with the C backend — same effects,
handlers, structured concurrency, contracts, defer, channels.

---

### `nova-codegen test-interp`

Run `test "..." { ... }` blocks in a file via the interpreter (no C-codegen).

```
nova-codegen test-interp FILE
```

**Pipeline:** parse → path-check → annotate_map_literals → desugar →
`interp::run_tests` → `tests: N passed, N failed`.

Exit `1` if at least one test failed; prints names of failed tests.

This is the interpreter-mode test runner (fast, but no C pipeline).
For codegen pipeline checks use [`test-build`](#nova-codegen-test-build)
or [`test-all`](#nova-codegen-test-all).

---

### `nova-codegen compile`

Compile a `.nv` file to `.c` (no linking).

```
nova-codegen compile FILE [-o OUTPUT] [--no-annotate-source] [--no-lint]
```

| Flag | Default | Description |
|---|---|---|
| `FILE` | — | `.nv` file |
| `-o OUTPUT` | `<name>.c` next to source | Output `.c` path |
| `--no-annotate-source` | off (annotations enabled) | Skip `/* SRC: ... */` comments — for compact output |
| `--no-lint` | off (lint enabled) | Disable lint passes (export-fail-untyped, etc.) |

**Pipeline:**

1. parse + path-check + type-check
2. `annotate_map_literals` + `desugar_module`
3. `types::infer_effects` — D28 effect inference for private fn
4. `lints::lint_module` (if `lint == true`)
5. `CEmitter::new()` + `set_source_for_annotations(src)` (for line:col
   in codegen errors)
6. `set_proven_contracts` (Plan 33.3 Ф.9.9 — selective contract
   stripping, true zero-cost)
7. `emit_module` → C source + warnings
8. `fs::write(&out_path, &c_code)`

**SRC annotations (`/* SRC: ... */`)** — for `.c`-debugging
convenience — map each C line to the originating Nova source.
Enabled by default.

---

### `nova-codegen emit-runtime-stubs`

Regenerate `std/runtime/string.nv` and `std/runtime/math.nv` from
`runtime_registry.rs` (Plan 13).

```
nova-codegen emit-runtime-stubs [--root PATH] [--check]
```

| Flag | Default | Description |
|---|---|---|
| `--root PATH` | `.` (CWD) | Repository root (with `std/runtime/`) |
| `--check` | off | Compare only; exit `1` on diff (CI / pre-commit guard) |

**Workflow for adding a new runtime fn** (e.g. `f64.@cbrt`):

1. Add `RuntimeFn { ... }` to `src/codegen/runtime_registry.rs`
2. Implement in `nova_rt/<module>.c` (or a libc wrapper)
3. Regenerate: `nova regen-runtime` (or `nova-codegen emit-runtime-stubs --root .`)
4. Commit all three (registry + .c + .nv)

`--check` normalizes line endings (CRLF → LF) before comparison.

`std/runtime/string.nv` and `math.nv` are **auto-generated.** Do not
edit by hand — guarded by `nova regen-runtime --check` in CI.

---

### `nova-codegen dump-runtime`

Sanity print of the runtime-functions registry (Plan 13).

```
nova-codegen dump-runtime
```

Output:
```
Nova runtime registry: N function(s) total.

=== <module> (N fns) ===
  <receiver> [mut] <.|@><name>(<params>) -> <return-ty>    [c: <c-name>]
  ...
```

- `.` = static fn (`Type.fn`)
- `@` = instance method (`obj.@fn`)

Useful for auditing registry vs actual C signatures in `nova_rt/`.

---

### `nova-codegen test-build`

Plan 24 — cross-platform per-file test runner: compile one `.nv` to
`.exe` and check `EXPECT` markers (D89).

```
nova-codegen test-build FILE [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
                             [--vcvars PATH] [--clang PATH]
                             [--cg-include PATH] [--rt-dir PATH] [--tmp-dir PATH]
                             [--display NAME] [--keep-artifacts]
                             [--timeout SECS] [--gc boehm|malloc]
```

| Flag | Default | Description |
|---|---|---|
| `FILE` | — | `.nv` test file |
| `--mode` | `dev` | `dev` (unoptimized) or `release` |
| `--toolchain` | `auto` | `auto`, `clang`, `msvc`, `gcc` |
| `--vcvars PATH` | auto via vswhere | Path to `vcvars64.bat` (Windows) |
| `--clang PATH` | auto detect | Path to `clang.exe` |
| `--cg-include PATH` | `<cwd>/compiler-codegen` | Path to `compiler-codegen/` (for `nova_rt/` includes) |
| `--rt-dir PATH` | `<cwd>/compiler-codegen/nova_rt` | Runtime sources |
| `--tmp-dir PATH` | `$TEMP/nova_tests` or `$TMPDIR/nova_tests` or `/tmp/nova_tests` | Tmp directory for `.c`/`.exe`/`.obj` |
| `--display NAME` | basename | Override display name |
| `--keep-artifacts` | off | Keep artifacts |
| `--timeout SECS` | `60` | Per-test timeout (Plan 26 Ф.1) |
| `--gc boehm\|malloc` | `boehm` | GC backend (Plan 27 Ф.4) |

**Pipeline:**

1. `Mode::parse` + `ToolchainPref::parse`
2. `detect_toolchain(&tc_opts)` — Clang first, MSVC/GCC fallback
3. `detect_or_build_libuv(&rt_dir, &repo_root, vcvars)` — runtime dep
4. `TestBuildOpts { ... }` + `test_runner::run_one(&opts)`
5. Output: `<STATUS:14> <display>  # <detail>`

EXPECT markers (D89):
- `// EXPECT: <line>` — exact stdout-line match
- `// EXPECT_STDERR: <line>` — for stderr
- `// EXPECT_COMPILE_ERROR: <substring>` — must fail to compile
- `// EXPECT_RUNTIME_ERROR: <substring>` — panic with substring
- `// REQUIRES_SMT_BACKEND` — skip if SMT not available

---

### `nova-codegen test-all`

Plan 24 — batch test runner: recursive walk of all `.nv` under
`--tests-dir`. Serves as the engine behind `nova test`
([docs/nova-cli.md](nova-cli.md)).

```
nova-codegen test-all [--tests-dir PATH] [--stdlib-dir PATH] [--include-stdlib]
                      [--filter SUBSTR] [--mode dev|release]
                      [--toolchain auto|clang|msvc|gcc]
                      [--vcvars PATH] [--clang PATH]
                      [--cg-include PATH] [--rt-dir PATH] [--tmp-dir PATH]
                      [--keep-artifacts] [--timeout SECS] [--jobs N]
                      [--format text|json|tap] [-v|-q]
                      [--results-file PATH] [--rerun-failed]
                      [--retries N] [--gc boehm|malloc]
```

| Flag | Default | Description |
|---|---|---|
| `--tests-dir PATH` | `nova_tests` | Test corpus root |
| `--stdlib-dir PATH` | `std` | `std/` root (if `--include-stdlib`) |
| `--include-stdlib` | off | Include `std/*` files |
| `--filter SUBSTR` | — | Filter by display name |
| `--mode` | `dev` | See [`test-build`](#nova-codegen-test-build) |
| `--toolchain` | `auto` | |
| `--vcvars`, `--clang` | auto | |
| `--cg-include`, `--rt-dir` | derived from CWD | |
| `--tmp-dir` | `$TEMP/nova_tests` or equivalent | |
| `--keep-artifacts` | off | |
| `--timeout` | `60` | Per-test timeout (Plan 26 Ф.1) |
| `--jobs N` | `0` (= num_cpus) | Parallel workers (Plan 26 Ф.3) |
| `--format` | `text` | `text`, `json`, `tap` (Plan 26 Ф.4) |
| `-v`, `--verbose` | off | Output for PASS tests (Plan 26 Ф.9) |
| `-q`, `--quiet` | off | FAIL + summary only (Plan 26 Ф.9) |
| `--results-file PATH` | — | `last-results.json` file (Plan 26 Ф.10) |
| `--rerun-failed` | off | Re-run only failed/timeout from `--results-file` |
| `--retries N` | `0` | Retry on transient AV/race failures (Plan 26 Ф.12; CI default 2) |
| `--gc boehm\|malloc` | `boehm` | See [`test-build`](#nova-codegen-test-build) |

**Informational messages** (text mode) — to stderr (like cargo):
```
Toolchain: clang, mode=Dev, jobs=8, tests-dir=nova_tests
libuv: enabled
```

Per-test events and summary — to stdout (so wrappers can stream stdout).

**Limitations:**

- `cache_dir: None` — Plan 26 Ф.5 (incremental cache) not implemented
  (hook left in `opts`)
- `list_only: false`, `filter_from: None`, `shuffle_seed: None`,
  `skip: &[]`, `mono_depth: None` — supported only via
  [`nova test`](nova-cli.md#nova-test) (Plan 26 Ф.13+ / 34 / 48)

---

## Environment variables

| Var | Effect |
|---|---|
| `NOVA_CACHE=0` | Disable caches: SMT contracts (Plan 33.3 Ф.12) + build `.c` cache (Plan 81 Ф.9). `off`/`false` also accepted |
| `NOVA_PERF_TIMER=1` | Enable `__PERF__` markers in the compiler (per-pass timing) |
| `NOVA_MONO_DEPTH=N` | Monomorphization-instantiation depth limit (default 500, [Plan 48](plans/48-closures-in-generics.md) Ф.7.6) |
| `NOVA_DEBUG_MONO=1` | Verbose debug print of mono instances (codegen diagnostics) |
| `NOVA_SMT_BACKEND=trivial\|z3` | Override the SMT backend for contracts |
| `NOVA_CACHE_DIR=PATH` | Override the SMT proof cache directory (default `<cwd>/target/`) |
| `NOVA_CLANG=PATH` | Override `clang.exe` for test-build/test-all |
| `NOVA_GCC=PATH` | Override `gcc` path |
| `NOVA_VCVARS=PATH` | Override `vcvars64.bat` (Windows MSVC) |
| `NOVA_MARCH_NATIVE=1` | Enable `-march=native` (release builds; non-portable binary) |
| `NOVA_GC_LIB_DIR=PATH` | Override libgc.a / gc.lib directory (Boehm) |
| `NOVA_GC_INCLUDE_DIR=PATH` | Override include path for `gc.h` |
| `VCPKG_ROOT=PATH` | vcpkg root (for libuv / libz3 auto-resolve) |
| `CC=name` | Fallback C compiler (POSIX) |
| `NOVA_VERSION=N.M.K` | Current version for deprecation diagnostics (Plan 45 Ф.21) |
| `NOVA_FEATURES=f1,f2` | Cfg feature set ([Plan 42.12](plans/42.12-cfg-conditional-compilation.md)) |
| `NOVA_TARGET_OS=name` | Override `target_os` for cfg resolve |
| `TEMP` (Windows) | Tmp directory |
| `TMPDIR` (Unix) | Tmp directory |
| `PATH` | Used to locate `clang`/`gcc`/`cl`/`vswhere` |
| `ProgramFiles(x86)` | Used to find vswhere for MSVC auto-detect |

---

## Cargo features

| Feature | Description |
|---|---|
| (default) | TrivialBackend SMT (reflexive `ensures`), no external dependencies |
| `z3-backend` | Links libz3 via vcpkg ([Plan 33.1](plans/33.1-contracts-core.md)). FFI bindings are in-tree in `src/verify/backend/z3_ffi.rs` (no `z3`/`z3-sys` crates — feedback: wrappers only in our files). Linkage controlled by `build.rs` + `vcpkg.json` |

**Building with Z3:**
```bash
cargo build --release --features z3-backend
# Run tests:
NOVA_SMT_BACKEND=z3 nova test
```

---

## Library API (`nova_codegen`)

`nova-cli` uses `nova-codegen` as a path dependency and consumes the
library API directly (no subprocess). Public modules from `lib.rs`:

| Module | What |
|---|---|
| `argbind` | Named/positional arg binding ([Plan 46](plans/46-named-parameters.md) / D102) |
| `ast` | AST types: `Module`, `Item`, `Expr`, `Stmt`, `Pattern`, ... |
| `callnorm` | Call-site normalization for named params |
| `codegen` | C backend: `CEmitter::emit_module`, `runtime_registry::all`, ... |
| `desugar` | Desugars map literals, `for-in`, and other sugar |
| `diag` | Structured diagnostics (`Diagnostic`, `Span`, `byte_to_line_col`) |
| `doc` | Plan 45 — DocModel, renderers, MCP server |
| `imports` | Plan 35 R31 — cross-file resolver (`resolve_imports_inline`) |
| `interp` | Treewalk interpreter (`Interpreter::new/load_module/run_main/run_tests`) |
| `lexer` | Tokenization, `lex(&src) -> Vec<Token>` |
| `lints` | D-rule based lints (`lint_module`) |
| `manifest` | `nova.toml` + D78 path/module enforcement |
| `parser` | Recursive-descent parser (`parse(&src) -> Result<Module, Diagnostic>`) |
| `perf_timer` | `NOVA_PERF_TIMER` instrumentation |
| `test_runner` | Test discovery + parallel execution + toolchain detect |
| `types` | Type-checker + effect inference (`check_module`, `infer_effects`, `annotate_map_literals`) |
| `verify` | SMT contracts integration (`TrivialBackend`, `Z3Backend` under feature) |

**Re-exports:** `Diagnostic`, `Span` directly from `nova_codegen::*`.

---

## Internal architecture

```
src/
  lexer/                  tokenization
  parser/                 recursive-descent parser
  ast/                    AST types
  types/                  type checker + effect inference + lints
  interp/                 treewalk interpreter
  codegen/                C backend
    emit_c.rs             main codegen (~20k LOC)
    runtime_registry.rs   source-of-truth for std/runtime/*.nv stubs
  imports.rs              Plan 35 R31: cross-file resolver
  diag/                   structured diagnostics (with FileId for cross-file)
  manifest.rs             nova.toml + D78 path enforcement
  lints.rs                D-rule based lints
  test_runner.rs          test discovery + parallel execution
  verify/                 contracts SMT (TrivialBackend + Z3 optional)
  doc/                    Plan 45 nova doc (parser, renderer, MCP)
  desugar.rs              desugaring passes
  argbind.rs              named-params binding (Plan 46)
  callnorm.rs             call-site normalization
  perf_timer.rs           NOVA_PERF_TIMER markers
  lib.rs                  re-exports
  main.rs                 CLI dispatch (~664 LOC)
```

### Cross-file resolver (Plan 35 R31)

`imports::resolve_imports_inline` — shared between `nova check`,
`nova build`, `nova test`:

- DFS with cycle detection (`in_progress` + `visited` sets)
- Selective import `X.{A, B}` (syntax; bootstrap MVP does not yet
  enforce the filter — full enforcement deferred to the post-bootstrap
  type checker)
- `export import X.{A}` re-export
- Auto-import of `std/prelude.nv` (R27)

### Folder-modules ([Plan 42](plans/42-folder-modules.md))

Module = single-file `X.nv` OR folder `X/` with peers (Go-style):

- `manifest::resolve_module_paths(parts, ...)` → `Vec<PathBuf>`,
  alphabetical sort (deterministic build)
- Filter `*_test.nv` peers when `!include_test_peers`
- `X.nv` + `X/` with a direct `.nv` → ambiguous error
- `internal/<...>` path protection — import only from parent's descendants
- `_module.nv` convention for module-level `#forbid` / `#cfg` / `#doc`

### D78 path/module enforcement

If a file lives inside a package (`nova.toml` in parent dirs), the
compiler verifies that `module X.Y.Z` matches the filesystem path.
Standalone `.nv` files without `nova.toml` pass through.

### Effect inference (D28)

`types::infer_effects` injects `Fail` into the effect row of private
functions if the body contains `throw` and `Fail` is not declared
explicitly. Public functions — explicit only (D28: "public API must
not implicitly throw").

### Plan 33.3 contract stripping (Ф.9.9)

`CEmitter::set_proven_contracts(&module_env.proven_contracts)` —
selectively strips bodies of proven contracts in codegen (true
zero-cost even in debug). SMT-proven
`requires`/`ensures`/`invariant` are not emitted as runtime
assertions.

---

## Runtime (`nova_rt/`)

C sources linked into every `.exe`. Minimum **3** `.c` files are
always linked:

| File | What |
|---|---|
| `alloc.c` (or `alloc_boehm.c`, `alloc_rc.c`) | Allocator (Boehm GC default since [Plan 27](plans/27-gc-switch.md)) |
| `effects.c` | Handler stack (D61), `nova_interrupt` / `nova_interrupt_ptr` ([Plan 39](plans/39-range-stdlib-fixes.md) Issue A) |
| `fibers.c` | Shim over `minicoro.h` + structured concurrency ([Plan 44.5](plans/44.5-work-stealing-scheduler.md)) |

**Header-only:**

| Header | What |
|---|---|
| `array.h` | `NovaArray_<T>`, `NovaOpt_<T>` auto-gen helpers |
| `cast.h` | D54 narrow casts (saturation, wrap-around semantics) |
| `effects.h` | `NovaThrowKind`, `nova_throw_cancel`, `Handler[E, IRT]` API |
| `fibers.h` | `nova_spawn`, `nova_supervised`, `nova_cancel_*`, M:N runtime |
| `channels.h` | `Channel[T]` mpsc ([Plan 44.1](plans/44.1-channel-hardening.md)), `select` waiter |
| `sync.h` | C11 atomics + mutex for channel hardening |
| `minicoro.h` | Vendored stackful coroutines (do not patch, version-pinned) |
| `nova_rt.h` | Single include — `nova_str_cmp`/`lt`/`le`/`gt`/`ge` byte-wise compare, etc. |

**Build scripts** in `compiler-codegen/`:
- `build_c.bat` / `build_c.ps1` / `build_c.sh` — compile one `.c` to `.exe`
- `vcpkg.json` / `vcpkg_installed/` — vendored libuv + libz3
- `build.rs` — feature-flag wiring (`z3-backend`)

---

## Related documents

- [`docs/nova-cli.md`](nova-cli.md) — `nova` CLI (recommended
  user-facing entry point)
- [`compiler-codegen/README.md`](../compiler-codegen/README.md) —
  original README with detailed architecture (Russian)
- [`spec/`](../spec/) — language specification
- [`spec/decisions/`](../spec/decisions/) — D-blocks
- [`docs/test-conventions.md`](test-conventions.md) — EXPECT markers
- [`docs/plans/13-runtime-stdlib-and-autogen.md`](plans/13-runtime-stdlib-and-autogen.md)
  — runtime registry + auto-gen
- [`docs/plans/24-cross-platform-test-runner.md`](plans/24-cross-platform-test-runner.md)
  — `test-build` / `test-all`
- [`docs/plans/26-test-runner-hardening.md`](plans/26-test-runner-hardening.md)
  — timeout / parallel / format / rerun-failed
- [`docs/plans/27-gc-switch.md`](plans/27-gc-switch.md) —
  `--gc boehm|malloc`
- [`docs/plans/35-cross-file-resolve.md`](plans/35-cross-file-resolve.md)
  — cross-file resolver
- [`docs/plans/42-folder-modules.md`](plans/42-folder-modules.md) —
  folder-modules
- [`docs/plans/33.1-contracts-core.md`](plans/33.1-contracts-core.md)
  — contracts + Z3 backend
- [`docs/plans/45-nova-doc.md`](plans/45-nova-doc.md) — `nova doc`
  (`nova_codegen::doc`)
- [`docs/plans/48-closures-in-generics.md`](plans/48-closures-in-generics.md)
  — monomorphization (`NOVA_MONO_DEPTH`)
