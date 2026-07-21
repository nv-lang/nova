<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 198 — nova_tests/ триаж: keep-and-migrate ценное, снести stale (не оптом)

**Статус:** 🔨 Ф.1-Ф.4c ВЫПОЛНЕНЫ (рассинхрон-фикс 2026-07-21 по аудиту 172→221: файл
не обновлялся с 2026-07-11, хотя работа шла — миграция 1000+ файлов в spec_tests,
Ф.4c-карантин разобран, семья `[M-198-f4c-*]` в backlog-followups.md). Остаток: Ф.5
(таблица-вердикт по подпапкам conformance — `[M-198-f5-conformance-subdir-verdict]`) +
живые `[M-198-f4c-2/5/6/7]` — учтены в подплане [221.1](221.1-bug-sweep.md).
Исходная строка: 📋 PROPOSED 2026-07-11 (решение владельца: «возможно там есть реально
нужные тесты — перерефакторить, а не удалять целиком»). **Приоритет:** P2.
**Связано:** [197](197-examples-revision.md) (та же семья legacy-корпус-ревизии),
[169.2](169.2-nova-tests-fix-sweep.md)/[169.1.2](169.1.2-consolidate-tests.md) (прежние
заходы по nova_tests-здоровью). **Разгейт CI уже сделан** (2026-07-11: positive-fast +
contracts → non-blocking; см. `.github/workflows/nova-test-regression.yml`,
`contracts-z3.yml`).

## Проблема

`nova_tests/` по конвенции — **НЕ гейт корректности** ([[feedback-nova-tests-not-correctness-gate]],
[[feedback-module-tests-beside-module]]): тесты std-модулей переезжают в `*_test.nv`
рядом с модулем, spec = conformance. Корпус дрейфанул на retired-API (str.len D249,
newtype-коэрция, plan104-128-дубли) → ~15-20 CODEGEN-FAIL держали CI красным (теперь
разгейчено). **НО** там есть **реально ценное, не покрытое другими гейтами**:

- **14 SOUNDNESS_REGRESSION-файлов** (`contracts/*`, `plan140_4/*`) — сторожат закрытые
  soundness-дыры; их число стережёт CI-ratchet (`contracts-z3.yml` soundness-guard,
  MIN=12). ЗАГРУЖЕНЫ — снести нельзя.
- **byte-identity-baseline** (nova_tests как эталон для codegen-DCE-проверок,
  [[feedback-codegen-dce-verification]]).
- Уникальные репро (напр. `trivial_string_len` — единственный тривиальный
  non-negative-string-length кейс; удалять запрещено шапкой файла).

Значит — **триаж per-subdir, не оптовое удаление**.

## Фазы

- **Ф.1 — census per-subdir (read-only):** пройти каждую папку nova_tests. Классы:
  **KEEP-LOADBEARING** (SOUNDNESS_REGRESSION, byte-identity-baseline, уникальные репро
  не покрытые conformance/module-тестами) / **MIGRATE** (ценно, но место неверное →
  указать цель: `spec_tests/conformance/` или `std/**/*_test.nv`) / **DELETE**
  (superseded conformance/module-тестами, или stale на retired-API). Таблица
  (папка · класс · цель миграции / причина сноса). Числа + список.
- **Ф.2 — миграция KEEP/MIGRATE в настоящие гейты:** soundness-regression → в
  `spec_tests/conformance/` (сохранить маркеры `SOUNDNESS_REGRESSION`, **перевесить
  CI-ratchet на новое место** — обновить `contracts-z3.yml` soundness-guard grep-путь и
  MIN); уникальные репро → `*_test.nv` рядом с релевантным модулем или в conformance.
  Каждый мигрированный — зелёный на НОВОМ гейте (не non-blocking).
- **Ф.3 — снос superseded + 2 отложенных решения:** удалить дубли/stale. Закрыть 2
  deferred (блокировали contracts-z3, ныне разгейчены): **(a)** `trivial_string_len` —
  решить: `byte_len()` в trivial-backend allow-list (компилятор-фикс) ИЛИ пометить
  кейс z3-only ИЛИ мигрировать в conformance как проверку самого E_STR_NO_LEN; **(b)**
  `f26_newtype` — решить семантику newtype↔int-коэрции (нужна явная конверсия
  `int(id)`/`AccountId(42)`? или implicit?) → обновить фикстуру под решение, мигрировать.
- **Ф.4 — уборка вестижа `tests_dir`:** убрать `repo.join("nova_tests")` из nova-cli
  (`resolve_paths`/`RepoPaths.tests_dir`, main.rs:1144) — вырожден: `nova test [path]`
  требует путь → `input_dirs.is_empty()`-fallback (`test_runner.rs:4458`) недостижим;
  display-strip → repo-relative. nova_tests становится обычной папкой без спец-кейса.
  Re-гейт contracts-z3 на мигрированное место (снять non-blocking после Ф.2).
- **Ф.5 — финал:** остаток nova_tests пуст/минимален → удалить папку из дерева;
  снять `nova_tests/STATUS.md`; снять non-blocking-разгейт где уже мигрировано.

## Гейты

Мигрированные тесты зелёные на НОВОМ (жёстком) гейте; soundness-ratchet count сохранён
(≥ прежнего MIN) на новом пути; conformance δ0; grep «нет retired-API в мигрированном»;
после Ф.4 — `nova test`/`nova build` зелёные без `tests_dir`-поля.

## Интерим (сделано в этой волне)

`nova_tests/STATUS.md`: «⚠️ LEGACY/UNVERIFIED — не гейт корректности, частично stale,
под ревизию/миграцию (Plan 198). Настоящие гейты: conformance + module `*_test.nv`.
Load-bearing внутри: SOUNDNESS_REGRESSION + byte-identity-baseline — не трогать до
миграции». CI-разгейт nova_tests — уже применён.

## Открытые решения (sign-off)

1. **Куда soundness-regression** — весь блок в `spec_tests/conformance/` (рек.) или
   отдельная `spec_tests/soundness/`?
2. ✅ РЕШЕНО (владелец 2026-07-11): **newtype↔int = ТОЛЬКО явная конверсия** (`int(id)`/
   `AccountId(42)`) — newtype = отдельный тип (type-safety, как Rust/Haskell). E7301 на
   неявном — корректно; фикстуру f26_newtype правим на явную конверсию (тест был устаревший).
   Требует D-блок-амендмент (язык-решение).
3. ✅ РЕШЕНО (владелец 2026-07-11): **`byte_len()` в TrivialBackend allow-list** (доказать
   `byte_len() >= 0` тривиально, как было у ретирнутого `len`); фикстура чинится на канон.

## Границы

Не переписывает std-тесты (те уже `*_test.nv`). Не про examples ([197](197-examples-revision.md)).
