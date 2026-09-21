<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1239's остатку -- `[]Point.of(a, b)`'s аргументы без `arg_pos`

## Минимальное репро

`typed_array_of.nv.txt` -- носитель того же класса, что `builtins.nv`'s
`ro universe = []BuiltinType.of(...)`:

```nova
type Point value {
    x int
    y int
}

ro pts = []Point.of(Point { x: 1, y: 2 }, Point { x: 3, y: 4 })
```

## Ловушка методики (для следующего окна)

Первая попытка репро использовала `type Point value { x int, y int }`
(поля через запятую на ОДНОЙ строке) -- это не легальный синтаксис Nova
(поля разделяются НОВОЙ СТРОКОЙ, без запятой), и давало безымянный
`novac did not parse this` за саму декларацию типа, маскируя предмет
находки целиком. Верный синтаксис -- поля на отдельных строках, как в
файле выше; без этого любой вывод про "arg_pos" был бы построен на
артефакте собственной пробы, не на реальном дефекте.

## Замер ДО фикса

```
d0: start=62 "a record constructor is compiled only as a binding
     initializer or a call argument -- other positions are not compiled
     yet" (первый Point{...})
d1: start=84 та же диагностика (второй Point{...})
```

## Замер ПОСЛЕ фикса

Ноль диагностик, `exit=0`.

## Корень

`novac/src/parse/expr.nv` строит `[]Point.of(a, b)` как ОДИН узел
`ArrayLit` (sub-plan L, 2026-08-26) -- элемент-тип, `.`, имя
конструктора и аргументы все сидят детьми ЭТОГО узла, а не внутри
вложенного `Call`/`MethodCall`. `novac/src/check/check.nv`'s общая
рекурсия помечает `arg_pos: true` только для детей `Call`/`MethodCall`/
`CtorField` -- `ArrayLit` в этот список не входит, поэтому аргументы
`.of(...)` никогда не получали позицию, которую `RecordCtor`'s гейт
требует (`init_pos || arg_pos`).

## Фикс

Новая дверь `array_of_lparen_at(kids) -> Option[int]`
(`novac/src/sem/slots.nv`, тот же файл и стиль, что `arm_body_at`) --
находит слот `(` СКАНИРОВАНИЕМ, не фиксированным индексом (форма
растёт: элемент-тип, `.`, имя конструктора -- три токена сегодня, могут
стать четырьмя). `check.nv`'s рекурсия вычисляет её ОДИН РАЗ на узел
(как `body_at` для `MatchArm`) и добавляет `kind == ArrayLit &&
of_lparen_at >= 0 && at > of_lparen_at` в условие `arg_pos`.

## Контроль отсутствия регрессии

- Обычный `[a, b]` (bracket-форма, без `.of()`) с record-конструкторами
  внутри -- ВСЁ ЕЩЁ честно отказывается (проверено отдельно: эта форма
  не JOIN'ится этим фиксом, у неё СВОЙ, отдельный, нетронутый долг
  -- array-literal-ELEMENT позиция для `RecordCtor` пока не поддержана
  вовсе, это другая находка, не эта).
- `novac/src/check/check_test.nv`, `novac/src/parse/parse_test.nv`,
  `novac/src/lex/lex_test.nv` -- все PASS.
- `check-novac-grammar-kinds`, `check-novac-no-default-branch`,
  `check-novac-file-size` -- зелёные (`slots.nv` подошёл к 1004 строкам
  после добавления двери, обрезан комментарий до 998).
- Тест-свидетель -- `check_test.nv`, "arg position: a typed array
  literal's `.of(...)` arguments get it too (registry #1239)".

## Носитель проверен напрямую

`novac/src/builtins/builtins.nv:349` (`ro universe = []BuiltinType.of(`)
-- под `NOVAC_SELF_PATH=novac/src`, ДО фикса не проверялся отдельно от
этой диагностики; ПОСЛЕ фикса `novac.exe check` на самом файле даёт
ОДНУ диагностику, и она НЕ про этот носитель вовсе -- про декларацию
типа в СОВСЕМ ДРУГОМ месте файла ("this type form is not compiled yet
(slices, tuples, fn types and qualifiers arrive with generics, E2-b)").
`arg_pos`-половина носителя закрыта полностью и проверена на реальном
файле, не только на минимальной пробе.
