# Test conventions

Практический guide для авторов и пользователей тестов Nova.
Нормативная спецификация D89 EXPECT-маркеров —
[spec/decisions/09-tooling.md](../spec/decisions/09-tooling.md#d89).
Test-runner — [Plan 24](plans/24-cross-platform-test-runner.md) +
[Plan 26](plans/26-test-runner-hardening.md).

---

## Как запускать тесты

### Quick start

**Windows (PowerShell):**
```powershell
cd compiler-codegen
cargo build
cd ..
.\run_tests.ps1
```

**Linux / macOS (bash):**
```bash
cd compiler-codegen && cargo build && cd ..
./run_tests.sh
```

Оба wrapper'а — тонкие shim'ы над `nova-codegen test-all`. Логика
runner'а (детект toolchain'а, EXPECT-маркеры, parallel scheduler,
per-test timeout, JSON output) живёт в Rust в
[compiler-codegen/src/test_runner.rs](../compiler-codegen/src/test_runner.rs).

### Параметры

Те же флаги работают в `.ps1` (с `PascalCase`) и `.sh` (с `--kebab-case`):

| .ps1 | .sh | Что |
|---|---|---|
| `-Filter <substr>` | `--filter <substr>` | Прогнать только тесты содержащие substring |
| `-IncludeStdlib` | `--include-stdlib` | Добавить `std/*.nv` к `nova_tests/*.nv` |
| `-Mode dev\|release` | `--mode dev\|release` | dev (default) или release с `-O3 -flto` |
| `-Toolchain auto\|clang\|msvc\|gcc` | `--toolchain ...` | Compiler. Default: auto (Clang → MSVC → GCC) |
| `-Timeout <secs>` | `--timeout <secs>` | Per-test timeout. Default 60 |
| `-Jobs <N>` | `--jobs <N>` | Parallel workers. 0 = num_cpus |
| `-Format text\|json\|tap` | `--format ...` | Output format. Default text |
| `-Verbose` / `-Quiet` | `--verbose` / `--quiet` | Verbosity |
| `-ResultsFile <path>` | `--results-file <path>` | Куда сохранить per-test JSON |
| `-RerunFailed` | `--rerun-failed` | Прогнать только тесты которые fail/timeout в results-file |
| `-KeepArtifacts` | `--keep-artifacts` | Не удалять .exe/.obj после прогона |

### Примеры

**Дефолтный прогон** (всё параллельно через Clang):
```powershell
.\run_tests.ps1
```

**Только подмножество** (TDD-loop):
```powershell
.\run_tests.ps1 -Filter syntax/closure
.\run_tests.ps1 -Filter "negative_capability/"
```

**Release-сборка** (с оптимизациями для perf-проверки):
```powershell
.\run_tests.ps1 -Mode release
```

**JSON output для CI**:
```bash
./run_tests.sh --format json --results-file ci-results.jsonl
```
Каждая строка — событие:
```json
{"event":"finished","test":"basics/literals","status":"pass","stage":"","elapsed_ms":234,"detail":""}
{"event":"summary","pass":140,"fail":1,"elapsed_ms":45678}
```

**TAP-13 output** (для legacy harnesses):
```bash
./run_tests.sh --format tap | tee results.tap
```

**TDD: перезапустить только упавшие**:
```powershell
.\run_tests.ps1                     # первый прогон — записывает results-file автоматически если -RerunFailed когда-то использовался
.\run_tests.ps1 -RerunFailed         # только бывшие fail-ы; намного быстрее
```

Или явно:
```bash
./run_tests.sh --results-file target/last-test-results.json
# правишь код...
./run_tests.sh --results-file target/last-test-results.json --rerun-failed
```

**Sequential** (для отладки race conditions):
```powershell
.\run_tests.ps1 -Jobs 1
```

**Долгие benchmark-тесты** (override default 60s timeout):
```powershell
.\run_tests.ps1 -Timeout 300 -Filter concurrency/sleep_leak
```

**Принудительный MSVC** (если хотите тестить под MSVC ABI):
```powershell
.\run_tests.ps1 -Toolchain msvc
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
- `--jobs 1 --timeout 60` — sequential (стабильно, но медленнее).
- **AV exclusion** для `target/`, `$TEMP/nova_tests/` — снимает
  bottleneck.
- В CI без Defender'а (Linux runners) parallel работает корректно.

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
- [run_tests.ps1](../run_tests.ps1) — текущая Windows + MSVC реализация.
- [nova_tests/negative_capability/](../nova_tests/negative_capability/) — примеры `EXPECT_COMPILE_ERROR`.
- [nova_tests/expected_runtime/](../nova_tests/expected_runtime/) — примеры остальных трёх маркеров.
