<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба к реестру №1259 -- if-let не читает квалифицированную голову варианта

## Находка

НАЙДЕНО ОКНОМ 274 (nova-46), 2026-09-21, при обходе self-build map
(274.5, §5о, пятый из семи оставшихся ненарезанных). Носитель --
`novac/src/resolve/resolve_test.nv:134`, `if FnLookup.FnOverloaded(_) =
got { ... } else { ... }`.

## Минимальное репро

`p1-qualified-some.nv.txt`:
```nova
fn g(o Option[int]) -> int {
    if Option.Some(v) = o {
        v
    } else {
        0
    }
}
fn h(o Option[int]) -> int {
    g(o)
}
```

`p2-qualified-runtime.nv.txt` -- та же форма, статусная позиция, с
реальным построением значения (`Some(7)`) и прогоном.

`novac check` -- безымянный fallback («did not parse this»). Оракул
компилирует и ИСПОЛНЯЕТ `p2` (печатает `7`, затем `0`) -- форма
легальна, не выдумка.

## Замер ДО фикса

`rc=1`, «novac did not parse this -- and it cannot tell whether the
form is one it does not read yet or the text is malformed».

## Замер ПОСЛЕ фикса

`rc=0` на всех формах. C-эмиссия сверена напрямую (`novac emit`):
`_novac_l1.tag == NOVA_TAG_Option_Some`, `nova_int v =
_novac_l1.value;` -- ровно та же форма, что голова без квалификатора
даёт для той же программы. Оракульская сборка того же `p2` печатает
`7` и `0`.

## Корень

`parse/parse.nv:614-616` -- лукахэд if-let ждал РОВНО `KwIf Ident
LParen Ident RParen Assign` (голову БЕЗ квалификатора). На `if
Option.Some(v) = o` третий токен -- `Dot`, не `LParen`, лукахэд не
совпадал целиком, форма падала в обычный `@expr()` и давала безымянный
fallback раньше `=`.

## Фикс

Тот же приём, что уже применён дважды этой ночью для того же класса
(голова варианта в образце `match` -- `pattern_head_slot`; `const
Type.NAME`, nova-a5): лукахэд РАСШИРЕН, чтобы узнавать ОБЕ формы
(бare и квалифицированную), а НЕ ФИКСИРОВАННЫЕ позиции детей заменены
дверью-сканером.

**Новые доверы** (`sem/slots.nv`): `iflet_is_qualified`,
`iflet_variant_at`, `iflet_binder_at`, `iflet_init_at`,
`iflet_body_at`, `iflet_else_at` -- каждая отвечает "где стоит X",
бare или квалифицированный, разница ровно +2 после квалификатора.

**Шесть мест перешли на двери** (ни одно не сохранило свой хардкод
рядом с новым):
- `parse/parse.nv` -- лукахэд + сбор детей (2 или 4 токена перед
  `(`, в зависимости от формы);
- `check/tail_rules.nv` -- `stmt_terminates`'s IfLet-арм (`lk[7]`/
  `lk[9]` → `lk[iflet_body_at(lk)]`/`lk[iflet_else_at(lk)]`) и
  `@report_if_tail_has_no_value`'s IfLet-проверка (`len() > 9` →
  `len() > iflet_else_at(lk)`);
- `check/typing.nv`, `@type_if_let` -- САМАЯ ГУСТАЯ дверь: variant-имя
  (`kids[1]`), binder (`kids[3]`), init (`kids[6]`), body (`kids[7]`),
  else (`kids[9]`) -- все пять позиций плюс три граничных проверки
  длины;
- `lower/lowering.nv`, ДВЕ функции -- `@lower_if_let_value` (значение,
  registry 1189) и `@lower_if_let` (оператор): init/binder/body/else
  в обеих, плюс `else_shape(kids, 9)` → `else_shape(kids, else_at)`.

## Побочная находка при регрессии: #1250 сам был строже оракула

Прогон полного пайплайн-набора тестов (`pipeline_test.nv`,
`subset_test.nv` -- батчатся вместе с `iflet_test.nv` в один бинарь)
поймал ДВЕ пинованные строки текста, устаревшие после сегодняшнего
#1250 (магнитудная дверь подчинена `u64`, не `int`'s own):
`pipeline_test.nv` ждал старый текст «does not fit in \`int\`» для
`2^64` -- поправлено на новый текст «does not fit any integer type».
`subset_test.nv` ждал ОТКАЗ на `0x8000000000000000` (16 hex-цифр,
ведущий 8) -- а ОРАКУЛ ЭТУ ФОРМУ ПРИНИМАЕТ (замерено напрямую: сборка
и прогон дают `-9223372036854775808`, двоичное дополнение читает биты
как `int`'s наиболее отрицательное значение, C делает тот же перенос
через `(nova_int)0x8000000000000000LL`) -- старый отказ был novac
СТРОЖЕ оракула на легальном коде, не магнитудным багом. #1250 починил
это побочно, подчинив дверь `u64`; тест поправлен на верное поведение,
добавлена НОВАЯ форма (17 hex-цифр) для контроля, что переполнение
`u64` по-прежнему честно отказано.

## Контроль отсутствия регрессии

- `check/check_test.nv`, `parse/parse_test.nv`, `resolve/resolve_test.nv`,
  `lower/lower_test.nv`, `pipeline/iflet_test.nv`,
  `pipeline/pipeline_test.nv`, `pipeline/subset_test.nv` -- все PASS
  (прогнаны напрямую по пути файла).
- `check-novac-grammar-kinds`, `check-novac-no-default-branch`,
  `check-novac-file-size` -- зелёные.
- `spec_tests/conformance/d49_statement_separator_newlines.nv` -- 5
  диагностик, без изменений.
- Тест-свидетели: `check_test.nv` (квалифицированная голова с ЧУЖИМ
  вариантом даёт тот же именованный отказ, что и bare-форма);
  `pipeline/iflet_test.nv` (позитивная форма: check чист, C-эмиссия
  ИДЕНТИЧНА bare-форме байт-в-байт, else-обязательность значения тоже
  работает).
- Оба пути проверены прямыми пробами: откат шести файлов -> обе формы
  красны безымянным fallback -> фикс восстановлен -> обе честно
  приняты, C-эмиссия и оракульский прогон совпадают.
