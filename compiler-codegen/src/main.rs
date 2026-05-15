//! Nova codegen compiler CLI.

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "nova-codegen", version, about = "Nova codegen compiler — compiles Nova to C")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Type-check файл, без запуска.
    Check { file: PathBuf },
    /// Type-check + интерпретировать (вызывается main).
    Run { file: PathBuf },
    /// Запустить тесты в файле.
    Test { file: PathBuf },
    /// Скомпилировать Nova-файл в C (вывод в stdout или -o файл).
    Compile {
        file: PathBuf,
        /// Выходной .c файл (по умолчанию: <name>.c)
        #[arg(short = 'o')]
        output: Option<PathBuf>,
        /// Не вставлять Nova-исходник как `/* SRC: ... */` комментарии.
        /// По умолчанию аннотации включены — для удобства отладки .c.
        #[arg(long = "no-annotate-source")]
        no_annotate_source: bool,
        /// Отключить lint-проверки (export-fail-untyped и т.д.).
        #[arg(long = "no-lint")]
        no_lint: bool,
    },
    /// Plan 13: auto-gen `std/runtime/string.nv` и `std/runtime/math.nv`
    /// из `runtime_registry.rs`. Файлы перезаписываются.
    /// `--check` режим — сравнить с существующими файлами; если diff,
    /// fail (для CI guard).
    EmitRuntimeStubs {
        /// Корень репозитория (где лежит `std/runtime/`). По умолчанию
        /// текущая директория.
        #[arg(long = "root", default_value = ".")]
        root: PathBuf,
        /// Не записывать; сравнить с существующими и упасть при
        /// несовпадении. Для CI/pre-commit guard.
        #[arg(long = "check")]
        check: bool,
    },
    /// Plan 13: распечатать registry runtime-функций для sanity.
    DumpRuntime,
    /// Plan 24: cross-platform test runner — сборка одного .nv в .exe
    /// и проверка EXPECT-маркера (D89). Заменяет per-file логику из
    /// run_tests.ps1.
    TestBuild {
        file: PathBuf,
        /// dev (по умолчанию) | release.
        #[arg(long, default_value = "dev")]
        mode: String,
        /// auto (по умолчанию) | clang | msvc | gcc.
        #[arg(long, default_value = "auto")]
        toolchain: String,
        /// Путь к vcvars64.bat (Windows). Auto-detect через vswhere.
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Путь к clang.exe (override auto-detect).
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Путь к compiler-codegen/ (для include nova_rt headers).
        /// По умолчанию вычисляется из layout репо.
        #[arg(long = "cg-include")]
        cg_include: Option<PathBuf>,
        /// Путь к compiler-codegen/nova_rt/ (alloc.c/effects.c/fibers.c).
        #[arg(long = "rt-dir")]
        rt_dir: Option<PathBuf>,
        /// Tmp директория для .exe/.obj артефактов.
        #[arg(long = "tmp-dir")]
        tmp_dir: Option<PathBuf>,
        /// Display name (override; по умолчанию — basename файла).
        #[arg(long)]
        display: Option<String>,
        /// Сохранить .c / .exe / .obj артефакты после прогона.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        /// Plan 26 Ф.1: timeout на child-процесс в секундах. Default 60.
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Plan 27 Ф.4: GC backend. boehm = Boehm GC (default). malloc = plain malloc (bench only).
        #[arg(long, value_parser = ["boehm", "malloc"], default_value = "boehm")]
        gc: String,
    },
    /// Plan 24: рекурсивный прогон всех .nv в `--tests-dir`. Заменяет
    /// run_tests.ps1 целиком; .ps1 / .sh wrapper'ы вызывают эту команду.
    TestAll {
        /// Корень nova_tests/ (рекурсивный поиск .nv).
        #[arg(long = "tests-dir", default_value = "nova_tests")]
        tests_dir: PathBuf,
        /// Корень std/ — добавляется если --include-stdlib.
        #[arg(long = "stdlib-dir", default_value = "std")]
        stdlib_dir: PathBuf,
        /// Включить std/* файлы в прогон.
        #[arg(long = "include-stdlib")]
        include_stdlib: bool,
        /// Фильтр по display-name (substring).
        #[arg(long)]
        filter: Option<String>,
        /// dev | release.
        #[arg(long, default_value = "dev")]
        mode: String,
        /// auto | clang | msvc | gcc.
        #[arg(long, default_value = "auto")]
        toolchain: String,
        /// Путь к vcvars64.bat (Windows).
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Путь к clang.exe.
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Путь к compiler-codegen/.
        #[arg(long = "cg-include")]
        cg_include: Option<PathBuf>,
        /// Путь к compiler-codegen/nova_rt/.
        #[arg(long = "rt-dir")]
        rt_dir: Option<PathBuf>,
        /// Tmp директория. По умолчанию $TEMP/nova_tests или /tmp/nova_tests.
        #[arg(long = "tmp-dir")]
        tmp_dir: Option<PathBuf>,
        /// Сохранить .exe/.obj артефакты.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        /// Plan 26 Ф.1: timeout на child-процесс в секундах. Default 60.
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Plan 26 Ф.3: количество параллельных worker'ов. 0 = num_cpus.
        #[arg(long, default_value_t = 0)]
        jobs: usize,
        /// Plan 26 Ф.4: text (default, human) | json | tap.
        #[arg(long, default_value = "text")]
        format: String,
        /// Plan 26 Ф.9: показывать output PASS-тестов тоже.
        #[arg(long, short = 'v')]
        verbose: bool,
        /// Plan 26 Ф.9: только FAIL + summary.
        #[arg(long, short = 'q')]
        quiet: bool,
        /// Plan 26 Ф.10: файл для last-results.json (для --rerun-failed).
        #[arg(long = "results-file")]
        results_file: Option<PathBuf>,
        /// Plan 26 Ф.10: прогнать только тесты которые fail/timeout
        /// в --results-file.
        #[arg(long = "rerun-failed")]
        rerun_failed: bool,
        /// Plan 26 Ф.12: количество retry для transient AV/race fail'ов
        /// (e.g., 'cannot open output file'). 0 = no retry. CI default 2.
        #[arg(long, default_value_t = 0)]
        retries: u32,
        /// Plan 27 Ф.4: GC backend. boehm = Boehm GC (default). malloc = plain malloc (bench only).
        #[arg(long, value_parser = ["boehm", "malloc"], default_value = "boehm")]
        gc: String,
    },
}

/// Запустить lint-проходы и вывести warning'и в stderr.
/// Lint-сообщения уже содержат «warning:» префикс — render через
/// прямой формат (без render() который вставляет «error:»).
fn run_lints(module: &nova_codegen::ast::Module, src: &str, file: &str) {
    let warnings = nova_codegen::lints::lint_module(module);
    for w in warnings {
        let (line, col) = nova_codegen::diag::byte_to_line_col(src, w.diag.span.start);
        eprintln!("{}:{}:{}: {} [{}]", file, line, col, w.diag.message, w.rule);
    }
}

/// D78 path/module enforcement. Если файл лежит внутри пакета
/// (нашли nova.toml в parent dirs), проверяем что declared module
/// соответствует file path относительно source root.
/// Если nova.toml не найден — skip (файл не часть пакета).
fn check_module_path(file: &PathBuf, module: &nova_codegen::ast::Module) -> Result<()> {
    nova_codegen::manifest::check_module_path(file.as_path(), &module.name)
        .map_err(|msg| anyhow!("{}", msg))
}

fn run() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Check { file } => cmd_check(&file),
        Cmd::Run { file } => cmd_run(&file),
        Cmd::Test { file } => cmd_test(&file),
        Cmd::Compile { file, output, no_annotate_source, no_lint } =>
            cmd_compile(&file, output.as_deref(), !no_annotate_source, !no_lint),
        Cmd::EmitRuntimeStubs { root, check } =>
            cmd_emit_runtime_stubs(&root, check),
        Cmd::DumpRuntime => cmd_dump_runtime(),
        Cmd::TestBuild { file, mode, toolchain, vcvars, clang, cg_include, rt_dir, tmp_dir, display, keep_artifacts, timeout, gc } =>
            cmd_test_build(&file, &mode, &toolchain, vcvars.as_deref(), clang.as_deref(), cg_include.as_deref(), rt_dir.as_deref(), tmp_dir.as_deref(), display.as_deref(), keep_artifacts, timeout, &gc),
        Cmd::TestAll { tests_dir, stdlib_dir, include_stdlib, filter, mode, toolchain, vcvars, clang, cg_include, rt_dir, tmp_dir, keep_artifacts, timeout, jobs, format, verbose, quiet, results_file, rerun_failed, retries, gc } =>
            cmd_test_all(&tests_dir, &stdlib_dir, include_stdlib, filter.as_deref(), &mode, &toolchain, vcvars.as_deref(), clang.as_deref(), cg_include.as_deref(), rt_dir.as_deref(), tmp_dir.as_deref(), keep_artifacts, timeout, jobs, &format, verbose, quiet, results_file.as_deref(), rerun_failed, retries, &gc),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// Точка входа: запускаем всё в потоке с увеличенным стеком (32 MiB).
///
/// AST-обходы (type-checker, SCC-based purity inference, SMT encoder)
/// взаимно рекурсивны через expr ↔ block ↔ stmt. На Windows стек
/// главного потока по умолчанию 1 MiB — для глубоко вложенных выражений
/// и больших модулей этого недостаточно. Spawn с explicit stack_size
/// вместо linker flags: portably работает на Windows/Linux/macOS.
fn main() -> ExitCode {
    std::thread::Builder::new()
        .name("nova-main".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .expect("spawn main thread")
        .join()
        .unwrap_or(ExitCode::FAILURE)
}

fn read_file(path: &PathBuf) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| anyhow!("failed to read {}: {}", path.display(), e))
}

fn cmd_check(path: &PathBuf) -> Result<()> {
    let src = read_file(path)?;
    let module = nova_codegen::parser::parse(&src).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    check_module_path(path, &module)?;
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let messages: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        anyhow!("{}", messages.join("\n"))
    })?;
    println!("ok: {} parsed and checked", path.display());
    Ok(())
}

fn cmd_run(path: &PathBuf) -> Result<()> {
    let src = read_file(path)?;
    let mut module = nova_codegen::parser::parse(&src).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    check_module_path(path, &module)?;
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let messages: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        anyhow!("{}", messages.join("\n"))
    })?;
    // Plan 52 Ф.5: десугаринг map-литералов `[k: v]` → block-expression
    // ПОСЛЕ type-check (типы проверены), ДО интерпретации.
    nova_codegen::desugar::desugar_module(&mut module);
    let mut interp = nova_codegen::interp::Interpreter::new();
    interp.load_module(&module).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    interp.run_main().map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    Ok(())
}

fn cmd_compile(path: &PathBuf, output: Option<&std::path::Path>, annotate_source: bool, lint: bool) -> Result<()> {
    let src = read_file(path)?;
    let mut module = nova_codegen::parser::parse(&src).map_err(|d| {
        anyhow!("{}", d.render(&src, &path.to_string_lossy()))
    })?;
    check_module_path(path, &module)?;
    let module_env = nova_codegen::types::check_module(&module).map_err(|errs| {
        let messages: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        anyhow!("{}", messages.join("\n"))
    })?;
    // Plan 52 Ф.4: десугаринг map-литералов `[k: v]` → block-expression
    // ПОСЛЕ type-check, ДО effect-inference и codegen. После прохода
    // codegen видит обычные method-call'ы (with_capacity / insert).
    nova_codegen::desugar::desugar_module(&mut module);
    // D28: effect inference для private fn — добавить `Fail` если throw
    // в теле и нет явного Fail в effect-row.
    nova_codegen::types::infer_effects(&mut module);
    if lint {
        run_lints(&module, &src, &path.to_string_lossy());
    }

    let mut emitter = nova_codegen::codegen::CEmitter::new();
    // Plan 14 std-fix: source передаётся всегда — для line:col в
    // codegen-ошибках. Активация SRC-комментариев — отдельный флаг.
    emitter.set_source_for_annotations(src.clone());
    if !annotate_source {
        emitter.disable_source_annotations();
    }
    // Plan 33.3 Ф.9.9: передаём proven контракты в codegen для
    // selective stripping (true zero-cost даже в debug).
    emitter.set_proven_contracts(&module_env.proven_contracts);
    let (c_code, warnings) = emitter
        .emit_module(&module)
        .map_err(|e| anyhow!("codegen error: {}", e))?;
    for w in &warnings {
        eprintln!("{}", w);
    }

    let out_path = match output {
        Some(p) => p.to_path_buf(),
        None => path.with_extension("c"),
    };
    std::fs::write(&out_path, &c_code)
        .map_err(|e| anyhow!("failed to write {}: {}", out_path.display(), e))?;
    eprintln!("ok: {} -> {}", path.display(), out_path.display());
    Ok(())
}

fn cmd_test(path: &PathBuf) -> Result<()> {
    let src = read_file(path)?;
    let mut module = nova_codegen::parser::parse(&src).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    check_module_path(path, &module)?;
    // Plan 52 Ф.5: десугаринг map-литералов перед интерпретацией тестов.
    nova_codegen::desugar::desugar_module(&mut module);
    let mut interp = nova_codegen::interp::Interpreter::new();
    interp.load_module(&module).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    let (passed, failed, failed_names) = interp.run_tests().map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    println!("tests: {} passed, {} failed", passed, failed);
    if failed > 0 {
        for name in failed_names {
            println!("  FAIL: {}", name);
        }
        return Err(anyhow!("{} test(s) failed", failed));
    }
    Ok(())
}

/// Plan 13 Ф.3: emit-runtime-stubs.
/// Generates `std/runtime/string.nv` + `std/runtime/math.nv` из
/// `runtime_registry.rs`. С `--check` сравнивает с существующими и
/// fail'ит при diff'е.
fn cmd_emit_runtime_stubs(root: &PathBuf, check: bool) -> Result<()> {
    use nova_codegen::codegen::runtime_registry;
    let registry = runtime_registry::all();
    let groups = runtime_registry::group_by_module(&registry);
    let mut total_files = 0;
    let mut diffed_files: Vec<String> = Vec::new();
    for (module, fns) in &groups {
        let rel_path = runtime_registry::module_to_path(module);
        let abs_path = root.join(&rel_path);
        let content = runtime_registry::render_nv(module, fns);
        if check {
            let existing = std::fs::read_to_string(&abs_path)
                .map_err(|e| anyhow!("failed to read {}: {}", abs_path.display(), e))?;
            // Normalize line endings (Windows CRLF vs LF).
            let norm = |s: &str| s.replace("\r\n", "\n");
            if norm(&existing) != norm(&content) {
                diffed_files.push(rel_path);
            }
        } else {
            // Ensure parent dir exists.
            if let Some(parent) = abs_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow!("failed to create {}: {}", parent.display(), e))?;
            }
            std::fs::write(&abs_path, &content)
                .map_err(|e| anyhow!("failed to write {}: {}", abs_path.display(), e))?;
            println!("wrote {}", rel_path);
        }
        total_files += 1;
    }
    if check {
        if !diffed_files.is_empty() {
            return Err(anyhow!(
                "auto-generated files diverge from registry:\n  {}\n\
                 Run `nova-codegen emit-runtime-stubs` to regenerate.",
                diffed_files.join("\n  ")
            ));
        }
        println!("OK: {} runtime stub file(s) match registry.", total_files);
    } else {
        println!("emitted {} runtime stub file(s).", total_files);
    }
    Ok(())
}

/// Plan 13: dump runtime registry для sanity-check'а.
fn cmd_dump_runtime() -> Result<()> {
    use nova_codegen::codegen::runtime_registry;
    let registry = runtime_registry::all();
    let groups = runtime_registry::group_by_module(&registry);
    println!("Nova runtime registry: {} function(s) total.", registry.len());
    for (module, fns) in &groups {
        println!("\n=== {} ({} fns) ===", module, fns.len());
        for f in fns {
            let recv = match f.receiver {
                Some(r) => format!("{} ", r),
                None => String::new(),
            };
            let dot = if f.is_static { "." } else { "@" };
            let mu = if f.is_mut { "mut " } else { "" };
            let params: Vec<String> = f.params.iter()
                .map(|(n, ty)| format!("{} {}", n, ty))
                .collect();
            println!(
                "  {}{}{}{}({}) -> {}    [c: {}]",
                recv, mu, dot, f.name, params.join(", "), f.return_ty, f.c_name
            );
        }
    }
    Ok(())
}

// ---------- Plan 24: cross-platform test runner ----------

use nova_codegen::test_runner;

fn default_repo_root() -> PathBuf {
    // Если запущен из корня репо (cwd = nova-lang) — `.`. Если из любого
    // другого места — пользователь передаст явные --tests-dir / --cg-include.
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
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

#[allow(clippy::too_many_arguments)]
fn cmd_test_build(
    file: &PathBuf,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    cg_include: Option<&Path>,
    rt_dir: Option<&Path>,
    tmp_dir: Option<&Path>,
    display: Option<&str>,
    keep_artifacts: bool,
    timeout_secs: u64,
    gc: &str,
) -> Result<()> {
    let mode = test_runner::Mode::parse(mode)?;
    let pref = test_runner::ToolchainPref::parse(toolchain)?;
    let repo_root = default_repo_root();
    let cg_include_buf = cg_include
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo_root.join("compiler-codegen"));
    let rt_dir_buf = rt_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo_root.join("compiler-codegen").join("nova_rt"));
    let tmp_dir_buf = tmp_dir.map(Path::to_path_buf).unwrap_or_else(default_tmp_dir);
    std::fs::create_dir_all(&tmp_dir_buf)
        .map_err(|e| anyhow!("create tmp_dir {}: {}", tmp_dir_buf.display(), e))?;

    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    let display_owned = match display {
        Some(d) => d.to_string(),
        None => file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<unknown>".to_string()),
    };
    let libuv = test_runner::detect_or_build_libuv(&rt_dir_buf, &repo_root, vcvars);
    let gc_kind = test_runner::GcKind::parse(gc)?;
    let opts = test_runner::TestBuildOpts {
        nv_file: file,
        toolchain: &tc,
        mode,
        cg_include: &cg_include_buf,
        rt_dir: &rt_dir_buf,
        tmp_dir: &tmp_dir_buf,
        display: &display_owned,
        keep_artifacts,
        libuv: libuv.as_ref(),
        timeout: std::time::Duration::from_secs(timeout_secs),
        gc_kind,
        verbosity: test_runner::Verbosity::Normal,
    };
    let status = test_runner::run_one(&opts);
    let label = status.label();
    let detail = status.detail();
    if detail.is_empty() {
        println!("{:<14} {}", label, display_owned);
    } else {
        println!("{:<14} {}  # {}", label, display_owned, detail);
    }
    if status.is_pass() {
        Ok(())
    } else {
        Err(anyhow!("test failed: {}", display_owned))
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_test_all(
    tests_dir: &PathBuf,
    stdlib_dir: &PathBuf,
    include_stdlib: bool,
    filter: Option<&str>,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    cg_include: Option<&Path>,
    rt_dir: Option<&Path>,
    tmp_dir: Option<&Path>,
    keep_artifacts: bool,
    timeout_secs: u64,
    jobs: usize,
    format: &str,
    verbose: bool,
    quiet: bool,
    results_file: Option<&Path>,
    rerun_failed: bool,
    retries: u32,
    gc: &str,
) -> Result<()> {
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
    let repo_root = default_repo_root();
    let cg_include_buf = cg_include
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo_root.join("compiler-codegen"));
    let rt_dir_buf = rt_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo_root.join("compiler-codegen").join("nova_rt"));
    let tmp_dir_buf = tmp_dir.map(Path::to_path_buf).unwrap_or_else(default_tmp_dir);

    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    // Plan 26 Ф.8: information messages в stderr (как у cargo); per-test
    // events и summary — в stdout. Wrappers смогут просто прогонять stdout.
    if format == test_runner::OutputFormat::Text {
        eprintln!(
            "Toolchain: {}, mode={:?}, jobs={}, tests-dir={}",
            tc.name(),
            mode,
            jobs,
            tests_dir.display()
        );
    }

    let libuv = test_runner::detect_or_build_libuv(&rt_dir_buf, &repo_root, vcvars);
    if format == test_runner::OutputFormat::Text {
        if libuv.is_some() {
            eprintln!("libuv: enabled");
        } else {
            eprintln!("libuv: disabled (Time.sleep через busy-yield fallback)");
        }
    }

    let stdlib_dir_opt = if include_stdlib {
        Some(stdlib_dir.as_path())
    } else {
        None
    };
    let gc_kind = test_runner::GcKind::parse(gc)?;
    let opts = test_runner::TestAllOpts {
        tests_dir,
        stdlib_dir: stdlib_dir_opt,
        include_stdlib,
        filter,
        mode,
        toolchain: tc,
        cg_include: &cg_include_buf,
        rt_dir: &rt_dir_buf,
        tmp_dir: &tmp_dir_buf,
        keep_artifacts,
        libuv,
        timeout: std::time::Duration::from_secs(timeout_secs),
        jobs,
        format,
        verbosity,
        cache_dir: None, // Ф.5 — не реализовано, оставлен крючок в opts.
        results_file,
        rerun_failed,
        retries,
        gc_kind,
        list_only: false,
        filter_from: None,
        shuffle_seed: None,
        skip: &[],
    };
    let summary = test_runner::run_all(opts)?;
    test_runner::print_summary(&summary, format);

    if summary.fail > 0 {
        Err(anyhow!("{} test(s) failed", summary.fail))
    } else {
        Ok(())
    }
}
