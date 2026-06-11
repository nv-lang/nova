<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 141 — Structural equality field-by-field (fix memcmp tuple/sum eq)

> **Создан:** 2026-06-11.  **Статус:** 📋 PLANNED.  **Эстимат:** ~0.5-1 dev-day (.rs codegen + tests).
> **Model:** Opus + Thinking (codegen correctness).
> **Триггер:** `[M-codegen-memcmp-equality-float-padding]` — равенство кортежей/sum через
> `memcmp` неверно для float/padding (soundness-баг).

---

## Проблема (recon emit_c.rs)

**1. Tuple-eq (emit_c.rs:16893-16899) = bitwise `memcmp`:**
```rust
BinOp::Eq => Ok(format!("(memcmp(&{}, &{}, sizeof({})) == 0)", l, r, struct_ty)),
```
Неверно для:
- **float** (f64/f32): `memcmp` делает `-0.0 != +0.0` (разные биты, а `==` = равны) и бит-идентичный
  `NaN == NaN` (а IEEE `==` = `NaN != NaN`). Кортеж с float → **неверное равенство**.
- **padded struct**: indeterminate padding-байты при mixed-size полях → два равных кортежа ≠.
- **nested composite**: вложенный record/tuple/sum сравнивается побайтово (pointer-bits / struct-bytes), не структурно.

**2. Sum-eq (emit_c.rs:17069-17109)** — уже **field-by-field** (комментарий :17072 «memcmp» STALE,
код делает per-field cmp), НО dispatch (:17087-17094) знает только:
- `nova_str` → `nova_str_eq` ✅
- else → `field._i == field._i`

`==` верен для скаляров+float, но **неверен** для вложенных композит-полей:
- **record-payload** (`Nova_X*`) → `==` сравнивает **указатели** (identity), не структуру.
- **nested-tuple-payload** (`_NovaTupleN`) → `==` на struct → **C compile error**.
- **nested-sum-payload** (`Nova_Y*`) → pointer `==`, не recursive tag+payload.

---

## Решение: общий `emit_field_eq(c_type, l, r)` helper

Извлечь shared helper, диспетчеризующий равенство **по C-типу**:

| C-тип поля | Равенство |
|---|---|
| скаляр (`nova_int`/`bool`/`byte`/`char`) | `(l == r)` |
| **float** (`nova_f64`/`nova_f32`) | `(l == r)` — IEEE (`-0.0==+0.0`, `NaN!=NaN`) ✅ |
| `nova_str` | `nova_str_eq(l, r)` |
| `_NovaTupleN` (tuple) | **recursive** field-by-field (читать tuple-схему, cmp каждое поле) |
| `Nova_X*` record | structural: `Nova_X_method_equal(l, r)` если есть, иначе recursive field-by-field record-полей — **НЕ** pointer-`==` |
| `Nova_Y*` sum | recursive tag + payload (текущая sum-eq логика) |

**Tuple-eq (16896):** заменить `memcmp` на field-by-field через `emit_field_eq` по каждому tuple-элементу
(читать tuple-схему элементных C-типов).
**Sum-eq (17087-17094):** заменить str-only dispatch на `emit_field_eq` per payload-поле.
**`memcmp` оставить ТОЛЬКО** для `[]u8`/byte-blob (Plan 90 `@compare`, где byte-eq = семантика).

Helper рекурсивен (nested tuple/record/sum) — нужен guard от циклов (record-cycle → out of scope V1,
прямая рекурсия по схеме конечна для не-циклических; document если cycle-risk).

---

## Фазы

### Setup — recon + baseline
Confirm tuple-eq:16896, sum-eq:17069-17109, field-dispatch:17087. Проверить **record-eq путь** (как
`Nova_X*` record сравнивается сейчас — есть auto-derived `@equal` Plan 126? memcmp? pointer-`==`?).
Найти tuple-схему (где хранятся элементные C-типы `_NovaTupleN`) + sum_schemas (есть, :17074).
Baseline eq-heavy dirs: plan126, plan131, plan138_2, plan59 (tuples), plan105 (sum), + общий.

### Ф.1 — emit_field_eq helper + tuple-eq + sum-eq (.rs)
Реализовать helper; переключить tuple-eq (drop memcmp) + sum-eq (extend dispatch). Rebuild.
**Pos/neg fixtures nova_tests/plan141/** (verify РЕЛИЗНЫМ nova):
- `t1_tuple_float_neg_zero`: `(0.0, 1) == (-0.0, 1)` → **true** (memcmp давал false).
- `t2_tuple_nan`: `(f64_nan(), 1) == (f64_nan(), 1)` → **false** (NaN≠NaN; memcmp давал true).
- `t3_tuple_padding`: `(1 as u8, 2) == (1 as u8, 2)` (mixed-size) → **true** независимо от padding.
- `t4_tuple_nested`: `((1,2), 3) == ((1,2), 3)` → true; `((1,2),3) == ((1,9),3)` → false.
- `t5_sum_record_payload`: sum-вариант с record-payload → structural (не pointer-identity).
- `t6_sum_nested_tuple`: sum-вариант с tuple-payload компилируется + structural (был C-error).
- `t7_str_tuple`: `("a", 1) == ("a", 1)` → true (str-eq в tuple).
Broad regression (eq-heavy + общий) — 0 новых FAIL.
**Commit:** `fix(plan141): structural eq field-by-field (tuple + sum); drop memcmp for composites`

### Ф.2 — Spec (Equatable structural eq)
Найти Equatable D-блок (Plan 126). Amend: structural `==` = field-by-field по типу; float = IEEE
(`-0.0`==`+0.0`, `NaN`!=`NaN`); `memcmp` НЕ используется для композитов (только byte-blob `[]u8`);
record-поля structural (`@equal`/recursive), не pointer-identity. Q-блок если нужно (record-cycle eq).
**Commit:** `spec(plan141): Equatable = structural field-by-field; float IEEE; no memcmp on composites`

### Ф.3 — Docs + close
Acceptance audit; project-creation + simplifications + backlog (УБРАТЬ `[M-codegen-memcmp-equality-float-padding]`
из OPEN-view) + nova-private/discussion-log. Memory. Final regression. Commit + (sync с main — по запросу).

---

## Risk register
| # | Риск | Sev | Mitigation |
|---|---|---|---|
| R1 | record-eq уже использует `@equal` (Plan 126) → helper должен переиспользовать, не дублировать | 🟡 MED | Setup recon record-eq путь; helper зовёт существующий `@equal` |
| R2 | recursive helper + record-cycle → бесконечная рекурсия в codegen | 🟡 MED | схема конечна для non-cyclic; cycle → document out-of-scope V1 |
| R3 | mangle/типы tuple-элементов недоступны на eq-site | 🟡 MED | Setup найти tuple-схему (как sum_schemas); если нет — добавить |
| R4 | регрессия по eq-heavy (plan126/59/105/131) от смены tuple/sum eq | 🟡 MED | broad regression Ф.1 |

## Acceptance
- **A1** — float tuple eq по IEEE: `(0.0,_)==(-0.0,_)` true, `(nan,_)==(nan,_)` false.
- **A2** — padded tuple eq корректен (не зависит от padding-байт).
- **A3** — nested composite (tuple/record/sum в tuple или sum-payload) — structural, не pointer/byte.
- **A4** — `memcmp` остался только для `[]u8` byte-blob.
- **A5** — spec Equatable документирует field-by-field + float IEEE + no-memcmp.
- **A6** — 0 регрессий (plan126/59/105/131/138_2 + общий); `[M-codegen-memcmp-equality-float-padding]` закрыт.

## Связь
- Закрывает `[M-codegen-memcmp-equality-float-padding]` (backlog P1 correctness).
- Equatable D-блок (Plan 126) — amend.
