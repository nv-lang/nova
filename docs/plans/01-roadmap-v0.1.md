# Nova — план разработки

Дорожная карта реализации компилятора. Дизайн-решения — в
[spec/decisions/](../../spec/decisions/), синтаксис — в
[syntax.md](../../spec/syntax.md). Здесь — план **реализации**.

## Bootstrap-стратегия

| Этап | Язык | Что делает |
|---|---|---|
| **v0.1–v1.0** | Rust | Bootstrap компилятор (см. [Q17](../../spec/open-questions.md)) |
| **v2.0+** | Nova | Self-hosting — переписать компилятор на Nova |

Выбор Rust обоснован в [Q17](../../spec/open-questions.md#q17-bootstrap-язык-компилятора-—-rust):
лучшая экосистема для PL (LLVM, парсеры), pattern matching/sum-types
для AST, AI-codegen качество, прецедент новых языков (Roc, Gleam,
Carbon, Mojo).

---

## v0.1 — Type-check + интерпретатор

**Цель.** Минимальная версия для итераций дизайна. НЕ компилятор native
кода. Интерпретатор запускает тестовые программы, type-check находит
несовпадение эффектов, структурированные ошибки помогают LLM.

**Критерий готовности.** Упрощённый [examples/audit.nv](../../examples/audit.nv)
(без БД, с in-memory mock'ом) запускается через `nova run` и проходит
свои тесты через `nova test`.

### Что входит

#### 1. Парсер — полный синтаксис v1.0

- **Лексер** для всех токенов: ключевые слова, идентификаторы,
  литералы (числа hex/bin/oct/dec/float, строки `"..."`, raw-strings
  `` `...` ``, char), операторы, пунктуация
- **Интерполяция** `${...}` в строках через автомат лексера
- **Парсер** recursive-descent (или `chumsky`-based):
  - **Декларации:** `module`, `import`/`export`, `fn`, `type`, `let`/
    `const`, `test`
  - **Типы:** примитивы, `[]T`, `[N]T`, `Имя[T]`, кортежи `(A, B)`,
    функциональные `T -> U`, структурные `{ name type, op() -> T }`
  - **Выражения:** литералы, `if`/`else`, `match` с guard'ами, `for`/
    `while`/`loop`, лямбды `(x) => ...`, вызовы, `?`/`??`, операторы
  - **Эффекты в сигнатуре:** `fn f(x int) Db Throws -> int`
  - **Контракты:** `requires`/`ensures` (парсинг, не доказательство)

**Не парсится в v0.1:** `comptime`, attributes/decorators, advanced
patterns (or-pattern `A | B`).

#### 2. Type checker

- Структурная типизация ([D15](../../spec/decisions/02-types.md#d15))
- Inference типов локальных переменных
- Generic-параметры через мономорфизацию
- **Effect inference** для private ([D28](../../spec/decisions/04-effects.md#d28))
- **Effect checking** для public — обязательны в сигнатуре
- Exhaustiveness check для match ([D17](../../spec/decisions/02-types.md#d17))
- Проверка `?` требует `Throws[E]` в сигнатуре ([D25](../../spec/decisions/04-effects.md#d25))
- Проверка `with X = ... { ... }` совместим с эффектом

**Не делается в v0.1:**
- SMT-проверка контрактов ([D24](../../spec/decisions/09-tooling.md#d24)) — только
  runtime-check в debug
- Effect polymorphism ([Q6](../../spec/open-questions.md))
- Concurrent GC ([D6](../../spec/decisions/05-memory.md#d6)) — для интерпретатора простой
  mark-and-sweep, production-уровневый concurrent collector с паузами
  <1ms — в v0.4+ вместе с LLVM
- `region { ... }` блок и эффект `Realtime` ([D6](../../spec/decisions/05-memory.md#d6)) —
  v0.5

#### 3. Интерпретатор

- AST walker (treewalk, не bytecode)
- Handler-стек для эффектов: `with X = handler { body }` помещает в
  стек, операции ищут handler сверху
- Fiber'ы через **stackful coroutines** для `Async`/`Par`. Конкретная
  реализация — открытый вопрос ([Q3](../../spec/open-questions.md#q3-реализация-fiber-stack-для-async)),
  склонность — `corosensei` для быстрого старта
- `panic` = смерть текущего fiber'а ([D13](../../spec/decisions/08-runtime.md#d13)),
  supervisor рестартует
- Реализация stdlib-prelude

**Не реализовано в v0.1:**
- Native codegen (LLVM)
- JIT
- Time-travel debugging ([R8](../../spec/revolutionary.md))
- Hot reload
- Production concurrent GC — для интерпретатора достаточно простого
  mark-and-sweep ([D6](../../spec/decisions/05-memory.md#d6))
- Preemptive scheduling fiber'ов — в v0.1 cooperative по yield-points
  (вызовы операций эффектов), полная preemption (как Go 1.14+) — позже

#### 4. Стандартный набор эффектов

Из списка [D26](../../spec/decisions/08-runtime.md#d26) в v0.1 реализуется:

- **`Io`** — `stdout.write`, `stderr.write`, `print`, `println`
- **`Throws[E]`** — `throw`/`?` через handler ([D25](../../spec/decisions/04-effects.md#d25))
- **`Time`** — `Time.now()`, `Time.sleep(d)`. В тестах — fixed handler
- **`Random`** — простой LCG. В тестах — seeded handler
- **`Log`** — структурный логгер, JSON-консоль по умолчанию
- **`Mut`** — мутация ссылок, нужна для интерпретатора локальных `mut`
- **`Async`/`Par`** — fiber-runtime + `parallel for`/`supervised`
- **`Db`, `Net`, `Fs`** — только сигнатуры и mock handler'ы для тестов

`Alloc[R]`, `Trace`, `Ask[T]` (тоже из D26) — откладываются: `Alloc[R]`
зависит от регионов (v0.5), `Trace` и `Ask[T]` не критичны для MVP.

#### 5. Структурированные ошибки

- Формат [R5.3](../../spec/revolutionary.md): место → причина →
  fix-suggestion → ссылка на доку
- JSON-режим `--json` для AI-интеграции
- Текстовый режим (default) для человека
- Exhaustiveness errors с непокрытыми вариантами
- Effect errors с цепочкой вызовов, добавивших эффект

#### 6. CLI

```bash
nova check file.nv              # type-check, без запуска
nova check --fragment 'fn f...' # type-check одной функции (R5.5)
nova run file.nv                # type-check + interpret
nova test                       # запустить test-блоки
nova fmt file.nv                # минимальное форматирование
```

**Не реализовано в v0.1:**
- `nova build` (нативная сборка)
- `nova run --record` (time-travel)
- `nova doc`, `nova bench`
- LSP-сервер
- Package manager

#### 7. Стандартная библиотека — минимум

- **prelude** ([D26](../../spec/decisions/08-runtime.md#d26)): `Option`, `Result`, `Error`,
  `Never`, `Ordering`, `Some`, `None`, `Ok`, `Err`, `print`, `println`,
  `panic`
- **`std.collections`:** `HashMap[K, V]`, `HashSet[T]` (динамические
  массивы — встроенный `[]T` по [D27](../../spec/decisions/03-syntax.md#d27), отдельный
  `Vec[T]` не нужен)
- **`std.string`:** `len`, `to_lower`, `to_upper`, `split`, `trim`,
  `starts_with`, `ends_with`, `contains`, `replace`, интерполяция
- **`std.option`/`std.result`:** `unwrap`, `unwrap_or`, `map`,
  `and_then`, `ok_or`
- **`std.io`:** `print`, `println`, `eprint`, `eprintln`
- **`std.fmt`:** `to_str` для встроенных типов

### Что откладывается

| Фича | До | Причина |
|---|---|---|
| Native codegen (LLVM) | v0.4 | Интерпретатор быстрее для итераций |
| Production concurrent GC | v0.4 | После LLVM, целевые паузы <1ms ([D6](../../spec/decisions/05-memory.md#d6)) |
| Region/Realtime эффект ([D6](../../spec/decisions/05-memory.md#d6)) | v0.5 | После основного GC |
| SMT для контрактов ([D24](../../spec/decisions/09-tooling.md#d24)) | v0.6 | Z3-интеграция большая работа |
| Effect polymorphism ([Q6](../../spec/open-questions.md)) | v0.5 | Сложная теория |
| `comptime`/macros ([Q7](../../spec/open-questions.md)) | v1.0+ | Не критично для итераций |
| JIT | v1.5+ | Не приоритет |
| Time-travel debugging ([R8](../../spec/revolutionary.md)) | v0.8 | Нужна запись handler-вызовов |
| Distributed handlers ([R12](../../spec/revolutionary.md)) | v1.0+ | Поверх готовой системы эффектов |
| LSP-сервер | v0.5 | После стабилизации API |
| Package manager | v0.6 | Сначала monorepo-style разработка |
| Hot reload | v1.0+ | После JIT |
| Schema evolution ([Q13](../../spec/open-questions.md)) | v0.9 | После того как stdlib устаканится |
| Bitflags / enum с числами ([Q15](../../spec/open-questions.md)/[Q16](../../spec/open-questions.md)) | v0.5 | Сначала базовый язык |
| Erlang-style per-fiber heap | v2.0+ | v1.0 — Go-style shared heap ([Q12](../../spec/open-questions.md)) |

### Архитектура bootstrap-компилятора

**Текущая раскладка** (один crate, плоская структура):

```
nova-lang/compiler-bootstrap/           (Rust crate, name = "nova")
├── Cargo.toml
├── src/
│   ├── lexer/                          токенизация
│   ├── parser/                         AST builder (ручной recursive-descent)
│   ├── ast/                            типы AST
│   ├── types/                          type checker (минимальный)
│   ├── interp/
│   │   ├── mod.rs                      treewalk interpreter
│   │   ├── env.rs                      окружения и handler-стек
│   │   ├── value.rs                    Value, Handler types
│   │   └── stdlib.rs                   встроенные методы
│   ├── diag.rs                         структурированные ошибки
│   ├── lib.rs
│   └── main.rs                         CLI: nova run / check / test
├── tests/
│   ├── common/                         тестовый helper
│   ├── integration.rs                  smoke-тесты компилятора
│   └── spec_nova.rs                    запуск ../nova_tests/*.nv
└── examples/                           Rust-side примеры (hello, effects, ...)
```

`nova_tests/` (на top-level репозитория) — conformance-тесты языка,
общие для bootstrap'а и будущего self-hosted компилятора.

**Изначально** план предполагал разделение на crates
(`nova-lexer`, `nova-parser`, ...) — отказались как от
overengineering для bootstrap'а. Production-компилятор может вернуть
crate-разделение если потребуется.

### Последовательность разработки v0.1

#### Этап 1. Lexer + Parser (2–3 недели)

- Lexer всех токенов (актуальные ключевые слова из
  [D22](../../spec/decisions/03-syntax.md#d22), [D27](../../spec/decisions/03-syntax.md#d27),
  [D29](../../spec/decisions/07-modules.md#d29))
- Parser деклараций
- Parser типов (`[]T`, `[N]T`, `Имя[T]`, structural `{...}`)
- Parser выражений (литералы, операторы, `if`, `match` с guards, `for`,
  `while`, `loop`, лямбды, вызовы, `?`, `??`)
- Parser контрактов (`requires`/`ensures` после сигнатуры, перед `=>`)
- Golden-тесты: каждое правило грамматики

**Готово, когда:** [examples/audit.nv](../../examples/audit.nv) парсится
без ошибок (type-check может падать).

#### Этап 2. Resolve + Type checker (3–4 недели)

- Resolve имён: модули, импорты ([D29](../../spec/decisions/07-modules.md#d29)), шейдинг,
  conflict detection, проверка отсутствия циклических импортов
- Type checker: примитивы, generic-инстанциация, structural matching,
  кортежи, массивы
- Pattern matching: exhaustiveness для sum-type
- Effect inference для private ([D28](../../spec/decisions/04-effects.md#d28))
- Effect checking для public

**Готово, когда:** type-check проходит на простых программах (hello
world с эффектами, парсер JSON, fizzbuzz, упрощённый audit с заглушками).

#### Этап 3. Интерпретатор + минимальный stdlib (3–4 недели)

- Treewalk interpreter
- Handler-стек, операции эффектов
- Fiber-runtime через `corosensei` (склонность по
  [Q3](../../spec/open-questions.md))
- `spawn`, `parallel for`, `supervised` ([Q12](../../spec/open-questions.md)
  v1.0-план)
- `panic` = exit fiber'а ([D13](../../spec/decisions/08-runtime.md#d13))
- Stdlib: `Option`, `Result`, `HashMap`, `String`, `print`, `Io`

**Готово, когда:** упрощённый `audit_simplified.nv` запускается и
проходит свои тесты через `nova test`.

#### Этап 4. CLI + diagnostics (1–2 недели)

- `nova run`, `nova check`, `nova test`, `nova fmt`
- Структурированные ошибки в стиле R5.3
- JSON-режим (`--json`)
- `--fragment` для одной функции

**Готово, когда:** ошибка в `audit.nv` показывает понятное сообщение
с fix-suggestion.

#### Этап 5. Документация + примеры (1 неделя)

- Tutorial: «Hello world → effects → handlers → tests»
- Cookbook: 10–15 типичных задач
- Reference: ссылка на [spec/decisions/](../../spec/decisions/)/[syntax.md](../../spec/syntax.md)
- Примеры в [examples/](../../examples/)

**Готово, когда:** новый человек прочитав tutorial пишет функцию с
эффектом за час.

### Тестовая стратегия

1. **Golden-тесты парсера** — каждое грамматическое правило, snapshot
   AST в JSON
2. **Type-check тесты** — пары (программа, ожидаемый тип/ошибка)
3. **Interpretation тесты** — тестовые программы с ассертами
4. **Property-based** для парсера (`proptest`) — random-генерация
   валидных AST, round-trip
5. **Differential testing** — сравнить вывод между версиями
   интерпретатора (для регрессий)

### Метрики готовности v0.1

- `audit_simplified.nv` компилируется и запускается через `nova run`
- `nova test` запускает тесты, все зелёные
- Все примеры из [syntax.md](../../spec/syntax.md),
  [effects.md](../../spec/effects.md) парсятся и type-check'аются
- Ошибки структурированные, формат [R5.3](../../spec/revolutionary.md)
- Время компиляции `audit.nv` (~430 строк) — type-check <500ms
- Размер компилятора ~10–15k LOC Rust

### Оценка времени

При работе одного человека с AI-помощью:

| Этап | Срок |
|---|---|
| 1. Lexer + Parser | 2–3 недели |
| 2. Resolve + Type checker | 3–4 недели |
| 3. Интерпретатор + stdlib | 3–4 недели |
| 4. CLI + diagnostics | 1–2 недели |
| 5. Документация + примеры | 1 неделя |
| **Итого** | **10–14 недель (2.5–3.5 месяца)** |

Это MVP, без шлифовки. После него — реальные пользователи пробуют,
дыры обнаруживаются, дизайн уточняется.

---

## После v0.1 — последующие версии

**v0.2** — полная stdlib для backend (HTTP-сервер, простой Postgres-
клиент, JSON, SQL builder). Цель — реальный `audit.nv` (не упрощённый)
работает.

**v0.3** — supervisor + structured concurrency полностью. Cancellation.
Real-time graceful shutdown.

**v0.4** — LLVM codegen. `nova build` даёт нативный бинарь. Production
concurrent GC ([D6](../../spec/decisions/05-memory.md#d6)) с паузами <1ms (ZGC/Shenandoah/
MMTk-уровень). До этого — простой mark-and-sweep интерпретатора. Сюда же
— preemptive scheduling fiber'ов (compiler-вставленные safepoints, как
Go 1.14+).

**v0.5** — `region { ... }` блок и эффект `Realtime`
([D6](../../spec/decisions/05-memory.md#d6)). Effect polymorphism
([Q6](../../spec/open-questions.md)), LSP-сервер. Закрывается
[Q15](../../spec/open-questions.md)/[Q16](../../spec/open-questions.md) (enum с
числами, bitflags), если готов macros-механизм.

**v0.6** — SMT для контрактов (Z3), `@must_verify`/`@unverified`,
structured error messages для непроверенных контрактов
([D24](../../spec/decisions/09-tooling.md#d24)). Package manager
(базовый — path/git зависимости). Полная экосистема (registry,
publish/add/update команды) — см. [03-package-ecosystem-roadmap.md](03-package-ecosystem-roadmap.md),
после v2.0+.

**v0.8** — Time-travel debugging ([R8](../../spec/revolutionary.md)). Запись
handler-вызовов и replay.

**v0.9** — Schema evolution stdlib-паттерн
([Q13](../../spec/open-questions.md)). Финализация концепции.

**v1.0** — стабилизация, документация, готовность к публичному релизу.
После v1.0 — стабильность синтаксиса ([R5.4](../../spec/revolutionary.md),
[D10](../../spec/decisions/01-philosophy.md#d10)).

**v1.5+** — JIT, hot reload.

**v2.0+** — переписывание компилятора на Nova (self-hosting).
Bootstrap-Rust выкидывается. После self-host'а — package ecosystem
(self-hosted CLI, registry-протокол, запуск `nova-registry.org`,
первые опубликованные либы) — см.
[03-package-ecosystem-roadmap.md](03-package-ecosystem-roadmap.md).

---

## Открытые вопросы реализации перед началом v0.1

1. **Lexer/parser tooling.** Ручной recursive-descent vs `chumsky` vs
   `logos`+`lalrpop`? Склонность — `chumsky` (современный,
   error-recovery, golden-тесты легко).
2. **Fiber-runtime в bootstrap.** `corosensei` (cross-platform
   stackful) vs ручная реализация. Склонность — `corosensei`,
   быстрый старт. Связано с
   [Q3](../../spec/open-questions.md#q3-реализация-fiber-stack-для-async).
3. **Структура репо.** Monorepo: всё живёт в `nova-lang/`. Текущая
   раскладка:
   - `compiler-bootstrap/` — bootstrap-интерпретатор на Rust
     (минимальный, для self-hosting'а v2.0)
   - `compiler/` — будущий self-hosted компилятор на Nova
     (появится после того как bootstrap сможет запустить парсер
     написанный на Nova)
   - `nova_tests/` (top-level) — conformance-тесты языка, общие
     для обоих компиляторов
   - `spec/` — дизайн-документы

   Альтернатива «отдельный репозиторий `nova-compiler/`» не
   рассматривалась как обязательная — пока всё помещается в monorepo
   удобно для синхронизации spec ↔ implementation.
4. **Минимальная версия Rust.** Stable 1.75+, никаких nightly-фич —
   стабильность важнее.

Эти вопросы решаются на старте Этапа 1, не блокируют дизайн.
