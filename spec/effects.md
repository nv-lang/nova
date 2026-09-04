---
source_rev: 6759a7f11
source_date: 2026-08-04
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

Network, disk, time, randomness, logging, errors, mutation, launching a process
(`std.os`, `Command.new(...).run()` — Plan 265 Ф.1, [D453](decisions/04-effects.md#d453)) — in Nova these
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

## An effect list is a lower bound, not an exact description

Effects on a function declaration read as **"can do at least this"**. Hence the
substitution rule: a function may be passed wherever **no more** effects are
required than it declares. Extra effects beyond the requirement are not an
obstacle.

```nova
fn run_handler[T, E](body fn() Fail[E] -> T) Fail[E] -> T => body()

fn user_handler() Time Fail[FwErr] -> int => 42
ro v = run_handler(user_handler)
```

Special case: `fn() -> T` requires nothing and therefore accepts a function with
any effects.

**Why.** A library that takes user code — an HTTP router, a scheduler, a test
runner, a retry policy, an iterator with a callback — cannot know its effects:
a handler may touch the database, files, the network, sleep and log, in any
combination. Under an "exactly this and nothing more" reading, the library would
reject user code for doing more than its author foresaw. Foreseeing is
impossible, so frameworks would be inexpressible under that rule.

**Guarantees are not weakened.** "Nothing beyond" is expressed separately, by
`forbid E1, E2 { … }`. The two sides are kept apart:

| construct | meaning | bound |
|---|---|---|
| effects in a signature | "can do at least this" | lower |
| `forbid E1, E2 { … }` | "this is not allowed here" | upper |

`--strict-effects` checks that a function declares **no less** than it uses —
the same lower bound, seen from the definition side.

Normative: [D448](decisions/04-effects.md).

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
- **Auto-cleanup (`@cleanup`, [D432](decisions/02-types.md#d432)) counts as a
  DIRECT effect** of the function in whose scope the compiler inserted the
  call: the call is physically generated in its body. Hold a `File` — `Fs`
  appears in the signature; the cleanup may fail — `Fail[E]` appears too.
  Without this clarification the rule would read as "transitive", i.e. it
  would degrade into a warning (D432 amendment, 2026-08-04).
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

**The outcome of cancellation is visible in the VALUE of the scope** ([D455](decisions/06-concurrency.md#d455)):
a scope with `cancel:` returns not `T` but `Outcome[T]` = `Done(T) | Cancelled`, and the caller
takes the outcome apart with `match`. Without `cancel:` the type is as before — only the one
who ordered cancellation pays. The reason: cancellation throws nothing, and without an outcome
in the value "the scope finished" and "the scope was cancelled" are indistinguishable — so a
correct completion has nothing to check.

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

## What a failure prints ([D462](decisions/08-runtime.md#d462))

An unhandled failure — a `Fail` nobody caught, a `panic`, a typed `Fail` — is
ONE record: the kind, the message, the throw site, the `?`-propagation chain
that carried it, and any cleanup errors it suppressed. Whoever RUNS the program
picks the shape; whoever built it does not have to decide in advance:

```
$ ./app                              # nobody is parsing — a person reads it
nova: unhandled Fail: leaf-error
  at app.nv:29 (throw site)
  propagation trace (`?`-chain, oldest first):
    via app.nv:19 (?)

$ NOVA_PANIC_FORMAT=json ./app       # a tool is parsing — one line, stable keys
{"nova_failure":1,"kind":"fail","message":"leaf-error",
 "site":{"file":"app.nv","line":29},"trace":[{"file":"app.nv","line":19}],
 "trace_dropped":0,"suppressed":[]}
```

Human output is the default: a tool asks for the machine format explicitly, a
person should never have to read JSON without asking. An environment variable
rather than a build flag — rebuilding a program to get a machine-readable log
would be absurd.

### Reading a failure from code ([D463](decisions/08-runtime.md#d463))

The same record the runtime prints is available to the program that handles the
failure -- three free accessors, no new types and no new keyword:

```nova
with Supervisor = effect Supervisor {
    on_child_fail(idx, err) -> Decision {
        Log.error(report())                                  // the whole record
        for s in suppressed() { Log.warn(s) }                // cleanup failures
        if Some(c) = cause() { Log.error("caused by: ", c) } // one step back
        return if err is Panic { Decision.Stop } else { Decision.Restart }
    }
}
```

`report()` renders through the SAME renderer that prints a terminal failure, so
`NOVA_PANIC_FORMAT` governs both -- there is no second format to drift. `cause()`
is one optional step back, the shape Rust spells `source()`, Java `getCause()` and
Go `errors.Unwrap`; walk it to get a chain. `suppressed()` is the D158 pocket and
does not change.

A handler that throws INSTEAD of the error it caught binds that error as the cause
automatically -- Nova needs no `from`, because the place where the replacement
happens is exactly the handler arm, where the caught error is the parameter.
A cleanup that throws BESIDE a still-propagating error goes to `suppressed()`
instead. The split is structural, not a heuristic.

## Cross-fiber safety — a property of the type, a requirement at the boundary (D446)

The rule in one sentence: **a value may be reachable from more than one fiber
only if it is immutable, or solely owned by one owner, or its type is declared
safe for concurrent use — and that declaration is checked by the compiler.**

It is checked in three local steps, without reachability analysis:

- a type carries a checkable "safe for concurrent use" property;
- a closure is safe when everything it captured is safe;
- a function that accepts a value which will LATER run concurrently
  (installing middleware, registering a handler) declares that requirement in
  its own signature.

The compiler does not work out who calls from where: every entry into
concurrency demands the property from what it accepts. Details and rationale —
[D446](decisions/06-concurrency.md#d446).

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

The **`?? return ...`** form (early exit from the enclosing function) depends
on whether the function has a wrapper to propagate
([D86](decisions/04-effects.md#d86) amend 2026-07-23 + amend 2026-09-01):

* the function returns `Option`/`Result` — the form is **refused**
  (`E_COALESCE_RETURN_FALLBACK`): it would be a second door to `?`. The canon
  is `X?` (the same wrapper outward), `.ok()?` (Result → the function returns
  Option), `.map_err(...)?` (the error type changes), `.ok_or(err)?`
  (Option → the function returns Result);
* the function returns anything else (`bool`, a number, `()`, a tuple, a type
  of your own) — the form is **legal**: there is nothing to propagate, `?`
  does not apply, and its alternative is a two-arm `match` (also legal).

```nova
fn is_big(x int) -> bool {
    ro v = lookup(x) ?? return false   // legal: `-> bool`, no wrapper
    v > 10
}
```

The early exit is a real `return`: `defer`, `errdefer` and `ensures`
contracts all run. An `Err(e)` is discarded, as with ordinary `??`; if you
need `e` inside, write a `match`. Under a generic parameter the form is
refused — `T` may arrive as `Option`.

## Alternative: explicit Result

```nova
fn parse(s str) -> Result[int, ParseError] => ...
```

Two styles of the same thing. `Fail` — sugar over `Result`.
The default for application code is `Fail` (more readable); for libraries
with an important error type — explicit `Result`.

## What an effect operation looks like ([D456](decisions/04-effects.md#d456))

An effect operation is an ordinary Nova function with exactly one element taken away:
the receiver `@`, because an effect has no instance. Everything else the language can
do is available to it and expected of it — generics, `Result`/`Option`, records and
sums, collections, function parameters, named types instead of bare numbers.

The converse is a rule too: at an effect boundary there is no negative `errno` in
place of an error, no empty string as the sign of "none", no traversal by index
(`_len` + `_at`), no out-parameters, no raw `int` handles, no counters beside the
data, and no `str` holding non-UTF-8 bytes. C forms live in the `extern "C"` layer
`ffi.nv` and inside the `real_*` handler: **the handler is a translator, not a
pass-through channel.**

The reason is not beauty. The effect boundary is exactly what the author of a mock
sees, and substitutability is what we call the language's distinction: if a C form is
visible in the declaration, the translation simply has not been written, and everyone
who writes a test will have to write it.

**One exception, and the owner's decision of 2026-08-12 made it a named refusal rather
than a silent failure:** generics in effects are NOT supported on either axis of
generalisation — neither on the operation (`type Wrap effect { around[T](body
fn() -> T) -> T }`, registry 221.1 #570) nor on the effect itself (`type Store[T]
effect { ... }`, registry 221.1 #614). Before that decision both forms passed
`nova check` green and failed only in the C compiler (`Nova_T*`/`unknown type
name 'NovaVtable_Store'`) — now the checker rejects both with a named error
(`E_EFFECT_OP_GENERIC_UNSUPPORTED` / `E_EFFECT_GENERIC_UNSUPPORTED`) right at the
declaration. Rank-2 polymorphism through a vtable slot with a single signature is open
question Q6; only the retracted bootstrap interpreter ever supported it. The compiler
intrinsic `Fail[E]` is the exception to the refusal: it is the sugar target of
`throw`/`!!` with its own runtime machinery, not a user effect through the common
vtable path. For comparison: a generic PROTOCOL works both as a bound and directly as
a type (a box with a vtable) — verified by build and run. So the matter is not
generics as such, but the effect's vtable.

## The main point

Effects are a **promise in the signature** + a **catch point**. One
mechanism for what other languages spread across `try/catch`,
`async/await`, dependency injection, mocks, and `unsafe`.
