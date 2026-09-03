<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Where was I called from? — `CallerLoc` and `caller_loc()`

A diagnostic helper that reports *itself* is useless. Write a wrapper around
`panic`, and every failure points at the wrapper's own line — the same line
every time, telling you nothing about which of its hundred call sites went
wrong.

Nova gives a function the location it was called from, as an ordinary
parameter:

```nova
export fn ice(msg str, loc CallerLoc = caller_loc()) -> never {
    report(msg, loc.file, loc.line)
}

// resolve.nv:412
ice("unresolved symbol")        // loc = { file: "resolve.nv", line: 412 }
```

There is no attribute, no hidden channel, and the type of `ice` is an ordinary
function type. `loc` is a parameter like any other; it simply has a default.

## Why a default gives the *caller's* line

`caller_loc()` means one thing: **the location of this expression**. Written
plainly in a body it returns its own line — the plain `__FILE__`/`__LINE__`
you would expect:

```nova
fn here() -> CallerLoc => caller_loc()      // this function's own line
```

A default argument in Nova is substituted **at the call site**, so when
`caller_loc()` is a default it physically lands in the caller's body — and
"the location of this expression" is the caller's line. One rule, no modes.

It costs nothing at run time. There is no call: the compiler emits a constant
pointer to an interned static record, one per distinct file-and-line.

## Contracts, `assert`, `panic` and `throw` take it for free

Inside a function that **declares** a `CallerLoc` parameter, the location is
picked up automatically — you do not repeat it:

```nova
fn half(n int, loc CallerLoc = caller_loc()) -> int
    requires n > 0, "half wants a positive"
{
    n / 2
}

half(-1)     // panic names the CALLER's line, not the clause's
```

This applies to `requires`, `assert`, `debug_assert`, `panic` and `throw`
(which records it as the failure's `site`). Declaring two `CallerLoc`
parameters is refused by name — with two, nothing could choose between them,
and choosing quietly is worse than refusing.

### `ensures` is the deliberate exception

A violated **postcondition** keeps naming its own line, even in a function
that has the parameter. That is not an oversight: a caller cannot break your
postcondition even in principle — it controls only the arguments. If the
result is wrong for arguments that passed `requires`, the bug is in the body
(or in a `requires` too weak to exclude it), and both belong to the function.

Every design-by-contract language assigns blame the same way — Eiffel, D,
Ada, C#. One phrase instead of a table: **what speaks about the input, or
outward, points at the caller; `ensures` speaks about your own output.**

## Chains are forwarded by hand

A wrapper that wants *its* caller blamed passes the location on. Note the
name: a defaulted parameter is keyword-only, so positional does not compile.

```nova
fn ensure(ok bool, loc CallerLoc = caller_loc()) -> () {
    if !ok { ice("check failed", loc: loc) }   // by NAME
}

// typing.nv:88
ensure(t.is_known())      // ice() reports typing.nv:88, through both wrappers
```

Omit `loc: loc` and `ice` gets its own default — the line inside `ensure`.
Both readings are literal, and the difference is visible in the text.

**This is the price, and it is deliberate.** Nova follows C# and Swift here
rather than Rust: no implicit propagation, and therefore no hidden parameter
the type system has to carry. Forwarding is the author's discipline, and the
compiler will not remind you. What is bought with it: taking a function as a
value, passing it through a higher-order function, or calling it through a
protocol all keep working unchanged, because nothing about its type changed.

## The message keeps both places

```
caller.nv:17: assert failed: text (cond) [in ice at diag.nv:129]
```

First the location you need; in the tail, where the check physically fired, so
a bug in the wrapper itself stays findable.

## Reference

`type CallerLoc { file str, line int }` and `fn caller_loc() -> ro CallerLoc`
live in the prelude — no import. `file` holds the path exactly as the CLI
received it, the same convention contract violations already print.

The design and the reasoning behind each choice, including the implicit form
that was considered and rejected: [D468](../../spec/decisions/08-runtime.md#d468).
