// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57 L4 — Output formatters.
//!
//! Terminal table (default), markdown (PR comment), CSV (spreadsheet),
//! JSON v1 (через schema.rs).

use std::fmt::Write;

use super::schema::AnalyzedBench;
use super::repro::ReproMeta;

/// Human-readable duration formatter — ns/µs/ms/s.
pub fn fmt_duration(ns: f64) -> String {
    if ns < 1_000.0 {
        format!("{:.2} ns", ns)
    } else if ns < 1_000_000.0 {
        format!("{:.2} µs", ns / 1_000.0)
    } else if ns < 1_000_000_000.0 {
        format!("{:.2} ms", ns / 1_000_000.0)
    } else {
        format!("{:.2} s", ns / 1_000_000_000.0)
    }
}

pub fn fmt_throughput_bytes(bps: f64) -> String {
    if bps < 1e3 { format!("{:.1} B/s", bps) }
    else if bps < 1e6 { format!("{:.1} KB/s", bps / 1e3) }
    else if bps < 1e9 { format!("{:.1} MB/s", bps / 1e6) }
    else { format!("{:.1} GB/s", bps / 1e9) }
}

pub fn fmt_throughput_elem(eps: f64) -> String {
    if eps < 1e3 { format!("{:.1} elem/s", eps) }
    else if eps < 1e6 { format!("{:.1} Kelem/s", eps / 1e3) }
    else if eps < 1e9 { format!("{:.1} Melem/s", eps / 1e6) }
    else { format!("{:.1} Gelem/s", eps / 1e9) }
}

/// Plan 57.G.4 — ASCII histogram of raw_ns sample distribution.
/// 20 buckets, normalized к max-count via Unicode block chars `▁▂▃▄▅▆▇█`.
/// Median + Tukey fences marked via column annotations.
///
/// Полезно for quick distribution shape inspection without HTML dashboard
/// (mitata / hyperfine style).
pub fn ascii_histogram(b: &AnalyzedBench, width: usize) -> String {
    let st = &b.stats_ns;
    let raw = &b.raw.raw_ns;
    if raw.is_empty() {
        return String::from("  (no samples)\n");
    }
    let n_bins = width.max(8);
    let min_v = *raw.iter().min().expect("invariant: non-empty checked above") as f64;
    let max_v = *raw.iter().max().expect("invariant: non-empty checked above") as f64;
    let bin_width = if max_v > min_v { (max_v - min_v) / n_bins as f64 } else { 1.0 };
    let mut counts = vec![0usize; n_bins];
    for &v in raw {
        let v_f = v as f64;
        let idx = if bin_width > 0.0 {
            (((v_f - min_v) / bin_width) as usize).min(n_bins - 1)
        } else { 0 };
        counts[idx] += 1;
    }
    let max_count = *counts.iter().max().unwrap_or(&1).max(&1);
    // Unicode 1/8th blocks for 8-level resolution.
    const BLOCKS: &[char] = &['\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}',
                              '\u{2585}', '\u{2586}', '\u{2587}', '\u{2588}'];
    let mut bars = String::with_capacity(n_bins * 4);
    for &c in &counts {
        if c == 0 { bars.push(' '); continue; }
        // Map count к [0..8) — saturate top.
        let frac = (c as f64) / (max_count as f64);
        let lv = ((frac * BLOCKS.len() as f64) as usize)
            .min(BLOCKS.len() - 1);
        bars.push(BLOCKS[lv]);
    }
    // Index columns для median + Tukey fences.
    let bin_of = |v: f64| -> usize {
        if bin_width > 0.0 {
            (((v - min_v) / bin_width) as usize).min(n_bins - 1)
        } else { 0 }
    };
    let mut markers = vec![' '; n_bins];
    let lo_fence = st.p25 - 1.5 * st.iqr;
    let hi_fence = st.p75 + 1.5 * st.iqr;
    if lo_fence >= min_v { markers[bin_of(lo_fence)] = '['; }
    if hi_fence <= max_v { markers[bin_of(hi_fence)] = ']'; }
    markers[bin_of(st.median)] = 'M';

    let mut out = String::new();
    let _ = writeln!(out, "  histogram ({} buckets, max count = {}):", n_bins, max_count);
    let _ = writeln!(out, "    {}", bars);
    let _ = writeln!(out, "    {}", markers.iter().collect::<String>());
    let _ = writeln!(out, "    {} … {}  (M=median, [ ]=Tukey fences)",
        fmt_duration(min_v), fmt_duration(max_v));
    out
}

/// Terminal table — coloured if `color` is true.
/// Uses simple ASCII border characters; respects color through ANSI escape.
pub fn terminal_report(meta: &ReproMeta, benches: &[AnalyzedBench], color: bool) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Nova bench results — {} on {} ({} cores)",
        meta.timestamp_iso8601(),
        meta.cpu_model.as_deref().unwrap_or("unknown CPU"),
        meta.cpu_count);
    let _ = writeln!(out, "  OS: {} {} / arch: {} / GC: {} / build: {}",
        meta.os,
        meta.kernel.as_deref().unwrap_or(""),
        meta.arch,
        meta.gc_mode,
        meta.build_mode);
    if let Some(gov) = &meta.governor {
        let _ = writeln!(out, "  Governor: {}{}",
            gov,
            if meta.turbo == Some(true) { ", turbo: on" } else { "" });
    }
    // Plan 57.A.4: показываем доп. metrics если есть.
    let mut extras: Vec<String> = Vec::new();
    if let Some(t) = meta.cpu_temp_c {
        extras.push(format!("CPU temp: {:.1}°C", t));
    }
    if let Some(l) = meta.background_cpu_load_pct {
        extras.push(format!("background load: {:.0}%", l));
    }
    if let Some(a) = meta.cpu_affinity_count {
        extras.push(format!("affinity: {} cores", a));
    }
    if !extras.is_empty() {
        let _ = writeln!(out, "  {}", extras.join("  "));
    }
    let _ = writeln!(out, "");

    // Environment warnings.
    for (sev, msg) in meta.env_warnings() {
        let (prefix, reset) = if color {
            match sev.as_str() {
                "critical" => ("\x1b[1;31m✘ ", "\x1b[0m"),
                "warn"     => ("\x1b[1;33m⚠ ", "\x1b[0m"),
                _          => ("\x1b[1;36mℹ ", "\x1b[0m"),
            }
        } else {
            match sev.as_str() {
                "critical" => ("[CRIT] ", ""),
                "warn"     => ("[WARN] ", ""),
                _          => ("[INFO] ", ""),
            }
        };
        let _ = writeln!(out, "{}{}{}", prefix, msg, reset);
    }
    if !meta.env_warnings().is_empty() {
        let _ = writeln!(out, "");
    }

    if benches.is_empty() {
        let _ = writeln!(out, "(no benches collected)");
        return out;
    }

    // Per-bench block.
    for b in benches {
        let st = &b.stats_ns;
        let bold = if color { "\x1b[1m" } else { "" };
        let reset = if color { "\x1b[0m" } else { "" };
        let _ = writeln!(out, "{}{}{}", bold, b.raw.name, reset);
        let _ = writeln!(out, "  median:    {}  (± {} MAD)",
            fmt_duration(st.median), fmt_duration(st.mad));
        let _ = writeln!(out, "  mean:      {}  (± {} σ)",
            fmt_duration(st.mean), fmt_duration(st.stddev));
        let outl = st.outliers_low + st.outliers_high;
        let outl_part = if outl > 0 { format!("  [{} outliers]", outl) } else { String::new() };
        let _ = writeln!(out, "  range:     {} … {}{}",
            fmt_duration(st.min), fmt_duration(st.max), outl_part);
        let _ = writeln!(out, "  ci95:      {} … {}",
            fmt_duration(st.ci95_lo), fmt_duration(st.ci95_hi));
        let _ = writeln!(out, "  samples:   n={}, iters_per_sample={}",
            st.n, b.raw.iters_per_sample);
        if let Some(bps) = b.throughput_bytes_per_sec() {
            let _ = writeln!(out, "  throughput:{}", fmt_throughput_bytes(bps));
        }
        if let Some(eps) = b.throughput_elements_per_sec() {
            let _ = writeln!(out, "  elements:  {}", fmt_throughput_elem(eps));
        }
        if let Some(api) = b.raw.allocs_per_iter {
            let total = b.raw.allocs_total.unwrap_or(0);
            let _ = writeln!(out, "  allocs:    {}/iter ({} total)", api, total);
        }
        let _ = writeln!(out, "");
    }
    out
}

/// CSV output — one row per bench, columns identical к JSON stats.
pub fn csv_report(benches: &[AnalyzedBench]) -> String {
    let mut out = String::new();
    out.push_str("name,n,median_ns,mad_ns,mean_ns,stddev_ns,p25_ns,p75_ns,min_ns,max_ns,\
                  ci95_lo_ns,ci95_hi_ns,outliers,iters_per_sample,\
                  throughput_bytes,throughput_elements,allocs_per_iter\n");
    for b in benches {
        let st = &b.stats_ns;
        let _ = writeln!(out,
            "{},{},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0},{},{},{},{},{}",
            csv_escape(&b.raw.name),
            st.n,
            st.median, st.mad, st.mean, st.stddev,
            st.p25, st.p75, st.min, st.max,
            st.ci95_lo, st.ci95_hi,
            st.outliers_low + st.outliers_high,
            b.raw.iters_per_sample,
            b.raw.throughput_bytes.map(|x| x.to_string()).unwrap_or_default(),
            b.raw.throughput_elements.map(|x| x.to_string()).unwrap_or_default(),
            b.raw.allocs_per_iter.map(|x| x.to_string()).unwrap_or_default(),
        );
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

impl ReproMeta {
    pub fn timestamp_iso8601(&self) -> String {
        // Без chrono — пишем минимально. Format YYYY-MM-DDTHH:MM:SSZ.
        let t = self.timestamp_unix;
        let (year, mon, day, hour, min, sec) = unix_to_ymdhms(t);
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, mon, day, hour, min, sec)
    }
}

/// Convert unix seconds to (year, month, day, hour, min, sec). Public domain
/// algorithm (Howard Hinnant).
fn unix_to_ymdhms(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let z = (secs / 86400) as i64;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let z_adj = z + 719468;
    let era = if z_adj >= 0 { z_adj } else { z_adj - 146096 } / 146097;
    let doe = (z_adj - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m_calendar = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m_calendar <= 2 { y + 1 } else { y } as i32;
    (year, m_calendar as u32, d as u32, h as u32, m as u32, s as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_duration_scales() {
        assert!(fmt_duration(500.0).ends_with("ns"));
        assert!(fmt_duration(5_000.0).ends_with("µs"));
        assert!(fmt_duration(5_000_000.0).ends_with("ms"));
        assert!(fmt_duration(5_000_000_000.0).ends_with("s"));
    }

    #[test]
    fn fmt_throughput() {
        assert!(fmt_throughput_bytes(500.0).ends_with("B/s"));
        assert!(fmt_throughput_bytes(5e3).ends_with("KB/s"));
        assert!(fmt_throughput_bytes(5e6).ends_with("MB/s"));
        assert!(fmt_throughput_bytes(5e9).ends_with("GB/s"));
    }

    #[test]
    fn csv_escape_quotes() {
        assert_eq!(csv_escape("a,b"), r#""a,b""#);
        assert_eq!(csv_escape("a\"b"), r#""a""b""#);
        assert_eq!(csv_escape("simple"), "simple");
    }

    #[test]
    fn iso8601_format() {
        // Epoch + 0 = 1970-01-01.
        let m = ReproMeta { timestamp_unix: 0, ..Default::default() };
        let s = m.timestamp_iso8601();
        assert_eq!(s, "1970-01-01T00:00:00Z");
        // Add 1 hour 2 min 3 sec → format check.
        let m = ReproMeta { timestamp_unix: 3723, ..Default::default() };
        assert_eq!(m.timestamp_iso8601(), "1970-01-01T01:02:03Z");
    }
}
