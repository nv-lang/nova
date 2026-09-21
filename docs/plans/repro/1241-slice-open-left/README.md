<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1241 -- ведущий `..` (открытый слева слайс) не парсился вовсе

## Минимальное репро

`leading_dotdot.nv.txt` -- одна строка внутри `fn m() { ... }`:

```
t[..2]
```

`three_forms.nv.txt` -- три формы слайса рядом плюс контроль отсутствия
регрессии (`a + b` вне слайса):

```
fn open_left(t []int) -> int => t[..2][0]
fn open_right(t []int) -> int => t[2..][0]
fn closed(t []int) -> int => t[0..2][0]
fn unaffected(a int, b int) -> int => a + b
```

Прогон:

```sh
cp docs/plans/repro/1241-slice-open-left/three_forms.nv.txt D:/Temp/tp3.nv
./novac/target/novac.exe check D:/Temp/tp3.nv
```

## Замер ДО фикса (`git checkout -- novac/src/parse/expr.nv`, пересборка)

```
{"id":"d0",...,"message":"novac did not parse this -- and it cannot tell whether
 the form is one it does not read yet or the text is malformed; this is the
 parser's fallback, narrowed form by form",...}   <- open_left, ПЕРВЫЙ отказ
{"id":"d1",...,"message":"novac did not parse this -- ... fallback ...",...}
                                                    <- open_left, ВТОРОЙ отказ (каскад)
{"id":"d2",...,"message":"outside the subset: a slice `x[lo..hi]` is read but
 not compiled yet (E2-b)",...}                     <- open_right, честный E2-b
{"id":"d3",...,"message":"outside the subset: a slice `x[lo..hi]` is read but
 not compiled yet (E2-b)",...}                     <- closed, честный E2-b
```

`open_left` даёт ДВА отказа безымянного фоллбека вместо одного честного
`E2-b` -- ведущий `DotDot` не имеет ветки в `@primary()`, климбинг-луп его
не подхватывает, `2]` рассыпается на свои собственные фоллбеки.
`unaffected` не появляется в выводе вовсе -- уже само по себе доказывает, что
он не задет.

## Замер ПОСЛЕ фикса

```
{"id":"d0",...,"message":"outside the subset: a slice `x[lo..hi]` is read but
 not compiled yet (E2-b)",...}   <- open_left, ТЕПЕРЬ честный E2-b
{"id":"d1",...,"message":"outside the subset: a slice `x[lo..hi]` is read but
 not compiled yet (E2-b)",...}   <- open_right, как и было
{"id":"d2",...,"message":"outside the subset: a slice `x[lo..hi]` is read but
 not compiled yet (E2-b)",...}   <- closed, как и было
```

Три формы, три идентичные честные диагностики; `unaffected` -- вне вывода.

## Корень и фикс

`novac/src/parse/expr.nv`, `@primary()`: `..` объявлен ТОЛЬКО как бинарный
оператор (`is_binop`/`binding_power`), никогда как префикс. Для
`t[..2]` `@primary()` не находил ветки на `..`-первом-токене и падал в
generic Err-catch-all, съедая `..` одним stray-токеном. Фикс -- ветка в
начале `@primary()`:

```nova
if @peek() == TokenKind.DotDot {
    return @error_node(TokenKind.ErrorTok)
}
```

Возвращает MISSING-левую часть БЕЗ потребления `..` -- тот же
climbing-loop сразу после подхватывает `..` как обычный бинoп, строя
`Bin(missing, .., real-or-missing)`, ту же структуру, что открытый-справа
случай уже строил случайно (через нормальный бинарный разбор с
отсутствующим правым операндом). Чекер узнаёт эту структуру и даёт
честный E2-b для всех трёх форм одинаково.

Первая попытка -- бросить bare `@missing_of(TokenKind.ErrorTok)` без
обёртки -- давала ВТОРУЮ, дублирующую диагностику: отдельный проход
"unrecognized token" читает голый `ErrorTok`-лист как настоящий
нераспознанный токен. `@error_node(...)` (обёртка в `NodeKind.Err`)
убирает дубль -- этот шаблон совпадает с КАЖДЫМ другим сайтом
"missing-as-whole-result" в файле.

## Контроль отсутствия регрессии

- `spec_tests/conformance/d49_statement_separator_newlines.nv` -- 6 строк
  диагностик, без изменений.
- `novac/src/lex/lex_test.nv`, `novac/src/parse/parse_test.nv`,
  `novac/src/check/check_test.nv` -- все PASS.
- Тест-свидетель добавлен в `parse_test.nv`: "shape: a leading `..`
  (open-left slice) parses as an Index, not the parser's fallback
  (registry #1241)".

## Оговорка про носителя, и отдельная находка про раннер

Фикс носителя приёмкой не считается -- слайсы всё ещё вне подмножества
(E2-b), только диагностика стала честной и единообразной по всем трём
формам открытости.

**Отдельно, не про сам фикс:** проверка "красный на реверте, зелёный на
фиксе" через `nova test novac/src/parse/parse_test.nv` дала АНОМАЛИЮ --
новый тест PASS'ил даже на git-реверченном `expr.nv` (подтверждено
`grep -c "A LEADING" expr.nv` == 0 в момент прогона), хотя прямой вызов
`novac.exe check` на эквивалентных пробных файлах в тот же момент
корректно показывал старый краш. Проба в обе стороны для ЭТОГО репро
велась поэтому НАПРЯМУЮ через `novac.exe check` (см. замеры выше), а не
через модульный раннер. Аномалия зафиксирована отдельной строкой реестра
(№TBD, "К7", в том же коммите) -- не расследована до конца, подозрение на
`target/.nova-daemon` или посторонний процесс `nova` из главного дерева.
