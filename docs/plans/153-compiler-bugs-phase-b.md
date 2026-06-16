<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 153 — Compiler Bug Fixes (Phase B blockers)

> **Создан:** 2026-06-16. **Статус:** 🟡 IN PROGRESS (workflow запущен)
> **Цель:** устранить 3 compiler-блокера, которые держат Phase B итераторы в GATED-состоянии.
> **Ветка/папка:** `main`, `D:\Sources\nv-lang\nova`

---

## Блокеры

### Bug 1 — `[M-153.2-tuple-elem-adapter]` zip closure-typing
**Симптом:** `v.lazy().zip(other).collect()` не компилируется. Компилятор не выводит
тип `Option[(A,B)]` внутри step-замыкания `BoxIter[(A,B)]`.  
**Где:** `compiler-codegen/src/codegen/emit_c.rs` — inference return-типа closure
когда T — tuple.  
**Статус:** 🟡 GATED → исправляется workflow.

### Bug 2 — `[M-153.2-flat-map-inner-option]` mut-match Option[BoxIter[U]]
**Симптом:** `flat_map` делает `mut match` на `Option[BoxIter[U]]` внутри замыкания.
Компилятор не справляется с этой комбинацией (Option[T] где T — value-record с fn-ptr полем).  
**Где:** `emit_c.rs` — lowering Option[T] + mut-match arm binding когда T = value-record.  
**Статус:** 🟡 GATED → исправляется workflow.

### Bug 3 — `[requires-CC-FAIL]` return 0 вместо zero-init struct
**Симптом:** метод с `requires n > 0`, возвращающий value-record (`BoxIter[T]`),
генерирует `return 0` в guard-пути нарушения контракта → CC-FAIL (C несовместимые типы).  
**Обходной путь:** тест через `test "..." { }` блок вместо `fn main()`.  
**Где:** `emit_c.rs` — генерация requires-guard раннего return.  
**Статус:** 🟡 → исправляется workflow.

---

## Deliverables

| Deliverable | Статус |
|---|---|
| Диагностика root-cause всех 3 багов | 🟡 |
| Фикс Bug 1 (zip) в emit_c.rs | 🟡 |
| Фикс Bug 2 (flat_map) в emit_c.rs | 🟡 |
| Фикс Bug 3 (requires) в emit_c.rs | 🟡 |
| Rebuild release nova | 🟡 |
| Позитивные тесты zip (plan153_2/zip_basic.nv) | 🟡 |
| Позитивные тесты flat_map (plan153_2/flat_map_basic.nv) | 🟡 |
| Конвертация step_by_zero_neg в fn main() форму | 🟡 |
| Регрессия plan153_2 + plan153_0 + basics + generics + plan131 | 🟡 |
| D260 amend (spec) | 🟡 |
| backlog-followups.md обновлён | 🟡 |
| simplifications.md обновлён | 🟡 |
| project-creation.txt обновлён | 🟡 |
| nova-private/discussion-log.md обновлён | 🟡 |
| Коммиты по одному на задачу | 🟡 |

---

## Критерии приёмки

1. `v.lazy().zip(other).collect()` компилируется и возвращает правильные пары — **без упрощений как для прода**.
2. `v.lazy().flat_map(|x| ...).collect()` компилируется с `Option[BoxIter[U]]` внутри.
3. `step_by_zero_neg.nv` в форме `fn main()` проходит как `EXPECT_RUNTIME_PANIC requires` — без обходного `test` блока.
4. 0 новых регрессий по blast-radius (plan153_2/plan153_0/basics/generics/plan131).
5. Спека D260 обновлена: GATED → ✅ для исправленных пунктов.
6. **«Без упрощений как для прода»** — обязательный критерий: фиксы хирургические, не эвристики; тесты покрывают edge-case (пустой zip, разные длины, пустой flat_map результат).

---

## Статус по завершении

> _Заполнится workflow-агентом._

**Workflow run ID:** wf_2aa22c0c-ee3  
**Запущен:** 2026-06-16
