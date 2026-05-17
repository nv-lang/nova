// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.E.5 — Changepoint anomaly auto-detection.
//!
//! PELT (Pruned Exact Linear Time, Killick et al. 2012) — O(n)
//! algorithm для multiple changepoint detection в time series.
//!
//! Use case: scan historical bench median time-series, identify
//! где perf significantly shifted (likely from a specific commit).
//!
//! Algorithm:
//!   F(0) = -β
//!   F(t) = min over s<t: F(s) + C(s+1..t) + β
//!   C(s..t) — segment cost (SSE around segment mean).
//!   β — penalty constant (BIC-like: c·log(n)).
//!
//! Pruning: discard candidate s if F(s) + C(s+1..t) >= F(t) — never
//! be optimal предок. Reduces O(n²) brute force к O(n) amortized.

use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::history;
use super::schema::RunResultParsed;

/// Detected changepoint в time-series.
#[derive(Debug, Clone)]
pub struct Changepoint {
    /// Index в input series (0-based).
    pub index: usize,
    /// Mean ДО changepoint (segment перед).
    pub mean_before: f64,
    /// Mean ПОСЛЕ changepoint (segment после).
    pub mean_after: f64,
    /// Delta % = (after - before) / before * 100.
    pub delta_pct: f64,
}

/// PELT changepoint detection. Returns list of cutpoints (segment
/// boundaries — индексы где новый segment начинается).
///
/// `penalty` — β (typical: 2·log(n) — BIC; 4·log(n) — stricter).
/// Returns vector of usize >= 1 (cutpoints внутри [1, n-1]).
pub fn pelt(series: &[f64], penalty: f64) -> Vec<usize> {
    let n = series.len();
    if n < 4 { return Vec::new(); }

    // Pre-compute prefix sums для O(1) segment-mean / SSE.
    let mut prefix_sum = vec![0.0; n + 1];
    let mut prefix_sq = vec![0.0; n + 1];
    for i in 0..n {
        prefix_sum[i + 1] = prefix_sum[i] + series[i];
        prefix_sq[i + 1]  = prefix_sq[i]  + series[i] * series[i];
    }
    // SSE для [s..e] (0-indexed inclusive lo, exclusive hi).
    let sse = |s: usize, e: usize| -> f64 {
        let n_seg = (e - s) as f64;
        if n_seg <= 0.0 { return 0.0; }
        let sum = prefix_sum[e] - prefix_sum[s];
        let sq  = prefix_sq[e]  - prefix_sq[s];
        // SSE = Σx² - (Σx)²/n.
        sq - sum * sum / n_seg
    };

    let mut f = vec![f64::INFINITY; n + 1];
    let mut parent = vec![0usize; n + 1];
    f[0] = -penalty;
    let mut candidates: Vec<usize> = vec![0];

    for t in 1..=n {
        let mut best_val = f64::INFINITY;
        let mut best_s = 0usize;
        for &s in &candidates {
            let v = f[s] + sse(s, t) + penalty;
            if v < best_val {
                best_val = v;
                best_s = s;
            }
        }
        f[t] = best_val;
        parent[t] = best_s;
        // PELT pruning: разрешает только candidates где F(s) + C(s,t) <= F(t).
        let new_candidates: Vec<usize> = candidates.iter()
            .filter(|&&s| f[s] + sse(s, t) <= f[t] + penalty)
            .cloned()
            .collect();
        candidates = new_candidates;
        candidates.push(t);
    }

    // Backtrack: walk parent[n] → ... → 0; reverse для chronological.
    let mut cuts = Vec::new();
    let mut t = n;
    while t > 0 {
        let s = parent[t];
        if s > 0 { cuts.push(s); }
        t = s;
    }
    cuts.reverse();
    cuts
}

/// Convert cutpoints → Changepoint structs (с before/after means).
pub fn cuts_to_changepoints(series: &[f64], cuts: &[usize]) -> Vec<Changepoint> {
    if cuts.is_empty() || series.is_empty() { return Vec::new(); }
    let mut out = Vec::with_capacity(cuts.len());
    for (i, &cut) in cuts.iter().enumerate() {
        let prev_start = if i == 0 { 0 } else { cuts[i - 1] };
        let next_end = if i + 1 < cuts.len() { cuts[i + 1] } else { series.len() };
        let mean_before = {
            let seg: &[f64] = &series[prev_start..cut];
            if seg.is_empty() { 0.0 }
            else { seg.iter().sum::<f64>() / seg.len() as f64 }
        };
        let mean_after = {
            let seg: &[f64] = &series[cut..next_end];
            if seg.is_empty() { 0.0 }
            else { seg.iter().sum::<f64>() / seg.len() as f64 }
        };
        let delta_pct = if mean_before > 0.0 {
            (mean_after - mean_before) / mean_before * 100.0
        } else { 0.0 };
        out.push(Changepoint { index: cut, mean_before, mean_after, delta_pct });
    }
    out
}

/// Compute reasonable default penalty (BIC-like): 4·log(n)·σ² где σ — std-dev.
/// `2 log n` underfits, `4 log n` is conservative — preferred для CI noise.
pub fn default_penalty(series: &[f64]) -> f64 {
    if series.len() < 2 { return 1.0; }
    let n = series.len() as f64;
    let mean: f64 = series.iter().sum::<f64>() / n;
    let var = series.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    4.0 * n.ln() * var.max(1.0)
}

/// Scan history-branch для anomalies, return per-bench changepoints.
pub fn scan_history(repo: &Path, branch: &str)
    -> Result<Vec<(String, Vec<(history::HistoryEntry, Changepoint)>)>>
{
    let entries = history::list(repo, branch)?;
    if entries.is_empty() {
        return Err(anyhow!("no entries в branch `{}`", branch));
    }
    let mut chronological: Vec<history::HistoryEntry> = entries.clone();
    chronological.reverse();  // list returns newest first; want oldest first.

    // Parse all runs.
    let mut runs: Vec<(history::HistoryEntry, RunResultParsed)> = Vec::new();
    for e in chronological {
        let content = history::read_entry(repo, branch, &e.filename)
            .map_err(|err| anyhow!("read {}: {}", e.filename, err))?;
        let v: Value = serde_json::from_str(&content)
            .map_err(|err| anyhow!("parse {}: {}", e.filename, err))?;
        if let Ok(r) = RunResultParsed::from_json(&v) {
            runs.push((e, r));
        }
    }

    // Collect bench names (union).
    let mut bench_names: Vec<String> = Vec::new();
    for (_, run) in &runs {
        for b in &run.benches {
            if !bench_names.contains(&b.raw.name) {
                bench_names.push(b.raw.name.clone());
            }
        }
    }

    let mut results: Vec<(String, Vec<(history::HistoryEntry, Changepoint)>)>
        = Vec::with_capacity(bench_names.len());

    for name in bench_names {
        // Build series of median ns per run (skip missing).
        let mut series: Vec<f64> = Vec::new();
        let mut entries_for_series: Vec<history::HistoryEntry> = Vec::new();
        for (entry, run) in &runs {
            if let Some(b) = run.benches.iter().find(|b| b.raw.name == name) {
                series.push(b.stats_ns.median);
                entries_for_series.push(entry.clone());
            }
        }
        if series.len() < 4 { continue; }

        let penalty = default_penalty(&series);
        let cuts = pelt(&series, penalty);
        let cps = cuts_to_changepoints(&series, &cuts);

        let mut cp_with_entries: Vec<(history::HistoryEntry, Changepoint)>
            = cps.into_iter()
                .filter_map(|c| entries_for_series.get(c.index).cloned()
                    .map(|e| (e, c)))
                .collect();

        // Filter trivial: skip changepoints с |delta| < 5% (likely noise).
        cp_with_entries.retain(|(_, c)| c.delta_pct.abs() >= 5.0);

        if !cp_with_entries.is_empty() {
            results.push((name, cp_with_entries));
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pelt_no_change_flat_series() {
        let s = vec![100.0; 30];
        let cuts = pelt(&s, default_penalty(&s));
        assert!(cuts.is_empty(), "flat series shouldn't have cps, got {:?}", cuts);
    }

    #[test]
    fn pelt_detects_step_change() {
        // 15 samples mean 100, then 15 samples mean 200.
        let mut s: Vec<f64> = (0..15).map(|_| 100.0 + (rand_lcg() % 5) as f64).collect();
        s.extend((0..15).map(|_| 200.0 + (rand_lcg() % 5) as f64));
        let cuts = pelt(&s, default_penalty(&s));
        assert!(!cuts.is_empty(), "step change should be detected");
        // First cut should be near index 15.
        let first = cuts[0];
        assert!((first as i64 - 15).abs() <= 3,
            "expected cut near 15, got {}", first);
    }

    #[test]
    fn cuts_to_changepoints_computes_delta() {
        let s = vec![100.0, 100.0, 100.0, 100.0, 100.0,
                     200.0, 200.0, 200.0, 200.0, 200.0];
        let cuts = vec![5];
        let cps = cuts_to_changepoints(&s, &cuts);
        assert_eq!(cps.len(), 1);
        assert_eq!(cps[0].index, 5);
        assert!((cps[0].mean_before - 100.0).abs() < 1e-9);
        assert!((cps[0].mean_after - 200.0).abs() < 1e-9);
        assert!((cps[0].delta_pct - 100.0).abs() < 1e-9);
    }

    // Simple LCG для test reproducibility (no rand crate).
    fn rand_lcg() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static STATE: AtomicU64 = AtomicU64::new(0x12345);
        let s = STATE.fetch_update(Ordering::SeqCst, Ordering::SeqCst,
            |x| Some(x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)))
            .unwrap();
        s
    }
}
