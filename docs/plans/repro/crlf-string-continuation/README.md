<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру: строковый литерал с `\`-продолжением на CRLF

## Файл

`crlf_continuation.nv.txt` -- байты сохранены буквально (CR+LF внутри
строкового литерала важны для воспроизведения; `.txt`, конвенция улик
№695 п.2). Скопировать во временный `.nv` перед прогоном:

```sh
cp docs/plans/repro/crlf-string-continuation/crlf_continuation.nv.txt /tmp/crlf.nv
./novac/target/novac.exe check /tmp/crlf.nv
```

## Форма

Строка `src` внутри пробы -- ДВА `\`-продолжения (D467 §3) подряд, каждое
кончающееся CRLF (не LF), как в реальном `novac/src/parse/parse_test.nv`.

## Замер ДО фикса (`novac/src/lex/lex.nv`)

```
E_NOVAC_SUBSET "novac did not parse this -- ..." (безымянный fallback),
на смещении, приходящемся на ВТОРОЕ `\`-продолжение -- первое поглощается
как обычно, а на втором цикл сканирования строки обрывается на голом `\n`.
```

## Замер ПОСЛЕ фикса

```
outside the subset: this string escape is not compiled yet
(the subset knows only \" \\ \n \t \r)
```

Честный, названный отказ -- парсер читает ВЕСЬ литерал целиком (сравни
диапазон диагностики: было 20 байт, стало 27 -- literal читается дальше,
до настоящего конца). Само continuation-escape ещё не реализовано КАК
СЕМАНТИКА (это отдельный, честный подмножественный пробел), но лексер
больше не давится на форме.

## Корень

`novac/src/lex/lex.nv`, цикл сканирования строкового литерала: ветка
`b[i] == B_BACKSLASH && i + 1 < n` звала `i += 2` безусловно. Continuation
исходника на CRLF-файле -- это `\` + `\r` + `\n` (три байта), и старый код
поглощал только `\` и `\r`, оставляя `\n` НЕПОГЛОЩЁННЫМ -- а проверка
"newline ends the scan" (274.3/F19, самая первая строка цикла) обрывала
литерал именно на этом байте. На LF-файле (`\` + `\n`, без `\r`) форма
работала правильно всегда -- дефект специфичен CRLF.

## Фикс

```nova
} else if b[i] == B_BACKSLASH && i + 1 < n {
    if b[i + 1] == B_CR && i + 2 < n && b[i + 2] == B_LF {
        i += 3
    } else {
        i += 2
    }
} else {
```

## Доказано в обе стороны

`novac/src/lex/lex_test.nv`, тест "string literal's line-continuation
backslash survives a CRLF source" -- красный на откате `lex.nv` к базе
(assert failed: toks[0].kind == StrLit), зелёный после восстановления
фикса. `parse_test.nv` module test и
`spec_tests/conformance/d49_statement_separator_newlines.nv` (6 диагностик,
без изменений) -- без регрессии.
