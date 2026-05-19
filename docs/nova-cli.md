# Nova CLI

**English** | [Русский](nova-cli.ru.md)

`nova` is the single entry point to the Nova language toolchain. It
replaces `run_tests.ps1` / `run_tests.sh` / `regen_runtime.ps1`
(see [Plan 28](plans/28-nova-cli.md)).

Version: `0.1.0` (bootstrap). The binary ships as `nova` (Cargo
package `nova`, crate `nova-cli`).

---

## Contents

- [Quickstart](#quickstart)
- [Installation and build](#installation-and-build)
- [Global flags](#global-flags)
- [Exit codes](#exit-codes)
- [Project root discovery](#project-root-discovery)
- [Commands](#commands)
  - [`nova check`](#nova-check) — type-check
  - [`nova run`](#nova-run) — interpreter
  - [`nova build`](#nova-build) — compile to native
  - [`nova test`](#nova-test) — run tests
  - [`nova test-build`](#nova-test-build) — single-test build
  - [`nova regen-runtime`](#nova-regen-runtime) — regenerate runtime stubs
  - [`nova doc`](#nova-doc) — documentation (Plan 45)
  - [`nova doc-query`](#nova-doc-query) — DSL queries over JSON
  - [`nova doc-mcp`](#nova-doc-mcp) — MCP server
  - [`nova contracts`](#nova-contracts) — contract inspection (Plan 33.3)
  - [`nova bench`](#nova-bench) — benchmark infrastructure (Plan 57)
- [Environment variables](#environment-variables)
- [Migration binaries](#migration-binaries)
- [Related documents](#related-documents)

---

## Quickstart

```bash
# Inside a Nova project (sibling nova.toml present):
nova check                       # type-check whole workspace
nova check src/                  # walk a directory recursively
nova check src/lib.nv            # single file

nova run hello.nv                # interpret
nova build app.nv -o app         # compile to a native binary
nova test                        # run all nova_tests/
nova test --filter basics        # substring subset

nova doc src/lib.nv              # markdown to stdout
nova doc src/ --format json      # D107 JSON schema
nova doc src/ --check --strict   # CI doc validation

nova bench run bench.nv          # run benchmarks
nova contracts verify foo.nv     # SMT-verify contracts
```

---

## Installation and build

`nova-cli` lives in `nova-cli/` next to `compiler-codegen/`. No
workspace is used (see [Plan 28](plans/28-nova-cli.md) — both crates
are standalone).

```bash
# Debug build (default, opt-level=0)
cargo build --manifest-path nova-cli/Cargo.toml

# Release (opt-level=2, LTO thin)
cargo build --release --manifest-path nova-cli/Cargo.toml

# With the Z3 backend for contracts (Plan 33.1)
cargo build --release --manifest-path nova-cli/Cargo.toml --features z3-backend
```

You get:
- `nova-cli/target/{debug,release}/nova[.exe]`
- `nova-cli/target/{debug,release}/migrate_plan60[.exe]`
- `nova-cli/target/{debug,release}/migrate_plan65[.exe]`

`nova` has a path dependency on `nova_codegen` (`../compiler-codegen`)
— rebuilding the compiler automatically recompiles the CLI.

---

## Global flags

Apply to every subcommand:

| Flag | Values | Description |
|---|---|---|
| `--color` | `auto` (default), `always`, `never` | ANSI color control. See [Plan 36](plans/36-cli-production-hardening.md) R10. |

**Color auto-detection** (priority high → low):

1. CLI `--color always|never` — overrides everything
2. `CLICOLOR_FORCE=1` → always
3. `NO_COLOR` (any value) → never ([no-color.org](https://no-color.org))
4. `CLICOLOR=0` → never
5. `CI=true` → never
6. `TERM=dumb` → never
7. Default — on

---

## Exit codes

Cargo convention ([Plan 36](plans/36-cli-production-hardening.md) R7):

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Diagnostic failure (typecheck fail, test fail, contract violation, etc.) |
| `2` | Usage error (bad flag, file not found, wrong extension, missing `nova.toml`) |
| `101` | Internal panic (forced via `std::panic::set_hook` for cross-platform consistency) |

`nova doc --diff` additionally uses **3** for patch-level breaking
change (see [`nova doc`](#nova-doc)).

---

## Project root discovery

Most commands search for `nova.toml` walking up from CWD. The logic
lives in `nova_codegen::test_runner::find_repo_root_from`:

1. Walk from CWD up to the filesystem root
2. At each level read `nova.toml` if present
3. If it contains `[workspace]` — that is the root (workspace
   root), stop
4. Otherwise remember the last `nova.toml` seen and keep going
5. Return the `[workspace]`-marked root if found, else the topmost
   `nova.toml` directory

This is **workspace-aware** behavior (D78 AD6, [Plan 35](plans/35-cross-file-resolve.md))
— prevents a nested `nova_tests/nova.toml` from shadowing the real root.

If no `nova.toml` is found — exit `2`:
```
error: nova.toml not found — are you inside a Nova project?
```

Paths resolved under the workspace root:
- `<root>/nova_tests/` — test corpus
- `<root>/std/` — standard library
- `<root>/compiler-codegen/` — C-runtime include paths
- `<root>/compiler-codegen/nova_rt/` — runtime sources (libuv, GC)
- `<root>/target/last-test-results.json` — `--rerun-failed` cache

---

## Commands

### `nova check`

Type-check one or more `.nv` files or directories. Plan 36 MVP —
replaces `nova-codegen check`.

```
nova check [PATHS...] [--jobs N] [-q|-v] [--list] [--format human|short]
           [--include-runtime] [--skip PATTERN]...
```

**Positional arguments:**

- `PATHS` — list of files or directories. Empty → workspace root
  (recursive walk). Files must have a `.nv` extension, otherwise exit `2`.

**Flags:**

| Flag | Default | Description |
|---|---|---|
| `--jobs N` | `0` (= num_cpus) | Parallel workers |
| `-q`, `--quiet` | off | Only FAIL lines and summary |
| `-v`, `--verbose` | off | Extra info (timing) |
| `--list` | off | List collected files without checking |
| `--format` | `human` | `human` (colored) or `short` (`file:line:col: msg` for grep) |
| `--include-runtime` | off | Include `std/runtime/` (auto-gen, skipped by default) |
| `--skip PATTERN` | `[]` | Skip files matching substring (repeatable) |

**Hard-coded skip** (always excluded):

- `target/`, `node_modules/`, `vendor/`
- `.git/`, `.hg/`, `.svn/`
- directories starting with `_` or `.`
- `std/runtime/` (override via `--include-runtime`)

**Behavior:**

- Dedup via `canonicalize`
- Sorted for determinism
- Parallel walk via `thread::scope` + mpsc channel
- Per-file warnings (`yellow: warning:`) after the `ok:` line
- Summary: `pass=N fail=N warnings=N (X.YYs)`
- Exit `1` on any FAIL, `2` on usage error

`--format short`:
```
src/lib.nv: ok
src/foo.nv:42:5: error: type mismatch
```

`--format human` (default):
```
ok: src/lib.nv
FAIL: src/foo.nv
  src/foo.nv:42:5: type mismatch
```

**JSON / SARIF / JUnit** formats are reserved for sub-plan 36.A and
not yet implemented.

---

### `nova run`

Run a `.nv` file through the interpreter (no C-backend compile).

```
nova run FILE
```

- `FILE` — path to a `.nv` file with `fn main`
- Backed by `nova_codegen::interp::Interpreter`
- Equivalent to `nova-codegen run`

---

### `nova build`

Compile **one** `.nv` file to a native binary (via the C backend).

```
nova build FILE [-o OUTPUT] [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
           [--vcvars PATH] [--clang PATH] [--timeout SECS] [--keep-artifacts]
           [--mono-depth N]
```

**Single file only** — `-o` takes one path. For multi-file projects
use `import` from the entry point.

**Arguments:**

| Flag | Default | Description |
|---|---|---|
| `FILE` | — | Entry-point `.nv` with `fn main` |
| `-o OUTPUT` | `<name>[.exe]` in CWD | Output binary path |
| `--mode` | `dev` | `dev` (unoptimized) or `release` (`-O2` + LTO) |
| `--toolchain` | `auto` | `auto` (Clang → MSVC → GCC), `clang`, `msvc`, `gcc` |
| `--vcvars` | auto via vswhere | Path to `vcvars64.bat` (Windows) |
| `--clang` | auto detect | Path to `clang.exe` |
| `--timeout` | `120` | Compile timeout (seconds) |
| `--keep-artifacts` | off | Keep `.c`/`.exe`/`.obj` in tmp |
| `--mono-depth N` | `500` (or `NOVA_MONO_DEPTH`) | Monomorphization-instantiation depth limit ([Plan 48](plans/48-closures-in-generics.md) Ф.7.6) |

**Tmp directory:** `$TEMP/nova_tests/build/<path-hash>/` on Windows or
`$TMPDIR/nova_tests/build/<path-hash>/` on Unix. The hash uses
`DefaultHasher` over the absolute file path — unique without
crypto dependency.

**Pipeline:**

1. parse + typecheck + `infer_effects`
2. `CEmitter::emit_module` → C source
3. `detect_toolchain()` (auto-detects vcvars)
4. `detect_or_build_libuv()` — runtime may depend on libuv
5. `compile_c_to_exe(&tc, &build_opts, timeout)`
6. Copy exe → `-o` or CWD
7. Remove tmp (unless `--keep-artifacts`)

---

### `nova test`

Run tests from a directory or a file. Plan 28 (together with
[Plan 26](plans/26-test-runner-hardening.md), [Plan 27](plans/27-gc-switch.md),
[Plan 34](plans/34-stdlib-typecheck-and-compile-fix.md)).

```
nova test [PATH] [--filter SUBSTR] [--jobs N] [--format text|json|tap|junit]
          [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
          [--vcvars PATH] [--clang PATH] [--timeout SECS] [-v|-q]
          [--results-file PATH] [--rerun-failed] [--retries N]
          [--include-stdlib] [--keep-artifacts] [--gc boehm|malloc]
          [--list] [--filter-from PATH] [--shuffle [SEED]]
          [--skip PATTERN]... [--mono-depth N]
```

**Arguments:**

| Flag | Default | Description |
|---|---|---|
| `PATH` | `<root>/nova_tests/` | Test file or directory |
| `--filter SUBSTR` | — | Filter by display-name substring |
| `--jobs N` | `0` (= num_cpus) | Parallel workers |
| `--format` | `text` | `text`, `json`, `tap`, `junit` |
| `--mode` | `dev` | `dev` or `release` |
| `--toolchain` | `auto` | `auto`, `clang`, `msvc`, `gcc` |
| `--vcvars` | auto | Path to `vcvars64.bat` |
| `--clang` | auto | Path to `clang.exe` |
| `--timeout` | `60` | Per-test timeout (seconds) |
| `-v`, `--verbose` | off | Output for passing tests too |
| `-q`, `--quiet` | off | FAIL + summary only |
| `--results-file PATH` | `<root>/target/last-test-results.json` | Where to write results |
| `--rerun-failed` | off | Re-run only failed/timed-out from last run |
| `--retries N` | `0` | Retries for transient failures (AV races, etc.) |
| `--include-stdlib` | off | Include `std/` |
| `--keep-artifacts` | off | Keep `.c`/`.exe`/`.obj` |
| `--gc` | `boehm` | `boehm` (default) or `malloc` (internal only) |
| `--list` | off | List tests without running |
| `--filter-from PATH` | — | File with test names (one per line, exact match) |
| `--shuffle [SEED]` | off | Random order; optional seed for reproducibility |
| `--skip PATTERN` | `[]` | Skip tests by name or path substring (repeatable) |
| `--mono-depth N` | `500` (or env) | Monomorphization-instantiation depth limit |

**Output formats:**

- `text` — human-readable, colored, on stdout
- `json` — array with `name`, `status`, `duration_ms`, `stderr`
- `tap` — Test Anything Protocol v13
- `junit` — JUnit XML (for CI aggregators)

**`--rerun-failed`:** reads `--results-file`, selects entries with
`status != "pass"`, filters the suite, runs only those.

**EXPECT markers** in test files (see
[docs/test-conventions.md](test-conventions.md)):
- `// EXPECT: <stdout-line>` — exact line match
- `// EXPECT_STDERR: <line>` — for stderr
- `// EXPECT_COMPILE_ERROR: <substring>` — must fail to compile
- `// EXPECT_RUNTIME_ERROR: <substring>` — panic with substring
- `// REQUIRES_SMT_BACKEND` — skip if SMT not available

---

### `nova test-build`

Build + run **one** test file. Used by IDE / CI for targeted debug.

```
nova test-build FILE [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
                [--vcvars PATH] [--clang PATH] [--timeout SECS]
                [--keep-artifacts] [--gc boehm|malloc] [--mono-depth N]
```

| Flag | Default | Description |
|---|---|---|
| `FILE` | — | Path to a `.nv` test |
| `--mode` | `dev` | See [`nova test`](#nova-test) |
| `--toolchain` | `auto` | |
| `--vcvars` | auto | |
| `--clang` | auto | |
| `--timeout` | `60` | |
| `--keep-artifacts` | off | |
| `--gc` | `boehm` | |
| `--mono-depth N` | `500` | |

Equivalent to `nova test <FILE>` but without bulk-runner machinery
(single exe, single test-block per file).

---

### `nova regen-runtime`

Regenerate `std/runtime/*.nv` stubs from the compiler runtime
registry. Replaces `regen_runtime.ps1`.

```
nova regen-runtime [--check]
```

| Flag | Default | Description |
|---|---|---|
| `--check` | off | Compare only — exit `1` if stubs diverge from the registry (CI guard) |

Backed by `nova_codegen::codegen::runtime_registry::all()` + module
render. See [Plan 13](plans/13-runtime-stdlib-and-autogen.md).

---

### `nova doc`

Production-grade documentation (Plan 45 / D107). Markdown / JSON / HTML
+ doc-tests + coverage + mutation testing + watch + workspace mode.

```
nova doc [FILE] [--format markdown|json|html] [--json-schema]
         [--include-private] [--test] [--check] [--watch]
         [--coverage [--coverage-threshold PERCENT]] [--jobs N]
         [--diff OLD NEW] [--scrape-examples WORKSPACE]
         [--strict] [--mutate-contracts [--real-exec]]
         [--output-dir DIR]
```

**Arguments:**

| Flag | Default | Description |
|---|---|---|
| `FILE` | — (required unless `--json-schema`) | `.nv` file or directory |
| `--format` | `markdown` | `markdown`, `json` (D107 schema), `html` |
| `--json-schema` | off | Print the embedded JSON Schema 2020-12 and exit |
| `--include-private` | off | Include non-exported items |
| `--test` | off | Run doc-tests (Plan 45 Ф.7) |
| `--check` | off | Validate without rendering (broken links, missing summaries) |
| `--watch` | off | Re-render on mtime poll (500ms); Ctrl-C to exit |
| `--coverage` | off | Coverage metrics (% items with summary) |
| `--coverage-threshold N` | — | CI gate: exit `1` if coverage% < N |
| `--jobs N` | `0` (= num_cpus) | Parallel parse jobs for workspace |
| `--diff OLD NEW` | — | Compare two JSON outputs (semver detection) |
| `--scrape-examples WORKSPACE` | — | Attach top-3 usage examples per fn |
| `--strict` | off | Warnings → errors (CI) |
| `--mutate-contracts` | off | Mutation testing for contracts (Nova-unique) |
| `--real-exec` | off | Actually execute mutants (requires `--mutate-contracts`) |
| `--output-dir DIR` | — | Multi-page HTML; only with `--format html` |

**Exit codes for `--diff OLD NEW`:**

| Code | Meaning |
|---|---|
| `0` | No breaking changes |
| `1` | Major change (breaking) |
| `2` | Minor change (additive) |
| `3` | Patch change (cosmetic) |

**Mutation testing (`--mutate-contracts`):**

Generates mutants per function with contracts:
- `>` ↔ `>=`, `<` ↔ `<=`
- `==` ↔ `!=`
- Drop `requires`/`ensures`

Default — text-based heuristic (~1ms/mutant). With `--real-exec` —
runs mutated doc-tests through `test_runner` (~100ms/mutant, true
positive guarantee).

**Supported `///` doc formats** — see [Plan 45](plans/45-nova-doc.md)
(D107).

---

### `nova doc-query`

DSL queries against the `nova doc --format json` output
(Plan 45 Ф.32.1). Foundation for the MCP server
([`nova doc-mcp`](#nova-doc-mcp)).

```
nova doc-query JSON_FILE [QUERY]
```

**Query syntax:** `key=value,key=value,...`

| Key | Values |
|---|---|
| `kind` | `fn`, `type`, `effect`, `protocol`, `module`, ... |
| `name` | substring |
| `module` | exact module path |
| `module-prefix` | path prefix |
| `capability` | capability name |
| `effect` | effect name |
| `has-contracts` | `true`, `false` |
| `verified` | `true`, `false` |
| `stability` | `stable`, `unstable`, `experimental` |
| `deprecated` | `true`, `false` |

**Examples:**

```bash
nova doc src/ --format json > out.json
nova doc-query out.json "kind=fn,capability=pure"
nova doc-query out.json "name=add,has-contracts=true"
nova doc-query out.json "module-prefix=std,effect=Fs"
```

Empty query → returns the whole file as-is.

---

### `nova doc-mcp`

MCP server (Model Context Protocol) — JSON-RPC over stdio or HTTP
(Plan 45 Ф.32.3 / Ф.34.1). Compatible with MCP clients (Claude Code,
MCP Inspector).

```
nova doc-mcp FILE [--port PORT]
```

| Flag | Default | Description |
|---|---|---|
| `FILE` | — | `.nv` source or pre-generated `.json` |
| `--port PORT` | — (stdio) | HTTP mode on `127.0.0.1:PORT`, POST `/mcp` |

**Tools (exposed via `tools/list`):**

- `query_items(query)` — search via DSL ([`nova doc-query`](#nova-doc-query))
- `list_modules()` — list module paths
- `get_item(item_id)` — fetch full item JSON

**Protocol:** the MCP client sends `initialize` → `tools/list` →
`tools/call`.

---

### `nova contracts`

Contract inspection and verification (Plan 33 / D24). Output is JSON
(AI-friendly schema, see `docs/contracts-diag-schema.json`).

```
nova contracts <SUBCOMMAND>
```

#### `nova contracts list`

List all contracts in a file.

```
nova contracts list FILE
```

#### `nova contracts verify`

SMT-verify contracts. JSON output.

```
nova contracts verify FILE [--backend BACKEND]
```

| Flag | Default | Description |
|---|---|---|
| `FILE` | — | `.nv` file |
| `--backend BACKEND` | env `NOVA_SMT_BACKEND` | Override SMT backend (`trivial`, `z3`) |

**Z3 backend:** requires a build with `--features z3-backend`. See
[Plan 33.1](plans/33.1-contracts-core.md).

#### `nova contracts suggest`

AI-assisted contract suggestions (stubs).

```
nova contracts suggest FILE FN_NAME
```

#### `nova contracts counterexample`

Counterexample for a failing contract.

```
nova contracts counterexample FILE FN_NAME [--contract-id N]
```

| Flag | Default | Description |
|---|---|---|
| `FN_NAME` | — | Function name |
| `--contract-id N` | `0` | Contract index (0-based) |

---

### `nova bench`

Benchmark infrastructure (Plan 57 — `MVP+A+B+C+D+E+F+G+H` shipped).
Outperforms Criterion (Rust) / `testing.B`+benchstat (Go) /
tinybench (TS) on several axes. See
[docs/bench-conventions.md](bench-conventions.md).

```
nova bench <SUBCOMMAND>
```

**Subcommands:** [`run`](#nova-bench-run), [`diff`](#nova-bench-diff),
[`gate`](#nova-bench-gate), [`calibrate`](#nova-bench-calibrate),
[`cpu-instr-check`](#nova-bench-cpu-instr-check),
[`membw-check`](#nova-bench-membw-check),
[`hyperfine`](#nova-bench-hyperfine), [`callgrind`](#nova-bench-callgrind),
[`callgrind-check`](#nova-bench-callgrind-check),
[`runner-branch`](#nova-bench-runner-branch),
[`history-anomalies`](#nova-bench-history-anomalies),
[`remote`](#nova-bench-remote), [`corpus`](#nova-bench-corpus),
[`history-add`](#nova-bench-history-add), [`history-list`](#nova-bench-history-list),
[`history-squash`](#nova-bench-history-squash),
[`dashboard`](#nova-bench-dashboard).

#### `nova bench run`

Run `bench "..." { measure { ... } }` declarations.

```
nova bench run FILE [--filter PATTERN] [--samples N] [--warmup-ms MS]
                    [--time-budget SECS] [--gc boehm|malloc]
                    [--mode release|dev] [--toolchain auto|clang|msvc|gcc]
                    [--vcvars PATH] [--clang PATH]
                    [--compile-timeout SECS] [--run-timeout SECS]
                    [--keep-artifacts] [--mono-depth N]
                    [--out PATH] [--out-csv PATH] [--out-md PATH]
                    [--out-criterion DIR] [--profile MODE OUT]
                    [--histogram]
```

| Flag | Default | Description |
|---|---|---|
| `FILE` | — | `.nv` file with `bench "..."` blocks |
| `--filter PATTERN` | — | Comma-separated bench-name fragments |
| `--samples N` | `100` | Override sample count |
| `--warmup-ms` | `500` | Warmup duration in ms |
| `--time-budget` | `10` | Per-bench budget in seconds |
| `--gc` | `boehm` | See [`nova test`](#nova-test) |
| `--mode` | `release` | `release` (recommended) or `dev` |
| `--toolchain` | `auto` | See [`nova build`](#nova-build) |
| `--compile-timeout` | `120` | Compile timeout |
| `--run-timeout` | `600` | Bench process run timeout |
| `--out PATH` | — | Write JSON v1 |
| `--out-csv PATH` | — | Write CSV |
| `--out-md PATH` | — | Markdown (for PR comment) |
| `--out-criterion DIR` | — | Criterion-compatible JSON layout |
| `--profile MODE OUT` | — | `cpu`/`heap`/`gc` profile; cpu requires `samply` |
| `--histogram` | off | ASCII histogram per bench |

**Output formats:**

- `--out` (JSON v1): full schema with metadata (git SHA, toolchain, CPU model)
- `--out-criterion`: `<dir>/<safe-name>/new/{estimates,sample,benchmark}.json`,
  compatible with `cargo-criterion --message-format=criterion`
- `--out-md`: markdown table for PRs
- `--histogram`: 40 buckets, Unicode block chars, median and Tukey fences

**Profile modes:**

- `cpu` — wraps `samply` (install via `cargo install samply`)
- `heap` — `NOVA_BENCH_HEAP_SAMPLE_MS=10`
- `gc` — `NOVA_BENCH_GC_TRACE=1`

#### `nova bench diff`

Compare two bench results. Welch's t-test, geomean delta,
reproducibility check.

```
nova bench diff BASELINE NEW [--format terminal|markdown|json]
                              [--explain [--ai-config PATH] [--ai-max-tokens N]
                                         [--ai-dry-run]]
                              [--baseline-sha SHA] [--new-sha SHA]
```

| Flag | Default | Description |
|---|---|---|
| `BASELINE`, `NEW` | — | JSON files (`nova bench run --out`) |
| `--format` | `terminal` | `terminal`, `markdown`, `json` |
| `--explain` | off | AI regression interpretation (Plan 57.F.2, opt-in) |
| `--ai-config PATH` | `~/.nova-ai.toml` | AI config path |
| `--ai-max-tokens` | `4000` | Override max tokens |
| `--ai-dry-run` | off | Print request body without API call |
| `--baseline-sha`, `--new-sha` | auto from JSON | Git SHA for context |

`--explain` uses `system curl` (no RustCrypto stack) and requires
`NOVA_AI_API_KEY` or a config file.

#### `nova bench gate`

CI gate — apply thresholds from `bench.toml`. Exit `0` = pass,
`1` = regress.

```
nova bench gate BASELINE NEW [--config PATH] [--noise PATH]
```

| Flag | Default | Description |
|---|---|---|
| `--config` | `./bench.toml` | Path to bench.toml |
| `--noise` | `./.nova-bench-noise.json` if present | Auto-calibrated noise floor (see `calibrate`) |

#### `nova bench calibrate`

Auto-calibrate noise floor from ≥2 repeated runs of the same
baseline (Plan 57.A.3).

```
nova bench calibrate RUNS... [--out PATH]
```

| Flag | Default | Description |
|---|---|---|
| `RUNS...` | — | ≥2 JSON results of the same source |
| `--out` | `.nova-bench-noise.json` | Where to write the noise floor |

The file is machine-specific; do not commit to git.

#### `nova bench cpu-instr-check`

Diagnose CPU-instruction-counter availability (Plan 57.B.4).

```
nova bench cpu-instr-check
```

Linux: tries `perf_event_open` + measures a known loop. Other OS:
prints a stub message.

#### `nova bench membw-check`

Diagnose memory-bandwidth measurement availability (Plan 57.F.3).

```
nova bench membw-check
```

Linux: probes `/sys/devices/uncore_imc_*` + tries an LLC-miss perf
counter. Other OS: stub.

#### `nova bench hyperfine`

Hyperfine-style cross-binary timing — wall-clock measurement of
arbitrary commands (Plan 57.H.2). Output schema-compatible with
`nova bench diff`.

```
nova bench hyperfine SPECS... [--warmup N] [--samples N]
                              [--timeout SECS] [--workdir PATH] [--out PATH]
```

| Flag | Default | Description |
|---|---|---|
| `SPECS...` | ≥1 | `"name=binary args..."` or just `"binary args..."` |
| `--warmup` | `3` | Warmup runs (discarded) |
| `--samples` | `10` | Sample runs |
| `--timeout` | `300` | Per-command timeout |
| `--workdir PATH` | — | CWD for commands |
| `--out PATH` | stdout | JSON output |

**Example:**
```bash
nova bench hyperfine \
  "old=./nova-old build large.nv" \
  "new=./nova-new build large.nv" \
  --samples 10 --warmup 2 --out result.json
```

#### `nova bench callgrind`

Run under Valgrind Callgrind — deterministic CPU-instruction count
(Plan 57.H.3). Cross-platform fallback to `perf_event_open`
(Linux-only). Works on macOS + Linux with `valgrind` installed.

```
nova bench callgrind BINARY [ARGS...] [--cache-sim] [--workdir PATH] [--out PATH]
```

| Flag | Default | Description |
|---|---|---|
| `BINARY` | — | Path to executable |
| `ARGS...` | — | Arguments for the executable |
| `--cache-sim` | off | I1/D1/LL miss counts (slower) |
| `--workdir PATH` | — | CWD for the command |
| `--out PATH` | — | JSON `CallgrindResult` |

#### `nova bench callgrind-check`

Check valgrind availability + version.

```
nova bench callgrind-check
```

#### `nova bench runner-branch`

Print the recommended history-branch name based on the
`NOVA_BENCH_RUNNER_ID` env (Plan 57.D.4 — multi-runner CI matrix).

```
nova bench runner-branch
```

Returns `bench-history` if env is unset, else `bench-history-<id>`.

#### `nova bench history-anomalies`

Detect changepoints in the historical median time-series via PELT
(Plan 57.E.5). Identifies regimes with ≥5% delta.

```
nova bench history-anomalies [--branch BRANCH] [--format text|json]
```

| Flag | Default | Description |
|---|---|---|
| `--branch` | `auto` (NOVA_BENCH_RUNNER_ID-aware) | History branch |
| `--format` | `text` | `text` or `json` |

#### `nova bench remote`

SSH-distributed bench coordination (Plan 57.F.1).

```
nova bench remote <SUBCOMMAND>
```

##### `nova bench remote list`

List configured remotes from `~/.nova-bench-remotes.toml`.

```
nova bench remote list [--config PATH]
```

`--config` overridable via `NOVA_BENCH_REMOTES` env.

##### `nova bench remote ping`

SSH health check for one remote.

```
nova bench remote ping NAME [--config PATH]
```

##### `nova bench remote run`

Parallel bench across N remotes; gathers results.

```
nova bench remote run BENCH [--remotes LIST] [--gather-into DIR] [--sha SHA] [--config PATH]
```

| Flag | Default | Description |
|---|---|---|
| `BENCH` | — | `.nv` file path (relative to remote repo root) |
| `--remotes` | `all` | Comma-separated names or `all` |
| `--gather-into` | `remote-results` | Where to place per-remote JSON |
| `--sha SHA` | — | Optional git SHA to checkout before bench |

#### `nova bench corpus`

Measure per-pass compile time for corpus file(s) (Plan 57.C.8).
Wraps `nova build` with `NOVA_PERF_TIMER=1`, parses `__PERF__`
markers.

```
nova bench corpus PATH [--json] [--html PATH] [--echarts-url URL]
                       [--mode release|dev] [--toolchain auto|clang|msvc]
                       [--gc boehm|malloc]
```

| Flag | Default | Description |
|---|---|---|
| `PATH` | — | `.nv` file or directory |
| `--json` | off | JSON output (instead of table) |
| `--html PATH` | — | HTML compiler-perf dashboard (Plan 57.D.5) |
| `--echarts-url` | `https://cdn.jsdelivr.net/...` | Custom echarts URL (offline) |
| `--mode` | `release` | |
| `--toolchain` | `auto` | |
| `--gc` | `boehm` | |

#### `nova bench history-add`

Append a result JSON to the orphan history branch (Plan 57.A.1).

```
nova bench history-add RESULT [--branch BRANCH] [--push] [--remote NAME] [--dry-run]
```

| Flag | Default | Description |
|---|---|---|
| `RESULT` | — | JSON from `nova bench run --out` |
| `--branch` | `auto` | Orphan branch (defaults to `bench-history`) |
| `--push` | off | Push after commit |
| `--remote` | `origin` | Remote name when `--push` |
| `--dry-run` | off | Print what would happen without committing |

#### `nova bench history-list`

List entries in the history branch (newest first).

```
nova bench history-list [--branch BRANCH]
```

#### `nova bench history-squash`

Squash older entries per retention policy (Plan 57.C.6 — yearly
squash recommended).

```
nova bench history-squash --before-date YYYY-MM-DD [--branch BRANCH]
                          [--push] [--remote NAME] [--dry-run]
```

| Flag | Default | Description |
|---|---|---|
| `--before-date` | — (required) | Squash everything older than this UTC date |
| `--branch` | `auto` | |
| `--push` | off | |
| `--remote` | `origin` | |
| `--dry-run` | off | Print what would be removed |

#### `nova bench dashboard`

Static HTML dashboard from history (Plan 57.A.2).

```
nova bench dashboard [--history-branch BRANCH] [--out DIR] [--max-entries N] [--echarts-url URL]
```

| Flag | Default | Description |
|---|---|---|
| `--history-branch` | `auto` | History branch |
| `--out` | `dashboard` | Output directory |
| `--max-entries` | `200` | Max entries (newest first) |
| `--echarts-url` | jsdelivr URL | Custom echarts URL (offline = local) |

Generates `index.html` + `bench-<safe>.html` per bench + `data.json`.

---

## Environment variables

| Var | Used by | Effect |
|---|---|---|
| `NOVA_CODEGEN` | (reserved) | Override path to `nova-codegen` binary |
| `NOVA_MONO_DEPTH` | `build`, `test`, `test-build`, `bench` | Monomorphization-instantiation depth limit (default 500) |
| `NOVA_SMT_BACKEND` | `contracts` | SMT backend (`trivial`, `z3`) |
| `NOVA_PERF_TIMER` | `bench corpus` (auto-set) | Enables `__PERF__` markers in the compiler |
| `NOVA_PERF_TIMER_AGGREGATE` | `bench corpus` | Aggregate `__PERF__` across passes |
| `NOVA_BENCH_RUNNER_ID` | `bench history-*`, `runner-branch` | Multi-runner CI matrix; used in branch name |
| `NOVA_BENCH_REMOTES` | `bench remote` | Override path to `.nova-bench-remotes.toml` |
| `NOVA_BENCH_FILTER` | `bench run` (auto-set) | Forwarded to bench process |
| `NOVA_BENCH_SAMPLES` | `bench run` (auto-set) | Override sample count |
| `NOVA_BENCH_WARMUP_NS` | `bench run` (auto-set) | Warmup in nanoseconds |
| `NOVA_BENCH_TIME_BUDGET_NS` | `bench run` (auto-set) | Time budget in nanoseconds |
| `NOVA_BENCH_HEAP_SAMPLE_MS` | `bench run --profile heap` | Sample interval in ms |
| `NOVA_BENCH_GC_TRACE` | `bench run --profile gc` | Enables GC tracing |
| `NOVA_AI_PROVIDER` | `bench diff --explain` | AI provider (anthropic, openai, ...) |
| `NOVA_AI_MODEL` | `bench diff --explain` | Model override |
| `NOVA_AI_API_KEY` | `bench diff --explain` | API key (or via `~/.nova-ai.toml`) |
| `NOVA_C_COMPILER` | `bench repro` | Real path to the C compiler (captured in metadata) |
| `NOVA_SHA` | `bench repro` (compile-time `option_env!`) | Git SHA of the `nova` binary |
| `NO_COLOR` | global | Disable ANSI colors |
| `CLICOLOR` | global | `=0` → disable |
| `CLICOLOR_FORCE` | global | `=1` → force enable |
| `CI` | global | `=true` → disable colors |
| `TERM` | global | `=dumb` → disable colors |
| `TEMP` | Windows | Tmp directory for `build`/`test` artifacts |
| `TMPDIR` | Unix | Same |

---

## Migration binaries

Separate one-shot tools in `nova-cli/src/bin/`. Kept in the
repository as a reference for future atomic API-rename plans.

### `migrate_plan60`

Lexer-based migration of field-style size-accessors to method-form
(D117 / [Plan 60](plans/60-len-access-uniformity.md)):

```
expr.len      → expr.len()
expr.is_empty → expr.is_empty()
expr.byte_len → expr.byte_len()
expr.cap      → expr.capacity()
expr.capacity → expr.capacity()
```

**Skip conditions:** previous significant token == `=`
(method-value assignment: `let f = arr.len`).

```
migrate_plan60 [--apply] [--dry-run] [--md] [--paths DIR...]
```

| Flag | Default | Description |
|---|---|---|
| `--dry-run` | (default) | Print diff only |
| `--apply` | off | Actually write |
| `--md` | off | Include `.md` files (rewrite inside ` ```nova ` / ` ```nv ` blocks) |
| `--paths DIR...` | `std/`, `nova_tests/`, `examples/` | List of directories |

Token-level rewrite — comments / whitespace / formatting preserved 1:1.

### `migrate_plan65`

Lexer-based migration of `Time.after(<lit>)` →
`ChanReader.close_after(Duration.from_*(<lit>))`
([Plan 65](plans/65-chanreader-close-after.md) AD11):

```
Time.after(<INT>)    → ChanReader.close_after(Duration.from_millis(<INT>))
Time.after(<FLOAT>)  → ChanReader.close_after(Duration.from_secs_f64(<FLOAT>))
Time.after(<expr>)   → left as-is + // MIGRATE_MANUAL: Plan 65 — non-literal arg
```

```
migrate_plan65 [--apply] [--dry-run] [--md] [--paths DIR...]
```

**Exit codes (special set):**

| Code | Meaning |
|---|---|
| `0` | No changes needed (idempotent) |
| `1` | Manual markers emitted — CI gate fails |
| `2` | Changes applied (or would be applied in dry-run) |

Token-aware via `nova_codegen::lexer` — strings and comments are
naturally skipped.

---

## Related documents

- [`spec/`](../spec/) — language specification
- [`spec/decisions/09-tooling.md`](../spec/decisions/09-tooling.md) —
  tooling D-blocks (D89, D107, D121, ...)
- [`docs/test-conventions.md`](test-conventions.md) — EXPECT markers,
  test directives
- [`docs/bench-conventions.md`](bench-conventions.md) — bench-file
  conventions
- [`docs/plans/28-nova-cli.md`](plans/28-nova-cli.md) — CLI scaffold plan
- [`docs/plans/36-cli-production-hardening.md`](plans/36-cli-production-hardening.md)
  — exit codes, `--color`, parallel walk
- [`docs/plans/45-nova-doc.md`](plans/45-nova-doc.md) — `nova doc` / `doc-query` / `doc-mcp`
- [`docs/plans/57-perf-benchmark-infrastructure.md`](plans/57-perf-benchmark-infrastructure.md)
  — `nova bench` family
- [`docs/plans/33.3-contracts-advanced.md`](plans/33.3-contracts-advanced.md)
  — `nova contracts`
