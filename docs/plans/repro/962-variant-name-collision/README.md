# Проба к №TBD (носитель №136) — общее имя варианта у сумм разных модулей ломает `Result`-payload даже при квалифицированном конструкторе

Заведено исследовательским окном (owner-research) 2026-09-05 при реализации плана 282 Ф.4
(`str @to_f64()` на Nova). Решение владельца — общий словарь вариантов ошибок
`Invalid | AboveMax | BelowMin` у `ParseIntError`, `ParseFloatError`, `RangeError`, `CharError`.
С таким словарём `ParseFloatError` **компилируется неверно**.

## Что замерено (оракул, 2026-09-05)

Воспроизводится **на std**, не на отдельном файле:

1. В `std/src/runtime/string/parse_float.nv` дать вариантам целевые имена
   `enum Empty | Invalid { at int } | AboveMax | BelowMin` (конструкторы там написаны
   КВАЛИФИЦИРОВАННО: `ParseFloatError.Invalid { at: i }`, `ParseFloatError.AboveMax`).
2. Собрать программу в другом модуле, которая матчит `s.to_f64()` квалифицированными паттернами.

| вход | ожидание | факт |
|---|---|---|
| `" 1.5"`, `"1,5"`, `"nan"`, `"1.2.3"`, `"1e"` | `Err(ParseFloatError.Invalid { at })` | **ни один паттерн не совпал** (`Err(_)` → «other») |
| `"1e999"` | `Err(ParseFloatError.AboveMax)` | **`Invalid at 0`** — чужой тег с мусорным полем |
| `"-1e999"` | `BelowMin` | `BelowMin` (совпадение тегов случайно) |
| `""` | `Empty` | `Empty` |

3. Заменить имена на уникальные в CU (`Malformed`/`TooLarge`/`TooSmall`) — **всё верно**.
   Так std и оставлен до фикса (временные имена помечены в doc типа).

В CU у имени `Invalid` было три хозяина: `ParseBoolError.Invalid` (unit, `parse.nv`, тот же
модуль, файл раньше по алфавиту), `CharError.Invalid` (unit, prelude), `ParseFloatError.Invalid { at }`.
У `AboveMax` — четыре (`RangeError`, `CharError`, `ParseIntError`, `ParseFloatError`) с разными тегами.

## Что НЕ воспроизводит (контроли, важны для чинящего)

- `colliding.nv.txt` — те же три суммы **в одном файле** с `main`: печатает верно
  (`e -> Invalid at 3`, `big -> AboveMax`). Однофайловая раскладка не задета.
- `unique.nv.txt` — то же с уникальными именами: верно.
- Многофайловый репро в `examples/` собрать не удалось из-за правил модульных путей
  (`E_D78_MODULE_PATH_MISMATCH` / `cannot find module`), поэтому носитель остаётся std.

Вывод: сбой требует, чтобы сумма с payload-вариантом была объявлена в ДРУГОМ модуле (здесь —
`#no_prelude`-файл std, приезжающий через prelude), а имя её варианта совпадало с unit-вариантом
другой суммы того же CU. Класс — №136 «голое имя варианта резолвится в несвязанный тип»; новое
здесь: **квалифицированная запись `ParseFloatError.Invalid` не спасает** — кодоген опускает
квалификацию и идёт по голому имени.

## Как запускать контроли

```sh
cp docs/plans/repro/962-variant-name-collision/colliding.nv.txt docs/plans/repro/962-variant-name-collision/colliding.nv
nova-cli/target/release/nova.exe build docs/plans/repro/962-variant-name-collision/colliding.nv -o <куда-нибудь>.exe
```
