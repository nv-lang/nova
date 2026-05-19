// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.A.5 — Profile mode integration.
//!
//! `nova bench run --profile cpu out.svg` wraps `samply` external tool
//! (https://github.com/mstange/samply) для CPU sampling profile.
//!
//! Дизайн:
//!   - samply ⇒ статический бинарь без deps, cross-platform.
//!   - Profile run отдельный от measurement run (instrumentation noise
//!     не влияет на baseline numbers).
//!   - Output: SVG flame graph (via inferno) или JSON (samply native).
//!
//! Fallback: если samply не найден в PATH — warning + skip.

use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileMode {
    /// CPU sampling — wraps samply.
    Cpu,
    /// Heap profile через periodic gc.heap_size() sampling (Plan 32).
    /// Реализация: отдельный thread sampler внутри bench process.
    Heap,
    /// GC pause histogram — собирает pauses через gc.gc_pauses() API.
    Gc,
}

impl ProfileMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "cpu"  => Ok(ProfileMode::Cpu),
            "heap" => Ok(ProfileMode::Heap),
            "gc"   => Ok(ProfileMode::Gc),
            _ => Err(anyhow!("unknown profile mode: {} (expected: cpu|heap|gc)", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileOpts<'a> {
    pub mode: ProfileMode,
    pub out: &'a Path,
    /// Bench exe path (output из compile).
    pub bench_exe: &'a Path,
    /// Optional bench filter env.
    pub filter: Option<&'a str>,
    /// Reduce sample count для profile run (don't need 100 samples here).
    pub samples_override: u64,
}

/// Check whether samply is available на PATH.
pub fn samply_available() -> bool {
    Command::new("samply")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run profile mode. Returns 0 on success, non-zero on error.
pub fn run(opts: ProfileOpts) -> Result<i32> {
    match opts.mode {
        ProfileMode::Cpu => run_cpu(opts),
        ProfileMode::Heap => run_heap(opts),
        ProfileMode::Gc => run_gc(opts),
    }
}

fn run_cpu(opts: ProfileOpts) -> Result<i32> {
    if !samply_available() {
        bail!("samply not found in PATH.\n\
               Install: `cargo install samply`\n\
               (https://github.com/mstange/samply)");
    }

    eprintln!("profile/cpu: invoking samply record → {}", opts.out.display());

    let mut cmd = Command::new("samply");
    cmd.arg("record")
        .arg("--save-only")
        .arg("--output").arg(opts.out)
        .arg("--");
    cmd.arg(opts.bench_exe);

    // Reduce bench iterations для profile (faster + less noise).
    cmd.env("NOVA_BENCH_SAMPLES", opts.samples_override.to_string());
    if let Some(f) = opts.filter {
        cmd.env("NOVA_BENCH_FILTER", f);
    }

    let status = cmd.status()
        .map_err(|e| anyhow!("spawn samply: {}", e))?;
    if !status.success() {
        bail!("samply record exited {:?}", status.code());
    }
    eprintln!("profile/cpu: profile saved.\n\
               Open: samply load {} (browser flame graph)",
        opts.out.display());
    Ok(0)
}

fn run_heap(opts: ProfileOpts) -> Result<i32> {
    // Подход: запускаем bench-exe с env NOVA_BENCH_HEAP_SAMPLE_MS=10
    // — runtime будет periodic sampling gc.heap_size() (Plan 32 bridge)
    // и emit'ить __HEAP_SAMPLE__ <ns> <bytes> на stderr.
    //
    // MVP реализация: stub — пишем JSON header с placeholder ("heap
    // profile sampling not yet wired in runtime").
    //
    // Phase B: real runtime integration через bench.h heap-sampler thread.
    eprintln!("profile/heap: invoking bench-exe (stub — runtime sampler в Phase B)");

    let mut cmd = Command::new(opts.bench_exe);
    cmd.env("NOVA_BENCH_SAMPLES", opts.samples_override.to_string());
    cmd.env("NOVA_BENCH_HEAP_SAMPLE_MS", "10");
    if let Some(f) = opts.filter {
        cmd.env("NOVA_BENCH_FILTER", f);
    }
    let output = cmd.output()
        .map_err(|e| anyhow!("spawn bench: {}", e))?;

    // Stub: emit JSON placeholder.
    let stub = serde_json::json!({
        "profile_type": "heap",
        "format_version": "1",
        "note": "Heap profile runtime integration TBD Plan 57.B; \
                 this is a stub showing CLI surface.",
        "bench_exit": output.status.code().unwrap_or(-1),
    });
    std::fs::write(opts.out, serde_json::to_string_pretty(&stub)?)
        .map_err(|e| anyhow!("write heap profile: {}", e))?;
    eprintln!("profile/heap: stub written to {}", opts.out.display());
    Ok(0)
}

fn run_gc(opts: ProfileOpts) -> Result<i32> {
    // Подход: env NOVA_BENCH_GC_TRACE=1 → runtime эмитит __GC_PAUSE__ <ns>
    // на stderr при каждом collect'е. Парсим и эмитим histogram.
    //
    // MVP реализация: тот же stub что и heap — реальный runtime
    // integration TBD (требует gc.last_pause_ns API в Plan 32 ext).
    eprintln!("profile/gc: invoking bench-exe (stub — gc.last_pause_ns API \
               extension TBD Plan 57.B)");

    let mut cmd = Command::new(opts.bench_exe);
    cmd.env("NOVA_BENCH_SAMPLES", opts.samples_override.to_string());
    cmd.env("NOVA_BENCH_GC_TRACE", "1");
    if let Some(f) = opts.filter {
        cmd.env("NOVA_BENCH_FILTER", f);
    }
    let output = cmd.output()
        .map_err(|e| anyhow!("spawn bench: {}", e))?;

    let stub = format!(
        "GC pause profile (stub — Plan 57.B integration)\n\
         bench exit: {:?}\n\
         note: gc.last_pause_ns API в Plan 32 ext (TBD).\n",
        output.status.code());
    std::fs::write(opts.out, stub)
        .map_err(|e| anyhow!("write gc profile: {}", e))?;
    eprintln!("profile/gc: stub written to {}", opts.out.display());
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_mode_parse() {
        assert_eq!(ProfileMode::parse("cpu").unwrap(), ProfileMode::Cpu);
        assert_eq!(ProfileMode::parse("heap").unwrap(), ProfileMode::Heap);
        assert_eq!(ProfileMode::parse("gc").unwrap(), ProfileMode::Gc);
        assert!(ProfileMode::parse("invalid").is_err());
    }
}
