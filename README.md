**English** | [Русский](README.ru.md)

# Nova

A hypothetical programming language with **one central abstraction**
(algebraic effects + handlers) and **one killer use-case** (AI-first
programming with verifiable LLM-written code).

This is a design document, not an implementation. It evolves through
discussion.

> ⚠️ The full language specification is currently only available in
> Russian. See [decisions.md](decisions.md) and [spec/](spec/). This
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
- [decisions.md](decisions.md) — design decision log with rationale

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

For real-time zones (audio, trading, embedded) — an explicit
`region { ... }` block with the `Realtime` effect, GC disabled inside:

```nova
fn process_audio(samples []f32) Realtime -> []f32 =>
    region {
        let buf = []f32.with_capacity(1024)
        // ... processing, guaranteed no GC pauses
        buf.to_owned()
    }
```

For perf-critical code the compiler uses **escape analysis** —
non-escaping values stay on the stack with no allocations. The
programmer writes nothing special. See [decisions.md D6](decisions.md).

## Status

Conceptual draft. The main goal of this document is to capture design
decisions and the reasoning behind them.

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
