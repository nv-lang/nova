//! Plan 45 Ф.26.1 — Newtype variant correctness test.
//!
//! Regression guard: ранее в collector.rs было два match arm'а для
//! `TypeDeclKind::Newtype` — первый (MVP) переводил в Alias, второй (новый)
//! в правильный Newtype variant. Rust match-семантика гарантировала что
//! первый arm всегда срабатывал → second был dead code → spec violation D107.
//!
//! Этот test проверяет что `type Email = newtype str` → JSON `definition.kind = "newtype"`,
//! не `"alias"`.

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
fn newtype_renders_as_newtype_not_alias() {
    let src = r#"
module x

/// Validated email address.
export type Email str
"#;
    let tree = build_tree(src);
    let json = doc::render_json(&tree);
    // Critical: должен быть kind "newtype", не "alias".
    assert!(json.contains("\"kind\": \"newtype\""),
        "Newtype должен рендериться как kind:\"newtype\", не \"alias\" (D107 spec). JSON: {}", json);
    // И inner_type, не aliased_type.
    assert!(json.contains("\"inner_type\":"),
        "Newtype JSON должен иметь поле inner_type, не aliased_type. JSON: {}", json);
    assert!(!json.contains("\"aliased_type\": \"str\""),
        "Newtype НЕ должен иметь aliased_type. JSON: {}", json);
}

#[test]
fn alias_renders_as_alias() {
    let src = r#"
module x

/// String alias (not newtype).
export type Name alias str
"#;
    let tree = build_tree(src);
    let json = doc::render_json(&tree);
    // Alias всё ещё рендерится как alias.
    assert!(json.contains("\"kind\": \"alias\""),
        "Plain alias должен рендериться как kind:\"alias\". JSON: {}", json);
    assert!(json.contains("\"aliased_type\":"),
        "Alias JSON должен иметь поле aliased_type. JSON: {}", json);
}

#[test]
fn newtype_and_alias_coexist_distinctly() {
    let src = r#"
module x

/// Newtype wrapper.
export type Email str

/// Plain alias.
export type UserName alias str
"#;
    let tree = build_tree(src);
    let json = doc::render_json(&tree);
    // Оба kinds присутствуют distinct.
    assert!(json.contains("\"kind\": \"newtype\""));
    assert!(json.contains("\"kind\": \"alias\""));
}
