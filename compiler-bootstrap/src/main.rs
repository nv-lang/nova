//! Nova bootstrap CLI.

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "nova", version, about = "Nova bootstrap compiler")]
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Check { file } => cmd_check(&file),
        Cmd::Run { file } => cmd_run(&file),
        Cmd::Test { file } => cmd_test(&file),
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
    let module = nova::parser::parse(&src).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    nova::types::check_module(&module).map_err(|errs| {
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
    let module = nova::parser::parse(&src).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    nova::types::check_module(&module).map_err(|errs| {
        let messages: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        anyhow!("{}", messages.join("\n"))
    })?;
    let mut interp = nova::interp::Interpreter::new();
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

fn cmd_test(path: &PathBuf) -> Result<()> {
    let src = read_file(path)?;
    let module = nova::parser::parse(&src).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    let mut interp = nova::interp::Interpreter::new();
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
