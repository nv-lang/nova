# План 19: Closure & error-ops & handler-rev — миграция на `|x|` + `fn(...)` + `!!` + `Handler[E, IRT]`

**Статус:** ✅ **ЗАКРЫТ** (2026-05-10). C1–C14 реализованы; C16
(mut-capture codegen) трекается отдельно как post-Plan-19 задача.
**Дата создания:** 2026-05-10.
**Дата закрытия:** 2026-05-10.

## Retro

Реализация прошла за один день в 14 коммитах (C1–C14):

| Коммит | Тема | Статус |
|---|---|---|
| C1 (`85901b3`) | AST: ClosureLight + ClosureFull + Trailing + FnSigBody | ✅ |
| C2 (`245ec29`) | Parser closure-light `\|x\|` / `\|\|` / `\|_\|` | ✅ + 11 lib-tests |
| C3 (`d1c24ea`) | Parser closure-full `fn(...)` без имени | ✅ + 8 lib-tests |
| C4 (`1f73952`) | Parser trailing-fn + Trailing enum | ✅ + 5 lib-tests |
| C5 (`77c741b`) | Interp + codegen runtime для new closure-узлов | ✅ + 1 nova_test |
| C6 | Type-check bidirectional inference | DEFERRED (минимум в C5; полный — отдельная задача после type-checker'а) |
| C7 (`25214a6`) | Postfix `!!` + D85 семантика `?` | ✅ + 1 nova_test |
| C8 (`fdca989`) | Handler-лямбда `\|err\|` (D31-rev, interp-side) | ✅ |
| C9+C10 (`4bd8ea6`) | `Handler[E, IRT]` (D87) + default generic params (D88) | ✅ + 6 lib-tests |
| C11 (`0a2fd2a`) | Миграция nova_tests на новый синтаксис | ✅ |
| C12 (`306de4f`) | Миграция std + examples | ✅ |
| C13 (`335b907`) | Удаление старой `(params) =>` lambda + legacy trailing | ✅ |
| C14 (`0761d52`) | Corner-case regression-тесты | ✅ + 4 nova_tests |
| C15 | Docs retro (этот) | ✅ |
| C16 | Mut-capture codegen heap-cells | TODO (отдельный PR) |

### Финальные метрики

- **Lib-тесты:** 65/65 PASS (35 baseline + 30 новых для Plan 19).
- **nova_tests codegen:** 102/102 PASS (99 baseline + 3 новых файла:
  closure_rev.nv, postfix_bang_bang.nv, closure_corner_cases.nv).
- **std/ type-check:** 49/50 (1 baseline-fail json.nv не связан с
  Plan 19, существующее ограничение match-arm + `+=`).
- **Изменено файлов:** ~50 в spec/ + ~40 nova_tests + std + examples.

### Обнаруженные баги в существующем коде (починено попутно)

Применяя production-grade подход (cм. feedback_codegen_bugs):
- **let-stored closure call** (`let zero = || 0; zero()`) — раньше
  падал в codegen с link error на `nova_fn_zero`. Починено в C5
  через регистрацию ClosureLight/Full в `fn_param_sigs`.
- **Trailing-fn в expr-body** требует pre-rewrite в codegen — closure
  не теряется как arg. Починено в C5.
- **Disambiguation `||` boolean OR vs no-arg closure** — добавлен
  специальный path в parse_primary (Pipe/PipePipe только в
  expression-start position). Старый `assert(!!true)` в
  basics/operators.nv продолжает работать (parse_unary handles
  prefix `!!`).

### Отложено

- **C16: Mut-capture codegen** — `let mut x = 0; let f = || { x += 1 }`.
  Сейчас env копирует value, не shared reference. Production-grade
  fix: переключить storage mut-captured переменных на heap-allocated
  cells (`nova_int*`), reads/writes через `*ptr`. Требует analyse-pass
  + рефактор reads/writes. Tracked как Q-mut-capture-codegen.
- **C8 codegen handler-лямбда** — `with X = |err| body` через C-codegen
  не реализован (handler в `emit_with` ожидает NovaVtable struct).
  Interp-side D31-rev работает; codegen полная поддержка — отдельная
  задача (требует синтез handler-литерала из closure'а).
- **Effect inference на named fn** — отказ от R1, см. D22-rev
  ([D22](../../spec/decisions/03-syntax.md#d22-closure-light--и-full-fn)).
- **Bidirectional inference HOF arg → closure-light** — частичное
  через let-аннотацию работает (C5). Production-grade требует полного
  type-checker'а; bootstrap fallback (default `nova_int`) работает
  для основных паттернов.

---

## Архитектурные решения, принятые в ходе implementation
**Зависимости:**
- Closure-rev: [D22 rev](../../spec/decisions/03-syntax.md#d22-closure-light--и-full-fn),
  [D40 rev](../../spec/decisions/03-syntax.md#d40-тело-функции--для-одного-выражения--для-блока),
  [D43 rev](../../spec/decisions/03-syntax.md#d43-trailing-block--без-params-fnp-body-с-params).
- Error-ops: [D85](../../spec/decisions/04-effects.md#d85-операторы--и--унифицированное-поведение-для-result-и-option-throw-стиль-через-)
  (`?` / `!!`), [D86](../../spec/decisions/04-effects.md#d86--coalesce-оператор-fallback-для-resultoption) (`??`),
  [D67 отменён](../../spec/decisions/04-effects.md#d67--оператор-семантика-для-result-через-fail-для-option-через-ранний-return).
- Handler-rev: [D31 rev](../../spec/decisions/04-effects.md#d31-handler-лямбда-для-эффектов-с-одной-операцией)
  (handler-лямбда на `|x|`),
  [D87](../../spec/decisions/04-effects.md#d87-handlere-irt--параметризация-handler-типом-interruptа)
  (`Handler[E, IRT]`),
  [D88](../../spec/decisions/03-syntax.md#d88-default-значения-generic-параметров)
  (default generic params, `IRT = Never`).

---

## Цель

Атомарно закрыть **две связанные ревизии expression-grammar**:

**1. Closure-rev** — перевести Nova с лямбды `(params) => expr` на
двухуровневый closure:
- **closure-light** — `|x| body` для untyped one-liner'ов и block-форм
- **closure-full** — `fn(x T) Effects -> R body` для typed/effect-aware

Параллельно: trailing-block расщепить на `{ block }` (no params, DSL)
и `fn(p) body` (with params).

**2. Error-ops (D85)** — добавить постфиксный `!!` для throw-стиля:
- `expr?` теперь всегда ранний return обёртки (для обоих `Option` и
  `Result`); `?` отвязан от `Fail`.
- `expr!!` — новый постфиксный оператор throw-стиля. Бросает через
  `Fail[E]` (для `Result`) или `Fail[RuntimeNoneError]` (для `Option`).
- `??` остаётся как coalesce / кастомный fallback (D86).

Обе ревизии меняют expression-grammar, требуют миграции одних и тех
же тестов/stdlib — поэтому один план, один атомарный PR, одна
миграция.

Спека уже обновлена (2026-05-10). Этот план — про реализацию в
парсере / interp / codegen.

---

## ⚠️ Атомарность Ф.1–Ф.5 + Ф.10 — ОДИН PR

**Ф.1–Ф.5 + Ф.10 — это breaking change по expression-grammar и
должны быть выпущены одним атомарным коммитом / PR'ом.** Промежуточные
состояния **нелегальны**:

- Closure: парсер либо принимает старую `(params) =>`, либо новую
  `|x|` + `fn(...)`, но не обе одновременно (overlap создаёт
  ambiguity при `(x) =>` — это либо group-expr + closure-arrow,
  либо старая lambda-форма).
- Error-ops: `expr?` либо имеет старую D67-семантику (через `Fail`
  для Result), либо новую D85 (всегда return). Промежуточно — `?` на
  Result в Fail-функции работает в одних местах, не работает в
  других. Тоже нелегально.

**Что входит в атомарный PR:**
- Ф.1: lexer minor (`_` как parameter ident).
- Ф.2: parser closure-light `|x|`.
- Ф.3: parser closure-full `fn(...)` без имени.
- Ф.4: parser trailing-fn (`f(args) fn(p) body`).
- Ф.5: удаление старой `(params) =>` lambda-grammar и
  `f(args) { x => body }` trailing-with-params.
- **Ф.10: parser postfix `!!` (D85) + смена семантики `?` (D85).**

**Что НЕ входит** (отдельные PR'ы):
- Ф.6 — interp eval changes (отдельным PR, поверх атомарного).
- Ф.7 — type-check / inference (отдельный PR).
- Ф.8 — миграция existing nova_tests / stdlib (отдельный PR
  одновременно с атомарным, иначе тесты сломаются на момент мерджа).
- Ф.9 — новые corner-case regression-тесты.

**Migration coordination:** Ф.5 удаляет старую grammar; в этот же
момент **все** примеры в `nova_tests/`, stdlib и docs должны
использовать новую grammar. Ф.8 (migration) и атомарный PR должны
ехать вместе или Ф.8 — первым (предварительно подготовив тесты, но
парсер ещё на старой grammar — нелегально). Реалистично:
**Ф.8 + Ф.1–Ф.5 в одном merge'е**. Сначала Ф.8 в branch'е готовим
(заменяем синтаксис в тестах), потом Ф.1–Ф.5 поверх; merge — атомарно.

---

## Декомпозиция

### Ф.1. Lexer (parser-frontend)

Текущий лексер выдаёт `|` как одиночный токен (binary OR / `@or`),
`||` как `LogicalOr` (D46 говорит `||` short-circuit не перегружается,
но токен есть для оператора).

**Что нужно:**

1. Сохранить `|` как универсальный токен (binary OR + closure delim).
   Распознавание closure'а — на parser-уровне по позиции
   (expression-start vs after-operand).
2. `||` — оставить как `LogicalOr` (binary), но parser должен
   распознать `||` в expression-start position как **zero-arg
   closure-light start + end** (одновременно открывает и закрывает
   pipe-pair).
3. `_` — extension D59: уже разрешён как pattern wildcard, нужно
   разрешить как identifier в parameter-position (closure-light,
   closure-full, named fn). Малая правка name-resolution.

### Ф.2. Parser — closure-light

В expression-position: `parse_primary` после уже существующих веток
(literal, paren-group, array, и т.д.) добавить:

```rust
TokenKind::Pipe => parse_closure_light(...)        // |x| body
TokenKind::LogicalOr => parse_zero_arg_closure_light(...)  // || body
```

Грамматика:
```
closure-light = '|' [ ident { ',' ident } ] '|' (expression | block)
zero-arg-cl   = '||' (expression | block)
```

Тело: один из `parse_expression()` или `parse_block()`. Решение
по look-ahead на `{`: если первый токен `{` и за ним нет `:` (record-литерал) —
block; иначе expression.

**Дисамбигуация с binary OR:**
- В expression-start position (после `=`, `(`, `,`, `return`, `=>`,
  `{`, начало стейтмента) → closure.
- После operand (число, identifier, `)`, `]`, `}`) → binary OR.

Парсер уже различает unary/binary `-` и `*` по позиции — тот же
механизм для `|`.

### Ф.3. Parser — closure-full

В expression-position добавить ветку:
```rust
TokenKind::Fn if peek_after_fn_is_lparen() => parse_closure_full(...)
```

То есть `fn` без идентификатора, сразу `(` → closure-full. С идентификатором
после `fn` — это named fn (только в statement-position; в expression-position
запрещено = compile error «name is not allowed in anonymous fn»).

Грамматика:
```
closure-full = 'fn' '(' params ')' [ effects ] [ '->' type ] body
body         = '=>' expression | block
```

`params` — переиспользовать существующий `parse_fn_params` (named fn
parameters); типы параметров **обязательны** в closure-full.

### Ф.4. Parser — trailing-fn

В `parse_call_postfix` после уже существующего trailing-block branch
добавить:
```rust
if peek().kind == TokenKind::LBrace { parse_trailing_block() }
else if peek().kind == TokenKind::Fn { parse_trailing_fn() }
```

Где `parse_trailing_fn()` — то же что `parse_closure_full()` без
имени, но привязанный как trailing-аргумент к call'у.

Удалить старую логику trailing-block с params:
```rust
// УДАЛИТЬ:
// trailing-block = '{' [ params '=>' ] block-body '}'
// Теперь:
// trailing-block = '{' block-body '}'    -- БЕЗ params
```

### Ф.5. Удалить старую закрытие-грамматику

Полностью удалить:
- `parse_lambda` для формы `(params) =>`.
- AST-узел `ExprKind::Lambda { params, body, .. }` с полем `params`,
  где у каждого param есть optional type — поскольку closure-light
  не имеет типов, а closure-full переиспользует named-fn параметры.
- Старую логику парсинга `( ident, ident ) =>` в expression-position.

Заменить на два новых AST-узла:
- `ExprKind::ClosureLight { params: Vec<String>, body: ClosureBody }`
- `ExprKind::ClosureFull { params, effects, return_type, body }` —
  переиспользует тот же AST что и named fn (просто без имени).

### Ф.6. Interp — context-inference для closure-light

closure-light в interp выводит сигнатуру **во время вызова**, не на
этапе создания. Текущая реализация в
[compiler-codegen/src/interp/mod.rs:590-603](../../compiler-codegen/src/interp/mod.rs#L590)
уже работает в этом духе (closure хранит params без типов, body
выполняется в Env-context'е). Изменения минимальны:

1. Поле `params: Vec<Param { name, type? }>` → `Vec<String>` (имена
   только) для closure-light. closure-full остаётся с типами.
2. Receiver-capture (`@`) — без изменений.
3. Variadic-last для closure-light — **запретить** (closure-light не
   имеет grammar для `...rest`); closure-full может иметь.

### Ф.7. Type-checker (когда дойдёт)

В bootstrap-стадии type-checker частичный (Plan 15 BoundCtx). Для
closure-light нужно:

1. **Bidirectional inference**: тип closure'а выводится из expected
   sig (param-of-call / let annotation / return-position).
2. **First-use fix**: если closure хранится в let без аннотации —
   откладывать решение до первого вызова, фиксировать тип параметров.
3. **Effect propagation**: эффекты, использованные в теле, должны быть
   подмножеством ambient effect-set'а в точке создания closure'а.
   Compile error если parent не объявил эффект.

### Ф.8. Миграция тестов и stdlib

Существующие тесты в [compiler-codegen/tests/](../../compiler-codegen/tests/)
и stdlib используют старый синтаксис обеих ревизий. Нужно две
параллельные ветки миграции в одном PR:

**8a. Closure migration (closure-rev):**

1. Найти всё через `grep -r "=> " tests/ | grep -P "\\([a-z_, ]+\\) =>"`.
2. Заменить на `|x|` form. Те, что используют типы — на `fn(...)`.

**8b. Error-ops migration (D85):**

1. Найти все `expr?` в функциях с `Fail[E] -> T` (не `-> Result/Option`).
   ```bash
   grep -rn "?" std/ nova_tests/ | <фильтрация по контексту функции>
   ```
2. Для каждого случая решить:
   - **Если хотим throw-стиль:** заменить `expr?` на `expr!!`.
   - **Если хотим return-стиль:** изменить сигнатуру функции на
     `-> Result[T, E]` (без `Fail[E]`), `expr?` остаётся как был
     (по новой D85-семантике делает `return Err(e)`).
3. Найти все `expr?` на `Option`/`Result` в функциях с правильной
   обёрткой возврата — их семантика в D85 та же (ранний return
   обёртки), миграция не нужна, но проверить.
4. Найти все `xs.first()?` / `lookup(k)?` в функциях с `Fail` — те же
   правила: `!!` (бросает `RuntimeNoneError`) или сменить сигнатуру.

**Финальная проверка:** `nova_tests` — 0 regressions.

### Ф.9. Регрессионные тесты на corner case'ы

Добавить в test-suite:

**Closure (4 case'а):**

1. **free-variable resolution** — compile error для `|| a` где `a`
   параметр соседнего closure'а.
2. **body-type mismatch** — `|a| count += a` для sig `-> int` →
   compile error.
3. **multiple-shared-capture** — три closure'а на один `count`;
   выполнение последовательное, проверка ожидаемого финального
   значения.
4. **escape с captures** — closure живёт после parent-fn; проверить
   что captured переменные сохраняются.

**Error-ops (D85, 5 case'ов):**

5. **`?` на Result в `-> Result` функции** — корректно делает
   `return Err(e)`, не задействует `Fail`.
6. **`?` на Option в `-> Option` функции** — корректно делает
   `return None`.
7. **`!!` на Result** — корректно делает `throw e`, требует `Fail[E]`
   в сигнатуре; без `Fail` — compile error.
8. **`!!` на Option** — корректно делает `throw RuntimeNoneError`,
   требует `Fail[RuntimeNoneError]` в сигнатуре.
9. **Парсер edge-case `b!!c`** — compile error «два выражения
   подряд», hint про скобки или оператор.
10. **`?` и `!!` в одном выражении** — `parse(s)? + lookup(k)!!`
    в правильной сигнатуре — компилируется и работает.

### Ф.10. Parser — postfix `!!` + смена семантики `?`

В expression-grammar postfix-operators добавить `!!` параллельно с `?`:

```rust
// В parse_postfix:
TokenKind::Question  => parse_question_postfix()  // existing, семантика меняется
TokenKind::BangBang  => parse_bangbang_postfix()  // NEW — D85
```

**Новая семантика `?`:**
- Тип выражения = `Option[T]` или `Result[T, E]` — обязательно.
- Внешняя функция должна возвращать `Option[U]` (для `?` на Option)
  или `Result[U, E']` (для `?` на Result), где `E'` совместим с `E`.
- Desugar:
  - `Option`: `match { Some(v) => v; None => return None }`.
  - `Result`: `match { Ok(v) => v; Err(e) => return Err(e) }`.
- **Эффект `Fail` НЕ требуется и НЕ задействуется.** Если внешняя
  функция объявляет `Fail`, но `?` на Result не вписывается в
  return-type — compile error.

**Семантика `!!`:**
- Тип выражения = `Option[T]` или `Result[T, E]` — обязательно.
- Внешняя функция должна иметь `Fail[E']` в сигнатуре, где `E'`
  совместим с `E` (для Result) или `RuntimeNoneError` (для Option).
- Desugar:
  - `Option`: `match { Some(v) => v; None => throw RuntimeNoneError }`.
  - `Result`: `match { Ok(v) => v; Err(e) => throw e }`.
- Без `Fail[E]` в сигнатуре — compile error «`!!` requires
  `Fail[E]` in function signature».

**Парсер:**

- `!!` — двухсимвольный токен. Лексер должен распознавать `!!`
  отдельно от `!`, иначе `!!cond` будет парситься как `!(!cond)`.
- В expression-position (start) `!!cond` валиден семантически
  (двойной boolean not, бессмысленный — линтер warning); парсер
  принимает.
- В postfix-position (после operand) `expr!!` — D85 throw.
- Edge-case `b!!c` — два выражения подряд, parse error «expected
  operator between expressions», hint «put a space and operator
  (`b!! - c`) or wrap (`(b!!).method()`)».

**AST:**

```rust
ExprKind::Question { expr: Box<Expr>, span: Span }     // existing, semantics change
ExprKind::BangBang { expr: Box<Expr>, span: Span }     // NEW
```

**Удалить:**
- Старую D67/D4 codegen-логику для `?` через Fail. Заменить на
  новую: всегда `match + return` (для `Option` или `Result`).
- Зависимость `?` от ambient `Fail`-handler resolution.

**Bootstrap-runtime:**
- `RuntimeNoneError` — новый prelude-тип. Добавить в
  [compiler-codegen/src/types/prelude](...) (или эквивалент).
- C-codegen: `RuntimeNoneError` как unit-тип — простая `void*`-метка
  или enum-tag без полей.

### Ф.11. Handler-лямбда мигрирует на `|x|` (D31-rev)

Handler-лямбда [D31](../../spec/decisions/04-effects.md#d31-handler-лямбда-для-эффектов-с-одной-операцией)
переезжает с `(params) => expr` на `|params| body`. Сахар теперь:

```nova
with Fail[Error] = |err| interrupt log_and_default(err) { ... }
```

**Парсер:**
- В позиции `with EffectName = ...`: новый case'ом — closure-light
  (`|...|`) интерпретируется как handler-лямбда (компилятор смотрит
  на ожидаемый `Handler[E]` и unify'ит).
- В этой позиции closure-full с handler-семантикой (`fn(args) body`)
  не вводится — для много-операционных эффектов требуется
  `handler EffectName { ... }` literal.

**Type-checker:**
- Проверяет что эффект имеет ровно одну операцию.
- Параметры `|params|` сопоставляются с параметрами этой операции по
  позиции, типы выводятся из effect-декларации.

**Тело — bare expr или block** (как у closure-light D22).

### Ф.12. `Handler[E, IRT]` параметризация (D87)

Тип `Handler` получает второй generic-параметр. Зависит от Ф.13
(default generic).

**Парсер:**
- `Handler[E]` → парсится как `Handler[E, Never]` (через D88 default).
- `Handler[E, T]` → второй параметр — тип interrupt'а.
- В type-position и в return-type функций.

**Type-checker:**
- Inference IRT из тела handler-литерала: supertype всех
  `interrupt v` выражений, `Never` если их нет.
- Compile error: `Handler[E, Never]` содержит `interrupt`.
- Compile error: `interrupt v` где `typeof(v)` несовместим с явно
  объявленным IRT.
- Unification IRT с типом with-блока (`W`) при использовании в `with`.

**Codegen:**
- `Handler` runtime-структура без изменений (IRT — type-erasure'ится в
  runtime).
- Compile-time проверки — целиком в type-checker'е.

### Ф.13. Default generic params (D88)

Поддержка `[T = Default]` и `[T Bound = Default]` в генерике
объявлений (типов и функций).

**Лексер:** `=` уже есть как token, без изменений.

**Парсер:**
- В generic-list `[name [bound] [= default]]`. После `=` — type-expr.
- Constraint: после параметра с default не может быть параметр без
  default (compile error).

**Type-checker / inference:**
- При monomorphization, если `T` не выведен из аргументов и не указан
  явно — подставить `Default`.
- Проверка: `Default ⊑ Bound` (если bound есть) — compile error
  иначе.

**Codegen:**
- Default-параметры monomorphize'ются как обычные generics с
  подставленным значением. Без runtime-overhead.

**Migration:**
- `Handler[E]` в существующих сигнатурах ≡ `Handler[E, Never]`.
  Старый код продолжает работать без правок.
- Места где handler делает `interrupt` — мигрировать на `Handler[E, IRT]`
  (~10 мест, см. spec migration в коммите).

---

## Порядок исполнения

| # | Фаза | Тема | Зависимости | Атом? | Тесты |
|---|---|---|---|---|---|
| Ф.1 | Lexer minor — `_` как parameter ident | closure | — | **A** | unit |
| Ф.2 | Parser closure-light `\|x\|` | closure | Ф.1 | **A** | parse-tests |
| Ф.3 | Parser closure-full `fn(...)` | closure | — | **A** | parse-tests |
| Ф.4 | Parser trailing-fn | closure | Ф.3 | **A** | parse-tests |
| Ф.5 | Удалить старую `(params) =>` + `f(args) { x => body }` | closure | Ф.2, Ф.3, Ф.4 | **A** | regression |
| Ф.6 | Interp eval (closure-light/full + trailing-fn) | closure | Ф.1-Ф.5 | post-A | runtime-tests |
| Ф.7 | Type-check / bidirectional inference | closure | Ф.6 | post-A | full pass |
| Ф.8 | Миграция existing nova_tests, stdlib, docs (closure + error-ops) | both | Ф.5, Ф.10 | **A** (одновременно) | nova_tests passes |
| Ф.9 | Новые corner-case regression-тесты (closure + error-ops) | both | Ф.7, Ф.10 | post-A | new tests pass |
| Ф.10 | Parser postfix `!!` + смена семантики `?` (D85) | error-ops | — | **A** | parse-tests + runtime |
| Ф.11 | Handler-лямбда мигрирует на `\|x\|` (D31-rev) | handler | Ф.2 | **A** | parse + runtime |
| Ф.12 | `Handler[E, IRT]` параметризация (D87) | handler | Ф.13 | **A** | type-check |
| Ф.13 | Default generic params (D88) | generics | — | **A** | parse + type-check |

**A** = входит в атомарный PR (Ф.1–Ф.5 + Ф.8 + Ф.10–Ф.13 одновременно).
Эти фазы должны мерджиться вместе, иначе либо парсер сломан, либо
тесты сломаны на момент мерджа.

**post-A** = отдельные PR'ы поверх атомарного. Ф.6 первым, потом
Ф.7, потом Ф.9.

DRAFT-файл (`spec/decisions/closure-rev2026-05-DRAFT.md`) удалён
2026-05-10 — D22/D40/D43 + D85/D86 в живых
[03-syntax.md](../../spec/decisions/03-syntax.md) и
[04-effects.md](../../spec/decisions/04-effects.md) единственные
source of truth.

Подробнее про атомарность Ф.1–Ф.5 + Ф.10 + Ф.8 — см. секцию
[«⚠️ Атомарность Ф.1–Ф.5 + Ф.10 — ОДИН PR»](#%EF%B8%8F-атомарность-ф1ф5--ф10--один-pr) выше.

---

## Риски

**Closure:**

1. **Bitwise-OR ambiguity.** Если parser ошибётся в позиции
   expression-start vs after-operand, `a | b` будет парситься как
   `a | (closure b)`. Mitigation: чёткое правило «closure-start
   только после `=`, `(`, `,`, `return`, `=>`, `{`, `;`, начала
   line-statement».
2. **First-use inference затруднит error-messages.** Если closure
   используется впервые на line 200, а определена на line 50 — error
   message укажет на line 200 «type fixed here». Mitigation: дополнить
   error «note: first-use here, signature inferred» с указанием line 50.
3. **Migration churn.** ~30 примеров в spec/, ~20 в tests/, ~10 в
   docs/. Запустить migration-script bash/PowerShell перед manual
   review.
4. **Effect-propagation в bootstrap пока недоразвит.** В Plan 15
   эффекты-bound не enforced; closure-light effect-check тоже будет
   частичным. Mitigation: пометить как Q-closure-effects-incomplete,
   закрыть после полного inference в Plan 20+.

**Error-ops (D85):**

5. **Каждый `parse(s)?` в Fail-функции ломается.** Это **самый
   многочисленный** сломанный паттерн в stdlib и тестах — все функции
   с `Fail[E] -> T` сигнатурой и `?` использовали старую D67-семантику
   через Fail. Для каждого случая нужно решение: переход на `!!` (если
   хочется throw-стиль) или смена сигнатуры на `-> Result` (если хочется
   return-стиль). Mitigation: автоматический grep + ручной review каждого
   случая, нельзя script'ом — выбор стиля смысловой.
6. **`!!` парсер edge-case `b!!c`.** Двойной символ может ловить
   неожиданные паттерны. Mitigation: чёткий compile-error «expected
   operator between expressions» с конкретным hint'ом про скобки или
   оператор.
7. **`RuntimeNoneError` в prelude — новый тип.** Нужно убедиться, что
   все места prelude-load в bootstrap его подхватывают (interp, codegen,
   builtins.nv). Mitigation: тест, который явно использует
   `Fail[RuntimeNoneError]` в нескольких разных контекстах.
8. **Пользовательский код за пределами stdlib.** Если кто-то начал
   писать на Nova (даже бутстрапные эксперименты) — все его `?` в
   Fail-функциях сломаются после мерджа. На bootstrap-этапе цена низкая,
   но после v1.0 такая ломающая правка была бы недопустима. Этот PR —
   последняя возможность поменять `?` без обещаний backward-compat.

---

## Definition of Done

- [ ] **Атомарный PR (Ф.1–Ф.5 + Ф.10 + Ф.8)** замерджен; nova_tests
  **0 regressions** на момент мерджа.
- [ ] Ф.6 — interp eval работает на 4 closure corner case'ах.
- [ ] Ф.7 — bidirectional inference работает в bootstrap (хотя бы базово).
- [ ] Ф.9 — 4 closure + 6 error-ops corner-case regression-тестов
  добавлены и проходят.
- [ ] `RuntimeNoneError` доступен из prelude во всех путях (interp,
  codegen).
- [x] DRAFT удалён (2026-05-10), spec/decisions/03-syntax.md +
  04-effects.md — единственные source of truth.
- [x] Запись в [docs/project-creation.txt](../project-creation.txt) и
  [docs/simplifications.md](../simplifications.md) (2026-05-10).
- [x] discussion-log в nova-lang-private обновлён (2026-05-10).

---

## Связь с другими планами

- [Plan 14](14-stdlib-codegen-gaps.md) — closure'ы в stdlib (HOF
  методы Array/HashMap) — нужна миграция примеров.
- [Plan 15](15-generic-bounds-enforcement.md) — generic-bounds могут
  взаимодействовать с closure-effect-inference.
- [Plan 18](18-stdlib-roadmap.md) — общая stdlib roadmap; closure-rev
  упрощает написание HOF API.
