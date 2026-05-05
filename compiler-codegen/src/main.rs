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
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Check { file } => cmd_check(&file),
        Cmd::Run { file } => cmd_run(&file),
        Cmd::Test { file } => cmd_test(&file),
        Cmd::Compile { file, output } => cmd_compile(&file, output.as_deref()),
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

fn cmd_compile(path: &PathBuf, output: Option<&std::path::Path>) -> Result<()> {
    let src = read_file(path)?;
    let module = nova_codegen::parser::parse(&src).map_err(|d| {
        anyhow!("{}", d.render(&src, &path.to_string_lossy()))
    })?;
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let messages: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        anyhow!("{}", messages.join("\n"))
    })?;

    let emitter = nova_codegen::codegen::CEmitter::new();
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
