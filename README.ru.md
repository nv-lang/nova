<div align="center">
  <img src="img/nova-logo.png" alt="Nova" width="120" />

  <h1>Nova</h1>

  <p><strong>Язык программирования для эпохи ИИ</strong></p>

  <p>
    <a href="https://nv-lang.org">Сайт</a> |
    <a href="docs/guide/quickstart.md">Быстрый старт</a> |
    <a href="spec/overview.md">Документация</a> |
    <a href="CONTRIBUTING.md">Участие в проекте</a>
  </p>

  <p><a href="README.md">English</a> | <strong>Русский</strong></p>

  <img src="img/og-image.png" alt="Nova — A language for the AI era" />
</div>

---

Nova компилируется в C, а затем в нативный бинарь — без VM, без
интерпретатора. Побочные эффекты каждой функции (`Db`, `Net`, `Io`, `Time`,
...) — часть её типа, проверяется компилятором, так что ревьюер видит, чего
касается функция, не читая её тело. Память по умолчанию управляется Boehm
GC; для ресурсов, которым нужна детерминированная очистка без пауз (файлы,
сокеты, блокировки) `consume`/владение даёт гарантированный `on_exit` при
выходе из scope, без GC в цепочке. Конкурентность — структурная (`spawn`,
`parallel for`, `supervised`) поверх M:N work-stealing файбер-планировщика —
без `async`/`await`, без раскола по «цвету функции». Стандартная библиотека
идёт с батарейками в комплекте: `std` (коллекции, IO, время, JSON, ...) плюс
отдельно версионируемые пакеты `net`, `tls`, `http` и `compress`.

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
// Declare an effect — a contract of operations, no fields
type Db effect {
    query(q Sql) -> []Row
    exec(q Sql)  -> ()
}

// Business logic: Db effect in the signature, implementation unknown
fn transfer(from u64, to u64, amount money) Db Fail -> () {
    ro src = Db.query(sql`SELECT * FROM accounts WHERE id = ${from}`)
    if src[0].balance < amount { throw InsufficientFunds }
    Db.exec(sql`UPDATE accounts SET balance = balance - ${amount} WHERE id = ${from}`)
    Db.exec(sql`UPDATE accounts SET balance = balance + ${amount} WHERE id = ${to}`)
}

// Production: real handler
fn main() Io Fail -> () =>
    with Db = postgres("postgres://...") {
        transfer(1, 2, 100)
    }

// Test: same code, in-memory handler, no mocks at all
test "transfer moves money" {
    ro mem = in_memory_db([
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
        ro resp = Http.get(url)!!
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

Вот этот паттерн вживую — флагманская демка
([examples/flagship/aggregator](examples/flagship/aggregator)): fan-out на
6 источников под одним общим дедлайном; опоздавшие по-настоящему
**отменяются**, а не бросаются; сервер репортит
`fibers_spawned/closed: 12/12` — ноль утечек как проверяемый факт:

![Nova flagship aggregator — parallel fan-out with real cancellation](docs/assets/aggregator-demo.gif)

Запустить самому: `docker run --rm -p 8187:8187` (образ — см.
[README демки](examples/flagship/aggregator/README.md)), или
дистиллированная версия на 30 строк:
[examples/mini_aggregator.nv](examples/mini_aggregator.nv).

### 3. Детерминированный random в тесте

```nova
fn pick_winner(participants []str) Random -> str =>
    participants[Random.range(0, participants.len())]

test "winner is deterministic with seed" {
    ro people = ["alice", "bob", "carol", "dave"]
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
        samples.map(|x| x * gain)      // no GC, no suspension
    }
```

Для perf-критичного кода компилятор использует **escape analysis** —
не утекающие значения остаются на стеке без аллокаций. Программист не
пишет ничего особого.

## Что выкинуто из обычных языков

- **Заголовочные файлы, `package`/`module` дуализм** — одно понятие
  модуля: модуль — это файл **или** папка peer-файлов с общим
  namespace (Go-style), объявляется `module parent.name`
  ([spec/decisions/07-modules.md](spec/decisions/07-modules.md), D29).
- **`null`** — только `Option[T]`.
- **Невидимые исключения** — только эффект `Fail[E]`, видимый в сигнатуре.
- **Никаких `async`/`await` keyword'ов** — suspension это ambient runtime, эффекты в типах: `Net`, `Io`, `Db`.
- **Перегрузка операторов на произвольные типы** — только стандартные через `@plus`, `@times`, ...
- **Макросы** — их нет вовсе; compile-time вычисления — `const` / `const fn`
  (типизированы и проверяются как обычный код — D199).
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
- [docs/guide/typed-pointers.md](docs/guide/typed-pointers.md) — каноничный синтаксис семейства `*T` (правило право-связывания V2/V3, ключевое слово `safe`, правила композиции модификаторов)
- [compiler-codegen/](compiler-codegen/) — компилятор Nova (Rust): парсер, type-checker, C-backend codegen, нативный runtime

## Экосистема

Компилятор, стандартная библиотека и спецификация живут в этом
репозитории. Всё, чему не обязательно быть в комплекте с компилятором, —
отдельный пакет, написанный на самой Nova и подтягиваемый через
`nova.lock.toml`:

| Пакет | Что это | Выпущено |
|---|---|---|
| [nova-tls](https://github.com/nv-lang/nova-tls) | TLS-клиент/сервер — рукопожатие, ALPN, SNI, горячая перезагрузка сертификатов | `v0.1.4` |
| [nova-http](https://github.com/nv-lang/nova-http) | HTTP/1.1-клиент + сервер — запрос/ответ, заголовки, URL, транспорт | `v0.1.1` |
| [nova-compress](https://github.com/nv-lang/nova-compress) | Кодеки `deflate` / `gzip` / `zlib` / `brotli` | `v0.1.1` |
| [nova-polaris](https://github.com/nv-lang/nova-polaris) | Polaris ⭐ — веб-фреймворк поверх HTTP-ядра: маршрутизатор, экстракторы, middleware, аутентификация, websocket'ы | тега пока нет |
| [nova-bignum](https://github.com/nv-lang/nova-bignum) | Числа произвольной точности на чистом Nova, без C-зависимостей | в работе |
| [tree-sitter-nova](https://github.com/nv-lang/tree-sitter-nova) | Грамматика tree-sitter для языка | `v0.1.0` |

## Статус

**v0.1.0 — первый публичный релиз.** Рано, но работает: компилятор
(парсер, type-checker, C-backend codegen), CLI (`nova build`/`check`/
`test`/`doc`), языковой сервер (`nova-lsp`) с расширением для VSCode, и
стандартная библиотека, покрывающая коллекции, IO, время, JSON, а также —
отдельными пакетами — сеть, TLS, HTTP и сжатие. Спецификация стабильна по
ключевым возможностям (эффекты, handler'ы, синтаксис, память,
конкурентность); некоторые участки (SMT-верификация контрактов за
пределами тривиальных случаев, конкурентный GC) всё ещё в roadmap. Один
компилятор:

- **compiler-codegen** — реализация на Rust с парсером, type-checker'ом и
  C-backend codegen'ом. Компилирует Nova в C через нативный runtime
  (эффекты, файберы, GC, каналы); обслуживает и тестовые прогоны (`test`),
  и нативную компиляцию (`build`).
- **nova-cli** — единственная пользовательская точка входа (`nova check`,
  `nova build`, `nova test`, `nova regen-runtime`). Точка входа
  интерпретатора `nova run` сейчас **не поддерживается** — Nova
  компилируется в C, поэтому используйте `nova build` (нативный бинарь)
  или `nova test`.
  `nova-codegen` — внутренний крейт-компилятор (движок, который `nova`
  вызывает изнутри) + несколько maintainer-only build-тулов
  (`unicode`-таблицы UCD, `compile` Nova→C, `dump-runtime`). **Для любой
  обычной работы — только `nova`** (nova-cli): `nova check / build / test /
  test-build <file> / lint / regen-runtime`. У `nova` есть свой
  `test-build` (один файл), так что вызывать `nova-codegen` напрямую не
  нужно; его `test-build` берёт ОДИН файл (директория → «read: os error 5»).

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
  `with X = h { body }`, `interrupt v`, `Effect[E, IRT]` first-class
  тип. `forbid`, `realtime` capability-блоки.
- Structured concurrency (D71/D75/D92): `spawn`, `supervised`,
  `supervised(cancel: tok)`, `parallel for`, channels, `select`.
- **M:N runtime** (Plans 44.1–44.7): work-stealing scheduler,
  per-worker libuv event loop, preemption (D103), GC_THREADS.
- Контракты (D24): `requires`/`ensures`/`old`/`result`/`invariant`/
  `reads`/`modifies`/`decreases`/`ghost let`/`assume`/`assert_static`.
  Bootstrap SMT через TrivialBackend (reflexive ensures); Z3 — milestone.
- `defer` + cleanup consume-области (D90/D188): `defer { ... }`
  выполняется при любом выходе из scope — включая `throw` и `panic` (в
  отличие от Rust `Drop` при `panic=abort`). Ресурс, связанный через
  `consume x = acquire() { ... }`, при выходе из scope запускает свой
  `Consumable.on_exit(outcome)`, получая `ScopeOutcome` (`Success` /
  `Failure` / `Panic`) для очистки, зависящей от исхода. (Более ранние
  формы `errdefer` / `okdefer` / `defer |result|` были ретрагированы —
  D189.)
- Boehm GC default с introspection API (`heap_size`, `live_count`,
  `collect`).

## Установка

Самый простой способ начать на Windows x64 — предсобранный релизный архив
(`nova.exe` + `nova-lsp.exe` + стандартная библиотека + C-рантайм, Rust
toolchain не нужен — нужен только C-компилятор): скачайте его со
[страницы релизов на GitHub](https://github.com/nv-lang/nova/releases),
распакуйте и выполните `. .\setup-env.ps1`. Полное руководство, включая
путь сборки из исходников на Linux и первую программу «Hello, Nova!»:
**[docs/guide/quickstart.md](docs/guide/quickstart.md)**.

## Сборка из исходников

Соберите `nova` CLI, затем используйте его для компиляции Nova-программ:

```sh
# build nova CLI (requires Rust + Cargo)
cd nova-cli && cargo build --release && cd ..

# compile a Nova file to a native binary, then run it
nova-cli/target/release/nova build path/to/hello.nv -o hello
./hello

# type-check only
nova-cli/target/release/nova check path/to/hello.nv
```

Pipeline двухступенчатый: `nova-codegen` (внутренний) производит `.c`,
нативный C-компилятор линкует его с runtime'ом (`nova_rt/`). `nova build`
оркестрирует это автоматически.

Ручной pipeline (без `nova` CLI):

```sh
cd compiler-codegen
cargo run -- compile path/to/hello.nv          # Nova → C
gcc path/to/hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
    -I. -o hello                                # C → binary
./hello
```

Полный guide, опции, известные ограничения:
[compiler-codegen/README.md](compiler-codegen/README.md).

## Первые шаги

Когда `nova` собрана, запустите демонстрационную программу — один
самодостаточный файл, который компилируется, запускается и тестируется без
дополнительной настройки:

```sh
# build it to a native binary, then run it (prints the cart totals)
nova-cli/target/release/nova build examples/getting_started.nv -o getting_started
./getting_started

# run its in-file tests (handler-swapped, no mocks)
nova-cli/target/release/nova test examples/getting_started.nv
```

[`examples/getting_started.nv`](examples/getting_started.nv) проходит по
ключевой части стандартной библиотеки 0.1 примерно в 150 строках с
комментариями:

- `fn main` + `println` — базовый hello;
- тип **record** с доступом по именованным полям;
- **sum-тип** + исчерпывающий `match`;
- цикл `for`, накапливающий результат по диапазону;
- **алгебраический эффект**, подставляемый через `with`-блок handler'ом в
  `main`, а затем **подменяемый на другой in-memory handler** внутри
  `test {}` — та же бизнес-логика проверяется без единого мока.

Последний пункт — главный тезис Nova: handler'ы — это тестовый шов, так что
тестам не нужен mocking-фреймворк.

## Запуск тестов

Соберите `nova` CLI, затем запустите полный набор тестов:

```sh
# build nova CLI (one-time, or after changes)
cd nova-cli && cargo build --release && cd ..

# run all tests
nova-cli/target/release/nova test
```

Частые флаги:

```sh
nova test --filter syntax/closure        # subset of tests
nova test --mode release                 # -O3 -flto compilation
nova test --toolchain clang              # force toolchain
nova test --timeout 60                   # timeout per test
nova test --format json                  # JSON events (one per line)
nova test --format junit > results.xml   # JUnit XML for CI parsers
nova test --retries 2                    # retry transient AV/race fails
nova test --rerun-failed                 # only failed-last-time
nova test --include-stdlib               # include std/* alongside nova_tests/*
```

Отладка одиночного теста (без walkdir, без параллельных накладных
расходов):

```sh
./compiler-codegen/target/debug/nova-codegen test-build nova_tests/basics/literals.nv \
    --toolchain clang --keep-artifacts
```

Настройка toolchain'а:
- **Windows:** `winget install LLVM.LLVM` (Clang, рекомендуется) +
  Visual Studio Build Tools (MSVC SDK + линкер, нужны и для Clang тоже).
- **Linux:** `apt install clang` или `dnf install clang`; GCC обычно уже
  установлен.
- **macOS:** `xcode-select --install` (Apple Clang).

Автоопределение выбирает сначала Clang, затем MSVC (Windows) или GCC
(Linux). Переопределить: `--toolchain clang|msvc|gcc` или через
переменные окружения (`NOVA_CLANG`, `NOVA_GCC`, `NOVA_VCVARS`).

Подробный гайд флагов test-runner, EXPECT-маркеры, troubleshooting:
[docs/dev/test-conventions.md](docs/dev/test-conventions.md).

## Документация (`nova doc`)

Генерация документации из `///` и `//!` doc-comment'ов, с doc-tests,
intra-doc-links, stability/deprecation, JSON Schema 2020-12 output:

```sh
nova doc src/api.nv                # Markdown to stdout
nova doc src/api.nv --format json  # JSON (D107 schema v1)
nova doc src/api.nv --test         # run doc-tests
nova doc src/api.nv --check        # validate (broken links, missing summaries)
```

Полный user guide: [docs/nova-doc.md](docs/nova-doc.md).

## SMT-верификация + настройка Z3

Nova включает статический верификатор контрактов (`requires`/`ensures`/`invariant`).
По умолчанию используется **TrivialBackend** (reflexive tautologies, constant folding) —
работает без внешних зависимостей. Для полноценной верификации нужен **Z3**.

### Без Z3 (по умолчанию)

Работает сразу после обычной сборки. Доказывает только рефлексивные
контракты и константные выражения. Z3-тесты автоматически SKIP.

```bash
cd nova-cli && cargo build --release
nova test nova_tests/contracts/
# PASS: 82  SKIP: 9 (z3-only)
```

### С Z3

**Шаг 1: установить Z3 через vcpkg** (один раз)

```bash
# Windows:
cd compiler-codegen
vcpkg install --triplet x64-windows-static --x-manifest-root=.

# Linux:
cd compiler-codegen
vcpkg install --triplet x64-linux --x-manifest-root=.

# macOS:
cd compiler-codegen
vcpkg install --triplet x64-osx --x-manifest-root=.
```

`vcpkg.json` уже содержит `z3` и `bdwgc` — обе зависимости устанавливаются
одной командой. Результат: `vcpkg_installed/<triplet>/lib/libz3.a`.

**Шаг 2: собрать с feature `z3-backend`**

```bash
cd nova-cli
cargo build --release --features z3-backend
```

**Шаг 3: запустить с Z3**

```bash
NOVA_SMT_BACKEND=z3 nova test nova_tests/contracts/
# PASS: 91  SKIP: 0
```

> `VCPKG_TRIPLET` переопределяет triplet если нужен нестандартный
> (например `arm64-linux`).

Подробнее: [docs/plans/33-contracts-implementation.md](docs/plans/33-contracts-implementation.md) — раздел «Z3 dev-setup».

## Поддержка редакторов

Плагины подсветки синтаксиса для нескольких редакторов лежат в
[editors/](editors/). Это TextMate / написанные вручную грамматики — только
подсветка синтаксиса. Семантические возможности (диагностика и т. д.)
приходят из отдельного языкового сервера, [`nova-lsp/`](nova-lsp/); его
подключение к этим редакторным плагинам в процессе.

| Редактор | Подкаталог | Заметки |
|---|---|---|
| VSCode / Cursor / VSCodium | [`editors/vscode/`](editors/vscode/) | TextMate grammar |
| Sublime Text / TextMate | [`editors/sublime/`](editors/sublime/) | переиспользует `.tmLanguage.json` от VSCode |
| Vim / Neovim | [`editors/vim/`](editors/vim/) | написанный вручную `syntax/nova.vim` |
| Emacs | [`editors/emacs/`](editors/emacs/) | major-mode `nova-mode.el` |

Полный обзор, команды установки для каждого редактора и roadmap (LSP,
tree-sitter, JetBrains) — см. [editors/README.md](editors/README.md).

## Зеркала

**GitHub — источник истины.** Issues и pull request'ы принимаются только
там. Два других хостинга — зеркала, синхронизируемые пушем во все три
сразу — изменение, сделанное прямо в зеркале, будет перезаписано следующим
пушем, так что не присылайте туда патчи.

| Хостинг | Организация | Роль |
|---|---|---|
| GitHub | [github.com/nv-lang](https://github.com/nv-lang) | **источник истины** — issues, pull request'ы, релизы |
| GitVerse | [gitverse.ru/nv-lang](https://gitverse.ru/nv-lang) | зеркало |
| SourceCraft | [sourcecraft.dev/nv-lang](https://sourcecraft.dev/nv-lang/repos) | зеркало |

Каждый репозиторий из раздела [Экосистема](#экосистема) существует на всех
трёх хостингах под тем же именем, так что любой из них можно клонировать,
если GitHub недоступен.

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
