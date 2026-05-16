//! Plan 45 Ф.29.4 — workspace mutation testing real-exec integration tests.
//!
//! Verify что `run_mutation_analysis_executed_workspace` corrigly resolves
//! per-module source через `sources_by_module_path` map и runs doc-tests
//! с мутированным source своего file.

use nova_codegen::doc;
use nova_codegen::doc::mutation::{MutantOutcome, run_mutation_analysis_executed_workspace};
use nova_codegen::parser;
use nova_codegen::types;

fn build_workspace_with_sources(
    files: &[(&str, &str)], // [(module_path, source), ...]
) -> (doc::DocTree, std::collections::BTreeMap<String, String>) {
    let mut modules: Vec<nova_codegen::ast::Module> = Vec::new();
    let mut sources_map: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (path, src) in files {
        let mut module = parser::parse(src).expect("parse");
        let _ = types::check_module(&module);
        types::infer_effects(&mut module);
        sources_map.insert(path.to_string(), src.to_string());
        modules.push(module);
    }
    let tree = doc::build_workspace(&modules);
    (tree, sources_map)
}

#[test]
fn workspace_mutation_no_contracts_zero_mutants() {
    let module_a = r#"
module a

export fn helper(x int) -> int => x * 2
"#;
    let module_b = r#"
module b

export fn other(x int) -> int => x + 1
"#;
    let (tree, sources) = build_workspace_with_sources(&[
        ("a", module_a),
        ("b", module_b),
    ]);
    let report = run_mutation_analysis_executed_workspace(&tree, &sources);
    assert_eq!(report.total, 0, "no contracts → no mutants");
}

#[test]
fn workspace_mutation_no_doctests_outcome_no_tests() {
    let module_a = r#"
module a

export fn safe(x int) -> int
    requires x >= 0
    ensures result == x
    => x
"#;
    let (tree, sources) = build_workspace_with_sources(&[
        ("a", module_a),
    ]);
    let report = run_mutation_analysis_executed_workspace(&tree, &sources);
    // Все мутанты NoTests (нет doc-tests).
    for m in &report.mutants {
        assert_eq!(m.outcome, MutantOutcome::NoTests,
            "expected NoTests без doc-tests, got: {:?}", m);
    }
}

#[test]
fn workspace_mutation_resolves_per_module_source() {
    // Module A имеет fn с contracts, module B — другие fn без contracts.
    // Mutation должна найти correct source через module_path.
    let module_a = r#"
module a

export fn pos(x int) -> int
    requires x > 0
    => x
"#;
    let module_b = r#"
module b

export fn helper(x int) -> int => x + 10
"#;
    let (tree, sources) = build_workspace_with_sources(&[
        ("a", module_a),
        ("b", module_b),
    ]);
    let report = run_mutation_analysis_executed_workspace(&tree, &sources);
    // Мутанты только для module a (где есть contracts).
    assert!(!report.mutants.is_empty(),
        "expected mutants для `requires x > 0` в module a");
    for m in &report.mutants {
        assert!(m.item_id.starts_with("a::"),
            "mutant item_id должен быть из module a, got: {}", m.item_id);
    }
}

#[test]
fn workspace_mutation_deterministic() {
    let module_a = r#"
module a

export fn f(x int) -> int
    requires x > 0
    ensures result == x
    => x
"#;
    let (tree, sources) = build_workspace_with_sources(&[
        ("a", module_a),
    ]);
    let first = run_mutation_analysis_executed_workspace(&tree, &sources);
    let second = run_mutation_analysis_executed_workspace(&tree, &sources);
    assert_eq!(first.mutants, second.mutants,
        "workspace mutation must be deterministic");
}

#[test]
fn workspace_mutation_skips_unknown_module() {
    // Если item_id ссылается на module которого нет в sources_by_module_path,
    // mutant получает NoTests (а не panic).
    let module_a = r#"
module a

export fn f(x int) -> int
    requires x > 0
    => x
"#;
    let (tree, _sources_dummy) = build_workspace_with_sources(&[
        ("a", module_a),
    ]);
    // Передаём empty sources map — все мутанты должны быть NoTests.
    let empty_sources = std::collections::BTreeMap::new();
    let report = run_mutation_analysis_executed_workspace(&tree, &empty_sources);
    for m in &report.mutants {
        assert_eq!(m.outcome, MutantOutcome::NoTests,
            "unknown module → NoTests (no panic), got: {:?}", m);
    }
}
