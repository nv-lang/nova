// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57 L1+L4 — `nova bench` runner.
//!
//! Compile одного .nv файла с set_bench_mode(true), запустить exe, парсить
//! JSONL output (__BENCH_START__ / __BENCH_RESULT__), аналайз через stats,
//! emit отчёт.

use std::path::{Path, PathBuf};
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
    /// Plan 57.G.4 — render ASCII histogram per-bench after the table.
    pub histogram: bool,
}

pub fn run(opts: BenchRunOpts) -> Result<i32> {
    // Plan 57.C.5: recursive directory discovery.
    if !opts.bench_path.exists() {
        bail!("bench path not found: {}", opts.bench_path.display());
    }
    if opts.bench_path.is_dir() {
        return run_dir(opts);
    }
    let bench_path = opts.bench_path.canonicalize()
        .map_err(|e| anyhow!("cannot resolve path {}: {}", opts.bench_path.display(), e))?;
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
    // Plan 57.B.3: expand parameterized bench sweeps ДО type-check.
    // `bench "name" (n in [10,100]) { ... }` → 2 separate BenchDecl entries
    // с `let n = <value>;` prepended к setup, так что name-resolution видит
    // `n` как regular let.
    expand_bench_sweeps(&mut module);
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let msgs: Vec<String> = errs.iter()
            .map(|d| d.render(&src, &path_str))
            .collect();
        anyhow!("{}", msgs.join("\n"))
    })?;
    nova_codegen::types::infer_effects(&mut module);
    // Plan 57.C.7: run lints (включая bench-specific warnings).
    for w in nova_codegen::lints::lint_module(&module) {
        let (line, col) = nova_codegen::diag::byte_to_line_col(&src, w.diag.span.start);
        eprintln!("warning: {}:{}:{}: {} [{}]",
            bench_path.display(), line, col, w.diag.message, w.rule);
    }
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
    // Plan 83.1 Ф.5: микро-бенчи по умолчанию single-worker — M:N-шум
    // (work-stealing, sysmon-preemption) искажает single-thread-замеры.
    // Бенч, которому нужен M:N-параллелизм, делает явный runtime.init(N)
    // — explicit бьёт env (D136).
    cmd.env("NOVA_MAXPROCS", "1");
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
    // Plan 57.C.3: pipe stderr тоже, чтобы parse __HEAP_SAMPLE__.
    cmd.stderr(std::process::Stdio::piped());

    // Run with timeout via thread+join (no async runtime required).
    let mut child = cmd.spawn().map_err(|e| anyhow!("spawn bench exe: {}", e))?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout from bench exe"))?;
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr from bench exe"))?;

    // Read stdout + stderr concurrently (parallel threads).
    use std::io::{BufRead, BufReader};
    let stderr_handle = std::thread::spawn(move || -> Vec<(u64, u64)> {
        let reader = BufReader::new(stderr);
        let mut heap_samples: Vec<(u64, u64)> = Vec::new();
        for line in reader.lines() {
            let line = match line { Ok(l) => l, Err(_) => break };
            // __HEAP_SAMPLE__ <ts_ns> <bytes>
            if let Some(rest) = line.strip_prefix("__HEAP_SAMPLE__ ") {
                let mut parts = rest.splitn(2, ' ');
                let ts = parts.next().and_then(|s| s.parse::<u64>().ok());
                let by = parts.next().and_then(|s| s.parse::<u64>().ok());
                if let (Some(t), Some(b)) = (ts, by) {
                    heap_samples.push((t, b));
                    continue;
                }
            }
            // Pass through non-__HEAP_SAMPLE__ stderr lines (diagnostics).
            eprintln!("{}", line);
        }
        heap_samples
    });

    let reader = BufReader::new(stdout);
    let mut raw_results: Vec<RawBenchResult> = Vec::new();
    // Plan 57.G.5 — accumulate __BENCH_METRIC__ markers per *current*
    // bench (между __BENCH_START__ <name> и __BENCH_RESULT__ {...}).
    // Aggregate by (metric_name, unit) → samples vec.
    let mut current_bench_name: Option<String> = None;
    let mut pending_metrics: Vec<(String, i64, String)> = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| anyhow!("read bench stdout: {}", e))?;
        if let Some(r) = RawBenchResult::parse_line(&line) {
            // Attach pending metrics (aggregated by name+unit) to this bench.
            let mut groups: std::collections::BTreeMap<(String, String), Vec<i64>>
                = std::collections::BTreeMap::new();
            for (n, v, u) in pending_metrics.drain(..) {
                groups.entry((n, u)).or_default().push(v);
            }
            let mut r = r;
            r.custom_metrics = groups.into_iter()
                .map(|((name, unit), samples)|
                    super::schema::CustomMetric { name, unit, samples })
                .collect();
            raw_results.push(r);
            current_bench_name = None;
        } else if let Some(rest) = line.strip_prefix("__BENCH_START__") {
            // Switch to new bench — discard pending metrics из предыдущего
            // (на случай abort/crash без __BENCH_RESULT__).
            pending_metrics.clear();
            current_bench_name = Some(rest.trim().to_string());
            eprintln!("{}", rest.trim());
        } else if line.starts_with("__BENCH_METRIC__") {
            if current_bench_name.is_some() {
                if let Some((n, v, u)) = super::schema::parse_metric_line(&line) {
                    pending_metrics.push((n, v, u));
                }
            }
        }
        // Other lines passed silently.
    }
    let status = child.wait().map_err(|e| anyhow!("wait bench exe: {}", e))?;
    let heap_samples = stderr_handle.join().unwrap_or_default();
    if !heap_samples.is_empty() {
        let bytes_only: Vec<u64> = heap_samples.iter().map(|(_, b)| *b).collect();
        let min_b = *bytes_only.iter().min().unwrap_or(&0);
        let max_b = *bytes_only.iter().max().unwrap_or(&0);
        let mut sorted = bytes_only.clone();
        sorted.sort();
        let median_b = sorted[sorted.len() / 2];
        eprintln!("heap profile: {} samples, min={} KB, median={} KB, max={} KB",
            heap_samples.len(), min_b / 1024, median_b / 1024, max_b / 1024);
    }
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
    // Plan 57.G.4 — opt-in ASCII histogram per bench (--histogram flag).
    if opts.histogram {
        for b in &benches {
            println!("\nDistribution: {}", b.raw.name);
            print!("{}", report::ascii_histogram(b, 40));
        }
    }
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
    expand_bench_sweeps(&mut module);
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

/// Plan 57.C.5: recursive bench discovery. Walks directory, runs каждого
/// .nv file как отдельный bench session, агрегирует результаты в один
/// финальный output. Skip non-bench .nv files (no `bench "..." { ... }`).
///
/// Plan 57.D.3: aggregated JSON output — если `opts.out_json` set, каждый
/// file пишет в temp JSON, потом merged в final aggregated JSON
/// (per-file benches concatenated, metadata из первого file).
fn run_dir(opts: BenchRunOpts) -> Result<i32> {
    let dir = opts.bench_path;
    let mut files: Vec<PathBuf> = Vec::new();
    walk_nv(dir, &mut files);
    files.sort();
    if files.is_empty() {
        bail!("no .nv files found in directory: {}", dir.display());
    }
    eprintln!("nova bench: discovered {} .nv files в {}", files.len(), dir.display());

    let mut total_benches = 0usize;

    // Plan 57.D.3: temp dir для per-file JSON aggregation.
    let agg_tmp_dir = if opts.out_json.is_some() || opts.out_csv.is_some()
                        || opts.out_md.is_some() || opts.out_criterion.is_some() {
        let d = std::env::temp_dir()
            .join(format!("nova-bench-agg-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        Some(d)
    } else { None };
    let mut per_file_json_paths: Vec<PathBuf> = Vec::new();

    for f in &files {
        // Filter — skip files without `bench` keyword (cheap text check).
        match std::fs::read_to_string(f) {
            Ok(src) => {
                if !src.contains("\nbench ") && !src.starts_with("bench ") {
                    eprintln!("nova bench: skip {} (no `bench` declarations)", f.display());
                    continue;
                }
            }
            Err(_) => continue,
        }
        eprintln!("nova bench: running {}", f.display());

        // Generate per-file temp JSON path если aggregation needed.
        let per_file_json = agg_tmp_dir.as_ref().map(|d| {
            let stem = f.file_stem().and_then(|s| s.to_str()).unwrap_or("bench");
            d.join(format!("{}-{}.json", stem, per_file_json_paths.len()))
        });
        let per_file_json_ref = per_file_json.as_deref();

        let single_opts = BenchRunOpts {
            bench_path: f,
            repo: opts.repo,
            stdlib_dir: opts.stdlib_dir,
            cg_include: opts.cg_include,
            rt_dir: opts.rt_dir,
            tc_opts: test_runner::ToolchainOpts {
                pref: opts.tc_opts.pref,
                explicit_clang: opts.tc_opts.explicit_clang,
                explicit_vcvars: opts.tc_opts.explicit_vcvars,
            },
            filter: opts.filter.clone(),
            samples: opts.samples,
            warmup_ms: opts.warmup_ms,
            time_budget_s: opts.time_budget_s,
            gc_kind: opts.gc_kind,
            compile_timeout_secs: opts.compile_timeout_secs,
            run_timeout_secs: opts.run_timeout_secs,
            keep_artifacts: opts.keep_artifacts,
            mono_depth: opts.mono_depth,
            mode: opts.mode,
            out_json: per_file_json_ref,
            out_csv: None,
            out_md: None,
            out_criterion: None,
            color: opts.color,
            histogram: opts.histogram,
        };
        let r = run(single_opts);
        if let Err(e) = r {
            eprintln!("nova bench: file {} failed — {}", f.display(), e);
        } else {
            total_benches += 1;
            if let Some(p) = per_file_json {
                if p.exists() { per_file_json_paths.push(p); }
            }
        }
    }

    eprintln!("\nnova bench: {} files processed", total_benches);

    // Plan 57.D.3: aggregate per-file JSON в один.
    if let Some(out_path) = opts.out_json {
        if per_file_json_paths.is_empty() {
            eprintln!("nova bench: no per-file JSON outputs to aggregate.");
        } else {
            let agg = aggregate_json_files(&per_file_json_paths, dir)?;
            std::fs::write(out_path, serde_json::to_string_pretty(&agg)?)
                .map_err(|e| anyhow!("write aggregated JSON: {}", e))?;
            eprintln!("nova bench: wrote aggregated JSON to {} ({} benches across {} files)",
                out_path.display(),
                agg.get("benches").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0),
                per_file_json_paths.len());
        }
    }
    if let Some(out_path) = opts.out_csv {
        if !per_file_json_paths.is_empty() {
            let agg = aggregate_json_files(&per_file_json_paths, dir)?;
            let benches: Vec<super::schema::AnalyzedBench> =
                super::schema::RunResultParsed::from_json(&agg)
                    .map_err(|e| anyhow!("aggregate parse: {}", e))?
                    .benches;
            std::fs::write(out_path, super::report::csv_report(&benches))
                .map_err(|e| anyhow!("write aggregated CSV: {}", e))?;
            eprintln!("nova bench: wrote aggregated CSV to {} ({} benches)",
                out_path.display(), benches.len());
        }
    }
    if let Some(out_path) = opts.out_md {
        if !per_file_json_paths.is_empty() {
            let agg = aggregate_json_files(&per_file_json_paths, dir)?;
            let parsed = super::schema::RunResultParsed::from_json(&agg)
                .map_err(|e| anyhow!("aggregate parse: {}", e))?;
            let mut md = String::new();
            md.push_str(&format!("# Bench results — directory `{}`\n\n", dir.display()));
            md.push_str("| Bench | median | MAD | mean | stddev | n | outliers |\n");
            md.push_str("|---|---|---|---|---|---|---|\n");
            for b in &parsed.benches {
                let st = &b.stats_ns;
                md.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} |\n",
                    b.raw.name,
                    super::report::fmt_duration(st.median),
                    super::report::fmt_duration(st.mad),
                    super::report::fmt_duration(st.mean),
                    super::report::fmt_duration(st.stddev),
                    st.n,
                    st.outliers_low + st.outliers_high));
            }
            std::fs::write(out_path, md)
                .map_err(|e| anyhow!("write aggregated MD: {}", e))?;
            eprintln!("nova bench: wrote aggregated markdown to {}", out_path.display());
        }
    }
    if let Some(out_path) = opts.out_criterion {
        if !per_file_json_paths.is_empty() {
            let agg = aggregate_json_files(&per_file_json_paths, dir)?;
            let benches: Vec<super::schema::AnalyzedBench> =
                super::schema::RunResultParsed::from_json(&agg)
                    .map_err(|e| anyhow!("aggregate parse: {}", e))?
                    .benches;
            let n = super::criterion_compat::write_all(out_path, &benches)?;
            eprintln!("nova bench: wrote aggregated Criterion-compat layout to {} ({} benches)",
                out_path.display(), n);
        }
    }

    // Cleanup temp aggregation dir.
    if let Some(d) = agg_tmp_dir {
        if !opts.keep_artifacts {
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    Ok(0)
}

/// Plan 57.D.3: merge multiple per-file JSON results в один aggregated JSON.
/// Metadata взята из первого file; benches concatenated. Format совместим
/// с single-file output (`RunResultParsed::from_json` accepts).
fn aggregate_json_files(paths: &[PathBuf], dir: &Path) -> Result<serde_json::Value> {
    use serde_json::{json, Value};
    let mut first_meta: Option<Value> = None;
    let mut all_benches: Vec<Value> = Vec::new();
    for p in paths {
        let text = std::fs::read_to_string(p)
            .map_err(|e| anyhow!("read {}: {}", p.display(), e))?;
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| anyhow!("parse {}: {}", p.display(), e))?;
        if first_meta.is_none() {
            if let Some(m) = v.get("metadata") {
                first_meta = Some(m.clone());
            }
        }
        if let Some(arr) = v.get("benches").and_then(|x| x.as_array()) {
            for b in arr {
                all_benches.push(b.clone());
            }
        }
    }
    // Wrap первой метаданной в верхнеуровневый объект; добавляем
    // aggregation marker (для downstream tooling).
    let mut meta = first_meta.unwrap_or(json!({}));
    if let Some(m) = meta.as_object_mut() {
        m.insert("aggregated_from_directory".to_string(),
            json!(dir.display().to_string()));
        m.insert("aggregated_files_count".to_string(), json!(paths.len()));
    }
    Ok(json!({
        "format_version": super::SCHEMA_VERSION,
        "metadata": meta,
        "benches": all_benches,
    }))
}

/// Walk all .nv files recursively (skip hidden dirs + corpus/).
fn walk_nv(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for ent in entries.flatten() {
        let p = ent.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with('.') { continue; }
        if p.is_dir() {
            // Skip corpus/ — это compiler-perf, не runtime benches.
            if name == "corpus" { continue; }
            walk_nv(&p, out);
        } else if name.ends_with(".nv") {
            out.push(p);
        }
    }
}

/// Plan 57.B.3 / 57.B.5: expand parameterized + grouped bench sweeps
/// в N separate plain BenchDecl entries — runs ДО type-check для
/// name-resolution validity.
pub fn expand_bench_sweeps(module: &mut nova_codegen::ast::Module) {
    use nova_codegen::ast::{Item, BenchDecl, Stmt, LetDecl, Expr, ExprKind, Pattern};
    let mut new_items = Vec::with_capacity(module.items.len());
    for it in module.items.drain(..) {
        match it {
            // Plan 57.B.5: groups → flat entries с composite names.
            Item::Bench(b) if !b.groups.is_empty() => {
                for grp in &b.groups {
                    for case in &grp.cases {
                        let composite = format!("{}/{}/{}", b.name, grp.name, case.name);
                        new_items.push(Item::Bench(BenchDecl {
                            name: composite,
                            setup: case.setup.clone(),
                            measure_body: case.measure_body.clone(),
                            teardown: case.teardown.clone(),
                            params: None,
                            groups: Vec::new(),
                            span: case.span,
                        }));
                    }
                }
            }
            // Plan 57.B.3: params → flat entries с `let n = <v>;` prepended.
            Item::Bench(b) if b.params.is_some() => {
                let params = b.params.unwrap();
                for v in &params.values {
                    let int_lit = Expr {
                        kind: ExprKind::IntLit(*v),
                        span: params.span,
                    };
                    let let_stmt = Stmt::Let(LetDecl {
                        mutable: false,
                        pattern: Pattern::Ident {
                            name: params.var_name.clone(),
                            span: params.span,
                        },
                        ty: None,
                        value: int_lit,
                        span: params.span,
                        is_ghost: false,
                        consume: false,
                    });
                    let mut new_setup = vec![let_stmt];
                    for s in &b.setup {
                        new_setup.push(s.clone());
                    }
                    new_items.push(Item::Bench(BenchDecl {
                        name: format!("{}/p={}", b.name, v),
                        setup: new_setup,
                        measure_body: b.measure_body.clone(),
                        teardown: b.teardown.clone(),
                        params: None,
                        groups: Vec::new(),
                        span: b.span,
                    }));
                }
            }
            other => new_items.push(other),
        }
    }
    module.items = new_items;
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
