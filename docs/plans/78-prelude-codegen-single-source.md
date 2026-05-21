# Plan 78: Prelude codegen single-source — устранить hardcoded зеркала

> **Создан 2026-05-21.** Follow-up Plan 62 (prelude hardcode migration).
>
> **Статус:** 🚧 in progress (2026-05-22) — **Ф.1 ✅** (вариант 2 —
> `method_routing` переклассифицирован как легитимный реестр
> C-реализации, не хардкод-зеркало), **Ф.3 ✅** (deferred на Plan 68),
> **Ф.4 ✅** (аудит — legitimate codegen). Осталось: **Ф.2** (удалить
> `sum_schemas` pre-populate — это variants-зеркало) + **Ф.5** (spec/docs).
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

### Ф.1 — Method-routing: переклассификация (P2) ✅ ВЫПОЛНЕН 2026-05-22

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

#### Ф.1.0 — design (✅ ВЫПОЛНЕН 2026-05-22)

Изучена method-dispatch машинерия (`sum_schema_registry.rs` +
`emit_c.rs`). Вывод по схемам (a)/(b)/(c):

**`MethodRouting`** имеет 3 формы. Для Ф.1 значимы:
`HardcodedRuntimeFn { c_name, is_per_t }` (с особым sentinel'ом
`c_name == "<inline>"`). Не-inline методы → реальный C-trampoline;
inline-методы → `<inline>` → codegen падает в match-блоки
(closure-applying, variant-construction).

**Не-inline методы — convention РАБОТАЕТ.**
- Option (`is_some`/`is_none`/`unwrap_or`/`or`): per-T →
  `Nova_Option_method_<m>_<sani_T>`. Суффикс `_<T>` добавляет call-site.
- Result (`is_ok`/`is_err`/`unwrap_or`/`ok`): per-(T,E) →
  `Nova_Result_method_<m>_<ok>_<err>` (mono) либо legacy
  `Nova_Result_method_<m>`. (T,E)-суффикс уже вычисляет call-site
  (`result_method_c_name`); routing лишь сообщает «не-inline».
- Т.е. для не-inline routing сводится к булеву «inline или нет» +
  базовому имени `Nova_<T>_method_<m>` — выводится из имени типа/метода.

**Inline-методы — convention НЕ РАБОТАЕТ, нужен ЯВНЫЙ маркер.**
Inline: Option `unwrap`/`unwrap_or_else`/`map`/`ok_or`; Result
`unwrap`/`unwrap_or_else`/`map`/`map_err`/`err`. Вывести «inline» из
сигнатуры **нельзя надёжно** — контрпример: Result `@ok() -> Option[T]`
(НЕ inline, trampoline) и `@err() -> Option[E]` (inline) имеют
**неразличимые** сигнатуры. Closure-параметр / `Fail`-эффект покрывают
часть, но не `err`.

**Развилка вынесена владельцу плана. Выбран вариант (2) —
переклассификация** (2026-05-22):

Чистая naming-convention из сигнатур невозможна (контрпример
`ok()`/`err()`). Вариант (a) — новый `.nv`-атрибут — затронул бы
parser + spec ради данных, которых в декларации концептуально нет.
Поэтому `method_routing` **переклассифицирован**: это не
хардкод-зеркало `.nv`-декларации, а **легитимный реестр C-реализации**
рантайма — тот же класс, что `runtime_registry.rs` /
`external_registry.rs`. Методы Option/Result физически реализованы
C-функциями в `nova_rt/array.h`; «трамплин/inline», `c_name`,
`is_per_t` — implementation-факты, не declaration-факты. Набор
C-реализованных prelude-типов фиксирован и не растёт с пользовательским
кодом (пользовательские типы получают методы с Nova-телом через
обычный codegen — без routing-таблицы).

#### Ф.1 — итог (✅ ВЫПОЛНЕН 2026-05-22, вариант 2)

- `init_hardcoded_baseline` **остаётся** — его `method_routing`
  легитимен (добавлен переклассифицирующий doc-комментарий в
  `sum_schema_registry.rs`). Удалять нечего; `init_prelude_decls_from_items`
  продолжает наследовать routing от baseline — это корректно.
- Реальное зеркало — `variants` (Some/None, Ok/Err): дубль
  `.nv`-type-деклараций. Слоёный registry уже отдаёт приоритет
  `DeclaredFromPrelude`; чистку pre-populate `sum_schemas` в `emit_c.rs`
  делает Ф.2.
- «Порядок-капкан» (удалять baseline до готового `.nv`-routing) больше
  не актуален — baseline не удаляется. Ф.1 не блокирует Ф.2.

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

### Ф.3 — print/println — делегировать Plan 68 (deferred) ✅ ЗАФИКСИРОВАН 2026-05-22

**Решение зафиксировано:** Ф.3 = ссылка, не реализация. print/println как
обычные Nova-функции через protocol-as-value — это **Plan 68** (proposed,
блокирован Q1-Q6). Plan 78 спец-кейсы print/println НЕ трогает, пока
Plan 68 не разблокирован и не реализован. Делать в Ф.3 сейчас нечего.

**Сейчас:** `emit_c.rs` спец-кейсы `name == "println"` / `"print"`
(~12747, 21091) — per-arg type dispatch через `infer_print_helper`.

**Цель:** print/println как обычные Nova-функции через protocol-as-
value (`[]Into[str]`) — это **Plan 68** (proposed, блокирован Q1-Q6).
Plan 78 **не дублирует** Plan 68 — Ф.3 = «дождаться Plan 68, тогда
спец-кейс удаляется». Ссылка, не реализация.

### Ф.4 — panic/exit/assert/debug_assert — аудит (P3) ✅ ВЫПОЛНЕН 2026-05-22

**Заключение аудита.** Спец-кейсы `panic`/`exit`/`assert`/`debug_assert`
в `emit_c.rs` (~13137-13186) — **legitimate codegen-механика, НЕ
prelude-хардкод-зеркало.**

Сами функции объявлены в `std/prelude/runtime.nv` как `external fn`
(тип/сигнатура — оттуда, не из codegen). Codegen-спец-кейс делает
другое — **expression-position lowering**:
- `panic`/`exit` → `(nv_panic(msg), (nova_int)0LL)` comma-expression:
  `nv_panic` имеет C-сигнатуру `void` и не возвращается (longjmp/abort),
  но C требует value-expression как cast-target в expression-position
  (`??`-coalesce, тернарник). Comma-operator даёт value, не нарушая
  short-circuit.
- `assert`/`debug_assert` → `(nova_assert(cond, "<text>"), NOVA_UNIT)`:
  тот же comma-приём для `nova_unit`-типа в expression-position +
  auto-derived `cond_text` (D89 / Plan 11 — текст условия в сообщении,
  выводится из AST, не из декларации).

Это **обработка `Never`/`void`-возвращающих функций в
expression-position** — её нельзя выразить декларацией в `.nv`,
поэтому она остаётся в codegen. Дублирования prelude-декларации здесь
нет. **Действие: оставить как есть, переклассифицировано** (не хардкод-
зеркало). Отдельной записи в `simplifications.md` не требуется — это
не «упрощение», а корректная codegen-механика.

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
