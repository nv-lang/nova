//! Plan 45 Ф.21.9 — performance bench (§14.5 wall-clock targets).
//!
//! Минимальная измерительная инфраструктура без `criterion` (heavy
//! dep). Цель: catch regressions, не precise micro-benches.
//!
//! Targets (Plan 45 §14.5, MVP):
//! - `nova doc <single-file>` (~200 LOC) ≤ 200 ms
//! - workspace на 50 модулях ≤ 3 s
//!
//! Учитываем что тест запускается в `cargo test --release`. Local
//! dev-machine референс — i7-12700H. Slow CI runner может иметь 2-3x
//! более slow times — поэтому targets relaxed для CI (4x).
//!
//! Если perf падает значительно ниже target — это сигнал к
//! investigation. Test fail вызывает acknowledgement.

use std::path::PathBuf;
use std::time::Instant;

// Registry #1159/#1154 (2026-09-30): these three tests used to assert an
// ABSOLUTE wall-clock budget (SINGLE_FILE_BUDGET_MS / WORKSPACE_BUDGET_MS /
// a bare `2000`). That judges the MACHINE running the test, not the doc
// pipeline — the sibling defect caught the same class red under gate load
// and green alone with a 19x margin (nova-lsp `rename::edge_very_long_file`).
// Fix, applied here too: measure at two sizes BACK-TO-BACK IN THE SAME RUN
// and assert the cost RATIO stays near the size ratio; load inflates both
// arms together so the ratio holds even when the raw milliseconds do not,
// and an O(n^2)-or-worse regression still reddens because it pushes the
// ratio well past what linear growth would produce.

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("nova_tests/doc/fixtures")
}

// №655: компилирующий блок — на нити с гарантированным стеком; на нити
// cargo-test (Linux, мало) parse/check переполняются. Обёртка снаружи
// замера времени не живёт — она внутри хелпера, и её цена (~доли мс на
// spawn) тонет в бюджетах 800/12000 мс.
fn parse_and_build(src: &str) -> nova_codegen::doc::DocTree {
    nova_codegen::testing::on_compiler_stack(|| {
        let mut module = nova_codegen::parser::parse(src).expect("parse");
        let _ = nova_codegen::types::check_module(&module);
        nova_codegen::types::infer_effects(&mut module);
        nova_codegen::doc::build(&module)
    })
}

/// Синтезирует source с N экспортированных функций — каждая с
/// doc-comment'ом, sections, intra-doc-link. ≈10 LOC на fn → 200 LOC
/// при N=20.
fn synthesize_source(n: usize) -> String {
    let mut s = String::from(
        "//! Synthetic perf benchmark fixture.\n\nmodule bench_synth\n\n",
    );
    for i in 0..n {
        s.push_str(&format!(
            "/// Function `f_{}` — does something useful.\n\
             ///\n\
             /// # Examples\n\
             ///\n\
             /// ```nova\n\
             /// assert(f_{}(1) == 2)\n\
             /// ```\n\
             ///\n\
             /// See [f_{}] for related behavior.\n\
             export fn f_{}(x int) -> int => x + 1\n\n",
            i, i, (i + 1) % n, i,
        ));
    }
    s
}

/// Median-of-3 wall time (ms) to parse+check+build+render `n` synthetic
/// exported functions. Warms up once before measuring (module-compile
/// caches, allocator, etc.) so the timed loop isolates the per-call cost.
fn measure_single_file_ms(n: usize) -> f64 {
    let src = synthesize_source(n);
    let _ = parse_and_build(&src); // warm-up
    let mut best = u128::MAX;
    for _ in 0..3 {
        let start = Instant::now();
        let tree = parse_and_build(&src);
        let _ = nova_codegen::doc::render_json(&tree);
        best = best.min(start.elapsed().as_millis());
    }
    best as f64
}

#[test]
fn perf_single_file_200_loc() {
    // small=4 fns (≈40 LOC), big=20 fns (≈200 LOC) — a 5x size ratio measured
    // back-to-back in the same run; see the module-level note above.
    let small_ms = measure_single_file_ms(4).max(1.0);
    let big_ms = measure_single_file_ms(20);
    let ratio = big_ms / small_ms;
    eprintln!(
        "perf single-file: 4-fn={} ms, 20-fn={} ms, ratio={:.1} (size ratio 5x)",
        small_ms, big_ms, ratio
    );
    assert!(
        ratio < 15.0,
        "perf regression: cost grew {:.1}x for a 5x larger file (4-fn={} ms, \
         20-fn={} ms) — expected near-linear scaling (~5x); this looks like an \
         O(n^2) regression in the doc pipeline, not machine load (registry #1159, \
         Plan 45 §14.5)",
        ratio, small_ms, big_ms
    );
}

/// Builds `n` synthetic workspace modules (4 fn each — close to real std/
/// scale at n=50) and returns the median-of-2 wall time (ms) to
/// build_workspace + render_json them.
fn measure_workspace_ms(n: usize) -> f64 {
    let mut modules: Vec<nova_codegen::ast::Module> = Vec::with_capacity(n);
    for i in 0..n {
        let src = format!(
            "module bench_workspace.m_{}\n\n\
             /// Fn one.\n\
             export fn one_{}() -> int => 0\n\n\
             /// Fn two.\n\
             export fn two_{}() -> int => 1\n\n\
             /// Fn three.\n\
             export fn three_{}() -> int => 2\n\n\
             /// Fn four.\n\
             export fn four_{}() -> int => 3\n\n",
            i, i, i, i, i,
        );
        // №655: тот же компилирующий блок, та же дверь — при заведении
        // обёртки этот второй сайт был пропущен, и доказательство на 1 МБ
        // нитях поймало пропуск до CI.
        let m = nova_codegen::testing::on_compiler_stack(|| {
            let mut m = nova_codegen::parser::parse(&src).expect("parse");
            let _ = nova_codegen::types::check_module(&m);
            nova_codegen::types::infer_effects(&mut m);
            m
        });
        modules.push(m);
    }
    let mut best = u128::MAX;
    for _ in 0..2 {
        let start = Instant::now();
        let tree = nova_codegen::doc::build_workspace(&modules);
        let _ = nova_codegen::doc::render_json(&tree);
        best = best.min(start.elapsed().as_millis());
    }
    best as f64
}

#[test]
fn perf_workspace_50_modules() {
    // small=10 modules, big=50 modules (close to real std/ scale) — a 5x
    // size ratio measured back-to-back in the same run; see the
    // module-level note above.
    let small_ms = measure_workspace_ms(10).max(1.0);
    let big_ms = measure_workspace_ms(50);
    let ratio = big_ms / small_ms;
    eprintln!(
        "perf workspace: 10-mod={} ms, 50-mod={} ms, ratio={:.1} (size ratio 5x)",
        small_ms, big_ms, ratio
    );
    assert!(
        ratio < 15.0,
        "perf regression: cost grew {:.1}x for a 5x larger workspace (10-mod={} ms, \
         50-mod={} ms) — expected near-linear scaling (~5x); this looks like an \
         O(n^2) regression in build_workspace, not machine load (registry #1159, \
         Plan 45 §14.5)",
        ratio, small_ms, big_ms
    );
}

/// Median-of-3 wall time (ms) to parse+build+render the first `n` of the 8
/// real doc fixtures.
fn measure_fixtures_ms(n: usize) -> f64 {
    let names = ["basic", "sections", "kinds", "links", "orphan", "doctests", "stability", "real_attrs"];
    let srcs: Vec<String> = names[..n]
        .iter()
        .map(|name| std::fs::read_to_string(fixtures_root().join(name).join("sample.nv")).unwrap())
        .collect();
    let mut best = u128::MAX;
    for _ in 0..3 {
        let start = Instant::now();
        for src in &srcs {
            let tree = parse_and_build(src);
            let _ = nova_codegen::doc::render_json(&tree);
        }
        best = best.min(start.elapsed().as_millis());
    }
    best as f64
}

#[test]
fn perf_real_fixtures_combined() {
    // Sanity: 8 real fixtures + render ≈ instantaneous. small=4 fixtures,
    // big=8 fixtures (2x) measured back-to-back in the same run; see the
    // module-level note above. The size ratio here is smaller (2x, not 5x)
    // because there are only 8 fixtures total, so the tolerance below is
    // wider to absorb the fixtures' uneven individual sizes.
    let small_ms = measure_fixtures_ms(4).max(1.0);
    let big_ms = measure_fixtures_ms(8);
    let ratio = big_ms / small_ms;
    eprintln!(
        "perf 8 real fixtures: first-4={} ms, all-8={} ms, ratio={:.1} (size ratio ~2x)",
        small_ms, big_ms, ratio
    );
    assert!(
        ratio < 10.0,
        "perf regression: cost grew {:.1}x for ~2x more fixtures (first-4={} ms, \
         all-8={} ms) — investigate (registry #1159; this used to be an absolute \
         2000ms budget, which judges the machine, not the fixtures)",
        ratio, small_ms, big_ms
    );
}
