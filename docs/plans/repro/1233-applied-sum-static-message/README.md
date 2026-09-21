<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1233's остатку -- статичное `Option[int]` в сообщении

## Минимальное репро

`result_match.nv.txt` -- матч на `Result`, не на `Option`:

```nova
fn f(r Result[int, str]) -> int {
    match r {
        Ok(a) => a
        Err(e) => 0
    }
}
```

## Замер ДО фикса

```sh
./novac.exe check result_match.nv
```
```
{"...", "message":"outside the subset: a `match` on an applied sum
 (`Option[int]`) is read but not compiled yet -- its tags belong to the
 shell (E2-b2, with generics)", ...}
```
Сообщение НАЗЫВАЕТ `Option[int]`, хотя предмет -- `Result[int, str]`.

## Замер ПОСЛЕ фикса

```
{"...", "message":"outside the subset: a `match` on an applied sum
 (`Result[int, str]`) is read but not compiled yet -- its tags belong
 to the shell (E2-b2, with generics)", ...}
```

## Корень и фикс

`novac/src/check/match_arms.nv`, `@report_if_applied_sum` -- сообщение
было `const APPLIED_SUM_ARM_MSG` в `messages.nv`, с ЖЁСТКО ВШИТЫМ
`Option[int]`, не читающее реальный `scr_t`. Фикс: сообщение строится
на месте вызова (единственном, `@report_if_applied_sum` -- метод
`Checker`, имеет `@ctx`), интерполируя `${ty_source_name(@ctx, scr_t)}`
-- та же дверь, которой уже пользуются соседние сообщения в этом же
файле (строка 341). Константа удалена из `messages.nv` (единственный
вызывающий).

Настоящий `match Option[int] {...}` остаётся с тем же текстом (замер:
`novac.exe check` на `matchwiden_probe5.nv` и на изолированной
`Option`-пробе -- без изменений).

## Контроль отсутствия регрессии

- `novac/src/check/check_test.nv`, `novac/src/parse/parse_test.nv`,
  `novac/src/lex/lex_test.nv` -- все PASS.
- `check-novac-grammar-kinds`, `check-novac-no-default-branch`,
  `check-novac-file-size` -- зелёные.
- `spec_tests/conformance/d49_statement_separator_newlines.nv` -- 5
  диагностик, без изменений.
