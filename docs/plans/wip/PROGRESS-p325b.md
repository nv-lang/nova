# PROGRESS-p325b — Ш.2: линейность наследуется контейнером (№325)

Окно p325-inherit, worktree `d:/Sources/nv-lang/nova-p325b`, ветка
`p325-inherit`, модель sonnet. Опора: `spec/decisions/02-types.md` D156
амендмент 2026-08-04 (шаг B плана 246), запись №325 в
`docs/plans/221.1-bug-sweep.md`.

## Диагноз (до кода)

Заслон Ш.0 (`check_consume_in_std_collections`) — module-wide sweep по
КАЖДОЙ TypeRef-позиции + TurboFish-сайту, флагующий `CONSUME_UNSAFE_STD_COLLECTIONS`
(8 жёстко зашитых имён) с consume-generic-аргументом. Работает через
`LinearityRegistry::type_is_consume` — рекурсивную, generic-aware функцию,
которая УЖЕ (без всякого списка) трактует ЛЮБОЙ `Named{generics}` как
consume, если хотя бы один generic-аргумент consume («Generic wrap:
Option[Transaction], Box[Tx], Wrapper[T]» — комментарий в коде, строка
34539). Значит сам механизм генерален с самого начала; жёсткий список был
нужен ТОЛЬКО потому, что заслон — отдельный module-wide sweep, а не часть
основного потока обязательств.

Настоящая дыра (репро №325: `mut v = Vec[Res].new(); consume r = mk(1);
v.push(r)`) — не в `type_is_consume`, а в том, что D180-энфорс
обязательности `consume`-keyword на `let`/`mut`-биндинге
(`rhs_yields_consume_type`, `types/mod.rs` ~38952-38959) использует
**плоскую** проверку `lin_reg.consume_types.contains(bare_name)`, а
`bare_name` берётся из `infer_let_type`, которая **отбрасывает generics**
(возвращает голое имя типа, `path[0]`, без generic-списка) — и, что хуже,
для TurboFish-конструктора (`Vec[Res].new()`) вообще не резолвит имя:
`infer_value_type`'s `Call{Member{obj,...}}`-ветка не знает формы
`obj = TurboFish{...}` (нет такой ветки в match), рекурсия падает в `None`.
Итог: `mut v = Vec[Res].new()` получает `inferred_ty_d180 = None` →
`rhs_yields_consume_type = false` → `consume`-keyword не требуется →
биндинг `v` вообще не становится consume-obligation → потребление
контейнера никогда не проверяется.

`for consume x in vec` (D156 collection-aware iteration) уже реализован
ОБЩЕ (не по списку имён): `consume_walk_consume_for` декларирует loop-var
`declare_consume_binding(n, None)` безусловно, и есть fallback
(строка ~40808) — если receiver с неизвестным типом вызывает метод,
зарегистрированный consume-методом ХОТЬ ГДЕ-ТО (`lin_reg.consume_methods`
по любому типу) и receiver — consume-obligation, вызов трактуется как
consuming. Значит `for consume tx in txs { tx.commit() }` уже работает для
ЛЮБОГО контейнера, включая пользовательский — проверено существующими
фикстурами `for_consume_iter_ok.nv`/`vec_push_consume_ok.nv` (обе не
std-специфичны по механизму). Аналогично `return v`/передача `v` в
consume-параметр — общие, type-agnostic пути (`mark_consumed` по имени).

**Вывод:** единственная реальная правка — научить D180
`rhs_yields_consume_type` видеть generics RHS, а не расширять сам
`type_is_consume` (он уже общий) и не трогать `for consume`/return/pass-
through (уже общие).

## План правки

1. `ConsumeCtx` получает поле `module: &'a Module` (нужно
   `type_is_consume(&self, t, module)` — единственный способ дотянуться до
   `module` без протаскивания параметра через десятки взаимно-рекурсивных
   `consume_walk_*`-функций). 2 call-сайта `ConsumeCtx::new`.
2. Новый метод `ConsumeCtx::infer_let_type_ref(&self, decl) -> Option<TypeRef>`
   — сиблинг `infer_let_type`, но сохраняет generics: явная аннотация
   (`decl.ty`) возвращается как есть; иначе — `turbofish_ctor_type_ref`
   (новый free fn) резолвит форму `Type[Args].ctor(...)` (тот же список
   конструкторов, что в `infer_value_type`: `new`/`with_capacity`/`from`/
   `default`/`filled`/`of`) в `TypeRef::Named{path:[Type], generics:Args}`.
   Переиспользует существующий `turbofish_base_name` (был приватным
   хелпером барьера — остаётся, барьер снят, хелпер used дальше).
3. `rhs_yields_consume_type` = старая проверка (bare-name, не трогаем — она
   покрывает случаи БЕЗ generics и не должна регрессировать) ИЛИ новая:
   `infer_let_type_ref(decl)` прогоняется через `lin_reg.type_is_consume`
   (полная, generic-aware, рекурсивная) — вот она и есть общее правило
   «контейнер наследует линейность элемента», работающее на ЛЮБОМ generic-
   типе (std и пользовательском одинаково), без единого хардкода имени.
4. Заслон Ш.0 снимается целиком: `check_consume_in_std_collections`,
   `check_typeref_consume_collection_barrier`,
   `walk_block_for_consume_collection_barrier`,
   `walk_expr_for_consume_collection_barrier`,
   `consume_collection_diagnostic`, `CONSUME_UNSAFE_STD_COLLECTIONS`, call-
   сайт в основном проходе. `turbofish_base_name` остаётся (переиспользуется
   в п.2).
5. Обе neg-фикстуры заслона (`consume_collection_vec_push_forgotten_neg.nv`,
   `consume_collection_vec_push_two_owners_neg.nv`) — тот же исходник, НО
   ожидаемая диагностика меняется с `E_CONSUME_IN_STD_COLLECTION` на
   `E_CONSUME_KEYWORD_MISSING` (новая причина: биндинг контейнера обязан
   быть `consume`, а не "in std collection").

## Осознанно НЕ тронуто (объём окна)

- Параметрная default-классификация view/consume (строка ~38057, тоже
  bare-name `consume_types.contains`) — тот же класс дыры для
  ФУНКЦИЙ-параметров генериков (`fn f(v Vec[Res])` не получает
  `declare_view_param`), но брифом не потребован ни один фикстур на этот
  путь, и `type_is_consume` там и так недостижим без module (уже доступен
  в этой точке — `module` в скоупе цикла `Item::Fn`, дешёвый фикс) —
  ОСТАВЛЕНО КАК НАХОДКА в отчёте, не как правка, чтобы не расширять периметр
  за пределы приказанного (declaration-site container-inherits для
  `let`/`mut`-биндингов).
- Другие bare-name `consume_types.contains(...)` сайты (39413, 39519, 39595,
  40976, 41471, 36588, 36641) — Option/Result payload-propagation
  (D157-амендмент), НЕ про контейнеры; трогать — риск случайно задеть
  `Result[File, IoError]` pos-контроль. Не трогаю.
- Drop-glue / эффект-переменная / бáунд `Cleanup[E]` реализация — вне
  периметра (шаг C плана 246, после тега). Само упоминание формы бáунда в
  брифе (`Vec[T consume Cleanup[E]]`) — контекст для будущего, не код этой
  волны.

## Статус

См. финальный отчёт для вердиктов прогонов (заполняется по ходу).
