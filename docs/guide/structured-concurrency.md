**English** | [Русский](structured-concurrency.ru.md)

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Structured concurrency: `supervised` scopes, deadlines, cancellation

A `supervised { }` scope owns every fiber spawned inside it: the scope does
not exit until all children finished or were cancelled, and `spawn` is only
legal inside such a scope (D50). This page covers the scope *lifetime*
controls — deadlines (`timeout:` / `deadline:`) and cooperative cancellation
(`cancel:`) — and the one placement rule people get wrong on the first try.

For channels and `select`, see [channels](channels.md). For resource cleanup
on scope exit, see the [cleanup cookbook](cleanup-cookbook.md).

## Deadlines: `timeout:` / `deadline:`

A scope can carry its own deadline — either relative (`timeout:` takes a
`Duration`) or absolute (`deadline:` takes a `Monotonic` instant):

```nova
supervised(timeout: 5.to_seconds()) {
    spawn { work() }
}
```

When the deadline expires, all children are cancelled and the scope **fails
with a typed `TimeoutError`**. This is the "deadline on a scope" school
(Kotlin/Swift/Trio) rather than the Go/Rust "deadline on a descriptor"
school: the deadline is attached to a region of the program, not to each
individual I/O handle inside it.

### Where the handler goes — outside the scope

The deadline belongs to the scope, so its expiry is an **exit** event: by the
time `TimeoutError` flies, the scope — including any handler installed inside
it — is already unwound. The handler must be installed **around** the scope:

```nova
// ✅ WORKING form: handler OUTSIDE supervised(timeout:)
mut timed_out = AtomicBool.new(false)
ro r = with Fail[TimeoutError] = |_e| {
    timed_out.store(true)
    0
} {
    supervised(timeout: 50.to_millis()) {
        spawn { 5000.to_millis().sleep() }
    }
    5                       // reached only when the scope finished in time
}
```

The natural-looking inverse — handler *inside* the scope — compiles, but
catches nothing:

```nova
// ❌ NON-WORKING form: handler INSIDE the scope it is supposed to guard.
// Compiles, but on expiry the program dies with:
//   nova: unhandled Fail: supervised-timeout: scope deadline exceeded
supervised(timeout: 50.to_millis()) {
    with Fail[TimeoutError] = |_e| { println("never reached") } {
        spawn { 5000.to_millis().sleep() }
    }
}
```

Both snippets are verified against the real compiler; the second one exits
with code 127 and the message shown above. If you don't need a fallback
value, it is also fine to install no handler and let `TimeoutError`
propagate to your caller.

(The `mut timed_out` flag is an `AtomicBool` deliberately: a `with Fail`
handler runs in the fiber of the failing operation, not the installing
fiber — a bare `mut` flag would be a data race under M:N, D441.)

## Cooperative cancellation: `cancel:`

A scope can also be finished early from outside the deadline machinery — via
a `CancelToken`:

```nova
ro tok = CancelToken.new()
supervised(cancel: tok) {
    spawn { 10.to_millis().sleep(); tok.cancel() }
    spawn { 5000.to_millis().sleep() }
}
assert(tok.is_cancelled())      // distinguish the outcome after the scope
```

Unlike a deadline, **cancellation throws nothing** — there is nothing to
catch, and that is by design: `tok.cancel()` is a *normal* early completion,
not a failure. The scope simply wraps up sooner and control continues on the
next line. To learn *how* the scope ended, ask the token:
`tok.is_cancelled()`.

`cancel:` and `timeout:` compose — the earlier of the two wins. If the token
fires first, no `TimeoutError` is raised; if the deadline fires first, it is.

> **Current limitation (tracked, being fixed):** today `tok.cancel()` reliably
> cancels **spawned children** of the scope, but does not interrupt a direct
> blocking operation in the scope's own body. Until that is fixed, structure
> cancellable work as `spawn`-children — exactly as in the snippet above.

## Network reads: always under a deadline

Every network read in your program should live under `supervised(timeout:)`:

```nova
fn fetch_head(addr str) Net Time -> Option[str] {
    with Fail[TimeoutError] = |_e| { None } {
        mut out = ""
        supervised(timeout: 5.to_seconds()) {
            consume conn = TcpStream.connect(addr)!!
            out = conn.read_text(1024)!!
            conn.close()
        }
        Some(out)
    }
}
```

This is not a style preference. The everyday scenario "the server sent part
of the reply and closed the connection" otherwise leaves a bare `read()`
stuck **forever** — and there is a known open defect where a *second* read
after partially received data is not woken even by the scope deadline
(tracked in the project registry; root cause in the libuv layer on Windows,
deliberately deferred until the next tag). The scope deadline reliably
interrupts the *first* read — which is exactly what the pattern above
guards. Until the defect is closed, do not build protocol loops that issue
repeated reads on a stream whose peer may half-close mid-reply; prefer
single bounded reads per scope, as above.

## See also

- [channels](channels.md) — `select`, timeout-as-an-arm pattern, `ChanReader.close_after`
- [cleanup cookbook](cleanup-cookbook.md) — `consume{}` exit timeouts on scope unwind
- `std/src/concurrency/supervised_deadline_test.nv` — the authoritative
  executable examples for every `timeout:`/`deadline:`/`cancel:` combination
- Spec: D50 (structured scopes), D349 (deadlines), D441 (handler fiber
  semantics), `spec/decisions/06-concurrency.md`
