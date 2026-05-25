// SPDX-License-Identifier: MIT OR Apache-2.0
# Idiom: Generic consume bound `[T consume]`

> **Plan 100.2 — D156.** Когда использовать, как писать, что проверяется.

---

## Ситуация

Вы пишете generic-функцию, которая должна работать с consume-типами
(Transaction, File, Lock). Без `[T consume]` bound — Nova не проверяет,
что T-значения consumed: если T = Transaction и вы забыли `.commit()` —
**ошибки нет** (backward-compat, silent-ignore по умолчанию).

С `[T consume]` — Nova проверяет строго: любое T-значение должно быть
либо consumed, либо returned (transferred), либо передано в consume-param.

---

## Базовый паттерн

```nova
type Transaction consume {
    id int,
}
fn Transaction consume @commit() -> () { () }

// External concrete consume-param (например, из API).
fn finish(consume t Transaction) -> () {
    t.commit()
}

// [T consume] bound: внутри тела T трактуется как possibly-consume.
fn delegate[T consume](consume x T) -> () {
    finish(x)           // x consumed via consume-param ✅
}

test "delegate commits transaction" {
    consume tx = Transaction { id: 1 }
    delegate(tx)        // tx consumed at call ✅
}
```

---

## Возврат T как consume-transfer

```nova
// Вернуть T — это consume-transfer (obligation передаётся наверх).
fn wrap_and_get[T consume](consume x T) -> T {
    x                   // return = consume-transfer ✅
}

test "wrap_and_get delegates ownership" {
    consume tx = Transaction { id: 2 }
    consume tx2 = wrap_and_get(tx)  // обязательство перешло в tx2
    tx2.commit()                    // ✅
}
```

---

## Multi-param consume

```nova
fn commit_both[T consume](consume a T, consume b T) -> () {
    finish(a)           // a consumed ✅
    finish(b)           // b consumed ✅
}
```

---

## `for consume tx in vec` — consume-iteration

Когда есть коллекция consume-типов — `for consume` итерирует с ownership:

```nova
fn commit_all() {
    let txs = [Transaction { id: 1 }, Transaction { id: 2 }, Transaction { id: 3 }]
    for consume tx in txs {
        tx.commit()     // каждый tx consumed в своей итерации ✅
    }
    // После цикла txs → Consumed
}
```

**Важно:** каждая итерация независима. Branch-анализ применяется per-iter:

```nova
fn commit_with_break() {
    let txs = [Transaction { id: 1 }, Transaction { id: 2 }]
    for consume tx in txs {
        if tx.id == 1 {
            tx.commit()
            break       // остальные итерации не произойдут — компилятор это знает
        } else {
            tx.commit() // каждая итерация consumed ✅
        }
    }
}
```

---

## Ошибки компилятора

### D156-strict-forget — переменная T не consumed

```nova
fn bad_forget[T consume](consume x T) -> () {
    // x не consumed
    // ❌ [D156-strict-forget] переменная `x` (тип `T`) не consumed до scope-exit
}
```

### D156-strict-forget — часть tuple потеряна

```nova
fn bad_first[T consume](pair (T, T)) -> T {
    return pair.0
    // ❌ [D156-strict-forget] переменная `pair` (тип `T`) не consumed до scope-exit
    //    (pair.1 потерян — вот в чём суть этой ошибки)
}
```

### D156-iter-not-consumed — тело цикла не consumed

```nova
fn bad_loop() {
    let txs = [Transaction { id: 1 }]
    for consume tx in txs {
        let _ = tx.id   // ❌ [D156-iter-not-consumed] tx не consumed в итерации
    }
}
```

### D156-iter-maybe-consumed — ветвление без else

```nova
fn bad_branch_loop() {
    let txs = [Transaction { id: 1 }, Transaction { id: 2 }]
    for consume tx in txs {
        if tx.id == 1 {
            tx.commit()
        }
        // нет else → ❌ [D156-iter-maybe-consumed] tx consumed только на части путей
    }
}
```

### D133-move-to-non-consume — передача в non-consume param

```nova
fn log_tx(t Transaction) -> () { () }   // non-consume param

fn bad_pass[T consume](consume x T) -> () {
    log_tx(x)   // ❌ [D133-not-consumed] x не consumed (log_tx не consume-param)
}
```

---

## Без bound — backward-compat (silent-ignore)

```nova
// БЕЗ [T consume]: T не трактуется как consume.
fn silent_forget[T](x T) -> () {
    // x не consumed — ни ошибки нет (backward-compat)
}

// С [T consume]: x должен быть consumed.
fn strict_forget[T consume](consume x T) -> () {
    // x не consumed → ❌ D156-strict-forget
}
```

---

## Сравнение с Rust

| Концепция | Rust | Nova |
|---|---|---|
| Generic linear bound | `T: Drop` (все типы), `T: Copy` (copy-exempt) | `[T consume]` — opt-in strict |
| Default для generics | Move semantics (всегда) | Silent-ignore (backward-compat) |
| Strict-forget detection | ❌ `mem::forget` escape hatch | ✅ compile-error D156 |
| Iter ownership | `for x in vec` — consume по умолчанию | `for consume x in vec` — explicit |

Nova: **opt-in strict** vs Rust's **default-strict**. Преимущество — backward-compat
с существующим кодом. Недостаток — silent-leaks в не-аннотированных функциях.

---

## Когда использовать `[T consume]`

**✅ Добавьте bound, если:**
- Функция получает `consume x T` и должна гарантировать consume.
- Функция итерирует коллекцию consume-типов (`for consume`).
- Функция — HOF, принимающий `fn(consume T) -> U` callback.
- Вы хотите выявить silent-leaks на этапе компиляции.

**❌ НЕ добавляйте bound, если:**
- Функция работает только с copy/value типами (int, str).
- Функция viewing (не owning) — используйте view T (Plan 100.3).
- Backward-compat: существующий код без bound работает, не трогайте.
