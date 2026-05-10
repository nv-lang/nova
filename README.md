**English** | [Русский](README.ru.md)

# Nova

A programming language with **one central abstraction**
(algebraic effects + handlers) and **one killer use-case** (AI-first
programming with verifiable LLM-written code).

> ⚠️ The full language specification is currently only available in
> Russian. See [spec/decisions/](spec/decisions/) and [spec/](spec/). This
> README gives an English overview of the core ideas.

## Core thesis

> **Nova is a language in which an LLM can write code a human can
> trust — because effects make everything visible, contracts make
> everything verifiable, and handlers make everything testable.**

## Contents

- [spec/overview.md](spec/overview.md) — main ideas, what is borrowed from where, tooling
- [spec/revolutionary.md](spec/revolutionary.md) — **flagship features**:
  effects + handlers, AI-first design, contracts, time-travel debugging
- [spec/syntax.md](spec/syntax.md) — syntax examples
- [spec/effects.md](spec/effects.md) — effect system (introduction)
- [spec/open-questions.md](spec/open-questions.md) — unresolved questions
- [spec/decisions/](spec/decisions/) — design decision log with rationale
- [compiler-codegen/](compiler-codegen/) — Nova compiler (Rust): parser, type-checker, treewalk interpreter, C-backend codegen

## What follows from a single idea

One idea: **anything impure is an effect, any effect is intercepted by
a handler**. From that, the following fall out automatically:

- Tests without mocks (handler substitution)
- Transactions, undo/redo, snapshot (handler `Db`)
- Capability security (`forbid X { ... }` blocks an effect in a scope)
- Time-travel debugging (record handler calls)
- Deterministic repro (handlers `Time` + `Random` with fixed seed)
- Erlang-style supervision (`supervised { spawn ... }` + restart strategy)
- LLM-safe code (side effects are visible in the type signature)

## Memory: managed by default, real-time opt-in

**The programmer writes, the GC works.** No memory prefixes in regular
code. Cycles are reclaimed automatically. A modern concurrent GC keeps
pauses below 1ms.

For real-time zones (audio, trading, embedded) — a `realtime { ... }`
block. Inside it the compiler guarantees no suspension and no GC pauses;
violation is a compile-time error:

```nova
fn map_audio(samples []f32, gain f32) -> []f32 =>
    realtime {
        samples.map(|x| x * gain)      // no GC, no suspension
    }
```

For perf-critical code the compiler uses **escape analysis** —
non-escaping values stay on the stack with no allocations. The
programmer writes nothing special. See [spec/decisions/05-memory.md#d6](spec/decisions/05-memory.md#d6).

## Status

Active development. The specification is stable across core features (effects,
handlers, syntax, memory, concurrency). Single compiler:

- **compiler-codegen** — Rust implementation with parser, type-checker,
  treewalk interpreter, and C-backend codegen. Compiles Nova to C via a
  native runtime (effects, fibers, GC); used for both interactive runs
  (`run`, `test`) and native compilation (`compile`).

## Building from source

The pipeline is two-stage: `nova-codegen` produces `.c`, a native C
compiler links it with the runtime (`nova_rt/`). Wrapper scripts make
this a single command:

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
