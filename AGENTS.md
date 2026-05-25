# AGENTS.md

> Instructions for AI agents and coding assistants working in this repository.
> Think of this as a README for agents. Human contributors: see [README.md](README.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

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

Error tests use `EXPECT_ERR`:

```nova
// EXPECT_ERR: type mismatch
```

A test file for a new feature `X` goes in `nova_tests/<category>/X.nv`. For a soundness regression, add `// SOUNDNESS_REGRESSION` in the first lines.

Full marker reference: [docs/test-conventions.md](docs/test-conventions.md).

## Contribution rules

- **DCO sign-off required** on every commit — CI enforces this:
  ```sh
  git commit -s -m "your message"
  ```
- **`git add` specific files only** — never `git add .` or `git add -A`. Multiple agents may work in parallel worktrees.
- **One commit per logical task.** Multiple tasks → multiple commits.
- **No `Co-Authored-By: <AI tool>` trailers** in commit messages.
- **License:** code is `MIT OR Apache-2.0`; docs are `CC-BY-4.0`. See [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).

## Key reference files

| File | What it covers |
|---|---|
| [spec/decisions/README.md](spec/decisions/README.md) | Index of all D-blocks |
| [docs/test-conventions.md](docs/test-conventions.md) | EXPECT markers, test runner flags |
| [docs/simplifications.md](docs/simplifications.md) | History of removed complexity |
| [compiler-codegen/README.md](compiler-codegen/README.md) | Compiler internals, build options |
| [docs/nova-cli.md](docs/nova-cli.md) | CLI command reference |
