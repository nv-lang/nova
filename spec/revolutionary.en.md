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

### R5.3. Compiler errors as a learning signal

Every error message has a structure optimized for an LLM:

```
error E0142: missing effect `Net`

  in function `fetch_user` at src/users.nv:34
  ┌─ src/users.nv:34:5
  │
  34 │     http.get(url)
  │     ^^^^^^^^^^^^^ this call requires effect `Net`
  │
  function signature is:
    fn fetch_user(id u64) -> User

  function should be:
    fn fetch_user(id u64) Net -> User
                          ^^^

  why: `http.get` performs network I/O. Functions that perform I/O
       must declare it in their signature so callers can decide
       whether to allow it.

  fix-suggestion: add `Net` to the effect list before `->`

  see also: docs/effects/Net.md
```

Format: location → reason → how to fix → **ready-made patch** →
a documentation link. The LLM applies the patch in one iteration.

### R5.4. Syntax stability

An explicit design commitment: **no breaking syntax changes
after v1.0**. New features — only additively. This is a guarantee for LLMs
trained on old data that their code stays valid.

The price — design mistakes cannot be fixed later. Therefore v1.0 ships
late, after a long preview period.

### R5.5. Fragment checkability

The ability to typecheck **one function** without the whole project:

```bash
nova check --fragment 'fn double(x int) -> int = x * 2'
# → ok

nova check --fragment 'fn double(x) = x * 2' --infer
# → fn double[T Mul[T, int]](x T) -> T  (выведенная сигнатура)
```

An LLM can generate functions and check them one by one, without the
whole project's context. This changes the feedback loop radically.

### R5.6. Self-describing API

The standard library is written so that each function describes
itself through the signature + a structured doc comment. Per
[D62](decisions/04-effects.md#d62) the signature contains direct effects
+ the full throw picture; transitive side effects are additionally
stated in the doc comment for clarity.

```nova
/// Sends an HTTP GET request.
///
/// effect.Net: makes an outgoing request
/// effect.Time: waits up to `timeout` ms
/// effect.Fail[NetError]: on connection failure, timeout, non-2xx
///
/// example:
///     let body = http.get("https://api.example.com/users/1")
///
/// see also: http.post, http.client
fn http.get(url str, timeout ms = 30000) Net Time Fail[NetError] -> Response
```

The doc comment has a **structure**, is parsed by the compiler,
and is checked for consistency with the signature. The LLM uses it
as context — structured, not free-form text.

### R5.7. spec ↔ impl reversibility

This is a **tooling capability**, not a language feature. No new syntax
constructs — only a description of a workflow that becomes possible
thanks to [R4](revolutionary.md) (contracts in the signature),
[R5.2](revolutionary.md) (signature = complete description) and
[R5.3](revolutionary.md) (structured errors).

The Nova LSP/IDE supports **two generation directions** between the contract
and the implementation.

#### Direction 1: impl → spec

The programmer writes the implementation. The LSP asks the LLM to generate
`requires`/`ensures` from the code. The programmer confirms or edits the
proposed contracts. Accepted contracts become **part of the code** and
are checked by the compiler (statically where it can, at runtime in debug).

```nova
// программист написал:
fn withdraw(mut acc Account, amount money) Fail[Overdraft] -> () {
    if amount > acc.balance { throw Overdraft }
    acc.balance -= amount
}

// LSP предлагает дополнить:
fn withdraw(mut acc Account, amount money) Fail[Overdraft] -> ()
    requires amount > 0
    ensures result.is_ok || acc.balance == old(acc.balance)
    ensures result.is_ok ==> acc.balance == old(acc.balance) - amount
{
    if amount > acc.balance { throw Overdraft }
    acc.balance -= amount
}
```

The programmer sees the contracts, evaluates their correctness, accepts or
edits them. This is **review**, not trusting the LLM on its word — but
cheaper than writing contracts from scratch.

#### Direction 2: spec → impl

The programmer writes **only** the signature and the contracts. The body is
generated by the LLM (via the "Generate body" IDE command), and the compiler
checks conformance to the contract. The loop runs until convergence or manual
intervention.

```nova
fn withdraw(mut acc Account, amount money) Fail[Overdraft] -> ()
    requires amount > 0
    ensures result.is_ok ==> acc.balance == old(acc.balance) - amount
    ensures result.is_err ==> acc.balance == old(acc.balance)
=>
    // [генерируется LSP]
```

The LSP calls the LLM, gets the body, the **compiler checks the contract**:

- If the contract holds (statically or in debug runtime) — OK.
- If violated — the error is returned to the LLM as a learning signal ([R5.3](revolutionary.md)),
  and the iteration is repeated.

#### No language changes

This lives **entirely** in the LSP/IDE. No `@ai-impl` directive, no
generation at compile time, no build dependency on an LLM.
Build reproducibility is preserved.

What the language needs for this workflow to work — **already exists**:

- Contracts in the signature ([R4](revolutionary.md))
- Structured compiler errors ([R5.3](revolutionary.md))
- Context locality ([R5.1](revolutionary.md)) — a function
  typechecks without the whole project (`nova check --fragment`)
- Effects in the signature ([R5.2](revolutionary.md)) — the LLM knows which
  side effects are allowed

#### What this changes in economics

Today, in industry, writing a function **with invariants** costs more than
**without**. Contracts are written only for critical code. R5.7 flips the
economics: **a contract is written faster than the body**, because the human
describes "what must be true", and the LLM does the boring part.

This shifts programming from "writing code" to "describing invariants".
Close to Dafny / F* / TLA+, but without a special specification language —
the same Nova.

#### Where this works and where it doesn't

**Works well:**
- Pure functions with a clear contract (parsing, validation,
  arithmetic)
- Functions with effects, where the contract is described in terms of inputs/outputs
- Small functions (< 50 lines)
- Functions with a known pattern (CRUD, routing, formatting)

**Works poorly:**
- Large stateful functions with subtle invariants over several
  types
- Functions with distributed effects, where the contract requires
  global reasoning (see [R12](revolutionary.md))
- Functions for which the SMT check of the contract does not converge in a
  reasonable time (see the SMT limitations in [decisions/09-tooling.md#d24](decisions/09-tooling.md#d24))

#### Limitations

1. **A quality LSP integration is needed.** Not every editor provides it;
   standardization is outside the language.
2. **A contract can be incomplete.** The LLM will generate a body that
   passes the contract but does something other than what the programmer
   wanted. The protection — human code review, as usual.
3. **Contract semantics through handler-state — an open question.**
   `ensures Db.balance(acc) == ...` — can the SMT solver check that?
   See [decisions/09-tooling.md#d24](decisions/09-tooling.md#d24).

#### Relationship to other decisions

- **Develops [R4](revolutionary.md)** — contracts become a utilitarian
  tool, not a theoretical superstructure.
- **Uses [R5.3](revolutionary.md)** — structured errors as a
  learning signal for the LLM.
- **Relies on [decisions/09-tooling.md#d24](decisions/09-tooling.md#d24)** — the strategy
  of SMT contract checking.

---

## R6. Capability mode for safe composition

A function can **forbid** certain effects in its scope:

```nova
fn run_user_script(code str) Fail -> Result =>
    forbid Net, Fs, Db {
        // внутри этого блока компилятор не позволит
        // вызвать ни одну функцию с эффектами Net, Fs, Db
        eval(code)
    }
```

The compile-time check works on the **direct** effects of the called
functions. If a function declares `Net` — its call inside `forbid Net`
is forbidden. Transitive effects are caught non-strictly ([D62](decisions/04-effects.md#d62)) —
a function without `Net` in its signature, but calling `helper()` with `Net`,
is not blocked at compile time. The full capability-sandbox guarantee
is achieved through **closure boundaries** with an explicit declaration of
allowed effects and through a project-level whitelist in `Nova.toml`.

Useful for:
- Plugins (with closure parameters of a fixed capability)
- User scripts (via the project whitelist)
- LLM-generated code (pin effects at a closure boundary)
- Deterministic computations (forbid `Time`, `Random`, `Io`)

`Async` is not forbiddable — it is an ambient capability, not part of the
type system ([D62](decisions/04-effects.md#d62)). If you need the
guarantee "the function does not suspend" — that is a runtime flag of the
fiber runtime, not a type-check.

---

## R7. Async — invisible infrastructure

In Nova functions can suspend (network roundtrip, sleep,
channel.recv, async-Db) — but this is **not expressed in types at all**.
There is no function color; no "sync" vs "async" split. There is no `await`
keyword either.

```nova
fn fetch(url str) Net -> Response => ...

fn handler(req Request) Net Db -> Response {
    ro user = fetch_user(req.id)        // suspendable, но не в типах
    ro posts = fetch_posts(user.id)
    Response.json(posts)
}
```

The return type is `Response`, not `Future<Response>`. The signature has only
the effects the programmer **sees** as accesses to the outside world
(`Net`, `Db`); suspension — an implementation detail.

Under the hood — a **fiber-based scheduler** (like Go/Erlang/OCaml 5).
When an effect operation suspends, the fiber is put into a waiting
queue, and the scheduler picks another fiber. The programmer writes neither
`async` nor `await` nor an `Async` effect in signatures.

### D62 decision — Async ambient capability

[D62](decisions/04-effects.md#d62) explicitly fixes: `Async` is **not an
effect** in Nova. Not part of the type system. This keeps
backend code compact — in a real backend almost every
function "can suspend", and an explicit `Async` effect would be noise
without informativeness.

### Comparison with other languages

|  | Rust async | Nova |
|---|---|---|
| Function color | yes (`async fn`) | no |
| `await` needed | yes | no |
| Return type changes | `Future<T>` | no |
| Async in signature | yes | **never** |
| Task cost | ~64 bytes | ~4–8 KB (fiber stack) |
| Cancellation | manual | structured |
| C-interop blocking | no problems | requires `detach to OS thread` |

Nova is closer to **Erlang/Go** in runtime: goroutines/fibers can
be preempted at any point; the programmer does not write `async`. It pays
with **memory** (fiber stacks) for **code simplicity**.

### Structured concurrency — separate language primitives

`spawn`, `supervised` (+ optional `cancel:`), `select`, `parallel for`,
`detach`, `blocking` — **runtime keywords**; `race`, `with_timeout`
— **library functions** on top of them. Not effects:

```nova
fn fetch_all(urls []str) Net -> []Response =>
    parallel for url in urls {
        fetch(url)
    }  // ждёт всех, отменяет хвост при ошибке

fn with_timeout[T](dur Duration, body fn() -> T) Fail -> T =>
    race {
        body(),
        sleep(dur).then { throw Timeout }
    }
```

Details — [decisions/06-concurrency.md#d14](decisions/06-concurrency.md#d14).

---

## R8. Time-travel debugging out of the box

Since all effects pass through handlers, **recording and replaying any run**
is a standard feature:

```bash
nova run --record trace.nrec ./server
# ... ловим баг

nova replay trace.nrec --step
# пошаговый repro с возможностью вернуться назад
```

This gives Erlang-level observability in any application, without
special code instrumentation.

---

## R9. Compile-time supervision (Erlang-style)

Effects imply built-in structured concurrency with supervision:

```nova
fn server() Net Fail -> () =>
    supervised {
        spawn handle_requests()      // если упадёт — рестарт
        spawn periodic_cleanup()     // если упадёт — рестарт
        spawn metrics_reporter()     // если упадёт — рестарт стратегии one_for_one
    } strategy = one_for_one, max_restarts = 3
```

Erlang/OTP supervision — built into the language, without a separate framework.

---

## R10. Effects at boundaries: typing, erasure, dynamics

Static typing of effects **propagates** into queues, channels, and
schedulers. That is good for typed pipelines and bad for
heterogeneous tasks. The solution — **three levels**, the programmer chooses:

**Level 1 — a typed scheduler** (default):
```nova
ro order_queue Queue[fn(OrderId) Db Log Fail -> ()]
```

**Level 2 — explicit erasure** (when heterogeneity is needed):
```nova
fn erase[E](task fn() E -> ()) E -> fn() -> () {
    ro captured = capture_handlers[E]()
    || with captured { task() }
}

universal_queue.enqueue(erase(send_email_task))
universal_queue.enqueue(erase(cleanup_db_task))
```

**Level 3 — dynamic effects** (plugins, serialization):
a runtime `EffectSet` structure, the `DynFn` type. Used rarely.

Details — [decisions/04-effects.md#d12](decisions/04-effects.md#d12).

---

## R11. Panic — what is NOT an effect

Not every interruption of a computation is an effect. **Hardware/mathematical
faults** (division by zero, overflow, out-of-bounds array access, OOM,
stack overflow) are **not stated in the signature**:

```nova
// никакого Fail[DivByZero]
fn mean(xs []int) -> int =>
    xs.sum() / xs.len()
```

They form the `Panic` category. The programmer does **not catch panic in
code** — panic means the death of the current fiber, the runtime handles it at
the boundary:

```nova
fn handle_request(r Request) Db Log -> Response =>
    process(r)             // panic → fiber умирает, runtime вернёт 500

fn server() Net Fail -> () =>
    supervised {
        spawn handle_requests()
    } strategy = one_for_one
    // supervisor рестартует упавшие fiber'ы
```

Otherwise `Fail[DivByZero]` would be in every other signature — the
informativeness would disappear. This is a **conscious compromise**; the
boundary is drawn explicitly: "there is no way to handle it, it must die" → Panic;
"it can and should be handled" → Fail.

The optional `@strict_total` — for critical code, turns the function into a
total one (the compiler requires handling all possible
panic sources). Details — [decisions/08-runtime.md#d13](decisions/08-runtime.md#d13).
