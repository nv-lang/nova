# Nova bootstrap compiler

Минимальный treewalk-интерпретатор Nova на Rust. Цель — поддержать
достаточно языковых фич для того, чтобы переписать компилятор уже на
Nova (self-hosting в v2.0).

## Что в нём есть

- Лексер всех токенов (D27/D44/D49)
- Парсер: модули, типы, выражения, декларации; D17/D52/D55/D58/D59/D60
- Resolve и базовая проверка имён
- Type checker с inference для локальных переменных и эффектов
  (D28). Generics через мономорфизацию.
- Treewalk-интерпретатор + handler-стек (D10/D31)
- Минимальные эффекты: `Io`, `Throws[E]`, `Mut`, `Random`
- CLI: `nova run`, `nova check`, `nova test`

## Чего нет (намеренно)

- LLVM codegen — интерпретатор достаточно для написания компилятора
- Concurrent GC — циклы текут, для bootstrap'а ок
- Async/Par fiber-runtime — синхронный исполняется
- SMT contracts — парсятся, не проверяются
- Comptime/macros, region/Realtime, JIT, hot reload, time-travel
- LSP, package manager, doc generator, formatter

## Запуск

```sh
cargo build --release
cargo test
cargo run -- run examples/hello.nv
cargo run -- check examples/effects.nv
cargo run -- test examples/
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
tests/        интеграционные тесты
examples/     минимальные тестовые программы
```
