**English** | [Русский](README.ru.md)

# Nova

```nova
fn process_order(o Order) Db Net Time Fail -> Receipt
```

Reading this single line, you know the function:

- talks to the **database** (`Db`)
- makes **network requests** (`Net`)
- reads **the clock** (`Time`) — so its result depends on time
- can **throw an error** (`Fail`)
- and **nothing else**: it doesn't write files, read stdin, or use
  randomness — otherwise it would be in the signature.

This is **algebraic effects** — an idea from the academic language
Koka, brought to a practical form. When side effects are visible in
the type, review becomes local: you can verify a function without
reading its body or the bodies of all the things it calls.

> **Nova's main bet:** more and more code will be written by LLMs,
> but humans will still review it. Languages designed before the AI
> era are optimized for the opposite ratio. Nova is the first
> language explicitly optimized for the «LLM writes, human reviews»
> pair.

> ⚠️ The full language specification is currently only available in
> Russian. See [spec/decisions/](spec/decisions/) and [spec/](spec/).
> This README gives an English overview of the core ideas.

## Show me the code

### 1. Effect → handler → tests without mocks

```nova
// Declare an effect — a contract of operations, no fields
type Db effect {
    query(q Sql) -> []Row
    exec(q Sql)  -> ()
}

// Business logic: Db effect in the signature, implementation unknown
fn transfer(from u64, to u64, amount money) Db Fail -> () {
    let src = Db.query(sql`SELECT * FROM accounts WHERE id = ${from}`)
    if src[0].balance < amount { throw InsufficientFunds }
    Db.exec(sql`UPDATE accounts SET balance = balance - ${amount} WHERE id = ${from}`)
    Db.exec(sql`UPDATE accounts SET balance = balance + ${amount} WHERE id = ${to}`)
}

// Production: real handler
fn main() Io Fail -> () =>
    with Db = postgres("postgres://...") {
        transfer(1, 2, 100)
    }

// Test: same code, in-memory handler, no mocks at all
test "transfer moves money" {
    let mem = in_memory_db([
        Account { id: 1, balance: 500 },
        Account { id: 2, balance: 0 },
    ])
    with Db = mem {
        transfer(1, 2, 100)
        assert(mem.get(1).balance == 400)
        assert(mem.get(2).balance == 100)
    }
}
```

The same `transfer` runs in production and in tests — because the
`Db` implementation is supplied via `with`, not hard-wired in the
code. No DI framework, no mocking library.

### 2. Concurrency without `async`/`await`

```nova
fn check_all(urls []str) Net Fail -> []HealthStatus =>
    parallel for url in urls {
        let resp = Http.get(url)!!
        HealthStatus { url, code: resp.status, latency: resp.elapsed }
    }
```

The return type is `[]HealthStatus`, not `Future<[]HealthStatus>`.
**Function colour does not exist** — `Http.get` is not declared
async/sync, it declares the `Net Fail` effect in its signature, and
that's enough.

`parallel for` is structured concurrency: all requests run in
parallel, the scope waits for all of them, the tail is cancelled on
error and `throw` propagates to the caller via the `Fail` effect —
the same error-handling mechanism as in synchronous code. The same
`Http.get` works in a regular loop and in `parallel for` — without
changing the signature.

### 3. Deterministic random in tests

```nova
fn pick_winner(participants []str) Random -> str =>
    participants[Random.range(0, participants.len())]

test "winner is deterministic with seed" {
    let people = ["alice", "bob", "carol", "dave"]
    with Random = seed(42) {
        assert(pick_winner(people) == "carol")
        assert(pick_winner(people) == "alice")
    }
}
```

`Random` is an ordinary effect. In production — a real generator;
in tests — a fixed seed, and the result is **reproducible**. No
`MockRandom`, no patches. The same `pick_winner` works in both
cases.

### 4. Contracts — a gradient from Go to F\*

```nova
fn withdraw(mut acc Account, amount money) Fail -> ()
    requires amount > 0
    requires acc.balance >= amount
    ensures  acc.balance == old(acc.balance) - amount
=>
    acc.balance -= amount
```

Contracts are **optional**. Without them the code runs as in Go.
With them, the compiler tries to prove invariants statically (like
F\* / Dafny); what it can't prove is turned into a runtime check in
debug mode and stripped in release.

The same language covers a spectrum from a script to
correctness-critical code — write as many contracts as you need.

## What follows from a single idea

| Feature | How it falls out of effect+handler |
|---|---|
| Tests without mocks | Handler substitution via `with` |
| Transactions | A `Db` handler buffers operations, commits at scope exit |
| Capability security | `forbid Net, Fs { ... }` blocks an effect — compile error |
| Time-travel debugging | Record handler calls → replay |
| Erlang-style supervision | `supervised { spawn ... }` + handler restart strategy |
| LLM-safe code | Side effects are visible in the function signature |

## Memory: managed by default, real-time opt-in

**The programmer writes, the GC works.** No memory prefixes in
regular code. Cycles are reclaimed automatically. A modern
concurrent GC keeps pauses below 1ms.

For real-time zones (audio, trading, embedded) — a `realtime { ... }`
block. Inside it the compiler guarantees no suspension and no GC
pauses; violation is a compile-time error:

```nova
fn map_audio(samples []f32, gain f32) -> []f32 =>
    realtime {
        samples.map(|x| x * gain)      // no GC, no suspension
    }
```

For perf-critical code the compiler uses **escape analysis** —
non-escaping values stay on the stack with no allocations. The
programmer writes nothing special.

## What's removed from typical languages

- **Header files, `package`/`module` dualism** — one file is one module.
- **`null`** — only `Option[T]`.
- **Invisible exceptions** — only the `Fail[E]` effect, visible in the signature.
- **`async`/`await` keywords** — suspension is ambient runtime, effects in types: `Net`, `Io`, `Db`.
- **Operator overloading on arbitrary types** — only standard ones via `@plus`, `@times`, ...
- **Macros as preprocessor** — only typed comptime (Zig-style).
- **Global mutable state** — `mut` fields/parameters locally, or named state effects (`Counter`, `Cache`).
- **DI through reflection** — dependencies in effects or parameters.
- **Mocking libraries** — handlers from the language itself.

## Contents

- [spec/overview.md](spec/overview.md) — main ideas, what is borrowed from where, tooling
- [spec/revolutionary.md](spec/revolutionary.md) — **flagship features**: effects + handlers, AI-first design, contracts, time-travel debugging
- [spec/syntax.md](spec/syntax.md) — syntax examples
- [spec/effects.md](spec/effects.md) — effect system (introduction)
- [spec/open-questions.md](spec/open-questions.md) — unresolved questions
- [spec/decisions/](spec/decisions/) — design decision log with rationale
- [compiler-codegen/](compiler-codegen/) — Nova compiler (Rust): parser, type-checker, treewalk interpreter, C-backend codegen

## Status

Active development. The specification is stable across core features
(effects, handlers, syntax, memory, concurrency). Single compiler:

- **compiler-codegen** — Rust implementation with parser,
  type-checker, treewalk interpreter, and C-backend codegen.
  Compiles Nova to C via a native runtime (effects, fibers, GC);
  used for both interactive runs (`run`, `test`) and native
  compilation (`compile`).

## Building from source

The pipeline is two-stage: `nova-codegen` produces `.c`, a native C
compiler links it with the runtime (`nova_rt/`). Wrapper scripts
make this a single command:

```powershell
# Windows (requires MSVC Build Tools)
cd compiler-codegen
cargo build
.\build_c.ps1 path\to\hello.nv -Run
```

```sh
# Linux / Mac (requires gcc or clang)
cd compiler-codegen
cargo build
./build_c.sh path/to/hello.nv --run
```

Without the wrapper:

```sh
cd compiler-codegen
cargo run -- compile path/to/hello.nv          # Nova → C
gcc path/to/hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
    -I. -o hello                                # C → binary
./hello
```

There are also non-codegen modes: `cargo run -- run file.nv` (treewalk
interpreter), `cargo run -- check file.nv` (type-check only),
`cargo run -- test file.nv` (run `test "..."` blocks via interpreter).

Full guide, options, known limitations:
[compiler-codegen/README.md](compiler-codegen/README.md).

## Running tests

After `cargo build`, run the full test suite via the cross-platform
wrappers:

```powershell
# Windows
.\run_tests.ps1
```

```sh
# Linux / macOS
./run_tests.sh
```

Both wrappers are thin shims over `nova-codegen test-all` — the actual
runner (toolchain detection, EXPECT-marker parsing, parallel scheduler,
per-test timeout, JSON/TAP output, `--rerun-failed`) lives in Rust at
[compiler-codegen/src/test_runner.rs](compiler-codegen/src/test_runner.rs).

Common flags (identical names in `.ps1` PascalCase and `.sh` kebab-case):

```powershell
.\run_tests.ps1 -Filter syntax/closure        # subset of tests
.\run_tests.ps1 -Mode release                 # -O3 -flto compilation
.\run_tests.ps1 -Toolchain clang              # force toolchain
.\run_tests.ps1 -Jobs 4 -Timeout 60           # parallel + timeout
.\run_tests.ps1 -Format json                  # JSON events (one per line)
.\run_tests.ps1 -Format junit > results.xml   # JUnit XML for CI parsers
.\run_tests.ps1 -Retries 2                    # retry transient AV/race fails
.\run_tests.ps1 -RerunFailed                  # only failed-last-time
```

```sh
./run_tests.sh --filter basics --mode release
./run_tests.sh --jobs 8 --format tap
./run_tests.sh --include-stdlib               # include std/* alongside nova_tests/*
```

Single-test debugging (no walkdir, no parallel overhead):

```sh
./compiler-codegen/target/debug/nova-codegen test-build nova_tests/basics/literals.nv \
    --toolchain clang --keep-artifacts
```

Toolchain setup:
- **Windows:** `winget install LLVM.LLVM` (Clang, recommended) +
  Visual Studio Build Tools (MSVC SDK + linker, required by Clang too).
- **Linux:** `apt install clang` or `dnf install clang`; GCC usually
  pre-installed.
- **macOS:** `xcode-select --install` (Apple Clang).

Auto-detection picks Clang first, then MSVC (Windows) or GCC (Linux).
Override with `-Toolchain clang|msvc|gcc` or via env-vars
(`NOVA_CLANG`, `NOVA_GCC`, `NOVA_VCVARS`).

Full reference of test-runner flags, EXPECT-markers, troubleshooting:
[docs/test-conventions.md](docs/test-conventions.md).

## Editor support

Syntax highlighting plugins for several editors are in
[editors/](editors/). All are TextMate grammar / handcrafted — no
semantic analysis (LSP is not yet implemented).

| Editor | Subdir | Notes |
|---|---|---|
| VSCode / Cursor / VSCodium | [`editors/vscode/`](editors/vscode/) | TextMate grammar |
| Sublime Text / TextMate | [`editors/sublime/`](editors/sublime/) | reuses VSCode `.tmLanguage.json` |
| Vim / Neovim | [`editors/vim/`](editors/vim/) | handcrafted `syntax/nova.vim` |
| Emacs | [`editors/emacs/`](editors/emacs/) | major-mode `nova-mode.el` |

See [editors/README.md](editors/README.md) for the full overview,
install commands per editor, and roadmap (LSP, tree-sitter, JetBrains).

## License

Nova is dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

`SPDX-License-Identifier: MIT OR Apache-2.0`

Documentation and the language specification are licensed under
[CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).

### Contributions

Any contribution intentionally submitted for inclusion in the project
is dual-licensed as `MIT OR Apache-2.0`, without any additional terms
or conditions — per Section 5 of the Apache License 2.0.

See [CONTRIBUTING.md](CONTRIBUTING.md) for details. In short: commits
must be DCO-signed (`git commit -s`); this is enforced by CI.
