# План 17: Q-resolutions — закрыть полу-открытые вопросы

**Статус:** ✅ ЗАКРЫТ (2026-05-08).
**Дата создания:** 2026-05-08.
**Цель:** закрыть Q-вопросы которые **уже работают в коде**, но не
зафиксированы в spec, и Q-вопросы где простое решение очевидно.

## Выполнение (2026-05-08)

| # | Q | Результат |
|---|---|---|
| 1 | Q-string-interpolation | ✅ **CLOSED**: spec D44 + полная реализация (Plan 17 Ф.4). Lexer escape `\$`→SOH+$ sentinel. Parser sub-lex/parse каждого `${expr}` в `ExprKind::InterpolatedStr`. Codegen — **StringBuilder цепочка** с pre-size estimate (одна аллокация, без O(N²) от `+`). Per-fragment type dispatch: nova_str / bool→bool_to_str / f64→f64_to_str / char-literal→char_to_str / user-type через D73 `@into()->str` / fallback int_to_str. Interp — конкатенация через `format!`. Регрессия: `nova_tests/types/string_interpolation.nv` (13 тестов). |
| 2 | Q-clone-semantics | ✅ CLOSED через D26 (shallow по умолчанию; deep — для StringBuilder/WriteBuffer как исключение). Regression: `nova_tests/runtime/clone_semantics.nv` (5 тестов). |
| 3 | Q-style-coercion | ✅ CLOSED через D55 «Style-guide» — permissive с таблицей рекомендаций. |
| 4 | Q-array-api | ✅ CLOSED через D38 «Built-in API для `[]T`» — minimal built-in + extensions через D35. |
| 5 | Q-keywords-as-fields | ✅ Уже был CLOSED через D83 (Plan 17 подтвердил). |
| 6 | Q-protocol-method-prefix | ✅ CLOSED через D53 «Method-prefix» — обе формы валидны. |
| 7 | Q-pipe-operator | ⏸ DEFERRED to v0.5+ с rationale. |
| 8 | Q-fail-coercion | ⏸ DEFERRED — нужен дизайн, проблема known. |
| 9 | Q-default-generic | ⏸ DEFERRED — ждёт generic Complex/Vector/Matrix. |
| 10 | Q-numeric-coercion | ⏸ DEFERRED — ждёт JsonValue. |
| 11 | Q-static-method-protocol | ⏸ DEFERRED — ждёт Plan 15 generic bounds. |

**Итог:** 6 Q закрыто, 5 Q защищены явным DEFER-rationale.
Регрессия (clone_semantics + string_interpolation) — 18 тестов,
общий прогон 87/87 PASS.

## Ф.4 — string interpolation codegen (2026-05-08)

После audit'а (Ф.1) выяснилось, что заявленное «работает де-факто»
неверно — codegen вообще не разворачивает `${}` в обычных строках.
Ф.4 реализовала полный стек:

1. **Lexer** ([compiler-codegen/src/lexer/mod.rs](../../compiler-codegen/src/lexer/mod.rs)):
   escape `\$` → sentinel `\x01$` (SOH+`$`), чтобы парсер мог
   отличить literal-`${` от interpolation-`${`. SOH в обычном Nova-
   коде не встречается (control char).

2. **AST** ([compiler-codegen/src/ast/mod.rs](../../compiler-codegen/src/ast/mod.rs)):
   новый вариант `ExprKind::InterpolatedStr { parts: Vec<InterpStrPart> }`,
   где `InterpStrPart::Lit(String)` или `InterpStrPart::Expr(Box<Expr>)`.

3. **Parser** ([compiler-codegen/src/parser/mod.rs](../../compiler-codegen/src/parser/mod.rs)):
   `desugar_string_interpolation` после получения `TokenKind::Str(s)`
   в expr-position. Сканирует строку на `${...}` (с учётом SOH-escape),
   sub-lex + sub-parse каждого фрагмента, balanced `{}` внутри expr.
   Если `${}`-нет → возвращает обычный `StrLit`. Pattern-position и
   test-name `Str(s)` остаются как literal (никакой интерполяции).

4. **Codegen** ([compiler-codegen/src/codegen/emit_c.rs](../../compiler-codegen/src/codegen/emit_c.rs)):
   `emit_interpolated_str` эмитит:
   - `Nova_StringBuilder_static_with_capacity(N)` где N — точная
     сумма литералов + 16 байт на каждое expr (эвристика).
   - Для каждой части `Nova_StringBuilder_method_append_str(sb, ...)`.
     Per-fragment dispatch по `infer_expr_c_type`:
     - `nova_str` — pass-through.
     - `nova_bool` — `nova_bool_to_str("true"/"false")`.
     - `nova_f64` — `nova_f64_to_str` (snprintf %g).
     - `CharLit` — `nova_char_to_str` (UTF-8 encode codepoint).
     - User-type с `@into() -> str` (D73 into_targets) —
       `Nova_T_method_into`.
     - Fallback `nova_int_to_str`.
   - Финал: `nova_str result = Nova_StringBuilder_method_into(sb)`.

5. **Interp** ([compiler-codegen/src/interp/mod.rs](../../compiler-codegen/src/interp/mod.rs)):
   eval каждой части, конкатенация через `format!("{}", value)`.

6. **Const-context guard:** `emit_const_expr` для `InterpolatedStr`
   возвращает compile error «not allowed in const initialiser» —
   StringBuilder требует runtime аллокаций.

**Известное ограничение** (Q-char-tracking-in-interpolation):
char-переменная (`let c = 'A'`) в `${c}` печатается как codepoint-
число, потому что bootstrap-codegen не отслеживает char-vs-int через
var-types. Workaround — char-литерал прямо в `${'A'}` или
`${str.from(c)}` если будет. Future task.

---

Spec без формального ответа = LLM не знает «что верно». Закрытие Q
улучшает AI-first без implementation-цены.

---

## Q-вопросы для закрытия в этом плане

### 1. Q-string-interpolation — `"hello ${name}"` через `str.from`

**Текущее:** работает де-факто (codegen разворачивает `${expr}` в
`nova_str_concat(..., str.from(expr), ...)`), но в spec не закреплено.

**Решение:** зафиксировать в [D44 числовые литералы](../../spec/decisions/03-syntax.md#d44)
или новой секции «строковые литералы»: `"... ${expr} ..."` — sugar над
`str.from(parts[0]) + str.from(expr) + str.from(parts[1]) + ...`.
Каждый `${}` фрагмент — `str.from(value)` через D73 [Into].

**Файлы:** `spec/decisions/03-syntax.md` (новая микро-секция или
расширение D44), `spec/syntax.md` (краткий пример).

**Кода менять не нужно** — codegen уже работает. Нужны только
**1-2 теста** чтобы зафиксировать поведение для regression-detection:

- `nova_tests/types/string_interpolation.nv` — int/float/bool/Option
  values в `${}`.

**Объём:** spec — 30 строк, тест — 30 строк.

---

### 2. Q-clone-semantics — `@clone()` это **shallow**

**Текущее:** не зафиксировано. В std `@clone()` встречается на
HashMap/Vec/Set (shallow — копирует buckets/data array, элементы по
ссылке).

**Решение:** зафиксировать в [D26 prelude](../../spec/decisions/08-runtime.md#d26):

> `@clone() -> Self` возвращает **shallow** copy: для record копирует
> поля; для коллекций копирует внутренний storage, элементы остаются
> разделяемыми. Для глубокой копии — `@deep_clone()` (не в prelude,
> определяется по необходимости).

Совместимо с Rust convention (`Clone` shallow, `DeepClone` руками).

**Файлы:** `spec/decisions/08-runtime.md` D26 — добавить пункт.

**Тестов нового кода не нужно** — std уже использует shallow,
зафиксировать поведение тестом в `nova_tests/runtime/clone_semantics.nv`
(новый файл, ~5 тестов на shared-references).

**Объём:** spec — 20 строк, тест — 50 строк.

---

### 3. Q-style-coercion — когда применять D55 coercion

**Текущее:** D55 разрешает `let u User = { id: 1, name: "x" }`, но в
spec нет style-guide когда это **уместно**, а когда писать тип явно.

**Решение:** добавить в [D55](../../spec/decisions/02-types.md#d55) или
[D52](../../spec/decisions/02-types.md#d52) — секцию **«Когда coerce,
когда тип явно»**:

- ✅ `let u User = { ... }` — type binding делает тип очевидным.
- ✅ `serve({ ...DEFAULTS, port: 9000 })` — позиция параметра.
- ✅ `return { ok: true }` — return-position.
- ❌ `let u = { id: 1, name: "x" }` — без аннотации, читателю
  непонятно что за record.
- ❌ Передача в overloaded function без явного типа — ambiguity.

**Файлы:** `spec/decisions/02-types.md` D55 — расширить.

**Кода и тестов не нужно.**

**Объём:** ~60 строк spec.

---

### 4. Q-array-api — minimal canonical API

**Текущее:** `[]T.len`, `.push`, `.pop`, `.get`, `.is_empty` —
встроены. `.first`/`.last`/`.contains`/`.iter`/`.map`/... — extension
methods в `std/collections/vec.nv`. Нет правила что считается «частью
языка», а что — стандартной библиотекой.

**Решение:** зафиксировать в [D38](../../spec/decisions/03-syntax.md#d38)
или [D26](../../spec/decisions/08-runtime.md#d26) минимальный
**built-in API** для `[]T` (то что compiler знает напрямую), и
**stdlib API** (то что приходит через `std/collections/vec.nv`).

Built-in (по spec D38):
- `len` (свойство, без скобок) → codepoints / elements
- `push(x)`, `pop() -> Option[T]`
- `get(i) -> Option[T]`, `get_or(i, d)`, `[i]` (panic на out-of-range)
- `is_empty`
- `iter() -> Iter[T]`
- `[]T.new()`, `[]T.with_capacity(n)`, `[]T.filled(v, n)`

Stdlib extensions (в `std/collections/vec.nv` через D35 method
extension on `[]T`):
- `first/last`, `contains`, `index_of`, `reverse`, `sort`,
  `map/filter/fold`, `any/all`, `enumerate`, `zip`, `take/drop`,
  `unique`, и т.д.

**Файлы:** `spec/decisions/03-syntax.md` D38 — секция «Built-in API
для `[]T`».

**Тестов не нужно** (всё уже работает). Если найдём расхождение
spec-vs-impl — отдельные fix-коммиты.

**Объём:** ~80 строк spec.

---

### 5. Q-keywords-as-fields — окончательно зафиксировать (он закрыт, но не помечен)

**Текущее:** [D83](../../spec/decisions/03-syntax.md#d83) запрещает
keywords как identifier'ы. В open-questions помечено как ✅ закрыто
но в Q-списке остаётся. Откомментить.

**Файлы:** `spec/open-questions.md` — отметить Q-keywords-as-fields
как **CLOSED → D83** или удалить.

**Объём:** 5 строк.

---

## Q-вопросы для **отложения** с явной фиксацией статуса

Эти оставить как `Q-Open with rationale`, чтобы не висело как
«непонятный TODO»:

### Q-pipe-operator `|>`

**Решение:** **отложить до v0.5+**. Запросов из stdlib пока нет,
trailing-block + method-chain покрывают большинство случаев. Можно
вернуться когда появится реальный use-case в большом коде.

Зафиксировать в open-questions: «отложен до v0.5+, обоснование:
текущие альтернативы (method chain, trailing-block) покрывают paint
points; добавление |> требует решения о ассоциативности и приоритете,
лишний complexity без ясного выигрыша».

### Q-fail-coercion — auto-coerce типов ошибок при `?`

**Решение:** **открыт, нужен дизайн**. Идея: `parse(s)? + db_query()?`
где `parse → Fail[ParseError]` и `db_query → Fail[DbError]` — чтобы
скомпилировалось без ручного `map_err`, нужна From-цепочка
`ParseError → AppError`, `DbError → AppError`.

Зафиксировать в open-questions:
- Ссылку на конкретный паттерн где это нужно (примеры из stdlib).
- Варианты решения (auto-derive From через `#[from]`-маркер; явный
  cast `?.into()`; sum-type AppError с `Or<A, B>`).
- Решение **не сейчас**, но проблема known.

### Q-default-generic — default-значения generic-параметров

**Решение:** **отложить**. `HashMap[K, V = str]` (V default = str) —
nice-to-have, но добавляет сложность вывода типов. Не сейчас.

### Q-numeric-coercion — coercion числовых литералов через D55

**Решение:** D55 **уже работает** для record/sum, числовые литералы
имеют отдельный механизм через type-context inference. Закрыть как
«not applicable, see D44 numeric literals + type-context».

### Q-anonymous-effect / Q-effect-type-anonymous

**Решение:** **отложить**, требует серьёзного дизайна effect-row
synthesis.

### Q-static-method-protocol

**Решение:** **отложить**, требует решения о protocol с
non-instance-методами (это другая семантика).

### Q-protocol-method-prefix — `@method()` vs голое в protocol-объявлении

**Решение:** зафиксировать в [D53](../../spec/decisions/02-types.md#d53):
**и `@method()` и голое имя** валидны в protocol-декларации,
эквивалентны (`@` факультативен потому что в protocol-context method
всегда instance).

`std/testing/property.nv` сейчас использует non-`@` форму без проблем.

**Файлы:** D53 — короткая секция «protocol method syntax variations».

**Объём:** 20 строк spec.

---

## Фазы

### Ф.1 — spec-only Q closures (1-3, 4, 5)

Прямая правка spec, **код не трогаем**.

**Файлы:**
- `spec/decisions/03-syntax.md` — D44/D55-extension/D38-array-api/D55-style-guide.
- `spec/decisions/02-types.md` — D52/D53/D55.
- `spec/decisions/08-runtime.md` — D26 clone-semantics.
- `spec/open-questions.md` — статусы / удаления Q-помеченных как
  closed.

**Объём:** ~250 строк spec.

### Ф.2 — Тесты-фиксаторы поведения

Добавить regression-тесты на поведение которое мы зафиксировали:

- `nova_tests/types/string_interpolation.nv` — `${}` для разных типов.
- `nova_tests/runtime/clone_semantics.nv` — shallow vs reference-share.
- (опционально) `nova_tests/types/array_builtin_api.nv` — built-in vs
  extension methods (на случай если кто-то «случайно» начнёт hard-code'ить
  extension method в codegen).

**Файлы:** новые .nv в nova_tests/.

**Объём:** ~150 строк тестов.

### Ф.3 — Q-вопросы «defer with rationale»

Обновить `spec/open-questions.md` для каждого отложенного Q —
добавить **DEFER-rationale**: почему сейчас не делаем, что является
триггером для пересмотра.

**Файлы:** `spec/open-questions.md`.

**Объём:** ~100 строк (по 10-15 на каждый Q).

---

## Что НЕ делаем

- **Q-cstring** — ждёт реального FFI use-case.
- **Q-string-interning** — performance-tuning v1.0+.
- **Q-readonly-types** — TS-style `Readonly<T>` — отдельный major
  feature, не сейчас.
- **Q-record-spread-args** — спред record в args function call.
  Уверенно решим после Ф.6 D69 (Plan 14).

---

## Оценка

~500 строк spec + 150 строк тестов = **1 день**, без кодинга
компилятора.

ROI: **высокий для AI-first** — каждое закрытое Q = это место где
LLM знает «что верно».

---

## Связь

- [Plan 14](14-stdlib-codegen-gaps.md) — некоторые Q зависят от Ф.6
  variadic.
- [spec/open-questions.md](../../spec/open-questions.md) — главный
  файл правок этого плана.

---

## Ссылки

- `spec/open-questions.md` — список всех Q.
- `spec/decisions/` — куда переносим closed Q.
