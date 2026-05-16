//! Plan 45 Ф.26.2 / Ф.23.4 — handler matrix integration tests.
//!
//! Verify что workspace pass находит `handler <Effect> { ... }` literal'ы и
//! populates `Effect.handlers` с правильными HandlerRef'ами.

use nova_codegen::doc;
use nova_codegen::parser;
use nova_codegen::types;

fn build_tree_with_handlers(src: &str) -> doc::DocTree {
    let mut module = parser::parse(src).expect("parse");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    let mut tree = doc::build(&module);
    doc::populate_handler_matrix(&mut tree, src);
    tree
}

fn find_effect_handlers<'a>(tree: &'a doc::DocTree, name: &str) -> Option<&'a Vec<doc::doctree::HandlerRef>> {
    for m in &tree.modules {
        for it in &m.items {
            if it.name == name {
                if let doc::ItemKind::Effect { handlers, .. } = &it.kind {
                    return Some(handlers);
                }
            }
        }
    }
    None
}

#[test]
fn handler_inline_in_with_registered() {
    let src = r#"
module m

export type Store effect {
    Set(v int)
    #pure get() -> int
}

fn use_store() -> int {
    let mut s = 0
    with #trusted Store = handler Store {
        Set(v) { s = v }
        get() => s
    } {
        Store.Set(5)
        Store.get()
    }
}
"#;
    let tree = build_tree_with_handlers(src);
    let handlers = find_effect_handlers(&tree, "Store")
        .expect("Store effect must be in DocTree");
    assert_eq!(handlers.len(), 1, "expected 1 handler, got: {:?}", handlers);
    assert!(handlers[0].caller_item_id.ends_with("use_store"),
        "caller should be 'use_store', got: {}", handlers[0].caller_item_id);
    assert_eq!(handlers[0].kind, "inline");
}

#[test]
fn no_handlers_means_empty_array() {
    let src = r#"
module m

export type Logger effect {
    log(msg str)
}
"#;
    let tree = build_tree_with_handlers(src);
    let handlers = find_effect_handlers(&tree, "Logger").expect("Logger effect");
    assert!(handlers.is_empty(), "no handlers in source → empty array");
}

#[test]
fn multiple_callers_distinct_entries() {
    let src = r#"
module m

export type Store effect {
    Set(v int)
    #pure get() -> int
}

fn caller_a() -> int {
    let mut s = 0
    with #trusted Store = handler Store {
        Set(v) { s = v }
        get() => s
    } {
        Store.Set(1)
        Store.get()
    }
}

fn caller_b() -> int {
    let mut s = 0
    with #trusted Store = handler Store {
        Set(v) { s = v }
        get() => s
    } {
        Store.Set(2)
        Store.get()
    }
}
"#;
    let tree = build_tree_with_handlers(src);
    let handlers = find_effect_handlers(&tree, "Store").expect("Store effect");
    assert_eq!(handlers.len(), 2, "two callers → two entries, got: {:?}", handlers);
    let ids: Vec<&str> = handlers.iter().map(|h| h.caller_item_id.as_str()).collect();
    // Sorted alphabetically: caller_a < caller_b.
    assert!(ids[0].ends_with("caller_a"));
    assert!(ids[1].ends_with("caller_b"));
}

#[test]
fn handlers_are_deterministic() {
    // Множественные builds дают identical handlers (sorted, dedup'd).
    let src = r#"
module m

export type Store effect {
    Set(v int)
    #pure get() -> int
}

fn use_store() -> int {
    let mut s = 0
    with #trusted Store = handler Store {
        Set(v) { s = v }
        get() => s
    } {
        Store.get()
    }
}
"#;
    let first = build_tree_with_handlers(src);
    let second = build_tree_with_handlers(src);
    let h1 = find_effect_handlers(&first, "Store").unwrap();
    let h2 = find_effect_handlers(&second, "Store").unwrap();
    assert_eq!(h1, h2, "handler matrix should be deterministic");
}

#[test]
fn handler_json_renders_with_kind() {
    let src = r#"
module m

export type Store effect {
    Set(v int)
    #pure get() -> int
}

fn use_store() -> int {
    let mut s = 0
    with #trusted Store = handler Store {
        Set(v) { s = v }
        get() => s
    } {
        Store.get()
    }
}
"#;
    let tree = build_tree_with_handlers(src);
    let json = doc::render_json(&tree);
    assert!(json.contains("\"handlers\":"), "JSON should have handlers field");
    assert!(json.contains("\"caller_item_id\":"), "JSON should have caller_item_id");
    assert!(json.contains("\"kind\": \"inline\""), "JSON should have kind=inline");
}

#[test]
fn handler_md_section_when_non_empty() {
    let src = r#"
module m

export type Store effect {
    Set(v int)
    #pure get() -> int
}

fn use_store() -> int {
    let mut s = 0
    with #trusted Store = handler Store {
        Set(v) { s = v }
        get() => s
    } {
        Store.get()
    }
}
"#;
    let tree = build_tree_with_handlers(src);
    let md = doc::render_markdown(&tree);
    assert!(md.contains("#### Handlers"), "MD should have Handlers section");
    assert!(md.contains("use_store"), "MD should mention caller");
    assert!(md.contains("(inline)"), "MD should show handler kind");
}
