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

/// Капчуренный output после run с timeout. Заменяет `Output` из
/// `Command::output()` — там нет варианта «убит по таймауту».
pub struct CapturedOutput {
    pub status: Option<ExitStatus>,  // None = timeout
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed: Duration,
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
                // STDOUT, STDERR, COMPILE_WARNING allow multiple patterns.
                ExpectMarker::Stdout(_) | ExpectMarker::Stderr(_)
                | ExpectMarker::CompileWarning(_) => false,
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

fn find_clang_path(explicit: Option<&Path>) -> Option<PathBuf> {
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
    pub libs: Vec<String>,
}

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
    // Plan 83.12: net.c — compiled only when libuv is available (conditional
    // on libuv presence, added inside the libuv if-let blocks per toolchain).
    let rt_net = opts.rt_dir.join("net.c");
    let march = march_flag();

    // Plan 27 Ф.1+Ф.D: Boehm paths resolved via detect_boehm (env overrides
    // + local vcpkg + global vcpkg). На Linux/macOS Some(BoehmConfig) с
    // include_dir=Some из system path, lib_dir=None — линкер через -lgc.
    // Под Windows detect_boehm всегда даёт both Some(...).
    // Если backend = Malloc → cfg = None, paths не используются.
    let boehm_cfg = if opts.gc_kind == GcKind::Boehm {
        detect_boehm(opts.cg_include)
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
            // Plan 22 libuv (cross-platform).
            if let (Some(inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                c.arg("-DNOVA_USE_LIBUV=1");
                c.arg("-I").arg(inc_path);
                // Plan 83.12: net.c compiled only when libuv is present.
                c.arg(&rt_net);
                // Windows: libuv link via -L/-l flags (env has LIB set by vcvars).
                #[cfg(target_os = "windows")]
                {
                    c.arg(lib_path);
                    c.arg(evloop);
                    for syslib in LIBUV_WIN_SYSLIBS {
                        c.arg(format!("-l{}", syslib.replace(".lib", "")));
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    /* Linux ld обрабатывает .a archives только для symbols
                     * undefined в момент когда archive seen. Используем
                     * --start-group / --end-group чтобы symbols искались
                     * commutative с object files в command line. */
                    c.arg(evloop);
                    #[cfg(target_os = "linux")]
                    c.arg("-Wl,--start-group");
                    c.arg(lib_path);
                    for syslib in LIBUV_UNIX_SYSLIBS {
                        c.arg(syslib);
                    }
                    #[cfg(target_os = "linux")]
                    c.arg("-Wl,--end-group");
                }
            }
            // Plan 27 Ф.1+Ф.D: Boehm link flags for Clang.
            if opts.gc_kind == GcKind::Boehm {
                #[cfg(target_os = "windows")]
                {
                    c.arg("-I").arg(&vcpkg_include);
                    c.arg("-L").arg(&vcpkg_lib);
                    c.arg("-lgc");
                    c.arg("-latomic_ops");
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
            // Plan 115 D214 [M-115-ffi-build-pipeline]: system libs (-l) в link phase.
            if let Some(ffi) = opts.ffi {
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
                // Plan 83.12: net.c compiled only when libuv is present.
                c.arg(&rt_net);
                c.arg(evloop);
                c.arg(lib_path);
                #[cfg(target_os = "windows")]
                for syslib in LIBUV_WIN_SYSLIBS {
                    c.arg(syslib);
                }
            }
            c.arg(opts.c_file);
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
            // Plan 27 Ф.1: Boehm link flags for MSVC (after sources, before /link).
            // Plan 115 D214 [M-115-ffi-build-pipeline]: also pass user FFI libs.
            let has_link_phase = opts.gc_kind == GcKind::Boehm
                || opts.ffi.map_or(false, |f| !f.libs.is_empty());
            if has_link_phase {
                c.arg("/link");
                if opts.gc_kind == GcKind::Boehm {
                    // PathBuf-аргумент — Command экранирует сам; ручные кавычки
                    // не нужны (и вредны, см. комментарий к /Fo выше).
                    c.arg(vcpkg_lib.join("gc.lib"));
                    c.arg(vcpkg_lib.join("atomic_ops.lib"));
                }
                if let Some(ffi) = opts.ffi {
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
            c.arg("-I").arg(opts.cg_include);
            // Plan 22 libuv (Linux).
            if let (Some(inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                c.arg("-DNOVA_USE_LIBUV=1");
                c.arg("-I").arg(inc_path);
                // Plan 83.12: net.c compiled only when libuv is present.
                c.arg(&rt_net);
                c.arg(lib_path);
                c.arg(evloop);
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                for syslib in LIBUV_UNIX_SYSLIBS {
                    c.arg(syslib);
                }
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI shim flags (GCC).
            // .h shims via -include (force-include); .c via compilation unit.
            if let Some(ffi) = opts.ffi {
                for inc in &ffi.include_dirs {
                    c.arg("-I").arg(inc);
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
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI libs (GCC).
            if let Some(ffi) = opts.ffi {
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
    // Plan 27 Ф.D: graceful exit если backend = Boehm и libgc не найден.
    let _ = resolve_gc_or_exit(opts.gc_kind, opts.cg_include);
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

/// Plan 140 Ф.2 (D24 amend): per-fixture директива `// CONTRACTS off`
/// (или `// CONTRACTS enforce`) в первых 30 строках. Override codegen-
/// политики контрактов **для этого фикстура** — позволяет регрессионным
/// фикстурам Plan 140 (t5_build_policy_off) проверять `--contracts=off`
/// поведение в обычном `test-all` прогоне без отдельной CLI-команды.
/// Возвращает `Some(true)` для `off`, `Some(false)` для `enforce`,
/// `None` если директивы нет (используется build-policy из opts).
pub fn parse_contracts_policy(src: &str) -> Option<bool> {
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
            "off" => return Some(true),
            "enforce" => return Some(false),
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
                    ExpectMismatch::NoCcError { .. } => "NEG-NO-CC-ERROR",
                    ExpectMismatch::WrongCcMsg { .. } => "NEG-WRONG-CC-MSG",
                    ExpectMismatch::NoPanic { .. } => "NEG-NO-PANIC",
                    ExpectMismatch::WrongPanic { .. } => "NEG-WRONG-PANIC",
                    ExpectMismatch::WrongExit { .. } => "NEG-WRONG-EXIT",
                    ExpectMismatch::WrongStdout { .. } => "NEG-WRONG-STDOUT",
                    ExpectMismatch::WrongStderr { .. } => "NEG-WRONG-STDERR",
                    ExpectMismatch::WrongCompileWarning { .. } => "NEG-WRONG-WARN",
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
    /// Plan 140 Ф.2 (D24 amend): build-policy `--contracts=off`. Когда
    /// `true` — codegen элидирует ВСЕ контракт-проверки (legacy zero-cost).
    /// Default `false` (enforce — недоказанные проверяются в debug И release).
    pub contracts_off: bool,
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

    // Plan 27 Ф.6: AllocConstraint — check before any build work.
    let alloc_constraint = parse_alloc_constraint(&src);
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
    if let Some(required) = parse_smt_backend_requirement(&src) {
        let actual = active_smt_backend();
        if actual != required {
            return Outcome::Skipped {
                reason: SkipReason::SmtBackend { required, actual },
                elapsed: start.elapsed(),
            };
        }
    }

    // Plan 27 Б.2: per-test timeout override via EXPECT_TIMEOUT_MS.
    let effective_timeout = parse_timeout_ms(&src).unwrap_or(opts.timeout);

    // Plan 83.1 Ф.2: per-test env vars (// ENV NAME=VALUE) — applied to
    // the run step only.
    let env_vars = parse_env(&src);

    // Plan 27 Б.3: capture stdout/stderr на PASS при --verbose.
    let verbose = matches!(opts.verbosity, Verbosity::Verbose);

    let expect = parse_expect(&src);
    let find_compile_error = || expect.iter().find_map(|m| if let ExpectMarker::CompileError(p) = m { Some(p) } else { None });
    let find_cc_error      = || expect.iter().find_map(|m| if let ExpectMarker::CcError(p)      = m { Some(p) } else { None });
    let find_runtime_panic = || expect.iter().find_map(|m| if let ExpectMarker::RuntimePanic(p) = m { Some(p) } else { None });
    let find_exit_code     = || expect.iter().find_map(|m| if let ExpectMarker::ExitCode(n)     = m { Some(*n) } else { None });
    let find_stdout        = || expect.iter().filter_map(|m| if let ExpectMarker::Stdout(p)     = m { Some(p.as_str()) } else { None }).collect::<Vec<_>>();
    let find_stderr        = || expect.iter().filter_map(|m| if let ExpectMarker::Stderr(p)     = m { Some(p.as_str()) } else { None }).collect::<Vec<_>>();
    // Plan 52 Ф.9: multi-pattern EXPECT_COMPILE_WARNING для NaN/dup-key
    // и других lint-warning сверок.
    let find_compile_warnings = || expect.iter().filter_map(|m| if let ExpectMarker::CompileWarning(p) = m { Some(p.as_str()) } else { None }).collect::<Vec<_>>();

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
    // codegen_to_c returns Ok((codegen_warns, lint_warns)) on success,
    // Err(msg) on compile error. codegen_warnings — lints от CEmitter
    // (anonymous-embed override etc); lint_warnings — от lints::lint_module
    // (Plan 52 Ф.9: NaN-key, duplicate-map-key, и др. для
    // EXPECT_COMPILE_WARNING сверки).
    // Plan 48 Ф.7.6: mono_depth прокинут через opts (None = default 500).
    // Plan 140 Ф.2: contracts_off прокинут через opts (build-policy opt-out).
    // Per-fixture `// CONTRACTS off|enforce` директива переопределяет
    // build-policy для этого фикстура (regression-guard t5_build_policy_off).
    let contracts_off = parse_contracts_policy(&src).unwrap_or(opts.contracts_off);
    let codegen_result = codegen_to_c(opts.nv_file, &src, opts.mono_depth, contracts_off);
    let codegen_warnings: Vec<String> = match &codegen_result {
        Ok((ws, _)) => ws.clone(),
        Err(_) => vec![],
    };
    let lint_warnings: Vec<String> = match &codegen_result {
        Ok((_, ls)) => ls.clone(),
        Err(_) => vec![],
    };
    let cg_warn_str: String = codegen_warnings.join("\n");

    // EXPECT_COMPILE_ERROR — handled на этапе codegen.
    if let Some(pat) = find_compile_error() {
        return match &codegen_result {
            Ok(_) => Outcome::Fail {
                stage: Stage::Expectation {
                    mismatch: ExpectMismatch::NoCompileError { expected_pat: pat.clone() },
                },
                elapsed: start.elapsed(),
            },
            Err(msg) => {
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

    let c_file = opts.nv_file.with_extension("c");
    if !c_file.is_file() {
        return Outcome::Fail { stage: Stage::NoCFile, elapsed: start.elapsed() };
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
    let resolved_ffi: Option<ResolvedFfiConfig> = {
        let manifest = crate::manifest::find_manifest(opts.nv_file);
        manifest.and_then(|m| m.ffi.map(|cfg| {
            let base = m.source_root.clone();
            ResolvedFfiConfig {
                c_shims: cfg.c_shims.iter()
                    .map(|p| base.join(p))
                    .collect(),
                include_dirs: cfg.include_dirs.iter()
                    .map(|p| base.join(p))
                    .collect(),
                libs: cfg.libs.clone(),
            }
        }))
    };

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

    let (cc_captured, cc_status) = 'cc: {
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

    if !cc_status.success() {
        let combined = format!(
            "{}{}",
            bytes_to_string(&cc_captured.stdout),
            bytes_to_string(&cc_captured.stderr)
        );
        let errs: Vec<&str> = combined
            .lines()
            .filter(|l| l.to_lowercase().contains("error"))
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
    // Plan 83.1 Ф.2: apply `// ENV NAME=VALUE` directives to the test exe.
    for (key, val) in &env_vars {
        run_cmd.env(key, val);
    }
    let run_captured = match run_with_timeout(run_cmd, effective_timeout) {
        Ok(o) => o,
        Err(e) => {
            return Outcome::Fail {
                stage: Stage::Run { error: format!("spawn exe: {}", e) },
                elapsed: start.elapsed(),
            };
        }
    };
    // Plan 169.1 Ф.1: capture run_ms immediately after execution completes.
    let run_ms = run_start.elapsed().as_millis();
    let stdout = bytes_to_string(&run_captured.stdout);
    let stderr = bytes_to_string(&run_captured.stderr);
    let run_status = match run_captured.status {
        Some(s) => s,
        None => return Outcome::Timeout { elapsed: start.elapsed() },
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
                // Prefer lines that actually name the failure (FAIL/assert/panic);
                // the in-binary harness prints many PASS lines then a summary, so
                // a blind "last 3 lines" only shows the trailing PASS + count and
                // hides WHICH test failed. Fall back to last-3 if none match.
                let fail_lines: Vec<&str> = stdout.lines().chain(stderr.lines())
                    .filter(|l| {
                        let lc = l.to_lowercase();
                        lc.contains("fail") || lc.contains("assert") || lc.contains("panic")
                    })
                    .take(4)
                    .collect();
                let detail = if !fail_lines.is_empty() {
                    fail_lines.join(" | ")
                } else {
                    let last_lines: Vec<&str> = stdout.lines().chain(stderr.lines()).rev().take(3).collect();
                    last_lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
                };
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
        let parent = dir.parent()?.to_path_buf();
        if parent == dir {
            // Reached filesystem root.
            return last_toml_dir;
        }
        dir = parent;
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

/// Plan 52 Ф.9: возвращает `(codegen_warnings, lint_warnings)` — последние
/// используются для `EXPECT_COMPILE_WARNING` сверки. Lints вызываются
/// после type-check, ДО desugar — иначе MapLit/RecordLit-узлы уже
/// заменены на Block'и, и lint check_map_literal_lints не сработает.
///
/// Plan 48 Ф.7.6: `mono_depth` — optional CLI override для
/// CEmitter.mono_depth_limit (None = default из env var или 500).
fn codegen_to_c(path: &Path, src: &str, mono_depth: Option<usize>, contracts_off: bool) -> Result<(Vec<String>, Vec<String>), String> {
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
    // Bug fix 2026-06-01: emit W_D78_REV1_DEPRECATED warning instead of
    // silent acceptance для rev-1 legacy declarations.
    match manifest::check_module_path_with_kind(path, &module.name, is_folder_module) {
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
    let sig_table_opt: Option<crate::imports::ModuleSigTable> =
        if let Some(repo) = find_repo_root_from(path) {
            let stdlib_dir = crate::manifest::resolve_std_path(repo.as_ref());
            let _t = crate::perf_timer::PerfTimer::new("imports-resolve");
            // Plan 42 правило F: test mode = include `*_test.nv` peers.
            crate::imports::resolve_imports_inline_ex(path, &mut module, &repo, &stdlib_dir, true)
                .map_err(|e| format!("import resolution: {}", e))?;
            // Collect signatures AFTER imports are resolved so all imported
            // items are present in module.imports for the sig-walk.
            Some(
                crate::imports::collect_all_signatures(path, &module, &repo, &stdlib_dir)
                    .unwrap_or_else(|_| crate::imports::ModuleSigTable::new()),
            )
        } else {
            None
        };

    // Plan 172.1 U.4.1: number every expr of the FULLY-ASSEMBLED module
    // (after import inlining via resolve_imports_inline_ex above, before
    // type-check) so check_module can annotate ModuleEnv.resolved_types and
    // codegen READS it instead of re-deriving (`infer_expr_c_type`, §0/§1).
    // Must run post-inline: numbering the merged module once yields globally
    // unique ids (per-peer parse would restart at 1 → folder-module collisions).
    let resolved_types = crate::number_exprs::number_exprs(&mut module);

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
                .map(|d| d.render(src, &path.to_string_lossy()))
                .collect::<Vec<_>>()
                .join("\n")
        })?
    };
    // Plan 172.1 U.4.1: hand the literal resolved-type seed to ModuleEnv.
    // Plan 172.1 U.4.4(b): merge the checker's semantic Ident annotations OVER the seed.
    let checker_annotations = std::mem::take(&mut module_env.resolved_types);
    module_env.resolved_types = resolved_types;
    module_env.resolved_types.extend(checker_annotations);
    // Plan 52 Ф.9: lints — ПОСЛЕ check_module (типы validated), ДО
    // desugar (lints видят MapLit-узлы). Возвращаются caller'у для
    // EXPECT_COMPILE_WARNING сверки.
    let mut lint_warnings: Vec<String> = {
        let _t = crate::perf_timer::PerfTimer::new("lints");
        crate::lints::lint_module(&module)
            .iter()
            .map(|w| w.diag.render(src, &path.to_string_lossy()))
            .collect()
    };
    // Ф.7.4 (Plan 33.6): verify-warnings (W2401/W2402) тоже dispatch'им в lint stream.
    // Plan 140 Ф.3: proven contracts уже получены через `module_env` выше
    // (check_module → VerificationPipeline). Этот вызов остаётся ТОЛЬКО ради
    // verify-warnings, которые check_module глушит (types/mod.rs: `report.warnings`
    // intentionally silent). Proven set здесь намеренно НЕ используется.
    {
        let _t = crate::perf_timer::PerfTimer::new("verify");
        let verify_report = crate::verify::verify_module(&module);
        for w in &verify_report.warnings {
            lint_warnings.push(w.render(src, &path.to_string_lossy()));
        }
    }
    {
        // Plan 114.4.2 (D199) Ф.3: const fn AST rewrite + codegen drop.
        // Runs ПЕРЕД annotate-maps/desugar чтобы они уже видели literals.
        let _t = crate::perf_timer::PerfTimer::new("const-fn-rewrite");
        let cfn_errs = crate::const_fn_eval::rewrite_const_fn_calls(&mut module);
        if !cfn_errs.is_empty() {
            return Err(cfn_errs.iter()
                .map(|d| d.render(src, &path.to_string_lossy()))
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
                .map(|d| d.render(src, &path.to_string_lossy()))
                .collect::<Vec<_>>()
                .join("\n"));
        }
    }
    {
        // Plan 126.2 Ф.2: synthesize built-in protocol methods (Equatable/
        // Hashable/Cloneable/Comparable/Printable) for `#impl(P)` types and
        // inject them into module.items as Item::Fn, so codegen emits C bodies
        // and operator dispatch (`==`/`<`/`.clone()`/...) resolves them.
        // Runs AFTER check_module (impl_protocols validated), BEFORE desugar/
        // codegen. User-explicit methods always win (never overwritten).
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
        crate::callnorm::normalize_module(&mut module);
    crate::chain_norm::normalize_chains_module(&mut module);
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

    let (c_code, warnings) = {
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
        // Plan 140 Ф.2 (D24 amend): build-policy `--contracts=off` элидирует
        // все контракт-проверки на codegen (legacy zero-cost). Default enforce.
        emitter.set_contracts_off(contracts_off);
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
        // Plan 172.1 U.4.3: feed the resolved-callee channel (ExprId → chosen callee
        // FnDecl.span) so codegen reads its OWN view of the chosen callee instead of
        // re-resolving the overload (§0). Stage (a): equivalence-assert (debug).
        emitter.set_resolved_callees(&module_env.resolved_callees);
        emitter.emit_module(&module)
            .map_err(|e| format!("codegen error: {}", e))?
    };
    let out_path = path.with_extension("c");
    std::fs::write(&out_path, &c_code).map_err(|e| {
        format!(
            "failed to write {}: {}",
            out_path.display(),
            e
        )
    })?;
    Ok((warnings, lint_warnings))
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
    /// Plan 140 Ф.2 (D24 amend): build-policy `--contracts=off` — элидировать
    /// все контракт-проверки на codegen для всего прогона (legacy zero-cost).
    /// Propagated to every per-test TestBuildOpts. Default `false` (enforce).
    pub contracts_off: bool,
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

/// Plan 27 Ф.D: если backend = Boehm, проверяет наличие через detect_boehm.
/// На fail печатает platform-specific install hint и завершает процесс.
/// Возвращает Some(BoehmConfig) если backend = Boehm и detection OK,
/// None если backend = Malloc (Boehm не нужен).
pub fn resolve_gc_or_exit(gc: GcKind, cg_include: &Path) -> Option<BoehmConfig> {
    if gc != GcKind::Boehm {
        return None;
    }
    if let Some(cfg) = detect_boehm(cg_include) {
        return Some(cfg);
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
         \n\
         To fix:\n\
           cd compiler-codegen\n\
           vcpkg install bdwgc:x64-windows-static\n\
         \n\
         Or use --gc malloc for benchmarks (no GC, leaks).",
        cg_include.display()
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
        std::fs::write(&rsp, lines.join("\n"))
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
        std::fs::write(&lib_rsp, lib_lines.join("\n"))
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

fn collect_c_files(dir: &Path, out: &mut Vec<PathBuf>, recursive: bool) -> Result<()> {
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

/// Plan 156: per-file slow-test suffix. A test file whose stem ends in
/// `_slow` (e.g. `collation_conformance_slow.nv`) is a large/slow test,
/// excluded from the default run, included only via --include-slow/--slow-only.
/// Peeled BEFORE `_test` and the OS-suffix (canonical `<core>[_<os>][_test][_slow]`)
/// so it composes with them. Zero per-file I/O: matched on the dirent name in
/// `walk_nv_filtered` — the file body is never read at default discovery.
pub fn is_slow_file_stem(stem: &str) -> bool { stem.ends_with("_slow") }

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
        if line.contains("EXPECT_TIMEOUT")       { return TestType::Timeout; }
        if line.contains("EXPECT_EXIT")           { return TestType::Exit; }
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
                let stem_no_slow = stem.strip_suffix("_slow").unwrap_or(stem);
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
        // Folder-module без test-блоков — библиотека, пропускаем как раньше.
        if folder_module_has_tests(&direct_nv) {
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
        walk_nv_filtered(&sub, out, lane)?;
    }
    Ok(())
}

/// Plan 169.1.1: Like `walk_nv_filtered` but uses `TestSelection` to filter by type
/// (EXPECT_* marker) AND slow-file suffix. Reads file header only for type detection.
pub fn walk_nv_selected(root: &Path, out: &mut Vec<PathBuf>, sel: &TestSelection) -> Result<()> {
    if root.is_file() {
        let stem = root.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let is_slow = is_slow_file_stem(stem);
        if is_slow && !sel.include_slow {
            return Ok(());
        }
        let test_type = detect_test_type(root);
        if sel.types.contains(&test_type) {
            out.push(root.to_path_buf());
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
                if is_slow && !sel.include_slow { continue; }
                let stem_no_slow = stem.strip_suffix("_slow").unwrap_or(stem);
                let core_stem = stem_no_slow.strip_suffix("_test").unwrap_or(stem_no_slow);
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
                }
            }
        }
    } else {
        for p in direct_nv {
            let test_type = detect_test_type(&p);
            if sel.types.contains(&test_type) {
                out.push(p);
            }
        }
    }
    for sub in sub_dirs {
        walk_nv_selected(&sub, out, sel)?;
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
                let trunc: String = detail.chars().take(120).collect();
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

    // Plan 27 Ф.D (audit 2026-05-12): early Boehm detection с graceful exit
    // если backend = Boehm и gc.lib/libgc не найден. Без этого юзер получает
    // cryptic linker error для каждого теста.
    let _ = resolve_gc_or_exit(opts.gc_kind, opts.cg_include);

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
    for dir_or_file in effective_dirs {
        if dir_or_file.is_file() {
            inputs.push(dir_or_file.clone());
        } else {
            let mut found = Vec::new();
            walk_nv_selected(dir_or_file, &mut found, &opts.selection)?;
            inputs.extend(found);
        }
    }
    // Стабильный порядок по пути — shuffle потом переопределит если нужно.
    inputs.sort();

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

    // Build job list applying all filters.
    let mut jobs: Vec<(String, PathBuf)> = Vec::new();
    for nv_path in &inputs {
        let display = display_name(nv_path, &cwd);
        if let Some(filter) = opts.filter {
            if !display.contains(filter) { continue; }
        }
        // Plan 36.D: --skip применяется к display name И к raw path string
        // (для skip типа `std/runtime/` который может не попадать в display).
        if !opts.skip.is_empty() {
            let path_str = nv_path.to_string_lossy().replace('\\', "/");
            let skip_match = opts.skip.iter().any(|pat| {
                !pat.is_empty() && (display.contains(pat.as_str()) || path_str.contains(pat.as_str()))
            });
            if skip_match { continue; }
        }
        if let Some(set) = &filter_from_set {
            if !set.contains(&display) { continue; }
        }
        if let Some(set) = &rerun_set {
            if !set.contains(&display) { continue; }
        }
        jobs.push((display, nv_path.clone()));
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
        for (display, _) in &jobs {
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
            let contracts_off = opts.contracts_off;

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
                let (display, nv_path) = &jobs[idx];
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
                    contracts_off,
                };
                // Plan 26 Ф.12: retry для transient AV/linker race fails.
                // Exponential backoff: 100ms, 200ms, 400ms.
                // Plan 26 Ф.17 #1: cumulative elapsed.
                let retry_start = Instant::now();
                // Plan 169.1 Ф.1: split timing output from run_one.
                let mut split: (u128, u128) = (0, 0);
                let mut outcome = run_one(&test_opts, &mut split);
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
                    let trunc: String = detail.chars().take(120).collect();
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
        let result = codegen_to_c(&nv_path, &src, None, false);
        assert!(result.is_ok(), "P3-B vtable dispatch: codegen должен успешно скомпилировать, но: {:?}", result.err());
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
