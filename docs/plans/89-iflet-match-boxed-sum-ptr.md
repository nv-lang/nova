# Plan 89 — деструктуризация боксированного sum-элемента (`for o in []Option[T]`)

> **Статус:** 📋 proposed 2026-05-22 (production-grade), не начат
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
let opts []Option[int] = [Some(1), None, Some(3)]
for o in opts {
    if let Some(v) = o { ... }     // ← CC-FAIL
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

> Заполняется по результатам аудита: таблица «контекст → симптом
> (CC-FAIL / silent-wrong) → починен Ф.1». Выбор подхода A/B +
> обоснование. До аудита раздел пуст.

## Acceptance criteria

- [ ] `for o in opts { if let Some(v) = o { ... } }` для
      `opts []Option[int]` — компилируется, линкуется, корректно
      работает.
- [ ] То же для `match`, `while let`, оператора `==` и передачи
      элемента в функцию, ждущую `Option[T]`.
- [ ] `[]Result[T,E]` и массив мономорфизированного user-sum —
      деструктуризация элемента работает.
- [ ] Records (`[]User` и т.п.) не сломаны — genuine-pointer элементы
      не разбоксовываются.
- [ ] Полный `nova test` — 0 новых FAIL относительно baseline.
- [ ] `[M-iflet-match-boxed-sum-ptr]` закрыт.

## Non-scope

- **Общая ревизия value/pointer-модели sum-типов** — Plan 89 чинит
  именно боксированный-в-массиве элемент, не переписывает ABI sum'ов.
- **Existential / dynamic dispatch** sum-типов — не относится.
- **`if let` / `match` по pointer-to-record** — records уже корректны
  (паттерн-codegen полагается на указатель by design); Plan 89 их не
  трогает.
