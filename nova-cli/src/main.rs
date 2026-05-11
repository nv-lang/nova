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

// ---------- CLI definition ----------

#[derive(Parser)]
#[command(
    name = "nova",
    version,
    about = "Nova language CLI — build, run, test Nova programs"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Type-check a Nova source file.
    Check {
        file: PathBuf,
    },
    /// Run a Nova source file via the interpreter.
    Run {
        file: PathBuf,
    },
    /// Compile a Nova source file to a native binary.
    Build {
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
    /// Run all Nova tests in nova_tests/.
    Test {
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
        /// Path to nova_tests/ directory (default: auto from nova.toml root).
        #[arg(long = "tests-dir")]
        tests_dir: Option<PathBuf>,
        /// GC backend: 'boehm' (default after Plan 27) or 'malloc' (no GC, internal only).
        /// Reserved for Plan 27 — currently accepted but has no effect.
        #[arg(long, value_parser = ["boehm", "malloc"])]
        gc: Option<String>,
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
// Minimal ANSI — no extra deps. Disabled when stdout/stderr is not a tty
// or when NO_COLOR env var is set (https://no-color.org).

fn colors_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    // On Windows, ANSI is supported in Windows Terminal / modern conhost.
    // Check TERM or just always enable — worst case: harmless escape codes.
    true
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

fn cmd_check(path: &Path) -> Result<()> {
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
    println!("{} {}", green("ok:"), path.display());
    Ok(())
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
        bail!("--timeout must be >= 1 second");
    }
    let build_start = std::time::Instant::now();
    let repo = find_repo_root()?;
    let paths = resolve_paths(&repo);

    let path = path.canonicalize()
        .map_err(|e| anyhow!("cannot resolve path {}: {}", path.display(), e))?;
    let src = read_file(&path)?;
    let path_str = path.to_string_lossy();

    // parse + typecheck + codegen
    let mut module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    check_module_path(&path, &module)?;
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
    let c_code = emitter
        .emit_module(&module)
        .map_err(|e| anyhow!("codegen error: {}", e))?;

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
    let vcvars_path = match &tc {
        test_runner::Toolchain::Clang { vcvars, .. } => vcvars.as_deref(),
        test_runner::Toolchain::Msvc { vcvars } => Some(vcvars.as_path()),
        test_runner::Toolchain::Gcc { .. } => None,
    };
    let libuv = test_runner::detect_or_build_libuv(&paths.rt_dir, &repo, vcvars_path);

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
    tests_dir_override: Option<&Path>,
    gc: &str,
) -> Result<()> {
    if timeout_secs == 0 {
        bail!("--timeout must be >= 1 second");
    }
    if let Some(f) = filter {
        if f.is_empty() {
            bail!("--filter cannot be empty");
        }
    }
    if verbose && quiet {
        bail!("cannot use --verbose and --quiet simultaneously");
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

    // extract vcvars before tc is moved into TestAllOpts
    let vcvars_from_tc: Option<PathBuf> = match &tc {
        test_runner::Toolchain::Clang { vcvars, .. } => vcvars.clone(),
        test_runner::Toolchain::Msvc { vcvars } => Some(vcvars.clone()),
        test_runner::Toolchain::Gcc { .. } => None,
    };

    let libuv = test_runner::detect_or_build_libuv(
        &paths.rt_dir,
        &repo,
        vcvars_from_tc.as_deref(),
    );

    if format == test_runner::OutputFormat::Text {
        eprintln!(
            "Toolchain: {}, mode={:?}, jobs={}, tests-dir={}",
            tc.name(),
            mode,
            jobs,
            tests_dir_override
                .unwrap_or(&paths.tests_dir)
                .display()
        );
        if libuv.is_some() {
            eprintln!("libuv: enabled");
        } else {
            eprintln!("libuv: disabled");
        }
    }

    let tests_dir = tests_dir_override
        .map(Path::to_path_buf)
        .unwrap_or(paths.tests_dir);
    if !tests_dir.is_dir() {
        bail!("tests directory not found: {}", tests_dir.display());
    }
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
    let tmp_dir = default_tmp_dir();
    let opts = test_runner::TestAllOpts {
        tests_dir: &tests_dir,
        stdlib_dir: stdlib_dir_opt,
        include_stdlib,
        filter,
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

    let vcvars_path = match &tc {
        test_runner::Toolchain::Clang { vcvars, .. } => vcvars.as_deref(),
        test_runner::Toolchain::Msvc { vcvars } => Some(vcvars.as_path()),
        test_runner::Toolchain::Gcc { .. } => None,
    };
    let libuv = test_runner::detect_or_build_libuv(&paths.rt_dir, &repo, vcvars_path);

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
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Check { file } => cmd_check(&file),
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
            filter, jobs, format, mode, toolchain, vcvars, clang, timeout,
            verbose, quiet, results_file, rerun_failed, retries,
            include_stdlib, keep_artifacts, tests_dir, gc,
        } => cmd_test(
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
            tests_dir.as_deref(),
            gc.as_deref().unwrap_or("malloc"),
        ),
        Cmd::TestBuild { file, mode, toolchain, vcvars, clang, timeout, keep_artifacts, gc } => cmd_test_build(
            &file,
            &mode,
            &toolchain,
            vcvars.as_deref(),
            clang.as_deref(),
            timeout,
            keep_artifacts,
            gc.as_deref().unwrap_or("malloc"),
        ),
        Cmd::RegenRuntime { check } => cmd_regen_runtime(check),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {}", bold(&red("error:")), e);
            ExitCode::FAILURE
        }
    }
}
