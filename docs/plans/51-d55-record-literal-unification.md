# Plan 51: D55 full implementation — record-literal syntax unification

> **Создан 2026-05-15.** Production-grade, без упрощений.
>
> **СТАТУС:** черновик плана (не реализован).
>
> Доводит [D55](../../spec/decisions/02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
> до полной реализации и устраняет TIMTOWTDI: **единственная** форма
> record-литерала — `{ ... }`; имя типа всегда берётся из контекста.

---

## Зачем

### Проблема: D55 — полу-реализованная спека + TIMTOWTDI

D55 нормативно вводит coercion литерала в позиции с явным целевым
типом — **6 позиций** (let-аннотация, аргумент функции, return-выражение,
generic после конкретизации, match-arm, элемент `[]T`).

**Реально реализовано ~2 из 6.** Codegen тащит target-тип через поле
`expected_record_type` ([emit_c.rs:282](../../compiler-codegen/src/codegen/emit_c.rs))
только в:
- теле функции/метода (из return-типа — `emit_fn_body`/`emit_method_body`);
- `const` с аннотацией.

Не протянуто: `let x T = { ... }`, `f({ ... })`, элементы `[]T`,
match-arm, вложенные record-поля, generic-параметры. Type-checker
аналогично — `infer_arg_ty` ([types/mod.rs:797](../../compiler-codegen/src/types/mod.rs))
выводит тип только из `type_name: Some(...)`; для `None` — ничего.
Back-fill-пасса (`type_name: None → Some`) нет нигде.

Следствие — **две формы одного и того же**:
- `fn f() -> FiberStatus => FiberStatus { ... }` — `FiberStatus` дважды;
- `fn f() -> FiberStatus => { ... }` — каноничная форма (работает);
- `let s FiberStatus = { ... }` — **не работает сегодня**, нужен
  `FiberStatus { ... }`.

То есть: где D55 дотянулся — обе формы валидны (TIMTOWTDI); где не
дотянулся — обязателен префикс. Это ровно тот анти-паттерн, который
Nova отвергает в других местах (Plan 42.3 / 42.7 — отклонены как
«два способа для одной вещи»).

### Ключевой факт: анонимных record-значений в Nova нет

`emit_record_lit` ([emit_c.rs:9349-9351](../../compiler-codegen/src/codegen/emit_c.rs))
для `type_name: None` без `expected_record_type` и без spread даёт
**жёсткую ошибку** «anonymous record literal without spread not
supported in codegen». То есть **каждый** record-литерал обязан
разрешиться в номинальный тип — анонимные record'ы не являются
first-class значениями.

**Это делает полное удаление `TypeName { }` чистым решением:** литерал
*и так* всегда нуждается в разрешимом типе; убирая префикс, мы лишь
переносим «написание имени» в аннотацию/сигнатуру. Ничего не ломается
концептуально — анонимных record-значений, которые мы могли бы потерять,
просто не существует.

### Цель

**Один каноничный синтаксис record-литерала — `{ ... }`.** Номинальный
тип всегда выводится из контекста. `TypeName { ... }` удаляется из языка.
D55 реализован полностью — во всех 6 позициях, для record- **и**
sum-coercion.

---

## Архитектурное решение

### Resolution pass вместо разрозненного контекст-стека

Сейчас codegen тащит target-тип ad-hoc через `expected_record_type`
(save/restore вокруг `emit_expr`). Это хрупко и покрывает 2 позиции.

**Production-решение:** пасса разрешения типов record-литералов,
интегрированная в type-checker. Type-checker и так обходит выражения
с контекстом ожидаемого типа для проверок — он же **back-fill'ит**
`RecordLit.type_name: None → Some(resolved_path)`. AST у bootstrap общий
и мутабельный (`infer_effects` уже его мутирует) — back-fill `type_name`
естественен.

После пассы:
- **каждый** `RecordLit` имеет `type_name: Some(...)`;
- codegen всегда читает `type_name` — поле `expected_record_type`, его
  save/restore-обвязка и ветка «typeless + expected» **удаляются**
  (codegen упрощается);
- ветка `else → error` остаётся как internal-error safety net (для
  well-typed программ недостижима).

Единственный источник истины «какого типа этот литерал» — одна пасса,
а не контекст, размазанный по codegen.

### Полное удаление `TypeName { }` (рекомендация, решается в Ф.0)

Рекомендация — **полное удаление**, не «убрать где есть target, оставить
где нет». Обоснование: анонимных record-значений нет (см. выше), значит
«позиция без target-типа» сегодня и так требует `TypeName { }` ИЛИ
не компилируется. После унификации такая позиция требует **аннотации**
(`let x T = { ... }` вместо `let x = TypeName { ... }`). Спека D55 уже
говорит: позиция без явного типа → coercion не применяется. Аннотация
в let / сигнатуре — естественное «явное место для типа».

Альтернатива (оставить `TypeName { }` для no-context позиций) —
сохраняет TIMTOWTDI и противоречит цели. Отвергается, если Ф.0 census
не вскроет позицию, где аннотацию физически некуда поставить.

---

## Фазы

### Ф.0 — Census + design decision + spec

- **Census:** grep всех `TypeName { }` сайтов в `std/*`, `nova_tests/*`,
  `examples/*`. Категоризовать по позиции: return / let / аргумент /
  элемент массива / match-arm / вложенное поле / generic / **no-context**.
  Точный масштаб миграции (предв. оценка — 150-300 сайтов).
- **No-context аудит:** для каждого сайта без выводимого target-типа —
  подтвердить, что чинится аннотацией. Если найдётся позиция, где
  аннотацию поставить негде — это hard blocker, поднять явно, не
  заметать.
- **Sum-coercion аудит:** D55 покрывает и sum-конструкторы
  (`let a StrOrInt = "test"` → `S("test")`). Проверить полноту
  реализации sum-coercion по тем же 6 позициям. Gaps → в Ф.1/Ф.2;
  если полно — зафиксировать. (Синтаксическая унификация — про record;
  «полный D55» — про обе половины.)
- **Решение:** полное удаление `TypeName { }` vs partial. Рекомендация —
  полное.
- **Spec:** ревизия D55 — `{ ... }` каноничен, `TypeName { }` удалён;
  зафиксировать правило унификации. Folds in: когда `=>`-тело — record-
  литерал, return-тип **обязателен** (нет return-типа → нет контекста →
  ошибка; это покрывает форму `fn f() => T { }` из обсуждения).

**Acceptance:** census-таблица; решение зафиксировано; D55 обновлён;
sum-coercion статус ясен.

### Ф.1 — Type-checker: expected-type propagation + resolution pass

Самая объёмная и рискованная фаза.

- Построить/расширить bidirectional «expected type» propagation: на
  каждом `RecordLit` вывести target-тип из объемлющего контекста:
  - return-тип функции (`=>` тело и хвост блок-тела);
  - аннотация `let x T = ...`;
  - тип параметра на call-site `f({ ... })`;
  - тип элемента `[]T` (литерал массива с известным элементом);
  - результат match-arm с фиксированным типом ветки;
  - тип объявленного поля для **вложенных** `{ outer: { inner } }`;
  - generic-параметр после конкретизации.
- **Resolution pass** back-fill'ит `RecordLit.type_name: None → Some(path)`.
- Валидация полей typeless-литерала против разрешённого типа (имеющиеся
  проверки — теперь у них всегда есть тип).
- Чёткая ошибка, если target-тип не выводится: «cannot infer record
  type — add a type annotation».

**Риск:** bootstrap type-checker — «best-effort permissive»; неизвестно,
хватает ли в нём type-info для робастной propagation во всех 6 позициях
(особенно вложенность + generic). Ф.0 census показывает глубину
проблемы. Если для позиции propagation не выходит робастной — это
blocker, поднять, не упрощать молча.

**Acceptance:** после пассы каждый `RecordLit` в полном regression-
корпусе имеет `type_name: Some(...)` (debug-assert); type-checker
проходит.

### Ф.2 — Codegen: читать разрешённый `type_name`, удалить контекст-стек

- После Ф.1 `RecordLit.type_name` всегда `Some`. Удалить из codegen:
  поле `expected_record_type`, его save/restore в `emit_fn_body` /
  `emit_method_body` / const-decl, ветку «typeless + expected_record_type».
- Ветка `else → error` остаётся как internal-error (для well-typed
  программ недостижима).
- spread-ветка (`{ ...p, y }`) — отдельный механизм, не трогается
  (инференс из var_types).

**Acceptance:** codegen упрощён, `expected_record_type` удалён;
regression-корпус компилируется.

### Ф.3 — Parser/check: запрет `TypeName { }`

- Парсер по-прежнему **распознаёт** `TypeName { }` (чтобы дать хорошую
  ошибку), но check отклоняет: «record literals are written `{ ... }` —
  the type is inferred from context (here: `T`)». Либо парсер отклоняет
  прямо в `parse_primary` / `parse_record_lit_after_path` при непустом
  `path`. Точку выбрать по Ф.0.
- Чёткая ошибка для `{ ... }` без выводимого target-типа.

**Acceptance:** `TypeName { }` → ошибка с подсказкой; `{ ... }`
без контекста → ошибка с подсказкой про аннотацию.

### Ф.4 — Миграция

- Переписать **все** `TypeName { }` → `{ ... }` в `std/*`,
  `nova_tests/*`, `examples/*`.
- Где target-тип не выводится — добавить аннотацию (`let x T = { ... }`).
- Механические случаи (`=> T { }`, `= T { }`) — скриптом; каждый
  результат верифицировать компиляцией.
- Коммитить пачками (std / nova_tests / examples) с прогоном регрессии.

**Acceptance:** ни одного `TypeName { }` в репозитории; полный
regression компилируется.

### Ф.5 — Тесты

- **Positive:** typeless `{ ... }` в каждой из 6 D55-позиций — return,
  let-аннотация, аргумент, элемент `[]T`, match-arm, вложенное поле,
  generic после конкретизации.
- **Negative (`EXPECT_COMPILE_ERROR`):**
  - `TypeName { ... }` — отклоняется;
  - `{ ... }` в позиции без выводимого target-типа — чёткая ошибка;
  - `fn f() => { ... }` без return-типа — ошибка (нет контекста).
- Sum-coercion тесты — если Ф.0 вскрыл gaps.

### Ф.6 — Регрессия + docs + commit

- Полный `nova test` (release) — без новых FAIL.
- `project-creation.txt` + `simplifications.md` записи.
- discussion-log private-репы.
- Каждая фаза — отдельный commit.

---

## Critical files

| Файл | Действие |
|---|---|
| `spec/decisions/02-types.md` (D55) | Ф.0 — ревизия: `{ }` каноничен |
| `compiler-codegen/src/types/mod.rs` | Ф.1 — expected-type propagation + resolution pass back-fill `type_name` |
| `compiler-codegen/src/codegen/emit_c.rs` | Ф.2 — читать `type_name`, удалить `expected_record_type` |
| `compiler-codegen/src/parser/mod.rs` | Ф.3 — запрет непустого `path` у record-литерала |
| `std/**/*.nv`, `nova_tests/**/*.nv`, `examples/**/*.nv` | Ф.4 — миграция |
| `nova_tests/types/*` (+ новые) | Ф.5 — positive + negative |

---

## Acceptance criteria (production-grade)

- [ ] D55 реализован полностью — record-coercion во всех 6 позициях
      спеки; sum-coercion аудирован и (если были gaps) доделан.
- [ ] `RecordLit.type_name` после resolution pass всегда `Some(...)`
      для well-typed программ.
- [ ] `TypeName { }` удалён из языка — компилятор отклоняет с подсказкой.
- [ ] `{ ... }` без выводимого target-типа — чёткая ошибка про аннотацию.
- [ ] `expected_record_type` контекст-стек удалён из codegen.
- [ ] Вся кодовая база (`std`/`nova_tests`/`examples`) мигрирована —
      ноль `TypeName { }`.
- [ ] Positive-тесты на все 6 позиций + negative на запрещённые формы.
- [ ] Полный release-regression без новых FAIL.

---

## Риски / trade-offs

- **Ф.1 — главный риск.** Робастность expected-type propagation в
  bootstrap type-checker'е (вложенность, generic) — неизвестна до Ф.0
  census. Если позиция не покрывается робастно — hard blocker, поднять
  явно (прецедент честного re-defer — Plan 42.14 → 42.15; но цель тут —
  именно полнота, не половинчатость).
- **Масштаб миграции** — 150-300 сайтов. Механически, но объёмно;
  пачками + регрессия после каждой.
- **Behavior change** — код, полагавшийся на `TypeName { }` в позиции
  без контекста, потребует добавления аннотации. Ф.0 census оценивает
  объём; ожидается мало.
- **Не делаем half-measure.** Узкая версия (запрет только в `=>`-теле)
  была обсуждена и **отвергнута**: она создаёт новую непоследовательность
  («`=>`-тело — единственное место, где префикс запрещён, а не
  опционален»). Plan 51 — это «правильное решение, не минимальное».

---

## Что НЕ входит

- **Анонимные record-типы как first-class значения** — их нет сейчас
  (codegen ошибается), и Plan 51 их не вводит. Каждый литерал
  по-прежнему обязан разрешиться в номинальный тип — просто имя теперь
  всегда из контекста.
- **Structural typing / row polymorphism** — вне scope, D5/D55 не про это.
- **Spread-инференс** (`{ ...p, y }`) — отдельный рабочий механизм,
  не трогается.

---

## Связь

- [D55](../../spec/decisions/02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
  — спецификация (будет ревизована в Ф.0).
- [D5](../../spec/decisions/02-types.md) — отказ от `pub`-granularity /
  structural typing (контекст «почему номинально»).
- Plan 42.3 / 42.7 — прецеденты отказа от TIMTOWTDI-синтаксиса.
