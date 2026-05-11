# План 26: hardening test-runner до cargo/go-test уровня

**Статус:** roadmap, не начат.
**Дата создания:** 2026-05-11.
**Тип:** инфраструктурный (DX + CI). Не меняет семантику Nova, улучшает test-runner до production-grade.

---

## Тезис

Plan 24 закрыл cross-platform foundation: `nova-codegen test-build / test-all` + thin wrappers `.ps1`/`.sh`. Это **80% production-grade** — архитектура правильная, но не хватает критичных stability + CI-grade фич, которые есть в `cargo test` / `go test`.

После Plan 25:
- Test-runner подходит для GitHub Actions CI (зависший тест не блокирует pipeline).
- Параллелизм даёт 5-7× speedup на типичной 8-core машине.
- JSON-output совместим с GitHub Actions / GitLab parse'ерами.
- Test caching — TDD-разработчик правит один файл, runner re-builds только зависимое.

---

## Текущее состояние (после Plan 24, commit 51b61a5)

`compiler-codegen/src/test_runner.rs` (~1400 строк):

✅ Что хорошо:
- Cross-platform toolchain detection (Clang/MSVC/GCC).
- 5 D89 EXPECT-маркеров с unit-тестами.
- Streaming output (`eprintln + flush` per-test).
- Lazy libuv build + cache (Plan 22 integration).
- CP1251 decoder для Windows stderr.
- Wrapper'ы: `.ps1` 71 строка, `.sh` 37 строк.

⚠️ Что мешает production-grade:

| # | Проблема | Симптом |
|---|---|---|
| 1 | Нет per-test timeout | Зависший exe → весь runner стоит forever |
| 2 | Общая `tmp_dir` для всех тестов | State leakage между тестами (наблюдалось: `errdefer_throw` PASS изолированно, FAIL в полном прогоне) |
| 3 | Sequential execution | На 130 тестов ~30-60 sec; на 1000 — несколько минут |
| 4 | Plain-text output | Хрупкий regex-парсинг в CI; нет JSON для GitHub Actions / GitLab |
| 5 | Нет caching | Каждый прогон — полный rebuild всех 130 тестов |
| 6 | Status enum 12 вариантов | Дубликат в `label()`/`detail()`; неструктурный для JSON |
| 7 | Heuristic CP1251 decoder | Хрупко; production делает `chcp 65001` + UTF-8 |
| 8 | Streaming в stderr, summary в stdout | Несогласованность; `2>&1` в wrappers'ах костыль |
| 9 | Нет `--verbose` / `--quiet` | Test stdout не виден на PASS; нельзя suppress PASS lines |
| 10 | Нет `--rerun-failed` | TDD: после fail-исправления приходится прогонять весь suite |
| 11 | `.ps1` wrapper — PowerShell stderr-trap | NativeCommandError в output; косметика |

---

## Цель

После Plan 25:

- ✅ Per-test timeout с default 60s (override через `--timeout`).
- ✅ Unique tmp subdir per test (`tmp_dir/<display-hash>/`).
- ✅ Parallel execution (`--jobs N`, default = `num_cpus`).
- ✅ Structured output (`--format json|tap|text`, default text).
- ✅ Test caching (`target/test-cache/`, hash от source + toolchain flags).
- ✅ Refactor `Status` → `Outcome { Pass, Fail { stage, ... } }`.
- ✅ UTF-8 force через `chcp 65001` (Windows) / `LANG=C.UTF-8` (Unix); CP1251 decoder в fallback.
- ✅ `--verbose` (показывать output PASS-тестов), `--quiet` (только FAIL + summary).
- ✅ `--rerun-failed` (использует `target/last-results.json`).
- ✅ Cleanup `.ps1` wrapper'а (без stderr-trap warning'ов).

---

## Не цель

- **Замена `cargo test`** для compiler-codegen lib-тестов — остаётся как есть.
- **`nova test` как stand-alone CLI** (без compiler subcommand) — это уже `nova-codegen test-all`, переименование когда self-host.
- **Test discovery через AST** (как `cargo test` ищет `#[test]` функции в .rs) — у нас отдельные `.nv` файлы, walkdir достаточен.
- **Property-based testing** (как `proptest` / `gopter`) — отдельная задача.
- **Snapshot testing** (как `insta`) — отдельная задача.

---

## Фазы

### Ф.1 — Per-test timeout (`--timeout SECS`, default 60)

**Текущая проблема.** `Command::output()` блокирующий. Если тест `time_handler.nv` или `concurrency/*.nv` зависает (deadlock в fiber, missing wake), весь runner застрял. Нет способа продолжить с следующего теста.

**Решение.** `Command::spawn()` + `Child::wait_timeout(Duration)`. Cross-platform:
- Linux/macOS: `waitpid(WNOHANG)` poll loop с sleep, либо `pidfd_open` (Linux 5.3+).
- Windows: `WaitForSingleObject(handle, ms)`.

Можно использовать crate **`wait-timeout`** (popular, MIT, ~150 строк) — единый API; либо собственный poll-loop:

```rust
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<Option<ExitStatus>> {
    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(Some(status)),
            None => {}
        }
        if start.elapsed() >= timeout {
            child.kill()?;  // SIGKILL on Unix, TerminateProcess on Windows
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
```

CLI: `--timeout 60` (секунды). Default 60s.

При timeout: `Status::Timeout(elapsed)` — новый вариант (или `Outcome::Fail { stage: Run, reason: Timeout }` после Ф.6 refactor).

Ф.1 объём: ~40 строк + 2 unit-теста (sleep 1, sleep 100 with timeout 1).

### Ф.2 — Unique tmp subdir per test

**Текущая проблема.** `tmp_dir/syntax__errdefer_throw.exe` + `tmp_dir/syntax__errdefer_throw-obj/`. Если **antivirus** держит handle на старом .exe, или **previous run** не успел освободить — collision. Параллелизм (Ф.3) без isolation невозможен.

**Решение.** `tmp_dir/<hash>/<exe_name>.exe`. Hash = `sha256(display).truncate(8)`, чтобы воспроизводимо. На cleanup — удалить вся subdir.

```rust
fn test_tmp_dir(global_tmp: &Path, display: &str) -> PathBuf {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    use std::hash::Hasher;
    h.write(display.as_bytes());
    let hash = h.finish();
    global_tmp.join(format!("t-{:016x}", hash))
}
```

Изоляция: каждый тест работает в своей subdir. Если AV держит handle — следующий run использует other path. State leakage между тестами невозможен.

Ф.2 объём: ~20 строк, ~10 строк тестов.

### Ф.3 — Параллельная execution (`--jobs N`)

**Текущая проблема.** Sequential. 130 тестов × ~250ms каждый = ~30-60s. На 8-core это ~5-8s параллельно.

**Решение.** Использовать `rayon` (новая dep) либо `std::thread::scope` с N workers + channel'ом для results.

```rust
use rayon::prelude::*;
inputs.par_iter().map(|nv| run_one(&opts_for(nv))).collect()
```

Параметр `--jobs N`, default = `std::thread::available_parallelism()`.

После Ф.2 (per-test isolation) — нет race conditions, кроме одного: **codegen эмитит `.c` рядом с `.nv`** (`opts.nv_file.with_extension("c")`). Если parallel прогон обрабатывает разные .nv — нет проблем (имена unique). Но **общий nova-codegen process state** (`CEmitter::new()` per-test) должен быть thread-safe — это надо проверить.

Caveat: streaming output из множества тредов смешается. Решение — `Mutex<Stdout>` или per-test buffer'ы с flush в порядке завершения.

Ф.3 объём: ~80 строк (rayon integration + output coordination), ~30 строк тестов.

Caveat 2: **dependency rayon** добавляет ~1MB к бинарю. Альтернатива — собственный thread pool через `std::thread::scope` (Rust 1.63+, у нас 1.85+ OK).

### Ф.4 — Structured output (`--format json|tap|text`)

**Текущая проблема.** Plain text «PASS xxx» / «FAIL yyy». CI/IDE парсит regex'ами — хрупко.

**Решение.**

**JSON:** одна строка на event.
```json
{"event":"started","test":"basics/literals"}
{"event":"finished","test":"basics/literals","status":"pass","elapsed_ms":234}
{"event":"finished","test":"syntax/foo","status":"fail","stage":"cc","detail":"...","elapsed_ms":1234}
{"event":"summary","pass":129,"fail":1,"elapsed_ms":45678}
```

**TAP:** TAP-13 формат (TAP = Test Anything Protocol, поддерживается миллионом tool'ов):
```
TAP version 13
1..130
ok 1 - basics/literals
not ok 2 - syntax/foo
  ---
  message: cc failed
  ...
```

**Text:** текущий формат (default, человек-friendly).

CLI: `--format json | tap | text` default `text`.

Ф.4 объём: ~80 строк (два формата). Не зависит от dependency'ев — `serde_json` не нужен, мы пишем простой одностройчный JSON через format!. Альтернатива — `serde_json` (~+500KB binary).

### Ф.5 — Test caching

**Текущая проблема.** Каждый прогон — полный rebuild. На 130 тестов ~30s в dev. Если правишь 1 file (типичный TDD-loop) — re-build всех 130 тестов **ничего не даёт**.

**Решение.** Cache в `target/test-cache/<hash>/<display>.exe`. Hash = `sha256(source + toolchain_flags + nova_codegen_version + nova_rt_mtime)`. При совпадении — пропустить codegen и cc, сразу exec.

Storage:
```
target/test-cache/
  basics__literals__<hash>.exe
  basics__literals__<hash>.json   # last result (для --rerun-failed)
```

При cache hit: log `CACHED basics/literals`, status = previous result. При miss: rebuild + сохранить.

CLI: `--no-cache` (force rebuild), `--cache-dir <path>` (override).

Ф.5 объём: ~150 строк. Сложность — invariant что hash включает все relevant inputs.

### Ф.6 — Refactor Status → Outcome

**Текущая проблема.** 12 вариантов `Status` enum. Дубликат в `label()`/`detail()` match arm. Неструктурный для JSON.

**Решение.**

```rust
pub enum Outcome {
    Pass { detail: String, elapsed: Duration },
    Fail { stage: Stage, elapsed: Duration },
    Timeout { elapsed: Duration },
    Skipped { reason: String },  // для --rerun-failed
}

pub enum Stage {
    Codegen { error: String },
    Cc { error: String },
    Run { error: String },
    Expectation {
        kind: ExpectMarker,
        mismatch: ExpectMismatch,
    },
}

pub enum ExpectMismatch {
    NoError,            // compile-error expected but codegen passed
    WrongMessage(String),
    NoPanic,
    WrongPanic(String),
    WrongExit { expected: i32, got: i32 },
    WrongStdout(String),
    WrongStderr(String),
}
```

Преимущества:
- Меньше дубликата (`label()` обходит дерево).
- Естественный mapping в JSON.
- Легче добавлять новые EXPECT-маркеры (один новый variant в `ExpectMismatch`).

Ф.6 объём: ~100 строк рефакторинг + adjust call-sites. Compile-time check — низкий риск регрессии.

### Ф.7 — UTF-8 force codepage

**Текущая проблема.** Windows cl.exe выводит stderr в active codepage (часто CP1251 для русского locale). `bytes_to_string()` heuristic decoder работает, но fragile (только CP1251 mapping).

**Решение.** 

На Windows: `chcp 65001` (UTF-8 codepage) **перед** cc invocation. cl.exe тогда сразу пишет UTF-8.

```rust
// Windows: prepend `chcp 65001 > nul && ` к cmd-line
let inner = if cfg!(target_os = "windows") {
    format!("chcp 65001 > nul && {}", inner)
} else {
    inner
};
```

На Linux/macOS: `cmd.env("LANG", "C.UTF-8")` или `LC_ALL=C.UTF-8`.

После этого decoder редуцируется до `String::from_utf8` + lossy fallback на edge cases. CP1251 mapping остаётся для backward compat (если cmd.exe не поддерживает chcp 65001, что бывает на старых systems).

Ф.7 объём: ~10 строк + проверить что русский stderr корректно проходит.

### Ф.8 — Streaming/summary в одном потоке

**Текущая проблема.** `eprintln!` в per-test → stderr. `println!` в summary → stdout. `2>&1` в wrappers — костыль.

**Решение.** Всё в **stdout** (cargo/go test convention). Stderr оставить для **ошибок самого runner'а** (вроде «vcvars not found»).

Ф.8 объём: 5-10 строк (change `eprintln!` → `println!` в run_all + adjust flush).

### Ф.9 — `--verbose` / `--quiet`

**Текущая проблема.** Test stdout не виден на PASS. Если хочешь posmotreть `println("hello")` test — приходится `--keep-artifacts` + ручной запуск.

**Решение.**
- `-v` / `--verbose`: показывать stdout/stderr тестов даже на PASS.
- `-q` / `--quiet`: только FAIL + summary, без per-test PASS lines.

Ф.9 объём: ~20 строк.

### Ф.10 — `--rerun-failed`

**Текущая проблема.** TDD: правлю один файл, прогоняю всё. Если 5 failed → правлю → прогоняю всё снова (130 тестов).

**Решение.** После каждого прогона сохранять `target/last-test-results.json`. При `--rerun-failed` читать, отфильтровать тесты которые were `Fail` или `Timeout`, прогнать только их.

Ф.10 объём: ~30 строк. Зависит от Ф.5 (cache + last-result mechanism).

### Ф.11 — Cleanup `.ps1` wrapper

**Текущая проблема.** PowerShell stderr-trap → NativeCommandError warning в output. Нет functional impact, но коробит.

**Решение.** Использовать `Start-Process` с RedirectStandardError либо просто `cmd /c "nova-codegen test-all ..."` — cmd не имеет PS-trap'ов.

Ф.11 объём: 5-10 строк.

---

## Acceptance criteria

- ✅ `nova-codegen test-all --timeout 5` на тесте который зависает на sleep(60) → `TIMEOUT after 5s`, прогон продолжается.
- ✅ `nova-codegen test-all --jobs 8` на 130 тестах — sub-15s на 8-core (vs current 30-60s).
- ✅ `nova-codegen test-all --format json` — каждый event одна JSON строка, parseable `jq`.
- ✅ Повторный прогон без изменений в source — все тесты `CACHED`, <1s total.
- ✅ Изменение одного `.nv` — re-runs только тот тест + dependents (если они уже найдены через walkdir).
- ✅ `--rerun-failed` после fix — прогоняет только бывшие failed.
- ✅ Cargo test `--lib` остаётся 77/77 PASS.
- ✅ Полный nova_tests прогон green (если не учитывать pre-existing real bugs sleep_leak_check, libuv_link).
- ⏸️ Linux/macOS smoke-test — отложено до access.

---

## Trade-offs / упрощения

### Какой parallel-фреймворк

- **`rayon`** — popular, удобный API, +1MB binary. Используется в `cargo` internals.
- **`std::thread::scope`** — без deps, scoped threads, ~50 строк boilerplate.
- **Async via tokio** — overkill, нам не нужен async I/O.

Выбор: **`std::thread::scope`** — минимум deps, Nova bootstrap должен быть lightweight.

### Какой cache hashing

- **`sha2` crate** — стандарт, но +200KB.
- **`std::collections::hash_map::DefaultHasher`** — SipHash, fast, but Rust internal (non-stable). OK для transient cache.
- **`crc32` собственный** — не cryptographic, но достаточно для cache-key.

Выбор: `DefaultHasher` (stable enough для cache-key, no extra deps).

### JSON output без `serde_json`

`serde_json` +500KB. Наши events простые (5-7 полей). Можно эмитить вручную:
```rust
println!(r#"{{"event":"finished","test":"{}","status":"{}","elapsed_ms":{}}}"#, ...);
```

Riskoff: escape spec. Решение: helper `json_escape(s)` который обрабатывает `\` `"` `\n` `\r` `\t`. ~10 строк.

### Cache invariant

Hash должен включать:
1. Hash содержимого .nv.
2. Hash всех `nova_rt/*.c` (изменение runtime → invalidate).
3. Hash `nova-codegen.exe` mtime (изменение codegen → invalidate).
4. Toolchain (clang/msvc/gcc) + mode (dev/release).
5. libuv enabled/disabled.

Если что-то забыли — false-positive cache hit и тест "PASS" когда должен FAIL. **Опасно**. Поэтому:
- Default `--no-cache` для CI (cache только для local dev).
- Альтернатива — **versioned cache key**: bump version при изменении runner internals.

### Per-test timeout default 60s

`cargo test` default 60s, `go test` default 10 minutes. Для bootstrap-language с lightweight тестами 60s агрессивный, но **разумный**: real test < 1s, > 60s = почти наверняка hang.

Slow тесты (`sleep_leak_check` — 15s budget) — override через **comment-маркер** `// EXPECT_TIMEOUT_MS 30000` (5-й D89 расширение?). Или через `--timeout 120`.

---

## План работ

1. **Ф.1** — per-test timeout (`wait_with_timeout`). Самый critical для CI.
2. **Ф.2** — unique tmp subdir per test. Готовит почву для Ф.3.
3. **Ф.6** — Status → Outcome refactor. Сделать до Ф.3/Ф.4 (легче на single thread).
4. **Ф.4** — JSON/TAP output. Маленькая, изолированная.
5. **Ф.3** — parallel execution. Большая, но после Ф.2/Ф.6 — естественная.
6. **Ф.7** — UTF-8 codepage. Маленькая, может быть в одном PR с Ф.1.
7. **Ф.8** — streaming/summary в одном потоке.
8. **Ф.9** — `--verbose` / `--quiet`.
9. **Ф.5** — test caching. После всего предыдущего, чтобы cache key стабилизировался.
10. **Ф.10** — `--rerun-failed`. Зависит от Ф.5.
11. **Ф.11** — `.ps1` wrapper cleanup. Косметика.

**Атомарные PR'ы:** Ф.1+Ф.7 → Ф.2 → Ф.6 → Ф.4 → Ф.3 → Ф.8+Ф.9 → Ф.5+Ф.10 → Ф.11.

---

## Оценка

**2-3 дня** работы:
- Ф.1: 0.5 час (timeout + 2 теста).
- Ф.2: 0.5 час (subdir + cleanup).
- Ф.6: 2 часа (рефакторинг).
- Ф.4: 1 час.
- Ф.3: 2 часа (thread::scope + coordination).
- Ф.7: 0.3 час.
- Ф.8: 0.2 час.
- Ф.9: 0.5 час.
- Ф.5: 3 часа (hash invariant — sensitive).
- Ф.10: 0.5 час.
- Ф.11: 0.3 час.
- Тестирование + docs: 2 часа.

---

## Что разблокирует

- **GitHub Actions Linux CI** — JSON output + per-test timeout критичны.
- **TDD workflow** — caching + parallel = sub-second feedback loop при правке.
- **CI matrix** (Windows MSVC + Windows Clang + Linux GCC + Linux Clang + macOS) — каждый прогон <1 min.
- **Self-host migration** — когда Nova compiler будет на Nova, test infra уже типизирована и production-grade.

---

## Связь с другими планами

- [Plan 24](24-cross-platform-test-runner.md) — foundation. Plan 25 hardens.
- [Plan 09](09-clang-migration.md) — Clang toolchain. Plan 25 добавляет parallelism который реально пользу даст от Clang/Linux build'ов.
- Будущий план Linux CI setup использует Plan 26 facilities.
- [Plan 25](25-production-readiness-roadmap.md) — runtime production-readiness gap analysis (отдельная тема: language/runtime, не test-tooling).

---

## Ссылки

- `cargo test` source: https://github.com/rust-lang/cargo/blob/master/src/cargo/ops/cargo_test.rs
- `go test` overview: https://pkg.go.dev/testing
- `cargo-nextest` (state-of-the-art Rust runner): https://nexte.st/
- TAP-13 spec: https://testanything.org/tap-version-13-specification.html
- `wait-timeout` crate: https://crates.io/crates/wait-timeout
- `compiler-codegen/src/test_runner.rs` — текущая реализация (Plan 24).
