// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57 L4 — JSON v1 schema для bench results.
//!
//! Schema стабильна (versioned via `format_version`); миграции —
//! явные через soak period (см. Plan 57 §R3 risk register).
//!
//! Без serde-derive — пишем JSON ручками через `serde_json::json!()` macro
//! (уже зависимость nova-cli). Это минимизирует Cargo dep-tree, согласовано
//! с feedback_third_party_libs.

use serde_json::{json, Value};

use super::repro::ReproMeta;
use super::stats::SampleStats;

pub const SCHEMA_VERSION: &str = "1";

/// Один JSONL "__BENCH_RESULT__ {...}" line, прочитанный от запущенного
/// bench-бинаря (codegen-эмиттер пишет это в stdout).
#[derive(Debug, Clone)]
pub struct RawBenchResult {
    pub name: String,
    pub iters_per_sample: u64,
    pub samples_count: u64,
    pub raw_ns: Vec<u64>,
    pub throughput_bytes: Option<u64>,
    pub throughput_elements: Option<u64>,
    pub allocs_per_iter: Option<i64>,
    pub allocs_total: Option<i64>,
    /// Plan 57.C.4: per-sample CPU instructions / iter (Linux only).
    pub cpu_instructions: Vec<u64>,
    /// Plan 57.G.5: per-bench custom metrics collected from
    /// `__BENCH_METRIC__` markers between __BENCH_START__ and
    /// __BENCH_RESULT__. Aggregated by (name, unit) tuple key.
    pub custom_metrics: Vec<CustomMetric>,
}

/// Plan 57.G.5 — custom user-defined metric (closes gap vs Go
/// `b.ReportMetric`). Each `bench.metric(name, value, unit)` call
/// emits one sample value; CLI aggregates за весь bench-run.
#[derive(Debug, Clone)]
pub struct CustomMetric {
    pub name: String,
    pub unit: String,
    pub samples: Vec<i64>,
}

impl CustomMetric {
    pub fn count(&self) -> usize { self.samples.len() }
    pub fn min(&self) -> Option<i64> { self.samples.iter().min().copied() }
    pub fn max(&self) -> Option<i64> { self.samples.iter().max().copied() }
    pub fn sum(&self) -> i64        { self.samples.iter().sum() }
    pub fn median(&self) -> Option<f64> {
        if self.samples.is_empty() { return None; }
        let mut s = self.samples.clone();
        s.sort();
        let n = s.len();
        Some(if n % 2 == 1 { s[n / 2] as f64 }
             else { (s[n / 2 - 1] as f64 + s[n / 2] as f64) / 2.0 })
    }
}

/// Plan 57.G.5: parse a `__BENCH_METRIC__\t<name>\t<value>\t<unit>` line.
/// Returns Some((name, value, unit)) если distinct marker, иначе None.
pub fn parse_metric_line(line: &str) -> Option<(String, i64, String)> {
    let body = line.strip_prefix("__BENCH_METRIC__")?;
    let parts: Vec<&str> = body.split('\t').collect();
    // body starts с TAB, so parts[0] = "", parts[1] = name, [2] = value, [3] = unit.
    if parts.len() < 4 { return None; }
    let name = parts[1].to_string();
    let value: i64 = parts[2].trim().parse().ok()?;
    // Trailing unit may have newline; trim.
    let unit = parts[3].trim_end_matches(['\r', '\n']).to_string();
    Some((name, value, unit))
}

impl RawBenchResult {
    /// Parse __BENCH_RESULT__ line. Returns None если line не bench-result.
    pub fn parse_line(line: &str) -> Option<Self> {
        let body = line.strip_prefix("__BENCH_RESULT__ ")?.trim();
        let v: Value = serde_json::from_str(body).ok()?;
        let name = v.get("name")?.as_str()?.to_string();
        let iters_per_sample = v.get("iters_per_sample")?.as_u64()?;
        let samples_count = v.get("samples_count")?.as_u64()?;
        let raw_ns_arr = v.get("raw_ns")?.as_array()?;
        let raw_ns: Vec<u64> = raw_ns_arr.iter()
            .filter_map(|x| x.as_u64())
            .collect();
        let throughput_bytes = v.get("throughput_bytes").and_then(|x| x.as_u64());
        let throughput_elements = v.get("throughput_elements").and_then(|x| x.as_u64());
        let allocs_per_iter = v.get("allocs_per_iter").and_then(|x| x.as_i64());
        let allocs_total = v.get("allocs_total").and_then(|x| x.as_i64());
        let cpu_instructions: Vec<u64> = v.get("cpu_instructions")
            .and_then(|x| x.as_array())
            .map(|arr| arr.iter().filter_map(|y| y.as_u64()).collect())
            .unwrap_or_default();
        Some(Self {
            name, iters_per_sample, samples_count, raw_ns,
            throughput_bytes, throughput_elements,
            allocs_per_iter, allocs_total, cpu_instructions,
            custom_metrics: Vec::new(),
        })
    }
}

/// Полный bench result после статанализа.
#[derive(Debug, Clone)]
pub struct AnalyzedBench {
    pub raw: RawBenchResult,
    pub stats_ns: SampleStats,
}

impl AnalyzedBench {
    pub fn from_raw(raw: RawBenchResult) -> Option<Self> {
        if raw.raw_ns.is_empty() {
            return None;
        }
        let samples_f64: Vec<f64> = raw.raw_ns.iter().map(|x| *x as f64).collect();
        let stats_ns = super::stats::analyze(&samples_f64);
        Some(Self { raw, stats_ns })
    }

    /// Throughput in bytes/sec (если bench.bytes() set) or None.
    pub fn throughput_bytes_per_sec(&self) -> Option<f64> {
        let b = self.raw.throughput_bytes? as f64;
        if self.stats_ns.median <= 0.0 { return None; }
        Some(b * 1e9 / self.stats_ns.median)
    }

    /// Throughput in elements/sec.
    pub fn throughput_elements_per_sec(&self) -> Option<f64> {
        let e = self.raw.throughput_elements? as f64;
        if self.stats_ns.median <= 0.0 { return None; }
        Some(e * 1e9 / self.stats_ns.median)
    }
}

/// Сериализация одного AnalyzedBench в JSON-объект.
pub fn analyzed_to_json(a: &AnalyzedBench) -> Value {
    let st = &a.stats_ns;
    let mut obj = json!({
        "name": a.raw.name,
        "iters_per_sample": a.raw.iters_per_sample,
        "samples_count": a.raw.samples_count,
        "raw_ns": a.raw.raw_ns,
        "stats": {
            "n": st.n,
            "median_ns": st.median,
            "mad_ns": st.mad,
            "mean_ns": st.mean,
            "stddev_ns": st.stddev,
            "p25_ns": st.p25,
            "p75_ns": st.p75,
            "iqr_ns": st.iqr,
            "min_ns": st.min,
            "max_ns": st.max,
            "ci95_lo_ns": st.ci95_lo,
            "ci95_hi_ns": st.ci95_hi,
            "outliers_low": st.outliers_low,
            "outliers_high": st.outliers_high,
            "drift_slope_ns_per_sample": st.drift_slope_ns_per_sample,
            "drift_r_squared": st.drift_r_squared,
        }
    });
    let m = obj.as_object_mut()
        .expect("invariant: json!({...}) constructs Value::Object");
    if let Some(b) = a.raw.throughput_bytes {
        m.insert("throughput_bytes".to_string(), json!(b));
        if let Some(bps) = a.throughput_bytes_per_sec() {
            m.insert("throughput_bytes_per_sec".to_string(), json!(bps));
        }
    }
    if let Some(e) = a.raw.throughput_elements {
        m.insert("throughput_elements".to_string(), json!(e));
        if let Some(eps) = a.throughput_elements_per_sec() {
            m.insert("throughput_elements_per_sec".to_string(), json!(eps));
        }
    }
    if let Some(a_pi) = a.raw.allocs_per_iter {
        m.insert("allocs_per_iter".to_string(), json!(a_pi));
    }
    if let Some(a_t) = a.raw.allocs_total {
        m.insert("allocs_total".to_string(), json!(a_t));
    }
    if !a.raw.cpu_instructions.is_empty() {
        m.insert("cpu_instructions".to_string(), json!(a.raw.cpu_instructions));
        let mut sorted = a.raw.cpu_instructions.clone();
        sorted.sort();
        let median_instr = sorted[sorted.len() / 2];
        m.insert("cpu_instructions_median".to_string(), json!(median_instr));
    }
    // Plan 57.G.5 — custom metrics aggregate per (name, unit).
    if !a.raw.custom_metrics.is_empty() {
        let arr: Vec<Value> = a.raw.custom_metrics.iter().map(|cm| {
            json!({
                "name": cm.name,
                "unit": cm.unit,
                "count": cm.count(),
                "min": cm.min(),
                "max": cm.max(),
                "sum": cm.sum(),
                "median": cm.median(),
                "samples": cm.samples,
            })
        }).collect();
        m.insert("custom_metrics".to_string(), json!(arr));
    }
    obj
}

/// Полный run result с metadata + benches.
pub fn run_result_to_json(meta: &ReproMeta, benches: &[AnalyzedBench]) -> Value {
    let bench_objs: Vec<Value> = benches.iter().map(analyzed_to_json).collect();
    json!({
        "format_version": SCHEMA_VERSION,
        "metadata": meta.to_json(),
        "benches": bench_objs,
    })
}

/// Парсинг run JSON для `nova bench diff` consumption.
///
/// `format_version` хранится для будущей JSON v2 schema migration —
/// consumer'ы могут проверять `parsed.format_version != "1"` и fall'нуть
/// fast. Сейчас только `nova bench v1` всё что мы emit'им, поэтому
/// поле не читается в текущем pipeline.
#[derive(Debug, Clone)]
pub struct RunResultParsed {
    #[allow(dead_code)] // reserved for v2 schema migration
    pub format_version: String,
    pub metadata: ReproMeta,
    pub benches: Vec<AnalyzedBench>,
}

impl RunResultParsed {
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let format_version = v.get("format_version")
            .and_then(|x| x.as_str())
            .ok_or_else(|| "missing format_version".to_string())?
            .to_string();
        if format_version != SCHEMA_VERSION {
            return Err(format!(
                "schema version mismatch: file has {}, this tool supports {}",
                format_version, SCHEMA_VERSION
            ));
        }
        let metadata = ReproMeta::from_json(v.get("metadata")
            .ok_or_else(|| "missing metadata".to_string())?)?;
        let benches_arr = v.get("benches")
            .and_then(|x| x.as_array())
            .ok_or_else(|| "missing benches array".to_string())?;
        let mut benches = Vec::new();
        for b in benches_arr {
            let name = b.get("name").and_then(|x| x.as_str())
                .ok_or_else(|| "bench missing name".to_string())?.to_string();
            let iters_per_sample = b.get("iters_per_sample")
                .and_then(|x| x.as_u64()).unwrap_or(1);
            let samples_count = b.get("samples_count")
                .and_then(|x| x.as_u64()).unwrap_or(0);
            let raw_ns: Vec<u64> = b.get("raw_ns")
                .and_then(|x| x.as_array())
                .map(|arr| arr.iter().filter_map(|y| y.as_u64()).collect())
                .unwrap_or_default();
            let throughput_bytes = b.get("throughput_bytes").and_then(|x| x.as_u64());
            let throughput_elements = b.get("throughput_elements").and_then(|x| x.as_u64());
            let allocs_per_iter = b.get("allocs_per_iter").and_then(|x| x.as_i64());
            let allocs_total = b.get("allocs_total").and_then(|x| x.as_i64());
            let cpu_instructions: Vec<u64> = b.get("cpu_instructions")
                .and_then(|x| x.as_array())
                .map(|arr| arr.iter().filter_map(|y| y.as_u64()).collect())
                .unwrap_or_default();
            // Plan 57.G.5 — custom_metrics reconstruct (samples array stored).
            let custom_metrics: Vec<CustomMetric> = b.get("custom_metrics")
                .and_then(|x| x.as_array())
                .map(|arr| arr.iter().filter_map(|m| {
                    let name = m.get("name")?.as_str()?.to_string();
                    let unit = m.get("unit")?.as_str().unwrap_or("").to_string();
                    let samples = m.get("samples")?.as_array()?.iter()
                        .filter_map(|v| v.as_i64()).collect();
                    Some(CustomMetric { name, unit, samples })
                }).collect())
                .unwrap_or_default();
            let raw = RawBenchResult {
                name, iters_per_sample, samples_count, raw_ns,
                throughput_bytes, throughput_elements,
                allocs_per_iter, allocs_total, cpu_instructions,
                custom_metrics,
            };
            if let Some(a) = AnalyzedBench::from_raw(raw) {
                benches.push(a);
            }
        }
        Ok(Self { format_version, metadata, benches })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_raw_line() {
        let line = r#"__BENCH_RESULT__ {"name":"x","iters_per_sample":10,"samples_count":3,"raw_ns":[100,110,90]}"#;
        let r = RawBenchResult::parse_line(line).unwrap();
        assert_eq!(r.name, "x");
        assert_eq!(r.iters_per_sample, 10);
        assert_eq!(r.samples_count, 3);
        assert_eq!(r.raw_ns, vec![100, 110, 90]);
        assert!(r.throughput_bytes.is_none());
    }

    #[test]
    fn parse_with_throughput() {
        let line = r#"__BENCH_RESULT__ {"name":"y","iters_per_sample":1,"samples_count":2,"raw_ns":[1000,1100],"throughput_bytes":4096,"allocs_per_iter":3,"allocs_total":6}"#;
        let r = RawBenchResult::parse_line(line).unwrap();
        assert_eq!(r.throughput_bytes, Some(4096));
        assert_eq!(r.allocs_per_iter, Some(3));
        assert_eq!(r.allocs_total, Some(6));
    }

    #[test]
    fn reject_non_bench_line() {
        assert!(RawBenchResult::parse_line("Hello world").is_none());
        assert!(RawBenchResult::parse_line("__BENCH_START__ \"foo\"").is_none());
    }

    // Plan 57.G.5 — custom metric parsing + aggregation tests.

    #[test]
    fn parse_metric_line_basic() {
        let line = "__BENCH_METRIC__\tcache_hits\t42\tcount";
        let (n, v, u) = parse_metric_line(line).unwrap();
        assert_eq!(n, "cache_hits");
        assert_eq!(v, 42);
        assert_eq!(u, "count");
    }

    #[test]
    fn parse_metric_line_empty_unit() {
        let line = "__BENCH_METRIC__\tlock_wait\t100\t";
        let (n, v, u) = parse_metric_line(line).unwrap();
        assert_eq!(n, "lock_wait");
        assert_eq!(v, 100);
        assert_eq!(u, "");
    }

    #[test]
    fn parse_metric_line_negative_value() {
        let line = "__BENCH_METRIC__\tdelta\t-7\tunits";
        let (n, v, _) = parse_metric_line(line).unwrap();
        assert_eq!(n, "delta");
        assert_eq!(v, -7);
    }

    #[test]
    fn parse_metric_line_rejects_other_markers() {
        assert!(parse_metric_line("__BENCH_RESULT__ {...}").is_none());
        assert!(parse_metric_line("__BENCH_START__ foo").is_none());
        assert!(parse_metric_line("plain text").is_none());
    }

    #[test]
    fn parse_metric_line_rejects_non_int_value() {
        let line = "__BENCH_METRIC__\tx\thello\tunit";
        assert!(parse_metric_line(line).is_none());
    }

    #[test]
    fn custom_metric_aggregates() {
        let cm = CustomMetric {
            name: "x".to_string(),
            unit: "ms".to_string(),
            samples: vec![10, 20, 30, 40, 50],
        };
        assert_eq!(cm.count(), 5);
        assert_eq!(cm.min(), Some(10));
        assert_eq!(cm.max(), Some(50));
        assert_eq!(cm.sum(), 150);
        assert_eq!(cm.median(), Some(30.0));
    }

    #[test]
    fn custom_metric_median_even_count() {
        let cm = CustomMetric {
            name: "x".to_string(), unit: "".to_string(),
            samples: vec![10, 20, 30, 40],
        };
        assert_eq!(cm.median(), Some(25.0));  // (20+30)/2
    }

    #[test]
    fn custom_metric_empty_returns_none() {
        let cm = CustomMetric {
            name: "x".to_string(), unit: "".to_string(),
            samples: vec![],
        };
        assert_eq!(cm.count(), 0);
        assert_eq!(cm.min(), None);
        assert_eq!(cm.max(), None);
        assert_eq!(cm.median(), None);
    }
}
