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
regular code. Cycles are reclaimed automatically. Boehm GC runs by default — conservative, with stop-the-world
pauses under 16ms measured in practice. Concurrent incremental GC
is on the v1.0 roadmap (Plan 25).

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
  Compiles Nova to C via a native runtime (effects, fibers, GC, channels);
  used for both interactive runs (`run`, `test`) and native
  compilation (`build`).
- **nova-cli** — single user-facing entry point (`nova check`,
  `nova build`, `nova run`, `nova test`, `nova regen-runtime`).
  `nova-codegen` остаётся как внутренний инструмент для IDE / CI /
  отладки.

What works today (bootstrap):

- Cross-file imports (`import X.Y.Z`, selective `import X.{A, B}`,
  `export import X`, prelude auto-import) with DFS cycle detection.
- **Folder-modules** (D29 rev-3 / Plan 42): module = single-file `X.nv`
  ИЛИ folder `X/` with peer files (Go-style). All peers declare same
  `module parent.X` and share namespace. Internal helpers without
  `export`. Test isolation via `_test.nv` suffix. `internal/` directory
  for library boundaries. File-level `#forbid Net, Fs` capability
  attribute (Nova-unique).
- Effects + handlers (D61/D87): `effect`/`handler` keywords,
  `with X = h { body }`, `interrupt v`, `Handler[E, IRT]` first-class
  type. `forbid`, `realtime` capability blocks.
- Structured concurrency (D71/D75/D92): `spawn`, `supervised`,
  `supervised(cancel: tok)`, `parallel for`, `channels`, `select`.
- **M:N runtime** (Plans 44.1–44.7): work-stealing scheduler,
  per-worker libuv event loop, preemption (D103), GC_THREADS.
- Contracts (D24): `requires`/`ensures`/`old`/`result`/`invariant`/
  `reads`/`modifies`/`decreases`/`ghost let`/`assume`/`assert_static`.
  Bootstrap SMT через TrivialBackend (reflexive ensures); Z3 — milestone.
- `defer` / `errdefer` cleanup (D90).
- Boehm GC default with introspection API (`heap_size`, `live_count`,
  `collect`).

## Building from source

Build the `nova` CLI, then use it to compile Nova programs:

```sh
# build nova CLI (requires Rust + Cargo)
cd nova-cli && cargo build --release && cd ..

# compile a Nova file to a native binary
nova-cli/target/release/nova build path/to/hello.nv

# run via interpreter (no native compilation)
nova-cli/target/release/nova run path/to/hello.nv

# type-check only
nova-cli/target/release/nova check path/to/hello.nv
```

The pipeline is two-stage: `nova-codegen` (internal) produces `.c`, a
native C compiler links it with the runtime (`nova_rt/`). `nova build`
orchestrates this automatically.

Manual pipeline (without `nova` CLI):

```sh
cd compiler-codegen
cargo run -- compile path/to/hello.nv          # Nova → C
gcc path/to/hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
    -I. -o hello                                # C → binary
./hello
```

Full guide, options, known limitations:
[compiler-codegen/README.md](compiler-codegen/README.md).

## Running tests

Build `nova` CLI, then run the full test suite:

```sh
# build nova CLI (one-time, or after changes)
cd nova-cli && cargo build --release && cd ..

# run all tests
nova-cli/target/release/nova test
```

Common flags:

```sh
nova test --filter syntax/closure        # subset of tests
nova test --mode release                 # -O3 -flto compilation
nova test --toolchain clang              # force toolchain
nova test --timeout 60                   # timeout per test
nova test --format json                  # JSON events (one per line)
nova test --format junit > results.xml   # JUnit XML for CI parsers
nova test --retries 2                    # retry transient AV/race fails
nova test --rerun-failed                 # only failed-last-time
nova test --include-stdlib               # include std/* alongside nova_tests/*
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
Override with `--toolchain clang|msvc|gcc` or via env-vars
(`NOVA_CLANG`, `NOVA_GCC`, `NOVA_VCVARS`).

Full reference of test-runner flags, EXPECT-markers, troubleshooting:
[docs/test-conventions.md](docs/test-conventions.md).

## Documentation (`nova doc`)

Generate documentation from `///` and `//!` doc-comments with
doc-tests, intra-doc-links, stability/deprecation, JSON Schema 2020-12
output:

```sh
nova doc src/api.nv                # Markdown to stdout
nova doc src/api.nv --format json  # JSON (D107 schema v1)
nova doc src/api.nv --test         # run doc-tests
nova doc src/api.nv --check        # validate (broken links, missing summaries)
```

Full user guide: [docs/nova-doc.md](docs/nova-doc.md).

## SMT verification + Z3 setup

Nova включает статический верификатор контрактов (`requires`/`ensures`/`invariant`).
По умолчанию используется **TrivialBackend** (reflexive tautologies, constant folding) —
работает без внешних зависимостей. Для полноценной верификации нужен **Z3**.

### Без Z3 (по умолчанию)

Работает сразу после обычной сборки. Доказывает только рефлексивные
контракты и константные выражения. Z3-тесты автоматически SKIP.

```bash
cd nova-cli && cargo build --release
nova test nova_tests/contracts/
# PASS: 82  SKIP: 9 (z3-only)
```

### С Z3

**Шаг 1: установить Z3 через vcpkg** (один раз)

```bash
# Windows:
cd compiler-codegen
vcpkg install --triplet x64-windows-static --x-manifest-root=.

# Linux:
cd compiler-codegen
vcpkg install --triplet x64-linux --x-manifest-root=.

# macOS:
cd compiler-codegen
vcpkg install --triplet x64-osx --x-manifest-root=.
```

`vcpkg.json` уже содержит `z3` и `bdwgc` — обе зависимости устанавливаются
одной командой. Результат: `vcpkg_installed/<triplet>/lib/libz3.a`.

**Шаг 2: собрать с feature `z3-backend`**

```bash
cd nova-cli
cargo build --release --features z3-backend
```

**Шаг 3: запустить с Z3**

```bash
NOVA_SMT_BACKEND=z3 nova test nova_tests/contracts/
# PASS: 91  SKIP: 0
```

> `VCPKG_TRIPLET` переопределяет triplet если нужен нестандартный
> (например `arm64-linux`).

Подробнее: [docs/plans/33-contracts-implementation.md](docs/plans/33-contracts-implementation.md) — раздел «Z3 dev-setup».

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
