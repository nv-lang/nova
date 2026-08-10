//! Plan 24: cross-platform test runner. Реализует `nova-codegen test-build`
//! и `nova-codegen test-all` — кросс-платформенный аналог `run_tests.ps1`.
//!
//! Pipeline для одного .nv:
//!   1. Парсит D89 EXPECT-маркер из первых 30 строк.
//!   2. Codegen .nv → .c через `CEmitter::emit_module`.
//!   3. Если `EXPECT_COMPILE_ERROR` — проверяет pattern в codegen-error.
//!   4. Иначе компилирует .c → .exe через выбранный toolchain (clang/cl/gcc).
//!   5. Запускает .exe, читает stdout/stderr, exit code.
//!   6. Сравнивает с EXPECT (или с default exit=0).
//!
//! Toolchain detection — кросс-платформенный:
//!   - Windows: Clang (LLVM install), MSVC (через vcvars64.bat), GCC (MSYS).
//!   - Linux/macOS: Clang (system), GCC (system).

use crate::ast;
use crate::codegen::CEmitter;
use crate::manifest;
use crate::parser;
use crate::types;
use anyhow::{anyhow, Result};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// ---------- Окно p401b-p67-class (реестр 221.1 №401, "ПЕРЕОТКРЫТ"): per-unit
// panic containment ----------
//
// A codegen-side internal-error panic (any `panic!` reached during
// `codegen_to_c` — the `[P67-LEGACY]` class among others) used to be fatal to
// the WHOLE `nova test <dir>` process: the CLI's global panic hook
// (`nova-cli/src/main.rs`) called `std::process::exit(101)` unconditionally
// on ANY thread's panic, so one file's gap killed every file queued after it
// — "цена" documented in реестр 221.1 №401. This flag (thread-local: each
// `jobs` worker owns its own) tells that hook "a caller up this SAME thread's
// stack is about to `catch_unwind` around a single compile-unit — stay quiet,
// don't print the misleading 'internal error / please report a bug' banner,
// and don't exit the process; let the panic unwind to that catcher." The
// catcher (below, wrapping the `codegen_to_c` call) reports the panic through
// the EXACT SAME `Result<_, String>` channel `codegen_to_c` already uses for
// ordinary compile errors (`E_FFI_C_NAME_OVERLOAD_CONFLICT` and siblings) —
// so the rest of the per-test pipeline (FAIL reporting, results-file
// writing, batch continuation to the NEXT file) needs zero changes: it
// already handles `Err(String)` from this exact call correctly (verified:
// a genuine syntax error in one file of a batch does NOT stop the batch).
//
// Outside this window (`nova build` on a single file, `nova check`, or any
// panic NOT reached through this wrapped call) the flag stays `false` and
// behavior is BYTE-IDENTICAL to before: hook prints the banner, and the
// outermost `catch_unwind` in `nova-cli::main` exits 101 (same user-visible
// result, only the `exit` call itself moved from the hook to the outer
// catcher so THIS thread-local can intercept it first).
thread_local! {
    static CATCHING_UNIT_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `nova-cli`'s panic hook (crate boundary: the hook lives in the `nova`
/// binary, this flag lives in the `nova_codegen` library both link against)
/// calls this to decide whether to print the "internal error ... please
/// report it" banner. `true` = a `catch_unwind` up this thread's OWN stack
/// is about to handle the panic as an ordinary per-unit compile failure —
/// suppress the banner (the catcher reports it as a normal `FAIL`/`CC-FAIL`-
/// style line instead, no "please report a bug" noise).
pub fn catching_panic_active() -> bool {
    CATCHING_UNIT_PANIC.with(|f| f.get())
}

/// Run `f` with a per-compile-unit panic net: sets [`catching_panic_active`]
/// for the duration (suppressing the misleading crash banner in the hook),
/// runs `f` under `catch_unwind`, and on a caught panic formats the payload
/// into the SAME `Err(String)` shape `codegen_to_c`'s ordinary compile-error
/// path already returns — so callers need no new match arm. `pub`: also used
/// by `nova-cli`'s `cmd_build` (single-file build path) around
/// `emitter.emit_module_multi_tu(...)`, which already returns
/// `Result<_, String>` for ordinary codegen errors — same reuse, same
/// reasoning, one file instead of a whole batch.
pub fn catch_unit_panic<T>(f: impl FnOnce() -> Result<T, String> + std::panic::UnwindSafe) -> Result<T, String> {
    CATCHING_UNIT_PANIC.with(|flag| flag.set(true));
    let result = std::panic::catch_unwind(f);
    CATCHING_UNIT_PANIC.with(|flag| flag.set(false));
    match result {
        Ok(inner) => inner,
        Err(payload) => {
            let msg = payload.downcast_ref::<&str>().map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<no panic message>".to_string());
            Err(format!("[INTERNAL-PANIC] {}", msg))
        }
    }
}

// ---------- Plan 26 Ф.1: per-test timeout ----------

/// Запускает `child` и ждёт завершения с timeout. Возвращает:
/// - `Ok(Some(status))` — child завершился до timeout;
/// - `Ok(None)` — timeout, child killed (best-effort).
///
/// Кросс-платформенно через poll-loop `try_wait`. Дёшево (10 ms sleep
/// между опросами), для тестов в диапазоне 100 ms — 60 s overhead < 1%.
pub fn wait_with_timeout(child: &mut Child, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
    let start = Instant::now();
    // Plan 26 Ф.16 #8: adaptive poll backoff. 1ms → 2 → 5 → 10 → 25 → 50 ms.
    // На fast тестах (<10ms) overhead был 100% c fixed 10ms; теперь <1ms
    // на первой итерации. Для long тестов экономим CPU 5× через 50ms cap.
    let poll_steps_ms = [1, 2, 5, 10, 25, 50];
    let mut step = 0usize;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if start.elapsed() >= timeout {
            // Best-effort kill. На Windows TerminateProcess, на Unix SIGKILL.
            let _ = child.kill();
            // Дренируем zombie, иначе fd-leak.
            let _ = child.wait();
            return Ok(None);
        }
        let poll_ms = poll_steps_ms[step.min(poll_steps_ms.len() - 1)];
        std::thread::sleep(Duration::from_millis(poll_ms));
        step = (step + 1).min(poll_steps_ms.len() - 1);
    }
}

/// Plan 26 Ф.16 #2: join thread с safety-timeout. Возвращает результат
/// если поток закончил в течение `timeout`, иначе detach + empty default.
/// Cross-platform через mpsc channel — std::thread::JoinHandle не
/// предоставляет timed join.
fn join_with_timeout(
    handle: std::thread::JoinHandle<Vec<u8>>,
    timeout: Duration,
) -> Vec<u8> {
    use std::sync::mpsc;
    // Re-wrap join'а в отдельном thread'е → result через channel.
    // Если channel.recv_timeout вернул Err — оригинальный поток detach'нут
    // (он живёт до конца process'а, но мы не блокированы).
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = handle.join().unwrap_or_default();
        let _ = tx.send(result);
    });
    rx.recv_timeout(timeout).unwrap_or_default()
}

/// Plan 209 Ф.2: synthesize a plain success/failure `ExitStatus`. The
/// multi-TU compile+link path (`compile_multi_tu_to_exe`) runs SEVERAL
/// subprocesses (N parallel part compiles + a link) folded into one
/// `anyhow::Result` — this lets `run_one` feed that single verdict into the
/// SAME `(CapturedOutput, ExitStatus)`-shaped post-cc branching
/// (`EXPECT_CC_ERROR` matching, run-the-exe, …) that a real single child
/// process's exit status already drives for the single-TU path, without
/// duplicating that (large) downstream logic.
#[cfg(target_os = "windows")]
fn synth_exit_status(success: bool) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(if success { 0 } else { 1 })
}
#[cfg(not(target_os = "windows"))]
fn synth_exit_status(success: bool) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(if success { 0 } else { 1 })
}

/// Капчуренный output после run с timeout. Заменяет `Output` из
/// `Command::output()` — там нет варианта «убит по таймауту».
pub struct CapturedOutput {
    pub status: Option<ExitStatus>,  // None = timeout
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed: Duration,
}

/// [M-test-runner-tempdir-race-jobs]: classify a `Command::spawn()` failure
/// as a TRANSIENT Windows exec-lock (worth retrying) vs a genuine error
/// (missing binary, permissions the user actually needs to fix, etc — must
/// fail immediately, never silently retried into a false PASS). Raw OS error
/// codes, not string-matching: `spawn()` failures are OS-level (`CreateFileW`
/// under the hood), carrying no descriptive child-process text to match on
/// (unlike the CC/link retry above, which greps the CHILD's own stdout/
/// stderr). `5` = `ERROR_ACCESS_DENIED`, `32` = `ERROR_SHARING_VIOLATION` —
/// the two codes Windows returns when another process (classically Defender/
/// AV scanning a freshly-written .exe on first execution) holds a
/// conflicting handle for a moment. Pure/pub(crate) so a unit test can feed
/// synthetic `io::Error`s without needing an actual locked file on disk.
pub(crate) fn is_transient_exec_lock_error(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5) | Some(32))
}

/// Стандартный `Command::output()` блокирует вечно если child зависает.
/// Эта функция запускает child + читает stdout/stderr через pipes +
/// убивает по таймауту. Threads нужны потому что piped stdout/stderr
/// надо drain'ить параллельно (full pipe-buffer = deadlock).
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> std::io::Result<CapturedOutput> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let start = Instant::now();
    let mut child = cmd.spawn()?;

    // Drain stdout/stderr в фоновых потоках, чтобы не deadlock'нуть
    // на полном pipe-buffer'е (Windows ~4 KB, Linux ~64 KB).
    // Plan 26 Ф.15: explicit error если pipe internal-invariant нарушен
    // вместо panic. `Stdio::piped()` гарантирует Some(...), но defensive.
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => return Err(std::io::Error::new(
            std::io::ErrorKind::Other, "child stdout pipe missing")),
    };
    let stderr = match child.stderr.take() {
        Some(s) => s,
        None => return Err(std::io::Error::new(
            std::io::ErrorKind::Other, "child stderr pipe missing")),
    };
    // Plan 26 Ф.15: read buffer cap. Тест, печатающий 100 MB stdout
    // (бесконечный print-loop), не должен OOM'нуть runner. Cap = 4 MB —
    // больше чем хватит для real test output, меньше чем разумный stress.
    // Plan 26 Ф.16 #9: при переполнении добавляем truncation marker —
    // silent truncate скрывал бы важные ошибки в конце stdout.
    const READ_CAP: u64 = 4 * 1024 * 1024;
    const TRUNC_MARKER: &[u8] = b"\n... (output truncated at 4 MB)\n";
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut s = std::io::Read::take(stdout, READ_CAP);
        let _ = std::io::Read::read_to_end(&mut s, &mut buf);
        if buf.len() as u64 == READ_CAP {
            buf.extend_from_slice(TRUNC_MARKER);
        }
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut s = std::io::Read::take(stderr, READ_CAP);
        let _ = std::io::Read::read_to_end(&mut s, &mut buf);
        if buf.len() as u64 == READ_CAP {
            buf.extend_from_slice(TRUNC_MARKER);
        }
        buf
    });

    let status = wait_with_timeout(&mut child, timeout)?;
    // Plan 26 Ф.16 #2: thread join с safety-timeout. После kill child'а
    // pipe должен закрыться → read_to_end вернётся. На Windows
    // TerminateProcess не всегда закрывает pipe handles немедленно;
    // если drain thread висит — лучше потерять часть output'а чем
    // hang'нуть runner. 500ms — generous для real-world Windows close.
    let stdout_bytes = join_with_timeout(stdout_handle, Duration::from_millis(500));
    let stderr_bytes = join_with_timeout(stderr_handle, Duration::from_millis(500));
    Ok(CapturedOutput {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
        elapsed: start.elapsed(),
    })
}

// ---------- D89 EXPECT-маркеры ----------

#[derive(Debug, Clone)]
pub enum ExpectMarker {
    /// codegen error содержит pattern.
    CompileError(String),
    /// C-compiler (cc/clang/cl) error содержит pattern.
    /// Используется для capability-isolation тестов (D91): Nova codegen
    /// успешен, но C-компилятор выдаёт ошибку (no member, undeclared id).
    CcError(String),
    /// exe exit != 0 + stderr содержит pattern.
    RuntimePanic(String),
    /// exit code == N (любой stdout/stderr).
    ExitCode(i32),
    /// stdout содержит pattern (любой exit code).
    Stdout(String),
    /// stderr содержит pattern (любой exit code).
    Stderr(String),
    /// Plan 52 Ф.9: lint warning (от `lints::lint_module`) содержит
    /// pattern. Allows asserting NaN-key, duplicate-map-key, и других
    /// lint выдач, которые не error'ятся и не leak'ятся в stdout/stderr.
    /// Multi-pattern (как Stdout/Stderr) — несколько маркеров OK.
    CompileWarning(String),
    /// №463 (owner review, p463 window, 2026-08-08): `nova lint`'s
    /// CONV_RULES registry (`lints::run_conv_rules`) содержит правило с
    /// этим ID (`W_...` rule id, ТОЧНОЕ совпадение, не substring сообщения)
    /// среди находок. ОТДЕЛЬНЫЙ канал от `CompileWarning` — та сверяется
    /// только с `lints::lint_module` (unconditional AST-lint pass, часть
    /// build/check pipeline), а CONV_RULES (`nova lint`-only опциональный
    /// реестр конвенций) туда НЕ попадает: `nova test` без этого маркера
    /// вообще не вызывал `run_conv_rules` — фикстура на CONV_RULES-правило
    /// не ассертила НИЧЕГО (обнаружено ревью владельца — `EXPECT_LINT_WARNING`
    /// изобретён окном p463 и молча игнорировался раннером, страж
    /// `check-expect-markers.sh` поймал). Multi-pattern (как CompileWarning).
    LintWarning(String),
}

/// Парсит D89 EXPECT-маркеры из первых 30 строк.
///
/// Возвращает все маркеры в порядке появления. Несколько маркеров разных
/// типов поддерживаются одновременно (например `EXPECT_RUNTIME_PANIC` +
/// `EXPECT_STDOUT` для тестов где defer fires перед panic).
///
/// Ограничения совместимости: не более одного `COMPILE_ERROR` и не более
/// одного `CC_ERROR` (дублирование этих двух выдаёт warning и берёт первый).
/// `RUNTIME_PANIC`, `STDOUT`, `STDERR`, `EXIT_CODE` — можно несколько,
/// хотя на практике больше одного `RUNTIME_PANIC` или `EXIT_CODE` не имеет
/// смысла (проверяется только один exit-code/panic-pattern).
///
/// **Важно**: non-comment lines пропускаются (`continue`), не прерывают
/// поиск — маркер в строке 5 находится даже если строка 1 = `module foo`.
pub fn parse_expect(src: &str) -> Vec<ExpectMarker> {
    let mut found: Vec<ExpectMarker> = Vec::new();
    for line in src.lines().take(30) {
        let trimmed = line.trim_start();
        let Some(body) = trimmed.strip_prefix("//") else {
            continue;
        };
        let body = body.trim_start();

        let parsed: Option<ExpectMarker> = if let Some(rest) = body.strip_prefix("EXPECT_COMPILE_ERROR") {
            let arg = rest.trim();
            // Empty pattern matches any compile error (same as EXPECT_CC_ERROR behaviour).
            Some(ExpectMarker::CompileError(arg.to_string()))
        } else if let Some(rest) = body.strip_prefix("EXPECT_CC_ERROR") {
            let arg = rest.trim();
            Some(ExpectMarker::CcError(arg.to_string()))
        } else if let Some(rest) = body.strip_prefix("EXPECT_RUNTIME_PANIC") {
            let arg = rest.trim();
            (!arg.is_empty()).then(|| ExpectMarker::RuntimePanic(arg.to_string()))
        } else if let Some(rest) = body.strip_prefix("EXPECT_EXIT_CODE") {
            rest.trim().parse::<i32>().ok().map(ExpectMarker::ExitCode)
        } else if let Some(rest) = body.strip_prefix("EXPECT_STDOUT") {
            let arg = rest.trim();
            (!arg.is_empty()).then(|| ExpectMarker::Stdout(arg.to_string()))
        } else if let Some(rest) = body.strip_prefix("EXPECT_STDERR") {
            let arg = rest.trim();
            (!arg.is_empty()).then(|| ExpectMarker::Stderr(arg.to_string()))
        } else if let Some(rest) = body.strip_prefix("EXPECT_COMPILE_WARNING") {
            // Plan 52 Ф.9: multi-pattern (like Stdout/Stderr) — несколько
            // EXPECT_COMPILE_WARNING могут coexist (например NaN + dup-key
            // в одном литерале).
            let arg = rest.trim();
            (!arg.is_empty()).then(|| ExpectMarker::CompileWarning(arg.to_string()))
        } else if let Some(rest) = body.strip_prefix("EXPECT_LINT_WARNING") {
            // №463: `nova lint` CONV_RULES rule id (точное совпадение,
            // напр. `W_REDUNDANT_PAREN`), multi-pattern как CompileWarning.
            let arg = rest.trim();
            (!arg.is_empty()).then(|| ExpectMarker::LintWarning(arg.to_string()))
        } else {
            None
        };

        if let Some(marker) = parsed {
            // Each marker type is only kept once (first-wins for same type),
            // but different types can coexist.
            // Exception: STDOUT and STDERR can appear multiple times (all patterns checked).
            let is_dup = match &marker {
                ExpectMarker::CompileError(_) => found.iter().any(|m| matches!(m, ExpectMarker::CompileError(_))),
                ExpectMarker::CcError(_)      => found.iter().any(|m| matches!(m, ExpectMarker::CcError(_))),
                ExpectMarker::RuntimePanic(_) => found.iter().any(|m| matches!(m, ExpectMarker::RuntimePanic(_))),
                ExpectMarker::ExitCode(_)     => found.iter().any(|m| matches!(m, ExpectMarker::ExitCode(_))),
                // STDOUT, STDERR, COMPILE_WARNING, LINT_WARNING allow multiple patterns.
                ExpectMarker::Stdout(_) | ExpectMarker::Stderr(_)
                | ExpectMarker::CompileWarning(_) | ExpectMarker::LintWarning(_) => false,
            };
            if is_dup {
                eprintln!(
                    "warning: duplicate D89 EXPECT marker (type already present) — ignoring: {:?}",
                    marker
                );
            } else {
                found.push(marker);
            }
        }
    }
    found
}

// ---------- toolchain detection ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dev,
    Release,
}

impl Mode {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "dev" => Ok(Mode::Dev),
            "release" => Ok(Mode::Release),
            _ => Err(anyhow!("unknown mode `{}` (expected dev|release)", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainPref {
    Auto,
    Clang,
    Msvc,
    Gcc,
}

impl ToolchainPref {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(ToolchainPref::Auto),
            "clang" => Ok(ToolchainPref::Clang),
            "msvc" => Ok(ToolchainPref::Msvc),
            "gcc" => Ok(ToolchainPref::Gcc),
            _ => Err(anyhow!("unknown toolchain `{}` (expected auto|clang|msvc|gcc)", s)),
        }
    }
}

/// Конкретный детектированный toolchain. На Windows vcvars env захвачен
/// один раз при detect_toolchain — передаётся напрямую в Command::envs(),
/// избегая повторного вызова vcvars64.bat (~7 sec) на каждом тесте.
#[derive(Debug, Clone)]
pub enum Toolchain {
    /// `env`: vcvars64 env snapshot (Windows), empty on Linux/macOS.
    /// `vcvars`: path retained for detect_or_build_libuv (one-time build).
    Clang { clang: PathBuf, env: Vec<(OsString, OsString)>, vcvars: Option<PathBuf> },
    /// `env`: vcvars64 env snapshot.
    /// `vcvars`: path retained for detect_or_build_libuv (one-time build).
    Msvc { env: Vec<(OsString, OsString)>, vcvars: Option<PathBuf> },
    Gcc { gcc: PathBuf },
}

impl Toolchain {
    pub fn name(&self) -> &'static str {
        match self {
            Toolchain::Clang { .. } => "clang",
            Toolchain::Msvc { .. } => "msvc",
            Toolchain::Gcc { .. } => "gcc",
        }
    }

    /// Path to vcvars64.bat, if any. Used only by detect_or_build_libuv
    /// (one-time build) — not used for per-test compilation.
    pub fn vcvars_path(&self) -> Option<&Path> {
        match self {
            Toolchain::Clang { vcvars, .. } => vcvars.as_deref(),
            Toolchain::Msvc { vcvars, .. } => vcvars.as_deref(),
            Toolchain::Gcc { .. } => None,
        }
    }
}

/// Поиск исполняемого в `PATH` — кросс-платформенный аналог `which` / `Get-Command`.
fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exe_name = if cfg!(target_os = "windows") && !name.ends_with(".exe") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(&exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn find_clang_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    if let Some(env_path) = std::env::var_os("NOVA_CLANG") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    if cfg!(target_os = "windows") {
        let candidates = [
            PathBuf::from(r"C:\Program Files\LLVM\bin\clang.exe"),
            PathBuf::from(r"C:\Program Files (x86)\LLVM\bin\clang.exe"),
        ];
        for c in &candidates {
            if c.is_file() {
                return Some(c.clone());
            }
        }
    } else {
        let candidates = [
            PathBuf::from("/usr/bin/clang"),
            PathBuf::from("/usr/local/bin/clang"),
            PathBuf::from("/opt/homebrew/bin/clang"),
        ];
        for c in &candidates {
            if c.is_file() {
                return Some(c.clone());
            }
        }
    }
    which("clang")
}

/// Plan 210 Ф.8 (Go-паритет+, 2026-07-17): runtime feature-probe for C23
/// `#embed`. NOT a version-string check — verified empirically that this is
/// necessary: on THIS machine `clang --version` reports 22.1.5 and `#embed`
/// compiles cleanly; a `docker run ubuntu:24.04` matching GitHub's
/// `ubuntu-latest` (the base `nova-gate.yml`'s `apt-get install clang` runs
/// on) gives clang **18.1.3**, where `#embed` is rejected outright
/// (`error: invalid preprocessing directive`, both `-std=c23` and
/// `-std=c2x` — the preprocessor doesn't recognize the token, this isn't a
/// semantic/type error). Different vendors/platforms number clang
/// differently (Apple clang is a well-known example) — a behavior probe is
/// the only portable answer. Cached process-wide (`OnceLock`) — the probe
/// itself is cheap (`-fsyntax-only`, no object file) but there is no reason
/// to repeat it per-blob or per-file within one compiler invocation.
static EMBED_C23_SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Returns true iff the auto-detected clang can compile a minimal `#embed`
/// probe. `false` for every non-clang toolchain (MSVC/GCC — `#embed` sidecar
/// emission is a clang-only fast path; other toolchains keep the existing
/// hex-array rendering unconditionally) and for any clang that rejects the
/// probe (older clang, or the probe's own I/O failing for any reason —
/// fails closed to the always-correct hex fallback, never fails open).
pub(crate) fn embed_c23_supported() -> bool {
    *EMBED_C23_SUPPORTED.get_or_init(|| {
        let Some(clang) = find_clang_path(None) else {
            return false;
        };
        let probe_dir = std::env::temp_dir().join(format!(
            "nova_embed_c23_probe_{}",
            std::process::id()
        ));
        if std::fs::create_dir_all(&probe_dir).is_err() {
            return false;
        }
        let bin_path = probe_dir.join("p.bin");
        let c_path = probe_dir.join("p.c");
        let ok = std::fs::write(&bin_path, [0x2Au8])
            .and_then(|_| {
                std::fs::write(
                    &c_path,
                    "static const unsigned char p[] = {\n#embed \"p.bin\"\n};\nint _use(void){return p[0];}\n",
                )
            })
            .is_ok();
        let supported = ok
            && Command::new(&clang)
                .arg("-std=c23")
                .arg("-fsyntax-only")
                .arg(&c_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
        let _ = std::fs::remove_dir_all(&probe_dir);
        supported
    })
}

fn find_gcc_path() -> Option<PathBuf> {
    if let Some(env_path) = std::env::var_os("NOVA_GCC") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    if !cfg!(target_os = "windows") {
        let candidates = [
            PathBuf::from("/usr/bin/gcc"),
            PathBuf::from("/usr/local/bin/gcc"),
        ];
        for c in &candidates {
            if c.is_file() {
                return Some(c.clone());
            }
        }
    }
    which("gcc")
}

/// Найти vcvars64.bat. На Windows — через `vswhere.exe`. На Linux/macOS — None.
fn find_vcvars(explicit: Option<&Path>) -> Option<PathBuf> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    if let Some(env_path) = std::env::var_os("NOVA_VCVARS") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    let pf86 = std::env::var("ProgramFiles(x86)").ok()?;
    let vswhere = PathBuf::from(&pf86)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    if !vswhere.is_file() {
        return None;
    }
    let output = Command::new(&vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-find",
            r"VC\Auxiliary\Build\vcvars64.bat",
        ])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let p = PathBuf::from(line.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Capture the environment produced by vcvars64.bat once.
/// Returns key-value pairs suitable for `Command::envs()`.
/// Calling vcvars once at startup and passing its env directly to clang/cl
/// avoids the ~7-second `call vcvars64.bat` overhead on every compile.
#[cfg(target_os = "windows")]
fn capture_vcvars_env(vcvars: &Path) -> Result<Vec<(OsString, OsString)>> {
    let inner = format!(
        "\"call \"{}\" > nul && set\"",
        vcvars.display()
    );
    let mut cmd = Command::new("cmd");
    cmd.raw_arg("/c").raw_arg(&inner);
    let out = cmd.output().map_err(|e| anyhow!("spawn cmd: {}", e))?;
    if !out.status.success() {
        return Err(anyhow!("vcvars64.bat failed (exit {:?})", out.status.code()));
    }
    let stdout = bytes_to_string(&out.stdout);
    let mut vars: Vec<(OsString, OsString)> = Vec::new();
    for line in stdout.lines() {
        if let Some(eq) = line.find('=') {
            let key = &line[..eq];
            let val = &line[eq + 1..];
            vars.push((OsString::from(key), OsString::from(val)));
        }
    }
    Ok(vars)
}

#[cfg(not(target_os = "windows"))]
fn capture_vcvars_env(_vcvars: &Path) -> Result<Vec<(OsString, OsString)>> {
    Ok(vec![])
}

pub struct ToolchainOpts<'a> {
    pub pref: ToolchainPref,
    pub explicit_clang: Option<&'a Path>,
    pub explicit_vcvars: Option<&'a Path>,
}

pub fn detect_toolchain(opts: &ToolchainOpts) -> Result<Toolchain> {
    let clang = find_clang_path(opts.explicit_clang);
    let vcvars = find_vcvars(opts.explicit_vcvars);
    let gcc = find_gcc_path();

    // Capture vcvars env once. On non-Windows this is a no-op.
    // The ~7s call vcvars64.bat cost is paid here once, not per-test.
    let vcvars_env: Option<Vec<(OsString, OsString)>> = if let Some(ref v) = vcvars {
        let env = capture_vcvars_env(v)
            .map_err(|e| anyhow!("vcvars64.bat capture failed: {}", e))?;
        Some(env)
    } else {
        None
    };

    let try_clang = || -> Result<Toolchain> {
        let clang = clang.clone().ok_or_else(|| {
            anyhow!(
                "clang not found. Install LLVM:\n  \
                 - Windows: `winget install LLVM.LLVM`\n  \
                 - Linux: `apt install clang` or `dnf install clang`\n  \
                 - macOS: ships with Xcode CLI tools\n  \
                 Or set NOVA_CLANG to clang.exe path."
            )
        })?;
        if cfg!(target_os = "windows") && vcvars_env.is_none() {
            return Err(anyhow!(
                "clang on Windows requires vcvars64.bat for MSVC SDK headers/libs. \
                 Install Visual Studio Build Tools, or set NOVA_VCVARS."
            ));
        }
        Ok(Toolchain::Clang {
            clang,
            env: vcvars_env.clone().unwrap_or_default(),
            vcvars: vcvars.clone(),
        })
    };
    let try_msvc = || -> Result<Toolchain> {
        if !cfg!(target_os = "windows") {
            return Err(anyhow!("MSVC toolchain unavailable on non-Windows OS"));
        }
        let env = vcvars_env.clone().ok_or_else(|| {
            anyhow!(
                "vcvars64.bat not found. Install Visual Studio Build Tools, \
                 or set NOVA_VCVARS to vcvars64.bat path."
            )
        })?;
        // Plan 209 Ф.6 (владелец 2026-08-11): много-TU включён по умолчанию,
        // но для MSVC он НЕ РЕАЛИЗОВАН (Ф.2 remainder — compile+link по
        // частям умеет только clang/gcc). Перевернув дефолт и не сделав
        // этого, мы бы выдали пользователю с MSVC жёсткую ошибку вместо
        // сборки — то есть починили бы скорость ценой работоспособности.
        //
        // Поэтому: молча возвращаемся к одной единице трансляции. Явно
        // попросивший (`NOVA_MULTI_TU=1`) получает прежний честный отказ —
        // он знает, чего просит; умолчание же обязано работать.
        if std::env::var("NOVA_MULTI_TU").is_err() {
            std::env::set_var("NOVA_MULTI_TU", "0");
        }
        Ok(Toolchain::Msvc { env, vcvars: vcvars.clone() })
    };
    let try_gcc = || -> Result<Toolchain> {
        let gcc = gcc.clone().ok_or_else(|| {
            anyhow!("gcc not found in PATH. Install GCC.")
        })?;
        Ok(Toolchain::Gcc { gcc })
    };

    match opts.pref {
        ToolchainPref::Clang => try_clang(),
        ToolchainPref::Msvc => try_msvc(),
        ToolchainPref::Gcc => try_gcc(),
        ToolchainPref::Auto => {
            // Windows: Clang > MSVC > GCC. Linux/macOS: Clang > GCC.
            if cfg!(target_os = "windows") {
                try_clang().or_else(|_| try_msvc()).or_else(|_| try_gcc())
            } else {
                try_clang().or_else(|_| try_gcc())
            }
        }
    }
}

// ---------- build invocation ----------

fn march_flag() -> String {
    if std::env::var("NOVA_MARCH_NATIVE").as_deref() == Ok("1") {
        "native".to_string()
    } else {
        "x86-64-v3".to_string()
    }
}

/// Plan 22 Ф.6 production: decode bytes от child-process'а (stdout/stderr
/// от cl.exe / clang / cc / ar / lib).
///
/// Strategy:
///   1. Try UTF-8 strict → если valid, использовать (zero-copy).
///   2. Если invalid UTF-8 на Windows — try CP1251 (русская локаль MSVC
///      пишет error сообщения в CP1251, не UTF-8).
///   3. Fallback — `from_utf8_lossy` (invalid bytes → U+FFFD).
///
/// Cl.exe на машине с русской локалью пишет error-сообщения в CP1251.
/// `from_utf8_lossy` превращает их в '▒' что **ломает substring-match**
/// в EXPECT_COMPILE_ERROR тестах (pattern на русском не найдётся).
pub fn bytes_to_string(b: &[u8]) -> String {
    // (1) Strict UTF-8.
    if let Ok(s) = std::str::from_utf8(b) {
        return s.to_string();
    }
    // (2) Windows CP1251 fallback.
    #[cfg(target_os = "windows")]
    {
        // Простой CP1251 → Unicode mapping (только printable + кириллица).
        // CP1251 char 0x80-0xFF → Unicode code points.
        let mut out = String::with_capacity(b.len());
        for &c in b {
            if c < 0x80 {
                out.push(c as char);
            } else {
                // CP1251 → Unicode mapping table.
                out.push(cp1251_to_char(c));
            }
        }
        return out;
    }
    // (3) Lossy fallback.
    #[allow(unreachable_code)]
    String::from_utf8_lossy(b).into_owned()
}

#[cfg(target_os = "windows")]
fn cp1251_to_char(c: u8) -> char {
    // Полный mapping CP1251 (0x80-0xFF).
    match c {
        0x80 => 'Ђ', 0x81 => 'Ѓ', 0x82 => '‚', 0x83 => 'ѓ',
        0x84 => '„', 0x85 => '…', 0x86 => '†', 0x87 => '‡',
        0x88 => '€', 0x89 => '‰', 0x8A => 'Љ', 0x8B => '‹',
        0x8C => 'Њ', 0x8D => 'Ќ', 0x8E => 'Ћ', 0x8F => 'Џ',
        0x90 => 'ђ', 0x91 => '\u{2018}', 0x92 => '\u{2019}', 0x93 => '\u{201C}',
        0x94 => '\u{201D}', 0x95 => '•', 0x96 => '–', 0x97 => '—',
        0x99 => '™', 0x9A => 'љ', 0x9B => '›',
        0x9C => 'њ', 0x9D => 'ќ', 0x9E => 'ћ', 0x9F => 'џ',
        0xA0 => '\u{A0}', 0xA1 => 'Ў', 0xA2 => 'ў', 0xA3 => 'Ј',
        0xA4 => '¤', 0xA5 => 'Ґ', 0xA6 => '¦', 0xA7 => '§',
        0xA8 => 'Ё', 0xA9 => '©', 0xAA => 'Є', 0xAB => '«',
        0xAC => '¬', 0xAD => '\u{AD}', 0xAE => '®', 0xAF => 'Ї',
        0xB0 => '°', 0xB1 => '±', 0xB2 => 'І', 0xB3 => 'і',
        0xB4 => 'ґ', 0xB5 => 'µ', 0xB6 => '¶', 0xB7 => '·',
        0xB8 => 'ё', 0xB9 => '№', 0xBA => 'є', 0xBB => '»',
        0xBC => 'ј', 0xBD => 'Ѕ', 0xBE => 'ѕ', 0xBF => 'ї',
        0xC0..=0xDF => {
            // А-Я (0xC0='А', 0xDF='Я')
            char::from_u32(0x0410 + (c - 0xC0) as u32).unwrap_or('?')
        }
        0xE0..=0xFF => {
            // а-я (0xE0='а', 0xFF='я')
            char::from_u32(0x0430 + (c - 0xE0) as u32).unwrap_or('?')
        }
        _ => '?',
    }
}

/// Plan 22: конфигурация libuv для линковки в test-exe.
/// Plan 22 F2: libuv mandatory. detect_or_build_libuv больше не возвращает
/// None — panic'ит если libuv не build'ится. Option<&'a LibuvConfig> в
/// BuildOpts остаётся для API gradual transition, но в реальном flow
/// всегда Some(_).
/// path + library file + extra runtime sources.
#[derive(Clone)]
pub struct LibuvConfig {
    pub include_dir: PathBuf,    /* path to libuv/include */
    pub lib_file: PathBuf,       /* path to libuv.lib (Windows) / libuv.a (Unix) */
    pub eventloop_src: PathBuf,  /* nova_rt/eventloop.c */
}

/// Plan 27 Ф.D (audit 2026-05-12): Boehm GC paths resolved at startup.
///
/// На Windows: vcpkg-installed gc.lib + atomic_ops.lib + headers.
/// Lookup order:
///   1. `$NOVA_GC_LIB_DIR` + `$NOVA_GC_INCLUDE_DIR` env override (CI/custom).
///   2. Local vcpkg: `<cg_include>/vcpkg_installed/x64-windows-static/`.
///   3. Global vcpkg: `$VCPKG_ROOT/installed/x64-windows-static/`.
///
/// На Linux/macOS: system libgc через `-lgc` (path-less). `include_dir`
/// проверяется только для diagnostic-hint'а (`/usr/include/gc.h` etc.).
///
/// Если backend = Boehm и detection fail → `detect_boehm` возвращает None
/// с graceful eprintln-hint'ом (см. resolve_gc_or_exit).
#[derive(Clone)]
pub struct BoehmConfig {
    /// Headers path (для `-I`). На Linux/macOS может быть None (system include).
    pub include_dir: Option<PathBuf>,
    /// Library directory (для `-L`/MSVC `/link <dir>\gc.lib`). На Linux/macOS
    /// = None → линкер ищет в system path через `-lgc`.
    pub lib_dir: Option<PathBuf>,
}

/// GC backend selection. Wired through BuildOpts → build_command.
/// Malloc = plain malloc, no GC (internal/benchmark only — any loop that
/// allocates will OOM eventually; not for production use).
/// Plan 27 Ф.4: Boehm is the default GC backend.
/// Malloc kept for runtime benchmarks/development (--gc malloc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GcKind {
    Malloc,
    #[default]
    Boehm,
}

impl GcKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "malloc" => Ok(GcKind::Malloc),
            "boehm"  => Ok(GcKind::Boehm),
            _ => Err(anyhow!("unknown gc backend `{}` (expected malloc|boehm)", s)),
        }
    }

    pub fn alloc_c_name(self) -> &'static str {
        match self {
            GcKind::Malloc => "alloc.c",
            GcKind::Boehm  => "alloc_boehm.c",
        }
    }

    /// Конвертирует в GcKindTag (без данных) для AllocConstraint проверки.
    pub fn tag(self) -> GcKindTag {
        match self {
            GcKind::Malloc => GcKindTag::Malloc,
            GcKind::Boehm  => GcKindTag::Boehm,
        }
    }
}

/// Plan 115 D214 [M-115-ffi-build-pipeline]: resolved [ffi] config.
/// Paths уже абсолютные (resolved от nova.toml dir).
#[derive(Debug, Clone, Default)]
pub struct ResolvedFfiConfig {
    pub c_shims: Vec<PathBuf>,
    pub include_dirs: Vec<PathBuf>,
    /// Plan 193 Ф.2 gap-1: linker search directories (`-L`/MSVC
    /// `/LIBPATH:`) for `libs` below — resolved absolute, same
    /// manifest-dir-relative contract as `c_shims`/`include_dirs`.
    pub lib_dirs: Vec<PathBuf>,
    pub libs: Vec<String>,
    /// Plan 193 Ф.2 gate-3: vendored C source dirs for generic
    /// build-and-cache (`manifest::FfiConfig::vendor_src_dirs` doc-comment).
    pub vendor_src_dirs: Vec<PathBuf>,
}

/// Plan 193 Ф.2 gate-3 (mbedtls-vendored, 2026-07-12): Windows
/// `std::fs::canonicalize` always returns a `\\?\`-verbatim-prefixed path —
/// the origin here is `manifest::find_manifest`'s canonicalize call, which
/// `ResolvedFfiConfig::from_manifest`'s `manifest_dir.join(p)` inherits for
/// EVERY `[ffi]`-declared path. Empirically confirmed (both cl.exe AND
/// clang, so not an MSVC-only quirk): a `\\?\`-prefixed path given as a
/// SOURCE FILE or `-I`/`/I` search directory fails to resolve (`c1: fatal
/// error C1083` / `fatal error: '...' file not found`) even though the
/// underlying file/dir exists and the SAME path minus the prefix compiles
/// clean — this had simply never been exercised end-to-end before
/// (existing `[ffi]` consumers' shims are either header-free or don't
/// `#include` anything from their own declared `include_dirs`). Strip the
/// prefix once, here, at the one place ALL `[ffi]` paths are constructed —
/// every downstream consumer (`build_command`'s 3 toolchain branches, the
/// vendor build-and-cache mechanism below) inherits the fix for free.
/// A no-op on non-Windows (canonicalize there never adds this prefix).
/// `pub(crate)` (Ф.1 #268 link-prep extraction, 2026-08-02): `link_prep.rs`
/// (vendor-FFI build-and-cache, moved out of this file) needs it too — the
/// contract/ownership stays HERE (this is where `[ffi]` paths are first
/// constructed, in `ResolvedFfiConfig::from_manifest` below), `link_prep`
/// just imports it.
pub(crate) fn strip_verbatim_prefix(p: &Path) -> PathBuf {
    match p.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => p.to_path_buf(),
    }
}

impl ResolvedFfiConfig {
    /// Plan 115: собрать resolved `[ffi]` из манифеста (paths → абсолютные
    /// от директории `nova.toml`, D214 doc-contract — см.
    /// `manifest::FfiConfig`). Plan 193 Ф.1 continuation (2026-07-12): было
    /// `m.source_root`, которая расходится с `nova.toml`-директорией для
    /// legacy `[lib] src = "<subdir>"` (напр. `nova-tls`'s `src = "src"`),
    /// ломая `c_shims`/`include_dirs` на любом пакете с non-trivial `[lib]
    /// src` — `manifest_dir` всегда = директория `nova.toml`.
    /// None — у манифеста нет `[ffi]`.
    pub fn from_manifest(m: &crate::manifest::Manifest) -> Option<Self> {
        let cfg = m.ffi.as_ref()?;
        let base = m.manifest_dir.clone();
        Some(ResolvedFfiConfig {
            c_shims: cfg.c_shims.iter().map(|p| strip_verbatim_prefix(&base.join(p))).collect(),
            include_dirs: cfg.include_dirs.iter().map(|p| strip_verbatim_prefix(&base.join(p))).collect(),
            lib_dirs: cfg.lib_dirs.iter().map(|p| strip_verbatim_prefix(&base.join(p))).collect(),
            libs: cfg.libs.clone(),
            vendor_src_dirs: cfg.vendor_src_dirs.iter().map(|p| strip_verbatim_prefix(&base.join(p))).collect(),
        })
    }

    /// Plan 03.1 (ext-dep native/FFI propagation): смёржить `[ffi]` внешней
    /// (`path`/`git`) зависимости в СВОЙ resolved config. `c_shims` /
    /// `include_dirs` / `lib_dirs` / `libs` — конкатенация (свои первыми,
    /// `lib_dirs`/`libs` дедуплицируются).
    pub fn merge(&mut self, other: ResolvedFfiConfig) {
        self.c_shims.extend(other.c_shims);
        self.include_dirs.extend(other.include_dirs);
        for dir in other.lib_dirs {
            if !self.lib_dirs.contains(&dir) {
                self.lib_dirs.push(dir);
            }
        }
        for lib in other.libs {
            if !self.libs.contains(&lib) {
                self.libs.push(lib);
            }
        }
        for dir in other.vendor_src_dirs {
            if !self.vendor_src_dirs.contains(&dir) {
                self.vendor_src_dirs.push(dir);
            }
        }
    }
}

/// Ф.1 (#268 [M-tls-vendor-autobuild-not-on-build-path], 2026-08-02):
/// `ffi_lib_candidate_names`/`first_missing_ffi_lib` moved to
/// `crate::link_prep` — the shared link-preparation module now called from
/// BOTH `nova build` (`nova-cli::cmd_build`) and `nova test` (`run_one`
/// below), instead of living only in this test-runner file. Re-exported
/// here via `use` so the rest of this file's call sites (`ffi_have_defines`,
/// `run_one`) need no further changes beyond the `link_prep::` prefix.
use crate::link_prep::first_missing_ffi_lib;

/// Plan 193 Ф.2 gate-3 (mbedtls-vendored, 2026-07-12): generic compile-time
/// feature-gate defines for `[ffi] libs` — `NOVA_FFI_HAVE_<LIB>` (sanitized
/// uppercase) per lib name, emitted ONLY when `first_missing_ffi_lib`
/// confirms all declared libs are actually present (never emitted on a path
/// that's about to SKIP — a shim gated on this define must never reference
/// symbols the linker won't find). Lets a package's own `.c` shim tell real
/// backend from feature-gate-stub at compile time WITHOUT the compiler
/// hardcoding any specific library's name (mirrors the now-guarded-off
/// built-in `NOVA_USE_MBEDTLS` monorepo special-case one层 up, generalized —
/// nova-tls's `native/tls_c_shim.c` checks `NOVA_FFI_HAVE_MBEDTLS`).
/// `lib_dirs` empty (no explicit search path, same precedent as
/// `first_missing_ffi_lib`) → no defines (can't verify presence, and the
/// legacy system-`-l`-search consumers predate this mechanism / don't need
/// it — unchanged behaviour).
fn ffi_have_defines(ffi: &ResolvedFfiConfig) -> Vec<String> {
    if ffi.lib_dirs.is_empty() || !ffi.libs.is_empty() && first_missing_ffi_lib(ffi).is_some() {
        return Vec::new();
    }
    ffi.libs.iter().map(|lib| {
        let sanitized: String = lib.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
            .collect();
        format!("NOVA_FFI_HAVE_{}", sanitized)
    }).collect()
}

/// Ф.1 (#268 [M-tls-vendor-autobuild-not-on-build-path], 2026-08-02):
/// `VENDOR_FFI_BUILD_LOCK`/`build_missing_vendor_ffi_libs`/
/// `build_vendor_ffi_lib` moved to `crate::link_prep` (the shared
/// link-preparation module now called from both `nova build` and
/// `nova test`) — re-exported here so `run_one` below needs no further
/// changes beyond this `use`.
pub use crate::link_prep::build_missing_vendor_ffi_libs;

/// Параметры сборки одного теста.
pub struct BuildOpts<'a> {
    pub c_file: &'a Path,
    pub exe_file: &'a Path,
    pub obj_dir: &'a Path,
    pub cg_include: &'a Path,
    pub rt_dir: &'a Path,
    pub mode: Mode,
    pub libuv: Option<&'a LibuvConfig>,
    /// Plan 27 Ф.1: GC backend. Default = Malloc (current behavior).
    pub gc_kind: GcKind,
    /// Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI shim files + libs
    /// from `[ffi]` section в package nova.toml. None — нет [ffi] config'а
    /// для test_file's package; пустой Some(...) — секция есть но пуста.
    pub ffi: Option<&'a ResolvedFfiConfig>,
    /// Plan 149 D233: `[runtime]` fiber arena tuning from package nova.toml.
    /// Baked as -DNOVA_FIBER_STACK_DEFAULT / -DNOVA_MAX_FIBERS_DEFAULT (raw
    /// integers). None — нет [runtime] секции → builtin #define defaults.
    pub runtime: Option<&'a crate::manifest::RuntimeConfig>,
}

/// Windows system libs needed by libuv (linker dependencies).
#[cfg(target_os = "windows")]
const LIBUV_WIN_SYSLIBS: &[&str] = &[
    "ws2_32.lib", "iphlpapi.lib", "psapi.lib", "userenv.lib",
    "user32.lib", "shell32.lib", "ole32.lib", "uuid.lib",
    "advapi32.lib", "dbghelp.lib",
];

/// Linux system libs needed by libuv.
#[cfg(target_os = "linux")]
const LIBUV_UNIX_SYSLIBS: &[&str] = &["-lpthread", "-ldl", "-lrt", "-lm"];

#[cfg(target_os = "macos")]
const LIBUV_UNIX_SYSLIBS: &[&str] = &["-lpthread", "-ldl", "-lm"];

/// Plan 149 D233: build the `-D...DEFAULT` flags for the `[runtime]` section.
/// `prefix` is `-D` (clang/gcc) or `/D` (MSVC). fiber_stack → bytes,
/// max_fibers → count, via parse_size_to_bytes (mirrors the C parser). The
/// value MUST be a raw integer (it feeds a C `#define X <int>` consumed by
/// `#ifndef`). Unparseable toml value → build warning + SKIP the -D (fall back
/// to builtin #define) — never pass garbage to the compiler.
fn runtime_define_args(runtime: Option<&crate::manifest::RuntimeConfig>,
                       prefix: &str) -> Vec<String> {
    let mut args = Vec::new();
    let Some(rc) = runtime else { return args; };
    if let Some(fs) = &rc.fiber_stack {
        match crate::manifest::parse_size_to_bytes(fs) {
            Some(bytes) => args.push(format!("{}NOVA_FIBER_STACK_DEFAULT={}", prefix, bytes)),
            None => eprintln!(
                "nova: warning: [runtime] fiber_stack = \"{}\" unparseable — ignoring (using builtin 4MB default)",
                fs),
        }
    }
    if let Some(mf) = &rc.max_fibers {
        match crate::manifest::parse_size_to_bytes(mf) {
            Some(count) => args.push(format!("{}NOVA_MAX_FIBERS_DEFAULT={}", prefix, count)),
            None => eprintln!(
                "nova: warning: [runtime] max_fibers = \"{}\" unparseable — ignoring (using builtin 16384 default)",
                mf),
        }
    }
    args
}

/// Plan 174.4: effect-registry compile-time size. Codegen emits a
/// `/* nova-effect-count: N */` marker on line 1 of the generated `.c`
/// (N = distinct effects: built-in Fail/Time/Mem + user, from `effect_schemas`).
/// The `.c` and every runtime `.c` are compiled together in ONE cc invocation, so
/// we pass `-DNOVA_MAX_EFFECT_STORAGES=N` (prefix `-D` clang/gcc, `/D` MSVC) to the
/// whole command → `NovaEffectRegistry`/`NovaEffectSnapshot` get an identical array
/// size in every TU (a per-.c `#define` would size the generated TU differently from
/// runtime `effects.c` and corrupt the TLS registry). Reads only the first line.
/// Returns `None` (→ effects.h `#ifndef` fallback 32, uniform) if the marker is
/// absent or unparseable — never passes garbage to the compiler.
fn effect_count_define_arg(c_file: &Path, prefix: &str) -> Option<String> {
    use std::io::BufRead;
    let f = std::fs::File::open(c_file).ok()?;
    let mut first = String::new();
    std::io::BufReader::new(f).read_line(&mut first).ok()?;
    effect_count_define_arg_from_line(&first, prefix)
}

/// Plan 209 Ф.2: same parse as [`effect_count_define_arg`], factored out so the
/// multi-TU toolchain (which holds `common_h` in memory — the effect-count
/// marker is ALWAYS its first line, recon-notes.md §1) can reuse the exact
/// same logic without round-tripping through a file on disk.
fn effect_count_define_arg_from_line(first_line: &str, prefix: &str) -> Option<String> {
    let n: u32 = first_line
        .split("nova-effect-count:")
        .nth(1)?
        .trim()
        .trim_end_matches("*/")
        .trim()
        .parse()
        .ok()?;
    if n == 0 { return None; }
    Some(format!("{}NOVA_MAX_EFFECT_STORAGES={}", prefix, n))
}


/// Возвращает command, готовую к запуску. Для Clang/MSVC на Windows
/// инкапсулирует cmd /c "vcvars && actual-cmd" — иначе headers/libs
/// MSVC SDK недоступны.
fn build_command(tc: &Toolchain, opts: &BuildOpts) -> Command {
    // Plan 27 Ф.1: alloc source chosen by GC backend.
    let rt_alloc = opts.rt_dir.join(opts.gc_kind.alloc_c_name());
    let rt_effects = opts.rt_dir.join("effects.c");
    let rt_fibers = opts.rt_dir.join("fibers.c");
    // Plan 44.2 Etap 1: fiber stack arena POSIX (mmap). Windows-branch
    // файла — no-op marker.
    let rt_fiber_arena = opts.rt_dir.join("fiber_arena.c");
    // Plan 82 Ф.1: fiber stack arena Windows (VirtualAlloc lazy-commit).
    // POSIX-branch файла — no-op marker. Оба файла линкуются всегда,
    // каждый — пустой TU вне своей ОС.
    let rt_fiber_arena_win = opts.rt_dir.join("fiber_arena_win.c");
    // Plan 44.2 Etap 3: cross-platform stats wrappers for std.runtime.fibers.
    let rt_fiber_stats = opts.rt_dir.join("fiber_stats.c");
    // Plan 44 Этап 0: M:N runtime (opt-in через nova_runtime_init).
    let rt_runtime = opts.rt_dir.join("runtime.c");
    // Plan 83.11 Ф.2: centralized I/O driver — dedicated thread с UV loop.
    let rt_driver = opts.rt_dir.join("driver.c");
    /* Plan 61 Ф.1: TypeId weak-fallback (nova_typeid_to_name). Codegen может
     * emit'ить overriding implementation в preamble; weak fallback —
     * safety-net для minimal tests. */
    let rt_typeid = opts.rt_dir.join("typeid.c");
    /* Plan 83.11 §12.31: in-process SEGV crash localizer. Gated by
     * NOVA_DIAG_SEGV env var; no overhead when unset. No-op TU on non-Windows. */
    let rt_segv_diag = opts.rt_dir.join("segv_diag.c");
    // Plan 83.12/183: net.c — std/net substrate (one FFI layer, byte transport,
    // zero-copy, M:N-safe), compiled only when libuv is available (conditional
    // on libuv presence, added inside the libuv if-let blocks per toolchain).
    // Plan 182 Ф.1: net2.c renamed to net.c (old std/net removed, net2 promoted).
    let rt_net = opts.rt_dir.join("net.c");
    // Plan 176 Ф.2: fs.c — std/fs async uv_fs_* backend, same libuv gating as net.c.
    let rt_fs = opts.rt_dir.join("fs.c");
    // Plan 265 Ф.1: process.c — std/os subprocess substrate (uv_spawn/uv_process_t),
    // same libuv gating as net.c/fs.c.
    let rt_process = opts.rt_dir.join("process.c");
    let march = march_flag();

    // Plan 218: prebuilt runtime archive. If a bucket-matching `libnova_rt`
    // is cached (or gets built here, one-time), link it instead of adding
    // every rt_* source below as an individual per-build compile unit —
    // see the "Plan 218" doc block above `detect_or_build_rt_archive` for
    // the bucket-key rationale (effect-count/runtime-define ABI hazard).
    // `None` (disabled via `NOVA_RT_ARCHIVE=0`, or any build failure) keeps
    // the exact pre-218 behavior below — zero regression by construction.
    let rt_archive: Option<RtArchiveConfig> = opts.rt_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|repo_root| detect_or_build_rt_archive(opts.rt_dir, repo_root, tc, opts));
    let use_rt_archive = rt_archive.is_some();

    // Plan 27 Ф.1+Ф.D + #269 Ф.2: Boehm paths resolved via detect_boehm (env
    // overrides + local vcpkg + global vcpkg), falling back — #269 Ф.2 — to
    // the vendored bdwgc submodule build when neither is present (mirrors
    // `resolve_gc_or_exit`'s own fallback exactly; MUST be consulted here
    // too, not just at the early honest-exit check above, otherwise a
    // successful fallback build there is invisible to the ACTUAL compile
    // flags computed below — `vcpkg_include`/`vcpkg_lib` would silently
    // fall through to their `unwrap_or_else` legacy vcpkg-path default,
    // which doesn't exist on a fallback-built clean clone). Idempotent —
    // the fallback fn's own cache check makes this a cheap disk stat on the
    // (overwhelmingly common) case where `resolve_gc_or_exit` already ran.
    // На Linux/macOS Some(BoehmConfig) с include_dir=Some из system path,
    // lib_dir=None — линкер через -lgc. Под Windows detect_boehm/fallback
    // всегда даёт both Some(...). Если backend = Malloc → cfg = None, paths
    // не используются.
    let boehm_cfg = if opts.gc_kind == GcKind::Boehm {
        detect_boehm(opts.cg_include).or_else(|| {
            opts.rt_dir.parent().and_then(|p| p.parent())
                .and_then(|repo_root| detect_or_build_boehm_fallback(opts.rt_dir, repo_root, tc.vcvars_path()))
        })
    } else {
        None
    };
    // Legacy fallback path (для случаев когда detect_boehm вернул None и
    // mistake'нно дошли до build_command — например тест прямо вызывает
    // build_command минуя resolve_gc_or_exit). Оставляем как safety-net.
    let vcpkg_include = boehm_cfg.as_ref()
        .and_then(|c| c.include_dir.clone())
        .unwrap_or_else(|| opts.cg_include
            .join("vcpkg_installed")
            .join("x64-windows-static")
            .join("include"));
    let vcpkg_lib = boehm_cfg.as_ref()
        .and_then(|c| c.lib_dir.clone())
        .unwrap_or_else(|| opts.cg_include
            .join("vcpkg_installed")
            .join("x64-windows-static")
            .join("lib"));

    // Plan 22: libuv linkage. Если libuv config present — добавляем
    // eventloop.c в sources, -DNOVA_USE_LIBUV=1, libuv include, libuv.lib
    // + Windows system libs.
    let libuv_eventloop = opts.libuv.map(|c| c.eventloop_src.clone());
    let libuv_include = opts.libuv.map(|c| c.include_dir.clone());
    let libuv_lib = opts.libuv.map(|c| c.lib_file.clone());

    match tc {
        Toolchain::Clang { clang, env, .. } => {
            // GCC-style flags. Target явный (msvc/linux/darwin).
            let target = if cfg!(target_os = "windows") {
                "--target=x86_64-pc-windows-msvc"
            } else if cfg!(target_os = "macos") {
                "" // системный default
            } else {
                "" // linux: default
            };
            let mut flags: Vec<String> = match opts.mode {
                // Plan 140 Ф.1 (D24 amend): контракты эмитятся безусловно
                // (enforce-with-elision), `#ifdef NOVA_CONTRACTS_RUNTIME` снят
                // на codegen → флаг `-DNOVA_CONTRACTS_RUNTIME=1` больше не нужен
                // ни в debug, ни в release. Недоказанные контракты проверяются
                // в обоих режимах; Z3-proven элидируются на codegen (zero-cost).
                // Build-opt-out (`--contracts=off`) — Ф.2.
                Mode::Dev => vec![
                    "-O0".to_string(),
                    "-g".to_string(),
                    "-Wno-everything".to_string(),
                ],
                Mode::Release => vec![
                    "-O3".to_string(),
                    "-flto".to_string(),
                    format!("-march={}", march),
                    "-DNDEBUG".to_string(),
                    "-Wno-everything".to_string(),
                ],
            };
            if !target.is_empty() {
                flags.insert(0, target.to_string());
            }
            // Plan 81 Ф.7.1: linker-level DCE. -ffunction-sections /
            // -fdata-sections кладут каждую функцию/данные в отдельную
            // секцию; линкер затем удаляет неиспользуемые. Отсечение
            // делает линкер (как в Go) — без анализа в компиляторе,
            // near-zero риск. На Linux/macOS активируется -Wl,--gc-sections
            // (ниже, cfg-блок); на Windows lld-link folding включён по
            // умолчанию (/OPT:REF) — секции дают линкеру гранулярность.
            flags.push("-ffunction-sections".to_string());
            flags.push("-fdata-sections".to_string());
            // Plan 82 Ф.5: release-mode добавляет -flto; LLVM LTO требует
            // LLVM-линкера. Без -fuse-ld=lld clang на Windows падает
            // «error: LTO requires -fuse-ld=lld» (MSVC link.exe не умеет
            // LLVM LTO). Чинит `nova bench` и `nova test --mode release`
            // на Windows; lld поставляется в комплекте LLVM.
            #[cfg(target_os = "windows")]
            if matches!(opts.mode, Mode::Release) {
                flags.push("-fuse-ld=lld".to_string());
            }
            // Plan 198 (defect #8a insurance, NOT the fix): a modest stack
            // RESERVE safety margin on Windows. The real fix for the merged-CU
            // stack overflow is bounding `nova_fn_main_impl`'s C frame via
            // fixed-size test-chunk functions (see emit_main_wrapper /
            // TEST_CHUNK_SIZE in emit_c.rs) — that keeps the frame constant
            // regardless of corpus size. This flag is pure belt-and-suspenders:
            // RESERVE only consumes address space (pages commit lazily), so a
            // generous bump costs nothing at rest and gives headroom for
            // legitimately deep call stacks (generics/recursion) in real test
            // bodies, without masking a still-broken O(N) frame. Both
            // lld-link and link.exe accept `/stack:<reserve>`.
            #[cfg(target_os = "windows")]
            flags.push("-Wl,/stack:0x1000000".to_string()); // 16 MiB reserve
            // Plan 44.2 P41-5 + audit round 5: stack-clash protection (CVE-2017-1000366).
            // -fstack-clash-protection inserts page-by-page probing on stack frames
            // >4KB, preventing skip past single guard page in one SP subtraction.
            // -fstack-protector-strong adds canaries on functions with arrays.
            // Linux/macOS clang/gcc support. Windows clang-cl/MSVC: skip (different
            // mechanisms via /GS by default).
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                flags.push("-fstack-clash-protection".to_string());
                flags.push("-fstack-protector-strong".to_string());
                // Plan 81 Ф.7.1: GNU ld / lld удаляют неиспользуемые
                // секции (function/data sections выше). На Windows
                // lld-link делает то же по умолчанию (/OPT:REF).
                flags.push("-Wl,--gc-sections".to_string());
            }
            // Plan 44.5: NOVA_GC_BOEHM activates GC root registration in fibers.h.
            // GC_THREADS — Boehm compiled with -DGC_THREADS (vcpkg build.ninja confirms);
            // client side must define it too to expose GC_register_my_thread / GC_allow_register_threads.
            // Required for M:N workers (Plan 44.5 Layer 4+5).
            if opts.gc_kind == GcKind::Boehm {
                flags.push("-DNOVA_GC_BOEHM".to_string());
                flags.push("-DGC_THREADS".to_string());
            }
            // Plan 149 D233: nova.toml [runtime] → -DNOVA_FIBER_STACK_DEFAULT /
            // -DNOVA_MAX_FIBERS_DEFAULT (raw ints). After GC, before libuv.
            for da in runtime_define_args(opts.runtime, "-D") {
                flags.push(da);
            }
            // Plan 174.4: effect-registry size (all TUs, ABI-uniform).
            if let Some(da) = effect_count_define_arg(opts.c_file, "-D") {
                flags.push(da);
            }

            // Direct clang invocation with pre-captured vcvars env.
            // On Windows: env snapshot from capture_vcvars_env() at detect_toolchain() time.
            // Saves ~7s per test by avoiding `call vcvars64.bat` on every compile.
            let mut c = Command::new(clang);
            if !env.is_empty() {
                // Replace process env with the vcvars snapshot so clang sees
                // INCLUDE, LIB, PATH from VS Build Tools without re-running the bat.
                c.env_clear().envs(env.iter().cloned());
            }
            for f in &flags {
                if !f.is_empty() {
                    c.arg(f);
                }
            }
            c.arg("-I").arg(opts.cg_include);
            // Plan 22 libuv (cross-platform): defines + include path only
            // here. [M-linux-mn-conformance-red] fix (2026-07-20): the
            // LIBRARY (`libuv.a`) and any libuv-dependent LOOSE SOURCES
            // (`rt_net.c`/`rt_fs.c`, non-archive path) are placed LATER —
            // right after `opts.c_file` + the rt-archive/individual rt_*
            // sources are added (see the matching block below, right before
            // FFI libs). Root cause: GNU `ld` resolves archive members only
            // against symbols undefined AT THE MOMENT the archive is seen on
            // the command line; a reference appearing LATER (from an object
            // added AFTER the archive) is never satisfied — confirmed
            // empirically on WSL2/Linux (`nova test`, Plan 218 rt-archive
            // path, default-on): `undefined reference to uv_strerror`
            // (`fibers.h`'s `_nova_sleep_via_libuv`/`nova_blocking_offload`,
            // folded into `libnova_rt.a`) because `libuv.a` used to be added
            // HERE — before `libnova_rt.a`/`opts.c_file` further down.
            // Windows/MSVC's linker does a full symbol-table pass (not
            // strictly left-to-right for `.lib`), so the Windows sub-block
            // below stays at this ORIGINAL (early) position — zero behavior
            // change there, this split only takes effect on non-Windows.
            if let (Some(inc_path), Some(_lib_path), Some(_evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                c.arg("-DNOVA_USE_LIBUV=1");
                c.arg("-I").arg(inc_path);
                // Windows: libuv link via -L/-l flags (env has LIB set by vcvars).
                #[cfg(target_os = "windows")]
                {
                    // Plan 83.12/183: net.c compiled only when libuv is present.
                    // Plan 218: already inside libnova_rt.lib when the archive is
                    // active — skip re-adding as a loose source (double-define).
                    if !use_rt_archive {
                        c.arg(&rt_net);
                        // Plan 176 Ф.2: fs.c — std/fs backend, same libuv gate.
                        c.arg(&rt_fs);
                        // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
                        c.arg(&rt_process);
                    }
                    c.arg(_lib_path);
                    if !use_rt_archive {
                        c.arg(_evloop);
                    }
                    for syslib in LIBUV_WIN_SYSLIBS {
                        c.arg(format!("-l{}", syslib.replace(".lib", "")));
                    }
                }
            }
            // Plan 27 Ф.1+Ф.D: Boehm link flags for Clang.
            if opts.gc_kind == GcKind::Boehm {
                #[cfg(target_os = "windows")]
                {
                    c.arg("-I").arg(&vcpkg_include);
                    c.arg("-L").arg(&vcpkg_lib);
                    c.arg("-lgc");
                    // #269 Ф.2: conditional on existing — see the matching
                    // MSVC-branch comment (`atomic_ops_lib.is_file()`
                    // above) for the full rationale: the vcpkg-built
                    // `bdwgc` port links a separate `atomic_ops.lib`, the
                    // #269 Ф.2 fallback build doesn't produce/need one
                    // (header-only atomics on x86_64 MSVC).
                    if vcpkg_lib.join("atomic_ops.lib").is_file() {
                        c.arg("-latomic_ops");
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    // Linux/macOS: если detect_boehm нашёл non-system path
                    // (например Homebrew /opt/homebrew или env override) —
                    // передаём явно. Иначе linker ищет в system path через -lgc.
                    if let Some(cfg) = &boehm_cfg {
                        if let Some(inc) = &cfg.include_dir {
                            // Передаём только если non-default (не /usr/include).
                            let s = inc.to_string_lossy();
                            if !s.starts_with("/usr/include") {
                                c.arg("-I").arg(inc);
                            }
                        }
                        if let Some(lib) = &cfg.lib_dir {
                            c.arg("-L").arg(lib);
                        }
                    }
                    c.arg("-lgc");
                    #[cfg(target_os = "linux")]
                    c.arg("-lpthread");
                }
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI flags
            // BEFORE -o + c_file. include_dirs → -I; .h shims → -include
            // (force-included в каждый TU AFTER cg_include setup чтобы
            // shim header мог `#include "nova_rt/nova_rt.h"`); .c shims
            // → отдельные compilation units; libs (-l) в link phase ниже.
            if let Some(ffi) = opts.ffi {
                for inc in &ffi.include_dirs {
                    c.arg("-I").arg(inc);
                }
                // Plan 193 Ф.2 gate-3: generic feature-gate defines — see
                // `ffi_have_defines` doc-comment.
                for def in ffi_have_defines(ffi) {
                    c.arg(format!("-D{}=1", def));
                }
                for shim in &ffi.c_shims {
                    let ext = shim.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext.eq_ignore_ascii_case("c") {
                        c.arg(shim);
                    } else if ext.eq_ignore_ascii_case("h") {
                        c.arg("-include").arg(shim);
                    }
                }
            }
            c.arg("-o").arg(opts.exe_file);
            c.arg(opts.c_file);
            // Plan 218: prebuilt archive replaces the individual rt_* source
            // args below when available (see `use_rt_archive` computed above).
            if let Some(cfg) = &rt_archive {
                c.arg(&cfg.lib_file);
            } else {
                c.arg(&rt_alloc);
                c.arg(&rt_effects);
                c.arg(&rt_fibers);
                c.arg(&rt_fiber_arena);  /* Plan 44.2 Etap 1 */
                c.arg(&rt_fiber_arena_win);  /* Plan 82 Ф.1 */
                c.arg(&rt_fiber_stats);  /* Plan 44.2 Etap 3 */
                c.arg(&rt_runtime);      /* Plan 44 Этап 0 */
                c.arg(&rt_driver);       /* Plan 83.11 Ф.2 */
                c.arg(&rt_typeid);       /* Plan 61 Ф.1 */
                c.arg(&rt_segv_diag);    /* Plan 83.11 §12.31 */
            }
            // [M-linux-mn-conformance-red] fix: libuv object/library
            // placement, non-Windows only — see the comment at the early
            // libuv defines block above (`-DNOVA_USE_LIBUV=1` site). Must
            // come AFTER `opts.c_file` + `libnova_rt.a`/individual rt_*
            // sources (just above) so `ld` sees the libuv-dependent
            // references BEFORE `libuv.a` on the command line.
            #[cfg(not(target_os = "windows"))]
            if let (Some(_inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                // Plan 83.12/183: net.c compiled only when libuv is present.
                // Plan 218: already inside libnova_rt.a when the archive is active
                // — skip re-adding as a loose source (would double-define symbols).
                if !use_rt_archive {
                    c.arg(&rt_net);
                    // Plan 176 Ф.2: fs.c — std/fs backend, same libuv gate.
                    c.arg(&rt_fs);
                    // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
                    c.arg(&rt_process);
                    c.arg(evloop);
                }
                /* Linux ld обрабатывает .a archives только для symbols
                 * undefined в момент когда archive seen. Используем
                 * --start-group / --end-group чтобы symbols искались
                 * commutative с object files в command line. */
                #[cfg(target_os = "linux")]
                c.arg("-Wl,--start-group");
                c.arg(lib_path);
                for syslib in LIBUV_UNIX_SYSLIBS {
                    c.arg(syslib);
                }
                #[cfg(target_os = "linux")]
                c.arg("-Wl,--end-group");
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: system libs (-l) в link phase.
            // Plan 193 Ф.2 gap-1: lib_dirs (-L) BEFORE -l so the linker's
            // search path includes non-default-path native libs (mirrors
            // detect_mbedtls's now-generalized -L <lib_dir> pattern).
            if let Some(ffi) = opts.ffi {
                for dir in &ffi.lib_dirs {
                    c.arg("-L").arg(dir);
                }
                for lib in &ffi.libs {
                    c.arg(format!("-l{}", lib));
                }
            }
            c
        }
        Toolchain::Msvc { env, .. } => {
            // cl.exe с pre-captured vcvars env (no bat overhead per compile).
            let mut c = Command::new("cl.exe");
            c.env_clear().envs(env.iter().cloned());
            match opts.mode {
                Mode::Dev => {
                    // /Z7 (а НЕ /Zi): CodeView в .obj без PDB. /Zi
                    // создаёт vc<N>.pdb в cwd (cl-проектная PDB); при
                    // параллельном `nova test` (16 jobs) все cl.exe'ы
                    // лезут в одну PDB → C1041 «cannot open program
                    // database». /Z7 даёт ту же отладочную информацию
                    // без shared-PDB contention (стандартное решение
                    // для параллельных билдов: Ninja/MSBuild делают
                    // также для unity-сборок).
                    c.args(["/nologo", "/W0", "/Od", "/Z7"]);
                    // Plan 140 Ф.1 (D24 amend): `/DNOVA_CONTRACTS_RUNTIME=1`
                    // снят — контракты эмитятся безусловно (enforce-with-elision),
                    // `#ifdef` на codegen больше нет. Проверяется в debug И release.
                }
                Mode::Release => { c.args(["/nologo", "/W0", "/O2", "/DNDEBUG"]); }
            }
            // /std: НЕ задаём. MSVC default («Microsoft C») — permissive
            // C99+/C11+ с расширениями: codegen эмитит struct-cast
            // `(nova_str)(x)` (GCC/Clang extension), валидный в permissive
            // mode, но в strict /std:c11 → C2440 «cannot convert struct».
            // compat-header диспатчит по `sizeof`, не по `_Generic` —
            // работает без /std:c11.
            // Plan 82 followup: GCC/Clang builtin compat для cl.exe.
            // Runtime использует __atomic_* / __builtin_*-builtin'ы (sync.h
            // Tier-1 — clang); MSVC их не имеет → C2065. /FI force-инклюдит
            // compat-header в КАЖДЫЙ TU (генерированный тест-код + nova_rt
            // .c) до любых других include'ов; macros/inline-функции
            // отображают GCC builtin'ы на _Interlocked* / _BitScan* / rdtsc.
            // Под clang-cl (`__clang__` defined) compat-header — no-op.
            c.arg("/FI").arg(opts.rt_dir.join("nova_msvc_compat.h"));
            // Plan 81 Ф.7.1: /Gy — function-level linking (каждая функция
            // в свой COMDAT); link.exe /OPT:REF (default в release) удаляет
            // неиспользуемые. MSVC-эквивалент -ffunction-sections.
            c.arg("/Gy");
            // Plan 44.5: NOVA_GC_BOEHM + GC_THREADS — Boehm compiled with -DGC_THREADS;
            // client must define it too for GC_register_my_thread API (M:N workers).
            // ВАЖНО: кавычки в аргументы НЕ добавляем вручную. `Command`
            // сам экранирует каждый аргумент для CreateProcess по правилам
            // MSVC CRT (см. clang-ветку — там пути передаются «сырыми»).
            // Ручная кавычка `/Fo"path\\"` попадёт в argv буквально → cl.exe
            // видит кавычку как часть имени → D8036 «invalid /Fo». Путь с
            // пробелом обрабатывается экранированием Command автоматически.
            if opts.gc_kind == GcKind::Boehm {
                c.arg("/DNOVA_GC_BOEHM");
                c.arg("/DGC_THREADS");
                c.arg(format!("/I{}", vcpkg_include.display()));
            }
            // Plan 149 D233: nova.toml [runtime] → /DNOVA_FIBER_STACK_DEFAULT /
            // /DNOVA_MAX_FIBERS_DEFAULT (raw ints). After GC, before libuv.
            for da in runtime_define_args(opts.runtime, "/D") {
                c.arg(da);
            }
            // Plan 174.4: effect-registry size (all TUs, ABI-uniform).
            if let Some(da) = effect_count_define_arg(opts.c_file, "/D") {
                c.arg(da);
            }
            c.arg(format!("/I{}", opts.cg_include.display()));
            // /Fo с завершающим '\' → cl.exe трактует как директорию
            // (каждый .obj по имени исходника); без '\' — как имя файла,
            // что с несколькими source-файлами даёт D8036.
            c.arg(format!("/Fo{}\\", opts.obj_dir.display()));
            c.arg(format!("/Fe{}", opts.exe_file.display()));
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI shim flags (MSVC).
            // include_dirs → /I; .c shims → compilation units;
            // .h shims → /FI<header> (force-include); libs → /link <name>.lib.
            if let Some(ffi) = opts.ffi {
                for inc in &ffi.include_dirs {
                    c.arg(format!("/I{}", inc.display()));
                }
                // Plan 193 Ф.2 gate-3: generic feature-gate defines — see
                // `ffi_have_defines` doc-comment.
                for def in ffi_have_defines(ffi) {
                    c.arg(format!("/D{}=1", def));
                }
                for shim in &ffi.c_shims {
                    let ext = shim.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext.eq_ignore_ascii_case("c") {
                        c.arg(shim);
                    } else if ext.eq_ignore_ascii_case("h") {
                        c.arg("/FI").arg(shim);
                    }
                }
            }
            // Plan 22: libuv for MSVC.
            if let (Some(inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                c.arg("/DNOVA_USE_LIBUV=1");
                c.arg(format!("/I{}", inc_path.display()));
                // Plan 83.12/183: net.c compiled only when libuv is present.
                // Plan 218: already inside libnova_rt.lib when the archive is
                // active — skip re-adding as a loose source (double-define).
                if !use_rt_archive {
                    c.arg(&rt_net);
                    // Plan 176 Ф.2: fs.c — std/fs backend, same libuv gate.
                    c.arg(&rt_fs);
                    // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
                    c.arg(&rt_process);
                    c.arg(evloop);
                }
                c.arg(lib_path);
                #[cfg(target_os = "windows")]
                for syslib in LIBUV_WIN_SYSLIBS {
                    c.arg(syslib);
                }
            }
            c.arg(opts.c_file);
            // Plan 218: prebuilt archive replaces the individual rt_* source
            // args below when available (see `use_rt_archive` computed above).
            if let Some(cfg) = &rt_archive {
                c.arg(&cfg.lib_file);
            } else {
                c.arg(&rt_alloc);
                c.arg(&rt_effects);
                c.arg(&rt_fibers);
                c.arg(&rt_fiber_arena);  /* Plan 44.2 Etap 1 */
                c.arg(&rt_fiber_arena_win);  /* Plan 82 Ф.1 */
                c.arg(&rt_fiber_stats);  /* Plan 44.2 Etap 3 */
                c.arg(&rt_runtime);      /* Plan 44 Этап 0 */
                c.arg(&rt_driver);       /* Plan 83.11 Ф.2 */
                c.arg(&rt_typeid);       /* Plan 61 Ф.1 */
                c.arg(&rt_segv_diag);    /* Plan 83.11 §12.31 */
            }
            // Plan 27 Ф.1: Boehm link flags for MSVC (after sources, before /link).
            // Plan 115 D214 [M-115-ffi-build-pipeline]: also pass user FFI libs.
            // Plan 193 Ф.2 gap-1: also open the /link phase when lib_dirs is
            // declared alone (e.g. libs list empty but search path wired for
            // a future c_shim-only consumer — matches `!libs.is_empty()` OR
            // gate below).
            let has_link_phase = opts.gc_kind == GcKind::Boehm
                || opts.ffi.map_or(false, |f| !f.libs.is_empty() || !f.lib_dirs.is_empty());
            if has_link_phase {
                c.arg("/link");
                // Plan 198 (defect #8a insurance, NOT the fix — see the
                // matching clang-branch comment above): modest RESERVE-only
                // stack safety margin.
                c.arg("/STACK:0x1000000"); // 16 MiB reserve
                if opts.gc_kind == GcKind::Boehm {
                    // PathBuf-аргумент — Command экранирует сам; ручные кавычки
                    // не нужны (и вредны, см. комментарий к /Fo выше).
                    c.arg(vcpkg_lib.join("gc.lib"));
                    // #269 Ф.2: conditional on existing — vcpkg's bdwgc port
                    // links a separate `atomic_ops.lib` (its own port
                    // dependency), but the #269 Ф.2 fallback build (vendored
                    // bdwgc amalgamation) doesn't produce one: the needed
                    // atomics are header-only on x86_64 MSVC (confirmed
                    // empirically — see `detect_or_build_boehm_fallback`
                    // doc), so there is nothing to link there. Guarding on
                    // `.is_file()` keeps the vcpkg path byte-identical
                    // (file always present there) while making the
                    // fallback path not fail with a "cannot open
                    // atomic_ops.lib" linker error over a file that was
                    // never needed.
                    let atomic_ops_lib = vcpkg_lib.join("atomic_ops.lib");
                    if atomic_ops_lib.is_file() {
                        c.arg(atomic_ops_lib);
                    }
                }
                if let Some(ffi) = opts.ffi {
                    // Plan 193 Ф.2 gap-1: /LIBPATH: BEFORE bare <name>.lib
                    // entries — MSVC's `LIB` env var (vcvars-snapshot,
                    // env_clear()-isolated above) doesn't see external
                    // overrides, so a non-default-path native lib is
                    // otherwise unfindable on Windows (no system default
                    // search path analogue to /usr/lib).
                    for dir in &ffi.lib_dirs {
                        c.arg(format!("/LIBPATH:{}", dir.display()));
                    }
                    for lib in &ffi.libs {
                        // MSVC: -l<name> не поддерживается, нужен <name>.lib.
                        c.arg(format!("{}.lib", lib));
                    }
                }
            }
            c
        }
        Toolchain::Gcc { gcc } => {
            let mut c = Command::new(gcc);
            match opts.mode {
                Mode::Dev => {
                    c.args(["-O0", "-g", "-w"]);
                    // Plan 140 Ф.1 (D24 amend): `-DNOVA_CONTRACTS_RUNTIME=1`
                    // снят — контракты эмитятся безусловно (enforce-with-elision).
                }
                Mode::Release => {
                    c.arg("-O3");
                    c.arg("-flto");
                    c.arg(format!("-march={}", march));
                    c.arg("-DNDEBUG");
                    c.arg("-w");
                }
            }
            // Plan 81 Ф.7.1: linker-level DCE (GNU ld удаляет
            // неиспользуемые секции).
            c.arg("-ffunction-sections");
            c.arg("-fdata-sections");
            c.arg("-Wl,--gc-sections");
            // Plan 44.5: NOVA_GC_BOEHM + GC_THREADS for M:N worker thread registration.
            if opts.gc_kind == GcKind::Boehm {
                c.arg("-DNOVA_GC_BOEHM");
                c.arg("-DGC_THREADS");
            }
            // Plan 149 D233: nova.toml [runtime] → -DNOVA_FIBER_STACK_DEFAULT /
            // -DNOVA_MAX_FIBERS_DEFAULT (raw ints). After GC, before libuv.
            for da in runtime_define_args(opts.runtime, "-D") {
                c.arg(da);
            }
            // Plan 174.4: effect-registry size (all TUs, ABI-uniform).
            if let Some(da) = effect_count_define_arg(opts.c_file, "-D") {
                c.arg(da);
            }
            c.arg("-I").arg(opts.cg_include);
            // Plan 22 libuv (Linux): defines + include path only here.
            // [M-linux-mn-conformance-red] fix (2026-07-20): object/library
            // placement (rt_net.c/rt_fs.c loose sources + libuv.a itself)
            // MOVED below, after `opts.c_file` + rt-archive/individual rt_*
            // sources — same root cause + fix rationale as the Clang branch
            // above (GNU `ld` only resolves archive members against symbols
            // undefined AT THE POINT the archive is seen; `libuv.a` was
            // being placed BEFORE `libnova_rt.a`, so `uv_strerror` etc.
            // (referenced from fibers.h, folded into the rt-archive) never
            // resolved — confirmed empirically, `undefined reference to
            // uv_strerror`).
            if let (Some(inc_path), Some(_lib_path), Some(_evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                c.arg("-DNOVA_USE_LIBUV=1");
                c.arg("-I").arg(inc_path);
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI shim flags (GCC).
            // .h shims via -include (force-include); .c via compilation unit.
            if let Some(ffi) = opts.ffi {
                for inc in &ffi.include_dirs {
                    c.arg("-I").arg(inc);
                }
                // Plan 193 Ф.2 gate-3: generic feature-gate defines — see
                // `ffi_have_defines` doc-comment.
                for def in ffi_have_defines(ffi) {
                    c.arg(format!("-D{}=1", def));
                }
                for shim in &ffi.c_shims {
                    let ext = shim.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext.eq_ignore_ascii_case("c") {
                        c.arg(shim);
                    } else if ext.eq_ignore_ascii_case("h") {
                        c.arg("-include").arg(shim);
                    }
                }
            }
            c.arg("-o").arg(opts.exe_file);
            c.arg(opts.c_file);
            // Plan 218: prebuilt archive replaces the individual rt_* source
            // args below when available (see `use_rt_archive` computed above).
            if let Some(cfg) = &rt_archive {
                c.arg(&cfg.lib_file);
            } else {
                c.arg(&rt_alloc);
                c.arg(&rt_effects);
                c.arg(&rt_fibers);
                c.arg(&rt_fiber_arena);  /* Plan 44.2 Etap 1 */
                c.arg(&rt_fiber_arena_win);  /* Plan 82 Ф.1 */
                c.arg(&rt_fiber_stats);  /* Plan 44.2 Etap 3 */
                c.arg(&rt_runtime);      /* Plan 44 Этап 0 */
                c.arg(&rt_driver);       /* Plan 83.11 Ф.2 */
                c.arg(&rt_typeid);       /* Plan 61 Ф.1 */
                c.arg(&rt_segv_diag);    /* Plan 83.11 §12.31 */
            }
            // [M-linux-mn-conformance-red] fix: libuv object/library
            // placement — see the comment at the early libuv defines block
            // above. Must come AFTER `opts.c_file` + `libnova_rt.a`/
            // individual rt_* sources (just above) so `ld` sees the
            // libuv-dependent references BEFORE `libuv.a` on the command line.
            if let (Some(_inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                // Plan 83.12/183: net.c compiled only when libuv is present.
                // Plan 218: already inside libnova_rt.a when the archive is
                // active — skip re-adding as a loose source (double-define).
                if !use_rt_archive {
                    c.arg(&rt_net);
                    // Plan 176 Ф.2: fs.c — std/fs backend, same libuv gate.
                    c.arg(&rt_fs);
                    // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
                    c.arg(&rt_process);
                    c.arg(evloop);
                }
                #[cfg(target_os = "linux")]
                c.arg("-Wl,--start-group");
                c.arg(lib_path);
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                for syslib in LIBUV_UNIX_SYSLIBS {
                    c.arg(syslib);
                }
                #[cfg(target_os = "linux")]
                c.arg("-Wl,--end-group");
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI libs (GCC).
            // Plan 193 Ф.2 gap-1: lib_dirs (-L) BEFORE -l, same as Clang.
            if let Some(ffi) = opts.ffi {
                for dir in &ffi.lib_dirs {
                    c.arg("-L").arg(dir);
                }
                for lib in &ffi.libs {
                    c.arg(format!("-l{}", lib));
                }
            }
            // Plan 27 Ф.1+Ф.D: Boehm link flags for GCC.
            if opts.gc_kind == GcKind::Boehm {
                if let Some(cfg) = &boehm_cfg {
                    if let Some(inc) = &cfg.include_dir {
                        let s = inc.to_string_lossy();
                        if !s.starts_with("/usr/include") {
                            c.arg("-I").arg(inc);
                        }
                    }
                    if let Some(lib) = &cfg.lib_dir {
                        c.arg("-L").arg(lib);
                    }
                }
                c.arg("-lgc");
                #[cfg(target_os = "linux")]
                c.arg("-lpthread");
            }
            c
        }
    }
}

/// Plan 28 Ф.0: публичная обёртка над `build_command` + `run_with_timeout`.
/// Используется из `nova-cli` (`nova build`) минуя subprocess.
///
/// Компилирует `opts.c_file` → `opts.exe_file` через выбранный toolchain.
/// Возвращает путь к exe на success, anyhow::Error на fail.
pub fn compile_c_to_exe(
    tc: &Toolchain,
    opts: &BuildOpts,
    timeout: Duration,
) -> anyhow::Result<PathBuf> {
    // Plan 27 Ф.D + #269 Ф.2: graceful exit (or vendored fallback build) если
    // backend = Boehm и libgc не найден.
    let _ = resolve_gc_or_exit(opts.gc_kind, opts.cg_include, opts.rt_dir, tc.vcvars_path());
    let cmd = build_command(tc, opts);
    let out = run_with_timeout(cmd, timeout)
        .map_err(|e| anyhow!("spawn compiler: {}", e))?;
    let ok = out.status.map(|s| s.success()).unwrap_or(false);
    if !ok {
        let stderr = bytes_to_string(&out.stderr);
        let stdout = bytes_to_string(&out.stdout);
        let detail = if stderr.is_empty() { stdout } else { stderr };
        let reason = if out.status.is_none() {
            format!("compiler timed out after {:.1}s", timeout.as_secs_f64())
        } else {
            format!("compiler error:\n{}", detail.trim())
        };
        return Err(anyhow!("{}", reason));
    }
    Ok(opts.exe_file.to_path_buf())
}

// ---------- Plan 209 Ф.2: multi-TU parallel compile + link ----------

/// Build a `clang -c <src> -o <obj>` compile-only Command sharing `flags`/
/// `includes`/`force_includes` with every other part/runtime-object compile
/// in this CU (ABI-invariant — see `compile_multi_tu_to_exe` doc).
fn clang_compile_obj_cmd(
    clang: &Path,
    env: &[(OsString, OsString)],
    flags: &[String],
    includes: &[PathBuf],
    force_includes: &[PathBuf],
    src: &Path,
    obj: &Path,
) -> Command {
    let mut c = Command::new(clang);
    if !env.is_empty() {
        c.env_clear().envs(env.iter().cloned());
    }
    for f in flags {
        if !f.is_empty() {
            c.arg(f);
        }
    }
    for inc in includes {
        c.arg("-I").arg(inc);
    }
    for fi in force_includes {
        c.arg("-include").arg(fi);
    }
    c.arg("-c").arg(src);
    c.arg("-o").arg(obj);
    c
}

/// GCC mirror of [`clang_compile_obj_cmd`] (no vcvars env to replay).
fn gcc_compile_obj_cmd(
    gcc: &Path,
    flags: &[String],
    includes: &[PathBuf],
    force_includes: &[PathBuf],
    src: &Path,
    obj: &Path,
) -> Command {
    let mut c = Command::new(gcc);
    for f in flags {
        if !f.is_empty() {
            c.arg(f);
        }
    }
    for inc in includes {
        c.arg("-I").arg(inc);
    }
    for fi in force_includes {
        c.arg("-include").arg(fi);
    }
    c.arg("-c").arg(src);
    c.arg("-o").arg(obj);
    c
}

/// Plan 209 Ф.2 (B1-B4): parallel multi-TU compile + link. Entry point for
/// the `EmitOutput::Split { common_h, parts }` shape (Ф.1 A4,
/// `CEmitter::emit_module_multi_tu`) — the `EmitOutput::Single` shape keeps
/// going through `compile_c_to_exe`/`build_command` UNCHANGED (0 risk to the
/// default path, recon-notes.md §6 threshold-gate).
///
/// Writes `<stem>_common.h` + `<stem>_partK.c` under `opts.obj_dir`, compiles
/// every part **in parallel** (thread pool sized to
/// `available_parallelism()`) alongside the runtime/.ffi
/// "compile-once" sources, then links every resulting `.o` into one exe.
///
/// **ABI-invariant (recon-notes.md §1, §9.5):** `flags` (built once, below)
/// is the exact same `Vec<String>` passed to EVERY compile invocation
/// (parts AND runtime objects) AND to the final link — this is what
/// guarantees `-DNOVA_MAX_EFFECT_STORAGES=N` and every other `-D` stay
/// byte-identical across every TU without hand-threading each flag through
/// N call sites separately.
///
/// **MSVC unsupported** (recon-notes.md §9 неопределённость 3): `cl.exe`/
/// `link.exe`'s two-phase object+link syntax differs enough from
/// clang/gcc's `-c`/`-o` that it needs its own builder — left as a Ф.2
/// remainder (documented in 209-f2-notes.md). Callers MUST gate multi-TU
/// off for `Toolchain::Msvc` (this returns `Err` defensively if reached).
pub fn compile_multi_tu_to_exe(
    tc: &Toolchain,
    opts: &BuildOpts,
    common_h: &str,
    parts: &[String],
    timeout: Duration,
) -> anyhow::Result<PathBuf> {
    let is_gcc = matches!(tc, Toolchain::Gcc { .. });
    let (compiler, env): (PathBuf, Vec<(OsString, OsString)>) = match tc {
        Toolchain::Clang { clang, env, .. } => (clang.clone(), env.clone()),
        Toolchain::Gcc { gcc } => (gcc.clone(), vec![]),
        Toolchain::Msvc { .. } => {
            return Err(anyhow!(
                "multi-TU compile+link is not implemented for the MSVC \
                 toolchain yet (Plan 209 Ф.2 remainder) — pass \
                 --toolchain=clang, or unset NOVA_MULTI_TU"
            ));
        }
    };
    let _ = resolve_gc_or_exit(opts.gc_kind, opts.cg_include, opts.rt_dir, tc.vcvars_path());

    let stem = opts
        .c_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("cu")
        .to_string();
    let obj_dir = opts.obj_dir;
    std::fs::create_dir_all(obj_dir).map_err(|e| anyhow!("mkdir obj_dir: {}", e))?;

    // Write common.h + part_K.c side by side — parts `#include` the header
    // by its relative filename, so both must live in the same directory.
    let common_h_path = obj_dir.join(format!("{}_common.h", stem));
    std::fs::write(&common_h_path, common_h)
        .map_err(|e| anyhow!("write {}: {}", common_h_path.display(), e))?;
    let mut part_paths: Vec<PathBuf> = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        let p = obj_dir.join(format!("{}_part{}.c", stem, i));
        std::fs::write(&p, part).map_err(|e| anyhow!("write {}: {}", p.display(), e))?;
        part_paths.push(p);
    }

    // Plan 174.4: effect-registry size, read from common.h's line 1 (recon-
    // notes.md §1 doc: the marker is ALWAYS moved there by `split_tu`).
    let effect_arg = common_h.lines().next().and_then(|l| effect_count_define_arg_from_line(l, "-D"));

    let march = march_flag();
    // #269 Ф.2: same fallback-aware resolution as `build_command`/
    // `detect_or_build_rt_archive` (see those call sites' comments) — this
    // function has its OWN independent `boehm_cfg` derivation (Plan 209
    // Ф.2 multi-TU path), so it needs the same fix.
    let boehm_cfg = if opts.gc_kind == GcKind::Boehm {
        detect_boehm(opts.cg_include).or_else(|| {
            opts.rt_dir.parent().and_then(|p| p.parent())
                .and_then(|repo_root| detect_or_build_boehm_fallback(opts.rt_dir, repo_root, tc.vcvars_path()))
        })
    } else {
        None
    };
    let vcpkg_include = boehm_cfg
        .as_ref()
        .and_then(|c| c.include_dir.clone())
        .unwrap_or_else(|| opts.cg_include.join("vcpkg_installed").join("x64-windows-static").join("include"));
    let vcpkg_lib = boehm_cfg
        .as_ref()
        .and_then(|c| c.lib_dir.clone())
        .unwrap_or_else(|| opts.cg_include.join("vcpkg_installed").join("x64-windows-static").join("lib"));

    // ---- shared flags: IDENTICAL for every compile (-c) AND the final
    // link — mirrors build_command's Clang/Gcc arms so per-TU codegen
    // assumptions (target triple, -D defines, section-based DCE) stay
    // uniform across parts + runtime objects (see fn doc, ABI-invariant).
    let mut flags: Vec<String> = Vec::new();
    if !is_gcc && cfg!(target_os = "windows") {
        flags.push("--target=x86_64-pc-windows-msvc".to_string());
    }
    match opts.mode {
        Mode::Dev => {
            flags.push("-O0".into());
            flags.push("-g".into());
            flags.push(if is_gcc { "-w".into() } else { "-Wno-everything".into() });
        }
        Mode::Release => {
            flags.push("-O3".into());
            flags.push("-flto".into());
            flags.push(format!("-march={}", march));
            flags.push("-DNDEBUG".into());
            flags.push(if is_gcc { "-w".into() } else { "-Wno-everything".into() });
        }
    }
    flags.push("-ffunction-sections".into());
    flags.push("-fdata-sections".into());
    #[cfg(target_os = "windows")]
    if !is_gcc && matches!(opts.mode, Mode::Release) {
        flags.push("-fuse-ld=lld".into());
    }
    #[cfg(target_os = "windows")]
    flags.push("-Wl,/stack:0x1000000".into());
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        flags.push("-fstack-clash-protection".into());
        flags.push("-fstack-protector-strong".into());
        flags.push("-Wl,--gc-sections".into());
    }
    if opts.gc_kind == GcKind::Boehm {
        flags.push("-DNOVA_GC_BOEHM".into());
        flags.push("-DGC_THREADS".into());
    }
    for da in runtime_define_args(opts.runtime, "-D") {
        flags.push(da);
    }
    if let Some(ea) = &effect_arg {
        flags.push(ea.clone());
    }

    // ---- includes (compile-only) ----
    let mut includes: Vec<PathBuf> = vec![opts.cg_include.to_path_buf()];
    let libuv_include = opts.libuv.map(|c| c.include_dir.clone());
    if let Some(inc) = &libuv_include {
        flags.push("-DNOVA_USE_LIBUV=1".into());
        includes.push(inc.clone());
    }
    if let Some(ffi) = opts.ffi {
        for inc in &ffi.include_dirs {
            includes.push(inc.clone());
        }
        for def in ffi_have_defines(ffi) {
            flags.push(format!("-D{}=1", def));
        }
    }
    if opts.gc_kind == GcKind::Boehm {
        #[cfg(target_os = "windows")]
        includes.push(vcpkg_include.clone());
        #[cfg(not(target_os = "windows"))]
        if let Some(cfg) = &boehm_cfg {
            if let Some(inc) = &cfg.include_dir {
                let s = inc.to_string_lossy();
                if !s.starts_with("/usr/include") {
                    includes.push(inc.clone());
                }
            }
        }
    }

    // ---- force-include headers (compile-only: FFI .h shims) ----
    let mut force_includes: Vec<PathBuf> = Vec::new();
    if let Some(ffi) = opts.ffi {
        for shim in &ffi.c_shims {
            let ext = shim.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext.eq_ignore_ascii_case("h") {
                force_includes.push(shim.clone());
            }
        }
    }

    // ---- "compile once" extra sources: runtime .c + libuv eventloop +
    // FFI .c shims. Compiled to `.o` exactly once (not once
    // per part) — no regression vs. the single-TU path, which also only
    // ever compiled these once.
    let mut extra_sources: Vec<PathBuf> = vec![
        opts.rt_dir.join(opts.gc_kind.alloc_c_name()),
        opts.rt_dir.join("effects.c"),
        opts.rt_dir.join("fibers.c"),
        opts.rt_dir.join("fiber_arena.c"),
        opts.rt_dir.join("fiber_arena_win.c"),
        opts.rt_dir.join("fiber_stats.c"),
        opts.rt_dir.join("runtime.c"),
        opts.rt_dir.join("driver.c"),
        opts.rt_dir.join("typeid.c"),
        opts.rt_dir.join("segv_diag.c"),
    ];
    if let Some(libuv) = opts.libuv {
        extra_sources.push(opts.rt_dir.join("net.c"));
        extra_sources.push(opts.rt_dir.join("fs.c"));
        // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
        extra_sources.push(opts.rt_dir.join("process.c"));
        extra_sources.push(libuv.eventloop_src.clone());
    }
    if let Some(ffi) = opts.ffi {
        for shim in &ffi.c_shims {
            let ext = shim.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext.eq_ignore_ascii_case("c") {
                extra_sources.push(shim.clone());
            }
        }
    }

    // ---- job list: (src, obj) for every part + every "once" source ----
    let mut jobs: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(part_paths.len() + extra_sources.len());
    for p in &part_paths {
        let obj = p.with_extension("o");
        jobs.push((p.clone(), obj));
    }
    for (i, src) in extra_sources.iter().enumerate() {
        let base = src.file_stem().and_then(|s| s.to_str()).unwrap_or("rt");
        let obj = obj_dir.join(format!("{}_{}.o", base, i));
        jobs.push((src.clone(), obj));
    }

    // ---- parallel compile: thread pool sized to available_parallelism,
    // contiguous static partition (part/runtime .c sizes are comparable —
    // parts are each ≤ MULTI_TU_PART_THRESHOLD_BYTES, runtime files small).
    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(1)
        .min(jobs.len().max(1));
    let chunk_size = if jobs.is_empty() { 1 } else { (jobs.len() + nthreads - 1) / nthreads };
    let mut compile_results: Vec<(PathBuf, PathBuf, std::io::Result<CapturedOutput>)> = Vec::new();
    {
        let compiler_ref = &compiler;
        let env_ref = &env;
        let flags_ref = &flags;
        let includes_ref = &includes;
        let force_includes_ref = &force_includes;
        std::thread::scope(|scope| {
            let handles: Vec<_> = jobs
                .chunks(chunk_size.max(1))
                .map(|chunk| {
                    scope.spawn(move || {
                        let mut out = Vec::with_capacity(chunk.len());
                        for (src, obj) in chunk {
                            let cmd = if is_gcc {
                                gcc_compile_obj_cmd(compiler_ref, flags_ref, includes_ref, force_includes_ref, src, obj)
                            } else {
                                clang_compile_obj_cmd(compiler_ref, env_ref, flags_ref, includes_ref, force_includes_ref, src, obj)
                            };
                            let r = run_with_timeout(cmd, timeout);
                            out.push((src.clone(), obj.clone(), r));
                        }
                        out
                    })
                })
                .collect();
            for h in handles {
                if let Ok(part) = h.join() {
                    compile_results.extend(part);
                }
            }
        });
    }

    // Any compile failure (incl. spawn error / timeout) fails the whole
    // build — report the first one with its captured output.
    for (src, _obj, r) in &compile_results {
        match r {
            Ok(out) if out.status.map(|s| s.success()).unwrap_or(false) => {}
            Ok(out) => {
                let stderr = bytes_to_string(&out.stderr);
                let stdout = bytes_to_string(&out.stdout);
                let detail = if stderr.is_empty() { stdout } else { stderr };
                let reason = if out.status.is_none() {
                    format!("compiler timed out after {:.1}s", timeout.as_secs_f64())
                } else {
                    format!("compiler error ({}):\n{}", src.display(), detail.trim())
                };
                return Err(anyhow!("{}", reason));
            }
            Err(e) => return Err(anyhow!("spawn compiler ({}): {}", src.display(), e)),
        }
    }

    // ---- link: same `flags` + every `.o` + libs (mirrors build_command's
    // tail — objects, then GC, then libuv, then FFI) ----
    let mut link = Command::new(&compiler);
    if !env.is_empty() {
        link.env_clear().envs(env.iter().cloned());
    }
    for f in &flags {
        if !f.is_empty() {
            link.arg(f);
        }
    }
    link.arg("-o").arg(opts.exe_file);
    for (_src, obj, _r) in &compile_results {
        link.arg(obj);
    }
    if opts.gc_kind == GcKind::Boehm {
        #[cfg(target_os = "windows")]
        {
            link.arg("-L").arg(&vcpkg_lib);
            link.arg("-lgc");
            // #269 Ф.2: conditional on existing — see build_command's
            // matching comment for the full rationale.
            if vcpkg_lib.join("atomic_ops.lib").is_file() {
                link.arg("-latomic_ops");
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            if let Some(cfg) = &boehm_cfg {
                if let Some(lib) = &cfg.lib_dir {
                    link.arg("-L").arg(lib);
                }
            }
            link.arg("-lgc");
            #[cfg(target_os = "linux")]
            link.arg("-lpthread");
        }
    }
    if let Some(libuv) = opts.libuv {
        #[cfg(target_os = "windows")]
        {
            link.arg(&libuv.lib_file);
            for syslib in LIBUV_WIN_SYSLIBS {
                link.arg(format!("-l{}", syslib.replace(".lib", "")));
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            #[cfg(target_os = "linux")]
            link.arg("-Wl,--start-group");
            link.arg(&libuv.lib_file);
            for syslib in LIBUV_UNIX_SYSLIBS {
                link.arg(syslib);
            }
            #[cfg(target_os = "linux")]
            link.arg("-Wl,--end-group");
        }
    }
    if let Some(ffi) = opts.ffi {
        for dir in &ffi.lib_dirs {
            link.arg("-L").arg(dir);
        }
        for lib in &ffi.libs {
            link.arg(format!("-l{}", lib));
        }
    }
    let link_out = run_with_timeout(link, timeout).map_err(|e| anyhow!("spawn linker: {}", e))?;
    let link_ok = link_out.status.map(|s| s.success()).unwrap_or(false);
    if !link_ok {
        let stderr = bytes_to_string(&link_out.stderr);
        let stdout = bytes_to_string(&link_out.stdout);
        let detail = if stderr.is_empty() { stdout } else { stderr };
        let reason = if link_out.status.is_none() {
            format!("linker timed out after {:.1}s", timeout.as_secs_f64())
        } else {
            format!("linker error:\n{}", detail.trim())
        };
        return Err(anyhow!("{}", reason));
    }
    Ok(opts.exe_file.to_path_buf())
}

// ---------- Plan 27 Ф.6 / Б.2-Б.7: AllocConstraint + helper parsers ----------

/// Tag-enum без данных — используется в AllocConstraint чтобы избежать
/// circular dep между AllocConstraint и GcKind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcKindTag { Malloc, Boehm }

impl GcKindTag {
    fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "malloc" => Some(GcKindTag::Malloc),
            "boehm"  => Some(GcKindTag::Boehm),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self { GcKindTag::Malloc => "malloc", GcKindTag::Boehm => "boehm" }
    }
}

/// Из заголовка теста: `// ALLOC_REQUIRES boehm` / `// ALLOC_EXCLUDES malloc`.
#[derive(Debug, Clone, Copy)]
pub enum AllocConstraint { None, Requires(GcKindTag), Excludes(GcKindTag) }

impl AllocConstraint {
    pub fn allows(self, gc: GcKindTag) -> bool {
        match self {
            AllocConstraint::None => true,
            AllocConstraint::Requires(t) => gc == t,
            AllocConstraint::Excludes(t) => gc != t,
        }
    }
}

/// Причина пропуска теста.
#[derive(Debug, Clone)]
pub enum SkipReason {
    AllocBackend { constraint: AllocConstraint, actual: GcKindTag },
    /// Plan 33 V1: тест требует конкретный SMT backend
    /// (через `// REQUIRES_SMT_BACKEND z3`), но активный backend другой.
    SmtBackend { required: String, actual: String },
    /// [M-runner-testless-units-main-impl]: юнит без единого `test "..."`
    /// блока и без явного `fn main()` — `nova_fn_main_impl` в этом случае
    /// codegen НЕ эмитит (см. emit_main_wrapper/emit_nova_main в
    /// codegen/emit_c.rs), поэтому cc/link упал бы `undefined symbol:
    /// nova_fn_main_impl` несмотря на то что codegen (.nv → .c) прошёл
    /// успешно. Компиляция уже проверена — SKIP вместо CC-FAIL, cc/link/run
    /// не выполняются (дешевле оборвать сразу после codegen).
    NoEntryPoint,
    /// Plan 193 Ф.2 gap-1 (03.1 [ffi] libs detect-and-degrade): a declared
    /// `[ffi] libs` entry has an explicit `lib_dirs` search path, but the
    /// platform lib file was not found in any of them. Degrades to SKIP
    /// instead of a hard CC/link-FAIL — mirrors the retired built-in
    /// MbedtlsConfig/BrotliConfig graceful-degrade contract (missing native
    /// lib → never a hard link error), generalized to generic `[ffi] libs`.
    FfiLibNotFound { lib: String, searched: Vec<PathBuf> },
    /// [M-trap-tests-silent-skip-default-lane]: file was discovered but its
    /// lane (`EXPECT_*` type, or `_slow` suffix) isn't in the active
    /// `TestSelection` — see [`LaneExclusion`]. Synthesized directly by
    /// `run_all` for `walk_nv_selected_ex`'s `excluded` list (no codegen/cc/
    /// run attempted — cheaper than a real SKIP and, unlike the silent drop
    /// this replaces, always shows up as one `SKIP <path> # …` row).
    LaneExcluded { lane: &'static str, hint: &'static str },
}

impl SkipReason {
    fn description(&self) -> String {
        match self {
            SkipReason::AllocBackend { constraint, actual } => match constraint {
                AllocConstraint::Requires(t) => format!(
                    "requires gc={} but running with gc={}", t.as_str(), actual.as_str()
                ),
                AllocConstraint::Excludes(t) => format!(
                    "excluded for gc={} (running with gc={})", t.as_str(), actual.as_str()
                ),
                AllocConstraint::None => "skipped (no constraint — bug)".to_string(),
            },
            SkipReason::SmtBackend { required, actual } => format!(
                "requires NOVA_SMT_BACKEND={} but running with {}",
                required, actual,
            ),
            SkipReason::NoEntryPoint =>
                "no test blocks and no fn main() — nothing to link/run (compiled OK)".to_string(),
            SkipReason::FfiLibNotFound { lib, searched } => format!(
                "[ffi] lib `{}` not found in lib_dirs ({})",
                lib,
                searched.iter().map(|p| p.display().to_string())
                    .collect::<Vec<_>>().join(", "),
            ),
            SkipReason::LaneExcluded { lane, hint } => format!(
                "{} lane — requires {}", lane, hint,
            ),
        }
    }
}

/// Plan 33 V1: `// REQUIRES_SMT_BACKEND <name>`. Тест выполняется
/// только когда активный backend (`NOVA_SMT_BACKEND` env var, default
/// `trivial`) совпадает с указанным именем.
pub fn parse_smt_backend_requirement(src: &str) -> Option<String> {
    for line in src.lines().take(30) {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("// REQUIRES_SMT_BACKEND") {
            let name = rest.trim();
            if !name.is_empty() {
                return Some(name.to_ascii_lowercase());
            }
        }
    }
    None
}

/// Активный backend, как его читает `VerificationPipeline::from_env`.
fn active_smt_backend() -> String {
    std::env::var("NOVA_SMT_BACKEND")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "trivial".to_string())
}

/// Читает первые 30 строк файла и ищет `// ALLOC_REQUIRES <tag>` или
/// `// ALLOC_EXCLUDES <tag>`. Возвращает AllocConstraint::None если маркер не найден.
pub fn parse_alloc_constraint(src: &str) -> AllocConstraint {
    for line in src.lines().take(30) {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("// ALLOC_REQUIRES") {
            if let Some(tag) = GcKindTag::parse(rest) {
                return AllocConstraint::Requires(tag);
            }
        } else if let Some(rest) = t.strip_prefix("// ALLOC_EXCLUDES") {
            if let Some(tag) = GcKindTag::parse(rest) {
                return AllocConstraint::Excludes(tag);
            }
        }
    }
    AllocConstraint::None
}

/// Читает первые 30 строк файла и ищет `// EXPECT_TIMEOUT_MS <N>`.
/// Возвращает Duration если найдено и N > 0.
pub fn parse_timeout_ms(src: &str) -> Option<Duration> {
    for line in src.lines().take(30) {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("// EXPECT_TIMEOUT_MS") {
            if let Ok(ms) = rest.trim().parse::<u64>() {
                if ms > 0 {
                    return Some(Duration::from_millis(ms));
                }
            }
        }
    }
    None
}

/// Plan 194 A2.1 (замена Plan 140 Ф.2 / D24 amend): per-fixture директива
/// `// CONTRACTS checked|optimized|verified` в первых 30 строках. Override
/// codegen build-policy режима **для этого фикстура** — позволяет
/// регрессионным фикстурам проверять конкретный режим в обычном `test-all`
/// прогоне без отдельной CLI-команды. Legacy `off`/`enforce` keywords
/// больше НЕ распознаются (флаг `off` убран атомом A2.1 целиком; `enforce`
/// переименован в `checked` без alias — конвенция «чище убрать»,
/// см. атом-заметку). Возвращает `None`, если директивы нет (используется
/// build-policy из opts) — старые `// CONTRACTS off`/`// CONTRACTS enforce`
/// в непереехавших фикстурах молча игнорируются (fallback на opts default).
pub fn parse_contracts_policy(src: &str) -> Option<ast::ContractsMode> {
    for line in src.lines().take(30) {
        let trimmed = line.trim_start();
        let Some(body) = trimmed.strip_prefix("//") else {
            continue;
        };
        let body = body.trim_start();
        let Some(rest) = body.strip_prefix("CONTRACTS") else {
            continue;
        };
        // Требуем whitespace-разделитель, чтобы не матчить `CONTRACTSX`.
        if !rest.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        match rest.trim() {
            "checked" => return Some(ast::ContractsMode::Checked),
            "optimized" => return Some(ast::ContractsMode::Optimized),
            _ => continue,
        }
    }
    None
}

/// Plan 83.1 Ф.2: парсит директивы `// ENV NAME=VALUE` из первых 30
/// строк файла. Каждая выставляет переменную окружения **только** для
/// шага запуска тестового исполняемого файла (не для codegen/компиляции
/// C — те детерминированы по исходнику). Несколько директив допустимы.
///
/// Формат строгий: `// ENV` + whitespace + `NAME=VALUE`. `NAME` не может
/// быть пустым; `VALUE` может (тогда переменная задаётся пустой строкой).
/// Используется для тестов рантайм-конфигурации — например
/// `NOVA_MAXPROCS` (Plan 83.1).
pub fn parse_env(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in src.lines().take(30) {
        let trimmed = line.trim_start();
        let Some(body) = trimmed.strip_prefix("//") else {
            continue;
        };
        let body = body.trim_start();
        let Some(rest) = body.strip_prefix("ENV") else {
            continue;
        };
        // Требуем разделитель после `ENV`, чтобы не матчить `ENVOTHER=...`.
        if !rest.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let rest = rest.trim();
        if let Some(eq) = rest.find('=') {
            let key = rest[..eq].trim();
            let val = rest[eq + 1..].trim();
            if !key.is_empty() {
                out.push((key.to_string(), val.to_string()));
            }
        }
    }
    out
}

// ---------- Plan 26 Ф.6: Outcome — typed test result ----------

/// Результат одного теста. Production-grade: typed stages вместо
/// 12-вариантного enum'а. Один источник правды для label/detail/JSON.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Тест прошёл. `detail` опционален — обычно «», но для negative-
    /// тестов содержит контекстную метку вроде «(negative)» / «(stdout)».
    /// `captured_stdout/stderr` заполняются только при Verbosity::Verbose.
    /// `retries` — количество повторных попыток до успеха (0 = с первой).
    Pass {
        detail: String,
        elapsed: Duration,
        captured_stdout: Option<String>,
        captured_stderr: Option<String>,
        retries: u32,
    },
    /// Не прошёл. `stage` указывает на этап провала.
    Fail { stage: Stage, elapsed: Duration },
    /// Превысил `--timeout` — child killed.
    Timeout { elapsed: Duration },
    /// Пропущен из-за AllocConstraint несоответствия (Plan 27 Ф.6).
    Skipped { reason: SkipReason, elapsed: Duration },
}

/// Этап на котором тест упал. Структурно: `Codegen`/`Cc`/`Run` —
/// инфраструктура; `Expectation` — несоответствие D89 EXPECT-маркеру.
#[derive(Debug, Clone)]
pub enum Stage {
    /// Codegen .nv → .c упал (для тестов БЕЗ EXPECT_COMPILE_ERROR).
    Codegen { error: String },
    /// .c сгенерирован, но cc (clang/cl/gcc) упал.
    Cc { error: String },
    /// Exe запустился, но exit != 0 (для тестов БЕЗ EXPECT-маркера).
    Run { error: String },
    /// Codegen эмитнул `.c` но файл отсутствует на диске (codegen bug).
    NoCFile,
    /// EXPECT-маркер не выполнен: codegen прошёл хотя ожидался error,
    /// или runtime не упал/упал не так как ожидалось.
    Expectation { mismatch: ExpectMismatch },
}

/// Конкретный mismatch EXPECT-маркера. Один-к-одному с `ExpectMarker`,
/// плюс «succeeded when fail expected» варианты.
#[derive(Debug, Clone)]
pub enum ExpectMismatch {
    /// `EXPECT_COMPILE_ERROR <pat>`, но codegen succeeded.
    NoCompileError { expected_pat: String },
    /// `EXPECT_COMPILE_ERROR <pat>`, codegen упал но без pat.
    WrongCompileMsg { expected_pat: String, got: String },
    /// Plan 262 Part Б (owner decision 2026-08-09): `// nova:expect E_CODE
    /// -- reason` found and `E_CODE` DID occur in the compile error, but on
    /// a DIFFERENT line than `entry.line + 1` (the line right after the
    /// comment) — the precise failure mode the pin exists to catch (see
    /// `m510_vec_generic_bracket_sugar_turbofish_neg` postmortem in
    /// `lints::parse_nova_expect_comments` doc comment).
    WrongCompileLine { expected_pat: String, expected_line: usize, got_line: Option<usize>, got: String },
    /// `EXPECT_CC_ERROR <pat>`, но CC succeeded.
    NoCcError { expected_pat: String },
    /// `EXPECT_CC_ERROR <pat>`, CC упал но без pat.
    WrongCcMsg { expected_pat: String, got: String },
    /// `EXPECT_RUNTIME_PANIC <pat>`, но exit=0.
    NoPanic { expected_pat: String },
    /// `EXPECT_RUNTIME_PANIC <pat>`, exit!=0 но без pat.
    WrongPanic { expected_pat: String, got: String },
    /// `EXPECT_EXIT_CODE <N>`, но exit != N.
    WrongExit { expected: i32, got: i32 },
    /// `EXPECT_STDOUT <pat>` не найден.
    WrongStdout { expected_pat: String, got: String },
    /// `EXPECT_STDERR <pat>` не найден.
    WrongStderr { expected_pat: String, got: String },
    /// Plan 52 Ф.9: `EXPECT_COMPILE_WARNING <pat>` не найден среди lints.
    WrongCompileWarning { expected_pat: String, got: String },
    /// №463: `EXPECT_LINT_WARNING <rule_id>` не найден среди находок
    /// `nova lint` CONV_RULES-реестра (`lints::run_conv_rules`).
    WrongLintWarning { expected_pat: String, got: String },
}

impl Outcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, Outcome::Pass { .. })
    }

    pub fn is_skipped(&self) -> bool {
        matches!(self, Outcome::Skipped { .. })
    }

    /// Plan 26 Ф.17 #1: override elapsed для retry cumulative-time.
    /// Per-attempt run_one() имеет свой start; в JSON/JUnit summary
    /// нужно показать **общее** время от первого attempt до последнего.
    pub fn with_elapsed(self, elapsed: Duration) -> Self {
        match self {
            Outcome::Pass { detail, captured_stdout, captured_stderr, retries, .. } =>
                Outcome::Pass { detail, elapsed, captured_stdout, captured_stderr, retries },
            Outcome::Fail { stage, .. } => Outcome::Fail { stage, elapsed },
            Outcome::Timeout { .. } => Outcome::Timeout { elapsed },
            Outcome::Skipped { reason, .. } => Outcome::Skipped { reason, elapsed },
        }
    }

    /// Записывает retry count в Pass. На не-Pass вариантах — no-op.
    pub fn with_retries(self, retries: u32) -> Self {
        match self {
            Outcome::Pass { detail, elapsed, captured_stdout, captured_stderr, .. } =>
                Outcome::Pass { detail, elapsed, captured_stdout, captured_stderr, retries },
            other => other,
        }
    }

    /// Короткий лейбл для табличного output'а.
    pub fn label(&self) -> &'static str {
        match self {
            Outcome::Pass { .. } => "PASS",
            Outcome::Timeout { .. } => "TIMEOUT",
            Outcome::Skipped { .. } => "SKIP",
            Outcome::Fail { stage, .. } => match stage {
                Stage::Codegen { .. } => "CODEGEN-FAIL",
                Stage::Cc { .. } => "CC-FAIL",
                Stage::Run { .. } => "RUN-FAIL",
                Stage::NoCFile => "NO-C-FILE",
                Stage::Expectation { mismatch } => match mismatch {
                    ExpectMismatch::NoCompileError { .. } => "NEG-NO-ERROR",
                    ExpectMismatch::WrongCompileMsg { .. } => "NEG-WRONG-MSG",
                    ExpectMismatch::WrongCompileLine { .. } => "NEG-WRONG-LINE",
                    ExpectMismatch::NoCcError { .. } => "NEG-NO-CC-ERROR",
                    ExpectMismatch::WrongCcMsg { .. } => "NEG-WRONG-CC-MSG",
                    ExpectMismatch::NoPanic { .. } => "NEG-NO-PANIC",
                    ExpectMismatch::WrongPanic { .. } => "NEG-WRONG-PANIC",
                    ExpectMismatch::WrongExit { .. } => "NEG-WRONG-EXIT",
                    ExpectMismatch::WrongStdout { .. } => "NEG-WRONG-STDOUT",
                    ExpectMismatch::WrongStderr { .. } => "NEG-WRONG-STDERR",
                    ExpectMismatch::WrongCompileWarning { .. } => "NEG-WRONG-WARN",
                    ExpectMismatch::WrongLintWarning { .. } => "NEG-WRONG-LINT-WARN",
                },
            },
        }
    }

    /// Детальная human-readable строка (для table output + FAIL summary).
    pub fn detail(&self) -> String {
        match self {
            Outcome::Pass { detail, .. } => detail.clone(),
            Outcome::Timeout { elapsed } => format!("killed after {}ms", elapsed.as_millis()),
            Outcome::Skipped { reason, .. } => reason.description(),
            Outcome::Fail { stage, .. } => match stage {
                Stage::Codegen { error } | Stage::Cc { error } | Stage::Run { error } => {
                    // §1: не обрезать диагностику так агрессивно, чтобы скрыть суть — длинные
                    // обёртки (folder-module 'import resolution: in entry-folder peer (<длинный path>):
                    // <file>:<line>: <inner>') съедали 400 симв на path, пряча сам <inner>. 1500 даёт
                    // path×2 + реальную ошибку.
                    error.chars().take(1500).collect()
                }
                Stage::NoCFile => String::new(),
                Stage::Expectation { mismatch } => mismatch.detail(),
            },
        }
    }

    pub fn elapsed(&self) -> Duration {
        match self {
            Outcome::Pass { elapsed, .. }
            | Outcome::Fail { elapsed, .. }
            | Outcome::Timeout { elapsed }
            | Outcome::Skipped { elapsed, .. } => *elapsed,
        }
    }
}

impl ExpectMismatch {
    fn detail(&self) -> String {
        match self {
            ExpectMismatch::NoCompileError { expected_pat } => format!(
                "expected `// EXPECT_COMPILE_ERROR {}` but codegen succeeded",
                expected_pat
            ),
            ExpectMismatch::WrongCompileMsg { expected_pat, got } => {
                let snippet: String = got.chars().take(120).collect();
                format!("expected pattern '{}' not found in: {}", expected_pat, snippet)
            }
            ExpectMismatch::WrongCompileLine { expected_pat, expected_line, got_line, got } => {
                let snippet: String = got.chars().take(120).collect();
                let got_line_disp = got_line.map(|l| l.to_string()).unwrap_or_else(|| "?".to_string());
                format!(
                    "`nova:expect {}` pinned to line {}, but the matching error landed on line {} \
                     instead — pin is on the wrong line (or the error moved): {}",
                    expected_pat, expected_line, got_line_disp, snippet
                )
            }
            ExpectMismatch::NoPanic { expected_pat } => format!(
                "expected `// EXPECT_RUNTIME_PANIC {}` but exe succeeded (exit=0)",
                expected_pat
            ),
            ExpectMismatch::WrongPanic { expected_pat, got } => {
                let snippet: String = got.chars().take(120).collect();
                format!("expected panic pattern '{}' not found in: {}", expected_pat, snippet)
            }
            ExpectMismatch::WrongExit { expected, got } => {
                format!("expected exit code {}, got {}", expected, got)
            }
            ExpectMismatch::WrongStdout { expected_pat, got } => {
                let snippet: String = got.chars().take(120).collect();
                format!("expected stdout pattern '{}' not found in: {}", expected_pat, snippet)
            }
            ExpectMismatch::WrongStderr { expected_pat, got } => {
                let snippet: String = got.chars().take(120).collect();
                format!("expected stderr pattern '{}' not found in: {}", expected_pat, snippet)
            }
            ExpectMismatch::NoCcError { expected_pat } => format!(
                "expected `// EXPECT_CC_ERROR {}` but CC succeeded",
                expected_pat
            ),
            ExpectMismatch::WrongCcMsg { expected_pat, got } => {
                let snippet: String = got.chars().take(120).collect();
                format!("expected CC error pattern '{}' not found in: {}", expected_pat, snippet)
            }
            ExpectMismatch::WrongCompileWarning { expected_pat, got } => {
                let snippet: String = got.chars().take(120).collect();
                format!("expected compile warning pattern '{}' not found in lint output: {}",
                    expected_pat, snippet)
            }
            ExpectMismatch::WrongLintWarning { expected_pat, got } => {
                let snippet: String = got.chars().take(200).collect();
                format!("expected `nova lint` rule '{}' not found among CONV_RULES findings: {}",
                    expected_pat, snippet)
            }
        }
    }
}

/// Backward-compat alias чтобы старые call-sites внутри тестов работали.
/// Постепенно убрать; на момент Plan 26 main.rs использует `Outcome` напрямую.
pub type Status = Outcome;

pub struct TestBuildOpts<'a> {
    pub nv_file: &'a Path,
    pub toolchain: &'a Toolchain,
    pub mode: Mode,
    pub cg_include: &'a Path,
    pub rt_dir: &'a Path,
    pub tmp_dir: &'a Path,
    pub display: &'a str,
    pub keep_artifacts: bool,
    /// Plan 22 F2: libuv config. После detect_or_build_libuv всегда Some(_)
    /// в normal flow — failure → process exit. Option сохранён для
    /// API gradual transition / test mocks.
    pub libuv: Option<&'a LibuvConfig>,
    /// Plan 26 Ф.1: global timeout. Per-test `EXPECT_TIMEOUT_MS` (Б.2)
    /// может переопределить для конкретного теста. Default 60 s.
    pub timeout: Duration,
    /// Plan 27 Ф.1: GC backend. Propagates to BuildOpts → build_command.
    pub gc_kind: GcKind,
    /// Plan 27 Б.3: verbosity — при Verbose захватываем stdout/stderr PASS.
    pub verbosity: Verbosity,
    /// Plan 48 Ф.7.6: optional monomorphization-depth override (`--mono-depth=N`).
    /// `None` = use codegen default (env var NOVA_MONO_DEPTH or 500).
    pub mono_depth: Option<usize>,
    /// Plan 83.1 Ф.5: бюджет NOVA_MAXPROCS для тестового subprocess'а.
    /// `nova test` гоняет тест-файлы как `workers` параллельных
    /// процессов; без бюджета каждый M:N-тест с auto-detect (`init(0)`)
    /// поднял бы NumCPU worker'ов → NumCPU² потоков суммарно. Бюджет =
    /// max(1, NumCPU/workers) держит общее число worker-потоков ≈ NumCPU.
    /// Применяется к шагу запуска exe; `// ENV NOVA_MAXPROCS=...` его
    /// переопределяет (для тестов, проверяющих сам NOVA_MAXPROCS).
    /// Explicit `runtime.init(n>0)` тоже бьёт env (D136). `None` — не
    /// выставлять.
    pub maxprocs_budget: Option<u32>,
    /// Plan 194 A2.1 (замена Plan 140 Ф.2 / D24 amend `contracts_off: bool`):
    /// build-policy режим (`checked`/`optimized`/`verified`). Legacy `off`
    /// убран — недоказанные контракты проверяются в debug И release под
    /// всеми тремя значениями. Default `Checked`.
    pub contracts_mode: ast::ContractsMode,
    /// [M-standalone-out-of-tree-interp-sb-typedef]: the project root and
    /// resolved std-source-root for THIS `nova test`/`nova test-build`
    /// invocation — already resolved once by the caller (CWD-based
    /// `find_repo_root()` + `resolve_std_path`, see `nova-cli::resolve_paths`),
    /// mirroring what `nova build` (`cmd_build`) already threads through to
    /// `resolve_imports_inline`/`resolve_embeds`. `codegen_to_c` uses these
    /// directly instead of re-deriving a repo root from `nv_file`'s own
    /// filesystem location (`find_repo_root_from`) — that per-file walk
    /// returns `None` for any `.nv` file living outside the project tree
    /// (e.g. a `%TEMP%` probe file), which silently skipped ALL cross-file
    /// import resolution including the implicit `std.prelude` auto-import —
    /// so prelude-only types like `StringBuilder` (interpolation lowering's
    /// hand-synthesized `Nova_StringBuilder_*` calls, Plan 109/D179) never
    /// entered the module and their C typedef/bodies were never emitted.
    pub repo: &'a Path,
    pub stdlib_dir: &'a Path,
}

/// Plan 26 Ф.2: unique tmp subdir per test. Хеш от display даёт
/// воспроизводимый, но collision-resistant id. Решает:
/// 1. State leakage между тестами (AV-handle hold, leftover .obj).
/// 2. Возможность parallel execution (Ф.3) — каждый worker в своей
///    директории, no races.
fn test_subdir(global_tmp: &Path, display: &str) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut h = DefaultHasher::new();
    h.write(display.as_bytes());
    // 64-bit hash в hex; collision probability ~2^-32 для 130 тестов.
    global_tmp.join(format!("t-{:016x}", h.finish()))
}

/// Plan 26 Ф.16 #1: RAII guard для tmp subdirectory. Cleanup
/// гарантирован на любом return-path (включая panic), не только
/// на single happy-path в конце `run_one`. Mimics `tempfile::TempDir`
/// design без extra dep.
///
/// `keep` field — escape hatch для `--keep-artifacts`: при true
/// cleanup пропускается.
struct TempSubdir {
    path: PathBuf,
    keep: bool,
}

impl TempSubdir {
    fn new(path: PathBuf, keep: bool) -> std::io::Result<Self> {
        std::fs::create_dir_all(&path)?;
        Ok(TempSubdir { path, keep })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempSubdir {
    fn drop(&mut self) {
        if !self.keep {
            // best-effort cleanup; ошибки игнорируем (AV-handle leaks
            // одиночно безопасны — next run re-create'ит через hash).
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Запустить codegen + cc + run + check для одного .nv.
/// Production-grade: per-test isolation + timeout. Возвращает `Outcome`.
/// Plan 169.1 Ф.1: `split_out` receives (compile_ms, run_ms) split timing
/// before every return path. Both default to 0 on early exits.
pub fn run_one(opts: &TestBuildOpts, split_out: &mut (u128, u128)) -> Outcome {
    *split_out = (0, 0);
    let start = Instant::now();
    let src = match std::fs::read_to_string(opts.nv_file) {
        Ok(s) => s,
        Err(e) => {
            return Outcome::Fail {
                stage: Stage::Codegen { error: format!("read: {}", e) },
                elapsed: start.elapsed(),
            }
        }
    };

    // [A-S1 mutclock-regress]: folder-module CUs (Plan 169.1 Ф.8) pick the
    // ALPHABETICALLY-FIRST peer as `opts.nv_file` (walk_nv_filtered_ex above:
    // "Первый файл (по алфавиту) — entry") — for `core.nv` + `core_test.nv`
    // that's `core.nv` (`.` < `_` in ASCII), which typically carries NO
    // header directives (the directives live on the peer that actually
    // declares the `test "..."` blocks, e.g. `core_test.nv`'s
    // `// ENV NOVA_MAXPROCS=1` / `// ENV NOVA_AUTOARM=0`). Every marker
    // parse below (`parse_env`/`parse_alloc_constraint`/
    // `parse_smt_backend_requirement`/`parse_timeout_ms`/`parse_expect`)
    // scans only `src` (the entry file) — a directive living on a
    // non-entry peer was silently dropped, never applied to the run step.
    // Concretely: `std/src/testing/handlers` (core.nv/core_test.nv)'s
    // `NOVA_AUTOARM=0` escape hatch never reached the test exe's env, so
    // the mut_clock auto-idle-advance concurrent-sleep test ran under the
    // default-armed M:N runtime instead of the cooperative bootstrap path
    // its ordering guarantee depends on (nova_vclock_alive_count/
    // nova_vclock_park_until, fibers.h) — non-deterministic spawn-order
    // firing instead of deadline-order (core_test.nv "конкурентные sleep
    // будятся в порядке дедлайна" / "часы после конкурентных sleep").
    // Fix: gather header directives from same-module peer files (sharing
    // `opts.nv_file`'s directory + `module X` declaration, same predicate
    // `is_folder_module_dir` already uses) as SEPARATE marker-scan sources
    // — codegen/compile still use `src`/`path` unchanged (peer merge for
    // compilation already happens correctly via
    // `resolve_imports_inline_ex(..., include_test_peers=true)` further
    // below); this only widens what the CHEAP pre-compile marker scan sees.
    // Kept as a `Vec` of whole per-file sources (NOT concatenated into one
    // string) — every `parse_*` below does its own `.lines().take(30)` on
    // its input, so concatenating first would let the entry file's own 30
    // lines crowd out a peer's directives past that combined offset;
    // calling each `parse_*` once per source and merging RESULTS avoids
    // that entirely.
    let marker_srcs = collect_marker_sources(&src, opts.nv_file);

    // Plan 27 Ф.6: AllocConstraint — check before any build work.
    let alloc_constraint = marker_srcs
        .iter()
        .map(|s| parse_alloc_constraint(s))
        .find(|c| !matches!(c, AllocConstraint::None))
        .unwrap_or(AllocConstraint::None);
    if !alloc_constraint.allows(opts.gc_kind.tag()) {
        return Outcome::Skipped {
            reason: SkipReason::AllocBackend {
                constraint: alloc_constraint,
                actual: opts.gc_kind.tag(),
            },
            elapsed: start.elapsed(),
        };
    }

    // Plan 33 V1: REQUIRES_SMT_BACKEND — skip если активный backend
    // не совпадает с тем, который тест ожидает (Z3-only / trivial-only).
    if let Some(required) = marker_srcs.iter().find_map(|s| parse_smt_backend_requirement(s)) {
        let actual = active_smt_backend();
        if actual != required {
            return Outcome::Skipped {
                reason: SkipReason::SmtBackend { required, actual },
                elapsed: start.elapsed(),
            };
        }
    }

    // Plan 27 Б.2: per-test timeout override via EXPECT_TIMEOUT_MS.
    let effective_timeout = marker_srcs
        .iter()
        .find_map(|s| parse_timeout_ms(s))
        .unwrap_or(opts.timeout);

    // Plan 83.1 Ф.2: per-test env vars (// ENV NAME=VALUE) — applied to
    // the run step only. Merged across every marker source (entry + same-
    // module peers); later sources win on key collision (peer directives
    // are expected to be the specific/authoritative ones — see fn doc on
    // `collect_marker_sources`).
    let env_vars: Vec<(String, String)> = {
        let mut merged: Vec<(String, String)> = Vec::new();
        for s in &marker_srcs {
            for (k, v) in parse_env(s) {
                if let Some(existing) = merged.iter_mut().find(|(ek, _)| *ek == k) {
                    existing.1 = v;
                } else {
                    merged.push((k, v));
                }
            }
        }
        merged
    };

    // Plan 27 Б.3: capture stdout/stderr на PASS при --verbose.
    let verbose = matches!(opts.verbosity, Verbosity::Verbose);

    let expect: Vec<ExpectMarker> = marker_srcs.iter().flat_map(|s| parse_expect(s)).collect();
    // Plan 262 Part Б (owner decision 2026-08-09): `// nova:expect E_CODE --
    // reason` line-pins compile-error matching (see full rationale on
    // `lints::parse_nova_expect_comments`). Scanned from the ENTRY file
    // only (`src`, not every `marker_srcs` peer) — negative fixtures using
    // this form are single-file in every case the plan specifies; a
    // `nova:expect` on a peer file is simply not pinned (falls back to
    // legacy "anywhere" behaviour for that file's own EXPECT_COMPILE_ERROR,
    // if any).
    let nova_expect: Vec<crate::lints::NovaExpectEntry> = crate::lints::parse_nova_expect_comments(&src);
    let find_compile_error = || expect.iter().find_map(|m| if let ExpectMarker::CompileError(p) = m { Some(p) } else { None });
    let find_cc_error      = || expect.iter().find_map(|m| if let ExpectMarker::CcError(p)      = m { Some(p) } else { None });
    let find_runtime_panic = || expect.iter().find_map(|m| if let ExpectMarker::RuntimePanic(p) = m { Some(p) } else { None });
    let find_exit_code     = || expect.iter().find_map(|m| if let ExpectMarker::ExitCode(n)     = m { Some(*n) } else { None });
    let find_stdout        = || expect.iter().filter_map(|m| if let ExpectMarker::Stdout(p)     = m { Some(p.as_str()) } else { None }).collect::<Vec<_>>();
    let find_stderr        = || expect.iter().filter_map(|m| if let ExpectMarker::Stderr(p)     = m { Some(p.as_str()) } else { None }).collect::<Vec<_>>();
    // Plan 52 Ф.9: multi-pattern EXPECT_COMPILE_WARNING для NaN/dup-key
    // и других lint-warning сверок.
    let find_compile_warnings = || expect.iter().filter_map(|m| if let ExpectMarker::CompileWarning(p) = m { Some(p.as_str()) } else { None }).collect::<Vec<_>>();
    // №463: multi-pattern EXPECT_LINT_WARNING (`nova lint` CONV_RULES rule id).
    let find_lint_warnings = || expect.iter().filter_map(|m| if let ExpectMarker::LintWarning(p) = m { Some(p.as_str()) } else { None }).collect::<Vec<_>>();

    // Helper: build a Pass outcome with optional verbose capture.
    // codegen_warnings_str is prepended to err (if non-empty) so warnings appear
    // in captured_stderr and in EXPECT_STDERR matching without leaking to terminal.
    let make_pass_with_cg_warn = |detail: String, elapsed: Duration, out: Option<&str>, err: Option<&str>, cg_warn: &str| {
        let merged_err = if cg_warn.is_empty() {
            err.map(|s| s.to_string())
        } else {
            Some(match err {
                Some(s) if !s.is_empty() => format!("{}\n{}", cg_warn, s),
                _ => cg_warn.to_string(),
            })
        };
        Outcome::Pass {
            detail,
            elapsed,
            captured_stdout: if verbose { out.map(|s| s.to_string()) } else { None },
            captured_stderr: if verbose { merged_err } else { None },
            retries: 0,
        }
    };
    let make_pass = |detail: String, elapsed: Duration, out: Option<&str>, err: Option<&str>| Outcome::Pass {
        detail,
        elapsed,
        captured_stdout: if verbose { out.map(|s| s.to_string()) } else { None },
        captured_stderr: if verbose { err.map(|s| s.to_string()) } else { None },
        retries: 0,
    };

    // Plan 169.1 Ф.1: split timing — compile phase starts here (codegen + cc).
    let compile_start = Instant::now();

    // Step 1: codegen.
    // codegen_to_c returns Ok((codegen_warns, lint_warns, has_runnable_entry))
    // on success, Err(msg) on compile error. codegen_warnings — lints от
    // CEmitter (anonymous-embed override etc); lint_warnings — от
    // lints::lint_module (Plan 52 Ф.9: NaN-key, duplicate-map-key, и др. для
    // EXPECT_COMPILE_WARNING сверки). has_runnable_entry (Fix
    // [M-runner-testless-units-main-impl]) — true если module имеет ≥1
    // test-блок или явный `fn main()` (иначе `nova_fn_main_impl` не
    // эмитится, cc/link неизбежно упадёт — SKIP ниже).
    // Plan 48 Ф.7.6: mono_depth прокинут через opts (None = default 500).
    // Plan 194 A2.1 (замена Plan 140 Ф.2): contracts_mode прокинут через opts
    // (build-policy режим). Per-fixture `// CONTRACTS checked|optimized|verified`
    // директива переопределяет build-policy для этого фикстура.
    let contracts_mode = marker_srcs
        .iter()
        .find_map(|s| parse_contracts_policy(s))
        .unwrap_or(opts.contracts_mode);
    // Окно p401b-p67-class: per-unit panic net — a codegen internal-error
    // panic (`[P67-LEGACY]` and any other) is caught HERE and reported
    // through the same `Err(String)` channel as an ordinary compile error,
    // instead of killing the whole batch (see `catch_unit_panic` doc above).
    let codegen_result = catch_unit_panic(std::panic::AssertUnwindSafe(|| {
        codegen_to_c(
            opts.nv_file, &src, opts.mono_depth, contracts_mode, opts.repo, opts.stdlib_dir,
        )
    }));
    let codegen_warnings: Vec<String> = match &codegen_result {
        Ok((ws, _, _, _)) => ws.clone(),
        Err(_) => vec![],
    };
    let lint_warnings: Vec<String> = match &codegen_result {
        Ok((_, ls, _, _)) => ls.clone(),
        Err(_) => vec![],
    };
    // [M-runner-testless-units-main-impl]: true когда codegen succeeded И
    // module имеет ≥1 test-блок или явный `fn main()`. `false` на Err (не
    // используется в этом случае — codegen-error path возвращает раньше).
    let has_runnable_entry: bool = match &codegen_result {
        Ok((_, _, entry, _)) => *entry,
        Err(_) => false,
    };
    // Plan 209 Ф.2: which shape codegen produced (Single = default/pre-209
    // path, Split = multi-TU). Extracted here (borrow) — `codegen_result` is
    // MOVED/consumed a few lines below by `if let Err(msg) = codegen_result`.
    let codegen_artifact: CodegenArtifact = match &codegen_result {
        Ok((_, _, _, art)) => art.clone(),
        Err(_) => CodegenArtifact::Single, // unused on the Err early-return path below
    };
    let cg_warn_str: String = codegen_warnings.join("\n");

    // EXPECT_COMPILE_ERROR — handled на этапе codegen.
    //
    // Plan 262 Part Б: when the entry file carries at least one REASONED
    // `nova:expect` entry, it becomes AUTHORITATIVE for the pass/fail
    // decision below — the fixture passes only if some pinned entry's rule
    // id is found in the compile error AND that error's own rendered line
    // is `entry.line + 1` (comment directly above the failing line, same
    // convention as `nova:allow`). This is intentionally STRICTER than the
    // legacy file marker (which keeps matching "anywhere in file" when NO
    // `nova:expect` is present anywhere in the file — Б.3 backward compat
    // for fixtures not yet migrated). A no-reason `nova:expect` is excluded
    // from `pinned_expect` (mirrors `nova:allow`: unreasoned = inert for
    // its primary effect, separately flagged by `E_LINT_EXPECT_NO_REASON`
    // via `nova lint` — see `lints::apply_nova_expect_no_reason_check`).
    let pinned_expect: Vec<&crate::lints::NovaExpectEntry> =
        nova_expect.iter().filter(|e| e.has_reason).collect();
    let file_pat = find_compile_error();
    if file_pat.is_some() || !pinned_expect.is_empty() {
        let display_pat = || -> String {
            if let Some(p) = file_pat { return p.clone(); }
            pinned_expect
                .iter()
                .flat_map(|e| e.rule_ids.iter().cloned())
                .collect::<Vec<_>>()
                .join(",")
        };
        return match &codegen_result {
            Ok(_) => Outcome::Fail {
                stage: Stage::Expectation {
                    mismatch: ExpectMismatch::NoCompileError { expected_pat: display_pat() },
                },
                elapsed: start.elapsed(),
            },
            Err(msg) => {
                if !pinned_expect.is_empty() {
                    let got_line = extract_error_location(msg).map(|(_, l)| l);
                    let matched_pinned = pinned_expect.iter().find(|e| {
                        e.rule_ids.iter().any(|rid| msg.contains(rid.as_str()))
                            && got_line == Some(e.line + 1)
                    });
                    if matched_pinned.is_some() {
                        make_pass("(negative)".to_string(), start.elapsed(), None, None)
                    } else {
                        // Distinguish "right code, wrong line" (the exact
                        // failure this pin exists to catch — precedent:
                        // m510_vec_generic_bracket_sugar_turbofish_neg) from
                        // "code not found anywhere".
                        let matched_elsewhere = pinned_expect.iter().find(|e| {
                            e.rule_ids.iter().any(|rid| msg.contains(rid.as_str()))
                        });
                        if let Some(e) = matched_elsewhere {
                            Outcome::Fail {
                                stage: Stage::Expectation {
                                    mismatch: ExpectMismatch::WrongCompileLine {
                                        expected_pat: display_pat(),
                                        expected_line: e.line + 1,
                                        got_line,
                                        got: msg.clone(),
                                    },
                                },
                                elapsed: start.elapsed(),
                            }
                        } else {
                            Outcome::Fail {
                                stage: Stage::Expectation {
                                    mismatch: ExpectMismatch::WrongCompileMsg {
                                        expected_pat: display_pat(),
                                        got: msg.clone(),
                                    },
                                },
                                elapsed: start.elapsed(),
                            }
                        }
                    }
                } else {
                    // Legacy path, unchanged: file marker matches ANYWHERE
                    // in the compile error (Б.3 backward compat).
                    let pat = file_pat.expect("gated: pinned_expect empty implies file_pat.is_some()");
                    if msg.contains(pat) {
                        make_pass("(negative)".to_string(), start.elapsed(), None, None)
                    } else {
                        Outcome::Fail {
                            stage: Stage::Expectation {
                                mismatch: ExpectMismatch::WrongCompileMsg {
                                    expected_pat: pat.clone(),
                                    got: msg.clone(),
                                },
                            },
                            elapsed: start.elapsed(),
                        }
                    }
                }
            }
        };
    }

    if let Err(msg) = codegen_result {
        return Outcome::Fail {
            stage: Stage::Codegen { error: msg },
            elapsed: start.elapsed(),
        };
    }

    // Plan 52 Ф.9: EXPECT_COMPILE_WARNING — все ожидаемые pattern'ы должны
    // присутствовать среди lint-warnings (lints::lint_module). Проверяется
    // ПОСЛЕ codegen succeed (т.е. compile errors не было) и ДО CC/run.
    // Если ВСЕ warning'и найдены — early return Pass (lint-only тест, без
    // запуска runtime). Если есть хоть один pending warning — продолжаем
    // обычный flow (тест может комбинировать WARNING + RUNTIME_PANIC).
    let expected_warnings = find_compile_warnings();
    if !expected_warnings.is_empty() {
        // Plan 59 Ф.7.3: codegen_warnings (e.g. sizeof warning для big
        // mono'd tuples из register_mono_tuple) тоже учитываются для
        // EXPECT_COMPILE_WARNING match'а — раньше только lint_warnings
        // (lints::lint_module AST-based pass) были видны.
        let mut combined = lint_warnings.clone();
        combined.extend(codegen_warnings.iter().cloned());
        let all_lints_str = combined.join("\n");
        for pat in &expected_warnings {
            if !all_lints_str.contains(*pat) {
                return Outcome::Fail {
                    stage: Stage::Expectation {
                        mismatch: ExpectMismatch::WrongCompileWarning {
                            expected_pat: pat.to_string(),
                            got: all_lints_str.clone(),
                        },
                    },
                    elapsed: start.elapsed(),
                };
            }
        }
        // Если других expectation'ов нет (CC/PANIC/STDOUT/EXIT) — это
        // pure lint-test, можно early-return Pass без CC+run.
        let has_other_expectations = expect.iter().any(|m| matches!(m,
            ExpectMarker::CcError(_) | ExpectMarker::RuntimePanic(_)
            | ExpectMarker::ExitCode(_) | ExpectMarker::Stdout(_)
            | ExpectMarker::Stderr(_)));
        if !has_other_expectations {
            return make_pass_with_cg_warn(
                format!("(warning: {})", expected_warnings.len()),
                start.elapsed(),
                None, None, &cg_warn_str);
        }
    }

    // №463: EXPECT_LINT_WARNING — все ожидаемые CONV_RULES rule id'ы
    // (`nova lint`-реестр, `lints::run_conv_rules`) должны присутствовать
    // среди находок. ОТДЕЛЬНЫЙ канал от EXPECT_COMPILE_WARNING выше: та
    // сверяется только с `lints::lint_module` (unconditional AST-lint pass,
    // часть build/check pipeline) — CONV_RULES туда не попадает вообще
    // (`nova lint`-only опциональный реестр конвенций, nova-cli::cmd_lint).
    // Без ЭТОГО блока `EXPECT_LINT_WARNING` парсился (`parse_expect` уже
    // [INV-GUARD: check-expect-markers.sh] — описание УСТРАНЁННОГО дефекта,
    // а не живого инварианта: сегодня неизвестный маркер ловит страж.
    // знает про него), но НИКОГДА не проверялся — фикстура на CONV_RULES- [INV-GUARD: check-expect-markers.sh]
    // правило (напр. W_REDUNDANT_PAREN, реестр 221.1 №463) не ассертила
    // ничего и оставалась зелёной, даже если правило сломано целиком
    // (обнаружено ревью владельца, страж `check-expect-markers.sh`).
    // Точное совпадение rule id (`w.rule`), НЕ substring текста сообщения —
    // устойчивее к правкам формулировки, симметрично `--deny=RULE_ID` в
    // `nova lint`.
    let expected_lint_warnings = find_lint_warnings();
    if !expected_lint_warnings.is_empty() {
        let conv_module = crate::parser::parse(&src).ok();
        let conv_warnings = crate::lints::run_conv_rules(
            conv_module.as_ref(), &src, &crate::lints::ConvLintOptions::default(), None,
        );
        let found_rules: Vec<&str> = conv_warnings.iter().map(|w| w.rule).collect();
        for pat in &expected_lint_warnings {
            if !found_rules.contains(pat) {
                let got = if found_rules.is_empty() {
                    "0 findings".to_string()
                } else {
                    found_rules.join(", ")
                };
                return Outcome::Fail {
                    stage: Stage::Expectation {
                        mismatch: ExpectMismatch::WrongLintWarning {
                            expected_pat: pat.to_string(),
                            got,
                        },
                    },
                    elapsed: start.elapsed(),
                };
            }
        }
        // Как EXPECT_COMPILE_WARNING выше: если других expectation'ов нет —
        // pure lint-test, early-return Pass без CC+run.
        let has_other_lint_expectations = expect.iter().any(|m| matches!(m,
            ExpectMarker::CcError(_) | ExpectMarker::RuntimePanic(_)
            | ExpectMarker::ExitCode(_) | ExpectMarker::Stdout(_)
            | ExpectMarker::Stderr(_) | ExpectMarker::CompileWarning(_)));
        if !has_other_lint_expectations {
            return make_pass_with_cg_warn(
                format!("(lint-warning: {})", expected_lint_warnings.len()),
                start.elapsed(),
                None, None, &cg_warn_str);
        }
    }

    let c_file = opts.nv_file.with_extension("c");
    // Plan 209 Ф.2: multi-TU (`CodegenArtifact::Split`) never writes a
    // single `.c` next to `opts.nv_file` (codegen_to_c doc) — `common_h`/
    // `parts` are compiled from the per-test `obj_dir` further below
    // instead. The `NoCFile` sanity-check only applies to the Single shape.
    if matches!(codegen_artifact, CodegenArtifact::Single) && !c_file.is_file() {
        return Outcome::Fail { stage: Stage::NoCFile, elapsed: start.elapsed() };
    }

    // [M-runner-testless-units-main-impl]: codegen прошёл, .c записан
    // (compile-check уже подтверждён) — но модуль без test-блоков и без
    // `fn main()` не эмитит `nova_fn_main_impl`, поэтому cc/link неизбежно
    // упадёт `undefined symbol: nova_fn_main_impl` (см. emit_main_wrapper).
    // SKIP здесь — самая дешёвая точка обрыва: НЕ создаём tmp subdir, НЕ
    // компилируем/линкуем C, НЕ запускаем exe.
    if !has_runnable_entry {
        return Outcome::Skipped {
            reason: SkipReason::NoEntryPoint,
            elapsed: start.elapsed(),
        };
    }

    // Step 2 — isolated tmp subdir per test (Plan 26 Ф.2).
    // RAII guard: cleanup гарантирован на любом return-path (Plan 26 Ф.16 #1).
    let subdir_path = test_subdir(opts.tmp_dir, opts.display);
    let subdir_guard = match TempSubdir::new(subdir_path, opts.keep_artifacts) {
        Ok(g) => g,
        Err(e) => {
            return Outcome::Fail {
                stage: Stage::Cc { error: format!("mkdir subdir: {}", e) },
                elapsed: start.elapsed(),
            };
        }
    };
    let subdir = subdir_guard.path();

    let basename = opts.nv_file.file_stem().and_then(|s| s.to_str()).unwrap_or("test");
    let exe_name = if cfg!(target_os = "windows") {
        format!("{}.exe", basename)
    } else {
        basename.to_string()
    };
    let exe_file = subdir.join(&exe_name);
    // Windows: lld-link cannot overwrite a locked exe (AV / previous run handle).
    let _ = std::fs::remove_file(&exe_file);
    let obj_dir = subdir.join("obj");
    if let Err(e) = std::fs::create_dir_all(&obj_dir) {
        return Outcome::Fail {
            stage: Stage::Cc { error: format!("mkdir obj_dir: {}", e) },
            elapsed: start.elapsed(),
        };
    }

    // Plan 115 D214 [M-115-ffi-build-pipeline]: resolve [ffi] section в
    // package nova.toml для test_file. Paths становятся абсолютными
    // относительно директории nova.toml. None — нет manifest или нет
    // [ffi] section; пустой Some(...) — секция есть.
    let own_ffi: Option<ResolvedFfiConfig> =
        crate::manifest::find_manifest(opts.nv_file)
            .as_ref()
            .and_then(ResolvedFfiConfig::from_manifest);
    // Plan 03.1 (ext-dep native/FFI propagation): собрать [ffi] ВСЕХ
    // объявленных path/git-зависимостей своего пакета — native-артефакты
    // зависимости (её .c-шимы) должны линковаться в бинарь импортёра
    // симметрично тому, как её .nv-модули резолвятся в компиляцию (§3.2
    // explicit dependency graph). Own package's [ffi] идёт первым
    // (см. ResolvedFfiConfig::merge).
    //
    // [M-vendor-ffi-build-race-in-git-dep-cache] (backlog #152): каждый
    // провайдер собирается в СВОЁМ, ЕЩЁ НЕ смёрженном `ResolvedFfiConfig`
    // (`all_ffi`, ниже) — merge() в ОДИН `resolved_ffi` происходит ТОЛЬКО
    // после того, как `build_missing_vendor_ffi_libs` уже прошла по
    // каждому провайдеру ПООДИНОЧКЕ (см. цикл ниже). До этого фикса merge
    // происходил ПЕРЕД сборкой — `build_missing_vendor_ffi_libs` получала
    // ОДИН `ResolvedFfiConfig` с `vendor_src_dirs`/`libs` уже смешанными
    // из ВСЕХ провайдеров пакета (напр. tls's mbedTLS + compress's brotli
    // одновременно для polaris) и компилировала/архивировала их ВМЕСТЕ —
    // молча ломая сборку при коллизии basename между РАЗНЫМИ провайдерами
    // (mbedTLS's `library/platform.c` vs brotli's `common/platform.c`),
    // не только под гонкой потоков. Итоговый `resolved_ffi` (merged) ниже
    // используется КАК ПРЕЖДЕ — только для линковки (lib_dirs/libs
    // search-путей), где объединение всех провайдеров корректно и нужно.
    let mut all_ffi: Vec<ResolvedFfiConfig> = Vec::new();
    if let Some(f) = own_ffi {
        all_ffi.push(f);
    }
    if let Some(pkg_dir) = crate::imports::package_root_of(opts.nv_file) {
        for dep_root in crate::imports::resolved_dependency_roots(&pkg_dir) {
            let dep_toml = dep_root.join("nova.toml");
            let Some(dep_manifest) = crate::manifest::parse_manifest(&dep_toml, &dep_root) else {
                continue;
            };
            if let Some(dep_ffi) = ResolvedFfiConfig::from_manifest(&dep_manifest) {
                all_ffi.push(dep_ffi);
            }
        }
    }

    // Plan 193 Ф.2 gate-3: если [ffi] declares `vendor_src_dirs`, try to
    // build-and-cache the missing lib(s) from vendored source FIRST — this
    // can turn what would've been a SKIP into a real build. No-op (and
    // never fatal here) when vendor_src_dirs is empty/absent, or when the
    // libs are already cached from a previous test in this run — see
    // `build_missing_vendor_ffi_libs` doc-comment. Called ONCE PER
    // PROVIDER (own package's `[ffi]` + each dependency's, still
    // UNMERGED) — see #152 comment above for why merging first was wrong.
    for ffi in &all_ffi {
        build_missing_vendor_ffi_libs(ffi, opts.toolchain.vcvars_path());
    }
    let mut resolved_ffi: Option<ResolvedFfiConfig> = None;
    for ffi in all_ffi {
        match &mut resolved_ffi {
            Some(base) => base.merge(ffi),
            None => resolved_ffi = Some(ffi),
        }
    }

    // Plan 193 Ф.2 gap-1: detect-and-degrade — если merged [ffi] объявляет
    // явный lib_dirs search path, но declared libs-файл не найден ни в
    // одной из директорий, деградируем к SKIP вместо hard CC/link-FAIL
    // (см. first_missing_ffi_lib doc-comment). Проверяется ПОСЛЕ merge
    // (own + ext-dep [ffi]), ДО build_command/CC — самая дешёвая точка
    // обрыва для этого случая (subdir/obj_dir уже созданы выше, но CC ещё
    // не запущен).
    if let Some(ffi) = &resolved_ffi {
        if let Some((lib, searched)) = first_missing_ffi_lib(ffi) {
            return Outcome::Skipped {
                reason: SkipReason::FfiLibNotFound { lib, searched },
                elapsed: start.elapsed(),
            };
        }
    }

    // Plan 149 D233: resolve [runtime] section в package nova.toml для
    // test_file. Plain strings (no path resolution) — baked as -D...DEFAULT.
    let resolved_runtime: Option<crate::manifest::RuntimeConfig> =
        crate::manifest::find_manifest(opts.nv_file).and_then(|m| m.runtime);

    let build_opts = BuildOpts {
        c_file: &c_file,
        exe_file: &exe_file,
        obj_dir: &obj_dir,
        cg_include: opts.cg_include,
        rt_dir: opts.rt_dir,
        mode: opts.mode,
        libuv: opts.libuv,
        gc_kind: opts.gc_kind,
        ffi: resolved_ffi.as_ref(),
        runtime: resolved_runtime.as_ref(),
    };

    // Windows file-lock retry (lld-link "cannot open output file *.exe").
    const CC_LOCK_RETRIES: u32 = 3;
    const CC_LOCK_DELAY_MS: u64 = 250;

    // Plan 255 Ф.0: `nova test`'s cc invocation (single-TU clang compile+
    // link, OR multi-TU compile_multi_tu_to_exe) had NO PerfTimer wrap —
    // `cmd_build`'s "c-compile" marker (nova-cli/src/main.rs) only fires for
    // `nova build`, never for `nova test` (which is what the mega-CU gate
    // step actually runs). Same name ("c-compile") so both commands land in
    // the same NOVA_PERF_TIMER_AGGREGATE=1 bucket. Zero overhead when the
    // env switch is unset (see perf_timer.rs doc).
    let _t_cc = crate::perf_timer::PerfTimer::new("c-compile");
    let (cc_captured, cc_status) = 'cc: {
        // Plan 209 Ф.2: multi-TU path — parallel per-part compile + link
        // (`compile_multi_tu_to_exe`), folded into the SAME
        // `(CapturedOutput, ExitStatus)` shape the single-TU retry loop
        // below produces, so every downstream branch (EXPECT_CC_ERROR
        // matching, run-the-exe, …) is reused UNCHANGED for both paths.
        if let CodegenArtifact::Split { common_h, parts } = &codegen_artifact {
            let result = compile_multi_tu_to_exe(
                opts.toolchain, &build_opts, common_h, parts, effective_timeout);
            let success = result.is_ok();
            let stderr = match &result {
                Ok(_) => Vec::new(),
                Err(e) => e.to_string().into_bytes(),
            };
            let status = synth_exit_status(success);
            break 'cc (
                CapturedOutput {
                    status: Some(status),
                    stdout: Vec::new(),
                    stderr,
                    elapsed: Duration::default(),
                },
                status,
            );
        }
        let mut last_captured;
        let mut last_status;
        let mut attempt = 0u32;
        loop {
            let cmd = build_command(opts.toolchain, &build_opts);
            last_captured = match run_with_timeout(cmd, effective_timeout) {
                Ok(o) => o,
                Err(e) => {
                    return Outcome::Fail {
                        stage: Stage::Cc { error: format!("spawn cc: {}", e) },
                        elapsed: start.elapsed(),
                    }
                }
            };
            last_status = match last_captured.status {
                Some(s) => s,
                None => return Outcome::Timeout { elapsed: start.elapsed() },
            };
            if last_status.success() { break 'cc (last_captured, last_status); }
            let combined_peek = format!(
                "{}{}",
                bytes_to_string(&last_captured.stdout),
                bytes_to_string(&last_captured.stderr)
            );
            let is_file_lock = combined_peek.contains("cannot open output file")
                && combined_peek.contains(".exe");
            if is_file_lock && attempt < CC_LOCK_RETRIES {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(CC_LOCK_DELAY_MS * attempt as u64));
                continue;
            }
            break 'cc (last_captured, last_status);
        }
    };
    _t_cc.stop();

    if !cc_status.success() {
        let combined = format!(
            "{}{}",
            bytes_to_string(&cc_captured.stdout),
            bytes_to_string(&cc_captured.stderr)
        );
        // [M-linux-mn-conformance-red] (2026-07-20): opt-in full dump — the
        // 3-line "error"-substring filter below misses linker diagnostics
        // that don't literally contain the word "error" (GNU ld's
        // `undefined reference to` lines don't), truncating the visible
        // detail down to just clang's generic "linker command failed"
        // wrapper line. Zero overhead when unset (mirrors
        // NOVA_DEBUG_TIMEOUT_DUMP above).
        if std::env::var("NOVA_DEBUG_CC_DUMP").as_deref() == Ok("1") {
            eprintln!(
                "=== CC-FAIL STDOUT ===\n{}\n=== CC-FAIL STDERR ===\n{}\n=== END ===",
                bytes_to_string(&cc_captured.stdout),
                bytes_to_string(&cc_captured.stderr)
            );
        }
        // [M-linux-mn-conformance-red]: GNU `ld`'s own diagnostic lines
        // (`undefined reference to ...`, `cannot find -l...`) don't contain
        // the literal substring "error" — only the front-end's wrapper line
        // does (e.g. clang's "linker command failed with exit code 1"). Widen
        // the filter so a link failure's detail shows the ACTUAL undefined
        // symbol instead of just the uninformative wrapper line.
        let errs: Vec<&str> = combined
            .lines()
            .filter(|l| {
                let lc = l.to_lowercase();
                lc.contains("error")
                    || lc.contains("undefined reference")
                    || lc.contains("cannot find -l")
            })
            .take(3)
            .collect();
        let detail = if errs.is_empty() {
            combined.chars().take(200).collect::<String>().replace('\n', " | ")
        } else {
            errs.join(" | ")
        };
        if let Some(pat) = find_cc_error() {
            return if pat.is_empty() || combined.contains(pat.as_str()) {
                make_pass("(negative-cc)".to_string(), start.elapsed(), None, None)
            } else {
                Outcome::Fail {
                    stage: Stage::Expectation {
                        mismatch: ExpectMismatch::WrongCcMsg {
                            expected_pat: pat.clone(),
                            got: detail,
                        },
                    },
                    elapsed: start.elapsed(),
                }
            };
        }
        return Outcome::Fail {
            stage: Stage::Cc { error: detail },
            elapsed: start.elapsed(),
        };
    }
    if let Some(pat) = find_cc_error() {
        return Outcome::Fail {
            stage: Stage::Expectation {
                mismatch: ExpectMismatch::NoCcError { expected_pat: pat.clone() },
            },
            elapsed: start.elapsed(),
        };
    }

    // Plan 169.1 Ф.1: compile phase complete — capture elapsed ms before run.
    let compile_ms = compile_start.elapsed().as_millis();

    // Step 3 — run с timeout.
    let run_start = Instant::now();
    // Plan 255 Ф.0: execution stage — was entirely absent from the
    // PerfTimer picture (compile_ms/run_ms JSON split existed since Plan
    // 169.1, but not in the same NOVA_PERF_TIMER_AGGREGATE=1 table as the
    // codegen/cc passes). Stopped explicitly at the same point `run_ms` is
    // captured below — NOT at fn-scope drop, which would also swallow the
    // post-run stdout/stderr/expectation-matching work.
    let _t_run = crate::perf_timer::PerfTimer::new("run");
    // Plan 221.1 №158: peer set for this CU (entry + any same-module
    // folder-peers, same predicate `walk_nv_filtered_ex`/`collect_marker_sources`
    // use) — needed for two things below: (a) force `NOVA_DIAG_SEGV=1` for
    // this run when it's a folder-module (peer_paths.len() > 1), so a
    // genuine crash leaves a stack trace to attribute against; (b) turn
    // that trace into an honest culprit-or-"не определён" RUN-FAIL detail
    // (`attribute_merged_cu_crash` below) instead of silently blaming
    // whichever file `walk_nv_filtered_ex` collapsed the folder-module
    // discovery down to (alphabetically first — the exact bug that let
    // `d62_raw_effect_op_pos`'s NULL-handler segfault masquerade as
    // `a_q3_println_debug_record`/`d61_effect_handler_direct_call` for
    // weeks). Single-file CUs (the overwhelming majority) get
    // `peer_paths.len() == 1` — zero added env, zero behavior change.
    let peer_paths = collect_peer_paths(opts.nv_file);
    // [M-test-runner-tempdir-race-jobs] fix: retry a TRANSIENT exec-lock on
    // spawning the just-linked `exe_file` — under `--jobs N` a freshly
    // written .exe can momentarily fail to open for execution (Windows
    // Defender/AV scanning it on first launch: `ERROR_ACCESS_DENIED`/raw OS
    // error 5, or `ERROR_SHARING_VIOLATION`/raw OS error 32 — a DIFFERENT
    // process briefly holds the handle). This is the SAME class of transient
    // Windows file-lock the CC/link step already retries above
    // (`CC_LOCK_RETRIES`, "cannot open output file") — but that retry only
    // covers the compiler/linker's OWN diagnostic text; `Command::spawn()`
    // failing outright on the RUN step (this fn's `exe_file`, already
    // successfully linked moments earlier) had ZERO retry, so a transient
    // exec-lock surfaced as a hard `RUN-FAIL` (`spawn exe: ...`) instead of
    // the harness quietly waiting it out — observed spuriously on the
    // longest-running job in the suite (the `spec_tests/conformance`
    // folder-module mega-CU), which is simply "in flight" the longest and
    // therefore statistically most likely to overlap a burst of OTHER
    // workers' concurrent exec/link activity under `--jobs 16`. `Command`
    // is single-shot (consumed by `spawn`), so each attempt rebuilds it.
    const RUN_LOCK_RETRIES: u32 = 5;
    const RUN_LOCK_DELAY_MS: u64 = 200;
    let mut run_attempt = 0u32;
    let run_captured = loop {
        let mut run_cmd = Command::new(&exe_file);
        #[cfg(not(target_os = "windows"))]
        {
            run_cmd.env("LC_ALL", "C.UTF-8");
            run_cmd.env("LANG", "C.UTF-8");
        }
        // Plan 83.1 Ф.5: thread-budget — NOVA_MAXPROCS для тестового exe.
        // Ставится ДО `// ENV`-директив, чтобы тест, проверяющий сам
        // NOVA_MAXPROCS, мог переопределить бюджет своей директивой.
        if let Some(budget) = opts.maxprocs_budget {
            run_cmd.env("NOVA_MAXPROCS", budget.to_string());
        }
        // Plan 221.1 №158: folder-module (merged) CU — force the in-process
        // SEGV stack-trace diagnostic on so a genuine crash can be honestly
        // attributed below, instead of guessed. Ставится ДО `// ENV`-
        // директив, как и NOVA_MAXPROCS выше, чтобы тест мог явно
        // переопределить (`// ENV NOVA_DIAG_SEGV=0`) при желании.
        if peer_paths.len() > 1 {
            run_cmd.env("NOVA_DIAG_SEGV", "1");
        }
        // Plan 83.1 Ф.2: apply `// ENV NAME=VALUE` directives to the test exe.
        for (key, val) in &env_vars {
            run_cmd.env(key, val);
        }
        match run_with_timeout(run_cmd, effective_timeout) {
            Ok(o) => break o,
            Err(e) => {
                if is_transient_exec_lock_error(&e) && run_attempt < RUN_LOCK_RETRIES {
                    run_attempt += 1;
                    std::thread::sleep(Duration::from_millis(RUN_LOCK_DELAY_MS * run_attempt as u64));
                    continue;
                }
                return Outcome::Fail {
                    stage: Stage::Run { error: format!("spawn exe: {}", e) },
                    elapsed: start.elapsed(),
                };
            }
        }
    };
    // Plan 169.1 Ф.1: capture run_ms immediately after execution completes.
    let run_ms = run_start.elapsed().as_millis();
    _t_run.stop();
    let stdout = bytes_to_string(&run_captured.stdout);
    let stderr = bytes_to_string(&run_captured.stderr);
    let run_status = match run_captured.status {
        Some(s) => s,
        None => {
            if std::env::var("NOVA_DEBUG_TIMEOUT_DUMP").as_deref() == Ok("1") {
                eprintln!("=== TIMEOUT STDOUT ===\n{}\n=== TIMEOUT STDERR ===\n{}\n=== END ===", stdout, stderr);
            }
            return Outcome::Timeout { elapsed: start.elapsed() };
        }
    };
    let exit = run_status.code().unwrap_or(-1);

    // Step 4: check EXPECT-маркеры (multi-marker: все должны выполниться).
    let outcome = {
        if let Some(pat) = find_runtime_panic() {
            if exit == 0 {
                Outcome::Fail {
                    stage: Stage::Expectation {
                        mismatch: ExpectMismatch::NoPanic { expected_pat: pat.clone() },
                    },
                    elapsed: start.elapsed(),
                }
            } else if !stderr.contains(pat) && !stdout.contains(pat) {
                Outcome::Fail {
                    stage: Stage::Expectation {
                        mismatch: ExpectMismatch::WrongPanic {
                            expected_pat: pat.clone(),
                            got: format!("{} {}", stdout, stderr),
                        },
                    },
                    elapsed: start.elapsed(),
                }
            } else {
                let stdout_pats = find_stdout();
                let stderr_pats = find_stderr();
                let mut fail: Option<Outcome> = None;
                for spat in &stdout_pats {
                    if !stdout.contains(spat) {
                        fail = Some(Outcome::Fail {
                            stage: Stage::Expectation {
                                mismatch: ExpectMismatch::WrongStdout {
                                    expected_pat: spat.to_string(),
                                    got: stdout.clone(),
                                },
                            },
                            elapsed: start.elapsed(),
                        });
                        break;
                    }
                }
                if fail.is_none() {
                    for spat in &stderr_pats {
                        if !stderr.contains(spat) {
                            fail = Some(Outcome::Fail {
                                stage: Stage::Expectation {
                                    mismatch: ExpectMismatch::WrongStderr {
                                        expected_pat: spat.to_string(),
                                        got: stderr.clone(),
                                    },
                                },
                                elapsed: start.elapsed(),
                            });
                            break;
                        }
                    }
                }
                fail.unwrap_or_else(|| {
                    make_pass_with_cg_warn("(runtime-panic)".to_string(), start.elapsed(), Some(&stdout), Some(&stderr), &cg_warn_str)
                })
            }
        } else if let Some(n) = find_exit_code() {
            if exit != n {
                Outcome::Fail {
                    stage: Stage::Expectation {
                        mismatch: ExpectMismatch::WrongExit { expected: n, got: exit },
                    },
                    elapsed: start.elapsed(),
                }
            } else {
                make_pass_with_cg_warn(format!("(exit-code {})", n), start.elapsed(), Some(&stdout), Some(&stderr), &cg_warn_str)
            }
        } else {
            let stdout_pats = find_stdout();
            let stderr_pats = find_stderr();
            let has_content_marker = !stdout_pats.is_empty() || !stderr_pats.is_empty();

            if !has_content_marker && exit != 0 {
                // Prefer lines that actually name the failure (a genuine "  FAIL: …"
                // harness line, or a runtime panic banner); the in-binary harness
                // prints many PASS lines then a summary, so a blind "last 3 lines"
                // only shows the trailing PASS + count and hides WHICH test failed.
                //
                // [M-run-fail-detail-substring-false-positive] (2026-07-13): the prior
                // filter matched ANY line containing the substring "fail" case-
                // insensitively (also "assert"/"panic") and took the FIRST 4 matches.
                // Test PROSE routinely contains "Fail" as English text (the Fail
                // EFFECT feature: `with Fail = …`, `"with Fail: recoverable USER throw
                // …"` test descriptions) — those are ordinary "  PASS: …" lines, not
                // failure reports. On a mid-stream crash (segfault — no "FAIL:" line
                // is EVER printed for the killed test) this filter still matched the
                // first few *unrelated* "Fail"-mentioning PASS lines near the top of
                // the corpus and reported them as the "detail", which is always the
                // SAME misleading text regardless of where the process actually died
                // (confirmed: direct re-runs of the identical exe crashed at wildly
                // different points in the output, but `nova test`'s summary always
                // named the same 4 early D13/D158 lines). Fix: match the harness's
                // OWN failure-line prefix (`FAIL:` after trim) or a genuine panic
                // marker, not bare prose containing "fail"; take the LAST such
                // markers (nearest the crash) — and when none exist (pure crash, no
                // FAIL: line ever printed), fall back to the true last 3 lines, which
                // (since every PASS/FAIL print is followed by `fflush(stdout)`) are
                // the last test that actually completed before the process died.
                // [race-state-dump 2026-07-13]: `contains("panic")` case-
                // insensitively is the SAME false-positive class the 2026-07-13
                // fix above already fixed for "fail" — ordinary "  PASS: …" test
                // descriptions routinely mention "panic" in prose (the Fail/
                // panic effect feature: "compile to Panic-class outcome",
                // "runs without panic", "panic-категория не съедена"). On a
                // pure segfault (no line ever printed for the killed test) this
                // still matched the first/last few *unrelated* panic-mentioning
                // PASS lines and reported them as "detail" — confirmed: the
                // merged spec_tests/conformance CU's `app_effect_basic_t8_1`
                // RUN-FAIL always showed the SAME 4 early PASS lines (Plan
                // 140.3 / "@field contract" / "interpolated-message contract" /
                // D325 A1/R0) regardless of where the process actually died.
                // Match the GENUINE panic banner (`nv_panic` writes literal
                // lowercase "panic: " — effects.h) instead of any-case
                // substring "panic" anywhere in the line.
                let is_real_failure_line = |l: &&str| {
                    let t = l.trim_start();
                    t.starts_with("FAIL:") || t.starts_with("panic: ")
                };
                let fail_lines: Vec<&str> = stdout.lines().chain(stderr.lines())
                    .filter(is_real_failure_line)
                    .rev()
                    .take(4)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                let raw_detail = if !fail_lines.is_empty() {
                    fail_lines.join(" | ")
                } else {
                    let last_lines: Vec<&str> = stdout.lines().chain(stderr.lines()).rev().take(3).collect();
                    last_lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
                };
                // Plan 221.1 №158: merged (folder-module) CU — a genuine crash
                // (`fail_lines` empty: no harness "FAIL:"/"panic: " line was
                // EVER printed, i.e. the process died mid-run with no clue of
                // its own) used to be silently reported under whichever peer
                // file `walk_nv_filtered_ex` collapsed the folder-module
                // discovery down to (alphabetically first) — nothing in the
                // detail hinted this was a multi-file merge, so `RUN-FAIL
                // a_q3_...` read as "a_q3 is broken" for WEEKS while the real
                // culprit (`d62_raw_effect_op_pos`'s NULL-handler segfault)
                // was three files away. Prefix an honest merged-CU marker:
                // the REAL culprit when the SEGV-DIAG stack trace (forced on
                // via `NOVA_DIAG_SEGV=1` above for exactly this case) lets us
                // pin it down through its keystone frame's Nova-mangled fn
                // name (`attribute_merged_cu_crash`), or an EXPLICIT "не
                // определён" + the full candidate list otherwise — never
                // silence, never a guess dressed up as a fact.
                let detail = if peer_paths.len() > 1 && fail_lines.is_empty() {
                    let names: Vec<String> = peer_paths.iter()
                        .map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default())
                        .collect();
                    match attribute_merged_cu_crash(&stderr, &peer_paths) {
                        Some(culprit) => format!(
                            "[MERGED CU, {} файлов: {}] вероятный виновник: {} | {}",
                            peer_paths.len(), names.join(", "),
                            culprit.file_name().map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            raw_detail
                        ),
                        None => format!(
                            "[MERGED CU, {} файлов: {}] файл-виновник НЕ определён (нет \
                             однозначного кадра в SEGV-стеке — нужен NOVA_DIAG_SEGV-\
                             совместимый crash или уникальный `nova_fn_...` кадр) | {}",
                            peer_paths.len(), names.join(", "), raw_detail
                        ),
                    }
                } else {
                    raw_detail
                };
                // [M-test-runner-tempdir-race-jobs] investigation aid: opt-in
                // full dump (exit code + last 50 lines) for a genuine
                // RUN-FAIL — mirrors NOVA_DEBUG_CC_DUMP/NOVA_DEBUG_TIMEOUT_DUMP
                // above. Needed because a mid-stream crash with no "FAIL:"/
                // "panic: " line ever printed (fail_lines empty — the exact
                // case this investigation hit on the `spec_tests/conformance`
                // mega-CU) only shows 3 trailing PASS lines by default, which
                // don't say WHY the process died (exit code invisible).
                if std::env::var("NOVA_DEBUG_RUN_DUMP").as_deref() == Ok("1") {
                    let tail = |s: &str| -> String {
                        s.lines().rev().take(50).collect::<Vec<_>>().into_iter().rev()
                            .collect::<Vec<_>>().join("\n")
                    };
                    eprintln!(
                        "=== RUN-FAIL exit={} ===\n--- stdout (tail) ---\n{}\n--- stderr (tail) ---\n{}\n=== END ===",
                        exit, tail(&stdout), tail(&stderr)
                    );
                }
                Outcome::Fail {
                    stage: Stage::Run { error: detail },
                    elapsed: start.elapsed(),
                }
            } else {
                let mut fail: Option<Outcome> = None;
                for spat in &stdout_pats {
                    if !stdout.contains(spat) {
                        fail = Some(Outcome::Fail {
                            stage: Stage::Expectation {
                                mismatch: ExpectMismatch::WrongStdout {
                                    expected_pat: spat.to_string(),
                                    got: stdout.clone(),
                                },
                            },
                            elapsed: start.elapsed(),
                        });
                        break;
                    }
                }
                if fail.is_none() {
                    for spat in &stderr_pats {
                        if !stderr.contains(spat) {
                            fail = Some(Outcome::Fail {
                                stage: Stage::Expectation {
                                    mismatch: ExpectMismatch::WrongStderr {
                                        expected_pat: spat.to_string(),
                                        got: stderr.clone(),
                                    },
                                },
                                elapsed: start.elapsed(),
                            });
                            break;
                        }
                    }
                }
                fail.unwrap_or_else(|| {
                    let label = if has_content_marker { "(stdout/stderr)".to_string() } else { String::new() };
                    make_pass_with_cg_warn(label, start.elapsed(), Some(&stdout), Some(&stderr), &cg_warn_str)
                })
            }
        }
    };

    // Plan 169.1 Ф.1: record split timing for the successful run path.
    *split_out = (compile_ms, run_ms);

    // Cleanup через subdir_guard Drop (RAII).
    outcome
}

/// Codegen .nv → .c. Возвращает Ok(warnings) на успех, Err(rendered-error-string) на ошибку.
/// Warnings (напр. anonymous-embed lint) возвращаются caller'у для routing в captured_stderr,
/// вместо прямого eprintln! который утекал бы в терминал при параллельном запуске тестов.
/// Plan 35 R31: find **workspace** root от given path. Walks parents
/// looking for nova.toml с `[workspace]` секцией. Если не найден —
/// возвращает самый верхний nova.toml directory (на случай если
/// workspace declaration отсутствует).
///
/// AD6 (Plan 35 v2): unified ManifestResolver — package roots ≠
/// workspace root. Этот helper находит **workspace** для resolve
/// std/* imports.
///
/// Plan 35 sub-plan 35.B (sync): сделан `pub` для использования из
/// nova-cli — раньше nova-cli имел отдельный, legacy lookup (первый
/// nova.toml), который мог найти nova_tests/nova.toml вместо
/// repo/nova.toml в repos с nested manifest'ами.
pub fn find_repo_root_from(start: &Path) -> Option<PathBuf> {
    let abs = start.canonicalize().ok()?;
    let mut dir = abs.parent()?.to_path_buf();
    let mut last_toml_dir: Option<PathBuf> = None;
    loop {
        let toml = dir.join("nova.toml");
        if toml.exists() {
            // Check для `[workspace]` маркер.
            if let Ok(content) = std::fs::read_to_string(&toml) {
                if content.contains("[workspace]") {
                    return Some(dir);
                }
            }
            last_toml_dir = Some(dir.clone());
        }
        // Plan 193 Ф.1 continuation (2026-07-12): `dir.parent()?` used to
        // `?`-propagate `None` out of the WHOLE function the moment the walk
        // reached the filesystem root (`Path::parent()` returns `None` at
        // the root — it never returns `Some(dir)`, so the `parent == dir`
        // check below was dead code, unreachable). That discarded an
        // already-found `last_toml_dir` (a leaf/external package's own
        // non-`[workspace]` `nova.toml`, e.g. `nova-tls/nova.toml`) and
        // returned `None` — silently skipping `resolve_imports_inline_ex` +
        // `collect_all_signatures` in `codegen_to_c` for ANY standalone
        // package with no `[workspace]`-marked ancestor (module stayed at
        // its raw single-file item count; sibling `.nv` peers in the same
        // folder never got folded in), causing false `[D133-not-consumed]`
        // (and other cross-module-blind) diagnostics. Fix: match instead of
        // `?`, mirroring the already-correct fallback loop in
        // `nova-cli::find_repo_root`.
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => return last_toml_dir,
        }
    }
}

/// Plan 42 D29 rev-3: heuristic — is this file a peer of folder-module?
/// Plan 81 Ф.10: delegates to the canonical
/// `crate::imports::is_folder_module_peer` — single source of truth
/// (Plan 42.17 Ф.3 scanner-consolidation), now also consumed by
/// `manifest::check_module_path` for folder-module entry validation.
fn is_folder_module_peer(path: &Path) -> bool {
    crate::imports::is_folder_module_peer(path)
}

/// [A-S1 mutclock-regress] fix: `run_one`'s cheap pre-compile header-marker
/// scan (`parse_env`/`parse_alloc_constraint`/`parse_smt_backend_requirement`/
/// `parse_timeout_ms`/`parse_expect`/`parse_contracts_policy`) only ever saw
/// `entry_src` (`opts.nv_file`'s own content) — for a folder-module CU
/// (Plan 169.1 Ф.8), `opts.nv_file` is the ALPHABETICALLY-FIRST peer
/// (`walk_nv_filtered_ex`: "Первый файл (по алфавиту) — entry"), which is
/// frequently a non-test library file (`core.nv` before `core_test.nv`) that
/// carries none of the directives — those live on the peer that actually
/// declares the `test "..."` blocks. Directives placed there (e.g.
/// `std/src/testing/handlers/core_test.nv`'s `// ENV NOVA_AUTOARM=0`) were
/// silently dropped, never applied to the run step (root cause of the
/// mut_clock auto-idle-advance ordering flake — the escape hatch never
/// reached the test exe's env, so `spawn` took the default-armed M:N path
/// instead of the cooperative bootstrap path the deadline-order guarantee
/// depends on).
///
/// Returns one entry per file: `entry_src` itself, followed by each
/// same-module sibling peer's own full source (peers found via the same
/// `module X` declaration match `is_folder_module_dir` uses — NOT full
/// import resolution, this runs before that and must stay cheap). Kept as
/// SEPARATE strings (not concatenated) — every `parse_*` marker function
/// does its own `.lines().take(30)` on whatever it's given, so a single
/// combined string would let the entry file's own first 30 lines crowd a
/// peer's directives out of every scanner's window (a peer positioned past
/// line 30 of a naive concatenation would never be seen). Callers must
/// invoke each `parse_*` once per returned source and merge the results
/// (`find_map`/`flat_map`/explicit merge — see call sites in `run_one`).
/// Single-file CUs (the common case) get back a one-element vec — no
/// peers found, callers observe byte-identical behavior to before this fix.
/// Codegen/compilation still use the original `entry_src`/`path` directly —
/// this helper's output is ONLY fed to the marker scan.
/// Снимок каталога: `(путь, исходник, объявление модуля)` по каждому `.nv`.
///
/// ЗАЧЕМ (реестр 221.1 №521). `collect_marker_sources` и `collect_peer_paths`
/// спрашивают у каталога одно и то же и делали это заново на КАЖДОЙ работе:
/// для `neg/` — 2×561 чтение на работу × 551 работа ≈ 618 000 чтений файлов,
/// 21 % работы шага мега-CU (замер окна `p259-gate-speed`, 2026-08-09). Та же
/// фикстура стоила 6,53 с среди 561 соседа и 0,83 с в своём каталоге — то есть
/// корпус штрафовал сам себя за рост.
///
/// Снимок берётся ОДИН РАЗ за процесс, по тому же доводу, что и индекс модулей
/// в плане 252: дерево не меняется во время прогона, а если бы менялось —
/// результат прогона был бы бессмыслен независимо от кэша.
struct NvDirEntry {
    path: PathBuf,
    src: String,
    decl: Option<Vec<String>>,
}

fn nv_dir_index(dir: &Path) -> std::sync::Arc<Vec<NvDirEntry>> {
    use std::sync::{Arc, Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<std::collections::HashMap<PathBuf, Arc<Vec<NvDirEntry>>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));

    if let Ok(guard) = cache.lock() {
        if let Some(hit) = guard.get(dir) {
            return Arc::clone(hit);
        }
    }

    let mut entries: Vec<NvDirEntry> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut paths: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("nv"))
            .collect();
        paths.sort();
        for p in paths {
            let Ok(src) = std::fs::read_to_string(&p) else { continue };
            let decl = crate::imports::scan_module_decl(&src);
            entries.push(NvDirEntry { path: p, src, decl });
        }
    }
    let arc = Arc::new(entries);
    if let Ok(mut guard) = cache.lock() {
        // Гонка двух потоков на один каталог безвредна: снимок один и тот же,
        // побеждает любой — важно лишь, что читаем мы его после этого из кэша.
        guard.insert(dir.to_path_buf(), Arc::clone(&arc));
    }
    arc
}

/// Plan 262 Part Б: pull `(path, line)` out of the first `path:line:col:
/// error: ...` occurrence inside a rendered compile-error message — the
/// exact shape `Diagnostic::render`/`render_with_map` produce (`diag.rs`).
/// Scans line-by-line (not anchored at byte 0) because some error strings
/// have wrapper text ahead of the rendered diagnostic (e.g.
/// `codegen_to_c`'s `format!("import resolution: {}", e)`). `rsplitn(3,
/// ':')` on the prefix before `": error: "` — splits off `col` then `line`
/// from the RIGHT, leaving `path` (including a Windows drive-letter colon,
/// e.g. `D:\...\file.nv`) as the remainder. Returns `None` for messages
/// with no such line (manifest/import-resolution errors that never reached
/// `Diagnostic::render`) — callers must treat that as "location unknown" [INV-PROPERTY]
/// not "line 0".
fn extract_error_location(msg: &str) -> Option<(&str, usize)> {
    for line in msg.lines() {
        let Some(idx) = line.find(": error: ") else { continue };
        let prefix = &line[..idx];
        let mut parts = prefix.rsplitn(3, ':');
        let _col = parts.next()?;
        let line_no: usize = parts.next()?.trim().parse().ok()?;
        let path = parts.next()?;
        return Some((path, line_no));
    }
    None
}

fn collect_marker_sources(entry_src: &str, entry_path: &Path) -> Vec<String> {
    let mut sources = vec![entry_src.to_string()];
    let Some(dir) = entry_path.parent() else {
        return sources;
    };
    let Some(my_decl) = crate::imports::scan_module_decl(entry_src) else {
        return sources;
    };
    for e in nv_dir_index(dir).iter() {
        if e.path.as_path() == entry_path {
            continue;
        }
        if e.decl.as_ref() != Some(&my_decl) {
            continue;
        }
        sources.push(e.src.clone());
    }
    sources
}

/// Plan 221.1 №158: same peer-discovery predicate as `collect_marker_sources`
/// (entry + same-`module X`-declaring `.nv` siblings in its directory), but
/// returns the PATHS (entry first, then peers sorted) instead of source
/// text. Used by `run_one` to (a) decide whether to force
/// `NOVA_DIAG_SEGV=1` for the run step (only worth the extra stderr for an
/// actual folder-module/merged CU — `len() > 1`), and (b) list honest
/// RUN-FAIL candidates / feed `attribute_merged_cu_crash` when the run
/// crashes outright. Single-file CUs (the overwhelming majority) get back
/// a one-element vec, `len() == 1` — every caller must gate on that, not on
/// `is_folder_module_peer`/`is_folder_module_dir` (which additionally
/// require ≥2 files AND — for the latter — a "has tests" check irrelevant
/// here: we want the peer SET whether or not `opts.nv_file` itself is the
/// walk's collapsed entry).
fn collect_peer_paths(entry_path: &Path) -> Vec<PathBuf> {
    let mut out = vec![entry_path.to_path_buf()];
    let Some(dir) = entry_path.parent() else { return out; };
    let index = nv_dir_index(dir);
    let Some(my_decl) = index
        .iter()
        .find(|e| e.path.as_path() == entry_path)
        .and_then(|e| e.decl.clone())
    else {
        return out;
    };
    for e in index.iter() {
        if e.path.as_path() == entry_path {
            continue;
        }
        if e.decl.as_ref() != Some(&my_decl) {
            continue;
        }
        out.push(e.path.clone());
    }
    out
}

/// Plan 221.1 №158: demangle a Nova codegen fn C-symbol
/// (`nova_fn_<len><seg><len><seg>…`, Itanium-style length-prefixed segments
/// — see `emit_c.rs`'s `mangle_fn`/generic `nova_fn_<modpath>_<name>`
/// scheme) into `(module_path_segments, short_fn_name)`. Returns `None` for
/// anything that doesn't fit the scheme — synthetic names like
/// `nova_fn_main_impl`, `nova_test_<description>_<idx>`, `Nova_<Effect>_<op>`
/// dispatch shims, or the collision-disambiguated `nova_fn_<mod>_f<id>_<name>`
/// variant (Plan 209/emit_c.rs `mangle_fn` file-id fallback) — callers must
/// treat `None` as "this frame isn't attributable", not as an error, and
/// keep looking at the NEXT stack frame.
fn demangle_nova_fn(sym: &str) -> Option<(Vec<String>, String)> {
    let rest = sym.strip_prefix("nova_fn_")?;
    let bytes = rest.as_bytes();
    let mut i = 0usize;
    let mut segs: Vec<String> = Vec::new();
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None; // no length prefix here — not this mangling scheme
        }
        let len: usize = rest[start..i].parse().ok()?;
        if len == 0 || i + len > rest.len() {
            return None;
        }
        segs.push(rest[i..i + len].to_string());
        i += len;
    }
    if segs.len() < 2 {
        // need at least one module segment + the fn's own short name
        return None;
    }
    let short_name = segs.pop().unwrap();
    Some((segs, short_name))
}

/// Plan 221.1 №158: parse `segv_diag.c`'s `_nova_segv_veh` stack-trace block
/// (`  #NN <addr>  <module>!<name>+0x<disp>  (<file>:<line>)`, printed to
/// stderr only when `NOVA_DIAG_SEGV=1` — forced on above for merged CUs)
/// into an ordered list of raw symbol names (frame 0 = crash site first).
/// `name == "?"` (SymFromAddr failed to resolve, e.g. runtime/libc frames
/// past the fiber entry) is skipped — never fed to `demangle_nova_fn` as a
/// spurious "match".
fn parse_segv_stack_frames(stderr: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        let t = line.trim_start();
        if !t.starts_with('#') {
            continue;
        }
        let Some(bang) = t.find('!') else { continue; };
        let after = &t[bang + 1..];
        let Some(plus) = after.find("+0x") else { continue; };
        let name = &after[..plus];
        if name.is_empty() || name == "?" {
            continue;
        }
        out.push(name.to_string());
    }
    out
}

/// Plan 221.1 №158: best-effort honest attribution for a merged
/// (folder-module) CU's genuine RUN-FAIL crash. Walks the SEGV-DIAG stack
/// trace (`parse_segv_stack_frames`, present in `stderr` only because the
/// run step forced `NOVA_DIAG_SEGV=1` for this CU — see the `run_cmd.env`
/// call above) top-down (frame 0 = crash site), demangling each frame's
/// symbol (`demangle_nova_fn`) until one succeeds, then checks which
/// peer SOURCE FILE actually declares a top-level `fn <short_name>(` /
/// `fn <short_name> (` matching it (peer files all share the SAME `module
/// X` — the short name alone, no module-path disambiguation needed).
///
/// Returns `Some(path)` ONLY on an unambiguous single-file match for SOME
/// frame; `None` when there's no SEGV-DIAG block at all (e.g. an `abort()`/
/// non-access-violation crash — `nv_panic`'s own last-resort path, or a
/// stack-overflow that never reaches the VEH), no frame demangles, or a
/// demangled name matches zero or MORE THAN ONE peer file (ambiguous —
/// honesty requires refusing to guess, not picking the first alphabetically
/// the way the OLD bug did one level up). Callers must render `None` as an
/// explicit "не определён" + full candidate list, never silently fall back
/// to naming a file.
fn attribute_merged_cu_crash(stderr: &str, peer_paths: &[PathBuf]) -> Option<PathBuf> {
    for sym in parse_segv_stack_frames(stderr) {
        let Some((_, short_name)) = demangle_nova_fn(&sym) else { continue; };
        let needle_a = format!("fn {}(", short_name);
        let needle_b = format!("fn {} (", short_name);
        let matches: Vec<&PathBuf> = peer_paths.iter()
            .filter(|p| {
                std::fs::read_to_string(p)
                    .map(|src| src.contains(&needle_a) || src.contains(&needle_b))
                    .unwrap_or(false)
            })
            .collect();
        if matches.len() == 1 {
            return Some(matches[0].clone());
        }
        // Zero or ambiguous match for THIS frame — try the next one instead
        // of guessing.
    }
    None
}

/// Plan 52 Ф.9: возвращает `(codegen_warnings, lint_warnings)` — последние
/// используются для `EXPECT_COMPILE_WARNING` сверки. Lints вызываются
/// после type-check, ДО desugar — иначе MapLit/RecordLit-узлы уже
/// заменены на Block'и, и lint check_map_literal_lints не сработает.
///
/// Plan 209 Ф.2: which shape `codegen_to_c` produced.
///
/// `Single` — the existing/default behavior, UNCHANGED: a single `.c`
/// already written to `path.with_extension("c")`, byte-identical to
/// pre-209 (`NOVA_MULTI_TU` unset, or the CU is under the split threshold).
///
/// `Split` — multi-TU (env `NOVA_MULTI_TU=1` AND the CU exceeds the Ф.1
/// threshold, `CEmitter::emit_module_multi_tu` / `EmitOutput::Split`):
/// `common_h`/`parts` are held IN MEMORY here — NOT written next to `path`
/// (there is no single "`the` `.c`" for this CU anymore). `run_one` hands
/// them to `compile_multi_tu_to_exe`, which writes them under the test's
/// own per-test `obj_dir` instead.
#[derive(Clone)]
enum CodegenArtifact {
    Single,
    Split { common_h: String, parts: Vec<String> },
}

/// Plan 48 Ф.7.6: `mono_depth` — optional CLI override для
/// CEmitter.mono_depth_limit (None = default из env var или 500).
///
/// [M-standalone-out-of-tree-interp-sb-typedef]: `repo`/`stdlib_dir` are the
/// CALLER's already-resolved project root + std-source-root (CWD-based
/// `find_repo_root()`, see `TestBuildOpts::repo` doc-comment) — NOT
/// re-derived here from `path`'s own filesystem location. A `.nv` file
/// living outside the project tree (e.g. a `%TEMP%` probe file) has no
/// `nova.toml` ancestor of its OWN, but it still belongs to the project that
/// invoked `nova test` — exactly like `nova build` (`cmd_build`) already
/// treats it, threading its own CWD-resolved `repo`/`stdlib_dir` through to
/// `resolve_imports_inline`/`resolve_embeds` unconditionally.
fn codegen_to_c(
    path: &Path,
    src: &str,
    mono_depth: Option<usize>,
    contracts_mode: ast::ContractsMode,
    repo: &Path,
    stdlib_dir: &Path,
) -> Result<(Vec<String>, Vec<String>, bool, CodegenArtifact), String> {
    // Plan 57.D.1: PerfTimer wraps вокруг каждого pass. Markers эмитятся
    // если NOVA_PERF_TIMER=1, accumulated если NOVA_PERF_TIMER_AGGREGATE=1.
    let mut module = {
        let _t = crate::perf_timer::PerfTimer::new("parse");
        parser::parse(src).map_err(|d| d.render(src, &path.to_string_lossy()))?
    };
    // Plan 42 D29 rev-3: detect — is this file a peer of folder-module?
    // Folder-module = parent dir содержит >1 .nv files, и все они
    // объявляют тот же `module X`. Если да — manifest check использует
    // is_folder_module=true (parent.X rule).
    let is_folder_module = is_folder_module_peer(path);
    // `[M-oot-dash-module-name-e78]` (2026-07-21): a file outside `repo`
    // (the SAME CWD-resolved project root already threaded below into
    // `resolve_imports_inline*`, per `[M-standalone-out-of-tree-interp-sb-typedef]`)
    // is exempt from D78 — `find_manifest` inside `check_module_path_with_kind`
    // walks to the nearest ancestor `nova.toml` regardless of which project
    // invoked `nova`, so a file living e.g. under a shared `%TEMP%` scratch
    // tree can land under a wholly UNRELATED leftover manifest several
    // directories up and get its `parent.target` rule wrongly enforced.
    // In-tree files (the overwhelming case) are untouched (δ0).
    let skip_d78_oot = manifest::is_outside_repo(path, repo);
    // Bug fix 2026-06-01: emit W_D78_REV1_DEPRECATED warning instead of
    // silent acceptance для rev-1 legacy declarations.
    let d78_result = if skip_d78_oot {
        Ok(manifest::ModulePathCheck::Rev3)
    } else {
        manifest::check_module_path_with_kind(path, &module.name, is_folder_module)
    };
    match d78_result {
        Ok(manifest::ModulePathCheck::Rev3) => {}
        Ok(manifest::ModulePathCheck::Rev1Deprecated(msg)) => {
            eprintln!("warning: {}", msg);
        }
        Err(s) => return Err(s.to_string()),
    }

    // Plan 35 R31 (unified pipeline): cross-file resolve через inline
    // expansion. Тот же codepath что в `nova-cli::cmd_build`. Без этого
    // `nova test foo.nv` с `import std.X.Y` падает «cannot resolve
    // iterator type 'nova_int'».
    // Plan 35 sub-plan 35.A R27: prelude auto-import работает даже когда
    // user не делает explicit import — поэтому вызываем resolve_imports_inline
    // безусловно (resolver сам auto-добавит prelude если файл существует).
    // Plan 162.2 Ф.2: collect cross-module signatures before type-check so
    // that is_known_type / is_known_fn can answer cross-module questions during
    // check_module_with_sig_table (suppresses false E_UNKNOWN_PROTOCOL /
    // E_BOUND_UNKNOWN / E7401 for symbols from transitively imported modules).
    // [M-standalone-out-of-tree-interp-sb-typedef]: `repo`/`stdlib_dir` are
    // the caller's CWD-resolved project root (see fn doc-comment above) —
    // used unconditionally, same as `nova build`. Previously this branched
    // on `find_repo_root_from(path)` (a walk from `path`'s OWN directory),
    // which returned `None` — silently skipping this entire block, INCLUDING
    // the implicit `std.prelude` auto-import — for any `.nv` file living
    // outside a `nova.toml` tree.
    // Plan 35 R31 / Plan 262 Ф.А.1-bis (registry №531, unified pipeline):
    // cross-file resolve + sig-table + embed-resolve + serde-derive
    // injection + alpha_rename + number_exprs, all via
    // `crate::check_pipeline::prepare_module_for_check_with` — the same
    // shared function `nova check`/`nova build`/the doc-test runner/
    // nova-lsp use (see that module's doc comment for the full rationale).
    // `include_test_peers=true` — Plan 42 правило F: test mode includes
    // `*_test.nv` peers.
    // [M-standalone-out-of-tree-interp-sb-typedef]: `repo`/`stdlib_dir` are
    // the caller's CWD-resolved project root (see fn doc-comment above) —
    // used unconditionally, same as `nova build`.
    let (embed_dir_warnings, sig_table_opt, resolved_types) = {
        let _t = crate::perf_timer::PerfTimer::new("imports-resolve");
        let prepared = crate::check_pipeline::prepare_module_for_check_with(
            path, &mut module, repo, stdlib_dir, /* include_test_peers */ true,
            |m| {
                // Plan 180: inject SERDE synthesized methods
                // (`#impl(Serialize/Deserialize)`) BEFORE numbering +
                // type-check so their bodies are type-checked + annotated
                // (codegen's annotation-free infer cannot resolve serde's
                // cross-method return types). Non-serde protocols
                // (Equal/…/Display/Debug) inject AFTER check (below) —
                // some of their bodies are intentionally not
                // type-checkable. Must run between embed-resolve and
                // alpha-rename (synthesized bodies share the uniquify
                // invariant) — `prepare_module_for_check_with`'s extension
                // point exists for exactly this, same as `nova build`'s use.
                crate::protocols::auto_derive::inject_synthesized_methods_filtered(
                    m, |p| p == "Serialize" || p == "Deserialize");
            },
        )
        .map_err(|e| match e {
            crate::check_pipeline::PrepareError::Import(e) => format!("import resolution: {}", e),
            crate::check_pipeline::PrepareError::Embed(diags) => {
                // module.peer_files is already populated (import-resolve —
                // step 1 of `prepare_module_for_check_with` — succeeded
                // before embed-resolve failed), so a SourceMap built from
                // it here still attributes each diagnostic's span to the
                // right peer file, same as the success path below.
                let source_map = crate::diag::SourceMap::from_peer_files(&module.peer_files, path, src);
                diags
                    .iter()
                    .map(|d| d.render_with_map(&source_map))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        })?;
        (prepared.embed_warnings, prepared.sig_table, prepared.resolved_types_seed)
    };

    // [M-vec-access-e7320-as-bytes-str] variant D (span-misrender fix,
    // 2026-07-07): every diagnostic rendered below this point runs on the
    // FULLY-MERGED module — spans may carry a `file_id` pointing at an
    // imported peer file, not `path`/`src` (Plan 35 Ф.0 wired `file_id` into
    // `Span`, but `codegen_to_c` still rendered every diagnostic through
    // `SrcResolver::Single { source: src, file: path }`, which ignores
    // `span.file_id` entirely — cross-file errors printed at the ENTRY
    // file's line/col instead of the true one). `render_with_map` (diag.rs)
    // resolves per-span via a `SourceMap`.
    //
    // [M-crossmerge-diagnostic-sourcemap-file-id-misattribution] (№132,
    // 2026-07-26): this used to build the map by pushing `module.peer_files`
    // in vector order and letting `SourceMap::register`'s auto-increment
    // assign ids POSITIONALLY — on the false assumption that insertion
    // order into `peer_files` always matches ascending `file_id` order.
    // That assumption breaks already for a single multi-peer folder module
    // (the resolver allocates every peer's id up front in PASS 1, then
    // pushes peers one at a time in PASS 2, recursing into each peer's own
    // imports — which pushes THAT peer's transitively-reached peer_files
    // with HIGHER ids — before pushing the NEXT (alphabetically later)
    // sibling peer of the SAME folder, whose id is LOWER than what was
    // just pushed). Diamond dependency graphs (e.g. `std` reachable both
    // directly and transitively through `http`) just made the resulting
    // id/position drift observable at scale (integrator repro:
    // `nova-polaris test src --strict-effects` → 38 of 39 diagnostics with
    // correct MESSAGE but garbage `file:line`, e.g. landing on
    // `http/src/mime.nv` for a `std`-side effect diagnostic). Fixed by
    // `SourceMap::from_peer_files`, which looks up each `file_id` EXPLICITLY
    // (`HashMap<FileId, &PeerFile>`) rather than trusting push order — same
    // robust pattern `nova-cli::build_source_map` already used. Peer source
    // text isn't kept post-parse, so non-entry files are re-read from disk
    // (diagnostic path only — no perf concern).
    let source_map = crate::diag::SourceMap::from_peer_files(&module.peer_files, path, src);

    // Plan 140 Ф.3 (D24 amend): capture ModuleEnv. `check_module` runs the
    // VerificationPipeline (types/mod.rs `env.proven_contracts = report.proven`)
    // on THIS build path — proven contracts must be fed to codegen below for
    // zero-cost elision. Previously the env was discarded → proven set empty
    // on the test-build path → proven contracts were NOT elided (R4: pipeline
    // ran but proven was never wired to the emitter).
    // Plan 162.2 Ф.2: use check_module_with_sig_table when sig_table available.
    let mut module_env = {
        let _t = crate::perf_timer::PerfTimer::new("type-check");
        match sig_table_opt {
            Some(sig_table) => types::check_module_with_sig_table(&module, sig_table),
            None => types::check_module(&module),
        }
        .map_err(|errs| {
            errs.iter()
                .map(|d| d.render_with_map(&source_map))
                .collect::<Vec<_>>()
                .join("\n")
        })?
    };
    // Plan 172.1 U.4.1: hand the literal resolved-type seed to ModuleEnv.
    // Plan 172.1 U.4.4(b): merge the checker's semantic Ident annotations OVER the seed.
    let checker_annotations = std::mem::take(&mut module_env.resolved_types);
    module_env.resolved_types = resolved_types;
    module_env.resolved_types.extend(checker_annotations);
    // Plan 174 (D409): auto-return lowering для `-> @` тел. check_module выше
    // уже отгейтил E_EXPLICIT_SELF_RETURN на as-written AST; эта мутация
    // синтезирует `@` на implicit exit'ах, переиспользуя существующую
    // emission manual pre-D409 формы (self_return_lower.rs doc).
    crate::self_return_lower::lower_module(&mut module);
    // Plan 52 Ф.9: lints — ПОСЛЕ check_module (типы validated), ДО
    // desugar (lints видят MapLit-узлы). Возвращаются caller'у для
    // EXPECT_COMPILE_WARNING сверки.
    let mut lint_warnings: Vec<String> = {
        let _t = crate::perf_timer::PerfTimer::new("lints");
        crate::lints::lint_module(&module)
            .iter()
            .map(|w| w.diag.render_with_map(&source_map))
            .collect()
    };
    // Plan 210: embed_dir's W_EMBED_DIR_* (captured earlier, before this
    // pipeline point) join the same lint_warnings stream — this is what
    // `EXPECT_COMPILE_WARNING` matches against (§9.1 warning-канал fix).
    for w in &embed_dir_warnings {
        lint_warnings.push(w.diag.render_with_map(&source_map));
    }
    // Ф.7.4 (Plan 33.6): verify-warnings (W2401/W2402) тоже dispatch'им в lint stream.
    // Plan 140 Ф.3: proven contracts уже получены через `module_env` выше
    // (check_module → VerificationPipeline). Этот вызов остаётся ТОЛЬКО ради
    // verify-warnings, которые check_module глушит (types/mod.rs: `report.warnings`
    // intentionally silent). Proven set здесь намеренно НЕ используется.
    {
        let _t = crate::perf_timer::PerfTimer::new("verify");
        let verify_report = crate::verify::verify_module(&module);
        for w in &verify_report.warnings {
            lint_warnings.push(w.render_with_map(&source_map));
        }
    }
    {
        // Plan 114.4.2 (D199) Ф.3: const fn AST rewrite + codegen drop.
        // Runs ПЕРЕД annotate-maps/desugar чтобы они уже видели literals.
        let _t = crate::perf_timer::PerfTimer::new("const-fn-rewrite");
        let cfn_errs = crate::const_fn_eval::rewrite_const_fn_calls(&mut module);
        if !cfn_errs.is_empty() {
            return Err(cfn_errs.iter()
                .map(|d| d.render_with_map(&source_map))
                .collect::<Vec<_>>()
                .join("\n"));
        }
    }
    {
        // Plan 114.4.4.5 V4.1: monomorphize mixed const fns.
        let _t = crate::perf_timer::PerfTimer::new("const-fn-mono");
        let mono_errs = crate::const_fn_mono::specialize_mixed_const_fns(&mut module);
        if !mono_errs.is_empty() {
            return Err(mono_errs.iter()
                .map(|d| d.render_with_map(&source_map))
                .collect::<Vec<_>>()
                .join("\n"));
        }
    }
    {
        // Plan 126.2 Ф.2: inject the NON-serde synthesized built-in protocol
        // methods (Equal/Hash/Clone/Compare/Display/Debug) into module.items so
        // codegen emits C bodies and operator dispatch (`==`/`<`/`.clone()`/…)
        // resolves them. After check_module; serde was already injected pre-check
        // (this pass skips it — already provided). User-explicit methods win.
        let _t = crate::perf_timer::PerfTimer::new("auto-derive-inject");
        crate::protocols::auto_derive::inject_synthesized_methods(&mut module);
    }
    {
        let _t = crate::perf_timer::PerfTimer::new("annotate-maps");
        types::annotate_map_literals(&mut module);
    }
    {
        let _t = crate::perf_timer::PerfTimer::new("desugar");
        crate::desugar::desugar_module(&mut module);
    }
    {
        let _t = crate::perf_timer::PerfTimer::new("effects-infer");
        types::infer_effects(&mut module);
    }
    {
        let _t = crate::perf_timer::PerfTimer::new("callnorm");
        crate::callnorm::normalize_module(&mut module, &module_env.resolved_callees);
    crate::chain_norm::normalize_chains_module(&mut module, &module_env.resolved_types);
    }
    // Plan 123.1 (D217): method-local receiver field caching. AST-pass
    // вставляет prefix-let `let _at_<F> = @<F>` для ro-fields accessed
    // ≥ threshold times — устраняет redundant `self->X` derefs в .c
    // output. Pass — pure AST→AST трансформация, semantic equivalence
    // guaranteed (D217 §1). Escape hatch — env var NOVA_FIELD_CACHE=0
    // или CLI flag (см. cmd_test_all). Threshold default 2, max 8
    // per fn.
    {
        let _t = crate::perf_timer::PerfTimer::new("field-cache");
        let cfg = crate::field_cache::FieldCacheConfig::from_env_or_default();
        crate::field_cache::cache_module(&mut module, &cfg);
    }
    // 172.1.2 post-normalize канал (2026-07-04): нормализации/field_cache
    // создают синтетические узлы (UNSET id) — канал чекера для них недостижим
    // по построению. Нумеруем ТОЛЬКО их (оффсет 2^30, существующие id
    // стабильны) и ПЕРЕ-ЧЕКАЕМ нормализованное дерево (annotate-only: ошибки
    // подавляются — семантика сохранена нормализациями, повторные диагностики
    // не нужны); канал заменяется полным (чек на ТОМ ЖЕ дереве, что эмитится —
    // §0-целевая форма). Err пере-чека → остаёмся на старом канале (degrade).
    let module_env = {
        let _t = crate::perf_timer::PerfTimer::new("post-normalize-annotate");
        let extra_lits = crate::number_exprs::number_unset_exprs(&mut module);
        match types::check_module(&module) {
            Ok(mut env2) => {
                for (k, v) in extra_lits { env2.resolved_types.entry(k).or_insert(v); }
                for (k, v) in module_env.resolved_types.iter() {
                    env2.resolved_types.entry(*k).or_insert_with(|| v.clone());
                }
                // proven-наборы/callees из ПЕРВОГО чека сохраняем при пустых.
                if env2.proven_contracts.is_empty() {
                    env2.proven_contracts = module_env.proven_contracts.clone();
                }
                env2
            }
            Err(_) => module_env,
        }
    };

    // [M-runner-testless-units-main-impl]: does this compilation unit have
    // a runnable entry point? `nova_fn_main_impl` is only emitted (see
    // emit_main_wrapper/emit_nova_main in codegen/emit_c.rs) when the FINAL
    // merged module (post folder-module peer-merge/imports above) has ≥1
    // `test "..."` block or an explicit top-level `fn main()`. `bench`-only
    // modules don't count — bench_mode is off on the `nova test` path, so
    // the bench branch of emit_main_wrapper never fires here (bench items
    // ignored под nova test/nova build per D57 doc-comment on Item::Bench).
    // Checked on `module` (not raw `src`) so folder-module CUs whose tests
    // live in a peer file (not the alphabetically-first entry) are still
    // detected correctly.
    let has_runnable_entry = module.items.iter().any(|it| match it {
        ast::Item::Test(_) => true,
        ast::Item::Fn(f) => f.name == "main",
        _ => false,
    });

    let (emit_output, warnings) = {
        let _t = crate::perf_timer::PerfTimer::new("codegen");
        let mut emitter = CEmitter::new();
        emitter.set_source_for_annotations(src.to_string());
        // Plan 140.1 Ф.2 (D24/D13 amend): source file name for the
        // location-first contract/assert diagnostic prefix.
        {
            let fname = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            emitter.set_source_file_name(fname);
        }
        if let Some(n) = mono_depth {
            emitter.set_mono_depth_limit(n);
        }
        // Plan 194 A2.1 (замена Plan 140 Ф.2): build-policy режим. Legacy
        // `off` retired — недоказанные проверяются под всеми тремя значениями.
        emitter.set_contracts_mode(contracts_mode);
        // Plan 140 Ф.3 (D24 amend): feed Z3/Trivial-proven contracts from the
        // VerificationPipeline (run inside check_module above) so proven
        // requires/ensures are elided at codegen (zero-cost). Without this the
        // proven set is empty → every contract is runtime-checked even when
        // statically proven. Безопасный degrade без Z3 (TrivialBackend proves
        // a smaller class → больше runtime-checked, не unsafe).
        emitter.set_proven_contracts(&module_env.proven_contracts);
        // Plan 140.2 Part B (D257 / B.4): proven index-сайты для элизии bounds-check.
        emitter.set_proven_index_sites(&module_env.proven_index_sites);
        emitter.set_proven_index_sites_contract(&module_env.proven_index_sites_contract);
        // Plan 172.1 U.4.1: feed per-Expr resolved-type annotations to the emitter.
        emitter.set_resolved_types(&module_env.resolved_types);
        // №279: feed the per-pattern resolved-sum-name channel to the emitter.
        emitter.set_pattern_variant_types(&module_env.pattern_variant_types);
        // Plan 172.1 U.4.3: feed the resolved-callee channel (ExprId → chosen callee
        // FnDecl.span) so codegen reads its OWN view of the chosen callee instead of
        // re-resolving the overload (§0). Stage (a): equivalence-assert (debug).
        emitter.set_resolved_callees(&module_env.resolved_callees);
        // Plan 196.5 Stage-A: feed the per-call subst-value channel (mirrors
        // set_resolved_callees above).
        emitter.set_node_substs(&module_env.node_substs);
        // Plan 209 Ф.2: `emit_module_multi_tu` back-compat wrapper — runs the
        // IDENTICAL emission `emit_module` always ran, then only if
        // `NOVA_MULTI_TU=1` AND the CU exceeds the Ф.1 threshold does it hand
        // off to `split_tu`. Under any other condition it returns
        // `EmitOutput::Single` wrapping the EXACT SAME string `emit_module`
        // would have produced (Ф.1 A4 doc) — the `Single` arm below is
        // therefore byte-identical to the pre-209 write.
        let cu_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("cu").to_string();
        emitter.emit_module_multi_tu(&module, &cu_name)
            .map_err(|e| format!("codegen error: {}", e))?
    };
    let artifact = match emit_output {
        crate::codegen::EmitOutput::Single(c_code) => {
            let out_path = path.with_extension("c");
            std::fs::write(&out_path, &c_code).map_err(|e| {
                format!(
                    "failed to write {}: {}",
                    out_path.display(),
                    e
                )
            })?;
            CodegenArtifact::Single
        }
        crate::codegen::EmitOutput::Split { common_h, parts } => {
            CodegenArtifact::Split { common_h, parts }
        }
    };
    Ok((warnings, lint_warnings, has_runnable_entry, artifact))
}

// ---------- test-all: walk + summary ----------

pub struct TestAllOpts<'a> {
    /// [36.D.1] One or more directories/files to scan. Replaces single tests_dir
    /// + include_stdlib. Display names are built relative to cwd.
    pub input_dirs: &'a [PathBuf],
    /// Kept for fallback when input_dirs is empty.
    pub tests_dir: &'a Path,
    pub filter: Option<&'a str>,
    pub mode: Mode,
    pub toolchain: Toolchain,
    pub cg_include: &'a Path,
    pub rt_dir: &'a Path,
    pub tmp_dir: &'a Path,
    pub keep_artifacts: bool,
    /// Plan 22: libuv path (None = auto-detect через rt_dir/libuv).
    pub libuv: Option<LibuvConfig>,
    /// Plan 26 Ф.1: timeout на каждый child-процесс. Default 60 s.
    pub timeout: Duration,
    /// Plan 26 Ф.3: количество worker-threads для параллельного прогона.
    /// 1 = sequential (legacy mode). Default `num_cpus()`.
    pub jobs: usize,
    /// Plan 26 Ф.4: формат output. `Text` (default) — human-friendly,
    /// `Json` (one event per line) — для CI parser'ов, `Tap` — TAP-13.
    pub format: OutputFormat,
    /// Plan 26 Ф.9: verbose/quiet mode.
    pub verbosity: Verbosity,
    /// Plan 26 Ф.5: путь к test-cache (None = cache disabled).
    pub cache_dir: Option<&'a Path>,
    /// Plan 26 Ф.10: путь к last-results.json — для --rerun-failed.
    /// None = не писать results на диск.
    pub results_file: Option<&'a Path>,
    /// Если true: фильтровать только тесты которые были fail/timeout
    /// в `results_file`. Если results_file нет или unreadable — error.
    pub rerun_failed: bool,
    /// Plan 26 Ф.12: количество retry для **transient** fail'ов
    /// (AV-race `cannot open output file`, etc.). 0 = no retry.
    /// Default 0 в CLI, типичное значение для CI = 2.
    pub retries: u32,
    /// Plan 27 Ф.1: GC backend. Propagated to every TestBuildOpts → BuildOpts.
    pub gc_kind: GcKind,
    /// Plan 27 Б.5: перечислить тесты без запуска (--list).
    pub list_only: bool,
    /// Plan 27 Б.5: фильтровать тесты из файла (--filter-from <path>).
    /// Exact-match по display name, один тест на строку.
    pub filter_from: Option<&'a Path>,
    /// Plan 27 Б.7: seed для Fisher-Yates shuffle (--shuffle [SEED]).
    /// None = не перемешивать. 0 = случайный seed из system time.
    pub shuffle_seed: Option<u64>,
    /// Plan 36.D: skip patterns — substring match по display name.
    /// Example: `--skip std/runtime/` исключает все runtime тесты.
    /// Repeatable: `--skip A --skip B` исключает оба.
    pub skip: &'a [String],
    /// Plan 48 Ф.7.6: optional monomorphization-depth override.
    /// Propagated to every per-test TestBuildOpts so polymorphic-recursion
    /// guard уходит из hardcoded 500 в configurable CLI knob.
    pub mono_depth: Option<usize>,
    /// Plan 194 A2.1 (замена Plan 140 Ф.2 / D24 amend `contracts_off: bool`):
    /// build-policy режим для всего прогона. Propagated to every per-test
    /// TestBuildOpts. Default `Checked`. Legacy `off` убран.
    pub contracts_mode: ast::ContractsMode,
    /// Plan 169.1.1: test type + slow selection. Default = {Positive}, no slow.
    pub selection: TestSelection,
    /// [M-169-timing-report-regression-gate]: if > 0, after run_all report
    /// tests whose total elapsed_ms exceeds this threshold and exit with
    /// code 3. Default 0 (disabled).
    pub max_test_ms: u128,
    /// Plan 172.1 U.7.1: after the run, emit the CC-FAIL audit report
    /// (un-expected type-class CC-FAIL leaks on the corpus + a classification
    /// of every existing EXPECT_CC_ERROR fixture). Tooling-only, no codegen
    /// change. Default `false`. See [`print_cc_leak_report`].
    pub report_cc_leaks: bool,
    /// [M-standalone-out-of-tree-interp-sb-typedef]: propagated to every
    /// per-job `TestBuildOpts` — see its doc-comment.
    pub repo: &'a Path,
    pub stdlib_dir: &'a Path,
}

// ---------- Plan 26 Ф.13: graceful Ctrl+C ----------

use std::sync::atomic::{AtomicBool, Ordering};

/// Global cancellation flag. Set'ится из signal-handler'а при Ctrl+C
/// (SIGINT) и проверяется worker thread'ами перед каждым тестом.
/// Если true — worker'ы возвращают сразу, run_all возвращает partial
/// summary.
static CANCELLED: AtomicBool = AtomicBool::new(false);

/// Установить SIGINT/Ctrl+C handler. Idempotent — повторные вызовы
/// корректно ждут завершения первого install'а.
/// Внутри handler'а: atomic flag, **никаких** allocations (signal-safety
/// rules).
///
/// Plan 26 Ф.17 #3: 3-state machine для thread-safe idempotency.
/// Состояния: 0 = not started, 1 = installing, 2 = installed.
/// Без этого 2 одновременных вызова `swap(true)` могли вернуться **до**
/// того как первый закончил unsafe-блок.
pub fn install_cancel_handler() {
    use std::sync::atomic::AtomicU8;
    const STATE_NEW: u8 = 0;
    const STATE_INSTALLING: u8 = 1;
    const STATE_DONE: u8 = 2;
    static STATE: AtomicU8 = AtomicU8::new(STATE_NEW);

    // Пытаемся claim install slot: NEW → INSTALLING.
    match STATE.compare_exchange(
        STATE_NEW,
        STATE_INSTALLING,
        Ordering::SeqCst,
        Ordering::SeqCst,
    ) {
        Ok(_) => {
            // Мы owner — продолжаем install.
        }
        Err(STATE_DONE) => {
            // Уже установлен — return.
            return;
        }
        Err(_) => {
            // STATE_INSTALLING — другой thread в процессе. Spin до DONE
            // (install сам должен закончиться за микросекунды).
            while STATE.load(Ordering::SeqCst) != STATE_DONE {
                std::hint::spin_loop();
            }
            return;
        }
    }
    #[cfg(target_os = "windows")]
    {
        // SetConsoleCtrlHandler via raw Win32. Signature:
        //   BOOL WINAPI HandlerRoutine(DWORD dwCtrlType);
        // Возвращает TRUE = handled, FALSE = next handler.
        type PhandlerRoutine = unsafe extern "system" fn(u32) -> i32;
        extern "system" {
            fn SetConsoleCtrlHandler(handler: PhandlerRoutine, add: i32) -> i32;
        }
        unsafe extern "system" fn handler(_ctrl_type: u32) -> i32 {
            CANCELLED.store(true, Ordering::SeqCst);
            1 // TRUE — handled, не пускаем дефолтному terminate'у завершить
              // процесс мгновенно, дадим workers cleanup.
        }
        unsafe {
            SetConsoleCtrlHandler(handler, 1);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // POSIX signal через `libc::signal`. Минимальный handler —
        // только atomic store.
        extern "C" {
            fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
        }
        const SIGINT: i32 = 2;
        const SIGTERM: i32 = 15;
        extern "C" fn handler(_sig: i32) {
            CANCELLED.store(true, Ordering::SeqCst);
        }
        unsafe {
            signal(SIGINT, handler);
            signal(SIGTERM, handler);
        }
    }
    // Plan 26 Ф.17 #3: mark install complete — concurrent callers spinning
    // на STATE_INSTALLING выйдут.
    STATE.store(STATE_DONE, Ordering::SeqCst);
}

/// Проверить установлен ли cancel-флаг. Worker thread'ы вызывают перед
/// каждым тестом — если true, прекращают забирать новые jobs.
pub fn is_cancelled() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

/// Reset cancel-флага для unit-тестов.
#[cfg(test)]
fn reset_cancelled_for_test() {
    CANCELLED.store(false, Ordering::SeqCst);
}

/// Plan 26 Ф.12: classify whether outcome looks like transient AV/race
/// failure которую стоит retry'нуть. Real test fails (expectation mismatch,
/// codegen error) — НЕ retry'им, это были бы false-PASS.
pub fn is_transient_fail(outcome: &Outcome) -> bool {
    match outcome {
        Outcome::Fail { stage, .. } => match stage {
            // Linker race: lld-link / cl.exe не может открыть .exe потому
            // что AV держит handle от свежей сборки соседнего worker'а.
            // Также: `cannot open input file` (.obj locked).
            Stage::Cc { error } => {
                let e = error.to_lowercase();
                e.contains("cannot open output file")
                    || e.contains("cannot open input file")
                    || e.contains("being used by another process")
                    || e.contains("permission denied")
                    || e.contains("access is denied")
                    || e.contains("os error 5")
                    || e.contains("os error 32")  // ERROR_SHARING_VIOLATION
            }
            // Run-fail: AV может также блокировать запуск exe.
            Stage::Run { error } => {
                let e = error.to_lowercase();
                e.contains("being used by another process")
                    || e.contains("access is denied")
                    || e.contains("os error 5")
                    || e.contains("os error 32")
            }
            // Codegen errors, expectation mismatches, NoCFile — real fails.
            _ => false,
        },
        // Timeout — потенциально transient (heavy load), но обычно реальный hang.
        // Не retry'им по умолчанию — пользователь явно увидит и решит.
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Tap,
    /// Plan 26 Ф.14: JUnit XML — стандарт CI (GitHub Actions, GitLab,
    /// Jenkins, Azure DevOps, TeamCity). Emit'ится только в summary
    /// (per-test events не stream'ятся; XML требует cumulative aggregate).
    Junit,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "text" => Ok(OutputFormat::Text),
            "json" => Ok(OutputFormat::Json),
            "tap" => Ok(OutputFormat::Tap),
            "junit" => Ok(OutputFormat::Junit),
            _ => Err(anyhow!("unknown format `{}` (expected text|json|tap|junit)", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    /// `--quiet` — print только FAIL lines + summary.
    Quiet,
    /// Default — print per-test PASS/FAIL + summary.
    Normal,
    /// `--verbose` — то же + stdout/stderr child процессов на PASS.
    /// (TODO: реальная capture-stdout, сейчас только маркер.)
    Verbose,
}

impl Verbosity {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "quiet" => Ok(Verbosity::Quiet),
            "normal" => Ok(Verbosity::Normal),
            "verbose" => Ok(Verbosity::Verbose),
            _ => Err(anyhow!("unknown verbosity `{}` (quiet|normal|verbose)", s)),
        }
    }
}

/// Plan 26 Ф.10: serializable record для last-results.json. Структура
/// stable, чтобы старые results-files оставались читаемы при minor-bumps.
/// Plan 169.1 Ф.1: split timing — compile_ms (codegen→C→cc), run_ms (exe execution).
/// Missing fields in old files decode as 0 (backward-compat).
#[derive(Debug, Clone)]
pub struct ResultRecord {
    pub name: String,
    pub passed: bool,
    pub elapsed_ms: u128,
    /// Time spent in codegen (.nv→.c) + C compiler (cc) phase. 0 for skip/timeout.
    pub compile_ms: u128,
    /// Time spent executing the compiled binary. 0 for skip/timeout/compile-fail.
    pub run_ms: u128,
}

/// Helper: best-effort `num_cpus()` без extra-deps. Stable API в std 1.59+.
pub fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Plan 22 F2: libuv MANDATORY. Auto-detect libuv submodule в rt_dir/libuv.
/// Если submodule initialized И libuv.lib built — возвращает LibuvConfig.
/// Если submodule нет либо build fails — eprintln + std::process::exit(1).
/// Plan 22 R7 «no busy-loops anywhere» absolute: no fallback path.
pub fn detect_or_build_libuv(rt_dir: &Path, repo_root: &Path,
                              vcvars: Option<&Path>) -> Option<LibuvConfig> {
    let libuv_dir = rt_dir.join("libuv");
    let include_dir = libuv_dir.join("include");
    let uv_h = include_dir.join("uv.h");
    if !uv_h.is_file() {
        eprintln!(
            "nova: FATAL libuv submodule not initialized at {}.\n\
             Plan 22 F2: libuv is mandatory. Run:\n\
             \tgit submodule update --init compiler-codegen/nova_rt/libuv",
            libuv_dir.display()
        );
        std::process::exit(1);
    }
    let eventloop_src = rt_dir.join("eventloop.c");
    if !eventloop_src.is_file() {
        eprintln!("nova: FATAL eventloop.c not found at {}", eventloop_src.display());
        std::process::exit(1);
    }
    let cache_dir = repo_root.join("target").join("libuv-cache");
    let lib_name = if cfg!(target_os = "windows") { "libuv.lib" } else { "libuv.a" };
    let lib_file = cache_dir.join(lib_name);
    if lib_file.is_file() {
        return Some(LibuvConfig {
            include_dir,
            lib_file,
            eventloop_src,
        });
    }
    // Build libuv lazy при первом запуске.
    eprintln!("nova: libuv not built, building (one-time, ~30 sec)...");
    if let Err(e) = build_libuv_lib(&libuv_dir, &cache_dir, vcvars) {
        eprintln!(
            "nova: FATAL failed to build libuv: {}\n\
             Plan 22 F2: libuv is mandatory. Check vcvars64.bat, \
             cl.exe / clang availability, and libuv submodule integrity.",
            e
        );
        std::process::exit(1);
    }
    if lib_file.is_file() {
        Some(LibuvConfig {
            include_dir,
            lib_file,
            eventloop_src,
        })
    } else {
        eprintln!(
            "nova: FATAL libuv build succeeded but {} not found",
            lib_file.display()
        );
        std::process::exit(1);
    }
}

/// Plan 27 Ф.D (audit 2026-05-12): detect Boehm GC installation with
/// graceful fallback. Returns Some(config) если найден, None — иначе
/// (caller вызывает resolve_gc_or_exit для honest exit).
///
/// **Lookup order:**
///
/// 1. `$NOVA_GC_LIB_DIR` (+ optional `$NOVA_GC_INCLUDE_DIR`) — CI/custom override.
/// 2. **Windows:**
///    a. Local vcpkg: `<cg_include>/vcpkg_installed/x64-windows-static/`.
///    b. Global vcpkg: `$VCPKG_ROOT/installed/x64-windows-static/`.
/// 3. **Linux:** проверяет `gc.h` в стандартных paths — если найден, возвращает
///    Some({include_dir: Some, lib_dir: None}). Иначе None.
/// 4. **macOS:** Homebrew (`/opt/homebrew/include/gc.h` на Apple Silicon или
///    `/usr/local/include/gc.h` на Intel).
pub fn detect_boehm(cg_include: &Path) -> Option<BoehmConfig> {
    // 1. Env override (highest priority).
    if let Ok(lib_dir_env) = std::env::var("NOVA_GC_LIB_DIR") {
        let lib_dir = PathBuf::from(&lib_dir_env);
        let include_dir = std::env::var("NOVA_GC_INCLUDE_DIR")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                // Авто-вывод include из lib: lib/../include (vcpkg-layout).
                lib_dir.parent().map(|p| p.join("include")).filter(|p| p.exists())
            });
        return Some(BoehmConfig {
            include_dir,
            lib_dir: Some(lib_dir),
        });
    }

    // 2. Windows: vcpkg paths.
    #[cfg(target_os = "windows")]
    {
        // 2a. Local vcpkg (current behaviour).
        let local_inc = cg_include
            .join("vcpkg_installed")
            .join("x64-windows-static")
            .join("include");
        let local_lib = cg_include
            .join("vcpkg_installed")
            .join("x64-windows-static")
            .join("lib");
        if local_lib.join("gc.lib").is_file() {
            return Some(BoehmConfig {
                include_dir: Some(local_inc),
                lib_dir: Some(local_lib),
            });
        }
        // 2b. Global vcpkg via VCPKG_ROOT.
        if let Ok(vcpkg_root) = std::env::var("VCPKG_ROOT") {
            let global_inc = PathBuf::from(&vcpkg_root)
                .join("installed")
                .join("x64-windows-static")
                .join("include");
            let global_lib = PathBuf::from(&vcpkg_root)
                .join("installed")
                .join("x64-windows-static")
                .join("lib");
            if global_lib.join("gc.lib").is_file() {
                return Some(BoehmConfig {
                    include_dir: Some(global_inc),
                    lib_dir: Some(global_lib),
                });
            }
        }
        return None;
    }

    // 3. Linux: system libgc — проверяем header через известные paths.
    #[cfg(target_os = "linux")]
    {
        let _ = cg_include;  // silence unused warning
        let candidates = [
            "/usr/include/gc.h",
            "/usr/include/gc/gc.h",
            "/usr/local/include/gc.h",
        ];
        for c in candidates {
            if std::path::Path::new(c).is_file() {
                // lib_dir None → linker finds via -lgc в standard path.
                let inc = std::path::Path::new(c).parent().map(PathBuf::from);
                return Some(BoehmConfig {
                    include_dir: inc,
                    lib_dir: None,
                });
            }
        }
        return None;
    }

    // 4. macOS: Homebrew paths.
    #[cfg(target_os = "macos")]
    {
        let _ = cg_include;
        let candidates = [
            "/opt/homebrew/include/gc.h",   // Apple Silicon
            "/usr/local/include/gc.h",      // Intel
        ];
        for c in candidates {
            if std::path::Path::new(c).is_file() {
                let p = std::path::Path::new(c);
                let inc = p.parent().map(PathBuf::from);
                let lib = p.parent()
                    .and_then(|d| d.parent())
                    .map(|prefix| prefix.join("lib"));
                return Some(BoehmConfig {
                    include_dir: inc,
                    lib_dir: lib,
                });
            }
        }
        return None;
    }

    #[allow(unreachable_code)]
    None
}

/// #269 Ф.2: `nova_rt` sources use BOTH the flat upstream include convention
/// (`#include <gc.h>` — alloc_boehm.c, runtime.c, driver.c, ...) AND a
/// `gc/`-namespaced one (`#include <gc/gc.h>`, `<gc/gc_mark.h>` —
/// fiber_arena.c/fiber_arena_win.c only). vcpkg's `bdwgc` port happens to
/// install headers BOTH ways (`vcpkg_installed/.../include/gc.h` AND
/// `.../include/gc/gc.h`, confirmed by directory listing — bdwgc's own
/// `CMakeLists.txt` has no `install()` rule at all, so this is a vcpkg-side
/// convention, not upstream's), which is why the existing vcpkg/env path
/// never hit this gap. The raw bdwgc submodule tree only has the flat form
/// (`gc_dir/include/gc.h`, no nested `gc/` folder) — copy the handful of
/// headers `nova_rt` actually needs into `cache_dir/include/` (flat, for
/// `<gc.h>`) AND `cache_dir/include/gc/` (nested, for `<gc/gc.h>`) so a
/// single `-I cache_dir/include` satisfies both forms, matching vcpkg's
/// observed layout — found by running the actual clean-clone acceptance
/// gate (`fiber_arena_win.c(55): fatal error C1083: ... gc/gc.h: No such
/// file or directory`), not by reading the vcpkg port source ahead of time.
/// Copies into the submodule's OWN `include/` dir are deliberately avoided
/// (never write into a vendored git submodule's working tree — would show
/// up as an unexpected dirty submodule / risk being committed by accident).
fn populate_boehm_include_dir(gc_include: &Path, cache_include: &Path) -> Result<()> {
    let nested = cache_include.join("gc");
    std::fs::create_dir_all(&nested)
        .map_err(|e| anyhow!("create {}: {}", nested.display(), e))?;
    for entry in std::fs::read_dir(gc_include)
        .map_err(|e| anyhow!("read {}: {}", gc_include.display(), e))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("h") {
            continue;
        }
        let name = path.file_name().unwrap();
        std::fs::copy(&path, cache_include.join(name))
            .map_err(|e| anyhow!("copy {} (flat): {}", path.display(), e))?;
        std::fs::copy(&path, nested.join(name))
            .map_err(|e| anyhow!("copy {} (nested gc/): {}", path.display(), e))?;
    }
    Ok(())
}

/// #269 Ф.2 [M-gc-lib-not-bundled-clean-install]: one-time fallback build of
/// Boehm GC from the vendored `bdwgc` submodule (`rt_dir/gc`) — mirrors
/// `detect_or_build_libuv` 1:1. Called ONLY when `detect_boehm` (env var /
/// vcpkg lookup, unchanged priority) returns `None` — a clean clone with no
/// vcpkg installed no longer dead-ends on a FATAL, it self-builds instead,
/// exactly like libuv already does.
///
/// **Windows only** in this window (Ф.2 scope decision, see PROGRESS-gc269.md):
/// bdwgc's official single-file amalgamation (`extra/gc.c`, includes every
/// other `.c` in the tree via relative `"../foo.c"` quote-includes — no
/// cmake needed, just compile this ONE file) needs a real atomics backend
/// on MSVC (`cl.exe` has no `__atomic_*` GCC/clang builtins), which bdwgc's
/// own `include/private/gc_atomic_ops.h` falls back to only via the
/// external `libatomic_ops` project's `atomic_ops.h` (confirmed against
/// bdwgc's own `CMakeLists.txt`: `if (... OR MSVC ...) include_directories
/// (libatomic_ops/src)`, and against the vcpkg `bdwgc` port, which pulls in
/// vcpkg's separate `libatomic-ops` port for exactly this reason) — hence
/// the SECOND submodule `rt_dir/libatomic_ops` (pinned v7.8.2, matching the
/// version found in this repo's own vcpkg cache). On x86_64 the needed
/// primitives are header-only (MSVC `_Interlocked*` intrinsics inline in
/// `atomic_ops.h` — confirmed empirically: linking against a `gc.lib` built
/// this way pulls in ZERO unresolved externals without any separately
/// compiled `atomic_ops.lib`; bdwgc's own CMakeLists agrees, leaving
/// `ATOMIC_OPS_LIBS` empty with a "assume library not needed" comment), so
/// only the header directory is needed — no second archive to build/link.
/// Linux/macOS already has a working zero-vcpkg path (`apt install
/// libgc-dev` / `brew install bdw-gc`, existing `detect_boehm` branches
/// below) and gcc/clang provide `__atomic_*` builtins directly
/// (`GC_BUILTIN_ATOMIC`), so a from-source fallback isn't the blocking gap
/// there — left for a follow-up window if a vcpkg-equivalent gap ever shows
/// up on those platforms (not verified end-to-end on this window's
/// Windows-only dev machine; explicitly NOT claimed done here).
///
/// Cache: `repo_root/target/gc-cache/gc.lib` (+ obj scratch dir, cleaned up
/// after build) — same `target/`-rooted convention as `libuv-cache`.
/// Idempotent/cheap on repeat calls (disk `is_file()` check short-circuits
/// before ever touching the compiler), safe to call from both the early
/// honest-exit check (`resolve_gc_or_exit`) and `build_command`'s own
/// flag-derivation block — see call sites.
pub fn detect_or_build_boehm_fallback(
    rt_dir: &Path,
    repo_root: &Path,
    vcvars: Option<&Path>,
) -> Option<BoehmConfig> {
    #[cfg(target_os = "windows")]
    {
        let gc_dir = rt_dir.join("gc");
        let ao_dir = rt_dir.join("libatomic_ops");
        let gc_include = gc_dir.join("include");
        let gc_amalgam = gc_dir.join("extra").join("gc.c");
        let ao_header = ao_dir.join("src").join("atomic_ops.h");
        if !gc_include.join("gc.h").is_file() || !gc_amalgam.is_file() || !ao_header.is_file() {
            // Submodule(s) not initialized — not fatal here; the caller's
            // combined FATAL message (resolve_gc_or_exit) names both the
            // vcpkg AND the submodule-init remedy.
            return None;
        }
        let cache_dir = repo_root.join("target").join("gc-cache");
        let gc_lib = cache_dir.join("gc.lib");
        let cache_include = cache_dir.join("include");
        if gc_lib.is_file() && cache_include.join("gc.h").is_file()
            && cache_include.join("gc").join("gc.h").is_file()
        {
            return Some(BoehmConfig { include_dir: Some(cache_include), lib_dir: Some(cache_dir) });
        }
        eprintln!(
            "nova: Boehm GC (gc.lib) not found via $NOVA_GC_LIB_DIR/vcpkg — \
             building from vendored bdwgc submodule (one-time, ~10 sec)..."
        );
        if let Err(e) = populate_boehm_include_dir(&gc_include, &cache_include) {
            eprintln!("nova: warning: bdwgc fallback build failed: copy headers: {}", e);
            return None;
        }
        if let Err(e) = build_boehm_lib(&gc_dir, &ao_dir, &cache_dir, vcvars) {
            eprintln!("nova: warning: bdwgc fallback build failed: {}", e);
            return None;
        }
        if gc_lib.is_file() {
            eprintln!("nova: gc.lib built from vendored bdwgc source ({})", gc_lib.display());
            return Some(BoehmConfig { include_dir: Some(cache_include), lib_dir: Some(cache_dir) });
        }
        eprintln!("nova: FATAL bdwgc fallback build succeeded but {} not found", gc_lib.display());
        return None;
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Ф.2 scope decision: Linux/macOS keep the existing apt/brew-based
        // `detect_boehm` path unchanged — see fn doc above for why this
        // isn't the blocking gap on those platforms in this window.
        let _ = (rt_dir, repo_root, vcvars);
        None
    }
}

/// Plan 27 Ф.D + #269 Ф.2: если backend = Boehm, проверяет наличие через
/// `detect_boehm` (env var / vcpkg — unchanged priority), затем — #269 Ф.2 —
/// пытается one-time fallback build из вендорённого bdwgc-сабмодуля
/// (`detect_or_build_boehm_fallback`) ПЕРЕД honest fatal. На fail печатает
/// platform-specific install hint и завершает процесс. Возвращает
/// Some(BoehmConfig) если backend = Boehm и detection (или fallback build)
/// OK, None если backend = Malloc (Boehm не нужен).
///
/// `rt_dir`: `compiler-codegen/nova_rt` — anchors the `gc`/`libatomic_ops`
/// submodule lookup AND (via `.parent().parent()`) `repo_root` for the
/// `target/gc-cache` build cache, mirrors `detect_or_build_libuv`'s own
/// `rt_dir`/`repo_root` pair. `vcvars`: passed straight through to the
/// fallback builder's `cl.exe`/`lib.exe` invocations (Windows only).
pub fn resolve_gc_or_exit(gc: GcKind, cg_include: &Path, rt_dir: &Path, vcvars: Option<&Path>) -> Option<BoehmConfig> {
    if gc != GcKind::Boehm {
        return None;
    }
    if let Some(cfg) = detect_boehm(cg_include) {
        return Some(cfg);
    }
    if let Some(repo_root) = rt_dir.parent().and_then(|p| p.parent()) {
        if let Some(cfg) = detect_or_build_boehm_fallback(rt_dir, repo_root, vcvars) {
            return Some(cfg);
        }
    }
    // Honest fatal с platform-specific hint.
    #[cfg(target_os = "windows")]
    eprintln!(
        "nova: FATAL Boehm GC (gc.lib) not found.\n\
         \n\
         Lookup order tried:\n\
           1. $NOVA_GC_LIB_DIR env var\n\
           2. {}\\vcpkg_installed\\x64-windows-static\\lib\\gc.lib\n\
           3. $VCPKG_ROOT\\installed\\x64-windows-static\\lib\\gc.lib\n\
           4. vendored bdwgc submodule fallback build ({}\\gc\\extra\\gc.c)\n\
         \n\
         To fix (pick one):\n\
           cd compiler-codegen && vcpkg install bdwgc:x64-windows-static\n\
           git submodule update --init compiler-codegen/nova_rt/gc \\\n\
                                        compiler-codegen/nova_rt/libatomic_ops\n\
         \n\
         Or use --gc malloc for benchmarks (no GC, leaks).",
        cg_include.display(),
        rt_dir.display()
    );
    #[cfg(target_os = "linux")]
    eprintln!(
        "nova: FATAL Boehm GC (libgc) not found.\n\
         \n\
         Header `gc.h` not present in /usr/include, /usr/local/include.\n\
         \n\
         To fix:\n\
           sudo apt install libgc-dev        # Debian/Ubuntu\n\
           sudo dnf install gc-devel         # Fedora/RHEL\n\
           sudo pacman -S gc                 # Arch\n\
         \n\
         Or use --gc malloc for benchmarks (no GC, leaks)."
    );
    #[cfg(target_os = "macos")]
    eprintln!(
        "nova: FATAL Boehm GC (libgc) not found.\n\
         \n\
         Header `gc.h` not present in /opt/homebrew/include or /usr/local/include.\n\
         \n\
         To fix:\n\
           brew install bdw-gc\n\
         \n\
         Or use --gc malloc for benchmarks (no GC, leaks)."
    );
    std::process::exit(1);
}

/// Plan 22 Ф.1: compile libuv source files в libuv.lib / libuv.a.
/// Кэшируется в repo_root/target/libuv-cache/ через VERSION stamp.
fn build_libuv_lib(libuv_dir: &Path, cache_dir: &Path,
                    vcvars: Option<&Path>) -> Result<()> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| anyhow!("create cache_dir: {}", e))?;
    let obj_dir = cache_dir.join("obj");
    if obj_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&obj_dir);
    }
    std::fs::create_dir_all(&obj_dir)
        .map_err(|e| anyhow!("create obj_dir: {}", e))?;

    // Collect source files: src/*.c + src/{win,unix}/*.c.
    let mut srcs: Vec<PathBuf> = Vec::new();
    let src_root = libuv_dir.join("src");
    collect_c_files(&src_root, &mut srcs, /*recursive*/ false)?;
    #[cfg(target_os = "windows")]
    {
        collect_c_files(&src_root.join("win"), &mut srcs, /*recursive*/ false)?;
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        /* libuv puts platform-specific impls в src/unix/ как отдельные .c
         * (linux.c, freebsd.c, openbsd.c, darwin.c, sunos.c, aix.c,
         * ibmi.c, os390.c, ...). Whitelist approach: компилируем common
         * unix files + platform-specific subset. См. libuv CMakeLists.txt
         * для reference list.
         */
        const COMMON_UNIX: &[&str] = &[
            "async.c", "core.c", "dl.c", "fs.c",
            "getaddrinfo.c", "getnameinfo.c",
            "loop-watcher.c", "loop.c", "pipe.c", "poll.c",
            "process.c",
            "random-devurandom.c",
            "signal.c", "stream.c", "tcp.c", "thread.c", "tty.c", "udp.c",
        ];
        #[cfg(target_os = "linux")]
        const PLATFORM_FILES: &[&str] = &[
            "linux.c", "procfs-exepath.c",
            "proctitle.c",
            "random-getrandom.c", "random-sysctl-linux.c",
            "no-fsevents.c",
            /* hrtime: linux.c provides uv__hrtime; не подключаем posix-hrtime.c */
        ];
        #[cfg(target_os = "macos")]
        const PLATFORM_FILES: &[&str] = &[
            "darwin.c", "darwin-proctitle.c",
            "kqueue.c", "fsevents.c",
            "bsd-ifaddrs.c", "bsd-proctitle.c",
            "random-getentropy.c",
            "posix-hrtime.c",  /* macOS uses generic POSIX hrtime */
        ];

        let unix_dir = src_root.join("unix");
        for name in COMMON_UNIX.iter().chain(PLATFORM_FILES.iter()) {
            let p = unix_dir.join(name);
            if p.is_file() {
                srcs.push(p);
            }
        }
    }
    if srcs.is_empty() {
        return Err(anyhow!("no libuv source files found in {}",
                            src_root.display()));
    }

    let inc_pub = libuv_dir.join("include");
    let inc_src = libuv_dir.join("src");
    let inc_win = libuv_dir.join("src").join("win");

    #[cfg(target_os = "windows")]
    {
        let vcv = vcvars.ok_or_else(|| anyhow!("vcvars required for libuv build on Windows"))?;
        // Write response file (cl.exe @file).
        let rsp = cache_dir.join("compile.rsp");
        let mut lines: Vec<String> = Vec::new();
        lines.push("/c /nologo /W0 /MT /O2 /D_WIN32_WINNT=0x0602 /DWIN32_LEAN_AND_MEAN /DBUILDING_UV_SHARED=0".to_string());
        lines.push(format!("/I \"{}\"", inc_pub.display()));
        lines.push(format!("/I \"{}\"", inc_src.display()));
        lines.push(format!("/I \"{}\"", inc_win.display()));
        lines.push(format!("/Fo\"{}\\\\\"", obj_dir.display()));
        for s in &srcs {
            lines.push(format!("\"{}\"", s.display()));
        }
        // №287: UTF-8 BOM — cl.exe decodes a BOM-less response file under the
        // process ANSI codepage, so a non-ASCII path (a Cyrillic user
        // directory, say) is mangled and every source after it fails with a
        // spurious `C1083: file not found`. Same fix `link_prep.rs` already
        // carries; this sibling never got it.
        std::fs::write(&rsp, format!("\u{FEFF}{}", lines.join("\n")))
            .map_err(|e| anyhow!("write rsp: {}", e))?;
        let inner = format!(
            "\"call \"{}\" >nul 2>&1 && cl.exe @\"{}\"\"",
            vcv.display(), rsp.display()
        );
        let mut cmd = Command::new("cmd");
        #[cfg(target_os = "windows")]
        {
            cmd.raw_arg("/c").raw_arg(&inner);
        }
        let out = cmd.output()
            .map_err(|e| anyhow!("spawn cl.exe: {}", e))?;
        if !out.status.success() {
            let combined = format!("{}{}",
                bytes_to_string(&out.stdout),
                bytes_to_string(&out.stderr));
            return Err(anyhow!("libuv compile failed: {}",
                combined.lines().take(10).collect::<Vec<_>>().join("\n")));
        }
        // Archive all .obj into libuv.lib через lib.exe.
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for entry in std::fs::read_dir(&obj_dir)? {
            let p = entry?.path();
            if p.extension().and_then(|s| s.to_str()) == Some("obj") {
                obj_files.push(p);
            }
        }
        let lib_file = cache_dir.join("libuv.lib");
        let lib_rsp = cache_dir.join("lib.rsp");
        let mut lib_lines: Vec<String> = Vec::new();
        lib_lines.push("/nologo".to_string());
        lib_lines.push(format!("/OUT:\"{}\"", lib_file.display()));
        for o in &obj_files {
            lib_lines.push(format!("\"{}\"", o.display()));
        }
        // №287: BOM — see the compile.rsp comment above (same non-ASCII-path
        // mangling applies to lib.exe response files).
        std::fs::write(&lib_rsp, format!("\u{FEFF}{}", lib_lines.join("\n")))
            .map_err(|e| anyhow!("write lib.rsp: {}", e))?;
        let lib_inner = format!(
            "\"call \"{}\" >nul 2>&1 && lib.exe @\"{}\"\"",
            vcv.display(), lib_rsp.display()
        );
        let mut lib_cmd = Command::new("cmd");
        lib_cmd.raw_arg("/c").raw_arg(&lib_inner);
        let lib_out = lib_cmd.output()
            .map_err(|e| anyhow!("spawn lib.exe: {}", e))?;
        if !lib_out.status.success() {
            return Err(anyhow!("lib.exe failed: {}",
                bytes_to_string(&lib_out.stderr)));
        }
        eprintln!("nova: libuv.lib built ({} files)", srcs.len());
        return Ok(());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        // Linux/macOS: compile через cc → object files → ar archive.
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for src in &srcs {
            let obj = obj_dir.join(
                src.file_name().unwrap().to_string_lossy()
                    .replace(".c", ".o")
            );
            let mut c = Command::new(&cc);
            c.args(["-c", "-O2", "-w", "-fPIC"]);
            c.arg("-D_GNU_SOURCE");
            c.arg("-I").arg(&inc_pub);
            c.arg("-I").arg(&inc_src);
            c.arg("-o").arg(&obj);
            c.arg(src);
            let out = c.output()
                .map_err(|e| anyhow!("spawn {}: {}", cc, e))?;
            if !out.status.success() {
                return Err(anyhow!("libuv compile failed on {}: {}",
                    src.display(),
                    bytes_to_string(&out.stderr)));
            }
            obj_files.push(obj);
        }
        let lib_file = cache_dir.join("libuv.a");
        let mut ar = Command::new("ar");
        ar.arg("rcs").arg(&lib_file);
        for o in &obj_files {
            ar.arg(o);
        }
        let ar_out = ar.output()
            .map_err(|e| anyhow!("spawn ar: {}", e))?;
        if !ar_out.status.success() {
            return Err(anyhow!("ar failed: {}",
                bytes_to_string(&ar_out.stderr)));
        }
        eprintln!("nova: libuv.a built ({} files)", srcs.len());
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (libuv_dir, cache_dir, vcvars);
        Err(anyhow!("unsupported platform for libuv build"))
    }
}

/// #269 Ф.2: compile bdwgc's official single-file amalgamation
/// (`gc_dir/extra/gc.c` — includes every other `.c` in `gc_dir` via
/// relative `"../foo.c"` quote-includes, so compiling this ONE file is the
/// entire build, no cmake needed) into `gc.lib`. Mirrors `build_libuv_lib`'s
/// shape (rsp file, `cl.exe` under vcvars, `lib.exe` archive) — Windows
/// only, see `detect_or_build_boehm_fallback`'s doc for why Linux/macOS
/// aren't wired to this in this window.
///
/// Defines mirror EXACTLY what this repo's own vcpkg cache used to build
/// the `bdwgc` port for `x64-windows-static` (extracted from
/// `vcpkg_installed/.../blds/bdwgc/config-x64-windows-static-rel-ninja.log`
/// `DEFINES = ...` line) — proven byte-compatible by direct empirical
/// comparison: a `gc.lib` built here and one built by vcpkg both link a
/// throwaway `GC_INIT`/`GC_MALLOC`/`GC_gcollect` C program with ZERO
/// unresolved externals and both produce identical runtime behavior
/// (single- and multi-threaded `GC_register_my_thread`/`GC_MALLOC`
/// smoke test, verified in this window's PROGRESS-gc269.md). `/I
/// ao_dir/src` supplies bdwgc's own `include/private/gc_atomic_ops.h`
/// fallback-branch dependency (`#include "atomic_ops.h"`, needed on real
/// MSVC — `cl.exe` has no GCC/clang `__atomic_*` builtins for
/// `GC_BUILTIN_ATOMIC`) — header-only on x86_64, no separate
/// `atomic_ops.lib` to build (see caller's doc).
fn build_boehm_lib(gc_dir: &Path, ao_dir: &Path, cache_dir: &Path,
                    vcvars: Option<&Path>) -> Result<()> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| anyhow!("create cache_dir: {}", e))?;
    let obj_dir = cache_dir.join("obj");
    if obj_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&obj_dir);
    }
    std::fs::create_dir_all(&obj_dir)
        .map_err(|e| anyhow!("create obj_dir: {}", e))?;

    let gc_amalgam = gc_dir.join("extra").join("gc.c");
    let gc_include = gc_dir.join("include");
    let ao_include = ao_dir.join("src");

    #[cfg(target_os = "windows")]
    {
        let vcv = vcvars.ok_or_else(|| anyhow!("vcvars required for bdwgc fallback build on Windows"))?;
        let rsp = obj_dir.join("compile.rsp");
        let defines = "/DDONT_USE_USER32_DLL /DEMPTY_GETENV_RESULTS /DENABLE_DISCLAIM \
                        /DGC_ATOMIC_UNCOLLECTABLE /DGC_ENABLE_SUSPEND_THREAD /DGC_GCJ_SUPPORT \
                        /DGC_MISSING_EXECINFO_H /DGC_NOT_DLL /DGC_NO_SIGSETJMP /DGC_THREADS \
                        /DJAVA_FINALIZATION /DNO_GETCONTEXT /DPARALLEL_MARK /DTHREAD_LOCAL_ALLOC \
                        /D_CRT_SECURE_NO_DEPRECATE";
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("/c /nologo /W0 /MT /O2 {}", defines));
        lines.push(format!("/I \"{}\"", strip_verbatim_prefix(&gc_include).display()));
        lines.push(format!("/I \"{}\"", strip_verbatim_prefix(&ao_include).display()));
        lines.push(format!("/Fo\"{}\\\\\"", strip_verbatim_prefix(&obj_dir).display()));
        lines.push(format!("\"{}\"", strip_verbatim_prefix(&gc_amalgam).display()));
        // BOM — same non-ASCII-path codepage-misdecode risk as
        // `link_prep.rs`'s rsp files (see that module's matching comment);
        // this repo's own user profile path can contain Cyrillic.
        std::fs::write(&rsp, format!("\u{FEFF}{}", lines.join("\n")))
            .map_err(|e| anyhow!("write rsp: {}", e))?;
        let inner = format!(
            "\"call \"{}\" >nul 2>&1 && cl.exe @\"{}\"\"",
            vcv.display(), rsp.display()
        );
        let mut cmd = Command::new("cmd");
        cmd.raw_arg("/c").raw_arg(&inner);
        let out = cmd.output()
            .map_err(|e| anyhow!("spawn cl.exe: {}", e))?;
        if !out.status.success() {
            let combined = format!("{}{}",
                bytes_to_string(&out.stdout),
                bytes_to_string(&out.stderr));
            return Err(anyhow!("bdwgc amalgamation compile failed: {}",
                combined.lines().take(15).collect::<Vec<_>>().join("\n")));
        }
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for entry in std::fs::read_dir(&obj_dir)? {
            let p = entry?.path();
            if p.extension().and_then(|s| s.to_str()) == Some("obj") {
                obj_files.push(p);
            }
        }
        if obj_files.is_empty() {
            return Err(anyhow!("bdwgc amalgamation compile produced no .obj files"));
        }
        let lib_file = cache_dir.join("gc.lib");
        let lib_rsp = obj_dir.join("lib.rsp");
        let mut lib_lines: Vec<String> = Vec::new();
        lib_lines.push("/nologo".to_string());
        lib_lines.push(format!("/OUT:\"{}\"", strip_verbatim_prefix(&lib_file).display()));
        for o in &obj_files {
            lib_lines.push(format!("\"{}\"", strip_verbatim_prefix(o).display()));
        }
        std::fs::write(&lib_rsp, format!("\u{FEFF}{}", lib_lines.join("\n")))
            .map_err(|e| anyhow!("write lib.rsp: {}", e))?;
        let lib_inner = format!(
            "\"call \"{}\" >nul 2>&1 && lib.exe @\"{}\"\"",
            vcv.display(), lib_rsp.display()
        );
        let mut lib_cmd = Command::new("cmd");
        lib_cmd.raw_arg("/c").raw_arg(&lib_inner);
        let lib_out = lib_cmd.output()
            .map_err(|e| anyhow!("spawn lib.exe: {}", e))?;
        if !lib_out.status.success() {
            return Err(anyhow!("lib.exe failed: {}", bytes_to_string(&lib_out.stderr)));
        }
        eprintln!("nova: gc.lib built (bdwgc extra/gc.c amalgamation)");
        let _ = std::fs::remove_dir_all(&obj_dir);
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (gc_amalgam, gc_include, ao_include, cache_dir, vcvars, &obj_dir);
        Err(anyhow!("unsupported platform for bdwgc fallback build"))
    }
}

// `pub(crate)` (Ф.1 #268 link-prep extraction, 2026-08-02): shared by both
// `build_libuv_lib` (below, unchanged) and `link_prep::build_missing_vendor_ffi_libs`.
pub(crate) fn collect_c_files(dir: &Path, out: &mut Vec<PathBuf>, recursive: bool) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| anyhow!("read_dir {}: {}", dir.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| anyhow!("read_dir entry: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            if recursive { collect_c_files(&path, out, true)?; }
        } else if path.extension().and_then(|s| s.to_str()) == Some("c") {
            out.push(path);
        }
    }
    Ok(())
}

// ---------- Plan 218: prebuilt runtime archive (libnova_rt) ----------
//
// C-compiling a fresh nova program pays ~6.5s recompiling the ~10-12
// nova_rt/*.c translation units on EVERY build (parsing windows.h/uv.h/gc.h
// dominates — docs/plans/218-prebuilt-runtime-archive.md). Mirrors
// `detect_or_build_libuv` above: compile the runtime .c files ONCE into a
// static archive, cache it keyed by content+flags, link the archive on
// subsequent builds instead of recompiling. Purely a build-latency
// optimization — any failure here (disabled, build error, missing tool)
// falls back to the pre-218 per-build inline-compile path (`build_command`'s
// existing behavior), never turns a build that used to succeed into a
// failure.
//
// **ABI hazard found while implementing this (not flagged by the original
// latency research):** `effects.c` DEFINES the actual TLS storage for
// `NovaEffectRegistry`, and `runtime.c` allocates/consumes
// `NovaEffectSnapshot` by `sizeof`. Both types are sized by the
// **per-program** `-DNOVA_MAX_EFFECT_STORAGES=N` marker (Plan 174.4 — see
// effects.h's doc-comment above `NOVA_MAX_EFFECT_STORAGES`): `N` = distinct
// effects USED BY THIS PROGRAM, read from a comment on line 1 of the
// generated `.c` by `effect_count_define_arg`. That define is deliberately
// applied to the WHOLE cc invocation (not `#define`d inside one `.c`)
// specifically so `NovaEffectRegistry`/`NovaEffectSnapshot` have an
// IDENTICAL layout in every TU compiled together. Precompiling
// `effects.c`/`runtime.c` into one FIXED-N archive and linking it against a
// DIFFERENT-N app.c would silently violate that invariant —
// `NovaEffectRegistry.count`'s byte offset shifts with N, and
// `NovaEffectSnapshot`-sized heap allocations could be undersized — a
// classic silent memory-corruption bug, not a link error. Same reasoning
// applies to `[runtime]` `fiber_stack`/`max_fibers` overrides
// (`runtime_define_args`) baked into `fiber_arena.c`/`fiber_arena_win.c` —
// not memory-unsafe, but a behavior difference (wrong default stack/fiber
// count), which would violate the byte-identical-behavior gate.
//
// Fix: `rt_archive_key` below folds `N` and the runtime-defines into the
// bucket key — a program only ever links against an archive built with ITS
// OWN effect count / runtime overrides. This still captures the dominant
// dev-loop win (editing and rebuilding the SAME program repeatedly never
// changes its effect count) and, in practice, most programs share `N`
// (built-ins Fail+Time only, no custom effects) so cross-program reuse
// still happens routinely.

/// Resolved lookup/build result for one archive bucket.
#[derive(Clone)]
pub struct RtArchiveConfig {
    pub lib_file: PathBuf,
}

/// `NOVA_RT_ARCHIVE=0`/`off`/`false` disables — falls back to the pre-218
/// per-build inline compile of every `nova_rt/*.c` (escape hatch, symmetric
/// to `NOVA_CACHE=0` in `build_cache.rs`). Default: enabled.
fn rt_archive_enabled() -> bool {
    !matches!(std::env::var("NOVA_RT_ARCHIVE").as_deref(),
              Ok("0") | Ok("off") | Ok("false"))
}

/// The `nova_rt/*.c` translation units folded into the archive — exactly
/// the set `build_command` otherwise adds as individual source args (mirrors
/// the `rt_alloc..rt_segv_diag` + conditional `rt_net`/`rt_fs`/eventloop.c
/// list there 1:1 — keep in sync on future nova_rt additions). The
/// GC-backend alloc file varies with `gc_kind` (part of the bucket key).
fn rt_archive_sources(rt_dir: &Path, gc_kind: GcKind, libuv: Option<&LibuvConfig>) -> Vec<PathBuf> {
    let mut v = vec![
        rt_dir.join(gc_kind.alloc_c_name()),
        rt_dir.join("effects.c"),
        rt_dir.join("fibers.c"),
        rt_dir.join("fiber_arena.c"),
        rt_dir.join("fiber_arena_win.c"),
        rt_dir.join("fiber_stats.c"),
        rt_dir.join("runtime.c"),
        rt_dir.join("driver.c"),
        rt_dir.join("typeid.c"),
        rt_dir.join("segv_diag.c"),
    ];
    if let Some(uv) = libuv {
        v.push(rt_dir.join("net.c"));
        v.push(rt_dir.join("fs.c"));
        // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
        v.push(rt_dir.join("process.c"));
        v.push(uv.eventloop_src.clone());
    }
    v
}

/// Every `nova_rt/*.{c,h}` file that participates in content-hashing for
/// invalidation (Plan 218 requirement: hash CONTENT, never mtime — a
/// `touch` without real changes must NOT bust the cache). Broader than
/// `rt_archive_sources` on purpose: every archived `.c` `#include`s a chain
/// of `nova_rt/*.h` — a header-only edit (e.g. `fibers.h`) must invalidate
/// the archive even though no `.c` itself changed. Only files directly
/// inside `rt_dir` — does NOT recurse into `rt_dir/libuv` (that submodule
/// has its own independent cache/lifecycle, see `detect_or_build_libuv`).
fn rt_hashable_files(rt_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(rt_dir) else { return out; };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            continue;
        }
        match p.extension().and_then(|s| s.to_str()) {
            Some("c") | Some("h") => out.push(p),
            _ => {}
        }
    }
    out.sort();
    out
}

/// Fingerprint (size + mtime-nanos) of the archive-builder compiler binary
/// — mirrors `build_cache.rs`'s compiler-exe fingerprint pattern. A
/// toolchain upgrade (new clang/cl.exe) must bust the archive.
fn rt_archive_compiler_fingerprint(cc_path: &Path) -> Option<(u64, u128)> {
    let meta = std::fs::metadata(cc_path).ok()?;
    let nanos = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((meta.len(), nanos))
}

/// Content-addressed key for one archive bucket. Every dimension that can
/// change the actual bytes/behavior of the compiled objects is folded in —
/// see the module doc above for the effect-count/runtime-define hazard
/// that isn't obvious from `nova_rt/*.c` alone. Separate buckets (Plan 218
/// requirement): dev vs release, GC boehm|malloc, platform (via
/// `std::env::consts::OS`), libuv on/off, per-program effect count, per-
/// package `[runtime]` overrides, release march. Returns `None` only if a
/// hashable file vanished mid-read (races with a concurrent edit) — caller
/// treats that exactly like "disabled" (falls back, non-fatal).
fn rt_archive_key(
    rt_dir: &Path,
    mode: Mode,
    gc_kind: GcKind,
    libuv_present: bool,
    effect_define: Option<&str>,
    runtime_defines: &[String],
    cc_fingerprint: Option<(u64, u128)>,
) -> Option<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    "nova-rt-archive-v1".hash(&mut h);
    format!("{:?}", mode).hash(&mut h);
    format!("{:?}", gc_kind).hash(&mut h);
    std::env::consts::OS.hash(&mut h);
    libuv_present.hash(&mut h);
    effect_define.hash(&mut h);
    runtime_defines.hash(&mut h);
    if matches!(mode, Mode::Release) {
        march_flag().hash(&mut h);
    }
    cc_fingerprint.hash(&mut h);

    let files = rt_hashable_files(rt_dir);
    files.len().hash(&mut h);
    for f in &files {
        f.to_string_lossy().hash(&mut h);
        let bytes = std::fs::read(f).ok()?;
        bytes.hash(&mut h);
    }
    Some(format!("{:016x}", h.finish()))
}

/// Process-wide memoization: `nova test`/`nova bench` build MANY files in
/// one process, each calling `detect_or_build_rt_archive` — without this,
/// every single build would re-read+re-hash ~1-1.5MB of `nova_rt/*.{c,h}`
/// content (cheap once, wasteful hundreds of times over one test run).
/// Keyed by the CHEAP dimensions only (no content hash) — nova_rt content
/// on disk cannot change mid-process for a single `nova test`/`nova build`
/// invocation (same non-goal as `build_cache.rs`, which makes the same
/// assumption for its own per-process source reads).
static RT_ARCHIVE_MEMO: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, Option<RtArchiveConfig>>>,
> = std::sync::OnceLock::new();

fn rt_archive_memo_key(
    rt_dir: &Path,
    mode: Mode,
    gc_kind: GcKind,
    libuv_present: bool,
    effect_define: Option<&str>,
    runtime_defines: &[String],
) -> String {
    format!(
        "{}|{:?}|{:?}|{}|{:?}|{:?}",
        rt_dir.display(), mode, gc_kind, libuv_present, effect_define, runtime_defines
    )
}

/// Resolve the fixed archive-builder compiler path for fingerprinting
/// (`rt_archive_compiler_fingerprint`). On Windows: search the vcvars-
/// captured `PATH` (from `tc`'s env snapshot — the current PROCESS's own
/// `PATH` does NOT have `cl.exe` unless vcvars was called for it) for
/// `cl.exe`; falls back to the literal `cl.exe` (unresolvable → fingerprint
/// becomes `None`, a safe degrade — see doc above). On Unix: `$CC` or `cc`.
fn resolve_archive_cc_path(tc: &Toolchain) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let env: &[(OsString, OsString)] = match tc {
            Toolchain::Clang { env, .. } => env,
            Toolchain::Msvc { env, .. } => env,
            Toolchain::Gcc { .. } => &[],
        };
        if let Some((_, path_val)) = env.iter().find(|(k, _)| {
            k.to_string_lossy().eq_ignore_ascii_case("PATH")
        }) {
            for dir in std::env::split_paths(path_val) {
                let candidate = dir.join("cl.exe");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
        PathBuf::from("cl.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = tc;
        std::env::var("CC").ok().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cc"))
    }
}

/// Build (or reuse) the runtime archive for this exact bucket. `tc` is used
/// only for `vcvars_path()` on Windows — the archive itself is compiled
/// with a FIXED compiler (`cl.exe` via vcvars on Windows, `$CC`/`cc` on
/// Unix), independent of the app's chosen `--toolchain`. This exactly
/// mirrors the existing `detect_or_build_libuv`/`build_libuv_lib`
/// precedent: `libuv.lib` is likewise built once with cl.exe/cc and linked
/// into both clang- and msvc-toolchain app builds — COFF/ELF objects from
/// either front-end interlink fine on their platform. Consequence (and
/// deliberate, precedented trade-off): archived objects are NOT literally
/// byte-identical machine code to what `--toolchain=clang` would emit
/// inline (different compiler/no cross-TU `-flto` into the archive — see
/// `build_rt_archive_lib` doc) — they ARE behaviorally identical, which is
/// the actual gate (same C source, same effective defines, deterministic
/// semantics; `libuv.lib` has coexisted with `-flto` app builds under this
/// exact scheme since Plan 22).
///
/// Returns `None` on ANY failure — pure optimization, never a hard
/// requirement; callers fall back to the pre-218 inline-compile path.
pub fn detect_or_build_rt_archive(
    rt_dir: &Path,
    repo_root: &Path,
    tc: &Toolchain,
    opts: &BuildOpts,
) -> Option<RtArchiveConfig> {
    if !rt_archive_enabled() {
        return None;
    }
    let d_prefix = if cfg!(target_os = "windows") { "/D" } else { "-D" };
    let effect_define = effect_count_define_arg(opts.c_file, d_prefix);
    let runtime_defines = runtime_define_args(opts.runtime, d_prefix);
    let libuv_present = opts.libuv.is_some();

    let memo_key = rt_archive_memo_key(
        rt_dir, opts.mode, opts.gc_kind, libuv_present,
        effect_define.as_deref(), &runtime_defines,
    );
    let memo = RT_ARCHIVE_MEMO.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    // [M-218-rt-archive-parallel-jobs-race] fix: hold ONE guard across the
    // whole check -> build -> memoize sequence below (previously the lock
    // was released right after the lookup and re-acquired only to insert
    // the result — leaving a window between them). `nova test --jobs N`
    // runs its worker pool as N THREADS inside ONE process
    // (`std::thread::scope` in this file's parallel test runner, not
    // separate OS processes), all calling this fn concurrently for
    // (usually) the SAME bucket. With the lock released in that window,
    // every thread could observe a memo-miss AND an absent on-disk
    // `lib_file` at once, then race to build/overwrite the SAME
    // `target/rt-archive-cache/<key>/` bucket concurrently — shared obj
    // dir, shared `.rsp` files, shared output archive clobbered by
    // multiple `cl.exe`/`lib.exe` (or `cc`/`ar`) invocations at once, not
    // merely wasted work but actual corruption (observed as a flaky FAIL
    // under `--jobs`, PASS in isolation — the archive some later reader
    // linked against was mid-write). Holding the lock for the full
    // sequence makes the first thread to reach a bucket do the real
    // build while every other thread blocks; they then either hit the
    // now-populated memo (same bucket — the common case per the module
    // doc: most programs share one bucket) or build their own DIFFERENT
    // bucket serially right after (rare). `build_rt_archive_lib` below is
    // ALSO hardened with its own unique-scratch-dir + atomic-rename
    // publish (mirrors `build_cache.rs::store_c`'s temp+rename idiom) as
    // defense-in-depth for builders in SEPARATE OS processes, which this
    // in-process mutex can't serialize (e.g. a concurrent `nova build` in
    // another terminal against the same repo `target/`).
    let mut guard = match memo.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(cached) = guard.get(&memo_key) {
        return cached.clone();
    }

    let result = (|| -> Option<RtArchiveConfig> {
        let cc_path = resolve_archive_cc_path(tc);
        let cc_fingerprint = rt_archive_compiler_fingerprint(&cc_path);

        let key = rt_archive_key(
            rt_dir, opts.mode, opts.gc_kind, libuv_present,
            effect_define.as_deref(), &runtime_defines, cc_fingerprint,
        )?;

        let cache_dir = repo_root.join("target").join("rt-archive-cache").join(&key);
        let lib_name = if cfg!(target_os = "windows") { "libnova_rt.lib" } else { "libnova_rt.a" };
        let lib_file = cache_dir.join(lib_name);
        if lib_file.is_file() {
            return Some(RtArchiveConfig { lib_file });
        }

        let sources = rt_archive_sources(rt_dir, opts.gc_kind, opts.libuv);
        // #269 Ф.2: same fallback-aware resolution as `build_command`'s own
        // `boehm_cfg` (see that call site's comment) — this builder has its
        // OWN independent GC include/lib derivation (Plan 218 prebuilt
        // `libnova_rt` archive), so it needs the SAME fix, not just the
        // early honest-exit check in `resolve_gc_or_exit`. Found by running
        // the actual clean-clone acceptance gate: without this, the rt_*.c
        // sources (which `#include <gc.h>` unconditionally under
        // `GcKind::Boehm`) failed with "gc.h file not found" even though
        // the fallback `gc.lib` had already been built successfully one
        // step earlier — `boehm_cfg` here was silently `None`.
        let boehm_cfg = if opts.gc_kind == GcKind::Boehm {
            detect_boehm(opts.cg_include).or_else(|| detect_or_build_boehm_fallback(rt_dir, repo_root, tc.vcvars_path()))
        } else {
            None
        };
        eprintln!("nova: libnova_rt archive not built for this config, building (one-time, ~5-7 sec)...");
        let build_result = build_rt_archive_lib(
            &sources, &cache_dir, &lib_file, rt_dir, opts.cg_include, opts.mode,
            opts.gc_kind, boehm_cfg.as_ref(), opts.libuv,
            effect_define.as_deref(), &runtime_defines, tc.vcvars_path(),
        );
        match build_result {
            Ok(()) if lib_file.is_file() => {
                eprintln!("nova: libnova_rt archive built ({})", lib_file.display());
                Some(RtArchiveConfig { lib_file })
            }
            Ok(()) => {
                eprintln!(
                    "nova: warning: libnova_rt archive build reported success but {} \
                     missing — falling back to inline compile",
                    lib_file.display()
                );
                None
            }
            Err(e) => {
                eprintln!(
                    "nova: warning: libnova_rt archive build failed ({}) — \
                     falling back to per-build inline compile",
                    e
                );
                None
            }
        }
    })();

    guard.insert(memo_key, result.clone());
    result
}

/// Unique-enough per-attempt tag for `build_rt_archive_lib`'s scratch
/// dir/files (see that fn's doc for why). PID gives cross-process
/// uniqueness (two separate `nova` OS processes racing on the same repo
/// `target/`); the atomic counter gives intra-process uniqueness across
/// threads in case this is ever called without the caller's memo-mutex
/// serialization; the nanosecond timestamp is a cheap extra safety
/// margin. [M-218-rt-archive-parallel-jobs-race].
fn unique_build_tag() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", std::process::id(), n, nanos)
}

/// Compile `sources` → objects and archive them into `lib_file`. Mirrors
/// `build_libuv_lib`'s Windows(cl.exe+lib.exe)/Unix(cc+ar) structure exactly.
/// `mode` controls optimization level only — deliberately NO `-flto`/`/GL`
/// here even in Release (matches `build_libuv_lib`, which never LTOs
/// `libuv.lib` either): mixing an LTO-bitcode static archive built by one
/// compiler with a final link potentially done by a DIFFERENT compiler
/// (clang vs gcc on Unix, since the archive's compiler is fixed but the
/// app's `--toolchain` is not) risks incompatible bitcode formats. app.c
/// itself still gets full `-flto` in `build_command` — only the archived
/// runtime's cross-TU inlining into app.c is traded away, a bounded,
/// precedented cost (`libuv.lib` already forgoes it entirely).
///
/// **[M-218-rt-archive-parallel-jobs-race]:** everything this fn writes
/// (obj files, `.rsp` files, the linked archive itself) goes into a
/// scratch dir UNIQUE to this one build ATTEMPT
/// (`cache_dir/.build-<pid>-<counter>-<nanos>`, never a fixed shared
/// `cache_dir/obj` — that was the actual corruption vector: two builders
/// writing the same obj/.rsp/archive paths at once). The real `lib_file`
/// path is only ever touched once, at the very end, via an atomic
/// rename — a reader either sees no file or a fully-written one, never a
/// partial one. The caller (`detect_or_build_rt_archive`) already
/// serializes same-process callers with a widened mutex; this is
/// defense-in-depth for builders in separate OS processes, which that
/// mutex cannot see.
#[allow(clippy::too_many_arguments)]
fn build_rt_archive_lib(
    sources: &[PathBuf],
    cache_dir: &Path,
    lib_file: &Path,
    rt_dir: &Path,
    cg_include: &Path,
    mode: Mode,
    gc_kind: GcKind,
    boehm_cfg: Option<&BoehmConfig>,
    libuv: Option<&LibuvConfig>,
    effect_define: Option<&str>,
    runtime_defines: &[String],
    vcvars: Option<&Path>,
) -> Result<()> {
    std::fs::create_dir_all(cache_dir).map_err(|e| anyhow!("create cache_dir: {}", e))?;
    let obj_dir = cache_dir.join(format!(".build-{}", unique_build_tag()));
    std::fs::create_dir_all(&obj_dir).map_err(|e| anyhow!("create obj_dir: {}", e))?;
    for src in sources {
        if !src.is_file() {
            let _ = std::fs::remove_dir_all(&obj_dir);
            return Err(anyhow!("rt archive source not found: {}", src.display()));
        }
    }
    // Linked INSIDE the isolated scratch dir first — published to the
    // real `lib_file` path only via the atomic rename at the bottom.
    let tmp_lib_file = obj_dir.join(
        lib_file.file_name().unwrap_or_else(|| std::ffi::OsStr::new("libnova_rt.tmp"))
    );

    let build: Result<()> = (|| -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let vcv = vcvars.ok_or_else(|| anyhow!("vcvars required for libnova_rt archive build on Windows"))?;
        let mode_flags = match mode {
            Mode::Dev => "/Od /Z7",
            Mode::Release => "/O2 /DNDEBUG",
        };
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("/c /nologo /W0 {} /Gy", mode_flags));
        lines.push(format!("/FI \"{}\"", rt_dir.join("nova_msvc_compat.h").display()));
        if gc_kind == GcKind::Boehm {
            lines.push("/DNOVA_GC_BOEHM /DGC_THREADS".to_string());
            if let Some(cfg) = boehm_cfg {
                if let Some(inc) = &cfg.include_dir {
                    lines.push(format!("/I \"{}\"", inc.display()));
                }
            }
        }
        for da in runtime_defines {
            lines.push(da.clone());
        }
        if let Some(ea) = effect_define {
            lines.push(ea.to_string());
        }
        if let Some(uv) = libuv {
            lines.push("/DNOVA_USE_LIBUV=1".to_string());
            lines.push(format!("/I \"{}\"", uv.include_dir.display()));
        }
        lines.push(format!("/I \"{}\"", cg_include.display()));
        lines.push(format!("/Fo\"{}\\\\\"", obj_dir.display()));
        for s in sources {
            lines.push(format!("\"{}\"", s.display()));
        }
        let rsp = obj_dir.join("compile.rsp");
        // №287: BOM against ANSI-codepage mangling of non-ASCII paths.
        std::fs::write(&rsp, format!("\u{FEFF}{}", lines.join("\n")))
            .map_err(|e| anyhow!("write rsp: {}", e))?;
        let inner = format!(
            "\"call \"{}\" >nul 2>&1 && cl.exe @\"{}\"\"",
            vcv.display(), rsp.display()
        );
        let mut cmd = Command::new("cmd");
        cmd.raw_arg("/c").raw_arg(&inner);
        let out = cmd.output().map_err(|e| anyhow!("spawn cl.exe: {}", e))?;
        if !out.status.success() {
            let combined = format!("{}{}", bytes_to_string(&out.stdout), bytes_to_string(&out.stderr));
            return Err(anyhow!("libnova_rt compile failed: {}",
                combined.lines().take(20).collect::<Vec<_>>().join("\n")));
        }
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for entry in std::fs::read_dir(&obj_dir).map_err(|e| anyhow!("read obj_dir: {}", e))? {
            let p = entry.map_err(|e| anyhow!("read_dir entry: {}", e))?.path();
            if p.extension().and_then(|s| s.to_str()) == Some("obj") {
                obj_files.push(p);
            }
        }
        if obj_files.len() != sources.len() {
            return Err(anyhow!(
                "libnova_rt compile: expected {} .obj files, found {} in {}",
                sources.len(), obj_files.len(), obj_dir.display()
            ));
        }
        let lib_rsp = obj_dir.join("lib.rsp");
        let mut lib_lines: Vec<String> =
            vec!["/nologo".to_string(), format!("/OUT:\"{}\"", tmp_lib_file.display())];
        for o in &obj_files {
            lib_lines.push(format!("\"{}\"", o.display()));
        }
        // №287: BOM against ANSI-codepage mangling of non-ASCII paths.
        std::fs::write(&lib_rsp, format!("\u{FEFF}{}", lib_lines.join("\n")))
            .map_err(|e| anyhow!("write lib.rsp: {}", e))?;
        let lib_inner = format!(
            "\"call \"{}\" >nul 2>&1 && lib.exe @\"{}\"\"",
            vcv.display(), lib_rsp.display()
        );
        let mut lib_cmd = Command::new("cmd");
        lib_cmd.raw_arg("/c").raw_arg(&lib_inner);
        let lib_out = lib_cmd.output().map_err(|e| anyhow!("spawn lib.exe: {}", e))?;
        if !lib_out.status.success() {
            return Err(anyhow!("lib.exe failed: {}", bytes_to_string(&lib_out.stderr)));
        }
        eprintln!("nova: libnova_rt.lib built ({} files)", sources.len());
        return Ok(());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for src in sources {
            let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("rt");
            let obj = obj_dir.join(format!("{}.o", stem));
            let mut c = Command::new(&cc);
            match mode {
                Mode::Dev => { c.args(["-O0", "-g", "-w"]); }
                Mode::Release => {
                    c.arg("-O3");
                    c.arg(format!("-march={}", march_flag()));
                    c.arg("-DNDEBUG");
                    c.arg("-w");
                }
            }
            c.arg("-c");
            c.arg("-fPIC");
            // [M-linux-mn-conformance-red] fix (2026-07-20): without per-
            // function/data sections, `-Wl,--gc-sections` at the final link
            // (main `build_command`'s Linux/macOS branch) can only discard
            // an ENTIRE archive-member .o as a unit, never an individual
            // dead function within one — so once effects.o/runtime.o/etc.
            // get pulled in for symbols they DO provide, dead code inside
            // them (e.g. `nova_bench_heap_sampler_thread` in bench.h,
            // included unconditionally, whose `NOVA_BENCH_STATE_DEFINE`
            // globals are only DEFINED in bench_mode builds) drags in
            // unresolved externs even for a plain `nova test`/`nova build`.
            // Confirmed empirically on WSL2/Linux: `undefined reference to
            // _nova_bench_heap_sample_interval_ns`/`_nova_bench_heap_sampler_stop`
            // when linking against the rt-archive, absent in the pre-218
            // per-build inline-compile path (which already passes these
            // flags — see `build_command`'s Clang/Gcc Unix branches). The
            // Windows half of this function already has the equivalent
            // (`/Gy`, function-level linking) — this brings the Unix half
            // to parity.
            c.arg("-ffunction-sections");
            c.arg("-fdata-sections");
            c.arg("-D_GNU_SOURCE");
            if gc_kind == GcKind::Boehm {
                c.arg("-DNOVA_GC_BOEHM");
                c.arg("-DGC_THREADS");
                if let Some(cfg) = boehm_cfg {
                    if let Some(inc) = &cfg.include_dir {
                        let s = inc.to_string_lossy();
                        if !s.starts_with("/usr/include") {
                            c.arg("-I").arg(inc);
                        }
                    }
                }
            }
            for da in runtime_defines {
                c.arg(da);
            }
            if let Some(ea) = effect_define {
                c.arg(ea);
            }
            if let Some(uv) = libuv {
                c.arg("-DNOVA_USE_LIBUV=1");
                c.arg("-I").arg(&uv.include_dir);
            }
            c.arg("-I").arg(cg_include);
            c.arg("-o").arg(&obj);
            c.arg(src);
            let out = c.output().map_err(|e| anyhow!("spawn {}: {}", cc, e))?;
            if !out.status.success() {
                return Err(anyhow!("libnova_rt compile failed on {}: {}",
                    src.display(), bytes_to_string(&out.stderr)));
            }
            obj_files.push(obj);
        }
        let mut ar = Command::new("ar");
        ar.arg("rcs").arg(&tmp_lib_file);
        for o in &obj_files {
            ar.arg(o);
        }
        let ar_out = ar.output().map_err(|e| anyhow!("spawn ar: {}", e))?;
        if !ar_out.status.success() {
            return Err(anyhow!("ar failed: {}", bytes_to_string(&ar_out.stderr)));
        }
        eprintln!("nova: libnova_rt.a built ({} files)", sources.len());
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (sources, cache_dir, lib_file, rt_dir, cg_include, mode, gc_kind,
                 boehm_cfg, libuv, effect_define, runtime_defines, vcvars, &tmp_lib_file);
        Err(anyhow!("unsupported platform for libnova_rt archive build"))
    }
    })();

    // [M-218-rt-archive-parallel-jobs-race]: publish atomically. By this
    // point the archive is either fully written at `tmp_lib_file` (inside
    // the isolated scratch dir) or `build` is `Err` and nothing has
    // touched the real `lib_file` path at all — a half-finished build
    // NEVER becomes visible at `lib_file`. Mirrors
    // `build_cache.rs::store_c`'s temp-file + `fs::rename` idiom (same
    // repo, same class of problem: don't let readers observe a partial
    // write).
    let published = build.and_then(|()| {
        if !tmp_lib_file.is_file() {
            return Err(anyhow!(
                "libnova_rt archive build reported success but {} missing",
                tmp_lib_file.display()
            ));
        }
        match std::fs::rename(&tmp_lib_file, lib_file) {
            Ok(()) => Ok(()),
            Err(e) => {
                // This process's callers are already serialized by
                // `detect_or_build_rt_archive`'s widened mutex, but a
                // genuinely separate `nova` OS process racing on the same
                // repo `target/` is NOT covered by that mutex. If it won
                // and already published a file at `lib_file`, that
                // archive is content-addressed by the very same bucket
                // key we just built from — byte-identical inputs, so
                // treat its presence as success instead of surfacing a
                // spurious failure (e.g. a Windows sharing violation from
                // renaming over a file the other process still has open).
                if lib_file.is_file() {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "publish {} -> {}: {}",
                        tmp_lib_file.display(), lib_file.display(), e
                    ))
                }
            }
        }
    });

    // Scratch dir is disposable regardless of outcome — best-effort
    // cleanup, never fails the build over it (matches this module's other
    // `let _ = std::fs::remove_dir_all(...)` best-effort precedent, e.g.
    // `build_libuv_lib` above).
    let _ = std::fs::remove_dir_all(&obj_dir);

    published
}

/// Сводный результат для `test-all`.
pub struct Summary {
    pub pass: usize,
    pub fail: usize,
    /// Plan 27 Ф.6: тесты пропущенные из-за AllocConstraint.
    /// Не входят в pass/fail — отдельная категория.
    pub skip: usize,
    pub results: Vec<(String, Status)>,
}

// ---------- Plan 27 Б.7: xorshift64 PRNG + Fisher-Yates shuffle ----------

/// xorshift64 — минимальный PRNG без extra deps.
/// Период 2^64-1. Достаточно для shuffling тест-листа.
struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        // Seed 0 запрещён в xorshift; используем любое non-zero fallback.
        Xorshift64(if seed == 0 { 0xDEAD_BEEF_CAFE_1337 } else { seed })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Случайное число в [0, n).
    fn next_usize(&mut self, n: usize) -> usize {
        if n <= 1 { return 0; }
        (self.next() % n as u64) as usize
    }
}

/// Fisher-Yates shuffle slice на месте.
fn shuffle<T>(slice: &mut [T], rng: &mut Xorshift64) {
    let n = slice.len();
    for i in (1..n).rev() {
        let j = rng.next_usize(i + 1);
        slice.swap(i, j);
    }
}

/// Seed из system time для --shuffle без аргумента.
fn random_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}

/// Plan 55 Ф.8: detect fixture directory — should be **excluded** from
/// test discovery. Fixtures = .nv files used as **input** для tooling
/// (e.g. `nova doc` ingestion samples in `nova_tests/doc/fixtures/`), не
/// настоящие tests (часто без `main`, без `test "..."` блоков).
///
/// Convention (production-grade, parity Rust `tests/data/`, Python `fixtures/`):
/// 1. Directory имя literally `"fixtures"` → skip recursively.
/// 2. Sentinel file `_fixture.toml` в каталоге → skip (explicit override).
///
/// Эти tests доступны через explicit `nova check <path>` или Plan 45
/// `nova doc` pipeline.
pub fn is_fixture_dir(dir: &Path) -> bool {
    // Convention: directory имя == "fixtures".
    if dir.file_name().and_then(|s| s.to_str()) == Some("fixtures") {
        return true;
    }
    // Explicit sentinel.
    if dir.join("_fixture.toml").is_file() {
        return true;
    }
    false
}

/// Plan 156: slow-lane selection mode for test discovery.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlowLane {
    /// Default `nova test`: skip `*_slow.nv` entirely (large/slow tests).
    Exclude,
    /// `--include-slow`: run normal tests AND `*_slow.nv`.
    Include,
    /// `--slow-only`: run ONLY `*_slow.nv`.
    Only,
}

/// Test type determined by EXPECT_* header marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestType {
    Positive,
    CompileError,
    Panic,
    Timeout,
    Exit,
}

/// Additive selection model: types = OR of enabled types, slow flag controls slow files.
/// Default = {Positive}, include_slow=false.
#[derive(Debug, Clone)]
pub struct TestSelection {
    pub types: std::collections::HashSet<TestType>,
    pub include_slow: bool,
}

impl Default for TestSelection {
    fn default() -> Self {
        let mut types = std::collections::HashSet::new();
        types.insert(TestType::Positive);
        TestSelection { types, include_slow: false }
    }
}

impl TestSelection {
    /// All types + slow (--full flag).
    pub fn full() -> Self {
        let mut types = std::collections::HashSet::new();
        types.insert(TestType::Positive);
        types.insert(TestType::CompileError);
        types.insert(TestType::Panic);
        types.insert(TestType::Timeout);
        types.insert(TestType::Exit);
        TestSelection { types, include_slow: true }
    }
}

/// [M-trap-tests-silent-skip-default-lane]: reason a file/folder-module entry
/// was excluded from `walk_nv_selected_ex`'s `out` (the file exists, was
/// discovered, but the active `TestSelection` doesn't run its lane). Before
/// this, `walk_nv_selected` just dropped these silently — `nova test
/// std/src/time/rt` (a dir holding only 3 legit `EXPECT_RUNTIME_PANIC` trap
/// tests) reported a bare "PASS: 0  FAIL: 0", indistinguishable from an
/// empty/typo'd directory. Every variant here becomes one visible SKIP row
/// (`SKIP <path> # <lane> lane — requires <hint>`) in the SAME `SKIP:` tally
/// as other `Outcome::Skipped` reasons (AllocBackend/SmtBackend/…) — a lane
/// exclusion is not a bug, but it must never look like zero tests exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneExclusion {
    /// File's `EXPECT_*` marker maps to a `TestType` not in `sel.types`
    /// (default run = `{Positive}` only — `EXPECT_RUNTIME_PANIC`/
    /// `EXPECT_COMPILE_ERROR`/`EXPECT_TIMEOUT`/`EXPECT_EXIT` all excluded).
    Type(TestType),
    /// `*_slow.nv` stem and `sel.include_slow == false` (D376). Distinct from
    /// `Type` because slow-ness is an orthogonal per-file suffix, not an
    /// `EXPECT_*` marker — a slow file can be any `TestType`.
    Slow,
    /// №453(а): confirmed folder-module (2+ peers, same `module X` decl —
    /// `is_folder_module_dir`) where NO peer contains a local `test "..."`
    /// block (`folder_module_has_tests` == false) — nothing to run
    /// standalone. Before this arm, the `is_folder_module` branch had no
    /// `else`: the directory was silently dropped, unlike the sibling
    /// checks for single files (`Type`, below at `:6507`-era) and `_slow`
    /// (`Slow`, above) which both honestly land in `excluded`. Measured
    /// fallout: 31 directories vanished with zero SKIP row (26
    /// `nova_tests.old`, 3 `spec_tests/conformance`, 2 `std/src` —
    /// `runtime/string`/`unicode`, by-design testless but still owed a
    /// visible SKIP per `test-conventions.md:670-680`). Distinct from
    /// `Type`/`Slow`: there is no `TestSelection` flag that unlocks this —
    /// the fix is adding a test to the module, not passing `--full` or
    /// `--include-slow`.
    NoLocalTests,
}

impl LaneExclusion {
    /// Human-facing lane name for the SKIP detail string.
    pub fn lane_name(self) -> &'static str {
        match self {
            LaneExclusion::Type(TestType::Positive) => "positive",
            LaneExclusion::Type(TestType::CompileError) => "compile-error",
            LaneExclusion::Type(TestType::Panic) => "runtime-panic",
            LaneExclusion::Type(TestType::Timeout) => "timeout",
            LaneExclusion::Type(TestType::Exit) => "exit",
            LaneExclusion::Slow => "slow",
            LaneExclusion::NoLocalTests => "no-tests",
        }
    }

    /// Flag that unlocks the lane — the fix for "this SKIP" the row names.
    /// `NoLocalTests` has no unlocking flag (nothing is selectable — there's
    /// no test to run); the text still slots into the same "requires <hint>"
    /// sentence as the flag-shaped hints.
    pub fn hint(self) -> &'static str {
        match self {
            LaneExclusion::Slow => "--include-slow/--slow-only",
            LaneExclusion::Type(_) => "--full",
            LaneExclusion::NoLocalTests => "a local `test \"...\"` block (nothing to run standalone)",
        }
    }
}

/// [M-d376-slow-suffix-folder-module-peer-merge]: process-wide "this `nova
/// test` invocation was asked to include `_slow` tests" flag, set ONCE by
/// `run_all` from `opts.selection.include_slow` before any entry is
/// compiled, read by `imports.rs`'s entry-sibling peer-merge.
///
/// Why this exists (in addition to inferring "is the entry itself `_slow`"
/// from its filename, which alone handles the SlowLane-based walker's
/// `SlowLane::Only` grouping — see `walk_nv_filtered_ex` — used only by
/// `nova check`/internal tests): `nova test`'s actual walker,
/// `walk_nv_selected`, groups a folder-module's peers (co-equal `.nv` files
/// declaring the same `module X`, e.g. nova-tls's `src/` root-peers) into
/// ONE compile-unit represented by a SINGLE alphabetically-first file —
/// `_slow`/non-`_slow` status of individual peers never changes WHICH file
/// is picked as that representative, only whether `_slow` peers are even
/// eligible to join the group's candidate pool. So for a folder-module, the
/// picked representative is (almost) never itself `_slow`, and a
/// per-entry-filename check alone would mean `--slow`/`--include-slow` can
/// never actually pull a folder-module's `_slow` peers into its CU. This
/// flag is the run-level signal that closes that gap: when the whole test
/// run opted into slow tests, ANY folder-module's peer-merge — not just one
/// whose representative happens to be `_slow` — includes its `_slow`
/// siblings too.
static TEST_RUN_INCLUDE_SLOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set once by `run_all` before test compilation starts. Thread-safe process
/// global — `nova test` compiles every selected entry within ONE process
/// (worker threads, not subprocesses), and `include_slow` is a single
/// run-wide decision from CLI flags, so a plain `AtomicBool` (no per-file
/// variance) is the correct, minimal mechanism — no need to thread a new
/// parameter through `codegen_to_c` and its many non-test callers (`nova
/// build`/`nova check`/IDE tooling), none of which have any notion of
/// "slow" at all.
pub fn set_test_run_include_slow(v: bool) {
    TEST_RUN_INCLUDE_SLOW.store(v, std::sync::atomic::Ordering::Relaxed);
}

/// Read by `imports.rs`'s entry-sibling peer-merge predicate.
pub fn test_run_include_slow() -> bool {
    TEST_RUN_INCLUDE_SLOW.load(std::sync::atomic::Ordering::Relaxed)
}

/// Plan 156: per-file slow-test suffix. A test file whose stem ends in
/// `_slow` (e.g. `collation_conformance_slow.nv`) is a large/slow test,
/// excluded from the default run, included only via --include-slow/--slow-only.
/// Peeled BEFORE `_test` and the OS-suffix (canonical `<core>[_<os>][_test][_slow]`)
/// so it composes with them. Zero per-file I/O: matched on the dirent name in
/// `walk_nv_filtered` — the file body is never read at default discovery.
pub fn is_slow_file_stem(stem: &str) -> bool { stem.ends_with("_slow") }

/// Peel the outermost `_slow` suffix (see [`is_slow_file_stem`] doc-comment
/// for the canonical suffix order). Shared by every `_slow`-aware peer/entry
/// scan in this crate ([M-d376-slow-suffix-folder-module-peer-merge]) —
/// `imports.rs`'s peer-merge predicate calls this instead of re-deriving its
/// own `strip_suffix("_slow")`.
pub fn strip_slow_suffix(stem: &str) -> &str {
    stem.strip_suffix("_slow").unwrap_or(stem)
}

/// Read the EXPECT_* marker from the first 30 lines of a .nv file.
/// Returns TestType based on the first matching marker found.
pub fn detect_test_type(path: &Path) -> TestType {
    use std::io::{BufRead, BufReader};
    let Ok(f) = std::fs::File::open(path) else { return TestType::Positive };
    let reader = BufReader::new(f);
    for line in reader.lines().take(30) {
        let Ok(line) = line else { break };
        if line.contains("EXPECT_COMPILE_ERROR") { return TestType::CompileError; }
        if line.contains("EXPECT_RUNTIME_PANIC") { return TestType::Panic; }
        // №453: `EXPECT_TIMEOUT_MS` — это ПЕР-ТЕСТОВЫЙ БЮДЖЕТ ВРЕМЕНИ
        // (`parse_timeout_ms`), а НЕ дорожка «ожидается зависание». Подстрочный
        // матч ниже уносил такие фикстуры в timeout-лейн, которого авторитетный
        // гейт (`--positive --compile-error`) не берёт, — и они молча выпадали из
        // проверки. Так выпало СЕМЬ фикстур, все про конкурентность и сеть, среди
        // них регресс на use-after-free, успевший перестать компилироваться
        // незамеченным. Гейт этого поймать не мог: уехавшая фикстура не даёт ни
        // PASS, ни FAIL, а число SKIP не ассертится (см. №453).
        if line.contains("EXPECT_TIMEOUT_MS")    { continue; }
        if line.contains("EXPECT_TIMEOUT")       { return TestType::Timeout; }
        if line.contains("EXPECT_EXIT")           { return TestType::Exit; }
        // №463: `EXPECT_LINT_WARNING` (CONV_RULES rule id) — как
        // `EXPECT_COMPILE_WARNING`, не заводит отдельную TestType-дорожку
        // (файл остаётся Positive: компилируется/запускается штатно, лишь
        // ДОПОЛНИТЕЛЬНО ассертит находку `nova lint`-реестра в run_one).
        // Explicit `continue`, а не молчаливый fallthrough — тот же приём,
        // что EXPECT_TIMEOUT_MS выше (№453): защищает от случайного
        // будущего substring-перехвата другой веткой этого цикла.
        if line.contains("EXPECT_LINT_WARNING")   { continue; }
    }
    TestType::Positive
}

/// Рекурсивный обход директории, возвращает все .nv файлы.
/// Plan 36: pub — используется в `nova check <dir>` flow.
/// Plan 55 Ф.8: skip fixture directories per `is_fixture_dir` convention.
pub fn walk_nv(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    // Explicit-path / `nova check <dir>` must see slow files too.
    walk_nv_filtered(root, out, SlowLane::Include)
}

/// Plan 156: slow-lane-aware variant of [`walk_nv`]. The default test run
/// (`nova test`) passes [`SlowLane::Exclude`] to skip `*_slow.nv` files
/// without ever reading their bodies; `--include-slow` / `--slow-only` route
/// through [`SlowLane::Include`] / [`SlowLane::Only`].
pub fn walk_nv_filtered(root: &Path, out: &mut Vec<PathBuf>, lane: SlowLane) -> Result<()> {
    walk_nv_filtered_ex(root, out, lane, false)
}

/// [M-check-folder-enumerator-skips-no-prelude] (2026-07-17): like [`walk_nv`],
/// but does NOT drop a folder-module purely because none of its peers contain a
/// local `test "..."` block. `walk_nv`'s "skip untested folder-modules" gate
/// (below) is correct for `nova test`'s TEST-DISCOVERY purpose — a folder-module
/// with no local test has nothing to run standalone. `nova check <dir>` wants
/// the opposite guarantee: verify every REAL module compiles, tested or not.
/// Repro before this fix: `nova check std/src/runtime/string` silently reported
/// "no .nv files to check" — the folder-module (`chars.nv`/`core.nv`/`parse.nv`/
/// `search.nv`/`slice.nv`/`transform.nv`, all `module runtime.string`) has zero
/// local `_test.nv` peers (its coverage lives in `spec_tests/conformance`), so
/// the untested-folder-module gate dropped it to a silent empty walk.
pub fn walk_nv_for_check(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    walk_nv_filtered_ex(root, out, SlowLane::Include, true)
}

/// Shared implementation behind [`walk_nv_filtered`] / [`walk_nv_for_check`].
/// `include_untested_folder_modules` — see [`walk_nv_for_check`] doc; `false`
/// preserves the original `walk_nv_filtered` test-discovery behavior exactly.
fn walk_nv_filtered_ex(
    root: &Path,
    out: &mut Vec<PathBuf>,
    lane: SlowLane,
    include_untested_folder_modules: bool,
) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    // Plan 55 Ф.8: skip fixtures directories entirely (no recursion).
    if is_fixture_dir(root) {
        return Ok(());
    }
    let entries = std::fs::read_dir(root)
        .map_err(|e| anyhow!("read_dir {}: {}", root.display(), e))?;
    // Plan 42 D29 rev-3: collect direct .nv files в этой папке.
    // Если они — peers of folder-module (все объявляют одинаковый
    // `module X`), они НЕ компилируются как standalone test entries
    // (нет main, peers depend друг от друга). Folder-module
    // компилируется только через import из внешнего entry.
    // Plan 42.12 Ф.1: target OS filter — `_windows.nv` / `_linux.nv` /
    // `_macos.nv` standalone tests skip'аются на других платформах.
    let target = crate::imports::current_target_os();
    let mut direct_nv: Vec<PathBuf> = Vec::new();
    let mut sub_dirs: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| anyhow!("read_dir entry: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            sub_dirs.push(path);
        } else if path.extension().and_then(|s| s.to_str()) == Some("nv") {
            // Plan 42.12 Ф.1: standalone test с OS-specific suffix
            // skip'ается на других платформах.
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                // Plan 42.10: `_module.nv` — module-config peer, никогда
                // не запускается как standalone test (нет items, только attrs).
                if stem == "_module" {
                    continue;
                }
                // Plan 156: peel _slow (outermost suffix) -> slow-lane routing.
                let is_slow = is_slow_file_stem(stem);
                match lane {
                    SlowLane::Exclude => { if is_slow { continue; } }
                    SlowLane::Only    => { if !is_slow { continue; } }
                    SlowLane::Include => {}
                }
                let stem_no_slow = strip_slow_suffix(stem);
                let core_stem = stem_no_slow.strip_suffix("_test").unwrap_or(stem_no_slow);
                if !crate::imports::peer_active_for_target_pub(core_stem, target) {
                    continue;
                }
            }
            direct_nv.push(path);
        }
    }
    let is_folder_module = direct_nv.len() >= 2 && is_folder_module_dir(&direct_nv);
    if is_folder_module {
        // Plan 169.1 Ф.8: folder-module с test-блоками → один compile unit.
        // Первый файл (по алфавиту) — entry; resolver подтянет остальных peers
        // через resolve_imports_inline_ex (include_test_peers=true).
        // Folder-module без test-блоков — библиотека, пропускаем как раньше
        // (test-discovery semantics) — ЕСЛИ вызывающий не запросил
        // `include_untested_folder_modules` (`nova check`'s walk_nv_for_check,
        // [M-check-folder-enumerator-skips-no-prelude]: check must still verify
        // a tested-less library folder-module compiles).
        if include_untested_folder_modules || folder_module_has_tests(&direct_nv) {
            let mut sorted = direct_nv;
            sorted.sort();
            out.push(sorted.into_iter().next().unwrap());
        }
    } else {
        // Каждый файл — standalone test entry.
        for p in direct_nv {
            out.push(p);
        }
    }
    // Sub-dirs recursive (могут быть other modules / sub-modules).
    // Plan 55 Ф.8: fixture sub-dirs skip'аются через is_fixture_dir check
    // в самом walk_nv (defensive: можно skip здесь чтобы избежать syscalls,
    // но centralized check внутри walk_nv — единственная точка истины).
    for sub in sub_dirs {
        walk_nv_filtered_ex(&sub, out, lane, include_untested_folder_modules)?;
    }
    Ok(())
}

/// Plan 169.1.1: Like `walk_nv_filtered` but uses `TestSelection` to filter by type
/// (EXPECT_* marker) AND slow-file suffix. Reads file header only for type detection.
///
/// Silent-drop wrapper kept for existing callers/tests that don't care WHY a
/// file was excluded — see [`walk_nv_selected_ex`]
/// ([M-trap-tests-silent-skip-default-lane]) for the variant `run_all` uses,
/// which also reports the reason so it can surface a visible SKIP row.
pub fn walk_nv_selected(root: &Path, out: &mut Vec<PathBuf>, sel: &TestSelection) -> Result<()> {
    let mut excluded = Vec::new();
    walk_nv_selected_ex(root, out, &mut excluded, sel)
}

/// [M-trap-tests-silent-skip-default-lane]: like [`walk_nv_selected`], but
/// additionally collects every file/folder-module entry that WAS discovered
/// yet excluded purely because its lane (`EXPECT_*` type, or `_slow` suffix)
/// isn't in `sel` — tagged with [`LaneExclusion`] so the caller can turn each
/// into a visible `SKIP <path> # <lane> lane — requires <hint>` row instead
/// of the file just vanishing (the bug this fixes: `nova test
/// std/src/time/rt` — a dir holding only `EXPECT_RUNTIME_PANIC` trap tests —
/// used to report a bare "PASS: 0  FAIL: 0", indistinguishable from an empty
/// directory). `excluded` order follows discovery order (not sorted); callers
/// that need determinism sort it themselves (`run_all` does, alongside its
/// `inputs`).
pub fn walk_nv_selected_ex(
    root: &Path,
    out: &mut Vec<PathBuf>,
    excluded: &mut Vec<(PathBuf, LaneExclusion)>,
    sel: &TestSelection,
) -> Result<()> {
    if root.is_file() {
        let stem = root.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let is_slow = is_slow_file_stem(stem);
        if is_slow && !sel.include_slow {
            excluded.push((root.to_path_buf(), LaneExclusion::Slow));
            return Ok(());
        }
        let test_type = detect_test_type(root);
        if sel.types.contains(&test_type) {
            out.push(root.to_path_buf());
        } else {
            excluded.push((root.to_path_buf(), LaneExclusion::Type(test_type)));
        }
        return Ok(());
    }
    if !root.is_dir() {
        return Ok(());
    }
    if is_fixture_dir(root) {
        return Ok(());
    }
    let entries = std::fs::read_dir(root)
        .map_err(|e| anyhow!("read_dir {}: {}", root.display(), e))?;
    let target = crate::imports::current_target_os();
    let mut direct_nv: Vec<PathBuf> = Vec::new();
    let mut sub_dirs: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| anyhow!("read_dir entry: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            sub_dirs.push(path);
        } else if path.extension().and_then(|s| s.to_str()) == Some("nv") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem == "_module" { continue; }
                let is_slow = is_slow_file_stem(stem);
                if is_slow && !sel.include_slow {
                    // D376 "zero per-file I/O" preserved: no read, just the
                    // dirent name we already have — same cost as the silent
                    // `continue` this replaces.
                    excluded.push((path.clone(), LaneExclusion::Slow));
                    continue;
                }
                let stem_no_slow = strip_slow_suffix(stem);
                let core_stem = stem_no_slow.strip_suffix("_test").unwrap_or(stem_no_slow);
                // OS-suffix mismatch (`_windows.nv` on non-Windows, etc.) is a
                // DIFFERENT, already-expected category (platform gating, not a
                // lane selection) — deliberately NOT reported as LaneExclusion
                // here: it would spam a SKIP row per foreign-OS peer file
                // across the whole std/spec_tests tree on every run.
                if !crate::imports::peer_active_for_target_pub(core_stem, target) { continue; }
            }
            direct_nv.push(path);
        }
    }
    let is_folder_module = direct_nv.len() >= 2 && is_folder_module_dir(&direct_nv);
    if is_folder_module {
        if folder_module_has_tests(&direct_nv) {
            let mut sorted = direct_nv.clone();
            sorted.sort();
            if let Some(entry) = sorted.into_iter().next() {
                // Type for folder-module is determined by first file (entry)
                let test_type = detect_test_type(&entry);
                if sel.types.contains(&test_type) {
                    out.push(entry);
                } else {
                    excluded.push((entry, LaneExclusion::Type(test_type)));
                }
            }
        } else {
            // №453(а): confirmed folder-module, but no peer has a local
            // `test "..."` block — previously fell through with no `else`
            // and vanished with zero SKIP row (see `LaneExclusion::NoLocalTests`
            // doc-comment for the measured 31-directory fallout). Report the
            // same alphabetically-first peer used as the "entry" in the
            // has-tests branch above, so the SKIP row names a real file.
            let mut sorted = direct_nv.clone();
            sorted.sort();
            if let Some(entry) = sorted.into_iter().next() {
                excluded.push((entry, LaneExclusion::NoLocalTests));
            }
        }
    } else {
        for p in direct_nv {
            let test_type = detect_test_type(&p);
            if sel.types.contains(&test_type) {
                out.push(p);
            } else {
                excluded.push((p, LaneExclusion::Type(test_type)));
            }
        }
    }
    for sub in sub_dirs {
        walk_nv_selected_ex(&sub, out, excluded, sel)?;
    }
    Ok(())
}

/// Plan 42 D29 rev-3: detect — все эти .nv files объявляют тот же
/// `module X` (folder-module peers)?
fn is_folder_module_dir(files: &[PathBuf]) -> bool {
    if files.len() < 2 {
        return false;
    }
    // Plan 42.17 Ф.3: единый сканер `crate::imports::scan_module_decl`.
    let mut decls: Vec<Vec<String>> = Vec::with_capacity(files.len());
    for f in files {
        let src = match std::fs::read_to_string(f) {
            Ok(s) => s,
            Err(_) => return false,
        };
        match crate::imports::scan_module_decl(&src) {
            Some(d) => decls.push(d),
            None => return false,
        }
    }
    let first = &decls[0];
    decls.iter().all(|d| d == first)
}

/// Plan 169.1 Ф.8: хотя бы один peer содержит `test "` блок?
/// Читаем тела файлов — вызывается только для подтверждённых folder-module
/// (is_folder_module_dir уже прошёл), поэтому I/O здесь оправдан.
fn folder_module_has_tests(files: &[PathBuf]) -> bool {
    files.iter().any(|f| {
        std::fs::read_to_string(f)
            .map(|s| s.contains("test \""))
            .unwrap_or(false)
    })
}

/// Сборка display-name для теста на основе path + base.
/// `nova_tests/basics/literals.nv` → `basics/literals`.
/// `std/checksums/fnv.nv` → `std/checksums/fnv`.
/// [36.D.1] Build display name relative to cwd (or the nearest parent that
/// is one of the input dirs). Falls back to the full path if strip fails.
fn display_name(path: &Path, cwd: &Path) -> String {
    let rel = path.strip_prefix(cwd).unwrap_or(path);
    rel.with_extension("").to_string_lossy().replace('\\', "/")
}

/// JSON-escape для строк. Минимальный — обрабатывает контрольные символы.
/// `serde_json` не подключаем (extra dependency не нужна для одной функции).
///
/// Plan 26 Ф.17 #12: вход `&str` гарантирует valid UTF-8 (Rust invariant),
/// поэтому surrogate halves невозможны — non-BMP chars (эмодзи) выходят
/// как raw UTF-8 bytes что валидно по JSON spec (RFC 8259 §7). Также
/// дополнительно escape'аем `<` `>` `&` для HTML-embed safety (некоторые
/// CI dashboards рендерят JSON прямо в HTML page).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            // U+2028 LINE SEPARATOR и U+2029 PARAGRAPH SEPARATOR —
            // валидны в JSON но ломают eval'd JavaScript (исторический
            // gotcha). Escape'аем как `\u20xx`. Cargo делает то же.
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out
}

/// Emit one line per test event в соответствии с `format`. Streaming —
/// output flush'ится сразу после каждой строки.
/// Б.3: при verbose — печатает захваченный stdout/stderr для Pass.
fn emit_event(format: OutputFormat, idx: usize, total: usize, name: &str, outcome: &Outcome, verbosity: Verbosity) {
    let mut out = std::io::stdout().lock();
    match format {
        OutputFormat::Text => {
            let label = outcome.label();
            let detail = outcome.detail();
            if detail.is_empty() {
                let _ = writeln!(out, "{:<14} {}", label, name);
            } else {
                let trunc: String = detail.chars().take(600).collect();
                let _ = writeln!(out, "{:<14} {}  # {}", label, name, trunc);
            }
            // Б.3: verbose — dump captured output after Pass line.
            if matches!(verbosity, Verbosity::Verbose) {
                if let Outcome::Pass { captured_stdout, captured_stderr, .. } = outcome {
                    if let Some(s) = captured_stdout {
                        if !s.is_empty() {
                            let _ = writeln!(out, "  stdout: {}", s.trim_end());
                        }
                    }
                    if let Some(s) = captured_stderr {
                        if !s.is_empty() {
                            let _ = writeln!(out, "  stderr: {}", s.trim_end());
                        }
                    }
                }
            }
        }
        OutputFormat::Json => {
            let status = match outcome {
                Outcome::Pass { .. }    => "pass",
                Outcome::Timeout { .. } => "timeout",
                Outcome::Skipped { .. } => "skip",
                Outcome::Fail { .. }    => "fail",
            };
            let stage = match outcome {
                Outcome::Pass { .. }    => "",
                Outcome::Timeout { .. } => "timeout",
                Outcome::Skipped { .. } => "skip",
                Outcome::Fail { stage, .. } => match stage {
                    Stage::Codegen { .. }    => "codegen",
                    Stage::Cc { .. }         => "cc",
                    Stage::Run { .. }        => "run",
                    Stage::NoCFile           => "no-c-file",
                    Stage::Expectation { .. }=> "expectation",
                },
            };
            let detail = outcome.detail();
            let _ = writeln!(
                out,
                "{{\"event\":\"finished\",\"test\":\"{}\",\"status\":\"{}\",\"stage\":\"{}\",\"elapsed_ms\":{},\"detail\":\"{}\"}}",
                json_escape(name),
                status,
                stage,
                outcome.elapsed().as_millis(),
                json_escape(&detail),
            );
        }
        OutputFormat::Tap => {
            // TAP-13: skip → "ok N - name # SKIP reason".
            let _ = match outcome {
                Outcome::Pass { .. } => writeln!(out, "ok {} - {}", idx + 1, name),
                Outcome::Skipped { .. } => {
                    let detail = outcome.detail();
                    writeln!(out, "ok {} - {} # SKIP {}", idx + 1, name, detail)
                }
                _ => {
                    let detail = outcome.detail();
                    if detail.is_empty() {
                        writeln!(out, "not ok {} - {}", idx + 1, name)
                    } else {
                        writeln!(out, "not ok {} - {} # {}", idx + 1, name, detail)
                    }
                }
            };
        }
        OutputFormat::Junit => {
            // JUnit XML — batch format. Per-test events не stream'им.
        }
    }
    let _ = out.flush();
    let _ = (idx, total);
}

/// Plan 26 Ф.14: XML-escape для атрибутов / содержимого JUnit XML.
/// Минимальный — &<>"' и control chars.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\r' && c != '\t' => {
                // XML 1.0 не допускает control chars кроме \n\r\t.
                out.push(' ');
            }
            c => out.push(c),
        }
    }
    out
}

/// Plan 26 Ф.10: загрузить ResultRecord'ы из JSON. Простой format
/// (один record на строку) — не нужен serde_json.
/// Plan 169.1 Ф.1: parse compile_ms/run_ms with backward-compat (missing → 0).
fn load_results(path: &Path) -> Vec<ResultRecord> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            continue;
        }
        // Парсим: {"name":"...","passed":true,"elapsed_ms":123,"compile_ms":80,"run_ms":43}
        // Минималистично через manual split — без regex/serde_json.
        // compile_ms/run_ms optional for backward compat (old files → 0).
        let name = extract_json_str(line, "\"name\":\"");
        let passed_str = extract_json_field(line, "\"passed\":");
        let elapsed_str = extract_json_field(line, "\"elapsed_ms\":");
        if let (Some(name), Some(passed), Some(elapsed)) = (name, passed_str, elapsed_str) {
            let passed = passed.trim() == "true";
            let elapsed_ms = elapsed.trim_end_matches('}').trim().parse::<u128>().unwrap_or(0);
            let compile_ms = extract_json_field(line, "\"compile_ms\":")
                .and_then(|s| s.trim_end_matches('}').trim().parse::<u128>().ok())
                .unwrap_or(0);
            let run_ms = extract_json_field(line, "\"run_ms\":")
                .and_then(|s| s.trim_end_matches('}').trim().parse::<u128>().ok())
                .unwrap_or(0);
            out.push(ResultRecord {
                name,
                passed,
                elapsed_ms,
                compile_ms,
                run_ms,
            });
        }
    }
    out
}

fn extract_json_str(line: &str, key: &str) -> Option<String> {
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn extract_json_field(line: &str, key: &str) -> Option<String> {
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    let end = rest.find(',').unwrap_or_else(|| rest.find('}').unwrap_or(rest.len()));
    Some(rest[..end].to_string())
}

fn save_results(path: &Path, records: &[ResultRecord]) -> std::io::Result<()> {
    let mut s = String::new();
    for r in records {
        // Plan 169.1 Ф.1: include compile_ms/run_ms split timing fields.
        s.push_str(&format!(
            "{{\"name\":\"{}\",\"passed\":{},\"elapsed_ms\":{},\"compile_ms\":{},\"run_ms\":{}}}\n",
            json_escape(&r.name),
            r.passed,
            r.elapsed_ms,
            r.compile_ms,
            r.run_ms,
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, s)
}

pub fn run_all(opts: TestAllOpts) -> Result<Summary> {
    // Plan 26 Ф.13: install Ctrl+C handler один раз.
    install_cancel_handler();

    // [M-d376-slow-suffix-folder-module-peer-merge]: latch the run-wide
    // slow-inclusion decision BEFORE any entry is walked/compiled — see
    // `set_test_run_include_slow`'s doc-comment for why this is needed in
    // addition to the entry-itself-is-`_slow` check.
    set_test_run_include_slow(opts.selection.include_slow);

    // Plan 27 Ф.D (audit 2026-05-12) + #269 Ф.2: early Boehm detection (or
    // vendored fallback build) с graceful exit если backend = Boehm и
    // gc.lib/libgc не найден. Без этого юзер получает cryptic linker error
    // для каждого теста.
    let _ = resolve_gc_or_exit(opts.gc_kind, opts.cg_include, opts.rt_dir, opts.toolchain.vcvars_path());

    // [36.D.1] Collect .nv files from all input_dirs (or fallback to tests_dir).
    let cwd = std::env::current_dir().unwrap_or_else(|_| opts.tests_dir.to_path_buf());
    let fallback_dir;
    let effective_dirs: &[PathBuf] = if opts.input_dirs.is_empty() {
        fallback_dir = [opts.tests_dir.to_path_buf()];
        &fallback_dir
    } else {
        opts.input_dirs
    };
    let mut inputs: Vec<PathBuf> = Vec::new();
    // [M-trap-tests-silent-skip-default-lane]: files discovered but excluded
    // purely by lane (EXPECT_* type not in selection, or *_slow without
    // --include-slow) — see `walk_nv_selected_ex` doc. Turned into visible
    // pre-computed SKIP jobs below instead of vanishing from `inputs`.
    let mut excluded_inputs: Vec<(PathBuf, LaneExclusion)> = Vec::new();
    for dir_or_file in effective_dirs {
        if dir_or_file.is_file() {
            inputs.push(dir_or_file.clone());
        } else {
            let mut found = Vec::new();
            walk_nv_selected_ex(dir_or_file, &mut found, &mut excluded_inputs, &opts.selection)?;
            inputs.extend(found);
        }
    }
    // Стабильный порядок по пути — shuffle потом переопределит если нужно.
    inputs.sort();
    excluded_inputs.sort_by(|a, b| a.0.cmp(&b.0));

    std::fs::create_dir_all(opts.tmp_dir)
        .map_err(|e| anyhow!("create tmp_dir: {}", e))?;

    // Plan 26 Ф.10: --rerun-failed pre-load list.
    let rerun_set: Option<std::collections::HashSet<String>> = if opts.rerun_failed {
        let path = opts.results_file
            .ok_or_else(|| anyhow!("--rerun-failed requires --results-file"))?;
        let prev = load_results(path);
        if prev.is_empty() {
            return Err(anyhow!(
                "--rerun-failed: results file {} empty or unreadable",
                path.display()
            ));
        }
        Some(prev.iter().filter(|r| !r.passed).map(|r| r.name.clone()).collect())
    } else {
        None
    };

    // Plan 27 Б.5: --filter-from exact-match set.
    let filter_from_set: Option<std::collections::HashSet<String>> = if let Some(p) = opts.filter_from {
        let text = std::fs::read_to_string(p)
            .map_err(|e| anyhow!("--filter-from: cannot read {}: {}", p.display(), e))?;
        Some(text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    } else {
        None
    };

    // Build job list applying all filters. Shared by both real jobs and the
    // synthesized lane-exclusion SKIP jobs below, so --filter/--skip/
    // --filter-from/--rerun-failed narrow the SKIP rows exactly like real
    // tests (a lane-excluded file outside the requested --filter shouldn't
    // show up either).
    let passes_filters = |nv_path: &Path, display: &str| -> bool {
        if let Some(filter) = opts.filter {
            if !display.contains(filter) { return false; }
        }
        // Plan 36.D: --skip применяется к display name И к raw path string
        // (для skip типа `std/runtime/` который может не попадать в display).
        if !opts.skip.is_empty() {
            let path_str = nv_path.to_string_lossy().replace('\\', "/");
            let skip_match = opts.skip.iter().any(|pat| {
                !pat.is_empty() && (display.contains(pat.as_str()) || path_str.contains(pat.as_str()))
            });
            if skip_match { return false; }
        }
        if let Some(set) = &filter_from_set {
            if !set.contains(display) { return false; }
        }
        if let Some(set) = &rerun_set {
            if !set.contains(display) { return false; }
        }
        true
    };
    // `Option<SkipReason>` = precomputed outcome for lane-excluded entries —
    // `None` (real job, goes through `run_one`) vs `Some(reason)` (skip
    // immediately, no codegen/cc/run attempted).
    let mut jobs: Vec<(String, PathBuf, Option<SkipReason>)> = Vec::new();
    for nv_path in &inputs {
        let display = display_name(nv_path, &cwd);
        if !passes_filters(nv_path, &display) { continue; }
        jobs.push((display, nv_path.clone(), None));
    }
    for (nv_path, reason) in &excluded_inputs {
        let display = display_name(nv_path, &cwd);
        if !passes_filters(nv_path, &display) { continue; }
        jobs.push((display, nv_path.clone(), Some(SkipReason::LaneExcluded {
            lane: reason.lane_name(),
            hint: reason.hint(),
        })));
    }

    // Plan 27 Б.7: shuffle если задан seed.
    if let Some(raw_seed) = opts.shuffle_seed {
        let seed = if raw_seed == 0 { random_seed() } else { raw_seed };
        eprintln!("nova: shuffling {} tests with seed {}", jobs.len(), seed);
        let mut rng = Xorshift64::new(seed);
        shuffle(&mut jobs, &mut rng);
    }

    let total = jobs.len();

    // Plan 27 Б.5: --list — print names без запуска.
    if opts.list_only {
        for (display, _, _) in &jobs {
            println!("{}", display);
        }
        return Ok(Summary { pass: 0, fail: 0, skip: 0, results: Vec::new() });
    }

    // TAP-13 header.
    if opts.format == OutputFormat::Tap {
        println!("TAP version 13");
        println!("1..{}", total);
        let _ = std::io::stdout().flush();
    }

    // Plan 26 Ф.3: параллельный прогон через std::thread::scope.
    let jobs_arc = std::sync::Arc::new(jobs);
    let next_idx = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // Plan 169.1 Ф.1: store split timing (compile_ms, run_ms) alongside each result.
    let results_mutex = std::sync::Arc::new(std::sync::Mutex::new(
        Vec::<(usize, String, Outcome, (u128, u128))>::with_capacity(total),
    ));

    let workers = std::cmp::max(1, opts.jobs).min(total.max(1));

    // Plan 83.1 Ф.5: thread-budget против NumCPU²-oversubscription.
    // `workers` тест-процессов идут параллельно; каждому даём бюджет
    // NOVA_MAXPROCS = max(1, NumCPU / workers), чтобы суммарное число
    // M:N worker-потоков было ≈ NumCPU, а не NumCPU². Тесты с явным
    // `runtime.init(n>0)` или `// ENV NOVA_MAXPROCS=...` переопределяют.
    let maxprocs_budget: Option<u32> =
        Some(std::cmp::max(1, default_jobs() / workers) as u32);

    std::thread::scope(|s| {
        for _ in 0..workers {
            let jobs = std::sync::Arc::clone(&jobs_arc);
            let next_idx = std::sync::Arc::clone(&next_idx);
            let results_mutex = std::sync::Arc::clone(&results_mutex);
            let format = opts.format;
            let verbosity = opts.verbosity;
            let toolchain = &opts.toolchain;
            let libuv_ref = opts.libuv.as_ref();
            let tmp_dir = opts.tmp_dir;
            let cg_include = opts.cg_include;
            let rt_dir = opts.rt_dir;
            let mode = opts.mode;
            let timeout = opts.timeout;
            let keep_artifacts = opts.keep_artifacts;
            let retries = opts.retries;
            let gc_kind = opts.gc_kind;
            let mono_depth = opts.mono_depth;
            let contracts_mode = opts.contracts_mode;
            let repo = opts.repo;
            let stdlib_dir = opts.stdlib_dir;

            // [M-codegen-conformance-stack-overflow]: large generated test files
            // (Unicode conformance fixtures — thousands of asserts in one block)
            // need a deep codegen stack. The default scoped-thread stack (~2 MB)
            // overflows where the 8 MB main thread (`nova-codegen test-build`) does
            // not — so give workers 64 MB of headroom. Root fix (not a band-aid):
            // codegen depth is fine on a normal stack, only the worker stack was
            // undersized.
            std::thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn_scoped(s, move || loop {
                if is_cancelled() { return; }
                let idx = next_idx.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if idx >= jobs.len() { return; }
                let (display, nv_path, preskip) = &jobs[idx];
                let test_opts = TestBuildOpts {
                    nv_file: nv_path,
                    toolchain,
                    mode,
                    cg_include,
                    rt_dir,
                    tmp_dir,
                    display,
                    keep_artifacts,
                    libuv: libuv_ref,
                    timeout,
                    gc_kind,
                    verbosity,
                    mono_depth,
                    maxprocs_budget,
                    contracts_mode,
                    repo,
                    stdlib_dir,
                };
                // Plan 26 Ф.12: retry для transient AV/linker race fails.
                // Exponential backoff: 100ms, 200ms, 400ms.
                // Plan 26 Ф.17 #1: cumulative elapsed.
                let retry_start = Instant::now();
                // Plan 169.1 Ф.1: split timing output from run_one.
                let mut split: (u128, u128) = (0, 0);
                // [M-trap-tests-silent-skip-default-lane]: lane-excluded jobs
                // carry a precomputed SkipReason — never touch codegen/cc/run,
                // just surface the reason as an immediate Skipped outcome.
                let mut outcome = if let Some(reason) = preskip {
                    Outcome::Skipped { reason: reason.clone(), elapsed: Duration::from_millis(0) }
                } else {
                    run_one(&test_opts, &mut split)
                };
                let mut retry_count = 0u32;
                for attempt in 1..=retries {
                    if !is_transient_fail(&outcome) { break; }
                    let backoff = Duration::from_millis(100 * (1 << (attempt - 1)));
                    std::thread::sleep(backoff);
                    outcome = run_one(&test_opts, &mut split);
                    if outcome.is_pass() {
                        retry_count = attempt;
                        // DX-сигнал: retry помог — есть AV-race.
                        if matches!(format, OutputFormat::Text) {
                            let mut sout = std::io::stdout().lock();
                            let _ = writeln!(sout, "  ↻ retry-{} passed: {}", attempt, display);
                            let _ = sout.flush();
                        }
                        break;
                    }
                }
                if retries > 0 {
                    outcome = outcome.with_elapsed(retry_start.elapsed());
                }
                // Plan 27 Б.6: записываем retry count в Pass outcome.
                if retry_count > 0 {
                    outcome = outcome.with_retries(retry_count);
                }

                // Streaming output: Quiet — только FAIL (не Skipped); Normal/Verbose — все.
                let should_emit = match verbosity {
                    Verbosity::Quiet => !outcome.is_pass() && !outcome.is_skipped(),
                    Verbosity::Normal | Verbosity::Verbose => true,
                };
                if should_emit {
                    emit_event(format, idx, jobs.len(), display, &outcome, verbosity);
                }
                let mut guard = match results_mutex.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                guard.push((idx, display.clone(), outcome, split));
            }).expect("failed to spawn test worker thread");
        }
    });

    // Reassemble в порядке job-index.
    let mutex_inner = match std::sync::Arc::try_unwrap(results_mutex) {
        Ok(m) => m,
        Err(arc) => {
            eprintln!(
                "warning: results-mutex Arc has {} extra strong refs after scope() — \
                 worker leak; returning partial results",
                std::sync::Arc::strong_count(&arc) - 1
            );
            return Ok(Summary { pass: 0, fail: 0, skip: 0, results: Vec::new() });
        }
    };
    let mut indexed = match mutex_inner.into_inner() {
        Ok(v) => v,
        Err(poison) => { eprintln!("warning: results mutex poisoned, recovering"); poison.into_inner() }
    };
    indexed.sort_by_key(|(idx, _, _, _)| *idx);
    // Plan 169.1 Ф.1: carry split timing through results for save_results.
    let results_with_split: Vec<(String, Outcome, (u128, u128))> = indexed
        .into_iter()
        .map(|(_, name, outcome, split)| (name, outcome, split))
        .collect();
    let results: Vec<(String, Outcome)> = results_with_split
        .iter()
        .map(|(name, outcome, _)| (name.clone(), outcome.clone()))
        .collect();

    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut skip = 0usize;
    for (_, s) in &results {
        if s.is_pass() { pass += 1; }
        else if s.is_skipped() { skip += 1; }
        else { fail += 1; }
    }

    // Plan 26 Ф.10: save results. Skip'ы не сохраняем (not pass/fail).
    if let Some(path) = opts.results_file {
        let records: Vec<ResultRecord> = results_with_split
            .iter()
            .filter(|(_, o, _)| !o.is_skipped())
            .map(|(name, outcome, split)| ResultRecord {
                name: name.clone(),
                passed: outcome.is_pass(),
                elapsed_ms: outcome.elapsed().as_millis(),
                compile_ms: split.0,
                run_ms: split.1,
            })
            .collect();
        if let Err(e) = save_results(path, &records) {
            eprintln!("warning: failed to save results file {}: {}", path.display(), e);
        }
    }

    // [M-169-timing-report-regression-gate]: if --max-test-ms N is set,
    // collect violators and exit(3) so CI catches accidental slow tests.
    if opts.max_test_ms > 0 {
        let mut violators: Vec<(String, u128)> = results_with_split
            .iter()
            .filter(|(_, o, _)| !o.is_skipped())
            .filter(|(_, o, _)| o.elapsed().as_millis() > opts.max_test_ms)
            .map(|(name, o, _)| (name.clone(), o.elapsed().as_millis()))
            .collect();
        if !violators.is_empty() {
            violators.sort_by(|a, b| b.1.cmp(&a.1));
            eprintln!(
                "\nerror: {} test(s) exceeded --max-test-ms {} threshold:",
                violators.len(),
                opts.max_test_ms
            );
            for (name, ms) in &violators {
                eprintln!("  {:>8}ms  {}", ms, name);
            }
            std::process::exit(3);
        }
    }

    // Plan 172.1 U.7.1: CC-FAIL audit report (un-expected type-class CC-FAIL
    // leaks on the corpus + classification of existing EXPECT_CC_ERROR fixtures).
    // Tooling-only — runs after results are assembled, changes no compilation.
    if opts.report_cc_leaks {
        print_cc_leak_report(&results, effective_dirs);
    }

    Ok(Summary { pass, fail, skip, results })
}

/// Вывод финального summary. Per-test events уже отстримлены в run_all.
///
/// Plan 26 Ф.4: формат влияет — Text печатает таблицу, JSON финальный
/// summary-event, TAP — `# pass/fail` комментарий.
/// Plan 26 Ф.8: всё в stdout (cargo/go test convention).
pub fn print_summary(summary: &Summary, format: OutputFormat) {
    let mut out = std::io::stdout().lock();
    match format {
        OutputFormat::Text => {
            let _ = writeln!(out);
            let _ = writeln!(out, "===== SUMMARY =====");
            let mut had_fail = false;
            for (name, status) in &summary.results {
                if status.is_pass() || status.is_skipped() { continue; }
                had_fail = true;
                let label = status.label();
                let detail = status.detail();
                let line = if detail.is_empty() {
                    format!("{:<14} {}", label, name)
                } else {
                    let trunc: String = detail.chars().take(600).collect();
                    format!("{:<14} {}  # {}", label, name, trunc)
                };
                let _ = writeln!(out, "{}", line);
            }
            if had_fail { let _ = writeln!(out); }

            // Plan 27 Ф.6: skip count. Plan 33 V1: причин теперь несколько
            // (alloc-backend + smt-backend) — общее «skipped», конкретика
            // в каждой SKIP-строке выше.
            if summary.skip > 0 {
                let _ = writeln!(out, "PASS: {}  FAIL: {}  SKIP: {} (skipped)",
                    summary.pass, summary.fail, summary.skip);
            } else {
                let _ = writeln!(out, "PASS: {}  FAIL: {}", summary.pass, summary.fail);
            }

            // Plan 27 Б.4: slowest tests — top 10 если тестов > 10.
            let runnable: Vec<(&str, Duration)> = summary.results.iter()
                .filter(|(_, o)| !o.is_skipped())
                .map(|(name, o)| (name.as_str(), o.elapsed()))
                .collect();
            if runnable.len() > 10 {
                let mut by_time = runnable.clone();
                by_time.sort_by(|a, b| b.1.cmp(&a.1));
                let _ = writeln!(out);
                let _ = writeln!(out, "===== SLOWEST TESTS (top 10) =====");
                for (name, elapsed) in by_time.iter().take(10) {
                    let _ = writeln!(out, "  {:.3}s  {}", elapsed.as_secs_f64(), name);
                }
            }
        }
        OutputFormat::Json => {
            // Plan 26 Ф.16 #11: failed-list в summary event.
            let total_ms: u128 = summary.results.iter().map(|(_, o)| o.elapsed().as_millis()).sum();
            let failed_names: Vec<String> = summary
                .results.iter()
                .filter(|(_, o)| !o.is_pass() && !o.is_skipped())
                .map(|(name, _)| format!("\"{}\"", json_escape(name)))
                .collect();
            let _ = writeln!(
                out,
                "{{\"event\":\"summary\",\"pass\":{},\"fail\":{},\"skip\":{},\"elapsed_ms\":{},\"failed\":[{}]}}",
                summary.pass, summary.fail, summary.skip, total_ms,
                failed_names.join(",")
            );
        }
        OutputFormat::Tap => {
            let _ = writeln!(out, "# pass {}", summary.pass);
            let _ = writeln!(out, "# fail {}", summary.fail);
            if summary.skip > 0 {
                let _ = writeln!(out, "# skip {}", summary.skip);
            }
        }
        OutputFormat::Junit => {
            // JUnit XML batch. Schema: <testsuites><testsuite><testcase>.
            // Skipped → <skipped/>. Pass with retry → <system-out>.
            let non_skip: Vec<(&String, &Outcome)> = summary.results.iter()
                .filter(|(_, o)| !o.is_skipped())
                .map(|(n, o)| (n, o))
                .collect();
            let total_s: f64 = non_skip.iter().map(|(_, o)| o.elapsed().as_secs_f64()).sum();
            let timestamp = chrono_like_iso8601();
            let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
            let _ = writeln!(out,
                "<testsuites name=\"nova_tests\" tests=\"{}\" failures=\"{}\" skipped=\"{}\" time=\"{:.3}\">",
                non_skip.len(), summary.fail, summary.skip, total_s);
            let _ = writeln!(out,
                "  <testsuite name=\"nova_tests\" tests=\"{}\" failures=\"{}\" skipped=\"{}\" time=\"{:.3}\" timestamp=\"{}\">",
                non_skip.len(), summary.fail, summary.skip, total_s, xml_escape(&timestamp));
            // Skipped tests first (per JUnit convention, any order is fine though).
            for (name, outcome) in summary.results.iter().filter(|(_, o)| o.is_skipped()) {
                let (classname, testname) = match name.rfind('/') {
                    Some(idx) => (&name[..idx], &name[idx + 1..]),
                    None => ("", name.as_str()),
                };
                let elapsed_s = outcome.elapsed().as_secs_f64();
                let detail = outcome.detail();
                let _ = writeln!(out,
                    "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\">",
                    xml_escape(classname), xml_escape(testname), elapsed_s);
                let _ = writeln!(out, "      <skipped message=\"{}\"/>", xml_escape(&detail));
                let _ = writeln!(out, "    </testcase>");
            }
            for (name, outcome) in &non_skip {
                let (classname, testname) = match name.rfind('/') {
                    Some(idx) => (&name[..idx], &name[idx + 1..]),
                    None => ("", name.as_str()),
                };
                let elapsed_s = outcome.elapsed().as_secs_f64();
                match outcome {
                    Outcome::Pass { retries, .. } => {
                        if *retries > 0 {
                            // Б.6: retry count visible in JUnit.
                            let _ = writeln!(out,
                                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\">",
                                xml_escape(classname), xml_escape(testname), elapsed_s);
                            let _ = writeln!(out,
                                "      <system-out>retried {} time(s) before pass</system-out>",
                                retries);
                            let _ = writeln!(out, "    </testcase>");
                        } else {
                            let _ = writeln!(out,
                                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"/>",
                                xml_escape(classname), xml_escape(testname), elapsed_s);
                        }
                    }
                    Outcome::Fail { .. } | Outcome::Timeout { .. } => {
                        let stage_str = match outcome {
                            Outcome::Timeout { .. } => "timeout",
                            Outcome::Fail { stage, .. } => match stage {
                                Stage::Codegen { .. }    => "codegen",
                                Stage::Cc { .. }         => "cc",
                                Stage::Run { .. }        => "run",
                                Stage::NoCFile           => "no-c-file",
                                Stage::Expectation { .. }=> "expectation",
                            },
                            _ => "unknown",
                        };
                        let detail = outcome.detail();
                        let _ = writeln!(out,
                            "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\">",
                            xml_escape(classname), xml_escape(testname), elapsed_s);
                        let _ = writeln!(out,
                            "      <failure type=\"{}\" message=\"{}\"/>",
                            xml_escape(stage_str), xml_escape(&detail));
                        let _ = writeln!(out, "    </testcase>");
                    }
                    // Skipped already handled above.
                    _ => {}
                }
            }
            let _ = writeln!(out, "  </testsuite>");
            let _ = writeln!(out, "</testsuites>");
        }
    }
    let _ = out.flush();
}

// ---------- Plan 172.1 U.7.1: CC-FAIL audit harness ----------
//
// Purpose (compiler-conventions §0/§1/§6): C is a sanity-net, NEVER the first
// type-checker. A test that ends in `Stage::Cc` ("CC-FAIL") for a TYPE reason is
// a front-end gap — the checker should have produced a clean Nova diagnostic.
// This harness MEASURES the remaining gap so later 172.1 phases (U.3/U.4/172.2)
// can drive it to zero (the §0-progress metric), and reclassifies every existing
// `EXPECT_CC_ERROR` fixture into type-class (a leak to fix) vs capability-class
// (a legitimate D91 forbid-effect assertion to KEEP) vs toolchain/link.
//
// NB on §3: matching a C-compiler's diagnostic TEXT is not the banned hardcode —
// §3 forbids baking Nova type/fn NAMES into the compiler. Here we pattern-match
// the BACKEND's diagnostic phrases (clang/gcc text + MSVC codes), which is the
// only way to classify cc output. Genuinely ambiguous inputs are reported as
// `Unknown` (not force-fit), per §4 ("no silent holes") and §7.3 (human-confirm
// the borderline set).

/// Classification of a C-compiler failure / `EXPECT_CC_ERROR` assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CcErrorClass {
    /// Type-system leak: SHOULD be a clean Nova checker diagnostic (§0/§1/§6).
    /// Category mismatch, narrowing, no-overload, no-member, unknown `Nova_` type,
    /// no-such-variant/method. These are the leaks U.7 drives to zero.
    Type,
    /// Capability-isolation (D91 forbid-effect): a method/member is genuinely
    /// absent because the effect was forbidden — a legitimate CC assertion. KEEP.
    Capability,
    /// Toolchain/link failure (linker, file-lock, missing header/runtime symbol).
    /// NOT a type error; a separate bucket the U.7 gate must never count as a leak.
    Toolchain,
    /// Could not be classified from the available text — needs human review.
    Unknown,
}

impl CcErrorClass {
    pub fn label(self) -> &'static str {
        match self {
            CcErrorClass::Type => "TYPE",
            CcErrorClass::Capability => "CAPABILITY",
            CcErrorClass::Toolchain => "TOOLCHAIN",
            CcErrorClass::Unknown => "UNKNOWN",
        }
    }
}

/// clang/gcc text + MSVC codes that denote a TYPE error. Operand `s` must be
/// already-lowercased.
fn cc_text_is_type_class(s: &str) -> bool {
    const PATS: &[&str] = &[
        "incompatible",
        "passing argument",
        "no member named",
        "too few arguments",
        "too many arguments",
        "no matching function",
        "is not a member",
        "is not a structure or union",
        "member reference",
        "subscripted value is not",
        "called object type",
        "conflicting types for",
        "undeclared identifier",
        "unknown type name",
        "initializing",
        // MSVC diagnostic text / codes
        "cannot convert",
        "does not take",
        "c2440", "c2664", "c2660", "c2039", "c2027", "c2065", "c2228", "c2036",
    ];
    PATS.iter().any(|p| s.contains(p))
}

/// clang/gcc/MSVC phrases that denote a TOOLCHAIN/LINK failure (never a type
/// leak). Operand `s` must be already-lowercased.
fn cc_text_is_toolchain(s: &str) -> bool {
    const PATS: &[&str] = &[
        "cannot open output file",
        "spawn cc",
        "mkdir subdir",
        "mkdir obj_dir",
        "undefined reference",
        "undefined symbol",
        "unresolved external",
        "lld-link",
        "ld.lld",
        "linker command failed",
        "lnk2019", "lnk1120", "lnk2001",
        "cannot open include file",
        "file not found",
        "no such file",
        "c1083",
    ];
    PATS.iter().any(|p| s.contains(p))
}

/// Classify a raw CC error string captured at run time (list A — the actual
/// `Stage::Cc` failures on the corpus). Toolchain checked first: a failure that
/// reached the linker compiled cleanly, so it is never a type leak.
pub fn classify_cc_error_text(error: &str) -> CcErrorClass {
    let s = error.to_lowercase();
    if cc_text_is_toolchain(&s) {
        return CcErrorClass::Toolchain;
    }
    if cc_text_is_type_class(&s) {
        return CcErrorClass::Type;
    }
    CcErrorClass::Unknown
}

/// Classify an `EXPECT_CC_ERROR` marker (list B) by its asserted pattern + path
/// context. D91 capability-isolation fixtures live under `negative_capability/`;
/// the asserted symbol's case disambiguates a mangled Nova *type* (`Nova_…`,
/// front-end gap) from a runtime C *function* symbol (`nova_…`, link assertion).
pub fn classify_cc_expect(pat: &str, path: &Path) -> CcErrorClass {
    let p = pat.to_lowercase();
    let path_str = path.to_string_lossy().replace('\\', "/");
    let in_capability_dir = path_str.contains("/negative_capability/");

    // Mangled Nova *type*/variant symbol (capital `Nova_…`, `MemOrdering_…`):
    // unknown-type / no-such-variant / no-such-method = a front-end gap (type-class).
    if pat.starts_with("Nova_") || pat.starts_with("MemOrdering") {
        return CcErrorClass::Type;
    }
    // Lowercase runtime C symbol (e.g. `nova_fn_main_impl`) or an explicit link
    // phrase → a link/toolchain assertion, not a type leak.
    if pat.starts_with("nova_") || cc_text_is_toolchain(&p) {
        return CcErrorClass::Toolchain;
    }
    if cc_text_is_type_class(&p) {
        return CcErrorClass::Type;
    }
    // Empty/generic pattern: rely on directory context.
    if in_capability_dir {
        return CcErrorClass::Capability;
    }
    // No source-only signal (typically an empty-pattern type test outside the
    // capability dir) → needs human review rather than a forced guess (§4/§7.3).
    CcErrorClass::Unknown
}

/// Recursively collect every `.nv` file under `dir` (raw — no folder-module /
/// slow-lane / target-OS filtering), so the EXPECT_CC_ERROR audit is exhaustive
/// over the corpus rather than over the runnable-entry subset.
fn collect_all_nv_raw(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_all_nv_raw(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("nv") {
            out.push(path);
        }
    }
}

/// Plan 172.1 U.7.1: emit the CC-FAIL audit report.
///
/// - **(A)** Un-expected CC-FAIL leaks on the run corpus, classified — a
///   `Stage::Cc` outcome is, by construction, a test whose cc step failed with
///   NO satisfied `EXPECT_CC_ERROR` (run_one converts a matching EXPECT_CC_ERROR
///   to `Pass`). The type-class subtotal is the §0-progress metric U.7 drives to
///   zero.
/// - **(B)** Every existing `EXPECT_CC_ERROR` fixture under `scan_dirs`,
///   classified type-class (a leak to migrate to a Nova diagnostic) vs
///   capability-class (legitimate D91, KEEP) vs toolchain/link.
pub fn print_cc_leak_report(results: &[(String, Outcome)], scan_dirs: &[PathBuf]) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out);
    let _ = writeln!(out, "===== CC-FAIL AUDIT (Plan 172.1 U.7.1) =====");

    // ---- List A: un-expected CC-FAILs (tests WITHOUT a satisfied EXPECT_CC_ERROR).
    let mut a_rows: Vec<(CcErrorClass, &str, String)> = Vec::new();
    let (mut a_type, mut a_tool, mut a_unknown) = (0usize, 0usize, 0usize);
    for (name, outcome) in results {
        if let Outcome::Fail { stage: Stage::Cc { error }, .. } = outcome {
            let class = classify_cc_error_text(error);
            match class {
                CcErrorClass::Type => a_type += 1,
                CcErrorClass::Toolchain => a_tool += 1,
                // Capability is not expected from raw run-time cc text on the
                // positive corpus; fold any into `unknown` for visibility.
                CcErrorClass::Capability | CcErrorClass::Unknown => a_unknown += 1,
            }
            let detail: String = error.chars().take(140).collect();
            a_rows.push((class, name.as_str(), detail));
        }
    }
    a_rows.sort_by(|x, y| x.0.label().cmp(y.0.label()).then(x.1.cmp(y.1)));
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "[A] Un-expected CC-FAIL leaks on run corpus ({} tests ran):",
        results.len()
    );
    if a_rows.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for (class, name, detail) in &a_rows {
            let _ = writeln!(out, "  {:<10} {}  # {}", class.label(), name, detail);
        }
    }
    let _ = writeln!(
        out,
        "  --- A totals: type-class={}  toolchain={}  unknown={}  (total CC-FAIL={})",
        a_type, a_tool, a_unknown, a_rows.len()
    );

    // ---- List B: existing EXPECT_CC_ERROR fixtures, classified (source scan).
    let mut files: Vec<PathBuf> = Vec::new();
    for d in scan_dirs {
        if d.is_dir() {
            collect_all_nv_raw(d, &mut files);
        } else if d.extension().and_then(|s| s.to_str()) == Some("nv") {
            files.push(d.clone());
        }
    }
    files.sort();
    files.dedup();
    let mut b_rows: Vec<(CcErrorClass, String, String)> = Vec::new();
    let (mut b_type, mut b_cap, mut b_tool, mut b_unknown) = (0usize, 0usize, 0usize, 0usize);
    for f in &files {
        let Ok(src) = std::fs::read_to_string(f) else { continue; };
        for m in parse_expect(&src) {
            if let ExpectMarker::CcError(pat) = m {
                let class = classify_cc_expect(&pat, f);
                match class {
                    CcErrorClass::Type => b_type += 1,
                    CcErrorClass::Capability => b_cap += 1,
                    CcErrorClass::Toolchain => b_tool += 1,
                    CcErrorClass::Unknown => b_unknown += 1,
                }
                let rel = f.to_string_lossy().replace('\\', "/");
                let pat_disp = if pat.is_empty() { "<any>".to_string() } else { pat.clone() };
                b_rows.push((class, rel, pat_disp));
            }
        }
    }
    b_rows.sort_by(|x, y| x.0.label().cmp(y.0.label()).then(x.1.cmp(&y.1)));
    let _ = writeln!(out);
    let _ = writeln!(out, "[B] Existing EXPECT_CC_ERROR fixtures (classified):");
    if b_rows.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for (class, path, pat) in &b_rows {
            let _ = writeln!(out, "  {:<10} {}  // EXPECT_CC_ERROR {}", class.label(), path, pat);
        }
    }
    let _ = writeln!(
        out,
        "  --- B totals: type-class={}  capability={}  toolchain={}  unknown={}  (total={})",
        b_type, b_cap, b_tool, b_unknown, b_rows.len()
    );

    // ---- Headline §0-progress metric.
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        ">>> U.7 §0-progress metric — type-class CC-FAIL leaks (A) = {}",
        a_type
    );
    let _ = writeln!(
        out,
        ">>> EXPECT_CC_ERROR still type-class (candidates to become Nova diagnostics) = {}",
        b_type
    );
    let _ = out.flush();
}

/// Best-effort ISO-8601 timestamp без extra deps. Format: YYYY-MM-DDTHH:MM:SS.
/// На systems где SystemTime accuracy ≥1 s — достаточно для JUnit timestamp.
fn chrono_like_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0);
    // Простой Y/M/D разбор. Не handlим leap seconds, UTC always.
    let days = (secs / 86400) as i64;
    let h = ((secs % 86400) / 3600) as u32;
    let m = ((secs % 3600) / 60) as u32;
    let s = (secs % 60) as u32;
    // Days since 1970-01-01. Простое вычисление Y/M/D через
    // алгоритм Howard Hinnant (civil_from_days).
    let (y, mo, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", y, mo, d, h, m, s)
}

/// Howard Hinnant's civil_from_days — стандартный алгоритм
/// для конверсии days-since-epoch → (year, month, day) без libc/chrono.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i32 + (era as i32) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_marker(src: &str) -> Option<ExpectMarker> {
        parse_expect(src).into_iter().next()
    }

    // ---- [M-test-runner-tempdir-race-jobs]: transient exec-lock classifier ----

    #[test]
    fn exec_lock_classifies_transient_windows_codes() {
        // ERROR_ACCESS_DENIED / ERROR_SHARING_VIOLATION — the two codes a
        // freshly-linked .exe momentarily locked by AV/Defender scan-on-
        // execute (or another process still holding a handle) surfaces as.
        // These SHOULD be retried instead of failing the test outright.
        let denied = std::io::Error::from_raw_os_error(5);
        let sharing = std::io::Error::from_raw_os_error(32);
        assert!(is_transient_exec_lock_error(&denied));
        assert!(is_transient_exec_lock_error(&sharing));
    }

    #[test]
    fn exec_lock_does_not_classify_real_errors() {
        // A genuinely missing binary / real permissions problem must NOT be
        // retried — that would turn an honest failure into a false PASS (or
        // just needlessly slow down a real, reproducible failure).
        let not_found = std::io::Error::from_raw_os_error(2); // ERROR_FILE_NOT_FOUND
        let generic = std::io::Error::new(std::io::ErrorKind::Other, "boom");
        assert!(!is_transient_exec_lock_error(&not_found));
        assert!(!is_transient_exec_lock_error(&generic));
    }

    // ---- Plan 172.1 U.7.1: CC-FAIL classifier tests ----

    #[test]
    fn cc_classify_text_type_vs_toolchain() {
        // Type-class C diagnostics (clang/gcc text + MSVC codes).
        assert_eq!(
            classify_cc_error_text("foo.c:9:5: error: passing 'int' to parameter of incompatible type 'nova_str'"),
            CcErrorClass::Type
        );
        assert_eq!(
            classify_cc_error_text("error: no member named 'host_str' in 'NovaSocketAddr'"),
            CcErrorClass::Type
        );
        assert_eq!(
            classify_cc_error_text("error: too few arguments to function call, expected 2"),
            CcErrorClass::Type
        );
        assert_eq!(
            classify_cc_error_text("error: member reference type 'nova_int' (aka 'long long') is not a structure or union"),
            CcErrorClass::Type
        );
        // codegen-emitted invalid C (lvalue cast) is NOT a type-checking gap →
        // stays Unknown for human review, not force-fit into Type.
        assert_eq!(
            classify_cc_error_text("error: assignment to cast is illegal, lvalue casts are not supported"),
            CcErrorClass::Unknown
        );
        assert_eq!(
            classify_cc_error_text("x.c(12): error C2440: 'initializing': cannot convert from 'int' to 'char'"),
            CcErrorClass::Type
        );
        // Toolchain/link must win even if a type-looking token appears.
        assert_eq!(
            classify_cc_error_text("lld-link: error: undefined symbol: nova_fn_main_impl"),
            CcErrorClass::Toolchain
        );
        assert_eq!(
            classify_cc_error_text("spawn cc: program not found"),
            CcErrorClass::Toolchain
        );
        assert_eq!(
            classify_cc_error_text("LINK : fatal error LNK1120: 1 unresolved externals"),
            CcErrorClass::Toolchain
        );
        // Genuinely unclassifiable → Unknown, not a forced guess (§4).
        assert_eq!(classify_cc_error_text("some unrelated message"), CcErrorClass::Unknown);
    }

    #[test]
    fn cc_classify_expect_by_pattern_and_path() {
        use std::path::PathBuf;
        // Empty pattern in the capability dir = legitimate D91 assertion (KEEP).
        let cap = PathBuf::from("nova_tests/negative_capability/channel_sender_no_recv.nv");
        assert_eq!(classify_cc_expect("", &cap), CcErrorClass::Capability);
        // Type-class pattern.
        let typ = PathBuf::from("nova_tests/plan135/neg2_wrong_arg_type.nv");
        assert_eq!(classify_cc_expect("incompatible type", &typ), CcErrorClass::Type);
        let nomem = PathBuf::from("nova_tests/plan135/neg1_no_overload_not_found.nv");
        assert_eq!(classify_cc_expect("no member named", &nomem), CcErrorClass::Type);
        // Mangled Nova *type*/variant symbol (capital) = front-end gap (type-class).
        let atomics = PathBuf::from("nova_tests/atomics/neg/x.nv");
        assert_eq!(classify_cc_expect("Nova_AtomicI32_static_from_bytes", &atomics), CcErrorClass::Type);
        assert_eq!(classify_cc_expect("MemOrdering_NoSuchVariant", &atomics), CcErrorClass::Type);
        // Lowercase runtime C symbol = link/toolchain assertion (NOT a type leak).
        let link = PathBuf::from("nova_tests/plan159/neg_library_not_pruned.nv");
        assert_eq!(classify_cc_expect("nova_fn_main_impl", &link), CcErrorClass::Toolchain);
        // Empty pattern outside the capability dir = no source-only signal → review.
        let amb = PathBuf::from("nova_tests/plan59/neg/f9_tuple_type_mismatch_rejected.nv");
        assert_eq!(classify_cc_expect("", &amb), CcErrorClass::Unknown);
    }

    #[test]
    fn parse_expect_compile_error() {
        let src = "// EXPECT_COMPILE_ERROR undefined identifier\nmodule x\n";
        match first_marker(src) {
            Some(ExpectMarker::CompileError(p)) => assert_eq!(p, "undefined identifier"),
            other => panic!("expected CompileError, got {:?}", other),
        }
    }

    #[test]
    fn parse_expect_runtime_panic() {
        let src = "// EXPECT_RUNTIME_PANIC index out of bounds\nmodule x\n";
        match first_marker(src) {
            Some(ExpectMarker::RuntimePanic(p)) => assert_eq!(p, "index out of bounds"),
            other => panic!("expected RuntimePanic, got {:?}", other),
        }
    }

    #[test]
    fn parse_expect_exit_code() {
        let src = "// EXPECT_EXIT_CODE 42\nmodule x\n";
        match first_marker(src) {
            Some(ExpectMarker::ExitCode(n)) => assert_eq!(n, 42),
            other => panic!("expected ExitCode, got {:?}", other),
        }
    }

    #[test]
    fn parse_expect_stdout() {
        let src = "// EXPECT_STDOUT hello\nmodule x\n";
        match first_marker(src) {
            Some(ExpectMarker::Stdout(p)) => assert_eq!(p, "hello"),
            other => panic!("expected Stdout, got {:?}", other),
        }
    }

    #[test]
    fn parse_expect_stderr() {
        let src = "// EXPECT_STDERR panic\nmodule x\n";
        match first_marker(src) {
            Some(ExpectMarker::Stderr(p)) => assert_eq!(p, "panic"),
            other => panic!("expected Stderr, got {:?}", other),
        }
    }

    #[test]
    fn parse_expect_multi_marker() {
        // RUNTIME_PANIC + STDOUT работают вместе — оба маркера собираются.
        let src = "// EXPECT_RUNTIME_PANIC nova: unhandled Fail: bang\n\
                   // EXPECT_STDOUT DEFER_FIRED\nmodule x\n";
        let markers = parse_expect(src);
        assert_eq!(markers.len(), 2, "expected 2 markers, got {:?}", markers);
        assert!(matches!(&markers[0], ExpectMarker::RuntimePanic(p) if p == "nova: unhandled Fail: bang"));
        assert!(matches!(&markers[1], ExpectMarker::Stdout(p) if p == "DEFER_FIRED"));
    }

    #[test]
    fn parse_expect_multiple_stdout() {
        // Несколько EXPECT_STDOUT-паттернов — все собираются.
        let src = "// EXPECT_STDOUT line1\n// EXPECT_STDOUT line2\nmodule x\n";
        let markers = parse_expect(src);
        assert_eq!(markers.len(), 2);
        assert!(matches!(&markers[0], ExpectMarker::Stdout(p) if p == "line1"));
        assert!(matches!(&markers[1], ExpectMarker::Stdout(p) if p == "line2"));
    }

    #[test]
    fn parse_expect_skips_after_30_lines() {
        // 30 пустых + комментарий-маркер на 31-й
        let mut src = String::new();
        for _ in 0..30 {
            src.push_str("\n");
        }
        src.push_str("// EXPECT_EXIT_CODE 7\n");
        assert!(parse_expect(&src).is_empty());
    }

    #[test]
    fn parse_expect_none_no_marker() {
        let src = "module x\nfn main() { print(\"hi\") }\n";
        assert!(parse_expect(src).is_empty());
    }

    #[test]
    fn parse_expect_after_module_line() {
        // Ф.15 regression: до fix'а `?` оператор возвращал None на
        // первой non-`//` строке, не дочитав маркер ниже.
        let src = "module foo\n\n// EXPECT_EXIT_CODE 42\ntest \"x\" {}\n";
        match first_marker(src) {
            Some(ExpectMarker::ExitCode(42)) => {}
            other => panic!("expected ExitCode(42), got {:?}", other),
        }
    }

    #[test]
    fn parse_expect_after_blank_line() {
        // Blank line на 1-й строке не должна abort'нуть поиск.
        let src = "\n// EXPECT_STDOUT hello\nmodule foo\n";
        match first_marker(src) {
            Some(ExpectMarker::Stdout(p)) => assert_eq!(p, "hello"),
            other => panic!("expected Stdout(hello), got {:?}", other),
        }
    }

    #[test]
    fn parse_expect_mixed_comment_and_code() {
        // Mix of comment, code, and marker — marker должен найтись.
        let src = "// some doc comment\nmodule foo\n// more doc\n\
                   // EXPECT_RUNTIME_PANIC index out of bounds\ntest {}\n";
        match first_marker(src) {
            Some(ExpectMarker::RuntimePanic(p)) => assert_eq!(p, "index out of bounds"),
            other => panic!("expected RuntimePanic, got {:?}", other),
        }
    }

    // ---------- Plan 83.1 Ф.2: parse_env (`// ENV NAME=VALUE`) ----------

    #[test]
    fn parse_env_single() {
        let src = "// ENV NOVA_MAXPROCS=3\nmodule x\n";
        assert_eq!(parse_env(src), vec![("NOVA_MAXPROCS".into(), "3".into())]);
    }

    #[test]
    fn parse_env_multiple() {
        let src = "// ENV FOO=1\n// ENV BAR=two\nmodule x\n";
        assert_eq!(
            parse_env(src),
            vec![
                ("FOO".into(), "1".into()),
                ("BAR".into(), "two".into()),
            ]
        );
    }

    #[test]
    fn parse_env_empty_value() {
        // VALUE может быть пустым — переменная задаётся пустой строкой.
        let src = "// ENV NOVA_MAXPROCS=\nmodule x\n";
        assert_eq!(parse_env(src), vec![("NOVA_MAXPROCS".into(), "".into())]);
    }

    #[test]
    fn parse_env_value_with_equals() {
        // Только первый `=` разделяет; остаток уходит в VALUE.
        let src = "// ENV KEY=a=b=c\nmodule x\n";
        assert_eq!(parse_env(src), vec![("KEY".into(), "a=b=c".into())]);
    }

    #[test]
    fn parse_env_requires_separator() {
        // `ENVOTHER` не должен матчиться как директива ENV.
        let src = "// ENVOTHER=1\nmodule x\n";
        assert!(parse_env(src).is_empty());
    }

    #[test]
    fn parse_env_ignores_no_equals() {
        let src = "// ENV JUSTNAME\nmodule x\n";
        assert!(parse_env(src).is_empty());
    }

    #[test]
    fn parse_env_none() {
        let src = "module x\ntest \"t\" {}\n";
        assert!(parse_env(src).is_empty());
    }

    #[test]
    fn parse_env_skips_after_30_lines() {
        let mut src = String::new();
        for _ in 0..30 {
            src.push('\n');
        }
        src.push_str("// ENV NOVA_MAXPROCS=4\n");
        assert!(parse_env(&src).is_empty());
    }

    // ---------- [A-S1 mutclock-regress]: collect_marker_sources ----------
    // Root cause of the mut_clock auto-idle-advance ordering flake: a
    // folder-module CU's `opts.nv_file` is the alphabetically-first peer
    // (e.g. `core.nv`), which typically has no header directives — those
    // live on the peer that declares the `test "..."` blocks (`core_test.nv`).
    // `collect_marker_sources` must surface that peer's own source as a
    // SEPARATE entry (not lose it past a naive 30-line-of-the-concatenation
    // cutoff) so callers' per-source `parse_env`/etc still see it.

    #[test]
    fn collect_marker_sources_single_file_no_peers() {
        let dir = std::env::temp_dir().join(format!("nova_cms_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let entry = dir.join("solo.nv");
        std::fs::write(&entry, "module solo\ntest \"x\" { assert(true) }\n").unwrap();
        let src = std::fs::read_to_string(&entry).unwrap();
        let sources = collect_marker_sources(&src, &entry);
        assert_eq!(sources.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_marker_sources_finds_same_module_peer_directive() {
        let dir = std::env::temp_dir().join(format!("nova_cms_test2_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // Entry file: alphabetically first, same shape as core.nv — no
        // ENV directive of its own, `module` line pushed past line 30 by
        // a long header comment (mirrors the real fixture).
        let entry = dir.join("core.nv");
        let mut entry_src = String::new();
        for i in 0..40 {
            entry_src.push_str(&format!("// header line {}\n", i));
        }
        entry_src.push_str("module testing.handlers\n");
        std::fs::write(&entry, &entry_src).unwrap();
        // Peer file: alphabetically after, carries the directive AND the
        // `test` blocks — same module declaration.
        let peer = dir.join("core_test.nv");
        std::fs::write(
            &peer,
            "// ENV NOVA_AUTOARM=0\nmodule testing.handlers\ntest \"y\" { assert(true) }\n",
        )
        .unwrap();
        let sources = collect_marker_sources(&entry_src, &entry);
        assert_eq!(sources.len(), 2, "expected entry + 1 same-module peer");
        // Simulates the `run_one` merge: at least one source must parse
        // the directive — this is exactly what silently returned empty
        // before the fix (parse_env(&src) only ever saw `entry_src`).
        let found_autoarm = sources
            .iter()
            .flat_map(|s| parse_env(s))
            .any(|(k, v)| k == "NOVA_AUTOARM" && v == "0");
        assert!(found_autoarm, "NOVA_AUTOARM=0 directive from peer file must be visible");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_marker_sources_ignores_different_module_peer() {
        let dir = std::env::temp_dir().join(format!("nova_cms_test3_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let entry = dir.join("a.nv");
        let entry_src = "module a\ntest \"x\" { assert(true) }\n".to_string();
        std::fs::write(&entry, &entry_src).unwrap();
        // Unrelated .nv file in the same directory, different module —
        // must NOT be pulled in.
        let unrelated = dir.join("b.nv");
        std::fs::write(&unrelated, "// ENV SHOULD_NOT_APPEAR=1\nmodule b\n").unwrap();
        let sources = collect_marker_sources(&entry_src, &entry);
        assert_eq!(sources.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------- Plan 26 Ф.17 #11: civil_from_days regression tests ----------

    #[test]
    fn civil_from_days_epoch() {
        // Unix epoch 1970-01-01.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn civil_from_days_y2k() {
        // 2000-01-01 = 10957 дней с epoch.
        assert_eq!(civil_from_days(10957), (2000, 1, 1));
    }

    #[test]
    fn civil_from_days_leap_year_29_feb() {
        // 2000 leap year → 29 Feb валидно. 10957 + 31 + 28 = 11016.
        assert_eq!(civil_from_days(11016), (2000, 2, 29));
        // Следующий день — 1 Mar.
        assert_eq!(civil_from_days(11017), (2000, 3, 1));
    }

    #[test]
    fn civil_from_days_recent() {
        // 2024-01-15 = 19737 дней с epoch.
        assert_eq!(civil_from_days(19737), (2024, 1, 15));
    }

    // ---------- Plan 26 Ф.16 #10: duplicate marker first-wins ----------

    #[test]
    fn parse_expect_duplicate_first_wins() {
        let src = "// EXPECT_EXIT_CODE 1\n// EXPECT_STDOUT hello\ntest {}\n";
        match first_marker(src) {
            Some(ExpectMarker::ExitCode(1)) => {}
            other => panic!("expected ExitCode(1) (first), got {:?}", other),
        }
    }

    #[test]
    fn display_name_simple() {
        let path = Path::new("d:/repo/nova_tests/basics/literals.nv");
        let cwd = Path::new("d:/repo");
        assert_eq!(display_name(path, cwd), "nova_tests/basics/literals");
    }

    #[test]
    fn display_name_stdlib() {
        let path = Path::new("d:/repo/std/checksums/fnv.nv");
        let cwd = Path::new("d:/repo");
        assert_eq!(display_name(path, cwd), "std/checksums/fnv");
    }

    #[test]
    fn march_flag_default() {
        std::env::remove_var("NOVA_MARCH_NATIVE");
        assert_eq!(march_flag(), "x86-64-v3");
    }

    #[test]
    fn march_flag_native_env() {
        std::env::set_var("NOVA_MARCH_NATIVE", "1");
        assert_eq!(march_flag(), "native");
        std::env::remove_var("NOVA_MARCH_NATIVE");
    }

    #[test]
    fn parse_smt_backend_marker_present() {
        let src = "// REQUIRES_SMT_BACKEND z3\n// EXPECT_COMPILE_ERROR x\nmodule m\n";
        assert_eq!(parse_smt_backend_requirement(src), Some("z3".into()));
    }

    #[test]
    fn parse_smt_backend_marker_case_insensitive() {
        let src = "// REQUIRES_SMT_BACKEND Trivial\nmodule m\n";
        assert_eq!(parse_smt_backend_requirement(src), Some("trivial".into()));
    }

    #[test]
    fn parse_smt_backend_marker_missing() {
        let src = "module m\nfn f() => 1\n";
        assert_eq!(parse_smt_backend_requirement(src), None);
    }

    #[test]
    fn parse_smt_backend_marker_only_first_30_lines() {
        let mut s = String::new();
        for _ in 0..40 { s.push_str("// padding\n"); }
        s.push_str("// REQUIRES_SMT_BACKEND z3\n");
        // 31-я строка и далее не учитываются.
        assert_eq!(parse_smt_backend_requirement(&s), None);
    }

    // Plan 72 P3-B: vtable dispatch for protocol-as-value.
    // Originally (P0) this fixture expected E7201; P3-B implements vtable codegen
    // so the same pattern now succeeds. Verifies codegen_to_c succeeds (no E7201).
    #[test]
    fn p0_erased_now_dispatches_via_vtable() {
        let nv_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .join("nova_tests/plan72/p0_erased_method_call_neg.nv");
        if !nv_path.exists() {
            return;
        }
        let src = std::fs::read_to_string(&nv_path).expect("read p0 fixture");
        let repo = find_repo_root_from(&nv_path).expect("p0 fixture is in-tree");
        let stdlib_dir = crate::manifest::resolve_std_path(&repo);
        let result = codegen_to_c(
            &nv_path, &src, None, ast::ContractsMode::Checked, &repo, &stdlib_dir,
        );
        assert!(result.is_ok(), "P3-B vtable dispatch: codegen должен успешно скомпилировать, но: {:?}", result.err());
    }

    // ---- Plan 221.1 №158: honest merged-CU RUN-FAIL attribution ----

    #[test]
    fn demangle_nova_fn_splits_module_and_short_name() {
        let (segs, name) = demangle_nova_fn(
            "nova_fn_10spec_tests11conformance11ok_declared",
        ).expect("should demangle");
        assert_eq!(segs, vec!["spec_tests".to_string(), "conformance".to_string()]);
        assert_eq!(name, "ok_declared");
    }

    #[test]
    fn demangle_nova_fn_rejects_synthetic_names() {
        // `nova_fn_main_impl` / test wrappers / dispatch shims are NOT this
        // scheme (no length-prefix after `nova_fn_`) — must return None so
        // callers skip to the next stack frame instead of misparsing.
        assert!(demangle_nova_fn("nova_fn_main_impl").is_none());
        assert!(demangle_nova_fn("Nova_Log1_info").is_none());
        assert!(demangle_nova_fn("nova_test_d62__131__0").is_none());
    }

    #[test]
    fn attribute_merged_cu_crash_finds_real_culprit_from_captured_segv_trace() {
        // Real `NOVA_DIAG_SEGV=1` stderr captured 2026-07-30 off the ACTUAL
        // pre-fix `d62_raw_effect_op_pos.nv` crash (`Nova_Log1_info`
        // deref'ing a NULL `_nova_handler_Log1`) — see registry №158. Before
        // this fix, `walk_nv_filtered_ex`'s folder-module collapse blamed
        // whichever peer sorted first alphabetically (`a_q3_...`), never the
        // real file. This asserts the NEW attribution correctly reads the
        // KEYSTONE frame (`nova_fn_10spec_tests11conformance11ok_declared`)
        // and maps it back to `d62_raw_effect_op_pos.nv` — the ONLY peer that
        // actually declares `fn ok_declared`.
        let stderr = r#"
=== [SEGV-DIAG] EXCEPTION_ACCESS_VIOLATION ===
=== Stack trace (frame[1] = caller of crash site = KEYSTONE) ===
  #00 00007FF6269C55CB  d62_raw_effect_op_pos!Nova_Log1_info+0x2B  (d62_raw_effect_op_pos.c:1281)
  #01 00007FF6269C5323  d62_raw_effect_op_pos!nova_fn_10spec_tests11conformance11ok_declared+0x33  (d62_raw_effect_op_pos.c:9256)
  #02 00007FF6269C4AD6  d62_raw_effect_op_pos!nova_test_d62__131__exported_fn_______________________raw_op_________0+0x26  (d62_raw_effect_op_pos.c:9433)
  #03 00007FF6269C4276  d62_raw_effect_op_pos!nova_test_chunk_0+0x186  (d62_raw_effect_op_pos.c:10265)
  #04 00007FF6269C3E3B  d62_raw_effect_op_pos!nova_fn_main_impl+0x3B  (d62_raw_effect_op_pos.c:10361)
=== [SEGV-DIAG END] ===
"#;
        let root = std::env::temp_dir().join(format!("nova_p221_attr_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        let a_q3 = root.join("a_q3_println_debug_record.nv");
        let d61 = root.join("d61_effect_handler_direct_call.nv");
        let d62 = root.join("d62_raw_effect_op_pos.nv");
        std::fs::write(&a_q3, "module spec_tests.conformance\nfn unrelated_a() -> int { 1 }\n").unwrap();
        std::fs::write(&d61, "module spec_tests.conformance\nfn unrelated_d61() -> int { 2 }\n").unwrap();
        std::fs::write(
            &d62,
            "module spec_tests.conformance\nexport fn ok_declared(s str) -> int { 1 }\n",
        ).unwrap();
        let peers = vec![a_q3.clone(), d61.clone(), d62.clone()];
        let culprit = attribute_merged_cu_crash(stderr, &peers);
        assert_eq!(culprit, Some(d62), "must name the REAL culprit, not the alphabetically-first peer (a_q3)");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn attribute_merged_cu_crash_honestly_refuses_to_guess() {
        // No SEGV-DIAG block at all (e.g. a stack-overflow/abort that never
        // reaches the VEH) — must return `None`, never fall back to naming
        // some file as if it were determined.
        assert_eq!(attribute_merged_cu_crash("no diag here", &[]), None);
        // A keystone frame whose short name matches MULTIPLE peers is
        // ambiguous — must also refuse (`None`), not pick the first one.
        let stderr = "  #01 0000000000000000  x!nova_fn_5mymod6shared+0x1  (x.c:1)\n";
        let root = std::env::temp_dir().join(format!("nova_p221_attr_ambig_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        let f1 = root.join("f1.nv");
        let f2 = root.join("f2.nv");
        std::fs::write(&f1, "fn shared() -> int { 1 }\n").unwrap();
        std::fs::write(&f2, "fn shared() -> int { 2 }\n").unwrap();
        assert_eq!(attribute_merged_cu_crash(stderr, &[f1, f2]), None);
        let _ = std::fs::remove_dir_all(&root);
    }

}

// Plan 156: slow-test-lane discovery (`*_slow.nv`).
#[cfg(test)]
mod plan156_slow_lane_tests {
    use super::{is_slow_file_stem, walk_nv_filtered, SlowLane};
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    /// Collect the bare file names (basename, with extension) discovered by a
    /// walk — robust to path separators across platforms.
    fn names(files: &[PathBuf]) -> BTreeSet<String> {
        files
            .iter()
            .map(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .expect("utf-8 file name")
                    .to_string()
            })
            .collect()
    }

    fn write(path: &Path, body: &str) {
        std::fs::write(path, body).expect("write fixture file");
    }

    #[test]
    fn is_slow_file_stem_classification() {
        assert!(is_slow_file_stem("big_slow"), "_slow stem must be slow");
        assert!(
            !is_slow_file_stem("notslow"),
            "ends with 'slow' but not '_slow' -> NOT slow"
        );
        assert!(!is_slow_file_stem("a"), "plain stem must not be slow");
    }

    #[test]
    fn walk_nv_filtered_slow_lanes() {
        // Unique, deterministic temp dir (process id, no timestamps/random).
        let root = std::env::temp_dir().join(format!("nova_p156_slowlane_{}", std::process::id()));
        // Idempotency: start from a clean slate.
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).expect("create temp sub");

        // Distinct `module X` per file (or none) so folder-module detection
        // does NOT group them into a single non-standalone unit.
        write(&root.join("a.nv"), "module a_mod\n");
        write(&root.join("big_slow.nv"), "module big_slow_mod\n");
        // Edge: ends with "slow" but NOT "_slow" -> treated as NORMAL.
        write(&root.join("notslow.nv"), "module notslow_mod\n");
        write(&sub.join("nested_slow.nv"), "module nested_slow_mod\n");
        write(&sub.join("plain.nv"), "module plain_mod\n");

        // Exclude: skip *_slow.nv at every level.
        let mut excl = Vec::new();
        walk_nv_filtered(&root, &mut excl, SlowLane::Exclude).expect("walk exclude");
        let excl = names(&excl);
        assert!(excl.contains("a.nv"), "Exclude must keep a.nv: {:?}", excl);
        assert!(
            excl.contains("notslow.nv"),
            "Exclude must keep notslow.nv (edge): {:?}",
            excl
        );
        assert!(
            excl.contains("plain.nv"),
            "Exclude must keep sub/plain.nv: {:?}",
            excl
        );
        assert!(
            !excl.contains("big_slow.nv"),
            "Exclude must drop big_slow.nv: {:?}",
            excl
        );
        assert!(
            !excl.contains("nested_slow.nv"),
            "Exclude must drop sub/nested_slow.nv: {:?}",
            excl
        );

        // Include: everything.
        let mut incl = Vec::new();
        walk_nv_filtered(&root, &mut incl, SlowLane::Include).expect("walk include");
        let incl = names(&incl);
        for f in ["a.nv", "big_slow.nv", "notslow.nv", "nested_slow.nv", "plain.nv"] {
            assert!(incl.contains(f), "Include must contain {}: {:?}", f, incl);
        }

        // Only: ONLY *_slow.nv.
        let mut only = Vec::new();
        walk_nv_filtered(&root, &mut only, SlowLane::Only).expect("walk only");
        let only = names(&only);
        let expected: BTreeSet<String> =
            ["big_slow.nv", "nested_slow.nv"].iter().map(|s| s.to_string()).collect();
        assert_eq!(only, expected, "Only must contain exactly the *_slow.nv files");

        // Cleanup.
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn walk_nv_selected_type_filter() {
        use super::{walk_nv_selected, TestSelection, TestType};
        use std::fs;
        let root = std::env::temp_dir().join(format!("nova_p169_sel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        // positive
        fs::write(root.join("pos.nv"), "fn main() {}").unwrap();
        // compile-error
        fs::write(root.join("ce.nv"), "// EXPECT_COMPILE_ERROR\nfn main() {}").unwrap();
        // panic
        fs::write(root.join("pan.nv"), "// EXPECT_RUNTIME_PANIC\nfn main() {}").unwrap();
        // timeout
        fs::write(root.join("to.nv"), "// EXPECT_TIMEOUT\nfn main() {}").unwrap();
        // exit
        fs::write(root.join("ex.nv"), "// EXPECT_EXIT\nfn main() {}").unwrap();
        // slow positive
        fs::write(root.join("slow_pos_slow.nv"), "fn main() {}").unwrap();

        // default: only Positive, no slow
        let sel = TestSelection::default();
        let mut out = vec![];
        walk_nv_selected(&root, &mut out, &sel).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].ends_with("pos.nv"));

        // compile-error only
        let sel_ce = TestSelection { types: [TestType::CompileError].into(), include_slow: false };
        let mut out2 = vec![];
        walk_nv_selected(&root, &mut out2, &sel_ce).unwrap();
        assert_eq!(out2.len(), 1);
        assert!(out2[0].ends_with("ce.nv"));

        // panic + positive
        let sel_pp = TestSelection { types: [TestType::Positive, TestType::Panic].into(), include_slow: false };
        let mut out3 = vec![];
        walk_nv_selected(&root, &mut out3, &sel_pp).unwrap();
        assert_eq!(out3.len(), 2);

        // full
        let sel_full = TestSelection::full();
        let mut out4 = vec![];
        walk_nv_selected(&root, &mut out4, &sel_full).unwrap();
        assert_eq!(out4.len(), 6); // all 6 files
        let _ = std::fs::remove_dir_all(&root);
    }

    /// [M-trap-tests-silent-skip-default-lane]: `walk_nv_selected` silently
    /// dropped every non-Positive/non-included-slow file — `nova test
    /// std/src/time/rt` (3 legit EXPECT_RUNTIME_PANIC trap tests, no
    /// positives) reported a bare "PASS: 0  FAIL: 0" with zero trace of why.
    /// `walk_nv_selected_ex` must report EVERY excluded file tagged with the
    /// right `LaneExclusion`, so `run_all` can turn each into a visible SKIP
    /// row (`SKIP <path> # <lane> lane — requires <hint>`).
    #[test]
    fn walk_nv_selected_ex_reports_excluded_lanes() {
        use super::{walk_nv_selected_ex, LaneExclusion, TestSelection, TestType};
        use std::fs;
        let root = std::env::temp_dir().join(format!("nova_p_trap_excl_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        fs::write(root.join("pos.nv"), "fn main() {}").unwrap();
        fs::write(root.join("ce.nv"), "// EXPECT_COMPILE_ERROR\nfn main() {}").unwrap();
        fs::write(root.join("pan.nv"), "// EXPECT_RUNTIME_PANIC\nfn main() {}").unwrap();
        fs::write(root.join("to.nv"), "// EXPECT_TIMEOUT\nfn main() {}").unwrap();
        fs::write(root.join("ex.nv"), "// EXPECT_EXIT\nfn main() {}").unwrap();
        fs::write(root.join("big_slow.nv"), "fn main() {}").unwrap();

        // Default selection (Positive-only, no slow) — a dir like
        // std/src/time/rt (only EXPECT_RUNTIME_PANIC files) must NOT look
        // like an empty/typo'd directory: every non-positive file shows up
        // in `excluded`, none silently vanish.
        let sel = TestSelection::default();
        let mut out = vec![];
        let mut excluded = vec![];
        walk_nv_selected_ex(&root, &mut out, &mut excluded, &sel).unwrap();
        assert_eq!(out.len(), 1, "only pos.nv selected: {:?}", out);
        assert!(out[0].ends_with("pos.nv"));
        assert_eq!(excluded.len(), 5, "5 files excluded, none silently dropped: {:?}", excluded);

        let find = |suffix: &str| -> LaneExclusion {
            excluded.iter().find(|(p, _)| p.ends_with(suffix))
                .unwrap_or_else(|| panic!("{} missing from excluded: {:?}", suffix, excluded))
                .1
        };
        assert_eq!(find("ce.nv"), LaneExclusion::Type(TestType::CompileError));
        assert_eq!(find("pan.nv"), LaneExclusion::Type(TestType::Panic));
        assert_eq!(find("to.nv"), LaneExclusion::Type(TestType::Timeout));
        assert_eq!(find("ex.nv"), LaneExclusion::Type(TestType::Exit));
        assert_eq!(find("big_slow.nv"), LaneExclusion::Slow);

        // Lane-name/hint text is exactly what the SKIP row prints
        // (SkipReason::LaneExcluded's description: "<lane> lane — requires <hint>").
        assert_eq!(LaneExclusion::Type(TestType::Panic).lane_name(), "runtime-panic");
        assert_eq!(LaneExclusion::Type(TestType::Panic).hint(), "--full");
        assert_eq!(LaneExclusion::Slow.lane_name(), "slow");
        assert_eq!(LaneExclusion::Slow.hint(), "--include-slow/--slow-only");

        // --full selects everything — nothing left excluded.
        let sel_full = TestSelection::full();
        let mut out_full = vec![];
        let mut excluded_full = vec![];
        walk_nv_selected_ex(&root, &mut out_full, &mut excluded_full, &sel_full).unwrap();
        assert_eq!(out_full.len(), 6);
        assert!(excluded_full.is_empty(), "full selection must exclude nothing: {:?}", excluded_full);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// №453(а): a confirmed folder-module (2+ peers declaring the same
    /// `module X`) with NO local `test "..."` block must show up as a
    /// visible SKIP row, not vanish. Before the `else` branch was added to
    /// `walk_nv_selected_ex`'s `is_folder_module` check, this directory
    /// produced zero entries in both `out` and `excluded` — indistinguishable
    /// from an empty/typo'd path (measured fallout: 31 real directories).
    #[test]
    fn walk_nv_selected_ex_reports_testless_folder_module() {
        use super::{walk_nv_selected_ex, LaneExclusion, TestSelection};
        use std::fs;
        let root = std::env::temp_dir().join(format!("nova_p453_notest_fm_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        // Two co-equal peers declaring the SAME module, neither with a
        // `test "..."` block — a real folder-module, just untested.
        fs::write(root.join("a.nv"), "module notest_mod\n\npub fn helper() -> int {\n    return 1\n}\n").unwrap();
        fs::write(root.join("b.nv"), "module notest_mod\n\npub fn helper2() -> int {\n    return 2\n}\n").unwrap();

        let sel = TestSelection::default();
        let mut out = vec![];
        let mut excluded = vec![];
        walk_nv_selected_ex(&root, &mut out, &mut excluded, &sel).unwrap();
        assert!(out.is_empty(), "nothing runnable in a testless folder-module: {:?}", out);
        assert_eq!(excluded.len(), 1, "the folder-module must show up ONCE in excluded, not vanish: {:?}", excluded);
        let (path, reason) = &excluded[0];
        assert!(path.ends_with("a.nv"), "reports the alphabetically-first peer: {:?}", path);
        assert_eq!(*reason, LaneExclusion::NoLocalTests);
        assert_eq!(reason.lane_name(), "no-tests");
        assert_eq!(reason.hint(), "a local `test \"...\"` block (nothing to run standalone)");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The exact SKIP detail string a user sees for a runtime-panic trap
    /// test excluded from the default lane (the owner-reported symptom:
    /// `std/src/time/rt/*_trap_test.nv` PASS 0 FAIL 0, no SKIP visible).
    #[test]
    fn lane_excluded_skip_reason_description() {
        use super::{LaneExclusion, SkipReason, TestType};
        let reason = SkipReason::LaneExcluded {
            lane: LaneExclusion::Type(TestType::Panic).lane_name(),
            hint: LaneExclusion::Type(TestType::Panic).hint(),
        };
        assert_eq!(reason.description(), "runtime-panic lane — requires --full");
    }

    #[test]
    fn detect_test_type_markers() {
        use super::{detect_test_type, TestType};
        use std::fs;
        let root = std::env::temp_dir().join(format!("nova_p169_det_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        let pos = root.join("pos.nv");
        fs::write(&pos, "fn main() {}").unwrap();
        assert_eq!(detect_test_type(&pos), TestType::Positive);
        let ce = root.join("ce.nv");
        fs::write(&ce, "// EXPECT_COMPILE_ERROR\nfn main() {}").unwrap();
        assert_eq!(detect_test_type(&ce), TestType::CompileError);
        let pan = root.join("pan.nv");
        fs::write(&pan, "// EXPECT_RUNTIME_PANIC\nfn main() {}").unwrap();
        assert_eq!(detect_test_type(&pan), TestType::Panic);
        let _ = std::fs::remove_dir_all(&root);
    }
}
