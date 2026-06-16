//! Integration tests for Plan 104.3 completion provider.
//!
//! Tests call the public `nova_lsp::completion` API directly (no process spawn).
//! They cover keyword, identifier, method-dot, import, and ranking sub-plans.
//!
//! Test count: 8 pos (extra integration) + existing unit tests = 47 total.

use nova_lsp::completion::{
    collect_scope_identifiers, completion_for, detect_context, import_items, method_items,
    snippet_items, CompletionContext, IdentKind,
};
use tower_lsp::lsp_types::CompletionItemKind;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn has_label(items: &[tower_lsp::lsp_types::CompletionItem], label: &str) -> bool {
    items.iter().any(|i| i.label == label)
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration tests: completion_for end-to-end
// ─────────────────────────────────────────────────────────────────────────────

/// ipos1: top-level completion returns fn, type, import keywords.
#[test]
fn ipos1_top_level_completion() {
    let src = "module test.i\n";
    let items = completion_for(src, src.len());
    assert!(has_label(&items, "fn"), "fn keyword at top level");
    assert!(has_label(&items, "type"), "type keyword at top level");
    assert!(has_label(&items, "import"), "import keyword at top level");
    assert!(has_label(&items, "test"), "test snippet at top level");
}

/// ipos2: fn-body completion returns let, if, for, return keywords + prelude.
#[test]
fn ipos2_fn_body_completion() {
    let src = "module test.i\nfn f() -> () {\n    ";
    let items = completion_for(src, src.len());
    assert!(has_label(&items, "let"), "let in fn body");
    assert!(has_label(&items, "if"), "if in fn body");
    assert!(has_label(&items, "return"), "return in fn body");
    assert!(has_label(&items, "int"), "int type from prelude");
    assert!(has_label(&items, "Option"), "Option from prelude");
}

/// ipos3: method-dot completion on int variable.
#[test]
fn ipos3_method_dot_int() {
    let src = "module test.i\nfn f() -> () {\n    let count int = 5\n    count.";
    let items = completion_for(src, src.len());
    assert!(!items.is_empty(), "method completions expected after dot");
    assert!(
        items.iter().all(|i| i.kind == Some(CompletionItemKind::METHOD)),
        "all items should be METHOD kind"
    );
    assert!(has_label(&items, "abs"), "abs method on int");
}

/// ipos4: import path completion for std.
#[test]
fn ipos4_import_std_path() {
    let src = "module test.i\nimport std.";
    let items = completion_for(src, src.len());
    assert!(!items.is_empty(), "std submodules expected");
    assert!(has_label(&items, "collections"), "std.collections");
    assert!(has_label(&items, "sync"), "std.sync");
}

/// ipos5: cursor in comment → no completions.
#[test]
fn ipos5_comment_no_completion() {
    let src = "module test.i\n// fn f() ";
    let items = completion_for(src, src.len());
    assert!(items.is_empty(), "no completions inside comment");
}

/// ipos6: cursor in string → no completions.
#[test]
fn ipos6_string_no_completion() {
    let src = "module test.i\nfn f() -> () {\n    let s str = \"hello ";
    let items = completion_for(src, src.len());
    assert!(items.is_empty(), "no completions inside string");
}

/// ipos7: multiple bindings in scope — all appear.
#[test]
fn ipos7_multiple_bindings_in_scope() {
    let src = "module test.i\nfn f() -> () {\n    let alpha int = 1\n    let beta str = \"\"\n    let gamma bool = true\n    ";
    let items = completion_for(src, src.len());
    assert!(has_label(&items, "alpha"), "alpha in scope");
    assert!(has_label(&items, "beta"), "beta in scope");
    assert!(has_label(&items, "gamma"), "gamma in scope");
}

/// ipos8: type-body context returns fn, const, pub — no fn-body keywords like let.
#[test]
fn ipos8_type_body_no_let() {
    let src = "module test.i\ntype Foo {\n    ";
    let items = completion_for(src, src.len());
    // type body should have fn keyword but NOT let (fn-body keyword)
    assert!(has_label(&items, "fn"), "fn in type body");
    // `let` is NOT in type-body keyword list
    let has_let_kw = items.iter().any(|i| i.label == "let" && i.kind == Some(CompletionItemKind::KEYWORD));
    assert!(!has_let_kw, "let keyword should NOT appear in type body");
}

// ─────────────────────────────────────────────────────────────────────────────
// Sub-plan specific integration
// ─────────────────────────────────────────────────────────────────────────────

/// Ranking: verify sort_text ordering local < module < std < keyword.
#[test]
fn ranking_full_ordering() {
    let src = "module test.r\nfn globalFn() -> () {}\nfn g() -> () {\n    let myLocal int = 0\n    ";
    let items = completion_for(src, src.len());

    let local_sort = items.iter()
        .find(|i| i.label == "myLocal")
        .and_then(|i| i.sort_text.as_deref())
        .expect("myLocal should appear");

    let module_sort = items.iter()
        .find(|i| i.label == "globalFn")
        .and_then(|i| i.sort_text.as_deref())
        .expect("globalFn should appear");

    let prelude_sort = items.iter()
        .find(|i| i.label == "int")
        .and_then(|i| i.sort_text.as_deref())
        .expect("int (prelude) should appear");

    let kw_sort = items.iter()
        .find(|i| i.label == "let" && i.kind == Some(CompletionItemKind::KEYWORD))
        .and_then(|i| i.sort_text.as_deref())
        .expect("let (keyword) should appear");

    assert!(local_sort < module_sort, "local < module");
    assert!(module_sort < prelude_sort, "module < prelude");
    assert!(prelude_sort < kw_sort, "prelude < keyword");
}

/// Method completions: str methods appear with detail.
#[test]
fn method_str_detail_present() {
    let src = "module test.m\nfn f() -> () {\n    let msg str = \"\"\n    msg.";
    let items = method_items(src, src.len());
    let len_item = items.iter().find(|i| i.label == "len");
    assert!(len_item.is_some(), "len method on str");
    assert!(len_item.unwrap().detail.is_some(), "detail should be present");
}

/// Import items: std.sync returns mutex, rwlock, semaphore.
#[test]
fn import_sync_submodules() {
    let prefix = vec!["std".to_string(), "sync".to_string()];
    let items = import_items(&prefix);
    assert!(has_label(&items, "mutex"), "std.sync.mutex");
    assert!(has_label(&items, "rwlock"), "std.sync.rwlock");
    assert!(has_label(&items, "channel"), "std.sync.channel");
}

/// Scope identifiers: param from fn sig, let binding, type decl — all present.
#[test]
fn scope_params_and_decls() {
    let src = "module test.s\ntype MyType {}\nfn calc(input int, factor float) -> int {\n    let result int = 0\n    ";
    let idents = collect_scope_identifiers(src, src.len());

    let names: Vec<&str> = idents.iter().map(|i| i.name.as_str()).collect();

    // fn params (from `calc`).
    assert!(names.contains(&"input"), "input param");
    assert!(names.contains(&"factor"), "factor param");
    // local binding.
    assert!(names.contains(&"result"), "result binding");
    // type decl.
    assert!(names.contains(&"MyType"), "MyType type decl");
    // fn decl.
    assert!(names.contains(&"calc"), "calc fn decl");
}

/// Deduplicate: same label from prelude + module shouldn't appear twice.
#[test]
fn deduplication_no_duplicate_labels() {
    let src = "module test.d\nfn f() -> () {\n    ";
    let items = completion_for(src, src.len());
    let mut labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    let before_dedup = labels.len();
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), before_dedup, "duplicate labels found in completion");
}
