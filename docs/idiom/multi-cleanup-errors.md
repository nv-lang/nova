// SPDX-License-Identifier: MIT OR Apache-2.0
# Multi-cleanup errors — MultiError chain inspection

> Practical guide для [D161](../../spec/decisions/03-syntax.md#d161)
> + [D158](../../spec/decisions/03-syntax.md#d158) (Plan 100.4.4 +
> 100.4.1). Когда несколько cleanup'ов могут fail одновременно.

## Сценарий

Production resource с несколькими live consume-vars; body fails;
LIFO unwinding запускает каждый cleanup → каждый может fail:

```nova
fn process() Fail -> () {
    consume tx1 = begin()
    consume tx2 = begin()
    defer { tx2.commit() }                      // (D158 failable)
    defer { tx1.commit() }
    do_work()?                                   // throws Err_main

    // LIFO unwinding (D161):
    //   1. body throws Err_main
    //   2. tx1.commit() fails Err1 → suppressed [Err1]
    //   3. tx2.commit() fails Err2 → suppressed [Err1, Err2]
    //   composite: { primary: Err_main, suppressed: [Err1, Err2] }
}
```

**Все N cleanups attempted** — Rust does NOT do this (panic-in-drop =
abort). **Превосходит Rust** на этой оси.

## MultiError API

`std/prelude/runtime.nv` (или `std/error/`) экспортирует:

```nova
type MultiError {
    primary: Err,
    suppressed: []Err,                          // LIFO order: first to fail = first
}

fn MultiError @primary() -> Err
fn MultiError @suppressed() -> []Err
fn MultiError @fmt_chain() -> str               // readable representation
fn MultiError @has_panics() -> bool             // convenience predicate
```

## Caller inspection

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

Или через `fmt_chain()`:

```nova
match process() {
    Ok(_) => println("done"),
    Err(e) => Log.error(e.fmt_chain()),
}
```

## Panic в defer body — тоже composes (D161)

```nova
fn process() Fail -> () {
    defer { panic("cleanup broken") }
    throw "body failed"
    // composite: { primary: BodyErr, suppressed: [Panic("...")] }
    // Никаких abort'ов — Plan 49 composition handles.
}
```

`MultiError.has_panics()` — predicate для distinguishing panic vs
regular fail в chain.

## Diagnostic format

Compiler/runtime выводит chain читабельно:

```
error: composite error during scope exit
  primary error:
    Err_main ("operation failed")
        at do_work (process.nv:12)
  suppressed during defer LIFO:
    [1] Err1 ("commit tx1 failed")
        at tx1.commit() in defer (process.nv:8)
    [2] Err2 ("commit tx2 failed")
        at tx2.commit() in defer (process.nv:7)
```

LIFO order shows failures в порядке firing (first-fired-first-listed).

## Selective handling

Caller может handle specific suppressed errors:

```nova
match process() {
    Ok(_) => println("done"),
    Err(e) => {
        ro has_network = e.suppressed().any(|s| s is NetworkErr)
        if has_network {
            retry_with_backoff()
        } else {
            propagate()
        }
    }
}
```

## Best practices

✅ **Bounded cleanup через `Time.timeout`** — иначе infinite-hang
маскирует remaining cleanups:

```nova
defer {
    with Time.timeout(5_s) {                    // bound cleanup time
        long_cleanup()
    }
}
```

✅ **Каждый defer — atomic action** — не запускай в defer операции
которые сами требуют cleanup:

```nova
defer {
    consume x = begin()                         // ❌ nested consume in defer = D133-not-consumed
    x.commit()                                   //    on defer body exit
}
```

✅ **Log composite chain at top-level** — даже если caller handle'ит
primary, suppressed может содержать полезную информацию:

```nova
fn main() -> () {
    match process() {
        Ok(_) => println("ok"),
        Err(e) => Log.error(e.fmt_chain())      // log полный chain
    }
}
```

## Сравнение

| Capability | Rust | TS | Java | Nova D161 |
|---|---|---|---|---|
| Multi-cleanup LIFO continues | ❌ first-panic = abort | ✅ SuppressedError | ✅ addSuppressed | ✅ **all attempted** |
| Panic в cleanup composes | ❌ double-panic-abort | ⚠️ SuppressedError | ⚠️ try-catch | ✅ **MultiError + no abort** |
| All N cleanups attempted | ❌ | ⚠️ depends | ✅ try-with-resources | ✅ **guaranteed** |
| Chain inspection API | ⚠️ source() chain | ✅ cause chain | ✅ getSuppressed | ✅ **MultiError API** |

Nova **превосходит Rust** уверенно (no abort + all attempted).
Matches Java/TS на composition. **Превосходит на visibility** через
effect-typed `Fail[E]`.

## Связь

- [D161](../../spec/decisions/03-syntax.md#d161) — multi-defer LIFO +
  panic composition.
- [D158](../../spec/decisions/03-syntax.md#d158) — failable cleanup
  foundation.
- [D85](../../spec/decisions/04-effects.md#d85),
  [D118](../../spec/decisions/04-effects.md#d118) — Plan 49 multi-error
  composition.
- [cleanup-on-failure idiom](cleanup-on-failure.md) — broader patterns.
- Plan 100.4.4 — `100.4.4-multi-defer-error-accumulation.md`.
