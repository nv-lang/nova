# PROGRESS-pbound303 — №303, бáунд получателя не проверяется на месте вызова

## Дизайн-записка (ДО кода)

**Где врезана проверка:** `compiler-codegen/src/types/mod.rs`, чекер-канал.
Новая функция `check_receiver_carrier_bounds` (рядом с существующей
`check_receiver_shape_match`, Plan 221.1 №88, ~строка 25098), вызывается из
`walk_expr` сразу после неё (~строка 24291-24293).

**Почему здесь, а не в emit_c.rs.** §0/196: фикс — в чекер-канал, легаси
emit_c.rs не наращивать (ratchet lines/infer подтверждает: 0 изменений).

**Существующая инфраструктура, на которую опирается фикс (не изобретается
заново):**

1. `ast/mod.rs::Receiver.carrier_bounds: Vec<GenericParam>` — бáунды из
   carrier-скобок (`fn Holder[T Display] @show()`) парсер УЖЕ извлекает
   (parser/mod.rs:3336) и хранит с комментарием «Stored for future
   enforcement; currently informational only» — подтверждённый факт: до
   этого окна поле НИГДЕ не читалось для энфорса, только для заполнения
   generic-scope тела метода (`fn_generic_scope`/аналог в `check_fn_decl`,
   строки ~22908-22946 и ~5775-5808) — то есть ВНУТРИ тела `T` уже "знает"
   свой бáунд, но снаружи, на месте вызова, никто не проверяет, что
   КОНКРЕТНЫЙ receiver этому бáунду реально удовлетворяет.
2. `check_receiver_shape_match` (Plan 221.1 №88, ~25098) уже строит
   structural-unify (`const_fn_trampoline::unify_type` +
   `canonicalize_array_to_vec`) между ДЕКЛАРИРОВАННОЙ формой receiver'а
   (`recv.receiver_ty`, напр. `Holder[T]`) и ФАКТИЧЕСКИМ типом на месте
   вызова (`Holder[Plain]`), с выходом `subst: {T -> Plain}` — ЭТО и есть
   вход для бáунд-проверки, отдельно от проверки формы.
3. `check_satisfaction`/`check_satisfaction_against_methods` (~26180/26340,
   тот же движок, что уже используют `check_call_bounds` для free-fn'ов и
   `check_method_call_bounds` для префикс-generic ресиверов, Plan 15 Ф.3 /
   Plan 101.2) — единая функция «удовлетворяет ли concrete-тип бáунду»,
   даёт внятное сообщение `type X does not satisfy \`P\` bound (...)` с
   перечнем недостающих методов протокола (проверено фикстурой
   `neg_no_compare_bound.nv` — тот же паттерн сообщения уже используется
   как `EXPECT_COMPILE_ERROR`-маркер).
4. Механизм №295 (`infer_call_ret_c` B08, emit_c.rs) — отбор кандидата
   среди N перегрузок ОДНОГО имени по признаку «чей бáунд реально
   удовлетворён» — используется здесь КАК ОБРАЗЕЦ АЛГОРИТМА (не как код):
   моя функция при ≥2 кандидатах в `method_table` для (receiver-база,
   имя-метода) перебирает всех, унифицирует форму КАЖДОГО, и если ХОТЯ БЫ
   ОДИН кандидат и по форме подходит, и по бáунду удовлетворён — вызов
   легитимен (не ошибка); ошибка — только если НИ ОДИН подходящий по форме
   кандидат бáунду не удовлетворяет.

**Алгоритм `check_receiver_carrier_bounds(obj, method_name, span, scope, errors)`:**

1. `infer_arg_ty(obj, scope)` → receiver type; peel `Readonly/Mut/Uninit`.
2. Non-`Named` receiver → skip (бесполезно для carrier-бáундов).
3. `method_table.get(base_type_name)` → `overloads.get(method_name)`.
4. Кандидаты = только `ReceiverKind::Instance` с непустым `carrier_bounds`
   (методы без carrier-бáунда не в скоупе этой проверки — их обслуживает
   существующий `check_receiver_shape_match`/ничего).
5. Если кандидатов нет — return (нет carrier-бáунда на этом методе).
6. Для каждого кандидата: structural unify (та же пара
   canonicalize+unify_type, что и `check_receiver_shape_match`) декларации
   receiver'а против фактического — если unify падает, кандидат не тот по
   форме (не относится к этому вызову, пропустить). Если unify успешен —
   собрать `subst`, прогнать `check_satisfaction` по каждому
   `carrier_bounds`-элементу, чьё имя есть в `subst`.
7. Если хотя бы один кандидат прошёл (форма+бáунд) — вызов легитимен,
   ничего не эмитим. Если ни один подходящий по форме кандидат бáунд не
   удовлетворяет — эмитим диагностики ПЕРВОГО такого кандидата (тот же
   `check_satisfaction`-текст, без нового кода сообщений).

**Параметризованный бáунд (`[S Iter[T]]`):** `check_satisfaction` уже
извлекает `bound_name` из `TypeRef::Named{path,..}` НЕЗАВИСИМО от
`generics` бáунда (`path.len()==1` не требует пустых generics) — параметр
протокола (`T` в `Iter[T]`) игнорируется, проверяется структурное наличие
метода `iter`/`next` и т.п. — не нужен отдельный код, второй neg-фикстурой
просто проверяется, что это ветка тоже срабатывает.

**Риск вскрытия нарушений в корпусе:** ожидаемо — `Vec[T Compare]
@is_sorted/@binary_search/@compare/@sort*`, `Vec[T Clone] @clone`,
`HashMap[K Clone, V Clone] @clone`, `Set[T Clone] @clone` — первые реальные
carrier-бáунды, которые эта проверка коснётся. План — включить, прогнать
`nova check std/src`, классифицировать вскрытые нарушения (если найдутся),
чинить в этой же волне; если счёт «десятки» — честный стоп со списком.

## Статус по шагам (обновляется по ходу)

- [x] Код: `check_receiver_carrier_bounds` добавлен и подключён (types/mod.rs)
- [x] Побочная находка и фикс: `current_recv_ty` (nested `Self` passthrough в
      параметрах — `repro_param.nv`, см. ниже)
- [x] cargo build чистый (compiler-codegen debug + nova-cli release)
- [x] Фикстуры: 2 neg + 1 pos написаны и прогнаны (`nova check` + `nova
      test`, изолированно и в whole-corpus контексте) — все зелёные
- [x] 4-я фикстура (pos-regress, две подписи разными бáундами) НЕ поставлена
      в корпус — см. находку ниже (D84 блокирует identical-signature форму)
- [x] `nova check std/src` — 147/26/60, байт-в-байт канон, БЕЗ сдвига
- [x] `nova lint` на правленых .nv — 0 находок
- [x] polaris `./nova.sh test src --strict-effects` — 37/0/18, байт-в-байт канон
- [x] ratchet — δ0 (lines=64504, infer=348), emit_c.rs не тронут
- [x] Регрессия найдена и починена в ЭТОЙ ЖЕ волне (см. ниже)
- [ ] Чекпоинт-коммит (следующим шагом)

## Находки в ходе волны

1. **Найдена и исправлена в этой же волне: `Self`-passthrough false positive.**
   Проверка нового `check_receiver_carrier_bounds` регрессировала
   `spec_tests/conformance/repro_param.nv` (существующая, невиновная
   фикстура): `fn MapItP[I Next[T], T, U] mut @pair_with(mut g
   FiltItP[Self, U]) -> Vec[U] => g.drain()` — `Self` внутри ТИПА параметра
   `g` (ссылка на receiver ОБРАМЛЯЮЩЕГО метода, `MapItP[I,T,U]`) доходил до
   `check_satisfaction` НЕ разрешённым — как тип, буквально названный
   `"Self"`, которого нет в `method_table` → ложное «Self does not satisfy
   Next». Этот чекер-проход (`BoundCtx`, `types/mod.rs`) в отличие от
   `TypeCheckCtx` НИКОГДА не сеет `"@"` в scope — обнаружено эмпирически
   (трассировкой), не по интуиции. Фикс: новое поле `current_recv_ty`
   (RefCell, по образцу `current_fn_gs`) — символический тип receiver'а
   текущей fn (`Named{type_name, generics: r.generics}`), выставляется в
   `check_module`'s Item::Fn ветке; `check_receiver_carrier_bounds`
   подставляет его вместо буквального имени "Self" через
   `const_fn_trampoline::subst_type_ref_pub` перед унификацией. Верифицировано:
   `repro_param.nv` + `arity_overload_concrete_vs_bound_generic_closurefull.nv`
   (тот же compile-unit, весь `spec_tests/conformance/` целиком — 2/0/130
   warn, байт-в-байт как на неизменённом baseline).

2. **Наименование-коллизия (моя ошибка, исправлена):** фикстура `pos`
   изначально объявляла `type Named` — уже существующее имя в
   `pos_impl_debug.nv` (тот же co-equal-file модуль `spec_tests.conformance`)
   → `E_DUPLICATE_NAME`. Переименовано в `M303Named`.

3. **Отдельная находка, НЕ фиксилась (вне объёма этого окна):** запрошенная
   брифом 4-я фикстура («две подписи с одним именем метода, разными
   carrier-бáундами, вызов выбирает удовлетворённую») НЕ конструируема как
   компилируемая программа сегодня:
   - При ИДЕНТИЧНОЙ сигнатуре (0 параметров, тот же return-type, отличие
     ТОЛЬКО в carrier-бáунде) — существующий, не связанный с №303 гейт D84
     («duplicate definition ... overload requires distinct param types,
     arity, или return type») отклоняет объявление ДО того, как мой код
     вообще успевает сработать. Подтверждено: `fn M303Multi[T Display]
     @describe() -> str` + `fn M303Multi[T Compare] @describe() -> str` →
     `E_DUPLICATE...` (D84 не делает скидку на carrier-бáунд).
   - Если различить перегрузки искусственным квалификатором (`mut`), D84
     пропускает объявление, ЧЕКЕР (мой новый код) корректно СОГЛАШАЕТСЯ на
     ОБА call site (не ложно отклоняет) — но CODEGEN (emit_c, легаси,
     ВНЕ объёма этого окна по §0/196) диспетчеризует НЕ по бáунду вообще:
     реальный C-билд даёт CC-FAIL (`nova_int`-подобная путаница C-типов —
     тело ОДНОЙ перегрузки эмитится для ресивера, которому подходит ДРУГАЯ).
   - Вывод: истинно «bound-only differentiated overloading» (план 246,
     `Vec[T] @push(v T)` / `Vec[T consume] @push(consume v T)`) требует ДВУХ
     отдельных, более глубоких фиксов вне этого окна — (a) D84 должен
     научиться считать carrier-бáунд частью сигнатуры (или явно разрешить
     это конкретно для `consume`, per D156), и (b) codegen (emit_c dispatch,
     `emit_call`) должен получить bound-aware отбор кандидата — сегодня
     дispatch слепой к бáунду там, где чекер уже (после этого окна) честно
     проверяет вызывающую сторону. №303 закрывает ТОЛЬКО чекер-энфорсмент
     (тема брифа); (a)/(b) — задел для следующего окна плана 246, номер не
     присваивался (по правилу брифа).
