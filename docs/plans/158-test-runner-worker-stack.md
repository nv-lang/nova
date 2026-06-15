<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 158 — Test-runner worker-thread stack size (`[M-codegen-conformance-stack-overflow]`)

> **Создан:** 2026-06-15. **Статус:** ✅ **DONE** (ветка `plan-cgstack`, worktree nova-p156).
> **Владеет:** `[M-codegen-conformance-stack-overflow]`. **Зависит от:** Plan 24/26 (test-runner). P1.
> **Триггер:** регрессия — дефолтный `nova test` падал stack-overflow на больших conformance-фикстурах.

## Проблема
`nova-codegen test-all` падал **`thread has overflowed its stack`** (exit 127) на больших
сгенерированных тест-файлах (Unicode conformance — тысячи `assert` в одном `test`-блоке;
напр. `nova_tests/plan152_4/normalization_conformance.nv`, ~6000 asserts). Ломало дефолтный
регресс на наборе plan152_4.

## Разведка корня (важно — первичный диагноз был неверен)
- Тот же файл через **`nova-codegen test-build` (одиночный, главный поток, 8 МБ стек) →
  exit 0, ПРОХОДИТ.** Значит codegen-глубина **нормальна** для обычного стека.
- Падали только **worker-потоки** `test-all`: `std::thread::scope` + `s.spawn(...)`
  (`test_runner.rs`) спавнит воркеры с **дефолтным ~2 МБ стеком**.
- Регрессия дельты main `2eb59b04..b0095867` (Plan 153.x codegen-lowering стал чуть
  глубже/тяжелее по стеку) → перешагнул 2 МБ воркера, но не 8 МБ главного.
- **Вывод:** корень — недосайженный стек worker-потоков, НЕ «рекурсивный codegen».
  `recursion→iteration` переписывание codegen — отклонено (огромный regression-риск ради
  патологии глубже 64 МБ; на нормальном стеке проблемы нет, доказано test-build).

## Решение
`test_runner.rs`: воркеры спавнятся через
`std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn_scoped(s, …)` вместо
`s.spawn(…)`. 64 МБ headroom (вместо ~2 МБ). ~3 строки + комментарий. **Это и есть корень**
(не band-aid): глубина codegen в норме, недосайзен был только стек воркера.

## Спека / D / Q
- **D / Q:** новый D-блок НЕ требуется — это деталь реализации test-runner (размер стека
  воркера), не языковое/тулинговое решение. Зафиксировано комментарием в коде +
  `[M-…]`-маркером + этим планом. Q отсутствует (не открытый вопрос).
- Связано с D277 (test-discovery конвенции) — там механизм lane'ов; здесь — устойчивость
  раннера к большим файлам.

## Тесты (через релизный бинарь)
- **POS (регресс-покрытие):** уже-коммиченный `nova_tests/plan152_4/normalization_conformance.nv`
  (~6000 asserts) через `test-all` теперь **PASS** (был exit 127). Это и есть regression-тест
  (выделенная большая фикстура не нужна — существующая conformance-фикстура её роль играет).
- **NEG/контроль:** `basics` 8/0, plan152_4 без overflow.
- Замечание: plan152_4 = 15/1, где 1 FAIL = flaky `lld-link: cannot open output file`
  (AV-гонка Windows Defender, не регрессия, уходит с `--retries`).

## Критерии приёмки
- **A1.** `test-all` на `normalization_conformance` (и любом большом conformance-файле) НЕ
  падает stack-overflow. ✅ (PASS, был 127).
- **A2.** Дефолтный `nova test` на plan152_4 не падает по overflow (остаётся лишь flaky
  lld-link, ортогональный). ✅
- **A3.** Фикс не меняет поведение для нормальных файлов; сборка зелёная. ✅
- **G0 (обязательный, «без упрощений как для прода»):** это корневой фикс (worker-стек), а
  не маскировка; recursion→iteration осознанно отклонён как непропорциональный. ✅

## Статус по завершении
✅ Реализовано + верифицировано (ветка `plan-cgstack`): `test_runner.rs` worker spawn →
`stack_size(64 MB)`; `normalization_conformance` через test-all PASS; basics 8/0; сборка
release зелёная. Маркер `[M-codegen-conformance-stack-overflow]` закрыт. Смёржено в main.
