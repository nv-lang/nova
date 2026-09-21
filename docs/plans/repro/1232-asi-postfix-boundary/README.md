<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Пробы к реестру №1232 (и его сестринской К2-строке про `Option[int]`)

Три пробы, каждая копируется во временный `.nv` перед прогоном (`.nv.txt` —
конвенция репозитория для улик, `require_nova_source` отвергает не-`.nv`
пути напрямую, см. правило №695 п.2).

## Файлы

- `a_match_original_order.nv.txt` — форма `basics/demo.nv`: `[] => -1`
  первым arm'ом, `[x, ..] => x` вторым. ДО фикса — безымянный отказ парсера
  (адреса на `,` и `..` внутри второго arm — `-1` от первого arm'а
  проглатывает `[` второго как индексацию).
- `b_match_reordered.nv.txt` — те же два arm'а, порядок обратный. ДО фикса
  парсится чисто (смежность не задета) и доходит до чекера с честным
  отказом `match на applied sum (Option[int])`.
- `c_call_across_newline.nv.txt` — общий случай, НЕ внутри `match`: `foo`
  на одной строке, `(7)` на следующей. Спека (`spec/decisions/03-syntax.md`,
  D49, "Edge cases") прямо требует ДВА statement'а. ДО фикса это глоталось
  в ОДИН вызов `foo(7)` — компилятор отвечал "this call writes more
  arguments than the callable declares".

## Команда прогона

```sh
NOVAC=./novac/target/novac.exe
for f in a_match_original_order b_match_reordered c_call_across_newline; do
    cp "docs/plans/repro/1232-asi-postfix-boundary/${f}.nv.txt" "/tmp/${f}.nv"
    echo "=== ${f} ==="
    "$NOVAC" check "/tmp/${f}.nv"
done
```

## Замер ДО фикса (HEAD перед коммитом, `novac/src/parse/{parse,expr}.nv`)

- `a_match_original_order` → `E_NOVAC_SUBSET`, "novac did not parse this",
  ДВА диагностики на `,` и `..` внутри второго arm'а.
- `b_match_reordered` → `E_NOVAC_SUBSET`, "match на applied sum
  (Option[int])" + "unknown name" — семантический слой, порядок arm'ов
  МЕНЯЕТ диагностику.
- `c_call_across_newline` → "this call writes more arguments than the
  callable declares" (`foo` объявлен без параметров, вызов получил один
  аргумент — 7 — склеенный с соседней строки).

## Замер ПОСЛЕ фикса

- `a_match_original_order` → **ТА ЖЕ** диагностика, что у `b_match_reordered`
  ("match на applied sum (Option[int])" + "unknown name") — порядок arm'ов
  больше не меняет результат. Разбор `[x, ..]` больше не глотает предыдущий
  arm.
- `b_match_reordered` → без изменений (контроль: фикс не должен был его
  трогать).
- `c_call_across_newline` → "unknown name" на `x` (последнее выражение) --
  `(7)` больше не глотается как вызов; остаточная диагностика относится к
  ДРУГОМУ, известному подмножественному пробелу (голая функция-значение как
  выражение-statement), не к этой строке.

## Что фикс НЕ трогает (контроль отсутствия регрессии)

`spec_tests/conformance/d49_statement_separator_newlines.nv` даёт
БАЙТ-В-БАЙТ идентичный список из 6 диагностик до и после фикса (тот же
`start`/`end`, тот же текст) -- методом временного `git checkout --` двух
правленых файлов и пересборки. Метод-чейн через перенос строки (`.`),
логические `||`/`&&` с leading-формой, массив-литерал через несколько строк
и группировка `( ... )` -- ни один не задет: гейт `@peek_same_line()` стоит
только у `[` (постфиксный цикл, Index-после-имени, Turbofish-после-имени) и
у `(` (Call-после-имени); `.` и `?` остаются newline-толерантными по D49
правилу 3, как и было.

## Корень (для реестра)

`novac/src/parse/parse.nv` — новая дверь `@peek_same_line(skip int = 0) ->
bool`, читающая `leading`-trivia токена на предмет `\n`. Применена в
`novac/src/parse/expr.nv` в трёх местах: постфиксный цикл `@primary()`
(строка с `while @peek() == TokenKind.Dot || (@peek() == TokenKind.LBracket
&& @peek_same_line())`), Index-после-имени и Turbofish-после-имени внутри
`@atom()`, и Call-после-имени там же.
