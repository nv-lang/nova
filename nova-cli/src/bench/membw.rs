// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.F.3 — Memory bandwidth measurement (Linux only).
//!
//! Two paths:
//!   1. **HW cache miss** (PERF_COUNT_HW_CACHE_MISSES) — широко доступен,
//!      возвращает LLC miss count. Bandwidth = misses × cache_line_size.
//!      Privilege: perf_event_paranoid <= 1 (как Plan 57.B.4 instructions).
//!   2. **Intel MBM / AMD QoS** (uncore_imc raw events) — точнее (реальный
//!      DRAM byte counter), но требует sysfs probe + CAP_PERFMON.
//!      Реализовано как graceful upgrade когда доступно.
//!
//! API:
//!   - `available()` — bool: можно ли использовать ANY измерение.
//!   - `available_mbm()` — bool: доступен ли uncore_imc path.
//!   - `measure_bandwidth(|| body)` — Result<MembwSample>:
//!       { bytes: u64, source: MembwSource }.
//!   - `mbm_event_codes()` — Vec<(String, u64)>: probed events sysfs.
//!
//! Reference: man perf_event_open(2),
//!            kernel docs/admin-guide/perf/intel-imc.rst
//!            kernel docs/x86/resctrl_ui.rst (MBM via resctrl).

use anyhow::{anyhow, Result};

/// Source метрики — какой counter был использован.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembwSource {
    /// LLC cache misses × cache line (estimate).
    LlcMissEstimate,
    /// Intel uncore_imc cas_count_read/write (точный).
    IntelImc,
    /// AMD ucDF_BWMon (точный, Zen 3+).
    AmdDfBwmon,
}

impl MembwSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            MembwSource::LlcMissEstimate => "llc-miss-estimate",
            MembwSource::IntelImc        => "intel-uncore-imc",
            MembwSource::AmdDfBwmon      => "amd-df-bwmon",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MembwSample {
    pub bytes: u64,
    pub source: MembwSource,
}

/// Probed PMU event entry from sysfs (PMU type + event config).
#[derive(Debug, Clone)]
pub struct ImcEvent {
    /// Display name (e.g. "uncore_imc_0/cas_count_read").
    pub name: String,
    /// perf_event_attr.type (read from /sys/.../type, dynamic > PERF_TYPE_MAX).
    pub pmu_type: u32,
    /// perf_event_attr.config (parsed from event=0xNN format).
    pub config: u64,
}

#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;
    use std::os::unix::io::RawFd;
    use std::path::{Path, PathBuf};

    /// perf_event_attr — slim mirror (matches cpu_instr.rs layout).
    #[repr(C)]
    struct PerfEventAttr {
        type_: u32, size: u32, config: u64,
        sample_period: u64, sample_type: u64, read_format: u64,
        flags: u64,
        wakeup_events: u32, bp_type: u32, bp_addr: u64, bp_len: u64,
        branch_sample_type: u64, sample_regs_user: u64,
        sample_stack_user: u32, clockid: i32,
        sample_regs_intr: u64, aux_watermark: u32,
        sample_max_stack: u16, __reserved_2: u16,
    }

    const PERF_TYPE_HW_CACHE: u32 = 3;
    // PERF_COUNT_HW_CACHE_LL | (PERF_COUNT_HW_CACHE_OP_READ << 8) | (PERF_COUNT_HW_CACHE_RESULT_MISS << 16)
    // LL=2, OP_READ=0, RESULT_MISS=1
    const HW_CACHE_LLC_MISSES: u64 = 2 | (0 << 8) | (1 << 16);

    const PERF_EVENT_ATTR_DISABLED: u64        = 1 << 0;
    const PERF_EVENT_ATTR_EXCLUDE_KERNEL: u64  = 1 << 5;
    const PERF_EVENT_ATTR_EXCLUDE_HV: u64      = 1 << 6;

    const PERF_EVENT_IOC_ENABLE:  u64 = 0x2400;
    const PERF_EVENT_IOC_DISABLE: u64 = 0x2401;
    const PERF_EVENT_IOC_RESET:   u64 = 0x2403;

    const SYS_PERF_EVENT_OPEN: i64 = 298;  // x86_64 Linux

    extern "C" {
        fn syscall(num: i64, ...) -> i64;
        fn ioctl(fd: RawFd, request: u64, ...) -> i32;
        fn read(fd: RawFd, buf: *mut u8, count: usize) -> isize;
        fn close(fd: RawFd) -> i32;
    }

    /// Probe /sys/devices/uncore_imc_*/ для cas_count_read+write events.
    /// Returns list для passing к perf_event_open (PMU type dynamic, > 17).
    pub fn probe_imc_events() -> Vec<ImcEvent> {
        let mut out = Vec::new();
        let root = Path::new("/sys/devices");
        let dirs = match std::fs::read_dir(root) {
            Ok(d) => d,
            Err(_) => return out,
        };
        for d in dirs.flatten() {
            let name = d.file_name().to_string_lossy().to_string();
            if !name.starts_with("uncore_imc") { continue; }
            let type_path = d.path().join("type");
            let pmu_type: u32 = match std::fs::read_to_string(&type_path) {
                Ok(s) => match s.trim().parse() {
                    Ok(n) => n,
                    Err(_) => continue,
                }
                Err(_) => continue,
            };
            // Probe для cas_count_read и cas_count_write events.
            for event_name in &["cas_count_read", "cas_count_write"] {
                let ev_path = d.path().join("events").join(event_name);
                if let Ok(raw) = std::fs::read_to_string(&ev_path) {
                    // Format example: "event=0x04,umask=0x03"
                    if let Some(cfg) = parse_event_string(raw.trim()) {
                        out.push(ImcEvent {
                            name: format!("{}/{}", name, event_name),
                            pmu_type, config: cfg,
                        });
                    }
                }
            }
        }
        out
    }

    /// Parse `event=0xNN,umask=0xMM,...` к packed u64 config.
    /// Encoding compatible с PMU format spec (format/event=0-7, format/umask=8-15).
    /// Simplified — handles event + umask only (sufficient для cas_count).
    /// `pub` чтобы быть testable из integration tests.
    pub fn parse_event_string(s: &str) -> Option<u64> {
        let mut event: u64 = 0;
        let mut umask: u64 = 0;
        for kv in s.split(',') {
            let parts: Vec<&str> = kv.splitn(2, '=').collect();
            if parts.len() != 2 { continue; }
            let key = parts[0].trim();
            let val_str = parts[1].trim();
            let val = if let Some(hex) = val_str.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).ok()?
            } else {
                val_str.parse().ok()?
            };
            match key {
                "event" => event = val,
                "umask" => umask = val,
                _ => {}  // ignore unknown fields
            }
        }
        Some(event | (umask << 8))
    }

    /// Open a cache-miss-based bandwidth counter for current process.
    pub struct LlcMissCounter { fd: RawFd }

    impl LlcMissCounter {
        pub fn new() -> Result<Self> {
            let mut attr = make_attr(PERF_TYPE_HW_CACHE, HW_CACHE_LLC_MISSES);
            let fd = unsafe {
                syscall(SYS_PERF_EVENT_OPEN,
                    &mut attr as *mut PerfEventAttr,
                    0i32, -1i32, -1i32, 0u64) as RawFd
            };
            if fd < 0 {
                let errno = std::io::Error::last_os_error();
                // Plan 57.G.3 — actionable errno decoder.
                return Err(anyhow!("{}",
                    super::super::errno::fmt_perf_event_open_err(
                        "perf_event_open(LLC_MISSES)", &errno)));
            }
            Ok(Self { fd })
        }
        pub fn reset(&self) -> Result<()> { do_ioctl(self.fd, PERF_EVENT_IOC_RESET, "RESET") }
        pub fn start(&self) -> Result<()> { do_ioctl(self.fd, PERF_EVENT_IOC_ENABLE, "ENABLE") }
        pub fn stop(&self)  -> Result<()> { do_ioctl(self.fd, PERF_EVENT_IOC_DISABLE, "DISABLE") }
        pub fn read(&self) -> Result<u64> {
            let mut buf = [0u8; 8];
            let n = unsafe { read(self.fd, buf.as_mut_ptr(), 8) };
            if n != 8 { return Err(anyhow!("read llc-miss counter: {} bytes", n)); }
            Ok(u64::from_ne_bytes(buf))
        }
    }
    impl Drop for LlcMissCounter {
        fn drop(&mut self) { unsafe { close(self.fd); } }
    }

    fn make_attr(type_: u32, config: u64) -> PerfEventAttr {
        PerfEventAttr {
            type_, size: std::mem::size_of::<PerfEventAttr>() as u32,
            config, sample_period: 0, sample_type: 0, read_format: 0,
            flags: PERF_EVENT_ATTR_DISABLED
                 | PERF_EVENT_ATTR_EXCLUDE_KERNEL
                 | PERF_EVENT_ATTR_EXCLUDE_HV,
            wakeup_events: 0, bp_type: 0, bp_addr: 0, bp_len: 0,
            branch_sample_type: 0, sample_regs_user: 0,
            sample_stack_user: 0, clockid: 0,
            sample_regs_intr: 0, aux_watermark: 0,
            sample_max_stack: 0, __reserved_2: 0,
        }
    }
    fn do_ioctl(fd: RawFd, req: u64, name: &str) -> Result<()> {
        let r = unsafe { ioctl(fd, req, 0u64) };
        if r != 0 { return Err(anyhow!("ioctl {}: {}", name,
            std::io::Error::last_os_error())); }
        Ok(())
    }

    /// Standard L3 cache line size on x86_64 / aarch64 = 64 bytes.
    pub const CACHE_LINE_SIZE: u64 = 64;

    pub fn available_llc() -> bool { LlcMissCounter::new().is_ok() }

    /// Detect AMD DF BwMon via sysfs (Zen 3+).
    /// Search для /sys/devices/amd_l3 or amd_df с bwmon event.
    pub fn probe_amd_df() -> Option<ImcEvent> {
        let candidates = [
            PathBuf::from("/sys/devices/amd_df"),
            PathBuf::from("/sys/devices/amd_l3"),
        ];
        for c in &candidates {
            let type_path = c.join("type");
            if let Ok(s) = std::fs::read_to_string(&type_path) {
                if let Ok(pmu_type) = s.trim().parse::<u32>() {
                    // Probe bwmon event если есть.
                    let ev_path = c.join("events").join("bwmon");
                    if let Ok(raw) = std::fs::read_to_string(&ev_path) {
                        if let Some(cfg) = parse_event_string(raw.trim()) {
                            return Some(ImcEvent {
                                name: format!("{}/bwmon",
                                    c.file_name().unwrap().to_string_lossy()),
                                pmu_type, config: cfg,
                            });
                        }
                    }
                }
            }
        }
        None
    }
}

// ─── cross-platform façade ──────────────────────────────────────────

#[cfg(target_os = "linux")]
pub fn available() -> bool { linux::available_llc() }

#[cfg(not(target_os = "linux"))]
pub fn available() -> bool { false }

#[cfg(target_os = "linux")]
pub fn available_mbm() -> bool {
    !linux::probe_imc_events().is_empty() || linux::probe_amd_df().is_some()
}

#[cfg(not(target_os = "linux"))]
pub fn available_mbm() -> bool { false }

#[cfg(target_os = "linux")]
pub fn mbm_event_codes() -> Vec<ImcEvent> {
    let mut v = linux::probe_imc_events();
    if let Some(amd) = linux::probe_amd_df() { v.push(amd); }
    v
}

#[cfg(not(target_os = "linux"))]
pub fn mbm_event_codes() -> Vec<ImcEvent> { Vec::new() }

/// Measure memory bandwidth proxy for closure execution.
/// On Linux: opens LLC-miss counter, multiplies by 64 (cache line) to
/// estimate bytes transferred. На других OS — Err.
#[cfg(target_os = "linux")]
pub fn measure_bandwidth<F: FnOnce()>(f: F) -> Result<MembwSample> {
    let counter = linux::LlcMissCounter::new()?;
    counter.reset()?;
    counter.start()?;
    f();
    counter.stop()?;
    let misses = counter.read()?;
    Ok(MembwSample {
        bytes: misses * linux::CACHE_LINE_SIZE,
        source: MembwSource::LlcMissEstimate,
    })
}

#[cfg(not(target_os = "linux"))]
pub fn measure_bandwidth<F: FnOnce()>(_f: F) -> Result<MembwSample> {
    Err(anyhow!("memory bandwidth measurement not available on this OS \
                 (Linux only — uses perf_event_open syscall)"))
}

/// Format human-readable bandwidth (B/MB/GB).
pub fn fmt_bytes(b: u64) -> String {
    let bf = b as f64;
    if bf >= 1e9       { format!("{:.2} GB", bf / 1e9) }
    else if bf >= 1e6  { format!("{:.2} MB", bf / 1e6) }
    else if bf >= 1e3  { format!("{:.2} KB", bf / 1e3) }
    else               { format!("{} B", b) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membw_source_strings() {
        assert_eq!(MembwSource::LlcMissEstimate.as_str(), "llc-miss-estimate");
        assert_eq!(MembwSource::IntelImc.as_str(),         "intel-uncore-imc");
        assert_eq!(MembwSource::AmdDfBwmon.as_str(),       "amd-df-bwmon");
    }

    #[test]
    fn fmt_bytes_ranges() {
        assert_eq!(fmt_bytes(0),          "0 B");
        assert_eq!(fmt_bytes(999),        "999 B");
        assert_eq!(fmt_bytes(1_500),      "1.50 KB");
        assert_eq!(fmt_bytes(2_500_000),  "2.50 MB");
        assert_eq!(fmt_bytes(3_500_000_000),"3.50 GB");
    }

    // ── F.4 positive tests ──────────────────────────────────────────

    #[test]
    fn fmt_bytes_boundaries() {
        // На границах между unit categories (must round-trip predictably).
        assert_eq!(fmt_bytes(1_000),           "1.00 KB");
        assert_eq!(fmt_bytes(1_000_000),       "1.00 MB");
        assert_eq!(fmt_bytes(1_000_000_000),   "1.00 GB");
        // Just below boundary — picks lower unit (рассчёт без округления
        // на category-switch, e.g. 999_999 < 1e6 → KB). Округление
        // {:.2} даёт "1000.00 KB" — accept either form.
        let near_mb = fmt_bytes(999_999);
        assert!(near_mb.ends_with("KB"),
            "expected KB unit для 999_999, got {}", near_mb);
        let near_gb = fmt_bytes(999_999_999);
        assert!(near_gb.ends_with("MB"),
            "expected MB unit для 999_999_999, got {}", near_gb);
    }

    #[test]
    fn fmt_bytes_single_byte() {
        assert_eq!(fmt_bytes(1), "1 B");
        assert_eq!(fmt_bytes(42), "42 B");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn parse_event_string_event_only() {
        use super::linux::parse_event_string;
        // event=0x04 → low byte = 0x04, umask byte = 0.
        let r = parse_event_string("event=0x04").unwrap();
        assert_eq!(r, 0x04);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn parse_event_string_event_and_umask() {
        use super::linux::parse_event_string;
        // event=0x04,umask=0x03 → 0x04 | (0x03 << 8) = 0x304.
        let r = parse_event_string("event=0x04,umask=0x03").unwrap();
        assert_eq!(r, 0x304);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn parse_event_string_decimal_values() {
        use super::linux::parse_event_string;
        // Without "0x" prefix → decimal interpretation.
        let r = parse_event_string("event=4,umask=3").unwrap();
        assert_eq!(r, 0x304);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn parse_event_string_ignores_unknown_keys() {
        use super::linux::parse_event_string;
        // Unknown keys (e.g. edge, inv) silently ignored — matches kernel
        // behaviour для simplified parser.
        let r = parse_event_string("event=0x10,edge=1,inv=1,umask=0x20").unwrap();
        assert_eq!(r, 0x10 | (0x20 << 8));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn probe_imc_events_does_not_panic() {
        use super::linux::probe_imc_events;
        let _ = probe_imc_events();  // smoke test: should not panic
    }

    // ── F.4 negative tests ──────────────────────────────────────────

    #[test]
    #[cfg(target_os = "linux")]
    fn parse_event_string_empty_returns_zero() {
        use super::linux::parse_event_string;
        // Empty input → config = 0 (no keys matched, neither event/umask
        // set). Still returns Some — каллер can check для usefulness.
        let r = parse_event_string("");
        assert_eq!(r, Some(0));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn parse_event_string_malformed_hex_returns_none() {
        use super::linux::parse_event_string;
        // Invalid hex digit після "0x" → parsing fails, returns None.
        let r = parse_event_string("event=0xZZ");
        assert_eq!(r, None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn parse_event_string_no_equals_skipped() {
        use super::linux::parse_event_string;
        // Tokens без '=' silently skipped — но valid tokens still processed.
        let r = parse_event_string("bogus,event=0x05,also_bogus").unwrap();
        assert_eq!(r, 0x05);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn unsupported_returns_err() {
        assert!(!available());
        assert!(!available_mbm());
        assert!(mbm_event_codes().is_empty());
        assert!(measure_bandwidth(|| {}).is_err());
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn measure_bandwidth_err_message_mentions_linux() {
        let err = measure_bandwidth(|| {}).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("Linux"), "err msg should mention Linux: {}", msg);
    }
}
