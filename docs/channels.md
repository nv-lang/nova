# Channels and `select` in Nova

**English** | [Русский](channels.ru.md)

`Channel[T]` is the primary inter-fiber communication primitive. The
model is **capability-split** (Rust mpsc-style): `Channel.new(cap)`
returns a **pair** of objects with split capabilities —
`ChanWriter[T]` ("send only") and `ChanReader[T]` ("receive only").

`select { ... }` is multiplexed channel operations: it waits on
several recv/send operations at once and wakes on the first ready arm.

Spec: [D91](../spec/decisions/06-concurrency.md#d91) (channel
revision) + [D94](../spec/decisions/06-concurrency.md#d94) (select).

---

## Contents

- [Quickstart](#quickstart)
- [`Channel.new`](#channelnew)
- [`ChanWriter[T]` API](#chanwritert-api)
- [`ChanReader[T]` API](#chanreadert-api)
- [Idioms](#idioms)
  - [Drain via `while let`](#drain-via-while-let)
  - [Producer/consumer](#producerconsumer)
  - [Ping-pong](#ping-pong)
  - [Fan-in (multi-writer)](#fan-in-multi-writer)
  - [Relay (cross-channel pipeline)](#relay-cross-channel-pipeline)
  - [Passing to functions](#passing-to-functions)
- [`select { ... }`](#select--)
  - [Syntax and semantics](#syntax-and-semantics)
  - [Recv arm](#recv-arm)
  - [Send arm](#send-arm)
  - [Guard arms](#guard-arms)
  - [Default arm](#default-arm)
  - [Wildcard `_ = rx`](#wildcard-_--rx)
  - [Timeout via `ChanReader.close_after`](#timeout-via-chanreaderclose_after)
  - [Multi-arm fairness](#multi-arm-fairness)
- [`supervised(cancel:)` + `select`](#supervisedcancel--select)
- [Closing channels](#closing-channels)
- [Panic scenarios](#panic-scenarios)
- [Bootstrap limitations](#bootstrap-limitations)
- [Related documents](#related-documents)

---

## Quickstart

```nova
test "channel: send + recv FIFO" {
    ro { tx, rx } = Channel.new(4)
    tx.send(10)
    tx.send(20)
    tx.send(30)
    ro a = rx.recv()
    ro b = rx.recv()
    ro c = rx.recv()
    assert(a.unwrap_or(-1) == 10)
    assert(b.unwrap_or(-1) == 20)
    assert(c.unwrap_or(-1) == 30)
    tx.close()
}
```

```nova
test "select: data wins over timeout" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    mut branch = 0
    supervised {
        spawn {
            tx.send(99)
            select {
                Some(v) = rx                                          => { branch = v }
                Some(_) = ChanReader.close_after(Duration.from_millis(200)) => { branch = -1 }
            }
        }
    }
    assert(branch == 99)
}
```

---

## `Channel.new`

```nova
fn Channel[T].new(capacity int) -> { tx ChanWriter[T], rx ChanReader[T] }
```

Returns a **pair** — a record with fields `tx` (writer capability) and
`rx` (reader capability). Three extraction forms are supported:

```nova
// 1. Record destructure (Plan 53, most idiomatic)
ro { tx, rx } = Channel.new(4)

// 2. Record destructure with renaming
ro { tx: sender, rx: receiver } = Channel.new(4)

// 3. Tuple destructure (compat with D91 spec examples)
ro (tx, rx) = Channel.new(4)

// 4. Record access (when distinct lifetimes are needed)
ro ch = Channel.new(4)
ro tx = ch.tx
ro rx = ch.rx
```

**Capacity ≥ 1.** `Channel.new(0)` currently panics with
`"capacity must be >= 1"` ([Plan 44.1](plans/44.1-channel-hardening.md)
Ф.3) — zero-capacity rendezvous channels are not yet implemented.

**The element type (`T`)** is inferred from the first `send`/`recv`:

```nova
ro { tx, rx } = Channel.new(8)
tx.send(42)         // T = int
ro v = rx.recv()   // Option[int]
```

Explicit annotation via turbofish: `Channel[str].new(8)`.

---

## `ChanWriter[T]` API

| Method | Signature | Semantics |
|---|---|---|
| `send` | `(v T) -> bool` | Blocking send. Returns `true` on success; `false` if the channel is closed (no panic — [Plan 30](plans/30-channel-improvements.md)) |
| `try_send` | `(v T) -> bool` | Non-blocking. `true` if accepted; `false` if buffer full or closed |
| `close` | `() -> ()` | Closes this writer capability. Idempotent. With multi-writer (`clone`) — ref-counted: the channel actually closes only when all writers close |
| `clone` | `() -> ChanWriter[T]` | Creates an additional writer over the same buffer. `writer_count++` |
| `is_closed` | `() -> bool` | `true` if the buffer is closed *and* this writer no longer has send capability |

### `send` returns `bool`

```nova
test "channel: send after close returns false, does not panic" {
    ro { tx, rx: _rx } = Channel.new(2)
    assert(tx.send(1))
    tx.close()
    assert(!tx.send(99))    // false: channel closed
}
```

Useful for graceful shutdown without try/catch wrapping:

```nova
fn produce(tx ChanWriter[Job], jobs []Job) {
    mut i = 0
    while i < jobs.len() {
        if !tx.send(jobs[i]) {
            break               // consumer closed — exit silently
        }
        i = i + 1
    }
}
```

### `try_send` — non-blocking

```nova
test "channel: try_send full buffer" {
    ro { tx, rx } = Channel.new(2)
    assert(tx.try_send(10))
    assert(tx.try_send(20))
    assert(!tx.try_send(30))            // buffer full
    assert(rx.recv().unwrap_or(-1) == 10)
    assert(tx.try_send(30))             // slot freed
    tx.close()
}
```

### `clone` — multi-writer

```nova
test "channel: fan-in — two writers, one reader" {
    ro { tx, rx } = Channel.new(8)
    ro tx2 = tx.clone()                // writer_count = 2
    mut sum = 0
    supervised {
        spawn { tx.send(1);  tx.send(2);  tx.send(3);  tx.close() }
        spawn { tx2.send(10); tx2.send(20); tx2.send(30); tx2.close() }
        spawn {
            while Some(v) = rx.recv() { sum = sum + v }
        }
    }
    assert(sum == 66)
}
```

The channel closes **only when all writers have called `close()`.**
Internally — a ref count (`writer_count`): `Channel.new` initializes
to 1, `clone()` increments, `close()` decrements. When it reaches 0,
the channel actually closes and `rx.recv()` starts returning `None`.

---

## `ChanReader[T]` API

| Method | Signature | Semantics |
|---|---|---|
| `recv` | `() -> Option[T]` | Blocking recv. `Some(v)` while there is data or the channel is open; `None` once the channel is closed *and* the buffer is empty |
| `try_recv` | `() -> Option[T]` | Non-blocking. `None` if buffer is empty (does NOT mean the channel is closed — check `is_closed()` separately) |
| `len` | `() -> int` | Number of items currently in the buffer |
| `capacity` | `() -> int` | Capacity set by `Channel.new` |
| `is_closed` | `() -> bool` | `true` once all writers have closed |

### `recv` → `Option[T]`

A closed channel is **not an error**; it is a valid "source exhausted"
outcome. `Option[T]` composes with `match`, `?`, `??`, and the
idiomatic `while let` loop.

```nova
test "channel: close + recv drain" {
    ro { tx, rx } = Channel.new(4)
    tx.send(1)
    tx.send(2)
    tx.close()
    assert(rx.recv().unwrap_or(-1) == 1)
    assert(rx.recv().unwrap_or(-1) == 2)
    assert(rx.recv().is_none())             // drained — None
    assert(rx.recv().is_none())             // repeated — still None
}
```

### `try_recv` distinguishes empty-open from empty-closed

```nova
test "channel: try_recv distinguishes empty-open from empty-closed via is_closed" {
    ro { tx, rx } = Channel.new(4)
    assert(rx.try_recv().is_none())     // empty, open
    assert(!rx.is_closed())
    tx.close()
    assert(rx.try_recv().is_none())     // empty, closed — same None
    assert(rx.is_closed())              // distinguish via is_closed
}
```

### `len` / `capacity`

```nova
test "channel: len and capacity" {
    ro { tx, rx } = Channel.new(8)
    assert(rx.capacity() == 8)
    assert(rx.len() == 0)
    tx.send(1)
    tx.send(2)
    assert(rx.len() == 2)
    ro _ = rx.recv()
    assert(rx.len() == 1)
    tx.close()
}
```

---

## Idioms

### Drain via `while let`

```nova
test "channel: while-let drain pattern" {
    ro { tx, rx } = Channel.new(4)
    tx.send(10)
    tx.send(20)
    tx.send(30)
    tx.close()
    mut sum = 0
    while Some(v) = rx.recv() {
        sum = sum + v
    }
    assert(sum == 60)
}
```

This is the **most idiomatic** receiver pattern. The loop terminates
automatically once the channel is closed and the buffer is empty —
`recv()` returns `None`.

### Producer/consumer

```nova
test "channel: producer-consumer pipeline" {
    ro { tx, rx } = Channel.new(4)
    mut sum = 0
    supervised {
        spawn {
            tx.send(1)
            tx.send(2)
            tx.send(3)
            tx.send(4)
            tx.send(5)
            tx.close()                  // important: producer closes after finishing
        }
        spawn {
            while Some(v) = rx.recv() {
                sum = sum + v
            }
        }
    }
    assert(sum == 15)
}
```

### Ping-pong

```nova
test "channel: ping-pong" {
    ro { tx: tx1, rx: rx1 } = Channel.new(1)
    ro { tx: tx2, rx: rx2 } = Channel.new(1)
    mut result = 0
    supervised {
        spawn {
            tx1.send(10)
            ro reply = rx2.recv()
            result = reply.unwrap_or(-1)
            tx1.close()
        }
        spawn {
            ro msg = rx1.recv()
            tx2.send(msg.unwrap_or(0) * 2)
            tx2.close()
        }
    }
    assert(result == 20)
}
```

### Fan-in (multi-writer)

Several spawns produce, one consumes.

```nova
ro { tx, rx } = Channel.new(8)
supervised {
    for item in work_items {
        ro worker_tx = tx.clone()      // each spawn gets its own capability
        spawn {
            worker_tx.send(process(item))
            worker_tx.close()
        }
    }
    tx.close()                          // close the root writer
    spawn {
        while Some(v) = rx.recv() {
            collect(v)
        }
    }
}
```

**Why `clone()` is required:** without it, every spawn would capture
the same `tx` by managed reference; `close()` from the first one would
close the channel for everyone. With `clone()`, each spawn holds its
own capability and closes it independently — the channel only closes
once all `worker_count + 1` writers have called `close()`.

### Relay (cross-channel pipeline)

```nova
fn relay(rx ChanReader[int], tx ChanWriter[int]) {
    while Some(v) = rx.recv() {
        tx.send(v * 2)
    }
    tx.close()
}

test "channel: relay — Receiver → Sender pipeline through a function" {
    ro { tx: tx1, rx: rx1 } = Channel.new(4)
    ro { tx: tx2, rx: rx2 } = Channel.new(4)
    tx1.send(1)
    tx1.send(2)
    tx1.send(3)
    tx1.close()
    relay(rx1, tx2)
    mut s = 0
    while Some(v) = rx2.recv() { s = s + v }
    assert(s == 12)
}
```

### Passing to functions

Capability types in signatures make APIs explicit.

```nova
fn fill_channel(tx ChanWriter[int], values []int) {
    mut i = 0
    while i < values.len() {
        tx.send(values[i])
        i = i + 1
    }
    tx.close()
}

fn drain_channel(rx ChanReader[int]) -> int {
    mut sum = 0
    while Some(v) = rx.recv() {
        sum = sum + v
    }
    sum
}

test "channel: Sender and Receiver passed independently" {
    ro { tx, rx } = Channel.new(8)
    fill_channel(tx, [100, 200, 300])
    ro s = drain_channel(rx)
    assert(s == 600)
}
```

Pass `tx` to a function that should not be able to recv — the type
system guarantees the callee cannot read (and vice versa).

---

## `select { ... }`

### Syntax and semantics

```
select-expr  = 'select' '{' NL* select-arm+ '}'
select-arm   = channel-arm | default-arm
channel-arm  = pattern '=' (recv-target | send-op) guard? '=>' arm-body NL*
recv-target  = expr                                 // bare rx
send-op      = expr '.' 'send' '(' expr ')'
guard        = 'if' expr
default-arm  = '_' '=>' arm-body NL*
arm-body     = block | stmt
```

> **Bootstrap recv form**: `Some(v) = rx => { ... }` — bare `rx`
> without `.recv()`. The spec also describes `pattern = rx.recv()`;
> the current compiler only accepts the bare form.

**Semantics** ([D94](../spec/decisions/06-concurrency.md#d94)):

1. **Guard evaluation** — `if <expr>` before the arrow disables the
   arm when false.
2. **Immediate check** — all enabled arms are checked in pseudo-random
   order (Fisher-Yates). If ≥1 is ready — the arm runs without
   parking.
3. **Park** — if none is ready and there is no default: register a
   waiter on every arm, park the fiber.
4. **Wake** — the first ready arm wakes the fiber; other waiters are
   unlinked. A `done` flag prevents double-wake.
5. **Fairness** — Fisher-Yates shuffle on every iteration (no
   starvation).
6. **`_ => ...` (default)** — when present: step 2 always succeeds;
   the fiber never parks.
7. **All channels closed + no default** → panic
   `"select: all channels closed"`.
8. **Cancel** (`tok.cancel()` from `supervised(cancel:)`) — cancels
   all pending waiters; the fiber wakes up, checks `cancel_requested`.

### Recv arm

```nova
test "select single recv: value from channel" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    supervised {
        spawn { tx.send(42) }
        spawn {
            mut got = 0
            select {
                Some(v) = rx => { got = v }
            }
            assert(got == 42)
        }
    }
}
```

### Send arm

```nova
test "select send arm: sends to channel with space" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    mut sent = 0
    select {
        tx.send(77) => { sent = 1 }
        _           => { sent = -1 }
    }
    assert(sent == 1)
    ro opt = rx.recv()
    mut got = 0
    match opt {
        Some(v) => { got = v }
        None    => { got = -1 }
    }
    assert(got == 77)
}
```

### Guard arms

```nova
test "select guard: disabled arm falls through to default" {
    ro ch = Channel.new(1)
    ch.tx.send(10)
    ro rx = ch.rx
    ro enabled = false
    mut branch = 0
    select {
        Some(v) = rx if enabled => { branch = v }
        _                       => { branch = -1 }
    }
    assert(branch == -1)         // arm disabled — default ran
}
```

A guard is a pre-condition. If `false`, the arm is off even before the
channel's ready state is checked. Equivalent to `if` in Tokio
`select!`. Go does not support guards.

### Default arm

`_ => { ... }` runs when no channel arm is ready *right now*. Turns
`select` into a non-blocking probe.

```nova
test "select recv with default: default when channel empty" {
    ro ch = Channel.new(1)
    ro rx = ch.rx
    mut branch = 0
    select {
        Some(_) = rx => { branch = 1 }
        _            => { branch = 2 }     // ← default
    }
    assert(branch == 2)
}
```

### Wildcard `_ = rx`

A wildcard in the recv-target fires on **both** states: `Some(v)` and
`None` (closed). `Some(v) = rx` fires only on a real value.

```nova
test "Some arm skips closed+empty, picks open channel with data" {
    ro ch1 = Channel.new(1)
    ro ch2 = Channel.new(1)
    ro tx1 = ch1.tx
    ro tx2 = ch2.tx
    ro rx1 = ch1.rx
    ro rx2 = ch2.rx

    tx1.close()                  // ch1 closed+empty
    tx2.send(42)                 // ch2 has data

    mut result = 0
    select {
        Some(v) = rx1 => { result = -1 }     // Some does NOT fire on closed
        Some(v) = rx2 => { result = v  }     // ← runs
    }
    assert(result == 42)
}

test "wildcard fires immediately on closed+empty channel" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    tx.close()

    mut fired = false
    select {
        _ = rx => { fired = true }           // ← wildcard catches closed
    }
    assert(fired)
}
```

**Rule:**
- `Some(v) = rx` — need a real value from the channel
- `_ = rx` — need **any** ready state (value or closed)

A dedicated `None = rx` arm is not implemented yet (Plan 31 "spec
differences" section); use `_ = rx` + `match` inside the arm body or
`rx.is_closed()` after `recv` to differentiate.

### Timeout via `ChanReader.close_after`

There is no dedicated `timeout =>` arm — a timeout is just a regular
recv channel produced by `ChanReader.close_after(Duration)`.

```nova
import std.time.duration

test "select timeout: fires when channel stays empty" {
    ro ch = Channel.new(1)
    ro rx = ch.rx
    mut branch = 0
    supervised {
        spawn {
            select {
                Some(_) = rx                                          => { branch = 1 }
                Some(_) = ChanReader.close_after(Duration.from_millis(50)) => { branch = 2 }
            }
        }
    }
    assert(branch == 2)
}

test "select timeout: data wins over timeout" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    mut branch = 0
    supervised {
        spawn {
            tx.send(99)
            select {
                Some(v) = rx                                           => { branch = v }
                Some(_) = ChanReader.close_after(Duration.from_millis(200)) => { branch = -1 }
            }
        }
    }
    assert(branch == 99)
}
```

`ChanReader.close_after(d Duration) -> ChanReader[()]` lives in
[`std/concurrency/timer.nv`](../std/concurrency/timer.nv) as a
compiler builtin (the runtime call is
`nova_chan_reader_close_after_ns(d.nanos)`). The channel closes after
`d`; the first `recv()` returns `Some(())` post-firing, then `None`.

**Type safety** (Plan 65 revision, 2026-05-18): the API used to be
`Time.after(int ms)` — bare int (ms/µs/sec?). Now — typed `Duration`.
Migration: `cargo run --bin migrate_plan65 -- --apply` rewrites
literal arguments automatically (see
[docs/nova-cli.md](nova-cli.md#migrate_plan65)).

**Edge cases:**
- `Duration.ZERO` or `Duration.from_*(0)` — the channel is created
  *already closed*; the first `recv()` returns `None` without
  yielding (fast path, no libuv timer)
- Sub-millisecond `Duration` (`from_nanos(500_000)`) — rounded **up**
  to 1 ms (libuv granularity)
- Negative `Duration` — runtime panic with the nanosecond value

**Performance:** each call currently allocates a fresh `uv_timer_t`
(~120 bytes + a syscall). Adequate for idiomatic 10-100 concurrent
timers. A custom timer wheel for high-throughput (10k+ HTTP timeouts)
is [Plan 66](plans/66-timer-wheel-and-tick-every.md).

### Multi-arm fairness

```nova
test "select multi-arm: fairness — both channels get served" {
    ro n = 50
    ro ch1 = Channel.new(n)
    ro ch2 = Channel.new(n)
    ro tx1 = ch1.tx
    ro tx2 = ch2.tx
    ro rx1 = ch1.rx
    ro rx2 = ch2.rx

    mut from1 = 0
    mut from2 = 0

    supervised {
        spawn {
            mut i = 0
            while i < n {
                tx1.send(1)
                tx2.send(2)
                i += 1
            }
        }
        spawn {
            mut total = 0
            while total < n * 2 {
                select {
                    Some(v) = rx1 => { from1 += 1; ro _ = v }
                    Some(v) = rx2 => { from2 += 1; ro _ = v }
                }
                total += 1
            }
        }
    }
    assert(from1 > 0)
    assert(from2 > 0)
    assert(from1 + from2 == n * 2)
}
```

Fisher-Yates shuffle on every iteration ensures both channels get
their share (Go uses the same approach — Nova's `select` is
semantically compatible).

---

## `supervised(cancel:)` + `select`

```nova
test "select: data wins supervised(cancel:) race" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    mut branch = 0
    mut error_seen = false

    ro tok = CancelToken.new()
    with Fail = handler Fail {
        fail(_msg) {
            error_seen = true
            interrupt ()
        }
    } {
        supervised(cancel: tok) {
            spawn {
                tx.send(77)
                Time.sleep(500)
                tok.cancel()
            }
            spawn {
                select {
                    Some(v) = rx                                           => { branch = v }
                    Some(_) = ChanReader.close_after(Duration.from_millis(200)) => { branch = -1 }
                }
            }
        }
    }
    assert(!error_seen)
    assert(branch == 77)
}
```

`tok.cancel()` cancels **every** pending waiter in any `select` block
inside `supervised(cancel: tok)`. The fiber wakes, checks
`cancel_requested`, and exits the supervised block via structured
cancellation (D75 / [Plan 49](plans/49-cancel-throw-routing.md)).

Cancellation is **not an error** — it does not turn into `throw` and
does not invoke a `Fail` handler. The behavior is symmetric to Go's
`context.Done()` but with a typed `CancelToken` (D75) instead of an
`error` channel.

---

## Closing channels

### Idiom: `defer tx.close()`

**Spec preference** — `defer` guarantees `close` on scope exit:

```nova
fn run_pipeline() Net -> () {
    ro { tx, rx } = Channel[Job].new(10)
    defer tx.close()

    supervised {
        spawn { for j in jobs { tx.send(j) } }
        spawn { while Some(j) = rx.recv() { process(j) } }
    }
}   // <-- tx.close() always runs; rx.recv() in the spawn gets None and terminates
```

### Bootstrap limitation: `defer` + tuple/record destructure

> ⚠️ **Known issue:** `defer tx.close()` does **not** work alongside
> `let (tx, rx) = Channel.new(N)` or `let { tx, rx } = Channel.new(N)`
> — `defer` emits the setjmp frame *before* the variable declarations,
> which breaks scope (Plan 25 G8 — will be fixed once open-coded defer
> lands).
>
> **Workaround:** explicit `tx.close()` at the end of the function, or
> split the destructure:
>
> ```nova
> let ch = Channel.new(N)
> let tx = ch.tx
> let rx = ch.rx
> defer tx.close()    // OK — tx is declared directly
> // ...
> ```

### No auto-close on drop

Unlike Rust mpsc, Nova does not have deterministic destructors
(managed heap, [D6](../spec/decisions/05-memory.md#d6)). The GC will
collect a sender "eventually" — which is **non-deterministic** and
would make tests flaky. That is why `close()` is always explicit.

### Idempotent

```nova
test "channel: close idempotent" {
    ro { tx, rx } = Channel.new(2)
    tx.close()
    tx.close()                  // not an error
    assert(rx.is_closed())
}
```

With multi-writer (`clone`), a repeated `close()` on *one* writer does
not double-decrement `writer_count` (idempotent per instance).

---

## Panic scenarios

| Condition | Message |
|---|---|
| `Channel.new(0)` | `"capacity must be >= 1"` (Plan 44.1 Ф.3) |
| `select` with all channels closed and no default | `"select: all channels closed"` (Plan 31 Ф.6) |
| `ChanReader.close_after(<negative Duration>)` | panic with the nanosecond value |
| `select` with `arm_count > stack` | overflow caught before allocation — explicit panic |

`tx.send` on a closed channel is **not a panic** — returns `false`
(Plan 30). `rx.recv` on closed+drained is **not a panic** — returns
`None`.

---

## Bootstrap limitations

| What does not work / is deferred | Plan |
|---|---|
| Dedicated `None = rx` arm (only `_ = rx` wildcard) | Plan 31 follow-up |
| `Channel.new(0)` zero-capacity rendezvous | Plan 44.2+ |
| `defer tx.close()` + tuple/record destructure | [Plan 25](plans/25-production-readiness-roadmap.md) G8 |
| `pattern = rx.recv()` (with `.recv()`) form in select | only bare `pattern = rx` works |
| `oneshot::channel<T>` / `watch::channel<T>` / `broadcast::channel<T>` (Tokio variants) | Plan 44.2 |
| `recv_many` batch API | Plan 44.1 Ф.4 follow-up |
| Lock-free SPSC flavor | Plan 50+ (Loom-verified) |
| `tick_every(Duration)` periodic ticker | [Plan 66](plans/66-timer-wheel-and-tick-every.md) |
| `close_at(Monotonic)` absolute deadline | [Plan 65](plans/65-chanreader-close-after.md) Ф.13 (✅ shipped) |
| Time effect mock for deterministic timer tests | [Plan 65](plans/65-chanreader-close-after.md) Ф.10 (✅ shipped) |

---

## Related documents

- [`spec/decisions/06-concurrency.md`](../spec/decisions/06-concurrency.md) —
  D79 / D91 / D94 / D75 / D97 (channels, select, cancel, fiber stacks)
- [`docs/plans/21-channel-revision-implementation.md`](plans/21-channel-revision-implementation.md)
  — D91 implementation (capability split)
- [`docs/plans/30-channel-improvements.md`](plans/30-channel-improvements.md)
  — `send → bool` + `tx.clone()`
- [`docs/plans/31-channel-select.md`](plans/31-channel-select.md) —
  `select { ... }` (D94)
- [`docs/plans/44.1-channel-hardening.md`](plans/44.1-channel-hardening.md)
  — production-grade M:N safety (atomics, doubly-linked, cache padding)
- [`docs/plans/49-cancel-throw-routing.md`](plans/49-cancel-throw-routing.md)
  — cancel semantics (typed `CancelToken[T]`)
- [`docs/plans/65-chanreader-close-after.md`](plans/65-chanreader-close-after.md)
  — `ChanReader.close_after(Duration)` (rename of `Time.after`)
- [`docs/plans/66-timer-wheel-and-tick-every.md`](plans/66-timer-wheel-and-tick-every.md)
  — periodic ticker + custom timer wheel (P2)
- [`std/concurrency/timer.nv`](../std/concurrency/timer.nv) —
  `ChanReader.close_after` doc surface
- [`std/time/duration.nv`](../std/time/duration.nv) — `Duration` type
- [`nova_tests/runtime/channels.nv`](../nova_tests/runtime/channels.nv)
  — 22 channel API tests
- [`nova_tests/concurrency/`](../nova_tests/concurrency/) —
  `select_*.nv` tests (7 files)
