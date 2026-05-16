//! Plan 45 Ф.28.2 — real-exec mutation testing integration tests.
//!
//! Verify что real-exec evaluator actually runs mutated doc-tests
//! (vs textual heuristic Ф.25.4) и правильно классифицирует killed/survived.

use nova_codegen::doc;
use nova_codegen::doc::mutation::{MutantOutcome, run_mutation_analysis_executed};
use nova_codegen::parser;
use nova_codegen::types;

fn build_tree(src: &str) -> doc::DocTree {
    let mut module = parser::parse(src).expect("parse");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    doc::build(&module)
}

#[test]
fn fn_without_doctests_has_no_tests_outcome() {
    // Mutate работает только если есть doc-tests. Без них — NoTests.
    let src = r#"
module m

export fn pos(x int) -> int
    requires x > 0
    ensures result == x
    => x
"#;
    let tree = build_tree(src);
    let report = run_mutation_analysis_executed(&tree, src);
    // Все мутанты должны иметь NoTests (no doc-tests).
    for m in &report.mutants {
        assert_eq!(m.outcome, MutantOutcome::NoTests,
            "expected NoTests без doc-tests, got: {:?}", m);
    }
}

#[test]
fn report_includes_mutants_for_contracted_fn() {
    // Sanity check: pretty-print expression передаётся в mutator.
    // После Ф.28.1 contract.expr может быть parenthesized: `(x > 0)`.
    let src = r#"
module m

export fn pos(x int) -> int
    requires x > 0
    => x
"#;
    let tree = build_tree(src);
    let report = run_mutation_analysis_executed(&tree, src);
    // Должны иметь некоторые мутанты для `requires x > 0`.
    assert!(!report.mutants.is_empty(),
        "expected mutants для функции с `requires`, got 0");
}

#[test]
fn real_exec_is_deterministic() {
    // Multiple runs одного и того же кода дают identical reports.
    let src = r#"
module m

export fn safe(x int) -> int
    requires x >= 0
    ensures result == x
    => x
"#;
    let tree = build_tree(src);
    let first = run_mutation_analysis_executed(&tree, src);
    let second = run_mutation_analysis_executed(&tree, src);
    assert_eq!(first.total, second.total);
    assert_eq!(first.killed, second.killed);
    assert_eq!(first.survived, second.survived);
    assert_eq!(first.mutants, second.mutants);
}

#[test]
fn empty_contracts_zero_mutants() {
    let src = r#"
module m

export fn pure_fn(x int) -> int => x * 2
"#;
    let tree = build_tree(src);
    let report = run_mutation_analysis_executed(&tree, src);
    assert_eq!(report.total, 0,
        "no contracts → no mutants, got: {:?}", report);
}
