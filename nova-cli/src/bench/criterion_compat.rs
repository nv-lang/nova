// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.B.2 — Criterion-compatible JSON output.
//!
//! Emits JSON files в layout совместимом с Criterion 0.5 schema:
//!   `<out>/<bench-id>/new/estimates.json`
//!   `<out>/<bench-id>/new/sample.json`
//!   `<out>/<bench-id>/new/benchmark.json`
//!
//! Это позволяет downstream tools (`cargo-criterion --message-format=...`,
//! `criterion-table`, `criterion-cmp`) обрабатывать Nova bench results.
//!
//! Reference: https://bheisler.github.io/criterion.rs/book/analysis.html

use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::schema::AnalyzedBench;

/// Write Criterion-compatible JSON layout для одного bench.
/// Creates: `<out_dir>/<safe_name>/new/{estimates,sample,benchmark}.json`
pub fn write_bench(out_dir: &Path, bench: &AnalyzedBench) -> Result<()> {
    let safe = sanitize(&bench.raw.name);
    let bench_dir = out_dir.join(&safe).join("new");
    std::fs::create_dir_all(&bench_dir)
        .map_err(|e| anyhow!("create {}: {}", bench_dir.display(), e))?;

    // estimates.json — Criterion stats blob.
    let estimates = estimates_json(bench);
    std::fs::write(bench_dir.join("estimates.json"),
        serde_json::to_string_pretty(&estimates)?)
        .map_err(|e| anyhow!("write estimates.json: {}", e))?;

    // sample.json — raw timing samples + iter counts.
    let sample = sample_json(bench);
    std::fs::write(bench_dir.join("sample.json"),
        serde_json::to_string_pretty(&sample)?)
        .map_err(|e| anyhow!("write sample.json: {}", e))?;

    // benchmark.json — metadata.
    let metadata = benchmark_metadata_json(bench);
    std::fs::write(bench_dir.join("benchmark.json"),
        serde_json::to_string_pretty(&metadata)?)
        .map_err(|e| anyhow!("write benchmark.json: {}", e))?;

    Ok(())
}

/// Write all benches into Criterion layout.
pub fn write_all(out_dir: &Path, benches: &[AnalyzedBench]) -> Result<usize> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| anyhow!("create out dir: {}", e))?;
    for b in benches {
        write_bench(out_dir, b)?;
    }
    Ok(benches.len())
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

/// Criterion `estimates.json` shape (simplified — необходимый минимум).
fn estimates_json(bench: &AnalyzedBench) -> Value {
    let st = &bench.stats_ns;
    // Criterion uses {point_estimate, standard_error, confidence_interval}.
    let mk = |est: f64, se: f64, lo: f64, hi: f64| -> Value {
        json!({
            "point_estimate": est,
            "standard_error": se,
            "confidence_interval": {
                "confidence_level": 0.95,
                "lower_bound": lo,
                "upper_bound": hi,
            }
        })
    };
    json!({
        "mean": mk(st.mean, st.stddev / (st.n as f64).sqrt(),
                   st.mean - 1.96 * st.stddev / (st.n as f64).sqrt(),
                   st.mean + 1.96 * st.stddev / (st.n as f64).sqrt()),
        "median": mk(st.median, st.mad, st.ci95_lo, st.ci95_hi),
        "median_abs_dev": mk(st.mad, 0.0, 0.0, st.mad * 2.0),
        "slope": mk(st.median, st.mad, st.ci95_lo, st.ci95_hi),  // single-point slope
        "std_dev": mk(st.stddev, 0.0, 0.0, st.stddev * 2.0),
    })
}

/// Criterion `sample.json` — raw timing data.
fn sample_json(bench: &AnalyzedBench) -> Value {
    let iters_per: u64 = bench.raw.iters_per_sample;
    let iters: Vec<u64> = (0..bench.raw.raw_ns.len()).map(|_| iters_per).collect();
    // Times — это total ns per sample (raw_ns хранит per-iter), нужно
    // умножить обратно для Criterion-style.
    let times: Vec<u64> = bench.raw.raw_ns.iter()
        .map(|per_iter_ns| per_iter_ns * iters_per)
        .collect();
    json!({
        "sampling_mode": "Linear",
        "iters": iters,
        "times": times,
    })
}

fn benchmark_metadata_json(bench: &AnalyzedBench) -> Value {
    json!({
        "group_id": "nova_bench",
        "function_id": bench.raw.name,
        "value_str": null,
        "throughput": throughput_json(bench),
        "full_id": bench.raw.name,
        "directory_name": sanitize(&bench.raw.name),
        "title": bench.raw.name,
    })
}

fn throughput_json(bench: &AnalyzedBench) -> Value {
    if let Some(b) = bench.raw.throughput_bytes {
        json!({ "Bytes": b })
    } else if let Some(e) = bench.raw.throughput_elements {
        json!({ "Elements": e })
    } else {
        Value::Null
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::schema::RawBenchResult;

    #[test]
    fn sanitize_basic() {
        assert_eq!(sanitize("foo"), "foo");
        assert_eq!(sanitize("a/b/c"), "a_b_c");
        assert_eq!(sanitize("hashmap_insert_n=1000"), "hashmap_insert_n_1000");
    }

    #[test]
    fn writes_layout() {
        let raw = RawBenchResult {
            name: "foo_bench".to_string(),
            iters_per_sample: 10,
            samples_count: 5,
            raw_ns: vec![100, 110, 90, 105, 95],
            throughput_bytes: Some(1024),
            throughput_elements: None,
            allocs_per_iter: None,
            allocs_total: None,
        };
        let bench = AnalyzedBench::from_raw(raw).unwrap();
        let tmp = std::env::temp_dir().join("nova-crit-test");
        let _ = std::fs::remove_dir_all(&tmp);
        write_bench(&tmp, &bench).unwrap();
        assert!(tmp.join("foo_bench/new/estimates.json").exists());
        assert!(tmp.join("foo_bench/new/sample.json").exists());
        assert!(tmp.join("foo_bench/new/benchmark.json").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
