//! Plan 45 Ф.25.4 — doc-test mutation testing integration tests.

use nova_codegen::doc;
use nova_codegen::doc::mutation::{MutantOutcome, run_mutation_analysis};
use nova_codegen::parser;
use nova_codegen::types;

fn build_tree(src: &str) -> doc::DocTree {
    let mut module = parser::parse(src).expect("parse");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    doc::build(&module)
}

#[test]
fn no_contracts_produces_no_mutants() {
    let src = "
module x

/// No contracts here.
export fn id(x int) -> int => x
";
    let tree = build_tree(src);
    let report = run_mutation_analysis(&tree);
    assert_eq!(report.total, 0);
}

#[test]
fn requires_generates_mutants() {
    let src = "
module x

/// Absolute value.
export fn abs_val(x int) -> int
    requires x > 0
    => x
";
    let tree = build_tree(src);
    let report = run_mutation_analysis(&tree);
    // `x > 0` → `x >= 0` (gt-to-ge) + drop-requires.
    assert!(report.total >= 2, "expected ≥2 mutants for `requires x > 0`, got {}: {:?}",
        report.total, report.mutants);
    assert!(report.mutants.iter().any(|m| m.operator == "gt-to-ge"));
    assert!(report.mutants.iter().any(|m| m.operator == "drop-requires"));
}

#[test]
fn ensures_generates_drop_ensures_mutant() {
    // Plan 45 Ф.29.3: drop-ensures теперь generated (раньше только drop-requires).
    let src = "
module x

/// Add one — postcondition only.
export fn inc(x int) -> int
    ensures result == x + 1
    => x + 1
";
    let tree = build_tree(src);
    let report = run_mutation_analysis(&tree);
    // `result == x + 1` → `result != x + 1` (eq-to-ne).
    assert!(report.mutants.iter().any(|m| m.operator == "eq-to-ne"));
    // Plan 45 Ф.29.3: drop-ensures mutator теперь существует.
    assert!(report.mutants.iter().any(|m| m.operator == "drop-ensures"),
        "expected drop-ensures mutant для ensures-contract, got: {:?}",
        report.mutants.iter().map(|m| m.operator.as_str()).collect::<Vec<_>>());
    // No drop-requires (это ensures fn без requires).
    assert!(!report.mutants.iter().any(|m| m.operator == "drop-requires"));
}

#[test]
fn fn_with_both_requires_and_ensures_gets_both_drops() {
    let src = "
module x

export fn safe_inc(x int) -> int
    requires x >= 0
    ensures result == x + 1
    => x + 1
";
    let tree = build_tree(src);
    let report = run_mutation_analysis(&tree);
    assert!(report.mutants.iter().any(|m| m.operator == "drop-requires"));
    assert!(report.mutants.iter().any(|m| m.operator == "drop-ensures"));
}

#[test]
fn no_doc_tests_means_no_tests_outcome() {
    let src = "
module x

/// No doc-tests, just contract.
export fn pos(x int) -> int
    requires x > 0
    => x
";
    let tree = build_tree(src);
    let report = run_mutation_analysis(&tree);
    // Все мутанты должны иметь outcome=NoTests.
    for m in &report.mutants {
        assert_eq!(m.outcome, MutantOutcome::NoTests,
            "expected NoTests for mutant {:?} (no doc-tests), got {:?}",
            m, m.outcome);
    }
}

#[test]
fn mutants_are_deterministic() {
    // Запускаем 5 раз — должны получить identical mutant lists.
    let src = "
module x

/// Test fn.
export fn f(x int) -> int
    requires x > 0
    ensures result >= 0
    => x
";
    let tree = build_tree(src);
    let first = run_mutation_analysis(&tree);
    for _ in 0..4 {
        let next = run_mutation_analysis(&tree);
        assert_eq!(first.mutants, next.mutants, "mutation analysis must be deterministic");
    }
}

#[test]
fn report_counts_correctly() {
    let src = "
module x

/// A.
export fn a(x int) -> int
    requires x > 0
    => x

/// B.
export fn b(y int) -> int
    requires y >= 5
    => y
";
    let tree = build_tree(src);
    let report = run_mutation_analysis(&tree);
    assert_eq!(report.total, report.mutants.len());
    // Without doc-tests — все мутанты NoTests, not survived/killed.
    assert_eq!(report.survived, 0);
    assert_eq!(report.killed, 0);
}
