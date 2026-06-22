# AGENTS.md

> Instructions for AI agents and coding assistants working in this repository.
> Think of this as a README for agents. Human contributors: see [README.md](README.md) and [CONTRIBUTING.md](CONTRIBUTING.md).
>
> **New here?** How development actually works — plan-driven dev, the worktree model, the daily loop, and the
> hard operational rules — is in [docs/dev-workflow.md](docs/dev-workflow.md) (Russian). Read it before picking up work.

## What is Nova

Nova is a systems programming language with algebraic effects, structured concurrency, and optional contracts. Side effects are visible in function signatures (`Db Net Fail`), enabling local code review and handler-based testing without mocks. See [README.md](README.md) for a full overview.

## Build

```sh
# Build the nova CLI (main entry point for everything)
cd nova-cli && cargo build --release && cd ..

# The resulting binary:
# nova-cli/target/release/nova   (Windows: nova.exe)

# Build compiler internals only (no CLI wrapper)
cd compiler-codegen && cargo build && cd ..
```

After any change to Rust sources in `compiler-codegen/` or `nova-cli/`, rebuild before running tests.

## Test

```sh
# Full test suite (C-codegen pipeline)
nova-cli/target/release/nova test

# Targeted: run only tests matching a substring
nova-cli/target/release/nova test --filter syntax/closure

# Single-file debug (no parallelism, keeps build artifacts)
./compiler-codegen/target/debug/nova-codegen test-build nova_tests/basics/literals.nv \
    --toolchain clang --keep-artifacts

# Interpreter pipeline (no C compilation)
./compiler-codegen/target/debug/nova-codegen test-interp nova_tests/basics/literals.nv
```

Common flags for `nova test`:

| Flag | Effect |
|---|---|
| `--filter <substr>` | Run only matching tests |
| `--mode release` | Compile with `-O3 -flto` |
| `--toolchain clang\|msvc\|gcc` | Force toolchain (default: auto) |
| `--timeout <secs>` | Per-test timeout (default: 60) |
| `--rerun-failed` | Re-run only previously failed tests |
| `--format json\|junit` | Machine-readable output |

Full test guide: [docs/test-conventions.md](docs/test-conventions.md).

## Repository structure

```
nova/
├── nova-cli/            # User-facing CLI: nova build/run/test/check/doc
├── compiler-codegen/    # Rust compiler: parser, type-checker, C-backend codegen, runtime
│   └── nova_rt/         # C runtime: effects, fibers, GC, libuv scheduler
├── nova_tests/          # Test fixtures (.nv files with EXPECT markers)
├── std/                 # Nova standard library source
├── spec/                # Language specification
│   ├── decisions/       # Design decisions (D-blocks) — READ BEFORE CHANGING SEMANTICS
│   └── effects.md       # Effect system intro
├── docs/                # Developer guides
│   ├── test-conventions.md   # Test authoring and EXPECT markers
│   └── simplifications.md    # Running list of removed complexity
├── editors/             # Syntax highlighting plugins (VSCode, Vim, Emacs, Sublime)
└── examples/            # Nova code examples
```

## Design decisions — read before changing syntax or semantics

Nova's design is recorded in **D-blocks** in [spec/decisions/](spec/decisions/). Before adding a new construct or changing existing behavior:

1. Search `spec/decisions/` for relevant D-blocks.
2. Check `spec/decisions/history/rejected.md` — the idea may have been considered and rejected.
3. If the change contradicts an existing D-block, open an issue first.

**Never invent Nova syntax by analogy with other languages.** The spec is the ground truth.

## Writing tests

Test files live in `nova_tests/`. Each `.nv` file uses `EXPECT` markers to declare expected output or error:

```nova
// EXPECT: hello
fn main() Io -> () => print("hello")
```

Error tests declare the expected failure with an `EXPECT_*` marker, matched as a substring against the first ~30 lines:

```nova
// EXPECT_COMPILE_ERROR: type mismatch
```

Other markers: `EXPECT_RUNTIME_PANIC`, `EXPECT_EXIT` / `EXPECT_EXIT_CODE`, `EXPECT_STDOUT`, `EXPECT_STDERR`, `EXPECT_TIMEOUT`, `EXPECT_LINT_WARNING`. The runner classifies a test by its marker (not by folder or filename suffix), so `neg/` and `_neg` are human signals only. Full list: [docs/test-conventions.md](docs/test-conventions.md).

A test file for a new feature `X` goes in `nova_tests/<category>/X.nv`. For a soundness regression, add `// SOUNDNESS_REGRESSION` in the first lines.

Full marker reference: [docs/test-conventions.md](docs/test-conventions.md).

## Followup markers (`[M-…]`)

Deferred work is tracked with `[M-<kebab-name>]` markers in docs and code comments.

- **Plan-bound** markers (followups of a specific plan) live in that plan's **Followups** section in `docs/plans/<plan>.md`.
- **Floating** markers (cross-cutting, not owned by any plan) — the *open* ones are listed in [docs/plans/backlog-followups.md](docs/plans/backlog-followups.md), the curated **OPEN-view** (what is still live and actionable).
- [docs/simplifications.md](docs/simplifications.md) is the append-only **history log** of all markers/simplifications — *not* a status view. It records that a marker existed; it does not tell you whether it is still open.

**Lifecycle:**

1. Create a floating marker → add a row to `backlog-followups.md` **and** log the change in `simplifications.md` (house style).
2. Resolve it → **remove the row** from `backlog-followups.md` (the history stays in `simplifications.md` and the commit). Keep the OPEN-view short — only live items.
3. When a marker grows into its own plan → move it to that plan's Followups and drop it from the backlog.
4. Before starting work in a subsystem, scan `backlog-followups.md` for relevant open items.

## Contribution rules

- **DCO sign-off required** on every commit — CI enforces this:
  ```sh
  git commit -s -m "your message"
  ```
- **`git add` specific files only** — never `git add .` or `git add -A`. Multiple agents may work in parallel worktrees.
- **One commit per logical task.** Multiple tasks → multiple commits.
- **No `Co-Authored-By: <AI tool>` trailers** in commit messages. A repo hook strips them automatically — do **not** add the trailer by hand (and no need to check for it manually; the hook removes it on commit).
- **Language convention.** Commit *subjects* use English conventional-commits (`fix(...)`, `docs(...)`). Commit *bodies* and the project's internal dev logs (`docs/project-creation.txt`, `docs/simplifications.md`, and the team's discussion log) are written in **Russian** with English technical terms inline — the house style; match the surrounding entries rather than switching to all-English prose. Public-facing docs (`README.md`, `AGENTS.md`, `CONTRIBUTING.md`) stay in English.
- **License:** code is `MIT OR Apache-2.0`; docs are `CC-BY-4.0`. See [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).

## Key reference files

| File | What it covers |
|---|---|
| [docs/dev-workflow.md](docs/dev-workflow.md) | **How development works** — sources of truth, plan-driven dev, worktrees, the daily loop, operational rules |
| [spec/decisions/README.md](spec/decisions/README.md) | Index of all D-blocks |
| [docs/plans/README.md](docs/plans/README.md) | Index of all plans |
| [docs/plans/backlog-followups.md](docs/plans/backlog-followups.md) | Registry of floating `[M-…]` followup markers **not** tied to a plan (codegen / perf / debug-info backlog). Plan-bound markers live in their plan's Followups section. |
| [docs/test-conventions.md](docs/test-conventions.md) | EXPECT markers, test runner flags |
| [docs/module-conventions.md](docs/module-conventions.md) | **Designing any Nova module (std/app/third-party) + C integration** — effect-family architecture (mockable plumbing + type-method facade), value/must-consume types, structured `Result` errors, byte-first, the `extern "C"` `ffi.nv` layer (CStr vs `(*u8,len)`, errno, value-records), `#cfg` platform-split. (`extern "nova"`/runtime park-wake/`#stable` are std-runtime-only — marked in §Применимость.) Complements [ffi-cookbook.md](docs/ffi-cookbook.md) (FFI mechanics) and [nv-coding-style.md](docs/nv-coding-style.md) (`.nv` style). |
| [docs/simplifications.md](docs/simplifications.md) | History of removed complexity |
| [compiler-codegen/README.md](compiler-codegen/README.md) | Compiler internals, build options |
| [docs/nova-cli.md](docs/nova-cli.md) | CLI command reference |
