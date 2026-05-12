//! Plan 28: `nova` CLI — единая точка входа для пользователя.
//!
//! `nova test`, `nova build`, `nova run`, `nova check`, `nova regen-runtime`.
//! Заменяет run_tests.ps1 / run_tests.sh / regen_runtime.ps1.
//!
//! `nova-codegen` CLI сохраняется нетронутым — используется IDE, CI, отладкой.

use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use nova_codegen::test_runner;

// ---------- Plan 36 R7: structured CLI error types ----------

/// Plan 36 R7: usage error — bad flag, path not found, wrong extension,
/// no nova.toml. Mapped to exit code **2** (distinct from diagnostic
/// failures which are exit=1).
///
/// Cargo-style: 0 = success, 1 = diagnostic, 2 = usage, 101 = panic.
#[derive(Debug)]
struct UsageError(String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UsageError {}

/// Helper: создаёт UsageError-wrapped anyhow::Error.
fn usage_err(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(UsageError(msg.into()))
}

// ---------- CLI definition ----------

#[derive(Parser)]
#[command(
    name = "nova",
    version,
    about = "Nova language CLI — build, run, test Nova programs"
)]
struct Cli {
    /// Plan 36 R10: color control. `auto` (default) detects via env,
    /// `always` forces ANSI even in pipes, `never` disables completely.
    /// Respects NO_COLOR, CLICOLOR, CLICOLOR_FORCE, CI, TERM=dumb env vars.
    #[arg(long, global = true, default_value = "auto",
          value_parser = ["auto", "always", "never"])]
    color: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Type-check Nova source files or directories.
    ///
    /// Polymorphic positional path: file → single check, directory → recursive walk.
    /// Multiple paths supported. Empty → check current workspace (walks parents
    /// до nova.toml).
    Check {
        /// Paths to check (files or directories). If empty, uses workspace root.
        #[arg(num_args = 0..)]
        paths: Vec<PathBuf>,
        /// Number of parallel workers (0 = num_cpus, default 0).
        #[arg(long, default_value_t = 0)]
        jobs: usize,
        /// Show only failures and summary (no per-file `ok:` lines).
        #[arg(long, short = 'q', conflicts_with = "verbose")]
        quiet: bool,
        /// Show extra info per file (timing, expanded warnings).
        #[arg(long, short = 'v', conflicts_with = "quiet")]
        verbose: bool,
        /// List files that would be checked, without checking. Useful для
        /// отладки --skip / implicit-excludes.
        #[arg(long)]
        list: bool,
        /// Output format. `human` (default) — colored per-file; `short` —
        /// `file:line:col: msg` для grep. JSON/SARIF/JUnit — sub-plan 36.A.
        #[arg(long, default_value = "human", value_parser = ["human", "short"])]
        format: String,
        /// Include std/runtime/ (auto-gen, normally skipped).
        #[arg(long = "include-runtime")]
        include_runtime: bool,
        /// Skip files whose path matches this substring (repeatable).
        #[arg(long = "skip", value_name = "PATTERN")]
        skip: Vec<String>,
    },
    /// Run a Nova source file via the interpreter.
    Run {
        file: PathBuf,
    },
    /// Compile a single Nova source file to a native binary.
    ///
    /// **Single file only** — `nova build` produces one binary per invocation
    /// (`-o output` is one path). For multi-file projects, use imports within
    /// the entry-point file. To typecheck multiple files use `nova check <dir>`.
    Build {
        /// Single .nv file (entry-point with `fn main`).
        file: PathBuf,
        /// Output binary path (default: <name>[.exe] next to source).
        #[arg(short = 'o')]
        output: Option<PathBuf>,
        /// Build mode: 'dev' (unoptimized) or 'release' (optimized).
        #[arg(long, default_value = "dev", value_parser = ["dev", "release"])]
        mode: String,
        /// C compiler to use.
        #[arg(long, default_value = "auto", value_parser = ["auto", "clang", "msvc", "gcc"])]
        toolchain: String,
        /// Path to vcvars64.bat (Windows, auto-detected via vswhere).
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Path to clang.exe (override auto-detect).
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Compiler timeout in seconds (default: 120).
        #[arg(long, default_value_t = 120)]
        timeout: u64,
        /// Keep .c / .exe / .obj build artifacts after the build.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
    },
    /// Run Nova tests from a directory or single file.
    ///
    /// Positional path: file → run that file (must have test blocks),
    /// directory → walk recursively. Default: `<repo>/nova_tests/`.
    Test {
        /// Path to tests directory or file. Default: `<repo>/nova_tests/`.
        #[arg(num_args = 0..=1)]
        path: Option<PathBuf>,
        /// Filter by display-name substring.
        #[arg(long)]
        filter: Option<String>,
        /// Number of parallel workers (0 = num_cpus, default 0).
        #[arg(long, default_value_t = 0)]
        jobs: usize,
        /// Output format: 'text', 'json', 'tap', or 'junit'.
        #[arg(long, default_value = "text", value_parser = ["text", "json", "tap", "junit"])]
        format: String,
        /// Build mode: 'dev' (unoptimized) or 'release' (optimized).
        #[arg(long, default_value = "dev", value_parser = ["dev", "release"])]
        mode: String,
        /// C compiler to use.
        #[arg(long, default_value = "auto", value_parser = ["auto", "clang", "msvc", "gcc"])]
        toolchain: String,
        /// Path to vcvars64.bat (Windows, auto-detected via vswhere).
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Path to clang.exe (override auto-detect).
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Per-test timeout in seconds (default: 60).
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Show output of passing tests too.
        #[arg(long, short = 'v')]
        verbose: bool,
        /// Show only failures and summary.
        #[arg(long, short = 'q')]
        quiet: bool,
        /// Path to write last-test-results.json.
        /// Default: <repo>/target/last-test-results.json
        #[arg(long = "results-file")]
        results_file: Option<PathBuf>,
        /// Re-run only tests that failed/timed-out in the last run.
        #[arg(long = "rerun-failed")]
        rerun_failed: bool,
        /// Retry count for transient failures (AV races, etc). Default 0.
        #[arg(long, default_value_t = 0)]
        retries: u32,
        /// Include std/ files in the run.
        #[arg(long = "include-stdlib")]
        include_stdlib: bool,
        /// Keep .c / .exe / .obj build artifacts after the run.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        /// GC backend: 'boehm' (default after Plan 27) or 'malloc' (no GC, internal only).
        /// Reserved for Plan 27 — currently accepted but has no effect.
        #[arg(long, value_parser = ["boehm", "malloc"])]
        gc: Option<String>,
        /// List collected tests without running them.
        #[arg(long)]
        list: bool,
        /// Path to a file containing test names (one per line) to run (exact display-name match).
        #[arg(long = "filter-from")]
        filter_from: Option<PathBuf>,
        /// Shuffle test execution order. Optional seed value; omit for random seed.
        /// Example: --shuffle 42  or  --shuffle (random seed).
        #[arg(long, num_args = 0..=1, value_name = "SEED")]
        shuffle: Option<Option<u64>>,
        /// Skip tests whose display name OR file path contains this substring.
        /// Repeatable: `--skip A --skip B` excludes both.
        /// Example: `nova test std/ --skip std/runtime/`.
        #[arg(long = "skip", value_name = "PATTERN")]
        skip: Vec<String>,
    },
    /// Build and run a single Nova test file (used by IDE / CI for one-shot debug).
    #[command(name = "test-build")]
    TestBuild {
        file: PathBuf,
        /// Build mode: 'dev' (unoptimized) or 'release' (optimized).
        #[arg(long, default_value = "dev", value_parser = ["dev", "release"])]
        mode: String,
        /// C compiler to use.
        #[arg(long, default_value = "auto", value_parser = ["auto", "clang", "msvc", "gcc"])]
        toolchain: String,
        /// Path to vcvars64.bat (Windows, auto-detected via vswhere).
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Path to clang.exe (override auto-detect).
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Timeout in seconds (default: 60).
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Keep .c / .exe / .obj build artifacts after the run.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        /// GC backend: 'boehm' (default after Plan 27) or 'malloc' (no GC, internal only).
        /// Reserved for Plan 27 — currently accepted but has no effect.
        #[arg(long, value_parser = ["boehm", "malloc"])]
        gc: Option<String>,
    },
    /// Regenerate std/runtime/*.nv stubs from the runtime registry.
    #[command(name = "regen-runtime")]
    RegenRuntime {
        /// Only compare — fail if stubs diverge from registry (CI guard).
        #[arg(long)]
        check: bool,
    },
}

// ---------- repo root discovery ----------

/// Walk up from CWD until a directory containing `nova.toml` is found.
fn find_repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()
        .map_err(|e| anyhow!("cannot determine current directory: {}", e))?;
    loop {
        if dir.join("nova.toml").exists() {
            return Ok(dir);
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => bail!(
                "nova.toml not found — are you inside a Nova project?\n\
                 (Searched from {} up to filesystem root.)",
                std::env::current_dir().unwrap_or_default().display()
            ),
        }
    }
}

struct RepoPaths {
    tests_dir: PathBuf,
    stdlib_dir: PathBuf,
    cg_include: PathBuf,
    rt_dir: PathBuf,
    default_results_file: PathBuf,
}

fn resolve_paths(repo: &Path) -> RepoPaths {
    RepoPaths {
        tests_dir: repo.join("nova_tests"),
        stdlib_dir: repo.join("std"),
        cg_include: repo.join("compiler-codegen"),
        rt_dir: repo.join("compiler-codegen").join("nova_rt"),
        default_results_file: repo.join("target").join("last-test-results.json"),
    }
}

// ---------- ANSI colors ----------
// Plan 36 R10: `--color auto|always|never` + env detection.
//
// Auto-detection precedence (highest first):
//   1. `--color always|never` CLI flag
//   2. `CLICOLOR_FORCE=1` env → always
//   3. `NO_COLOR=*` env → never (https://no-color.org)
//   4. `CLICOLOR=0` env → never
//   5. `CI=true` env → never (CI logs usually no-ANSI)
//   6. `TERM=dumb` env → never
//   7. default → auto = on (если nothing above triggers)
//
// На Windows ANSI поддерживается в Windows Terminal / modern conhost.
// is-terminal crate отложен (post-MVP); пока default = always on.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(ColorMode::Auto),
            "always" => Ok(ColorMode::Always),
            "never" => Ok(ColorMode::Never),
            _ => Err(usage_err(format!(
                "invalid --color value `{}` (expected: auto, always, never)", s))),
        }
    }
}

static COLOR_MODE: std::sync::OnceLock<ColorMode> = std::sync::OnceLock::new();

fn set_color_mode(mode: ColorMode) {
    let _ = COLOR_MODE.set(mode);
}

fn colors_enabled() -> bool {
    let mode = COLOR_MODE.get().copied().unwrap_or(ColorMode::Auto);
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            // CLICOLOR_FORCE wins over all "off" signals.
            if std::env::var_os("CLICOLOR_FORCE")
                .map(|v| v != "0")
                .unwrap_or(false)
            {
                return true;
            }
            if std::env::var_os("NO_COLOR").is_some() {
                return false;
            }
            if std::env::var("CLICOLOR").as_deref() == Ok("0") {
                return false;
            }
            if std::env::var("CI").as_deref() == Ok("true") {
                return false;
            }
            if std::env::var("TERM").as_deref() == Ok("dumb") {
                return false;
            }
            true
        }
    }
}

fn red(s: &str) -> String {
    if colors_enabled() { format!("\x1b[31m{}\x1b[0m", s) } else { s.to_string() }
}
fn yellow(s: &str) -> String {
    if colors_enabled() { format!("\x1b[33m{}\x1b[0m", s) } else { s.to_string() }
}
fn green(s: &str) -> String {
    if colors_enabled() { format!("\x1b[32m{}\x1b[0m", s) } else { s.to_string() }
}
fn bold(s: &str) -> String {
    if colors_enabled() { format!("\x1b[1m{}\x1b[0m", s) } else { s.to_string() }
}

// ---------- helpers ----------

fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)
        .map_err(|e| anyhow!("failed to read {}: {}", path.display(), e))
}

fn default_tmp_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Some(temp) = std::env::var_os("TEMP") {
            return PathBuf::from(temp).join("nova_tests");
        }
    }
    if let Some(tmpdir) = std::env::var_os("TMPDIR") {
        return PathBuf::from(tmpdir).join("nova_tests");
    }
    PathBuf::from("/tmp/nova_tests")
}

/// Hash `path` to a short hex string — used to make unique tmp subdirs
/// without adding a sha256 crate dependency.
fn path_hash(path: &Path) -> String {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// RAII guard: removes a tmp directory on drop (best-effort, errors ignored).
/// Set `keep = true` to skip cleanup (--keep-artifacts).
struct TmpDirGuard<'a> {
    path: &'a Path,
    keep: bool,
}

impl Drop for TmpDirGuard<'_> {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(self.path);
        }
    }
}

// ---------- subcommand implementations ----------

fn check_module_path(path: &Path, module: &nova_codegen::ast::Module) -> Result<()> {
    nova_codegen::manifest::check_module_path(path, &module.name)
        .map_err(|msg| anyhow!("{}", msg))
}

// Plan 35 R31: resolve_imports_inline extracted в nova_codegen::imports
// для использования из всех 3 pipelines (cmd_check, cmd_build, test_runner).
// Local thin wrapper для backward-compat usage в cmd_build (UsageError tagging).

/// Plan 36 Ф.1 + 36.D: polymorphic path argument + verbosity + filters.
/// Принимает список путей (file-or-dir, рекурсивный walk для dir).
/// Empty → walks parents до nova.toml, использует workspace root.
///
/// Hard-coded skip (override через --include-runtime / --skip):
///   - `target/`, `node_modules/`, `vendor/`, `.git/`, `.hg/`, `.svn/`
///   - directories starting with `_` or `.`
///   - `std/runtime/` (auto-gen, Plan 13)
///
/// Flags:
///   - `--jobs N` parallel workers (0=num_cpus).
///   - `-q`/`--quiet` only failures + summary.
///   - `-v`/`--verbose` extra info per file (timing).
///   - `--list` list files без check.
///   - `--format human|short` (JSON/SARIF/JUnit — sub-plan 36.A).
///   - `--include-runtime` skip std/runtime/ override.
///   - `--skip PATTERN` repeatable substring skip filter.
#[allow(clippy::too_many_arguments)]
fn cmd_check(
    paths: &[PathBuf],
    jobs: usize,
    quiet: bool,
    verbose: bool,
    list_only: bool,
    format: &str,
    include_runtime: bool,
    skip: &[String],
) -> Result<()> {
    // Если пути не указаны — используем workspace root.
    let owned_root;
    let resolved_paths: Vec<PathBuf> = if paths.is_empty() {
        owned_root = find_repo_root()?;
        vec![owned_root.clone()]
    } else {
        paths.iter().cloned().collect()
    };

    // Собираем список .nv файлов: для file — сам файл, для dir — рекурсивный walk.
    let mut files: Vec<PathBuf> = Vec::new();
    for p in &resolved_paths {
        if !p.exists() {
            return Err(usage_err(format!("path not found: {}", p.display())));
        }
        if p.is_file() {
            // Проверка расширения.
            if p.extension().and_then(|s| s.to_str()) != Some("nv") {
                return Err(usage_err(format!("not a Nova source: {}", p.display())));
            }
            files.push(p.clone());
        } else if p.is_dir() {
            let mut found = Vec::new();
            nova_codegen::test_runner::walk_nv(p, &mut found)
                .map_err(|e| anyhow!("walk {}: {}", p.display(), e))?;
            for f in found {
                if !should_skip_path_full(&f, include_runtime, skip) {
                    files.push(f);
                }
            }
        } else {
            return Err(usage_err(format!("path is neither file nor directory: {}", p.display())));
        }
    }

    // Дедуп через canonicalize.
    let mut seen = std::collections::HashSet::new();
    files.retain(|p| {
        match p.canonicalize() {
            Ok(c) => seen.insert(c),
            Err(_) => true,
        }
    });

    // Sort для детерминизма.
    files.sort();

    // --list: show files, no check.
    if list_only {
        for f in &files {
            println!("{}", f.display());
        }
        if !quiet {
            eprintln!("\n{} file(s) would be checked", files.len());
        }
        return Ok(());
    }

    if files.is_empty() {
        if !quiet {
            println!("no .nv files to check");
        }
        return Ok(());
    }

    let n_workers = if jobs == 0 { num_cpus() } else { jobs };
    let total = files.len();
    let start = std::time::Instant::now();

    // Parallel via thread::scope + mpsc.
    let (tx, rx) = std::sync::mpsc::channel::<CheckResult>();
    let files_arc = std::sync::Arc::new(files);

    std::thread::scope(|s| {
        let n_threads = n_workers.min(total).max(1);
        let chunk = (total + n_threads - 1) / n_threads;
        for i in 0..n_threads {
            let start = i * chunk;
            let end = ((i + 1) * chunk).min(total);
            if start >= end {
                continue;
            }
            let files = files_arc.clone();
            let tx = tx.clone();
            s.spawn(move || {
                for f in &files[start..end] {
                    let res = check_one_file(f, verbose);
                    let _ = tx.send(res);
                }
            });
        }
        drop(tx);
    });

    // Aggregate в стабильном file-name order.
    let mut results: Vec<CheckResult> = rx.iter().collect();
    results.sort_by(|a, b| a.file.cmp(&b.file));

    let mut pass = 0;
    let mut fail = 0;
    let mut warn_count = 0;
    for r in &results {
        match &r.error {
            None => {
                pass += 1;
                let timing = if verbose && r.elapsed_ms > 0 {
                    format!(" ({}ms)", r.elapsed_ms)
                } else {
                    String::new()
                };
                let warn_suffix = if r.warnings.is_empty() {
                    String::new()
                } else {
                    format!(" ({} warning(s))", r.warnings.len())
                };
                // -q: skip per-file ok: lines.
                if !quiet {
                    match format {
                        "short" => {
                            // short формат — без декорации, по строке на файл.
                            println!("{}: ok{}{}", r.file.display(), timing, warn_suffix);
                        }
                        _ => {
                            println!("{} {}{}{}",
                                green("ok:"), r.file.display(), timing, warn_suffix);
                        }
                    }
                }
                for w in &r.warnings {
                    if format == "short" {
                        eprintln!("{}", w);
                    } else {
                        eprintln!("  {} {}", bold(&yellow("warning:")), w);
                    }
                    warn_count += 1;
                }
            }
            Some(msg) => {
                fail += 1;
                let timing = if verbose && r.elapsed_ms > 0 {
                    format!(" ({}ms)", r.elapsed_ms)
                } else {
                    String::new()
                };
                if format == "short" {
                    // short: каждая строка ошибки на отдельной строке.
                    for line in msg.lines() {
                        eprintln!("{}: {}", r.file.display(), line);
                    }
                } else {
                    eprintln!("{} {}{}", red("FAIL:"), r.file.display(), timing);
                    for line in msg.lines() {
                        eprintln!("  {}", line);
                    }
                }
                for w in &r.warnings {
                    if format == "short" {
                        eprintln!("{}", w);
                    } else {
                        eprintln!("  {} {}", bold(&yellow("warning:")), w);
                    }
                    warn_count += 1;
                }
            }
        }
    }

    let elapsed = start.elapsed();

    // Summary — всегда (даже в --quiet).
    if format != "short" {
        println!();
        println!("===== SUMMARY =====");
    }
    let timing_suffix = if verbose {
        format!(" ({:.2}s)", elapsed.as_secs_f64())
    } else {
        String::new()
    };
    if fail == 0 {
        if warn_count == 0 {
            println!("{}: {}  FAIL: 0{}", green("PASS"), pass, timing_suffix);
        } else {
            println!("{}: {}  FAIL: 0  WARN: {}{}",
                green("PASS"), pass, warn_count, timing_suffix);
        }
        Ok(())
    } else {
        println!("PASS: {}  {}: {}  WARN: {}{}",
            pass, red("FAIL"), fail, warn_count, timing_suffix);
        Err(anyhow!("{} file(s) failed type-check", fail))
    }
}

#[derive(Debug)]
struct CheckResult {
    file: PathBuf,
    error: Option<String>,
    /// Lint warnings (Plan 36 Ф.0) — non-fatal, отображаются под file
    /// если present. PASS если только warnings (no error).
    warnings: Vec<String>,
    /// Per-file elapsed (milliseconds) — only filled когда verbose=true.
    /// 0 = not measured (Plan 36.D).
    elapsed_ms: u64,
}

/// Single-file check — full pipeline (Plan 36 Ф.0 correctness fix).
/// Phases:
///   1. parse
///   2. check_module_path (D78)
///   3. types::check_module (типы + effects/capability inference embedded)
///   4. types::infer_effects (D28 — effects на private fn)
///   5. lints::lint_module (anonymous-embed override и т.д.)
///
/// До Plan 36 Ф.0 `cmd_check` дёргал только 1-3 — molча пропускал
/// effect-inference и lints, которые `cmd_build` ловит. Это был
/// silent correctness gap.
fn check_one_file(path: &Path, verbose: bool) -> CheckResult {
    let t0 = if verbose { Some(std::time::Instant::now()) } else { None };
    let measure = |t0: Option<std::time::Instant>| -> u64 {
        t0.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0)
    };

    let src = match read_file(path) {
        Ok(s) => s,
        Err(e) => return CheckResult {
            file: path.to_path_buf(),
            error: Some(format!("read: {}", e)),
            warnings: Vec::new(),
            elapsed_ms: measure(t0),
        },
    };
    let path_str = path.to_string_lossy();

    // 1. parse
    let mut module = match nova_codegen::parser::parse(&src) {
        Ok(m) => m,
        Err(d) => return CheckResult {
            file: path.to_path_buf(),
            error: Some(d.render(&src, &path_str)),
            warnings: Vec::new(),
            elapsed_ms: measure(t0),
        },
    };

    // 2. check_module_path
    if let Err(e) = check_module_path(path, &module) {
        return CheckResult {
            file: path.to_path_buf(),
            error: Some(format!("{}", e)),
            warnings: Vec::new(),
            elapsed_ms: measure(t0),
        };
    }

    // 2.5 Plan 35 R31: cross-file resolve через inline expansion.
    // Безопасно для check: type-check merged AST — same correctness как
    // cmd_build. Закрывает Plan 35 R19 (nova check parity).
    if !module.imports.is_empty() {
        if let Ok(repo) = find_repo_root() {
            let paths = resolve_paths(&repo);
            if let Err(e) = nova_codegen::imports::resolve_imports_inline(
                path, &mut module, &repo, &paths.stdlib_dir,
            ) {
                return CheckResult {
                    file: path.to_path_buf(),
                    error: Some(format!("import resolution: {}", e)),
                    warnings: Vec::new(),
                    elapsed_ms: measure(t0),
                };
            }
        }
        // Если find_repo_root() не нашёл nova.toml — silently skip imports
        // (single-file mode без cross-file context).
    }

    // 3. types::check_module
    if let Err(errs) = nova_codegen::types::check_module(&module) {
        let msgs: Vec<String> = errs.iter().map(|d| d.render(&src, &path_str)).collect();
        return CheckResult {
            file: path.to_path_buf(),
            error: Some(msgs.join("\n")),
            warnings: Vec::new(),
            elapsed_ms: measure(t0),
        };
    }

    // 4. infer_effects (D28) — fills in inferred effects on private fn.
    nova_codegen::types::infer_effects(&mut module);

    // 5. lints::lint_module — anonymous-embed override, etc.
    let lint_warnings: Vec<String> = nova_codegen::lints::lint_module(&module)
        .into_iter()
        .map(|w| {
            let (line, col) = nova_codegen::diag::byte_to_line_col(&src, w.diag.span.start);
            format!("{}:{}:{}: {} [{}]", path.display(), line, col, w.diag.message, w.rule)
        })
        .collect();

    CheckResult {
        file: path.to_path_buf(),
        error: None,
        warnings: lint_warnings,
        elapsed_ms: measure(t0),
    }
}

/// Hard-coded skip patterns (Plan 36 R3 minimal version).
/// `include_runtime=true` отключает skip `std/runtime/`.
/// `skip` — user-provided substrings (--skip flag).
fn should_skip_path_full(p: &Path, include_runtime: bool, skip: &[String]) -> bool {
    let s = p.to_string_lossy().replace('\\', "/");
    // User-provided skip patterns first.
    for pat in skip {
        if !pat.is_empty() && s.contains(pat) {
            return true;
        }
    }
    // Skip auto-gen std/runtime/ (unless --include-runtime).
    if !include_runtime && (s.contains("/std/runtime/") || s.starts_with("std/runtime/")) {
        return true;
    }
    // Skip if any component starts with `_` or `.`, or is target/node_modules/vendor.
    for comp in p.components() {
        if let Some(name) = comp.as_os_str().to_str() {
            if name.starts_with('_') || name.starts_with('.') {
                return true;
            }
            if matches!(name, "target" | "node_modules" | "vendor") {
                return true;
            }
        }
    }
    false
}

/// Backward-compat shim (default flags — no override).
#[allow(dead_code)]
fn should_skip_path(p: &Path) -> bool {
    should_skip_path_full(p, false, &[])
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn cmd_run(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }
    let src = read_file(path)?;
    let path_str = path.to_string_lossy();
    let module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    check_module_path(path, &module)?;
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let msgs: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path_str))
            .collect();
        anyhow!("{}", msgs.join("\n"))
    })?;
    let mut interp = nova_codegen::interp::Interpreter::new();
    interp
        .load_module(&module)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    interp
        .run_main()
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    Ok(())
}

fn cmd_build(
    path: &Path,
    output: Option<&Path>,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    timeout_secs: u64,
    keep_artifacts: bool,
) -> Result<()> {
    if timeout_secs == 0 {
        return Err(usage_err("--timeout must be >= 1 second"));
    }
    // Plan 36 R6: validate path semantics before canonicalize (better errors).
    // `nova build` принимает **только single file** (по дизайну — `-o output`
    // один binary, multi-source builds через imports внутри одного entry-point).
    if !path.exists() {
        return Err(usage_err(format!("path not found: {}", path.display())));
    }
    if path.is_dir() {
        return Err(usage_err(format!(
            "`nova build` requires a single .nv file, got directory: {}\n\
             For multi-file projects, use imports within one entry-point file.\n\
             To check multiple files: `nova check <dir>` (typecheck only).",
            path.display())));
    }
    if path.extension().and_then(|s| s.to_str()) != Some("nv") {
        return Err(usage_err(format!("not a Nova source: {}", path.display())));
    }

    let build_start = std::time::Instant::now();
    let repo = find_repo_root()?;
    let paths = resolve_paths(&repo);

    let path = path.canonicalize()
        .map_err(|e| usage_err(format!("cannot resolve path {}: {}", path.display(), e)))?;
    let src = read_file(&path)?;
    let path_str = path.to_string_lossy();

    // parse
    let mut module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    check_module_path(&path, &module)?;

    // Plan 35 Ф.1 MVP: cross-file resolve через inline expansion.
    // Walks `module.imports` recursively, парсит каждый imported .nv,
    // merge'ит Item::Type и Item::Fn в текущий `module.items` ДО typecheck.
    //
    // Limitations (sub-plans 35.A-E):
    //   - Нет visibility filter (is_export — informational, не enforced).
    //   - Нет symbol mangling — collision error если user re-defines.
    //   - Нет DCE — вся imported module emit'ится.
    //   - Нет signature/body split — full re-typecheck merged AST.
    //   - Wildcard `import X.*` не поддерживается (R25, sub-plan 35.A).
    //
    // Cycle detection: visited set по canonical path.
    nova_codegen::imports::resolve_imports_inline(&path, &mut module, &repo, &paths.stdlib_dir)?;

    nova_codegen::types::check_module(&module).map_err(|errs| {
        let msgs: Vec<String> = errs.iter()
            .map(|d| d.render(&src, &path_str))
            .collect();
        anyhow!("{}", msgs.join("\n"))
    })?;
    nova_codegen::types::infer_effects(&mut module);
    for w in nova_codegen::lints::lint_module(&module) {
        let (line, col) = nova_codegen::diag::byte_to_line_col(&src, w.diag.span.start);
        eprintln!("{} {}:{}:{}: {} [{}]", bold(&yellow("warning:")), path.display(), line, col, w.diag.message, w.rule);
    }

    let mut emitter = nova_codegen::codegen::CEmitter::new();
    emitter.set_source_for_annotations(src.clone());
    let (c_code, warnings) = emitter
        .emit_module(&module)
        .map_err(|e| anyhow!("codegen error: {}", e))?;
    for w in &warnings {
        eprintln!("{}", w);
    }

    // determine output path
    let exe_stem = path.file_stem().unwrap_or_default();
    let exe_name = if cfg!(target_os = "windows") {
        format!("{}.exe", exe_stem.to_string_lossy())
    } else {
        exe_stem.to_string_lossy().into_owned()
    };
    let final_exe = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            // Place output next to CWD, not next to the source file (predictable).
            std::env::current_dir()
                .unwrap_or_else(|_| path.parent().unwrap_or(&path).to_path_buf())
                .join(&exe_name)
        });
    if final_exe.is_dir() {
        bail!("output path is a directory: {}", final_exe.display());
    }

    // write .c to a unique tmp dir; cleaned up on any exit path via Drop
    let hash = path_hash(&path);
    let tmp_path = default_tmp_dir().join(format!("build-{}", &hash[..hash.len().min(12)]));
    std::fs::create_dir_all(&tmp_path)
        .map_err(|e| anyhow!("create tmp dir: {}", e))?;
    let _tmp_guard = TmpDirGuard { path: &tmp_path, keep: keep_artifacts };
    let c_file = tmp_path.join(format!("{}.c", exe_stem.to_string_lossy()));
    let exe_file = tmp_path.join(&exe_name);
    std::fs::write(&c_file, &c_code)
        .map_err(|e| anyhow!("write .c file: {}", e))?;

    // detect toolchain
    let mode = test_runner::Mode::parse(mode)?;
    let pref = test_runner::ToolchainPref::parse(toolchain)?;
    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    // detect libuv
    let libuv = test_runner::detect_or_build_libuv(&paths.rt_dir, &repo, tc.vcvars_path());

    test_runner::install_cancel_handler();

    // compile .c → .exe
    let build_opts = test_runner::BuildOpts {
        c_file: &c_file,
        exe_file: &exe_file,
        obj_dir: &tmp_path,
        cg_include: &paths.cg_include,
        rt_dir: &paths.rt_dir,
        mode,
        libuv: libuv.as_ref(),
        gc_kind: test_runner::GcKind::default(),
    };
    test_runner::compile_c_to_exe(&tc, &build_opts, Duration::from_secs(timeout_secs))?;

    // move exe to final destination
    if let Some(parent) = final_exe.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("create output dir: {}", e))?;
    }
    std::fs::rename(&exe_file, &final_exe)
        .or_else(|_| std::fs::copy(&exe_file, &final_exe).map(|_| ()))
        .map_err(|e| anyhow!("move executable: {}", e))?;

    println!("{} {} ({:.2}s)", green("built:"), final_exe.display(), build_start.elapsed().as_secs_f64());
    Ok(())
}

fn cmd_test(
    path_arg: Option<&Path>,
    filter: Option<&str>,
    jobs: usize,
    format: &str,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    timeout_secs: u64,
    verbose: bool,
    quiet: bool,
    results_file: Option<&Path>,
    rerun_failed: bool,
    retries: u32,
    include_stdlib: bool,
    keep_artifacts: bool,
    gc: &str,
    list_only: bool,
    filter_from: Option<&Path>,
    shuffle: Option<Option<u64>>,
    skip: &[String],
) -> Result<()> {
    if timeout_secs == 0 {
        return Err(usage_err("--timeout must be >= 1 second"));
    }
    if let Some(f) = filter {
        if f.is_empty() {
            return Err(usage_err("--filter cannot be empty"));
        }
    }
    if verbose && quiet {
        return Err(usage_err("cannot use --verbose and --quiet simultaneously"));
    }
    let repo = find_repo_root()?;
    let paths = resolve_paths(&repo);

    let mode = test_runner::Mode::parse(mode)?;
    let pref = test_runner::ToolchainPref::parse(toolchain)?;
    let format = test_runner::OutputFormat::parse(format)?;
    let verbosity = if quiet {
        test_runner::Verbosity::Quiet
    } else if verbose {
        test_runner::Verbosity::Verbose
    } else {
        test_runner::Verbosity::Normal
    };
    let jobs = if jobs == 0 { test_runner::default_jobs() } else { jobs };

    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    let libuv = test_runner::detect_or_build_libuv(
        &paths.rt_dir,
        &repo,
        tc.vcvars_path(),
    );

    // Plan 36 Ф.1: positional path argument.
    // None → default <repo>/nova_tests/.
    // Some(file) → use parent dir as tests_dir, filter to single file via display name.
    // Some(dir) → use as tests_dir.
    let (tests_dir, single_file_filter): (PathBuf, Option<String>) = match path_arg {
        None => (paths.tests_dir.clone(), None),
        Some(p) => {
            if !p.exists() {
                return Err(usage_err(format!("path not found: {}", p.display())));
            }
            if p.is_file() {
                if p.extension().and_then(|s| s.to_str()) != Some("nv") {
                    return Err(usage_err(format!("not a Nova source: {}", p.display())));
                }
                // Use parent dir as tests_dir, derive display name relative to it.
                let parent = p.parent()
                    .ok_or_else(|| usage_err(format!("cannot get parent of {}", p.display())))?
                    .to_path_buf();
                let stem = p.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| usage_err(format!("invalid file name: {}", p.display())))?;
                (parent, Some(stem.to_string()))
            } else if p.is_dir() {
                (p.to_path_buf(), None)
            } else {
                return Err(usage_err(format!("path is neither file nor directory: {}", p.display())));
            }
        }
    };

    if format == test_runner::OutputFormat::Text {
        eprintln!(
            "Toolchain: {}, mode={:?}, jobs={}, tests-dir={}",
            tc.name(),
            mode,
            jobs,
            tests_dir.display()
        );
        if libuv.is_none() {
            eprintln!("warning: libuv not found — concurrency tests will fail");
        }
    }

    if !tests_dir.is_dir() {
        return Err(usage_err(format!("tests directory not found: {}", tests_dir.display())));
    }

    // If single-file requested, filter through display-name to that one file
    // (path-filter ⊥ test-name-filter — single-file is path-filter).
    // If user passes both single-file path + --filter — single-file wins (path is
    // path-filter, --filter is test-name-filter; current test_runner uses single
    // string filter for display-name, so we cannot orthogonally combine).
    let effective_filter: Option<String> = single_file_filter.or_else(|| filter.map(str::to_string));
    let effective_filter_ref: Option<&str> = effective_filter.as_deref();
    let stdlib_dir_opt = if include_stdlib {
        Some(paths.stdlib_dir.as_path())
    } else {
        None
    };

    // ensure target/ dir exists for the default results file
    let results_path: PathBuf = results_file
        .map(Path::to_path_buf)
        .unwrap_or(paths.default_results_file);
    if results_path.is_dir() {
        bail!("results file path is a directory: {}", results_path.display());
    }
    if rerun_failed && !results_path.is_file() {
        bail!(
            "--rerun-failed requires a previous results file at {}\n\
             Run `nova test` first to generate it.",
            results_path.display()
        );
    }
    if let Some(parent) = results_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("cannot create results dir {}: {}", parent.display(), e))?;
    }

    let gc_kind = test_runner::GcKind::parse(gc)?;
    // --shuffle: None = no shuffle, Some(None) = random seed, Some(Some(n)) = fixed seed
    let shuffle_seed: Option<u64> = match shuffle {
        None => None,
        Some(None) => Some(0),      // 0 = random seed (resolved in run_all)
        Some(Some(n)) => Some(n),
    };
    let tmp_dir = default_tmp_dir();
    let opts = test_runner::TestAllOpts {
        tests_dir: &tests_dir,
        stdlib_dir: stdlib_dir_opt,
        include_stdlib,
        filter: effective_filter_ref,
        mode,
        toolchain: tc,
        cg_include: &paths.cg_include,
        rt_dir: &paths.rt_dir,
        tmp_dir: &tmp_dir,
        keep_artifacts,
        libuv,
        timeout: Duration::from_secs(timeout_secs),
        jobs,
        format,
        verbosity,
        cache_dir: None,
        results_file: Some(&results_path),
        rerun_failed,
        retries,
        gc_kind,
        list_only,
        filter_from,
        shuffle_seed,
        skip,
    };

    let summary = test_runner::run_all(opts)?;
    test_runner::print_summary(&summary, format);

    if summary.fail > 0 {
        Err(anyhow!("{} test(s) failed", summary.fail))
    } else {
        Ok(())
    }
}

fn cmd_test_build(
    path: &Path,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    timeout_secs: u64,
    keep_artifacts: bool,
    gc: &str,
) -> Result<()> {
    if timeout_secs == 0 {
        bail!("--timeout must be >= 1 second");
    }
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }
    let repo = find_repo_root()?;
    let paths = resolve_paths(&repo);

    let mode = test_runner::Mode::parse(mode)?;
    let pref = test_runner::ToolchainPref::parse(toolchain)?;
    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    let libuv = test_runner::detect_or_build_libuv(&paths.rt_dir, &repo, tc.vcvars_path());

    let display = path
        .strip_prefix(&paths.tests_dir)
        .or_else(|_| path.strip_prefix(&repo))
        .unwrap_or(path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/");

    let gc_kind = test_runner::GcKind::parse(gc)?;
    let tmp_dir = default_tmp_dir();
    let opts = test_runner::TestBuildOpts {
        nv_file: path,
        toolchain: &tc,
        mode,
        cg_include: &paths.cg_include,
        rt_dir: &paths.rt_dir,
        tmp_dir: &tmp_dir,
        display: &display,
        keep_artifacts,
        libuv: libuv.as_ref(),
        timeout: Duration::from_secs(timeout_secs),
        gc_kind,
        verbosity: test_runner::Verbosity::Normal,
    };

    test_runner::install_cancel_handler();
    let outcome = test_runner::run_one(&opts);
    let label = outcome.label();
    let elapsed = outcome.elapsed();
    let detail = outcome.detail();
    if outcome.is_pass() {
        println!("{} {} ({:.2}s)", green(label), display, elapsed.as_secs_f64());
        Ok(())
    } else {
        eprintln!("{} {} ({:.2}s)", bold(&red(label)), display, elapsed.as_secs_f64());
        if !detail.is_empty() {
            eprintln!("{}", detail);
        }
        Err(anyhow!("test failed"))
    }
}

fn cmd_regen_runtime(check: bool) -> Result<()> {
    let repo = find_repo_root()?;
    use nova_codegen::codegen::runtime_registry;
    let registry = runtime_registry::all();
    let groups = runtime_registry::group_by_module(&registry);
    let mut total = 0;
    let mut diffs: Vec<String> = Vec::new();
    for (module, fns) in &groups {
        let rel = runtime_registry::module_to_path(module);
        let abs = repo.join(&rel);
        let content = runtime_registry::render_nv(module, fns);
        if check {
            let existing = std::fs::read_to_string(&abs)
                .map_err(|e| anyhow!("failed to read {}: {}", abs.display(), e))?;
            let norm = |s: &str| s.replace("\r\n", "\n");
            if norm(&existing) != norm(&content) {
                diffs.push(rel);
            }
        } else {
            if let Some(parent) = abs.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow!("create dir {}: {}", parent.display(), e))?;
            }
            std::fs::write(&abs, &content)
                .map_err(|e| anyhow!("write {}: {}", abs.display(), e))?;
            println!("wrote {}", rel);
        }
        total += 1;
    }
    if check {
        if !diffs.is_empty() {
            return Err(anyhow!(
                "auto-generated files diverge from registry:\n  {}\n\
                 Run `nova regen-runtime` to regenerate.",
                diffs.join("\n  ")
            ));
        }
        println!("OK: {} runtime stub file(s) match registry.", total);
    } else {
        println!("emitted {} runtime stub file(s).", total);
    }
    Ok(())
}

// ---------- entry point ----------

fn main() -> ExitCode {
    // Plan 36 R7: guarantee exit=101 on panic (cargo convention, cross-platform).
    // Без этого default Rust panic handler даёт 101 на Unix но 0xC0000409 на Windows.
    std::panic::set_hook(Box::new(|info| {
        let msg = info.payload().downcast_ref::<&str>().copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<no message>");
        let loc = info.location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "<unknown>".to_string());
        eprintln!("nova: internal error at {}: {}", loc, msg);
        eprintln!("This is a bug in nova. Please report it.");
        std::process::exit(101);
    }));

    let cli = Cli::parse();

    // Plan 36 R10: apply --color setting before any output.
    match ColorMode::parse(&cli.color) {
        Ok(mode) => set_color_mode(mode),
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    }

    let result = match cli.cmd {
        Cmd::Check { paths, jobs, quiet, verbose, list, format, include_runtime, skip } => cmd_check(
            &paths,
            jobs,
            quiet,
            verbose,
            list,
            &format,
            include_runtime,
            &skip,
        ),
        Cmd::Run { file } => cmd_run(&file),
        Cmd::Build { file, output, mode, toolchain, vcvars, clang, timeout, keep_artifacts } => cmd_build(
            &file,
            output.as_deref(),
            &mode,
            &toolchain,
            vcvars.as_deref(),
            clang.as_deref(),
            timeout,
            keep_artifacts,
        ),
        Cmd::Test {
            path, filter, jobs, format, mode, toolchain, vcvars, clang, timeout,
            verbose, quiet, results_file, rerun_failed, retries,
            include_stdlib, keep_artifacts, gc,
            list, filter_from, shuffle, skip,
        } => cmd_test(
            path.as_deref(),
            filter.as_deref(),
            jobs,
            &format,
            &mode,
            &toolchain,
            vcvars.as_deref(),
            clang.as_deref(),
            timeout,
            verbose,
            quiet,
            results_file.as_deref(),
            rerun_failed,
            retries,
            include_stdlib,
            keep_artifacts,
            gc.as_deref().unwrap_or("boehm"),
            list,
            filter_from.as_deref(),
            shuffle,
            &skip,
        ),
        Cmd::TestBuild { file, mode, toolchain, vcvars, clang, timeout, keep_artifacts, gc } => cmd_test_build(
            &file,
            &mode,
            &toolchain,
            vcvars.as_deref(),
            clang.as_deref(),
            timeout,
            keep_artifacts,
            gc.as_deref().unwrap_or("boehm"),
        ),
        Cmd::RegenRuntime { check } => cmd_regen_runtime(check),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {}", bold(&red("error:")), e);
            // Plan 36 R7: usage errors → exit=2, diagnostics → exit=1.
            // Discriminate через downcast UsageError type.
            if e.downcast_ref::<UsageError>().is_some() {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE  // = 1
            }
        }
    }
}
