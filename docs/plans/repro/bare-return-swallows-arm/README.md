<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру: беззначный `return` в конце match-arm'а глотает следующий arm

## Файл

`probe.nv.txt` -- `None => return` (без значения), затем `Some(_) => {}`.

## Замер ДО фикса

```
{"code":"E_NOVAC_SUBSET", "start":<offset of "=>">, "message":"novac did not
parse this -- ..."}
```

Байты на смещении -- `=>` СЛЕДУЮЩЕГО arm'а, а не что-то рядом с `return`:
адрес честен здесь (в отличие от других случаев этой ночи), но причина
всё равно не была бы видна без чтения кода парсера.

## Замер ПОСЛЕ фикса

Чисто (для этой формы; полный файл с реальным `Option[int]`-параметром
даёт честный отказ про match на applied sum -- отдельный, известный класс).

## Корень

`novac/src/parse/parse.nv`, разбор `return`:

```nova
if @peek() != TokenKind.RBrace {
    kids.push(@expr())
}
```

Проверка "есть ли значение у `return`" смотрела ТОЛЬКО на `}` -- то есть
работала для `{ ... return }` (конец блока), но НЕ для конца match-arm'а,
где после `return` идёт ПАТТЕРН следующего arm'а (`Some`), не `}`. Тот же
класс, что №1232 этой же ночи (ASI-подобная склейка через перенос строки),
только на другой позиции грамматики.

## Фикс

```nova
if @peek() != TokenKind.RBrace && @peek_same_line() {
    kids.push(@expr())
}
```

Одна строка, использует УЖЕ СУЩЕСТВУЮЩУЮ дверь `@peek_same_line()`
(добавленную для №1232 в этом же файле).

## Доказано в обе стороны

`novac/src/parse/parse_test.nv`, тест "a bare `return` at a match arm's
end does not swallow the next arm" -- красный на temporary-откате правки
(`arms == 2` не проходит), зелёный после восстановления.
`check/check_test.nv` и `spec_tests/conformance/d49_statement_separator_newlines.nv`
(6 диагностик, без изменений) -- без регрессии.
