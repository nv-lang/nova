<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1242 -- `@out.len() > 0` абсолютный, не относительный

## Как это было найдено

Изначально проявлялось как ICE (`channel.nv:409: no type recorded for
this node`) при self-check batch на `novac/src/check/match_arms.nv` --
несколько окон (Карина + помощник) искали причину несколькими раундами
инструментированных пробников. Финальный, подтверждённый корень: два
места в `match_arms.nv` (`@type_match_stmt`, `@type_match`) сравнивали
`@out.len()` с ЛИТЕРАЛЬНЫМ `0` вместо метки, взятой перед конкретным
вызовом, который проверяется.

## Минимальное репро -- не ICE, а ТИХОЕ РАСШИРЕНИЕ подмножества

ICE-версия требовала self-check batch (сложно воспроизвести коротко).
Тот же корень даёт короче воспроизводимый, более серьёзный по классу
симптом: **novac перестаёт проверять исчерпываемость match'а после
ЛЮБОГО более раннего отказа в том же файле** -- то есть тихо принимает
программу, которую оракул отказывает (класс К-C, "форма разрешена шире
языка").

`widen.nv.txt`:

```nova
module m

type S enum A | B

fn first(s S) -> int {
    match s {
        A => 1
    }
}

fn second(s S) -> int {
    match s {
        A => 1
    }
}
```

Оба матча пропускают вариант `B` -- оракул отказывает ОБА
(`E_MATCH_NON_EXHAUSTIVE`).

## Замер ДО фикса

```sh
git checkout -- novac/src/check/match_arms.nv   # baseline
# пересборка, затем:
./novac/target/novac_baseline.exe check widen.nv
```
Один диагноз (только `first`'s матч):
```
{"id":"d0",...,"message":"match on a sum leaves a variant uncovered ...","poisoned_by":null}
```
`second`'s ИДЕНТИЧНЫЙ пропуск варианта `B` -- НЕ НАЗВАН вовсе. Причина:
`@type_match`'s собственная проверка сразу после типизации скрутини --
`if @out.len() > 0 { return }` -- истинна уже потому, что `first`'s отказ
СУЩЕСТВУЕТ в файле (любой, не только про этот же матч), и `second`'s
матч возвращается ДО того, как дойдёт до собственного
`@report_if_applied_sum`/фолда армов/`@report_if_not_exhaustive`.

## Замер ПОСЛЕ фикса

```sh
./novac/target/novac.exe check widen.nv
```
Два диагноза -- по одному на каждую функцию, оба про непокрытый `B`.

## Корень, точно

`novac/src/check/match_arms.nv`, ДВЕ функции (`@type_match_stmt` строка
~80, `@type_match` строка ~126, номера до фикса): обе делают
```nova
@type_expr(scr)
if @out.len() > 0 { return }
```
сразу после типизации скрутини, и СИММЕТРИЧНО, ещё раз, после цикла по
армам (строки ~90/~137):
```nova
if @out.len() > 0 { return }
@report_if_not_exhaustive(kids, scr_t)
```
`Checker.out` копится на ВЕСЬ ФАЙЛ (одна структура `Checker` на один
`check()`-прогон, не на один матч) -- поэтому `> 0` истинно, если В ФАЙЛЕ
было ХОТЬ ЧТО-ТО отказано РАНЬШЕ, а не только если СКРУТИНИ/АРМЫ ЭТОГО
МАТЧА дали отказ. Каждый из 26+ похожих гейтов в `check/` (`exprs.nv`,
`typing.nv`, `binds.nv`, `calls.nv`, `casts.nv`, `destructure.nv`,
`rules.nv`, `variant_rules.nv`) берёт метку ПЕРЕД вызовом и сравнивает
`> mark` -- эти два места в `match_arms.nv` были единственными,
сравнивавшими с буквальным `0`.

## Фикс

Обе функции, оба места: метка `before`/`before_arms` берётся ПЕРЕД
соответствующим вызовом, сравнение -- `@out.len() > before`.

## Оговорка про носителя

Радиус: КАЖДЫЙ файл с матчем ПОСЛЕ хотя бы одного отказа где-либо раньше
в том же файле терял типизацию/проверку исчерпываемости этого матча
молча. Проверены и починены только эти ДВА конкретных места
(`match_arms.nv`); та же СХЕМА поиска (`grep -n "@out\.len() > 0"`) даёт
и другие кандидаты в `match_arms.nv` (строка ~519, `@type_arm`'s
собственный аналогичный гейт) и в `rules.nv` (~572, ~808) -- НЕ
проверены и НЕ починены в этой волне: они относятся к ДРУГИМ вопросам
(тип отдельного арма, generic-декларация), не к exhaustiveness/ICE,
найдены попутно и заведены отдельной строкой реестра (№TBD),
самостоятельный фикс отложен.

## Контроль отсутствия регрессии

- `novac/src/check/check_test.nv`, `novac/src/parse/parse_test.nv`,
  `novac/src/lex/lex_test.nv` -- все PASS.
- Тест-свидетель добавлен в `check_test.nv`: "match exhaustiveness: a
  later match is still checked after an earlier one's own refusal
  (registry #1242)".
- Первичный ICE-репро (`novac.exe check novac/src/check/match_arms.nv`
  под `NOVAC_SELF_PATH=novac/src`) больше не крашится -- даёт честные
  E_NOVAC_SUBSET диагностики вместо `E_NOVAC_ICE`.
