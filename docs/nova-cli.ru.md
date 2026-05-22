# Nova CLI

[English](nova-cli.md) | **Русский**

`nova` — единая точка входа в инструментарий языка Nova. Заменяет
`run_tests.ps1` / `run_tests.sh` / `regen_runtime.ps1` (см. [Plan 28](plans/28-nova-cli.md)).

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
  - [`nova check`](#nova-check) — type-check
  - [`nova run`](#nova-run) — интерпретатор
  - [`nova add`](#nova-add) — добавить зависимость
  - [`nova update`](#nova-update) — пере-резолвить git-пины
  - [`nova build`](#nova-build) — компиляция в native
  - [`nova test`](#nova-test) — запуск тестов
  - [`nova test-build`](#nova-test-build) — единичный тест-build
  - [`nova regen-runtime`](#nova-regen-runtime) — регенерация runtime stubs
  - [`nova doc`](#nova-doc) — документация (Plan 45)
  - [`nova doc-query`](#nova-doc-query) — DSL-запросы к JSON-выводу
  - [`nova doc-mcp`](#nova-doc-mcp) — MCP-сервер
  - [`nova contracts`](#nova-contracts) — инспекция контрактов (Plan 33.3)
  - [`nova bench`](#nova-bench) — бенчмарк-инфраструктура (Plan 57)
- [Переменные окружения](#переменные-окружения)
- [Migration-бинарники](#migration-бинарники)
- [Связанные документы](#связанные-документы)

---

## Quickstart

```bash
# Внутри Nova-проекта (рядом есть nova.toml):
nova check                       # type-check всего workspace
nova check src/                  # рекурсивно по директории
nova check src/lib.nv            # одиночный файл

nova run hello.nv                # запустить через интерпретатор
nova build app.nv -o app         # скомпилировать в native binary
nova test                        # запустить все nova_tests/
nova test --filter basics        # подмножество по подстроке

nova doc src/lib.nv              # markdown в stdout
nova doc src/ --format json      # JSON-схема D107
nova doc src/ --check --strict   # CI-валидация документации

nova bench run bench.nv          # запустить бенчмарки
nova contracts verify foo.nv     # SMT-верификация контрактов
```

---

## Установка и сборка

`nova-cli` живёт в `nova-cli/` рядом с `compiler-codegen/`. Workspace
не используется (см. [Plan 28](plans/28-nova-cli.md) — оба crate
самостоятельные).

```bash
# Debug-сборка (default, opt-level=0)
cargo build --manifest-path nova-cli/Cargo.toml

# Release (opt-level=2, LTO thin)
cargo build --release --manifest-path nova-cli/Cargo.toml

# С Z3-бэкендом для контрактов (Plan 33.1)
cargo build --release --manifest-path nova-cli/Cargo.toml --features z3-backend
```

Получаешь:
- `nova-cli/target/{debug,release}/nova[.exe]`
- `nova-cli/target/{debug,release}/migrate_plan60[.exe]`
- `nova-cli/target/{debug,release}/migrate_plan65[.exe]`

`nova` имеет path-зависимость на `nova_codegen` (`../compiler-codegen`)
— rebuild compiler автоматически перекомпилирует CLI.

---

## Глобальные флаги

Применяются ко всем субкомандам:

| Флаг | Значения | Описание |
|---|---|---|
| `--color` | `auto` (default), `always`, `never` | Управление ANSI-цветами. См. [Plan 36](plans/36-cli-production-hardening.md) R10. |

**Авто-детект цвета** (priority high → low):

1. CLI `--color always|never` — принудительно
2. `CLICOLOR_FORCE=1` → always
3. `NO_COLOR` (любое значение) → never ([no-color.org](https://no-color.org))
4. `CLICOLOR=0` → never
5. `CI=true` → never
6. `TERM=dumb` → never
7. По умолчанию — включено

---

## Коды выхода

Cargo-конвенция ([Plan 36](plans/36-cli-production-hardening.md) R7):

| Код | Значение |
|---|---|
| `0` | Успех |
| `1` | Диагностическая ошибка (тип-чек fail, тест fail, contract violation, etc.) |
| `2` | Usage error (неверный флаг, файл не найден, не `.nv`, нет `nova.toml`) |
| `101` | Internal panic (через `std::panic::set_hook` для cross-platform consistency) |

`nova doc --diff` дополнительно использует **3** = patch-level breaking
change (см. [`nova doc`](#nova-doc)).

---

## Поиск корня проекта

Большинство команд ищут `nova.toml` снизу вверх от CWD. Логика
вынесена в `nova_codegen::test_runner::find_repo_root_from`:

1. Идём от CWD вверх до filesystem root
2. На каждом уровне читаем `nova.toml` если есть
3. Если в нём есть `[workspace]` — это и есть корень (workspace
   root), останавливаемся
4. Иначе запоминаем последний найденный `nova.toml` и идём дальше
5. Если найден корень с `[workspace]` — возвращаем его, иначе —
   самый верхний `nova.toml`

Это **workspace-aware** поведение (D78 AD6, [Plan 35](plans/35-cross-file-resolve.md))
— защищает от ситуации «вложенный `nova_tests/nova.toml` затмил
основной».

Если `nova.toml` не найден — exit `2`:
```
error: nova.toml not found — are you inside a Nova project?
```

Под workspace root резолвятся:
- `<root>/nova_tests/` — корпус тестов
- `<root>/std/` — стандартная библиотека
- `<root>/compiler-codegen/` — include-пути C-runtime
- `<root>/compiler-codegen/nova_rt/` — runtime sources (libuv, GC)
- `<root>/target/last-test-results.json` — кеш `--rerun-failed`

---

## Команды

### `nova check`

Type-check одного или нескольких `.nv` файлов / директорий. Plan 36
MVP — заменяет `nova-codegen check`.

```
nova check [PATHS...] [--jobs N] [-q|-v] [--list] [--format human|short]
           [--include-runtime] [--skip PATTERN]...
```

**Позиционные аргументы:**

- `PATHS` — список файлов или директорий. Если пусто — workspace root
  (рекурсивно). Файл должен иметь расширение `.nv`, иначе exit `2`.

**Флаги:**

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--jobs N` | `0` (= num_cpus) | Параллельных воркеров |
| `-q`, `--quiet` | off | Только FAIL-строки и summary |
| `-v`, `--verbose` | off | Дополнительная информация (timing) |
| `--list` | off | Показать список файлов, не проверяя |
| `--format` | `human` | `human` (цветной) или `short` (`file:line:col: msg` для grep) |
| `--include-runtime` | off | Включить `std/runtime/` (auto-gen, по умолчанию пропускается) |
| `--skip PATTERN` | `[]` | Пропустить файлы по substring (repeatable) |

**Hard-coded skip** (всегда исключаются):

- `target/`, `node_modules/`, `vendor/`
- `.git/`, `.hg/`, `.svn/`
- директории, начинающиеся с `_` или `.`
- `std/runtime/` (override через `--include-runtime`)

**Поведение:**

- Дедуп через `canonicalize`
- Сортировка для детерминизма
- Параллельный walk через `thread::scope` + mpsc-channel
- Per-file warnings (`yellow: warning:`) после `ok:`-строки
- Summary: `pass=N fail=N warnings=N (X.YYs)`
- Exit `1` если есть FAIL, `2` если usage-ошибка

`--format short`:
```
src/lib.nv: ok
src/foo.nv:42:5: error: type mismatch
```

`--format human` (default):
```
ok: src/lib.nv
FAIL: src/foo.nv
  src/foo.nv:42:5: type mismatch
```

**JSON / SARIF / JUnit** форматы зарезервированы под sub-plan 36.A,
сейчас не реализованы.

---

### `nova run`

Запустить `.nv`-файл через интерпретатор (без компиляции в C).

```
nova run FILE
```

- `FILE` — путь к `.nv`-файлу с `fn main`
- Под капотом — `nova_codegen::interp::Interpreter`
- Аналогично `nova-codegen run`

---

### `nova add`

Добавить зависимость в `[dependencies]` `nova.toml` текущего пакета и
обновить `nova.lock` ([Plan 03.1](plans/03.1-path-git-dependencies.md)).

```
nova add NAME (--path DIR | --git URL [--tag T | --branch B | --rev R | --version REQ])
```

| Флаг | Описание |
|---|---|
| `NAME` | Имя зависимости — должно совпадать с `[package].name` пакета-зависимости |
| `--path DIR` | Локальная path-зависимость (другой пакет на диске) |
| `--git URL` | Git-зависимость (URL репозитория) |
| `--tag T` | Git-пин: тег (только с `--git`) |
| `--branch B` | Git-пин: ветка (только с `--git`) |
| `--rev R` | Git-пин: commit / rev (только с `--git`) |
| `--version REQ` | Git-пин: semver-диапазон, напр. `^1.2` (только с `--git`, [Plan 03.2](plans/03.2-version-resolution.md)) |

- `--path` и `--git` взаимоисключающие; ровно один обязателен.
- `--tag` / `--branch` / `--rev` / `--version` взаимоисключающие;
  опциональны (без пина — ветка по умолчанию, в lock всё равно пишется
  точный commit).
- `--version` выбирает наибольший подходящий semver-тег репозитория и
  пишет в `nova.lock` и версию, и commit.
- Правит секцию `[dependencies]` (создаёт при отсутствии). Дубль имени
  → exit `2`.
- После правки запускает lock-sync: материализует git-зависимость в
  кэше и пишет резолвнутый commit в `nova.lock`.
- Работает только внутри пакета (`nova.toml` с `[package]`), не на
  голом `[workspace]`-манифесте.

```bash
nova add mathlib --path ../mathlib
nova add gitlib  --git https://example.org/gitlib.nv --tag v1.0.0
nova add libfoo  --git https://example.org/libfoo.nv --version "^1.2"
```

---

### `nova update`

Пере-резолвить git-зависимости и обновить `nova.lock`
([Plan 03.1](plans/03.1-path-git-dependencies.md) /
[03.2](plans/03.2-version-resolution.md)).

```
nova update [NAME] [--precise NAME@VERSION]
```

- `NAME` — конкретная git-зависимость для обновления. Без аргумента —
  все git-зависимости.
- Снимает целевые git-записи из `nova.lock`, затем пере-резолвит:
  branch/tag-пины берут текущий commit, `version`-диапазоны — наибольший
  подходящий тег. Остальные остаются зафиксированными
  (воспроизводимость).
- `--precise NAME@VERSION` — зафиксировать `version`-диапазонную
  git-зависимость на точной версии (напр. `nova update --precise
  libfoo@1.2.0`). Резолвер обязан согласовать её с остальным деревом,
  иначе — конфликт.
- `path`-зависимости пинов не имеют — такой аргумент отвергается с
  пояснением.

---

### `nova build`

Скомпилировать **один** `.nv`-файл в native binary (через C-backend).

```
nova build FILE [-o OUTPUT] [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
           [--vcvars PATH] [--clang PATH] [--timeout SECS] [--keep-artifacts]
           [--mono-depth N]
```

**Только один файл за раз** — `-o` принимает один путь. Для multi-file
проектов используй `import` внутри entry-point.

**Аргументы:**

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | Entry-point `.nv` с `fn main` |
| `-o OUTPUT` | `<name>[.exe]` в CWD | Путь к выходному бинарнику |
| `--mode` | `dev` | `dev` (unoptimized) или `release` (`-O2` + LTO) |
| `--toolchain` | `auto` | `auto` (Clang → MSVC → GCC), `clang`, `msvc`, `gcc` |
| `--vcvars` | auto через vswhere | Путь к `vcvars64.bat` (Windows) |
| `--clang` | auto detect | Путь к `clang.exe` |
| `--timeout` | `120` | Таймаут компиляции в секундах |
| `--keep-artifacts` | off | Не удалять `.c`/`.exe`/`.obj` в tmp |
| `--mono-depth N` | `500` (или `NOVA_MONO_DEPTH`) | Лимит monomorphization-инстанциаций ([Plan 48](plans/48-closures-in-generics.md) Ф.7.6) |

**Tmp-директория:** `$TEMP/nova_tests/build/<path-hash>/` (Windows) или
`$TMPDIR/nova_tests/build/<path-hash>/` (Unix). Hash через
`DefaultHasher` от абсолютного пути файла — обеспечивает
уникальность без crypto-зависимости.

**Pipeline:**

1. parse + typecheck + `infer_effects`
2. `CEmitter::emit_module` → C-код
3. `detect_toolchain()` (с авто-детектом vcvars)
4. `detect_or_build_libuv()` — runtime может зависеть от libuv
5. `compile_c_to_exe(&tc, &build_opts, timeout)`
6. Копирование exe → `-o` или CWD
7. Удаление tmp (если не `--keep-artifacts`)

---

### `nova test`

Запуск тестов из директории или файла. Plan 28 (вместе с
[Plan 26](plans/26-test-runner-hardening.md), [Plan 27](plans/27-gc-switch.md),
[Plan 34](plans/34-stdlib-typecheck-and-compile-fix.md)).

```
nova test [PATH] [--filter SUBSTR] [--jobs N] [--format text|json|tap|junit]
          [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
          [--vcvars PATH] [--clang PATH] [--timeout SECS] [-v|-q]
          [--results-file PATH] [--rerun-failed] [--retries N]
          [--include-stdlib] [--keep-artifacts] [--gc boehm|malloc]
          [--list] [--filter-from PATH] [--shuffle [SEED]]
          [--skip PATTERN]... [--mono-depth N]
```

**Аргументы:**

| Флаг | По умолчанию | Описание |
|---|---|---|
| `PATH` | `<root>/nova_tests/` | Файл или директория с тестами |
| `--filter SUBSTR` | — | Фильтр по display-name (substring) |
| `--jobs N` | `0` (= num_cpus) | Параллельные воркеры |
| `--format` | `text` | `text`, `json`, `tap`, `junit` |
| `--mode` | `dev` | `dev` или `release` |
| `--toolchain` | `auto` | `auto`, `clang`, `msvc`, `gcc` |
| `--vcvars` | auto | Путь к `vcvars64.bat` |
| `--clang` | auto | Путь к `clang.exe` |
| `--timeout` | `60` | Per-test timeout (секунды) |
| `-v`, `--verbose` | off | Вывод проходящих тестов |
| `-q`, `--quiet` | off | Только FAIL + summary |
| `--results-file PATH` | `<root>/target/last-test-results.json` | Куда писать результаты |
| `--rerun-failed` | off | Перезапустить только failed/timed-out из последнего прогона |
| `--retries N` | `0` | Повторов на transient-фейлах (AV-races и т.п.) |
| `--include-stdlib` | off | Включить `std/` |
| `--keep-artifacts` | off | Не удалять `.c`/`.exe`/`.obj` |
| `--gc` | `boehm` | `boehm` (default) или `malloc` (internal only) |
| `--list` | off | Список тестов без запуска |
| `--filter-from PATH` | — | Файл с именами тестов (по одному на строку, exact match) |
| `--shuffle [SEED]` | off | Случайный порядок; опц. seed для воспроизводимости |
| `--skip PATTERN` | `[]` | Пропустить тесты по substring имени или пути (repeatable) |
| `--mono-depth N` | `500` (или env) | Лимит mono-instantiation |

**Форматы вывода:**

- `text` — человекочитаемый, цветной, в stdout
- `json` — массив объектов с полями `name`, `status`, `duration_ms`, `stderr`
- `tap` — Test Anything Protocol v13
- `junit` — JUnit XML (для CI-агрегаторов)

**`--rerun-failed`:** читает `--results-file`, выбирает имена с
`status != "pass"`, фильтрует suite, запускает только их.

**EXPECT-маркеры** в тестовых файлах (см.
[docs/test-conventions.md](test-conventions.md)):
- `// EXPECT: <stdout-line>` — точное совпадение строки
- `// EXPECT_STDERR: <line>` — для stderr
- `// EXPECT_COMPILE_ERROR: <substring>` — должно упасть при компиляции
- `// EXPECT_RUNTIME_ERROR: <substring>` — panic с подстрокой
- `// REQUIRES_SMT_BACKEND` — пропуск если SMT недоступен

---

### `nova test-build`

Build + run **одного** тестового файла. Используется IDE / CI для
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

Эквивалентно `nova test <FILE>`, но без machinery для bulk-runner'а
(одиночный exe, single test-block в файле).

---

### `nova regen-runtime`

Регенерация `std/runtime/*.nv` стабов из compiler-runtime реестра.
Заменяет `regen_runtime.ps1`.

```
nova regen-runtime [--check]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--check` | off | Только сравнить — exit `1` если файлы расходятся с реестром (CI guard) |

Под капотом — `nova_codegen::codegen::runtime_registry::all()` +
render каждого модуля. См. [Plan 13](plans/13-runtime-stdlib-and-autogen.md).

---

### `nova doc`

Production-grade документация (Plan 45 / D107). Markdown / JSON / HTML
+ doc-tests + coverage + mutation testing + watch + workspace-mode.

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
| `--include-private` | off | Включить non-exported items |
| `--test` | off | Запустить doc-tests (Plan 45 Ф.7) |
| `--check` | off | Валидировать без рендера (broken links, missing summaries) |
| `--watch` | off | Re-render по mtime-poll (500ms); Ctrl-C для выхода |
| `--coverage` | off | Метрики coverage (% items with summary) |
| `--coverage-threshold N` | — | CI-gate: exit `1` если coverage% < N |
| `--jobs N` | `0` (= num_cpus) | Параллельных parse-jobs для workspace |
| `--diff OLD NEW` | — | Сравнить два JSON-вывода (semver detection) |
| `--scrape-examples WORKSPACE` | — | Привязать top-3 usage examples к каждой fn |
| `--strict` | off | Warnings → errors (CI) |
| `--mutate-contracts` | off | Mutation testing для contracts (Nova-unique) |
| `--real-exec` | off | Реально исполнять mutants (требует `--mutate-contracts`) |
| `--output-dir DIR` | — | Multi-page HTML; только с `--format html` |

**Exit-коды для `--diff OLD NEW`:**

| Код | Значение |
|---|---|
| `0` | Нет breaking changes |
| `1` | Major change (breaking) |
| `2` | Minor change (additive) |
| `3` | Patch change (cosmetic) |

**Mutation testing (`--mutate-contracts`):**

Генерирует мутанты для каждой функции с контрактами:
- `>` ↔ `>=`, `<` ↔ `<=`
- `==` ↔ `!=`
- Дроп `requires`/`ensures`

Default — text-based heuristic (~1ms/мутант). С `--real-exec` —
запускает мутированные doc-tests через test_runner (~100ms/мутант,
true positive guarantee).

**Поддерживаемые форматы документации в `///`** см.
[Plan 45](plans/45-nova-doc.md) (D107).

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
| `module` | exact module path |
| `module-prefix` | префикс пути |
| `capability` | capability-name |
| `effect` | effect-name |
| `has-contracts` | `true`, `false` |
| `verified` | `true`, `false` |
| `stability` | `stable`, `unstable`, `experimental` |
| `deprecated` | `true`, `false` |

**Примеры:**

```bash
nova doc src/ --format json > out.json
nova doc-query out.json "kind=fn,capability=pure"
nova doc-query out.json "name=add,has-contracts=true"
nova doc-query out.json "module-prefix=std,effect=Fs"
```

Пустой query → весь файл as-is.

---

### `nova doc-mcp`

MCP-сервер (Model Context Protocol) — JSON-RPC over stdio или HTTP
(Plan 45 Ф.32.3 / Ф.34.1). Совместим с MCP-клиентами (Claude Code,
MCP Inspector).

```
nova doc-mcp FILE [--port PORT]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | `.nv`-исходник или pre-generated `.json` |
| `--port PORT` | — (stdio) | HTTP-режим на `127.0.0.1:PORT`, POST `/mcp` |

**Tools (экспортируется через `tools/list`):**

- `query_items(query)` — поиск через DSL ([`nova doc-query`](#nova-doc-query))
- `list_modules()` — список module-путей
- `get_item(item_id)` — полный JSON одного item

**Protocol:** MCP-клиент шлёт `initialize` → `tools/list` → `tools/call`.

---

### `nova contracts`

Инспекция и верификация контрактов (Plan 33 / D24). Output — JSON
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

SMT-верификация контрактов. Output — JSON.

```
nova contracts verify FILE [--backend BACKEND]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | `.nv` файл |
| `--backend BACKEND` | env `NOVA_SMT_BACKEND` | Override SMT-backend (`trivial`, `z3`) |

**Z3-бэкенд:** требует build с `--features z3-backend`. См.
[Plan 33.1](plans/33.1-contracts-core.md).

#### `nova contracts suggest`

AI-assisted предложения для контрактов (стабы).

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

Бенчмарк-инфраструктура (Plan 57 — `MVP+A+B+C+D+E+F+G+H` закрыты).
Лучше Criterion (Rust) / `testing.B`+benchstat (Go) / tinybench (TS)
по ряду параметров. См. [docs/bench-conventions.md](bench-conventions.md).

```
nova bench <SUBCOMMAND>
```

**Субкоманды:** [`run`](#nova-bench-run), [`diff`](#nova-bench-diff),
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
| `--filter PATTERN` | — | Comma-separated bench-name fragments |
| `--samples N` | `100` | Override sample count |
| `--warmup-ms` | `500` | Warmup duration в мс |
| `--time-budget` | `10` | Per-bench бюджет в секундах |
| `--gc` | `boehm` | См. [`nova test`](#nova-test) |
| `--mode` | `release` | `release` (рекомендуется) или `dev` |
| `--toolchain` | `auto` | См. [`nova build`](#nova-build) |
| `--compile-timeout` | `120` | Таймаут компиляции |
| `--run-timeout` | `600` | Таймаут запуска bench-процесса |
| `--out PATH` | — | Записать JSON v1 |
| `--out-csv PATH` | — | Записать CSV |
| `--out-md PATH` | — | Markdown (для PR comment) |
| `--out-criterion DIR` | — | Criterion-compatible JSON layout |
| `--profile MODE OUT` | — | `cpu`/`heap`/`gc` profile, требует `samply` для cpu |
| `--histogram` | off | ASCII-гистограмма на каждый bench |

**Output-форматы:**

- `--out` (JSON v1): полная схема с metadata (git SHA, toolchain, CPU model)
- `--out-criterion`: `<dir>/<safe-name>/new/{estimates,sample,benchmark}.json`,
  совместимо с `cargo-criterion --message-format=criterion`
- `--out-md`: markdown-таблица для PR
- `--histogram`: 40 buckets, Unicode block chars, медиана и Tukey fences

**Profile-режимы:**

- `cpu` — заворачивает в `samply` (нужен `cargo install samply`)
- `heap` — `NOVA_BENCH_HEAP_SAMPLE_MS=10`
- `gc` — `NOVA_BENCH_GC_TRACE=1`

#### `nova bench diff`

Сравнение двух bench-результатов. Welch's t-test, geomean delta,
reproducibility check.

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
| `--explain` | off | AI-интерпретация regressions (Plan 57.F.2, opt-in) |
| `--ai-config PATH` | `~/.nova-ai.toml` | Путь к AI-конфигу |
| `--ai-max-tokens` | `4000` | Override max tokens |
| `--ai-dry-run` | off | Печатает request body без API call |
| `--baseline-sha`, `--new-sha` | auto из JSON | Git SHA для context |

`--explain` использует `system curl` (без RustCrypto-стека) и
требует `NOVA_AI_API_KEY` или конфиг.

#### `nova bench gate`

CI-gate: применяет пороги из `bench.toml`. Exit `0` = pass, `1` = regress.

```
nova bench gate BASELINE NEW [--config PATH] [--noise PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--config` | `./bench.toml` | Путь к bench.toml |
| `--noise` | `./.nova-bench-noise.json` если есть | Auto-calibrated noise-floor (см. `calibrate`) |

#### `nova bench calibrate`

Авто-калибровка noise-floor из ≥2 повторных прогонов того же
baseline (Plan 57.A.3).

```
nova bench calibrate RUNS... [--out PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `RUNS...` | — | ≥2 JSON-результата одного и того же source |
| `--out` | `.nova-bench-noise.json` | Куда записать noise-floor |

Файл machine-specific; в git добавлять не нужно.

#### `nova bench cpu-instr-check`

Диагностика доступности CPU-instruction-counter (Plan 57.B.4).

```
nova bench cpu-instr-check
```

Linux: проверяет `perf_event_open` + measures known loop. Other OS:
печатает stub-сообщение.

#### `nova bench membw-check`

Диагностика memory-bandwidth measurement (Plan 57.F.3).

```
nova bench membw-check
```

Linux: probes `/sys/devices/uncore_imc_*` + LLC-miss perf counter.
Other OS: stub.

#### `nova bench hyperfine`

Hyperfine-style cross-binary timing — wall-clock измерение
произвольных команд (Plan 57.H.2). Output совместим с
`nova bench diff`.

```
nova bench hyperfine SPECS... [--warmup N] [--samples N]
                              [--timeout SECS] [--workdir PATH] [--out PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `SPECS...` | ≥1 | `"name=binary args..."` или просто `"binary args..."` |
| `--warmup` | `3` | Warmup runs (отбрасываются) |
| `--samples` | `10` | Sample runs |
| `--timeout` | `300` | Per-command timeout |
| `--workdir PATH` | — | CWD для команд |
| `--out PATH` | stdout | JSON output |

**Пример:**
```bash
nova bench hyperfine \
  "old=./nova-old build large.nv" \
  "new=./nova-new build large.nv" \
  --samples 10 --warmup 2 --out result.json
```

#### `nova bench callgrind`

Запуск под Valgrind Callgrind — deterministic CPU-instructions count
(Plan 57.H.3). Cross-platform fallback к `perf_event_open` (Linux-only).
Работает на macOS + Linux при наличии `valgrind`.

```
nova bench callgrind BINARY [ARGS...] [--cache-sim] [--workdir PATH] [--out PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `BINARY` | — | Путь к executable |
| `ARGS...` | — | Аргументы executable |
| `--cache-sim` | off | I1/D1/LL miss counts (медленнее) |
| `--workdir PATH` | — | CWD для команды |
| `--out PATH` | — | JSON `CallgrindResult` |

#### `nova bench callgrind-check`

Проверка наличия и версии valgrind.

```
nova bench callgrind-check
```

#### `nova bench runner-branch`

Печатает рекомендованное имя history-branch на основе env
`NOVA_BENCH_RUNNER_ID` (Plan 57.D.4 — multi-runner CI matrix).

```
nova bench runner-branch
```

Возвращает `bench-history` если env не задан, иначе
`bench-history-<id>`.

#### `nova bench history-anomalies`

Детекция changepoints в historical median time-series через PELT
algorithm (Plan 57.E.5). Идентифицирует regimes с ≥5% delta.

```
nova bench history-anomalies [--branch BRANCH] [--format text|json]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--branch` | `auto` (NOVA_BENCH_RUNNER_ID-aware) | History branch |
| `--format` | `text` | `text` или `json` |

#### `nova bench remote`

SSH-distributed bench coordination (Plan 57.F.1).

```
nova bench remote <SUBCOMMAND>
```

##### `nova bench remote list`

Список remotes из `~/.nova-bench-remotes.toml`.

```
nova bench remote list [--config PATH]
```

`--config` override через `NOVA_BENCH_REMOTES` env.

##### `nova bench remote ping`

SSH health-check одного remote.

```
nova bench remote ping NAME [--config PATH]
```

##### `nova bench remote run`

Параллельный bench на N remotes; gathering results.

```
nova bench remote run BENCH [--remotes LIST] [--gather-into DIR] [--sha SHA] [--config PATH]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `BENCH` | — | `.nv` file path (relative to repo root на remote) |
| `--remotes` | `all` | Comma-separated имена или `all` |
| `--gather-into` | `remote-results` | Куда складывать per-remote JSON |
| `--sha SHA` | — | Опц. git SHA для checkout перед bench |

#### `nova bench corpus`

Измерение per-pass compile time для corpus файла(ов) — Plan 57.C.8.
Заворачивает `nova build` с `NOVA_PERF_TIMER=1`, парсит `__PERF__`
маркеры.

```
nova bench corpus PATH [--json] [--html PATH] [--echarts-url URL]
                       [--mode release|dev] [--toolchain auto|clang|msvc]
                       [--gc boehm|malloc]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `PATH` | — | `.nv` файл или директория |
| `--json` | off | JSON output (вместо таблицы) |
| `--html PATH` | — | HTML compiler-perf dashboard (Plan 57.D.5) |
| `--echarts-url` | `https://cdn.jsdelivr.net/...` | Custom echarts URL (offline) |
| `--mode` | `release` | |
| `--toolchain` | `auto` | |
| `--gc` | `boehm` | |

#### `nova bench history-add`

Дописать result JSON в orphan history-branch (Plan 57.A.1).

```
nova bench history-add RESULT [--branch BRANCH] [--push] [--remote NAME] [--dry-run]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `RESULT` | — | JSON из `nova bench run --out` |
| `--branch` | `auto` | Orphan branch (`bench-history` по умолчанию) |
| `--push` | off | Push после commit |
| `--remote` | `origin` | Remote name при `--push` |
| `--dry-run` | off | Показать что было бы, без commit |

#### `nova bench history-list`

Список entries в history-branch (newest first).

```
nova bench history-list [--branch BRANCH]
```

#### `nova bench history-squash`

Squash старых entries по retention policy (Plan 57.C.6 — рекомендуется
yearly squash).

```
nova bench history-squash --before-date YYYY-MM-DD [--branch BRANCH]
                          [--push] [--remote NAME] [--dry-run]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--before-date` | — (обязательный) | Squash всё старше этой даты UTC |
| `--branch` | `auto` | |
| `--push` | off | |
| `--remote` | `origin` | |
| `--dry-run` | off | Показать что было бы удалено |

#### `nova bench dashboard`

Статический HTML dashboard из history (Plan 57.A.2).

```
nova bench dashboard [--history-branch BRANCH] [--out DIR] [--max-entries N] [--echarts-url URL]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--history-branch` | `auto` | History branch |
| `--out` | `dashboard` | Output directory |
| `--max-entries` | `200` | Max entries (newest first) |
| `--echarts-url` | jsdelivr URL | Custom echarts URL (offline = local) |

Генерирует `index.html` + `bench-<safe>.html` per bench + `data.json`.

---

## Переменные окружения

| Var | Используется в | Эффект |
|---|---|---|
| `NOVA_CODEGEN` | (зарезервировано) | Override пути к `nova-codegen` binary |
| `NOVA_MONO_DEPTH` | `build`, `test`, `test-build`, `bench` | Лимит monomorphization-инстанциаций (default 500) |
| `NOVA_HOME` | `add`, `build` (git-deps) | Корень кэша git-зависимостей; default `~/.nova` (кэш в `<NOVA_HOME>/git`) |
| `NOVA_OFFLINE` | `add`, `build` (git-deps) | `=1` → запрет сети (clone/fetch); сборка только из готового кэша |
| `NOVA_SMT_BACKEND` | `contracts` | SMT-backend (`trivial`, `z3`) |
| `NOVA_PERF_TIMER` | `bench corpus` (auto-set) | Включает `__PERF__` маркеры в компиляторе |
| `NOVA_PERF_TIMER_AGGREGATE` | `bench corpus` | Aggregate `__PERF__` по проходам |
| `NOVA_BENCH_RUNNER_ID` | `bench history-*`, `runner-branch` | Multi-runner CI matrix; используется в branch name |
| `NOVA_BENCH_REMOTES` | `bench remote` | Override пути к `.nova-bench-remotes.toml` |
| `NOVA_BENCH_FILTER` | `bench run` (auto-set) | Forwarded в bench-процесс |
| `NOVA_BENCH_SAMPLES` | `bench run` (auto-set) | Override sample count |
| `NOVA_BENCH_WARMUP_NS` | `bench run` (auto-set) | Warmup в наносекундах |
| `NOVA_BENCH_TIME_BUDGET_NS` | `bench run` (auto-set) | Time budget в наносекундах |
| `NOVA_BENCH_HEAP_SAMPLE_MS` | `bench run --profile heap` | Sample interval в мс |
| `NOVA_BENCH_GC_TRACE` | `bench run --profile gc` | Включает GC tracing |
| `NOVA_AI_PROVIDER` | `bench diff --explain` | AI-provider (anthropic, openai, ...) |
| `NOVA_AI_MODEL` | `bench diff --explain` | Модель override |
| `NOVA_AI_API_KEY` | `bench diff --explain` | API key (или `~/.nova-ai.toml`) |
| `NOVA_C_COMPILER` | `bench repro` | Реальный path к компилятору (фиксируется в metadata) |
| `NOVA_SHA` | `bench repro` (compile-time `option_env!`) | Git SHA `nova` бинарника |
| `NO_COLOR` | global | Отключить ANSI цвета |
| `CLICOLOR` | global | `=0` → отключить |
| `CLICOLOR_FORCE` | global | `=1` → принудительно включить |
| `CI` | global | `=true` → отключить цвета |
| `TERM` | global | `=dumb` → отключить цвета |
| `TEMP` | Windows | Tmp-директория для `build`/`test` artifacts |
| `TMPDIR` | Unix | То же |

---

## Migration-бинарники

Отдельные one-shot инструменты в `nova-cli/src/bin/`. Сохраняются
в репозитории как reference для будущих атомарных API-rename планов.

### `migrate_plan60`

Lexer-based миграция field-style size-accessors в method-form
(D117 / [Plan 60](plans/60-len-access-uniformity.md)):

```
expr.len      → expr.len()
expr.is_empty → expr.is_empty()
expr.byte_len → expr.byte_len()
expr.cap      → expr.capacity()
expr.capacity → expr.capacity()
```

**Skip conditions:** предыдущий значимый token == `=`
(method-value assignment: `let f = arr.len`).

```
migrate_plan60 [--apply] [--dry-run] [--md] [--paths DIR...]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--dry-run` | (default) | Только показать diff |
| `--apply` | off | Реально записать |
| `--md` | off | Включить `.md` файлы (rewrite внутри ` ```nova ` / ` ```nv ` блоков) |
| `--paths DIR...` | `std/`, `nova_tests/`, `examples/` | Список директорий |

Token-level rewrite — комментарии / whitespace / formatting сохраняются 1:1.

### `migrate_plan65`

Lexer-based миграция `Time.after(<lit>)` →
`ChanReader.close_after(Duration.from_*(<lit>))` ([Plan 65](plans/65-chanreader-close-after.md)
AD11):

```
Time.after(<INT>)    → ChanReader.close_after(Duration.from_millis(<INT>))
Time.after(<FLOAT>)  → ChanReader.close_after(Duration.from_secs_f64(<FLOAT>))
Time.after(<expr>)   → как есть + // MIGRATE_MANUAL: Plan 65 — non-literal arg
```

```
migrate_plan65 [--apply] [--dry-run] [--md] [--paths DIR...]
```

**Exit codes (специальный набор):**

| Код | Значение |
|---|---|
| `0` | Изменений не требуется (idempotent) |
| `1` | Emitted manual markers — CI gate fails |
| `2` | Изменения применены (или были бы применены в dry-run) |

Token-aware через `nova_codegen::lexer` — пропускает строки и
комментарии естественным образом.

---

## Связанные документы

- [`spec/`](../spec/) — спецификация языка
- [`spec/decisions/09-tooling.md`](../spec/decisions/09-tooling.md) —
  D-блоки про тулинг (D89, D107, D121, ...)
- [`docs/test-conventions.md`](test-conventions.md) — EXPECT-маркеры,
  директивы тестов
- [`docs/bench-conventions.md`](bench-conventions.md) — convention для
  bench-файлов
- [`docs/plans/28-nova-cli.md`](plans/28-nova-cli.md) — план каркаса CLI
- [`docs/plans/36-cli-production-hardening.md`](plans/36-cli-production-hardening.md)
  — exit codes, `--color`, parallel walk
- [`docs/plans/45-nova-doc.md`](plans/45-nova-doc.md) — `nova doc` / `doc-query` / `doc-mcp`
- [`docs/plans/57-perf-benchmark-infrastructure.md`](plans/57-perf-benchmark-infrastructure.md)
  — `nova bench` family
- [`docs/plans/33.3-contracts-advanced.md`](plans/33.3-contracts-advanced.md)
  — `nova contracts`
