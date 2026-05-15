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

const SINGLE_FILE_BUDGET_MS: u128 = 800; // 4x slack для CI (200ms target)
const WORKSPACE_BUDGET_MS: u128 = 12_000; // 4x slack для CI (3s target)

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("nova_tests/doc/fixtures")
}

fn parse_and_build(src: &str) -> nova_codegen::doc::DocTree {
    let mut module = nova_codegen::parser::parse(src).expect("parse");
    let _ = nova_codegen::types::check_module(&module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::doc::build(&module)
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

#[test]
fn perf_single_file_200_loc() {
    let src = synthesize_source(20); // ≈200 LOC
    // Warm-up.
    let _ = parse_and_build(&src);
    // Measured run (среднее по 3 итерациям — для уменьшения шума).
    let start = Instant::now();
    for _ in 0..3 {
        let tree = parse_and_build(&src);
        let _ = nova_codegen::doc::render_json(&tree);
    }
    let elapsed_ms = start.elapsed().as_millis() / 3;
    eprintln!("perf single-file ({}-fn, ≈200 LOC): {} ms (budget {})",
        20, elapsed_ms, SINGLE_FILE_BUDGET_MS);
    assert!(
        elapsed_ms < SINGLE_FILE_BUDGET_MS,
        "perf regression: {} ms > {} ms (Plan 45 §14.5)",
        elapsed_ms, SINGLE_FILE_BUDGET_MS
    );
}

#[test]
fn perf_workspace_50_modules() {
    // 50 modules × 4 fn = 200 fn total (close to real std/ scale).
    let mut modules: Vec<nova_codegen::ast::Module> = Vec::with_capacity(50);
    for i in 0..50 {
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
        let mut m = nova_codegen::parser::parse(&src).expect("parse");
        let _ = nova_codegen::types::check_module(&m);
        nova_codegen::types::infer_effects(&mut m);
        modules.push(m);
    }
    let start = Instant::now();
    let tree = nova_codegen::doc::build_workspace(&modules);
    let _ = nova_codegen::doc::render_json(&tree);
    let elapsed_ms = start.elapsed().as_millis();
    eprintln!("perf workspace (50 modules, 200 fn): {} ms (budget {})",
        elapsed_ms, WORKSPACE_BUDGET_MS);
    assert!(
        elapsed_ms < WORKSPACE_BUDGET_MS,
        "perf regression: {} ms > {} ms (Plan 45 §14.5)",
        elapsed_ms, WORKSPACE_BUDGET_MS
    );
}

#[test]
fn perf_real_fixtures_combined() {
    // Sanity: 8 real fixtures + render ≈ instantaneous (sanity check).
    let names = ["basic", "sections", "kinds", "links", "orphan", "doctests", "stability", "real_attrs"];
    let start = Instant::now();
    for name in &names {
        let path = fixtures_root().join(name).join("sample.nv");
        let src = std::fs::read_to_string(&path).unwrap();
        let tree = parse_and_build(&src);
        let _ = nova_codegen::doc::render_json(&tree);
    }
    let elapsed_ms = start.elapsed().as_millis();
    eprintln!("perf 8 real fixtures: {} ms", elapsed_ms);
    // Очень слабый budget — 2 секунды на 8 small fixtures.
    assert!(elapsed_ms < 2000, "8 small fixtures took {} ms — investigate", elapsed_ms);
}
