---
source_rev: 21dff1b37
source_date: 2026-08-02
---

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Tutorial — освобождение ресурсов через `consume{}` (Plan 110)

[English](tutorial-cleanup.md) | **Русский**

> **Plan 110.8.2.** Глава туториала, знакомящая с scope-блоком
> `consume X = ... { body }` для освобождения ресурсов.

## Почему важно освобождение ресурсов

При работе с ресурсами (файлы, соединения с БД, блокировки) нужно гарантировать,
что они **всегда освобождаются**, даже при ошибках. Забытое освобождение
приводит к:

- **Утечкам ресурсов** — заблокированные файлы, зависшие соединения с БД.
- **Взаимоблокировкам** — Mutex удерживается во время паники, остальные ждут
  вечно.
- **Повреждению данных** — транзакция не откачена, наполовину записанное
  состояние.

Nova предоставляет scope-блок `consume X = expr { body }`, чтобы освобождение
было **автоматическим и надёжным**.

## Первый пример — чтение файла

```nova
fn read_config(path str) Fail[IoError] -> Config {
    consume f = File.open(path)? {
        ro raw = f.read_all()?
        Config.parse(raw)?
    }
    // f.@cleanup (File.close) automatically called here.
}
```

Что происходит:
1. `File.open(path)?` открывает файл. Если это не удалось, `?` пробрасывает
   ошибку — `f` никогда не связан, освобождение не нужно.
2. `f` доступен внутри тела `{ ... }`.
3. После завершения тела (успех ИЛИ ошибка) `f.@cleanup(outcome)` вызывается
   автоматически — закрывает файл.
4. Если тело упало, ошибка пробрасывается дальше ПОСЛЕ освобождения.

## Протокол `Cleanup[E]`

Любой тип можно использовать в `consume X = ... { body }`, реализовав протокол
`Cleanup[E]`:

```nova
type Cleanup[E] protocol {
    @cleanup(outcome ScopeOutcome) Fail[E] -> ()
}

type ScopeOutcome
    | Success
    | Failure(str)
    | Panic(str)
```

- `E` — тип ошибок, которые может бросить сам `@cleanup` (например, `IoError`,
  если close может упасть).
- `Success` — тело завершилось нормально.
- `Failure(msg)` — тело бросило ошибку (включая отмену).
- `Panic(msg)` — тело запаниковало (программный баг).

## Реализация освобождения для своего типа

### Пример: транзакция БД

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

Использование:
```nova
fn process_order(db Db, order Order) Fail[DbError] -> () {
    consume tx = db.begin() {
        db.insert_order(order)?
        db.notify_warehouse(order.id)?
    }
    // Success → commit; failure → rollback (automatic).
}
```

### Пример: безошибочное освобождение (Mutex-лок)

Для ресурсов, где освобождение НЕ МОЖЕТ упасть, используйте `Cleanup[never]`:

```nova
fn MutexGuard consume @cleanup(_outcome ScopeOutcome) -> () => @unlock()
//                                                       ^^^^ no Fail[E]
```

Вызывающему не нужен `Fail[E]`:
```nova
fn increment_counter(state State) -> () {        // no Fail!
    consume _l = state.mutex.lock() {             // Cleanup[never]
        state.value += 1
    }
}
```

## Различение исхода

Тело `@cleanup` может ветвиться по исходу:

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

## Вложенные области

Области вкладываются естественно — внутренняя выходит раньше внешней (LIFO):

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

## Смесь `consume{}` + `defer`

Оба работают вместе. `defer` срабатывает внутри своей области ДО `@cleanup`:

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

## Формы инициализации (D196)

Выражение инициализации поддерживает:

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

Формы, которые не работают:
```nova
// ❌ Option without unwrap
consume tx = maybe_tx() { ... }
// → D196-wrapped-init-needs-unwrap

// ❌ Different types in branches
consume r = if cond { ResA.new() } else { ResB.new() } { ... }
// → D196-divergent-consumable
```

## Сравнение с другими языками

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

Преимущества Nova:
- **Видимость**: освобождение явное, а не магический `Drop`.
- **Cancel-shield**: освобождение защищено от шторма отмен (D188 R3).
- **Осведомлённость об исходе**: ресурс различает success/failure/panic.
- **Async-совместимость**: можно `await` внутри `@cleanup` (D191).

## Что дальше

- Прочитай [Q-cleanup-semantics](../dev/idioms/consume-scope-cleanup.md) для
  деревьев решений.
- Прочитай [Q-consumable-protocol](../dev/idioms/consume-scope-cleanup.md) для
  деталей реализации.
- Прочитай [Q-application-effect](../dev/idioms/application-effect.md) для
  жизненного цикла приложения.
- Прочитай [cleanup-cookbook.md](cleanup-cookbook.md) для производственных
  рецептов.

## См. также

- [D188 — Cleanup + scope-block](../../spec/decisions/03-syntax.md#d188).
- [Plan 110](../plans/110-scoped-resources-radical-simplification.md).
- Все Q-блоки в [docs/dev/idioms/](../dev/idioms/).
