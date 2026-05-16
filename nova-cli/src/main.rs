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
    /// Plan 45 / D107: produce documentation for a Nova source file.
    ///
    /// MVP: one file at a time, output to stdout. Supported formats:
    /// `markdown` (default), `json` (D107 schema v1).
    Doc {
        /// Path to a `.nv` file or directory. Optional when `--json-schema` is used.
        #[arg(num_args = 0..=1)]
        file: Option<PathBuf>,
        /// Output format: `markdown` (default) or `json`.
        #[arg(long = "format", default_value = "markdown")]
        format: String,
        /// Print the embedded JSON Schema 2020-12 and exit (offline-
        /// validation, IDE auto-completion, LLM prompt context).
        #[arg(long = "json-schema")]
        json_schema: bool,
        /// Include private (non-exported) items in output.
        /// By default only items marked `export` are documented.
        #[arg(long = "include-private")]
        include_private: bool,
        /// Plan 45 Ф.7: run doc-tests (`nova` fenced code blocks) instead
        /// of rendering. Reports pass/fail/skipped per test.
        #[arg(long = "test")]
        run_doc_tests: bool,
        /// Plan 45 Ф.14: validate doc-content without rendering. Reports
        /// broken intra-doc-links and missing summaries; exits non-zero
        /// on any issue. Useful in CI.
        #[arg(long = "check")]
        check: bool,
        /// Plan 45 Ф.15: re-render on file change (mtime poll, 500ms).
        /// Ctrl-C to exit. Works with --format and --check.
        #[arg(long = "watch")]
        watch: bool,
        /// Plan 45 Ф.21.6 / D105: report doc-coverage metrics
        /// (% items with summary, broken down by kind). Useful in CI.
        #[arg(long = "coverage")]
        coverage: bool,
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
/// Plan 35 sub-plan 35.B (sync): workspace-aware lookup из CWD. Использует
/// `nova_codegen::test_runner::find_repo_root_from` — тот же helper что
/// test_runner pipeline (D78 AD6: prefer `[workspace]`-marked nova.toml,
/// иначе topmost nova.toml). Без sync nova-cli мог найти первый встреченный
/// nova.toml (nova_tests/nova.toml в nested-repos), что ломало std-import
/// resolve.
fn find_repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| anyhow!("cannot determine current directory: {}", e))?;
    // find_repo_root_from принимает path к файлу — даём ему синтетический
    // path в cwd (parent будет cwd), что симулирует «ищем root от cwd».
    let probe = cwd.join("__novacli_probe__.nv");
    nova_codegen::test_runner::find_repo_root_from(&probe)
        .or_else(|| {
            // Fallback: если canonicalize не сработал (probe не существует),
            // walk вверх от cwd напрямую.
            let mut dir = cwd.clone();
            let mut last_toml_dir: Option<PathBuf> = None;
            loop {
                let toml = dir.join("nova.toml");
                if toml.exists() {
                    if let Ok(content) = std::fs::read_to_string(&toml) {
                        if content.contains("[workspace]") {
                            return Some(dir);
                        }
                    }
                    last_toml_dir = Some(dir.clone());
                }
                match dir.parent() {
                    Some(p) if p != dir => dir = p.to_path_buf(),
                    _ => return last_toml_dir,
                }
            }
        })
        .ok_or_else(|| anyhow!(
            "nova.toml not found — are you inside a Nova project?\n\
             (Searched from {} up to filesystem root.)",
            std::env::current_dir().unwrap_or_default().display()
        ))
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
    // Plan 35 sub-plan 35.A R27: вызываем безусловно (resolver auto-добавит
    // prelude если std/prelude.nv существует, даже без explicit imports).
    {
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
    let mut module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    check_module_path(path, &module)?;
    // Plan 50 Ф.2 (закрытие [M-interp-named]): cross-file resolve через
    // inline expansion — тот же codepath, что в `cmd_build` и
    // `test_runner::codegen_to_c`. Импортированные callee мёрджатся в
    // `module` ДО type-check → `callnorm` ниже видит ВСЕ сигнатуры (в
    // т.ч. дефолты импортированных функций) и раскладывает named args
    // корректно. Раньше `cmd_run` нормализовал только single-file —
    // переставленные named для импортированного callee давали неверный
    // результат в `nova run` ([M-interp-named]).
    //
    // Graceful: если файл вне Nova-проекта (нет nova.toml) — repo не
    // найден, resolve пропускается; single-file без импортов работает
    // и так (prelude auto-import тоже требует repo — поведение
    // консистентно с отсутствием stdlib вне проекта).
    if let Some(repo) = nova_codegen::test_runner::find_repo_root_from(path) {
        let stdlib_dir = repo.join("std");
        nova_codegen::imports::resolve_imports_inline(path, &mut module, &repo, &stdlib_dir)
            .map_err(|e| anyhow!("import resolution: {}", e))?;
    }
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let msgs: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path_str))
            .collect();
        anyhow!("{}", msgs.join("\n"))
    })?;
    // Plan 46 (D102) Ф.2: нормализация call-site для treewalk-interp —
    // named args → positional + вставка defaults. После resolve_imports
    // (нужны все сигнатуры) и type-check, до запуска интерпретатора.
    nova_codegen::callnorm::normalize_module(&mut module);
    let mut interp = nova_codegen::interp::Interpreter::new();
    interp
        .load_module(&module)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    interp
        .run_main()
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    Ok(())
}

/// Plan 45 Ф.12 / D107: `nova doc <file> [--format markdown|json]
/// [--json-schema]`.
///
/// MVP: один входной файл, вывод в stdout. Никаких подкоманд (workspace/
/// --output-dir/--watch — Plan 45.A или отдельные субкоманды позже).
fn cmd_doc(path: &Path, format: &str, json_schema: bool, include_private: bool, run_doc_tests: bool, check: bool, watch: bool, coverage: bool) -> Result<()> {
    // `--json-schema` — печатает embedded схему и выходит (D107).
    if json_schema {
        println!("{}", nova_doc_embedded_schema());
        return Ok(());
    }
    // Plan 45 Ф.21.7: workspace-режим. Если path — каталог, рекурсивно
    // парсим все *.nv и строим multi-module DocTree.
    if path.is_dir() {
        return cmd_doc_workspace(path, format, include_private, run_doc_tests, check, coverage);
    }
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }
    if watch {
        return cmd_doc_watch(path, format, include_private, run_doc_tests, check, coverage);
    }
    let src = read_file(path)?;
    let path_str = path.to_string_lossy();
    let mut module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    check_module_path(path, &module)?;
    // Plan 45 MVP: для single-file mode `nova doc <file>` НЕ резолвим
    // импорты — иначе items из auto-imported std/prelude и других
    // модулей попадают в output. Это даёт "documentation of THIS
    // file" по дефолту. Workspace-режим (`nova doc --workspace`) и
    // multi-module DocTree — Plan 45.A.
    //
    // Без resolve_imports type-check может ругаться на cross-file
    // символы. Для MVP doc-pipeline'а мы прощаем type-check ошибки
    // (но всё ещё парсим — без parse fail нельзя получить AST).
    // Если type-check падает — продолжаем с partial information;
    // production-grade `nova doc --check` (Plan 45 Ф.14) будет
    // делать полный type-check.
    let _ = nova_codegen::types::check_module(&module);
    nova_codegen::types::infer_effects(&mut module);
    let mut tree = nova_codegen::doc::build(&module);
    // Plan 45 Ф.22.3 / D107: source_root = parent dir файла, relative
    // от CWD (для portable/deterministic output across machines).
    tree.source_root = path.parent().map(|d| {
        let s = d.display().to_string();
        if s.is_empty() { ".".to_string() } else { s.replace('\\', "/") }
    });
    if !include_private {
        nova_codegen::doc::strip_private(&mut tree);
    }
    if coverage {
        return cmd_doc_coverage(&tree);
    }
    if check {
        return cmd_doc_check(&tree);
    }
    if run_doc_tests {
        let summary = nova_codegen::doc::test_runner::run_doc_tests_with_source(&tree.doc_tests, Some(&src));
        let total = summary.results.len();
        let passed = summary.passed();
        let failed = summary.failed();
        let skipped = summary.skipped();
        for r in &summary.results {
            match &r.outcome {
                nova_codegen::doc::test_runner::DocTestOutcome::Passed => {
                    println!("ok   {}", r.id);
                }
                nova_codegen::doc::test_runner::DocTestOutcome::Failed(msg) => {
                    println!("FAIL {} — {}", r.id, msg);
                }
                nova_codegen::doc::test_runner::DocTestOutcome::Skipped(reason) => {
                    println!("skip {} ({})", r.id, reason);
                }
            }
        }
        println!(
            "\ndoc-tests: {} passed, {} failed, {} skipped (total {})",
            passed, failed, skipped, total
        );
        if !summary.all_passed() {
            std::process::exit(1);
        }
        return Ok(());
    }
    let out = match format {
        "markdown" | "md" => nova_codegen::doc::render_markdown(&tree),
        "json" => nova_codegen::doc::render_json_with_source(&tree, &src),
        other => {
            return Err(usage_err(format!(
                "unknown --format `{}` (supported: `markdown`, `json`)",
                other
            )));
        }
    };
    print!("{}", out);
    Ok(())
}

/// Plan 45 Ф.21.7 — workspace mode. Рекурсивно собирает все *.nv
/// в каталоге, парсит каждый, строит unified DocTree (cross-module
/// intra-doc-links резолвятся). Поддерживает --format / --check /
/// --test / --coverage / --include-private.
fn cmd_doc_workspace(
    dir: &Path,
    format: &str,
    include_private: bool,
    run_doc_tests: bool,
    check: bool,
    coverage: bool,
) -> Result<()> {
    let mut files: Vec<PathBuf> = Vec::new();
    walk_nv_files(dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        bail!("no .nv files found under `{}`", dir.display());
    }
    // Plan 45 Ф.22.5 / D107: workspace graceful-fail — продолжаем при
    // parse-ошибках в отдельных файлах (как rustdoc), выводим warnings
    // в stderr. Hard-fail только если 0 файлов распарсилось.
    let mut modules: Vec<nova_codegen::ast::Module> = Vec::with_capacity(files.len());
    // Для Ф.22.7: храним source каждого файла рядом с модулем.
    let mut sources: Vec<String> = Vec::with_capacity(files.len());
    let mut parse_warnings: Vec<String> = Vec::new();
    for f in &files {
        let src = match read_file(f) {
            Ok(s) => s,
            Err(e) => {
                parse_warnings.push(format!("warning: {}: {}", f.display(), e));
                continue;
            }
        };
        let path_str = f.to_string_lossy();
        match nova_codegen::parser::parse(&src) {
            Ok(mut m) => {
                let _ = nova_codegen::types::check_module(&m);
                nova_codegen::types::infer_effects(&mut m);
                sources.push(src);
                modules.push(m);
            }
            Err(d) => {
                parse_warnings.push(format!(
                    "warning: {}: {}",
                    path_str,
                    d.render(&src, &path_str)
                ));
            }
        }
    }
    for w in &parse_warnings {
        eprintln!("{}", w);
    }
    if modules.is_empty() && !files.is_empty() {
        bail!("все файлы в `{}` содержат ошибки парсинга", dir.display());
    }
    let mut tree = nova_codegen::doc::build_workspace(&modules);
    // Plan 45 Ф.22.3 / D107: workspace source_root = переданный dir,
    // как есть (relative от CWD для portability).
    tree.source_root = Some(dir.display().to_string().replace('\\', "/"));
    if !include_private {
        nova_codegen::doc::strip_private(&mut tree);
    }
    if coverage {
        return cmd_doc_coverage(&tree);
    }
    if check {
        return cmd_doc_check(&tree);
    }
    if run_doc_tests {
        // Plan 45 Ф.22.7: workspace doc-tests с crate-scope.
        // Каждый тест получает source своего исходного модуля для
        // crate-scope injection (аналог single-file Ф.21.1).
        // Используем concatenated sources как best-effort fallback:
        // doc-test'ам нужен только scope своего модуля, но merged
        // sources покрывают cross-module cases тоже.
        let combined_source = sources.join("\n\n");
        let summary = nova_codegen::doc::test_runner::run_doc_tests_with_source(
            &tree.doc_tests,
            Some(&combined_source),
        );
        return print_doc_test_summary(summary);
    }
    let out = match format {
        "markdown" | "md" => nova_codegen::doc::render_markdown(&tree),
        "json" => nova_codegen::doc::render_json(&tree),
        other => bail!("unknown --format `{}`", other),
    };
    print!("{}", out);
    Ok(())
}

fn walk_nv_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            walk_nv_files(&p, out)?;
        } else if p.extension().map(|e| e == "nv").unwrap_or(false) {
            out.push(p);
        }
    }
    Ok(())
}

/// Helper для check-mode (используется и single-file и workspace).
/// Plan 45 Ф.23.12: теперь агрегирует все §11.5 lints.
fn cmd_doc_check(tree: &nova_codegen::doc::DocTree) -> Result<()> {
    let mut issues: Vec<String> = Vec::new();
    // Existing: broken links + missing summaries.
    for link in &tree.links {
        if link.target_id.is_none() {
            let from = link.from_id.as_deref().unwrap_or("<module>");
            issues.push(format!("[broken-link] [{}] in {}", link.text, from));
        }
    }
    for m in &tree.modules {
        for it in &m.items {
            if it.visibility == nova_codegen::doc::Visibility::Export && it.summary.is_none() {
                issues.push(format!("[missing-summary] exported item `{}`", it.id));
            }
        }
    }
    // Plan 45 Ф.23.12: additional §11.5 lints.
    for v in nova_codegen::doc::run_lints(tree) {
        issues.push(format!("[{}] {}: {}", v.rule, v.item_id, v.message));
    }
    if issues.is_empty() {
        println!("doc-check: ok ({} item(s), {} link(s))",
            tree.modules.iter().map(|m| m.items.len()).sum::<usize>(),
            tree.links.len());
        return Ok(());
    }
    for i in &issues {
        eprintln!("doc-check: {}", i);
    }
    eprintln!("\ndoc-check: {} issue(s)", issues.len());
    std::process::exit(1);
}

/// Helper: print doc-test summary + exit 1 on failures.
fn print_doc_test_summary(summary: nova_codegen::doc::test_runner::DocTestSummary) -> Result<()> {
    let total = summary.results.len();
    let passed = summary.passed();
    let failed = summary.failed();
    let skipped = summary.skipped();
    for r in &summary.results {
        match &r.outcome {
            nova_codegen::doc::test_runner::DocTestOutcome::Passed => println!("ok   {}", r.id),
            nova_codegen::doc::test_runner::DocTestOutcome::Failed(msg) => println!("FAIL {} — {}", r.id, msg),
            nova_codegen::doc::test_runner::DocTestOutcome::Skipped(reason) => println!("skip {} ({})", r.id, reason),
        }
    }
    println!("\ndoc-tests: {} passed, {} failed, {} skipped (total {})", passed, failed, skipped, total);
    if !summary.all_passed() {
        std::process::exit(1);
    }
    Ok(())
}

/// Plan 45 Ф.21.6 / D105: doc-coverage метрика. Считает items
/// (всего/задокументированных) и links (всего/broken). Output на
/// stdout, exit code = процент-непокрытых (0 если 100% покрыто).
fn cmd_doc_coverage(tree: &nova_codegen::doc::DocTree) -> Result<()> {
    use std::collections::BTreeMap;
    let mut total: BTreeMap<&str, usize> = BTreeMap::new();
    let mut documented: BTreeMap<&str, usize> = BTreeMap::new();
    let mut with_examples: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &tree.modules {
        for it in &m.items {
            let kind: &str = match &it.kind {
                nova_codegen::doc::ItemKind::Fn(_) => "fn",
                nova_codegen::doc::ItemKind::Type(_) => "type",
                nova_codegen::doc::ItemKind::Const { .. } => "const",
                nova_codegen::doc::ItemKind::Effect { .. } => "effect",
                nova_codegen::doc::ItemKind::Protocol { .. } => "protocol",
            };
            *total.entry(kind).or_insert(0) += 1;
            if it.summary.is_some() {
                *documented.entry(kind).or_insert(0) += 1;
            }
            // Plan 45 Ф.23.19: track items with Examples section or doc-tests.
            let has_examples = it.sections.contains_key("examples")
                || tree.doc_tests.iter().any(|t| t.from_id.as_deref() == Some(&it.id));
            if has_examples {
                *with_examples.entry(kind).or_insert(0) += 1;
            }
        }
    }
    let total_items: usize = total.values().sum();
    let documented_items: usize = documented.values().sum();
    let total_with_examples: usize = with_examples.values().sum();
    let total_links = tree.links.len();
    let broken_links = tree.links.iter().filter(|l| l.target_id.is_none()).count();

    println!("doc-coverage:");
    println!("  items: {}/{} documented ({:.1}%)",
        documented_items, total_items,
        if total_items == 0 { 100.0 } else { 100.0 * documented_items as f64 / total_items as f64 });
    println!("  examples: {}/{} with examples ({:.1}%)",
        total_with_examples, total_items,
        if total_items == 0 { 100.0 } else { 100.0 * total_with_examples as f64 / total_items as f64 });
    for (kind, &total_n) in &total {
        let doc_n = documented.get(kind).copied().unwrap_or(0);
        let ex_n = with_examples.get(kind).copied().unwrap_or(0);
        println!("    {}: {}/{} documented, {}/{} with examples", kind, doc_n, total_n, ex_n, total_n);
    }
    println!("  links: {}/{} resolved ({} broken)",
        total_links - broken_links, total_links, broken_links);
    Ok(())
}

/// Plan 45 Ф.15: watch mode — re-render при изменении `path` (mtime
/// poll каждые 500ms). Без `notify` dep'ы для minimal footprint.
/// Ctrl-C завершает loop.
fn cmd_doc_watch(
    path: &Path,
    format: &str,
    include_private: bool,
    run_doc_tests: bool,
    check: bool,
    coverage: bool,
) -> Result<()> {
    let mut last_mtime: Option<std::time::SystemTime> = None;
    eprintln!("nova doc --watch: monitoring {} (Ctrl-C to exit)", path.display());
    loop {
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if mtime != last_mtime {
            last_mtime = mtime;
            // Очистка экрана + cursor home (ANSI). Сохраняем scrollback.
            eprint!("\x1b[2J\x1b[H");
            eprintln!(
                "─── nova doc --watch ({}) ───",
                chrono_like_now()
            );
            // Re-run одним проходом через cmd_doc (без watch/json_schema).
            match cmd_doc(path, format, false, include_private, run_doc_tests, check, false, coverage) {
                Ok(_) => {}
                Err(e) => eprintln!("error: {}", e),
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// MVP timestamp для watch-header'а. Без chrono dep'ы — простая
/// HH:MM:SS из SystemTime (UTC).
fn chrono_like_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let rem = now % 86_400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{:02}:{:02}:{:02} UTC", h, m, s)
}

/// Embedded JSON Schema (D107) — пока минимальная заглушка с
/// корректным `format_version` дискриминатором. Полная схема —
/// Plan 45 Ф.9 (schema.rs); этот placeholder уже валидный JSON Schema
/// 2020-12, описывающий обязательный `format_version: u32` (1) и
/// высокоуровневый shape.
fn nova_doc_embedded_schema() -> &'static str {
    nova_codegen::doc::schema::schema_v1()
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

    // Plan 46 (D102) Ф.2: нормализация call-site — named args → positional
    // + вставка defaults. После type-check, до codegen.
    nova_codegen::callnorm::normalize_module(&mut module);

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
        Cmd::Doc { file, format, json_schema, include_private, run_doc_tests, check, watch, coverage } => {
            // Plan 45 Ф.23.17: --json-schema works without FILE argument.
            if json_schema && file.is_none() {
                println!("{}", nova_doc_embedded_schema());
                return ExitCode::SUCCESS;
            }
            let path = file.as_deref().unwrap_or_else(|| {
                eprintln!("error: FILE argument required (unless --json-schema)");
                std::process::exit(1);
            });
            cmd_doc(path, &format, json_schema, include_private, run_doc_tests, check, watch, coverage)
        }
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
