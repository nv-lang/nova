//! Plan 45 Ф.30.1 — external crate-doc linking tests.
//!
//! `NOVA_DOC_EXTERN_LINKS` env: `prefix1=template1;prefix2=template2`.
//! Тесты serialized через ENV_MUTEX.

use nova_codegen::doc;
use nova_codegen::parser;
use nova_codegen::types;
use std::sync::Mutex;

static ENV_MUTEX: Mutex<()> = Mutex::new(());

fn build_tree(src: &str) -> doc::DocTree {
    let mut module = parser::parse(src).expect("parse");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    doc::build(&module)
}

const SRC_WITH_LINK: &str = r#"
module m

/// See [std.io.println] for output.
export fn f() -> int => 1
"#;

#[test]
fn extern_link_resolved_when_env_set() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var(
            "NOVA_DOC_EXTERN_LINKS",
            "std=https://docs.nova-lang.org/std/{path}",
        );
    }
    let tree = build_tree(SRC_WITH_LINK);
    unsafe { std::env::remove_var("NOVA_DOC_EXTERN_LINKS"); }

    // Найти link "std.io.println".
    let link = tree.links.iter()
        .find(|l| l.text == "std.io.println")
        .expect("link [std.io.println] should be extracted");
    assert!(link.target_id.is_none(),
        "internal target_id должен быть None (нет в workspace)");
    assert_eq!(
        link.target_url.as_deref(),
        Some("https://docs.nova-lang.org/std/io.println"),
        "target_url должен резолвиться через extern map, got: {:?}", link.target_url
    );
}

#[test]
fn extern_link_unset_means_no_url() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { std::env::remove_var("NOVA_DOC_EXTERN_LINKS"); }
    let tree = build_tree(SRC_WITH_LINK);
    let link = tree.links.iter()
        .find(|l| l.text == "std.io.println")
        .expect("link should be extracted");
    assert!(link.target_url.is_none(),
        "без env target_url должен быть None");
}

#[test]
fn extern_link_multiple_prefixes() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var(
            "NOVA_DOC_EXTERN_LINKS",
            "std=https://docs.nova-lang.org/std/{path};myorg.lib=https://myorg.dev/{path}",
        );
    }
    let src = r#"
module m

/// Uses [std.io.println] и [myorg.lib.helper].
export fn f() -> int => 1
"#;
    let tree = build_tree(src);
    unsafe { std::env::remove_var("NOVA_DOC_EXTERN_LINKS"); }

    let std_link = tree.links.iter().find(|l| l.text == "std.io.println");
    let myorg_link = tree.links.iter().find(|l| l.text == "myorg.lib.helper");
    assert!(std_link.is_some());
    assert!(myorg_link.is_some());
    assert!(std_link.unwrap().target_url.as_deref().unwrap().contains("docs.nova-lang.org"));
    assert!(myorg_link.unwrap().target_url.as_deref().unwrap().contains("myorg.dev"));
}

#[test]
fn extern_link_unknown_prefix_remains_broken() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var("NOVA_DOC_EXTERN_LINKS", "std=https://x.com/{path}");
    }
    let src = r#"
module m

/// See [unknown.prefix.fn] elsewhere.
export fn f() -> int => 1
"#;
    let tree = build_tree(src);
    unsafe { std::env::remove_var("NOVA_DOC_EXTERN_LINKS"); }

    let link = tree.links.iter().find(|l| l.text == "unknown.prefix.fn");
    if let Some(l) = link {
        assert!(l.target_id.is_none());
        assert!(l.target_url.is_none(),
            "unknown prefix не должен match'нуться в extern, got: {:?}", l.target_url);
    }
}

#[test]
fn json_emits_target_url_field() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var(
            "NOVA_DOC_EXTERN_LINKS",
            "std=https://docs.example/{path}",
        );
    }
    let tree = build_tree(SRC_WITH_LINK);
    let json = doc::render_json(&tree);
    unsafe { std::env::remove_var("NOVA_DOC_EXTERN_LINKS"); }

    assert!(json.contains("\"target_url\":"),
        "JSON должен содержать target_url field");
    assert!(json.contains("docs.example"),
        "JSON должен содержать external URL");
}

#[test]
fn md_uses_external_url_as_link_target() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var(
            "NOVA_DOC_EXTERN_LINKS",
            "std=https://docs.example/{path}",
        );
    }
    let tree = build_tree(SRC_WITH_LINK);
    let md = doc::render_markdown(&tree);
    unsafe { std::env::remove_var("NOVA_DOC_EXTERN_LINKS"); }

    // MD link rewriting должно использовать external URL вместо #anchor.
    assert!(md.contains("docs.example/io.println"),
        "MD должна содержать external URL для [std.io.println], got:\n{}", md);
}
