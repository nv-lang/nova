// SPDX-License-Identifier: MIT OR Apache-2.0
# Tutorial — Resource Cleanup with `consume{}` (Plan 110)

> **Plan 110.8.2.** Tutorial chapter introducing `consume X = ... { body }`
> scope-block pattern for resource cleanup.

## Why Cleanup Matters

When working with resources (files, database connections, locks),
you need to ensure they're **always released**, even when errors happen.
Forgetting to release leads to:

- **Resource leaks** — locked files, hung database connections.
- **Deadlocks** — Mutex held during panic, others waiting forever.
- **Data corruption** — Transaction not rolled back, half-written state.

Nova provides `consume X = expr { body }` scope-block to make
cleanup **automatic and reliable**.

## First Example — File Reading

```nova
fn read_config(path str) Fail[IoError] -> Config {
    consume f = File.open(path)? {
        ro raw = f.read_all()?
        Config.parse(raw)?
    }
    // f.@cleanup (File.close) automatically called here.
}
```

What happens:
1. `File.open(path)?` opens the file. If it fails, `?` propagates
   the error — `f` never bound, no cleanup needed.
2. `f` accessible inside `{ ... }` body.
3. After body completes (success OR error), `f.@cleanup(outcome)`
   is called automatically — closes the file.
4. If body errors, error re-propagates AFTER cleanup.

## The `Cleanup[E]` Protocol

Any type can be used in `consume X = ... { body }` by implementing
the `Cleanup[E]` protocol:

```nova
type Cleanup[E] protocol {
    @cleanup(outcome ScopeOutcome) Fail[E] -> ()
}

type ScopeOutcome
    | Success
    | Failure(str)
    | Panic(str)
```

- `E` is the type of errors `@cleanup` itself can throw
  (e.g., `IoError` if close can fail).
- `Success` — body completed normally.
- `Failure(msg)` — body threw an error (including cancel).
- `Panic(msg)` — body panicked (programming bug).

## Implementing Cleanup for Your Type

### Example: Database Transaction

```nova
type Transaction { conn DbConn, id int }

fn Transaction consume @cleanup(outcome ScopeOutcome) Fail[DbError] -> () {
    match outcome {
        Success      => @conn.commit(@id)?
        Failure(_)   => @conn.rollback(@id)?
        Panic(_)     => @conn.rollback_emergency()
    }
}
```

Usage:
```nova
fn process_order(db Db, order Order) Fail[DbError] -> () {
    consume tx = db.begin() {
        db.insert_order(order)?
        db.notify_warehouse(order.id)?
    }
    // Success → commit; failure → rollback (automatic).
}
```

### Example: Infallible Cleanup (Mutex Lock)

For resources where cleanup CAN'T fail, use `Cleanup[never]`:

```nova
fn MutexGuard consume @cleanup(_outcome ScopeOutcome) -> () => @unlock()
//                                                       ^^^^ no Fail[E]
```

Caller doesn't need `Fail[E]`:
```nova
fn increment_counter(state State) -> () {        // no Fail!
    consume _l = state.mutex.lock() {             // Cleanup[never]
        state.value += 1
    }
}
```

## Outcome Discrimination

`@cleanup` body can branch on outcome:

```nova
fn HttpRequest consume @cleanup(outcome ScopeOutcome) -> () {
    match outcome {
        Success      => @metrics.inc("http.success")
        Failure(msg) => {
            if msg.starts_with("cancel: ") {
                @metrics.inc("http.cancel")
            } else {
                @metrics.inc("http.error")
            }
        }
        Panic(_)     => @metrics.inc("http.panic")
    }
    @release_pool_slot()
}
```

## Nesting Scopes

Scopes nest naturally — inner exits before outer (LIFO):

```nova
fn deep_work(addr str) Fail[NetError] -> () {
    consume conn = pool.acquire()? {
        consume tx = conn.begin()? {
            consume stmt = tx.prepare(sql)? {
                stmt.execute(args)?
            }
            // stmt.@cleanup fires first.
        }
        // tx.@cleanup fires (commit or rollback).
    }
    // conn.@cleanup fires last (release to pool).
}
```

## Mixed `consume{}` + `defer`

Both work together. `defer` fires within its scope BEFORE `@cleanup`:

```nova
fn process() -> int {
    mut counter = 0
    consume r = Resource.new() {
        defer { counter += 100 }    // fires when body ends
        counter += r.id
    }
    // Order: defer body (counter += 100) → r.@cleanup
    counter
}
```

## Initialization Forms (D196)

Init expression supports:

```nova
// Direct method call
consume tx = db.begin() { ... }

// Result unwrap via ?
consume tx = db.try_begin()? { ... }   // Result[Tx, DbError] → Tx

// Option unwrap via !!
consume tx = maybe_tx()!! { ... }       // Option[Tx] → Tx

// Conditional (both branches same type)
consume r = if local { LocalRes.new() } else { LocalRes.connect()? } { ... }
```

Forms that don't work:
```nova
// ❌ Option without unwrap
consume tx = maybe_tx() { ... }
// → D196-wrapped-init-needs-unwrap

// ❌ Different types in branches
consume r = if cond { ResA.new() } else { ResB.new() } { ... }
// → D196-divergent-consumable
```

## Comparison with Other Languages

```rust
// Rust — implicit, no syntax marker
let f = File::open(path)?;
f.read_all()
// Drop fires automatically
```

```python
# Python with statement
with open(path) as f:
    f.read()
```

```nova
// Nova consume{}
consume f = File.open(path)? {
    f.read_all()?
}
```

Nova advantages:
- **Visible**: cleanup explicit, не magic Drop.
- **Cancel-shield**: cleanup protected from cancel storm (D188 R3).
- **Outcome-aware**: resource discriminates success/failure/panic.
- **Async-capable**: can `await` в `@cleanup` (D191).

## What's Next

- Read [Q-cleanup-semantics](../dev/idioms/consume-scope-cleanup.md) для
  decision trees.
- Read [Q-consumable-protocol](../dev/idioms/consume-scope-cleanup.md) для
  implementation details.
- Read [Q-application-effect](../dev/idioms/application-effect.md) для
  app-wide lifecycle.
- Read [cleanup-cookbook.md](cleanup-cookbook.md) для production recipes.

## See also

- [D188 — Cleanup + scope-block](../../spec/decisions/03-syntax.md#d188).
- [Plan 110](../plans/110-scoped-resources-radical-simplification.md).
- All Q-blocks under [docs/dev/idioms/](../dev/idioms/).
