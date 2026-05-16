// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.A.3 — Auto noise-floor calibration.
//!
//! Идея: запустить тот же bench N раз подряд на **той же** машине, и
//! измерить delta которая возникает чисто из noise (run-to-run variation).
//! Median noise floor (per-bench) — это лимит ниже которого регрессия —
//! шум, а не реальная регрессия.
//!
//! Nova-unique: ни Criterion, ни benchstat не делают этого автоматически —
//! пользователю нужно вручную tune'ить thresholds.
//!
//! Storage: `<repo>/.nova-bench-noise.json` (gitignored by convention).
//!
//! Schema:
//! ```json
//! {
//!   "format_version": "1",
//!   "machine_fingerprint": "AMD Ryzen 9 5950X|linux|x86_64",
//!   "calibrated_at_unix": 1779494400,
//!   "calibration_runs": 5,
//!   "per_bench": {
//!     "hashmap_insert": { "noise_floor_pct": 2.3 },
//!     ...
//!   },
//!   "suite_noise_pct": 3.1
//! }
//! ```

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::schema::{AnalyzedBench, RunResultParsed};
use super::stats::welch_t_test;

pub const NOISE_SCHEMA_VERSION: &str = "1";
pub const DEFAULT_NOISE_FILE: &str = ".nova-bench-noise.json";

#[derive(Debug, Clone)]
pub struct NoiseFloor {
    pub machine_fingerprint: String,
    pub calibrated_at_unix: u64,
    pub calibration_runs: u32,
    pub per_bench: std::collections::HashMap<String, f64>,
    pub suite_noise_pct: f64,
}

impl NoiseFloor {
    /// Save to file как JSON.
    pub fn save(&self, path: &Path) -> Result<()> {
        let map: serde_json::Map<String, Value> = self.per_bench.iter()
            .map(|(k, v)| (k.clone(), serde_json::json!({"noise_floor_pct": v})))
            .collect();
        let j = serde_json::json!({
            "format_version": NOISE_SCHEMA_VERSION,
            "machine_fingerprint": self.machine_fingerprint,
            "calibrated_at_unix": self.calibrated_at_unix,
            "calibration_runs": self.calibration_runs,
            "per_bench": map,
            "suite_noise_pct": self.suite_noise_pct,
        });
        std::fs::write(path, serde_json::to_string_pretty(&j)?)
            .map_err(|e| anyhow!("write noise floor: {}", e))?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("read noise floor: {}", e))?;
        let v: Value = serde_json::from_str(&raw)
            .map_err(|e| anyhow!("parse noise floor JSON: {}", e))?;
        let fv = v.get("format_version").and_then(|x| x.as_str())
            .ok_or_else(|| anyhow!("noise floor missing format_version"))?;
        if fv != NOISE_SCHEMA_VERSION {
            return Err(anyhow!("noise floor schema version {} != {}",
                fv, NOISE_SCHEMA_VERSION));
        }
        let mut per_bench = std::collections::HashMap::new();
        if let Some(pb) = v.get("per_bench").and_then(|x| x.as_object()) {
            for (k, val) in pb {
                if let Some(noise) = val.get("noise_floor_pct").and_then(|x| x.as_f64()) {
                    per_bench.insert(k.clone(), noise);
                }
            }
        }
        Ok(Self {
            machine_fingerprint: v.get("machine_fingerprint")
                .and_then(|x| x.as_str()).unwrap_or("unknown").to_string(),
            calibrated_at_unix: v.get("calibrated_at_unix")
                .and_then(|x| x.as_u64()).unwrap_or(0),
            calibration_runs: v.get("calibration_runs")
                .and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            per_bench,
            suite_noise_pct: v.get("suite_noise_pct")
                .and_then(|x| x.as_f64()).unwrap_or(0.0),
        })
    }

    /// Returns noise floor % for a bench name; fallback к suite-wide.
    pub fn get(&self, bench_name: &str) -> f64 {
        self.per_bench.get(bench_name).copied()
            .unwrap_or(self.suite_noise_pct)
    }
}

/// Compute noise floor from N runs of the **same** baseline bench JSON.
///
/// `runs` — vector of paths to JSON files (output of `nova bench run --out`
/// on the same source N times consecutively).
///
/// Algorithm:
/// 1. Parse all runs.
/// 2. For each bench name, compute pairwise deltas (run[i] vs run[i+1])
///    median: `delta_pct = |(b - a) / a| * 100`.
/// 3. Per-bench noise floor = max pairwise delta (worst-case).
/// 4. Suite-wide = median across all per-bench noise floors.
pub fn calibrate(runs: &[PathBuf]) -> Result<NoiseFloor> {
    if runs.len() < 2 {
        return Err(anyhow!("calibration requires >= 2 runs, got {}", runs.len()));
    }
    let mut parsed: Vec<RunResultParsed> = Vec::with_capacity(runs.len());
    for p in runs {
        let raw = std::fs::read_to_string(p)
            .map_err(|e| anyhow!("read {}: {}", p.display(), e))?;
        let v: Value = serde_json::from_str(&raw)
            .map_err(|e| anyhow!("parse {}: {}", p.display(), e))?;
        let r = RunResultParsed::from_json(&v)
            .map_err(|e| anyhow!("schema {}: {}", p.display(), e))?;
        parsed.push(r);
    }

    // Use metadata from first run.
    let first_meta = &parsed[0].metadata;
    let fingerprint = format!("{}|{}|{}",
        first_meta.cpu_model.as_deref().unwrap_or("unknown"),
        first_meta.os, first_meta.arch);

    // Bench name → vector of pairwise delta_pct values.
    let mut pairwise_deltas: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();

    for i in 0..parsed.len() - 1 {
        for j in (i + 1)..parsed.len() {
            for ab in &parsed[i].benches {
                if let Some(bb) = parsed[j].benches.iter().find(|b| b.raw.name == ab.raw.name) {
                    let a = ab.stats_ns.median;
                    let b = bb.stats_ns.median;
                    if a > 0.0 {
                        let d = ((b - a) / a * 100.0).abs();
                        pairwise_deltas.entry(ab.raw.name.clone())
                            .or_default()
                            .push(d);
                    }
                }
            }
        }
    }

    // Per-bench noise = 90th percentile of pairwise deltas (conservative).
    let mut per_bench: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut suite_values = Vec::new();
    for (name, mut deltas) in pairwise_deltas {
        if deltas.is_empty() { continue; }
        deltas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p90_idx = ((deltas.len() as f64 * 0.9) as usize).min(deltas.len() - 1);
        let noise = deltas[p90_idx];
        suite_values.push(noise);
        per_bench.insert(name, noise);
    }
    let suite_noise_pct = if suite_values.is_empty() {
        0.0
    } else {
        suite_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        suite_values[suite_values.len() / 2]  // median
    };

    Ok(NoiseFloor {
        machine_fingerprint: fingerprint,
        calibrated_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        calibration_runs: parsed.len() as u32,
        per_bench,
        suite_noise_pct,
    })
}

/// Verify machine fingerprint matches between calibration and current run.
pub fn fingerprint_matches(noise: &NoiseFloor, meta: &super::repro::ReproMeta) -> bool {
    let current = format!("{}|{}|{}",
        meta.cpu_model.as_deref().unwrap_or("unknown"),
        meta.os, meta.arch);
    current == noise.machine_fingerprint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_roundtrip() {
        let n = NoiseFloor {
            machine_fingerprint: "AMD Ryzen|linux|x86_64".to_string(),
            calibrated_at_unix: 1779494400,
            calibration_runs: 5,
            per_bench: {
                let mut m = std::collections::HashMap::new();
                m.insert("foo".to_string(), 2.3);
                m.insert("bar".to_string(), 1.8);
                m
            },
            suite_noise_pct: 2.0,
        };
        let tmp = std::env::temp_dir().join("nova-noise-test.json");
        n.save(&tmp).unwrap();
        let l = NoiseFloor::load(&tmp).unwrap();
        assert_eq!(l.machine_fingerprint, n.machine_fingerprint);
        assert_eq!(l.calibration_runs, n.calibration_runs);
        assert_eq!(l.per_bench.get("foo"), n.per_bench.get("foo"));
        assert_eq!(l.suite_noise_pct, n.suite_noise_pct);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn get_fallback_to_suite() {
        let n = NoiseFloor {
            machine_fingerprint: "x".to_string(),
            calibrated_at_unix: 0,
            calibration_runs: 1,
            per_bench: std::collections::HashMap::new(),
            suite_noise_pct: 5.0,
        };
        assert_eq!(n.get("anything"), 5.0);
    }
}
