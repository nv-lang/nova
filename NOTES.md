# №158 — заметки сессии (2026-07-30, sonnet)

## Итог: обе задачи закрыты, две головы коммитов

- `cf8d8730b` — фикс d62 (компилятор emit_c.rs + тест d62_raw_effect_op_pos.nv)
- `d2b142d59` — фикс атрибуции раннера (test_runner.rs) + ratchet-компрессия

## Корень падения d62

`compiler-codegen/src/codegen/emit_c.rs:~17439` (генератор `Nova_<Effect>_<op>`
dispatch-функций): `return _nova_handler_{name}->{field}(...)` разыменовывал
`_nova_handler_Log1` БЕЗ null-проверки. Формы (1)/(2) регресс-гарда
(`ok_declared`/`ok_private_inferred`) вызывались из `test`-блока БЕЗ какого-либо
`with Log1 = ...` в динамическом стеке — `_nova_handler_Log1 == NULL` →
access violation → весь процесс падает ДО первой PASS/FAIL строки → RUN-FAIL
без диагностики → раннер приписывал алфавитно первому файлу набора.

Доказательство через spec/decisions/04-effects.md D62 §«Семантика проверки»:
"Активный handler в runtime отсутствует на момент операции → runtime fail
(panic)" — сигнатурная декларация/D28-inference НЕ устанавливает handler,
только передаёт обязательство вызывающему. Тест был неверен (не хватало
`with` СНАРУЖИ вызова в test-блоке) — исправлено; компилятор ТАКЖЕ был неверен
(NULL-deref вместо управляемого panic) — исправлено.

## Что не сделано / попутные дефекты

- `[M-emit-c-loc-for-span-wrong-file-merged-cu]` — не трогал (не обязан).
- `p0_erased_now_dispatches_via_vtable` (test_runner.rs unit-тест,
  `nova_tests/plan72/*`) падает ДАЖЕ на debug-стеке (`STATUS_STACK_OVERFLOW`
  на дефолтном стеке; с `RUST_MIN_STACK=32MB` не крашится, но всё равно FAILS
  на устаревших `E_READONLY_COERCE`/`E_STR_CONCAT_PLUS` diagnostics в
  fixture-файлах `nova_tests/plan72/*`) — ПРЕДСУЩЕСТВУЮЩИЙ дефект, не мой,
  не трогал (не входит в мандат №158).
