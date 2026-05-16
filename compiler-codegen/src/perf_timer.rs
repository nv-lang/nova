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
        if !is_enabled() { return; }
        let ns = self.start.elapsed().as_nanos();
        eprintln!("__PERF__ {} {}", self.name, ns);
    }
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
