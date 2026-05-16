// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57 L5 — `nova bench diff`. Welch's t-test pairwise compare с
//! geomean aggregate + reproducibility check.

use std::fmt::Write as FmtWrite;
use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::schema::{AnalyzedBench, RunResultParsed};
use super::stats::{welch_t_test, geomean};
use super::report::fmt_duration;

#[derive(Debug, Clone, Copy)]
pub enum DiffFormat {
    Terminal,
    Markdown,
    Json,
}

impl DiffFormat {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "terminal" | "text" => Ok(DiffFormat::Terminal),
            "md" | "markdown"   => Ok(DiffFormat::Markdown),
            "json"              => Ok(DiffFormat::Json),
            _ => Err(anyhow!("unknown diff format: {} (expected: terminal|markdown|json)", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiffRow {
    pub name: String,
    pub baseline_median_ns: Option<f64>,
    pub new_median_ns: Option<f64>,
    pub delta_pct: Option<f64>,    // (new - base) / base * 100
    pub p_value: Option<f64>,      // Welch's t-test p
    pub n_baseline: usize,
    pub n_new: usize,
}

pub fn compare(baseline_path: &Path, new_path: &Path, format: DiffFormat) -> Result<i32> {
    let base_text = std::fs::read_to_string(baseline_path)
        .map_err(|e| anyhow!("read baseline {}: {}", baseline_path.display(), e))?;
    let new_text = std::fs::read_to_string(new_path)
        .map_err(|e| anyhow!("read new {}: {}", new_path.display(), e))?;
    let base_v: Value = serde_json::from_str(&base_text)
        .map_err(|e| anyhow!("parse baseline JSON: {}", e))?;
    let new_v: Value = serde_json::from_str(&new_text)
        .map_err(|e| anyhow!("parse new JSON: {}", e))?;
    let baseline = RunResultParsed::from_json(&base_v)
        .map_err(|e| anyhow!("baseline schema: {}", e))?;
    let new = RunResultParsed::from_json(&new_v)
        .map_err(|e| anyhow!("new schema: {}", e))?;

    let compat_warnings = baseline.metadata.compare_compatibility(&new.metadata);
    let rows = compute_diff(&baseline.benches, &new.benches);

    let (output, exit) = match format {
        DiffFormat::Terminal => (terminal_format(&rows, &compat_warnings), 0),
        DiffFormat::Markdown => (markdown_format(&rows, &compat_warnings), 0),
        DiffFormat::Json => (json_format(&rows, &compat_warnings)?, 0),
    };
    print!("{}", output);
    Ok(exit)
}

pub fn compute_diff(baseline: &[AnalyzedBench], new: &[AnalyzedBench]) -> Vec<DiffRow> {
    use std::collections::HashMap;
    let base_map: HashMap<&str, &AnalyzedBench> = baseline.iter()
        .map(|b| (b.raw.name.as_str(), b))
        .collect();
    let new_map: HashMap<&str, &AnalyzedBench> = new.iter()
        .map(|b| (b.raw.name.as_str(), b))
        .collect();
    let mut names: Vec<&str> = base_map.keys().chain(new_map.keys()).cloned().collect();
    names.sort();
    names.dedup();

    let mut rows = Vec::with_capacity(names.len());
    for n in names {
        let b = base_map.get(n).copied();
        let nw = new_map.get(n).copied();
        let baseline_median_ns = b.map(|x| x.stats_ns.median);
        let new_median_ns = nw.map(|x| x.stats_ns.median);
        let delta_pct = match (baseline_median_ns, new_median_ns) {
            (Some(bm), Some(nm)) if bm > 0.0 => Some((nm - bm) / bm * 100.0),
            _ => None,
        };
        let p_value = match (b, nw) {
            (Some(b), Some(nw)) if b.raw.raw_ns.len() >= 2 && nw.raw.raw_ns.len() >= 2 => {
                let ba: Vec<f64> = b.raw.raw_ns.iter().map(|x| *x as f64).collect();
                let na: Vec<f64> = nw.raw.raw_ns.iter().map(|x| *x as f64).collect();
                let (_t, p, _df) = welch_t_test(&ba, &na);
                Some(p)
            }
            _ => None,
        };
        rows.push(DiffRow {
            name: n.to_string(),
            baseline_median_ns,
            new_median_ns,
            delta_pct, p_value,
            n_baseline: b.map(|x| x.stats_ns.n).unwrap_or(0),
            n_new: nw.map(|x| x.stats_ns.n).unwrap_or(0),
        });
    }
    rows
}

pub fn terminal_format(rows: &[DiffRow], compat: &[String]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "name                       baseline        new             delta       p-value");
    let _ = writeln!(out, "─────────────────────────────────────────────────────────────────────────────");
    let mut ratios = Vec::new();
    for r in rows {
        let bn = r.baseline_median_ns.map(fmt_duration).unwrap_or_else(|| "—".to_string());
        let nn = r.new_median_ns.map(fmt_duration).unwrap_or_else(|| "—".to_string());
        let dp = r.delta_pct.map(|d| {
            let sign = if d >= 0.0 { "+" } else { "" };
            let stars = if let Some(p) = r.p_value {
                if p < 0.001 { " ***" } else if p < 0.01 { " **" } else if p < 0.05 { " *" } else { "" }
            } else { "" };
            format!("{}{:.1}%{}", sign, d, stars)
        }).unwrap_or_else(|| "—".to_string());
        let pv = r.p_value.map(|p| {
            if p < 0.001 { "<0.001".to_string() } else { format!("{:.3}", p) }
        }).unwrap_or_else(|| "—".to_string());
        let _ = writeln!(out, "{:<26} {:<15} {:<15} {:<12} {}",
            truncate(&r.name, 26), bn, nn, dp, pv);
        if let Some(d) = r.delta_pct {
            ratios.push(1.0 + d / 100.0);
        }
    }
    let _ = writeln!(out, "─────────────────────────────────────────────────────────────────────────────");
    if !ratios.is_empty() {
        let g = geomean(&ratios);
        let pct = (g - 1.0) * 100.0;
        let _ = writeln!(out, "                                          geomean delta: {}{:.1}%",
            if pct >= 0.0 { "+" } else { "" }, pct);
    }
    let _ = writeln!(out, "");
    let _ = writeln!(out, "Legend: *** p<0.001  ** p<0.01  * p<0.05");
    let _ = writeln!(out, "Tests with p>0.05 are within noise floor — not statistically significant.");

    if !compat.is_empty() {
        let _ = writeln!(out, "");
        let _ = writeln!(out, "Reproducibility check:");
        for w in compat {
            let _ = writeln!(out, "  ⚠ {}", w);
        }
    } else {
        let _ = writeln!(out, "");
        let _ = writeln!(out, "Reproducibility: ✓ baseline and new collected on compatible environment");
    }
    out
}

pub fn markdown_format(rows: &[DiffRow], compat: &[String]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "## Bench diff\n");
    let _ = writeln!(out, "| Bench | baseline | new | delta | p |");
    let _ = writeln!(out, "|---|---|---|---|---|");
    let mut ratios = Vec::new();
    for r in rows {
        let bn = r.baseline_median_ns.map(fmt_duration).unwrap_or_else(|| "—".to_string());
        let nn = r.new_median_ns.map(fmt_duration).unwrap_or_else(|| "—".to_string());
        let dp = r.delta_pct.map(|d| {
            let sign = if d >= 0.0 { "+" } else { "" };
            let badge = if let Some(p) = r.p_value {
                if p < 0.01 && d.abs() >= 5.0 { " ⚠️" } else { "" }
            } else { "" };
            format!("{}{:.1}%{}", sign, d, badge)
        }).unwrap_or_else(|| "—".to_string());
        let pv = r.p_value.map(|p| {
            if p < 0.001 { "<0.001".to_string() } else { format!("{:.3}", p) }
        }).unwrap_or_else(|| "—".to_string());
        let _ = writeln!(out, "| {} | {} | {} | {} | {} |", r.name, bn, nn, dp, pv);
        if let Some(d) = r.delta_pct {
            ratios.push(1.0 + d / 100.0);
        }
    }
    if !ratios.is_empty() {
        let g = geomean(&ratios);
        let pct = (g - 1.0) * 100.0;
        let _ = writeln!(out, "\n**Geomean delta: {}{:.1}%**",
            if pct >= 0.0 { "+" } else { "" }, pct);
    }
    if !compat.is_empty() {
        let _ = writeln!(out, "\n### ⚠️ Compatibility warnings");
        for w in compat {
            let _ = writeln!(out, "- {}", w);
        }
    }
    out
}

pub fn json_format(rows: &[DiffRow], compat: &[String]) -> Result<String> {
    use serde_json::json;
    let rows_json: Vec<_> = rows.iter().map(|r| json!({
        "name": r.name,
        "baseline_median_ns": r.baseline_median_ns,
        "new_median_ns": r.new_median_ns,
        "delta_pct": r.delta_pct,
        "p_value": r.p_value,
        "n_baseline": r.n_baseline,
        "n_new": r.n_new,
    })).collect();
    let mut ratios = Vec::new();
    for r in rows {
        if let Some(d) = r.delta_pct { ratios.push(1.0 + d / 100.0); }
    }
    let g = if !ratios.is_empty() { geomean(&ratios) } else { 1.0 };
    let out = json!({
        "rows": rows_json,
        "geomean_ratio": g,
        "geomean_delta_pct": (g - 1.0) * 100.0,
        "compatibility_warnings": compat,
    });
    Ok(serde_json::to_string_pretty(&out)? + "\n")
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::schema::RawBenchResult;

    fn mk(name: &str, ns: Vec<u64>) -> AnalyzedBench {
        let raw = RawBenchResult {
            name: name.to_string(),
            iters_per_sample: 1,
            samples_count: ns.len() as u64,
            raw_ns: ns,
            throughput_bytes: None,
            throughput_elements: None,
            allocs_per_iter: None,
            allocs_total: None,
        };
        AnalyzedBench::from_raw(raw).unwrap()
    }

    #[test]
    fn diff_no_change() {
        let b = vec![mk("foo", vec![100; 30])];
        let n = vec![mk("foo", vec![100; 30])];
        let rows = compute_diff(&b, &n);
        assert_eq!(rows.len(), 1);
        // 0% delta.
        assert!(rows[0].delta_pct.unwrap().abs() < 1e-9);
        // p-value high (no difference).
        assert!(rows[0].p_value.unwrap() > 0.9);
    }

    #[test]
    fn diff_regression() {
        let b = vec![mk("foo", vec![100, 102, 98, 101, 99, 100, 103, 97, 100, 100])];
        let n = vec![mk("foo", vec![200, 205, 195, 202, 198, 200, 203, 197, 200, 201])];
        let rows = compute_diff(&b, &n);
        assert_eq!(rows.len(), 1);
        let d = rows[0].delta_pct.unwrap();
        assert!(d > 90.0 && d < 110.0, "expected ~100% regression, got {}", d);
        // Very significant.
        assert!(rows[0].p_value.unwrap() < 0.001);
    }

    #[test]
    fn diff_missing_in_new() {
        let b = vec![mk("foo", vec![100; 5]), mk("bar", vec![200; 5])];
        let n = vec![mk("foo", vec![100; 5])];
        let rows = compute_diff(&b, &n);
        // bar is missing in new — new_median_ns = None.
        let bar = rows.iter().find(|r| r.name == "bar").unwrap();
        assert!(bar.new_median_ns.is_none());
        assert!(bar.delta_pct.is_none());
    }
}
