// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100.2 — GATE Probe (3 use-case'а, self-check D156)

> **Назначение:** верификация самосогласованности D156 (`[T consume]` bound +
> collection-aware iteration) через 3 hand-written примера до начала
> имплементации. Каждый пример — pseudo-Nova код с аннотациями того, что
> flow-analysis будет трекать.
>
> **Создан:** 2026-05-26 в ходе Ф.0 Plan 100.2.
> **Ссылка на spec:** [D156 в spec/decisions/02-types.md](../../../spec/decisions/02-types.md#d156)
> **Ссылка на plan:** [100.2-generic-propagation.md](../100.2-generic-propagation.md)

---

## Use-case 1: Generic function с `[T consume]` bound — happy path + error path

**Покрывает:** D156 (strict mode внутри generic body), D156-strict-forget,
D133 backward-compat (generic без bound = silent-ignore).

```nova
// D156 use-case: [T consume] bound активирует strict mode внутри body.
type Transaction consume {
    id int,
}
fn Transaction consume @commit() -> () { () }

// Без bound — silent-ignore (backward-compat, Plan 100.1 behavior).
fn forget_ok[T](x T) -> () {
    // x: T — нет обязательства consume, silence preserves.
}

// С [T consume] bound — strict mode.
fn delegate[T consume](consume x T) -> () {
    // x трактуется как possibly-consume → обязан быть consumed.
    // Передача в consume-param → x Consumed ✅
    finish(x)
}

fn first_err[T consume](pair (T, T)) -> T {
    // pair.0 → возвращается как T (OK),
    // pair.1 → молча теряется → ❌ D156-strict-forget
    return pair.0
}

fn forget_err[T consume](consume x T) -> () {
    // x не consumed → ❌ D156-strict-forget
}
```

**Flow-analysis трекает:**
- `[T consume]` in GenericParam → `consume_bound_generics = {"T"}`.
- Param `consume x T` → `declare_consume_binding("x", Some("T"))`.
- На scope-exit: `x` Live → `is_strict_generic = consume_bound_generics.contains("T")` → код D156-strict-forget.
- Без bound (`[T]`): `consume_bound_generics` пусто → `is_strict_generic = false` → код D133 (или ничего).

---

## Use-case 2: `for consume tx in vec` — consume-iteration mode

**Покрывает:** D156 (collection-aware iteration), D156-iter-not-consumed,
D156-iter-maybe-consumed, branch-join pessimism для loop body.

```nova
fn for_iter_ok() {
    ro txs = [Transaction { id: 1 }, Transaction { id: 2 }]
    for consume tx in txs {
        tx.commit()             // tx → Consumed в каждой итерации ✅
    }
    // После цикла: txs → Consumed (iter-consumed mode)
}

fn for_iter_skip_err() {
    ro txs = [Transaction { id: 1 }, Transaction { id: 2 }]
    for consume tx in txs {
        // tx не consumed → ❌ D156-iter-not-consumed в каждой итерации
    }
}

fn for_iter_branch_err() {
    ro txs = [Transaction { id: 1 }, Transaction { id: 2 }]
    for consume tx in txs {
        if tx.id == 2 {
            tx.commit()         // consumed на одном ветвлении
        }
        // else-arm: tx Live → pessimistic join → MaybeConsumed
        // → ❌ D156-iter-maybe-consumed
    }
}
```

**Flow-analysis трекает:**
- `for consume tx in txs` → `iter_consume = true` in AST.
- `consume_walk_consume_for`: объявляет `tx` как consume-obligation с `None` type.
- Pass-1 (throwaway): выявляет pessimism; Pass-2: реальный analysis.
- На выходе из тела: `tx` Live → D156-iter-not-consumed; MaybeConsumed → D156-iter-maybe-consumed.
- После цикла: iter-var `txs` → Consumed.

---

## Use-case 3: Generic с tuple param + consume-в-поле

**Покрывает:** D156 для complex type exprs (`(T, T)`), multi-arg generic.

```nova
fn consume_pair[T consume](consume a T, consume b T) -> () {
    finish(a)               // a → Consumed ✅
    finish(b)               // b → Consumed ✅
}

fn first_of_pair_err[T consume](pair (T, T)) -> T {
    // pair — not declared consume (plain param), но содержит T consume.
    // pair.0 → возвращается ✅, pair.1 → теряется.
    // ❌ D156-strict-forget на pair (тип "(T,T)" содержит consume-generic).
    return pair.0
}
```

**Flow-analysis трекает:**
- `typeref_contains_consume_generic((T, T), {"T"})` → true.
- `pty = None` (не простой Named), fallback: ищет первый generic из set → `pty = Some("T")`.
- `declare_consume_binding("pair", Some("T"))`.
- На scope-exit: `pair` Live → `is_strict_generic = true` (consume_bound_generics.contains("T")) → D156-strict-forget.

---

## Self-check checklist

- [x] Каждое consume в коде имеет соответствие в D156:
  - Use-case 1: D156 (generic bound), D156-strict-forget, D133 (backward-compat)
  - Use-case 2: D156-iter-not-consumed, D156-iter-maybe-consumed, pass-1/2 pessimism
  - Use-case 3: complex TypeRef containment, tuple param consume-tracking
- [x] Нет contradiction между секциями:
  - D156 применяется только при наличии `[T consume]` bound — без bound silent-ignore
  - iter-consume и function-body consume — разные code paths, одинаковая семантика
  - `typeref_contains_consume_generic` работает одинаково для named/tuple/array types
- [x] Ни одно правило не введено в probe без D-block parent'а:
  - generic bound → D156
  - strict-forget → D156-strict-forget
  - iter-not-consumed → D156-iter-not-consumed
  - iter-maybe-consumed → D156-iter-maybe-consumed
  - backward-compat → D133 (100.1)
