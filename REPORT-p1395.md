# REPORT p1395-value-self — №1395 и №1405 (облачная сессия, 2026-09-30)

Ветка `p1395-value-self` от `origin/main` 822b309; перед сдачей `git fetch origin` — `main` не сдвинулся (всё ещё 822b309), сливать было нечего. «База» ниже — бинарь, собранный из 822b309; «ветка» — бинарь этой ветки. Все «до/после» сняты переключением бинаря на одном и том же дереве фикстур/проб.

КОРЕНЬ: у задач РАЗНЫЕ корни, а цель №1405 (атрибут на `TypeKind`) держит ещё и третий, найденный здесь.
- №1395: ветка `"Self"` опускания типа (`resolved_named_to_c`, `compiler-codegen/src/codegen/emit_c.rs`) отдавала `receiver_c_type` — у value-типа указатель-носитель `NovaValue_X*`/`NovaTuple_X*` — ЛЮБОМУ `Self` метода экземпляра (параметр, `-> Self`, аннотация, аргумент дженерика). Спека даёт носитель только `-> @` (`spec/decisions/02-types.md:3846-3847`, D326 R1; `:3882` — `-> Self` owned-by-caller). Доказательство: на базе 11 из 14 проб клеток — CC-FAIL ровно теми сообщениями из строки реестра; на ветке все зелёные с верными ответами, `-> @`-цепочки и кучевой контроль не изменились.
- №1405: НЕ `Self` и не порядок методов как таковой. Хелпер `nova_opt_eq_<V>` для `Option[V]`, V — value-запись (`NovaValue_TypeDef`), строился в РАННЕЙ зоне (`novaopt_typedefs_buf`) под флагом `novaopt_early_gen` («тел структур и прототипов методов ещё нет»). Поле кучевой суммы (`TypeKind` — куча `Nova_TypeKind*`: забор A4, она элемент `[]`) там сравнивалось одним из двух неверных способов: `@equal` уже зарегистрирован → ВЫЗОВ до прототипа (C неявно объявляет → «static declaration of 'Nova_TypeKind_method_equal' follows non-static declaration», строка 2112 `main.c` самосборки); ещё не зарегистрирован → сравнение ПО АДРЕСУ, молча (`Some(Def{kind: K.A}) == Some(Def{kind: K.A})` из двух конструкций → `false`). «Межмодульность» лишь решает, какая из двух форм выпадет. Доказательство: C самосборки (`--keep-artifacts`), пробы и фикстуры ниже.
- Третий (найден здесь, НЕ чинился, строка `№TBD` заведена): с `#impl(Equal)` на `TypeKind` самосборка Карины на ветке ПРОХОДИТ, но 23 из 32 модульных тестов novac красные. C с атрибутом и без отличается ТОЛЬКО синтезированным `Nova_TypeKind_method_equal` (тело верное) и его вызовами; неверные ответы дают вызовы, где операнд — НЕ `TypeKind`: `novac/src/sem/collect.nv:428` `ro k = c.kind_of()` эмитится `Nova_TypeKind* k = Nova_Node_method_kind_of(c)` (`Node.kind_of -> NodeKind`, а тип взят у `Interner.kind_of -> TypeKind`). Без атрибута ошибка невидима: `==` сравнивал `->tag` через приведение, тег по тому же смещению.

ФИКС:
- `compiler-codegen/src/codegen/emit_c/self_value.rs` (новый) — `self_value_c` / `self_ret_c` / `fluent_ret_c` / `collect_fluent_ret_spans`: `Self` метода экземпляра — сам тип (у value-типа значение); носитель только у `-> @`, распознанного по span-у синтезированного парсером `Self` (пре-пасс по `returns_receiver`; `Self`, уже связанный подстановкой, не трогается).
- `emit_c.rs` ветка `"Self"` — `Some(recv) => self.self_value_c(recv)` вместо `receiver_c_type`.
- `emit_c.rs` `type_ref_to_c` — одной строкой спрашивает `fluent_ret_c` раньше общего пути.
- `emit_c.rs` регистрация сигнатуры метода (ветка `-> Self`) — `self_ret_c(.., f.returns_receiver)`.
- `emit_c.rs` тела протоколов по умолчанию (`try_emit_default_body_candidates`) — подстановка `Self` и `Self`-параметры переведены на значение.
- `compiler-codegen/src/protocols/auto_derive.rs` `synthesize_clone` — у именованного кортежа тело `@clone` строит позиционный конструктор `T(..)`, а не запись-литерал (та давала `nova_alloc(sizeof(Nova_T))`, «use of undeclared identifier 'Nova_T'»; найдено при проверке третьего value-вида, тот же класс).
- `compiler-codegen/src/codegen/emit_c/opt_eq_split.rs` (новый) — `emit_opt_eq_split`: у составной by-value нагрузки прототип `nova_opt_eq_*` рано (рядом с typedef), тело поздно (`novaopt_eq_fns_buf`, после тел структур и прототипов методов), как уже делали соседние ветки; оба сайта `register_novaopt_decl[_forced]` идут туда; флаг `novaopt_early_gen` и два его отступа к идентичности в `emit_field_eq` удалены как мёртвые.
- Второго пути не заведено: обёртки `==`/`<` (`nova_vr_ueq_*` / `nova_vr_binop_*`) уже брали `arg_is_ptr` из `param_c_types` + `method_byref_flag` и подстроились сами (теперь `b`, а не `&b`); преобразователь vtable (`is_self_typeref`) не тронут — value-тип в протокольный бокс не попадает (`emit_protocol_vtable_companion` требует `*`-тип), он видит только кучевые `Nova_X*`.

КЛЕТКИ: (база -> ветка)
- Equal × value-запись × `==`/`!=` -> PASS / PASS
- Equal × value-запись × `a.equal(b)` -> CC-FAIL / PASS
- Clone × value-запись × `a.clone()` -> CC-FAIL / PASS
- Compare × value-запись × `<`, `>=` -> CC-FAIL / PASS
- Compare × value-запись × `a.compare(b)` -> CC-FAIL / PASS
- Equal × сумма без payload × `==`/`!=` -> PASS / PASS
- Equal × сумма × `a.equal(b)` -> CC-FAIL / PASS
- Clone × сумма × `a.clone()` -> CC-FAIL / PASS
- Compare × сумма × `<`, `>=` -> CC-FAIL / PASS
- Compare × сумма × `a.compare(b)` -> CC-FAIL / PASS
- `fn T @m(other Self) -> Self`, прямой вызов, запись -> CC-FAIL / PASS
- то же, сумма -> CC-FAIL / PASS
- `-> @`-цепочка + `ro v Self` + `Vec[Self]` в теле, запись -> CC-FAIL / PASS
- `mut @m() -> @` у именованного кортежа и у суммы -> PASS / PASS
- именованный кортеж: `other Self` / `-> Self` / `a.equal(b)` -> CC-FAIL / PASS
- именованный кортеж: `#impl(Clone)` + `a.clone()` -> CC-FAIL / PASS
- контроль: кучевая запись, те же клетки -> PASS / PASS
- метод протокола по умолчанию с `Self` на value-записи / сумме / кортеже -> CC-FAIL / CC-FAIL («no member named 'near'» — вызов вообще не диспатчится; отдельный дефект, строка `№TBD`); на куче -> PASS / PASS
- обёртки `==` (`nova_vr_ueq_*`) — все `==`/`!=` клетки выше зелёные на ветке; vtable — `spec_tests --filter vtable` PASS.

№1405: минимальная проба — три модуля (`spec_tests/conformance/standalone/p1405_opt_eq_late/`): `p1405_kind` (`#impl(Equal) export type P1405Kind enum` + функция, отдающая `[]P1405Kind`, — забор A4 держит сумму в куче, как `TypeKind`), `p1405_def` (`export type P1405Def value { kind P1405Kind, n int }` и `Option[P1405Def] ==` в ТЕЛЕ функции), главный с тестами. База: CC-FAIL «static declaration of 'Nova_P1405Kind_method_equal' follows non-static declaration»; ветка: PASS. Вторая форма (`Option[V]` назван в сигнатуре → хелпер регистрируется раньше `@equal`, `p1405_opt_eq_identity.nv`): база RUN-FAIL (равные значения неравны), ветка PASS. Самосборка novac с `#impl(Equal)` на `TypeKind` командой CI (`nova build novac/src/main.nv -o novac/target/novac`): база — нет (та же ошибка на `Nova_TypeKind_method_equal`); ветка — СОБИРАЕТСЯ, НО модульные тесты novac с атрибутом 23/32 красные из-за третьего дефекта (см. КОРЕНЬ), поэтому коммит атрибута f458d1d откатан коммитом ec26296 — тег 0.2 по этому пункту пока держит новая строка `№TBD`. Строки №1405 в клоне нет — не заводил; этот абзац — её содержание.

ФИКСТУРЫ:
- `spec_tests/conformance/standalone/p1395_value_self_record.nv` (+ кучевой контроль)
- `spec_tests/conformance/standalone/p1395_value_self_sum.nv`
- `spec_tests/conformance/standalone/p1395_value_self_tuple.nv`
- `spec_tests/conformance/standalone/p1405_opt_eq_late/` (`p1405_opt_eq_late.nv`, `p1405_kind/p1405_kind.nv`, `p1405_def/p1405_def.nv`)
- `spec_tests/conformance/standalone/p1405_opt_eq_identity.nv`
Каждая: база красная, ветка зелёная.

КРЕЙТ: compiler-codegen --lib 1275/0 (дважды: после правки `emit_c` и после правки `auto_derive`).

ХРАПОВИК: emit_c строк 68813 (база — scripts/guards/arch-ratchet.baseline: 68857; до правки было 68843).

ПРОЧИЕ ПРОВЕРКИ:
- `nova test spec_tests --filter` equal / eq / clone / compare / protocol / derive / vtable / self / fluent / value_rec / vr_ / opt_eq — все 0 FAIL.
- Выборка по содержимому (всё с `Self` или `#impl(` в `spec_tests/conformance/standalone` и std-тестах, плюс `std/src/time/*_test.nv` и std-тесты с `#impl(Clone)`): 52 PASS / 1 FAIL — `std/src/time/overflow_safe_test.nv`, тот же отказ на базе, уже записан (план 200, backlog-followups).
- `novac/src/*/*_test.nv` без атрибута на ветке: 32 из 32 PASS (так же, как шаг CI `novac module tests`); с атрибутом — 9 из 32.

КОММИТЫ:
- 5804a4a fix(#1395): an instance method's Self is the type itself; only -> @ keeps the receiver carrier
- 5cfc2c8 fix(#1405): the nova_opt_eq body of a composite by-value payload is built late
- f458d1d novac: TypeKind carries #impl(Equal) (D363) - unblocked by #1405
- ec26296 novac: TypeKind drops #impl(Equal) again - the self-build links, the module tests do not
- (последний) этот отчёт

ОТКРЫТО:
- `#impl(Equal)` на `TypeKind` не стоит: держит новая строка `№TBD` (К1, «локальная от вызова метода типизируется чужим одноимённым методом»); минимальная проба вне Карины не доведена (две-три модульные пробы, в т.ч. со вторым `type Node`, не воспроизвели), улики — в строке.
- Строка `№TBD` (К1): методы протокола по умолчанию не диспатчатся на value-получателе (запись, сумма, кортеж) — не чинилось; тела по умолчанию для value-типов уже готовы к значению.
- В слитом CU `spec_tests/conformance` (1185 файлов — я по ошибке зацепил его выборкой, прервал на трети) два RUN-FAIL с SEGV без названного виновника: `mvinfer_option_map_method_value`, `p386_type_decl_bound_turbofish_pos`. Каждый отдельно — PASS и на базе, и на ветке. Сверить слитый CU с базой я не мог (мега-CU — только интегратор).
- Большие (>16 Б) value-записи в `other Self` передаются по значению: карта by-ref методов (`build_method_byref_map`) опускает `Self` вне контекста получателя и не помечает его. Корректно, но без оптимизации by-ref — как и до правки.
- Строки №1405 в реестре нет — её содержание в абзаце «№1405» выше.

ВОПРОСЫ ИНТЕГРАТОРУ:
- Слитый CU `spec_tests/conformance`: два SEGV (`mvinfer_option_map_method_value`, `p386_type_decl_bound_turbofish_pos`) на ветке. Варианты: (а) прогнать мега-CU на ветке и на `main` и сравнить; (б) считать шумом слитого CU. Рекомендую (а): правка меняет C-тип `Self` у value-типов по всему std, отдельно эти файлы зелёные, но в слитом CU виновник не назван — проверить может только ваш прогон.
- Коммиты f458d1d + ec26296 (атрибут и его откат) при слиянии можно сквошнуть или оставить: откат несёт объяснение, почему атрибута нет. Рекомендую оставить — сообщение ec26296 — единственная запись о том, что самосборка уже проходит, а держат тесты.
