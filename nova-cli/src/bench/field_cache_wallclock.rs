// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 123.6.2.1 (V6.2.1, 2026-06-03) — real wall-clock bench для
//! Plan 123 receiver field caching.
//!
//! Расширяет V6.2 static `cpu_savings_estimate` real wallclock-измерением
//! полнопрограммного runtime impact'а field-cache. Подход:
//!
//! 1. Для каждого `.nv` файла собираем **два** exe через subprocess
//!    `nova build`: с дефолтным cfg (cache ON) и с `NOVA_FIELD_CACHE=0`
//!    (cache OFF — same pipeline minus AST pass).
//! 2. Запускаем каждый exe N samples с warmup interleaved (off,on,off,on…);
//!    меряем wall-clock через `Instant::now()`.
//! 3. Median per variant; speedup = (off_med − on_med) / off_med × 100.
//! 4. Static V6.2 estimate в-процессе для cross-validation.
//! 5. Опциональный JSON output + baseline gate (regression-pp threshold).
//!
//! Cross-validates V6.2 cycle estimate vs реальный wallclock на корпусе
//! real-world программ. Стабилизация шума — median + interleaved sampling
//! + geomean aggregate (resistant к outliers / nondeterministic noise).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

/// JSON schema version for `kind: field-cache-wallclock` artifacts.
/// Bump when breaking changes are made to the emitted shape.
pub const FORMAT_VERSION: &str = "1";

pub struct FieldCacheWallclockOpts<'a> {
    /// Path to single .nv file or directory.
    pub path: &'a Path,
    /// Path to self (`nova[.exe]`) used as the `nova build` subprocess.
    pub self_exe: &'a Path,
    /// Build mode (release|dev). Default release.
    pub mode: String,
    /// Toolchain (auto|clang|msvc|gcc).
    pub toolchain: String,
    /// GC backend (malloc|boehm). Reserved for future use (`nova build` does
    /// not currently expose `--gc`; field is preserved for `--gc` rollout).
    pub gc: String,
    /// Samples per variant kept after warmup. Default 11.
    pub samples: u32,
    /// Warmup runs (discarded per variant). Default 2.
    pub warmup: u32,
    /// Per-build timeout in seconds (soft — relies on `nova build` own
    /// timeout default).
    pub build_timeout_secs: u64,
    /// Per-run timeout in seconds (soft — invoking exe blocks via wait()).
    pub run_timeout_secs: u64,
    /// JSON output path.
    pub out_json: Option<PathBuf>,
    /// Baseline JSON for regression gate.
    pub baseline: Option<PathBuf>,
    /// Gate regression threshold (percentage points). If
    /// `baseline_geomean − new_geomean > threshold`, exit 1.
    /// Default applied at call-site (2.0 pp).
    pub gate_regression_pp: Option<f64>,
    /// Skip files that don't compile / lack `fn main`; emit warn vs bail.
    pub skip_failed: bool,
}

#[derive(Debug, Clone)]
pub struct WallclockEntry {
    pub file: PathBuf,
    /// `"ok"` или `"skip: <reason>"` / `"fail: <reason>"`.
    pub status: String,
    pub off_samples_ns: Vec<u64>,
    pub on_samples_ns: Vec<u64>,
    pub off_median_ns: u64,
    pub on_median_ns: u64,
    /// (off_median − on_median) / off_median × 100. Positive = cache faster.
    pub speedup_pct: f64,
    /// V6.2 `cpu_savings_estimate(report).estimated_cycles_saved`.
    pub static_cycles: u64,
    pub static_per_layer: StaticLayerBreakdown,
}

#[derive(Debug, Clone, Default)]
pub struct StaticLayerBreakdown {
    pub ro: u64,
    pub mu: u64,
    pub licm: u64,
    pub pure: u64,
    pub chain: u64,
}

pub fn run(opts: FieldCacheWallclockOpts) -> Result<i32> {
    let files = discover_files(opts.path)?;
    if files.is_empty() {
        bail!("no .nv files found at {}", opts.path.display());
    }
    eprintln!("nova bench field-cache: {} candidate file(s)", files.len());

    let mut entries: Vec<WallclockEntry> = Vec::with_capacity(files.len());
    let mut failed_count = 0usize;
    for f in &files {
        match std::fs::read_to_string(f) {
            Ok(src) => {
                if !has_fn_main(&src) {
                    eprintln!("  skip {} (no fn main)", f.display());
                    entries.push(skip_entry(f, "no fn main"));
                    continue;
                }
            }
            Err(e) => {
                eprintln!("  skip {} (read: {})", f.display(), e);
                entries.push(skip_entry(f, "read error"));
                continue;
            }
        };
        eprintln!("nova bench field-cache: measuring {}", f.display());
        match measure_one(&opts, f) {
            Ok(e) => entries.push(e),
            Err(e) => {
                eprintln!("  warn: {} — {}", f.display(), e);
                failed_count += 1;
                entries.push(skip_entry(f, &format!("fail: {}", e)));
                if !opts.skip_failed {
                    // continue accumulating, but tracked для exit code.
                }
            }
        }
    }

    let measured: Vec<&WallclockEntry> = entries.iter()
        .filter(|e| e.status == "ok").collect();
    if measured.is_empty() {
        bail!("no files measured successfully ({} failed/skipped)", entries.len());
    }
    let geomean = geomean_speedup_pct(&measured);
    let total_static: u64 = measured.iter()
        .map(|e| e.static_cycles).sum();

    print_table(&entries, geomean, total_static);

    if let Some(p) = opts.out_json.as_ref() {
        let v = build_json(&entries, geomean, total_static, opts.samples,
                            opts.warmup);
        std::fs::write(p, serde_json::to_string_pretty(&v)?)
            .map_err(|e| anyhow!("write JSON: {}", e))?;
        eprintln!("nova bench field-cache: wrote JSON to {}", p.display());
    }

    if let Some(b) = opts.baseline.as_ref() {
        let threshold = opts.gate_regression_pp.unwrap_or(2.0);
        let baseline_v: Value = serde_json::from_str(
            &std::fs::read_to_string(b)
                .map_err(|e| anyhow!("read baseline {}: {}", b.display(), e))?
        ).map_err(|e| anyhow!("parse baseline JSON: {}", e))?;
        let base_geomean = baseline_v.get("aggregate")
            .and_then(|a| a.get("geomean_speedup_pct"))
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow!(
                "baseline JSON missing aggregate.geomean_speedup_pct"))?;
        let drop = base_geomean - geomean;
        eprintln!("nova bench field-cache: baseline geomean={:.2}%, \
                   new={:.2}%, drop={:+.2} pp (threshold {:.2} pp)",
            base_geomean, geomean, drop, threshold);
        if drop > threshold {
            eprintln!("nova bench field-cache: REGRESSION \
                       (drop {:.2} pp > threshold {:.2} pp)",
                drop, threshold);
            return Ok(1);
        }
    }

    if failed_count > 0 && !opts.skip_failed {
        return Ok(1);
    }
    Ok(0)
}

fn measure_one(opts: &FieldCacheWallclockOpts, file: &Path)
    -> Result<WallclockEntry>
{
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("prog");
    let hash = super::run::simple_hash(&file.display().to_string());
    let tmp_dir = std::env::temp_dir()
        .join(format!("nova-fc-wc-{}", &hash[..hash.len().min(12)]));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let ext = if cfg!(target_os = "windows") { ".exe" } else { "" };
    let exe_on = tmp_dir.join(format!("{}_on{}", stem, ext));
    let exe_off = tmp_dir.join(format!("{}_off{}", stem, ext));

    build_one(opts, file, &exe_on, /*fc_off=*/false)
        .map_err(|e| anyhow!("build ON: {}", e))?;
    build_one(opts, file, &exe_off, /*fc_off=*/true)
        .map_err(|e| anyhow!("build OFF: {}", e))?;

    // Interleaved sampling reduces systematic drift bias (CPU thermal,
    // scheduler) compared with sequential off-all-then-on-all.
    let total = opts.warmup as usize + opts.samples as usize;
    let mut on_samples: Vec<u64> = Vec::with_capacity(total);
    let mut off_samples: Vec<u64> = Vec::with_capacity(total);
    for i in 0..total {
        // Run off first per iteration; order-invariant after warmup drop.
        off_samples.push(run_one(&exe_off, opts.run_timeout_secs)?);
        on_samples.push(run_one(&exe_on, opts.run_timeout_secs)?);
        if i + 1 == opts.warmup as usize {
            on_samples.clear();
            off_samples.clear();
        }
    }

    let on_med = median(&on_samples);
    let off_med = median(&off_samples);
    let speedup = if off_med > 0 {
        ((off_med as f64 - on_med as f64) / off_med as f64) * 100.0
    } else { 0.0 };

    let (static_cycles, breakdown) = static_estimate(file)?;

    let _ = std::fs::remove_file(&exe_on);
    let _ = std::fs::remove_file(&exe_off);

    Ok(WallclockEntry {
        file: file.to_path_buf(),
        status: "ok".to_string(),
        off_samples_ns: off_samples,
        on_samples_ns: on_samples,
        off_median_ns: off_med,
        on_median_ns: on_med,
        speedup_pct: speedup,
        static_cycles,
        static_per_layer: breakdown,
    })
}

fn build_one(opts: &FieldCacheWallclockOpts, file: &Path,
              out: &Path, fc_off: bool) -> Result<()> {
    let _ = opts.build_timeout_secs; // soft — `nova build` имеет own timeout
    let _ = opts.gc; // see field docstring
    let mut cmd = Command::new(opts.self_exe);
    cmd.arg("build").arg(file).arg("-o").arg(out);
    cmd.args(["--mode", &opts.mode]);
    cmd.args(["--toolchain", &opts.toolchain]);
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());
    // Explicit override prevents inherited NOVA_FIELD_CACHE из shell от leak
    // в одну из веток (ON/OFF) и порчи измерения.
    if fc_off {
        cmd.env("NOVA_FIELD_CACHE", "0");
    } else {
        cmd.env("NOVA_FIELD_CACHE", "1");
    }
    let output = cmd.output()
        .map_err(|e| anyhow!("spawn nova build: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let preview: String = stderr.lines().take(5)
            .collect::<Vec<_>>().join(" | ");
        bail!("nova build exit {:?}: {}", output.status.code(), preview);
    }
    Ok(())
}

fn run_one(exe: &Path, timeout_secs: u64) -> Result<u64> {
    let _ = timeout_secs; // wait() blocks
    let start = Instant::now();
    let status = Command::new(exe)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| anyhow!("spawn {}: {}", exe.display(), e))?;
    let elapsed = start.elapsed();
    if !status.success() {
        bail!("exe exited non-zero: {:?}", status.code());
    }
    let ns = elapsed.as_nanos();
    Ok(if ns > u64::MAX as u128 { u64::MAX } else { ns as u64 })
}

pub(crate) fn median(samples: &[u64]) -> u64 {
    if samples.is_empty() { return 0; }
    let mut sorted = samples.to_vec();
    sorted.sort();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        // bit-safe average — avoids overflow на больших ns.
        let lo = sorted[mid - 1];
        let hi = sorted[mid];
        lo + (hi - lo) / 2
    } else {
        sorted[mid]
    }
}

/// Geometric mean of (1 + speedup/100) per file — unbiased combined
/// improvement factor (Hennessy & Patterson §1.10). Reported back as pct.
pub fn geomean_speedup_pct(entries: &[&WallclockEntry]) -> f64 {
    let ok: Vec<f64> = entries.iter()
        .filter(|e| e.status == "ok")
        .map(|e| 1.0 + e.speedup_pct / 100.0)
        .filter(|x| *x > 0.0)
        .collect();
    if ok.is_empty() { return 0.0; }
    let sum_ln: f64 = ok.iter().map(|x| x.ln()).sum();
    let geomean = (sum_ln / ok.len() as f64).exp();
    (geomean - 1.0) * 100.0
}

fn static_estimate(file: &Path) -> Result<(u64, StaticLayerBreakdown)> {
    let src = std::fs::read_to_string(file)
        .map_err(|e| anyhow!("read for estimate: {}", e))?;
    let mut module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("parse: {}",
            d.render(&src, &file.to_string_lossy())))?;
    if let Some(repo) = nova_codegen::test_runner::find_repo_root_from(file) {
        let stdlib_dir = repo.join("std");
        let _ = nova_codegen::imports::resolve_imports_inline_ex(
            file, &mut module, &repo, &stdlib_dir, true);
    }
    if nova_codegen::types::check_module(&module).is_err() {
        return Ok((0, StaticLayerBreakdown::default()));
    }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module);
    let cfg = nova_codegen::field_cache::FieldCacheConfig::from_env_or_default();
    let report = nova_codegen::field_cache::analyze_module(&module, &cfg);
    let savings = nova_codegen::field_cache::cpu_savings_estimate(&report);
    Ok((savings.estimated_cycles_saved, StaticLayerBreakdown {
        ro: savings.layer_ro,
        mu: savings.layer_mut,
        licm: savings.layer_licm,
        pure: savings.layer_pure,
        chain: savings.layer_chain,
    }))
}

fn has_fn_main(src: &str) -> bool {
    src.lines().any(|l| {
        let t = l.trim_start();
        // Match `fn main(` or `pub fn main(` — заголовок фn, не другие
        // identifiers вроде `mainish` / `main_loop`.
        let after = t.strip_prefix("pub ").unwrap_or(t);
        if let Some(rest) = after.strip_prefix("fn main") {
            matches!(rest.bytes().next(), Some(b'(') | Some(b' '))
        } else { false }
    })
}

fn discover_files(p: &Path) -> Result<Vec<PathBuf>> {
    if !p.exists() {
        bail!("path not found: {}", p.display());
    }
    if p.is_file() {
        if p.extension().and_then(|s| s.to_str()) != Some("nv") {
            bail!("not a Nova source: {}", p.display());
        }
        return Ok(vec![p.to_path_buf()]);
    }
    let mut out = Vec::new();
    walk(p, &mut out);
    out.sort();
    Ok(out)
}

fn walk(d: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(d) {
        Ok(e) => e,
        Err(_) => return,
    };
    for ent in entries.flatten() {
        let p = ent.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with('.') { continue; }
        if p.is_dir() { walk(&p, out); }
        else if name.ends_with(".nv") && name != "_module.nv" {
            out.push(p);
        }
    }
}

fn skip_entry(file: &Path, reason: &str) -> WallclockEntry {
    WallclockEntry {
        file: file.to_path_buf(),
        status: format!("skip: {}", reason),
        off_samples_ns: Vec::new(),
        on_samples_ns: Vec::new(),
        off_median_ns: 0,
        on_median_ns: 0,
        speedup_pct: 0.0,
        static_cycles: 0,
        static_per_layer: StaticLayerBreakdown::default(),
    }
}

fn fmt_ns(ns: u64) -> String {
    let v = ns as f64;
    if v < 1e3 { format!("{:.0}ns", v) }
    else if v < 1e6 { format!("{:.1}µs", v / 1e3) }
    else if v < 1e9 { format!("{:.2}ms", v / 1e6) }
    else { format!("{:.2}s", v / 1e9) }
}

fn print_table(entries: &[WallclockEntry], geomean: f64, total_static: u64) {
    println!();
    println!("field-cache wallclock results:");
    println!("{:<44} {:>10} {:>10} {:>9} {:>10}",
        "file", "off med", "on med", "speedup", "static cyc");
    println!("{}", "─".repeat(86));
    for e in entries {
        let name = e.file.file_name().and_then(|s| s.to_str())
            .unwrap_or("?");
        if e.status != "ok" {
            println!("{:<44} {:>41}", truncate(name, 44), e.status);
            continue;
        }
        println!("{:<44} {:>10} {:>10} {:>+8.2}% {:>10}",
            truncate(name, 44),
            fmt_ns(e.off_median_ns), fmt_ns(e.on_median_ns),
            e.speedup_pct, e.static_cycles);
    }
    println!("{}", "─".repeat(86));
    println!("{:<44} {:>10} {:>10} {:>+8.2}% {:>10}",
        "geomean", "", "", geomean, total_static);
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() }
    else {
        let prefix: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{}…", prefix)
    }
}

fn build_json(entries: &[WallclockEntry], geomean: f64,
              total_static: u64, samples: u32, warmup: u32) -> Value {
    let items: Vec<Value> = entries.iter().map(|e| {
        json!({
            "file": e.file.display().to_string(),
            "status": e.status,
            "off_median_ns": e.off_median_ns,
            "on_median_ns": e.on_median_ns,
            "off_samples_ns": e.off_samples_ns,
            "on_samples_ns": e.on_samples_ns,
            "speedup_pct": e.speedup_pct,
            "static_cycles": e.static_cycles,
            "static_per_layer": {
                "ro": e.static_per_layer.ro,
                "mut": e.static_per_layer.mu,
                "licm": e.static_per_layer.licm,
                "pure": e.static_per_layer.pure,
                "chain": e.static_per_layer.chain,
            },
        })
    }).collect();
    let ok = entries.iter().filter(|e| e.status == "ok").count();
    let skipped = entries.len() - ok;
    json!({
        "format_version": FORMAT_VERSION,
        "kind": "field-cache-wallclock",
        "samples_per_variant": samples,
        "warmup_runs": warmup,
        "entries": items,
        "aggregate": {
            "geomean_speedup_pct": geomean,
            "total_static_cycles": total_static,
            "files_measured": ok,
            "files_skipped": skipped,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v6_2_1_median_odd() {
        assert_eq!(median(&[3, 1, 2]), 2);
        assert_eq!(median(&[100, 50, 200]), 100);
    }

    #[test]
    fn v6_2_1_median_even() {
        assert_eq!(median(&[1, 2, 3, 4]), 2);
        // (100 + 200) / 2 — via lo + (hi-lo)/2 = 100 + 50 = 150
        assert_eq!(median(&[400, 100, 300, 200]), 250);
    }

    #[test]
    fn v6_2_1_median_empty() {
        assert_eq!(median(&[]), 0);
    }

    #[test]
    fn v6_2_1_has_fn_main_positive() {
        assert!(has_fn_main("fn main() Io -> () { }"));
        assert!(has_fn_main("  pub fn main() {}"));
        assert!(has_fn_main("fn main () { 1 }"));
    }

    #[test]
    fn v6_2_1_has_fn_main_negative() {
        assert!(!has_fn_main("fn mainish() {}"));
        assert!(!has_fn_main("type Main {}"));
        assert!(!has_fn_main("fn main_loop() {}"));
        assert!(!has_fn_main(""));
    }

    #[test]
    fn v6_2_1_geomean_three_files() {
        // 10%, 20%, 30% — geomean(1.1·1.2·1.3) = (1.716)^(1/3) ≈ 1.1972
        // → ≈19.72% (НЕ среднее арифметическое 20%; geomean ниже из-за
        // log-convexity, что и есть нужное свойство для composite ratio).
        let e1 = mock("a", 100, 90);
        let e2 = mock("b", 100, 80);
        let e3 = mock("c", 100, 70);
        let r: Vec<&WallclockEntry> = vec![&e1, &e2, &e3];
        let g = geomean_speedup_pct(&r);
        assert!((g - 19.72).abs() < 0.1, "expected ≈19.72%, got {}", g);
    }

    #[test]
    fn v6_2_1_geomean_empty_zero() {
        let r: Vec<&WallclockEntry> = vec![];
        assert_eq!(geomean_speedup_pct(&r), 0.0);
    }

    #[test]
    fn v6_2_1_geomean_skips_non_ok() {
        let e1 = mock("a", 100, 80);
        let mut e2 = mock("b", 100, 70);
        e2.status = "skip: no fn main".to_string();
        let r: Vec<&WallclockEntry> = vec![&e1, &e2];
        let g = geomean_speedup_pct(&r);
        // Only e1 contributes — exactly 20%.
        assert!((g - 20.0).abs() < 0.01);
    }

    #[test]
    fn v6_2_1_json_shape_ok() {
        let e = mock("x.nv", 1_000_000, 900_000);
        let v = build_json(&[e], 10.0, 100, 11, 2);
        assert_eq!(v["format_version"], "1");
        assert_eq!(v["kind"], "field-cache-wallclock");
        assert_eq!(v["samples_per_variant"], 11);
        assert_eq!(v["warmup_runs"], 2);
        assert_eq!(v["aggregate"]["geomean_speedup_pct"], 10.0);
        assert_eq!(v["aggregate"]["total_static_cycles"], 100);
        assert_eq!(v["aggregate"]["files_measured"], 1);
        assert_eq!(v["aggregate"]["files_skipped"], 0);
        let entry = &v["entries"][0];
        assert!(entry["static_per_layer"].is_object());
        assert!(entry["off_samples_ns"].is_array());
    }

    #[test]
    fn v6_2_1_skip_entry_excluded_from_aggregate() {
        let ok = mock("ok.nv", 100, 80);
        let skipped = skip_entry(&PathBuf::from("skip.nv"), "no fn main");
        let v = build_json(&[ok, skipped], 20.0, 50, 5, 1);
        assert_eq!(v["aggregate"]["files_measured"], 1);
        assert_eq!(v["aggregate"]["files_skipped"], 1);
        assert!(v["entries"][1]["status"].as_str().unwrap()
            .starts_with("skip:"));
    }

    #[test]
    fn v6_2_1_truncate_long_path() {
        let long = "a".repeat(60);
        let t = truncate(&long, 20);
        assert_eq!(t.chars().count(), 20);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn v6_2_1_fmt_ns_scales() {
        assert_eq!(fmt_ns(500), "500ns");
        assert_eq!(fmt_ns(5_000), "5.0µs");
        assert_eq!(fmt_ns(5_000_000), "5.00ms");
        assert_eq!(fmt_ns(5_000_000_000), "5.00s");
    }

    fn mock(name: &str, off: u64, on: u64) -> WallclockEntry {
        let sp = if off > 0 {
            ((off as f64 - on as f64) / off as f64) * 100.0
        } else { 0.0 };
        WallclockEntry {
            file: PathBuf::from(name),
            status: "ok".to_string(),
            off_samples_ns: vec![off],
            on_samples_ns: vec![on],
            off_median_ns: off,
            on_median_ns: on,
            speedup_pct: sp,
            static_cycles: 0,
            static_per_layer: StaticLayerBreakdown::default(),
        }
    }
}
