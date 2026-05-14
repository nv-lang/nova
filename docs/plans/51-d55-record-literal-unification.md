# Plan 51: D55 full implementation — record-literal syntax unification

> **Создан 2026-05-15, ревизия 2026-05-15** (audit против Go/Rust/TS,
> production-grade hardening). Без упрощений.
>
> **СТАТУС:** черновик плана (не реализован).
>
> Доводит [D55](../../spec/decisions/02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
> до полной реализации и устраняет TIMTOWTDI: **единственная** форма
> record-литерала — `{ ... }`; имя типа всегда из контекста.

---

## Правило (одно предложение)

> **Record-литерал всегда пишется `{ ... }`. Его тип — это тип, ожидаемый
> в данной позиции** (return-тип, аннотация, тип параметра, тип элемента,
> тип поля, тип match-ветки). **Record-литерал в позиции без ожидаемого
> типа — ошибка компиляции: нужна аннотация.**

Правило проще, чем у Go (см. ниже), и при этом мощнее. `TypeName { ... }`
удаляется из языка целиком.

---

## Зачем

### D55 — полу-реализованная спека + TIMTOWTDI

D55 нормативно вводит coercion литерала в позиции с явным целевым
типом. Спека перечисляет **6 позиций**, реально реализовано **~2**:

Codegen тащит target-тип через поле `expected_record_type`
([emit_c.rs:282](../../compiler-codegen/src/codegen/emit_c.rs)) только в
теле функции/метода (из return-типа) и `const` с аннотацией. Не
протянуто: `let x T = { ... }`, `f({ ... })`, элементы `[]T`, match-arm,
вложенные record-поля, generic-параметры, `return`-выражение, присваивание
типизированной переменной, ветки `if`/`match`-выражений, элементы tuple.
Type-checker аналогично — `infer_arg_ty`
([types/mod.rs:797](../../compiler-codegen/src/types/mod.rs), 2 call-site:
414, 664) выводит тип только из `type_name: Some(...)`. Back-fill-пассы
(`type_name: None → Some`) нет.

Следствие — **две формы одного и того же**:
- `fn f() -> FiberStatus => FiberStatus { ... }` — имя дважды;
- `fn f() -> FiberStatus => { ... }` — каноничная форма (работает);
- `let s FiberStatus = { ... }` — **не компилируется сегодня**.

Где D55 дотянулся — обе формы валидны (TIMTOWTDI); где не дотянулся —
обязателен префикс. Это анти-паттерн, отвергаемый в других местах Nova
(Plan 42.3 / 42.7).

### Ключевой факт: анонимных record-значений в Nova нет

`emit_record_lit` ([emit_c.rs:9349-9351](../../compiler-codegen/src/codegen/emit_c.rs))
для `type_name: None` без `expected_record_type` и без spread даёт
**жёсткую ошибку** «anonymous record literal without spread not
supported in codegen». **Каждый** record-литерал обязан разрешиться в
номинальный тип — анонимные record'ы не first-class значения.

Это делает **полное удаление `TypeName { }` чистым решением**: литерал
*и так* всегда требует разрешимого типа; убирая префикс, мы переносим
«написание имени» в аннотацию/сигнатуру — концептуально ничего не
теряется.

---

## Сравнение с Go / Rust / TS

| Аспект | Rust | Go | TypeScript | Nova + Plan 51 |
|---|---|---|---|---|
| Форма литерала | всегда `Name { ... }` | `Name{ ... }` | всегда `{ ... }` | всегда `{ ... }` |
| Type elision | ❌ нет нигде | ⚠️ только в элементах array/slice/map | ✅ везде (contextual typing) | ✅ везде, где есть target-тип |
| `return T{...}` elision | ❌ | ❌ | ✅ | ✅ |
| `let x T = {...}` elision | ❌ | ❌ | ✅ | ✅ |
| Аргумент `f({...})` elision | ❌ | ❌ | ✅ | ✅ |
| Номинальность | номинально | номинально | **структурно** | **номинально** |
| Анонимные record-типы | ❌ | ❌ | ✅ (footguns) | ❌ (by design, D5) |
| Functional update / spread | `..base` | нет | `...base` | `{ ...base, x }` |

**Вывод:**

- **Лучше Rust** — Rust *всегда* требует имя структуры; elision нет
  нигде. Nova+Plan51 эргономичнее.
- **Лучше Go** — Go elide'ит имя *только* в элементах коллекций
  (`[]Point{{1,2}}`); `return`, `var`, аргументы — с именем. Nova+Plan51
  elide'ит во **всех** target-typed позициях.
- **Эргономика ≈ TS, безопасность > TS** — TS даёт contextual typing,
  но он **структурный**: анонимные object-типы — first-class, отсюда
  footguns (excess-property-check quirks, две структуры одной формы
  взаимозаменяемы, слабые номинальные гарантии). Nova **номинальна**:
  `{ ... }` всегда разрешается в конкретный именованный тип. Nova+Plan51 =
  эргономика TS + номинальная безопасность.

**Честный trade-off — локальная читабельность.** Rust/Go: `Point { ... }`
сообщает тип inline, без взгляда на контекст. Nova+Plan51: тип нужно
увидеть в *смежной* return-аннотации / let-аннотации. Это осознанный
выбор; смягчается тем, что (а) тип **рядом** — та же строка сигнатуры/
аннотации, не в другом файле; (б) номинальные имена полей сами по себе
идентифицируют тип; (в) AI-first one-way — приоритетная ценность Nova;
(г) IDE-hover / `nova doc` показывают разрешённый тип. Признаём trade-off
явно, не выдаём за «строго лучше во всём».

---

## Архитектурное решение

### Bidirectional checking (contextual typing) + resolution pass

То, что TS называет *contextual typing*, а теория типов —
*checking-режимом bidirectional type checking*: ожидаемый тип **течёт
внутрь** выражения. Сейчас этого направления в type-checker'е нет
системно — есть ad-hoc `expected_record_type` в codegen на 2 позиции.

**Production-решение:** contextual-typing проход, интегрированный в
`types::check_module`. Type-checker уже обходит выражения; добавляем
параметр `expected: Option<&TypeRef>`, который протягивается во все
target-typed позиции. На каждом `RecordLit` проход **back-fill'ит**
`RecordLit.type_name: None → Some(resolved)`. AST у bootstrap общий и
мутабельный (`infer_effects` уже его мутирует) — back-fill естественен.

После прохода:
- **каждый** `RecordLit` well-typed программы имеет разрешённый тип;
- codegen всегда читает разрешённый тип — поле `expected_record_type`,
  его save/restore-обвязка и ветка «typeless + expected» **удаляются**
  (codegen упрощается);
- единственный источник истины — один проход, не контекст, размазанный
  по codegen.

### Тот же проход делает sum-coercion

D55 — это **и** record-, **и** sum-coercion (`let a StrOrInt = "test"`
→ `S("test")`). Механизм один: «привести выражение к ожидаемому типу».
Contextual-typing проход на target-typed позиции смотрит на выражение:
- `RecordLit { type_name: None }` → разрешить тип;
- значение типа `S`, target `T` — sum с единственным unary-ctor `C(S)`
  → обернуть в `C(...)`.

Одна инфраструктура — обе половины D55. «Полный D55» = обе.

### AST: разрешённый тип с генериками

`RecordLit.type_name: Option<Vec<String>>` хранит только path. Для
generic record-типов (`Box[int]`, `Pair[K,V]`) codegen-мономорфизации
нужен полный `TypeRef` с генериками. **Ф.0 решает:** хватает ли
`type_name: Vec<String>` (генерики выводятся из значений полей, как
сейчас для `Pair { a: 1, b: "x" }`) — или `RecordLit` нужно поле
`resolved_type: Option<TypeRef>` (AST-изменение). Не угадываем — решаем
по факту в Ф.0.

### Циклическая инференция — честное ограничение

`g({ ... })` где `g[T](x T)` и `T` ограничен **только** этим аргументом:
тип `T` выводится из аргумента, тип аргумента — из `T`. Цикл. Typeless-
литерал **не может** драйвить generic-инференцию (у него нет
собственного типа). Это — **ошибка** с внятным сообщением, не
«best-effort». Документируется в spec и тестируется (negative).

### Порядок проходов

`parse` → `manifest check` → `resolve_imports` → **`check_module`
(+ contextual-typing проход здесь)** → `infer_effects` → `callnorm`
→ codegen. Проход — часть `check_module` (там есть type-context). Param-
defaults (Plan 46): type-checker уже проверяет default против типа
параметра — contextual typing там получает param-type как target;
`callnorm` после этого видит уже разрешённые литералы.

`infer_arg_ty` (call-site 414, 664) — record-ветка субсумируется
contextual-typing проходом; Ф.1 решает: расширить `infer_arg_ty` делегацией
или заменить эти call-site'ы.

---

## Полная энумерация позиций (расширяет D55)

D55 спека перечисляет 6 позиций — для production-реализации список
**неполон**. Полная энумерация target-typed позиций (Ф.0 финализирует +
вносит в spec):

| # | Позиция | Источник target-типа |
|---|---|---|
| 1 | `let x T = <lit>` | аннотация |
| 2 | `fn f(x T)` на call-site `f(<lit>)` | тип параметра |
| 3 | `fn f() -> T => <lit>` / хвост блок-тела | return-тип |
| 4 | `return <lit>` (явный return) | return-тип функции |
| 5 | элемент `[]T`: `[<lit>, <lit>]` с известным `[]T` | тип элемента |
| 6 | match-arm result (тип ветки фиксирован) | тип ветки |
| 7 | ветка `if`-выражения (`let x T = if c { <lit> } else { <lit> }`) | тип if-выражения |
| 8 | вложенное поле: `{ outer: <lit> }` | declared-тип поля `outer` |
| 9 | присваивание: `x = <lit>` где `x` известного типа | тип lvalue |
| 10 | элемент tuple: `(a, <lit>)` с target `(A, B)` | тип компонента |
| 11 | generic-параметр после конкретизации (`Maybe[User]`) | конкретизированный generic |
| 12 | аргумент по умолчанию: `fn f(x T = <lit>)` | тип параметра |

Позиции **без** target-типа (`let x = <lit>` без аннотации, return
неаннотированного замыкания, единственный generic-драйвер) → ошибка
«cannot infer record type».

---

## Фазы

### Ф.0 — Census + энумерация + design-решения + spec

- **Census:** скрипт-grep всех `TypeName { }` сайтов в `std/*`,
  `nova_tests/*`, `examples/*`. Категоризация по позициям 1-12 + «no
  context». Точный масштаб (предв. — 150-300 сайтов; первичный grep дал
  100+ в 30 файлах).
- **No-context аудит:** каждый сайт без выводимого target — подтвердить,
  что чинится аннотацией. Позиция, где аннотацию поставить негде →
  **hard blocker**, поднять явно.
- **Sum-coercion аудит:** текущая полнота sum-coercion по позициям
  1-12. Gaps → Ф.1/Ф.2 (тот же проход).
- **Энумерация позиций** — финализировать таблицу 1-12, внести в spec
  (D55 table сейчас неполна).
- **AST-решение:** `type_name: Vec<String>` достаточно или нужен
  `resolved_type: TypeRef` (генерики). Проверить текущий codegen
  generic record-литералов.
- **Решение о форме запрета:** полное удаление `TypeName { }`
  (рекомендация) vs partial. Рекомендация — полное (см. «альтернативы»).
- **Spec:** ревизия D55 — `{ ... }` каноничен, `TypeName { }` удалён;
  полная таблица позиций; правило «нет target → ошибка»; циклическая
  инференция = ошибка. Folds in: `=>`-тело record-литерал ⇒ return-тип
  обязателен.

**Acceptance:** census-таблица; энумерация в spec; AST-решение
зафиксировано; D55 обновлён; sum-coercion статус ясен.

### Ф.1 — Type-checker: contextual typing + resolution pass

Самая объёмная и рискованная фаза.

- Добавить `expected: Option<&TypeRef>` в обход выражений `check_module`;
  протянуть во все 12 позиций.
- На `RecordLit` — разрешить тип из `expected`, back-fill `type_name`
  (или `resolved_type`).
- Тот же проход: sum-coercion (значение → unary-ctor).
- Валидация полей typeless-литерала против разрешённого типа: missing
  field, unknown field, field-type mismatch.
- Вложенность: разрешённый тип внешнего литерала → declared-типы полей
  → target для вложенных литералов (рекурсивно).
- Generic record-типы: разрешить с генериками (per Ф.0-решение).
- Циклическая generic-инференция → ошибка.
- `{ ... }` без `expected` → ошибка «cannot infer record type».

**Риск:** bootstrap type-checker — «best-effort permissive». Неизвестно,
хватает ли type-info для робастной propagation во всех 12 позициях
(вложенность + generic — главные сомнения). Ф.0 census показывает
глубину. Если позиция не покрывается робастно — **blocker**, поднять,
не упрощать молча (цель плана — полнота).

**Acceptance:** debug-assert — каждый `RecordLit` в regression-корпусе
разрешён после прохода; type-checker проходит; negative-кейсы (no
context, cyclic) дают внятные ошибки.

### Ф.2 — Codegen: читать разрешённый тип, удалить контекст-стек

- Codegen всегда читает разрешённый тип `RecordLit`. Удалить: поле
  `expected_record_type`, save/restore в `emit_fn_body` /
  `emit_method_body` / const-decl, ветку «typeless + expected».
- Ветка `else → error` остаётся как internal-error (для well-typed
  недостижима).
- spread-ветка (`{ ...p, y }`): согласовать с разрешённым типом — если
  есть и target, и spread-источник, они должны совпадать; добавить
  проверку.

**Acceptance:** codegen упрощён, `expected_record_type` удалён; полный
regression-корпус компилируется.

### Ф.3 — Parser/check: запрет `TypeName { }` + ревизия дисамбигуации

- `TypeName { }` → ошибка с подсказкой: «record literals are written
  `{ ... }`; type is inferred from context (here: `T`)». Парсер
  по-прежнему распознаёт форму (для хорошей ошибки) либо отклоняет в
  `parse_record_lit_after_path` при непустом `path` — точку выбрать.
- **Ревизия дисамбигуации `looks_like_record_lit`:** под полной
  унификацией typeless-литералы вездесущи. Проверить и задокументировать
  edge-cases: `{ x }` (сейчас — record-punning, не блок), `=> { x }`,
  пустой `{ }`. Решить, остаётся ли bare-`{ name }` punning (D52) или
  требует `{ name, }` / `@name`. Не менять D52 без причины — но
  зафиксировать поведение явно.
- `{ ... }` без выводимого target → внятная ошибка про аннотацию.

**Acceptance:** `TypeName { }` отклоняется; дисамбигуация-правила
задокументированы; negative-тесты на формы.

### Ф.4 — Миграция

- Переписать **все** `TypeName { }` → `{ ... }` в `std/*`,
  `nova_tests/*`, `examples/*`.
- Категории по сложности:
  - **механические** (`=> T { }`, `: T = T { }` — удалить `T `);
  - **вставка аннотации** (`let x = T { }` → `let x T = { }`);
  - **no-context** (добавить аннотацию в подходящем месте).
- Скрипт для механических; вставка аннотаций — полу-ручная с
  верификацией.
- **Big-bang, без deprecation-периода** — язык не в проде (память:
  «не бойся больших переделок»); compat-режим не нужен.
- Коммитить пачками (std / nova_tests / examples), регрессия после
  каждой.

**Acceptance:** ноль `TypeName { }` в репозитории; полный regression
компилируется.

### Ф.5 — Тесты

- **Positive — по тесту на каждую из 12 позиций.** Не «несколько
  тестов» — покрытие позиций.
- **Resolution-correctness:** тест проверяет, что выбран **правильный**
  тип (не просто «компилируется») — напр. два типа с одинаковыми полями,
  target различает.
- **Nesting:** вложенность ≥3 уровней; `{ outer: { mid: { inner } } }`.
- **Generic:** typeless-литерал в `Maybe[User]`, `[]User`, `Pair[K,V]`.
- **Negative (`EXPECT_COMPILE_ERROR`):**
  - `TypeName { ... }` — отклоняется;
  - `{ ... }` без target — ошибка про аннотацию;
  - `fn f() => { ... }` без return-типа — ошибка;
  - циклическая generic-инференция (`g({ })`, `g[T](x T)`) — ошибка;
  - missing field / unknown field / field-type mismatch — внятные;
  - дисамбигуация edge-case (если решено что-то запретить).
- **Sum-coercion** тесты — если Ф.0 вскрыл gaps.

### Ф.6 — Регрессия + docs + commit

- Полный `nova test` (release) — без новых FAIL.
- `project-creation.txt` + `simplifications.md` записи.
- discussion-log private-репы.
- README — если упоминается синтаксис record-литералов.
- Каждая фаза — отдельный commit.

---

## Диагностика (production-grade bar)

«Не хуже Rust/TS» = качество ошибок. Целевой уровень (с примерами):

| Ситуация | Сообщение |
|---|---|
| `TypeName { ... }` | «record literals are written `{ ... }` — the type is inferred from context (here: `TypeName`). Remove `TypeName`.» |
| `{ ... }` без target | «cannot infer record type for `{ ... }` — add a type annotation (`let x T = ...`) or a return type» |
| unknown field | «type `FiberStatus` has no field `eror_msg` — did you mean `error_msg`?» |
| missing field | «missing field `value` for type `FiberStatus`» |
| field-type mismatch | «field `value`: expected `int`, got `str`» |
| cyclic generic | «cannot infer record type — the call `g({...})` does not determine its type; annotate the argument» |
| `=>`-тело record без return-типа | «function returning a record literal must declare its return type: `fn f() -> T => { ... }`» |

Все — с точным span (FileId + line:col, инфраструктура есть).

---

## Альтернативы (рассмотрены и отвергнуты)

1. **Узкая версия — запрет только в `=>`-теле.** Создаёт новую
   непоследовательность: `=>`-тело становится единственным местом, где
   префикс *запрещён*, а не *опционален*. Больше правил, не меньше.
   Отвергнуто (обсуждено с пользователем).
2. **Partial per-position one-way** — `{ }` в context-позициях,
   `TypeName { }` в no-context. Технически one-way per-position, но два
   правила вместо одного, и перенос кода между позициями требует смены
   формы литерала. Полное удаление проще: одно правило.
3. **Оставить `TypeName { }` как опциональный explicitness-инструмент**
   (как `as` в TS) — это ровно TIMTOWTDI снова. Если `{ }` работает
   везде с аннотациями, `TypeName { }` не добавляет ничего, кроме
   второго способа.
4. **Структурная типизация (TS-путь)** — анонимные record-типы
   first-class. Отвергнуто D5; даёт footguns. Nova остаётся номинальной.

---

## Риски / trade-offs

- **Ф.1 — главный риск.** Робастность contextual-typing propagation в
  bootstrap type-checker'е (вложенность, generic). Неизвестна до Ф.0
  census. Если позиция не покрывается — hard blocker, поднять явно.
- **Масштаб миграции** — 150-300 сайтов. Механически, но объёмно;
  пачками + регрессия.
- **Behavior change** — код в no-context позициях требует добавления
  аннотаций. Ф.0 census оценивает; ожидается мало.
- **AST-изменение** (`resolved_type`) — если Ф.0 решит, что path
  недостаточно для генериков, это касается parser + type-checker +
  codegen + interp.
- **Локальная читабельность** — осознанный trade-off (см. сравнение).
- **Не делаем half-measure** — узкая версия отвергнута; Plan 51 —
  «правильное решение, не минимальное».

---

## Critical files

| Файл | Действие |
|---|---|
| `spec/decisions/02-types.md` (D55) | Ф.0 — ревизия: `{ }` каноничен, полная таблица позиций |
| `compiler-codegen/src/ast/mod.rs` | Ф.0/Ф.1 — возможно `RecordLit.resolved_type` |
| `compiler-codegen/src/types/mod.rs` | Ф.1 — contextual typing проход + resolution + sum-coercion + `infer_arg_ty` fate |
| `compiler-codegen/src/codegen/emit_c.rs` | Ф.2 — читать разрешённый тип, удалить `expected_record_type` |
| `compiler-codegen/src/parser/mod.rs` | Ф.3 — запрет непустого `path`; ревизия `looks_like_record_lit` |
| `compiler-codegen/src/interp/mod.rs` | Ф.1/Ф.2 — interp-путь record-литералов синхронизировать |
| `std/**`, `nova_tests/**`, `examples/**` | Ф.4 — миграция |
| `nova_tests/types/*` (+ новые) | Ф.5 — positive (12 позиций) + negative |

---

## Acceptance criteria (production-grade)

- [ ] D55 реализован полностью — record- **и** sum-coercion во всех
      позициях финализированной энумерации (1-12).
- [ ] `RecordLit` после contextual-typing прохода всегда разрешён для
      well-typed программ (debug-assert в regression-корпусе).
- [ ] `TypeName { }` удалён из языка — отклоняется с подсказкой.
- [ ] `{ ... }` без выводимого target → внятная ошибка про аннотацию.
- [ ] Циклическая generic-инференция → внятная ошибка (не silent).
- [ ] `expected_record_type` контекст-стек удалён из codegen.
- [ ] Диагностика на уровне таблицы выше (unknown/missing field,
      mismatch, cyclic) — с точными span.
- [ ] Вся кодовая база мигрирована — ноль `TypeName { }`.
- [ ] Positive-тест на каждую из 12 позиций + resolution-correctness +
      nesting + generic + negative на все запрещённые формы.
- [ ] Полный release-regression без новых FAIL.
- [ ] Spec D55 отражает реализацию (полная таблица, правило, ошибки).

---

## Что НЕ входит

- **Анонимные record-типы как first-class значения** — их нет сейчас
  (codegen ошибается), Plan 51 их не вводит. Каждый литерал по-прежнему
  разрешается в номинальный тип.
- **Структурная типизация / row polymorphism** — вне scope, D5.
- **Generic-инференция, драйвимая литералом** — невозможно (у typeless-
  литерала нет своего типа); это явная ошибка, не фича.
- **Spread-механизм** (`{ ...p, y }`) — рабочий, только согласуется с
  разрешённым типом (Ф.2), не переписывается.

---

## Связь

- [D55](../../spec/decisions/02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
  — спецификация (ревизуется в Ф.0).
- [D5](../../spec/decisions/02-types.md) — отказ от структурной
  типизации (контекст «почему номинально, а не TS-путь»).
- [D52](../../spec/decisions/02-types.md) — record punning (учитывается
  в ревизии дисамбигуации, Ф.3).
- Plan 46 — `callnorm` (порядок проходов, param-defaults).
- Plan 42.3 / 42.7 — прецеденты отказа от TIMTOWTDI-синтаксиса.
