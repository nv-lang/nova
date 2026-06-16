<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 153 — Compiler Bug Fixes (Phase B blockers)

> **Создан:** 2026-06-16. **Статус:** ✅ ЗАКРЫТ 2026-06-16 (коммиты `d505c0e5` + `542a3db8` + тесты)
> **Цель:** устранить 3 compiler-блокера, которые держали Phase B итераторы в GATED-состоянии.

---

## Критерии приёмки (все выполнены)

1. `v.lazy().zip(other).collect()` компилируется и возвращает правильные пары — **без упрощений как для прода**.
2. `v.lazy().flat_map(|x| ...).collect()` компилируется с `Option[BoxIter[U]]` внутри.
3. `v.lazy().step_by(0)` — runtime panic через `nova_contract_violation` (EXPECT_RUNTIME_PANIC).
4. 0 новых регрессий по blast-radius (plan153_2/plan153_0/basics/generics/plan131).
5. Спека D260 обновлена: Phase B GATED → ✅ ЗАКРЫТА.
6. **«Без упрощений как для прода»** — фиксы хирургические, не эвристики; тесты покрывают edge-case (пустой zip, разные длины, пустой flat_map результат, next() после None).

---

## Блокеры и фиксы

### Bug 1 — `[M-153.2-flat-map-inner-option]` ✅ FIXED

**Симптом:** `flat_map_basic.nv` → CC-FAIL "field has incomplete type NovaValue_BoxIter____nova_int".

**Root cause:** `register_novaopt_decl` для NovaOpt с payload типа NovaValue_ (by-value value-record)
эмитировал typedef до того как generic struct body был определён в C файле.
Порядок: `/*__EARLY_TYPEDEFS__*/` → `NovaOpt_BoxIter typedef` → `/*__GENERIC_TYPE_DEFS__*/` →
`NovaValue_BoxIter____nova_int struct`. Typedef reference перед struct body → CC-FAIL.

**Фикс (`compiler-codegen/src/codegen/emit_c.rs`):**
- Новое поле `novaopt_vr_typedefs_buf: RefCell<String>` в CGen структуре.
- Новый splice-маркер `/*__NOVAOPT_VR_TYPEDEFS__*/` размещён **ПОСЛЕ** `/*__GENERIC_TYPE_DEFS__*/`.
- В `register_novaopt_decl` и `register_novaopt_decl_forced`: если `c_ty.starts_with("NovaValue_")`,
  typedef роутится в `novaopt_vr_typedefs_buf` (сплайсируется после struct bodies).
- Тест: `plan153_2/flat_map_basic` — 7 pos + `plan153_2/flat_map_neg` — 4 neg.

---

### Bug 2 — `[M-153.2-tuple-elem-adapter]` ✅ FIXED (zip variant)

**Симптом:** `zip_min.nv` → CC-FAIL "incompatible types: NovaValue_BoxIter_____NovaTuple2 vs
NovaValue_BoxIter_____NovaTuple_2_8_nova_int_8_nova_int".

**Root cause:** Метод `fn BoxIter[A] @zip[B] -> BoxIter[(A,B)]` использует локальный typevar alias
`A` для receiver (не совпадает с `T` из `tmpl.generics`). Dispatch строил
`type_subst = {T: nova_int, B: nova_int}` — `A` отсутствовал.
В `register_mono_method_instance`: `type_ref_to_c(Tuple([A, B]))` не находил `A` в
`current_type_subst` → Tuple arm fallback → `_NovaTuple2` (legacy) вместо
`_NovaTuple_2_8_nova_int_8_nova_int` (mono) → CC-FAIL type mismatch.

**Фикс (`compiler-codegen/src/codegen/emit_c.rs`, ~строка 25132):**
В generic method dispatch, после построения `type_subst` из `tmpl.generics`, добавлена
`else`-ветка к блоку nested-receiver: для flat receivers с `receiver_ty` structurally биндим
локальные typevars через `collect_receiver_typevars` + `infer_type_param_binding`,
добавляя НОВЫЕ имена (`A`) не перезаписывая существующие (`T`).

- Тест: `plan153_2/zip_basic` — 9 pos + `plan153_2/zip_neg` — 3 neg + `plan153_2/zip_min`.

---

### Bug 3 — `[requires-CC-FAIL]` — НЕ ВОСПРОИЗВОДИТСЯ

**Оригинальный симптом:** метод с `requires` возвращающий value-record генерировал `return 0` →
CC-FAIL. Воркэраунд: тест через `test {}` блок.

**Исследование (2026-06-16):** `nova_contract_violation` вызывает `abort()` — `return 0` в contract
guard не присутствует в текущем компиляторе. CC-FAIL строк ~1838 в probe-файлах — это **pre-existing
баг** в `Nova_Set_method_insert__void_p_void_p` (forward decl отсутствует, возвращает int по
умолчанию), не связанный с `requires` + value-record.

**Заключение:** Bug 3 как описан (`return 0`) не воспроизводится. `step_by_zero_neg.nv` в форме
`test {}` корректен — runtime panic через `nova_contract_violation` PASS.

---

## Deliverables

| Deliverable | Статус |
|---|---|
| Диагностика root-cause Bug 1 (flat_map VR typedef ordering) | ✅ |
| Диагностика root-cause Bug 2 (zip receiver typevar alias) | ✅ |
| Исследование Bug 3 (requires + value-record) | ✅ не воспроизводится |
| Фикс Bug 1 в emit_c.rs | ✅ `d505c0e5` |
| Фикс Bug 2 в emit_c.rs | ✅ `d505c0e5` |
| Rebuild release nova | ✅ |
| Тесты zip pos (zip_basic: 9 cases, zip_min: 1) | ✅ |
| Тесты zip neg (zip_neg: 3 cases) | ✅ |
| Тесты flat_map pos (flat_map_basic: 7 cases) | ✅ |
| Тесты flat_map neg (flat_map_neg: 4 cases) | ✅ |
| step_by_zero_neg — runtime panic PASS | ✅ |
| Регрессия plan153_2 + plan153_0 + basics + generics | ✅ 0 новых FAIL |
| D260 amend (Phase B GATED → ✅ + критерии приёмки) | ✅ |
| backlog-followups.md обновлён | ✅ `542a3db8` |
| simplifications.md обновлён | ✅ `542a3db8` |
| project-creation.txt обновлён | ✅ `542a3db8` |
| nova-private/discussion-log.md обновлён | ✅ |

---

## Тест-матрица

| Тест | Вид | Что проверяет | Статус |
|---|---|---|---|
| `zip_basic` — "zip two equal-length vecs" | pos | collect() пар при одинаковой длине | ✅ PASS |
| `zip_basic` — "zip truncates to shorter — left longer" | pos | усечение по короткому (слева длиннее) | ✅ PASS |
| `zip_basic` — "zip truncates to shorter — right longer" | pos | усечение по короткому (справа длиннее) | ✅ PASS |
| `zip_basic` — "zip with empty left" | pos/edge | пустой левый → пустой результат | ✅ PASS |
| `zip_basic` — "zip with empty right" | pos/edge | пустой правый → пустой результат | ✅ PASS |
| `zip_basic` — "zip both empty" | pos/edge | оба пустых → пустой результат | ✅ PASS |
| `zip_basic` — "zip count" | pos | count() пар | ✅ PASS |
| `zip_basic` — "zip then map sum of pair" | pos/chain | zip → map(|p| p.0+p.1) цепочка | ✅ PASS |
| `zip_basic` — "zip single-element" | pos/edge | ровно 1 пара | ✅ PASS |
| `zip_min` — "zip count" | pos | минимальный smoke | ✅ PASS |
| `zip_neg` — "zip stops at shorter left" | neg | next() после исчерпания → None (и повторно) | ✅ PASS |
| `zip_neg` — "zip stops at shorter right" | neg | аналогично | ✅ PASS |
| `zip_neg` — "zip empty — first next is None" | neg | next() на пустом → None сразу | ✅ PASS |
| `flat_map_basic` — "flat_map basic next" | pos | первый next() после flat_map | ✅ PASS |
| `flat_map_basic` — "flat_map collect all" | pos | collect() полного потока | ✅ PASS |
| `flat_map_basic` — "flat_map empty outer" | pos/edge | пустой ввод → пустой collect | ✅ PASS |
| `flat_map_basic` — "flat_map empty inner" | pos/edge | все inner пусты → пустой collect | ✅ PASS |
| `flat_map_basic` — "flat_map mixed empty and non-empty inner" | pos/edge | пропуск пустых inner | ✅ PASS |
| `flat_map_basic` — "flat_map count" | pos | count() результата flat_map | ✅ PASS |
| `flat_map_basic` — "flat_map single element outer" | pos/edge | ровно 1 внешний элемент | ✅ PASS |
| `flat_map_neg` — "flat_map empty outer — first next is None" | neg | next() пустого → None | ✅ PASS |
| `flat_map_neg` — "flat_map all-empty inners — next is None" | neg | все inner пусты | ✅ PASS |
| `flat_map_neg` — "flat_map exhaust — next after last element is None" | neg | next() после исчерпания | ✅ PASS |
| `flat_map_neg` — "flat_map skips empty then yields from non-empty" | neg | [empty, non-empty, empty] | ✅ PASS |
| `step_by_zero_neg` — "step_by zero panics" | neg/panic | requires contract violation | ✅ PASS |
