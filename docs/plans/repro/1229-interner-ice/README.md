<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1229/1128 -- ICE в интернере типов, найденный корень и фикс

## Минимальный репро (8 строк)

`ti_final.nv.txt` -- временно кладётся НА МЕСТО `novac/src/check/binds.nv`
(модуль `novac.check`, чтобы `Checker` резолвился без импорта), прогоняется
батч-режимом:

```sh
cp docs/plans/repro/1229-interner-ice/ti_final.nv.txt novac/src/check/binds.nv
export NOVAC_SELF_PATH=novac/src
./novac/target/novac.exe check novac/src/check/binds.nv
# git checkout -- novac/src/check/binds.nv  -- ВОССТАНОВИТЬ после пробы, обязательно
```

## Замер ДО фикса

```
{"id":"ice",...,"message":"internal compiler error (novac bug, not yours):
types.nv:244: requires failed: types: kind_of asked for a type id outside
the interner (is_ty(t) && raw_ty(t) < @rows.len())"}
```

## Метод: инструментированный прогон, а не гадание

Пять кандидатов проверены и ОТВЕРГНУТЫ точечными ловушками (`is_ty`-гейт +
`ice("DEBUG-TRACED ...")` перед известным местом падения, чтобы увидеть,
КАКОЙ вызывающий добрался туда первым): `check/methods.nv:258`,
`check/exprs.nv:90,115,298`, `check/assign_rules.nv:85`,
`check/option_rules.nv:103`. Ни один не сработал -- падение происходило
дальше их всех.

Шестой кандидат, `sem/mangle.nv:423` (`c_method_symbol`), сработал точно:
метка `DEBUG-TRACED mangle.nv:423 c_method_symbol it no_ty, name=kind_of`
-- `name=kind_of` совпадает буквально с методом пробы.

## Корень

`sem/mangle.nv:401-410` (`impl_of_proto`) возвращает `same_ty(t)` --
практически неизменный `t` -- на каждом пути, где `t` не подходит под
протокол `FMT_PROTO`. Если `t` (получатель метода, тип `kids[0]` -- индекс
в `[]Node`) сам приходит уже `no_ty()` (потому что чекер НЕ РАЗРЕШИЛ тип
элемента при индексации `[]Node` -- отдельный, более глубокий вопрос
инференса типов, не тронутый этой правкой), `impl_of_proto` тихо проносит
`no_ty()` дальше. `c_method_symbol` (строка 423) звало
`ctx.tys.kind_of(it)` БЕЗ проверки `is_ty(it)` -- ICE.

## Фикс

`sem/mangle.nv:423`:

```nova
if is_ty(it) && ctx.tys.kind_of(it) == TypeKind.TkApp { return c_instance_method(ctx, it, name) }
```

(было: `if ctx.tys.kind_of(it) == TypeKind.TkApp { ... }`, без `is_ty(it) &&`).

## Замер ПОСЛЕ фикса

Та же 8-строчная проба -- ICE ушёл, два честных отказа:

```
{"message":"outside the subset: `branch_children` is declared in module
`novac.sem`, whose bodies novac does not compile into this unit yet ..."}
{"message":"outside the subset: the shell novac links into carries no
`kind_of` for an unreadable type -- novac read the signature but has no
body to emit"}
```

**НАСТОЯЩИЙ носитель** (не проба, реальный файл): `novac/src/check/binds.nv`
под `NOVAC_SELF_PATH=novac/src` -- ДО фикса падал тем же ICE; ПОСЛЕ --
доходит до конца файла, 144 обычных диагностики (E_NOVAC_SUBSET, честные,
разные причины), ни одного крэша. Это ровно та мера самосборки
(ступень 0.2), которая всю ночь 2026-09-21 стояла на артефакте краша, а не
на реальном долге подмножества.

## Контроль отсутствия регрессии

- `spec_tests/conformance/d49_statement_separator_newlines.nv` -- 6 строк
  диагностик, как до фикса.
- `novac/src/check/check_test.nv`, `novac/src/sem/mangle_test.nv` -- оба
  PASS.

## Что фикс НЕ решает

Почему `kids[0]`'s (индекс в `[]Node`) тип приходит `no_ty()` в этом
контексте -- отдельный, более глубокий вопрос инференса типов при
индексации generic-Vec в самохостном батче. Фикс переводит СЛЕДСТВИЕ
(крэш) в ЧЕСТНЫЙ ОТКАЗ; причина `no_ty()`-типа для `kids[0]` остаётся
не расследованной -- следующий шаг, если кто-то продолжит.
