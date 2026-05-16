//! Plan 45 Ф.27.1 — handler matrix workspace mode integration test.
//!
//! Раньше workspace mode выдавал `handlers: []` для всех Effect'ов даже
//! если их использовали в других модулях. Ф.27.1 это починил через
//! `populate_handler_matrix_workspace` API + sources_by_module map.
//!
//! Этот test verifies: Effect в module A, handler usage в module B →
//! `Effect.handlers` populated с правильным caller_item_id из module B.

use nova_codegen::doc;
use nova_codegen::parser;
use nova_codegen::types;

fn build_workspace_with_handlers(
    files: &[(&str, &str)], // [(module_path, source), ...]
) -> doc::DocTree {
    let mut modules: Vec<nova_codegen::ast::Module> = Vec::new();
    let mut sources_map: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (path, src) in files {
        let mut module = parser::parse(src).expect("parse should succeed");
        let _ = types::check_module(&module);
        types::infer_effects(&mut module);
        sources_map.insert(path.to_string(), src.to_string());
        modules.push(module);
    }
    let mut tree = doc::build_workspace(&modules);
    doc::populate_handler_matrix_workspace(&mut tree, &sources_map);
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
fn cross_file_handler_matrix_populated() {
    // Module A defines Effect, module B uses handler.
    let module_a = r#"
module store

export type Store effect {
    Set(v int)
    #pure get() -> int
}
"#;

    let module_b = r#"
module client

import store.{Store}

export fn use_store() -> int {
    let mut s = 0
    with #trusted Store = handler Store {
        Set(v) { s = v }
        get() => s
    } {
        Store.Set(42)
        Store.get()
    }
}
"#;

    let tree = build_workspace_with_handlers(&[
        ("store", module_a),
        ("client", module_b),
    ]);

    let handlers = find_effect_handlers(&tree, "Store")
        .expect("Store effect should be in tree");
    assert_eq!(handlers.len(), 1,
        "cross-file handler should be detected, got {:?}", handlers);
    assert!(handlers[0].caller_item_id.contains("use_store"),
        "caller should be use_store from module client, got {}", handlers[0].caller_item_id);
}

#[test]
fn multiple_modules_multiple_handlers() {
    let effect_module = r#"
module effects

export type Counter effect {
    incr()
    #pure get() -> int
}
"#;

    let caller_a = r#"
module callers.a

import effects.{Counter}

export fn a_use() -> int {
    let mut c = 0
    with #trusted Counter = handler Counter {
        incr() { c = c + 1 }
        get() => c
    } {
        Counter.incr()
        Counter.get()
    }
}
"#;

    let caller_b = r#"
module callers.b

import effects.{Counter}

export fn b_use() -> int {
    let mut c = 0
    with #trusted Counter = handler Counter {
        incr() { c = c + 1 }
        get() => c
    } {
        Counter.incr()
        Counter.incr()
        Counter.get()
    }
}
"#;

    let tree = build_workspace_with_handlers(&[
        ("effects", effect_module),
        ("callers.a", caller_a),
        ("callers.b", caller_b),
    ]);

    let handlers = find_effect_handlers(&tree, "Counter")
        .expect("Counter effect");
    assert_eq!(handlers.len(), 2, "two cross-file callers, got {:?}", handlers);
    // Sorted alphabetically by caller_item_id.
    let ids: Vec<&str> = handlers.iter().map(|h| h.caller_item_id.as_str()).collect();
    assert!(ids[0].contains("a_use"));
    assert!(ids[1].contains("b_use"));
}

#[test]
fn workspace_handlers_deterministic() {
    let module_a = r#"
module fs

export type Fs effect {
    read(path str) -> str
}
"#;

    let module_b = r#"
module reader

import fs.{Fs}

export fn read_config() -> str {
    with #trusted Fs = handler Fs {
        read(p) => "default"
    } {
        Fs.read("/etc/config")
    }
}
"#;

    let first = build_workspace_with_handlers(&[
        ("fs", module_a),
        ("reader", module_b),
    ]);
    let second = build_workspace_with_handlers(&[
        ("fs", module_a),
        ("reader", module_b),
    ]);
    let h1 = find_effect_handlers(&first, "Fs").unwrap();
    let h2 = find_effect_handlers(&second, "Fs").unwrap();
    assert_eq!(h1, h2, "workspace handler matrix should be deterministic");
}

#[test]
fn workspace_no_handlers_empty_array() {
    let module_a = r#"
module untouched

export type Logger effect {
    log(msg str)
}
"#;

    let module_b = r#"
module dummy

export fn just_a_fn() -> int => 42
"#;

    let tree = build_workspace_with_handlers(&[
        ("untouched", module_a),
        ("dummy", module_b),
    ]);

    let handlers = find_effect_handlers(&tree, "Logger").expect("Logger effect");
    assert!(handlers.is_empty(),
        "no handler usage → empty array, got {:?}", handlers);
}
