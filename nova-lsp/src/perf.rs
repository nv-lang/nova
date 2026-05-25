//! Performance measurement utilities for nova-lsp.
//!
//! Plan 104.1.Ф.7: `measure!` macro + NOVA_LSP_LOG-controlled tracing spans.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::perf::measure;
//! let result = measure!("check_workspace", check_workspace(&root));
//! ```
//!
//! The macro emits an `info`-level tracing span with duration in milliseconds.
//! Set `NOVA_LSP_LOG=nova_lsp=debug` to see per-call timings in the editor's
//! output panel.

use std::time::Instant;

/// Measure the wall-clock duration of an expression and log it.
///
/// Emits: `INFO nova_lsp::perf: <name> took <N>ms`.
#[macro_export]
macro_rules! measure {
    ($name:expr, $expr:expr) => {{
        let _start = ::std::time::Instant::now();
        let _result = $expr;
        let _elapsed = _start.elapsed();
        tracing::debug!(
            name = $name,
            elapsed_ms = _elapsed.as_millis(),
            "perf: {} took {}ms",
            $name,
            _elapsed.as_millis()
        );
        _result
    }};
}

pub use measure;

/// A simple wall-clock timer for use in non-macro contexts.
///
/// ```rust,ignore
/// let t = PerfTimer::start("label");
/// // … work …
/// t.finish(); // logs elapsed
/// ```
pub struct PerfTimer {
    label: &'static str,
    start: Instant,
}

impl PerfTimer {
    pub fn start(label: &'static str) -> Self {
        Self { label, start: Instant::now() }
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    pub fn finish(self) -> u128 {
        let ms = self.elapsed_ms();
        tracing::debug!(
            label = self.label,
            elapsed_ms = ms,
            "perf: {} took {}ms", self.label, ms
        );
        ms
    }
}
