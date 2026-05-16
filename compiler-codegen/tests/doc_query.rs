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

// Plan 45 Ф.32.2 — query через pre-generated JSON input.

#[test]
fn query_json_input_basic() {
    let tree = build_tree(SRC);
    let json_str = doc::render_json(&tree);
    let json = doc::json_parse::parse(&json_str).expect("parse");
    let q = parse_query("kind=fn").unwrap();
    let results = doc::query::execute_json(&json, &q);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"add"));
    assert!(names.contains(&"subtract"));
    assert!(!names.contains(&"Counter"));
}

#[test]
fn query_json_input_name_filter() {
    let tree = build_tree(SRC);
    let json_str = doc::render_json(&tree);
    let json = doc::json_parse::parse(&json_str).expect("parse");
    let q = parse_query("name=add").unwrap();
    let results = doc::query::execute_json(&json, &q);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "add");
}

#[test]
fn query_json_and_native_equivalent() {
    // Same query через native DocTree vs via JSON intermediate — same results.
    let tree = build_tree(SRC);
    let q = parse_query("kind=fn").unwrap();
    let native = doc::query::execute(&tree, &q);

    let json_str = doc::render_json(&tree);
    let json = doc::json_parse::parse(&json_str).expect("parse");
    let via_json = doc::query::execute_json(&json, &q);

    assert_eq!(native.len(), via_json.len(),
        "native and JSON-routed queries должны return same count");
    let n_ids: Vec<&str> = native.iter().map(|r| r.item_id.as_str()).collect();
    let j_ids: Vec<&str> = via_json.iter().map(|r| r.item_id.as_str()).collect();
    assert_eq!(n_ids, j_ids, "item_ids должны match");
}

#[test]
fn query_json_invalid_input_errors() {
    let r = doc::json_parse::parse("not valid json {");
    assert!(r.is_err());
}

#[test]
fn query_json_missing_items_array_returns_empty() {
    // Schema-shape mismatch: object без "items" key — empty results.
    let json = doc::json_parse::parse("{\"foo\": \"bar\"}").unwrap();
    let q = parse_query("kind=fn").unwrap();
    let results = doc::query::execute_json(&json, &q);
    assert!(results.is_empty());
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
