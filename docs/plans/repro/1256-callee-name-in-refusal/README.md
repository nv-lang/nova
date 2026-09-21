<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1256 -- отказ называл `println`, которого в файле нет

## Находка

НАЙДЕНО ОХОТНИКОМ 2026-09-21 (novac, клетка `check`×К2), отчёт
`docs/dev/hunts/novac/2026-09-21-check-k4.md`, раздел Н7. Оригинальная
проба хунтера -- `p16-panic-tail-names-println`.

## Минимальное репро

`panic_tail.nv.txt`:

```nova
fn f(ok bool) -> int {
    if ok {
        1
    } else {
        panic("no")
    }
}
```

`panic(...)` в хвостовой позиции (оракул принимает; novac честно
отказывает -- `panic` вне подмножества как значение, отдельный, законный
предел). Но ТЕКСТ отказа называл `println`, а не `panic`.

## Замер ДО фикса

```
{"...", "message":"outside the subset: this name is not a callable
 novac knows in an expression (println is a statement here, not a
 value)", ...}
```
`println` НЕ встречается в исходнике вообще -- сообщение цитирует
пример из ИСТОРИИ отказа (комментарий у двери), а не факт о текущем
файле.

## Замер ПОСЛЕ фикса

```
{"...", "message":"outside the subset: `panic` is not a callable novac
 knows in an expression -- either it does not exist, or (like
 `println`) it is a statement-only form novac does not read as a
 value (E2-b3)", ...}
```
Называет РЕАЛЬНЫЙ callee (`panic`); `println` остаётся только как
ПРИМЕР класса форм ("как `println`"), явно помеченный, не как
утверждение о файле.

Настоящий `println(...)` в той же хвостовой позиции продолжает
называться корректно -- сообщение зовёт его по имени, потому что он и
есть реальный callee в ЭТОМ случае.

## Корень

`novac/src/check/calls.nv`, `@type_free_call` -- сообщение было
СТАТИЧЕСКОЙ строкой, написанной про ОДИН исторический случай
(`println` в позиции значения через висящий оператор, 2026-08-16), но
звучащей как утверждение про ЛЮБОЙ вызов, падающий в эту ветку.
Callee-имя (`cname`) уже вычислено в этой же функции (`call_callee_text`,
строка выше) и просто не читалось сообщением.

## Фикс

Сообщение строится с `${cname}`, называя реальный неизвестный вызов;
`println` остаётся хедж-примером класса, явно оговорённым словом
«like».

## Контроль отсутствия регрессии

- Настоящий `println(...)` в той же позиции: сообщение теперь называет
  именно `println` (правильно -- он и есть callee).
- `novac/src/check/check_test.nv`, `novac/src/parse/parse_test.nv`,
  `novac/src/lex/lex_test.nv` -- все PASS.
- `check-novac-grammar-kinds`, `check-novac-no-default-branch`,
  `check-novac-file-size` -- зелёные.
- `spec_tests/conformance/d49_statement_separator_newlines.nv` -- 5
  диагностик, без изменений.
- Тест-свидетель -- `check_test.nv`, "diagnostic naming: an
  unknown-in-expression refusal names the ACTUAL callee (hunt check x
  K4, N7)".
