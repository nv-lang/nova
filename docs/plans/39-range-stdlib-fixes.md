// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 39: `std/collections/range.nv` stdlib fixes

> **Статус:** план, не начат. Низкий приоритет (follow-up Plan 38).
> **Создан:** 2026-05-12.
> **Обнаружен:** 2026-05-12 при работе над Plan 35 Ф.1 (cross-file
> resolve).
> **Зависит от:** [Plan 38](38-numeric-type-constants.md) (`int.MAX`
> mangling fix). Без Plan 38 — `range.nv` не компилируется вообще.

---

## Контекст

`std/collections/range.nv` определяет `Range` / `RangeIter` /
`StepRangeIter` / `ReverseRangeIter` — core types для всех for-in
циклов (выходит за primitive `0..N` literal). Currently file не
проходит full `nova build` / `nova test` из-за нескольких блокеров:

1. **`int.MAX` codegen mangling** → undefined C identifier `int_MAX`.
   Это **Plan 38** (numeric type constants), не Plan 39 territory.
   После Plan 38 — этот блокер уходит.

2. **Cross-file resolution не работает в test_runner.** `nova test
   std/collections/range.nv` использует test_runner pipeline (не
   `cmd_build`). Plan 35 Ф.1 MVP добавил `resolve_imports_inline`
   только в `cmd_build`. **Plan 35 Ф.1 follow-up** (test_runner
   parity, ~50 LOC) разблокирует это.

3. **`NovaOpt_nova_int` typedef mismatch на `r == None` ассертах в
   `range.nv` тестах.** Pre-existing — детально не диагностирован.

После закрытия Plan 38 + Plan 35 follow-up — этот план занимается
остаточными issues в `range.nv` (если таковые останутся) и **fix-up
коммит** для добавления `range.nv` в running test suite.

---

## Scope

### Ф.1 — Verify post-Plan 38 + Plan 35 follow-up

После завершения Plan 38 + Plan 35 follow-up:
1. `nova build std/collections/range.nv` — должен пройти.
2. `nova test std/collections/range.nv` — запустить, собрать список
   остаточных fails.

### Ф.2 — Fix остаточные issues

Зависит от output Ф.1. Возможные categories:

**Issue A: `NovaOpt_nova_int` typedef mismatch.**
Тесты вроде:
```nova
let r = (0..10).step_by(2)
let m = r.next()
// ... позже:
assert(r.next() == None)
```

Codegen emit'ит assert как `nova_opt_eq_nova_int(r.next(), None)` где
`None` имеет тип `NovaOpt_nova_int` (legacy) но `r.next()` возвращает
`NovaOpt_Nova_int` (newer). Type mismatch.

Mitigation: Plan 14 Ф.1 (`NovaOpt_<T>` правильно типизированный) уже
закрыл core path. Остаточное — corner case в pattern match comparison.

**Issue B: ReverseRangeIter / `step_by(negative)`.**
`Range.@step_by(step int)` strict positive. Reverse iteration через
отдельный `ReverseRangeIter`. Codegen-resolution.

**Issue C: `.. step_by(0)` validation throws.**
Throw на `step <= 0`. Plan 16 capability check для `Fail[OverflowError]`
effect — должно работать.

### Ф.3 — Тесты в running suite

Когда file полностью PASS — добавить в `nova_tests/std_smoke/`
(или подобное) для regression coverage.

---

## Acceptance criteria

- `nova build std/collections/range.nv` exit 0.
- `nova test std/collections/range.nv` — все 11 declared тестов PASS.
- Regression: 208/208 existing tests без regression.

---

## Связь

- **Plan 35 Ф.1** — cross-file resolution для test_runner. Required.
- **Plan 38** — `int.MAX` codegen mapping. Required.
- **Plan 14 Ф.1** — `NovaOpt_<T>` правильная типизация. Already done,
  но residual edge cases возможны.

---

## Что НЕ входит

- **Performance optimization** Range iteration (specialized loop unroll,
  etc.) — separate plan.
- **Generic Range[T]** (numeric trait abstraction) — Plan 15+ Plan 17.
  Currently `Range` only over `int`.

---

## Estimate

Зависит от Ф.1 output:
- Если 0 residual issues после Plan 38 + Plan 35 follow-up: **0 LOC**,
  только add'нуть file в test suite + commit.
- Если есть Issue A/B/C: **~50-200 LOC** в зависимости от severity.

---

## Risks

- **`range.nv` сам по себе может быть outdated** относительно current
  language semantics. Может требоваться refactor чтобы соответствовать
  новому codegen. Estimate выше может undershoot.

---

## Audit history

- **2026-05-12 v1:** создан после Plan 35 Ф.1 MVP. `range.nv` остаётся
  blocked даже после Plan 35 cross-file fix — pre-existing `int.MAX`
  codegen bug требует Plan 38.
