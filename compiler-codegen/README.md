# Nova codegen compiler

Компилятор Nova с C-бэкендом: парсер + type checker + treewalk-интерпретатор + codegen в C + cross-file resolver. Цель — компилировать Nova в нативный бинарь через C (GCC/Clang/MSVC).

> **Внутренний компонент.** Для повседневного использования есть
> `nova` CLI ([../nova-cli/](../nova-cli/)). `nova-codegen` остаётся
> точкой входа для IDE, CI, прямой отладки codegen.

## Что внутри

- Лексер, парсер, AST
- Type checker с inference для локальных переменных и эффектов
- Treewalk-интерпретатор (`src/interp/`) — те же возможности, что у codegen
- **Cross-file resolver** (`src/imports.rs`) — Plan 35 R31: shared между
  `nova check`, `nova build`, `nova test` (DFS с cycle detection через
  `in_progress`/`visited` множества; selective import `X.{A, B}`; prelude
  auto-import).
- **C-бэкенд** (`src/codegen/emit_c.rs`) — генерирует `.c` файл,
  поддерживает effect handlers (Plan 39 Issue A: NovaInterruptFrame с
  отдельными slot'ами `value`/`value_ptr` для IntLike/Pointer/ValueStruct
  результатов), numeric type constants (Plan 38), str lex compare
  operators, и многое другое.
- **Runtime** (`nova_rt/`) — заголовки и реализации:
  - `alloc.{h,c}` / `alloc_boehm.c` / `alloc_rc.c` — аллокатор (Boehm GC
    default с Plan 27, опциональные RC/plain malloc backends).
  - `effects.{h,c}` — механизм эффектов (handler-стек, D61, `interrupt` /
    `nova_interrupt_ptr`).
  - `fibers.{h,c}` — файберы через minicoro (stackful coroutines),
    structured concurrency (`supervised`, `spawn`, `cancel_scope`).
  - `channels.h` / `sync.h` — `Channel[T]` mpsc (Plan 40), `select`.
  - `nova_rt.h` — единый include для сгенерированного кода (вкл.
    `nova_str_cmp`/`lt`/`le`/`gt`/`ge` lex byte-wise compare).
- CLI: `nova-codegen check`, `run`, `test`, `compile`, `test-build`,
  `test-all`, `emit-runtime-stubs`, `dump-runtime`.

## Регенерация `std/runtime/*.nv` (Plan 13)

`std/runtime/string.nv` и `std/runtime/math.nv` — **auto-generated**
из `src/codegen/runtime_registry.rs`. **Не править вручную.**

Workflow добавления новой runtime fn (например `f64.@cbrt`):

1. Добавить запись в `src/codegen/runtime_registry.rs` (`RuntimeFn { ... }`).
2. Реализовать в `nova_rt/<module>.c` (или wrapper к libc).
3. Регенерировать stubs из корня nova-lang:
   ```sh
   nova regen-runtime
   ```
   Или через nova-codegen напрямую:
   `compiler-codegen\target\debug\nova-codegen.exe emit-runtime-stubs --root .`
4. Закоммитить все три (registry + .c + .nv).

**Проверка drift'а** (registry vs существующие .nv-файлы):
```sh
nova regen-runtime --check
```
Используется в CI / pre-commit hook'е для предотвращения manual edit'ов.

**Sanity-check реестра:**
```
compiler-codegen/target/debug/nova-codegen.exe dump-runtime
```
Печатает структурированный список всех зарегистрированных runtime-fn.

## Что поддерживает codegen

- Примитивные типы: `int`, `i8`-`i64`, `u8`-`u64`, `f32`, `f64`, `bool`,
  `str`, `byte`, `char`. Numeric type constants (`int.MAX`, `f64.NAN`,
  etc. — Plan 38).
- Records, sum types (tagged variants + match), generic types (через
  мономорфизацию), `Option[T]`, `Result[T, E]` через auto-gen typedef
  per element type + auto-gen `nova_opt_eq_<T>` / `Nova_Option_method_is_some_<T>`.
- Функции, методы (instance/static), method overloading (Plan 11),
  generic methods, variadic params (`...items []T` — D69).
- **Эффекты и handlers**: `type X effect { ... }`, `handler X { ... }`,
  `with X = h { body }`, `interrupt v`, `throw`, `?`, `!!`, `??`
  (D61/D62/D85/D86). `Handler[E, IRT]` first-class type (D87).
  `forbid X { body }` (D63), `realtime { body }` (D64).
- **Structured concurrency** (D71): `spawn`, `supervised`, `parallel for`,
  `cancel_scope`, `detach`, channels (`Channel[T]` mpsc через Plan 40),
  `select { ... }` (D94 / Plan 31).
- **Contracts** (D24): `requires` / `ensures` / `old` / `result` /
  `invariant` / `reads` / `modifies` / `decreases` / ghost let / assume
  / `assert_static`. Bootstrap MVP: TrivialBackend SMT (reflexive
  ensures); Z3 — milestone когда libz3 setup.
- **Cross-file imports** (Plan 35 Ф.1): `import X.Y.Z`, `import X.{A, B}`
  selective, `export import X.{A}` re-export, `std/prelude.nv`
  auto-import (R27). Cycle detection через DFS.
- `defer` / `errdefer` scope-level cleanup (D90 / Plan 20).
- str lex compare (`<`, `<=`, `>`, `>=`) — byte-wise (ASCII-correct,
  UTF-8 partial).

## Чего нет (намеренно или roadmap)

- **Z3 SMT backend** — currently TrivialBackend; Z3 wiring — milestone.
- **Type-checker bidirectional inference** — pull-based (codegen
  правильно эмитит, но type-checker не enforce'ит type compat для
  `interrupt v ⊑ T_body` в `with`-блоке; runtime UB исключён через
  codegen-side fix Plan 39 Issue A).
- **Concurrent GC** — Boehm conservative GC default; pauses < 16ms
  measured. v1.0+: concurrent GC (Plan 25 G3b).
- **JIT** — только AOT через C.
- **Comptime/macros, hot reload, LSP, formatter, package manager** —
  roadmap, не блокеры для bootstrap.

## Запуск

Для повседневной работы — `nova` CLI (`../nova-cli/`):

```sh
cd ../nova-cli && cargo build --release && cd -

# Type-check
../nova-cli/target/release/nova check path/to/hello.nv

# Run via интерпретатор
../nova-cli/target/release/nova run path/to/hello.nv

# Compile to native binary (.nv → .c → .exe)
../nova-cli/target/release/nova build path/to/hello.nv

# Run all tests
../nova-cli/target/release/nova test

# Regenerate runtime stubs (Plan 13)
../nova-cli/target/release/nova regen-runtime
```

`nova` сам выбирает toolchain (Clang приоритетно, MSVC/GCC fallback) и
auto-detect'ит libuv для concurrency tests.

### Прямой вызов nova-codegen (только для IDE / отладки)

```sh
cargo build --release

# Type-check без запуска
cargo run --release -- check path/to/hello.nv

# Скомпилировать в C (без линковки)
cargo run --release -- compile path/to/hello.nv          # -> path/to/hello.c
cargo run --release -- compile some.nv -o out.c           # кастомное имя

# Интерпретировать (treewalk)
cargo run --release -- run path/to/hello.nv

# Single-test debugging (compile + link + run одного теста)
cargo run --release -- test-build path/to/test.nv --toolchain clang --keep-artifacts

# Batch test runner
cargo run --release -- test-all --tests-dir ../nova_tests

# Rust-уровневые юнит-тесты компилятора
cargo test
```

CLI-флаги для `compile`:
- `-o <path>` — кастомное имя выходного `.c`.
- `--no-annotate-source` — не вставлять `/* SRC: ... */` комментарии.
- `--no-lint` — отключить lint-проверки.

### Что линкуется из `nova_rt/`

Минимум — три `.c`:

| Файл | Что |
|---|---|
| `nova_rt/alloc.c` (или `alloc_boehm.c`) | аллокатор (default — Boehm GC). |
| `nova_rt/effects.c` | dispatch handler-стека (D61), `nova_interrupt` / `nova_interrupt_ptr` (Plan 39). |
| `nova_rt/fibers.c` | shim над `minicoro.h` + structured concurrency runtime (D71). |

Заголовки (`array.h`, `effects.h`, `fibers.h`, `cast.h`, `channels.h`,
`sync.h`, `minicoro.h`, `nova_rt.h`) — header-only.

### Path/module enforcement (D78)

Если файл лежит внутри пакета (`nova.toml` в parent dirs), компилятор
проверяет соответствие `module path.X` ↔ filesystem path. Standalone
`.nv` без `nova.toml` проверку проходит.

### Cross-file resolve (Plan 35 R31 + Ф.1 + Plan 42)

`nova check` / `nova build` / `nova test` все используют общий
resolver (`src/imports.rs::resolve_imports_inline`):
- DFS с cycle detection (`in_progress` / `visited` множества).
- Selective import `X.Y.{A, B}` (синтаксис принимается; bootstrap MVP
  не enforce'ит filter — full enforcement через type-checker post-bootstrap).
- `export import X.{A}` re-export.
- Prelude auto-import: `std/prelude.nv` (если файл существует)
  автоматически подгружается без explicit `import`.

### Folder-modules (Plan 42 / D29 rev-3)

Module = single-file `X.nv` ИЛИ folder `X/` с peers (Go-style).

- `resolve_module_paths(parts, ...)` returns Vec<PathBuf> —
  alphabetical sort peers (deterministic build).
- Filter `*_test.nv` peers если !include_test_peers (Sub-plan 42.1 F).
- Conflict `X.nv` + `X/` с direct .nv → ambiguous error.
- `internal/<...>` path-protection (Sub-plan 42.1 H): import only from
  parent's descendants.
- File-level `#forbid Eff1, Eff2` (Sub-plan 42.1): attribute после
  `module X` объявляет per-file capability constraint, enforce'ится
  через `CapabilityCtx.forbidden_stack` initial frame.

`walk_nv` (test_runner.rs) folder-aware: peers одного folder-module
**не** компилируются как standalone test entries (только entry-files
получают `main` codegen).

`module declaration format = parent.X` (D29 rev-3): file basename для
single-file, folder name для folder-module peer. Compat mode принимает
**оба** формата (rev-1 full path + rev-3 parent.X) для постепенной
миграции std/* — sub-plan 42.6.

## Структура

```
src/
  lexer/                  токенизация
  parser/                 recursive-descent parser
  ast/                    типы AST (Module/Item/Expr/Stmt/Pattern/...)
  types/                  type checker + effect inference + lints
  interp/                 treewalk interpreter
  codegen/                C-бэкенд
    emit_c.rs             основной codegen (~12k LOC)
    runtime_registry.rs   source-of-truth для std/runtime/*.nv stubs
  imports.rs              Plan 35 R31: cross-file resolver
  diag/                   структурированные ошибки (с FileId для cross-file)
  manifest.rs             nova.toml + D78 path enforcement
  lints.rs                D-rule based lints
  test_runner.rs          test discovery + parallel execution
  verify.rs               contracts SMT integration (TrivialBackend + Z3 stub)
  lib.rs
  main.rs
nova_rt/                  C runtime
  alloc.{h,c}             allocator (Boehm GC default)
  alloc_boehm.c           bdwgc backend
  alloc_rc.c              ref-counting backend (legacy)
  effects.{h,c}           handler-стек, interrupt (incl. nova_interrupt_ptr)
  fibers.{h,c}            minicoro shim + supervised/spawn/cancel
  channels.h              Channel[T] mpsc (Plan 40)
  sync.h                  C11 atomics + mutex для channel hardening
  array.h                 NovaArray_<T>, NovaOpt_<T> auto-gen helpers
  cast.h                  D54 narrow casts
  minicoro.h              stackful coroutines (vendored, не патчим)
  nova_rt.h               единый include
examples/                 .nv демо-программы (+ .c / .exe для inspection)
tests/                    Rust-уровневые интеграционные тесты
```
