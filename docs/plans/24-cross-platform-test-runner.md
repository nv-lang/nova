# План 24: cross-platform test runner

**Статус:** активный, в работе.
**Дата создания:** 2026-05-11.
**Тип:** инфраструктурный (test pipeline). Делает Nova сборку и тесты
доступными на Linux/macOS, не только Windows.

---

## Тезис

Сейчас `run_tests.ps1` — единственный test-runner. Windows-only (PowerShell + vswhere.exe + vcvars64.bat для MSVC SDK). Linux-разработчик не может запустить тесты без полной portировки.

**Решение:** вынести логику runner'а в Rust subcommand `nova-codegen test-all`, оставить `.ps1` / `.sh` тонкими wrapper'ами. Один источник правды для EXPECT-маркеров, toolchain-детекта, mode-flag'ов.

Аналогично пути cargo: начинался как Makefile-обёртки вокруг rustc, постепенно поглотил build/test/run logic. Делаем то же — Nova CLI поглощает test-runner.

---

## Текущее состояние

`run_tests.ps1` (~320 строк PowerShell) содержит:
- Детект Clang/MSVC через `Get-Command` + `vswhere.exe`.
- Парсинг D89 EXPECT-маркеров (regex по первым 30 строкам .nv).
- Вызов `nova-codegen compile X.nv` → `X.c`.
- Сборка `clang.exe` или `cl.exe` через `cmd /c "vcvars64 && cmd"`.
- Запуск `.exe`, чтение stdout/stderr, сравнение с EXPECT.
- Аккумуляция PASS/FAIL.

Дублировать всё это в bash — плохо: drift между скриптами неизбежен (один обновили, другой забыли). Логика должна быть в одном месте.

---

## Цель

После Plan 24:

- ✅ `nova-codegen test-build <file.nv>` — собирает один тест и проверяет EXPECT-маркер.
- ✅ `nova-codegen test-all [--filter X] [--mode dev|release] [--toolchain auto|clang|msvc|gcc] [--include-stdlib]` — рекурсивно прогоняет все .nv в `nova_tests/` (+ `std/` если `--include-stdlib`).
- ✅ `run_tests.ps1` — тонкая обёртка ~30 строк, вызывает `nova-codegen test-all`. PowerShell-specific только: PSScriptRoot, vcvars detection для Windows MSVC.
- ✅ `run_tests.sh` — новый bash-wrapper ~30 строк, Linux/macOS. Не нуждается в vcvars (clang/gcc на Linux находят headers сами).
- ✅ Cross-platform path handling через `std::path::Path` / `PathBuf` в Rust.
- ⏸️ Реальная проверка на Linux/macOS — отложено (нет Linux-машины); ставим инфраструктуру слепо, проверим когда появится access.

---

## Не цель

- **Linux CI** — отдельная задача (нужны GitHub Actions/runner setup, Docker config). Делаем когда test-runner созрел.
- **Удаление `.ps1` / переход на pwsh-only** — оставляем оба скрипта; pwsh-only был бы лишним требованием для Linux-разработчиков.
- **Стандартизация EXPECT-маркеров за пределами D89** — оставляем существующие 5 (COMPILE_ERROR, RUNTIME_PANIC, EXIT_CODE, STDOUT, STDERR).
- **Замена `cargo test`** в `compiler-codegen/` — это Rust lib-тесты, отдельная подсистема.
- **Build modes для `cargo build`** — Rust сторона остаётся как есть.

---

## Что делаем

### Ф.1 — `nova-codegen test-build <file.nv>`

Новая subcommand в `compiler-codegen/src/main.rs`. Делает полный pipeline для **одного** теста.

**Параметры:**
```
nova-codegen test-build <file.nv>
    [--mode dev|release]                    # default: dev
    [--toolchain auto|clang|msvc|gcc]       # default: auto
    [--vcvars <path>]                       # Windows: путь к vcvars64.bat
    [--keep-artifacts]                      # не удалять .c/.exe/.obj
    [--tmp-dir <path>]                      # default: $TEMP/nova_tests
    [--cg-include <path>]                   # путь к nova_rt headers
    [--rt-dir <path>]                       # путь к nova_rt/*.c
```

**Логика:**

1. Прочитать file, parse первых 30 строк на EXPECT-маркеры (см. ниже).
2. Codegen .nv → .c (через `CEmitter::emit_module`).
3. Если EXPECT_COMPILE_ERROR — проверить pattern в codegen-error, exit.
4. Иначе вызвать C-компилятор через выбранный toolchain (см. Ф.1.2 toolchain detect).
5. Запустить .exe, разделить stdout/stderr через `std::process::Command`.
6. Сравнить результат с EXPECT-маркером (или с default exit=0).
7. Print `PASS <display>` или `FAIL <display>  # <detail>`. Exit code: 0 если PASS, 1 если FAIL.

#### Ф.1.1 — EXPECT-маркеры (D89)

Парсинг — Rust regex по первым 30 строкам. 5 маркеров (см. D89):

```
// EXPECT_COMPILE_ERROR <pattern>    — codegen error содержит pattern
// EXPECT_RUNTIME_PANIC <pattern>    — exit≠0 + stderr содержит pattern
// EXPECT_EXIT_CODE <N>              — exit == N
// EXPECT_STDOUT <pattern>           — stdout содержит pattern
// EXPECT_STDERR <pattern>           — stderr содержит pattern
```

Один маркер на файл (берём первый). Pattern — substring (escape regex chars).

Структура:
```rust
enum ExpectMarker {
    CompileError(String),
    RuntimePanic(String),
    ExitCode(i32),
    Stdout(String),
    Stderr(String),
}
fn parse_expect(src: &str) -> Option<ExpectMarker> { ... }
```

#### Ф.1.2 — Toolchain detection

Платформенно-зависимый детект, абстракция:

```rust
enum Toolchain { Clang(PathBuf), Msvc(PathBuf /* vcvars */), Gcc(PathBuf) }

fn detect_toolchain(prefer: Preference) -> Result<Toolchain> { ... }
```

**Windows:**
- Clang: `$env:NOVA_CLANG`, `C:\Program Files\LLVM\bin\clang.exe`, `Get-Command clang.exe`.
- MSVC: vcvars64.bat через `$env:NOVA_VCVARS`, иначе `vswhere.exe`.
- GCC: `Get-Command gcc.exe` (MSYS2/Cygwin/MinGW).

**Linux/macOS:**
- Clang: `/usr/bin/clang`, `which clang`, `$NOVA_CLANG`.
- GCC: `/usr/bin/gcc`, `which gcc`.
- MSVC: недоступен; ошибка при явном `--toolchain msvc`.

**Auto-preference:**
- Windows: Clang > MSVC > GCC.
- Linux: Clang > GCC.
- macOS: Clang (системный) > Apple Clang.

#### Ф.1.3 — Build flags

```rust
fn build_flags(tc: &Toolchain, mode: Mode) -> Vec<String> { ... }
```

| Toolchain | dev | release |
|---|---|---|
| Clang/GCC | `-O0 -g -Wno-everything` | `-O3 -flto -march=$march -DNDEBUG -Wno-everything` |
| MSVC | `/Od /Zi` | `/O2 /DNDEBUG` |

`$march` — `x86-64-v3` по умолчанию (Haswell+, 2013+), `native` если env `NOVA_MARCH_NATIVE=1`.

#### Ф.1.4 — Invocation

```rust
fn build_command(tc: &Toolchain, mode: Mode, c_file: &Path, exe: &Path, ...) -> Command { ... }
```

Для Clang на Windows нужен vcvars64 — invoke через `cmd /c "vcvars64 && clang ..."`. На Linux Clang/GCC находят headers через стандартный path lookup, vcvars не нужен.

Для cross-platform invocation использовать `std::process::Command` с `.arg()` для каждого параметра — не shell-string parsing.

### Ф.2 — `nova-codegen test-all`

Главная команда — рекурсивный прогон.

**Параметры:**
```
nova-codegen test-all
    [--tests-dir <path>]                    # default: ./nova_tests
    [--stdlib-dir <path>]                   # default: ./std
    [--include-stdlib]                      # включить std/ файлы
    [--filter <substr>]                     # фильтр по display-name
    [--mode dev|release]
    [--toolchain auto|clang|msvc|gcc]
    [--vcvars <path>]
```

**Логика:**

1. Найти все `.nv` файлы в `tests-dir` (walkdir).
2. Если `--include-stdlib` — добавить `stdlib-dir`.
3. Сортировать по relative path для стабильного порядка.
4. Для каждого .nv — вызов `test_build_one()` (внутренняя функция, не subprocess).
5. Аккумулировать результаты: `Vec<(display: String, status: Status, detail: String)>`.
6. Print таблицу как в `.ps1` («PASS <name>», «FAIL <name>  # <detail>»).
7. Print summary: `PASS: N  FAIL: M`.
8. Exit code: 0 если все PASS, 1 если хоть один FAIL.

**Цветной output** через `console`/`colored` crate (опционально; fallback на ANSI-escape кодов или `is-terminal`-check).

### Ф.3 — Thin wrappers

#### `run_tests.ps1` (упрощён)

```powershell
param(
    [string]$Filter = "",
    [switch]$IncludeStdlib,
    [ValidateSet("dev", "release")]
    [string]$Mode = "dev",
    [ValidateSet("auto", "clang", "msvc", "gcc")]
    [string]$Toolchain = "auto"
)

$ErrorActionPreference = "Stop"
$repo_root = $PSScriptRoot
$codegen = if ($env:NOVA_CODEGEN) { $env:NOVA_CODEGEN } else {
    Join-Path $repo_root "compiler-codegen\target\debug\nova-codegen.exe"
}

if (-not (Test-Path $codegen)) {
    Write-Host "ERROR: nova-codegen not found at $codegen. Run: cd compiler-codegen; cargo build" -ForegroundColor Red
    exit 1
}

# vcvars detection для Windows — передаём в test-all
$vcvars = $env:NOVA_VCVARS
if (-not $vcvars) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vcvars = & $vswhere -latest -products * `
            -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
            -find "VC\Auxiliary\Build\vcvars64.bat" | Select-Object -First 1
    }
}

$cli_args = @(
    "test-all",
    "--tests-dir", (Join-Path $repo_root "nova_tests"),
    "--stdlib-dir", (Join-Path $repo_root "std"),
    "--cg-include", (Join-Path $repo_root "compiler-codegen"),
    "--rt-dir", (Join-Path $repo_root "compiler-codegen\nova_rt"),
    "--mode", $Mode,
    "--toolchain", $Toolchain
)
if ($Filter) { $cli_args += @("--filter", $Filter) }
if ($IncludeStdlib) { $cli_args += "--include-stdlib" }
if ($vcvars) { $cli_args += @("--vcvars", $vcvars) }

& $codegen @cli_args
exit $LASTEXITCODE
```

~40 строк вместо 320.

#### `run_tests.sh` (новый)

```bash
#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
CODEGEN="${NOVA_CODEGEN:-$HERE/compiler-codegen/target/debug/nova-codegen}"

if [ ! -x "$CODEGEN" ]; then
    echo "ERROR: nova-codegen not found at $CODEGEN" >&2
    echo "Run: cd compiler-codegen && cargo build" >&2
    exit 1
fi

# Default args; pass-through всех остальных в test-all.
exec "$CODEGEN" test-all \
    --tests-dir "$HERE/nova_tests" \
    --stdlib-dir "$HERE/std" \
    --cg-include "$HERE/compiler-codegen" \
    --rt-dir "$HERE/compiler-codegen/nova_rt" \
    "$@"
```

Пользователь вызывает:
```bash
./run_tests.sh --filter basics --mode release
./run_tests.sh --include-stdlib --toolchain gcc
```

chmod +x'нуть.

### Ф.4 — Docs & CI

#### Ф.4.1 — README обновить

В корневом `README.md` — раздел «Building & testing»:

```markdown
### Windows
```powershell
cd compiler-codegen
cargo build
cd ..
.\run_tests.ps1
```

### Linux / macOS
```bash
cd compiler-codegen
cargo build
cd ..
./run_tests.sh
```
```

#### Ф.4.2 — `docs/test-conventions.md`

Уточнить что test-runner — теперь `nova-codegen test-all`, обёрнутый в `.ps1`/`.sh`. EXPECT-маркеры парсятся в Rust (не PowerShell regex).

#### Ф.4.3 — CI (отложено)

Linux runner на GitHub Actions / GitLab CI. Прогон Clang dev + GCC dev. Отдельная задача, не блокер этого плана.

---

## Acceptance criteria

- ✅ `nova-codegen test-build basics/literals.nv` — выводит `PASS` и exit 0.
- ✅ `nova-codegen test-all` без флагов — рекурсивный прогон, тот же результат что у `.ps1`.
- ✅ Все 130/130 nova_tests проходят через новый pipeline на Windows Clang dev.
- ✅ MSVC fallback работает (`-toolchain msvc`).
- ✅ `run_tests.ps1` упрощён до ~40 строк, делегирует в `test-all`.
- ✅ `run_tests.sh` существует, chmod +x, синтаксически корректен.
- ⏸️ Linux smoke-test — отложено до Linux access.
- ⏸️ macOS smoke-test — отложено до macOS access.
- ⏸️ Linux CI — отдельная задача.

---

## Trade-offs / упрощения

### Blind Linux/macOS implementation

Не можем проверить на Linux/macOS прямо сейчас (Windows-only dev машина). Реализуем cross-platform через `std::path` + `std::process` + standard detection patterns. Уверенность ~85% что заработает out of the box; 15% — возможны мелкие path-handling/exec issues. Mitigation: первый Linux-пользователь увидит ошибку, исправим точечно.

### EXPECT-парсер дублирован для regex / TextScanner

В `.ps1` сейчас regex по first 30 lines. В Rust сделаем то же. Альтернатива — расширить `nova-codegen check` чтобы парсер AST выдавал EXPECT-метку как метаданные. Откладываем — потребовало бы AST-extensions, не блокер для cross-platform.

### vcvars detection через vswhere — Windows-bound

Vswhere есть только на Windows. На Linux/macOS vcvars не нужен (Clang/GCC находят SDK сами). В Rust детект разветвлён через `cfg!(target_os = "windows")`.

### Не выносим cargo build в `nova-codegen`

Wrapper'ы НЕ запускают `cargo build` сами. Если binary не построен — fail с понятной ошибкой. Альтернатива (auto-build при запуске тестов) — convenient но slow и опасный (race с активной разработкой). Пользователь сам запускает `cargo build` после изменений в компиляторе.

---

## План работ

1. **Ф.1.1** — модуль `nova_codegen::test_runner` с парсером EXPECT-маркеров.
2. **Ф.1.2** — toolchain detection (Windows / Linux / macOS).
3. **Ф.1.3** — build flags.
4. **Ф.1.4** — subprocess invocation (Command builder).
5. **Ф.1.5** — `Cmd::TestBuild` в main.rs.
6. **Ф.2** — `Cmd::TestAll` + walkdir + summary.
7. **Ф.3.1** — упрощённый `run_tests.ps1`.
8. **Ф.3.2** — новый `run_tests.sh`.
9. **Ф.4.1** — README updates.
10. **Smoke + full** Windows прогон — 130/130 PASS должны сохраниться.

---

## Оценка

**3-4 часа** работы:
- ~500 строк Rust в `test_runner` module (parser EXPECT, detection, build, run, compare).
- ~50 строк изменений в `main.rs` (две новые команды).
- ~40 строк `run_tests.ps1` (rewrite).
- ~30 строк `run_tests.sh` (новый файл).
- ~100 строк markdown (README + plan retro + docs/simplifications + project-creation).

Тестирование на Windows — 130/130 PASS должны сохраниться. Linux/macOS — blind ship.

---

## Что разблокирует

- **Linux dev**: open repo, `cargo build`, `./run_tests.sh` — работает без знания PowerShell.
- **CI/CD**: GitHub Actions Linux runners (бесплатные tier'ы) можно настроить.
- **Cross-platform parity**: один источник правды для test-runner logic — никаких drift'ов между ps1 и sh.
- **Будущий self-host**: когда Nova compiler будет на Nova, тестовая инфраструктура уже в Rust (не в shell-скриптах) — легче перевести.

---

## Связь с другими планами

- [Plan 09](09-clang-migration.md) — Clang toolchain. Plan 24 расширяет это до Linux Clang/GCC.
- [Plan 18](18-stdlib-roadmap.md) P0 — stdlib для networking/fs. Cross-platform — необходимо.
- [Plan 22](22-sleep-libuv-integration.md) — libuv. Cross-platform — необходимо.

---

## Ссылки

- `run_tests.ps1` — текущий Windows runner (320 строк).
- `compiler-codegen/src/main.rs` — CLI entry point.
- D89 (`spec/decisions/09-tooling.md`) — EXPECT-маркеры.
- D90 / D91 / D92 — недавние spec'ы влияющие на codegen, но не на runner.
