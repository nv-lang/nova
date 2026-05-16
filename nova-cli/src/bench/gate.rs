// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57 L6 — `nova bench gate` CI gate.
//!
//! Применяет thresholds из bench.toml к diff между baseline и new.
//! Exit 0 → pass, 1 → regression(s) detected, 2 → usage error.

use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::config::BenchToml;
use super::diff::compute_diff;
use super::schema::RunResultParsed;

#[derive(Debug, Clone)]
pub struct GateVerdict {
    pub bench_name: String,
    pub delta_pct: f64,
    pub p_value: f64,
    pub threshold_pct: f64,
    pub threshold_p: f64,
    pub exempt: bool,
    pub regressed: bool,
}

pub fn run(baseline_path: &Path, new_path: &Path, config_path: Option<&Path>) -> Result<i32> {
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

    let cfg = match config_path {
        Some(p) => BenchToml::load_or_default(p),
        None => {
            // Try default location ./bench.toml in current dir.
            let default = std::env::current_dir().ok()
                .map(|d| d.join("bench.toml"));
            match default {
                Some(p) if p.exists() => BenchToml::load_or_default(&p),
                _ => BenchToml::default(),
            }
        }
    };
    for err in &cfg.parse_errors {
        eprintln!("bench.toml: {}", err);
    }

    let rows = compute_diff(&baseline.benches, &new.benches);

    let mut verdicts = Vec::new();
    for r in &rows {
        let (delta, p) = match (r.delta_pct, r.p_value) {
            (Some(d), Some(p)) => (d, p),
            _ => continue,
        };
        let gate = cfg.gate_for(&r.name);
        let exempt = cfg.is_exempt(&r.name);
        let regressed = !exempt
            && delta > gate.wall_clock_delta_pct
            && p < gate.significance_p;
        verdicts.push(GateVerdict {
            bench_name: r.name.clone(),
            delta_pct: delta,
            p_value: p,
            threshold_pct: gate.wall_clock_delta_pct,
            threshold_p: gate.significance_p,
            exempt,
            regressed,
        });
    }

    let regressed_count = verdicts.iter().filter(|v| v.regressed).count();

    println!("Gate verdict ({} bench{}):", verdicts.len(),
        if verdicts.len() == 1 { "" } else { "es" });
    for v in &verdicts {
        let tag = if v.exempt {
            "  [exempt]   "
        } else if v.regressed {
            "  REGRESSION"
        } else {
            "  ok        "
        };
        println!("{} {:<40} delta={:+.1}% (threshold {:.1}%, p={:.3})",
            tag, v.bench_name, v.delta_pct, v.threshold_pct, v.p_value);
    }
    println!("");
    if regressed_count == 0 {
        println!("Gate: PASS (no regressions detected)");
        Ok(0)
    } else {
        println!("Gate: FAIL ({} regression{} detected)",
            regressed_count, if regressed_count == 1 { "" } else { "s" });
        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::config::{BenchToml, GateConfig};
    use crate::bench::schema::{RawBenchResult, AnalyzedBench};
    use crate::bench::diff::compute_diff;

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
    fn gate_pass_within_threshold() {
        let b = vec![mk("foo", vec![100; 20])];
        let n = vec![mk("foo", vec![102; 20])];  // +2% — below 5% threshold
        let rows = compute_diff(&b, &n);
        let cfg = BenchToml::default();
        let mut regressed = 0;
        for r in &rows {
            if let (Some(d), Some(p)) = (r.delta_pct, r.p_value) {
                let gate = cfg.gate_for(&r.name);
                if d > gate.wall_clock_delta_pct && p < gate.significance_p {
                    regressed += 1;
                }
            }
        }
        assert_eq!(regressed, 0);
    }

    #[test]
    fn gate_fail_big_regression() {
        // Включаем хоть небольшую variance — иначе stddev=0 и Welch p=1.
        let b = vec![mk("foo", vec![100, 101, 99, 100, 102, 98, 100, 101, 99, 100,
                                    100, 101, 99, 100, 102, 98, 100, 101, 99, 100])];
        let n = vec![mk("foo", vec![150, 151, 149, 150, 152, 148, 150, 151, 149, 150,
                                    150, 151, 149, 150, 152, 148, 150, 151, 149, 150])];  // +50%
        let rows = compute_diff(&b, &n);
        let cfg = BenchToml::default();
        let mut regressed = 0;
        for r in &rows {
            if let (Some(d), Some(p)) = (r.delta_pct, r.p_value) {
                let gate = cfg.gate_for(&r.name);
                if d > gate.wall_clock_delta_pct && p < gate.significance_p {
                    regressed += 1;
                }
            }
        }
        assert_eq!(regressed, 1);
    }

    #[test]
    fn gate_exempt_skips() {
        let b = vec![mk("sleep_test", vec![100; 20])];
        let n = vec![mk("sleep_test", vec![500; 20])];
        let rows = compute_diff(&b, &n);
        let mut cfg = BenchToml::default();
        cfg.exempt_globs.push("sleep_*".to_string());
        let mut regressed = 0;
        for r in &rows {
            if cfg.is_exempt(&r.name) { continue; }
            if let (Some(d), Some(p)) = (r.delta_pct, r.p_value) {
                let gate = cfg.gate_for(&r.name);
                if d > gate.wall_clock_delta_pct && p < gate.significance_p {
                    regressed += 1;
                }
            }
        }
        assert_eq!(regressed, 0);
    }
}
