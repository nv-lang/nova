# Plan 78: Prelude codegen single-source — устранить hardcoded зеркала

> **Создан 2026-05-21.** Follow-up Plan 62 (prelude hardcode migration).
>
> **Статус:** 📋 proposed, не начат.
>
> **Приоритет:** P2 — корректностного бага нет (layered registry +
> hardcoded fallback работают), но есть **дублирование** = drift-риск.
>
> **Предшественники:** Plan 62 ✅ (вынес prelude в `.nv`); Plan 59 ✅
> (Result mono). Soft-зависимость: Plan 68 (print/println как
> protocol-as-value) — для Ф.3.

---

## Проблема

Plan 62 вынес prelude из Rust-хардкода в файлы `std/prelude/*.nv`.
Эти файлы — **source of truth** для type-checker'а, cross-file resolve
и `nova doc`.

Но **codegen** до сих пор содержит **параллельные hardcoded зеркала**
тех же самых prelude-сущностей:

| Хардкод | Где | Что дублирует |
|---------|-----|---------------|
| `init_hardcoded_baseline()` — `HardcodedBaseline` entries Option / Result / RuntimeError | `sum_schema_registry.rs` | variants + method_routing, которые объявлены в `std/prelude/core.nv` + `errors.nv` |
| `sum_schemas` pre-populate — `Option`, `NovaOpt_nova_int`, `RuntimeError` | `emit_c.rs` (~1120) | те же типы (Result-entry уже удалён Plan 62.A.bis Ф.4) |
| print/println/panic/assert/debug_assert/exit special-cases | `emit_c.rs` (~12747-21091) | `std/prelude/runtime.nv` declarations |

**Почему это плохо:** `.nv`-декларация и codegen-хардкод могут
разойтись (variant set, сигнатуры) — silent drift. Plan 62.A.bis
сделал registry «слоёным» (`DeclaredFromPrelude > HardcodedBaseline`)
именно как временную меру — hardcoded baseline остался fallback'ом.

**Это НЕ про автоген.** `std/runtime/*.nv` автогенерируется из
Rust-реестра потому что runtime реализован в C. Prelude — наоборот:
`.nv` уже source of truth, цель Plan 78 — чтобы **codegen тоже
читал `.nv`**, а не имел свою хардкод-копию. Направление: убрать
Rust-хардкод, не добавить генератор.

**Конечная цель:** `std/prelude/*.nv` — **единственный** источник
правды для prelude, включая codegen. Никаких параллельных зеркал.

---

## Фазы

### Ф.1 — Method-routing из деклараций (P2, ~3-4 dev-days)

**Сейчас:** `init_hardcoded_baseline()` несёт `method_routing` для
~20 методов Option/Result (`is_ok` → `Nova_Result_method_is_ok` и
т.д.). `init_prelude_decls_from_items` **наследует** routing от
baseline (см. `[M-legacy-sum-schemas-retained]`: без baseline
Prelude-entry не регистрируется → dispatch ломается).

**Цель:** routing выводится из самих `.nv`-деклараций — `external fn
Result[T, E] @is_ok() -> bool` в `core.nv` даёт codegen'у достаточно,
чтобы построить C-trampoline mapping. Тогда baseline-entry удаляется.

**Подходы (выбрать в Ф.1.0 design):**
- (a) Naming-convention: метод `m` на типе `T` → `Nova_<T>_method_<m>`
  (+ `<inline>` для closure-applying методов — нужен маркер).
- (b) Routing-аннотация в `.nv` (`#routing(...)` или doc-attr).
- (c) Гибрид: convention по умолчанию + override-аннотация для
  нестандартных (`<inline>` sentinel'ы).

После: `init_prelude_decls_from_items` строит routing с нуля из
declarations; `init_hardcoded_baseline` Option/Result entries удаляются.

> **Порядок-капкан (урок Plan 59).** Ф.1 **завершить и проверить
> полным прогоном ДО начала Ф.2.** `init_prelude_decls_from_items`
> наследует routing от baseline — если удалить baseline раньше, чем
> `.nv`-routing полностью работает, dispatch ломается молча. В Plan 59
> был аналог (флип D3 нельзя было до dual-mode D1/D2). Ф.1.0 design
> обязан сначала подтвердить, что выбранная схема покрывает **все**
> mono-trampoline'ы — включая per-(T,E) `_<n>`-суффиксы (как
> `Nova_Result_method_is_ok_<n>` из `register_novares_decl`), не
> только non-mono методы.

### Ф.2 — Удалить `sum_schemas` pre-populate (P2, ~1-2 dev-days)

**Сейчас:** `emit_c.rs` пре-популяцирует `sum_schemas["Option"]`,
`["NovaOpt_nova_int"]`, `["RuntimeError"]` (Result уже удалён Ф.4).

**Цель:** убрать pre-populate — registry / mono-типы единственный
источник. По образцу Plan 62.A.bis Ф.4:
- Option: `NovaOpt_<T>` уже мономорфизирован (Plan 14/59) — проверить
  что nested-pattern Option-typing идёт через mono, не `sum_schemas`
  (тот же класс бага что Plan 59 пост-фикс для Result).
- RuntimeError: non-generic value-type, 6 вариантов — registry
  `DeclaredFromPrelude` entry (из `errors.nv`) должен полностью
  покрывать; убрать `sum_schemas["RuntimeError"]`.

**Уроки Plan 59 (тот же класс работы — удаление hardcoded зеркала).**

1. **Чинить обе pattern-функции, не только `pattern_bind_typed`.**
   В Plan 59 gap был в **двух** местах: `pattern_bind_typed`
   (привязка переменной из payload) И `pattern_cond` (проверка тега
   варианта). Починка только первой оставляет `pattern_cond`
   сломанным для вложенных паттернов. Ф.2 обязана покрыть **обе**.

2. **Конкретный repro для Option-внутри.** Plan 59 пост-фикс закрыл
   `Option` *снаружи* (`Some(Ok(..))` / `Some(Err(..))` на
   `Option[Result[T,E]]`). Ф.2 про `sum_schemas["Option"]` — это
   симметричный, но **другой** баг: `Option` *внутри*. Обязательные
   repro-кейсы: `match x { Some(Some(v)) => v }` на
   `Option[Option[T]]`, плюс `Ok(Some(..))` на `Result[Option[T],E]`.
   Без явного кейса легко пропустить.

3. **RuntimeError — проверить record-variant путь.** `RuntimeError`
   имеет record-варианты (`IndexOutOfBounds {i, n}`). В Plan 59
   record-variant payload (`[M-result-record-payload-match]`) шёл
   через **отдельную** ветку `pattern_bind_typed` (Record-ветка,
   ~18599), не Tuple-ветку. Ф.2 для RuntimeError проверить и
   tuple-, и record-variant pattern-пути.

**Метод:** удалить insert → полный прогон → если регрессия, локализо-
вать (как с Result nested-pattern) → фикс или honest-defer.

> **Flaky-прогон (урок Plan 59).** При удалении `sum_schemas
> ["Result"]` единичный «1 FAIL» оказался flaky — не воспроизвёлся
> на повторном прогоне без изменений кода. При FAIL в Ф.2 —
> **перепрогнать дважды**, отличить flaky от реальной регрессии,
> прежде чем диагностировать.

### Ф.3 — print/println — делегировать Plan 68 (deferred)

**Сейчас:** `emit_c.rs` спец-кейсы `name == "println"` / `"print"`
(~12747, 21091) — per-arg type dispatch через `infer_print_helper`.

**Цель:** print/println как обычные Nova-функции через protocol-as-
value (`[]Into[str]`) — это **Plan 68** (proposed, блокирован Q1-Q6).
Plan 78 **не дублирует** Plan 68 — Ф.3 = «дождаться Plan 68, тогда
спец-кейс удаляется». Ссылка, не реализация.

### Ф.4 — panic/exit/assert/debug_assert — аудит (P3, ~1 dev-day)

**Сейчас:** спец-кейсы (~12826-12868) для:
- panic/exit — comma-expression обёртка `(nv_panic(msg), (nova_int)0)`
  в expression-position (`??` coalesce, if-else branches).
- assert/debug_assert — D89 expression-context + auto-derived
  `cond_text` (Plan 11).

**Цель:** проверить — это **prelude-хардкод** (зеркало `runtime.nv`)
или **легитимная codegen-механика** (expression-position для
`Never`-возвращающих fns не убирается в принципе)?

**Вероятный исход:** оставить как legitimate codegen, но
**переклассифицировать** — это не «дублирование prelude-декларации»,
а codegen-обработка expression-position. Задокументировать в
`simplifications.md`, чтобы не путать с настоящим хардкодом.

### Ф.5 — spec + docs + verification (P2, ~0.5 dev-day)

- Spec D26 / D-блок: «prelude — единственный source of truth, codegen
  читает `.nv`».
- `simplifications.md`: `[M-legacy-sum-schemas-retained]` остаток
  закрыт; print/println → Plan 68 reference.
- Acceptance: grep по `emit_c.rs` / `sum_schema_registry.rs` — ноль
  hardcoded prelude variant/routing данных (кроме Ф.4 legitimate).
- Полный прогон 0 регрессий.

---

## Что НЕ в scope

- `Never` в `types/mod.rs` builtins HashSet — bottom-тип, → Plan 76
  (`Never` → строчный keyword `never`).
- `gc` / `fibers` / `bench` namespaces в builtins — не prelude, это
  runtime/tooling namespaces (правильно хардкод — C-backed).
- Примитивы (`int`/`bool`/`str`/…) в builtins — genuine built-in типы,
  не prelude.
- Автогенерация `std/prelude.nv` facade из под-файлов — опционально,
  отдельная мелкая эргономика (не source-of-truth вопрос).
- `std/runtime/*.nv` автоген — корректен (C-backed), не трогаем.

---

## Связь

- [Plan 62](62-prelude-hardcode-migration.md) — вынес prelude в `.nv`;
  Plan 78 доводит до конца (codegen тоже).
- [Plan 62.A.bis](62.A.bis-sum-schema-registry.md) — слоёный registry;
  Ф.4 удалил `sum_schemas["Result"]` — Plan 78 Ф.1/Ф.2 убирают остаток.
- [Plan 59](59-tuple-monomorphization.md) — Result mono; образец для
  Option-mono проверки в Ф.2.
- [Plan 68](68-print-as-nova-function.md) — print/println как обычные
  функции; Plan 78 Ф.3 делегирует туда.
- [Plan 76](76-never-lowercase-keyword.md) — `Never` → `never`
  keyword; ортогонально.
- `[M-legacy-sum-schemas-retained]` (simplifications.md) — Ф.4-остаток
  (init_hardcoded_baseline) реклассифицирован; Plan 78 Ф.1 закрывает.
