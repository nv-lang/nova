<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 156 — Slow-test lane: большие тесты в репо, вне дефолт-регресса (`[M-test-runner-large-test-lane]`)

> **Создан:** 2026-06-14. **Ревизия:** 2026-06-14 (добавлен суффикс `_slow.nv` как основной
> механизм для одиночных файлов; папка/сентинел оставлены для поддеревьев). **Статус:** 📋
> PLANNED (дизайн готов, research-обоснован кодом). P2.
> **Владеет:** `[M-test-runner-large-test-lane]`. **Зависит от:** Plan 24/26 (test-runner).
> **Триггер:** ТРЕБОВАНИЕ — дефолтный `nova test`/CI быстрый по компиляции И выполнению
> (см. [docs/test-conventions.md](../test-conventions.md) §«регресс должен быть быстрым»).

## Проблема
Полные conformance-наборы огромны (collation 227800 пар, normalization 19965, …).
Сейчас 5 conformance-файлов (~5 MB) лежат в обычном `nova_tests/plan152_4/` и
**гоняются в каждом `nova test`** — медленно. Нужно: хранить большие тесты в репо, но
**не запускать** их в дефолте; дефолт = малый стайд-сэмпл (1500).

## Решение (research-обоснованное, код-grounded)

### Naming: суффикс `_slow.nv` (одиночные файлы, основной) + папка `slow/` / сентинел `_slow.toml` (поддеревья)
Два механизма, **зеркало fixtures** (которые уже работают двумя способами — папка
`fixtures/` ИЛИ сентинел `_fixture.toml`):

| Случай | Механизм | Прецедент |
|---|---|---|
| **один большой файл** (наш case: 5–6 conformance-корпусов) | **суффикс `_slow.nv`** | семейство `_windows.nv`/`_linux.nv`/`_test`/`_module.nv` |
| целое медленное поддерево / folder-module из peers | папка **`slow/`** (без `_`) или сентинел **`_slow.toml`** | `fixtures/` + `_fixture.toml` |

Правило Nova: спец-**директории** — обычным словом (`fixtures/`, `slow/`); спец-**файлы**
(сентинелы и per-file суффиксы) — с `_`-аффиксом (`_fixture.toml`, `_module.nv`,
`_windows.nv`). Поэтому директория lane = `slow/` (НЕ `_slow/` — было бы непоследовательно
с `fixtures/`).

### Механизм отбора — суффикс `_slow.nv` ИЛИ папка `slow/` (НЕ source-маркер)
Оба варианта решаются на этапе **discovery** в `walk_nv` (`test_runner.rs:3302`) — файл с
полным корпусом **никогда не читается** при дефолтном прогоне:

- **Суффикс `_slow.nv`** — проверка `stem.ends_with("_slow")` в том же цикле
  (`:3327-3341`), где уже фильтруются `_windows.nv`/`_module.nv`/`_test`. `read_dir` даёт
  лишь dirent (имя), содержимое 5–10 MB **не читается** → нулевой per-file I/O. Это
  **основной** механизм: со-локация (`collation_conformance.nv` рядом с
  `collation_conformance_slow.nv`), попадает в существующее семейство суффиксов, не плодит
  дерево.
- **Папка `slow/` / сентинел `_slow.toml`** — скип до `read_dir`, зеркало `is_fixture_dir`
  (`:3287`). Для случая «медленное поддерево» (folder-module из многих peers): один
  dir-check выкидывает всё дерево.

Почему НЕ source-маркер `// EXPECT_SLOW`: виден только после discovery + чтения первых 30
строк (`run_one`), т.е. **не убирает файл из hot-path** (в отличие от суффикса имени, который
виден до чтения). Manifest отвергнут (дублирование, против D89 anti-pattern).

### Флаги — `--include-slow` (default-skip) + `--slow-only`
- default: `_slow/` скипается (как Rust `#[ignore]`, инверсия Go `-short`).
- `--include-slow`: добавить `_slow/` к обычному прогону (merge-gate/nightly).
- `--slow-only`: только `_slow/` (выделенная CI-job, локальное доказательство G0).
- Композиция с `--filter`/`--skip`/`--include-stdlib` — ортогональна (lane = discovery,
  фильтры — после, `run_all:3649`).

### Реализация (~35 строк, discovery-level)
1. `test_runner.rs`: два хелпера рядом с `is_fixture_dir` —
   - `is_slow_file(path)`: `path.file_stem().ends_with("_slow")` (per-file суффикс).
   - `is_slow_dir(dir)`: имя `slow` или сентинел `_slow.toml` (поддеревья).
2. `walk_nv` → `walk_nv_filtered(root, out, include_slow)`; `walk_nv` зовёт с
   `include_slow=true` (сохранить поведение `nova check <dir>`, он `pub`). Гарды:
   `is_slow_dir` в начале + в цикле рекурсии по подпапкам; `is_slow_file` в цикле
   `direct_nv` рядом с проверкой `_module`/OS-суффиксов (`:3327-3341`).
3. `TestAllOpts` (`:2526`): `+include_slow: bool, +slow_only: bool`.
4. `run_all` (`:3606`): `include_slow = opts.include_slow || opts.slow_only`; при
   `slow_only` собрать ТОЛЬКО slow (инверс-фильтр: `walk_nv_filtered(..,true)` минус
   `walk_nv_filtered(..,false)`, либо отдельный обход с `keep = is_slow_*`).
5. `main.rs` clap `TestAll` (`:142`): `--include-slow`/`--slow-only`; протянуть через
   `cmd_test_all` (хардкод-блок `:1063-1092` сейчас не wired — там прецедент).
6. `docs/test-conventions.md`: флипнуть `[M-…]` note на done, описать суффикс `_slow.nv` +
   папку `slow/` + флаги.

### Генератор conformance — два lane
`nova-codegen unicode --emit-conformance` пишет ОБА (рядом, со-локация через суффикс):
- **fast** (committed, default path) `plan152_4/<kind>_conformance.nv`, limit 1500.
- **slow** (committed, opt-in) `plan152_4/<kind>_conformance_slow.nv`, без cap — через
  новый флаг `--conformance-full` (limit=usize::MAX → stride 1 = всё; renderers чанкуют по
  500 → ~456 блоков для 227800). `--check` проверяет оба при `--conformance-full`. Имя
  модуля у full-файла отдельное (`module …conformance_slow`), чтобы не коллидить с
  fast-сэмплом-peer'ом в той же папке.
- Альтернатива для очень больших корпусов — папка `plan152_4/slow/<kind>_full.nv` (если
  хочется отделить визуально); суффикс предпочтительнее ради со-локации с fast-сэмплом.
- collation-генератор уже существует (`unicode_data.rs`, `render_collation_conformance_nv`)
  — добавить `--conformance-full` ветку; остальные kind следуют тому же паттерну.

### CI — два гейта
- **fast** (каждый PR/push): `nova test` (теперь реально быстрый, `_slow/` исключён) —
  обязательный green-gate.
- **slow** (merge-to-main + nightly cron): `nova test --slow-only --timeout 600` —
  доказательство G0 «без упрощений», out-of-band. Блокирует merge, не блокирует
  итеративные PR-пуши (как LLVM separate suite / CPython resource flags).

### Размер репо — коммитить полные текст-фикстуры
Полный collation `.nv` ~10 MB+; но это детерминированный текст (git delta-жмёт хорошо,
меняется только на Unicode-bump). Коммитить **фикстуры** (не UCD): slow-job
self-contained (только nova+репо, без сети/UCD). `_slow.nv`/`slow/` вне дефолт-walk →
размер не стоит времени дефолт-прогона, только разовый clone-bandwidth.

### Миграция (важно)
5 текущих `*_conformance.nv` (~5 MB) сейчас в дефолт-пути — перегенерить в малый
fast-сэмпл (без изменения имени) + полную `*_conformance_slow.nv`-копию (regen), чтобы
дефолт-сьюта перестала гонять 5 MB. Опц.: понизить fast-lane limit (1500→300-500), раз
breadth теперь в slow-файлах.

## Спека (нормирование runner-конвенций)
Пробел: `fixtures/` + `_fixture.toml` (Plan 55 Ф.8) и per-file суффиксы
(`_windows.nv`/`_linux.nv`/`_macos.nv` Plan 42.12, `_test`) нормированы **только кодом**
(`test_runner.rs:3282`,`:3327-3341`), в `spec/decisions/` их НЕТ (в отличие от `_module.nv`
= D100, 07-modules.md:1575). Раз вводим ещё одну discovery-конвенцию (`_slow.nv`/`slow/`),
нормировать **все** одним D-блоком в `spec/decisions/09-tooling.md` (рядом с D89
EXPECT-маркерами): test-discovery skip/route-конвенции —
- папки-скип: `fixtures/`/`_fixture.toml` (вспом. входные данные), `slow/`/`_slow.toml`
  (большие поддеревья, opt-in);
- per-file суффиксы: `_module.nv` (peer-конфиг, не тест), `_windows.nv`/`_linux.nv`/
  `_macos.nv` (OS-гейт), `_test` (наименование, для OS-матча отрезается), `_slow.nv`
  (большой тест, opt-in).

Зафиксировать **порядок снятия суффиксов** (важно для корректной комбинации): `_module`
(скип всего файла) → отрезать `_slow` (роут в slow-lane) → отрезать `_test` → OS-суффикс
на остатке. Тогда суффиксы **комбинируются** корректно: `foo_windows_test.nv` (OS+test),
`collation_conformance_slow.nv` (slow), `foo_windows_slow.nv` (OS+slow — `_slow` снимается
ДО OS-проверки, иначе `file_target_suffix` увидит `…_slow` и вернёт None → OS-гейт
сломается). Канонический порядок в имени: `<core>[_<os>][_test][_slow].nv`.

## Критерии приёмки
- A1. `nova test` (default) НЕ запускает ни `*_slow.nv`, ни `slow/`; файл-корпус не читается
  (нулевой per-file I/O); время регресса не растёт от больших фикстур.
- A2. `nova test --include-slow` / `--slow-only` гоняют slow-файлы и `slow/`; композиция с
  `--filter`/`--skip` работает.
- A3. `nova check <path>` на slow-файле/папке (path-based) по-прежнему работает.
- A4. Суффикс `_slow.nv` комбинируется с прочими (`foo_conformance_slow.nv`,
  `bar_windows_slow.nv` — гейтится и по OS, и по slow).
- A5. Генератор пишет оба lane детерминированно (`<kind>_conformance.nv` +
  `<kind>_conformance_slow.nv`); `--check` зелёный на обоих.
- A6. Полные наборы (collation/normalization/…) как `*_conformance_slow.nv`, коммитнуты.
- A7. CI: fast-gate + slow-gate (merge/nightly).
- G0: без упрощений — полнота доказывается slow-lane прогоном.

## Прайор-арт
Go `-short`/build-tags; Rust `#[ignore]`+`--ignored`, двухуровневый bors-CI; LLVM
отдельный `test-suite` репо; CPython `@requires_resource`; ICU `testdata/`+`intltest`.
