// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.H.2 — `nova bench hyperfine <binary>...` cross-binary timing.
//!
//! Measures wall-clock time of arbitrary external commands (compiler
//! self-host comparison, two `nova` versions, etc.). Output совместим
//! с `nova bench diff` JSON schema v1 — каждый binary становится
//! отдельным `AnalyzedBench` entry, можно diff'ить как обычно.
//!
//! Inspired by `hyperfine` (Rust crate). Не reimplements all features
//! (нет shell args templating, prepare/cleanup phases) — focuses на
//! core use case: time N commands, output schema-compatible JSON.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{anyhow, bail, Result};

use super::schema::{RawBenchResult, AnalyzedBench, run_result_to_json};
use super::repro::{self, SamplingMeta};

#[derive(Debug, Clone)]
pub struct HyperfineSpec {
    /// Display name (e.g. "nova-old", "nova-new"). Default: argv[0]
    /// stripped к basename.
    pub name: Option<String>,
    /// Argv[0] — executable path.
    pub binary: PathBuf,
    /// Additional command-line args.
    pub args: Vec<String>,
}

impl HyperfineSpec {
    /// Parse "name=path arg1 arg2" или "path arg1 arg2".
    pub fn parse(s: &str) -> Result<Self> {
        // Format: optional "name=" prefix, then space-separated tokens.
        let (name, rest) = match s.find('=') {
            Some(eq) if !s[..eq].contains([' ', '/', '\\']) => {
                (Some(s[..eq].to_string()), &s[eq + 1..])
            }
            _ => (None, s),
        };
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        if tokens.is_empty() {
            bail!("hyperfine spec пустой: `{}`", s);
        }
        let binary = PathBuf::from(tokens[0]);
        let args: Vec<String> = tokens[1..].iter().map(|s| s.to_string()).collect();
        Ok(Self { name, binary, args })
    }

    pub fn display_name(&self) -> String {
        if let Some(n) = &self.name { return n.clone(); }
        self.binary.file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| self.binary.to_string_lossy().to_string())
    }
}

#[derive(Debug, Clone)]
pub struct HyperfineOpts {
    pub specs: Vec<HyperfineSpec>,
    pub warmup_runs: u32,
    pub samples: u32,
    pub timeout_secs: u64,
    pub workdir: Option<PathBuf>,
}

/// Время одного command run в ns.
fn time_one(spec: &HyperfineSpec, timeout: std::time::Duration,
            workdir: Option<&Path>) -> Result<u64> {
    let mut cmd = Command::new(&spec.binary);
    cmd.args(&spec.args);
    if let Some(d) = workdir { cmd.current_dir(d); }
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.stdin(Stdio::null());
    let start = Instant::now();
    let mut child = cmd.spawn()
        .map_err(|e| anyhow!("spawn {}: {}", spec.binary.display(), e))?;
    // Poll for completion с timeout.
    let deadline = Instant::now() + timeout;
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    bail!("{} timeout ({}s)", spec.display_name(), timeout.as_secs());
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => bail!("wait {}: {}", spec.display_name(), e),
        }
    };
    let elapsed_ns = start.elapsed().as_nanos() as u64;
    if !exit_status.success() {
        bail!("{} exited non-zero: {:?}",
            spec.display_name(), exit_status.code());
    }
    Ok(elapsed_ns)
}

/// Execute hyperfine measurements: для каждого spec — warmup + samples
/// runs, returns Vec<AnalyzedBench> for normal pipeline (JSON + diff).
pub fn run(opts: HyperfineOpts) -> Result<Vec<AnalyzedBench>> {
    if opts.specs.is_empty() {
        bail!("hyperfine: no commands specified");
    }
    let timeout = std::time::Duration::from_secs(opts.timeout_secs);
    let mut out = Vec::with_capacity(opts.specs.len());
    for spec in &opts.specs {
        eprintln!("hyperfine: {} (warmup {}, samples {})",
            spec.display_name(), opts.warmup_runs, opts.samples);
        // Warmup runs — discard timings.
        for _ in 0..opts.warmup_runs {
            let _ = time_one(spec, timeout, opts.workdir.as_deref());
        }
        let mut raw_ns: Vec<u64> = Vec::with_capacity(opts.samples as usize);
        for i in 0..opts.samples {
            let t = time_one(spec, timeout, opts.workdir.as_deref())
                .map_err(|e| anyhow!("sample {}/{}: {}", i + 1, opts.samples, e))?;
            raw_ns.push(t);
        }
        let raw = RawBenchResult {
            name: spec.display_name(),
            iters_per_sample: 1,
            samples_count: raw_ns.len() as u64,
            raw_ns,
            throughput_bytes: None,
            throughput_elements: None,
            allocs_per_iter: None,
            allocs_total: None,
            cpu_instructions: Vec::new(),
            custom_metrics: Vec::new(),
        };
        match AnalyzedBench::from_raw(raw) {
            Some(a) => out.push(a),
            None => bail!("hyperfine: {} no successful samples", spec.display_name()),
        }
    }
    Ok(out)
}

/// Compose JSON v1 output (same schema as `nova bench run`).
pub fn write_json(benches: &[AnalyzedBench], out_path: &Path) -> Result<()> {
    let sampling = SamplingMeta {
        warmup_ns: 0,
        target_ns: 0,
        samples: benches.first().map(|b| b.raw.samples_count).unwrap_or(0),
        time_budget_ns: 0,
    };
    let meta = repro::collect("hyperfine", sampling);
    let json = run_result_to_json(&meta, benches);
    std::fs::write(out_path, serde_json::to_string_pretty(&json)?)
        .map_err(|e| anyhow!("write JSON {}: {}", out_path.display(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_path_no_name() {
        let s = HyperfineSpec::parse("/usr/bin/echo hello").unwrap();
        assert_eq!(s.name, None);
        assert_eq!(s.binary, PathBuf::from("/usr/bin/echo"));
        assert_eq!(s.args, vec!["hello".to_string()]);
        assert_eq!(s.display_name(), "echo");
    }

    #[test]
    fn parse_name_equals_path() {
        let s = HyperfineSpec::parse("nova-old=/tmp/nova-old run.nv --gc malloc").unwrap();
        assert_eq!(s.name, Some("nova-old".to_string()));
        assert_eq!(s.binary, PathBuf::from("/tmp/nova-old"));
        assert_eq!(s.args, vec!["run.nv", "--gc", "malloc"]);
        assert_eq!(s.display_name(), "nova-old");
    }

    #[test]
    fn parse_path_with_equals_in_args_does_not_treat_as_name() {
        // First token не должен иметь `=` чтобы считаться name. Если в
        // path есть `=`, parser treats как path.
        let s = HyperfineSpec::parse("/usr/bin/env VAR=1 echo x").unwrap();
        // Heuristic: first eq found, but `s[..eq]` = "/usr/bin/env VAR" which
        // contains '/' — treated as path-form (no name).
        assert_eq!(s.name, None);
        assert_eq!(s.binary, PathBuf::from("/usr/bin/env"));
        assert_eq!(s.args, vec!["VAR=1", "echo", "x"]);
    }

    #[test]
    fn parse_just_binary() {
        let s = HyperfineSpec::parse("date").unwrap();
        assert_eq!(s.binary, PathBuf::from("date"));
        assert!(s.args.is_empty());
        assert_eq!(s.display_name(), "date");
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(HyperfineSpec::parse("").is_err());
        assert!(HyperfineSpec::parse("   ").is_err());
    }

    #[test]
    fn parse_named_with_just_binary() {
        let s = HyperfineSpec::parse("nameonly=binary").unwrap();
        assert_eq!(s.name, Some("nameonly".to_string()));
        assert_eq!(s.binary, PathBuf::from("binary"));
        assert!(s.args.is_empty());
    }
}
