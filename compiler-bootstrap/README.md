# Nova bootstrap compiler

Treewalk-интерпретатор Nova на Rust. Цель — поддержать достаточно языковых
фич для написания компилятора на самой Nova (self-hosting в v2.0).

## Что в нём есть

- Лексер, парсер, AST — все текущие конструкции языка
- Type checker с inference для локальных переменных и эффектов (D28).
  Generics через мономорфизацию.
- Treewalk-интерпретатор + handler-стек
- Эффекты: `Io`, `Fail[E]`, `Random`, `Log`, пользовательские
- Handler-литералы через `handler` keyword (D61), `interrupt`,
  прямой вызов на handler-значении
- `forbid X { ... }` — capability sandbox (D63)
- `realtime { ... }` — маркер реального времени (D64); в bootstrap
  семантически — обычный блок
- `Self` как универсальный self-тип в методах, protocol'ах, эффектах (D66)
- Stateful handlers через closure capture (D68)
- `spawn func()` / `spawn { body }` — keyword-конструкция (D43, D50)
- CLI: `nova run`, `nova check`, `nova test`

## Чего нет (намеренно)

- C/LLVM codegen — для этого есть `compiler-codegen/`
- Concurrent GC — циклы текут, для bootstrap'а ок
- Полноценный fiber runtime — `spawn` cooperative-планировщик из D71
  частично реализован, продакшн-runtime — расширение D71
- SMT contracts — парсятся, не проверяются
- Generic bounds (D72) и `From`/`Into` (D73) — формализованы в spec,
  пока не имплементированы в type-checker'е
- Comptime/macros, JIT, hot reload, time-travel
- LSP, package manager, doc generator, formatter

## Запуск

```sh
cargo build --release
cargo test
cargo run -- run examples/hello.nv
cargo run -- check examples/effects.nv
cargo run -- test nova_tests/effects/throws.nv
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
nova_tests/   тесты на самой Nova (basics/ types/ syntax/ effects/ concurrency/ runtime/ modules/)
examples/     демо-программы
```
