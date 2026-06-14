<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 156 — Slow-test lane: большие тесты в репо, вне дефолт-регресса (`[M-test-runner-large-test-lane]`)

> **Создан:** 2026-06-14. **Статус:** 📋 PLANNED (дизайн готов, research-обоснован кодом). P2.
> **Владеет:** `[M-test-runner-large-test-lane]`. **Зависит от:** Plan 24/26 (test-runner).
> **Триггер:** ТРЕБОВАНИЕ — дефолтный `nova test`/CI быстрый по компиляции И выполнению
> (см. [docs/test-conventions.md](../test-conventions.md) §«регресс должен быть быстрым»).

## Проблема
Полные conformance-наборы огромны (collation 227800 пар, normalization 19965, …).
Сейчас 5 conformance-файлов (~5 MB) лежат в обычном `nova_tests/plan152_4/` и
**гоняются в каждом `nova test`** — медленно. Нужно: хранить большие тесты в репо, но
**не запускать** их в дефолте; дефолт = малый стайд-сэмпл (1500).

## Решение (research-обоснованное, код-grounded)

### Naming: `slow/` (директория, без `_`) + `_slow.toml` (сентинел, с `_`)
Правило Nova (выявлено при ревью): спец-**директории** — обычным словом (`fixtures/`);
спец-**файлы-сентинелы** — с `_` (`_fixture.toml`, `_module.nv`). Поэтому директория
lane = **`slow/`** (НЕ `_slow/` — было бы непоследовательно с `fixtures/`), опц.
сентинел = **`_slow.toml`** (как `_fixture.toml`). (Ниже по тексту `_slow/` читать как
`slow/`.)

### Механизм отбора — папка `nova_tests/slow/` (НЕ source-маркер)
Скип на этапе **discovery** в `walk_nv` (`test_runner.rs:3302`), зеркало существующего
`is_fixture_dir` (`:3287`, скипает `fixtures/` + sentinel `_fixture.toml`). Папка
`_slow/` (или sentinel `_slow.toml`) пропускается до `read_dir` — **нулевой per-file I/O**,
runner даже не читает 5+ MB. Source-маркер `// EXPECT_SLOW` отвергнут: виден только
после discovery+чтения первых 30 строк (`run_one`), т.е. не убирает файл из hot-path.
Manifest/suffix отвергнуты (дублирование/фрагментация, против D89 anti-pattern).
Опц. fallback — sentinel `_slow.toml` (как `_fixture.toml`) для пометки подзадач.

### Флаги — `--include-slow` (default-skip) + `--slow-only`
- default: `_slow/` скипается (как Rust `#[ignore]`, инверсия Go `-short`).
- `--include-slow`: добавить `_slow/` к обычному прогону (merge-gate/nightly).
- `--slow-only`: только `_slow/` (выделенная CI-job, локальное доказательство G0).
- Композиция с `--filter`/`--skip`/`--include-stdlib` — ортогональна (lane = discovery,
  фильтры — после, `run_all:3649`).

### Реализация (~30 строк, discovery-level)
1. `test_runner.rs`: `is_slow_dir(dir)` рядом с `is_fixture_dir` (имя `_slow` или
   `_slow.toml`).
2. `walk_nv` → `walk_nv_filtered(root, out, include_slow)`; `walk_nv` зовёт с
   `include_slow=true` (сохранить поведение `nova check <dir>`, он `pub`). Гард в начале
   + в цикле рекурсии по подпапкам.
3. `TestAllOpts` (`:2526`): `+include_slow: bool, +slow_only: bool`.
4. `run_all` (`:3606`): `include_slow = opts.include_slow || opts.slow_only`; при
   `slow_only` walk только `_slow/`.
5. `main.rs` clap `TestAll` (`:142`): `--include-slow`/`--slow-only`; протянуть через
   `cmd_test_all` (хардкод-блок `:1063-1092` сейчас не wired — там прецедент).
6. `docs/test-conventions.md`: флипнуть `[M-…]` note на done, описать `_slow/` + флаги.

### Генератор conformance — два lane
`nova-codegen unicode --emit-conformance` пишет ОБА:
- **fast** (committed, default path) `plan152_4/<kind>_conformance.nv`, limit 1500.
- **slow** (committed, opt-in) `nova_tests/_slow/conformance/<kind>_conformance_full.nv`,
  без cap — через новый флаг `--conformance-full` (limit=usize::MAX → stride 1 = всё;
  renderers чанкуют по 500 → ~456 блоков для 227800). `--check` проверяет оба при
  `--conformance-full`. Модули у full-файлов отдельные (`module _slow.conformance.*`).
- collation-генератор пока не существует (`unicode_data.rs` без "collation") — при
  создании следовать тому же паттерну.

### CI — два гейта
- **fast** (каждый PR/push): `nova test` (теперь реально быстрый, `_slow/` исключён) —
  обязательный green-gate.
- **slow** (merge-to-main + nightly cron): `nova test --slow-only --timeout 600` —
  доказательство G0 «без упрощений», out-of-band. Блокирует merge, не блокирует
  итеративные PR-пуши (как LLVM separate suite / CPython resource flags).

### Размер репо — коммитить полные текст-фикстуры
Полный collation `.nv` ~10 MB+; но это детерминированный текст (git delta-жмёт хорошо,
меняется только на Unicode-bump). Коммитить **фикстуры** (не UCD): slow-job
self-contained (только nova+репо, без сети/UCD). `_slow/` вне дефолт-walk → размер не
стоит времени дефолт-прогона, только разовый clone-bandwidth.

### Миграция (важно)
5 текущих `*_conformance.nv` (~5 MB) сейчас в дефолт-пути — перегенерить в малый
fast-сэмпл + полную `_slow/`-копию (git mv/regen), чтобы дефолт-сьюта перестала гонять
5 MB. Опц.: понизить fast-lane limit (1500→300-500), раз breadth теперь в `_slow/`.

## Спека (нормирование runner-конвенций)
Пробел: `fixtures/` + `_fixture.toml` (Plan 55 Ф.8) нормированы **только кодом**
(`test_runner.rs:3282`), в `spec/decisions/` их НЕТ (в отличие от `_module.nv` = D100,
07-modules.md:1575). Раз вводим ещё одну discovery-конвенцию (`_slow/`), нормировать
**обе** D-блоком в `spec/decisions/09-tooling.md` (рядом с D89 EXPECT-маркерами):
test-discovery skip-конвенции — `fixtures/`/`_fixture.toml` (вспом. файлы) +
`_slow/`/`_slow.toml` (большие тесты, opt-in). Подчёркивание-префикс = служебное
имя (как `_module.nv`).

## Критерии приёмки
- A1. `nova test` (default) НЕ запускает `_slow/`; время регресса не растёт от больших фикстур.
- A2. `nova test --include-slow` / `--slow-only` гоняют `_slow/`; композиция с `--filter` работает.
- A3. `nova check nova_tests/_slow/...` (path-based) по-прежнему работает.
- A4. Генератор пишет оба lane детерминированно; `--check` зелёный на обоих.
- A5. Полные наборы (collation/normalization/…) в `_slow/conformance/`, коммитнуты.
- A6. CI: fast-gate + slow-gate (merge/nightly).
- G0: без упрощений — полнота доказывается slow-lane прогоном.

## Прайор-арт
Go `-short`/build-tags; Rust `#[ignore]`+`--ignored`, двухуровневый bors-CI; LLVM
отдельный `test-suite` репо; CPython `@requires_resource`; ICU `testdata/`+`intltest`.
