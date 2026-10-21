use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

mod carina_build;

#[derive(Parser)]
#[command(name = "nova", version, about = "Nova language CLI")]
#[allow(dead_code)]
struct Cli {
    #[arg(long, global = true, default_value = "auto", value_parser = ["auto", "always", "never"])]
    color: String,
    #[arg(long = "strict-effects", global = true)]
    strict_effects: bool,
    #[arg(long = "verify", global = true)]
    verify: bool,
    #[arg(long = "no-field-cache", global = true)]
    no_field_cache: bool,
    #[arg(long = "no-field-cache-licm", global = true)]
    no_field_cache_licm: bool,
    #[arg(long = "no-field-cache-pure", global = true)]
    no_field_cache_pure: bool,
    #[arg(long = "no-field-cache-chain", global = true)]
    no_field_cache_chain: bool,
    #[arg(long = "no-field-cache-ipa", global = true)]
    no_field_cache_ipa: bool,
    #[arg(long = "field-cache-threshold", global = true, value_name = "N")]
    field_cache_threshold: Option<usize>,
    #[arg(long = "field-cache-licm-threshold", global = true, value_name = "N")]
    field_cache_licm_threshold: Option<usize>,
    #[arg(long = "field-cache-pure-threshold", global = true, value_name = "N")]
    field_cache_pure_threshold: Option<usize>,
    #[arg(long = "field-cache-chain-threshold", global = true, value_name = "N")]
    field_cache_chain_threshold: Option<usize>,
    #[arg(long = "field-cache-max", global = true, value_name = "N")]
    field_cache_max: Option<usize>,
    #[arg(long = "field-cache-licm-max", global = true, value_name = "N")]
    field_cache_licm_max: Option<usize>,
    #[arg(long = "field-cache-chain-depth", global = true, value_name = "N")]
    field_cache_chain_depth: Option<usize>,
    #[arg(long = "field-cache-ipa-iter", global = true, value_name = "N")]
    field_cache_ipa_iter: Option<usize>,
    #[command(subcommand)]
    command: Option<CommandLine>,
}

#[derive(Subcommand)]
enum CommandLine {
    Build {
        file: PathBuf,
        #[arg(short = 'o')]
        output: Option<PathBuf>,
        #[arg(long, default_value = "dev", value_parser = ["dev", "release"])]
        mode: String,
        #[arg(long, default_value = "auto", value_parser = ["auto", "clang", "msvc", "gcc"])]
        toolchain: String,
        #[arg(long)]
        vcvars: Option<PathBuf>,
        #[arg(long)]
        clang: Option<PathBuf>,
        #[arg(long, default_value_t = 120)]
        timeout: u64,
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        #[arg(long = "mono-depth")]
        mono_depth: Option<usize>,
        #[arg(long, value_parser = ["checked", "optimized"])]
        contracts: Option<String>,
    },
}

fn main() {
    match dispatch() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("nova: {error:#}");
            std::process::exit(1);
        }
    }
}

fn dispatch() -> Result<i32> {
    let raw: Vec<_> = std::env::args_os().skip(1).collect();
    let command_index = selected_command(&raw);
    if command_index.is_none() || !cfg!(target_os = "windows") {
        return delegate(&raw);
    }
    if raw[command_index.unwrap()..]
        .iter()
        .any(|arg| arg == "--help" || arg == "-h")
    {
        return delegate(&raw);
    }
    let parsed = Cli::try_parse_from(
        std::iter::once("nova".into()).chain(raw[command_index.unwrap()..].iter().cloned()),
    )?;
    match parsed.command {
        Some(CommandLine::Build {
            file,
            output,
            mode,
            toolchain,
            vcvars,
            clang,
            timeout,
            keep_artifacts,
            mono_depth,
            contracts,
        }) => {
            carina_build::build(
                &file,
                output.as_deref(),
                &mode,
                &toolchain,
                vcvars.as_deref(),
                clang.as_deref(),
                timeout,
                keep_artifacts,
                mono_depth,
                contracts.as_deref().unwrap_or("checked"),
            )?;
            Ok(0)
        }
        None => delegate(&raw),
    }
}

fn selected_command(args: &[std::ffi::OsString]) -> Option<usize> {
    const VALUE_OPTIONS: &[&str] = &[
        "--color",
        "--field-cache-threshold",
        "--field-cache-licm-threshold",
        "--field-cache-pure-threshold",
        "--field-cache-chain-threshold",
        "--field-cache-max",
        "--field-cache-licm-max",
        "--field-cache-chain-depth",
        "--field-cache-ipa-iter",
    ];
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let value = arg.to_string_lossy();
        if value == "--" {
            return None;
        }
        if value.starts_with('-') {
            if VALUE_OPTIONS.iter().any(|option| value == *option) {
                index += 2; // its following value is not the subcommand
                continue;
            }
            index += 1;
            continue;
        }
        return (value == "build").then_some(index);
    }
    None
}

fn delegate(args: &[std::ffi::OsString]) -> Result<i32> {
    let executable = std::env::var_os("NOVA_COMPAT_PATH")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_exe()?.with_file_name(if cfg!(windows) {
            "nova-compat.exe"
        } else {
            "nova-compat"
        }));
    if !executable.is_file() {
        return Err(anyhow!(
            "compatibility CLI is missing: {}",
            executable.display()
        ));
    }
    let current = std::env::current_exe()
        .context("could not identify the nova driver executable")?
        .canonicalize()?;
    let target = executable.canonicalize()?;
    if current == target {
        return Err(anyhow!(
            "compatibility CLI resolves to the nova driver itself: {}",
            target.display()
        ));
    }
    let status = Command::new(&executable)
        .args(args)
        .status()
        .with_context(|| format!("could not start compatibility CLI {}", executable.display()))?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::selected_command;
    use std::ffi::OsString;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn selects_build_after_global_options_and_skips_option_values() {
        assert_eq!(
            selected_command(&args(&[
                "--strict-effects",
                "--color",
                "always",
                "build",
                "main.nv"
            ])),
            Some(3)
        );
        assert_eq!(
            selected_command(&args(&["--field-cache-threshold", "8", "build", "main.nv"])),
            Some(2)
        );
    }

    #[test]
    fn positional_build_word_does_not_capture_other_commands() {
        assert_eq!(
            selected_command(&args(&["test", "--filter", "build"])),
            None
        );
        assert_eq!(selected_command(&args(&["--version"])), None);
    }
}
