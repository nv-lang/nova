<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1243 -- `else if` цепочка в value-позиции не парсилась

## Минимальное репро

`else_if_value.nv.txt` -- носитель из карты 87 отказов самопроверки
(`novac/src/check/literal_rules.nv`, форма УТФ-8-декодера), сведённый до
минимума:

```nova
fn m(head int) -> int {
    ro want = if head < 128 {
        1
    } else if head >= 240 {
        4
    } else if head >= 224 {
        3
    } else {
        2
    }
    0
}
```

Прогон:

```sh
cp docs/plans/repro/1243-else-if-value/else_if_value.nv.txt D:/Temp/eiv.nv
./novac/target/novac.exe check D:/Temp/eiv.nv
```

## Корень, часть 1 (парсер)

`novac/src/parse/expr.nv`, value-`if` reader (`@atom()`'s `KwIf` branch):
`else`-ветка проверяла ТОЛЬКО `LBrace` -- ветки на `KwIf` (продолжение
`else if`) не было вовсе, в отличие от соседнего, уже рабочего
statement-`if` reader (`@stmt()`), который на этом же месте рекурсирует
на `KwIf` для построения цепочки. На `else if` парсер ставил
`missing_of(ErrorTok)` вместо блока, `IfExpr` обрывался на двух ветках, а
непотреблённый хвост (` if head >= 240 { ... } ...`) рассыпался дальше по
конвейеру безымянным фоллбеком.

**Фикс:** ветка `KwIf` добавлена в `else`-руку value-`if` reader'а,
рекурсивно вызывающая тот же reader (`@atom()`), которым была построена
внешняя `IfExpr` -- зеркало `@stmt()`'s собственной рекурсии на `KwIf`
для `else if`.

## Корень, часть 2 (чекер) -- НАЙДЕН ПРИ ПРОВЕРКЕ, парсер-фикса оказалось недостаточно

После фикса парсера дерево строилось верно, но чекер давал НЕ то честное
сообщение, которого ждала находка (`typing.nv:525`, "an `else if` chain is
not compiled as a value yet"), а более общее и вводящее в заблуждение
`IF_EXPR_MSG` ("an `if` in value position is not compiled yet") -- и
указывало на ВНУТРЕННИЙ `if` цепочки (`else if head >= 240`), а не на
внешний.

**Причина:** `novac/src/check/check.nv`, субсетный walk. `IfExpr` легален
в `init_pos` (первый `if` -- прямой инициализатор `ro want = ...`), но
когда walk "продолжает в детей как у любого узла", он делает это ОБЩЕЙ
рекурсией (строки ~974-996), которая помечает позиции `stmt_pos`/
`arg_pos`/`index_pos`/`arm_pos` для разных нужд, но НИКОГДА не
передаёт `init_pos` дальше. Вложенный `IfExpr` (сам `else if`,
сидящий в else-плече родителя) навещается со свежим `init_pos=false`
и отказывается ОБЩИМ `IF_EXPR_MSG`, даже не добравшись до
`@type_branch_value`'s специфичного сообщения про цепочку.

**Фикс:** та же рекурсия получила ветку `init_pos: init_pos &&
kind == NodeKind.IfExpr && at == children.len() - 1 && c.kind_of() ==
NodeKind.IfExpr` -- `else if` это ОДНО выражение с родителем, а не два,
и наследует его позицию.

## Замер ДО фикса парсера

Косвенно (через каскад): `t[..2]`-класс тот же самый паттерн (два
отказа безымянного фоллбека вместо одного честного), задокументирован в
`docs/plans/repro/1241-slice-open-left/README.md`. Прямой замер для
`else if` в value-позиции сделан ПОСЛЕ фикса парсера, до фикса чекера
(см. ниже) -- до фикса ПАРСЕРА исходный отказ выглядел так же:
непотреблённый хвост `else if head >= 240 { ... }` рассыпался дальше и
портил и следующие формы в том же файле (в частности, ИМЕННО этот класс
снял ОДНУ диагностику из регрессии D49: `spec_tests/conformance/
d49_statement_separator_newlines.nv`, тест "continuation: else/else if
on next line (D49)" несёт БУКВАЛЬНО D49's canonical example
`ro label = if n < 0 {...} else if n == 0 {...} else {...}` -- счёт
диагностик этого файла УПАЛ С 6 НА 5 этим же фиксом; проверено грепом
`else if` в файле и совпадением строк 48-53 с примером).

## Замер ПОСЛЕ фикса парсера, ДО фикса чекера

```
{"id":"d0",...,"start":75,"end":77,
 "message":"outside the subset: an `if` in value position is not compiled yet",...}
```
(указывает на ВНУТРЕННИЙ `if` цепочки -- смещения 75-77 это `if` в
`else if head >= 240`, не внешний `if`.)

## Замер ПОСЛЕ фикса чекера (итог)

```
{"id":"d0",...,"start":75,"end":77,
 "message":"outside the subset: an `else if` chain is not compiled as a value yet (E2-b)",...}
```

Одна честная, именованная диагностика -- ровно то сообщение, которое
находка предвидела. Простой value-`if` без `else if`
(`ro want = if c {1} else {2}`) -- ноль диагностик, без изменений.
Голый (не-initializer) `if` вне value-позиции -- всё ещё честно
отказывается `IF_EXPR_MSG` (регрессия не тронута).

## Контроль отсутствия регрессии

- `spec_tests/conformance/d49_statement_separator_newlines.nv` -- было 6
  диагностик, стало 5 (см. «Замер ДО» выше -- падение ОБЪЯСНЕНО, а не
  просто замечено).
- `novac/src/lex/lex_test.nv`, `novac/src/parse/parse_test.nv`,
  `novac/src/check/check_test.nv` -- все PASS.
- `check-novac-grammar-kinds`, `check-novac-no-default-branch` -- зелёные
  после обеих правок.
- Тест-свидетель добавлен в `parse_test.nv`: "shape: an `else if` chain
  in value position parses as nested IfExpr (registry #1243)".

## Оговорка про носителя

Фикс носителя приёмкой не считается -- `else if`-как-значение остаётся
вне подмножества (E2-b), только диагностика стала честной,
единообразной и адресной (указывает на цепочку, а не на случайный
внутренний `if`).
