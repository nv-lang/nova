# Plan 51: Record-literal — тип пишется ровно один раз

> **Создан 2026-05-15, ревизия 2026-05-15** (после investigation — scope
> сокращён с «полная унификация D55» до реальной задачи; затем расширен
> с `=>`-тела на `let`).
>
> **СТАТУС:** ✅ ЗАКРЫТ — Ф.1-Ф.5 реализованы и смёржены в `main`
> (коммиты `658954c`..`a1c7459`, 2026-05-16).
>
> Устраняет единственную живую TIMTOWTDI в записи record-литералов: тип
> не должен дублироваться. Принцип — **тип появляется ровно один раз**.

---

## Правило

**Record-литерал несёт имя типа только если тип не объявлен рядом.**

### Позиция `=>`-тело функции

Когда тело функции — **сразу** `=>` с record-литералом, тип берётся из
return-аннотации, литерал — без префикса:

- ✅ `fn f() -> T => { ... }` — каноничная форма;
- ❌ `fn f() -> T => T { ... }` — тип дважды;
- ❌ `fn f() => T { ... }` — нет return-типа, тип «спрятан» в литерале;
- ✅ `fn f() -> Result[User,E] => User { ... }` — тип литерала ≠
  return-тип: sum-coercion (D55), `User` нужен, правило **не** срабатывает.

`-> Self` резолвится к типу receiver'а: `fn Counter @clone() -> Self =>
Counter { ... }` — тоже избыточно (`Self` ≡ `Counter`).

### Позиция `let`

Аннотация `let` честно опциональна — тип может быть либо в аннотации,
либо в литерале, но **не дважды**:

- ✅ `let x T = { ... }` — тип в аннотации;
- ✅ `let x = T { ... }` — тип в литерале (нет аннотации);
- ❌ `let x T = T { ... }` — тип дважды.

То же для `let mut`.

### Почему асимметрия `=>` (обязывает сигнатуру) vs `let` (или-или)

Не TIMTOWTDI: `let x = ...` vs `let x T = ...` — это общая, всегда
доступная опция «аннотировать let», ортогональная record-литералам. Сам
литерал имеет **ровно одну форму на контекст**. В `=>` тип обязан быть в
сигнатуре, потому что сигнатура функции должна быть полной (дух D62);
`let` локален, аннотация опциональна. Асимметрия принципиальная.

---

## Почему этот scope (и почему НЕ «полная унификация D55»)

Investigation (пробники + census, 2026-05-15) показал, что широкий
вариант — «typeless везде, запретить `TypeName { }` глобально» — не
оправдан:

- **D55 реально работает только в `=>`-return** (пробники: `let x T={}`,
  `f({})`, `[]T`-элементы → hard codegen-fail).
- **«~900 избыточных мест» — переоценка.** Реально: `-> T => T{}` (тип
  **дважды**) — **~24 сайта**; `let x = Type{}` (тип **один раз**) —
  ~96, «унификация» там лишь **переместила** бы имя типа, не убрала;
  `Variant{}` (конструкторы вариантов sum-типа) — имя обязательно,
  мигрировать нельзя.
- Широкий вариант = рискованный bidirectional type-checker-проход +
  миграция сотен мест + правка AST — ради проблемы, которой по сути нет.

**Узкий scope бьёт по живой проблеме:** дублирование имени типа.
`=>`-тело и `let` — единственные места, где тип реально дублируется
(или может). Полная реализация D55 (typeless во всех 12 позициях) —
отдельный вопрос, не блокирует это.

---

## Почему это безопасно

- `=>`-тело typeless (`fn f() -> T => { ... }`) **уже компилируется**
  (пробник `p1_ret` — PASS). Миграция `=>`-сайтов = убрать префикс.
- `let x T = { }` сейчас codegen-fail'ит, **но фикс — ~4 строки**:
  обернуть emit значения в `expected_record_type`, дословно как уже
  сделано для `const` ([emit_c.rs:1304-1307](../../compiler-codegen/src/codegen/emit_c.rs)).
  `Stmt::Let` уже вычисляет `ty_c` из аннотации
  ([emit_c.rs:4947](../../compiler-codegen/src/codegen/emit_c.rs)).
  Type-checker уже пропускает `let x T = { }` — менять не надо.
- `let x T = T { }` — 0 сайтов в коде, чистая parser-проверка вперёд.
- **Нет** рискованного bidirectional-прохода, **нет** изменений AST,
  **нет** изменений type-checker'а.

---

## Фазы

### Ф.1 — Codegen: включить `let x T = { }`

`Stmt::Let` emit ([emit_c.rs:4932](../../compiler-codegen/src/codegen/emit_c.rs)):
если `decl.ty.is_some()` — обернуть `emit_expr_with_target_type` в
save/set/restore `expected_record_type` (из `ty_c`), mirror `const`.
~4 строки.

**Acceptance:** `let x T = { ... }` компилируется и даёт правильный
struct (пробник `p2_let` — PASS).

### Ф.2 — Parser: запрет дублирования типа

`parse_fn` (после разбора `return_type` + тела) и `parse_let_decl`:

- **`=>`-тело** = `RecordLit { type_name: Some(path) }`:
  - нет `return_type` → ошибка «function returning a record literal
    must declare its return type: `fn f() -> T => { ... }`»;
  - `return_type` `-> T`, `path` == `T` → ошибка «redundant type prefix
    — `-> T` already declares the type; write `=> { ... }`»;
  - `return_type` `Self` + receiver, `path` == receiver-тип → та же
    ошибка (резолвим `Self`);
  - `path` ≠ `T` → не трогаем (sum-coercion).
  - Сравнение по path-сегментам; генерики return-типа игнорируем.
- **`let`** decl: аннотация `Some(T)` И `value` = `RecordLit
  { type_name: Some(path) }` И `path` == `T` → ошибка «redundant type
  prefix — `let x T` already declares the type; write `= { ... }`».

~40 LOC + внятные ошибки.

### Ф.3 — Миграция (~24 `=>`-сайта; 0 `let`-сайтов)

`-> T => T { fields }` → `-> T => { fields }`; `-> Self => Receiver{...}`
→ `-> Self => { ... }`. Список (census 2026-05-15):

- `std/testing/property.nv` — BoolGen.new
- `nova_tests/types/generic_bounds.nv` — GbValue @from_key
- `nova_tests/syntax/compound_assign.nv` — Counter.new, State.new
- `nova_tests/syntax/const_complex.nv` — make_point, scale
- `nova_tests/syntax/method_values.nv` — Counter.new
- `nova_tests/syntax/methods.nv` — Point @scale, Point.origin, Point.from_pair
- `nova_tests/syntax/overload_method_values.nv` — Buf.new
- `nova_tests/concurrency/deep_gc.nv` — make_point, make_box, make_triple
- `nova_tests/concurrency/deep_spawn.nv` — make_counter, make_stage
- `nova_tests/concurrency/detach_test.nv` — make_counter
- `nova_tests/concurrency/supervised_errors.nv` — ok_status, err_status
- `nova_tests/runtime/clone_semantics.nv` — Counter @clone, Outer @clone (Self-форма)
- `nova_tests/plan34/inline_mut_clock_advance.nv` — clock_new
- `examples/effects/gc_coroutines_test.nv` — make_point, make_tree

**НЕ трогать** (sum-coercion, `path ≠ return`):
`nova_tests/syntax/is_sum.nv` — `-> Shape => Circle{}`, `-> Shape => Square{}`.

Верификация компиляцией, закоммитить пачкой.

### Ф.4 — Тесты

- **Positive:** `fn f() -> T => { ... }`; `let x T = { ... }`;
  `let x = T { ... }` — все три каноничные формы.
- **Negative (`EXPECT_COMPILE_ERROR`):**
  - `fn f() -> T => T { ... }` — «redundant type prefix»;
  - `fn f() => T { ... }` — «must declare its return type»;
  - `let x T = T { ... }` — «redundant type prefix»;
- **Anti-regression positive:** `fn f() -> Sum => Variant { ... }`
  (path ≠ return) — **компилируется** (sum-coercion не сломан).

### Ф.5 — Spec + регрессия + docs

- `spec/decisions/03-syntax.md` (или D55 в `02-types.md`) — зафиксировать
  правило «тип ровно один раз» для `=>`-тела и `let`.
- **Honesty-fix:** D55 обещает coercion в 6 позициях — реально `=>`-return
  (+ const, + теперь `let`-аннотация). Добавить честную пометку: прочие
  позиции — not yet implemented (урок Plan 42.17 — спека не врёт).
- Полный `nova test` (release) — без новых FAIL.
- `project-creation.txt` + discussion-log.
- Каждая фаза — отдельный commit.

---

## Acceptance criteria

- [ ] `let x T = { ... }` компилируется (Ф.1).
- [ ] `fn f() -> T => T { ... }` → compile error.
- [ ] `fn f() => T { ... }` (без return-типа) → compile error.
- [ ] `let x T = T { ... }` → compile error.
- [ ] `-> Self => Receiver { ... }` ловится (Self резолвится).
- [ ] `fn f() -> Sum => Variant { ... }` (sum-coercion) — **не** ломается.
- [ ] `let x = T { ... }` — без изменений, работает.
- [ ] Все ~24 `=>`-сайта мигрированы; `is_sum.nv` не тронут.
- [ ] Positive + negative тесты обеих позиций.
- [ ] Spec обновлён; D55 honesty-пометка добавлена.
- [ ] Полный release-regression без новых FAIL.

---

## Что НЕ входит

- **Полная реализация D55** (typeless в `f(...)`, `[]T`-элементах,
  match-arm и т.д.) — отдельный вопрос; `let`-аннотация включается здесь
  только потому, что это ~4 строки (mirror `const`).
- **Запрет `TypeName { }` вне `=>`-тела и `let`** — отвергнут
  investigation'ом (переезд имени, не устранение; конструкторы вариантов).
- **Изменения AST / type-checker** — не нужны.
- **Assignment `x = { }`** к типизированной переменной — не в scope.

---

## Связь

- [D55](../../spec/decisions/02-types.md#d55-literal-coercion-в-позиции-с-явным-типом-sum-конструкторы-и-record-литералы)
  — literal coercion (honesty-fix в Ф.5).
- Plan 42.3 / 42.7 — прецеденты отказа от TIMTOWTDI-синтаксиса.
- Plan 42.17 — урок «спека не должна врать» (Ф.5 honesty-fix).
