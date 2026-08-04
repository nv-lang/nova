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

Реализовано и проверено. Ключевые точки:

- `ConsumeCtx` получил поле `module: &'a Module` (2 call-сайта `new`).
- `ConsumeCtx::infer_let_type_ref` + free fn `turbofish_ctor_type_ref` —
  генерик-aware резолв RHS-типа для D180.
- `rhs_yields_consume_type` = старая проверка ИЛИ
  `type_is_consume(infer_let_type_ref(decl))`.
- Диагностика `E_CONSUME_KEYWORD_MISSING` получила приличное имя типа в
  сообщении (`inferred_ty_d180_display`) вместо `` `?` `` — раньше RHS
  TurboFish-конструктора не резолвился вовсе.
- Заслон Ш.0 снят целиком (`check_consume_in_std_collections` +
  4 хелпера + `CONSUME_UNSAFE_STD_COLLECTIONS` + call-сайт).
  `turbofish_base_name` остался (реиспользован).

### Находка №1 (чинена той же волной): `for consume` пессимизировал уже
консьюмленную переменную

`consume_walk_consume_for` (D156, существовал ДО этого окна) вычислял
`outer_consumed` только по ПОСТ-pass-1 состоянию, без сравнения с `pre` —
переменная, полностью потреблённая ДО входа в `for consume`-цикл (напр.
передана в consume-param прямо перед циклом), даунгрейдилась обратно до
`MaybeConsumed`, потому что тело цикла её просто не трогало (состояние
оставалось Consumed и до, и после prob-прохода — фильтр `matches!(post,
Consumed|MaybeConsumed)` этого не различал). Ложный `D133-not-consumed`
(«consumed только на части путей») на однозначно потреблённой переменной.
Найдено собственной pos-фикстурой этого окна (передача контейнера в
consume-param, затем `for consume` над возвращённым результатом) — НЕ
специфично для контейнеров, баг общего for-consume-механизма D156. Fix:
`outer_consumed` включает `k` только если `pre.get(k)` НЕ было уже
Consumed/MaybeConsumed (т.е. переход произошёл ВНУТРИ тела, а не раньше).

### Что найдено в корпусе после снятия жёсткого списка

`nova check std/src` (канон 148/26/61, БЕЗ сдвига) и polaris
`./nova.sh test src --strict-effects` (канон 37/0/18, БЕЗ сдвига) —
ОБА зелёные без единого нового FAIL/PASS-сдвига: новое правило нигде в
std/polaris не срабатывает — согласуется с аудитом самого заслона Ш.0
(«корпус на дыру не опирался»). Целевая grep-разведка (Explore-агент) по
std/examples/nova-http/nova-polaris/nova-tls/nova-bignum/www на TurboFish-
конструкторы (`Type[Args].new/with_capacity/from/default/filled/of`) и на
explicit-annotation `let`-формы с consume-типом внутри `[...]` дала НОЛЬ
находок во всех шести репах: везде generic-аргумент — обычный тип
(`int`/`str`/`DbRow`/`EmitRecord`/`JsonValue`/...), НИ РАЗУ — один из
известных must-consume типов (`File`, `TcpStream`/`TcpListener`/
`TcpReadHalf`/`TcpWriteHalf`, `UdpSocket`, `MutexGuard`/`ReadGuard`/
`WriteGuard`, `Permit`, `OnceGuard`, `StringBuilder`, `BufWriter[W]` —
std; `Body`/`Request`/`Response` — nova-http; `WebSocket` — nova-polaris;
`TlsStream` — nova-tls). Единственные места, где эти типы встречаются
внутри `[...]`, — сигнатуры возврата (`Result[File, IoError]`,
`Option[Permit]`, `Result[TlsStream, TlsError]`) — вне периметра
изменённой проверки (она смотрит только на RHS TurboFish-конструктора и
на type-annotation `let`-биндинга, не на fn-сигнатуры). `BufWriter[W]`
уже И ДО этого окна корректно требовал `consume` (сам `BufWriter`
объявлен `consume`, это база из D133, не новая generic-заразность) —
`d322_buffered_test.nv`/`d322_io_mock_test.nv`/`d322_bufwriter_*_neg.nv`
уже пишут `consume bw = BufWriter[...].new(...)`, найдено и подтверждено
не regression. nova-tls и nova-bignum — вообще ноль TurboFish-конструкторов
в исходниках. www — .nv-исходников нет вовсе (сайт).

**Итог риска: блэст-радиус по факту НУЛЕВОЙ** — ни один существующий файл
не спотыкается о новое правило; оно начинает действовать только для БУДУЩЕГО
кода, кладущего must-consume тип в generic-контейнер через TurboFish/
явную аннотацию.
