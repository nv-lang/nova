# Nova bootstrap compiler

Treewalk-интерпретатор Nova на Rust. Цель — поддержать достаточно языковых
фич для написания компилятора на самой Nova (self-hosting в v2.0).

## Что в нём есть

- Лексер, парсер, AST — все текущие конструкции языка
- Type checker с inference для локальных переменных и эффектов (D28).
  Generics через мономорфизацию.
- Treewalk-интерпретатор + handler-стек
- Эффекты: `Io`, `Fail[E]`, `Random`, `Log`, пользовательские
- Handler-литералы через `handler` keyword (D61), `interrupt`, прямой вызов на handler-значении
- `forbid X { ... }` — capability sandbox (D63)
- `realtime { ... }` — маркер реального времени (D64), без GC-пауз (семантика, в bootstrap просто блок)
- `Self` как универсальный self-тип в методах и handler'ах (D66)
- Stateful handlers через closure capture (D68)
- `spawn() { ... }` — выполняется синхронно (fiber runtime отсутствует)
- CLI: `nova run`, `nova check`, `nova test`

## Чего нет (намеренно)

- C/LLVM codegen — для этого есть `compiler-codegen/`
- Concurrent GC — циклы текут, для bootstrap'а ок
- Fiber runtime — `spawn()` исполняется синхронно
- SMT contracts — парсятся, не проверяются
- Comptime/macros, JIT, hot reload, time-travel
- LSP, package manager, doc generator, formatter

## Запуск

```sh
cargo build --release
cargo test
cargo run -- run examples/hello.nv
cargo run -- check examples/effects.nv
cargo run -- test tests-nova/13_effects.nv
```

## Структура

```
src/
  lexer/      токенизация
  parser/     recursive-descent parser
  ast/        типы AST
  types/      type checker + effect inference
  interp/     treewalk interpreter
  diag/       структурированные ошибки
  lib.rs
  main.rs
tests/        интеграционные тесты (Rust)
tests-nova/   тесты на самой Nova (01_literals … 36_self_universal)
examples/     демо-программы
```
