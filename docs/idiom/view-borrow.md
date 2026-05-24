// SPDX-License-Identifier: MIT OR Apache-2.0
# View borrow — read-only access (implicit default)

> Practical guide для [D157](../../spec/decisions/05-memory.md#d157)
> implicit view default model (Plan 100.3, Ред. 2 2026-05-24). Scope-
> only borrow для consume-типов; без Rust lifetime'ов; **без `view T`
> keyword'а** — view это absence of `consume`/`mut` qualifier.

## Mental model — 3 mode'а

Везде где есть «binding source» (param / for / match / if-let / let-
alias) единое правило:

| Form | Mode | Что |
|---|---|---|
| **no qualifier** | view | read-only borrow |
| `mut` | mut-view | + mut-методы |
| `consume` | transfer | ownership |

## Когда «view» (default — без qualifier'а)

✅ **Default view mode когда:**
- Helper-функция читает поля consume-value без consume.
- Pattern-match'ишь `Option[ConsumeType]` без consume (deep peek).
- Closure captures consume-var read-only (multi-invoke OK).
- Alias-binding для inspection (`let alias = tx`).

❌ **НЕ view (использовать `mut` или `consume`):**
- Функция владеет ресурсом → `consume t Transaction` param.
- Метод mutates state → `mut t Transaction` param.
- Identity-функция возвращает то, что приняла → `consume t` + `return t`.

## Базовый pattern

```nova
fn print_id(tx Transaction) -> () {
    println(tx.id)                              // ✅ чтение поля
}

consume tx = begin()
print_id(tx)                               // ✅ view-передача
tx.commit()                                     // ✅ tx Live после
```

## Deep peek в `Option[ConsumeType]` — `match view`

```nova
type Service consume {
    consume file Option[File],
}

fn Service @file_id() -> Option[int] {
    match @file {                          // ← view-match
        Some(f) => Some(f.fd),                  // f: view-bound, read-only (no consume keyword)
        None => None,
    }
    // @.file остаётся Live (не Consumed) ✅
}
```

Без D157 этот pattern невозможен (D133 match destructive).

## Closure capture — view vs consume

**View-closure (FnMut/Fn analog) — multiple invokes:**

```nova
consume tx = begin()
let logger = || println(tx.id)                  // только view-операции в body
logger()                                         // OK
logger()                                         // OK, multi-invoke
tx.commit()                                     // tx Live после
```

**Consume-closure (FnOnce analog) — single invoke:**

```nova
consume tx = begin()
let commit_it = || tx.commit()                  // consume-операция в body
commit_it()                                      // ✅ tx Consumed
commit_it()                                      // ❌ use-after-consume
```

Compiler определяет автоматически по операциям в body.

## Правила доступа — 3 mode'а

| Действие | view (no qualifier) | `mut t` | `consume t` |
|---|---|---|---|
| `t.field` (read) | ✅ | ✅ | ✅ |
| `t.regular_method()` | ✅ | ✅ | ✅ |
| `t.mut_method()` | ❌ E (D133-mut-via-view) | ✅ | ✅ |
| `t.consume_method()` | ❌ E (D133-consume-via-view) | ❌ | ✅ |
| передача в view-param | ✅ | ✅ | ✅ |
| передача в `consume`-param | ❌ | ❌ | ✅ |
| store в поле | ❌ E (D133-view-escape-store) | ❌ | ✅ |
| return (escape) | ❌ E (D133-view-escape-return) | ❌ | ✅ |

## Что нельзя делать

❌ **Stop using `view T` keyword** — keyword отвергнут в Ред. 2.
Default `tx Transaction` (без qualifier) уже view:
```nova
fn print_id(tx Transaction) { ... }             // ✅ view (default)
fn print_id(tx view Transaction) { ... }        // ❌ no such keyword
```

❌ **Return value из view-param-функции наружу:**
```nova
fn id_view(t Transaction) -> Transaction {
    return t                                    // ❌ view не escape;
                                                //    нужно (consume t Transaction)
}
```

❌ **Store consume-value через view binding:**
```nova
fn store(t Transaction, cache mut Cache) {
    cache.last = t                              // ❌ view не storable
}
```

❌ **Mutable view alias-binding в bootstrap** — `let mut alias = tx`
работает (D7), но scope-only — не может escape.

## Связь

- [D157](../../spec/decisions/05-memory.md#d157) — implicit view default + closure capture + match consume.
- [D133](../../spec/decisions/02-types.md#d133) — type-level consume foundation.
- [D131](../../spec/decisions/05-memory.md#d131) — affine consume D7
  read-only mode.
- [D75](../../spec/decisions/06-concurrency.md#d75) — почему borrow-
  checker отвергнут.
- [consume-types idiom](consume-types.md) — canonical patterns.
- Plan 100.3 — [100.3-borrow-and-view.md](../plans/100.3-borrow-and-view.md).
