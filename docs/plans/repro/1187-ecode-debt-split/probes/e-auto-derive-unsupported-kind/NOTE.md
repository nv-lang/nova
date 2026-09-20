<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Код живой, но найден через РЕТРАКТИРОВАННЫЙ синтаксис — оговорка обязательна

`p.nv` использует `external type RawHandle` (D126, retracted) — форма всё ещё
принимается парсером ради собственной диагностики. Она поднимает ТРИ кода
разом: `E_AUTO_DERIVE_UNSUPPORTED_KIND` (то, что искали), плюс
`E_EXTERNAL_TYPE_RETRACTED` и `E_IMPL_MISSING_METHODS`.

`control.nv` (`#impl(Equal)` на записи) остаётся чистым контролем — путь для
записей работает.

Открытый вопрос, не закрытый здесь: есть ли НЕретрактированный путь к тому же
коду (например, именованный кортеж `type X(a int)` из
`spec_tests/conformance/lint/conv_clean.nv:84`). Подробности и решение — в
`../../batch2-results.txt`, раздел «ЧЕТВЁРТАЯ ВОЛНА».
