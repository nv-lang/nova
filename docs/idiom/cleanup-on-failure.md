// SPDX-License-Identifier: MIT OR Apache-2.0
# Cleanup-on-failure — defer/errdefer/okdefer для consume-resources

> Practical guide для Plan 100.4 family
> ([D158](../../spec/decisions/03-syntax.md#d158)–
> [D162](../../spec/decisions/03-syntax.md#d162)). Production-grade
> resource cleanup на всех code-path'ах.

## Defer family — semantic recap

| Statement | Срабатывает на |
|---|---|
| `defer { ... }` | **все** exit-paths (success / error / panic / interrupt) |
| `errdefer { ... }` | **error-paths**: throw, panic, interrupt (после D162 amend) |
| `okdefer { ... }` | **success-path**: normal exit, return expr |

LIFO order. Mixed family — каждое срабатывает по своему predicate'у.

## Canonical Transaction lifecycle

```nova
fn process_order(data Data) Fail[OrderErr] Db -> Receipt {
    consume tx = db.begin()
    errdefer { tx.rollback()? }                 // error → rollback (D158 failable)
    okdefer  { tx.commit()?   }                 // success → commit (D160 + D158)
    let order = db.insert(data)?
    db.notify(order)?
    return Receipt { id: order.id }
}
```

Exhaustive cover (D162): errdefer + okdefer — оба exit-paths covered
без explicit commit/rollback в body. Failable cleanup composes
автоматически (D158 → Plan 49 multi-error).

## Когда что использовать

### `defer` — единственный cleanup-action

Когда нет «success vs error» distinction (только close):

```nova
fn process() Fs -> () {
    consume f = File.open("x.txt")?
    defer { f.close() }                         // и success, и error
    let data = f.read_all()?
    println(data)
}
```

### `errdefer { rollback }` + explicit commit

Когда хочется явный success-path:

```nova
fn process() Fail[Err] -> () {
    consume tx = begin()
    errdefer { tx.rollback() }                  // error → rollback
    do_work()?
    tx.commit()                                  // explicit success
}
```

### `errdefer { rollback }` + `okdefer { commit }` — symmetric

Симметрично, defer-family покрывает оба:

```nova
fn process() Fail[Err] -> () {
    consume tx = begin()
    errdefer { tx.rollback() }                  // error path
    okdefer  { tx.commit() }                    // success path
    do_work()?
}
```

## Async cleanup (D159)

`defer` body может suspend (Time.sleep, Channel.recv, Net.*):

```nova
fn process() Fs Time -> () {
    consume conn = open_connection()?
    defer { conn.graceful_drain(timeout: 5_s) } // async cleanup
    do_io()?
    conn.close()?
}
```

Cancel-safe: если outer scope cancelled mid-cleanup, **cleanup
completes ПЕРВЫМ**, cancel propagates AFTER. Без D159 graceful close
сокета или DB-connection невозможен.

## Failable cleanup composition (D158)

Когда cleanup сам fails:

```nova
fn process() Fail[Err] -> () {
    consume tx = begin()
    defer { tx.commit() }                       // commit may Fail
    do_work()?                                   // throws Err1

    // Unwinding:
    //   1. body throws Err1
    //   2. defer fires; tx.commit() fails CommitErr
    //   3. composite: { primary: Err1, suppressed: [CommitErr] }
    //   4. caller получает composite через Fail[Err]
}
```

`fn-sig` обязан declare `Fail[E]` — D158 enforces compile-time
visibility.

Caller inspects composite:

```nova
match process() {
    Ok(_) => println("done"),
    Err(e) => {
        println("primary: ${e.primary()}")
        for s in e.suppressed() {
            println("  suppressed: ${s}")
        }
    }
}
```

## Multi-defer LIFO + partial failure (D161)

```nova
fn process() Fail -> () {
    defer { cleanup_a() }                       // outer (LIFO LAST)
    defer { cleanup_b() }
    defer { cleanup_c() }                       // inner (LIFO FIRST)
    body()?                                      // throws Err_main
    // LIFO unwinding:
    //   cleanup_c — fails (suppressed [C])
    //   cleanup_b — fails (suppressed [C, B])
    //   cleanup_a — fails (suppressed [C, B, A])
    // ALL N attempted (Rust would abort here on first fail)
    // composite: { primary: Err_main, suppressed: [C, B, A] }
}
```

**Превосходит Rust** — нет `panic_in_drop = double-panic-abort`.

## Panic в defer body (D161)

Panic в defer body **композируется** с propagating (Plan 49 multi-error),
**не abort**:

```nova
fn process() Fail -> () {
    defer { panic("cleanup broken") }
    do_fails()?                                  // throws Err1
    // Unwinding:
    //   defer fires — panic
    //   panic composes с Err1
    //   composite: { primary: Err1, suppressed: [Panic("...")] }
}
```

## Что НЕ делать

❌ **`spawn` в defer body** — error D159-spawn-in-defer (leak supervised
hierarchy).

❌ **`return` / `throw` / `break` top-level в defer body** (D90 §6
unchanged) — defer is part of exit, не hijack.

❌ **Double-cover** — `okdefer { commit }` + explicit `tx.commit()` →
error D162-double-cover.

❌ **Partial cover** — `errdefer { rollback }` без okdefer/explicit
commit → error D162-not-consumed-on-path (success uncovered).

❌ **Cleanup без `Time.timeout`** для potentially-long cleanups —
infinite-hang risk. Программист обязан bound через `Time.timeout`:

```nova
defer {
    with Time.timeout(5_s) {
        long_cleanup()
    }
}
```

## Связь

- [D90](../../spec/decisions/03-syntax.md#d90) — defer/errdefer
  foundation (Plan 20).
- [D158](../../spec/decisions/03-syntax.md#d158) — failable cleanup
  body.
- [D159](../../spec/decisions/03-syntax.md#d159) — async/suspend.
- [D160](../../spec/decisions/03-syntax.md#d160) — okdefer + reason-
  aware.
- [D161](../../spec/decisions/03-syntax.md#d161) — multi-defer
  accumulation + panic composition.
- [D162](../../spec/decisions/03-syntax.md#d162) — consume-integration.
- [D85](../../spec/decisions/04-effects.md#d85) — Plan 49 cancel-routing
  + multi-error composition.
- [consume-types idiom](consume-types.md) — canonical consume patterns.
- Plan 100.4 family — `100.4-cleanup-on-failure.md` umbrella.
