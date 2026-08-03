---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

# Nova CLI

[English](nova-cli.md) | **Русский**

`nova` — единая точка входа в инструментарий языка Nova. Заменяет
`run_tests.ps1` / `run_tests.sh` / `regen_runtime.ps1` (см. [Plan 28](../plans/28-nova-cli.md)).

Версия: `0.1.0` (bootstrap). Бинарник публикуется как `nova` (Cargo
package `nova`, crate `nova-cli`).

---

## Содержание

- [Quickstart](#quickstart)
- [Установка и сборка](#установка-и-сборка)
- [Глобальные флаги](#глобальные-флаги)
- [Коды выхода](#коды-выхода)
- [Поиск корня проекта](#поиск-корня-проекта)
- [Команды](#команды)
  - [`nova check`](#nova-check) — проверка типов
  - [`nova run`](#nova-run) — интерпретатор (сейчас **НЕ поддерживается**)
  - [`nova add`](#nova-add) — добавить зависимость
  - [`nova update`](#nova-update) — переразрешить git-пины
  - [`nova info`](#nova-info) — effect-surface пакета
  - [`nova build`](#nova-build) — компиляция в нативный бинарник
  - [`nova test`](#nova-test) — запуск тестов
  - [`nova test-build`](#nova-test-build) — сборка + запуск одного теста
  - [`nova regen-runtime`](#nova-regen-runtime) — регенерация стабов рантайма
  - [`nova doc`](#nova-doc) — документация (Plan 45)
  - [`nova doc-query`](#nova-doc-query) — DSL-запросы к JSON-выводу
  - [`nova doc-mcp`](#nova-doc-mcp) — MCP-сервер
  - [`nova contracts`](#nova-contracts) — инспекция контрактов (Plan 33.3)
  - [`nova bench`](#nova-bench) — инфраструктура бенчмарков (Plan 57)
  - [`nova consume-analyze`](#nova-consume-analyze) — покрытие consume-типов (Plan 100.8)
- [Переменные окружения](#переменные-окружения)
- [Migration-бинарники](#migration-бинарники)
- [Связанные документы](#связанные-документы)

---

## Quickstart

```bash
# Inside a Nova project (sibling nova.toml present). Modules live at the
# package root — there is no `src/` directory (D78).
nova check                       # type-check whole workspace
nova check encoding/             # walk a directory recursively
nova check lib.nv                # single file

nova build app.nv -o app         # compile to a native binary (the way to run code)
./app                            # then execute it
nova add mathlib --path ../mathlib   # add a dependency, update nova.lock.toml
nova info mathlib                # a dependency's effect-surface
nova test nova_tests             # compile + run all nova_tests/
nova test nova_tests/plan118     # a single subdirectory
nova test std nova_tests         # multiple paths: std/ + nova_tests/
nova test --filter basics        # substring subset

nova doc lib.nv                  # markdown to stdout
nova doc . --format json         # D107 JSON schema
nova doc . --check --strict      # CI doc validation

nova bench run bench.nv          # run benchmarks
nova contracts verify foo.nv     # SMT-verify contracts
```

> **Интерпретатора нет.** Nova компилируется в C — команды `nova run`
> для запуска нет. Чтобы выполнить программу — `nova build`, затем
> запусти бинарник; для тестов — `nova test`. См. [`nova run`](#nova-run)
> ниже.

---

## Установка и сборка

`nova-cli` живёт в `nova-cli/` рядом с `compiler-codegen/`. Рабочее
пространство не используется (см. [Plan 28](../plans/28-nova-cli.md) — оба крейта
самостоятельные).

```bash
# Debug build (default, opt-level=0)
cargo build --manifest-path nova-cli/Cargo.toml

# Release (opt-level=2, LTO thin)
cargo build --release --manifest-path nova-cli/Cargo.toml

# With the Z3 backend for contracts (Plan 33.1)
cargo build --release --manifest-path nova-cli/Cargo.toml --features z3-backend
```

Получаешь:
- `nova-cli/target/{debug,release}/nova[.exe]`
- `nova-cli/target/{debug,release}/migrate_plan60[.exe]`
- `nova-cli/target/{debug,release}/migrate_plan65[.exe]`

`nova` имеет зависимость по пути на `nova_codegen` (`../compiler-codegen`)
— пересборка компилятора автоматически перекомпилирует CLI.

---

## Глобальные флаги

Применяются ко всем подкомандам:

| Флаг | Значения | Описание |
|---|---|---|
| `--color` | `auto` (по умолчанию), `always`, `never` | Управление ANSI-цветами. См. [Plan 36](../plans/36-cli-production-hardening.md) R10. |

**Автоопределение цвета** (приоритет от высокого к низкому):

1. CLI `--color always|never` — принудительно
2. `CLICOLOR_FORCE=1` → always
3. `NO_COLOR` (любое значение) → never ([no-color.org](https://no-color.org))
4. `CLICOLOR=0` → never
5. `CI=true` → never
6. `TERM=dumb` → never
7. По умолчанию — включено

### Тюнинг field-cache (advanced)

Каждая подкоманда также принимает «ручки» field-caching из
[Plan 123](../plans/123.1-core-cse.md). Это флаги для диагностики (forensic)
и запасного выхода (escape hatch)
— значения по умолчанию корректны для обычного использования; трогать их нужно только
при расследовании регрессии codegen-кэша.

| Флаг | Эффект |
|---|---|
| `--no-field-cache` | Полностью выключить field caching (== `NOVA_FIELD_CACHE=0`) |
| `--no-field-cache-licm` | Выключить фазу LICM (D218) |
| `--no-field-cache-pure` | Выключить фазу pure-call cache (D219) |
| `--no-field-cache-chain` | Выключить фазу chain cache (D217 V4) |
| `--no-field-cache-ipa` | Выключить IPA-refinements (D223 V7.1) |
| `--field-cache-threshold N` | Мин. чтений `@field` для кэша (по умолчанию 2) |
| `--field-cache-licm-threshold N` | Мин. чтений внутри цикла (по умолчанию 2) |
| `--field-cache-pure-threshold N` | Мин. вызовов `@method()` (по умолчанию 2) |
| `--field-cache-chain-threshold N` | Мин. вхождений цепочки (по умолчанию 2) |
| `--field-cache-max N` | Лимит на функцию по всем слоям (по умолчанию 8) |
| `--field-cache-licm-max N` | Лимит на цикл для LICM (по умолчанию 4) |
| `--field-cache-chain-depth N` | Макс. глубина цепочки (по умолчанию 4, мин. 2) |
| `--field-cache-ipa-iter N` | Лимит итеративного замыкания IPA (по умолчанию 10) |

`nova check` дополнительно открывает `--explain-cache`,
`--telemetry-cache`, `--telemetry-json`, `--telemetry-baseline FILE`,
`--telemetry-gate-affected-drop F` и `--telemetry-gate-caches-drop F`
для отчётов по анализу кэша и CI-проверок на регрессии.

Флаги field-cache опущены в таблицах ниже (по командам) ради читаемости;
считай, что их полное семейство принимает каждая команда.

---

## Коды выхода

Cargo-конвенция ([Plan 36](../plans/36-cli-production-hardening.md) R7):

| Код | Значение |
|---|---|
| `0` | Успех |
| `1` | Диагностическая ошибка (ошибка типизации, упавший тест, нарушение контракта и т.п.) |
| `2` | Ошибка использования (неверный флаг, файл не найден, не `.nv`, нет `nova.toml`) |
| `101` | Внутренняя паника (через `std::panic::set_hook` для единообразия на разных платформах) |

`nova doc --diff` дополнительно использует **3** = критическое изменение
уровня патча (см. [`nova doc`](#nova-doc)).

---

## Поиск корня проекта

Большинство команд ищут `nova.toml` снизу вверх от CWD. Логика
вынесена в `nova_codegen::test_runner::find_repo_root_from`:

1. Идём от CWD вверх до корня файловой системы
2. На каждом уровне читаем `nova.toml` если есть
3. Если в нём есть `[workspace]` — это и есть корень (корень
   рабочего пространства), останавливаемся
4. Иначе запоминаем последний найденный `nova.toml` и идём дальше
5. Если найден корень с `[workspace]` — возвращаем его, иначе —
   самый верхний `nova.toml`

Это поведение **с учётом рабочего пространства** (workspace-aware; D78 AD6,
[Plan 35](../plans/35-cross-file-resolve.md)) — защищает от ситуации, когда
вложенный `nova_tests/nova.toml` затмил настоящий корень.

Если `nova.toml` не найден — код выхода `2`:
```
error: nova.toml not found — are you inside a Nova project?
```

Пути, разрешаемые от корня рабочего пространства:
- `<root>/nova_tests/` — корпус тестов
- `<root>/std/` — стандартная библиотека
- `<root>/compiler-codegen/` — include-пути C-runtime
- `<root>/compiler-codegen/nova_rt/` — runtime sources (libuv, GC)
- `<root>/target/last-test-results.json` — кеш `--rerun-failed`

---

## Команды

### `nova check`

Проверка типов одного или нескольких `.nv` файлов / директорий. Plan 36
MVP — заменяет `nova-codegen check`.

```
nova check [PATHS...] [--jobs N] [-q|-v] [--list] [--format human|short]
           [--include-runtime] [--skip PATTERN]...
```

**Позиционные аргументы:**

- `PATHS` — список файлов или директорий. Если пусто — корень рабочего пространства
  (рекурсивно). Файл должен иметь расширение `.nv`, иначе код выхода `2`.

**Флаги:**

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--jobs N` | `0` (= num_cpus) | Параллельных воркеров |
| `-q`, `--quiet` | off | Только FAIL-строки и сводку |
| `-v`, `--verbose` | off | Дополнительная информация (время выполнения) |
| `--list` | off | Показать список файлов, не проверяя |
| `--format` | `human` | `human` (цветной) или `short` (`file:line:col: msg` для grep) |
| `--include-runtime` | off | Включить `std/runtime/` (автосгенерированный, по умолчанию пропускается) |
| `--skip PATTERN` | `[]` | Пропустить файлы по подстроке (повторяемый) |

**Жёстко зашитый пропуск** (всегда исключаются):

- `target/`, `node_modules/`, `vendor/`
- `.git/`, `.hg/`, `.svn/`
- директории, начинающиеся с `_` или `.`
- `std/runtime/` (переопределяется через `--include-runtime`)

**Поведение:**

- Дедупликация через `canonicalize`
- Сортировка для детерминизма
- Параллельный обход через `thread::scope` + mpsc-канал
- Пофайловые предупреждения (`yellow: warning:`) после `ok:`-строки
- Сводка: `pass=N fail=N warnings=N (X.YYs)`
- Код выхода `1` при любом FAIL, `2` при ошибке использования

`--format short`:
```
lib.nv: ok
parser.nv:42:5: error: type mismatch
```

`--format human` (по умолчанию):
```
ok: lib.nv
FAIL: parser.nv
  parser.nv:42:5: type mismatch
```

**JSON / SARIF / JUnit** форматы зарезервированы под подплан 36.A,
сейчас не реализованы.

---

### `nova run`

> **Сейчас НЕ поддерживается.** Интерпретатор с обходом дерева отключён.

`nova run` остаётся видимой подкомандой, но при вызове падает с ошибкой
и направляет на путь C-codegen:

```
nova run FILE
```

```
error: the Nova interpreter (`nova run`) is currently NOT supported.
Use `nova build <file>` to compile to an executable, or `nova test` to
compile and run tests (both via C codegen).
```

(код выхода `1`).

Nova компилируется в C; поддерживаемого интерпретатора нет. Чтобы
выполнить программу — [`nova build`](#nova-build) и запусти полученный
бинарник; чтобы скомпилировать и прогнать тесты — [`nova test`](#nova-test)
/ [`nova test-build`](#nova-test-build).

---

### `nova add`

Добавить зависимость в `[dependencies]` `nova.toml` текущего пакета и
обновить `nova.lock.toml` ([Plan 03.1](../plans/03.1-path-git-dependencies.md)).

```
nova add NAME (--path DIR | --git URL [--tag T | --branch B | --rev R | --version REQ])
```

| Флаг | Описание |
|---|---|
| `NAME` | Имя зависимости — должно совпадать с `[package].name` пакета-зависимости |
| `--path DIR` | Локальная зависимость по пути (другой пакет на диске) |
| `--git URL` | Git-зависимость (URL репозитория) |
| `--tag T` | Git-пин: тег (только с `--git`) |
| `--branch B` | Git-пин: ветка (только с `--git`) |
| `--rev R` | Git-пин: коммит / ревизия (только с `--git`) |
| `--version REQ` | Git-пин: semver-диапазон, напр. `^1.2` (только с `--git`, [Plan 03.2](../plans/03.2-version-resolution.md)) |

- `--path` и `--git` взаимоисключающие; ровно один обязателен.
- `--tag` / `--branch` / `--rev` / `--version` взаимоисключающие;
  опциональны (без пина — ветка по умолчанию, в lock всё равно пишется
  точный коммит).
- `--version` выбирает наибольший подходящий semver-тег репозитория и
  пишет в `nova.lock.toml` и версию, и коммит.
- Правит секцию `[dependencies]` (создаёт при отсутствии). Дубль имени
  → код выхода `2`.
- После правки запускает синхронизацию lock: материализует git-зависимость в
  кэше и пишет разрешённый коммит в `nova.lock.toml`.
- Работает только внутри пакета (`nova.toml` с `[package]`), не на
  голом `[workspace]`-манифесте.

```bash
nova add mathlib --path ../mathlib
nova add gitlib  --git https://example.org/gitlib.nv --tag v1.0.0
nova add libfoo  --git https://example.org/libfoo.nv --version "^1.2"
```

---

### `nova update`

Переразрешить git-зависимости и обновить `nova.lock.toml`
([Plan 03.1](../plans/03.1-path-git-dependencies.md) /
[03.2](../plans/03.2-version-resolution.md)).

```
nova update [NAME] [--precise NAME@VERSION]
```

- `NAME` — конкретная git-зависимость для обновления. Без аргумента —
  все git-зависимости.
- Снимает целевые git-записи из `nova.lock.toml`, затем переразрешает:
  пины по ветке и тегу берут текущий коммит, `version`-диапазоны —
  наибольший подходящий тег. Остальные остаются зафиксированными
  (воспроизводимость).
- `--precise NAME@VERSION` — зафиксировать `version`-диапазонную
  git-зависимость на точной версии (напр. `nova update --precise
  libfoo@1.2.0`). Резолвер обязан согласовать её с остальным деревом,
  иначе — конфликт.
- `path`-зависимости пинов не имеют — такой аргумент отвергается с
  пояснением.

---

### `nova info`

Показать **effect-surface** пакета — агрегированные эффекты его
публичного API ([Plan 03.4](../plans/03.4-effect-aware-tooling.md) / D140).
Nova-уникальное: в Cargo/npm узнать, что зависимость ходит в сеть, без
аудита кода невозможно.

```
nova info TARGET [--format human|json] [--diff BASE [--fail-on-new]]
```

| Флаг | Описание |
|---|---|
| `TARGET` | Путь к пакету (`.nv`-файл / каталог) либо имя зависимости из `[dependencies]` текущего пакета |
| `--format` | `human` (по умолчанию) или `json` |
| `--diff BASE` | Сравнить effect-surface TARGET с BASE (путь либо зависимость) — добавленные/убранные эффекты |
| `--fail-on-new` | С `--diff`: ненулевой код выхода при появлении новых эффектов (CI-проверка против дрейфа цепочки поставок) |

- Effect-surface = объединение эффектов всех `export`-функций (D28 —
  публичные функции объявляют эффекты явно → surface точна без
  межпроцедурного анализа). Приватные функции не входят.
- `--diff` — сигнал цепочки поставок: `Net`/`Fs`, появившиеся в
  patch/minor-релизе ранее «чистого» API, — красный флаг.

```bash
nova info ./mylib                    # effect-surface of a local package
nova info somedep                    # of a declared dependency
nova info somedep --format json
nova info ./v2 --diff ./v1 --fail-on-new   # CI: fail if v2 added effects
```

**Ограниченные по правам зависимости.** Зависимость можно ограничить через
`forbid` в `nova.toml`:

```toml
[dependencies]
parser = { git = "https://example.org/parser.nv", forbid = ["Net", "Fs"] }
```

`nova build` вычисляет effect-surface зависимости и **проваливает сборку**,
если она использует запрещённый эффект — песочница на уровне типов
(сильнее моделей разрешений в рантайме). См. D63 / D140.

---

### `nova build`

Скомпилировать **один** `.nv`-файл в нативный бинарник (через C-бэкенд).

```
nova build FILE [-o OUTPUT] [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
           [--vcvars PATH] [--clang PATH] [--timeout SECS] [--keep-artifacts]
           [--mono-depth N]
```

**Только один файл за раз** — `-o` принимает один путь. Для многофайловых
проектов используй `import` внутри точки входа.

**Аргументы:**

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | Точка входа `.nv` с `fn main` |
| `-o OUTPUT` | `<name>[.exe]` в CWD | Путь к выходному бинарнику |
| `--mode` | `dev` | `dev` (без оптимизации) или `release` (`-O2` + LTO) |
| `--toolchain` | `auto` | `auto` (Clang → MSVC → GCC), `clang`, `msvc`, `gcc` |
| `--vcvars` | auto через vswhere | Путь к `vcvars64.bat` (Windows) |
| `--clang` | автоопределение | Путь к `clang.exe` |
| `--timeout` | `120` | Таймаут компиляции в секундах |
| `--keep-artifacts` | off | Не удалять `.c`/`.exe`/`.obj` в tmp |
| `--mono-depth N` | `500` (или `NOVA_MONO_DEPTH`) | Лимит глубины инстанциации при мономорфизации ([Plan 48](../plans/48-closures-in-generics.md) Ф.7.6) |

**Временная директория:** `$TEMP/nova_tests/build/<path-hash>/` (Windows) или
`$TMPDIR/nova_tests/build/<path-hash>/` (Unix). Хеш через
`DefaultHasher` от абсолютного пути файла — обеспечивает
уникальность без криптозависимости.

**Pipeline:**

1. parse + typecheck + `infer_effects`
2. `CEmitter::emit_module` → C-код
3. `detect_toolchain()` (с автоопределением vcvars)
4. `detect_or_build_libuv()` — runtime может зависеть от libuv
5. `compile_c_to_exe(&tc, &build_opts, timeout)`
6. Копирование exe → `-o` или CWD
7. Удаление tmp (если не `--keep-artifacts`)

---

### `nova test`

Запуск тестов из директории или файла. Plan 28 (вместе с
[Plan 26](../plans/26-test-runner-hardening.md), [Plan 27](../plans/27-gc-switch.md),
[Plan 34](../plans/34-stdlib-typecheck-and-compile-fix.md)).

```
nova test [PATH]... [--filter SUBSTR] [--jobs N] [--format text|json|tap|junit]
          [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
          [--vcvars PATH] [--clang PATH] [--timeout SECS] [-v|-q]
          [--results-file PATH] [--rerun-failed] [--retries N]
          [--keep-artifacts] [--gc boehm|malloc]
          [--list] [--filter-from PATH] [--shuffle [SEED]]
          [--skip PATTERN]... [--mono-depth N]
          [--positive] [--compile-error] [--panic] [--timeout-type]
          [--exit] [--slow] [--full]
```

**Аргументы:**

| Флаг | По умолчанию | Описание |
|---|---|---|
| `PATH...` | — (обязательный) | Файлы и/или директории с тестами (минимум один) |
| `--filter SUBSTR` | — | Фильтр по отображаемому имени (подстрока) |
| `--jobs N` | `0` (= num_cpus) | Параллельные воркеры |
| `--format` | `text` | `text`, `json`, `tap`, `junit` |
| `--mode` | `dev` | `dev` или `release` |
| `--toolchain` | `auto` | `auto`, `clang`, `msvc`, `gcc` |
| `--vcvars` | auto | Путь к `vcvars64.bat` |
| `--clang` | auto | Путь к `clang.exe` |
| `--timeout` | `60` | Таймаут на тест (секунды) |
| `-v`, `--verbose` | off | Вывод проходящих тестов |
| `-q`, `--quiet` | off | Только FAIL-строки и сводку |
| `--results-file PATH` | `<root>/target/last-test-results.json` | Куда писать результаты |
| `--rerun-failed` | off | Перезапустить только проваленные/по таймауту из последнего прогона |
| `--retries N` | `0` | Повторов на временных сбоях (гонки AV и т.п.) |
| `--keep-artifacts` | off | Не удалять `.c`/`.exe`/`.obj` |
| `--gc` | `boehm` | `boehm` (по умолчанию) или `malloc` (только для внутреннего использования) |
| `--list` | off | Список тестов без запуска |
| `--filter-from PATH` | — | Файл с именами тестов (по одному на строку, точное совпадение) |
| `--shuffle [SEED]` | off | Случайный порядок; опциональный seed для воспроизводимости |
| `--skip PATTERN` | `[]` | Пропустить тесты по подстроке имени или пути (повторяемый) |
| `--mono-depth N` | `500` (или env) | Лимит глубины инстанциации при мономорфизации |
| `--positive` | on (по умолчанию) | Выбрать позитивные тесты (без `EXPECT_*`-маркера). По умолчанию, когда не задан ни один флаг категории. |
| `--compile-error` | off | Выбрать тесты `EXPECT_COMPILE_ERROR`. |
| `--panic` | off | Выбрать тесты `EXPECT_RUNTIME_PANIC`. |
| `--timeout-type` | off | Выбрать тесты `EXPECT_TIMEOUT`. |
| `--exit` | off | Выбрать тесты `EXPECT_EXIT_CODE`. |
| `--slow` | off | Дополнительно включить `*_slow.nv` (любого типа). Алиас: `--include-slow`. |
| `--full` | off | Все типы + slow (`--positive --compile-error --panic --timeout-type --exit --slow`). |

**Флаги категорий** (Plan 169.1.1, D304) аддитивны — несколько флагов объединяют
свои наборы тестов (OR). Без флага категории по умолчанию выбираются только
позитивные, быстрые (не медленные) тесты. Тип теста определяется по первому
`EXPECT_*`-маркеру в заголовке файла (первые 30 строк), а не по папке —
поэтому негативные тесты находятся даже вне `neg/`.

**Несколько путей** (Plan 36.D.1): передавать любое количество путей — директорий и/или файлов.
Минимум один путь обязателен (Plan 172.6). Чтобы добавить `std/`:

```bash
nova test nova_tests             # nova_tests/ only
nova test std nova_tests         # std/ + nova_tests/ together
nova test nova_tests/plan118     # specific subdirectory
```

**Отображаемое имя** формируется как путь от текущего рабочего каталога (cwd):
`nova_tests/plan118/t1_parse_ok` вместо абсолютного пути.

**Форматы вывода:**

- `text` — человекочитаемый, цветной, в stdout
- `json` — массив объектов с полями `name`, `status`, `duration_ms`, `stderr`
- `tap` — Test Anything Protocol v13
- `junit` — JUnit XML (для CI-агрегаторов)

**`--rerun-failed`:** читает `--results-file`, выбирает записи с
`status != "pass"`, фильтрует набор, запускает только их.

**EXPECT-маркеры** в тестовых файлах (см.
[docs/dev/test-conventions.md](../dev/test-conventions.md)):
- `// EXPECT: <stdout-line>` — точное совпадение строки
- `// EXPECT_STDERR: <line>` — для stderr
- `// EXPECT_COMPILE_ERROR: <substring>` — должно упасть при компиляции
- `// EXPECT_RUNTIME_ERROR: <substring>` — panic с подстрокой
- `// REQUIRES_SMT_BACKEND` — пропуск если SMT недоступен

---

### `nova test-build`

Сборка + запуск **одного** тестового файла. Используется IDE / CI для
точечной отладки.

```
nova test-build FILE [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
                [--vcvars PATH] [--clang PATH] [--timeout SECS]
                [--keep-artifacts] [--gc boehm|malloc] [--mono-depth N]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | Путь к `.nv`-тесту |
| `--mode` | `dev` | См. [`nova test`](#nova-test) |
| `--toolchain` | `auto` | |
| `--vcvars` | auto | |
| `--clang` | auto | |
| `--timeout` | `60` | |
| `--keep-artifacts` | off | |
| `--gc` | `boehm` | |
| `--mono-depth N` | `500` | |

Эквивалентно `nova test <FILE>`, но без механизмов массового запуска
(одиночный exe, один тест-блок в файле).

---

### `nova regen-runtime`

Регенерация `std/runtime/*.nv` стабов из реестра рантайма компилятора.
Заменяет `regen_runtime.ps1`.

```
nova regen-runtime [--check]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--check` | off | Только сравнить — код выхода `1`, если файлы расходятся с реестром (CI-проверка) |

Под капотом — `nova_codegen::codegen::runtime_registry::all()` +
render каждого модуля. См. [Plan 13](../plans/13-runtime-stdlib-and-autogen.md).

---

### `nova doc`

Документация уровня production (Plan 45 / D107). Markdown / JSON / HTML
+ doc-tests + покрытие + мутационное тестирование + watch + режим рабочего пространства.

```
nova doc [FILE] [--format markdown|json|html] [--json-schema]
         [--include-private] [--test] [--check] [--watch]
         [--coverage [--coverage-threshold PERCENT]] [--jobs N]
         [--diff OLD NEW] [--scrape-examples WORKSPACE]
         [--strict] [--mutate-contracts [--real-exec]]
         [--output-dir DIR]
```

**Аргументы:**

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — (обязателен кроме `--json-schema`) | `.nv` файл или директория |
| `--format` | `markdown` | `markdown`, `json` (D107 schema), `html` |
| `--json-schema` | off | Вывести встроенную JSON Schema 2020-12 и выйти |
| `--include-private` | off | Включить неэкспортируемые элементы |
| `--test` | off | Запустить doc-tests (Plan 45 Ф.7) |
| `--check` | off | Проверить без рендера (битые ссылки, отсутствующие сводки) |
| `--watch` | off | Повторный рендер по опросу mtime (500 мс); Ctrl-C для выхода |
| `--coverage` | off | Метрики покрытия (% элементов со сводкой) |
| `--coverage-threshold N` | — | CI-проверка: код выхода `1`, если coverage% < N |
| `--jobs N` | `0` (= num_cpus) | Параллельных задач разбора для рабочего пространства |
| `--diff OLD NEW` | — | Сравнить два JSON-вывода (определение semver-изменений) |
| `--scrape-examples WORKSPACE` | — | Привязать 3 самых частых примера использования к каждой функции |
| `--strict` | off | Предупреждения → ошибки (CI) |
| `--mutate-contracts` | off | Мутационное тестирование для контрактов (уникальная фича Nova) |
| `--real-exec` | off | Реально исполнять мутантов (требует `--mutate-contracts`) |
| `--output-dir DIR` | — | Многостраничный HTML; только с `--format html` |

**Exit-коды для `--diff OLD NEW`:**

| Код | Значение |
|---|---|
| `0` | Нет ломающих изменений |
| `1` | Мажорное изменение (ломающее) |
| `2` | Минорное изменение (аддитивное) |
| `3` | Патч-изменение (косметическое) |

**Mutation testing (`--mutate-contracts`):**

Генерирует мутанты для каждой функции с контрактами:
- `>` ↔ `>=`, `<` ↔ `<=`
- `==` ↔ `!=`
- Дроп `requires`/`ensures`

По умолчанию — текстовая эвристика (~1 мс/мутант). С `--real-exec` —
запускает мутированные doc-tests через test_runner (~100 мс/мутант,
гарантия реального срабатывания).

**Поддерживаемые форматы документации в `///`** см.
[Plan 45](../plans/45-nova-doc.md) (D107).

---

### `nova doc-query`

DSL-запросы к JSON-выводу `nova doc --format json` (Plan 45 Ф.32.1).
Фундамент для MCP-сервера ([`nova doc-mcp`](#nova-doc-mcp)).

```
nova doc-query JSON_FILE [QUERY]
```

**Синтаксис query:** `key=value,key=value,...`

| Ключ | Значения |
|---|---|
| `kind` | `fn`, `type`, `effect`, `protocol`, `module`, ... |
| `name` | substring |
| `module` | точный путь модуля |
| `module-prefix` | префикс пути |
| `capability` | capability-name |
| `effect` | effect-name |
| `has-contracts` | `true`, `false` |
| `verified` | `true`, `false` |
| `stability` | `stable`, `unstable`, `experimental` |
| `deprecated` | `true`, `false` |

**Примеры:**

```bash
nova doc . --format json > out.json
nova doc-query out.json "kind=fn,capability=pure"
nova doc-query out.json "name=add,has-contracts=true"
nova doc-query out.json "module-prefix=std,effect=Fs"
```

Пустой запрос → весь файл как есть.

---

### `nova doc-mcp`

MCP-сервер (Model Context Protocol) — JSON-RPC через stdio или HTTP
(Plan 45 Ф.32.3 / Ф.34.1). Совместим с MCP-клиентами (Claude Code,
MCP Inspector).

```
nova doc-mcp FILE [--port PORT]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | `.nv`-исходник или заранее сгенерированный `.json` |
| `--port PORT` | — (stdio) | HTTP-режим на `127.0.0.1:PORT`, POST `/mcp` |

**Инструменты (экспортируются через `tools/list`):**

- `query_items(query)` — поиск через DSL ([`nova doc-query`](#nova-doc-query))
- `list_modules()` — список путей модулей
- `get_item(item_id)` — полный JSON одного элемента

**Протокол:** MCP-клиент шлёт `initialize` → `tools/list` → `tools/call`.

---

### `nova contracts`

Инспекция и верификация контрактов (Plan 33 / D24). Вывод — JSON
(AI-friendly schema, см. `docs/contracts-diag-schema.json`).

```
nova contracts <SUBCOMMAND>
```

#### `nova contracts list`

Список всех контрактов в файле.

```
nova contracts list FILE
```

#### `nova contracts verify`

SMT-верификация контрактов. Вывод — JSON.

```
nova contracts verify FILE [--backend BACKEND]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | `.nv` файл |
| `--backend BACKEND` | env `NOVA_SMT_BACKEND` | Переопределяет SMT-бэкенд (`trivial`, `z3`) |

**Z3-бэкенд:** требует build с `--features z3-backend`. См.
[Plan 33.1](../plans/33.1-contracts-core.md).

#### `nova contracts suggest`

Предложения для контрактов с помощью AI (стабы).

```
nova contracts suggest FILE FN_NAME
```

#### `nova contracts counterexample`

Контрпример для падающего контракта.

```
nova contracts counterexample FILE FN_NAME [--contract-id N]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FN_NAME` | — | Имя функции |
| `--contract-id N` | `0` | Индекс контракта (0-based) |

---

### `nova bench`

Инфраструктура бенчмарков (Plan 57 — `MVP+A+B+C+D+E+F+G+H` закрыты).
Лучше Criterion (Rust) / `testing.B`+benchstat (Go) / tinybench (TS)
по ряду параметров. См. [docs/dev/bench-conventions.md](../dev/bench-conventions.md).

```
nova bench <SUBCOMMAND>
```

**Подкоманды:** [`run`](#nova-bench-run), [`diff`](#nova-bench-diff),
[`gate`](#nova-bench-gate), [`calibrate`](#nova-bench-calibrate),
[`cpu-instr-check`](#nova-bench-cpu-instr-check),
[`membw-check`](#nova-bench-membw-check),
[`hyperfine`](#nova-bench-hyperfine), [`callgrind`](#nova-bench-callgrind),
[`callgrind-check`](#nova-bench-callgrind-check),
[`runner-branch`](#nova-bench-runner-branch),
[`history-anomalies`](#nova-bench-history-anomalies),
[`remote`](#nova-bench-remote), [`corpus`](#nova-bench-corpus),
[`history-add`](#nova-bench-history-add), [`history-list`](#nova-bench-history-list),
[`history-squash`](#nova-bench-history-squash),
[`dashboard`](#nova-bench-dashboard).

#### `nova bench run`

Запустить `bench "..." { measure { ... } }` декларации.

```
nova bench run FILE [--filter PATTERN] [--samples N] [--warmup-ms MS]
                    [--time-budget SECS] [--gc boehm|malloc]
                    [--mode release|dev] [--toolchain auto|clang|msvc|gcc]
                    [--vcvars PATH] [--clang PATH]
                    [--compile-timeout SECS] [--run-timeout SECS]
                    [--keep-artifacts] [--mono-depth N]
                    [--out PATH] [--out-csv PATH] [--out-md PATH]
                    [--out-criterion DIR] [--profile MODE OUT]
                    [--histogram]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | `.nv` файл с `bench "..."` блоками |
| `--filter PATTERN` | — | Части имён бенчмарков через запятую |
| `--samples N` | `100` | Переопределяет число замеров |
| `--warmup-ms` | `500` | Длительность прогрева в мс |
| `--time-budget` | `10` | Бюджет на каждый bench в секундах |
| `--gc` | `boehm` | См. [`nova test`](#nova-test) |
| `--mode` | `release` | `release` (рекомендуется) или `dev` |
| `--toolchain` | `auto` | См. [`nova build`](#nova-build) |
| `--compile-timeout` | `120` | Таймаут компиляции |
| `--run-timeout` | `600` | Таймаут запуска bench-процесса |
| `--out PATH` | — | Записать JSON v1 |
| `--out-csv PATH` | — | Записать CSV |
| `--out-md PATH` | — | Markdown (для комментария в PR) |
| `--out-criterion DIR` | — | JSON-layout, совместимый с Criterion |
| `--profile MODE OUT` | — | профиль `cpu`/`heap`/`gc`, требует `samply` для cpu |
| `--histogram` | off | ASCII-гистограмма на каждый bench |

**Форматы вывода:**

- `--out` (JSON v1): полная схема с метаданными (git SHA, toolchain, модель CPU)
- `--out-criterion`: `<dir>/<safe-name>/new/{estimates,sample,benchmark}.json`,
  совместимо с `cargo-criterion --message-format=criterion`
- `--out-md`: markdown-таблица для PR
- `--histogram`: 40 корзин, Unicode-блоки, медиана и границы Тьюки

**Profile-режимы:**

- `cpu` — заворачивает в `samply` (нужен `cargo install samply`)
- `heap` — `NOVA_BENCH_HEAP_SAMPLE_MS=10`
- `gc` — `NOVA_BENCH_GC_TRACE=1`

#### `nova bench diff`

Сравнение двух bench-результатов. t-критерий Уэлча, геометрическое
среднее (geomean delta), проверка воспроизводимости.

```
nova bench diff BASELINE NEW [--format terminal|markdown|json]
                              [--explain [--ai-config PATH] [--ai-max-tokens N]
                                         [--ai-dry-run]]
                              [--baseline-sha SHA] [--new-sha SHA]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `BASELINE`, `NEW` | — | JSON-файлы (`nova bench run --out`) |
| `--format` | `terminal` | `terminal`, `markdown`, `json` |
| `--explain` | off | AI-интерпретация регрессий (Plan 57.F.2, по желанию) |
| `--ai-config PATH` | `~/.nova-ai.toml` | Путь к конфигу AI |
| `--ai-max-tokens` | `4000` | Переопределяет максимум токенов |
| `--ai-dry-run` | off | Печатает тело запроса без вызова API |
| `--baseline-sha`, `--new-sha` | auto из JSON | Git SHA для контекста |

`--explain` использует `system curl` (без RustCrypto-стека) и
требует `NOVA_AI_API_KEY` или конфигурацию.

#### `nova bench gate`

CI-проверка: применяет пороги из `bench.toml`. Код выхода `0` = проход,
`1` = регрессия.

```
nova bench gate BASELINE NEW [--config PATH] [--noise PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--config` | `./bench.toml` | Путь к bench.toml |
| `--noise` | `./.nova-bench-noise.json` если есть | Автокалибруемый уровень шума (см. `calibrate`) |

#### `nova bench calibrate`

Автокалибровка уровня шума из ≥2 повторных прогонов того же
baseline (Plan 57.A.3).

```
nova bench calibrate RUNS... [--out PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `RUNS...` | — | ≥2 JSON-результата одного и того же source |
| `--out` | `.nova-bench-noise.json` | Куда записать уровень шума |

Файл привязан к машине; в git добавлять не нужно.

#### `nova bench cpu-instr-check`

Диагностика доступности счётчика инструкций CPU (Plan 57.B.4).

```
nova bench cpu-instr-check
```

Linux: проверяет `perf_event_open` + измеряет известный цикл. Прочие ОС:
печатает заглушку.

#### `nova bench membw-check`

Диагностика измерения пропускной способности памяти (Plan 57.F.3).

```
nova bench membw-check
```

Linux: опрашивает `/sys/devices/uncore_imc_*` + счётчик промахов LLC.
Прочие ОС: заглушка.

#### `nova bench hyperfine`

Замер времени по типу Hyperfine для нескольких бинарников — измерение
по настенным часам произвольных команд (Plan 57.H.2). Вывод совместим с
`nova bench diff`.

```
nova bench hyperfine SPECS... [--warmup N] [--samples N]
                              [--timeout SECS] [--workdir PATH] [--out PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `SPECS...` | ≥1 | `"name=binary args..."` или просто `"binary args..."` |
| `--warmup` | `3` | Прогревочные запуски (отбрасываются) |
| `--samples` | `10` | Замеряемые запуски |
| `--timeout` | `300` | Таймаут на команду |
| `--workdir PATH` | — | CWD для команд |
| `--out PATH` | stdout | JSON-вывод |

**Пример:**
```bash
nova bench hyperfine \
  "old=./nova-old build large.nv" \
  "new=./nova-new build large.nv" \
  --samples 10 --warmup 2 --out result.json
```

#### `nova bench callgrind`

Запуск под Valgrind Callgrind — детерминированный подсчёт инструкций
CPU (Plan 57.H.3). Кроссплатформенный запасной путь к
`perf_event_open` (только Linux).
Работает на macOS + Linux при наличии `valgrind`.

```
nova bench callgrind BINARY [ARGS...] [--cache-sim] [--workdir PATH] [--out PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `BINARY` | — | Путь к исполняемому файлу |
| `ARGS...` | — | Аргументы исполняемого файла |
| `--cache-sim` | off | Счётчики промахов I1/D1/LL (медленнее) |
| `--workdir PATH` | — | CWD для команды |
| `--out PATH` | — | JSON `CallgrindResult` |

#### `nova bench callgrind-check`

Проверка наличия и версии valgrind.

```
nova bench callgrind-check
```

#### `nova bench runner-branch`

Печатает рекомендованное имя ветки истории на основе переменной окружения
`NOVA_BENCH_RUNNER_ID` (Plan 57.D.4 — CI-матрица из нескольких раннеров).

```
nova bench runner-branch
```

Возвращает `bench-history`, если переменная окружения не задана, иначе
`bench-history-<id>`.

#### `nova bench history-anomalies`

Обнаружение точек изменения (changepoints) в рядах медианных значений
за историю через алгоритм PELT (Plan 57.E.5). Идентифицирует режимы с
отклонением ≥5%.

```
nova bench history-anomalies [--branch BRANCH] [--format text|json]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--branch` | `auto` (с учётом NOVA_BENCH_RUNNER_ID) | Ветка истории |
| `--format` | `text` | `text` или `json` |

#### `nova bench remote`

Распределённая координация бенчмарков по SSH (Plan 57.F.1).

```
nova bench remote <SUBCOMMAND>
```

##### `nova bench remote list`

Список remotes из `~/.nova-bench-remotes.toml`.

```
nova bench remote list [--config PATH]
```

`--config` переопределяется через env `NOVA_BENCH_REMOTES`.

##### `nova bench remote ping`

SSH-проверка доступности одного remote.

```
nova bench remote ping NAME [--config PATH]
```

##### `nova bench remote run`

Параллельный bench на N remotes; сбор результатов.

```
nova bench remote run BENCH [--remotes LIST] [--gather-into DIR] [--sha SHA] [--config PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `BENCH` | — | Путь к `.nv` файлу (относительно корня репозитория на remote) |
| `--remotes` | `all` | Имена через запятую или `all` |
| `--gather-into` | `remote-results` | Куда складывать JSON от каждого remote |
| `--sha SHA` | — | Опциональный git SHA для checkout перед bench |

#### `nova bench corpus`

Измерение времени компиляции по проходам для файла(ов) корпуса —
Plan 57.C.8. Заворачивает `nova build` с `NOVA_PERF_TIMER=1`, парсит
`__PERF__` маркеры.

```
nova bench corpus PATH [--json] [--html PATH] [--echarts-url URL]
                       [--mode release|dev] [--toolchain auto|clang|msvc]
                       [--gc boehm|malloc]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `PATH` | — | `.nv` файл или директория |
| `--json` | off | JSON-вывод (вместо таблицы) |
| `--html PATH` | — | HTML compiler-perf dashboard (Plan 57.D.5) |
| `--echarts-url` | `https://cdn.jsdelivr.net/...` | Свой URL echarts (offline) |
| `--mode` | `release` | |
| `--toolchain` | `auto` | |
| `--gc` | `boehm` | |

#### `nova bench history-add`

Дописать JSON результата в orphan-ветку истории (Plan 57.A.1).

```
nova bench history-add RESULT [--branch BRANCH] [--push] [--remote NAME] [--dry-run]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `RESULT` | — | JSON из `nova bench run --out` |
| `--branch` | `auto` | Orphan-ветка (`bench-history` по умолчанию) |
| `--push` | off | Отправить (push) после коммита |
| `--remote` | `origin` | Имя remote при `--push` |
| `--dry-run` | off | Показать, что было бы, без коммита |

#### `nova bench history-list`

Список записей в ветке истории (сначала новые).

```
nova bench history-list [--branch BRANCH]
```

#### `nova bench history-squash`

Сжатие старых записей по политике хранения (Plan 57.C.6 — рекомендуется
ежегодное сжатие).

```
nova bench history-squash --before-date YYYY-MM-DD [--branch BRANCH]
                          [--push] [--remote NAME] [--dry-run]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--before-date` | — (обязательный) | Сжать всё старше этой даты UTC |
| `--branch` | `auto` | |
| `--push` | off | |
| `--remote` | `origin` | |
| `--dry-run` | off | Показать что было бы удалено |

#### `nova bench dashboard`

Статический HTML-дашборд из истории (Plan 57.A.2).

```
nova bench dashboard [--history-branch BRANCH] [--out DIR] [--max-entries N] [--echarts-url URL]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--history-branch` | `auto` | Ветка истории |
| `--out` | `dashboard` | Каталог вывода |
| `--max-entries` | `200` | Максимум записей (сначала новые) |
| `--echarts-url` | jsdelivr URL | Свой URL echarts (offline = локально) |

Генерирует `index.html` + `bench-<safe>.html` на каждый bench + `data.json`.

> Семейство `nova bench` также открывает диагностические подкоманды
> `field-cache` (реальное измерение по настенным часам влияния field-cache из
> Plan 123), `cpu-instr-check`, `membw-check` и `callgrind-check`.
> Флаги — через `nova bench <sub> --help`.

---

### `nova consume-analyze`

Анализатор покрытия consume-типов ([Plan 100.8](../plans/100.8-performance-ide-tooling.md) / D7).
Сканирует файл или директорию, собирает все consume-типизированные
биндинги и сообщает, сколько из них покрыто через consume-методы
(`Cleanup.@cleanup`, D188) или `defer`. Полезно как CI-проверка гигиены.

```
nova consume-analyze PATH [--format human|json] [--fail-on-uncovered]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `PATH` | — | `.nv` файл или директория для анализа |
| `--format` | `human` | `human` или `json` |
| `--fail-on-uncovered` | off | Ненулевой код выхода при наличии непокрытого consume-биндинга (CI-проверка) |

**Коды выхода:**

| Код | Значение |
|---|---|
| `0` | Все consume-биндинги покрыты |
| `1` | Найдены непокрытые биндинги |
| `2` | Ошибка использования |

---

## Переменные окружения

| Var | Используется в | Эффект |
|---|---|---|
| `NOVA_CODEGEN` | (зарезервировано) | Переопределяет путь к бинарнику `nova-codegen` |
| `NOVA_MONO_DEPTH` | `build`, `test`, `test-build`, `bench` | Лимит мономорфизационных инстанциаций (по умолчанию 500) |
| `NOVA_REACH_DCE` | `build`, `test`, `test-build` | Reachability-codegen DCE ([Plan 159](../plans/159-reachability-codegen.md), [D283](decisions/09-tooling.md#d283)). Не задана / `≠0` → **ON** (по умолчанию): в C эмитится только достижимое от `main`. `=0` → **OFF**: байт-идентичное до-159 поведение (эмитить всё) — запасной вариант для диагностики чрезмерного вырезания |
| `NOVA_HOME` | `add`, `build` (git-deps) | Корень кэша git-зависимостей; по умолчанию `~/.nova` (кэш в `<NOVA_HOME>/git`, глобальный конфиг прокси в `<NOVA_HOME>/config.toml`) |
| `NOVA_OFFLINE` | `add`, `build` (git-deps) | `=1` → запрет сети (clone/fetch); сборка только из готового кэша |
| `NOVA_PKG_PROXY` | `add`, `build` (git-deps) | HTTP(S)-прокси для скачивания пакетов (План 233 §1). Слоями, первый существующий выигрывает: (1) env `NOVA_PKG_PROXY`, либо стандартные `HTTPS_PROXY`/`HTTP_PROXY` (git уважает их сам); (2) `[net] proxy = "..."` в НЕкоммитимом `nova.override.toml` рядом с `nova.toml`; (3) `[net] proxy = "..."` в глобальном `~/.nova/config.toml` (либо `<NOVA_HOME>/config.toml`). В коммитимом `nova.toml` НЕ поддержан — прокси это свойство машины/CI, не пакета |
| `NOVA_SMT_BACKEND` | `contracts` | SMT-бэкенд (`trivial`, `z3`) |
| `NOVA_PERF_TIMER` | `bench corpus` (auto-set) | Включает `__PERF__` маркеры в компиляторе |
| `NOVA_PERF_TIMER_AGGREGATE` | `bench corpus` | Агрегирует `__PERF__` по проходам |
| `NOVA_BENCH_RUNNER_ID` | `bench history-*`, `runner-branch` | CI-матрица из нескольких раннеров; используется в имени ветки |
| `NOVA_BENCH_REMOTES` | `bench remote` | Переопределяет путь к `.nova-bench-remotes.toml` |
| `NOVA_BENCH_FILTER` | `bench run` (auto-set) | Пробрасывается в bench-процесс |
| `NOVA_BENCH_SAMPLES` | `bench run` (auto-set) | Переопределяет число замеров |
| `NOVA_BENCH_WARMUP_NS` | `bench run` (auto-set) | Прогрев в наносекундах |
| `NOVA_BENCH_TIME_BUDGET_NS` | `bench run` (auto-set) | Бюджет времени в наносекундах |
| `NOVA_BENCH_HEAP_SAMPLE_MS` | `bench run --profile heap` | Интервал замеров в мс |
| `NOVA_BENCH_GC_TRACE` | `bench run --profile gc` | Включает трассировку GC |
| `NOVA_AI_PROVIDER` | `bench diff --explain` | AI-провайдер (anthropic, openai, ...) |
| `NOVA_AI_MODEL` | `bench diff --explain` | Переопределяет модель |
| `NOVA_AI_API_KEY` | `bench diff --explain` | API-ключ (или `~/.nova-ai.toml`) |
| `NOVA_C_COMPILER` | `bench repro` | Реальный путь к компилятору (фиксируется в метаданных) |
| `NOVA_SHA` | `bench repro` (compile-time `option_env!`) | Git SHA `nova` бинарника |
| `NO_COLOR` | global | Отключить ANSI цвета |
| `CLICOLOR` | global | `=0` → отключить |
| `CLICOLOR_FORCE` | global | `=1` → принудительно включить |
| `CI` | global | `=true` → отключить цвета |
| `TERM` | global | `=dumb` → отключить цвета |
| `TEMP` | Windows | Временная директория для артефактов `build`/`test` |
| `TMPDIR` | Unix | То же |

---

## Migration-бинарники

Отдельные разовые инструменты в `nova-cli/src/bin/`. Сохраняются
в репозитории как справочник для будущих планов атомарного API-rename.

### `migrate_plan60`

Лексерная миграция size-аксессоров в стиле полей в форму методов
(D117 / [Plan 60](../plans/60-len-access-uniformity.md)):

```
expr.len      → expr.len()
expr.is_empty → expr.is_empty()
expr.byte_len → expr.byte_len()
expr.cap      → expr.capacity()
expr.capacity → expr.capacity()
```

**Условия пропуска:** предыдущий значимый токен == `=`
(присваивание значения метода: `let f = arr.len`).

```
migrate_plan60 [--apply] [--dry-run] [--md] [--paths DIR...]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--dry-run` | (по умолчанию) | Только показать diff |
| `--apply` | off | Реально записать |
| `--md` | off | Включить `.md` файлы (переписывание внутри ` ```nova ` / ` ```nv ` блоков) |
| `--paths DIR...` | `std/`, `nova_tests/`, `examples/` | Список директорий |

Переписывание на уровне токенов — комментарии / пробелы / форматирование
сохраняются 1:1.

### `migrate_plan65`

Лексерная миграция `Time.after(<lit>)` →
`ChanReader.close_after(Duration.from_*(<lit>))` ([Plan 65](../plans/65-chanreader-close-after.md)
AD11):

```
Time.after(<INT>)    → ChanReader.close_after(Duration.from_millis(<INT>))
Time.after(<FLOAT>)  → ChanReader.close_after(Duration.from_secs_f64(<FLOAT>))
Time.after(<expr>)   → left as-is + // MIGRATE_MANUAL: Plan 65 — non-literal arg
```

```
migrate_plan65 [--apply] [--dry-run] [--md] [--paths DIR...]
```

**Exit codes (специальный набор):**

| Код | Значение |
|---|---|
| `0` | Изменений не требуется (idempotent) |
| `1` | Эмитированы ручные маркеры — CI-проверка провалена |
| `2` | Изменения применены (или были бы применены в dry-run) |

С учётом токенов через `nova_codegen::lexer` — пропускает строки и
комментарии естественным образом.

---

## Связанные документы

- [`spec/`](../../spec/) — спецификация языка
- [`spec/decisions/09-tooling.md`](../../spec/decisions/09-tooling.md) —
  D-блоки про тулинг (D89, D107, D121, ...)
- [`docs/dev/test-conventions.md`](../dev/test-conventions.md) — EXPECT-маркеры,
  директивы тестов
- [`docs/dev/bench-conventions.md`](../dev/bench-conventions.md) — конвенция для
  bench-файлов
- [`docs/plans/28-nova-cli.md`](../plans/28-nova-cli.md) — план каркаса CLI
- [`docs/plans/36-cli-production-hardening.md`](../plans/36-cli-production-hardening.md)
  — коды выхода, `--color`, параллельный обход
- [`docs/plans/45-nova-doc.md`](../plans/45-nova-doc.md) — `nova doc` / `doc-query` / `doc-mcp`
- [`docs/plans/57-perf-benchmark-infrastructure.md`](../plans/57-perf-benchmark-infrastructure.md)
  — `nova bench` family
- [`docs/plans/33.3-contracts-advanced.md`](../plans/33.3-contracts-advanced.md)
  — `nova contracts`
