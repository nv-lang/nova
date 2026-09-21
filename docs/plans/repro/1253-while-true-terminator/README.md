<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1253 -- `while true { return }` не признан терминатором

## Находка

НАЙДЕНО ОХОТНИКОМ 2026-09-21 (novac, клетка `check`×К6 -- честно не К4:
второй двери здесь нет, есть неполный список), отчёт
`docs/dev/hunts/novac/2026-09-21-check-k4.md`, раздел Н4, проба
`p10-while-true-return`. Родня №1189 (там в списке терминаторов не
хватало `IfLet`).

## Минимальное репро

`while_true.nv.txt`:

```nova
fn f(n int) -> int {
    while true {
        return n
    }
}
```

Оракул принимает; novac отказывал: «fn declares a return type but its
body ends without a value».

## Замер ДО фикса

```
{"...", "message":"fn declares a return type but its body ends without
 a value", ...}
```

## Замер ПОСЛЕ фикса

Ноль диагностик, `exit=0`.

## Корень

`novac/src/check/tail_rules.nv`, `stmt_terminates(st Node) -> bool` --
перечисляет ФОРМЫ, которые завершают функцию на каждом пути
(`RetStmt`, `IfLet`, `IfStmt`), а не отвечает на СВОЙСТВО «управление не
проходит мимо этой строки». `while` не попал в список ни разу.

## Фикс, и почему он БЕЗОПАСЕН без обхода тела

`while true { ... }` (условие -- буквальный `true`) не может передать
управление ДАЛЬШЕ ЭТОЙ строки ни при каких обстоятельствах:
* условие никогда не станет ложным (это буквальный `true`, не
  вычисляемое выражение);
* выйти через `break` нельзя -- грепом по `novac/src/tree/`,
  `novac/src/parse/`: `BreakStmt`/`KwBreak` НЕ СУЩЕСТВУЕТ в дереве
  novac вовсе, значит `break` не пропускается парсером в принципе.

Значит либо тело когда-нибудь исполнит `return`, либо цикл продолжится
навсегда -- в обоих случаях контроль НЕ возвращается к строке после
`while`. Фикс проверяет РОВНО условие (`cond.kind_of() == NodeKind.Name
&& expr_leaf_name(cond) == "true"`), не заглядывая внутрь тела вовсе --
не требуется.

**Оговорка, названная явно в коде:** `while <не-true-условие> { ... }`
НЕ считается терминатором -- условие может стать ложным, и это ровно
тот же случай, что `if` без `else` (падение сквозь). Контроль в тесте
(`while n > 0 { return n }`) подтверждает: этот случай остаётся честно
отказанным.

## Контроль отсутствия регрессии

- `novac/src/check/check_test.nv`, `novac/src/parse/parse_test.nv`,
  `novac/src/lex/lex_test.nv` -- все PASS.
- `check-novac-grammar-kinds`, `check-novac-no-default-branch`,
  `check-novac-file-size` -- зелёные.
- `spec_tests/conformance/d49_statement_separator_newlines.nv` -- 5
  диагностик, без изменений.
- Тест-свидетель -- `check_test.nv`, "termination: an infinite `while
  true { return }` is a terminator, no dead-tail refusal (hunt check x
  K6, N4, registry #1253)" -- несёт ОБЕ формы (принимаемую и честно
  отказанную) в одном тесте.
