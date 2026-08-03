---
source_rev: dcdf639fa
source_date: 2026-05-31
---

> **Informative translation; the Russian text is normative.**

# Nova — revolutionary features

This document describes the features that make Nova not "just another good
language", but a language with a unique claim. All of them follow from one
central idea (see [decisions/01-philosophy.md#d10](decisions/01-philosophy.md#d10)):

> **Everything is an effect. A handler is a first-class function. Killer use-case —
> AI-first programming.**

---

## R1. Algebraic effects + handlers

### Idea

Network, disk, time, randomness, logging, errors, mutation — all of these are
effects. An effect is declared via `effect`, has operations, and a
**handler** intercepts the operations and decides what to do with them.

This is a generalization of `try/catch`, `async/await`, dependency injection,
and mocks into one thing: test mocks, transaction wrappers, retry,
distributed tracing — everything is written through the same handler
mechanism, not through four different libraries.

### Basic syntax

```nova
// объявление эффекта
type Logger effect {
    log(msg str) -> ()
}

// функция, использующая эффект
fn process(x int) Logger -> int {
    Logger.log("processing ${x}")
    x * 2
}

// handler — обычное значение через `handler` keyword
ro console = effect Logger {
    log(msg) => println("[LOG] ${msg}")
}

// применение handler'а
fn main() Io -> () =>
    with Logger = console {
        process(42)   // напечатает [LOG] processing 42
    }
```

`return value` (or a final expression) in a handler method — the resumption
of the computation with the returned value. To complete the whole
`with`-block early, `interrupt v` is used (that is how `Fail` works).

**The special case of `Fail[E]`.** The `Fail[E].fail` operation has the
return type `never` — there is nothing to return to the `throw` point. So a
`Fail[E]` handler has only two outcomes: `interrupt v` (complete the
with-block) or a fresh `throw` (rethrow further). The "return value" form
is forbidden for `Fail`.

Roles in error handling:

- **`throw err`** — language syntax, raises an error. After
  `throw` control never returns to that point.
- **`Fail[E]`** — the effect contract for catching and handling the
  error. An effect has no fields, only operation signatures.
- **a `Fail[E]` handler** — what catches the error. It has no fields
  of its own, but it captures variables from the environment (like an
  ordinary closure).

### What follows from this automatically

**Testing without mocks:**

```nova
test "process logs correctly" {
    mut buf = []
    ro collect = effect Logger {
        log(msg) { buf.push(msg); return () }
    }
    with Logger = collect {
        process(42)
    }
    assert(buf == ["processing 42"])
}
```

No mock library. No DI framework. This is **just a handler**.

**Transactions:**

```nova
type Db effect {
    query(q Sql) -> []DbRow
    exec(q Sql)  -> ()
}

fn transactional(real Effect[Db]) -> Effect[Db] => effect Db {
    query(q) => return real.query(q)
    exec(q)  { staged.push(q); return () }
}

with Db = transactional(real_db) {
    transfer(1, 2, 100)
    transfer(2, 3, 50)
}  // обе операции в одной транзакции, при ошибке — откат
```

A transaction is a handler. Nested transactions are nested handlers.

**Capability security:**

```nova
fn untrusted_plugin(input str) Logger -> str {
    // плагин может только логировать; Net/Db/Fs недоступны
    Logger.log("plugin called")
    input.reverse()
}
```

If the plugin tries to use `Net.get`, the **compiler will not let it
through** — the `Net` effect is absent from the signature. This is capability
security in types, not in the runtime.

---

## R2. The standard effect set

Unlike Koka, Nova ships with a **ready-made set of effects
for application programming**. You don't have to invent them in every project.

| Effect | What it describes | Example handler |
|---|---|---|
| `Fail[E]` | Contract for catching and handling an error of type E | catch, retry, log-and-continue |
| `Io` | stdin/stdout/stderr | capture-stdout, mock-stdin |
| `Fs` | File system | virtual filesystem |
| `Net` | Network requests | record/replay, fault injection |
| `Db` | Database | transaction, in-memory storage |
| `Time` | Clock, timers, delays | virtual clock, fast-forward |
| `Random` | RNG | seeded RNG for tests |
| `Log` | Structured logging | JSON, human-readable, capture |
| `Trace` | Distributed tracing | OpenTelemetry, off |
| `Ask[T]` | Reading from context (like Reader) | config substitution |
| `Alloc[R]` | Allocation in region R | arena, GC, pool |

**Async, Mut, Par are not in** the standard effect set
([D62](decisions/04-effects.md#d62)):

- `Async` — an ambient capability, not part of the type system. The
  programmer never writes it in signatures. The fiber runtime is under the
  hood (see R7).
- `Mut` — real state-machine scenarios are covered by specialized effects
  with clear names (Counter, Cache, IdGen, etc.); a generic
  `Mut[T]` would provoke the "unnamed shared state" anti-pattern.
- `Par` — the runtime keyword `parallel for` / `spawn`, not an effect.

The function color is **absent** — there is no "sync" vs "async" split, there
is "what effects does the function have". Async never appears in types.

---

## R3. Deterministic testing mode

It follows automatically from effects: **any program can be run completely
deterministically**, if all effects are replaced with
deterministic handlers.

```nova
test "complex flow is deterministic" {
    with Time = fixed(2026-04-28T10:00:00),
         Random = seed(42),
         Net = record_or_replay("testdata/flow.json"),
         Db = in_memory() {
        ro result = run_complex_flow()
        assert(result.snapshot() == expected_snapshot)
    }
}
```

This requires no mock libraries — **effect substitution is part of the
language**. Snapshot tests, property-based, time-travel — everything is built
from this.

---

## R4. Contracts in the signature (requires/ensures/invariant)

Effects give visibility into **what** a function does. Contracts — visibility
into **under what conditions it works**:

```nova
fn withdraw(mut acc Account, amount money) Fail -> ()
    requires amount > 0
    requires acc.balance >= amount
    ensures acc.balance == old(acc.balance) - amount
    ensures result.is_ok || acc.balance == old(acc.balance)
=
    acc.balance -= amount
```

Contracts are **optional**. Without them the code works as usual.
With them the compiler tries to prove them statically (like F* / Dafny),
and what it cannot prove — turns into a runtime check in debug mode
and removes it in release.

This gives a **gradient**: you write like in Go (no contracts); you want
stronger — you add `requires`; you want full verification —
you add `ensures` and `invariant`. One and the same language covers the
spectrum from a script to correctness-critical code.

---

## R5. AI-first design as an explicit goal

### R5.1. Context locality

Not a single feature that requires reading several files to understand one
function:

- **No implicit imports** — every identifier shows where it came from
- **No DI via reflection** — dependencies in parameters or effects
- **No invisible hook annotations** (like `@Autowired`, `@Inject`)
- **No global mutable state** — mutable state only via `mut`
  fields/parameters (locally) or via specialized effects (`Counter`, `Cache` —
  names visible in the signature). The generic `Mut` effect was removed in
  [D62](decisions/04-effects.md#d62).
- **No operator overloading on arbitrary types** — only
  for standard traits
- **No macro rewriting of syntax** — comptime only over types
  and values, not over the AST

An LLM given one function **sees everything it needs to understand it**.

### R5.2. Signature = direct effects + the full throw picture

Refined in [D62](decisions/04-effects.md#d62): the signature shows the
**direct** effects of the function (the ones it uses itself) and the **full
throw picture** via the transitivity of `Fail`. Transitive side effects
through nested calls — highlighted by a warning, not mandatory to declare.

```nova
type TransferError | InsufficientFunds | InvalidAccount

fn transfer(from AccountId, to AccountId, amount money)
    Fail[TransferError]
    Db Time Log
    requires amount > 0
    ensures from != to
    -> TransferReceipt
```

(Several error types — a sum type or multi-Fail in the row
`Fail[A] Fail[B]`, [D65](decisions/04-effects.md#d65). Multi-parameters
`Fail[A, B]` rejected by [D25](decisions/04-effects.md#d25).)

From this signature the LLM (and a human) knows:
- what it takes and returns
- what errors it throws (`Fail` is transitive — this is the **full**
  throw picture, including through nested calls)
- what effects the function uses **directly** (DB, time, log)
- what input constraints
- what output guarantees

What is **not** in the signature:
- Effects the function gets only through nested calls
  (a compiler warning on detection; can be suppressed via
  `@allow_transit` or Nova.toml).
- `Async` — invisible infrastructure, never in the signature.

This is a **compromise** made by D62: full transitivity of all effects
makes real backend signatures unreadable (8-10 effects
accumulate across 5 call levels). Direct + Fail-strict — a balance
between "the signature tells the truth" and "the signature is readable".

In Java/Python/Go this information is **not in the signature** — it is in
the code, or not there at all. The LLM has to read the body and guess. Nova
stays **ahead of the mainstream** in throw visibility + direct effects, it
just does not go all the way to full transitive visibility of side effects.
