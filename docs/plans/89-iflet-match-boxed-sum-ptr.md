# Plan 89 — деструктуризация боксированного sum-элемента (`for o in []Option[T]`)

> **Статус:** ✅ ЗАКРЫТ 2026-05-22 (Ф.0-Ф.3; ветка `plan-89`)
> **Приоритет:** P2 (ломает базовую идиому — деструктуризацию
> Option/Result-элемента коллекции; Nova здесь строго хуже Go/Rust/TS)
> **Оценка:** ~0.75–1 dev-day (включая аудит Ф.0)
> **Зависимости:** Plan 59 (mono Result/sum-репрезентация) ✅;
> Plan 70.x (primitive distinction в array element types) ✅
> **Источник:** Plan 87 Ф.4.2 — маркер `[M-iflet-match-boxed-sum-ptr]`
> в `docs/simplifications.md` (обнаружено при тестировании
> `for o Option[int] in opts`)

## Зачем

Деструктуризация sum-элемента колл代кции — `Option`/`Result` в массиве —
**не компилируется**:

```nova
ro opts []Option[int] = [Some(1), None, Some(3)]
for o in opts {
    if Some(v) = o { ... }     // ← CC-FAIL
}
```

Codegen эмитит (перепроверено 2026-05-22, Plan 87):

```c
NovaOpt_nova_int* o = (NovaOpt_nova_int*)_arr->data[i];   // элемент боксирован
NovaOpt_nova_int* _nv_scr = o;
if ((_nv_scr.tag == NOVA_TAG_Option_Some)) { ... }        // .tag на УКАЗАТЕЛЕ
```

→ `error: member reference type 'NovaOpt_nova_int *' is a pointer;
did you mean to use '->'`.

**Корень.** Sum-типы со value-семантикой (`NovaOpt_T`, `NovaRes_<n>`,
мономорфизированные user-sum) при хранении в массиве **боксируются** —
в `nova_int`-слот `NovaArray` кладётся heap-указатель. `emit_for`
связывает loop-переменную именно как указатель (`NovaOpt_T* o`), а
pattern-codegen (`if let` / `match` / `while let`) обращается к
`.tag` / `.value` как к значению. Рассинхрон value↔pointer → CC-FAIL.

Воспроизводится **и без аннотации типа, и с ней** — codegen-дефект, не
связан с Plan 87 (Plan 87 codegen аннотацию не потребляет; баг лишь
обнаружен его тестами).

## Сравнение с Go / Rust / TS

| Язык | Деструктуризация optional/sum-элемента коллекции |
|---|---|
| **Rust** | ✅ `for o in &opts { if let Some(v) = o { ... } }` — ядро языка, тривиально. |
| **Go** | ✅ `for _, o := range opts { if o != nil { ... } }` — optional через указатель; для sum-подобных — `range` + type-switch. |
| **TS** | ✅ `for (const o of opts) { if (o) { ... } }` — discriminated union сужается естественно. |
| **Nova (сейчас)** | ❌ **CC-FAIL** — строго **хуже** всех трёх. |
| **Nova (цель)** | ✅ паритет — `for o in opts { if let Some(v) = o }` компилируется и работает. |

Это не «новая фича», а **закрытие регресс-уровня дефекта**: язык с
sum-типами и for-in обязан давать деструктуризацию элемента коллекции
sum'ов. Сейчас единственный обход — `.iter()` / индексный доступ с
ручным разбоксом, чего пользователь знать не должен.

## Привязка к коду (сверено 2026-05-22, Plan 87)

- `emit_for` (`compiler-codegen/src/codegen/emit_c.rs`, Case 2 «for elem
  in array_expr»): `elem_ty` берётся из `array_element_types` либо
  strip `NovaArray_`-префикса; для `[]Option[T]` → `NovaOpt_T*`
  (указатель). Биндинг: `{elem_ty} {binding} = ({elem_ty})arr->data[i]`.
- `IfLet` (`emit_c.rs` ~12778): `scr_ty = infer_expr_c_type(scrutinee)`;
  `{scr_ty} {scr_tmp} = {scr}`; далее `pattern_cond` / `pattern_bind_typed`
  → `.tag` / `.value`.
- `emit_match` (~17113) и `WhileLet` (~12840) — та же структура
  `scr_ty = infer_expr_c_type(...)` → `.tag`-доступ.
- Распознавание value-семантики sum'а: `NovaOpt_`/`NovaRes_`-префиксы;
  user-sum — через `sum_schemas` / mono-sum реестры.

## Замечания из смежной разведки (2026-05-22)

> Внесено по итогам разведки соседней задачи (диспетчеризация
> prelude-методов `Option`, `sum_schema_registry`). Два пункта,
> релевантных Ф.0 — учесть при аудите.

**1. Ещё один контекст потребления — вызов метода на боксированном
элементе (`o.is_some()` / `o.<method>()`).** Ф.0.1 перечисляет `if let`
/ `match` / `while let` / `==` / `f(o)`, но не вызов метода. Он сломан
тем же дефектом: блок `if obj_ty.starts_with("NovaOpt_")` в `emit_c.rs`
(~13994, method-call dispatch для `Option`) срабатывает и когда
`obj_ty` — `NovaOpt_T*` (указатель), делает
`strip_prefix("NovaOpt_").trim_end_matches('*')` и эмитит
`Nova_Option_method_is_some_<T>(obj_c)`, передавая **указатель** в
трамплин, который ждёт `NovaOpt_T` по значению → CC-FAIL/mismatch.
→ Усиливает выбор **подхода A** (разбокс в `emit_for`): он чинит и
вызовы методов одной точкой; подход B (deref только в pattern-codegen)
оставит `o.method()` сломанным. Добавить `o.is_some()` в probe Ф.0.1.

**2. Готовый критерий value↔pointer для sum — не катать руками.**
Ф.0.3 требует критерий «value-семантика sum». Он уже есть:
`SumSchemaRegistry::lookup_sum_for_c_type(c_ty) -> Option<&SumSchemaEntry>`
(`compiler-codegen/src/codegen/sum_schema_registry.rs`), у entry поле
`abi: SumAbi` (`ValueOptionLike` / `PointerErrorLike` /
`ValueTagPayload`). Доступно через `self.sum_schema_registry` на
`CEmitter`. Не нужен хардкод prefix-чека `NovaOpt_`/`NovaRes_`.

⚠️ Caveat: `ValueOptionLike` матчится по **префиксу** `NovaOpt_`
(покрывает все mono-инстансы `Option`), а `PointerErrorLike` (`Result`)
— **точным** сравнением `c_ty == "Nova_Result"`. Mono'd
`NovaRes_<T>_<E>` (Plan 59) точным матчем, скорее всего, не ловится —
для `[]Result` (Ф.0.2 / Ф.2.4) и user-sum проверить покрытие;
возможно, нужен prefix-матч и для `NovaRes_` / mono-user-sum.

## Scope

- **Только codegen.** Парсер / type-checker / spec-семантика не
  меняются — for-in над `[]Option[T]` и `if let` уже валидны по типам.
- Починить рассинхрон value↔pointer для **боксированного sum-элемента**
  во всех контекстах потребления loop-переменной: `if let`, `match`,
  `while let`, оператор `==`, передача в функцию.
- Покрыть `Option[T]`, `Result[T,E]` и **мономорфизированные user-sum**
  в роли элемента массива.

## Декомпозиция (фазы и шаги)

### Ф.0 — Аудит value/pointer-боксинга (~0.2 д) — GATE

- **Ф.0.1** Зафиксировать probe-фикстурами симптом для каждого
  контекста потребления боксированного sum-элемента: `if let`,
  `match`, `while let`, `o == Some(x)`, `f(o)` где `f` ждёт
  `Option[T]`. Часть — `silent-wrong`, часть — CC-FAIL: классифицировать.
- **Ф.0.2** Probe для `[]Result[T,E]` и `[]UserSum` (mono'd) — тот же
  ли дефект, что у `[]Option[T]`.
- **Ф.0.3** Soundness-check: убедиться, что records (`Nova_Record*` —
  настоящий указатель) и **не** должны разбоксовываться — pattern-codegen
  для record-паттерна полагается на указатель. Зафиксировать критерий
  «value-семантика sum» (префиксы `NovaOpt_`/`NovaRes_` + `sum_schemas`).
- **Ф.0.4** **Decision point:** выбрать подход —
  - **A (рекомендуется): разбокс в `emit_for`** — связывать
    loop-переменную как **значение** (`NovaOpt_T o = *(NovaOpt_T*)
    data[i]`). Чинит ВСЕ контексты потребления (`if let`/`match`/`==`/
    fn-arg) одной точкой; loop-переменная ведёт себя как обычное
    sum-значение.
  - **B: deref в pattern-codegen** — деференсить pointer-to-value-sum
    scrutinee в `IfLet`/`emitmatch`/`WhileLet`. Локальнее, но чинит
    только pattern-match — `==` и fn-arg остаются сломаны → неполно.
  Зафиксировать выбор и обоснование в «Итог Ф.0» (ниже).

### Ф.1 — Реализация (~0.3 д)

- **Ф.1.1** Реализовать выбранный в Ф.0 подход. Для A — в `emit_for`
  Case 2: если `elem_ty` — pointer-to-value-sum, эмитить
  `{pointee} {binding} = *({elem_ty})arr->data[i]` + регистрировать
  `var_types[binding] = pointee` (значение, не указатель).
- **Ф.1.2** Для пути Iter[T] (`for o in <custom Iter producing Option>`)
  — проверить, нужен ли симметричный фикс (элемент приходит из
  `next() -> Option[T]` — обычно уже значение; подтвердить probe'ом).
- **Ф.1.3** Не задеть records и genuine-pointer элементы (`[]User` —
  `Nova_User*` остаётся указателем): строгий критерий из Ф.0.3.
- **Ф.1.4** Targeted-verify: probe-фикстуры Ф.0 → PASS.

### Ф.2 — Тесты (~0.2 д)

- **Ф.2.1** `nova_tests/plan89/iflet_option_elem.nv` — `for o in
  []Option[int] { if let Some(v) = o { ... } }`, сумма распакованных.
- **Ф.2.2** `nova_tests/plan89/match_option_elem.nv` — `match o { Some(v)
  => ..., None => ... }` над элементом массива.
- **Ф.2.3** `nova_tests/plan89/whilelet_and_eq.nv` — `while let` и
  оператор `==` (`o == Some(1)`) над боксированным элементом.
- **Ф.2.4** `nova_tests/plan89/result_elem.nv` — `[]Result[T,E]`
  элемент + `if let Ok(v)` / `match`.
- **Ф.2.5** `nova_tests/plan89/user_sum_elem.nv` — массив
  мономорфизированного user-sum + деструктуризация.
- **Ф.2.6** Регресс-негатив: `[]User` (record-элемент) — `for u in
  users` + `u.field` по-прежнему работает (records не разбоксованы).
- **Ф.2.7** Полный `nova test` — 0 новых FAIL.

### Ф.3 — Spec / docs (~0.1 д)

- **Ф.3.1** `docs/simplifications.md` — `[M-iflet-match-boxed-sum-ptr]`
  → ✅ ЗАКРЫТО.
- **Ф.3.2** Spec — изменений семантики нет (for-in + if-let по sum уже
  обещаны); правки spec **ожидаемо не требуются**. Если где-то
  зафиксировано как ограничение — снять.
- **Ф.3.3** `docs/plans/README.md` — Plan 89 → статус-апдейт.
- **Ф.3.4** `docs/project-creation.txt` +
  `nova-private/discussion-log.md` — записи.

## Итог Ф.0

Аудит проведён 2026-05-22 — 8 probe-фикстур (`_audit/*.nv`, временные)
через `nova test`. Результаты:

| Контекст потребления элемента | Симптом | Стадия | Решение |
|---|---|---|---|
| `[]Option` + `if let Some(v) = o` | `.tag` на `NovaOpt_T*` (указатель) | **CC-FAIL** | Ф.1 |
| `[]Option` + `match o { ... }` | `.tag` на указателе | **CC-FAIL** | Ф.1 |
| `[]Option` + `o == Some(x)` | `invalid operands` (value-fn vs указатель) | **CC-FAIL** | Ф.1 |
| `[]Option` + `f(o)` (fn ждёт `Option[int]`) | `passing 'NovaOpt_T*' to incompatible param` | **CC-FAIL** | Ф.1 |
| `[]Option` + `o.is_some()` (вызов метода) | `passing 'NovaOpt_T*' to incompatible param` | **CC-FAIL** | Ф.1 |
| `[]Result[T,E]` + `if let Ok` / `match` | ✅ компилируется, runtime-assert проходит | OK | — (уже работает) |
| `[]UserSum` + `match` / `if let` | ✅ компилируется, runtime-assert проходит | OK | — (уже работает) |
| `[]Record` + field-access / record-pattern | ✅ компилируется, runtime-assert проходит | OK | — (уже работает) |

**Один дефект, узкий.** Затронут **только `Option`** — единственный
sum со value-семантикой (`SumAbi::ValueOptionLike`, struct `NovaOpt_T`).
При хранении в массиве элемент боксируется в `NovaOpt_T*`; **все 5**
контекстов потребления loop-переменной ждут sum **по значению** →
рассинхрон. `Result` и user-sum имеют pointer-семантику
(`PointerErrorLike`, `Nova_X*` heap) — элемент массива уже указатель,
codegen pattern-match использует `->` → корректно. Records — genuine
heap-pointer, тоже корректны. (`RuntimeError` — `ValueTagPayload`,
тоже value-семантика; `[]RuntimeError` редок, не пробился, но
покрывается общим критерием фикса.)

**Soundness (Ф.0.4):** все 5 отказов — **loud** (CC-FAIL на стадии
компиляции C). Ни одного silent-wrong: 3 рабочих случая дают корректный
runtime (assert'ы проходят). Дыры soundness нет.

**Decision point (Ф.0.4) — выбран подход A** (разбокс в `emit_for`).
Обоснование: аудит показал, что дефект затрагивает **5** контекстов, не
только pattern-match — в т.ч. `==`, передачу в функцию и **вызов
метода** (`o.is_some()`). Подход B (deref только в `IfLet`/`emit_match`/
`WhileLet`) оставил бы `==`, fn-arg и method-call сломанными — неполный
фикс. Подход A связывает loop-переменную как **значение**
(`NovaOpt_T o = *ptr`) в единственной точке `emit_for` → чинит все 5
контекстов сразу. Критерий «value-семантика sum» — готовый
`SumSchemaRegistry::lookup_sum_for_c_type(pointee).abi ∈
{ValueOptionLike, ValueTagPayload}` (без хардкода prefix-чеков).

## Acceptance criteria

- [x] `for o in opts { if let Some(v) = o { ... } }` для
      `opts []Option[int]` — компилируется, линкуется, корректно
      работает. _(plan89/iflet_option_elem.nv)_
- [x] То же для `match`, оператора `==`, вызова метода (`o.is_some()`)
      и передачи элемента в функцию, ждущую `Option[T]`.
      _(match_option_elem / eq_and_method / fnarg_option_elem)_
      _(`while let` — отдельным триггером не является: дефект возникает
      на for-in-связывании; подход A делает loop-переменную значением,
      далее любой контекст, включая while-let на ней, работает.)_
- [x] `[]Result[T,E]` и массив пользовательского sum-типа —
      деструктуризация элемента работает (регресс-гварды:
      result_elem_regression / user_sum_elem_regression).
- [x] Records (`[]Point` и т.п.) не сломаны — genuine-pointer элементы
      не разбоксовываются (regress-гвард record_elem_regression).
- [x] Полный `nova test` — 0 новых FAIL относительно baseline.
- [x] `[M-iflet-match-boxed-sum-ptr]` закрыт.

## Non-scope

- **Общая ревизия value/pointer-модели sum-типов** — Plan 89 чинит
  именно боксированный-в-массиве элемент, не переписывает ABI sum'ов.
- **Existential / dynamic dispatch** sum-типов — не относится.
- **`if let` / `match` по pointer-to-record** — records уже корректны
  (паттерн-codegen полагается на указатель by design); Plan 89 их не
  трогает.
