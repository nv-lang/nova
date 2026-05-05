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
- [compiler-bootstrap/](compiler-bootstrap/) — treewalk interpreter (Rust)
- [compiler-codegen/](compiler-codegen/) — C-backend compiler (Rust → C → native)

## What follows from a single idea

One idea: **anything impure is an effect, any effect is intercepted by
a handler**. From that, the following fall out automatically:

- Tests without mocks (handler substitution)
- Transactions, undo/redo, snapshot (handler `db`/`mut`)
- Capability security (forbid effects in a scope)
- Time-travel debugging (record handler calls)
- Deterministic repro (handlers `time` + `random` with fixed seed)
- Erlang-style supervision (structured `par` + restart handler)
- LLM-safe code (side effects are visible in the type signature)

## Memory: managed by default, regions opt-in

**The programmer writes, the GC works.** No memory prefixes in regular
code. Cycles are reclaimed automatically. A modern concurrent GC keeps
pauses below 1ms.

For real-time zones (audio, trading, embedded) — the `Realtime` effect
in the signature. The compiler wraps the body in a region automatically
(GC off inside). An explicit `region { ... }` block is only needed to
manage multiple arenas manually:

```nova
fn map_audio(samples []f32, gain f32) Realtime -> []f32 =>
    samples.map((x) => x * gain)   // implicit region, no GC pauses
```

For perf-critical code the compiler uses **escape analysis** —
non-escaping values stay on the stack with no allocations. The
programmer writes nothing special. See [spec/decisions/05-memory.md#d6](spec/decisions/05-memory.md#d6).

## Status

Active development. The specification is stable across core features (effects,
handlers, syntax, memory, concurrency). Two compilers exist:

- **compiler-bootstrap** — treewalk interpreter, runs all spec tests
- **compiler-codegen** — compiles Nova to C via a native runtime (effects,
  fibers, GC); used for benchmarking and validation against the spec

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
