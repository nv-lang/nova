# nova-codegen

[English](nova-codegen.md) | **Русский**

`nova-codegen` — внутренний компилятор Nova: парсер + type-checker +
treewalk-интерпретатор + C-бэкенд + cross-file resolver + SMT-верификатор
контрактов.

> **Внутренний компонент.** Для повседневной работы — `nova` CLI
> ([docs/nova-cli.ru.md](nova-cli.ru.md)). `nova-codegen` остаётся
> точкой входа для IDE, CI, прямой отладки codegen, и используется
> `nova-cli` как path-зависимость.

Версия: `0.1.0` (bootstrap). Cargo package `nova-codegen`, crate
`nova_codegen`, binary `nova-codegen`.

---

## Содержание

- [Установка и сборка](#установка-и-сборка)
- [Коды выхода](#коды-выхода)
- [Команды](#команды)
  - [`nova-codegen check`](#nova-codegen-check)
  - [`nova-codegen run`](#nova-codegen-run)
  - [`nova-codegen test-interp`](#nova-codegen-test-interp)
  - [`nova-codegen compile`](#nova-codegen-compile)
  - [`nova-codegen emit-runtime-stubs`](#nova-codegen-emit-runtime-stubs)
  - [`nova-codegen dump-runtime`](#nova-codegen-dump-runtime)
  - [`nova-codegen test-build`](#nova-codegen-test-build)
  - [`nova-codegen test-all`](#nova-codegen-test-all)
- [Переменные окружения](#переменные-окружения)
- [Cargo features](#cargo-features)
- [Library API (`nova_codegen`)](#library-api-nova_codegen)
- [Внутренняя архитектура](#внутренняя-архитектура)
- [Runtime (`nova_rt/`)](#runtime-nova_rt)
- [Связанные документы](#связанные-документы)

---

## Установка и сборка

```bash
# Debug-сборка
cargo build --manifest-path compiler-codegen/Cargo.toml

# Release (для bootstrap opt-level=0, без size-оптимизаций)
cargo build --release --manifest-path compiler-codegen/Cargo.toml

# С Z3-бэкендом для контрактов (Plan 33.1)
cargo build --release --manifest-path compiler-codegen/Cargo.toml --features z3-backend

# Rust-уровневые юнит-тесты компилятора
cargo test --manifest-path compiler-codegen/Cargo.toml
```

Получаешь `compiler-codegen/target/{debug,release}/nova-codegen[.exe]`.

**Главный поток** запускается с **64 MiB** стеком (`std::thread::Builder`
+ `stack_size`) — AST-обходы взаимно рекурсивны (expr ↔ block ↔ stmt),
type-checker / SCC-purity / SMT-encoder требуют глубокого стека на
больших модулях. Default Windows 1 MiB недостаточен.

**Минимум зависимостей:** `clap`, `anyhow`. Bootstrap должен собираться
с пустым lockfile'ом на любом stable Rust 1.85+.

---

## Коды выхода

| Код | Значение |
|---|---|
| `0` | Успех |
| `1` | Ошибка (parse fail, type-check fail, codegen fail, runtime fail, тесты упали) |

В отличие от [`nova`](nova-cli.ru.md), `nova-codegen` использует
**бинарный** exit-режим (0/1), без разделения usage-ошибки и
диагностики.

---

## Команды

### `nova-codegen check`

Type-check файла без запуска.

```
nova-codegen check FILE
```

| Аргумент | Описание |
|---|---|
| `FILE` | Путь к `.nv` файлу |

**Pipeline:**

1. `read_file(path)`
2. `parser::parse(&src)` → AST
3. `check_module_path` — D78 path/module enforcement
4. `types::check_module` → type-check + effect-inference
5. Печатает `ok: {} parsed and checked`

При ошибке — рендерится через `Diagnostic::render` с line:col и
underline'ом исходника.

---

### `nova-codegen run`

Type-check + интерпретировать (вызывается `fn main`).

```
nova-codegen run FILE
```

**Pipeline:**

1. parse + path-check + type-check
2. `types::annotate_map_literals` (Plan 52 Ф.7) — аннотация
   `[k: v]`-литералов inferred K/V
3. `desugar::desugar_module` (Plan 52 Ф.5) — десугаринг map-литералов
   в `with_capacity` + `@insert`
4. `interp::Interpreter::new()` → `load_module` → `run_main`

Treewalk-интерпретатор имеет паритет с codegen-бэкендом — те же
эффекты, handler'ы, structured concurrency, контракты, defer, channels.

---

### `nova-codegen test-interp`

Запустить `test "..." { ... }` блоки в файле через интерпретатор (без C-codegen).

```
nova-codegen test-interp FILE
```

**Pipeline:** parse → path-check → annotate_map_literals → desugar →
`interp::run_tests` → `tests: N passed, N failed`.

Exit `1` если хотя бы один тест fail'нул; печатает имена failed-тестов.

Это интерпретаторный тест-режим (быстро, но без C-pipeline). Для
проверки codegen-pipeline'а используй [`test-build`](#nova-codegen-test-build)
или [`test-all`](#nova-codegen-test-all).

---

### `nova-codegen compile`

Скомпилировать `.nv` в `.c` (без линковки).

```
nova-codegen compile FILE [-o OUTPUT] [--no-annotate-source] [--no-lint]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | `.nv` файл |
| `-o OUTPUT` | `<name>.c` рядом с source | Путь к выходному `.c` |
| `--no-annotate-source` | off (аннотации включены) | Не вставлять `/* SRC: ... */` комментарии — для compact-вывода |
| `--no-lint` | off (lint включён) | Отключить lint-проверки (export-fail-untyped и др.) |

**Pipeline:**

1. parse + path-check + type-check
2. `annotate_map_literals` + `desugar_module`
3. `types::infer_effects` — D28 effect-inference для private fn
4. `lints::lint_module` (если `lint == true`)
5. `CEmitter::new()` + `set_source_for_annotations(src)` (для line:col в
   codegen-ошибках)
6. `set_proven_contracts` (Plan 33.3 Ф.9.9 — selective contract
   stripping, true zero-cost)
7. `emit_module` → C-код + warnings
8. `fs::write(&out_path, &c_code)`

**SRC-аннотации (`/* SRC: ... */`)** — для удобства отладки `.c` —
сопоставляют C-строки с Nova-исходником. По умолчанию включены.

---

### `nova-codegen emit-runtime-stubs`

Регенерация `std/runtime/string.nv` и `std/runtime/math.nv` из
`runtime_registry.rs` (Plan 13).

```
nova-codegen emit-runtime-stubs [--root PATH] [--check]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--root PATH` | `.` (CWD) | Корень репозитория (где `std/runtime/`) |
| `--check` | off | Только сравнить с существующими; exit `1` при diff (для CI/pre-commit) |

**Workflow добавления новой runtime fn** (например `f64.@cbrt`):

1. Добавить `RuntimeFn { ... }` в `src/codegen/runtime_registry.rs`
2. Реализовать в `nova_rt/<module>.c` (или wrapper к libc)
3. Регенерация: `nova regen-runtime` (или `nova-codegen emit-runtime-stubs --root .`)
4. Коммит всех трёх (registry + .c + .nv)

`--check` нормализует line endings (CRLF → LF) перед сравнением.

`std/runtime/string.nv` и `math.nv` — **auto-generated.** Не править
вручную — guard через `nova regen-runtime --check` в CI.

---

### `nova-codegen dump-runtime`

Sanity-print registry runtime-функций (Plan 13).

```
nova-codegen dump-runtime
```

Output:
```
Nova runtime registry: N function(s) total.

=== <module> (N fns) ===
  <receiver> [mut] <.|@><name>(<params>) -> <return-ty>    [c: <c-name>]
  ...
```

- `.` = static fn (`Type.fn`)
- `@` = instance method (`obj.@fn`)

Полезно для аудита расхождений registry vs реальных C-сигнатур в
`nova_rt/`.

---

### `nova-codegen test-build`

Plan 24 — cross-platform per-file test runner: компиляция одного `.nv`
в `.exe` и проверка `EXPECT`-маркеров (D89).

```
nova-codegen test-build FILE [--mode dev|release] [--toolchain auto|clang|msvc|gcc]
                             [--vcvars PATH] [--clang PATH]
                             [--cg-include PATH] [--rt-dir PATH] [--tmp-dir PATH]
                             [--display NAME] [--keep-artifacts]
                             [--timeout SECS] [--gc boehm|malloc]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `FILE` | — | `.nv` test-файл |
| `--mode` | `dev` | `dev` (unoptimized) или `release` |
| `--toolchain` | `auto` | `auto`, `clang`, `msvc`, `gcc` |
| `--vcvars PATH` | auto через vswhere | Путь к `vcvars64.bat` (Windows) |
| `--clang PATH` | auto detect | Путь к `clang.exe` |
| `--cg-include PATH` | `<cwd>/compiler-codegen` | Путь к `compiler-codegen/` (для include `nova_rt/`) |
| `--rt-dir PATH` | `<cwd>/compiler-codegen/nova_rt` | Runtime sources |
| `--tmp-dir PATH` | `$TEMP/nova_tests` или `$TMPDIR/nova_tests` или `/tmp/nova_tests` | Tmp директория для `.c`/`.exe`/`.obj` |
| `--display NAME` | basename файла | Override display-name |
| `--keep-artifacts` | off | Не удалять артефакты |
| `--timeout SECS` | `60` | Per-test timeout (Plan 26 Ф.1) |
| `--gc boehm\|malloc` | `boehm` | GC backend (Plan 27 Ф.4) |

**Pipeline:**

1. `Mode::parse` + `ToolchainPref::parse`
2. `detect_toolchain(&tc_opts)` — Clang приоритетно, MSVC/GCC fallback
3. `detect_or_build_libuv(&rt_dir, &repo_root, vcvars)` — runtime-зависимость
4. `TestBuildOpts { ... }` + `test_runner::run_one(&opts)`
5. Output: `<STATUS:14> <display>  # <detail>`

EXPECT-маркеры (D89):
- `// EXPECT: <line>` — точное совпадение stdout-строки
- `// EXPECT_STDERR: <line>` — для stderr
- `// EXPECT_COMPILE_ERROR: <substring>` — должен fail при компиляции
- `// EXPECT_RUNTIME_ERROR: <substring>` — panic с подстрокой
- `// REQUIRES_SMT_BACKEND` — пропуск если SMT недоступен

---

### `nova-codegen test-all`

Plan 24 — batch test runner: рекурсивный прогон всех `.nv` в
`--tests-dir`. Используется как движок для `nova test`
([docs/nova-cli.ru.md](nova-cli.ru.md)).

```
nova-codegen test-all [--tests-dir PATH] [--stdlib-dir PATH] [--include-stdlib]
                      [--filter SUBSTR] [--mode dev|release]
                      [--toolchain auto|clang|msvc|gcc]
                      [--vcvars PATH] [--clang PATH]
                      [--cg-include PATH] [--rt-dir PATH] [--tmp-dir PATH]
                      [--keep-artifacts] [--timeout SECS] [--jobs N]
                      [--format text|json|tap] [-v|-q]
                      [--results-file PATH] [--rerun-failed]
                      [--retries N] [--gc boehm|malloc]
```

| Флаг | По умолчанию | Описание |
|---|---|---|
| `--tests-dir PATH` | `nova_tests` | Корень тестового корпуса |
| `--stdlib-dir PATH` | `std` | Корень `std/` (если `--include-stdlib`) |
| `--include-stdlib` | off | Включить `std/*` файлы |
| `--filter SUBSTR` | — | Filter по display-name |
| `--mode` | `dev` | См. [`test-build`](#nova-codegen-test-build) |
| `--toolchain` | `auto` | |
| `--vcvars`, `--clang` | auto | |
| `--cg-include`, `--rt-dir` | вычисляются из CWD | |
| `--tmp-dir` | `$TEMP/nova_tests` или эквивалент | |
| `--keep-artifacts` | off | |
| `--timeout` | `60` | Per-test timeout (Plan 26 Ф.1) |
| `--jobs N` | `0` (= num_cpus) | Параллельные worker'ы (Plan 26 Ф.3) |
| `--format` | `text` | `text`, `json`, `tap` (Plan 26 Ф.4) |
| `-v`, `--verbose` | off | Output PASS-тестов (Plan 26 Ф.9) |
| `-q`, `--quiet` | off | Только FAIL + summary (Plan 26 Ф.9) |
| `--results-file PATH` | — | Файл `last-results.json` (Plan 26 Ф.10) |
| `--rerun-failed` | off | Перезапустить только failed/timeout из `--results-file` |
| `--retries N` | `0` | Retry на transient AV/race fail'ах (Plan 26 Ф.12; CI default 2) |
| `--gc boehm\|malloc` | `boehm` | См. [`test-build`](#nova-codegen-test-build) |

**Информационные сообщения** (text-режим) — в stderr (как cargo):
```
Toolchain: clang, mode=Dev, jobs=8, tests-dir=nova_tests
libuv: enabled
```

Per-test events и summary — в stdout (для wrapper-скриптов).

**Ограничения:**

- `cache_dir: None` — Plan 26 Ф.5 (incremental cache) не реализован
  (оставлен крючок в `opts`)
- `list_only: false`, `filter_from: None`, `shuffle_seed: None`,
  `skip: &[]`, `mono_depth: None` — поддерживаются только через
  [`nova test`](nova-cli.ru.md#nova-test) (Plan 26 Ф.13+ / 34 / 48)

---

## Переменные окружения

| Var | Эффект |
|---|---|
| `NOVA_CACHE=0` | Отключить кэши: SMT-кэш контрактов (Plan 33.3 Ф.12) + build-кэш `.c` (Plan 81 Ф.9). Принимаются также `off`/`false` |
| `NOVA_PERF_TIMER=1` | Включить `__PERF__` маркеры в компиляторе (per-pass timing) |
| `NOVA_MONO_DEPTH=N` | Лимит monomorphization-инстанциаций (default 500, [Plan 48](plans/48-closures-in-generics.md) Ф.7.6) |
| `NOVA_DEBUG_MONO=1` | Verbose debug-print mono-instances (диагностика поломок codegen) |
| `NOVA_SMT_BACKEND=trivial\|z3` | Override SMT-backend для контрактов |
| `NOVA_CACHE_DIR=PATH` | Override директории SMT proof cache (default `<cwd>/target/`) |
| `NOVA_CLANG=PATH` | Override `clang.exe` для test-build/test-all |
| `NOVA_GCC=PATH` | Override пути к `gcc` |
| `NOVA_VCVARS=PATH` | Override `vcvars64.bat` (Windows MSVC) |
| `NOVA_MARCH_NATIVE=1` | Включить `-march=native` (release builds; non-portable binary) |
| `NOVA_GC_LIB_DIR=PATH` | Override директории libgc.a / gc.lib (Boehm) |
| `NOVA_GC_INCLUDE_DIR=PATH` | Override include path для `gc.h` |
| `VCPKG_ROOT=PATH` | Корень vcpkg (для авто-резолва libuv / libz3) |
| `CC=name` | Fallback C compiler (POSIX) |
| `NOVA_VERSION=N.M.K` | Текущая версия для deprecation diagnostics (Plan 45 Ф.21) |
| `NOVA_FEATURES=f1,f2` | Cfg feature-set ([Plan 42.12](plans/42.12-cfg-conditional-compilation.md)) |
| `NOVA_TARGET_OS=name` | Override `target_os` для cfg-resolve |
| `TEMP` (Windows) | Tmp директория |
| `TMPDIR` (Unix) | Tmp директория |
| `PATH` | Поиск `clang`/`gcc`/`cl`/`vswhere` |
| `ProgramFiles(x86)` | Поиск vswhere для MSVC auto-detect |

---

## Cargo features

| Feature | Описание |
|---|---|
| (default) | TrivialBackend SMT (reflexive `ensures`), без external dependencies |
| `z3-backend` | Подключает libz3 через vcpkg ([Plan 33.1](plans/33.1-contracts-core.md)). FFI bindings — собственные в `src/verify/backend/z3_ffi.rs` (без `z3`/`z3-sys` crate'ов — feedback: обёртки только в наших файлах). Linkage контролируется через `build.rs` + `vcpkg.json` |

**Сборка с Z3:**
```bash
cargo build --release --features z3-backend
# Запуск тестов:
NOVA_SMT_BACKEND=z3 nova test
```

---

## Library API (`nova_codegen`)

`nova-cli` использует `nova-codegen` как path-зависимость и работает
с library-API напрямую (без subprocess). Публичные модули из `lib.rs`:

| Модуль | Что |
|---|---|
| `argbind` | Named/positional arg binding ([Plan 46](plans/46-named-parameters.md) / D102) |
| `ast` | Типы AST: `Module`, `Item`, `Expr`, `Stmt`, `Pattern`, ... |
| `callnorm` | Call-site normalization для named params |
| `codegen` | C-бэкенд: `CEmitter::emit_module`, `runtime_registry::all`, ... |
| `desugar` | Desugar map-литералов, `for-in`, и прочих сахаров |
| `diag` | Структурированные диагностики (`Diagnostic`, `Span`, `byte_to_line_col`) |
| `doc` | Plan 45 — DocModel, рендеры, MCP-server |
| `imports` | Plan 35 R31 — cross-file resolver (`resolve_imports_inline`) |
| `interp` | Treewalk-интерпретатор (`Interpreter::new/load_module/run_main/run_tests`) |
| `lexer` | Токенизация, `lex(&src) -> Vec<Token>` |
| `lints` | D-rule based lints (`lint_module`) |
| `manifest` | `nova.toml` + D78 path/module enforcement |
| `parser` | Recursive-descent parser (`parse(&src) -> Result<Module, Diagnostic>`) |
| `perf_timer` | `NOVA_PERF_TIMER` instrumentation |
| `test_runner` | Test discovery + parallel execution + toolchain detect |
| `types` | Type-checker + effect inference (`check_module`, `infer_effects`, `annotate_map_literals`) |
| `verify` | SMT contracts integration (`TrivialBackend`, `Z3Backend` под feature) |

**Re-exports:** `Diagnostic`, `Span` напрямую из `nova_codegen::*`.

---

## Внутренняя архитектура

```
src/
  lexer/                  токенизация
  parser/                 recursive-descent parser
  ast/                    типы AST
  types/                  type checker + effect inference + lints
  interp/                 treewalk interpreter
  codegen/                C-бэкенд
    emit_c.rs             основной codegen (~20k LOC)
    runtime_registry.rs   source-of-truth для std/runtime/*.nv stubs
  imports.rs              Plan 35 R31: cross-file resolver
  diag/                   структурированные ошибки (с FileId для cross-file)
  manifest.rs             nova.toml + D78 path enforcement
  lints.rs                D-rule based lints
  test_runner.rs          test discovery + parallel execution
  verify/                 contracts SMT (TrivialBackend + Z3 опционально)
  doc/                    Plan 45 nova doc (parser, renderer, MCP)
  desugar.rs              desugaring passes
  argbind.rs              named-params binding (Plan 46)
  callnorm.rs             call-site normalization
  perf_timer.rs           NOVA_PERF_TIMER markers
  lib.rs                  re-exports
  main.rs                 CLI dispatch (~664 LOC)
```

### Cross-file resolver (Plan 35 R31)

`imports::resolve_imports_inline` — shared между `nova check`,
`nova build`, `nova test`:

- DFS с cycle detection (`in_progress` + `visited` множества)
- Selective import `X.{A, B}` (синтаксис; bootstrap MVP не enforce'ит
  filter — full enforcement через type-checker post-bootstrap)
- `export import X.{A}` re-export
- Auto-import `std/prelude.nv` (R27)

### Folder-modules ([Plan 42](plans/42-folder-modules.md))

Module = single-file `X.nv` ИЛИ folder `X/` с peers (Go-style):

- `manifest::resolve_module_paths(parts, ...)` → `Vec<PathBuf>`,
  alphabetical sort (deterministic build)
- Filter `*_test.nv` peers если `!include_test_peers`
- `X.nv` + `X/` с direct `.nv` → ambiguous error
- `internal/<...>` path-protection — import только из parent's descendants
- `_module.nv` convention для module-level `#forbid` / `#cfg` / `#doc`

### D78 path/module enforcement

Если файл лежит внутри пакета (`nova.toml` в parent dirs), компилятор
проверяет соответствие `module X.Y.Z` ↔ filesystem path. Standalone
`.nv` без `nova.toml` проверку проходит.

### Effect inference (D28)

`types::infer_effects` добавляет `Fail` в effect-row private fn, если
тело содержит `throw` и нет явного `Fail`. Public fn — explicit only
(D28: «public API не должно неявно throw'ить»).

### Plan 33.3 contract stripping (Ф.9.9)

`CEmitter::set_proven_contracts(&module_env.proven_contracts)` —
selectively strip'ает body proven контрактов в codegen (true zero-cost
даже в debug). SMT-доказанные `requires`/`ensures`/`invariant` не
эмитятся как runtime assertions.

---

## Runtime (`nova_rt/`)

C-исходники линкуются в каждый `.exe`. Линкуется минимум **3** `.c`:

| Файл | Что |
|---|---|
| `alloc.c` (или `alloc_boehm.c`, `alloc_rc.c`) | Allocator (Boehm GC default с [Plan 27](plans/27-gc-switch.md)) |
| `effects.c` | Handler-стек (D61), `nova_interrupt` / `nova_interrupt_ptr` ([Plan 39](plans/39-range-stdlib-fixes.md) Issue A) |
| `fibers.c` | Shim над `minicoro.h` + structured concurrency ([Plan 44.5](plans/44.5-work-stealing-scheduler.md)) |

**Header-only:**

| Header | Что |
|---|---|
| `array.h` | `NovaArray_<T>`, `NovaOpt_<T>` auto-gen helpers |
| `cast.h` | D54 narrow casts (saturation, wrap-around semantics) |
| `effects.h` | `NovaThrowKind`, `nova_throw_cancel`, `Handler[E, IRT]` API |
| `fibers.h` | `nova_spawn`, `nova_supervised`, `nova_cancel_*`, M:N runtime |
| `channels.h` | `Channel[T]` mpsc ([Plan 44.1](plans/44.1-channel-hardening.md)), `select` waiter |
| `sync.h` | C11 atomics + mutex для channel hardening |
| `minicoro.h` | Vendored stackful coroutines (не патчим, version-pinned) |
| `nova_rt.h` | Единый include — `nova_str_cmp`/`lt`/`le`/`gt`/`ge` byte-wise compare и пр. |

**Build scripts** в `compiler-codegen/`:
- `build_c.bat` / `build_c.ps1` / `build_c.sh` — сборка одного `.c` в `.exe`
- `vcpkg.json` / `vcpkg_installed/` — vendored libuv + libz3
- `build.rs` — feature-flag wiring (`z3-backend`)

---

## Связанные документы

- [`docs/nova-cli.ru.md`](nova-cli.ru.md) — `nova` CLI (рекомендуемый
  user-facing entry point)
- [`compiler-codegen/README.md`](../compiler-codegen/README.md) —
  оригинальный README с детальной архитектурой
- [`spec/`](../spec/) — спецификация языка
- [`spec/decisions/`](../spec/decisions/) — D-блоки решений
- [`docs/test-conventions.md`](test-conventions.md) — EXPECT-маркеры
- [`docs/plans/13-runtime-stdlib-and-autogen.md`](plans/13-runtime-stdlib-and-autogen.md)
  — runtime registry + auto-gen
- [`docs/plans/24-cross-platform-test-runner.md`](plans/24-cross-platform-test-runner.md)
  — `test-build` / `test-all`
- [`docs/plans/26-test-runner-hardening.md`](plans/26-test-runner-hardening.md)
  — timeout / parallel / format / rerun-failed
- [`docs/plans/27-gc-switch.md`](plans/27-gc-switch.md) —
  `--gc boehm|malloc`
- [`docs/plans/35-cross-file-resolve.md`](plans/35-cross-file-resolve.md)
  — cross-file resolver
- [`docs/plans/42-folder-modules.md`](plans/42-folder-modules.md) —
  folder-modules
- [`docs/plans/33.1-contracts-core.md`](plans/33.1-contracts-core.md)
  — contracts + Z3 backend
- [`docs/plans/45-nova-doc.md`](plans/45-nova-doc.md) — `nova doc`
  (`nova_codegen::doc`)
- [`docs/plans/48-closures-in-generics.md`](plans/48-closures-in-generics.md)
  — monomorphization (`NOVA_MONO_DEPTH`)
