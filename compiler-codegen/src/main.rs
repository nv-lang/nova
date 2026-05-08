//! Nova codegen compiler CLI.

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
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

fn main() -> ExitCode {
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
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
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
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let messages: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        anyhow!("{}", messages.join("\n"))
    })?;
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
    let c_code = emitter
        .emit_module(&module)
        .map_err(|e| anyhow!("codegen error: {}", e))?;

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
    let module = nova_codegen::parser::parse(&src).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    check_module_path(path, &module)?;
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
