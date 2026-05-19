# План 28: `nova` CLI binary

**Статус:** ✅ ЗАКРЫТ 2026-05-18 (Ф.0-Ф.7; nova-cli/ crate, все субкоманды реализованы, run_tests.ps1/run_tests.sh/regen_runtime.ps1 удалены).
**Дата создания:** 2026-05-11. **Обновлён:** 2026-05-18.
**Тип:** инфраструктурный (DX). Заменяет run_tests.ps1 / run_tests.sh / regen_runtime.ps1 единым Rust-бинарём.

---

## Тезис

`run_tests.ps1` постоянно глючит из-за фундаментальных ограничений PowerShell:
output buffering, stderr-trapping (NativeCommandError), quoting путей с пробелами.
Bash-обёртка (`run_tests.sh`) лучше, но оба скрипта — костыли.

Мировая практика (go, cargo, zig): один CLI-бинарь языка как точка входа для
пользователя. Компилятор (`nova-codegen`) остаётся как внутренний инструмент.

**Результат:** `nova test`, `nova build`, `nova run`, `nova check`, `nova regen-runtime`.
`run_tests.ps1`, `run_tests.sh`, `regen_runtime.ps1` удаляются.

---

## Архитектура

```
nova test          ← пользователь
  → nova_codegen::test_runner::run_all()  ← path dep, без subprocess

nova build foo.nv
  → nova_codegen::codegen::CEmitter       ← path dep
  → clang/msvc/gcc (subprocess, через test_runner::compile_c_to_exe)

nova run foo.nv    ← nova_codegen::interp
nova check foo.nv  ← nova_codegen::parser + types
nova regen-runtime ← nova_codegen::codegen::runtime_registry
```

`nova-codegen` CLI сохраняется нетронутым — используется IDE, CI, прямой отладкой.

---

## Структура файлов

```
nova-cli/               ← новый crate (sibling к compiler-codegen/)
├── Cargo.toml
└── src/
    └── main.rs         (~380 строк, один файл)
```

Нет Rust workspace — `nova-cli` standalone crate, как `nova-codegen`.

---

## Cargo.toml (nova-cli)

```toml
[package]
name = "nova"
version = "0.1.0"
edition = "2021"
rust-version = "1.85"
description = "Nova language CLI — build, run, test Nova programs"
publish = false

[[bin]]
name = "nova"
path = "src/main.rs"

[dependencies]
clap         = { version = "4", features = ["derive"] }
anyhow       = "1"
nova_codegen = { path = "../compiler-codegen" }

[profile.release]
# Bootstrap: не оптимизируем размер — согласовано с compiler-codegen.
opt-level = 0

[profile.dev]
debug = true
```

`Cargo.lock` **не коммитится** — следуем конвенции `compiler-codegen` (`publish = false`,
оба crate — инструменты разработки одного репо).

`.gitignore` в корне репо содержит `target/` — покрывает `nova-cli/target/` автоматически
(проверить при создании crate; если не покрывает — добавить `nova-cli/target/` явно).

---

## Фазы реализации

### Ф.0 — `compile_c_to_exe` в test_runner.rs (единственное изменение в nova-codegen)

Добавить pub-обёртку над существующим приватным `build_command` + `run_with_timeout`:

```rust
// compiler-codegen/src/test_runner.rs
pub fn compile_c_to_exe(
    tc: &Toolchain,
    opts: &BuildOpts,
    timeout: Duration,
) -> anyhow::Result<PathBuf>
```

Возвращает путь к exe на success, `anyhow::Error` на fail — согласовано с
остальным API `nova_codegen`. ~20 строк, переиспользует приватные `build_command`
и `run_with_timeout`.

### Ф.1 — Каркас nova-cli

Создать `nova-cli/Cargo.toml` и `nova-cli/src/main.rs` с:
- Clap enum `Cmd { Test, TestBuild, Build, Run, Check, RegenRuntime }`
  — `TestBuild` = `nova test-build <file.nv>` (одиночный тест, полезен для IDE/CI)
- `find_repo_root()` — идёт вверх от CWD ища `nova.toml` (аналог cargo → Cargo.toml)
- `resolve_paths(repo: &Path) -> RepoPaths` — struct с полями:
  ```
  tests_dir:    {repo}/nova_tests
  stdlib_dir:   {repo}/std
  cg_include:   {repo}/compiler-codegen
  rt_dir:       {repo}/compiler-codegen/nova_rt
  results_file: {repo}/target/last-test-results.json   ← repo_root/target, не nova-cli/target
  tmp_dir:      $TEMP/nova_tests  (Windows) / /tmp/nova_tests  (Unix)
  ```
- `find_nova_codegen(repo: &Path) -> PathBuf` — `$NOVA_CODEGEN` env override,
  иначе `{repo}/compiler-codegen/target/debug/nova-codegen[.exe]`.
  Нужен только для будущих subprocess-вызовов; в текущем плане всё через lib.

### Ф.2 — `nova test`

Флаги (идентично run_tests.ps1/sh + Plan 27 Ф.1):
```
--filter <substr>
--jobs <N>              (0 = num_cpus; default_jobs() → pub fn в test_runner)
--format text|json|tap|junit
--mode dev|release
--toolchain auto|clang|msvc|gcc
--gc malloc|boehm       (Plan 27 Ф.1 — передаётся в TestAllOpts.gc_kind;
                         default: то же что в nova-codegen test-all на момент реализации)
--timeout <secs>        (default 60)
--verbose / --quiet
--results-file <path>   (default: {repo_root}/target/last-test-results.json)
--rerun-failed
--retries <N>           (тип u32 — согласовано с TestAllOpts.retries: u32)
--include-stdlib
--keep-artifacts
```

**Зависимость от Plan 27:** если Plan 27 Ф.1 реализован раньше Plan 28 —
`TestAllOpts` уже содержит `gc_kind: GcKind` и нужно передавать его.
Если Plan 27 ещё не реализован — `--gc` flag добавляется как заглушка
(parse + игнорировать, пока поле не появится в `TestAllOpts`).

Реализация `cmd_test()`:
1. `find_repo_root()` — fail с понятной ошибкой если nova.toml не найден
2. `resolve_paths(repo)` — все пути
3. Распарсить флаги в типы через `Mode::parse`, `ToolchainPref::parse`,
   `OutputFormat::parse`, `Verbosity::parse` — все `pub` в test_runner
4. `test_runner::detect_toolchain(&ToolchainOpts { pref, explicit_clang: None, explicit_vcvars: None })`
   — vcvars auto-detect встроен внутрь `detect_toolchain`
5. Извлечь vcvars из детектированного toolchain для передачи в libuv:
   ```rust
   let vcvars = match &tc { Toolchain::Clang { vcvars, .. } => vcvars.as_deref(),
                             Toolchain::Msvc { vcvars } => Some(vcvars.as_path()), _ => None };
   ```
6. `test_runner::detect_or_build_libuv(&rt_dir, &repo, vcvars)`
   — **делает `exit(1)` если libuv не инициализирован** (намеренно, не баг)
7. `test_runner::install_cancel_handler()` — вызвать **до** `run_all`;
   `run_all` сам не вызывает его — ответственность caller'а
8. Построить `TestAllOpts { toolchain: tc, .. }` — поле `toolchain` **owned** (move),
   не `&Toolchain`; значит детектировать toolchain до передачи в opts
9. Вызвать `test_runner::run_all(opts)?` — стримит прогресс в stdout сам
10. `test_runner::print_summary(&summary, format)` — пишет в stdout
11. `Err(anyhow!(...))` если `summary.fail > 0` — main вернёт `ExitCode::FAILURE`

### Ф.3 — `nova check <file.nv>`

Напрямую через lib:
1. `read_file(path)`
2. `nova_codegen::parser::parse(&src)`
3. `nova_codegen::types::check_module(&module)`
4. Вывод ошибок через `d.render(&src, &path)`

### Ф.4 — `nova run <file.nv>`

Интерпретатор (как `nova-codegen run`):
1. parse + typecheck
2. `nova_codegen::interp::Interpreter::new()` → `load_module` → `run_main`

### Ф.5 — `nova build <file.nv> [-o output]`

Compile .nv → .c → native binary:
1. parse + typecheck + `CEmitter::emit_module` → строка C-кода
2. Временная директория: `default_tmp_dir()` / `nova_build` / `<hash-of-abspath>`
   (уникальна по пути файла, не загрязняет рабочую директорию).
   `default_tmp_dir()` — та же логика что в `nova-codegen`:
   Windows `%TEMP%`, Unix `$TMPDIR` → `/tmp`
3. Записать `.c` во `tmp/foo.c`, `exe_file` = `tmp/foo[.exe]`, `obj_dir` = `tmp/`
4. `detect_toolchain()` → извлечь vcvars из tc
5. `detect_or_build_libuv()` — нужен т.к. любой `.nv` может линковаться с runtime,
   использующим libuv (Time.sleep, Net и др.)
6. `install_cancel_handler()` — вызвать до компиляции
7. Построить `BuildOpts { c_file, exe_file, obj_dir, cg_include, rt_dir, mode, libuv }`
8. `compile_c_to_exe(&tc, &build_opts, timeout)` (из Ф.0)
9. Скопировать exe в `-o output` (если задан) или `{cwd}/{stem}[.exe]`
   (рядом с CWD, не с исходником — предсказуемое место)
10. Удалить `tmp/` если не `--keep-artifacts`

**Флаги `nova build`:**
```
<file.nv>
[-o <output>]
[--mode dev|release]
[--toolchain auto|clang|msvc|gcc]
[--timeout <secs>]     (default 120 — компиляция дольше теста)
[--keep-artifacts]
```

### Ф.6 — `nova regen-runtime [--check]`

Напрямую через lib (заменяет regen_runtime.ps1):
- `nova_codegen::codegen::runtime_registry::all()`
- Render и write/compare — та же логика что в `cmd_emit_runtime_stubs`

### Ф.7 — Удаление скриптов и обновление docs

Удалить:
- `run_tests.ps1`
- `run_tests.sh`
- `regen_runtime.ps1`
- `regen_runtime.bat` (если существует)

Обновить:
- `README.md` — заменить все упоминания `run_tests.ps1` / `run_tests.sh` /
  `nova-codegen test-all` на `nova test`; раздел single-test debugging —
  `nova test-build nova_tests/basics/literals.nv`
- `CONTRIBUTING.md` — секция сборки и тестирования
- `docs/plans/README.md` — строка Plan 28 (уже добавлена)
- `docs/project-creation.txt` — запись о nova CLI
- `docs/simplifications.md` — убрать упоминания ps1 если есть
- `docs/test-conventions.md` — обновить Quick start: `nova test` вместо
  `.\run_tests.ps1` / `./run_tests.sh`

---

## Ловушки реализации

### `install_cancel_handler()` — вызов на caller'е, не внутри run_all

`run_all()` **не вызывает** `install_cancel_handler()` автоматически.
Caller (nova-cli) обязан вызвать его явно **до** `run_all` и **до** `compile_c_to_exe`.
Иначе Ctrl+C не будет обработан gracefully.

### `TestAllOpts.toolchain` — owned, не borrowed

```rust
pub struct TestAllOpts<'a> {
    pub toolchain: Toolchain,   // OWNED, не &Toolchain
    ...
}
```

Нельзя передать `&tc` — только `tc` (move). Значит после передачи в opts
нельзя использовать `tc` для libuv. Правильный порядок:
1. `detect_toolchain()` → `tc`
2. Извлечь `vcvars` из `tc` (до move)
3. `detect_or_build_libuv(..., vcvars)` (использует vcvars)
4. Построить `TestAllOpts { toolchain: tc, .. }` (move tc)

### vcvars для libuv — извлекать из Toolchain до move

```rust
let vcvars_path: Option<PathBuf> = match &tc {
    Toolchain::Clang { vcvars, .. } => vcvars.clone(),
    Toolchain::Msvc { vcvars }      => Some(vcvars.clone()),
    Toolchain::Gcc { .. }           => None,
};
let libuv = test_runner::detect_or_build_libuv(&rt_dir, &repo, vcvars_path.as_deref());
```

### `retries` — тип `u32`, не `usize`

`TestAllOpts.retries: u32`. Clap-аргумент должен быть `u32`:
```rust
#[arg(long, default_value_t = 0u32)]
retries: u32,
```

### `.gitignore` — `target/` покрывает nova-cli/target/

Файл содержит `target/` без `**/` — ripgrep/git обрабатывают это как
«любой `target/` в любом поддереве». `nova-cli/target/` покрыт.
Дополнительных записей не нужно.

### `nova build` — hash для tmp dir без sha256 crate

`sha256` нет в зависимостях. Для уникальности tmp dir использовать
простой hash от abspath через стандартный `DefaultHasher`:
```rust
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
fn path_hash(p: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    p.hash(&mut h);
    h.finish()
}
```
Не криптографический, но достаточен для уникальности tmp-директорий.

---

## Repo root detection

```rust
fn find_repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join("nova.toml").exists() {
            return Ok(dir);
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => bail!("nova.toml not found — are you inside a Nova project?"),
        }
    }
}
```

---

## Критические файлы

| Файл | Действие |
|---|---|
| `nova-cli/Cargo.toml` | создать |
| `nova-cli/src/main.rs` | создать (~400 строк) |
| `compiler-codegen/src/test_runner.rs` | добавить `pub fn compile_c_to_exe` |
| `run_tests.ps1` | удалить |
| `run_tests.sh` | удалить |
| `regen_runtime.ps1` | удалить |
| `README.md` | обновить (run_tests → nova test, single-test debugging) |
| `CONTRIBUTING.md` | обновить |
| `docs/test-conventions.md` | обновить Quick start |
| `docs/plans/README.md` | строка Plan 28 (уже добавлена) |
| `docs/project-creation.txt` | добавить запись |
| `docs/simplifications.md` | проверить/обновить |

---

## Acceptance criteria

- `nova test` на Windows (Clang dev) даёт те же PASS/FAIL что `.\run_tests.ps1`
- `nova test --filter basics` проходит
- `nova test --rerun-failed` работает после предыдущего прогона
- `nova test-build nova_tests/basics/literals.nv` → PASS exit 0
- `nova check nova_tests/basics/literals.nv` → exit 0
- `nova run nova_tests/basics/literals.nv` → запускается интерпретатором
- `nova build nova_tests/basics/literals.nv` → создаёт бинарь, `.c` не остаётся в CWD
- `nova build nova_tests/basics/literals.nv -o out/lit` → бинарь в `out/lit[.exe]`
- `nova regen-runtime --check` → exit 0 на чистом дереве
- `nova regen-runtime --check` → exit 1 после ручного изменения std/runtime/*.nv
- `run_tests.ps1`, `run_tests.sh`, `regen_runtime.ps1` не существуют в репо
- `nova-cli/target/` покрыт `.gitignore` (проверить после `cargo build`)
- README и CONTRIBUTING обновлены

---

## Что НЕ входит в этот план

- `nova fmt`, `nova lint`, `nova doc` — будущие субкоманды, заглушки не нужны
- Workspace-level `Cargo.toml` — не создаём
- `build_libuv.ps1` — оставляем (сложный build-скрипт, отдельная история)
- Linux/macOS smoke-test — как и раньше, blind ship

---

## Связь с другими планами

- [Plan 24](24-cross-platform-test-runner.md) — движок test-runner (остаётся в nova-codegen)
- [Plan 26](26-test-runner-hardening.md) — hardening test-runner (уже закрыт, переиспользуется)
- [Plan 03](03-package-ecosystem-roadmap.md) — будущий полный `nova` CLI (self-host, package manager)

---

## Implementation log

- **2026-05-18** — `nova-cli/` crate реализован. Команды в `src/main.rs`:
  - `nova check` — type-check файл или директорию (Plan 36 R7/R10, polymorphic path)
  - `nova run` — запустить файл через интерпретатор
  - `nova build` — компилировать .nv → native binary
  - `nova test` — запустить тесты (файл или директория, все флаги из плана)
  - `nova test-build` — build + run одного test-файла (IDE/CI)
  - `nova regen-runtime` — регенерировать runtime stubs (заменяет regen_runtime.ps1)
  - `nova doc` — генерация документации (Plan 45, D107: markdown/json, --test, --check, --watch, --coverage, --diff, --scrape-examples, --output-dir, MCP)
  - `nova doc-query` — query JSON doc output через DSL (Plan 45 Ф.32.1)
  - `nova doc-mcp` — MCP server JSON-RPC over stdio (Plan 45 Ф.32.3)
  - `migrate_plan60`, `migrate_plan65` — one-shot migration binaries (src/bin/)
  - Зависимости: `clap`, `anyhow`, `serde_json`, `nova_codegen`
  - Plan 36 R7 exit codes (0/1/2/101), R10 `--color auto|always|never`
