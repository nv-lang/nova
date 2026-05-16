// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.C.8 — `nova bench corpus` subcommand.
//!
//! Wraps `nova build` с NOVA_PERF_TIMER=1; parses `__PERF__ <pass> <ns>`
//! markers (Plan 57.C.1); emits per-pass timings table или JSON.
//!
//! Использование:
//!   nova bench corpus bench/corpus/03_generic_heavy.nv
//!   nova bench corpus bench/corpus/ --all  (whole directory)

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Result};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct CorpusEntry {
    pub file: PathBuf,
    pub passes: Vec<(String, u64)>,  // (pass_name, ns)
    pub total_ns: u64,
    pub status: String,  // "ok" / "fail: <err>"
}

/// Build one file via subprocess `nova build`, parse __PERF__ markers from
/// stderr, return per-pass timings.
pub fn measure_file(nova_cli_path: &Path, file: &Path,
                    gc: &str, mode: &str,
                    toolchain: &str) -> Result<CorpusEntry> {
    let mut cmd = Command::new(nova_cli_path);
    cmd.arg("build").arg(file);
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let exe_out = std::env::temp_dir().join(format!("{}_corpus.exe", stem));
    cmd.args(["-o"]).arg(&exe_out);
    // Pass через nova build flags (he supports --mode, --toolchain).
    cmd.args(["--mode", mode]);
    cmd.args(["--toolchain", toolchain]);
    let _ = gc;  // nova build не имеет --gc; use env override.
    cmd.env("NOVA_PERF_TIMER", "1");
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());
    let output = cmd.output()
        .map_err(|e| anyhow!("spawn nova build: {}", e))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut passes: Vec<(String, u64)> = Vec::new();
    for line in stderr.lines() {
        if let Some((p, n)) = nova_codegen::perf_timer::parse_perf_line(line) {
            passes.push((p, n));
        }
    }
    let total_ns = passes.iter().map(|(_, n)| n).sum();
    let status = if output.status.success() {
        "ok".to_string()
    } else {
        format!("fail: exit={:?}", output.status.code())
    };
    // Cleanup exe.
    let _ = std::fs::remove_file(&exe_out);
    Ok(CorpusEntry {
        file: file.to_path_buf(),
        passes,
        total_ns,
        status,
    })
}

/// Render terminal table — per-pass times sorted as observed.
pub fn render_terminal(entries: &[CorpusEntry]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for e in entries {
        let _ = writeln!(out, "{}  ({})", e.file.display(), e.status);
        if e.passes.is_empty() {
            let _ = writeln!(out, "  (no __PERF__ markers — check NOVA_PERF_TIMER=1)");
            continue;
        }
        for (pass, ns) in &e.passes {
            let d = fmt_ns(*ns as f64);
            let pct = if e.total_ns > 0 {
                (*ns as f64 / e.total_ns as f64) * 100.0
            } else { 0.0 };
            let _ = writeln!(out, "  {:<20} {:>10}  ({:>5.1}%)", pass, d, pct);
        }
        let _ = writeln!(out, "  {:<20} {:>10}", "── total ──", fmt_ns(e.total_ns as f64));
        let _ = writeln!(out, "");
    }
    out
}

/// Render as JSON for downstream tooling.
pub fn render_json(entries: &[CorpusEntry]) -> serde_json::Value {
    let items: Vec<serde_json::Value> = entries.iter().map(|e| {
        let passes: serde_json::Value = e.passes.iter().map(|(p, n)| {
            json!({"pass": p, "ns": n})
        }).collect();
        json!({
            "file": e.file.display().to_string(),
            "status": e.status,
            "total_ns": e.total_ns,
            "passes": passes,
        })
    }).collect();
    json!({
        "format_version": "1",
        "kind": "corpus-perf-breakdown",
        "entries": items,
    })
}

fn fmt_ns(ns: f64) -> String {
    if ns < 1e3 { format!("{:.0} ns", ns) }
    else if ns < 1e6 { format!("{:.1} µs", ns / 1e3) }
    else if ns < 1e9 { format!("{:.1} ms", ns / 1e6) }
    else { format!("{:.2} s", ns / 1e9) }
}

/// Walk corpus directory: collect .nv files (skip _module.nv and folders).
pub fn list_corpus_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(d: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(d)
        .map_err(|e| anyhow!("read_dir {}: {}", d.display(), e))?;
    for ent in entries.flatten() {
        let p = ent.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with('.') { continue; }
        if p.is_dir() { walk(&p, out)?; }
        else if name.ends_with(".nv") { out.push(p); }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_ns_scales() {
        assert_eq!(fmt_ns(500.0), "500 ns");
        assert_eq!(fmt_ns(5000.0), "5.0 µs");
        assert_eq!(fmt_ns(5_000_000.0), "5.0 ms");
        assert_eq!(fmt_ns(5_000_000_000.0), "5.00 s");
    }
}
