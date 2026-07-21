<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova language tour

A working tour of Nova for a reader who has never seen the language —
not the full specification. Every example on this page is a real,
compiling, running `.nv` file (`nova build` + running the binary, or
`nova test` where noted); none of it is aspirational. The source files
live under [`examples/tour/`](../examples/tour) in the Nova repository if
you want to build them yourself.

Nova compiles to C, then to a native binary — there is no interpreter.
See [quickstart.md](quickstart.md) if you haven't built anything yet.

## 1. Hello, functions, variables

`ro` declares a read-only binding (never reassigned); `mut` declares a
reassignable one. Types are inferred in almost all positions — write them
explicitly only where it helps a reader (a function signature, or a `mut`
binding whose initial value doesn't make the type obvious).

```nova
// hello.nv — functions, let/mut, type inference.
module tour.hello

fn add(a int, b int) -> int => a + b

fn main() {
    ro name = "Nova"                 // inferred: str
    mut count int = 0                // explicit type, still inferred-friendly
    count = count + 1
    println("Hello, ${name}! count=${count}, add(2,3)=${add(2, 3)}")
}
```

```
Hello, Nova! count=1, add(2,3)=5
```

## 2. Types: records, sum types, Option/Result, generics

`type X { ... }` declares a **record** — a heap-allocated, GC-managed
reference type. A **sum type** requires the `enum` marker
(`type X enum A | B | C`, [D406](../spec/decisions/02-types.md#d406-sum-type-синтаксис-через-enum-маркер-2026-07-01)) —
the older bare leading-`|` form is retired. `Option[T]`/`Result[T, E]` are
ordinary sum types from the prelude. `[T]` on a function introduces a
generic type parameter.

```nova
// types.nv — records, sum types (D406 `enum` marker), Option/Result, generics.
module tour.types

// record — heap-allocated reference type, GC-managed (`{}` braces).
type Point { x f64, y f64 }

// sum type — the `enum` marker is mandatory (D406); leading `|` alone is not
// valid syntax anymore. Inline form: first variant has no leading `|`.
type Shape enum
    | Circle(f64)
    | Rectangle(f64, f64)

fn area(s Shape) -> f64 => match s {
    Circle(r)       => 3.14 * r * r
    Rectangle(w, h) => w * h
}

// Generic function — [T] introduces a type parameter.
fn first[T](xs []T) -> Option[T] {
    if xs.len() == 0 { None } else { Some(xs[0]) }
}

fn main() {
    ro p = Point { x: 1.0, y: 2.0 }
    println("p = (${p.x}, ${p.y})")

    ro c = Circle(2.0)
    ro r = Rectangle(3.0, 4.0)
    println("area(circle)=${area(c)} area(rect)=${area(r)}")

    ro xs = []int.of(10, 20, 30)
    ro empty = Vec[int].new()   // `.of()` requires >=1 arg (D259); empty is `.new()`
    println("first(xs)=${first(xs)} first(empty)=${first(empty)}")
}
```

```
p = (1, 2)
area(circle)=12.56 area(rect)=12
first(xs)=Some(10) first(empty)=None
```

## 3. Methods, protocols, property methods

A method is declared *outside* the type body: `fn Type [mut] @name(...) -> R`.
Inside the body, `@field` reads the receiver's field. **Properties by
arity** (D84/D409) let one name serve as both getter and setter: `@x() -> T`
reads, `mut @x(v T) -> @` writes and fluently returns the receiver (no
explicit `return self` needed — D409 makes it automatic). `protocol`
declares a structural interface; `#impl(...)` opts a type into one
explicitly (D186).

```nova
// methods.nv — @-methods, protocols (#impl), property methods by arity.
module tour.methods

type Counter value { mut n int }

fn Counter mut @inc() -> () {
    @n += 1
}

// Property-by-arity (D84/D409): reading is `@x() -> T`, writing is
// `mut @x(v T) -> @` — a fluent setter that returns the receiver.
fn Counter @value() -> int => @n
fn Counter mut @value(v int) -> @ {
    @n = v   // D409: fluent setter, receiver returned automatically
}

// `protocol` = structural interface (behavior contract on a type).
// `#impl(...)` opts a type into a protocol explicitly (D186).
type Sized protocol {
    @size() -> int
}

type Box { items []int }
fn Box @size() -> int => @items.len()

fn sum_sizes[T Sized](a T, b T) -> int => a.size() + b.size()

fn main() {
    mut c = Counter { n: 0 }
    c.inc()
    c.inc()
    println("counter after two inc = ${c.value()}")

    ro c2 = c.value(100)   // fluent setter, returns @ (Counter)
    println("after fluent set = ${c2.value()}")

    ro b1 = Box { items: []int.of(1, 2, 3) }
    ro b2 = Box { items: []int.of(4, 5) }
    println("sum_sizes = ${sum_sizes(b1, b2)}")
}
```

```
counter after two inc = 2
after fluent set = 100
sum_sizes = 5
```

## 4. Pattern matching

`match` supports literal patterns, guards (`n if n > 0`), and
sum-variant destructuring. `if <Pattern> = expr { } else { }` is Nova's
if-let form — it matches a single-arm pattern and binds inside the `if`,
falling to `else` on no match (there's no separate `if let` keyword).

```nova
// patterns.nv — match, guards, `if <Pattern> = expr` (if-let form), record match.
module tour.patterns

type Shape enum
    | Circle(f64)
    | Square(f64)

fn describe(n int) -> str => match n {
    0          => "zero"
    n if n > 0 => "positive"
    _          => "negative"
}

fn area(s Shape) -> f64 => match s {
    Circle(r) => 3.14 * r * r
    Square(a) => a * a
}

fn main() {
    println("describe(0)=${describe(0)} describe(5)=${describe(5)} describe(-3)=${describe(-3)}")
    println("area(Circle(2.0))=${area(Circle(2.0))}")

    ro opts []Option[int] = [Some(1), None, Some(3)]
    mut total = 0
    mut missing = 0
    for o in opts {
        if Some(v) = o {
            total += v
        } else {
            missing += 1
        }
    }
    println("total=${total} missing=${missing}")
}
```

```
describe(0)=zero describe(5)=positive describe(-3)=negative
area(Circle(2.0))=12.56
total=4 missing=1
```

## 5. Errors: Result + `?`, panic

Nova's error-handling rule ([docs/idioms/error-handling.md](idioms/error-handling.md)):
**panic** is for a broken caller contract (a programmer bug — out-of-bounds
access, a violated `requires`) and is never recoverable; **`Result[T, E]`**
is for recoverable failure with an inspectable cause, and `?` propagates an
`Err` out of the enclosing `Result`-returning function (D85). `Option[T]` is
reserved for genuine absence (`find`, `get`), not fallibility — there is no
`_opt` twin of a fallible operation.

```nova
// errors.nv — Result + `?`, panic for programmer-bug invariants.
module tour.errors

type ParseErr enum Empty | BadDigit

fn parse_digit(s str) -> Result[int, ParseErr] {
    if s.byte_len() == 0 { return Err(Empty) }
    ro c = s.bytes()[0]
    if c < 48 || c > 57 { return Err(BadDigit) }
    Ok((c as int) - 48)
}

// `?` propagates Err out of a Result-returning function (D85).
fn parse_two(a str, b str) -> Result[int, ParseErr] {
    ro x = parse_digit(a)?
    ro y = parse_digit(b)?
    Ok(x + y)
}

fn main() {
    match parse_two("3", "4") {
        Ok(sum)  => println("sum = ${sum}")
        Err(_e)  => println("parse failed")
    }
    match parse_two("3", "x") {
        Ok(sum) => println("sum = ${sum}")
        Err(_e) => println("parse failed as expected")
    }

    // panic: an invariant the CALLER was required to satisfy — a genuine
    // programmer bug, not a recoverable external-input error. Never used for
    // bad user input/network/files (those are Result).
    ro xs = []int.of(1, 2, 3)
    if xs.len() > 5 {
        ro _oob = xs[10]  // would panic: out-of-bounds is not reached here
    }
    println("done")
}
```

```
sum = 7
parse failed as expected
done
```

## 6. Effects in types

Network, disk, the clock, randomness, logging, mutation, errors — in Nova
these are all **effects**. A function declares in its signature exactly
which effects *it itself* performs; calling another function does not pull
that function's effects up into the caller's signature (the one exception
is `Fail`, which propagates transitively). Each effect has a **handler**
that intercepts its operations, substituted via `with Handler = effect X
{ ... } { body }` — swap in a deterministic fake for tests without
touching the function under test. This is distinct from `protocol`: an
effect is "how to do something" (swappable implementation), a protocol is
"what a value can do" (fixed per type). Internal modules (`std/**`) and
programs (`examples/**`) build under `--strict-effects`
(`nova build --strict-effects`), an experimental flag that promotes
undeclared-transitive-effect and effect-erasure warnings to hard errors.

```nova
// effects_tour.nv — effects in function signatures, handler substitution (D61).
module tour.effects_tour

type Counter effect {
    next() -> int
}

// `Counter` in the signature says: this function performs Counter's
// operations, but does NOT say who handles them.
fn count_three() Counter -> int {
    ro a = Counter.next()
    ro b = Counter.next()
    ro c = Counter.next()
    a + b + c
}

fn main() {
    mut state = 0
    with Counter = effect Counter {
        next() {
            state += 1
            return state
        }
    } {
        ro total = count_three()
        println("total = ${total}")   // 1 + 2 + 3 = 6
    }
}
```

```
total = 6
```

## 7. Ownership: consume, defer, auto-`@cleanup` (D432 — new this release)

A `consume`-typed binding is ownership-tracked. Historically (D133) it had
to be consumed **exactly once**, or the compiler rejected the program —
strictly linear. **D432** (new in this release) lets a `consume` type opt
into an **affine** discipline instead: if it declares an effect-pure
`@cleanup(outcome ScopeOutcome) -> ()`, the compiler auto-inserts a call to
it on any exit path where the value is still live — forgetting to consume
is no longer an error. Types without `@cleanup` keep the old strict-linear
behavior unchanged. `defer { ... }` runs at scope exit, LIFO across
multiple `defer`s in the same scope (D189).

```nova
// consume_tour.nv — consume-params/bindings, defer, auto-`@cleanup` (D432).
module tour.consume_tour

// `consume` = an ownership-tracked type; a `consume` binding must be
// consumed exactly once UNLESS it declares `@cleanup` (see below).
type Resource consume { id int }

// A type with an effect-pure `@cleanup` shifts from linear (must-consume)
// to affine (may-forget) — the compiler auto-inserts `@cleanup(outcome)` on
// any dangling exit path (D432, new in this release). Without `@cleanup`,
// forgetting to consume is a compile error (D133) — that's the strict form.
fn Resource consume @cleanup(_outcome ScopeOutcome) -> () {
    ()
}

fn Resource consume @close() -> () { () }

fn make(id int) -> Resource => { id }

fn main() {
    // `defer { ... }` runs at scope exit, LIFO for multiple defers (D189).
    {
        defer { println("first defer registered, runs LAST") }
        defer { println("second defer registered, runs FIRST") }
        println("inside scope")
    }
    println("explicit-consume demo below")
}

// A bare consume-let, never explicitly consumed: this COMPILES (D432
// auto-cleanup covers it) — before D432 this was a D133 compile error.
test "D432: bare consume-let never touched still compiles + runs" {
    consume r = make(1)
    ro _id = r.id
    assert(_id == 1)
}

// Explicit consume still works exactly as before D432 (strict linear form).
test "explicit consume + close still works (pre-D432 form)" {
    consume r2 = make(2)
    r2.close()
    assert(true)
}
```

`nova build` output (`main` only — `test` blocks run under `nova test`):

```
inside scope
second defer registered, runs FIRST
first defer registered, runs LAST
explicit-consume demo below
```

`nova test` on the same file: `PASS: 2  FAIL: 0`.

## 8. Concurrency: spawn, parallel for, supervised, channels

There is no `async fn`, no `.await`, no `Future<T>`. `Time` appearing in a
function's signature is the only marker that it touches the clock —
concurrency is structured, not a separate async dialect. `spawn` inside a
`supervised` block starts a fire-and-forget fiber; `supervised(deadline:)`
gives that block a shared deadline, and a spawn that misses it is
genuinely cancelled, not left running with its result discarded.
`parallel for` fans out homogeneous work and collects results into a
`[]T` in order. `Channel.new(cap)` returns a capability-split
`{ tx, rx }` pair. This is the same shape as
[`examples/mini_aggregator.nv`](../examples/mini_aggregator.nv) (see
[quickstart.md](quickstart.md)) and the flagship
[`examples/flagship/aggregator`](../examples/flagship/aggregator) demo.

```nova
// concurrency.nv — spawn, parallel for, supervised(deadline:), channels.
module tour.concurrency

import std.time.duration

fn probe(latency_ms int, deadline Monotonic) Time -> str {
    ro { tx, rx } = Channel.new(1)
    mut timed_out = false
    with Fail[TimeoutError] = |_e| { timed_out = true } {
        supervised(deadline: deadline) {
            spawn {
                Time.sleep(latency_ms)
                ro _ = tx.try_send(true)
            }
        }
    }
    if timed_out {
        "cancelled"
    } else {
        match rx.try_recv() {
            Some(_) => "done"
            None    => "cancelled"
        }
    }
}

// `parallel for` — homogeneous fan-out: all iterations start at once,
// results collected into a []T in order.
fn fan_out(latencies []int, deadline Monotonic) Time -> []str {
    ro outcomes = parallel for i int in 0..latencies.len() {
        probe(latencies[i], deadline)
    }
    outcomes
}

fn main() Time {
    ro latencies []int = [10, 20, 300]   // ms; last one misses the budget
    ro t0 = Monotonic.now()
    ro deadline = t0 + 60.to_millis()
    ro outcomes = fan_out(latencies, deadline)
    mut done = 0
    mut cancelled = 0
    for i int in 0..outcomes.len() {
        if outcomes[i] == "done" { done += 1 } else { cancelled += 1 }
    }
    println("done=${done} cancelled=${cancelled}")
}
```

```
done=2 cancelled=1
```

(Timing-dependent in general, but structurally: two sources beat the
60ms budget, the 300ms one is cancelled — not silently discarded after
completing.)

## 9. Collections: Vec, HashMap, iterators

`[]T` is a **syntactic alias** for `Vec[T]` — methods work directly on a
`[]T` value, no `.iter()` boilerplate required to call adapters. A
map-literal `[k: v, ...]` constructs a `HashMap[K, V]` directly (D108).
`Option`/`Result` have real monadic `flat_map` (bind, no
`Option[Option[U]]` nesting) and `filter` (drop a `Some` by predicate).

```nova
// collections_tour.nv — Vec ([]T is a syntactic alias), HashMap, iterators.
module tour.collections_tour

import std.collections.hashmap.{HashMap}
import std.collections.vec_iter

fn main() {
    // `[]T` is a syntactic alias for `Vec[T]` — methods work directly, no
    // `.iter()` needed to call adapters like `.filter()`/`.count()`.
    ro xs []int = []int.of(1, 2, 3, 4, 5, 6, 7, 8)
    println("count=${xs.iter().count()} evens=${xs.iter().filter(|x| x % 2 == 0).count()}")

    // Map-literal `[k: v, ...]` constructs a HashMap[K, V] directly (D108).
    ro m HashMap[str, int] = ["a": 1, "b": 2, "c": 3]
    ro key = "b"
    ro val = m.get(key)
    println("m.len()=${m.len()} m[b]=${val}")

    // Option/Result combinators: flat_map for real monadic bind (no nested
    // Option[Option[U]]), filter to drop a Some by predicate.
    ro port = Some(10).flat_map(|x| Some(x * 100)) ?? 8080
    ro none_port = (None as Option[int]).flat_map(|x| Some(x * 100)) ?? 8080
    println("port=${port} none_port=${none_port}")

    ro evens_only = Some(10).filter(|x| x % 2 == 0)
    ro odds_dropped = Some(3).filter(|x| x % 2 == 0)
    println("evens_only=${evens_only} odds_dropped=${odds_dropped}")
}
```

```
count=8 evens=4
m.len()=3 m[b]=Some(2)
port=1000 none_port=8080
evens_only=Some(10) odds_dropped=None
```

## 10. Strings and formatting: `${}`, Display vs Debug

`"${expr}"` interpolates through the `Display` protocol (`@display`) —
plain, user-facing output. `"${expr:?}"` routes through `Debug` (`@debug`)
instead — the diagnostic form: a `str` gets quoted with escapes under
Debug but stays bare under Display; `int`/`bool` look the same either way.
`Option`/`Result` debug as `Some(v)`/`None`/`Ok(v)`/`Err(e)`.

```nova
// strings_tour.nv — `${}` interpolation, Display vs Debug format-spec `:?`.
module tour.strings_tour

fn main() {
    ro name = "Nova"
    ro n = 42
    // Plain interpolation routes through Display (@display) — bare values.
    println("hello ${name}, n=${n}")

    // `${expr:?}` routes to Debug (@debug) instead of Display — e.g. a str
    // gets quoted with escapes under Debug, bare under Display; primitives
    // like int/bool are the same either way.
    ro s = "hi"
    println("display=${s} debug=${s:?}")

    ro some = Some(7)
    ro none = None as Option[int]
    println("debug(some)=${some:?} debug(none)=${none:?}")
}
```

```
hello Nova, n=42
display=hi debug="hi"
debug(some)=Some(7) debug(none)=None
```

A type can also opt into `#impl(Debug)` to get a compiler-derived
memberwise `TypeName { field: value }` rendering (see
[D229](../spec/decisions/02-types.md) and
`spec_tests/conformance/d229_debug_format_spec.nv`) — verified there via
`nova test` and `assert`.

## 11. Modules: folder = module, imports, nova.toml

A **module** is either a single file `X.nv` or a **folder** `X/` whose
peer files all declare the *same* `module` path and share one namespace —
items in one peer file are visible in another without an import. Every
import path is fully qualified from the **package** root (the directory
with `nova.toml`); a package's own modules are imported the same way an
external package's are — e.g. `std.collections.vec` reaches into the
`std` package from anywhere, including from another module inside `std`
itself.

```nova
// greeter/core.nv — a FOLDER is one module made of co-equal peer files
// (tour.greeter), not one file per type. Both files here declare the same
// `module tour.greeter` (see loud.nv).
module tour.greeter

export type Greeting { text str }

export fn greet(name str) -> Greeting => { text: "Hello, ${name}!" }
```

```nova
// greeter/loud.nv — peer file, SAME module `tour.greeter` as core.nv.
// Items declared in either file are visible to both without an import —
// a folder-module shares one namespace across its peer files.
module tour.greeter

import std.unicode

export fn shout(g Greeting) -> str => g.text.to_upper()
```

```nova
// modules_tour.nv — importing a folder-module (tour.greeter, see
// tour/greeter/{core,loud}.nv). Every import path is fully qualified from
// the PACKAGE root — this file's package is `nova_examples` (declared in
// ../nova.toml), so a sibling folder-module is `nova_examples.tour.greeter`,
// same shape `std.collections.vec` uses to reach into the `std` package.
module tour.modules_tour

import nova_examples.tour.greeter.{greet, shout}

fn main() {
    ro g = greet("Nova")
    println(g.text)
    println(shout(g))
}
```

```
Hello, Nova!
HELLO, NOVA!
```

A minimal `nova.toml` at the package root declares the package name and
version — see [quickstart.md](quickstart.md#hello-nova) for the smallest
possible one. Workspaces (`[workspace] members = [...]`) group several
packages in a monorepo, as this repository's own root `nova.toml` does for
`std/`, `examples/`, and `spec_tests/`.

## 12. FFI and unsafe, briefly

Nova's opaque-pointer type is `*()` (pointer to unit — `void*` in C); the
old built-in `ptr` type was removed. Wrap a raw `*()` in a record for a
**typed handle** so distinct native resources (a file handle vs. a socket
handle) aren't interchangeable at compile time, even though both are
`void*` on the C side. `external fn name(args) -> ret` (D82) declares a
binding to a C symbol; the full cookbook — layered wrapping, tuple-by-value
returns, linking a static/shared library via `[ffi]`/`[ffi.staticlib]` in
`nova.toml` — is in [docs/ffi-cookbook.md](ffi-cookbook.md).

```nova
// ffi_tour.nv — FFI basics: opaque pointer `*()`, typed handles, `external fn`.
// Full cookbook: docs/ffi-cookbook.md. `ptr` as a built-in type was removed
// (Plan 134) — `*()` (pointer to unit = `void*` in C) is used everywhere.
module tour.ffi_tour

// A typed handle wraps a raw `*()` in a record so distinct resources
// (FileHandle vs SocketHandle) are NOT interchangeable at compile time,
// even though both are `void*` on the C side.
type FileHandle { ro value *() }

fn main() {
    // NULL literal — bitwise-zero opaque pointer.
    ro nothing *() = (0 as *())

    // *() constructed from an integer (normally this would come back from
    // an `external fn` call into a C library).
    ro raw *() = 0x1000 as *()

    // Round-trip cast: *() -> int -> *() (same bit pattern).
    ro raw_as_int = raw as int
    ro raw_back *() = raw_as_int as *()
    println("raw == raw_back: ${raw == raw_back}")

    ro handle = FileHandle { value: raw }
    println("handle.value as int = ${handle.value as int}")
    println("nothing as int = ${nothing as int}")
}
```

```
raw == raw_back: true
handle.value as int = 4096
nothing as int = 0
```

## Where to go next

- [spec/overview.md](../spec/overview.md) — the central idea (effects),
  the killer use-case, and the supporting design decisions in one page.
- [spec/decisions/](../spec/decisions/) — the D-numbered decision log,
  the authoritative source for every piece of Nova syntax and semantics;
  every construct in this tour traces back to a decision there.
- [docs/quickstart.md](quickstart.md) — install, build, and run
  `examples/mini_aggregator.nv` end to end.
- [examples/flagship/aggregator](../examples/flagship/aggregator) — the
  full-sized version of the concurrency example: a real HTTP server, a web
  UI, and the same effect-checked signature under `--strict-effects`.
- [docs/idioms/error-handling.md](idioms/error-handling.md),
  [docs/channels.md](channels.md), [docs/ffi-cookbook.md](ffi-cookbook.md),
  [docs/cleanup-cookbook.md](cleanup-cookbook.md) — deeper dives on
  errors, channels/`select`, FFI, and consume/cleanup respectively.
- [docs/test-conventions.md](test-conventions.md) — how `nova test` and
  `EXPECT_*` markers work.
