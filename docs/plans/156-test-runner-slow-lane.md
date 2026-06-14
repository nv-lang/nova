<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 156 — Slow-test lane: большие тесты в репо, вне дефолт-регресса (`[M-test-runner-large-test-lane]`)

> **Создан:** 2026-06-14. **Ревизия:** 2026-06-14 (rev-2: ужато до **suffix-only** —
> единственный механизм `_slow.nv`; папка `slow/`/сентинел `_slow.toml` отложены в
> `[M-156-slow-subtree-dir]` до появления первого медленного folder-module, YAGNI).
> **Статус:** ✅ IMPLEMENTED (suffix-only механизм `_slow.nv` + флаги
> `--include-slow`/`--slow-only`; нормировано [D277](../../spec/decisions/09-tooling.md#d277-test-discovery-skiproute-конвенции--fixtures-os-суффикс-_slownv)).
> Отложен только каталог-вариант `[M-156-slow-subtree-dir]`. P2.
> **Владеет:** `[M-test-runner-large-test-lane]`. **Зависит от:** Plan 24/26 (test-runner).
> **Триггер:** ТРЕБОВАНИЕ — дефолтный `nova test`/CI быстрый по компиляции И выполнению
> (см. [docs/test-conventions.md](../test-conventions.md) §«регресс должен быть быстрым»).

## Проблема
Полные conformance-наборы огромны (collation 227800 пар, normalization 19965, …).
Сейчас 5 conformance-файлов (~5 MB) лежат в обычном `nova_tests/plan152_4/` и
**гоняются в каждом `nova test`** — медленно. Нужно: хранить большие тесты в репо, но
**не запускать** их в дефолте; дефолт = малый стайд-сэмпл (1500).

## Статус реализации

✅ **Suffix-only механизм РЕАЛИЗОВАН** (discovery-level, как спроектировано ниже).
Что зашло, по фазам:

- **Ф.1 — discovery-хелперы:** `is_slow_file_stem(stem) -> stem.ends_with("_slow")`
  рядом с `is_fixture_dir` (`compiler-codegen/src/test_runner.rs`). `walk_nv` →
  `walk_nv_filtered(root, out, lane: SlowLane)`; `walk_nv` зовёт с `SlowLane::Include`
  (поведение path-based `nova check <dir>` сохранено). Гард `_slow` — в цикле
  `direct_nv` рядом с `_module`/OS-суффиксами; снятие суффиксов в каноническом
  порядке (`_module` whole-skip → peel `_slow` → peel `_test` → OS-суффикс на
  `core_stem`).
- **Ф.2 — lane-enum + опция:** `SlowLane { Exclude | Include | Only }` (default
  `Exclude`); `TestAllOpts` получил поле `slow_lane: SlowLane`; `run_all` зовёт
  `walk_nv_filtered(.., opts.slow_lane)` для tests-dir и stdlib-dir.
- **Ф.3 — CLI wiring:** clap-флаги `--include-slow` / `--slow-only` (booleans)
  протянуты через `cmd_test_all` и схлопнуты в `slow_lane` (`slow_only → Only`,
  иначе `include_slow → Include`, иначе `Exclude`).
- **Ф.4 — unit-тесты discovery:** модуль `plan156_slow_lane_tests` в `test_runner.rs`
  (`is_slow_file_stem`-классификация + `walk_nv_filtered` по каждому `SlowLane`).
- **Ф.5 — спека + доки:** [D277](../../spec/decisions/09-tooling.md#d277-test-discovery-skiproute-конвенции--fixtures-os-суффикс-_slownv)
  нормирует все discovery-конвенции (`fixtures/`/`_fixture.toml`, OS-суффикс,
  `_module.nv`, `_slow.nv` + порядок снятия); `docs/test-conventions.md` флипнут на
  IMPLEMENTED.

Отложено: каталог-вариант `slow/` + `_slow.toml` (`[M-156-slow-subtree-dir]`,
см. ниже) — добавится аддитивно для медленных folder-module.

## Решение (research-обоснованное, код-grounded)

### Naming: суффикс `_slow.nv` (единственный механизм)
**Один механизм — per-file суффикс `_slow.nv`.** Прецедент — существующее семейство
суффиксов `_windows.nv`/`_linux.nv`/`_macos.nv`/`_test`/`_module.nv` (snake_case,
`-` нельзя — это идентификатор). Наш реальный кейс (5–6 conformance-корпусов) — это
**по одному сгенерированному файлу**, суффикс покрывает его на 100%.

> **Папка `slow/` + сентинел `_slow.toml` НЕ вводятся** (rev-2). Они оправданы лишь когда
> медленный тест — это folder-module из ≥2 peer'ов (нельзя запускать поодиночке) и ВСЕ
> они медленные. Такого теста сейчас нет → YAGNI. Отложено в `[M-156-slow-subtree-dir]`;
> добавляется аддитивно (зеркало `fixtures/`+`_fixture.toml`), ничего не ломая, когда
> появится первый медленный folder-module.

### Механизм отбора — суффикс `_slow.nv` (НЕ source-маркер)
Решается на этапе **discovery** в `walk_nv` (`test_runner.rs:3302`) — файл с полным корпусом
**никогда не читается** при дефолтном прогоне. Проверка `stem.ends_with("_slow")` в том же
цикле (`:3327-3341`), где уже фильтруются `_windows.nv`/`_module.nv`/`_test`. `read_dir`
даёт лишь dirent (имя), содержимое 5–10 MB **не читается** → нулевой per-file I/O.
Со-локация: `collation_conformance.nv` лежит рядом с `collation_conformance_slow.nv`.

Почему НЕ source-маркер `// EXPECT_SLOW`: виден только после discovery + чтения первых 30
строк (`run_one`), т.е. **не убирает файл из hot-path** (в отличие от суффикса имени, который
виден до чтения). Manifest отвергнут (дублирование, против D89 anti-pattern).

### Флаги — `--include-slow` (default-skip) + `--slow-only`
- default: `*_slow.nv` скипается (как Rust `#[ignore]`, инверсия Go `-short`).
- `--include-slow`: добавить `*_slow.nv` к обычному прогону (merge-gate/nightly).
- `--slow-only`: только `*_slow.nv` (выделенная CI-job, локальное доказательство G0).
- Композиция с `--filter`/`--skip`/`--include-stdlib` — ортогональна (lane = discovery,
  фильтры — после, `run_all:3649`).

### Реализация (~25 строк, discovery-level)
1. `test_runner.rs`: один хелпер рядом с `is_fixture_dir` —
   `is_slow_file(stem)`: `stem.ends_with("_slow")` (per-file суффикс; снимается ДО `_test`
   и OS-суффикса — см. «порядок снятия» ниже).
2. `walk_nv` → `walk_nv_filtered(root, out, include_slow)`; `walk_nv` зовёт с
   `include_slow=true` (сохранить поведение `nova check <dir>`, он `pub`). Гард:
   `is_slow_file` в цикле `direct_nv` рядом с проверкой `_module`/OS-суффиксов
   (`:3327-3341`).
3. `TestAllOpts` (`:2526`): `+include_slow: bool, +slow_only: bool`.
4. `run_all` (`:3606`): `include_slow = opts.include_slow || opts.slow_only`; при
   `slow_only` собрать ТОЛЬКО slow (инверс-фильтр: `walk_nv_filtered(..,true)` минус
   `walk_nv_filtered(..,false)`, либо отдельный обход с `keep = is_slow_file`).
5. `main.rs` clap `TestAll` (`:142`): `--include-slow`/`--slow-only`; протянуть через
   `cmd_test_all` (хардкод-блок `:1063-1092` сейчас не wired — там прецедент).
6. `docs/test-conventions.md`: флипнуть `[M-…]` note на done, описать суффикс `_slow.nv` +
   флаги.

### Генератор conformance — два lane
`nova-codegen unicode --emit-conformance` пишет ОБА (рядом, со-локация через суффикс):
- **fast** (committed, default path) `plan152_4/<kind>_conformance.nv`, limit 1500.
- **slow** (committed, opt-in) `plan152_4/<kind>_conformance_slow.nv`, без cap — через
  новый флаг `--conformance-full` (limit=usize::MAX → stride 1 = всё; renderers чанкуют по
  500 → ~456 блоков для 227800). `--check` проверяет оба при `--conformance-full`. Имя
  модуля у full-файла отдельное (`module …conformance_slow`), чтобы не коллидить с
  fast-сэмплом-peer'ом в той же папке.
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
self-contained (только nova+репо, без сети/UCD). `*_slow.nv` вне дефолт-walk →
размер не стоит времени дефолт-прогона, только разовый clone-bandwidth.

### Миграция (важно)
5 текущих `*_conformance.nv` (~5 MB) сейчас в дефолт-пути — перегенерить в малый
fast-сэмпл (без изменения имени) + полную `*_conformance_slow.nv`-копию (regen), чтобы
дефолт-сьюта перестала гонять 5 MB. Опц.: понизить fast-lane limit (1500→300-500), раз
breadth теперь в slow-файлах.

## Спека (нормирование runner-конвенций)
✅ **СДЕЛАНО** — нормировано в [D277](../../spec/decisions/09-tooling.md#d277-test-discovery-skiproute-конвенции--fixtures-os-суффикс-_slownv)
(09-tooling.md, sibling к D89). Дизайн-обоснование ниже сохранено как rationale.

Пробел: `fixtures/` + `_fixture.toml` (Plan 55 Ф.8) и per-file суффиксы
(`_windows.nv`/`_linux.nv`/`_macos.nv` Plan 42.12, `_test`) нормированы **только кодом**
(`test_runner.rs:3282`,`:3327-3341`), в `spec/decisions/` их НЕТ (в отличие от `_module.nv`
= D100, 07-modules.md:1575). Раз вводим ещё одну discovery-конвенцию (`_slow.nv`),
нормировать **все** одним D-блоком в `spec/decisions/09-tooling.md` (рядом с D89
EXPECT-маркерами): test-discovery skip/route-конвенции —
- папки-скип: `fixtures/`/`_fixture.toml` (вспом. входные данные);
- per-file суффиксы: `_module.nv` (peer-конфиг, не тест), `_windows.nv`/`_linux.nv`/
  `_macos.nv` (OS-гейт), `_test` (наименование, для OS-матча отрезается), `_slow.nv`
  (большой тест, opt-in).
- (отложено `[M-156-slow-subtree-dir]`: папка `slow/`/`_slow.toml` для медленных поддеревьев
  — нормировать, когда будет введена.)

Зафиксировать **порядок снятия суффиксов** (важно для корректной комбинации): `_module`
(скип всего файла) → отрезать `_slow` (роут в slow-lane) → отрезать `_test` → OS-суффикс
на остатке. Тогда суффиксы **комбинируются** корректно: `foo_windows_test.nv` (OS+test),
`collation_conformance_slow.nv` (slow), `foo_windows_slow.nv` (OS+slow — `_slow` снимается
ДО OS-проверки, иначе `file_target_suffix` увидит `…_slow` и вернёт None → OS-гейт
сломается). Канонический порядок в имени: `<core>[_<os>][_test][_slow].nv`.

## Критерии приёмки
- A1. `nova test` (default) НЕ запускает `*_slow.nv`; файл-корпус не читается
  (нулевой per-file I/O); время регресса не растёт от больших фикстур.
- A2. `nova test --include-slow` / `--slow-only` гоняют slow-файлы; композиция с
  `--filter`/`--skip` работает.
- A3. `nova check <path>` на slow-файле (path-based) по-прежнему работает.
- A4. Суффикс `_slow.nv` комбинируется с прочими (`foo_conformance_slow.nv`,
  `bar_windows_slow.nv` — гейтится и по OS, и по slow).
- A5. Генератор пишет оба lane детерминированно (`<kind>_conformance.nv` +
  `<kind>_conformance_slow.nv`); `--check` зелёный на обоих.
- A6. Полные наборы (collation/normalization/…) как `*_conformance_slow.nv`, коммитнуты.
- A7. CI: fast-gate + slow-gate (merge/nightly).
- G0: без упрощений — полнота доказывается slow-lane прогоном.

## Отложено (out-of-scope rev-2)
- **`[M-156-slow-subtree-dir]`** — папка `slow/` + сентинел `_slow.toml` для случая
  «медленный folder-module из ≥2 peers». YAGNI до появления первого такого теста;
  добавляется аддитивно (зеркало `fixtures/`+`_fixture.toml`), не ломая suffix-механизм.

## Прайор-арт
Go `-short`/build-tags; Rust `#[ignore]`+`--ignored`, двухуровневый bors-CI; LLVM
отдельный `test-suite` репо; CPython `@requires_resource`; ICU `testdata/`+`intltest`.
