//! Plan 45 Ф.26.3 / D63 — allow_transit capability field.
//!
//! Verifies что Capabilities.allow_transit поле существует в DocItem,
//! рендерится в JSON (alphabetically первое в capabilities object), и
//! по умолчанию empty array (parser ещё не поддерживает `#allow_transit`).

use nova_codegen::doc;
use nova_codegen::parser;
use nova_codegen::types;

fn build_tree(src: &str) -> doc::DocTree {
    let mut module = parser::parse(src).expect("parse");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    doc::build(&module)
}

#[test]
fn capabilities_have_allow_transit_field() {
    let src = r#"
module m

/// Simple function.
export fn id(x int) -> int => x
"#;
    let tree = build_tree(src);
    // First (and only) item.
    let cap = &tree.modules[0].items[0].capabilities;
    assert!(cap.allow_transit.is_empty(),
        "allow_transit должно быть empty по умолчанию (parser не имеет attr support)");
}

#[test]
fn json_includes_allow_transit_array() {
    let src = r#"
module m

/// Function.
export fn f() -> int => 1
"#;
    let tree = build_tree(src);
    let json = doc::render_json(&tree);
    assert!(json.contains("\"allow_transit\":"),
        "JSON capabilities должно содержать allow_transit array. JSON: {}", json);
}

#[test]
fn allow_transit_alphabetically_before_forbid() {
    let src = r#"
module m

/// Function.
export fn f() -> int => 1
"#;
    let tree = build_tree(src);
    let json = doc::render_json(&tree);
    // Within capabilities object: allow_transit (a) < forbid (f).
    let at_pos = json.find("\"allow_transit\":").expect("allow_transit present");
    let fb_pos = json.find("\"forbid\":").expect("forbid present");
    assert!(at_pos < fb_pos, "allow_transit must precede forbid alphabetically");
}

#[test]
fn manually_set_allow_transit_renders() {
    let src = r#"
module m

/// Function.
export fn f() -> int => 1
"#;
    let mut tree = build_tree(src);
    // Manually populate (future parser will do this from `#allow_transit X` attr).
    tree.modules[0].items[0].capabilities.allow_transit = vec!["Log".to_string(), "Metrics".to_string()];
    let json = doc::render_json(&tree);
    assert!(json.contains("\"Log\""), "manually-set Log effect должен render");
    assert!(json.contains("\"Metrics\""), "manually-set Metrics effect должен render");
}

#[test]
fn md_renders_allow_transit_badge() {
    let src = r#"
module m

/// Function.
export fn f() -> int => 1
"#;
    let mut tree = build_tree(src);
    tree.modules[0].items[0].capabilities.allow_transit = vec!["Log".to_string()];
    let md = doc::render_markdown(&tree);
    assert!(md.contains("📤 `allow_transit(Log)`"),
        "MD должна показать allow_transit badge. MD: {}", md);
}
