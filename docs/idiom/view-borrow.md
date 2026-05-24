// SPDX-License-Identifier: MIT OR Apache-2.0
# `view T` — read-only borrow без lifetime

> Practical guide для [D157](../../spec/decisions/05-memory.md#d157)
> view-borrow механизма (Plan 100.3). Scope-only borrow для consume-
> типов; без Rust lifetime'ов.

## Когда писать `view T`

✅ **Использовать `view T` когда:**
- Hellper-функция читает поля consume-value без consume.
- Pattern-match'ишь `Option[ConsumeType]` без consume (deep peek).
- Closure captures consume-var read-only (multi-invoke OK).

❌ **НЕ использовать `view T` когда:**
- Функция владеет ресурсом → `consume t` param.
- Метод mutates state → `mut @method` на consume-record.
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
        Some(f) => Some(f.fd),                  // f: view File — read-only
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

## Правила доступа через view

| Действие | `view T` | `consume t T` |
|---|---|---|
| `t.field` (read) | ✅ | ✅ |
| `t.regular_method()` | ✅ | ✅ |
| `t.@mut_method()` | ❌ E (D157-mut-via-view) | ✅ |
| `t.@consume_method()` | ❌ E (D157-consume-via-view) | ✅ |
| передача в `view`-param | ✅ | ✅ |
| передача в `consume`-param | ❌ | ✅ (consume) |
| store в поле | ❌ E (D157-view-escape-store) | ✅ |
| return | ❌ E (D157-view-escape-return) | ✅ |

## Что нельзя делать

❌ **Bind view к локальной переменной** (bootstrap-ограничение):
```nova
let v = view tx                                 // ❌ scope-check сложно;
                                                //    bootstrap не поддержано
```

✅ **OK — view только в expression-position:**
```nova
print_id(tx)                               // ✅ в call expression
match @file { ... }                        // ✅ в match expression
```

❌ **Return view наружу:**
```nova
fn id_view(t Transaction) -> Transaction {
    return t                                    // ❌ escape — error D157
}
```

❌ **Store view в record:**
```nova
record Cache { v view Transaction }             // ❌ view не storable
```

❌ **Mutable borrow (`mut view T`)** — не вводится; mut-методы D131 +
field-aware flow D133 D5 покрывают.

## Связь

- [D157](../../spec/decisions/05-memory.md#d157) — `view T` spec.
- [D133](../../spec/decisions/02-types.md#d133) — type-level consume foundation.
- [D131](../../spec/decisions/05-memory.md#d131) — affine consume D7
  read-only mode.
- [D75](../../spec/decisions/06-concurrency.md#d75) — почему borrow-
  checker отвергнут.
- [consume-types idiom](consume-types.md) — canonical patterns.
- Plan 100.3 — [100.3-borrow-and-view.md](../plans/100.3-borrow-and-view.md).
