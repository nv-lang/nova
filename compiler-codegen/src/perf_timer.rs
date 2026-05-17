// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.C.1 — Per-pass compiler PerfTimer hooks.
//!
//! `PerfTimer::new("parse")` стартует timer; на `drop` или `.stop()`
//! emit'ит `__PERF__ <pass> <ns>` на stderr, если env `NOVA_PERF_TIMER=1`.
//!
//! Zero overhead когда env не set (lazy probe + early return).
//! Используется `nova bench corpus <file> --breakdown` для парсинга
//! stderr stream → JSON per-pass timings.
//!
//! Реализация — pure std, без deps. Time via `std::time::Instant`.

use std::time::Instant;
use std::sync::Mutex;
use std::collections::HashMap;

/// Plan 57.D.1: Aggregation mode (NOVA_PERF_TIMER_AGGREGATE=1).
/// Когда set, PerfTimer suppresses per-call emit и accumulates samples
/// в shared mutable state. `dump_aggregated()` rendere'ит сводку.
static AGGREGATOR: Mutex<Option<HashMap<String, Vec<u64>>>> = Mutex::new(None);

/// RAII timer: при создании captures Instant::now(), при drop emits
/// `__PERF__ <name> <ns>` на stderr (если NOVA_PERF_TIMER=1).
///
/// Usage:
/// ```ignore
/// let _t = PerfTimer::new("parse");
/// nova_codegen::parser::parse(src)?;
/// // _t dropped → emits if enabled
/// ```
///
/// Или explicit stop:
/// ```ignore
/// let t = PerfTimer::new("type-check");
/// nova_codegen::types::check_module(&m)?;
/// t.stop();  // emit now вместо при drop (важно если scope живёт дольше)
/// ```
pub struct PerfTimer {
    name: &'static str,
    start: Instant,
    stopped: bool,
}

impl PerfTimer {
    /// Start a timer. Cheap — single Instant::now().
    /// Emission triggered только при drop / stop if env active.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            start: Instant::now(),
            stopped: false,
        }
    }

    /// Explicit stop — emits __PERF__ marker (if enabled), prevents drop emission.
    pub fn stop(mut self) {
        self.emit();
        self.stopped = true;
    }

    fn emit(&mut self) {
        if !is_enabled() && !is_aggregating() { return; }
        let ns = self.start.elapsed().as_nanos() as u64;
        if is_aggregating() {
            // Accumulate в shared aggregator вместо per-call emit.
            let mut g = AGGREGATOR.lock().unwrap();
            let map = g.get_or_insert_with(HashMap::new);
            map.entry(self.name.to_string()).or_default().push(ns);
        } else {
            eprintln!("__PERF__ {} {}", self.name, ns);
        }
    }
}

/// Plan 57.D.1: enable aggregation mode (e.g. `nova test`-side caller).
pub fn enable_aggregation() {
    let mut g = AGGREGATOR.lock().unwrap();
    if g.is_none() { *g = Some(HashMap::new()); }
}

fn is_aggregating() -> bool {
    if let Ok(g) = AGGREGATOR.try_lock() {
        return g.is_some();
    }
    false
}

/// Plan 57.D.1: render aggregated summary table. Returns empty string
/// если aggregation не enabled или нет samples. Sorted by total ns desc.
pub fn dump_aggregated() -> String {
    let g = AGGREGATOR.lock().unwrap();
    let map = match g.as_ref() {
        Some(m) if !m.is_empty() => m,
        _ => return String::new(),
    };
    let mut rows: Vec<(&String, u64, u64, usize)> = map.iter()
        .map(|(name, samples)| {
            let total: u64 = samples.iter().sum();
            let mut sorted = samples.clone();
            sorted.sort();
            let median = sorted[sorted.len() / 2];
            (name, total, median, samples.len())
        })
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let grand_total: u64 = rows.iter().map(|(_, t, _, _)| t).sum();
    let mut out = String::new();
    use std::fmt::Write;
    let _ = writeln!(out, "");
    let _ = writeln!(out, "===== PerfTimer aggregated summary =====");
    let _ = writeln!(out, "{:<20} {:>12} {:>12} {:>8} {:>7}",
        "pass", "total", "median", "count", "share");
    let _ = writeln!(out, "{}", "-".repeat(62));
    for (name, total, median, count) in &rows {
        let share = if grand_total > 0 {
            (*total as f64 / grand_total as f64) * 100.0
        } else { 0.0 };
        let _ = writeln!(out, "{:<20} {:>12} {:>12} {:>8} {:>6.1}%",
            name, fmt_ns(*total as f64), fmt_ns(*median as f64), count, share);
    }
    let _ = writeln!(out, "{}", "-".repeat(62));
    let _ = writeln!(out, "{:<20} {:>12}", "grand total",
        fmt_ns(grand_total as f64));
    out
}

fn fmt_ns(ns: f64) -> String {
    if ns < 1e3 { format!("{:.0} ns", ns) }
    else if ns < 1e6 { format!("{:.1} µs", ns / 1e3) }
    else if ns < 1e9 { format!("{:.1} ms", ns / 1e6) }
    else { format!("{:.2} s", ns / 1e9) }
}

impl Drop for PerfTimer {
    fn drop(&mut self) {
        if !self.stopped {
            self.emit();
        }
    }
}

/// Probe env once at startup (cached via OnceLock thread-safely).
fn is_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("NOVA_PERF_TIMER")
            .map(|v| v == "1" || v == "true" || v == "yes")
            .unwrap_or(false)
    })
}

/// Parse `__PERF__ <pass> <ns>` line — used by CLI bench corpus consumer.
/// Returns `Some((pass, ns))` если строка matches; иначе None.
pub fn parse_perf_line(line: &str) -> Option<(String, u64)> {
    let rest = line.strip_prefix("__PERF__ ")?;
    let mut parts = rest.splitn(2, ' ');
    let pass = parts.next()?.to_string();
    let ns: u64 = parts.next()?.trim().parse().ok()?;
    Some((pass, ns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        assert_eq!(parse_perf_line("__PERF__ parse 1234"),
            Some(("parse".to_string(), 1234)));
        assert_eq!(parse_perf_line("__PERF__ type-check 999000"),
            Some(("type-check".to_string(), 999000)));
    }

    #[test]
    fn parse_rejects_non_perf() {
        assert!(parse_perf_line("Hello").is_none());
        assert!(parse_perf_line("__PERF__ ").is_none());
        assert!(parse_perf_line("__PERF__ pass not-a-number").is_none());
    }

    #[test]
    fn timer_no_panic_when_disabled() {
        // Default — env not set → no emission, no panic.
        let _t = PerfTimer::new("test_pass");
        // drop happens silently.
    }
}
