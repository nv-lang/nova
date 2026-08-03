---
source_rev: 337ec42af
source_date: 2026-07-26
---

> **Informative translation; the Russian text is normative.**
>
> Russian original (normative): [effects.md](effects.md)

# Nova — the effect system

This is an introduction to the concept. The full treatment with handlers,
AI-first rationale, and the standard effect set — in
[revolutionary.md](revolutionary.md). Questions of async, panic, and effect
erasure — in [D12](decisions/04-effects.md#d12), [D13](decisions/08-runtime.md#d13), [D14](decisions/06-concurrency.md#d14).

## Central principle

Network, disk, time, randomness, logging, errors, mutation — in Nova these
are all **effects**. A function declares in its signature the effects it
uses itself; calls to other functions do not pull those functions' effects
up into the caller's signature (the exception is `Fail` — errors are visible
transitively). Each effect has a **handler** that intercepts its operations.

If a signature has no direct effects and the function calls no effectful
functions — it is deterministic (with the caveat about Panic, see below).

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

### An effect has no fields

An effect has no fields — only operation signatures. State, if needed,
lives in the handler's environment and reaches it via capture (like an
ordinary closure).

## An effect is an interface + an implicit parameter

The most precise way to understand it:

> **Effect = interface + implicit parameter, checked by the compiler.**

Three parts:
1. **Interface** — a set of operations with signatures and no implementation
2. **Implicit parameter** — the implementation is passed via the `with` scope,
   not through an argument list
3. **Checked** — if a function uses an operation, the effect
   must be in its signature

```nova
type Db effect {
    query(q Sql) -> []DbRow                // только сигнатуры, без реализации
    exec(q Sql)  -> ()
}

// функция декларирует, какой эффект ей нужен
fn process(o Order) Db -> Receipt =>
    Db.query(sql`SELECT * FROM orders WHERE id = ${o.id}`)
                                           // вызов операции активного handler'а

// промежуточные функции просто пробрасывают эффект — без with
fn handle_request(o Order) Db Log -> Receipt {
    Log.info("processing")
    process(o)                              // handler берётся из вызывающего скоупа
}

// `with` ставится один раз — там, где определяется реализация
fn main() Io -> () =>
    with Db = postgres_handler {
        handle_request(o)                   // handler виден через всю цепочку
    }
```

`with` is needed **once**, at the point where the implementation is chosen.
Between that point and the operation call site there can be any number of
functions — each of them declares the effect in its signature, but `with` is
not repeated. This solves the problem of "the implementation has to be passed
as a parameter through every intermediate function": you set `with` once —
it is visible everywhere below on the stack.

## Syntax

Effects go **between the parameter list and `->`**:

```nova
fn double(x int) -> int                       // чистая
fn parse(s str) Fail -> int                 // может бросить
fn save(u User) Fail Db Log -> ()           // три эффекта
fn fetch(url str) Net Fail -> Response
```

The boundary is given by structure: everything between `)` and `->` — effects.

## Names — ordinary named types

Effects are **named `effect`-types** (per [D61](decisions/04-effects.md#d61)),
in PascalCase. Declared via the `effect` kind-token, distinguished from
structural contracts (`protocol`) by the semantics of `with`-substitution
and continuation-capture.

```nova
type Logger effect {
    log(msg str) -> ()
}

ro console = effect Logger {
    log(msg) -> () => println(msg)
}
```

`effect Logger { ... }` — a handler literal: the same `effect` keyword
as in the type declaration ([D61](decisions/04-effects.md#d61)/[D142](decisions/02-types.md#d142)
— symmetry between declaration and literal for `effect`/`protocol`).
**The old `handler` keyword for the literal is retracted without a
deprecated alias** (2026-05-23, clean break) — the compiler, on meeting
`handler X { ... }`, emits a diagnostic "`handler` keyword removed; use
`effect` (D142)". Unambiguity with a record literal (`User { id: 1 }`)
is provided by the `effect`/`protocol` keyword itself, not a separate prefix.

**Every operation of a handler literal must state `-> Type` explicitly**
([D434](decisions/04-effects.md#d434), 2026-07-22) — the only place in the
language where the return type used to be syntactically optional. Omitting
it gives `E_INCOMPLETE_HANDLER_OP_DECL`; a mismatch with the operation's
type in the effect declaration gives `E_HANDLER_OP_RETURN_TYPE_MISMATCH`.
Parameter types still don't have to be repeated (`log(msg) ->
() => ...`) — only the return is mandatory.

## The effect name in code — three positions

```nova
fn process() Db -> ()                  // 1. позиция типа
Db.query(sql`...`)                     // 2. позиция операции
ro captured = Db                      // 3. позиция выражения = активный handler
```

The parser distinguishes by position.

## Standard effects

| Effect | What it describes |
|---|---|
| `Fail[E]` | Contract for catching and handling an error of type E |
| `Io` | stdin/stdout/stderr |
| `Fs` | File system |
| `Net` | Network requests |
| `Db` | Database |
| `Time` | Clock, timers, delays |
| `Random` | RNG |
| `Log` | Structured logging |
| `Trace` | Distributed tracing |
| `Ask[T]` | Reading from context (like Reader) |
| `Alloc[R]` | Allocation in region R |
| `Detach` | A fire-and-forget task outliving the caller ([D50](decisions/06-concurrency.md#d50)) |
| `Blocking` | A synchronous C call on a blocking-pool thread ([D50](decisions/06-concurrency.md#d50)) |

`Async`, `Mut`, `Par` are **not** in the standard set per
[D62](decisions/04-effects.md#d62): `Async` — ambient capability
(not part of the type system), `Mut` — replaced by specialized
effects, `Par` — the runtime keyword `parallel for` / `spawn`.

A programmer can declare custom effects — that is an ordinary
type declaration via `effect`.

## Why this is needed

### 1. The type shows what a call does

```nova
ro x = double(5)            // не делает ничего
ro y = parse(s)?            // может упасть — обязан обработать
ro r = http.get(url)?       // ходит в сеть — видно в сигнатуре
```

An LLM (and a human) reading a signature **knows all the side effects**.
In Python/Java/Go this information is absent from the type.

### 2. Pure functions are separated from impure ones

If a signature has no effects — the compiler knows: it can be
memoized, called on any thread, replaced by a constant
on equal inputs.

### 3. It is impossible to "accidentally" add a side effect

Someone added `Log.info(...)` to a formatting utility — the build of
the callers **breaks**, because the `Log` effect appeared. It cannot be
smuggled in silently. **This is a feature.**

## Direct effects, not transitive ([D28](decisions/04-effects.md#d28))

A signature declares only the effects whose operations the function calls
**itself** — not the effects of nested calls:

```nova
type Db effect {
    exec(stmt str) -> ()
}

fn save(name str) Db -> () {
    Db.exec(name)              // прямое использование — Db в сигнатуре обязателен
}

fn helper(name str) -> () {
    save(name)                  // транзитивное Db — по умолчанию только warning
}
```

- **A direct** effect undeclared → **compile error**, always — including a
  RAW operation call (`Db.exec(...)` without an intermediate named fn):
  until [№131](decisions/04-effects.md#d62) this call-shape was an
  enforce-hole (`E_RAW_EFFECT_OP_UNDECLARED` now closes it at the export
  boundary — the same boundary as `!!`/[№113](decisions/04-effects.md#d85)
  below; private code gets D28 inference).
- **A transitive** effect undeclared → **warning**, suppressible via
  `#allow_transit(Db, Log)` on a function or `transit_effects = "off"` in
  `Nova.toml`.
- **The `--strict-effects` flag** (`nova check`/`build`/`test`, Plan 197) turns
  this warning into a hard error `E_UNDECLARED_TRANSITIVE_EFFECT` —
  the project convention requires building `std/**` and `examples/**` with
  exactly this flag (see `CLAUDE.md`). The same flag catches
  `E_EFFECT_ERASED_IN_FN_TYPE` — assigning/passing a function into a
  `fn(...) Row -> T` narrower in effects.
- **`Fail[E]`** — the exception to "direct": throw stays **strictly
  transitive** and is mandatory in the signature everywhere it can occur
  (see "The `?` and `!!` operators" below) — the compiler does not weaken
  the check with any flag here.
- A private (non-`export`) function can **not write** direct effects
  by hand at all — the compiler infers them from the body automatically
  (including adding `Fail[E]` if a private function uses `!!`/`throw`
  somewhere). In `export fn` direct effects must be explicit — that is
  the public contract.

## Async — invisible infrastructure (D62)

Suspension in Nova is **not an effect** but ambient runtime infrastructure.
**No `Future<T>` in the type.** No `await`. There is no function color.
The programmer does not see "can this function suspend" in its signature.

```nova
fn fetch(url str) Net -> Response => ...
fn handler(req Request) Net Db -> Response {
    ro user = fetch_user(req.id)        // никаких .await
    ro posts = fetch_posts(user.id)
    Response.json(posts)
}
```

Under the hood — a fiber-based scheduler (like Go/OCaml 5). The cost —
kilobytes of memory per fiber; a million fibers per machine — normal.

If you need the guarantee "no suspension allowed here" — use the
[`realtime { ... }`](decisions/04-effects.md#d64) block as an
inverse marker.

Details — [decisions/06-concurrency.md#d14](decisions/06-concurrency.md#d14),
[decisions/04-effects.md#d62](decisions/04-effects.md#d62).

## Default handler without `with` ([D431](decisions/04-effects.md#d431))

Some effects (`Time` — the canonical example) work **without an explicit
`with`**, if the programmer did not install their own handler:

```nova
fn log_uptime() Time Io -> () =>
    println("${Time.now()}")   // handler не установлен — используется дефолтный (real-clock)
```

This is not "an effect without a handler" — the compiler synthesizes a
**lazy, once-per-thread** default constructor via the
`#default_handler(EffectName)` attribute on an ordinary handler-literal
factory in `.nv` source (not a hardcode in Rust). `with Effect = ...`
still fully overrides the default — the mechanism does not lose
mockability, it just removes the need to write `with` for the typical
bootstrap case.

## What is NOT an effect — Panic

Not every interruption is an effect. **Hardware/mathematical faults**
are not stated in the signature:

- Division by zero
- Integer overflow
- Out-of-bounds array access
- Stack overflow
- Out-of-memory

They form the `Panic` category. The programmer does **not catch panic in
code** — it is the death of the current fiber, the runtime handles it at
the boundary:

```nova
import std.concurrency.supervisor as sup

fn handle_request(id int) -> int =>
    id / id                // если panic (напр. id == 0 → div/0) — fiber умирает

fn server(ids []int) -> () =>
    with Supervisor = sup.stop() {           // упавший ребёнок НЕ отменяет siblings
        supervised {
            for id in ids { spawn { handle_request(id) } }
        }
    }
```

The supervision strategy is an ordinary **effect-handler** (`Supervisor`,
[D416](decisions/06-concurrency.md#d416)), not a named parameter:
ready-made policies — `sup.stop()` (the failed one is "dropped", the others
continue, its error is not lost — retained) and `sup.escalate()` (equivalent
to the default: the error becomes primary, siblings are cancelled
cooperatively). A custom policy — an ordinary handler literal:
`on_child_fail(idx int, err any) -> Decision`,
where `Decision` is `Escalate` or `Stop`. **The `Restart` family is absent
from the vocabulary** (retracted 2026-07-10) — the restart idiom is foreign
to structured concurrency for fibers; retry lives inside the child's body
(`std.concurrency.retry`), not in the supervisor.

`panic` is the death of a **fiber**, not the process. In a server only the
current request falls, everything else keeps working. If you need to
guaranteed-kill the process — a separate `exit(code, msg)` function ([D13](decisions/08-runtime.md#d13)).

Otherwise `Fail[DivByZero]` would be in every other signature — the
informativeness of effects would disappear. A conscious compromise,
in detail — [decisions/08-runtime.md#d13](decisions/08-runtime.md#d13).

## Roles — `throw` / `Fail[E]` / handler

To avoid confusing the layers, three participants in error handling:

- **`throw err`** — language syntax, raises an error. After `throw`
  control never returns to that point (the operation type is `never`).
- **`Fail[E]`** — the effect contract for catching and handling an error.
- **a `Fail[E]` handler** — what catches the error. A handler has
  exactly two outcomes:
  - complete the `with`-block with a value via `interrupt v`,
  - rethrow the error further via `throw`.

  Resuming the call at the `throw` point is impossible — the operation
  type is `never`, there is nothing to return to that point.

## The `?` and `!!` operators

The programmer chooses the handling style at the usage site
([D85](decisions/04-effects.md#d85)):

- **`expr?`** — return-style: "didn't work — wrap it upward as a
  value". The enclosing function must return `Option`/`Result`.
- **`expr!!`** — throw-style: "didn't work — throw via `Fail`".
  The enclosing function must have `Fail[E]` in its signature.

```nova
fn pipeline_return(s str) -> Result[int, ParseError] {
    ro n = parse(s)?            // на Err: return Err(e)
    validate(n)?
    Ok(n)
}

fn pipeline_throw(s str) Fail[ParseError] -> int {
    ro n = parse(s)!!           // на Err: throw e
    validate(n)!!
    n
}
```

Both operators work for `Option[T]` and `Result[T, E]` alike. For
`Option!!`, `RuntimeNoneError` is thrown (a prelude unit type). The twin
methods `.unwrap()` / `.unwrap_or(v)` / `.unwrap_or_else(f)` were
**retracted** (2026-07-07) — the only canonical path is the operator one
(`!!`, `??`); there are no methods on `Option`/`Result` with the same
meaning in the prelude.

**Enforcement at the export boundary ([№113](decisions/04-effects.md#d85),
2026-07-25).** `expr!!` in an `export fn` whose signature carries no
compatible `Fail[E]` (and the throw is not caught by a local
`with Fail = ... {}`) — compile error `E_BANG_REQUIRES_FAIL`. For
**private** functions this does not apply — D28 auto-inference works there
(see above): `Fail` is silently inserted into the effect row if the body
uses `throw`/`!!`. If `!!` guards a program invariant rather than real
fallibility (typical example — a setter over a compile-time-known literal),
the way out is `?? panic("...")` instead of dragging `Fail` into the public
signature.

In parallel, **`??`** remains — coalesce / custom fallback:

```nova
ro port = config.get("port") ?? 8080                   // default
ro port = config.get("port") ?? throw MyError          // custom throw
ro port = config.get("port") ?? panic("no port")       // panic (D13)
```

The **`?? return ...`** form (early exit from the enclosing function) is
**retracted** ([D86](decisions/04-effects.md#d86) amend, 2026-07-23,
`E_COALESCE_RETURN_FALLBACK`): it was a second door to `?`, not an
independent niche. The canon instead — `X?` (the same wrapper outward),
`.ok()?` (Result → the function returns Option),
`.map_err(...)?` (the error type changes),
`.ok_or(err)?` (Option → the function returns Result), or an explicit
`match`, if there is no wrapper to propagate at all.

## Alternative: explicit Result

```nova
fn parse(s str) -> Result[int, ParseError] => ...
```

Two styles of the same thing. `Fail` — sugar over `Result`.
The default for application code is `Fail` (more readable); for libraries
with an important error type — explicit `Result`.

## The main point

Effects are a **promise in the signature** + a **catch point**. One
mechanism for what other languages spread across `try/catch`,
`async/await`, dependency injection, mocks, and `unsafe`.
