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
        samples.map((x) => x * gain)   // no GC, no suspension
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
