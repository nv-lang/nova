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
- CLI: `nova-codegen check`, `run`, `test`, `compile`

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

# Интерпретировать
cargo run -- run examples/basics/hello.nv

# Скомпилировать в C
cargo run -- compile examples/basics/hello.nv             # -> examples/basics/hello.c
cargo run -- compile examples/effects/effects.nv -o out.c

# Type-check без запуска
cargo run -- check examples/basics/records.nv

# Тесты
cargo run -- test examples/effects/with_tests.nv
cargo test
```

## Сборка нативного бинаря

После `compile` компилируем через GCC/Clang:

```sh
gcc examples/basics/hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
    -Inova_rt -o hello && ./hello
```

На Windows (через `build_c.bat`):

```bat
build_c.bat examples\basics\hello.c
```

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
