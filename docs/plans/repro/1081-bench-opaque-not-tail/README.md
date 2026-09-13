<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# №1081 — `bench.opaque(...)` голой строкой НЕ в хвосте роняет компилятор

Найдено окном 283 (2026-09-13), когда не собрался его бенч под волну Ф.5.
**Сверено интегратором своей пробой в тот же час** — чужой замер не берётся на
слово, даже точный.

## Две пробы, отличающиеся ПОРЯДКОМ ДВУХ СТРОК

Команда из корня репозитория:

```sh
nova-cli/target/release/nova bench run <проба>.nv
```

| проба | порядок строк в `measure` | исход |
|---|---|---|
| `a_tail.nv.txt` | `bench.elements(1)`, затем `bench.opaque(twice(21))` | ✅ RC=0, бенч отработал и напечатал числа |
| `b_not_tail.nv.txt` | те же строки НАОБОРОТ | ❌ RC=101, **internal error** |

Вердикт `b`, дословно:

```
nova: internal error at compiler-codegen\src\codegen\emit_c.rs:3439:
[E_CODEGEN_TYPE_UNKNOWN] method call `.opaque` return type unknown;
obj_ty="" obj=Ident(bench)
  --> <unknown>:7:9
  hint: the type-checker did not annotate this expression's C type
        (compiler-conventions.md §0) — this is usually a missing `import`
        for the type/effect used on the left of `.`; if the import is
        already present, this is a compiler bug, please report it.
This is a bug in nova. Please report it.
```

## Почему это дороже, чем «падает один бенч»

**Форма из корпуса работает не по своему свойству, а по СОСЕДСТВУ.** В
`bench/micro/arith.nv` голая `bench.opaque(fib(20))` стоит третьим бенчем и
является ПОСЛЕДНЕЙ строкой своего `measure` — случайно. Первые два бенча того же
файла используют другую форму (`acc = bench.opaque(...)`, присваивание), и она к
дефекту не относится вовсе.

Значит следующий, кто допишет `bench.elements(...)` после голой `bench.opaque`,
получит ICE — на коде, который выглядит как соседний и зелёный.

**И подсказка в диагностике уводит в сторону:** «usually a missing `import`».
Импорт тут ни при чём; `bench` — не тип и не эффект, а DSL-приёмник, вводимый
самой конструкцией `bench "..." { measure { … } }`.

## Чем это НЕ является — и почему у №292 отдельная судьба

Соседняя строка реестра **№292** говорит: «`nova bench run` падает ICE на ЛЮБОМ
файле», и называет это членом ICE-пачки P67 с адресом `emit_c.rs:60404`,
код `[P67-LEGACY] Index element type unknown`.

Это **другой дефект**: другой адрес (`:3439`), другой код
(`E_CODEGEN_TYPE_UNKNOWN`), другой предмет (тип возврата метода, а не тип
элемента индексации).

**А само утверждение №292 сегодня ОПРОВЕРГНУТО замером:**

```
$ nova-cli/target/release/nova bench run bench/micro/arith.nv
fibonacci recursive n=20
  median:    81.22 µs  (± 7.82 µs MAD)
  ...
RC=0
```

Нетронутый файл корпуса отрабатывает целиком. То есть «падает на любом файле»
перестало быть правдой, а строка стоит открытой, и план 221 числит её барьером
пользователя с выводом «замерочный DSL непригоден». Расхождение записано в саму
№292 тем же слиянием: строка, которую не поправили, отправляет следующее окно
не туда — и здесь она отправляет его чинить то, что уже работает, вместо
позиционного дефекта, который живёт.
