[English](README.md) | **Русский**

# Nova

```nova
fn process_order(o Order) Db Net Time Fail -> Receipt
```

Прочитав одну эту строчку, ты знаешь что функция:

- ходит в **базу данных** (`Db`)
- делает **сетевые запросы** (`Net`)
- читает **время** (`Time`) — значит её результат зависит от часов
- может **бросить ошибку** (`Fail`)
- и больше **ничего**: не пишет файлы, не читает stdin, не использует
  random — иначе это было бы в сигнатуре.

Это **алгебраические эффекты** — идея из академического языка Koka,
доведённая до прикладного состояния. Когда побочные действия видны в
типе, ревью становится локальным: можно проверить функцию не читая её
тело и тела всех её вызовов.

> **Главная ставка Nova:** код будут писать всё чаще LLM, а ревьюить —
> люди. Языки, спроектированные до AI-эпохи, оптимизированы под
> обратную пропорцию. Nova — первый язык, явно оптимизированный под
> пару «LLM пишет, человек ревьюит».

## Покажи код

### 1. Эффект → handler → тест без моков

```nova
// Объявляем эффект — контракт операций, без полей
type Db effect {
    query(q Sql) -> []Row
    exec(q Sql)  -> ()
}

// Бизнес-логика: эффект Db в сигнатуре, реализация неизвестна
fn transfer(from u64, to u64, amount money) Db Fail -> () {
    let src = Db.query(sql`SELECT * FROM accounts WHERE id = ${from}`)
    if src[0].balance < amount { throw InsufficientFunds }
    Db.exec(sql`UPDATE accounts SET balance = balance - ${amount} WHERE id = ${from}`)
    Db.exec(sql`UPDATE accounts SET balance = balance + ${amount} WHERE id = ${to}`)
}

// Production: реальный handler
fn main() Io Fail -> () =>
    with Db = postgres("postgres://...") {
        transfer(1, 2, 100)
    }

// Тест: тот же код, in-memory handler, никаких моков
test "transfer moves money" {
    let mem = in_memory_db([
        Account { id: 1, balance: 500 },
        Account { id: 2, balance: 0 },
    ])
    with Db = mem {
        transfer(1, 2, 100)
        assert(mem.get(1).balance == 400)
        assert(mem.get(2).balance == 100)
    }
}
```

Один и тот же `transfer` работает в проде и в тесте — потому что
реализация `Db` подставляется через `with`, а не зашита в код. Никакого
DI-фреймворка, никакой mock-библиотеки.

### 2. Параллелизм без `async`/`await`

```nova
fn check_all(urls []str) Net Fail -> []HealthStatus =>
    parallel for url in urls {
        let resp = Http.get(url)!!
        HealthStatus { url, code: resp.status, latency: resp.elapsed }
    }
```

Тип возврата — `[]HealthStatus`, не `Future<[]HealthStatus>`. **Цвета
функции не существует** — `Http.get` не объявлена async/sync, она
объявляет эффект `Net Fail` в сигнатуре, и этого достаточно.

`parallel for` — structured concurrency: все запросы летят параллельно,
scope ждёт всех, при ошибке хвост отменяется и `throw` пробрасывается
в caller через эффект `Fail` — обычный механизм обработки ошибок,
такой же как в синхронном коде. Та же `Http.get` работает и в обычном
цикле, и в `parallel for` — без изменений сигнатуры.

### 3. Детерминированный random в тесте

```nova
fn pick_winner(participants []str) Random -> str =>
    participants[Random.range(0, participants.len())]

test "winner is deterministic with seed" {
    let people = ["alice", "bob", "carol", "dave"]
    with Random = seed(42) {
        assert(pick_winner(people) == "carol")
        assert(pick_winner(people) == "alice")
    }
}
```

`Random` — обычный эффект. В проде — настоящий генератор; в тесте —
фиксированный seed, и результат **воспроизводим**. Никаких
`MockRandom`, никаких patch'ей. Тот же `pick_winner` работает в обоих
случаях.

### 4. Контракты — градиент от Go до F\*

```nova
fn withdraw(mut acc Account, amount money) Fail -> ()
    requires amount > 0
    requires acc.balance >= amount
    ensures  acc.balance == old(acc.balance) - amount
=>
    acc.balance -= amount
```

Контракты **опциональны**. Без них код работает как в Go. С ними
компилятор пытается доказать инварианты статически (как F\* / Dafny);
что не может доказать — превращает в runtime-проверку в debug-режиме
и убирает в release.

Один и тот же язык покрывает спектр от скрипта до критичного к
корректности кода — пишешь столько контрактов, сколько нужно.

## Что следует из одной идеи

| Возможность | Как получается из effect+handler |
|---|---|
| Тесты без моков | Подмена handler'а через `with` |
| Транзакции | Handler `Db` буферизует операции, коммитит в конце scope'а |
| Capability security | `forbid Net, Fs { ... }` запрещает эффект — compile error |
| Time-travel debugging | Запись handler-вызовов → replay |
| Erlang-style supervision | `supervised { spawn ... }` + restart-стратегия handler'а |
| LLM-безопасный код | Побочные действия видны в сигнатуре функции |

## Память: managed по умолчанию, real-time opt-in

**Программист пишет, GC работает.** Никаких префиксов памяти в обычном
коде. Циклы освобождаются автоматически. По умолчанию используется Boehm GC — консервативный,
паузы на практике до 16ms. Concurrent incremental GC — в roadmap v1.0
(Plan 25).

Для real-time зон (звук, торговля, embedded) — блок `realtime { ... }`.
Внутри него компилятор гарантирует отсутствие приостановок и GC-пауз;
нарушение — compile-time error:

```nova
fn map_audio(samples []f32, gain f32) -> []f32 =>
    realtime {
        samples.map(|x| x * gain)      // без GC, без suspension
    }
```

Для perf-критичного кода компилятор использует **escape analysis** —
не утекающие значения остаются на стеке без аллокаций. Программист не
пишет ничего особого.

## Что выкинуто из обычных языков

- **Заголовочные файлы, `package`/`module` дуализм** — один файл = модуль.
- **`null`** — только `Option[T]`.
- **Невидимые исключения** — только эффект `Fail[E]`, видимый в сигнатуре.
- **`async`/`await` keyword'ы** — suspension это ambient runtime, эффекты в типах: `Net`, `Io`, `Db`.
- **Перегрузка операторов на произвольные типы** — только стандартные через `@plus`, `@times`, ...
- **Макросы как препроцессор** — только typed comptime (как Zig).
- **Глобальное mutable state** — `mut` поля/параметры локально, или специализированные state-эффекты с именем (`Counter`, `Cache`).
- **DI через рефлексию** — зависимости в эффектах или параметрах.
- **Mock-библиотеки** — handler'ы из языка.

## Содержание

- [spec/overview.md](spec/overview.md) — главные идеи, что заимствует у кого, tooling
- [spec/revolutionary.md](spec/revolutionary.md) — **флагманские возможности**: effects + handlers, AI-first дизайн, контракты, time-travel debugging
- [spec/syntax.md](spec/syntax.md) — примеры синтаксиса
- [spec/effects.md](spec/effects.md) — система эффектов (базовое введение)
- [spec/open-questions.md](spec/open-questions.md) — нерешённые вопросы
- [spec/decisions/](spec/decisions/) — журнал дизайн-решений с эволюцией
- [compiler-codegen/](compiler-codegen/) — компилятор Nova (Rust): парсер, type-checker, treewalk-интерпретатор, C-backend codegen

## Статус

Активная разработка. Спецификация стабильна по ключевым областям
(эффекты, handlers, синтаксис, память, конкуренция). Один компилятор:

- **compiler-codegen** — Rust-реализация с парсером, type-checker'ом,
  treewalk-интерпретатором и C-backend codegen'ом. Компилирует Nova в C
  через нативный runtime (эффекты, файберы, GC, каналы); используется как
  для интерактивных запусков (`run`, `test`), так и для нативной
  компиляции (`build`).
- **nova-cli** — единая точка входа для пользователя (`nova check`,
  `nova build`, `nova run`, `nova test`, `nova regen-runtime`).
  `nova-codegen` остаётся как внутренний инструмент для IDE / CI /
  отладки codegen'а.

Что работает сегодня (bootstrap):

- Cross-file imports (`import X.Y.Z`, селективный `import X.{A, B}`,
  `export import X`, prelude auto-import) с DFS cycle detection.
- **Folder-modules** (D29 rev-3 / Plan 42): module = single-file `X.nv`
  ИЛИ folder `X/` с peer-файлами (Go-style). Все peers объявляют тот
  же `module parent.X` и share namespace. Internal helpers без `export`.
  Test isolation через `_test.nv` suffix. `internal/` directory для
  library boundaries. File-level `#forbid Net, Fs` capability
  attribute (Nova-unique).
- Эффекты + handlers (D61/D87): keyword'ы `effect`/`handler`,
  `with X = h { body }`, `interrupt v`, `Handler[E, IRT]` first-class
  тип. `forbid`, `realtime` capability-блоки.
- Structured concurrency (D71/D75/D92): `spawn`, `supervised`,
  `supervised(cancel: tok)`, `parallel for`, channels, `select`.
- **M:N runtime** (Plans 44.1–44.7): work-stealing scheduler,
  per-worker libuv event loop, preemption (D103), GC_THREADS.
- Контракты (D24): `requires`/`ensures`/`old`/`result`/`invariant`/
  `reads`/`modifies`/`decreases`/`ghost let`/`assume`/`assert_static`.
  Bootstrap SMT через TrivialBackend (reflexive ensures); Z3 — milestone.
- `defer` / `errdefer` cleanup (D90).
- Boehm GC default с introspection API (`heap_size`, `live_count`,
  `collect`).

## Сборка из исходников

Соберите `nova` CLI, затем используйте его для компиляции Nova-программ:

```sh
# собрать nova CLI (требуется Rust + Cargo)
cd nova-cli && cargo build --release && cd ..

# скомпилировать .nv в нативный бинарь
nova-cli/target/release/nova build path/to/hello.nv

# запустить через интерпретатор (без нативной компиляции)
nova-cli/target/release/nova run path/to/hello.nv

# только type-check
nova-cli/target/release/nova check path/to/hello.nv
```

Pipeline двухступенчатый: `nova-codegen` (внутренний) производит `.c`,
нативный C-компилятор линкует его с runtime'ом (`nova_rt/`). `nova build`
оркестрирует это автоматически.

Ручной pipeline (без `nova` CLI):

```sh
cd compiler-codegen
cargo run --release -- compile path/to/hello.nv          # Nova → C
gcc path/to/hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
    -I. -o hello                                          # C → бинарь
./hello
```

Полный guide, опции, известные ограничения:
[compiler-codegen/README.md](compiler-codegen/README.md).

## Запуск тестов

Соберите `nova` CLI, затем запустите полный набор тестов:

```sh
# собрать nova CLI (одноразово, или после изменений)
cd nova-cli && cargo build --release && cd ..

# все тесты
nova-cli/target/release/nova test
```

Частые флаги:

```sh
nova test --filter syntax/closure        # подмножество тестов
nova test --mode release                 # компиляция с -O3 -flto
nova test --toolchain clang              # принудительный toolchain
nova test --timeout 60                   # таймаут на тест
nova test --format json                  # JSON-события построчно
nova test --format junit > results.xml   # JUnit XML для CI
nova test --retries 2                    # повтор flaky AV/race fails
nova test --rerun-failed                 # только провалившиеся в last run
nova test --include-stdlib               # включить std/* помимо nova_tests/*
```

Подробный гайд флагов test-runner, EXPECT-маркеры, troubleshooting:
[docs/test-conventions.md](docs/test-conventions.md).

## Поддержка редакторов

Plugin'ы подсветки синтаксиса для нескольких редакторов лежат в
[editors/](editors/). Все — TextMate grammar / handcrafted, без
семантического анализа (LSP пока не реализован).

| Редактор | Подкаталог | Заметки |
|---|---|---|
| VSCode / Cursor / VSCodium | [`editors/vscode/`](editors/vscode/) | TextMate grammar |
| Sublime Text / TextMate | [`editors/sublime/`](editors/sublime/) | переиспользует `.tmLanguage.json` от VSCode |
| Vim / Neovim | [`editors/vim/`](editors/vim/) | handcrafted `syntax/nova.vim` |
| Emacs | [`editors/emacs/`](editors/emacs/) | major-mode `nova-mode.el` |

Полный обзор, команды установки для каждого редактора и roadmap
(LSP, tree-sitter, JetBrains): [editors/README.md](editors/README.md).

## Лицензия

Nova распространяется на условиях одной из двух лицензий по выбору
пользователя:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

`SPDX-License-Identifier: MIT OR Apache-2.0`

Документация и спецификация языка распространяются под
[CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).

### Контрибуции

Любой вклад, намеренно отправленный для включения в проект, по умолчанию
лицензируется как `MIT OR Apache-2.0`, без каких-либо дополнительных
условий — в соответствии с разделом 5 Apache License 2.0.

Подробности — в [CONTRIBUTING.md](CONTRIBUTING.md). Коротко: коммиты должны
быть подписаны DCO (`git commit -s`), это проверяется CI.
