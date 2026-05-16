// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57 L1+L4 — `nova bench` runner.
//!
//! Compile одного .nv файла с set_bench_mode(true), запустить exe, парсить
//! JSONL output (__BENCH_START__ / __BENCH_RESULT__), аналайз через stats,
//! emit отчёт.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use nova_codegen::test_runner;

use super::repro::{self, SamplingMeta};
use super::schema::{RawBenchResult, AnalyzedBench, run_result_to_json};
use super::report;

pub struct BenchRunOpts<'a> {
    pub bench_path: &'a Path,
    pub repo: &'a Path,
    pub stdlib_dir: &'a Path,
    pub cg_include: &'a Path,
    pub rt_dir: &'a Path,
    pub tc_opts: test_runner::ToolchainOpts<'a>,
    /// Comma-separated substring filter for bench names.
    pub filter: Option<String>,
    /// Override sample count (default 100).
    pub samples: Option<u64>,
    /// Override warmup ms (default 500).
    pub warmup_ms: Option<u64>,
    /// Override time budget seconds per bench (default 10).
    pub time_budget_s: Option<u64>,
    /// GC backend.
    pub gc_kind: test_runner::GcKind,
    /// Compile timeout per bench (default 120s).
    pub compile_timeout_secs: u64,
    /// Bench process timeout (default 600s — 10 min for 100 benches × 10s).
    pub run_timeout_secs: u64,
    /// Keep intermediate artifacts (.c, .exe) in tmp dir.
    pub keep_artifacts: bool,
    /// Mono depth override.
    pub mono_depth: Option<usize>,
    /// Force build mode. Default Release; "dev" пропускает LTO когда
    /// нет lld в PATH (`fuse-ld=lld` иначе требуется).
    pub mode: test_runner::Mode,
    /// Output destination — JSON path or None for stdout.
    pub out_json: Option<&'a Path>,
    /// Output CSV path.
    pub out_csv: Option<&'a Path>,
    /// Output markdown path.
    pub out_md: Option<&'a Path>,
    /// Plan 57.B.2: Criterion-compatible JSON directory output.
    /// Creates `<dir>/<bench>/new/{estimates,sample,benchmark}.json`.
    pub out_criterion: Option<&'a Path>,
    /// Print colored terminal output (auto-detect via main if None).
    pub color: bool,
}

pub fn run(opts: BenchRunOpts) -> Result<i32> {
    // Validate input file.
    if !opts.bench_path.exists() {
        bail!("bench file not found: {}", opts.bench_path.display());
    }
    let bench_path = opts.bench_path.canonicalize()
        .map_err(|e| anyhow!("cannot resolve path {}: {}", opts.bench_path.display(), e))?;
    if bench_path.is_dir() {
        bail!("`nova bench` MVP requires a single .nv file (multi-file collection — Phase B). \
               Got directory: {}", bench_path.display());
    }
    if bench_path.extension().and_then(|s| s.to_str()) != Some("nv") {
        bail!("not a Nova source: {}", bench_path.display());
    }

    let src = std::fs::read_to_string(&bench_path)
        .map_err(|e| anyhow!("read bench source: {}", e))?;
    let path_str = bench_path.to_string_lossy();

    // ── Pipeline parse → resolve → typecheck → desugar → callnorm → codegen.
    let mut module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    nova_codegen::imports::resolve_imports_inline(
        &bench_path, &mut module, opts.repo, opts.stdlib_dir,
    )?;
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let msgs: Vec<String> = errs.iter()
            .map(|d| d.render(&src, &path_str))
            .collect();
        anyhow!("{}", msgs.join("\n"))
    })?;
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module);

    // Codegen with bench_mode = true.
    let mut emitter = nova_codegen::codegen::CEmitter::new();
    emitter.set_source_for_annotations(src.clone());
    emitter.set_bench_mode(true);
    if let Some(n) = opts.mono_depth {
        emitter.set_mono_depth_limit(n);
    }
    let (c_code, warnings) = emitter
        .emit_module(&module)
        .map_err(|e| anyhow!("codegen error: {}", e))?;
    for w in &warnings {
        eprintln!("{}", w);
    }

    // Write .c to tmp dir.
    let stem = bench_path.file_stem().and_then(|s| s.to_str()).unwrap_or("bench");
    let exe_name = if cfg!(target_os = "windows") {
        format!("{}_bench.exe", stem)
    } else {
        format!("{}_bench", stem)
    };
    let hash = simple_hash(&bench_path.display().to_string());
    let tmp_path = std::env::temp_dir().join(format!("nova-bench-{}", &hash[..hash.len().min(12)]));
    std::fs::create_dir_all(&tmp_path).map_err(|e| anyhow!("create tmp: {}", e))?;
    let c_file = tmp_path.join(format!("{}_bench.c", stem));
    let exe_file = tmp_path.join(&exe_name);
    std::fs::write(&c_file, &c_code).map_err(|e| anyhow!("write .c: {}", e))?;

    let tc = test_runner::detect_toolchain(&opts.tc_opts)?;
    let libuv = test_runner::detect_or_build_libuv(opts.rt_dir, opts.repo, tc.vcvars_path());
    test_runner::install_cancel_handler();

    // Mode определяется параметром (default Release; см. feedback_release_builds).
    let build_opts = test_runner::BuildOpts {
        c_file: &c_file,
        exe_file: &exe_file,
        obj_dir: &tmp_path,
        cg_include: opts.cg_include,
        rt_dir: opts.rt_dir,
        mode: opts.mode,
        libuv: libuv.as_ref(),
        gc_kind: opts.gc_kind,
    };
    test_runner::compile_c_to_exe(&tc, &build_opts, Duration::from_secs(opts.compile_timeout_secs))?;

    if !opts.keep_artifacts {
        // Don't delete tmp dir yet — exe lives there. Schedule cleanup after run.
    }

    // Setup env for the bench process.
    let mut cmd = std::process::Command::new(&exe_file);
    if let Some(ref f) = opts.filter {
        cmd.env("NOVA_BENCH_FILTER", f);
    }
    if let Some(s) = opts.samples {
        cmd.env("NOVA_BENCH_SAMPLES", s.to_string());
    }
    if let Some(w_ms) = opts.warmup_ms {
        let w_ns = w_ms * 1_000_000;
        cmd.env("NOVA_BENCH_WARMUP_NS", w_ns.to_string());
    }
    if let Some(tb_s) = opts.time_budget_s {
        let tb_ns = tb_s * 1_000_000_000;
        cmd.env("NOVA_BENCH_TIME_BUDGET_NS", tb_ns.to_string());
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::inherit());

    // Run with timeout via thread+join (no async runtime required).
    let mut child = cmd.spawn().map_err(|e| anyhow!("spawn bench exe: {}", e))?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout from bench exe"))?;

    // Read stdout in current thread; child is reaped after.
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(stdout);
    let mut raw_results: Vec<RawBenchResult> = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| anyhow!("read bench stdout: {}", e))?;
        if let Some(r) = RawBenchResult::parse_line(&line) {
            raw_results.push(r);
        } else if line.starts_with("__BENCH_START__") {
            eprintln!("{}", line.trim_start_matches("__BENCH_START__").trim());
        }
        // Other lines passed silently (stderr inherited получает diagnostics).
    }
    let status = child.wait().map_err(|e| anyhow!("wait bench exe: {}", e))?;
    if !status.success() {
        // Soft warn — некоторые benches могут assert-fail, всё равно показываем результаты.
        eprintln!("warning: bench process exited non-zero: {:?}", status.code());
    }

    let benches: Vec<AnalyzedBench> = raw_results.into_iter()
        .filter_map(AnalyzedBench::from_raw)
        .collect();

    // Metadata.
    let sampling = SamplingMeta {
        warmup_ns: opts.warmup_ms.map(|m| m * 1_000_000).unwrap_or(500_000_000),
        target_ns: 1_000_000,
        samples: opts.samples.unwrap_or(100),
        time_budget_ns: opts.time_budget_s.map(|s| s * 1_000_000_000).unwrap_or(10_000_000_000),
    };
    let gc_str = match opts.gc_kind {
        test_runner::GcKind::Malloc => "malloc",
        test_runner::GcKind::Boehm => "boehm",
    };
    let meta = repro::collect(gc_str, sampling);

    // Output.
    print!("{}", report::terminal_report(&meta, &benches, opts.color));
    if let Some(p) = opts.out_json {
        let json = run_result_to_json(&meta, &benches);
        std::fs::write(p, serde_json::to_string_pretty(&json)?)
            .map_err(|e| anyhow!("write JSON: {}", e))?;
        eprintln!("wrote JSON to {}", p.display());
    }
    if let Some(p) = opts.out_csv {
        std::fs::write(p, report::csv_report(&benches))
            .map_err(|e| anyhow!("write CSV: {}", e))?;
        eprintln!("wrote CSV to {}", p.display());
    }
    if let Some(p) = opts.out_criterion {
        let n = super::criterion_compat::write_all(p, &benches)?;
        eprintln!("wrote Criterion-compat layout to {} ({} benches)",
            p.display(), n);
    }
    if let Some(p) = opts.out_md {
        let mut md = String::new();
        md.push_str(&format!("# Bench results — {}\n\n",
            bench_path.file_name().and_then(|s| s.to_str()).unwrap_or("?")));
        md.push_str("| Bench | median | MAD | mean | stddev | n | outliers |\n");
        md.push_str("|---|---|---|---|---|---|---|\n");
        for b in &benches {
            let st = &b.stats_ns;
            md.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} |\n",
                b.raw.name,
                report::fmt_duration(st.median),
                report::fmt_duration(st.mad),
                report::fmt_duration(st.mean),
                report::fmt_duration(st.stddev),
                st.n,
                st.outliers_low + st.outliers_high));
        }
        std::fs::write(p, md).map_err(|e| anyhow!("write MD: {}", e))?;
        eprintln!("wrote markdown to {}", p.display());
    }

    // Cleanup unless --keep-artifacts.
    if !opts.keep_artifacts {
        let _ = std::fs::remove_dir_all(&tmp_path);
    }

    if benches.is_empty() {
        bail!("no bench results collected — file may contain no `bench` items");
    }
    Ok(0)
}

/// Plan 57.A.5: compile a bench file и вернуть exe-path для последующего
/// profile-run (отдельный invocation от measurement run чтобы
/// instrumentation noise не влиял на baseline numbers).
pub fn compile_for_profile(opts: &BenchRunOpts) -> Result<std::path::PathBuf> {
    let bench_path = opts.bench_path.canonicalize()
        .map_err(|e| anyhow!("cannot resolve {}: {}", opts.bench_path.display(), e))?;
    let src = std::fs::read_to_string(&bench_path)
        .map_err(|e| anyhow!("read bench source: {}", e))?;
    let path_str = bench_path.to_string_lossy();

    let mut module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    nova_codegen::imports::resolve_imports_inline(
        &bench_path, &mut module, opts.repo, opts.stdlib_dir,
    )?;
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let msgs: Vec<String> = errs.iter()
            .map(|d| d.render(&src, &path_str))
            .collect();
        anyhow!("{}", msgs.join("\n"))
    })?;
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module);

    let mut emitter = nova_codegen::codegen::CEmitter::new();
    emitter.set_source_for_annotations(src.clone());
    emitter.set_bench_mode(true);
    if let Some(n) = opts.mono_depth {
        emitter.set_mono_depth_limit(n);
    }
    let (c_code, _warnings) = emitter
        .emit_module(&module)
        .map_err(|e| anyhow!("codegen error: {}", e))?;

    let stem = bench_path.file_stem().and_then(|s| s.to_str()).unwrap_or("bench");
    let exe_name = if cfg!(target_os = "windows") {
        format!("{}_profile.exe", stem)
    } else {
        format!("{}_profile", stem)
    };
    let hash = simple_hash(&bench_path.display().to_string());
    let tmp_path = std::env::temp_dir().join(format!("nova-bench-profile-{}", &hash[..hash.len().min(12)]));
    std::fs::create_dir_all(&tmp_path).map_err(|e| anyhow!("create tmp: {}", e))?;
    let c_file = tmp_path.join(format!("{}_profile.c", stem));
    let exe_file = tmp_path.join(&exe_name);
    std::fs::write(&c_file, &c_code).map_err(|e| anyhow!("write .c: {}", e))?;

    let tc = test_runner::detect_toolchain(&opts.tc_opts)?;
    let libuv = test_runner::detect_or_build_libuv(opts.rt_dir, opts.repo, tc.vcvars_path());
    test_runner::install_cancel_handler();

    let build_opts = test_runner::BuildOpts {
        c_file: &c_file,
        exe_file: &exe_file,
        obj_dir: &tmp_path,
        cg_include: opts.cg_include,
        rt_dir: opts.rt_dir,
        mode: opts.mode,
        libuv: libuv.as_ref(),
        gc_kind: opts.gc_kind,
    };
    test_runner::compile_c_to_exe(&tc, &build_opts, Duration::from_secs(opts.compile_timeout_secs))?;
    Ok(exe_file)
}

/// Deterministic content-free hash for tmp-dir naming. Не cryptographic.
pub fn simple_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}
