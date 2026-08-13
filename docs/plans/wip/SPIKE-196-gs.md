# GS_SPIKE.md — Разведка миграции `gs` под баунды

**Дата:** 2026-07-30  
**Язык:** русский  
**Модель:** deepseek-v4-flash-free  
**Ветка:** `p196-gs-spike`  

---

## (1) Что именно надо хранить вместо голого имени

Сегодня `gs: &HashSet<String>` несёт ТОЛЬКО имена generic-параметров. В точках резолва (а это ~17 читающих сайтов) **не хватает protocol bounds** — главная потеря.

### Потери информации

| Что теряется | Откуда берётся | Последствие |
|---|---|---|
| **Protocol bounds** (`GenericParam.bounds: Vec<TypeRef>`) | `ast/mod.rs:815` | Чекер не может разрешить `Option[T Debug]@debug` — в точке вызова не знает, какие методы есть у `T` |
| **Default-тип** (`GenericParam.default: Option<TypeRef>`) | `ast/mod.rs:820` | Потерян для диагностики/инференса (второстепенно, т.к. `node_substs` несёт конкретику) |
| **Consume-bound** (`GenericParam.consume_bound: bool`) | `ast/mod.rs:826` | Потерян флаг `[T consume]` — тело функции не знает, обязан ли потребить `T` |
| **Span** (`GenericParam.span`) | `ast/mod.rs:821` | Потеряна позиция для диагностик (второстепенно) |

### Доказательство: `current_fn_generics` уже обходит ту же проблему

В `types/mod.rs:3519` существует отдельное поле `current_fn_generics: RefCell<Vec<GenericParam>>`, которое несёт **полные** `GenericParam` (включая bounds). Оно используется функцией `resolve_generic_bound_method_return` (`types/mod.rs:11565-11594`) — единственный потребитель: Match-scrutinee с generic-bound-ресивером. Этот механизм ДОКАЗЫВАЕТ, что bounds нужны в чекере, и что паттерн «протащить полный `GenericParam`» уже реализован, но только для одного узкого case.

### Предлагаемая структура

```rust
pub struct GenericParamInfo {
    pub name: String,
    pub bounds: Vec<TypeRef>,
    pub default: Option<TypeRef>,
    pub consume_bound: bool,
}
```

Тип `gs` меняется с `&HashSet<String>` на:
- `&HashMap<String, GenericParamInfo>` (поиск по имени) — ИЛИ
- `&[GenericParam]` (порядок сохранён, поиск линейный) — дешевле, т.к. generic-параметров редко >3.

**Рекомендация:** `&HashMap<String, &GenericParamInfo>` — O(1) поиск, заменяет `gs.contains(name)` на `gs.contains_key(name)` и позволяет получить bounds:

```
// было:
if gs.contains(name) { ... }
// стало:
if let Some(gp) = gs.get(name) {
    for b in &gp.bounds { ... }
}
```

### 2-3 места, где новая структура снимает потерю

1. **`f1_expr_inner` — `Option[T]@debug`/`Result[T,E]@debug`** (`types/mod.rs:7985`, вокруг `resolve_generic_static_return`): сейчас чекер не может материализовать `resolved_types` для generic-возврата, потому что `gs` — только имена. С bounds (например `T Debug`) чекер мог бы найти протокол `Debug` через `gp.bounds`, найти метод `@debug`, вызвать его резолв и записать `resolved_types[call.id]` — сняв legacy-перевывод codegen'а.

2. **`walk_typeref` — обход графа типов** (`types/mod.rs:6488`): на generic-параметре (`gs.contains(name)` → `true`) рекурсия обрывается. С bounds можно рекурсивно обойти и их, чтобы, например, детектировать циклические зависимости через bound-протокол.

3. **`check_coalesce_return_fallback` / `coalesce_return_fallback_advice`** (`types/mod.rs:7812`, `21786`): gate `typeref_mentions_any(ret_ty, gs)` молча пропускает generic-возвраты. С bounds можно определить, что если `T` bound-нулся на `Fail`, то `Result[T, E]` конкретизируем — не нужно падать в fallback.

---

## (2) Реальный масштаб правки

Пройдены все 39 сигнатур `gs: &HashSet<String>` (включая `expr_gs`/`exp_gs`) в `types/mod.rs`.

### Таблица: файл:строка | классификация | действие

**Источник данных:** `types/mod.rs` — grep `gs: &HashSet` (39 матчей).

#### PASS-THROUGH (механическая замена типа — 22 сайта)

| Файл:строка | Функция | Примечание |
|---|---|---|
| `types/mod.rs:6475` | `walk_ref_return` | → `walk_typeref(inner, gs, errors)` |
| `types/mod.rs:6752` | `walk_block` | → `walk_stmt(s, gs, errors)` / `walk_expr(t, gs, errors)` |
| `types/mod.rs:6766` | `walk_stmt` | → `walk_expr` / `walk_typeref` |
| `types/mod.rs:7228` | `walk_else` | → `walk_block(b, gs, errors)` |
| `types/mod.rs:7240` | `walk_fn_sig_body` | → `walk_typeref` / `walk_expr` |
| `types/mod.rs:7487` | `f1_block` | → `f1_stmt(s, gs, scope, errors)` |
| `types/mod.rs:7529` | `f1_stmt` | → `f1_expr` / `f1_block` |
| `types/mod.rs:7872` | `check_coalesce_return_fallback` | → `coalesce_return_fallback_advice(_, _, gs)` |
| `types/mod.rs:7904` | `f1_expr` | → `f1_expr_inner(e, gs, scope, errors)` |
| `types/mod.rs:10224` | `f1_else` | → `f1_block(b, gs, scope, errors)` |
| `types/mod.rs:10237` | `f1_fn_sig_body` | → `f1_expr(e, gs, scope, errors)` |
| `types/mod.rs:10350` | `f1_check_assign_let` | → `assignable(value, ann, gs, gs, scope)` |
| `types/mod.rs:10930` | `f1_for_body` | → `f1_block(body, gs, scope, errors)` |
| `types/mod.rs:10956` | `f1_check_for_elem` | → `resolved_cat_of(ann, gs)` / `resolved_cat_of(&elem_tr, gs)` |
| `types/mod.rs:12120` | `overload_applicability` | → `assignable(arg.expr(), &param.ty, gs, &callee_gs, scope)` |
| `types/mod.rs:12223` | `check_instance_overload` | → `overload_applicability(c, args, gs, scope)` |
| `types/mod.rs:15288` | `check_interp_no_display` | → `resolve_interp_user_value_type(ex, gs, scope)` |
| `types/mod.rs:15381` | `check_interp_no_debug` | → `resolve_interp_user_value_type(ex, gs, scope)` |
| `types/mod.rs:16155,16156` | `assignable` (expr_gs, exp_gs) | → `assignable_direct(expr, expected, expr_gs, exp_gs, scope)` |
| `types/mod.rs:16597,16598` | `assignable_direct` (expr_gs, exp_gs) | → `assignable(item_expr, elem_ty, expr_gs, exp_gs, scope)` / `resolved_cat_of(expected, exp_gs)` |
| `types/mod.rs:20557` | `resolved_cat_of` | → `resolved_cat_of_depth(tr, gs, 0)` |

#### READER (нужна логическая правка — 17 сайтов)

| Файл:строка | Функция | Что делает с `gs` |
|---|---|---|
| `types/mod.rs:6222` | `infinite_dfs` | `gs.contains(name)` — стоп-лист generic-параметров при DFS |
| `types/mod.rs:6332` | `check_is_operand` | `gs.contains(base)` — generic-параметр → пропустить |
| `types/mod.rs:6427` | `ref_target_confirmed_heap` | `gs.contains(name)` — generic → хранение неизвестно |
| `types/mod.rs:6488` | `walk_typeref` | `gs.contains(name)` — generic → досрочный возврат |
| `types/mod.rs:6854` | `walk_expr` | 2× `gs.contains(name)` — turbofish arity gate + record-lit unknown-type skip |
| `types/mod.rs:7812` | `check_try_carrier_match` | 2× `typeref_mentions_any(_, gs)` — conservative silence |
| `types/mod.rs:7955` | `record_expr_type_ide` | `typeref_mentions_any(&tr, gs)` — не записывать generic IDE-type |
| `types/mod.rs:7985` | `f1_expr_inner` | ~15× `typeref_mentions_any(_, gs)` gate для channel materialization; 3× `gs.contains()` для generic-recv gates |
| `types/mod.rs:12791` | `check_fn_value_call` | `typeref_mentions_any(ret, gs)` — gate channel |
| `types/mod.rs:12881` | `f1_check_call` | `gs.contains(parts[0])` — generic static named-arg unsupported; 5× `typeref_mentions_any(ret_ty, gs)` — channel gates |
| `types/mod.rs:15245` | `resolve_interp_user_value_type` | `gs.contains(&tname)` — generic → None |
| `types/mod.rs:16942` | `protocol_mismatch_found` (exp_gs) | 2× `exp_gs.contains(name)` — generic → undecidable |
| `types/mod.rs:20564` | `resolved_cat_of_depth` | `gs.contains(name)` — generic → `ResolvedType::Any` |
| `types/mod.rs:21786` | `coalesce_return_fallback_advice` | 2× `typeref_mentions_any(_, gs)` — conservative silence |

### Итого

- **22 PASS-THROUGH** (механическая смена типа параметра, без логических изменений)
- **17 READER** (нужна логическая работа: чтение bounds из новой структуры)
- **0 PRODUCER** (все `gs` — `&HashSet<String>`, мутации нет; но есть **6 мест популяции**: `fn_generic_scope` `types/mod.rs:21596`, и inline в `f1_check_fn` `types/mod.rs:5580`, `walk_type_decl` `types/mod.rs:5884`, `check_direct_value_cycle` `types/mod.rs:6143`, `f1_check_fn` `types/mod.rs:7309`, и 3 анонимных в тестах `types/mod.rs:4727-4773`)

---

## (3) Упирается ли это в ключ `ExprId`

**Нет, не упирается.** Вывод по коду, не по догадке.

### Как работает ExprId-ключ

`ExprId(u32)` (`ast/mod.rs:2316`) — сквозной идентификатор каждого `Expr`-узла. Присваивается после парсинга, **до** type-checking (`ast/mod.rs:2332`). Каналы:
- `resolved_types: HashMap<ExprId, ResolvedType>` (`types/mod.rs:874`)
- `resolved_callees: HashMap<ExprId, Span>` (`types/mod.rs:888`)
- `node_substs: HashMap<ExprId, Vec<(String, ResolvedType)>>` (`types/mod.rs:896`)

Каждый call-site — отдельный `Expr` → отдельный `ExprId` → отдельная запись в каналах. Это **корректно** для `node_substs`: на одном ExprId хранятся конкретные подстановки для этого вызова.

### Почему bounds НЕ ломают ExprId

Generic-параметр **один раз объявлен** (`GenericParam` в `FnDecl.generics` / `TypeDecl.generics`), и его bounds **одинаковы** для всех сайтов использования в его области видимости.

- `fn[T Debug] foo(x: T)` — `T` всегда с `Debug`, на любом call-site
- `fn[T] Foo @bar[U Display]()` — `U` всегда с `Display`

Конкретная подстановка (`T = int`) — пер-call-site (записана в `node_substs`), а bounds — пер-декларация (не меняются от вызова к вызову). ExprId нужен только для первого.

**Единственный сценарий, где нужна осторожность:** если `gs` начнёт нести bounds как `&HashMap<String, &GenericParam>`, то **ссылка** на `GenericParam` валидна только пока жив `FnDecl`. В `walk_*`-паттерне это нормально (вызов синхронный), но при кешировании или отложенной работе — риск dangling reference.

### Вывод

ExprId — **не блокер**. Каналы `resolved_types`/`resolved_callees`/`node_substs` остаются ключевыми по ExprId без изменений. `gs` становится `Map<name → info>` — ключ по имени, не по ExprId.

---

## (4) Параллель с №150 — подтвердить или опровергнуть

**Подтверждаю: это ОДИН механизм.**

### Цитата из плана

`docs/plans/196-one-truth-closeout.md:588-590`:
> `gs` теряет баунды generic-параметра — share-ness теряет транзитивную достижимость через границу файбера (№150). По сути ОДНА проблема: checker не умеет нести мета-свойства (protocol bounds, shareness, thread-safety) сквозь generic-параметры от объявления до точки использования.

### Анализ по коду

**Проблема gs:** `gs: &HashSet<String>` — только имена. Protocol bounds (`GenericParam.bounds`) теряются при передаче от объявления к точке резолва. Чекер не знает, что `T` в `Option[T Debug]@debug` имеет `Debug`-bound, и не может материализовать `resolved_types` для этого вызова.

**Проблема №150** (`docs/plans/221.1-bug-sweep.md:142`): `[M-cross-fiber-shared-mut-reachability-not-transitive]` — share-ness не проверяется транзитивно. Замыкание с mut-захватом, переданное через spawn (параметром, полем, каналом), протаскивает небезопасный доступ между файберами. **Эталон — Rust Send/Sync.**

**Общая абстракция:** мета-свойство типа, привязанное к generic-параметру, которое должно «доехать» от объявления до точки использования:

| Механизм | Сегодня | Нужно |
|---|---|---|
| Protocol bounds (`T Debug`) | Потеряно в `gs` | `T.bounds` доступно в точке резолва |
| Share-ness (`T: Send`) | Не существует | `T` должен нести маркер «безопасно пересекает границу файбера» |

### Различие

Protocol bounds — **компиляторная проверка** (есть ли `@debug` у `T`?). Share-ness — **runtime-ограничение** (можно ли отдать `T` в другой поток?). Но в Nova's архитектуре share-ness может быть выражена как ещё один protocol bound (`T Share`), что сводит №150 к частному случаю общей проблемы.

### Итог

Один механизм: **«TypeVar с attached constraints»**. Набор мета-свойств, которые checker несёт сквозь generic-параметр и проверяет/консультирует в точке использования. Протокольные bounds — частный случай constraints; share-ness — другой. Если проектировать расширение TypeVar один раз для обоих — правильно. Подтверждаю рекомендацию плана 196.

---

## (5) Оценка и порядок

### Сколько работы

**Фазы (в порядке выполнения):**

| Фаза | Что делать | Сайтов | Тип работы | Оценка |
|---|---|---|---|---|
| **Ф.0 — Дизайн** | Определить `GenericParamInfo`, выбрать контейнер (`HashMap` vs `Vec` vs `HashMap<&str, &GenericParamInfo>`) | 0 | Архитектура | ½ волны |
| **Ф.1 — Популяция** | Исправить `fn_generic_scope` + 6 inline-мест популяции | 7 | Механика | 1 волна (sonnet) |
| **Ф.2 — PASS-THROUGH** | 22 сайта — сменить тип параметра, проверить сборку | 22 | Механика | ½ волны |
| **Ф.3 — READER (легкие)** | ~10 сайтов с `gs.contains(name)` → заменить на `gs.contains_key(name)` | 10 | Простая логика | 1 волна |
| **Ф.4 — READER (тяжелые)** | `f1_expr_inner` (~15 typeref_mentions_any gates), `f1_check_call` (~5), `check_fn_value_call` — нужно понимать bounds при gate'ах | 3 локации, ~25 gate'ов | Сложная логика | 2-3 волны (sonnet) |
| **Ф.5 — typeref_mentions_any** | Решить, нужен ли аналог для bounds — или gate остаётся check-only | 1 функция | Логика | ½ волны |
| **Ф.6 — Тесты** | `Option[T Debug]@debug` / `Result[T,E Debug]@debug` — conformance | ~5 новых тестов | Тесты | ½ волны |

**Итого:** ~6 волн (sonnet), ~4-5 дней при полной загрузке.

### Самые рискованные места

1. **`f1_expr_inner`** (`types/mod.rs:7985`) — 15+ gate'ов `typeref_mentions_any`, перемешанных с основной логикой. Каждый gate решает, материализовать ли `resolved_types`. Неверное чтение bounds → тихая потеря канала или ложная материализация → §0-баг (расхождение окон). Это САМОЕ опасное место.

2. **`fn_generic_scope`** (`types/mod.rs:21596`) — вызывается неявно из многих мест. Смена возвращаемого типа заденет всех call-сайтов. Нужно убедиться, что ни один потребитель не сломается (например, код, который использует `gs` как `HashSet<String>` для set-операций — пересечения, разности).

3. **`assignable` / `assignable_direct`** (`types/mod.rs:16155-16598`) — `expr_gs` и `exp_gs` используются для проверки assignability сквозь generics. Логика `protocol_mismatch_found` (`types/mod.rs:16942`) и `resolved_cat_of_depth` (`types/mod.rs:20564`) читают `gs.contains()` для принятия решения undecidable vs false. Неверное чтение bounds → false positive/negative assignability.

### Constructor-inference (`Vec[T].new()`)

**Не должен пострадать.** Constructor-inference использует:
- `node_substs[call_id]` для получения конкретных подстановок
- `gs.contains()` в `resolved_cat_of_depth` и `assignable` — эти gate'ы проверяют «является ли имя generic-параметром?» (бинарный признак). Если заменить `gs.contains(name)` на `gs.get(name).is_some()` — семантика НЕ меняется.

Единственный сценарий, где constructor-inference может задеть: если `GenericParamInfo.default` начнёт использоваться при инференсе (а не только при дефолтной подстановке). Это будет новое поведение, а не регресс.

### Что делать первым

1. **Дизайн** (Ф.0): зафиксировать структуру `GenericParamInfo` в D-блоке или решении под планом. Согласовать, нужна ли копия `GenericParam` или реф (одна копия bounds — в `GenericParam` исходного `FnDecl`; `gs` может быть `HashMap<&str, &GenericParam>` — zero-copy reference).
2. **Популяция** (Ф.1): `fn_generic_scope` — ключевая функция. Сменить возврат с `HashSet<String>` на новую структуру; поправить 6 inline-мест.
3. **PASS-THROUGH** (Ф.2): механическая волна — все 22 сайта.
4. **READER лёгкие** (Ф.3): 10 сайтов с `gs.contains()`.
5. **READER тяжёлые** (Ф.4): `f1_expr_inner`, `f1_check_call`, `check_fn_value_call` — на отдельную волну с conformance-гейтом.
6. **Ф.5 + Ф.6** — доводка и тесты на `Option[T Debug]@debug`.

**Важно:** каждая фаза — отдельный коммит с byte-parity по conformance (кроме Ф.4 — она должна ДОБАВИТЬ новые покрытые случаи, а не сломать старые). Без деградации.

---

## Модель (сводка)

| Вопрос | Ответ |
|---|---|
| (1) Что хранить | Имя + bounds + default + consume_bound. Структура `GenericParamInfo` (или `HashMap<&str, &GenericParam>`) |
| (2) Масштаб | 22 PASS-THROUGH (механика) + 17 READER (логика) + 6 мест популяции. ~6 волн. |
| (3) ExprId-ключ | НЕ блокер — bounds одинаковы для всех call-site одного generic; меняется только подстановка (уже в `node_substs`) |
| (4) Параллель №150 | **Подтверждена.** Один механизм: «TypeVar с constraints». Protocol bounds и share-ness — разные constraint'ы одного TypeVar |
| (5) Оценка | ~6 волн (sonnet). Самый риск: `f1_expr_inner` (15 gate'ов). Constructor-inference не должен пострадать |

---

*Хеш коммита:* `8f72dc98b`
