---
source_rev: d5199ce7d
source_date: 2026-08-03
---

> **Informative translation; the Russian text is normative.**
>
> Russian original (normative): [overview.md](overview.md)

# Nova — overview

## Central idea

Network, disk, time, randomness, logging, errors, mutation — in Nova these
are all **effects**. A function declares in its signature the effects it
uses itself; calls to other functions do not pull those functions' effects
up into the caller's signature (the exception is `Fail` — errors are visible
transitively). Each effect has a **handler** that intercepts its operations.

Everything else in the language follows from a single abstraction (algebraic
effects in the Koka/Effekt style, brought to a production-ready state).
See [revolutionary.md](revolutionary.md) for the full treatment.

### `effect` vs `protocol`

Nova has two different ways to describe "something with operations":

- **"How to do something"** — a function declares that it needs certain
  operations, and which implementation sits underneath them is decided by
  the calling code via a `with`-block (for production — Postgres, for
  tests — in-memory). This is an **effect**, declared via
  `type X effect { ... }`.
- **"What a value can do"** — the implementation is tightly bound to the
  type: `int` hashes this way, `str` that way, and that cannot be changed.
  This is a **protocol**, declared via `type X protocol { ... }`.

**When to use an effect and when a protocol in code:** if you want a
different implementation for testing — it is an effect. If testing just
means working with values of the type and there is nothing to substitute —
it is a protocol.

## Killer use-case

**AI-first programming.** When an LLM writes 50–80% of the code, the language
needs:
- side effects visible in the signature (effects)
- compile-time guarantees instead of runtime checks (contracts, capabilities)
- context locality (one function understandable without reading 10 files)
- compiler errors as a learning signal for the LLM
- syntax stability (LLMs learn from old data)

All existing languages were designed before the AI era. Nova is the first
language explicitly optimized for the pair "LLM writes, human reviews".

## Supporting decisions

1. **Compiled ONLY through a C backend (AOT), like Go/Rust.** The early
   idea of "one source, three execution modes" (AOT/JIT/interpreter) is
   **not implemented**: `nova run file.nv` (tree-walking interpreter) is
   retracted — the command remains in the CLI only as a stub that clearly
   reports this and points to `nova build`/`nova test` (see
   [`docs/dev/read-project.md`](../docs/dev/read-project.md)).
   Code is tested and shipped only through C-codegen too — there is no
   separate "interpreted" path with different semantics.
2. **Memory: managed by default (current: Boehm conservative GC; v1.0+:
   concurrent GC), regions opt-in for real-time.** The programmer writes code
   without memory prefixes — allocations are freed automatically. **Current
   state of the bootstrap runtime** ([Plan 27](../docs/plans/27-gc-switch.md),
   default since 2026-05-11): Boehm GC, measured pauses (see
   `nova_tests/concurrency/gc_pause_bench.nv`) on x86_64-v3 Windows debug-build:
   - 10k objects × 20 rounds: max < 16ms, p99 ≈ avg ≈ 0ms (within the tick
     of GetTickCount64 — Windows timer gran 15.6ms).
   - 100k objects × 10 rounds: max < 16ms.
   - 1M objects × 3 rounds: max < 16ms.

   These are **upper bounds via a low-res timer**; real pauses are most
   likely smaller. Hi-res measurement (uv_hrtime) is a separate task after
   bootstrap.

   **Design goal for v1.0+:** concurrent GC, p99 < 1ms on typical workloads
   ([decisions/05-memory.md#d6](decisions/05-memory.md#d6),
   [Plan 25 G3b](../docs/plans/25-production-readiness-roadmap.md#g3-memory-management--главное-упрощение-runtimeа)).

   Escape analysis keeps everything that does not escape on the stack
   (no GC overhead). For real-time zones (audio, trading, embedded) there is
   the `#realtime nogc fn` attribute
   ([D172 §7](decisions/06-concurrency.md#d172-realtimeblocking-sync-class-annotation-system-plan-1036);
   historically the `realtime nogc { }` block, [D64](decisions/04-effects.md#d64),
   retracted in Plan 113), combinable with `region { }` for arena allocations
   (⚠ `region` is not implemented in the current compiler).

   **Introspection API** ([Plan 32](../docs/plans/32-gc-introspection.md)):
   `gc.heap_size()`, `gc.collect()`, `gc.live_count()` available without import.
3. **Structural typing + type inference everywhere.**
4. **Protocols + data instead of classes.** No inheritance. Structural
   contracts via `protocol` (see [decisions/01-philosophy.md#d1](decisions/01-philosophy.md#d1), [decisions/02-types.md#d42](decisions/02-types.md#d42)).
5. **Contracts in the signature.** `requires`/`ensures`/`invariant` —
   optional, but statically checked where possible.
6. **Structured concurrency on top of an M:N scheduler (runtime codename —
   Vela).** `spawn`/`supervised`/`detach`/cancel tokens — the same fibers
   (mco-coroutines) that carry the async/await infrastructure from the
   section above. **`main()` itself runs as a fiber** ([D92](decisions/06-concurrency.md#d92),
   retraction of Rule 6, 2026-07-25) — blocking park/wake operations
   (`Time.sleep`, `TcpListener.accept()`, `Channel.recv()`) are legal
   **directly in `main()`**, without wrapping them in `supervised { spawn { … } }`.
   The same holds **directly in the body of `supervised { … }`** without an
   intermediate `spawn` ([D439](decisions/06-concurrency.md#d439),
   2026-07-30) — but such a direct blocking operation is NOT protected by the
   `timeout:`/`cancel:` of that scope (enforcement lives only in the join
   loop, which starts after the body's statements); to be protected by a
   deadline/token you need `spawn { … }`.
   Failure supervision is an ordinary `Supervisor` effect
   ([D416](decisions/06-concurrency.md#d416)): ready-made policies
   `escalate()`/`stop()`, custom ones — a handler literal
   `on_child_fail(idx, err) -> Decision`. Note: the `on_child_fail`
   serialization documented in D416§2 on the runtime's drive fiber is NOT yet
   guaranteed (disproved by measurement on 2026-07-31, registry №173) —
   mut-captures in such a handler are checked by the D441 enforcer like in
   any other, no exception. Naming conventions for this layer —
   [`docs/dev/mn-coding-conventions.md`](../docs/dev/mn-coding-conventions.md).
   **Memory model between fibers** ([D415](decisions/06-concurrency.md#d415-data-race-freedom--share-атрибут-capture-check-consume-в-spawn-plan-1733),
   [D441](decisions/06-concurrency.md#d441), 2026-07-31): a `mut`-capture is a
   linear resource of a single fiber; it can cross the boundary
   (`spawn`/`detach`/`parallel for`/channel/`with`-handler around a
   fiber-containing body) only via an explicit move (`consume`), a `ro`-view,
   or a value from the whitelist of synchronized types (`Atomic*`,
   `Mutex`, channel ends, `#share`-types) — checked transitively, including
   when a closure crosses the boundary as DATA (as a parameter/via a channel),
   not only on a direct syntactic capture. The same covers a precomputed
   handler (`ro h = |..| {..}` then `with X = h { … }`) and the transitive
   installation of a handler by parameter (A-V10, D441 §5, 2026-07-31). A
   separate axis is **`#thread_affine extern fn`** (A-V10, D441 §5): marks an
   M:N-unsafe C-side list (thread-local state), raised transitively along the
   call graph, gated at the `spawn`/`detach`/`parallel for` boundary.

## What it borrows from whom

| Feature | Source |
|------|----------|
| Algebraic effects + handlers | Koka, Effekt, Eff |
| Compilation speed, simple syntax | Go |
| Performance, traits, monomorphization | Rust |
| Concurrent GC, memory simplicity for backends | Go, Java ZGC |
| Pattern matching, ADT, sum types | OCaml/Rust |
| Memory regions | Zig, Odin |
| Structured concurrency, supervision | Erlang/OTP, Swift |
| Contracts, refinement types | Eiffel, Dafny, F* |
| Capability security | E, Pony |

## Tooling out of the box

**Today** — implemented in the `nova` CLI ([nova-cli/](../nova-cli/)):

- `nova build file.nv` — a static binary via the C backend (the only
  execution path, see "Supporting decisions" item 1)
- `nova check [paths]` — typecheck + lint without building (`--strict-effects` —
  Plan 197, transitive effects as hard errors; `--lint` — the same
  convention rules as `nova lint`)
- `nova test [filter]` — discovery + parallel run of `.nv` tests
  (C-codegen pipeline; structured errors with EXPECT markers for
  negative tests, D89)
- `nova lint` — a registry of conventional `W_*` rules (Plan 185),
  info mode by default, `--deny` — a CI gate
- `nova doc file.nv [--format markdown|json|html]` — documentation generator:
  doc-tests (`--test`), coverage (`--coverage[-threshold]`), watch mode,
  mutation-testing for contracts (`--mutate-contracts`) — Plan 45, CLOSED
- `nova bench file.nv` — running benchmarks (release mode, samples,
  regression gate) — Plan 57
- `nova add`/`nova update`/`nova info` — dependency management
  (git/path dependencies + `nova.lock.toml`; `nova info --diff` —
  the effect-surface diff of a package's public API as a supply-chain gate) —
  Plan 03.1-03.4. Download proxy (`NOVA_PKG_PROXY` /
  `nova.override.toml` `[net] proxy` / `~/.nova/config.toml`) — Plan 233
- `nova regen-runtime [--check]` — regeneration of `std/runtime/*.nv`
  stubs from `runtime_registry.rs` (Plan 13)
- `nova daemon start/stop/status` — a resident build daemon (only a
  latency optimization for repeated `nova build`, behavior byte-identical
  without it) — Plan 219
- **LSP** (`nova-lsp/`) — completion/hover/diagnostics/goto/rename,
  fully implemented (Plan 104.10, "V2 production", CLOSED 2026-07-04);
  development conventions — [`docs/dev/lsp-conventions.md`](../docs/dev/lsp-conventions.md)
- `nova run file.nv` — **NOT supported**: the command remains in the CLI
  only as a clear error ("use `nova build`/`nova test`"),
  the interpreter itself (treewalk) is not maintained

**Roadmap** (not implemented):

- `nova fmt`
- `nova check --fragment '...'` — typechecking a single function without a project
- A content-addressed package manager (like Deno + Nix) — today's
  `nova add`/`update` is simpler (git/path dependencies + lockfile),
  no content-addressed storage has been built
- Hot reload in dev mode
- AI-friendly patches in diagnostics (for LLMs)
     interpreter that was retracted; the current form of this idea
     (if it is still alive) is not described in any known plan as of this
     revision — I am not inventing a new command. -->

## Ecosystem (separate repositories)

The language core (this repository) is deliberately narrow; application
layers live as separate packages/repositories on top of it:

- **`nova-http`** — a byte-oriented HTTP transport (client/server on top of `std/net`).
- **Polaris** (`nova-polaris`) — a web framework on top of `nova-http`,
  Axum/FastAPI model (Router/Handler/Middleware/extractors); in development,
  full EN+RU documentation — a separate plan (229).
- **`nova-tls`** — TLS on top of `std/net`, vendored C + `.nv` facade, no Rust
  in the runtime path.

<!-- TODO(232): the exact public status/maturity of Polaris (what is already
     stable for an external user, and not just for development) is outside
     this repository; verify with nova-polaris at the next revision. -->

## What is thrown out of ordinary languages

- **Header files, namespaces, modules-vs-packages** — one file = one module
- **Null** — only `Option[T]`
- **Exceptions as invisible control flow** — only the `Fail[E]` effect
- **`async`/`await` keywords** — suspension is ambient runtime
  ([D62](decisions/04-effects.md#d62)), effects in types: `Net`, `Io`, `Db`
- **Operator overloading on arbitrary types**
- **Macros as a preprocessor** — only typed comptime (like Zig)
- **Global mutable state** — `mut` fields/parameters
  (locally) or specialized state effects (Counter, Cache)
- **DI via reflection** — dependencies in effects or parameters
- **Mock libraries** — handlers from the language
- **Hidden imports** — every identifier is visible from everywhere

## Reserved identifiers

Besides the grammar keywords (`fn`, `type`, `effect`, `handler`, `let`,
`if`, `match`, `return`, ... — about 38 words), Nova has
**identifiers with reserved semantics**. They are parsed like ordinary names,
but the compiler knows their special meaning in certain contexts.

| Identifier | Category | Where valid | See |
|---|---|---|---|
| `Self` | referential type | in any type context — refers to the receiver type of a method / the type satisfying a protocol | [D66](decisions/02-types.md#d66) |
| `any` | top type | everywhere; runtime type-tag for downcasts | [D54](decisions/03-syntax.md#d54) |
| `never` | bottom type | return type of non-returning functions (`throw`, `panic`, `loop`) | [D26](decisions/08-runtime.md#d26) |
| `Option[T]`, `Some`, `None` | sum type in the prelude | everywhere | [D26](decisions/08-runtime.md#d26) |
| `Result[T, E]`, `Ok`, `Err` | sum type in the prelude | everywhere | [D26](decisions/08-runtime.md#d26) |
| `Error` | record type in the prelude | for `throw err` | [D26](decisions/08-runtime.md#d26) |
| `RuntimeError` | sum type in the prelude | bottom-level runtime errors | [D26](decisions/08-runtime.md#d26) |
| `RuntimeNoneError` | unit type in the prelude | thrown via `expr!!` on `Option` | [D85](decisions/04-effects.md#d85) |
| `Effect[E, IRT]` | first-class type of a handler for effect `E` with interrupt-VAL type `IRT` (default `never` via D88); sugar `Effect[E]` ≡ `Effect[E, never]` | everywhere | [D61](decisions/04-effects.md#d61), [D87](decisions/04-effects.md#d87), [D88](decisions/03-syntax.md#d88) |
| `Fail[E]`, `Fail` | standard effect | in effect-row signatures | [D25](decisions/04-effects.md#d25), [D65](decisions/04-effects.md#d65) |
| `Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Log`, `Trace`, `Ask[T]`, `Alloc[R]`, `Detach`, `Blocking` | standard effects | in effect-row signatures | [D2 (REVISED)](decisions/04-effects.md#d2), [D50](decisions/06-concurrency.md#d50) |
| `int`, `i8`-`i64`, `u8`-`u64`, `f32`, `f64`, `str`, `bool`, `byte` | primitive types | everywhere | [D44](decisions/03-syntax.md#d44), [D27](decisions/03-syntax.md#d27) |

These identifiers can be **locally overridden** (for example, a `Net` type
from a user library), but that is an anti-pattern. The linter will emit
a warning.

## Main trade-offs

1. **Algebraic effects are hard to implement** — this is the cutting edge of PL,
   Koka has been around for 10+ years and is still academic.
2. **Understanding effects is a threshold** — solved **only** by the quality
   of compiler messages. If they are academically precise and humanly
   incomprehensible — the language is dead.
3. **The performance of effects** requires aggressive optimization (static
   handler resolution, inlining).
4. **Betting on AI coding** as the dominant trend is statistically likely,
   but not guaranteed.
5. **9 out of 10 such projects fail.** That is the normal risk
   of a revolutionary attempt. The alternative — a guaranteed "yet another Nim".
