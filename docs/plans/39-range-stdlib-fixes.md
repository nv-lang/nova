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

**Issue D: Iter[T] resolution в codegen — похоже неверно определяется
итератор для `for x in c`.**

При работе над Plan 35 Ф.1 (cross-file resolve) наблюдается: даже
после Range registered, **`for x in (0..N).step_by(K)`** иногда
fall back'ает на `for-in: unsupported iterator type 'nova_int'`.
Это значит codegen path для **Iter[T] protocol resolution** имеет
gap — либо method `next` не находится, либо `iter()` chain не
разворачивается.

#### Алгоритм из спецификации D58 (что должно происходить)

[spec/decisions/03-syntax.md#d58 §«`for x in c` — implicit iter»](../../spec/decisions/03-syntax.md):

```
for x in c { body }
```

компилируется как:

1. **Если `c` имеет `mut next() -> Option[T]`** — используется
   напрямую как итератор. Generated C-loop:
   ```c
   for (;;) {
       NovaOpt_<T> opt = <c_type>_method_next(c);
       if (opt.tag == NOVA_TAG_Option_None) break;
       <T> x = opt.value;
       <body>
   }
   ```

2. **Иначе если `c` имеет `iter() -> Iter[T]`** — компилятор вставляет
   `c.iter()` и применяет (1) к результату:
   ```c
   <iter_type>* it = <c_type>_method_iter(c);
   for (;;) {
       NovaOpt_<T> opt = <iter_type>_method_next(it);
       ...
   }
   ```

3. **Иначе** — compile error `"for-in: type <T> has neither
   `next` nor `iter()` method"`.

**Structural typing (D42/D53):** «есть метод» = **есть запись в
`method_overloads[(type_name, "next" | "iter")]`**. Не требуется
explicit `impl Iter for X` (нет такой грамматики).

**Specialization (Case 1 / Case 2):** Range literal `0..N` /
`a..b` — primitive int loop (Case 1 в `emit_for`, до Iter[T]
fallback). Это **shortcut**, не violation D58 — semantically
equivalent loop body.

**Array** `[]T` — primitive index loop (Case 2). Через `len`/`data`
pointers, не `iter().next()`. **Это violation D58 буквально**, но
acceptable performance optimization (D58 говорит «компилируется
**как** ...», не «компилируется в **literal** sequence of `iter()`
+ `next()` calls»).

#### Текущая реализация (что наблюдается)

`compiler-codegen/src/codegen/emit_c.rs::emit_for`:

- **Case 1 (line ~7881)**: `ExprKind::Range { .. }` — primitive int
  loop. OK для `for i in 0..N`.
- **Case 2 (line ~7916)**: `arr_ty.starts_with("NovaArray_")` —
  array iteration. OK для `for x in xs`.
- **Case 3 (line ~7964)**: Iter[T] protocol fallback. Trigger:
  `all_methods.contains(&(iter_struct, "next"))`. Generates
  generic loop с `next()`-call.
- **Fallback (line ~8079)**: `error: for-in: unsupported iterator
  type '...'`.

**Что неверно:**
- `infer_expr_c_type` для `ExprKind::Call { .. }` ищет
  `method_overloads.get((recv_type, method_name))`. **При cross-file
  imports** (Plan 35) — `recv_type` может быть **локальное имя**
  (`Range`), но `method_overloads` зарегистрировал под полным
  module path (`std.collections.range.Range`) или без префикса.
  Mismatch → fallback.
- **Auto-`iter()` insertion** (Case 2 алгоритма) — найти ли в
  codegen? Если **только Case 1 (next()) check** делается, и нет
  Case 2 — это **violation D58**: `for x in [1,2,3].iter()` будет
  работать, но `for x in some_collection` где collection имеет
  только `iter()` (не `next()`) — fall through fallback error.
- **`mut next()`-specifier** — codegen должен проверить что метод
  имеет `is_mut=true` (mutable receiver). Иначе iterator advance
  не обновляет state.

#### Что fix'ить в Plan 39 Ф.2 (Issue D)

1. **Audit `emit_for` Case 3**: воспроизвести D58 algorithm
   exactly:
   ```
   match arr_ty.method("next") {
       Some(sig) if sig.is_mut && sig.return_c_type.starts_with("NovaOpt_") =>
           generate_next_loop(arr_ty, sig)
       None => match arr_ty.method("iter") {
           Some(iter_sig) => {
               let iter_ty = iter_sig.return_c_type;
               // Recursive: lookup next() on iter_ty.
               match iter_ty.method("next") { ... }
           }
           None => fallback_error("type X has no `next` or `iter`")
       }
   }
   ```

2. **Diagnostic clarity**: вместо «unsupported iterator type 'nova_int'»
   эмитить точное:
   - `"for-in: type 'Range' has no `next` method, falling back to
     `iter()` lookup..."`
   - `"for-in: type 'Range' has neither `next` nor `iter` method"`.

3. **`mut`-receiver check**: assert `is_mut=true` для `next()`.

4. **Cross-file method resolution**: после Plan 35 ensured что
   imported method'ы registered под short name (Range, не
   std_collections_range_Range). Plan 39 Issue D просто verify'ит
   что resolution lookup ищет правильно.

5. **Test coverage** (новый file `nova_tests/syntax/for_in_iter_resolution.nv`):
   - Type с `next()` напрямую (без `iter()`).
   - Type с `iter()` returning другой type с `next()`.
   - Type без обоих → error message clear.
   - `mut`-receiver enforcement.
   - Generic `Iter[T]` через protocol-bound parameter.

### Ф.3 — Тесты в running suite

Когда file полностью PASS — добавить в `nova_tests/std_smoke/`
(или подобное) для regression coverage.

---

## Acceptance criteria

- `nova build std/collections/range.nv` exit 0.
- `nova test std/collections/range.nv` — все 11 declared тестов PASS.
- **Issue D acceptance:** новый файл `nova_tests/syntax/for_in_iter_resolution.nv`
  — 5 тестов (next() direct, iter()-chain, no-methods error, mut-receiver,
  generic Iter[T] bound) PASS.
- **Issue D diagnostic:** error `for-in: type 'X' has neither `next` nor
  `iter`` instead of generic «unsupported iterator type 'nova_int'».
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
- **Issue D** (Iter[T] resolution audit + auto-`iter()` insertion +
  diagnostic clarity): **~100-150 LOC** в `emit_c.rs::emit_for` Case 3
  + **~80 LOC** в новом test'е. Полдня.

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
