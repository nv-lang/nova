//! Plan 45 Ф.32.1 — query DSL integration tests.

use nova_codegen::doc;
use nova_codegen::doc::query::{parse_query, execute, render_results_json};
use nova_codegen::parser;
use nova_codegen::types;

fn build_tree(src: &str) -> doc::DocTree {
    let mut module = parser::parse(src).expect("parse");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    doc::build(&module)
}

const SRC: &str = r#"
module m

/// First fn.
export fn add(a int, b int) -> int => a + b

/// Second fn.
export fn subtract(a int, b int) -> int => a - b

/// Type.
export type Counter int

/// Constant.
export const PI int = 314
"#;

#[test]
fn query_by_kind_fn() {
    let tree = build_tree(SRC);
    let q = parse_query("kind=fn").unwrap();
    let results = execute(&tree, &q);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"add"));
    assert!(names.contains(&"subtract"));
    assert!(!names.contains(&"Counter"));
    assert!(!names.contains(&"PI"));
}

#[test]
fn query_by_name_substring() {
    let tree = build_tree(SRC);
    let q = parse_query("name=add").unwrap();
    let results = execute(&tree, &q);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "add");
}

#[test]
fn query_combined_kind_and_name() {
    let tree = build_tree(SRC);
    let q = parse_query("kind=fn,name=sub").unwrap();
    let results = execute(&tree, &q);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "subtract");
}

#[test]
fn query_by_module_exact() {
    let tree = build_tree(SRC);
    let q = parse_query("module=m").unwrap();
    let results = execute(&tree, &q);
    assert!(!results.is_empty());
    let q_wrong = parse_query("module=other").unwrap();
    let r2 = execute(&tree, &q_wrong);
    assert!(r2.is_empty());
}

#[test]
fn query_by_module_prefix() {
    let tree = build_tree(SRC);
    let q = parse_query("module-prefix=m").unwrap();
    let results = execute(&tree, &q);
    assert!(!results.is_empty());
}

#[test]
fn query_kind_type_only() {
    let tree = build_tree(SRC);
    let q = parse_query("kind=type").unwrap();
    let results = execute(&tree, &q);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"Counter"));
    assert!(!names.contains(&"add"));
}

#[test]
fn query_kind_const_only() {
    let tree = build_tree(SRC);
    let q = parse_query("kind=const").unwrap();
    let results = execute(&tree, &q);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"PI"));
}

#[test]
fn query_render_json_format() {
    let tree = build_tree(SRC);
    let q = parse_query("kind=fn,name=add").unwrap();
    let results = execute(&tree, &q);
    let json = render_results_json(&results);
    assert!(json.starts_with("[\n"));
    assert!(json.contains("\"item_id\""));
    assert!(json.contains("\"name\": \"add\""));
    assert!(json.contains("\"kind\": \"fn\""));
}

#[test]
fn query_empty_returns_all_items() {
    let tree = build_tree(SRC);
    let q = parse_query("").unwrap();
    let results = execute(&tree, &q);
    assert!(!results.is_empty());
    // Все 4 items должны быть.
    assert!(results.len() >= 4);
}

#[test]
fn query_has_contracts_filter() {
    let src = r#"
module m

/// With contracts.
export fn safe(x int) -> int
    requires x >= 0
    ensures result >= 0
    => x

/// Without contracts.
export fn nocontract(x int) -> int => x
"#;
    let tree = build_tree(src);
    let q = parse_query("has-contracts=true").unwrap();
    let results = execute(&tree, &q);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"safe"));
    assert!(!names.contains(&"nocontract"));
}

#[test]
fn query_results_sorted_by_id() {
    let src = r#"
module m

export fn zebra() -> int => 1
export fn alpha() -> int => 2
export fn middle() -> int => 3
"#;
    let tree = build_tree(src);
    let q = parse_query("kind=fn").unwrap();
    let results = execute(&tree, &q);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "middle", "zebra"],
        "results должны быть sorted by item_id");
}
