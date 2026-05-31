// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100.1 — GATE Probe (4 use-case'а, self-check D1-D10)

> **Назначение:** верификация самосогласованности D1-D10 + D5.1 + D5.2
> через 4 hand-written примера до начала имплементации. Каждый пример —
> pseudo-Nova код с аннотациями того, что flow-analysis будет трекать.
>
> **Создан:** 2026-05-25 в ходе Ф.0 Plan 100.1.
> **Ссылка на spec:** [D133 в spec/decisions/02-types.md](../../../spec/decisions/02-types.md#d133)
> **Ссылка на plan:** [100.1-core-must-consume.md](../100.1-core-must-consume.md)

---

## Use-case 1: Mock Transaction — commit/rollback (D1, D2, D3, D9)

**Покрывает:** D1 (type-level marker), D2 (consume actions table),
D3 (scope-exit check), D9 (binding keyword).

```nova
// D1: type Transaction помечен consume → все instances обязаны быть consumed.
type Transaction consume {
    id int,
    writes []Write,
}

// Consume-методы: каждый "уничтожает" transaction.
fn Transaction consume @commit() -> Result[(), DbErr] { ... }
fn Transaction consume @rollback() -> () { ... }

fn happy_path() -> Result[(), DbErr] {
    // D9: `consume` keyword обязателен для ownership binding.
    consume tx = begin()            // tx: Live (Transaction)
    tx.commit()?                    // D2 №1: tx → Consumed (consume-method)
    // Scope-exit: tx Consumed ✅ (D3 — нет Live consume-vars)
    return Ok(())
}

fn rollback_on_err(fail bool) {
    consume tx = begin()            // tx: Live
    if fail {
        tx.rollback()               // D2 №1: tx → Consumed (consume-method)
    } else {
        tx.commit()!!               // D2 №1: tx → Consumed
    }
    // D3 + D9 branch-join: оба ветки → Consumed ✅
}

fn forget_err() {
    consume tx = begin()            // tx: Live
    // (нет consume)
    // D3 scope-exit: tx Live → ❌ error D133-not-consumed
}

fn let_without_consume_err() {
    ro tx = begin()                // ❌ error D133-consume-needs-keyword (D9):
                                    //    consume-тип в binding без `consume` keyword.
}
```

**Flow-analysis трекает:**
- Объявление `consume tx = begin()` → `tx` в `ConsumeCtx` как `VarState::Live`.
- После `tx.commit()` → `tx` переходит в `VarState::Consumed`.
- На scope-exit: все `Live` consume-vars → emit `D133-not-consumed`.
- `let tx = begin()` без keyword → emit `D133-consume-needs-keyword` при парсинге/type-check.

---

## Use-case 2: Service с consume File + reopen (D5, D5.1)

**Покрывает:** D5 (consume-field в методах record'а), D5.1 (assign в
Live consume-field), D4 (double-marker consistency).

```nova
// D4: type Service consume + field consume — оба маркера обязательны.
type Service consume {
    consume file File,           // D4: field-level marker → field-marker-missing без `consume`
    name str,
}

// D5: consume-метод — record self-destructs, @file должен быть Consumed на exit.
fn Service consume @close() {
    @file.close()                // D2: @file → Consumed
    // Exit: @file Consumed ✅ (consume-метод — ожидаем Consumed)
}

// D5: mut-метод — invariant preserved, @file должен быть Live на exit.
fn Service mut @reopen() -> Result[(), OpenErr] {
    ro new_file = File.open()?  // новый файл
    @file.close()                // D2: @file → Consumed
    @file = new_file             // D5.1: assign OK (предшествующий Consumed)
    // Exit: @file Live ✅ (mut-метод — ожидаем Live)
    return Ok(())
}

// D5.1 violation: assign в Live field без consume.
fn Service mut @overwrite_wrong() {
    @file = File.open()?!!       // ❌ D133-assign-live-field:
                                  //    @file Live, перезапись без close → утечка.
}

// Regular method (без mut/consume): @file должна оставаться Live.
fn Service @get_name() -> str {
    return @name                 // @file не трогаем → Live ✅
}
```

**Flow-analysis трекает:**
- Инициализация конструкта `ConsumeCtx` для метода с `@file: Live` (от field-entry init).
- `@file.close()` → field-path `["@", "file"]` переходит в `Consumed`.
- Assign `@file = new_file` → field-path снова `Live` (rebind).
- Exit-check: consume-метод требует все consume-поля Consumed; mut/regular — Live.
- `@file = File.open()` на Live поле → emit `D133-assign-live-field`.

---

## Use-case 3: Inner→Outer nested (D5.2)

**Покрывает:** D5.2 (nested field paths, arbitrary depth), D4
(transitivity через вложенные типы).

```nova
type Inner consume {
    consume tx Transaction,
}

type Outer consume {
    consume inner Inner,
}

// Правильный паттерн: consume deep path + rebind.
fn Outer mut @commit_inner() {
    @inner.tx.commit()           // D2: deep path @.inner.tx → Consumed
    @inner = Inner.new()         // D5.1: @inner.tx Consumed → rebind @inner OK
                                 // Rebind Inner с fresh tx → @.inner.tx снова Live.
    // Exit: @inner Live, @inner.tx Live ✅ (mut-метод)
}

// D5.2 violation: consume deep path без restore.
fn Outer mut @broken_nested() {
    @inner.tx.commit()           // @.inner.tx → Consumed
    // exit без rebind @inner
    // ❌ D133-nested-broken: @.inner Live (Outer.inner не touched),
    //    но deeper path @.inner.tx Consumed — invariant broken.
}

// Полный consume на Outer уровне (consume-метод).
fn Outer consume @destroy() {
    @inner.tx.commit()           // @.inner.tx → Consumed
    // Exit: @inner.tx Consumed ✅ (consume-метод; весь record уничтожается)
    // @inner само по себе не трекается как отдельный Live path после tx consume-а
    // в consume-методе (record self-destructs).
}
```

**Flow-analysis трекает:**
- `ConsumeCtx` хранит FieldPath `["@", "inner", "tx"]` для nested field.
- `@inner.tx.commit()` → `try_extract_field_path(receiver)` возвращает `["@", "inner", "tx"]` → Consumed.
- Exit-check в mut-методе: если путь `["@", "inner"]` Live, то все deeper пути должны быть Live.
- D5.2 invariant: Live outer-field + Consumed deeper-field → emit `D133-nested-broken`.

---

## Use-case 4: Option-field peek — generic transitivity (D6)

**Покрывает:** D6 (generic-заразность `type_is_consume`), связка с
100.3 D4 (match view — будущее).

```nova
// D6: Option[Transaction] автоматически consume (generic wrap).
type TaskOpt consume {
    consume tx_opt Option[Transaction],   // Option[Transaction] → type_is_consume = true
    id int,
}

// Consume-метод: достать tx из Option и commit.
fn TaskOpt consume @run() -> Result[(), DbErr] {
    match @tx_opt {
        Some(consume tx) => {
            tx.commit()?             // D2: tx → Consumed
            return Ok(())
        }
        None => {
            return Err(DbErr.NoTx)
        }
    }
    // Exit: @tx_opt Consumed (деструктирован через match) ✅
}

// View-only метод: не consume, только читаем id.
fn TaskOpt @peek_id() -> int {
    // ↓ В 100.1 (foundation): match на consume-type = destructive.
    // ↓ В 100.3: будет добавлен `match view` для peek без consume.
    // Пока — компилятор должен reject попытку match без consume.
    return @id                   // @tx_opt не трогаем → Live ✅
}

// D6 error: non-consume binding для Option[Transaction].
fn bad_binding() {
    ro wrapped = get_task()     // ❌ D133-consume-needs-keyword:
                                  //    Option[Transaction] через type_is_consume — consume.
    consume wrapped = get_task() // ✅
}
```

**Flow-analysis трекает:**
- `type_is_consume(Option[Transaction])` → рекурсивно проверяет args → `Transaction` is consume → true.
- `consume tx_opt Option[Transaction]` field → конструируется FieldPath `["@", "tx_opt"]`.
- `match @tx_opt { Some(consume tx) => ... }`: scrutinee path → Consumed, arm-binding `tx` → Live.
- Arm-body с `tx.commit()` → `tx` → Consumed.
- Все match-arms должны consume `tx` для Consumed на exit.
- **100.3 cross-ref D4:** `match view` (peek-mode) — будущее расширение, не в 100.1.

---

## Self-check checklist

- [x] Каждое consume в коде имеет соответствие в D1-D10:
  - Use-case 1: D1 (type marker), D2 (commit/rollback), D3 (exit check), D9 (keyword)
  - Use-case 2: D4 (double-marker), D5 (method kinds), D5.1 (assign-live)
  - Use-case 3: D5.2 (nested invariant)
  - Use-case 4: D6 (generic wrap transitivity)
- [x] Нет contradiction между секциями:
  - D2 table применяется консистентно во всех use-case'ах
  - D5.1 assign-rule одинакова для полей и локальных vars
  - FieldPath semantics одна и та же в D5/D5.2
- [x] Ни одно правило не введено в probe без D-block parent'а:
  - `type_is_consume` → D6
  - field-entry init → D5
  - binding keyword → D9
  - exit-point check → D3
  - nested invariant → D5.2
  - assign-live check → D5.1
