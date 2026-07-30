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

## Ratchet: комментарий пришлось урезать

Первая версия фикса добавила развёрнутый doc-комментарий (+32 строки) —
`arch-ratchet.sh` упал (`lines=63783 > baseline=63751`). Обоснование убрано
из кода в commit message + этот файл + реестр; итоговый диф emit_c.rs —
только код (2 insertions), `wc -l` == 63751 == baseline. Ratchet зелёный
БЕЗ правки baseline.

## Итоговые вердикты (дословно)

1. Мин-репро `scratch159/` (a_reflect.nv + b_newtype.nv, один module/CU):
   RED до фикса (`CC-FAIL scratch159/a_reflect # ... undefined symbol:
   Tagged`) → GREEN после (`PASS: 1  FAIL: 0`).
2. `nova test spec_tests/conformance/reflect_sum_reprs_pos.nv` (тянет весь
   conformance-CU): `undefined symbol: Tagged` ОТСУТСТВУЕТ (проверено ДО и
   ПОСЛЕ урезания комментария — идентично). Строка дословно:
   `PASS: 0  FAIL: 1` — CC-FAIL сменился на ДРУГОЙ, ранее замаскированный
   дефект (см. ниже «Попутный дефект»).
3. Точечный набор (reflect_sum_reprs_pos + v3_generic_newtype_non_ptr_
   inner_ok + reflect_containers_pos + v3_user_generic_newtype_ok, один
   folder/CU): `PASS: 1  FAIL: 0`.
4. `nova check std/src`: `PASS: 147  FAIL: 26` — ровно 26, без изменений.
5. `bash scripts/guards/arch-ratchet.sh`: `lines=63751 <= 63751`,
   `infer=348 <= 348` — зелёный, baseline не тронут.
6. `nova build examples/flagship/aggregator/src/main.nv --strict-effects`:
   `built: .../main.exe (26.12s)`.

## Попутный дефект (НЕ исправлен, вне рамок №159)

Проверка 2 (весь conformance-CU) вскрыла ДРУГОЙ, ранее замаскированный
CC-FAIL (маскировка тем же механизмом, что и №158: `undefined symbol:
Tagged` скрывал его до этого фикса):

```
spec_tests/conformance/reflect_sum_reprs_pos.c:144180:15: error:
initializing 'nova_unit' with an expression of incompatible type
'Nova_TypeShape *'
```

Локальная переменная-scrutinee `_nv_scr_13737` в теле теста-инстанса
`..._222_8_D438__2409` объявлена как `nova_unit` вместо `Nova_TypeShape*`
для ТОЙ ЖЕ конструкции `ReflShape.reflect()`, которая в другом месте того
же огромного CU (напр. в моём `scratch159/a_reflect.nv`, соло) резолвится
верно. `Nova_ReflShape_static_reflect` объявлена и определена корректно
(`Nova_TypeShape*`, строки 20205/84682 .c) — расходится именно ТИП
ЛОКАЛЬНОЙ ПЕРЕМЕННОЙ в этом ОДНОМ call-site (тест №2409 из тысяч в мега-CU,
номер — счётчик де-дупликации имён, не признак повторного копирования).
Похоже на order/scale-зависимый сбой в резолве возвращаемого типа
static-метода `.reflect()` при тысячах конкурирующих одноимённых
static-методов в одном CU (last-wins/stale-cache гипотеза, не проверено
глубже — вне рамок задания). Кандидат для НОВОЙ записи реестра 221.1;
здесь только зафиксирован факт находки, воспроизводится строго через
полный conformance-CU (в scratch159/ с двумя файлами эта форма не
проявляется — нужен масштаб).
