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

use anyhow::{anyhow, Result};
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

/// Plan 57.D.5: Render HTML compiler-perf dashboard via echarts.
/// Stacked bar chart per-file showing each pass duration; clickable
/// to per-file detail (per-pass percentages).
pub fn render_html(entries: &[CorpusEntry], echarts_url: &str) -> String {
    let html_escape = |s: &str| -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
         .replace('"', "&quot;").replace('\'', "&#39;")
    };
    // Collect all unique pass names across entries (preserve first-seen order).
    let mut all_passes: Vec<String> = Vec::new();
    for e in entries {
        for (p, _) in &e.passes {
            if !all_passes.contains(p) { all_passes.push(p.clone()); }
        }
    }
    // For each pass, build [ns per file] array (0 if missing).
    let mut series_data: Vec<(String, Vec<u64>)> = Vec::new();
    for pass in &all_passes {
        let row: Vec<u64> = entries.iter().map(|e| {
            e.passes.iter().find(|(p, _)| p == pass).map(|(_, n)| *n).unwrap_or(0)
        }).collect();
        series_data.push((pass.clone(), row));
    }
    let file_labels: Vec<String> = entries.iter()
        .map(|e| e.file.file_name().and_then(|s| s.to_str())
            .unwrap_or("?").to_string())
        .collect();
    // Build echarts series JSON.
    let series_json: String = serde_json::to_string(
        &series_data.iter().map(|(name, data)| {
            // Convert ns → ms для readability.
            let data_ms: Vec<f64> = data.iter().map(|n| *n as f64 / 1e6).collect();
            serde_json::json!({
                "name": name,
                "type": "bar",
                "stack": "passes",
                "emphasis": {"focus": "series"},
                "data": data_ms,
            })
        }).collect::<Vec<_>>()
    ).unwrap_or("[]".to_string());
    let labels_json = serde_json::to_string(&file_labels).unwrap_or("[]".to_string());

    // Per-file table (rendered server-side).
    let mut table_html = String::new();
    use std::fmt::Write;
    let _ = writeln!(table_html, "<table><thead><tr><th>file</th><th>status</th>\
        <th>total</th>{}</tr></thead><tbody>",
        all_passes.iter().map(|p|
            format!("<th>{}</th>", html_escape(p))).collect::<String>());
    for e in entries {
        let pass_cells: String = all_passes.iter().map(|pass| {
            let ns = e.passes.iter().find(|(p, _)| p == pass).map(|(_, n)| *n).unwrap_or(0);
            if ns == 0 { "<td>—</td>".to_string() }
            else { format!("<td>{}</td>", fmt_ns(ns as f64)) }
        }).collect();
        let _ = writeln!(table_html,
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td>{}</tr>",
            html_escape(&e.file.display().to_string()),
            html_escape(&e.status),
            fmt_ns(e.total_ns as f64),
            pass_cells);
    }
    let _ = writeln!(table_html, "</tbody></table>");

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Nova compiler perf — corpus dashboard</title>
<script src="{echarts_url}"></script>
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI",
        Roboto, sans-serif; margin: 0; padding: 1em 2em; color: #222; }}
header {{ border-bottom: 1px solid #ccc; padding-bottom: 1em;
         margin-bottom: 1em; }}
h1 {{ margin: 0; font-size: 1.5em; }}
.meta {{ color: #666; font-size: 0.9em; }}
#chart {{ width: 100%; height: 500px; margin-bottom: 2em;
         border: 1px solid #ddd; border-radius: 4px; }}
table {{ width: 100%; border-collapse: collapse; font-size: 0.85em; }}
th, td {{ text-align: left; padding: 0.4em 0.6em;
          border-bottom: 1px solid #eee; }}
th {{ background: #f4f4f4; font-weight: 600; position: sticky; top: 0; }}
code {{ font-family: SFMono-Regular, Consolas, monospace; }}
section {{ margin-bottom: 2em; }}
.note {{ color: #999; font-size: 0.85em; margin-top: 3em; }}
</style>
</head>
<body>
<header>
<h1>Compiler perf — corpus dashboard</h1>
<div class="meta">{n_files} files · {n_passes} passes per file ·
  stacked by pass</div>
</header>

<section>
<h2>Per-file compile time breakdown (stacked, ms)</h2>
<div id="chart"></div>
</section>

<section>
<h2>Detailed timings table</h2>
{table_html}
</section>

<footer class="note">
Generated by <code>nova bench corpus --html</code> · Plan 57.D.5
</footer>

<script>
const files = {labels_json};
const series = {series_json};
const chart = echarts.init(document.getElementById('chart'));
chart.setOption({{
    tooltip: {{
        trigger: 'axis',
        axisPointer: {{ type: 'shadow' }},
        valueFormatter: v => v.toFixed(2) + ' ms'
    }},
    legend: {{ type: 'scroll', orient: 'horizontal', top: 8 }},
    grid: {{ left: '5%', right: '4%', bottom: '15%', top: 60 }},
    xAxis: {{ type: 'category', data: files,
              axisLabel: {{ rotate: -25, fontSize: 11 }} }},
    yAxis: {{ type: 'value', name: 'ms', nameLocation: 'middle', nameGap: 50 }},
    dataZoom: [
        {{ type: 'inside', start: 0, end: 100 }},
        {{ type: 'slider', start: 0, end: 100, bottom: 30 }}
    ],
    series: series
}});
window.addEventListener('resize', () => chart.resize());
</script>
</body>
</html>"#,
        echarts_url = html_escape(echarts_url),
        n_files = entries.len(),
        n_passes = all_passes.len(),
        labels_json = labels_json,
        series_json = series_json,
        table_html = table_html,
    )
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
