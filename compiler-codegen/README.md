# Nova codegen compiler

Компилятор Nova с C-бэкендом: парсер + type checker + treewalk-интерпретатор + codegen в C. Цель — компилировать Nova в нативный бинарь через C (GCC/Clang).

## Что внутри

- Лексер, парсер, AST — форк из bootstrap с доработками
- Type checker с inference для локальных переменных и эффектов
- Treewalk-интерпретатор (те же возможности, что у bootstrap)
- **C-бэкенд** (`src/codegen/emit_c.rs`) — генерирует `.c` файл
- **Runtime** (`nova_rt/`) — заголовки и реализации:
  - `alloc.{h,c}` — аллокатор (ref-counting + опциональный Boehm GC)
  - `effects.{h,c}` — механизм эффектов (handler-стек, D61)
  - `fibers.{h,c}` — файберы через minicoro (stackful coroutines)
  - `nova_rt.h` — единый include для сгенерированного кода
- CLI: `nova-codegen check`, `run`, `test`, `compile`,
  `emit-runtime-stubs`, `dump-runtime` (Plan 13)

## Регенерация `std/runtime/*.nv` (Plan 13)

`std/runtime/string.nv` и `std/runtime/math.nv` — **auto-generated**
из `src/codegen/runtime_registry.rs`. **Не править вручную.**

Workflow добавления новой runtime fn (например `f64.@cbrt`):

1. Добавить запись в `src/codegen/runtime_registry.rs` (`RuntimeFn { ... }`).
2. Реализовать в `nova_rt/<module>.c` (или wrapper к libc).
3. Регенерировать stubs из корня nova-lang:
   ```
   cargo build --manifest-path compiler-codegen/Cargo.toml
   compiler-codegen/target/debug/nova-codegen.exe emit-runtime-stubs
   ```
4. Закоммитить все три (registry + .c + .nv).

**Проверка drift'а** (registry vs существующие .nv-файлы):
```
compiler-codegen/target/debug/nova-codegen.exe emit-runtime-stubs --check
```
Используется в CI / pre-commit hook'е для предотвращения manual edit'ов.

**Sanity-check реестра:**
```
compiler-codegen/target/debug/nova-codegen.exe dump-runtime
```
Печатает структурированный список всех зарегистрированных runtime-fn.

## Что поддерживает codegen

- Примитивные типы: `int`, `f64`, `f32`, `bool`, `str`, `byte`
- Records, sum types (match)
- Функции, методы, generics (через мономорфизацию)
- Эффекты и handlers (D61: `handler` keyword, `with X = h`, `interrupt`)
- `spawn func()` / `spawn { body }` через minicoro stackful coroutines
- `let`, `mut`, арифметика, строки, `println`

## Чего нет (намеренно)

- Concurrent GC — ref-counting достаточно для текущих примеров,
  опциональный Boehm GC через `alloc_boehm.c`
- `supervised`, `parallel for`, `race` — файберы есть, structured
  concurrency и cooperative scheduler из D71 — следующий шаг
- SMT contracts (D24) — парсятся, не проверяются
- Generic bounds (D72) и `From`/`Into` (D73) — в spec формализованы,
  в type-checker'е пока нет
- `realtime { ... }` (D64) — парсится, без compile-time enforcement
  правил no-suspend
- Comptime/macros, JIT, hot reload
- LSP, package manager, formatter

## Запуск

```sh
cargo build --release

# Интерпретировать (treewalk)
cargo run -- run ../examples/basics/hello.nv

# Скомпилировать в C (без линковки)
cargo run -- compile ../examples/basics/hello.nv          # -> ../examples/basics/hello.c
cargo run -- compile some.nv -o out.c                      # кастомное имя

# Type-check без запуска
cargo run -- check ../examples/basics/records.nv

# Тесты в файле через интерпретатор
cargo run -- test ../examples/effects/with_tests.nv

# Rust-уровневые юнит-тесты компилятора
cargo test
```

CLI-флаги для `compile`:
- `-o <path>` — кастомное имя выходного `.c`.
- `--no-annotate-source` — не вставлять `/* SRC: ... */` комментарии.
- `--no-lint` — отключить lint-проверки (например, `export-fail-untyped`).

## Сборка нативного бинаря (`.nv` → `.c` → `.exe`)

Pipeline двухступенчатый: компилятор делает `.nv → .c`, затем нативный
C-компилятор линкует `.c` с runtime'ом (`nova_rt/alloc.c`, `effects.c`,
`fibers.c`).

### Windows — wrapper-скрипт `build_c.ps1`

Самый простой путь — `build_c.ps1` в корне `compiler-codegen/`:

```powershell
# Один раз — собрать компилятор
cargo build

# Скомпилировать .nv → .exe одной командой
.\build_c.ps1 ..\examples\basics\hello.nv

# Скомпилировать и сразу запустить
.\build_c.ps1 ..\examples\basics\hello.nv -Run

# Кастомный выходной путь
.\build_c.ps1 hello.nv -Output bin\hello.exe

# Сохранить промежуточный .c (по умолчанию удаляется)
.\build_c.ps1 hello.nv -KeepC

# Альтернативный путь к vcvars64.bat (по умолчанию — VS Build Tools 2025)
.\build_c.ps1 hello.nv -VCVarsPath "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
```

Требования:
- **MSVC Build Tools** (для `cl.exe` через `vcvars64.bat`).
- **`cargo build`** в `compiler-codegen/` (создаёт `target/debug/nova-codegen.exe`).

#### Advanced: `build_c.bat` с GC backend выбором

Для случаев когда нужен контроль над allocator'ом — `build_c.bat`
принимает **уже сгенерированный `.c`** и линкует с одним из трёх GC
backend'ов:

```bat
rem Default — plain malloc, без free (быстрый dev-режим)
build_c.bat hello.c

rem Reference counting — через nova_retain/nova_release
build_c.bat hello.c hello.exe gc=rc

rem Boehm tracing GC — collects cycles, требует bdwgc через vcpkg
build_c.bat hello.c hello.exe gc=boehm
```

Сначала нужно сделать `.nv → .c`:

```powershell
cargo run -- compile hello.nv
build_c.bat hello.c
```

`build_c.bat` использует `cl.exe + link.exe` отдельными шагами,
per-source `.obj` файлы, поддерживает Boehm GC через `vcpkg_installed/`.

| Опция | Что |
|---|---|
| `gc=malloc` (default) | plain malloc, без `free` (быстрый dev-режим) |
| `gc=rc` | refcount via `nova_retain`/`nova_release` (`alloc_rc.c`) |
| `gc=boehm` | Boehm tracing GC (`alloc_boehm.c` + `gc.lib` + `atomic_ops.lib`) |

`build_c.ps1` использует **только** plain `malloc` backend (соответствует
`gc=malloc`). Если нужны `rc` или `boehm` — `build_c.bat`.

### Linux / Mac — wrapper-скрипт `build_c.sh`

Аналог `build_c.ps1` для Unix-систем:

```sh
# Один раз — собрать компилятор
cargo build

# Скомпилировать .nv → нативный бинарь
./build_c.sh path/to/hello.nv

# Скомпилировать и сразу запустить
./build_c.sh path/to/hello.nv --run

# Кастомный выходной путь
./build_c.sh hello.nv -o bin/hello

# Использовать clang вместо gcc
./build_c.sh hello.nv --cc clang

# Сохранить промежуточный .c
./build_c.sh hello.nv --keep-c
```

Через переменную окружения тоже работает: `CC=clang ./build_c.sh hello.nv`.

### Linux / Mac — вручную (если нужно без скрипта)

```sh
# 1. Скомпилировать Nova → C
cargo run -- compile path/to/hello.nv

# 2. Слинковать .c с runtime'ом
gcc path/to/hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
    -I. -o hello

# 3. Запустить
./hello
```

`-I.` указывает на корень `compiler-codegen/`, потому что сгенерированный
`.c` содержит `#include "nova_rt/nova_rt.h"`. Можно использовать
абсолютный путь: `-I/path/to/compiler-codegen`. Аналогично с Clang —
заменить `gcc` на `clang`.

### Минимальный walkthrough (Windows)

```powershell
# Создать hello.nv где-нибудь вне репо (чтобы D78 path-check не сработал)
$tmp = "$env:TEMP\nova_demo"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
@'
fn main() {
    println("Hello, Nova!")
}
'@ | Out-File -Encoding ascii "$tmp\hello.nv"

# Из compiler-codegen/:
.\build_c.ps1 "$tmp\hello.nv" -Run

# Output:
# [1/3] codegen: ...\hello.nv -> ...\hello.c
# [2/3] cl.exe:  ...\hello.c  -> ...\hello.exe
# [3/3] built:   ...\hello.exe
# running ...\hello.exe ...
# Hello, Nova!
# exit code: 0
```

### Минимальный walkthrough (Linux/Mac)

```sh
echo 'fn main() { println("Hello, Nova!") }' > /tmp/hello.nv
cd compiler-codegen
cargo build
./build_c.sh /tmp/hello.nv --run

# Output:
# [1/3] codegen: /tmp/hello.nv -> /tmp/hello.c
# [2/3] gcc:     /tmp/hello.c  -> /tmp/hello
# [3/3] built:   /tmp/hello
# running /tmp/hello ...
# Hello, Nova!
# exit code: 0
```

### Что линкуется из `nova_rt/`

Минимум для работающего бинаря — три `.c` файла:

| Файл | Что |
|---|---|
| `nova_rt/alloc.c` | аллокатор (по умолчанию ref-counting; Boehm GC через `alloc_boehm.c`) |
| `nova_rt/effects.c` | dispatch handler-стека (D61) |
| `nova_rt/fibers.c` | thin shim над `minicoro.h` |

Заголовки (`array.h`, `buffer.h`, `effects.h`, `fibers.h`, `cast.h`, `channels.h`,
`minicoro.h`, `nova_rt.h`) подтягиваются через `#include`, отдельно
линковать не нужно — header-only.

### Batch-прогон тестов

`run_tests.ps1` в корне репозитория делает то же самое для всех `.nv`
в `nova_tests/`:

```powershell
.\run_tests.ps1                       # все
.\run_tests.ps1 -Filter "buffer"      # только buffer-тесты
.\run_tests.ps1 -IncludeStdlib        # плюс std/*.nv
```

### Известные ограничения

1. **Один `.nv` файл = один `.exe`.** Multi-module компиляция (cross-file
   imports) в codegen не поддержана. Импорты `import std.X.Y` сейчас
   работают только в interp-режиме (`cargo run -- run`).
2. **`std/*.nv` не подключается автоматически.** Если в твоём `.nv` есть
   `import std.collections.HashMap` — codegen не найдёт. Используй
   `cargo run -- run` для тестирования с stdlib, или inline-copy нужного
   кода.
3. **Path/module enforcement (D78).** Если файл лежит внутри пакета
   (есть `nova.toml` в parent dirs), компилятор проверяет соответствие
   `module path.X` ↔ filesystem path. Standalone `.nv` без `nova.toml`
   эту проверку проходит.

## Структура

```
src/
  lexer/      токенизация
  parser/     recursive-descent parser
  ast/        типы AST
  types/      type checker + effect inference
  interp/     treewalk interpreter
  codegen/    C-бэкенд (emit_c.rs)
  diag/       структурированные ошибки
  lib.rs
  main.rs
nova_rt/      C runtime (alloc, effects, fibers, minicoro)
examples/     .nv файлы (basics/, effects/, real-world/, stdlib/, effect-density/)
              + сгенерированные .c + скомпилированные .exe
tests/        интеграционные тесты
```
