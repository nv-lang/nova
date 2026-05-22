# Plan 93 — `Option.is_some`/`is_none` как Nova-методы (`DeclaredBody` routing)

> **Статус:** 🔵 Ф.0 ВЫПОЛНЕН — **GATE-STOP 2026-05-22** (ветка
> `plan-93`). Ф.1–Ф.4 заблокированы: аудит показал, что чистая
> реализация требует инфраструктуры «Option/Result в generic-method-
> mono» — отдельная инициатива. См. «Итог Ф.0».
> **Приоритет:** P3 (de-magic / single-source-of-truth; функционального
> выигрыша нет — корректность не меняется, это чистота прелюдии)
> **Оценка:** ~1.5–2 dev-day (бóльшая часть — инфраструктура
> `DeclaredBody`-routing; сами тела тривиальны)
> **Зависимости:** Plan 62.A (Option-методы объявлены в
> `std/prelude/core.nv`) ✅; Plan 62.A.bis (sum-schema registry +
> `MethodRouting`) ✅ — Plan 93 оживляет незавершённый вариант
> `DeclaredBody`
> **Источник:** обсуждение 2026-05-22 — `is_some` хочется видеть
> Nova-телом `fn Option[T] @is_some() => match @ { Some(_) => true,
> None => false }`, а не C-трамплином.

## Зачем

`Option.is_some()` / `is_none()` — тривиальные предикаты, выразимые на
Nova одной строкой:

```nova
fn Option[T] @is_some() -> bool => match @ { Some(_) => true, None => false }
fn Option[T] @is_none() -> bool => match @ { Some(_) => false, None => true }
```

Сейчас они — **компилятор-магия**: объявлены `external fn` в
`std/prelude/core.nv`, тело — C-трамплины `Nova_Option_method_is_some_<T>`
в `nova_rt/array.h` (по одному на каждый примитив), маршрутизация —
`sum_schema_registry` → `MethodRouting::HardcodedRuntimeFn`. Прелюдия
показывает `external fn` без тела там, где тело тривиально и
самодокументируемо. Single-source-of-truth нарушен: «что делает
`is_some`» нельзя прочитать в `.nv` — надо лезть в C-рантайм.

**Это узкий, осознанный пересмотр Plan 78 Ф.1.** Plan 78 решил, что
`method_routing` — легитимный реестр C-реализации и **не** хардкод-
зеркало. Это верно для методов, которые **нельзя** выразить на Nova
(`unwrap` — нужен Fail-handler dispatch D65/Plan 61; `unwrap_or_else`/
`map`/`ok_or` — closure-applying, спец-codegen). Но `is_some`/`is_none`
— **чистые тотальные предикаты над тегом sum'а**, ровно `match`. Для
них C-трамплин — не «реализация, которой нет в Nova», а именно
устранимое зеркало. Plan 93 переносит **только** это подмножество;
остальной реестр Plan 78 не трогает.

## Сравнение с Go / Rust / TS

| Язык | `Option`-предикат |
|---|---|
| **Rust** | `Option::is_some` — обычная библиотечная функция в `core`: `pub const fn is_some(&self) -> bool { matches!(*self, Some(_)) }`. **Не** компилятор-интринсик — ядро языка написано на самом языке. |
| **Go** | нет `Option`; stdlib-предикаты — обычный Go-код, не магия компилятора. |
| **TS** | type-guards (`x !== undefined`) — обычный TS. |
| **Nova (сейчас)** | `is_some` — компилятор-магия (C-трамплин + `external fn`-заглушка). **Хуже Rust** — магия там, где не нужна. |
| **Nova (цель)** | `is_some`/`is_none` — обычные Nova-методы в `std/prelude/core.nv`, как `Option::is_some` в Rust `core`. |

Выигрыш — **не производительность** (Nova-тело `match` компилируется в
тот же `o.tag == NOVA_TAG_Option_Some`, что и трамплин), а
**de-magic**: меньше компилятор-специфики, прелюдия само-документируема,
паритет с Rust-моделью «ядро на самом языке».

## Привязка к коду (сверено 2026-05-22)

- **Объявление:** `std/prelude/core.nv` — `external fn Option[T]
  @is_some() -> bool` / `@is_none()`.
- **C-трамплины:** `compiler-codegen/nova_rt/array.h` —
  `Nova_Option_method_is_some_<T>` / `_is_none_<T>` (per-примитив:
  `nova_int`, `nova_str`, `nova_char`, `nova_byte`, `nova_f64`,
  `nova_f32`, `int8/16/32_t`, `uint16/32/64_t`).
- **Routing:** `sum_schema_registry.rs` `init_hardcoded_baseline()` —
  `option_methods.insert("is_some", HardcodedRuntimeFn { c_name:
  "Nova_Option_method_is_some", is_per_t: true })`.
- **Перехват вызова:** `emit_c.rs` method-dispatch — хардкод-ветка
  `obj_ty.starts_with("NovaOpt_")` (район ~14000) перехватывает
  `opt.is_some()` ДО обычного method-dispatch'а → роутит в трамплин;
  любое Nova-тело затеняется.
- **`MethodRouting::DeclaredBody { has_nova_body }`** — вариант enum'а
  для Nova-тельного метода; **сейчас scaffold-only** (нигде не
  конструируется, в `emit_c.rs` — лишь `// DeclaredBody fall through`).
  Plan 62.A.bis.4 «DeclaredBody method routing» так и не выполнен
  (метка переиспользована под `sum_schemas` removal).
- **`init_prelude_decls_from_items`** — берёт только `external fn`
  Option/Result-методы (`if !f.is_external { return None }`) →
  Nova-тельный метод в прелюдии сейчас не попадёт в registry.

## Scope

- `fn Option[T] @is_some() -> bool` и `@is_none()` — **Nova-тело** в
  `std/prelude/core.nv` (выражение `=> match @ { ... }`).
- Инфраструктура: builtin-метод sum-типа с Nova-телом маршрутизируется
  к телу, а не к C-трамплину — оживить `MethodRouting::DeclaredBody`.
- Снять `is_some`/`is_none` из `NovaOpt_`-хардкод-перехвата `emit_c.rs`.
- Удалить ставшие мёртвыми C-трамплины `Nova_Option_method_is_some_<T>`
  / `_is_none_<T>` (single-source — не оставлять зеркало).

## Декомпозиция (фазы и шаги)

### Ф.0 — Аудит + decision point (~0.3 д) — GATE

- **Ф.0.1** Подтвердить, что Nova-тело `is_some` (`=> match @ {...}`)
  корректно **мономорфизируется per-T**: проба-фикстура
  `fn Option[T] @is_some()` с телом, вызов на `Option[int]`/`[str]`/
  `[char]`/`[user-record]` — сверить сгенерированный C с baseline'ом
  C-трамплина (ожидаемо идентичная семантика `tag == Some`).
- **Ф.0.2** Проверить путь `DeclaredBody`: что нужно, чтобы
  `init_prelude_decls_from_items` зарегистрировал не-`external`
  Option-метод; как codegen эмитит тело (через mono-метод-путь /
  `emit_method_overload`). Зафиксировать пофайловую карту.
- **Ф.0.3 — decision point.** Подтвердить границу: переносятся
  **только** `is_some`/`is_none` (чистые предикаты). `unwrap`/
  `unwrap_or`/`unwrap_or_else`/`map`/`ok_or` — **остаются** как есть
  (`external fn` + трамплин/`<inline>`): `unwrap` требует Fail-dispatch,
  остальные — closure-applying спец-codegen. Раскол осознанный —
  обосновать в «Итог Ф.0», это граница, не упрощение. Если Ф.0 покажет,
  что `DeclaredBody`-путь требует непропорционально много работы —
  re-scope (зафиксировать).

### Ф.1 — `DeclaredBody` routing (~0.7 д)

- **Ф.1.1** `init_prelude_decls_from_items` — распознавать Nova-тельный
  (не-`external`) метод с receiver'ом-sum (Option) → регистрировать
  `MethodRouting::DeclaredBody`.
- **Ф.1.2** `emit_c.rs` — путь эмиссии `DeclaredBody`: метод
  инстанцируется per-T (mono) и вызывается как обычный mono'd метод;
  C-имя совместимо с диспатчем call-site.
- **Ф.1.3** Снять `is_some`/`is_none` из `NovaOpt_`-хардкод-перехвата —
  чтобы вызов дошёл до `DeclaredBody`-routing, а не до трамплина.
- **Ф.1.4** Targeted-verify: проба Ф.0.1 → метод реально идёт через
  Nova-тело (не трамплин) — проверить по сгенерированному C.

### Ф.2 — Перенос `is_some`/`is_none` (~0.3 д)

- **Ф.2.1** `std/prelude/core.nv` — заменить `external fn Option[T]
  @is_some()` / `@is_none()` на Nova-тело `=> match @ { ... }`.
- **Ф.2.2** Удалить C-трамплины `Nova_Option_method_is_some_<T>` /
  `_is_none_<T>` из `nova_rt/array.h` (все per-примитив варианты) +
  записи `is_some`/`is_none` из `init_hardcoded_baseline` option_methods.
- **Ф.2.3** Targeted-verify: `Option[int/str/char/user]`.

### Ф.3 — Тесты (~0.2 д)

- **Ф.3.1** `nova_tests/plan93/` позитив: `is_some`/`is_none` на
  `Option[int]`/`[str]`/`[char]`/`Option[user-record]`; в for-in над
  `[]Option[T]`; в теле generic-функции с `[T]`-bound.
- **Ф.3.2** Регресс: существующие потребители `is_some`/`is_none`
  (`nova_tests/plan89/eq_and_method.nv`, `plan62/option_methods_from_
  prelude.nv`, stdlib `json.nv`) — зелёные.
- **Ф.3.3** Полный `nova test` — 0 новых FAIL.

### Ф.4 — Spec / docs (~0.1 д)

- **Ф.4.1** `docs/simplifications.md` — отметить, что `is_some`/
  `is_none` де-магифицированы; если был маркер — закрыть.
- **Ф.4.2** Plan 78 — аменд: зафиксировать, что `is_some`/`is_none`
  выведены из C-реестра в Nova-тело (узкий пересмотр Ф.1, не отмена).
- **Ф.4.3** `docs/plans/README.md` — Plan 93 статус-апдейт.
- **Ф.4.4** `docs/project-creation.txt` +
  `nova-private/discussion-log.md` — записи.

## Итог Ф.0

Аудит проведён 2026-05-22 (worktree `nova-p93`). Probe-фикстуры +
чтение кода.

### Probe-результаты

| Probe | Что | Результат |
|---|---|---|
| `pu_sum` | user generic-sum `type Maybe[T] \| Just(T) \| Nope` + `fn Maybe[T] @present() => match @ { Just(_) => true, Nope => false }` | ✅ **PASS** — generic-method-mono с `match @`-телом для user sum-типов работает |
| `pu_opt` | user-метод `fn Option[T] @my_present() => match @ { Some(_) => true, None => false }` на builtin `Option` | ❌ **CC-FAIL** — `incomplete definition of type 'Nova_Option'` |

### Корень (доказан чтением кода)

`Option`/`Result` **намеренно исключены** из generic-type-механизма —
`emit_c.rs:1526-1528`:

```rust
// Plan 62.A: Option/Result handled via NovaOpt_<T> / Nova_Result* infra
// — не регистрируем как generic template. ... drain_generic_type_worklist
// создал бы Nova_Option____<T> heap-allocated form, не совпадающий с
// runtime helper signatures.
if t.name == "Option" || t.name == "Result" { continue; }
```

Следствие цепочкой:
- `Option` нет в `generic_types` → метод `fn Option[T] @m()` не попадает
  в `generic_type_methods` (`emit_c.rs:1558`) → идёт в non-generic
  method-путь → `receiver_c_type("Option")` (`emit_c.rs:7056`) →
  `format!("Nova_{}*", "Option")` = **`Nova_Option*`** — тип, который
  как struct **никогда не определяется** (реальная репрезентация
  `Option` — спец-value-struct `NovaOpt_<T>`). → `incomplete type` →
  CC-FAIL.
- `Option` имеет **двойную codegen-идентичность**: спец-value-тип
  `NovaOpt_<T>` (`register_novaopt_decl`, `nova_rt/array.h`) И —
  потенциально — generic-template. Они **не связаны**: mono-путь
  выдаёт `Nova_Option____<T>` / `Nova_Option*`, реальный тип —
  `NovaOpt_<T>`.

### Что потребовала бы полная реализация

Связать `NovaOpt_<T>`-идентичность `Option` с generic-method-mono — это
не точечный фикс, а спец-кейс `Option`/`Result`, протянутый сквозь
**~8 связанных мест** codegen'а: исключение `1526`, `receiver_c_type`,
`compute_generic_type_c_name`, `generic_types`/`generic_type_templates`/
`generic_type_methods` регистрация, `drain_generic_type_worklist`
(чтобы НЕ эмитил конфликтующий `Nova_Option____<T>`), mono-worklist,
координация с `register_novaopt_decl` lazy-emit + `sum_schema_registry`
routing + снятие `obj_ty.starts_with("NovaOpt_")`-перехвата + удаление
трамплинов из `array.h`. На **самом используемом типе языка**, с
риском регрессий по всему suite.

### Decision point (Ф.0.3) — GATE НЕ ПРОЙДЕН, re-scope

Plan 93 Ф.0.3 явно предусматривал: «если Ф.0 покажет, что
`DeclaredBody`-путь требует непропорционально много работы — re-scope
(зафиксировать)». Ф.0 это показал:

- **Объём/риск:** ~8 связанных codegen-точек на самом используемом
  типе, риск регрессий — **непропорционально**.
- **Выигрыш:** **нулевой функциональный** — `match @` компилируется в
  тот же `o.tag == NOVA_TAG_Option_Some`, что и трамплин.
- **Конфликт решения:** разворачивает Plan 78 Ф.1 (method_routing —
  легитимный C-реестр) ради косметики.

**Вывод: Plan 93 в формулировке «is_some Nova-телом» — GATE-STOP.**
Не выполнять Ф.1–Ф.4 как «быстрый перенос» нельзя (это было бы
упрощением: либо хак, либо рискованная неконтролируемая хирургия
most-used типа). Реальная предпосылка — **«Option/Result как
generic-method-able тип с `NovaOpt_<T>`/`NovaRes_<…>` в роли mono'd
receiver»** — самостоятельная инфраструктурная инициатива (плановый
масштаб ~2 dev-day, отдельный план). До неё Plan 93 **заблокирован**.

Зафиксировано маркером `[M-option-methods-not-mono-able]` в
`docs/simplifications.md`.

> **Status после Ф.0:** GATE-STOP. Ф.1–Ф.4 заблокированы до
> инфраструктуры «Option в generic-method-mono». Это честный исход
> production-grade аудита, не отказ от качества: реализация без
> инфраструктуры = упрощение, которое запрещено.

## Acceptance criteria

> Ф.0 завершён GATE-STOP'ом — критерии Ф.1–Ф.4 ниже **не достигнуты
> и заблокированы** до инфраструктуры «Option в generic-method-mono»
> (см. «Итог Ф.0»). Критерий самого Ф.0 — выполнен.

- [x] **Ф.0:** root cause `is_some`-как-Nova-тело установлен и доказан
      (probe `pu_opt` CC-FAIL + `emit_c.rs:1526` исключение Option из
      generic-механизма); decision point отработал.
- [ ] ~~`Option.is_some()`/`is_none()` — Nova-тело в core.nv~~ —
      заблокировано (требует инфраструктуры).
- [ ] ~~Вызов `opt.is_some()` через Nova-тело~~ — заблокировано.
- [ ] ~~C-трамплины удалены~~ — заблокировано.
- [ ] ~~Работает для всех `Option[T]`~~ — заблокировано.
- [ ] ~~Полный `nova test` — 0 новых FAIL~~ — N/A (Ф.1–Ф.4 не
      выполнялись; рабочее дерево не менялось, кроме docs).
- [x] `unwrap`/`unwrap_or`/`map`/`ok_or` — не затронуты (Ф.1–Ф.4 не
      выполнялись).

## Non-scope

- **`unwrap` / `unwrap_or` / `unwrap_or_else` / `map` / `ok_or` и
  прочие Option-методы** — не переносятся: `unwrap` требует
  Fail-handler dispatch (Plan 61), остальные — closure-applying
  спец-codegen. Остаются `external fn` + C-routing. Граница
  обоснована в Ф.0.3.
- **`Result`-методы** (`is_ok`/`is_err`/…) — аналогичная де-магификация
  возможна на той же `DeclaredBody`-инфре, но это отдельная единица.
- **str-методы** (`starts_with`/`eq`/`to_lower`) — та же развязка
  (`str_method_to_rt`-перехват), но отдельная линия (Plan 90-lineage).
  Plan 93 строит `DeclaredBody`-инфру, которой та работа сможет
  воспользоваться — но str block-body методам нужна ещё поддержка
  многострочных тел (вне scope Plan 93: `is_some` — expression-body).
- **Полная отмена `method_routing`** — Plan 93 выводит из реестра
  только тривиальные предикаты, не сам реестр (Plan 78 в силе).
