# Plan 95 — `Option`/`Result` как generic-method-able типы (мономорфизация методов builtin sum-типов)

> **Статус:** 🔵 В РАБОТЕ с 2026-05-23 (worktree `nova-p95`, ветка
> `plan-95`). Ф.0 ✅ — GATE пройден, подход B. Ф.1–Ф.7 в работе.
> **Приоритет:** P3 (de-magic / single-source-of-truth; разблокирует
> Plan 93, частично питает Plan 94). Не блокер релиза 0.1, не блокер
> self-hosting (см. Plan 93 «Связь с self-hosting»).
> **Оценка:** ~2.5–3 dev-day (инфраструктура — Ф.1–Ф.3; перенос самих
> предикатов — Ф.4–Ф.5 тривиален поверх неё).
> **Зависимости:** Plan 62.A (`Option`/`Result` через `NovaOpt_<T>` /
> `NovaRes_<ok>_<err>`) ✅; Plan 62.A.bis (`sum_schema_registry` +
> `MethodRouting`) ✅; Plan 93 Ф.0 (аудит — этот план реализует его
> вывод) ✅.
> **Supersedes:** Plan 93 (`Option.is_some`/`is_none` как Nova-методы) —
> его Ф.0 завершился GATE-STOP с явным выводом «реальная предпосылка —
> отдельная инфраструктурная инициатива». Plan 95 = эта инициатива;
> Ф.4 поглощает Plan 93 Ф.1–Ф.4.
> **Источник:** Plan 93 «Итог Ф.0» + обсуждение 2026-05-23 («сделать
> stdlib на самом языке — амбициозно; нужна декомпозиция в план»).

## Зачем

Plan 93 хотел заменить компилятор-магию `Option.is_some`/`is_none`
(`external fn` + C-трамплины) на обычное Nova-тело
`fn Option[T] @is_some() => match @ { Some(_) => true, None => false }`.
Его Ф.0-аудит доказал, что это **невозможно точечно**: `Option`/`Result`
намеренно исключены из generic-механизма codegen'а
([emit_c.rs:1526](../../compiler-codegen/src/codegen/emit_c.rs#L1526)),
потому что их представление — спец-value-типы `NovaOpt_<T>` /
`NovaRes_<ok>_<err>`, не связанные с generic-method-мономорфизацией.
Метод с Nova-телом на `Option` падает с `incomplete type Nova_Option`.

Plan 95 строит **недостающую инфраструктуру**: builtin sum-тип может
нести методы с Nova-телом, которые мономорфизируются per-T с приёмником
правильного C-типа (`NovaOpt_<T>` / `NovaRes_<ok>_<err>`), при этом
**представление типа не меняется** — `register_novaopt_decl` /
`register_novares_decl` остаются единственным источником typedef'а.
Это узкий, аккуратный канал «method-only mono», а не разворот Plan 62.A.

Без этого канала **любой** перенос builtin-методов в `.nv` (Plan 93,
частично Plan 94) заблокирован. Plan 95 — фундамент; Ф.4/Ф.5 — первые
два потребителя (предикаты `Option` и `Result`), они же доказывают, что
инфраструктура работает (инфра без потребителя не тестируема).

### Что НЕ делает (важно для приоритизации)

- Не предпосылка self-hosting компилятора (компилятор-на-Nova вызывает
  `opt.is_some()` одинаково — за методом C-трамплин или Nova-тело;
  C-рантайм остаётся). Приоритет **P3**.
- Не трогает производительность: `match @` компилируется в тот же
  `o.tag == NOVA_TAG_Option_Some`, что и трамплин. Выигрыш — **de-magic
  / single-source-of-truth**: «что делает `is_some`» читается в `.nv`.
- Не отменяет `method_routing`-реестр (Plan 78 в силе) — выводит из
  него только тривиальные тег-предикаты.

## Сравнение с Go / Rust / TS

| Язык | `Option`/`Result`-предикаты |
|---|---|
| **Rust** | `Option::is_some` / `Result::is_ok` — обычные библиотечные функции в `core` (`matches!(*self, Some(_))`). Generic-методы на `enum` — рядовой механизм языка, не интринсик. |
| **Go** | нет `Option`/`Result`; предикаты stdlib — обычный Go-код, дженерики (1.18+) мономорфизируются компилятором единообразно для всех типов. |
| **TS** | discriminated unions + type-guards — обычный TS-код. |
| **Nova (сейчас)** | `Option`/`Result` — единственные sum-типы, **исключённые** из generic-method-mono. Методы на них = только C-магия. **Хуже Rust/Go** — спец-кейс там, где должен быть общий механизм. |
| **Nova (цель)** | builtin sum-типы участвуют в method-mono наравне с пользовательскими; предикаты — обычный Nova-код в прелюдии. Паритет с моделью «ядро на самом языке». |

## Привязка к коду (сверено 2026-05-23)

Карта точек, которые Plan 95 затрагивает. Ф.0.1 повторно сверяет
номера строк (другие агенты двигают код).

| # | Точка | Файл:строка | Роль сейчас |
|---|---|---|---|
| 1 | Исключение из generic-template | `emit_c.rs:1526` | `if t.name == "Option" \|\| "Result" { continue }` — до `generic_types` + `generic_type_templates`. |
| 2 | Сбор методов generic-типов | `emit_c.rs:1555` | гейтится `generic_types.contains(recv.type_name)` → методы `Option`/`Result` не попадают в `generic_type_methods`. |
| 3 | C-тип приёмника | `emit_c.rs:7026` `receiver_c_type` | `other => format!("Nova_{}*", other)` → сломанный `Nova_Option*`. |
| 4 | Эмиссия mono'd метода | `emit_c.rs:8766` `emit_monomorphized_method` | `recv_c = receiver_c_type(recv_type)`, регистрирует `nova_self` в `var_types`. |
| 5 | Регистрация mono-инстанса | `emit_c.rs:8684` `register_mono_method_instance` | формирует `mono_name` + worklist-key `__method__TYPE::name`. |
| 6 | Перехват вызова Option | `emit_c.rs:14015` | `obj_ty.starts_with("NovaOpt_")` — роутит через `lookup_method_routing` → `HardcodedRuntimeFn`. |
| 7 | Перехват вызова Result | `emit_c.rs:14204` | `is_result_like(&obj_ty)` — аналогично для `Result`. |
| 8 | Lazy-emit typedef+helpers Option | `emit_c.rs:20651` `register_novaopt_decl` | эмитит `NovaOpt_<T>` + `Nova_Option_method_is_some_<T>` для непримитивов. |
| 9 | Lazy-emit typedef+helpers Result | `emit_c.rs:~20900` `register_novares_decl` | эмитит `NovaRes_<n>` + `Nova_Result_method_is_ok_<n>`. |
| 10 | C-трамплины примитивов | `nova_rt/array.h` | `Nova_Option_method_is_some_<primitive>` / `Nova_Result_method_is_ok_<n>`. |
| 11 | Routing-реестр | `sum_schema_registry.rs` | `MethodRouting` enum (`DeclaredBody` — scaffold-only, нигде не конструируется); `init_hardcoded_baseline` (`is_some`/`is_none`/`is_ok`/`is_err` → `HardcodedRuntimeFn`); `init_prelude_decls_from_items` (`if !f.is_external { return None }`). |

## Архитектура (предлагаемое решение — утверждается в Ф.0.2)

Два подхода:

- **A — полная регистрация template'а.** Снять исключение #1, добавить
  `Option`/`Result` в `generic_type_templates`, научить
  `drain_generic_type_worklist` эмитить `NovaOpt_<T>` как канонический
  mono-form. **Минус:** конфликтует с `register_novaopt_decl`
  (две инфраструктуры на один typedef); `drain_generic_type_worklist`
  генерирует heap-allocated struct-form `Nova_Option____<T>` — пришлось
  бы переписать репрезентацию. Разворачивает Plan 62.A.

- **B — канал «method-only mono» (рекомендуется).** Представление типа
  **не трогаем** — `NovaOpt_<T>` / `NovaRes_<ok>_<err>` остаются за
  `register_novaopt_decl` / `register_novares_decl` (исключение #1 из
  `generic_type_templates` сохраняется). Добавляем **только** канал
  мономорфизации *методов*: `Option`/`Result` регистрируются в
  `generic_type_methods` (но не в `generic_type_templates`);
  `receiver_c_type` спец-кейсит их в value-тип через `current_type_subst`;
  диспетчеризация локализована в уже существующих перехватах #6/#7 —
  добавляется ветка `MethodRouting::DeclaredBody`.

**Рекомендация — B.** Уже, безопаснее, переиспользует всю существующую
mono-машинерию, не разворачивает Plan 62.A. Ключевое наблюдение,
снижающее риск: диспетчеризация `Option`/`Result`-методов **уже
локализована** в двух перехватах (#6 `NovaOpt_`, #7 `is_result_like`) —
не нужно трогать все ~10 generic-dispatch call-site'ов. `DeclaredBody`
обрабатывается **внутри** этих двух блоков.

## Декомпозиция (фазы и шаги)

### Ф.0 — Аудит-подтверждение + архитектурное решение (~0.3 д) — GATE

- **Ф.0.1** Повторно сверить карту точек #1–#11 (номера строк) с текущим
  `emit_c.rs` / `sum_schema_registry.rs`. Локализовать точно перехват
  Result-вызова (#7) и `register_novares_decl` (#9).
- **Ф.0.2 — decision point: подход A vs B.** Утвердить **B** (или
  зафиксировать обоснование A). Probe: минимальным патчем (#1 method-
  collection + #3 receiver_c_type + #6 `DeclaredBody`-ветка) довести
  `fn Option[T] @my_present() => match @ { Some(_) => true, None => false }`
  до компиляции; сверить сгенерированный C с baseline-трамплином
  (ожидается семантически идентичный `tag`-чек). Probe из Plan 93
  (`pu_opt`) — переиспользовать.
- **Ф.0.3 — decision point: граница переноса.** Переносятся **только**
  чистые тотальные тег-предикаты: `is_some`/`is_none` (Option),
  `is_ok`/`is_err` (Result). `unwrap` (Fail-dispatch),
  `unwrap_or`/`unwrap_or_else`/`map`/`ok_or`/`map_err` (closure-applying
  спец-codegen) — **остаются** C-routed. Граница = Plan 93 Ф.0.3,
  подтвердить без изменений.
- **Ф.0.4 — decision point: scope Result.** Включить `Result`-предикаты
  (`is_ok`/`is_err`, Ф.5) в этот план — **рекомендуется**: доказывает
  обобщаемость инфры на второй тип, маржинальная стоимость мала
  (`NovaRes_`-перехват — параллельная структура `NovaOpt_`). Если Ф.0.2
  выявит, что Result-ветка непропорционально дороже — re-scope:
  Ф.5 выносится в отдельный план-итем, Plan 95 закрывается на Option.
- **Ф.0.5** Подтвердить, что `match @` над `NovaOpt_<T>` в теле mono'd
  метода (где `nova_self : NovaOpt_<T>`) использует существующий
  Option-match-codegen без новых веток — проверить, что
  `emit_monomorphized_method` регистрирует `nova_self` в `var_types`
  как `NovaOpt_<T>` и match-диспетчер это подхватывает.

## Итог Ф.0 (выполнено 2026-05-23, worktree `nova-p95`)

Аудит проведён чтением кода (`emit_c.rs`, `sum_schema_registry.rs`,
`ast/mod.rs`). Карта точек подтверждена; обнаружены **2 уточнения**.

### Сверка карты точек

- **#1** — исключение `Option`/`Result` из generic-механизма
  существует в **двух** местах: `emit_c.rs:1526` (секция 1a — до
  `generic_types` + `generic_type_templates`) **и** `emit_c.rs:2083`
  (секция 1d — до `generic_types`). Оба сохраняем (представление не
  трогаем). Метод-коллекшн (#2) — `emit_c.rs:1555`.
- **#3** `receiver_c_type` — `emit_c.rs:7026`; **#4**
  `emit_monomorphized_method` — `:8766`; **#5**
  `register_mono_method_instance` — `:8684`; **#6** перехват
  `NovaOpt_` — `:14015`; **#7** перехват Result (`is_result_like`) —
  `:14204`; **#8** `register_novaopt_decl` — `:20651`;
  **#9** `register_novares_decl` — `:20852`.
- `Receiver` (`ast/mod.rs:499`) содержит `generics: Vec<TypeRef>` —
  имена type-параметров приёмника доступны.
- `current_type_subst` — значения суть **C-типы** (не Nova-типы);
  `register_novaopt_decl` хранит `novaopt_value_types[sanitized] →
  real_c_ty`; `novares_ok_err(c_ty) → (ok_c, err_c)`.

### Уточнение 1 (новая точка #12) — drain mono-worklist

`emit_c.rs:2262-2274`: drain `__method__TYPE::name` ищет `FnDecl` через
`mono_method_decls[key]` → fallback `generic_type_methods[base]` **только
если** `base_opt` разрезолвился через `generic_type_instance_info`. Для
builtin `Option`/`Result` `recv_type` в worklist-ключе = уже базовое имя
(`"Option"`), `generic_type_instance_info` его не содержит → `base_opt =
None` → `FnDecl` **не находится**, тело не эмитится. **Фикс:** добавить в
drain финальный fallback `generic_type_methods.get(recv_type)` напрямую
(для user-типов `recv_type` — mangled, `.get` вернёт `None`, безвредно).

### Уточнение 2 — collision имён mono'd функции и трамплина

Mono-имя выбрано как форма существующего трамплина
(`Nova_Option_method_<m>_<sani(T)>` / `Nova_Result_method_<m>_<n>`) —
чтобы call-site mangling не менялся. Следствие: эмиссия Nova-тела с этим
именем **конфликтует** с трамплином того же имени (C-redefinition). →
Удаление трамплинов (Ф.4.2 / Ф.5.2) обязано лэндиться **в одном
коммите** с переносом тела (Ф.4.1 / Ф.5.1), не отдельным «безопасным»
шагом. Инфра-фазы Ф.1–Ф.3 разрабатываются на probe-методе с **другим**
именем (`my_present`) — коллизии нет.

### Decision points

- **Ф.0.2 → подход B** (method-only mono). Подтверждён чтением:
  представление `NovaOpt_<T>`/`NovaRes_<…>` не трогается; диспетчеризация
  локализована в перехватах #6/#7; подход A (template-регистрация)
  отвергнут — конфликт с `register_novaopt_decl` + `is_generic_call`
  void*-boxing path (комментарий `emit_c.rs:2076`).
- **Ф.0.3 → граница без изменений**: переносятся только
  `is_some`/`is_none`/`is_ok`/`is_err`.
- **Ф.0.4 → Result in-scope**: `NovaRes_`-перехват (#7) — точная
  параллель `NovaOpt_` (#6); маржинальная стоимость мала. Ф.5 включена.
- **Ф.0.5 → подтверждено**: `emit_monomorphized_method:8822` регистрирует
  `nova_self` в `var_types` как `recv_c`; при `recv_c = NovaOpt_<T>` /
  `NovaRes_<…>*` `match @` использует существующий Option/Result-match-
  codegen (тот же, что `if let Some(x) = opt` — компилируется повсеместно).

**GATE ПРОЙДЕН — подход B, переходим к Ф.1.** Probe-патч из Ф.0.2 не
делается отдельным выбросом: инфра Ф.1–Ф.3 = и есть «минимальный патч»,
верифицируется probe-фикстурой (`my_present`) в конце Ф.3.

### Ф.1 — Method-collection + routing-scaffold (~0.4 д)

- **Ф.1.1** Ввести `builtin_mono_method_types: HashSet<String>` =
  `{"Option", "Result"}` (или эквивалент). Цикл сбора методов
  (`emit_c.rs:1555`) — расширить гейт:
  `generic_types.contains(..) || builtin_mono_method_types.contains(..)`
  → Nova-тельные методы `Option`/`Result` попадают в
  `generic_type_methods`. Исключение #1 из `generic_type_templates`
  **сохраняется** (представление не трогаем).
- **Ф.1.2** `init_prelude_decls_from_items` — снять фильтр
  `if !f.is_external { return None }`: не-`external` метод на
  `Option`/`Result` с Nova-телом регистрируется с
  `MethodRouting::DeclaredBody { has_nova_body: true }` (перекрывает
  унаследованный `HardcodedRuntimeFn` для этого имени).
- **Ф.1.3** `lookup_method_routing` — убедиться, что для перенесённого
  метода возвращается `DeclaredBody` (precedence Prelude > Hardcoded).
- **Ф.1.4** Unit-тесты `sum_schema_registry.rs`: declared Nova-body
  метод → `DeclaredBody`; не-перенесённый (`unwrap`) → прежний
  `HardcodedRuntimeFn`.

### Ф.2 — receiver-type + mono-name резолвинг (~0.6 д)

- **Ф.2.1** Хелпер `builtin_sum_receiver_c_type`: для `recv_type ==
  "Option"` → `NovaOpt_<sani(T)>` (T из `current_type_subst`); для
  `"Result"` → `NovaRes_<ok>_<err>` (через `result_mono_c_pair` /
  `novares_name`). `receiver_c_type` (#3) спец-кейсит `Option`/`Result`
  на этот хелпер вместо `Nova_{}*`. **Value-тип, без `*`** (приёмник —
  по значению, как примитив; согласовано с `NovaOpt_`-ABI).
- **Ф.2.2** Схема mono-имени: per-T детерминированное C-имя для
  `Option`/`Result`-методов. **Переиспользовать форму существующего
  трамплина** — `Nova_Option_method_<m>_<sani(T)>` /
  `Nova_Result_method_<m>_<n>` — тогда call-site mangling (#6/#7) не
  меняется, диспетчеризация минимально касается кода.
- **Ф.2.3** `emit_monomorphized_method` (#4) — для `Option`/`Result`
  `recv_c` берётся из Ф.2.1; `nova_self` регистрируется в `var_types`
  как `NovaOpt_<T>`; гарантировать вызов `register_novaopt_decl(T)` /
  `register_novares_decl` **до** эмиссии тела метода (topological order
  typedef'а — иначе `incomplete type`).
- **Ф.2.4** `register_mono_method_instance` (#5) — worklist-key
  `__method__Option::is_some` корректно драйнится для builtin-приёмника;
  `type_subst` = `[(T, elem_c_ty)]`.

### Ф.3 — Call-site dispatch: ветка `DeclaredBody` (~0.6 д)

- **Ф.3.1** Перехват `NovaOpt_` (#6, `emit_c.rs:14015`) — добавить ветку
  `MethodRouting::DeclaredBody`: извлечь `elem_ty`, найти `FnDecl` в
  `generic_type_methods["Option"]`, `register_mono_method_instance`
  с `type_subst=[(T, elem_ty)]`, эмитить вызов
  `<mono_name>(obj_c, args...)`.
- **Ф.3.2** Перехват Result (#7, `is_result_like`) — аналогичная ветка
  `DeclaredBody` для `Result` (`type_subst` = `[(T, ok), (E, err)]`).
- **Ф.3.3** Убрать `is_some`/`is_none`/`is_ok`/`is_err` из
  `HardcodedRuntimeFn`-fast-path в #6/#7, чтобы вызов доходил до
  `DeclaredBody`-ветки (или: `DeclaredBody` имеет приоритет в матчинге
  routing — проверяется первым).
- **Ф.3.4** Targeted-verify probe Ф.0.2 — вызов реально идёт через
  Nova-тело (mono'd функция), не трамплин: проверить сгенерированный C.

### Ф.4 — Перенос предикатов `Option` (потребитель #1; ~0.3 д)

> Поглощает Plan 93 Ф.1–Ф.4.

- **Ф.4.1** `std/prelude/core.nv` — `external fn Option[T] @is_some()` /
  `@is_none()` → Nova-тело
  `=> match @ { Some(_) => true, None => false }` (и зеркально `is_none`).
- **Ф.4.2** Удалить мёртвую магию: lazy-emit `Nova_Option_method_is_some_
  <T>` / `_is_none_<T>` из `register_novaopt_decl` (#8); примитивные
  трамплины из `array.h` (#10); записи `is_some`/`is_none` из
  `init_hardcoded_baseline` `option_methods` (#11). Single-source — не
  оставлять зеркало.
- **Ф.4.3** Targeted-verify: `Option[int]` / `[str]` / `[char]` /
  `Option[user-record]`.

### Ф.5 — Перенос предикатов `Result` (потребитель #2 — proof обобщаемости; ~0.2 д)

> Выполняется только если Ф.0.4 утвердил Result в scope.

- **Ф.5.1** `std/prelude/core.nv` — `is_ok`/`is_err` → Nova-тело
  `=> match @ { Ok(_) => true, Err(_) => false }` (зеркально).
- **Ф.5.2** Удалить трамплины `Nova_Result_method_is_ok_<n>` /
  `_is_err_<n>` (`register_novares_decl` #9 + `array.h` #10) + записи из
  `init_hardcoded_baseline` `result_methods`.
- **Ф.5.3** Targeted-verify: `Result[int,str]` / `[user,str]`.

### Ф.6 — Тесты позитив + негатив (~0.3 д)

- **Ф.6.1** `nova_tests/plan95/` позитив:
  - `is_some`/`is_none` на `Option[int]`/`[str]`/`[char]`/`[user-record]`;
  - `is_ok`/`is_err` на `Result[int,str]`/`[user,str]`;
  - предикаты в `for-in` над `[]Option[T]`;
  - в теле generic-функции с `[T]`-bound (`fn check[T](o Option[T])`);
  - вложенный `Option[Option[T]]`.
- **Ф.6.2** Негатив (`EXPECT_COMPILE_ERROR`):
  - Nova-тело метода `Option` с неподдержанной фичей (например,
    обращение к несуществующему методу) → чистая ошибка codegen,
    **без** silent-fallback на `Nova_Option*` (Plan 79-инвариант);
  - `fn Option[T] @is_some()` с неверной сигнатурой (лишний параметр /
    не-`bool` возврат) → ошибка type-check.
- **Ф.6.3** Регресс: существующие потребители — `nova_tests/plan89/
  eq_and_method.nv`, `plan62/option_methods_from_prelude.nv`, stdlib
  `json.nv` (использует `is_some`/`is_ok`) — зелёные.
- **Ф.6.4** Полный `nova test` — 0 новых FAIL.

### Ф.7 — Spec / docs (~0.2 д)

- **Ф.7.1** D-блок: зафиксировать модель «builtin sum-тип
  (`Option`/`Result`) участвует в method-mono через канал `DeclaredBody`;
  представление остаётся `NovaOpt_<T>`/`NovaRes_<…>`». Уточнить
  существующий D-блок про `method_routing` (Plan 62.A.bis / Plan 78).
- **Ф.7.2** `docs/simplifications.md` — закрыть маркер
  `[M-option-methods-not-mono-able]` (инфра-разрыв устранён).
- **Ф.7.3** Plan 78 — аменд: `is_some`/`is_none`/`is_ok`/`is_err`
  выведены из C-реестра в Nova-тело (узкий пересмотр, не отмена реестра).
- **Ф.7.4** Plan 93 — пометить ✅ закрытым «superseded by Plan 95»
  (Ф.4 этого плана = его исходная цель).
- **Ф.7.5** `docs/plans/README.md` — статус Plan 95 + Plan 93.
- **Ф.7.6** `docs/project-creation.txt` +
  `nova-private/discussion-log.md` — записи.

## Acceptance criteria

- [ ] **Ф.0:** карта точек сверена; подход (B рекомендован) утверждён;
      probe `fn Option[T] @my_present()` компилируется минимальным
      патчем; границы (Ф.0.3/Ф.0.4) зафиксированы.
- [ ] `Option`/`Result` участвуют в method-mono: Nova-тельный метод на
      builtin sum-типе мономорфизируется per-T с приёмником
      `NovaOpt_<T>`/`NovaRes_<…>`, без `incomplete type`.
- [ ] `MethodRouting::DeclaredBody` реально конструируется
      (`init_prelude_decls_from_items`) и потребляется (перехваты #6/#7).
- [ ] `Option.is_some()`/`is_none()` — Nova-тело в `core.nv`; вызов идёт
      через mono'd функцию, не трамплин (проверено по C).
- [ ] `Result.is_ok()`/`is_err()` — то же (если Ф.0.4 = in-scope).
- [ ] Мёртвые трамплины удалены (`array.h` + lazy-emit) — single-source.
- [ ] `unwrap`/`unwrap_or`/`map`/`ok_or`/`map_err` — не затронуты,
      по-прежнему C-routed.
- [ ] Позитив + негатив тесты в `nova_tests/plan95/`; полный
      `nova test` — 0 новых FAIL.
- [ ] Маркер `[M-option-methods-not-mono-able]` закрыт; Plan 93
      помечен superseded.

## Non-scope

- **str-методы** (`starts_with`/`find`/`to_lower`/…) — Plan 94. `str` —
  не sum-тип, иное представление (`nova_str`), method-mono не per-T;
  канал `DeclaredBody` этого плана — sum-специфичный. Plan 94 строит
  свой канал, но переиспользует **идею** routing'а через реестр и опыт
  Plan 95. Кроме того str-методам нужны многострочные block-body
  (Plan 94 Ф.1) — здесь все тела expression-body (`=> match`).
- **`unwrap`/`unwrap_or`/`unwrap_or_else`/`map`/`ok_or`/`map_err`** —
  не переносятся: `unwrap` → Fail-handler dispatch (Plan 61); остальные
  → closure-applying спец-codegen. Остаются `external fn` + C-routing.
- **Полная отмена `method_routing`-реестра** — Plan 95 выводит из реестра
  только тривиальные тег-предикаты; реестр (Plan 78) в силе для
  C-реализуемых-only методов.
- **Подход A (template-регистрация `Option`/`Result`)** — отвергается в
  Ф.0.2 в пользу B; если когда-нибудь понадобится унифицировать
  представление — отдельный план.

## Связь с другими планами

- **Plan 93** — superseded: его цель (`is_some`/`is_none` Nova-телом) =
  Ф.4 этого плана. Plan 93 Ф.0 (аудит) — вход Plan 95.
- **Plan 94** (str-методы на Nova) — независим по реализации (str ≠
  sum), но идеологически родственен: оба «de-magic stdlib». Если оба
  активны — Plan 95 идёт первым (меньше, доказывает подход на 2 типах).
- **Plan 78** (prelude-codegen single-source) — Plan 95 — узкий
  санкционированный пересмотр его Ф.1 для подмножества «чистые
  предикаты».
- **Plan 62.A / 62.A.bis** — Plan 95 оживляет недоделанный
  `MethodRouting::DeclaredBody` (scaffold-only огрызок 62.A.bis.4).
