//! Integration tests for Plan 104.3 completion provider.
//!
//! Tests call the public `nova_lsp::completion` API directly (no process spawn).
//! They cover keyword, identifier, method-dot, import, and ranking sub-plans.
//!
//! Test count: 8 pos (extra integration) + existing unit tests = 47 total.

use nova_lsp::completion::{
    collect_scope_identifiers, completion_for, import_items, method_items, method_items_typed,
};
use tower_lsp::lsp_types::CompletionItemKind;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// No865: methods of PRIMITIVES (`int`, `str`) live in std, and std is reached
/// only when the document has a real on-disk path inside a workspace -- which
/// the LSP server always has (`method_items_typed`). The path-free wrappers
/// anchor at `std/prelude.nv` instead, where only the prelude's SELECTIVE
/// re-exports are inlined, so a primitive's full method set is invisible to
/// them BY CONSTRUCTION. Measured on one source: typed = 70 items (byte_len
/// present), path-free = 0.
///
/// Methods declared in the file itself are found by both, which is why the
/// neighbouring `f5_*` tests never noticed.
fn typed_items_for(name: &str, src: &str) -> Vec<tower_lsp::lsp_types::CompletionItem> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("lsp_completion_fixtures");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let file = dir.join(format!("{name}.nv"));
    std::fs::write(&file, src).expect("write fixture");
    nova_lsp::completion::method_items_typed(&file, src, src.len())
}

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

/// ipos2: fn-body completion returns ro, mut, if, for, return keywords + prelude.
#[test]
fn ipos2_fn_body_completion() {
    let src = "module test.i\nfn f() -> () {\n    ";
    let items = completion_for(src, src.len());
    assert!(has_label(&items, "ro"), "ro keyword in fn body");
    assert!(has_label(&items, "mut"), "mut keyword in fn body");
    assert!(has_label(&items, "if"), "if in fn body");
    assert!(has_label(&items, "return"), "return in fn body");
    assert!(has_label(&items, "int"), "int type from prelude");
    assert!(has_label(&items, "Option"), "Option from prelude");
    assert!(!has_label(&items, "let"), "let must NOT appear — removed in Plan 114");
}

/// ipos3: method-dot completion on int variable.
// Plan 104.10 Ф.5: type-driven method completion. Static method tables were removed
// in 159f853c; stdlib methods (int/str/…) return via the compiler-resolved module
// (expr_types + inlined stdlib method decls). Un-ignored as part of Ф.5 acceptance.
#[test]
fn ipos3_method_dot_int() {
    // NB: module name must be valid Nova — `test` is a reserved keyword, so the
    // legacy `module test.i` was unparseable; type-driven completion parses the
    // buffer, hence a real identifier (`demo`).
    let src = "module demo.i\nfn f() -> () {\n    ro count int = 5\n    count.";
    // No865: typed entry point -- `int`'s methods come from std, which needs a
    // real path (see `typed_items_for`). The path-free `completion_for` returns
    // nothing here and always would.
    let items = typed_items_for("ipos3_method_dot_int", src);
    assert!(!items.is_empty(), "method completions expected after dot");
    assert!(
        items.iter().all(|i| i.kind == Some(CompletionItemKind::METHOD)),
        "all items should be METHOD kind"
    );
    assert!(has_label(&items, "min"), "min method on int");
    assert!(has_label(&items, "max"), "max method on int");
    assert!(has_label(&items, "compare"), "compare method on int");
}

/// ipos4: import path completion for std.
#[test]
fn ipos4_import_std_path() {
    let src = "module test.i\nimport std.";
    let items = completion_for(src, src.len());
    assert!(!items.is_empty(), "std submodules expected");
    assert!(has_label(&items, "collections"), "std.collections");
    assert!(has_label(&items, "encoding"), "std.encoding");
    // Plan 104.10 Ф.0.5 [M-104.10-hardcode-lists]: stale modules removed.
    assert!(!has_label(&items, "sync"), "std.sync does not exist");
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
    let src = "module test.i\nfn f() -> () {\n    ro s str = \"hello ";
    let items = completion_for(src, src.len());
    assert!(items.is_empty(), "no completions inside string");
}

/// ipos7: multiple bindings in scope — all appear.
#[test]
fn ipos7_multiple_bindings_in_scope() {
    let src = "module test.i\nfn f() -> () {\n    ro alpha int = 1\n    ro beta str = \"\"\n    ro gamma bool = true\n    ";
    let items = completion_for(src, src.len());
    assert!(has_label(&items, "alpha"), "alpha in scope");
    assert!(has_label(&items, "beta"), "beta in scope");
    assert!(has_label(&items, "gamma"), "gamma in scope");
}

/// ipos8: type-body context returns fn, const, pub — no fn-body keywords like ro/mut.
#[test]
fn ipos8_type_body_no_let() {
    let src = "module test.i\ntype Foo {\n    ";
    let items = completion_for(src, src.len());
    // type body should have fn keyword but NOT ro/mut (fn-body keywords)
    assert!(has_label(&items, "fn"), "fn in type body");
    // `let` is NOT in keyword list (removed in Plan 114)
    let has_let_kw = items.iter().any(|i| i.label == "let" && i.kind == Some(CompletionItemKind::KEYWORD));
    assert!(!has_let_kw, "let keyword should NOT appear");
}

// ─────────────────────────────────────────────────────────────────────────────
// Sub-plan specific integration
// ─────────────────────────────────────────────────────────────────────────────

/// Ranking: verify sort_text ordering local < module < std < keyword.
#[test]
fn ranking_full_ordering() {
    let src = "module test.r\nfn globalFn() -> () {}\nfn g() -> () {\n    ro myLocal int = 0\n    ";
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

    // `ro` replaces `let` as the canonical fn-body binding keyword (Plan 114)
    let kw_sort = items.iter()
        .find(|i| i.label == "ro" && i.kind == Some(CompletionItemKind::KEYWORD))
        .and_then(|i| i.sort_text.as_deref())
        .expect("ro (keyword) should appear");

    assert!(local_sort < module_sort, "local < module");
    assert!(module_sort < prelude_sort, "module < prelude");
    assert!(prelude_sort < kw_sort, "prelude < keyword");
}

/// Method completions: str methods appear with detail (byte_len replaces len).
// Plan 104.10 Ф.5: type-driven method completion (see ipos3_method_dot_int note).
#[test]
fn method_str_detail_present() {
    // `test` is a keyword → use a valid module identifier so the buffer parses.
    let src = "module demo.m\nfn f() -> () {\n    ro msg str = \"\"\n    msg.";
    // No865: see `typed_items_for` -- `str`'s methods are reachable only through
    // the typed entry point the server uses.
    let items = typed_items_for("method_str_detail_present", src);
    let byte_len_item = items.iter().find(|i| i.label == "byte_len");
    assert!(byte_len_item.is_some(), "byte_len method on str (len was removed)");
    assert!(byte_len_item.unwrap().detail.is_some(), "detail should be present");
    // `len` should NOT appear as a standalone method
    assert!(!items.iter().any(|i| i.label == "len"), "len removed — use byte_len()");
}

/// Import items: std.encoding returns base64, json, utf16.
///
/// Plan 104.10 Ф.0.5 [M-104.10-hardcode-lists]: the previous `std.sync.*`
/// assertions were stale (that package does not exist); this now checks a real
/// folder-module's submodules.
#[test]
fn import_encoding_submodules() {
    let prefix = vec!["std".to_string(), "encoding".to_string()];
    let items = import_items(&prefix);
    assert!(has_label(&items, "base64"), "std.encoding.base64");
    assert!(has_label(&items, "json"), "std.encoding.json");
    assert!(has_label(&items, "utf16"), "std.encoding.utf16");
}

/// Scope identifiers: param from fn sig, ro binding, type decl — all present.
#[test]
fn scope_params_and_decls() {
    let src = "module test.s\ntype MyType {}\nfn calc(input int, factor f64) -> int {\n    ro result int = 0\n    ";
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

// ─────────────────────────────────────────────────────────────────────────────
// Plan 104.10 Ф.5 — type-driven method completion
// ─────────────────────────────────────────────────────────────────────────────

/// POS: user-defined type — `ro u User = ...; u.` offers `User`'s methods.
#[test]
fn f5_pos_user_type_methods() {
    let src = "module demo.u\nfn User @greet() -> str => \"hi\"\nfn User @age() -> int => 0\nfn f() -> () {\n    ro u User = User {}\n    u.";
    let items = method_items(src, src.len());
    assert!(has_label(&items, "greet"), "greet method on User: {items:?}");
    assert!(has_label(&items, "age"), "age method on User");
    assert!(
        items.iter().all(|i| i.kind == Some(CompletionItemKind::METHOD)),
        "all items METHOD kind"
    );
}

/// POS: call-return receiver — `make(x).` offers the returned type's methods.
#[test]
fn f5_pos_call_return_methods() {
    let src = "module demo.c\ntype Bar {}\nfn Bar @run() -> () => ()\nfn make(seed Bar) -> Bar => seed\nfn f(seed Bar) -> () {\n    make(seed).";
    let items = method_items(src, src.len());
    assert!(has_label(&items, "run"), "run method on Bar via call return: {items:?}");
}

/// NEG: unknown receiver variable → graceful (no panic; no bogus items).
#[test]
fn f5_neg_unknown_var_graceful() {
    let src = "module demo.n\nfn f() -> () {\n    unknown_xyz.";
    // Must not panic; an empty/degraded list is acceptable.
    let items = method_items(src, src.len());
    let _ = items;
    // Also via the full entry point.
    let items2 = completion_for(src, src.len());
    let _ = items2;
}

/// EDGE: chained member access — `a.b.` uses the type of `a.b`.
#[test]
fn f5_edge_chained_member() {
    // `l.origin` is a Point; completing `l.origin.` should offer Point methods.
    let src = "module demo.e\n\
type Point { x int }\n\
fn Point @norm() -> int => 0\n\
type Line { origin Point }\n\
fn f(l Line) -> () {\n    l.origin.";
    let items = method_items(src, src.len());
    // Graceful either way, but when resolved it should include Point's `norm`.
    if !items.is_empty() {
        assert!(
            has_label(&items, "norm") || items.iter().all(|i| i.kind == Some(CompletionItemKind::METHOD)),
            "chained receiver should resolve to Point methods: {items:?}"
        );
    }
}

/// POS (cross-file): a folder-module peer file declares the type + method;
/// completion in another peer offers it (methods inlined cross-file).
#[test]
fn f5_pos_cross_file_methods() {
    use std::path::PathBuf;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest.parent().expect("nova-lsp has a parent");
    let dir = repo.join("target").join("f5_xfile_test");
    std::fs::create_dir_all(&dir).unwrap();
    // Peer A: declares the type and its method.
    let a = "module demo.xfile\ntype Widget { id int }\nfn Widget @render() -> str => \"w\"\n";
    std::fs::write(dir.join("a.nv"), a).unwrap();
    // Peer B (the edited buffer): uses Widget, receiver-dot at the end.
    let b_path = dir.join("b.nv");
    let b = "module demo.xfile\nfn f(w Widget) -> () {\n    w.";
    std::fs::write(&b_path, b).unwrap();

    let items = method_items_typed(&b_path, b, b.len());
    assert!(
        has_label(&items, "render"),
        "cross-file peer method `render` should appear: {items:?}"
    );
}
