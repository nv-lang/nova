// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.H.3 — Cross-platform CPU instructions count via Valgrind
//! Callgrind subprocess.
//!
//! Where it helps:
//!   - `perf_event_open` Linux-only (Plan 57.B.4).
//!   - iai-callgrind requires Rust crate dep, доступен только в Rust.
//!   - Valgrind ships on Linux + macOS (через Homebrew); not Windows.
//!
//! Determinism guarantee: callgrind counts retired instructions deterministically
//! при идентичном binary + input → same Ir count run-to-run. Идеально для
//! regression detection без noise floor concerns.
//!
//! Cost: valgrind ~50x slower than native, не подходит для long benches —
//! используем для single-shot deterministic comparison.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Result};

/// Check `valgrind --version` succeeds.
pub fn available() -> bool {
    Command::new("valgrind")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `valgrind --version` output для diagnostic.
pub fn version_string() -> Option<String> {
    let out = Command::new("valgrind").arg("--version").output().ok()?;
    if !out.status.success() { return None; }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[derive(Debug, Clone)]
pub struct CallgrindResult {
    /// Ir — retired instructions count (определяющая метрика).
    pub instructions: u64,
    /// Optional cache stats если `--cache-sim=yes` был указан.
    pub i1_misses: Option<u64>,
    pub d1_misses: Option<u64>,
    pub ll_misses: Option<u64>,
    /// Raw callgrind.out.<pid> path (kept for inspection или kcachegrind).
    pub raw_output_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CallgrindOpts<'a> {
    pub binary: &'a Path,
    pub args: &'a [String],
    /// Workdir для command (default: inherited).
    pub workdir: Option<&'a Path>,
    /// Output file для callgrind. Default: TMP/callgrind.out.nova-<pid>.
    pub out_file: Option<&'a Path>,
    /// Cache simulation (collect I1/D1/LL miss counts). Slow.
    pub cache_sim: bool,
}

/// Run binary under valgrind --tool=callgrind, parse retired instructions.
pub fn measure(opts: CallgrindOpts) -> Result<CallgrindResult> {
    if !available() {
        bail!("valgrind not found в PATH. Install:\n  \
               Linux:  sudo apt-get install valgrind  / dnf install valgrind\n  \
               macOS:  brew install --HEAD valgrind\n  \
               Windows: not supported (use perf_event_open Linux).");
    }
    let default_path = std::env::temp_dir()
        .join(format!("callgrind.out.nova-{}", std::process::id()));
    let out_file: PathBuf = opts.out_file
        .map(|p| p.to_path_buf())
        .unwrap_or(default_path);

    // Remove existing file для clean parse.
    let _ = std::fs::remove_file(&out_file);

    let mut cmd = Command::new("valgrind");
    cmd.arg("--tool=callgrind")
       .arg(format!("--callgrind-out-file={}", out_file.display()))
       .arg("--quiet");  // suppress valgrind banner
    if opts.cache_sim {
        cmd.arg("--cache-sim=yes");
    }
    cmd.arg(opts.binary);
    cmd.args(opts.args);
    if let Some(d) = opts.workdir { cmd.current_dir(d); }

    let status = cmd.status()
        .map_err(|e| anyhow!("spawn valgrind: {}", e))?;
    if !status.success() {
        bail!("valgrind exited non-zero: {:?}", status.code());
    }
    parse_output_file(&out_file).map(|mut r| {
        r.raw_output_path = out_file;
        r
    })
}

/// Parse callgrind output file. Format:
///   ...
///   summary: <Ir> [<I1mr> <D1mr> <ILmr>]
///   totals:  <Ir> [<I1mr> <D1mr> <ILmr>]
///   events: Ir [I1mr Dr Dw D1mr DLmr DLmw]
///   ...
pub fn parse_output_file(path: &Path) -> Result<CallgrindResult> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("read {}: {}", path.display(), e))?;
    parse_output(&text).ok_or_else(||
        anyhow!("could not parse callgrind output {}", path.display()))
}

/// Parse callgrind text content. Public для testing.
pub fn parse_output(text: &str) -> Option<CallgrindResult> {
    // Strategy: find "events:" line declaring column order, then find
    // "summary:" line (или "totals:" fallback) с numeric tokens.
    let mut event_columns: Vec<String> = Vec::new();
    let mut summary_values: Vec<u64> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("events:") {
            event_columns = rest.split_whitespace()
                .map(|s| s.to_string())
                .collect();
        } else if let Some(rest) = line.strip_prefix("summary:")
            .or_else(|| line.strip_prefix("totals:"))
        {
            summary_values = rest.split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            // Prefer "summary:" but accept "totals:" — both contain Ir
            // в same column order. Continue scanning — могут быть оба;
            // последний overrides (typically `summary:` comes последним
            // в callgrind.out).
        }
    }
    if event_columns.is_empty() || summary_values.is_empty() {
        return None;
    }
    let find_col = |name: &str| -> Option<u64> {
        event_columns.iter().position(|c| c == name)
            .and_then(|i| summary_values.get(i).copied())
    };
    let instructions = find_col("Ir")?;
    Some(CallgrindResult {
        instructions,
        i1_misses: find_col("I1mr"),
        d1_misses: find_col("D1mr"),
        ll_misses: find_col("ILmr").or_else(|| find_col("DLmr")),
        raw_output_path: PathBuf::new(),  // overridden by measure()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_output_ir_only() {
        let text = "\
desc: I1 cache: ...
desc: D1 cache: ...
cmd: ./prog
events: Ir
summary: 12345678
totals:  12345678
";
        let r = parse_output(text).unwrap();
        assert_eq!(r.instructions, 12_345_678);
        assert!(r.i1_misses.is_none());
        assert!(r.d1_misses.is_none());
    }

    #[test]
    fn parse_output_with_cache_sim() {
        let text = "\
events: Ir I1mr ILmr Dr Dw D1mr DLmr D1mw DLmw
summary: 1000000 50 5 200000 100000 1000 100 500 50
totals:  1000000 50 5 200000 100000 1000 100 500 50
";
        let r = parse_output(text).unwrap();
        assert_eq!(r.instructions, 1_000_000);
        assert_eq!(r.i1_misses, Some(50));
        assert_eq!(r.d1_misses, Some(1000));
        assert_eq!(r.ll_misses, Some(5));  // ILmr first match
    }

    #[test]
    fn parse_output_no_events_line_returns_none() {
        let text = "summary: 123\n";  // no `events:` declaration
        assert!(parse_output(text).is_none());
    }

    #[test]
    fn parse_output_no_summary_line_returns_none() {
        let text = "events: Ir\n";  // no summary/totals
        assert!(parse_output(text).is_none());
    }

    #[test]
    fn parse_output_ir_not_first_column() {
        // Verifies column-index lookup, не assume Ir == column 0.
        let text = "\
events: Bc Ir Bcm
summary: 100 999 50
";
        let r = parse_output(text).unwrap();
        assert_eq!(r.instructions, 999);
    }

    #[test]
    fn parse_output_uses_totals_if_no_summary() {
        let text = "\
events: Ir
totals:  500
";
        let r = parse_output(text).unwrap();
        assert_eq!(r.instructions, 500);
    }
}
