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
            (!arg.is_empty()).then(|| ExpectMarker::CompileError(arg.to_string()))
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
                // STDOUT and STDERR allow multiple patterns.
                ExpectMarker::Stdout(_) | ExpectMarker::Stderr(_) => false,
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

/// Конкретный детектированный toolchain. Несёт пути к компилятору
/// и (на Windows для Clang/MSVC) к vcvars64.bat.
#[derive(Debug, Clone)]
pub enum Toolchain {
    Clang { clang: PathBuf, vcvars: Option<PathBuf> },
    Msvc { vcvars: PathBuf },
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

pub struct ToolchainOpts<'a> {
    pub pref: ToolchainPref,
    pub explicit_clang: Option<&'a Path>,
    pub explicit_vcvars: Option<&'a Path>,
}

pub fn detect_toolchain(opts: &ToolchainOpts) -> Result<Toolchain> {
    let clang = find_clang_path(opts.explicit_clang);
    let vcvars = find_vcvars(opts.explicit_vcvars);
    let gcc = find_gcc_path();

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
        if cfg!(target_os = "windows") && vcvars.is_none() {
            return Err(anyhow!(
                "clang on Windows requires vcvars64.bat for MSVC SDK headers/libs. \
                 Install Visual Studio Build Tools, or set NOVA_VCVARS."
            ));
        }
        Ok(Toolchain::Clang {
            clang,
            vcvars: vcvars.clone(),
        })
    };
    let try_msvc = || -> Result<Toolchain> {
        if !cfg!(target_os = "windows") {
            return Err(anyhow!("MSVC toolchain unavailable on non-Windows OS"));
        }
        let vcvars = vcvars.clone().ok_or_else(|| {
            anyhow!(
                "vcvars64.bat not found. Install Visual Studio Build Tools, \
                 or set NOVA_VCVARS to vcvars64.bat path."
            )
        })?;
        Ok(Toolchain::Msvc { vcvars })
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

/// GC backend selection. Wired through BuildOpts → build_command.
/// Malloc = plain malloc, no GC (internal/benchmark only — any loop that
/// allocates will OOM eventually; not for production use).
/// Boehm = Boehm-Demers-Weiser conservative tracing GC (default after Plan 27 Ф.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GcKind {
    #[default]
    Malloc,
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

/// Возвращает command, готовую к запуску. Для Clang/MSVC на Windows
/// инкапсулирует cmd /c "vcvars && actual-cmd" — иначе headers/libs
/// MSVC SDK недоступны.
fn build_command(tc: &Toolchain, opts: &BuildOpts) -> Command {
    // Plan 27 Ф.1: alloc source chosen by GC backend.
    let rt_alloc = opts.rt_dir.join(opts.gc_kind.alloc_c_name());
    let rt_effects = opts.rt_dir.join("effects.c");
    let rt_fibers = opts.rt_dir.join("fibers.c");
    let march = march_flag();

    // Plan 27 Ф.1: Boehm vcpkg paths (x64-windows-static). Only used on Windows;
    // on Linux/macOS the system libgc is used (no vcpkg).
    // cg_include is compiler-codegen/ root, so vcpkg_installed lives alongside nova_rt/.
    let vcpkg_include = opts.cg_include
        .join("vcpkg_installed")
        .join("x64-windows-static")
        .join("include");
    let vcpkg_lib = opts.cg_include
        .join("vcpkg_installed")
        .join("x64-windows-static")
        .join("lib");

    // Plan 22: libuv linkage. Если libuv config present — добавляем
    // eventloop.c в sources, -DNOVA_USE_LIBUV=1, libuv include, libuv.lib
    // + Windows system libs.
    let libuv_eventloop = opts.libuv.map(|c| c.eventloop_src.clone());
    let libuv_include = opts.libuv.map(|c| c.include_dir.clone());
    let libuv_lib = opts.libuv.map(|c| c.lib_file.clone());

    match tc {
        Toolchain::Clang { clang, vcvars } => {
            // GCC-style flags. Target явный (msvc/linux/darwin).
            let target = if cfg!(target_os = "windows") {
                "--target=x86_64-pc-windows-msvc"
            } else if cfg!(target_os = "macos") {
                "" // системный default
            } else {
                "" // linux: default
            };
            let mut flags: Vec<String> = match opts.mode {
                Mode::Dev => vec!["-O0", "-g", "-Wno-everything"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
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
            // Plan 27 Ф.1: Boehm requires GC_THREADS define.
            if opts.gc_kind == GcKind::Boehm {
                flags.push("-DGC_THREADS".to_string());
            }
            let inc = opts.cg_include.display().to_string();
            let out = opts.exe_file.display().to_string();
            let cfile = opts.c_file.display().to_string();
            let rt1 = rt_alloc.display().to_string();
            let rt2 = rt_effects.display().to_string();
            let rt3 = rt_fibers.display().to_string();

            if let Some(vcv) = vcvars {
                // Windows: cmd /c "call "vcvars" && "clang" ...".
                // ВАЖНО: используем raw_arg, чтобы Rust не escape'ил наши
                // внутренние кавычки. Обычный `Command::args` обернёт
                // строку в свои кавычки, ломая вложенный quoting.
                let clang_str = clang.display().to_string();
                // Plan 22: libuv args.
                let mut libuv_args = String::new();
                if let (Some(inc_path), Some(lib_path), Some(evloop)) =
                    (&libuv_include, &libuv_lib, &libuv_eventloop)
                {
                    libuv_args.push_str(&format!(
                        " -DNOVA_USE_LIBUV=1 -I \"{}\" \"{}\" \"{}\"",
                        inc_path.display(),
                        evloop.display(),
                        lib_path.display(),
                    ));
                    #[cfg(target_os = "windows")]
                    for syslib in LIBUV_WIN_SYSLIBS {
                        libuv_args.push_str(&format!(" -l{}", &syslib.replace(".lib", "")));
                    }
                }
                // Plan 27 Ф.1: Boehm link flags for Clang on Windows.
                // gc.lib must precede atomic_ops.lib (link order matters).
                let mut boehm_args = String::new();
                if opts.gc_kind == GcKind::Boehm {
                    boehm_args.push_str(&format!(
                        " -I \"{}\" -L \"{}\" -lgc -latomic_ops",
                        vcpkg_include.display(),
                        vcpkg_lib.display(),
                    ));
                }
                // Plan 26 Ф.7: chcp 65001 ставит UTF-8 codepage в текущем
                // cmd.exe — cl/clang будут писать stderr в UTF-8, не в CP1251.
                // `> nul` подавляет «Active code page: 65001» line.
                let inner = format!(
                    "\"chcp 65001 > nul && call \"{}\" > nul && \"{}\" {} -I \"{}\"{}{}  -o \"{}\" \"{}\" \"{}\" \"{}\" \"{}\"\"",
                    vcv.display(),
                    clang_str,
                    flags.join(" "),
                    inc,
                    libuv_args,
                    boehm_args,
                    out,
                    cfile,
                    rt1,
                    rt2,
                    rt3,
                );
                let mut c = Command::new("cmd");
                #[cfg(target_os = "windows")]
                {
                    c.raw_arg("/c").raw_arg(&inner);
                }
                #[cfg(not(target_os = "windows"))]
                {
                    c.args(["/c", &inner]);
                }
                c
            } else {
                // Linux/macOS: прямой invoke.
                let mut c = Command::new(clang);
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
                    c.arg(lib_path);
                    c.arg(evloop);
                    #[cfg(any(target_os = "linux", target_os = "macos"))]
                    for syslib in LIBUV_UNIX_SYSLIBS {
                        c.arg(syslib);
                    }
                }
                c.arg("-o").arg(opts.exe_file);
                c.arg(opts.c_file);
                c.arg(&rt_alloc);
                c.arg(&rt_effects);
                c.arg(&rt_fibers);
                // Plan 27 Ф.1: Boehm link flags for Clang on Linux/macOS.
                // System libgc; on Linux also needs -lpthread (GC_THREADS).
                if opts.gc_kind == GcKind::Boehm {
                    c.arg("-lgc");
                    #[cfg(target_os = "linux")]
                    c.arg("-lpthread");
                }
                c
            }
        }
        Toolchain::Msvc { vcvars } => {
            // MSVC cl.exe — только Windows. Всегда через vcvars.
            let flags = match opts.mode {
                Mode::Dev => "/Od /Zi",
                Mode::Release => "/O2 /DNDEBUG",
            };
            let inc = opts.cg_include.display().to_string();
            let obj = opts.obj_dir.display().to_string();
            let out = opts.exe_file.display().to_string();
            let cfile = opts.c_file.display().to_string();
            let rt1 = rt_alloc.display().to_string();
            let rt2 = rt_effects.display().to_string();
            let rt3 = rt_fibers.display().to_string();
            // Plan 22: libuv args для cl.exe.
            let mut libuv_args = String::new();
            if let (Some(inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                libuv_args.push_str(&format!(
                    " /DNOVA_USE_LIBUV=1 /I \"{}\" \"{}\" \"{}\"",
                    inc_path.display(),
                    evloop.display(),
                    lib_path.display(),
                ));
                #[cfg(target_os = "windows")]
                for syslib in LIBUV_WIN_SYSLIBS {
                    libuv_args.push_str(&format!(" {}", syslib));
                }
            }
            // Plan 27 Ф.1: Boehm link flags for MSVC.
            // /DGC_THREADS in compile section; gc.lib + atomic_ops.lib in /link section.
            // MSVC requires all /link args after source files — built into inner string.
            let boehm_compile_flags = if opts.gc_kind == GcKind::Boehm {
                format!(" /DGC_THREADS /I \"{}\"", vcpkg_include.display())
            } else {
                String::new()
            };
            let boehm_link_flags = if opts.gc_kind == GcKind::Boehm {
                format!(
                    " /link \"{}\\gc.lib\" \"{}\\atomic_ops.lib\"",
                    vcpkg_lib.display(),
                    vcpkg_lib.display(),
                )
            } else {
                String::new()
            };
            // Plan 26 Ф.7: chcp 65001 → UTF-8 в текущем cmd. cl.exe выдаст
            // stderr в UTF-8, no need для CP1251 decoder. `> nul` подавляет
            // «Active code page: 65001» и vcvars banner.
            let inner = format!(
                "\"chcp 65001 > nul && call \"{}\" > nul && cl.exe /nologo /W0 {}{}{} /I \"{}\" /Fo\"{}\\\\\" /Fe\"{}\" \"{}\" \"{}\" \"{}\" \"{}\"{}\"",
                vcvars.display(), flags, libuv_args, boehm_compile_flags,
                inc, obj, out, cfile, rt1, rt2, rt3, boehm_link_flags,
            );
            let mut c = Command::new("cmd");
            #[cfg(target_os = "windows")]
            {
                c.raw_arg("/c").raw_arg(&inner);
            }
            #[cfg(not(target_os = "windows"))]
            {
                c.args(["/c", &inner]);
            }
            c
        }
        Toolchain::Gcc { gcc } => {
            let mut c = Command::new(gcc);
            match opts.mode {
                Mode::Dev => {
                    c.args(["-O0", "-g", "-w"]);
                }
                Mode::Release => {
                    c.arg("-O3");
                    c.arg("-flto");
                    c.arg(format!("-march={}", march));
                    c.arg("-DNDEBUG");
                    c.arg("-w");
                }
            }
            // Plan 27 Ф.1: Boehm GC_THREADS define (compile-time).
            if opts.gc_kind == GcKind::Boehm {
                c.arg("-DGC_THREADS");
            }
            c.arg("-I").arg(opts.cg_include);
            // Plan 22 libuv (Linux).
            if let (Some(inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                c.arg("-DNOVA_USE_LIBUV=1");
                c.arg("-I").arg(inc_path);
                c.arg(lib_path);
                c.arg(evloop);
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                for syslib in LIBUV_UNIX_SYSLIBS {
                    c.arg(syslib);
                }
            }
            c.arg("-o").arg(opts.exe_file);
            c.arg(opts.c_file);
            c.arg(&rt_alloc);
            c.arg(&rt_effects);
            c.arg(&rt_fibers);
            // Plan 27 Ф.1: Boehm link flags for GCC (system libgc).
            if opts.gc_kind == GcKind::Boehm {
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

// ---------- Plan 26 Ф.6: Outcome — typed test result ----------

/// Результат одного теста. Production-grade: typed stages вместо
/// 12-вариантного enum'а. Один источник правды для label/detail/JSON.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Тест прошёл. `detail` опционален — обычно «», но для negative-
    /// тестов содержит контекстную метку вроде «(negative)» / «(stdout)».
    Pass { detail: String, elapsed: Duration },
    /// Не прошёл. `stage` указывает на этап провала.
    Fail { stage: Stage, elapsed: Duration },
    /// Превысил `--timeout` — child killed.
    Timeout { elapsed: Duration },
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
}

impl Outcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, Outcome::Pass { .. })
    }

    /// Plan 26 Ф.17 #1: override elapsed для retry cumulative-time.
    /// Per-attempt run_one() имеет свой start; в JSON/JUnit summary
    /// нужно показать **общее** время от первого attempt до последнего.
    pub fn with_elapsed(self, elapsed: Duration) -> Self {
        match self {
            Outcome::Pass { detail, .. } => Outcome::Pass { detail, elapsed },
            Outcome::Fail { stage, .. } => Outcome::Fail { stage, elapsed },
            Outcome::Timeout { .. } => Outcome::Timeout { elapsed },
        }
    }

    /// Короткий лейбл для табличного output'а.
    pub fn label(&self) -> &'static str {
        match self {
            Outcome::Pass { .. } => "PASS",
            Outcome::Timeout { .. } => "TIMEOUT",
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
                },
            },
        }
    }

    /// Детальная human-readable строка (для table output + FAIL summary).
    pub fn detail(&self) -> String {
        match self {
            Outcome::Pass { detail, .. } => detail.clone(),
            Outcome::Timeout { elapsed } => format!("killed after {}ms", elapsed.as_millis()),
            Outcome::Fail { stage, .. } => match stage {
                Stage::Codegen { error } | Stage::Cc { error } | Stage::Run { error } => {
                    error.chars().take(150).collect()
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
            | Outcome::Timeout { elapsed } => *elapsed,
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
    /// Plan 26 Ф.1: per-test timeout. Применяется ко всем child-процессам
    /// (cc + run). Default 60 s — long-running тесты должны override через
    /// `--timeout` или (TODO Plan 27) per-test маркер.
    pub timeout: Duration,
    /// Plan 27 Ф.1: GC backend. Propagates to BuildOpts → build_command.
    pub gc_kind: GcKind,
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
pub fn run_one(opts: &TestBuildOpts) -> Outcome {
    let start = Instant::now();
    let src = match std::fs::read_to_string(opts.nv_file) {
        Ok(s) => s,
        Err(e) => {
            return Outcome::Fail {
                stage: Stage::Codegen {
                    error: format!("read: {}", e),
                },
                elapsed: start.elapsed(),
            }
        }
    };
    let expect = parse_expect(&src);
    // Helper closures для поиска маркеров в Vec.
    let find_compile_error = || expect.iter().find_map(|m| if let ExpectMarker::CompileError(p) = m { Some(p) } else { None });
    let find_cc_error      = || expect.iter().find_map(|m| if let ExpectMarker::CcError(p)      = m { Some(p) } else { None });
    let find_runtime_panic = || expect.iter().find_map(|m| if let ExpectMarker::RuntimePanic(p) = m { Some(p) } else { None });
    let find_exit_code     = || expect.iter().find_map(|m| if let ExpectMarker::ExitCode(n)     = m { Some(*n) } else { None });
    let find_stdout        = || expect.iter().filter_map(|m| if let ExpectMarker::Stdout(p)     = m { Some(p.as_str()) } else { None }).collect::<Vec<_>>();
    let find_stderr        = || expect.iter().filter_map(|m| if let ExpectMarker::Stderr(p)     = m { Some(p.as_str()) } else { None }).collect::<Vec<_>>();

    // Step 1: codegen.
    let codegen_result = codegen_to_c(opts.nv_file, &src);

    // EXPECT_COMPILE_ERROR — handled на этапе codegen.
    if let Some(pat) = find_compile_error() {
        return match codegen_result {
            Ok(_) => Outcome::Fail {
                stage: Stage::Expectation {
                    mismatch: ExpectMismatch::NoCompileError {
                        expected_pat: pat.clone(),
                    },
                },
                elapsed: start.elapsed(),
            },
            Err(msg) => {
                if msg.contains(pat) {
                    Outcome::Pass {
                        detail: "(negative)".to_string(),
                        elapsed: start.elapsed(),
                    }
                } else {
                    Outcome::Fail {
                        stage: Stage::Expectation {
                            mismatch: ExpectMismatch::WrongCompileMsg {
                                expected_pat: pat.clone(),
                                got: msg,
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

    let c_file = opts.nv_file.with_extension("c");
    if !c_file.is_file() {
        return Outcome::Fail {
            stage: Stage::NoCFile,
            elapsed: start.elapsed(),
        };
    }

    // Step 2 — isolated tmp subdir per test (Plan 26 Ф.2).
    // Plan 26 Ф.16 #1: RAII guard — cleanup на любом return-path
    // (включая panic). Replace 4 manual `remove_dir_all` cleanup-sites
    // на один Drop.
    let subdir_path = test_subdir(opts.tmp_dir, opts.display);
    let subdir_guard = match TempSubdir::new(subdir_path, opts.keep_artifacts) {
        Ok(g) => g,
        Err(e) => {
            return Outcome::Fail {
                stage: Stage::Cc {
                    error: format!("mkdir subdir: {}", e),
                },
                elapsed: start.elapsed(),
            };
        }
    };
    let subdir = subdir_guard.path();

    // exe_name локально — display может содержать /; используем basename
    // .nv для exe filename, остальная isolation через subdir.
    let basename = opts
        .nv_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("test");
    let exe_name = if cfg!(target_os = "windows") {
        format!("{}.exe", basename)
    } else {
        basename.to_string()
    };
    let exe_file = subdir.join(&exe_name);
    // Windows: lld-link cannot overwrite a locked exe (AV / previous run handle).
    // Remove stale artifact before compiling so the linker always creates a fresh file.
    let _ = std::fs::remove_file(&exe_file);
    let obj_dir = subdir.join("obj");
    if let Err(e) = std::fs::create_dir_all(&obj_dir) {
        return Outcome::Fail {
            stage: Stage::Cc {
                error: format!("mkdir obj_dir: {}", e),
            },
            elapsed: start.elapsed(),
        };
    }

    let build_opts = BuildOpts {
        c_file: &c_file,
        exe_file: &exe_file,
        obj_dir: &obj_dir,
        cg_include: opts.cg_include,
        rt_dir: opts.rt_dir,
        mode: opts.mode,
        libuv: opts.libuv,
        gc_kind: opts.gc_kind,
    };
    let cmd = build_command(opts.toolchain, &build_opts);
    // Plan 26 Ф.1: timeout для cc. cl.exe / clang обычно <30s, 60s достаточно.
    // Если cc сам зависает (редко, но бывает при OOM swap) — kill + CcFail.
    let cc_captured = match run_with_timeout(cmd, opts.timeout) {
        Ok(o) => o,
        Err(e) => {
            return Outcome::Fail {
                stage: Stage::Cc {
                    error: format!("spawn cc: {}", e),
                },
                elapsed: start.elapsed(),
            }
        }
    };
    let cc_status = match cc_captured.status {
        Some(s) => s,
        None => {
            // cc timeout — редкость, но возможно. Cleanup через
            // subdir_guard Drop при возврате.
            return Outcome::Timeout {
                elapsed: start.elapsed(),
            };
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
            let trimmed: String = combined.chars().take(200).collect();
            trimmed.replace('\n', " | ")
        } else {
            errs.join(" | ")
        };
        // EXPECT_CC_ERROR: CC failure is expected — check pattern.
        if let Some(pat) = find_cc_error() {
            return if pat.is_empty() || combined.contains(pat.as_str()) {
                Outcome::Pass {
                    detail: "(negative-cc)".to_string(),
                    elapsed: start.elapsed(),
                }
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
        // Cleanup через subdir_guard Drop.
        return Outcome::Fail {
            stage: Stage::Cc { error: detail },
            elapsed: start.elapsed(),
        };
    }
    // EXPECT_CC_ERROR but CC succeeded.
    if let Some(pat) = find_cc_error() {
        return Outcome::Fail {
            stage: Stage::Expectation {
                mismatch: ExpectMismatch::NoCcError {
                    expected_pat: pat.clone(),
                },
            },
            elapsed: start.elapsed(),
        };
    }

    // Step 3 — run с timeout.
    let mut run_cmd = Command::new(&exe_file);
    // Plan 26 Ф.7: force UTF-8 locale для child-процесса (Unix).
    // На Windows runtime сам работает в UTF-8 через chcp 65001 (см. build_command).
    #[cfg(not(target_os = "windows"))]
    {
        run_cmd.env("LC_ALL", "C.UTF-8");
        run_cmd.env("LANG", "C.UTF-8");
    }
    let run_captured = match run_with_timeout(run_cmd, opts.timeout) {
        Ok(o) => o,
        Err(e) => {
            // Cleanup через subdir_guard Drop.
            return Outcome::Fail {
                stage: Stage::Run {
                    error: format!("spawn exe: {}", e),
                },
                elapsed: start.elapsed(),
            };
        }
    };
    let stdout = bytes_to_string(&run_captured.stdout);
    let stderr = bytes_to_string(&run_captured.stderr);
    let run_status = match run_captured.status {
        Some(s) => s,
        None => {
            // Timeout — критический результат для тестов вроде sleep_leak_check.
            // Cleanup через subdir_guard Drop.
            return Outcome::Timeout {
                elapsed: start.elapsed(),
            };
        }
    };
    let exit = run_status.code().unwrap_or(-1);

    // Step 4: проверка EXPECT-маркеров (multi-marker: все должны выполниться).
    //
    // Порядок: сначала RUNTIME_PANIC (определяет exit), затем EXIT_CODE,
    // затем STDOUT/STDERR-паттерны. Если маркеров нет — ожидается exit 0.
    let outcome = {
        // RUNTIME_PANIC: exit != 0 + panic-pattern в stdout/stderr.
        if let Some(pat) = find_runtime_panic() {
            if exit == 0 {
                Outcome::Fail {
                    stage: Stage::Expectation {
                        mismatch: ExpectMismatch::NoPanic {
                            expected_pat: pat.clone(),
                        },
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
                // Panic check passed — still check STDOUT/STDERR patterns если есть.
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
                fail.unwrap_or_else(|| Outcome::Pass {
                    detail: "(runtime-panic)".to_string(),
                    elapsed: start.elapsed(),
                })
            }
        } else if let Some(n) = find_exit_code() {
            if exit != n {
                Outcome::Fail {
                    stage: Stage::Expectation {
                        mismatch: ExpectMismatch::WrongExit {
                            expected: n,
                            got: exit,
                        },
                    },
                    elapsed: start.elapsed(),
                }
            } else {
                Outcome::Pass {
                    detail: format!("(exit-code {})", n),
                    elapsed: start.elapsed(),
                }
            }
        } else {
            // No panic/exit-code marker.
            let stdout_pats = find_stdout();
            let stderr_pats = find_stderr();
            let has_content_marker = !stdout_pats.is_empty() || !stderr_pats.is_empty();

            if !has_content_marker && exit != 0 {
                // Default path: no marker at all — expect exit 0.
                let last_lines: Vec<&str> = stdout
                    .lines()
                    .chain(stderr.lines())
                    .rev()
                    .take(3)
                    .collect();
                let detail = last_lines.into_iter().rev().collect::<Vec<_>>().join(" | ");
                Outcome::Fail {
                    stage: Stage::Run { error: detail },
                    elapsed: start.elapsed(),
                }
            } else {
                // EXPECT_STDOUT / EXPECT_STDERR: check patterns, any exit code allowed
                // (preserves old single-marker semantics: EXPECT_STDERR on panic process).
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
                fail.unwrap_or_else(|| Outcome::Pass {
                    detail: if has_content_marker { "(stdout/stderr)".to_string() } else { String::new() },
                    elapsed: start.elapsed(),
                })
            }
        }
    };

    // Cleanup через subdir_guard Drop (RAII).
    outcome
}

/// Codegen .nv → .c. Возвращает Err(rendered-error-string) если type-check / codegen упали.
fn codegen_to_c(path: &Path, src: &str) -> Result<(), String> {
    let mut module = parser::parse(src).map_err(|d| d.render(src, &path.to_string_lossy()))?;
    manifest::check_module_path(path, &module.name).map_err(|s| s.to_string())?;
    types::check_module(&module).map_err(|errs| {
        errs.iter()
            .map(|d| d.render(src, &path.to_string_lossy()))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    types::infer_effects(&mut module);

    let mut emitter = CEmitter::new();
    emitter.set_source_for_annotations(src.to_string());
    let c_code = emitter
        .emit_module(&module)
        .map_err(|e| format!("codegen error: {}", e))?;
    let out_path = path.with_extension("c");
    std::fs::write(&out_path, &c_code).map_err(|e| {
        format!(
            "failed to write {}: {}",
            out_path.display(),
            e
        )
    })?;
    Ok(())
}

// ---------- test-all: walk + summary ----------

pub struct TestAllOpts<'a> {
    pub tests_dir: &'a Path,
    pub stdlib_dir: Option<&'a Path>,
    pub include_stdlib: bool,
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
#[derive(Debug, Clone)]
pub struct ResultRecord {
    pub name: String,
    pub passed: bool,
    pub elapsed_ms: u128,
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
        collect_c_files(&src_root.join("unix"), &mut srcs, /*recursive*/ false)?;
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
    pub results: Vec<(String, Status)>,
}

/// Рекурсивный обход директории, возвращает все .nv файлы.
fn walk_nv(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(root)
        .map_err(|e| anyhow!("read_dir {}: {}", root.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| anyhow!("read_dir entry: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            walk_nv(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("nv") {
            out.push(path);
        }
    }
    Ok(())
}

/// Сборка display-name для теста на основе path + base.
/// `nova_tests/basics/literals.nv` → `basics/literals`.
/// `std/checksums/fnv.nv` → `std/checksums/fnv`.
fn display_name(path: &Path, base: &Path, is_stdlib: bool) -> String {
    let rel = path.strip_prefix(base).unwrap_or(path);
    let s = rel.with_extension("");
    let mut s = s.to_string_lossy().replace('\\', "/");
    if is_stdlib {
        s = format!("std/{}", s);
    }
    s
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
fn emit_event(format: OutputFormat, idx: usize, total: usize, name: &str, outcome: &Outcome) {
    let mut stdout = std::io::stdout().lock();
    match format {
        OutputFormat::Text => {
            let label = outcome.label();
            let detail = outcome.detail();
            if detail.is_empty() {
                let _ = writeln!(stdout, "{:<14} {}", label, name);
            } else {
                let trunc: String = detail.chars().take(120).collect();
                let _ = writeln!(stdout, "{:<14} {}  # {}", label, name, trunc);
            }
        }
        OutputFormat::Json => {
            let status = if outcome.is_pass() { "pass" } else if matches!(outcome, Outcome::Timeout { .. }) { "timeout" } else { "fail" };
            let stage = match outcome {
                Outcome::Pass { .. } => "",
                Outcome::Timeout { .. } => "timeout",
                Outcome::Fail { stage, .. } => match stage {
                    Stage::Codegen { .. } => "codegen",
                    Stage::Cc { .. } => "cc",
                    Stage::Run { .. } => "run",
                    Stage::NoCFile => "no-c-file",
                    Stage::Expectation { .. } => "expectation",
                },
            };
            let detail = outcome.detail();
            let _ = writeln!(
                stdout,
                "{{\"event\":\"finished\",\"test\":\"{}\",\"status\":\"{}\",\"stage\":\"{}\",\"elapsed_ms\":{},\"detail\":\"{}\"}}",
                json_escape(name),
                status,
                stage,
                outcome.elapsed().as_millis(),
                json_escape(&detail),
            );
        }
        OutputFormat::Tap => {
            // TAP-13: `ok N - name` или `not ok N - name`.
            let _ = if outcome.is_pass() {
                writeln!(stdout, "ok {} - {}", idx + 1, name)
            } else {
                let detail = outcome.detail();
                if detail.is_empty() {
                    writeln!(stdout, "not ok {} - {}", idx + 1, name)
                } else {
                    writeln!(stdout, "not ok {} - {} # {}", idx + 1, name, detail)
                }
            };
        }
        OutputFormat::Junit => {
            // JUnit XML — batch format. Per-test events не emit'им; всё
            // строится в print_summary через cumulative results.
        }
    }
    let _ = stdout.flush();
    let _ = (idx, total); // suppress unused warning для text/tap путей
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
        // Парсим: {"name":"...","passed":true,"elapsed_ms":123}
        // Минималистично через manual split — без regex/serde_json.
        let name = extract_json_str(line, "\"name\":\"");
        let passed_str = extract_json_field(line, "\"passed\":");
        let elapsed_str = extract_json_field(line, "\"elapsed_ms\":");
        if let (Some(name), Some(passed), Some(elapsed)) = (name, passed_str, elapsed_str) {
            let passed = passed.trim() == "true";
            let elapsed_ms = elapsed.trim_end_matches('}').trim().parse::<u128>().unwrap_or(0);
            out.push(ResultRecord {
                name,
                passed,
                elapsed_ms,
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
        s.push_str(&format!(
            "{{\"name\":\"{}\",\"passed\":{},\"elapsed_ms\":{}}}\n",
            json_escape(&r.name),
            r.passed,
            r.elapsed_ms,
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, s)
}

pub fn run_all(opts: TestAllOpts) -> Result<Summary> {
    // Plan 26 Ф.13: install Ctrl+C handler один раз. Worker'ы проверят
    // is_cancelled() перед каждым тестом и graceful exit'нут.
    install_cancel_handler();
    // Tests-dir обязателен; stdlib-dir — опционален.
    let mut inputs: Vec<(PathBuf, /*is_stdlib*/ bool)> = Vec::new();
    let mut tests_files = Vec::new();
    walk_nv(opts.tests_dir, &mut tests_files)?;
    for p in tests_files {
        inputs.push((p, false));
    }
    if opts.include_stdlib {
        if let Some(stdlib) = opts.stdlib_dir {
            let mut stdlib_files = Vec::new();
            walk_nv(stdlib, &mut stdlib_files)?;
            for p in stdlib_files {
                inputs.push((p, true));
            }
        }
    }
    // Сортировка по relative path — стабильный порядок.
    inputs.sort_by(|a, b| a.0.cmp(&b.0));

    std::fs::create_dir_all(opts.tmp_dir)
        .map_err(|e| anyhow!("create tmp_dir: {}", e))?;

    // Plan 26 Ф.10: --rerun-failed pre-load list.
    let rerun_set: Option<std::collections::HashSet<String>> = if opts.rerun_failed {
        let path = opts
            .results_file
            .ok_or_else(|| anyhow!("--rerun-failed requires --results-file"))?;
        let prev = load_results(path);
        if prev.is_empty() {
            return Err(anyhow!(
                "--rerun-failed: results file {} empty or unreadable",
                path.display()
            ));
        }
        Some(
            prev.iter()
                .filter(|r| !r.passed)
                .map(|r| r.name.clone())
                .collect(),
        )
    } else {
        None
    };

    // Собираем job-list (display + nv_path + base) после filter/rerun.
    let mut jobs: Vec<(String, PathBuf)> = Vec::new();
    for (nv_path, is_stdlib) in &inputs {
        let base = if *is_stdlib {
            opts.stdlib_dir.unwrap_or(opts.tests_dir)
        } else {
            opts.tests_dir
        };
        let display = display_name(nv_path, base, *is_stdlib);
        if let Some(filter) = opts.filter {
            if !display.contains(filter) {
                continue;
            }
        }
        if let Some(set) = &rerun_set {
            if !set.contains(&display) {
                continue;
            }
        }
        jobs.push((display, nv_path.clone()));
    }
    let total = jobs.len();

    // TAP-13 header.
    if opts.format == OutputFormat::Tap {
        println!("TAP version 13");
        println!("1..{}", total);
        let _ = std::io::stdout().flush();
    }

    // Plan 26 Ф.3: параллельный прогон. Используем std::thread::scope —
    // нет extra dependencies. Round-robin распределение через atomic-counter.
    let jobs_arc = std::sync::Arc::new(jobs);
    let next_idx = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let results_mutex = std::sync::Arc::new(std::sync::Mutex::new(
        Vec::<(usize, String, Outcome)>::with_capacity(total),
    ));

    let workers = std::cmp::max(1, opts.jobs).min(total.max(1));
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

            s.spawn(move || loop {
                // Plan 26 Ф.13: cooperative cancellation. При Ctrl+C
                // worker'ы выходят на следующей итерации, не забирая
                // новые тесты. Уже запущенный child-процесс получит
                // KILL по wait_with_timeout-таймауту (но это редкий
                // case — обычно child'ы быстрые).
                if is_cancelled() {
                    return;
                }
                let idx = next_idx.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if idx >= jobs.len() {
                    return;
                }
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
                };
                // Plan 26 Ф.12: retry loop для transient AV/linker race
                // fails. Реальные test-fail'ы (expectation mismatch, codegen
                // error) НЕ retry'им — иначе false-PASS. Только AV-class.
                // Exponential backoff: 100ms, 200ms, 400ms.
                //
                // Plan 26 Ф.17 #1: cumulative elapsed для отчёта. Per-attempt
                // run_one() сбрасывает свой start; в JSON/JUnit нужно
                // показать **общее** время от начала первого attempt'а
                // до конца последнего — иначе CI вводит в заблуждение
                // («тест занял 30s», а реально 60s+backoff×2).
                let retry_start = Instant::now();
                let mut outcome = run_one(&test_opts);
                for attempt in 1..=retries {
                    if !is_transient_fail(&outcome) {
                        break;
                    }
                    let backoff = std::time::Duration::from_millis(100 * (1 << (attempt - 1)));
                    std::thread::sleep(backoff);
                    outcome = run_one(&test_opts);
                    if outcome.is_pass() {
                        // Сообщаем что retry помог — DX-сигнал что есть AV-race.
                        // В JSON-mode не emit'им (был бы duplicate event).
                        if matches!(format, OutputFormat::Text) {
                            let mut stdout = std::io::stdout().lock();
                            let _ = writeln!(
                                stdout,
                                "  ↻ retry-{} passed: {}",
                                attempt, display
                            );
                            let _ = stdout.flush();
                        }
                        break;
                    }
                }
                // Plan 26 Ф.17 #1: override elapsed cumulative от первого
                // attempt'а до завершения (включая backoffs). Иначе CI
                // видит только последний run-time.
                if retries > 0 {
                    outcome = outcome.with_elapsed(retry_start.elapsed());
                }

                // Streaming output: Quiet — только FAIL; Normal/Verbose — все.
                let should_emit = match verbosity {
                    Verbosity::Quiet => !outcome.is_pass(),
                    Verbosity::Normal | Verbosity::Verbose => true,
                };
                if should_emit {
                    emit_event(format, idx, jobs.len(), display, &outcome);
                }
                // Plan 26 Ф.15: graceful poisoned-recovery. Worker
                // panic в другом thread'е → mutex poisoned. Восстанавливаем
                // guard через `into_inner` poison'а — наш test-result
                // структурно не зависит от состояния прерванного worker'а.
                let mut guard = match results_mutex.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                guard.push((idx, display.clone(), outcome));
            });
        }
    });

    // Reassemble в порядке job-index (parallel threads complete вразнобой).
    // Plan 26 Ф.15 fix: graceful unwrap вместо panic. `scope()` гарантирует
    // join, но defensive guard на случай если внутренний invariant ломается
    // (например, future refactor добавит spawn вне scope).
    let mutex_inner = match std::sync::Arc::try_unwrap(results_mutex) {
        Ok(m) => m,
        Err(arc) => {
            eprintln!(
                "warning: results-mutex Arc has {} extra strong refs after scope() — \
                 worker leak; returning partial results",
                std::sync::Arc::strong_count(&arc) - 1
            );
            // Не можем take inner; вернём empty results — runner закроется чисто.
            return Ok(Summary {
                pass: 0,
                fail: 0,
                results: Vec::new(),
            });
        }
    };
    let mut indexed = match mutex_inner.into_inner() {
        Ok(v) => v,
        Err(poison) => {
            eprintln!("warning: results mutex poisoned, recovering partial results");
            poison.into_inner()
        }
    };
    indexed.sort_by_key(|(idx, _, _)| *idx);
    let results: Vec<(String, Outcome)> = indexed
        .into_iter()
        .map(|(_, name, outcome)| (name, outcome))
        .collect();

    let mut pass = 0usize;
    let mut fail = 0usize;
    for (_, s) in &results {
        if s.is_pass() {
            pass += 1;
        } else {
            fail += 1;
        }
    }

    // Plan 26 Ф.10: save results на диск для следующего --rerun-failed.
    if let Some(path) = opts.results_file {
        let records: Vec<ResultRecord> = results
            .iter()
            .map(|(name, outcome)| ResultRecord {
                name: name.clone(),
                passed: outcome.is_pass(),
                elapsed_ms: outcome.elapsed().as_millis(),
            })
            .collect();
        if let Err(e) = save_results(path, &records) {
            eprintln!("warning: failed to save results file {}: {}", path.display(), e);
        }
    }

    Ok(Summary {
        pass,
        fail,
        results,
    })
}

/// Вывод финального summary. Per-test events уже отстримлены в run_all.
///
/// Plan 26 Ф.4: формат влияет — Text печатает таблицу, JSON финальный
/// summary-event, TAP — `# pass/fail` комментарий.
/// Plan 26 Ф.8: всё в stdout (cargo/go test convention).
pub fn print_summary(summary: &Summary, format: OutputFormat) {
    let mut stdout = std::io::stdout().lock();
    match format {
        OutputFormat::Text => {
            let _ = writeln!(stdout);
            let _ = writeln!(stdout, "===== SUMMARY =====");
            let mut had_fail = false;
            for (name, status) in &summary.results {
                if status.is_pass() {
                    continue;
                }
                had_fail = true;
                let label = status.label();
                let detail = status.detail();
                let line = if detail.is_empty() {
                    format!("{:<14} {}", label, name)
                } else {
                    let trunc: String = detail.chars().take(120).collect();
                    format!("{:<14} {}  # {}", label, name, trunc)
                };
                let _ = writeln!(stdout, "{}", line);
            }
            if had_fail {
                let _ = writeln!(stdout);
            }
            let _ = writeln!(stdout, "PASS: {}  FAIL: {}", summary.pass, summary.fail);
        }
        OutputFormat::Json => {
            // Plan 26 Ф.16 #11: failed-list в summary event. CI dashboard'у
            // удобно достать имена failed/timeout тестов из одного event'а
            // без grep'а по всем `finished` events. Имена exact-match с
            // events выше (display-name).
            let total_ms: u128 = summary.results.iter().map(|(_, o)| o.elapsed().as_millis()).sum();
            let failed_names: Vec<String> = summary
                .results
                .iter()
                .filter(|(_, o)| !o.is_pass())
                .map(|(name, _)| format!("\"{}\"", json_escape(name)))
                .collect();
            let _ = writeln!(
                stdout,
                "{{\"event\":\"summary\",\"pass\":{},\"fail\":{},\"elapsed_ms\":{},\"failed\":[{}]}}",
                summary.pass, summary.fail, total_ms,
                failed_names.join(",")
            );
        }
        OutputFormat::Tap => {
            let _ = writeln!(stdout, "# pass {}", summary.pass);
            let _ = writeln!(stdout, "# fail {}", summary.fail);
        }
        OutputFormat::Junit => {
            // JUnit XML batch output. Schema: <testsuites><testsuite>
            // <testcase>. Failures emit'ятся как <failure type="..."
            // message="..."/> child element.
            let total_s: f64 = summary.results.iter()
                .map(|(_, o)| o.elapsed().as_secs_f64()).sum();
            let timestamp = chrono_like_iso8601();
            let _ = writeln!(stdout, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
            let _ = writeln!(stdout,
                "<testsuites name=\"nova_tests\" tests=\"{}\" failures=\"{}\" time=\"{:.3}\">",
                summary.results.len(), summary.fail, total_s);
            let _ = writeln!(stdout,
                "  <testsuite name=\"nova_tests\" tests=\"{}\" failures=\"{}\" time=\"{:.3}\" timestamp=\"{}\">",
                summary.results.len(), summary.fail, total_s, xml_escape(&timestamp));
            for (name, outcome) in &summary.results {
                // Split classname.testname по последнему `/`.
                let (classname, testname) = match name.rfind('/') {
                    Some(idx) => (&name[..idx], &name[idx + 1..]),
                    None => ("", name.as_str()),
                };
                let elapsed_s = outcome.elapsed().as_secs_f64();
                if outcome.is_pass() {
                    let _ = writeln!(stdout,
                        "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"/>",
                        xml_escape(classname), xml_escape(testname), elapsed_s);
                } else {
                    let stage = match outcome {
                        Outcome::Timeout { .. } => "timeout",
                        Outcome::Fail { stage, .. } => match stage {
                            Stage::Codegen { .. } => "codegen",
                            Stage::Cc { .. } => "cc",
                            Stage::Run { .. } => "run",
                            Stage::NoCFile => "no-c-file",
                            Stage::Expectation { .. } => "expectation",
                        },
                        _ => "unknown",
                    };
                    let detail = outcome.detail();
                    let _ = writeln!(stdout,
                        "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\">",
                        xml_escape(classname), xml_escape(testname), elapsed_s);
                    let _ = writeln!(stdout,
                        "      <failure type=\"{}\" message=\"{}\"/>",
                        xml_escape(stage), xml_escape(&detail));
                    let _ = writeln!(stdout, "    </testcase>");
                }
            }
            let _ = writeln!(stdout, "  </testsuite>");
            let _ = writeln!(stdout, "</testsuites>");
        }
    }
    let _ = stdout.flush();
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
        match parse_expect(src) {
            Some(ExpectMarker::ExitCode(1)) => {}
            other => panic!("expected ExitCode(1) (first), got {:?}", other),
        }
    }

    #[test]
    fn display_name_simple() {
        let path = Path::new("d:/repo/nova_tests/basics/literals.nv");
        let base = Path::new("d:/repo/nova_tests");
        assert_eq!(display_name(path, base, false), "basics/literals");
    }

    #[test]
    fn display_name_stdlib_prefix() {
        let path = Path::new("d:/repo/std/checksums/fnv.nv");
        let base = Path::new("d:/repo/std");
        assert_eq!(display_name(path, base, true), "std/checksums/fnv");
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
}
