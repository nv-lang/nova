// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100.2 — stdlib migration guide: аннотирование generic-функций `[T consume]`

> **Назначение:** гид по добавлению `[T consume]` bound в существующий
> stdlib generic-код. После 100.2 foundation `[T consume]` доступен — это
> поэтапный migration playbook для stdlib.
>
> **Создан:** 2026-05-26 в ходе Ф.5 Plan 100.2.
> **Ссылка на план:** [100.2-generic-propagation.md](../100.2-generic-propagation.md)
> **Полная stdlib migration:** [Plan 100.7](../100.7-stdlib-migration-playbook.md) (GATED на 100.1-6)

---

## Принцип: opt-in, backward-compat

Default поведение после 100.2 — **silent-ignore** для generic-функций без
явного `[T consume]` bound. Существующий stdlib-код **ничего не ломает**.
Migration — additive: добавляем bound там, где это семантически верно.

Три паттерна generic-функций в stdlib, которые **ДОЛЖНЫ** получить `[T consume]`:

### 1. «Forward through» — пробрасываем T как consume-param

```nova
// ДО (silent-ignore):
fn identity[T](consume x T) -> T => x  // T теряется если не consume, но молча OK

// ПОСЛЕ (strict):
fn identity[T consume](consume x T) -> T => x
// Теперь: T-значение track'ится; return x = consume → ✅
```

**Признак:** функция получает `consume x T` и возвращает `x` или передаёт в consume-param.

### 2. «Destructure and discard» — паттерны где нужно явно не терять

```nova
// ДО (silent-ignore, silent bug!):
fn first[T](pair (T, T)) -> T => pair.0  // pair.1 теряется для T=Transaction

// ПОСЛЕ (strict, выявляет баг):
fn first[T consume](pair (T, T)) -> T => pair.0  // ❌ D156-strict-forget на pair.1
// Правильный вариант:
fn first_and_commit[T consume](pair (T, T), consume second T) -> T where second = pair.1 { ... }
// Или: использовать View-вариант (Plan 100.3).
```

**Признак:** функция получает tuple/record с T-полями и не возвращает все.

### 3. «Collection iteration» — `for consume t in col`

```nova
// ДО (нет consume-iteration):
fn drain[T consume](items []T, handler fn(consume T) -> ()) -> () {
    for consume item in items {
        handler(item)           // item consumed через handler ✅
    }
}
```

**Признак:** функция итерирует по коллекции consume-типов и вызывает callback.

---

## Таблица: stdlib-функции — кандидаты на `[T consume]` аннотацию

| Файл | Функция | Причина | Статус |
|---|---|---|---|
| `std/collections/vec.nv` | `Vec.drain()` | forward each elem | 📋 100.7 |
| `std/collections/vec.nv` | `Vec.map(fn(consume T)->U)` | HOF consume | 📋 100.7 |
| `std/option.nv` | `Option.map(fn(consume T)->U)` | HOF consume | 📋 100.7 |
| `std/result.nv` | `Result.map(fn(consume T)->U)` | HOF consume | 📋 100.7 |
| `std/collections/hashmap.nv` | `HashMap.drain()` | forward each value | 📋 100.7 |
| `std/sync/*.nv` | Atomic exchange/swap | consume-value путь | 📋 103.x |

---

## Что НЕ нужно annotate

- Generic-функции без consume-semantics (`fn len[T](items []T) -> int`).
- Функции, которые НЕ принимают ownership (view-параметры — 100.3).
- Pure utility generics (`fn min[T](a T, b T) -> T` для copy-типов).

---

## Migration-шаги (когда готова 100.7)

1. Grep `fn.*\[T\].*consume` в stdlib — найти функции с `consume` param но без bound.
2. Для каждой: добавить `consume` к generic-param.
3. Запустить `nova test` — `D156-strict-forget` указывает на silent-leak.
4. Fix: либо правильно consume все T-значения, либо убрать bound если silent-ignore intentional.
5. Добавить тест-фикстуру в `nova_tests/plan100_7/` (pos+neg).

---

## Известные решения и trade-offs

**Trade-off 1: backward-compat vs strictness**
- Nova выбирает opt-in (`[T consume]`) vs Rust's default-Move.
- Преимущество: весь существующий stdlib-код продолжает работать.
- Недостаток: silent-leaks остаются в не-аннотированных функциях.
- Mitigation: compiler-warning (future Plan 100.8) для обнаружения.

**Trade-off 2: tuple params vs individual params**
- `fn first[T consume](pair (T, T))` — complex TypeRef, нужен `typeref_contains_consume_generic`.
- Альтернатива: `fn first[T consume](a T, b T)` — проще, но меняет API.
- Nova поддерживает оба паттерна (как показывает `generic_bound_err_first_drop.nv`).

**Trade-off 3: for-consume vs explicit drain**
- `for consume tx in txs { ... }` — синтаксический сахар для drain.
- Без HOF-closure (Plan 100.3 не реализован) — это правильный подход.
- После 100.3 можно добавить `txs.drain(|tx| tx.commit())`.
