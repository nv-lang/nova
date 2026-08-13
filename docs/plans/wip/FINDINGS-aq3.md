# AQ3 Изоляция: результаты тестирования

## Шаг 4: Изолированная копия (5 прогонов)

Файл: `aq3iso/a_q3_iso.nv` (копия `spec_tests/conformance/a_q3_println_debug_record.nv` с `module aq3iso`)

- ISO RUN 1: `PASS: 1  FAIL: 0`
- ISO RUN 2: `PASS: 1  FAIL: 0`
- ISO RUN 3: `PASS: 1  FAIL: 0`
- ISO RUN 4: `PASS: 1  FAIL: 0`
- ISO RUN 5: `PASS: 1  FAIL: 0`

## Шаг 5: Оригинал в conformance-папке (3 прогона)

Файл: `spec_tests/conformance/a_q3_println_debug_record.nv` (компилируется с полной папкой)

- ORIG RUN 1: `RUN-FAIL` + `PASS: 0  FAIL: 1`
- ORIG RUN 2: `RUN-FAIL` + `PASS: 0  FAIL: 1`
- ORIG RUN 3: `RUN-FAIL` + `PASS: 0  FAIL: 1`

## Шаг 6: Вывод (Исход B)

**Виноват НЕ тест** `a_q3_println_debug_record`. Изолированная копия проходит все 5 прогонов, а в составе большого compile-unit'а падает. Причина: эффект слияния с другими файлами папки либо неверная атрибуция.

## Шаг 7: Тестовые блоки, печатающиеся рядом

Во всех трёх прогонах оригинала в выводе RUN-FAIL печатаются три PASS-блока:

1. `D61 §8: прямой вызов резолвит ВЕРНЫЙ возврат операции — int и str на ОДНОМ handler-значении`
2. `D61 §8: два независимо связанных handler-ЗНАЧЕНИЯ одного эффекта остаются различны при прямом вызове`
3. `[M-novavtable-read-write-pointer-collision]: read()/write(v) op-имена на handler-значении дispatch'атся через vtable, не через B11d typed-pointer-deref`
