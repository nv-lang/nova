# №159 — довод фикса №136 до эмиссии (worktree m136-complete-emission)

## Итог разведки (важно: переквалификация симптома)

Задание фреймировало сайт как MATCH PATTERN (`reflect_sum_reprs_pos.nv:32`:
`match repr { Tagged(tag) => ... }`). Собрал ФАКТИЧЕСКОЕ мин-репро
(`scratch159/a_reflect.nv` = копия `reflect_sum_reprs_pos.nv` c
`module scratch159`, peer-файл `scratch159/b_newtype.nv` = релевантный
фрагмент `v3_generic_newtype_non_ptr_inner_ok.nv` — `type Tagged[T,U](int)` +
тест `Tagged[Persistent, Email](999)`, тот же модуль → один CU без импортов,
как в реальном conformance-фолдере, где ВСЕ файлы объявляют один и тот же
`module spec_tests.conformance`).

Результат: `CC-FAIL scratch159/a_reflect # undefined symbol: Tagged` —
воспроизведено 1:1. НО в `--keep-artifacts` C `match repr { Tagged(tag) =>
...}` скомпилировался ИДЕАЛЬНО (`_nv_scr_1013->tag == NOVA_TAG_SumRepr_Tagged`
+ `_nv_scr_1013->payload.Tagged._0` — корректный tag-check и payload-bind,
byte-perfect). Голый unmangled вызов `Tagged(((nova_int)999LL))` нашёлся в
СОВСЕМ ДРУГОМ месте — теле теста `multi-param non-ptr generic newtype`
(из b_newtype.nv, mirrors v3-файл), где `Tagged[Persistent, Email](999)`
(TurboFish generic-newtype ctor) деградировал в голый вызов.

Т.е. CC-FAIL атрибутирован ПЕРВОМУ ПО АЛФАВИТУ файлу шарда (`a_reflect` <
`b_newtype`, как `reflect_sum_reprs_pos` < `v3_generic_newtype...` в реальном
корпусе) — ТОТ ЖЕ механизм ложной атрибуции, что и №158 (a_q3/d62). Реестр
119.1 №159 указал на НЕВЕРНЫЙ файл/сайт как источник; match-pattern парность
(предмет задания) уже РАБОТАЕТ корректно и не требует правки.

Подтверждено изоляцией: `a_reflect.nv` один — PASS; `b_newtype.nv` один —
PASS; вместе — CC-FAIL. Оба по отдельности взяты из реально существующих
conformance-тестов (`reflect_sum_reprs_pos.nv`, `v3_generic_newtype_non_ptr_
inner_ok.nv`), оба сегодня зелёные соло — коллизия ТОЛЬКО в объединении.

## Настоящий корень

`compiler-codegen/src/codegen/emit_c.rs::emit_call`, ~38212-38252 (гейт
самого №136). `name_opt` схлопывал ДВЕ РАЗНЫЕ формы вызова (`Name(args)` —
bare Ident callee — и `Name[T,U](args)` — TurboFish callee) в одно и то же
голое имя для проверки `shadowed_by_variant = debt_find_variant_ctx(name,
Some(args.len())).is_some()`. Bare-Ident форма ДЕЙСТВИТЕЛЬНО неоднозначна
(ради неё №136 и написан). TurboFish-форма НЕ неоднозначна вообще: в Nova
нет синтаксиса «вариант суммы со своим turbofish» — turbofish на голом
Capitalized-имени БЕЗУСЛОВНО значит generic-type/newtype конструктор
(D214/Plan 91.12 V2, тот же комментарий чуть выше в файле это уже
документирует: «T — type-system fiction»). Раз `Tagged[T,U](int)` и
`SumRepr.Tagged(str)` совпали по arity (оба 1 арг), `shadowed_by_variant`
для TurboFish-вызова стало `true` → newtype-identity-cast (единственная
ветка, которая знает как эмитить этот constructor) была пропущена → ни одна
другая ветка func_c-match'а (Ident/Member/Path) не подхватывает TurboFish
callee → catch-all `_ => self.emit_expr(func)?` → `emit_expr`'s TurboFish
arm делегирует в `emit_expr(base)` → голый Ident `"Tagged"` без всякого
mangling → `Tagged(999)` — undefined symbol.

Второй (меньший) вклад: `debt_find_variant_ctx`'s single-candidate путь
(`plain.len() < 2` → `find_variant_compat` unconditionally) вообще не
проверяет `argc` — но это НЕ пришлось трогать: фикс на уровне
Ident-vs-TurboFish достаточен и точнее (не трогает существующую
многокандидатную арность-дизамбигуацию №136).

## Фикс

Тот же файл/место, что и №136 (не growing новую эвристику — сузил УЖЕ
существующий гейт до формы вызова, для которой он был задуман). Заменил
`name_opt: Option<&String>` на `(name_opt, is_turbofish)`; `shadowed_by_
variant` теперь `!is_turbofish && debt_find_variant_ctx(...).is_some()`.
Byte-identical для bare-Ident вызовов (единственная форма, где №136
когда-либо был нужен).

## Дальше по плану

1. RED подтверждён (`scratch159/`, CC-FAIL undefined symbol: Tagged).
2. Пересобираю nova-cli, прогоняю scratch159 → ожидаю GREEN.
3. `nova test spec_tests/conformance/reflect_sum_reprs_pos.nv` — точная
   строка PASS/FAIL.
4. Точечный набор reflect_sum_reprs_pos + v3_generic_newtype_non_ptr_
   inner_ok + пара соседей.
5. `nova check std/src` — ожидание FAIL: 26 (без изменений).
6. `bash scripts/guards/arch-ratchet.sh`.
7. Флагман aggregator `--strict-effects`.
