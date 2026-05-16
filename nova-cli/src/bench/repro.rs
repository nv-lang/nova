// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57 L10 — Reproducibility metadata + environment checks.
//!
//! Captured per-run: hostname, OS+kernel, cpu_model, cpu_count, governor,
//! turbo, gc_mode, compiler version, build mode, runner_id.
//!
//! Warnings (не errors) при:
//!   - governor != performance (Linux)
//!   - turbo enabled
//!   - debug build (Cargo profile != release)
//!   - background CPU% > 5%
//!   - thermal throttle
//!
//! Cross-platform best-effort: всё что не доступно — None или "unknown".

use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct ReproMeta {
    pub hostname: Option<String>,
    pub os: String,
    pub kernel: Option<String>,
    pub arch: String,
    pub cpu_model: Option<String>,
    pub cpu_count: usize,
    pub governor: Option<String>,
    pub turbo: Option<bool>,
    pub gc_mode: String,
    pub compiler: CompilerInfo,
    pub build_mode: String,
    pub runner_id: Option<String>,
    /// Unix timestamp seconds at run start.
    pub timestamp_unix: u64,
    /// Sampling parameters used.
    pub sampling: SamplingMeta,
    /// Plan 57.A.4: CPU temperature в °C (Linux /sys/class/thermal).
    /// None если не доступно (Windows/macOS чаще всего).
    pub cpu_temp_c: Option<f64>,
    /// Plan 57.A.4: background CPU load % at run start (0..100).
    pub background_cpu_load_pct: Option<f64>,
    /// Plan 57.A.4: process nice value (Linux).
    pub process_nice: Option<i32>,
    /// Plan 57.A.4: CPU pinning affinity mask (count of cores process can use).
    pub cpu_affinity_count: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct CompilerInfo {
    pub nova_sha: Option<String>,
    pub nova_version: String,
    pub c_compiler: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SamplingMeta {
    pub warmup_ns: u64,
    pub target_ns: u64,
    pub samples: u64,
    pub time_budget_ns: u64,
}

/// Build a fresh ReproMeta from current environment. Best-effort detection.
pub fn collect(gc_mode: &str, sampling: SamplingMeta) -> ReproMeta {
    ReproMeta {
        hostname: detect_hostname(),
        os: detect_os(),
        kernel: detect_kernel(),
        arch: detect_arch(),
        cpu_model: detect_cpu_model(),
        cpu_count: num_cpus(),
        governor: detect_governor(),
        turbo: detect_turbo(),
        gc_mode: gc_mode.to_string(),
        compiler: detect_compiler(),
        build_mode: detect_build_mode(),
        runner_id: std::env::var("NOVA_BENCH_RUNNER_ID").ok(),
        timestamp_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        sampling,
        cpu_temp_c: detect_cpu_temp(),
        background_cpu_load_pct: detect_background_load(),
        process_nice: detect_nice(),
        cpu_affinity_count: detect_affinity_count(),
    }
}

impl ReproMeta {
    pub fn to_json(&self) -> Value {
        json!({
            "hostname": self.hostname,
            "os": self.os,
            "kernel": self.kernel,
            "arch": self.arch,
            "cpu_model": self.cpu_model,
            "cpu_count": self.cpu_count,
            "governor": self.governor,
            "turbo": self.turbo,
            "gc_mode": self.gc_mode,
            "compiler": {
                "nova_sha": self.compiler.nova_sha,
                "nova_version": self.compiler.nova_version,
                "c_compiler": self.compiler.c_compiler,
            },
            "build_mode": self.build_mode,
            "runner_id": self.runner_id,
            "timestamp_unix": self.timestamp_unix,
            "sampling": {
                "warmup_ns": self.sampling.warmup_ns,
                "target_ns": self.sampling.target_ns,
                "samples": self.sampling.samples,
                "time_budget_ns": self.sampling.time_budget_ns,
            },
            "cpu_temp_c": self.cpu_temp_c,
            "background_cpu_load_pct": self.background_cpu_load_pct,
            "process_nice": self.process_nice,
            "cpu_affinity_count": self.cpu_affinity_count,
        })
    }

    pub fn from_json(v: &Value) -> Result<Self, String> {
        let get_str = |k: &str| -> Option<String> {
            v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string())
        };
        let get_bool = |k: &str| -> Option<bool> {
            v.get(k).and_then(|x| x.as_bool())
        };
        let compiler_v = v.get("compiler").cloned().unwrap_or(json!({}));
        let sampling_v = v.get("sampling").cloned().unwrap_or(json!({}));
        Ok(Self {
            hostname: get_str("hostname"),
            os: get_str("os").unwrap_or_else(|| "unknown".to_string()),
            kernel: get_str("kernel"),
            arch: get_str("arch").unwrap_or_else(|| "unknown".to_string()),
            cpu_model: get_str("cpu_model"),
            cpu_count: v.get("cpu_count").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
            governor: get_str("governor"),
            turbo: get_bool("turbo"),
            gc_mode: get_str("gc_mode").unwrap_or_else(|| "unknown".to_string()),
            compiler: CompilerInfo {
                nova_sha: compiler_v.get("nova_sha").and_then(|x| x.as_str()).map(|s| s.to_string()),
                nova_version: compiler_v.get("nova_version").and_then(|x| x.as_str()).unwrap_or("unknown").to_string(),
                c_compiler: compiler_v.get("c_compiler").and_then(|x| x.as_str()).map(|s| s.to_string()),
            },
            build_mode: get_str("build_mode").unwrap_or_else(|| "unknown".to_string()),
            runner_id: get_str("runner_id"),
            timestamp_unix: v.get("timestamp_unix").and_then(|x| x.as_u64()).unwrap_or(0),
            sampling: SamplingMeta {
                warmup_ns: sampling_v.get("warmup_ns").and_then(|x| x.as_u64()).unwrap_or(0),
                target_ns: sampling_v.get("target_ns").and_then(|x| x.as_u64()).unwrap_or(0),
                samples: sampling_v.get("samples").and_then(|x| x.as_u64()).unwrap_or(0),
                time_budget_ns: sampling_v.get("time_budget_ns").and_then(|x| x.as_u64()).unwrap_or(0),
            },
            cpu_temp_c: v.get("cpu_temp_c").and_then(|x| x.as_f64()),
            background_cpu_load_pct: v.get("background_cpu_load_pct").and_then(|x| x.as_f64()),
            process_nice: v.get("process_nice").and_then(|x| x.as_i64()).map(|x| x as i32),
            cpu_affinity_count: v.get("cpu_affinity_count").and_then(|x| x.as_u64()).map(|x| x as usize),
        })
    }

    /// Verify two metadata blocks are comparable (CPU model + OS + arch).
    /// Returns vec of warnings; non-empty if mismatched.
    pub fn compare_compatibility(&self, other: &Self) -> Vec<String> {
        let mut w = Vec::new();
        if self.arch != other.arch {
            w.push(format!("arch mismatch: {} vs {}", self.arch, other.arch));
        }
        if self.cpu_model.as_deref() != other.cpu_model.as_deref() {
            w.push(format!("cpu model mismatch: {:?} vs {:?}",
                self.cpu_model, other.cpu_model));
        }
        if self.os != other.os {
            w.push(format!("OS mismatch: {} vs {}", self.os, other.os));
        }
        if self.compiler.c_compiler != other.compiler.c_compiler {
            w.push(format!("C compiler mismatch: {:?} vs {:?}",
                self.compiler.c_compiler, other.compiler.c_compiler));
        }
        if self.gc_mode != other.gc_mode {
            w.push(format!("GC mode mismatch: {} vs {}", self.gc_mode, other.gc_mode));
        }
        w
    }

    /// Detect env-level issues that affect measurement noise.
    /// Returns vec of (severity, message). severity: "critical" / "warn" / "info".
    pub fn env_warnings(&self) -> Vec<(String, String)> {
        let mut w = Vec::new();
        if self.build_mode == "debug" {
            w.push(("critical".into(),
                "debug build detected — bench results will be 5-20× slower and noisy. \
                 Use --release.".into()));
        }
        if let Some(gov) = &self.governor {
            if gov != "performance" {
                w.push(("warn".into(),
                    format!("CPU governor is '{}' — expect ±10-15% noise. \
                             To fix: sudo cpupower frequency-set -g performance", gov)));
            }
        }
        if self.turbo == Some(true) {
            w.push(("info".into(),
                "Turbo Boost enabled — variable clock may add ±5% noise. \
                 For deterministic benches, disable in BIOS.".into()));
        }
        // Plan 57.A.4: thermal throttle warning.
        if let Some(temp) = self.cpu_temp_c {
            if temp >= 90.0 {
                w.push(("critical".into(),
                    format!("CPU temperature {:.1}°C — likely thermal throttling. \
                             Cool system before benchmarking.", temp)));
            } else if temp >= 80.0 {
                w.push(("warn".into(),
                    format!("CPU temperature {:.1}°C — approaching throttle threshold. \
                             Expect degrading samples.", temp)));
            }
        }
        // Plan 57.A.4: background load warning.
        if let Some(load) = self.background_cpu_load_pct {
            if load > 30.0 {
                w.push(("critical".into(),
                    format!("background CPU load {:.0}% — close other processes \
                             before benchmarking (>± 20-30% noise expected).", load)));
            } else if load > 10.0 {
                w.push(("warn".into(),
                    format!("background CPU load {:.0}% — may add ±10% noise.", load)));
            }
        }
        // Plan 57.A.4: low priority warning (Linux).
        if let Some(nice) = self.process_nice {
            if nice > 0 {
                w.push(("info".into(),
                    format!("process nice value {} (low priority) — \
                             may add scheduling jitter. Use `nice -n -20 nova bench ...`.", nice)));
            }
        }
        // Plan 57.A.4: CPU pinning recommendation.
        if let Some(cnt) = self.cpu_affinity_count {
            if cnt > 1 && cnt == self.cpu_count {
                w.push(("info".into(),
                    "no CPU affinity set — process can migrate across cores \
                     (cache misses). Use `taskset -c N nova bench ...` для pinning.".into()));
            }
        }
        w
    }
}

// ── Detection functions ──────────────────────────────────────────────────

fn detect_hostname() -> Option<String> {
    std::env::var("COMPUTERNAME")  // Windows
        .or_else(|_| std::env::var("HOSTNAME"))  // Linux/macOS
        .ok()
        .or_else(|| {
            // Fallback: read /proc/sys/kernel/hostname (Linux).
            std::fs::read_to_string("/proc/sys/kernel/hostname")
                .ok()
                .map(|s| s.trim().to_string())
        })
}

fn detect_os() -> String {
    std::env::consts::OS.to_string()
}

fn detect_arch() -> String {
    std::env::consts::ARCH.to_string()
}

fn detect_kernel() -> Option<String> {
    // Linux: /proc/sys/kernel/osrelease.
    if let Ok(k) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
        return Some(k.trim().to_string());
    }
    // macOS: try `uname -r` via std::process::Command — avoid for now (deps).
    // Windows: ver — same.
    None
}

fn detect_cpu_model() -> Option<String> {
    // Linux: /proc/cpuinfo first "model name" line.
    if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("model name") {
                if let Some(colon_pos) = rest.find(':') {
                    return Some(rest[colon_pos + 1..].trim().to_string());
                }
            }
        }
    }
    // Windows: PROCESSOR_IDENTIFIER env. Не самое детализированное, но что есть.
    if let Ok(p) = std::env::var("PROCESSOR_IDENTIFIER") {
        return Some(p);
    }
    None
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn detect_governor() -> Option<String> {
    std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .ok()
        .map(|s| s.trim().to_string())
}

fn detect_turbo() -> Option<bool> {
    // Linux: /sys/devices/system/cpu/intel_pstate/no_turbo (1 = disabled, 0 = enabled).
    if let Ok(s) = std::fs::read_to_string("/sys/devices/system/cpu/intel_pstate/no_turbo") {
        return Some(s.trim() == "0");
    }
    // AMD / non-pstate: hard to detect cross-platform; return None.
    None
}

fn detect_compiler() -> CompilerInfo {
    CompilerInfo {
        nova_sha: option_env!("NOVA_SHA").map(|s| s.to_string()),
        nova_version: env!("CARGO_PKG_VERSION").to_string(),
        // C compiler detected at run time by examining what `nova test`
        // would use — для bench запуска подставляем то же. Stub for now:
        // CLI знает Toolchain через nova_codegen, и подставит реальное.
        c_compiler: std::env::var("NOVA_C_COMPILER").ok(),
    }
}

fn detect_build_mode() -> String {
    if cfg!(debug_assertions) {
        "debug".to_string()
    } else {
        "release".to_string()
    }
}

// ── Plan 57.A.4: thermal / load / priority / affinity detection ────────

/// Read first available CPU temperature from `/sys/class/thermal/thermal_zone*/temp`.
/// Returns °C. Linux-only.
fn detect_cpu_temp() -> Option<f64> {
    for i in 0..16 {
        let path = format!("/sys/class/thermal/thermal_zone{}/temp", i);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let trimmed = content.trim();
            if let Ok(millideg) = trimmed.parse::<i64>() {
                // Convention: millidegrees Celsius.
                return Some(millideg as f64 / 1000.0);
            }
        }
    }
    None
}

/// Sample CPU load by reading /proc/stat twice (100ms apart).
/// Returns 0..100 — percentage of CPU NOT idle. Linux-only.
fn detect_background_load() -> Option<f64> {
    fn read_proc_stat() -> Option<(u64, u64)> {
        let s = std::fs::read_to_string("/proc/stat").ok()?;
        let line = s.lines().next()?;
        if !line.starts_with("cpu ") { return None; }
        let parts: Vec<u64> = line.split_whitespace()
            .skip(1).take(8)
            .filter_map(|x| x.parse().ok())
            .collect();
        if parts.len() < 4 { return None; }
        // total = user + nice + system + idle + iowait + irq + softirq + steal
        let total: u64 = parts.iter().sum();
        let idle = parts[3] + parts.get(4).copied().unwrap_or(0);  // idle + iowait
        Some((total, idle))
    }
    let (t1, i1) = read_proc_stat()?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    let (t2, i2) = read_proc_stat()?;
    if t2 <= t1 { return None; }
    let dt = (t2 - t1) as f64;
    let di = (i2.saturating_sub(i1)) as f64;
    Some(((dt - di) / dt) * 100.0)
}

/// Process nice value (Linux). Negative = high priority, positive = low.
fn detect_nice() -> Option<i32> {
    // Read from /proc/self/stat field 19 (nice).
    let s = std::fs::read_to_string("/proc/self/stat").ok()?;
    // Skip pid + (comm) — comm может содержать пробелы и скобки.
    let close_paren = s.rfind(')')?;
    let rest = &s[close_paren + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After ')', field 17 = priority, field 18 = nice (1-indexed from rest's start).
    // 0=state, ... 16=priority, 17=nice.
    fields.get(16).and_then(|x| x.parse().ok())
}

/// Number of CPUs process can run on (affinity mask). Linux-only via
/// /proc/self/status `Cpus_allowed_list`.
fn detect_affinity_count() -> Option<usize> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("Cpus_allowed_list:") {
            let mut count = 0;
            for range in rest.trim().split(',') {
                if let Some((a, b)) = range.split_once('-') {
                    let lo: usize = a.trim().parse().ok()?;
                    let hi: usize = b.trim().parse().ok()?;
                    count += hi.saturating_sub(lo) + 1;
                } else if !range.trim().is_empty() {
                    if range.trim().parse::<usize>().is_ok() { count += 1; }
                }
            }
            return Some(count);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_returns_something() {
        let m = collect("malloc", SamplingMeta::default());
        assert!(!m.os.is_empty());
        assert!(!m.arch.is_empty());
        assert!(m.cpu_count >= 1);
    }

    #[test]
    fn json_roundtrip() {
        let m = collect("boehm", SamplingMeta {
            warmup_ns: 500_000_000,
            target_ns: 1_000_000,
            samples: 100,
            time_budget_ns: 10_000_000_000,
        });
        let j = m.to_json();
        let m2 = ReproMeta::from_json(&j).unwrap();
        assert_eq!(m.os, m2.os);
        assert_eq!(m.gc_mode, m2.gc_mode);
        assert_eq!(m.sampling.samples, m2.sampling.samples);
    }

    #[test]
    fn debug_warning_emitted() {
        let mut m = ReproMeta::default();
        m.build_mode = "debug".to_string();
        let w = m.env_warnings();
        assert!(w.iter().any(|(s, _)| s == "critical"));
    }

    #[test]
    fn governor_warning_emitted() {
        let mut m = ReproMeta::default();
        m.build_mode = "release".to_string();
        m.governor = Some("powersave".to_string());
        let w = m.env_warnings();
        assert!(w.iter().any(|(s, msg)| s == "warn" && msg.contains("powersave")));
    }
}
