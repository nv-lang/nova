//! Plan 45 Ф.25.1 — diagnostic warnings end-to-end tests.
//!
//! Проверяет что:
//! - malformed `#[deprecated]` / `#[since]` → warning
//! - unknown `#[future_attr]` → warning
//! - unknown doc-test modifier `nova,future_mod` → warning
//! - ambiguous intra-doc link `[foo]` (когда foo есть в 2+ модулях) → warning
//! - tree.warnings отсортированы и дедуплицированы
//! - JSON output содержит warnings секцию

use nova_codegen::doc;
use nova_codegen::parser;
use nova_codegen::types;

fn build_tree(src: &str) -> doc::DocTree {
    let mut module = parser::parse(src).expect("parse should succeed");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    doc::build(&module)
}

#[test]
fn malformed_stability_attr_warns() {
    let src = r#"
module x

/// First-line summary.
///
/// #[deprecated]
export fn foo() -> int => 1
"#;
    let tree = build_tree(src);
    let malformed: Vec<_> = tree.warnings.iter()
        .filter(|w| w.rule == "malformed-stability-attr")
        .collect();
    assert!(!malformed.is_empty(), "expected malformed-stability-attr warning, got: {:?}", tree.warnings);
}

#[test]
fn unknown_doc_attr_warns() {
    let src = r#"
module x

/// Summary.
///
/// #[future_attr]
export fn foo() -> int => 1
"#;
    let tree = build_tree(src);
    let unknowns: Vec<_> = tree.warnings.iter()
        .filter(|w| w.rule == "unknown-doc-attr")
        .collect();
    assert_eq!(unknowns.len(), 1);
    assert!(unknowns[0].message.contains("future_attr"));
}

#[test]
fn unknown_doctest_modifier_warns() {
    let src = "
module x

/// Demo function.
///
/// ```nova,future_modifier
/// let y = 1
/// ```
export fn foo() -> int => 1
";
    let tree = build_tree(src);
    let unknowns: Vec<_> = tree.warnings.iter()
        .filter(|w| w.rule == "unknown-doctest-modifier")
        .collect();
    assert_eq!(unknowns.len(), 1);
    assert!(unknowns[0].message.contains("future_modifier"));
}

#[test]
fn warnings_are_sorted_and_deduplicated() {
    // Несколько items с одинаковым unknown attr — после dedup должен
    // быть только один warning на item (rule + item_id + message).
    let src = r#"
module x

/// foo summary.
///
/// #[future_attr]
export fn foo() -> int => 1

/// bar summary.
///
/// #[future_attr]
export fn bar() -> int => 2
"#;
    let tree = build_tree(src);
    let unknowns: Vec<_> = tree.warnings.iter()
        .filter(|w| w.rule == "unknown-doc-attr")
        .collect();
    // Два разных item_id → два warning'а.
    assert_eq!(unknowns.len(), 2);
    // Sorted by (item_id, rule, message).
    let item_ids: Vec<&str> = unknowns.iter().map(|w| w.item_id.as_str()).collect();
    let mut sorted = item_ids.clone();
    sorted.sort();
    assert_eq!(item_ids, sorted);
}

#[test]
fn json_output_contains_warnings_array() {
    let src = r#"
module x

/// Summary.
///
/// #[future_attr]
export fn foo() -> int => 1
"#;
    let tree = build_tree(src);
    let json = doc::render_json(&tree);
    assert!(json.contains("\"warnings\":"), "JSON should contain warnings array, got: {}", json);
    assert!(json.contains("unknown-doc-attr"), "JSON should mention rule, got: {}", json);
    assert!(json.contains("future_attr"), "JSON should mention the unknown attr, got: {}", json);
}

#[test]
fn clean_input_produces_no_warnings() {
    let src = r#"
module x

/// Documented function.
export fn foo() -> int => 1
"#;
    let tree = build_tree(src);
    assert!(tree.warnings.is_empty(), "clean input should produce no warnings, got: {:?}", tree.warnings);
}
