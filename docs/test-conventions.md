# Test conventions

Практический guide для авторов и пользователей тестов Nova.
Нормативная спецификация D89 EXPECT-маркеров —
[spec/decisions/09-tooling.md](../spec/decisions/09-tooling.md#d89).
Test-runner — [Plan 24](plans/24-cross-platform-test-runner.md) +
[Plan 26](plans/26-test-runner-hardening.md).

---

## Как запускать тесты

### Quick start

```sh
# build nova CLI (one-time, or after changes to compiler)
cd nova-cli && cargo build && cd ..

# run all tests
nova-cli/target/debug/nova test
```

Логика runner'а (детект toolchain'а, EXPECT-маркеры, parallel scheduler,
per-test timeout, JSON output) живёт в Rust в
[compiler-codegen/src/test_runner.rs](../compiler-codegen/src/test_runner.rs).

### Параметры

| Флаг | Что |
|---|---|
| `--filter <substr>` | Прогнать только тесты содержащие substring |
| `--include-stdlib` | Добавить `std/*.nv` к `nova_tests/*.nv` |
| `--mode dev\|release` | dev (default) или release с `-O3 -flto` |
| `--toolchain auto\|clang\|msvc\|gcc` | Compiler. Default: auto (Clang → MSVC → GCC) |
| `--timeout <secs>` | Per-test timeout. Default 60 |
| `--jobs <N>` | Parallel workers. 0 = num_cpus |
| `--format text\|json\|tap\|junit` | Output format. Default text |
| `--verbose` / `--quiet` | Verbosity |
| `--results-file <path>` | Куда сохранить per-test JSON |
| `--rerun-failed` | Прогнать только тесты которые fail/timeout в results-file |
| `--retries <N>` | Retry transient (AV-race) fails. CI default 2 |
| `--keep-artifacts` | Не удалять .exe/.obj после прогона |

### Примеры

**Дефолтный прогон** (всё параллельно через Clang):
```sh
nova test
```

**Только подмножество** (TDD-loop):
```sh
nova test --filter syntax/closure
nova test --filter "negative_capability/"
```

**Release-сборка** (с оптимизациями для perf-проверки):
```sh
nova test --mode release
```

**JSON output для custom CI parser'ов**:
```sh
nova test --format json --results-file ci-results.jsonl
```

**JUnit XML для CI** (GitHub Actions / GitLab CI / Jenkins / Azure DevOps):
```sh
nova test --format junit --retries 2 > test-results.xml
```
Стандартный JUnit XML schema — нативно парсится всеми mainstream CI:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nova_tests" tests="143" failures="0" time="91.082">
  <testsuite tests="143" failures="0" time="91.082" timestamp="2026-05-11T12:42:47">
    <testcase classname="basics" name="literals" time="0.234"/>
    <testcase classname="syntax" name="bad_test" time="0.514">
      <failure type="expectation" message="expected exit 42, got 0"/>
    </testcase>
  </testsuite>
</testsuites>
```
Каждая строка — событие:
```json
{"event":"finished","test":"basics/literals","status":"pass","stage":"","elapsed_ms":234,"detail":""}
{"event":"summary","pass":140,"fail":1,"elapsed_ms":45678}
```

**TAP-13 output** (для legacy harnesses):
```sh
nova test --format tap | tee results.tap
```

**TDD: перезапустить только упавшие**:
```sh
nova test                     # первый прогон — результаты пишутся в target/last-test-results.json
nova test --rerun-failed       # только бывшие fail-ы; намного быстрее
```

Явный путь к results-file:
```sh
nova test --results-file target/last-test-results.json
# правишь код...
nova test --results-file target/last-test-results.json --rerun-failed
```

**Sequential** (для отладки race conditions):
```sh
nova test --jobs 1
```

**Долгие benchmark-тесты** (override default 60s timeout):
```sh
nova test --timeout 300 --filter concurrency/sleep_leak
```

**Принудительный MSVC** (если хотите тестить под MSVC ABI):
```sh
nova test --toolchain msvc
```

### Запуск одного теста

Для отладки удобно вызывать `nova-codegen test-build` напрямую — он
собирает + запускает один `.nv` файл без overhead'а walkdir/parallel:

```powershell
.\compiler-codegen\target\debug\nova-codegen.exe test-build .\nova_tests\basics\literals.nv `
    --toolchain clang --timeout 30 --keep-artifacts
```

`--keep-artifacts` оставляет `.exe`/`.obj` в `$TEMP/nova_tests/t-<hash>/`
для пост-mortem отладки. Без флага артефакты удаляются после прогона.

### Toolchain setup

**Windows:**
- **Clang (recommended):** `winget install LLVM.LLVM`
- **MSVC fallback:** установить Visual Studio Build Tools (нужен и
  для Clang — даёт MSVC SDK headers + linker).

**Linux:**
- `apt install clang` или `dnf install clang` (Ubuntu/Fedora).
- GCC обычно уже установлен.

**macOS:**
- Clang идёт с Xcode CLI tools: `xcode-select --install`.

Env-override paths:
- `NOVA_CLANG` — путь к `clang.exe`/`clang`.
- `NOVA_GCC` — путь к `gcc`.
- `NOVA_VCVARS` — путь к `vcvars64.bat` (Windows).
- `NOVA_CODEGEN` — путь к `nova-codegen.exe` (обычно target/debug).
- `NOVA_MARCH_NATIVE=1` — `-march=native` вместо `-march=x86-64-v3`
  для release-сборки (не переносится между CPU).

### Известные limitations на Windows

При `--jobs > 1` под активным **Windows Defender** возможны
transient `lld-link: cannot open output file` ошибки — AV держит
handle на свежесгенерированном `.exe` пока соседний worker пытается
linkать. Workarounds:
- **`--retries 2`** (Plan 26 Ф.12) — transient AV/race fails автоматически
  ретраятся. Real test fails не ретраятся (только classifier по
  error-сигнатурам). Это recommended setting для CI.
- `--jobs 1 --timeout 60` — sequential (стабильно, но медленнее).
- **AV exclusion** для `target/`, `$TEMP/nova_tests/` — снимает
  bottleneck полностью.
- В CI без Defender'а (Linux runners) parallel работает корректно.

### Graceful cancellation

`Ctrl+C` во время прогона: worker'ы graceful exit на следующем тесте
(не забирают новые jobs из queue). Уже запущенные child-процессы
получат KILL по `--timeout`. Summary показывает что было выполнено
до cancel'а.

См. [Plan 26 retro](plans/26-test-runner-hardening.md) для деталей.

---

## Зачем маркеры

Обычные тесты в `nova_tests/` пишутся через `test "name" { ... }` —
test-runner запускает блок и проверяет что не упал. Это покрывает
**positive paths** — программа работает как ожидается.

Но иногда нужно проверить **что-то должно упасть** — определённым
способом, с конкретным сообщением, exit-кодом или выводом. Для этого
есть `EXPECT_*` маркеры.

Маркер — обычный комментарий в первых 30 строках `.nv`-файла. Test-
runner его читает, **переворачивает** обычную логику pass/fail.

---

## 5 стандартных маркеров

### 1. `EXPECT_COMPILE_ERROR <pattern>`

Проверяет, что **codegen упадёт** с error, чьё сообщение содержит
`<pattern>` (substring match, case-sensitive).

**Когда использовать:**
- Type-check errors (duplicate definition, type mismatch).
- Codegen errors (ambiguous overload, no matching overload).
- Capability violations (forbid + call с запрещённым эффектом).

**Пример:**

```nova
// EXPECT_COMPILE_ERROR duplicate definition

module nova_tests.negative_capability.overload_dup

fn process(n int) -> str { "first" }
fn process(n int) -> str { "second" }    // duplicate sig
```

**Поведение runner'а:**
- Codegen вызван, exit code должен быть **ненулевой**.
- Stdout/stderr codegen'а должны содержать `duplicate definition`.
- Файл **не компилируется** в exe (предполагается невалидный код).

**Если codegen прошёл успешно** — `NEG-NO-ERROR` (test fails).
**Если упал, но без pattern в сообщении** — `NEG-WRONG-MSG`.

---

### 2. `EXPECT_RUNTIME_PANIC <pattern>`

Проверяет, что exe **скомпилируется и запустится**, но **упадёт с
panic**, чьё сообщение содержит `<pattern>`.

**Когда использовать:**
- Тесты `panic("...")` в коде.
- Runtime errors (out-of-bounds, division by zero, при condition).
- Assertion failures.

**Пример:**

```nova
// EXPECT_RUNTIME_PANIC explicit panic

module nova_tests.expected_runtime.panic_main

fn main() Io -> () {
    panic("explicit panic")
}
```

**Поведение runner'а:**
- Codegen + cl.exe компиляция должны пройти.
- Exe запускается, exit code должен быть **ненулевой**.
- Stdout/stderr должны содержать `explicit panic`.

**Если exe вернул exit code 0** — `NEG-NO-PANIC`.
**Если упал, но без pattern** — `NEG-WRONG-PANIC`.

---

### 3. `EXPECT_EXIT_CODE <N>`

Проверяет, что exe завершится с **конкретным** exit-кодом.

**Когда использовать:**
- Тесты `exit(N, "...")` функции.
- CLI-программы с конкретными exit-кодами для shell-скриптов.
- Различение нескольких error-вариантов через коды.

**Пример:**

```nova
// EXPECT_EXIT_CODE 42

module nova_tests.expected_runtime.exit_code_42

fn main() Io -> () {
    exit(42, "intentional exit with code 42")
}
```

**Поведение runner'а:**
- Codegen + компиляция проходят.
- Exe запускается, exit code должен быть **ровно `N`**.

**Если exit code другой** — `NEG-WRONG-EXIT` с указанием
ожидаемого и фактического.

---

### 4. `EXPECT_STDOUT <pattern>`

Проверяет, что **только stdout** (не stderr) exe содержит `<pattern>`
(substring).

**Когда использовать:**
- Golden-file тесты для format/print-логики.
- Проверка что program вывела ожидаемое сообщение в stdout.
- Smoke-тесты hello-world уровня.

**Пример:**

```nova
// EXPECT_STDOUT hello world

module nova_tests.expected_runtime.stdout_hello

fn main() Io -> () {
    println("hello world from Nova")
}
```

**Поведение runner'а:**
- Codegen + компиляция проходят.
- Exe запускается (любой exit code OK — тест на **вывод**, не на код).
- **Только stdout** должен содержать `hello world` (substring match).
  Если pattern в stderr — тест **не** проходит. Для проверки stderr —
  отдельный маркер `EXPECT_STDERR`.

**Если pattern не найден** — `NEG-WRONG-STDOUT`.

---

### 5. `EXPECT_STDERR <pattern>`

Проверяет, что **только stderr** (не stdout) exe содержит `<pattern>`
(substring).

**Когда использовать:**
- Проверка warning'ов / diagnostic-сообщений в stderr.
- Тесты `panic(msg)` без жёсткой привязки к exit-коду
  (`EXPECT_RUNTIME_PANIC` дополнительно требует ненулевой exit).
- Проверка `exit(N, msg)` сообщения (вместе с `EXPECT_EXIT_CODE`
  это сделать нельзя — один маркер на файл, поэтому используют один
  или другой).
- Проверка output `Logger`-handler'ов, пишущих в stderr.

**Пример:**

```nova
// EXPECT_STDERR custom stderr message

module nova_tests.expected_runtime.stderr_panic

fn main() Io -> () {
    panic("custom stderr message")
}
```

**Поведение runner'а:**
- Codegen + компиляция проходят.
- Exe запускается (любой exit code OK — тест на **вывод**, не на код).
- **Только stderr** должен содержать pattern. Если в stdout — тест
  не проходит.

**Если pattern не найден** — `NEG-WRONG-STDERR`.

**Отличие от `EXPECT_RUNTIME_PANIC`:**
- `EXPECT_RUNTIME_PANIC` требует **ненулевой exit code** + pattern
  в любом потоке (panic-сообщение).
- `EXPECT_STDERR` принимает **любой exit code**, но pattern должен
  быть **именно в stderr**.

Для panic-тестов обычно используют `EXPECT_RUNTIME_PANIC` (он
проверяет два инварианта). `EXPECT_STDERR` — когда нужна только
проверка вывода, без требования к exit code'у.

---

## Правила

### Один маркер на файл

Маркеры **взаимоисключающие**. Если хочешь проверить несколько
ошибок — **отдельные файлы**. Это сознательно: один файл = один
test scenario, проще читать и точнее диагностировать падения.

### Substring, не regex

Pattern — **substring**, искать в выводе как есть. Не regex, никаких
escape'ов.

```
// EXPECT_COMPILE_ERROR duplicate definition `process`
//                                            ^^^^^^^^^
//                          backticks буквальные, не экранируются
```

### Case-sensitive

`EXPECT_COMPILE_ERROR Foo` НЕ сматчит сообщение «foo not found».

### Один pattern на одну строку

Multi-line patterns не поддерживаются. Runner склеивает вывод в
одну строку через пробел перед matching.

---

## Куда класть тесты

| Тип теста | Категория |
|---|---|
| Обычные positive | `nova_tests/<group>/` (basics, syntax, types, runtime, ...) |
| `EXPECT_COMPILE_ERROR` | `nova_tests/negative_capability/` |
| `EXPECT_RUNTIME_PANIC`, `EXPECT_EXIT_CODE`, `EXPECT_STDOUT` | `nova_tests/expected_runtime/` |

(Категории — **convention**, не enforced. Runner ходит по всем
файлам в `nova_tests/`.)

---

## Расширения для своего проекта

Если в твоём проекте появился use-case для нового маркера (например
`EXPECT_LINT_WARNING`) — **сначала** проверь, не покроет ли существующий
один из 4 стандартных. Если нужен новый — обсуди с авторами Nova
(возможно, маркер должен быть стандартизирован через D-block).

**Custom-маркеры** в одном проекте — допустимы, но **не используй
имена `EXPECT_*`** — они зарезервированы. Используй project-specific
префикс: `MYPROJ_EXPECT_*` или `INTERNAL_*`.

---

## Прецеденты в других языках

| Язык | Маркер | Похоже |
|---|---|---|
| **Rust** (`compiletest`) | `//~ ERROR pattern` | Nova `EXPECT_COMPILE_ERROR` |
| **Swift** (`utils/test`) | `// expected-error {{pattern}}` | то же |
| **Go** (`errorcheck`) | `// ERROR pattern` | то же |
| **TypeScript** | `// @ts-expect-error` | другой подход — атрибут языка |
| **LLVM lit** | `// CHECK: pattern` | универсальный для тестов tooling'а |

Nova ближе к Rust/Swift/Go — comment-маркер на уровне test-runner'а.

---

## Ссылки

- [D89 в spec/decisions/09-tooling.md](../spec/decisions/09-tooling.md#d89) — нормативная спецификация.
- [nova-cli/src/main.rs](../nova-cli/src/main.rs) — `nova test` CLI entry point.
- [compiler-codegen/src/test_runner.rs](../compiler-codegen/src/test_runner.rs) — runner implementation.
- [nova_tests/negative_capability/](../nova_tests/negative_capability/) — примеры `EXPECT_COMPILE_ERROR`.
- [nova_tests/expected_runtime/](../nova_tests/expected_runtime/) — примеры остальных трёх маркеров.


---

## Fixture directories (Plan 55 Ф.8, 2026-05-16)

Не каждый `.nv` файл в `nova_tests/` — это runnable test. Иногда нужны
**input fixtures** для tooling (Plan 45 `nova doc` ingestion samples,
intermediate doc-pipeline data). Такие файлы:
- часто **не имеют** `main`/`test "..."` блоков;
- не должны компилироваться как standalone tests (CC-FAIL `undefined
  symbol: nova_fn_main_impl`);
- доступны через explicit `nova check <path>` или integration tests
  (cargo tests).

### Convention

`nova test` (test discovery walker) **исключает**:

1. **Любую директорию с именем `fixtures`** — стандартная конвенция
   (параллель с Rust `tests/data/`, Python `fixtures/`).
2. **Любую директорию с sentinel-файлом `_fixture.toml`** — explicit
   override для случаев когда имя `fixtures` нельзя/нежелательно.

```
nova_tests/
├── doc/
│   ├── f24_*_positive.nv         ← обычные tests, run'аются
│   └── fixtures/                 ← skipped (по имени)
│       ├── basic/sample.nv
│       └── ...
└── plan42/
    └── custom_data/              ← обычная папка, run'ается
        └── _fixture.toml         ← теперь skipped (через sentinel)
```

### Доступ к fixtures извне

- **Type-check:** `nova check nova_tests/doc/fixtures/basic/sample.nv`
  работает напрямую (path-based, не через discovery).
- **Plan 45 `nova doc`:** ingestion pipeline принимает explicit paths.
- **Integration tests:** cargo tests в `compiler-codegen/tests/`
  могут load fixtures как Rust string includes.

### Параллель с другими языками

| Язык | Convention |
|---|---|
| **Rust** | `tests/data/`, `tests/fixtures/` (cargo test игнорирует sub-dirs без `mod` декларации) |
| **Python** | `fixtures/` (pytest skips если нет `test_*.py` или `*_test.py`) |
| **Go** | `testdata/` (стандартный exclude в go test) |
| **JS/TS** | `__fixtures__/`, `fixtures/` |
| **Nova** | `fixtures/` ИЛИ `_fixture.toml` sentinel |

## Флаки-тесты — политика (Plan 92, 2026-05-22)

**Флаки-тест** — тест, который проходит/падает недетерминированно при
неизменном коде. Это **производственная проблема, не косметика**: он
роняет «зелёный» прогон на ровном месте и **маскирует настоящие
регрессии** — при `1 FAIL` нельзя без ручного разбора отличить «флаки»
от реального бага. Молча мигающий тест эродирует доверие ко всему
прогону.

### Правило: тест проверяет только детерминированный контракт

`assert` в тесте должен проверять **гарантированное** свойство —
то, что обязано выполняться при любом валидном расписании потоков/
fiber'ов и любой нагрузке. Нельзя `assert`'ить наблюдаемое следствие
**эвристики планировщика**.

Канонический анти-паттерн (Plan 92 Ф.0 — `mn_runtime_actual_workload`):

```nova
// ПЛОХО: проверяет, что work-stealing РАСПРЕДЕЛИЛ 16 fibers по worker'ам.
//   Под CPU-starvation worker-потоков ОС один worker законно
//   выполняет всё → on_w0 == 16 → assert падает (66% под нагрузкой).
//   work-stealing — оппортунистичная эвристика, НЕ контракт.
assert(on_w0 < 16)
```

Что **детерминировано** и проверяемо у того же теста:

```nova
// ХОРОШО: все 16 fibers ОТРАБОТАЛИ на worker-пуле (worker_id >= 0,
//   а не -1 = main-thread). Это контракт M:N-рантайма — держится
//   при любой нагрузке.
assert(total_ran == 16)
```

### Вероятностные свойства — проверять статистически, не одним сэмплом

Если свойство по природе вероятностное (распределение work-stealing'а,
наличие параллелизма), оно проверяется **bounded-sampling**'ом, а не
одним наблюдением:

- прогнать свойство до `N` независимых раз;
- pass — если оно наблюдалось хотя бы раз (рабочая система даёт его
  с высокой вероятностью за попытку → `P(false-fail) = (1-p)^N`
  выбором `N` сводится к ничтожной);
- настоящая поломка (свойство недостижимо) → не наблюдается **ни
  разу** за `N` попыток → детерминированный fail.

Это корректный метод проверки вероятностного свойства (ср. Go
`runtime.TestGoroutineParallelism`), **не упрощение**: реальный баг
остаётся loud-детектируемым.

### Чего НЕ делать

- **Не** «лечить» флаки молчаливым `retry`. Retry **маскирует** —
  допустим лишь как явная временная quarantine-мера с tracking-планом,
  не как тихий дефолт.
- **Не** оставлять флаки-тест мигать. Либо чинить root cause (привести
  `assert` к детерминированному контракту / статистике), либо —
  если корень в реальной гонке рантайма — это soundness-дефект,
  эскалировать отдельным планом, тест в явный quarantine со ссылкой.

### Параллель с индустрией

| Экосистема | Практика |
|---|---|
| **Go** | `-race` для гонок; параллелизм-тесты структурно «complete-or-hang» + gated; флаки → fix или `t.Skip` + issue |
| **Rust** | детерминизм по умолчанию; флаки → `#[ignore]` + tracking issue; гонки — `loom`/`tsan` |
| **Industry** | флаки — либо root-cause fix, либо quarantine с tracking; **никогда** не «молча мигает» |
| **Nova** | тест проверяет детерминированный контракт; вероятностное — bounded-sampling; quarantine только явный, с планом |
