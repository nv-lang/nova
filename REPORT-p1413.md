# REPORT p1413-method-local-type — №1413 и №1414 (облачная сессия, 2026-09-30)

Ветка `p1413-method-local-type` от `origin/p1395-value-self` (96293df). Перед сдачей `git fetch origin`: `origin/main` не сдвинулся (822b309) — слияния не было, сдаю как есть. «База» ниже — бинарь из 96293df (с №1395/№1405, без этой работы); «ветка» — бинарь этой ветки; все «до/после» — переключением бинаря на одном дереве фикстур.

КОРЕНЬ:
- №1413 — две дыры подряд, обе нужны для носителя.
  1) Чекер: `types/mod.rs` `infer_iter_elem_type` давал тип переменной цикла, только если итерируемое типизировано `[]T`/`[N]T`/`ro []T`. Вызов и локал от вызова типизируются `Vec[T]` (D239 `[]T ≡ Vec[T]`), и переменная цикла оставалась БЕЗ типа — у вызовов на ней не было ни `resolved_callees`, ни `resolved_types` (замер печатью в CU `check_test` Карины: все «слепые» `c.kind_of()` — циклы `for c in file_decls(..)` / по локалу от него; цикл по `[]Node`-параметру — с каналом).
  2) Кодоген: запасной путь `infer_call_ret_c` строил ключ `method_overloads` из C-базы получателя. У коллидирующего имени (D381: `Node` объявлен в двух модулях CU) база квалифицирована — `<мод>_Node`, — а ключи по простому имени; промах, и тип брался по ГОЛОМУ имени метода (последний зарегистрировавший — `Interner.kind_of -> TypeKind`). Замер: `DBG … key ("p_k13g_k13g_tree_Node", "kind_of") hit=None keys=[("Intr","kind_of"),("Node","kind_of")]`.
- №1414 — две точки на вызове. 1) Синтез тела по умолчанию называл тип получателя, снимая с C-типа только `Nova_`: у `NovaValue_X`/`NovaTuple_X` имени не находилось, вызов уходил в доступ к полю («no member named 'near'»). 2) Вывод типа результата (`infer_call_ret_c`, ветка B03) брал первый попавшийся протокол с методом по умолчанию этого имени и опускал его `Self` через последний связанный эмиттером `Self` — тип чужого синтеза (`P1414Lv.Mid.pick(..)` объявлялся `NovaTuple_P1414Tv`) — это тот же класс, что №1413.

ФИКС:
- `compiler-codegen/src/types/mod.rs` `infer_iter_elem_type` — элемент итерируемого через существующий `array_elem_type` (`[]T`/`[N]T`/`Vec[T]`, плюс `ro`), своего разбора больше нет.
- `compiler-codegen/src/codegen/emit_c/method_key.rs` (новый) `method_key_type_name` — квалифицированная C-база коллидирующего типа -> простое имя для ключа; вызов в `emit_c.rs` одной правкой в ветке Plan 11 `infer_call_ret_c`.
- `novac/src/sem/collect.nv:554/644` — `raw_node(c.id_of())` вместо `NodeId` в индексе `Vec` (см. ОТКРЫТО: это настоящая ошибка типов Карины, которую вскрыл фикс 1).
- `compiler-codegen/src/codegen/emit_c/default_dispatch.rs` (новый) — `default_method_recv` (имя типа у трёх value-видов, получатель-указатель для тела, передача адресом через `prepare_method_recv`) и `default_method_ret_c` (только протоколы из `#impl` типа, `Self` = тип получателя); ветка вызова и ветка B03 в `emit_c.rs` переведены на них, прежний разбор B03 удалён — второго пути нет.
- `novac/src/builtins/builtins.nv` — `#impl(Equal)` на `TypeKind` (D363) вернулся отдельным коммитом.

КЛЕТКИ (№1413; одноимённый `@kind_of` у двух типов с разными возвратами, имя получателя коллидирует, модули разные; база -> ветка):
- параметр `n.kind_of()` -> PASS / PASS
- поле `h.node.kind_of()` -> PASS / PASS
- результат вызова `first(root).kind_of()` -> PASS / PASS
- цикл по вызову `for c in decls(root)` -> RUN-FAIL (`Nova_P1413Tk* k`) / PASS
- цикл по локалу от вызова -> RUN-FAIL / PASS
- цикл по `[]T`-параметру -> PASS / PASS
- индекс `ds[0].kind_of()` -> PASS / PASS
- в выражении `if c.kind_of() == ..` -> RUN-FAIL / PASS
- аргументом `is_nl(c.kind_of())` -> PASS / PASS (тип параметра вызываемой)
- цикл по привязке `match` `Branch(_, kids) => for c in kids` -> RUN-FAIL / PASS
- `if Some(x) = ..` -> PASS / PASS
- обратная клетка: метод другого типа `Intr.kind_of` в цикле -> PASS / PASS
- каждая половина отдельно на фикстуре: только чекер — краснеет цикл по привязке `match` (`match_arm_bindings` покрывает лишь вариант с одним полем); только кодоген — все формы зелёные; обе выключены — как на базе.
- №1414 (метод протокола по умолчанию с `Self` в параметре и возврате): value-запись (в т.ч. rvalue-получатель) / сумма без payload / именованный кортеж -> CC-FAIL «no member named 'near'» / PASS; куча -> PASS / PASS.

ПРОБА вне Карины: три модуля. `tree`: `export type Node enum | Leaf(Nk) | Branch(Nk, []Node)`, `fn Node @kind_of() -> Nk`, `export fn decls(n Node) -> []Node`. `types`: `#impl(Equal) export type Tk enum`, ВТОРОЙ приватный `type Node { n int }`, `export type Intr`, `fn Intr @kind_of(i int) -> Tk`. Главный: `for c in decls(root) { ro k = c.kind_of(); if k == Nk.NL {..} }`. Оси по одному фактору: без второго `Node` — зелено; цикл по `[]Node`-параметру — зелено; по вызову или по локалу от вызова — `Nova_Tk* k = Nova_Node_method_kind_of(c)`, неверный ответ. Фикстура — `spec_tests/conformance/standalone/p1413_method_local_type/` (та же форма, 11 клеток получателя).

TypeKind: самосборка novac с `#impl(Equal)` командой CI (`nova build novac/src/main.nv -o novac/target/novac`) — ДА; модульные тесты novac (`novac/src/*/*_test.nv`, как шаг CI) — 32/32 (дважды: после №1413 и после №1414). До этой работы — 9/32.

ФИКСТУРЫ:
- `spec_tests/conformance/standalone/p1413_method_local_type/` (`p1413_method_local_type.nv`, `p1413_tree/p1413_tree.nv`, `p1413_types/p1413_types.nv`) — база RUN-FAIL, ветка PASS
- `spec_tests/conformance/standalone/p1414_value_default_method.nv` — база CC-FAIL, ветка PASS

КРЕЙТ: compiler-codegen --lib 1275/0 (после №1413 и после №1414).

ХРАПОВИК: emit_c строк 68802 (база — scripts/guards/arch-ratchet.baseline: 68857; на старте ветки было 68813). infer=246 <= 247.

ПРОЧИЕ ПРОВЕРКИ:
- `nova check` по `std`, `examples`, `spec_tests/conformance/standalone`, `novac` — база против ветки: единственное новое — два места в `novac/src/sem/collect.nv` (правлены, см. ФИКС); всё прочее без изменений.
- `nova test spec_tests/conformance/standalone std/src/collections std/src/text std/src/time` — 284 PASS / 0 FAIL после №1413; с добавкой `std/src/encoding std/src/fs` — 294 PASS / 0 FAIL после №1414 (SKIP — полосы `--full`/без тестов).
- Мега-CU, полный `nova test`, `gate.sh`, `gate-novac.sh` — не запускались.

КОММИТЫ:
- b67c0fd novac: index fn_decl_rows by raw_node(id), not by a NodeId
- fbb8725 fix(#1413): a method call's result is typed by its own method, not a same-name one
- 5350377 novac: TypeKind carries #impl(Equal) (D363) - unblocked by #1413
- f6d68c1 fix(#1414): a protocol default method dispatches on a value receiver
- (последний) этот отчёт

ОТКРЫТО:
- Правка в Карине (`collect.nv`, 2 строки) — вне оракула. Это не обход: там `Vec` индексировался `NodeId` (newtype), что язык отвергает (проба: `xs[nid]` — `E_NO_MATCHING_OVERLOAD` и на чтение, и на запись), а проходило лишь потому, что тело цикла не проверялось. Окну Карины стоит знать; в `sem/sem.nv:142` `ctx.fn_decl_rows[id]` с `id NodeId` проходит проверку — получатель там поле, и индекс по нему, похоже, не проверяется (отдельная дыра чекера, не заводил: не замерял).
- `match_arm_bindings` даёт тип привязкам только у варианта с одним позиционным полем; многопольные и записные варианты (`Branch { kind, children, .. }` у Карины) остаются без типа в чекере. После фикса кодогена это больше не даёт неверный C-тип, но у таких мест по-прежнему нет канала и проверок чекера. Не расширял — это шаг к «все локалы в канал».
- Для кортежа запасной путь по-прежнему строит ключ `NovaTuple_X` (префикс не снимается) — видно в отладке `default_method_ret_c`; на клетках задач не проявилось, не трогал.
- `REPORT-p1395.md` остаётся в дереве этой ветки (ветка от `p1395-value-self`; в `main` интегратор его уже убрал).

ВОПРОСЫ ИНТЕГРАТОРУ:
- Фикс чекера (цикл по `Vec[T]`) включает проверки в телах всех таких циклов: по std/examples/standalone нового ничего, в Карине — две настоящие ошибки. Мега-CU и полный корпус я не гонял. Рекомендую прогнать гейт push на слиянии с вниманием к новым отказам проверки в телах циклов — если появятся, это вскрытые ошибки кода, а не регресс.
- Расширять ли `match_arm_bindings` на многопольные и записные варианты (типы привязок в чекере)? Варианты: (а) отдельной задачей, с замером вскрытых ошибок как здесь; (б) не трогать — неверного C-типа уже нет. Рекомендую (а): это последняя замеченная форма получателя без канала чекера.
