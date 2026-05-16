//! Plan 45 Ф.31.1 — HTML render MVP integration tests.

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
fn html_basic_shape() {
    let src = r#"
module m

/// Greet user.
export fn greet(name str) -> str => "Hello"
"#;
    let tree = build_tree(src);
    let html = doc::render_html(&tree);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<html"));
    assert!(html.contains("</html>"));
    assert!(html.contains("<head>"));
    assert!(html.contains("<body>"));
    assert!(html.contains("greet"));
    assert!(html.contains("Greet user"));
}

#[test]
fn html_includes_embedded_css() {
    let src = r#"
module m

export fn f() -> int => 1
"#;
    let tree = build_tree(src);
    let html = doc::render_html(&tree);
    assert!(html.contains("<style>"));
    assert!(html.contains("</style>"));
    assert!(html.contains("font:"));
}

#[test]
fn html_sidebar_lists_modules_and_items() {
    let src = r#"
module mymod

/// First.
export fn alpha() -> int => 1

/// Second.
export fn beta() -> int => 2
"#;
    let tree = build_tree(src);
    let html = doc::render_html(&tree);
    assert!(html.contains("nav class=\"sidebar\""));
    assert!(html.contains(">mymod</h2>"));
    assert!(html.contains("href=\"#mymod-alpha\""));
    assert!(html.contains("href=\"#mymod-beta\""));
}

#[test]
fn html_escapes_user_content_xss_safe() {
    let src = r#"
module m

/// Use <script>alert(1)</script> tag.
export fn unsafe_doc() -> int => 1
"#;
    let tree = build_tree(src);
    let html = doc::render_html(&tree);
    // Raw <script> must be escaped, not present as tag.
    assert!(!html.contains("<script>alert"),
        "raw <script> tag не должен попасть в HTML output (XSS!)");
    assert!(html.contains("&lt;script&gt;"),
        "должен быть escaped как &lt;script&gt;");
}

#[test]
fn html_renders_badges_for_stability_and_capabilities() {
    let src = r#"
module m

/// Stable fn.
#stable(since = "1.0")
export fn s() -> int => 1
"#;
    let tree = build_tree(src);
    let html = doc::render_html(&tree);
    assert!(html.contains("badge-stable"),
        "stable item должен иметь badge-stable class");
    assert!(html.contains("stable"));
}

#[test]
fn html_renders_fn_signature_pre_code() {
    let src = r#"
module m

export fn add(a int, b int) -> int => a + b
"#;
    let tree = build_tree(src);
    let html = doc::render_html(&tree);
    assert!(html.contains("<pre><code>"));
    // `->` пишется literally в pre/code (не через html_escape) — verify both forms.
    assert!(html.contains("fn add(a int, b int) -> int")
        || html.contains("fn add(a int, b int) -&gt; int"),
        "signature должна быть в pre>code, got: {}",
        html.lines().filter(|l| l.contains("add")).collect::<Vec<_>>().join("\n"));
}

#[test]
fn html_deterministic_output() {
    let src = r#"
module m

export fn f() -> int => 1
export fn g() -> int => 2
"#;
    let tree = build_tree(src);
    let first = doc::render_html(&tree);
    let second = doc::render_html(&tree);
    assert_eq!(first, second, "HTML output must be deterministic");
}

#[test]
fn html_includes_search_box_and_js() {
    // Plan 45 Ф.31.2: search bar + inline JS filter.
    let src = r#"
module m

export fn f() -> int => 1
"#;
    let tree = build_tree(src);
    let html = doc::render_html(&tree);
    assert!(html.contains("class=\"search-box\""),
        "HTML должна содержать search input");
    assert!(html.contains("id=\"nova-search\""),
        "search input должен иметь id для JS attach");
    assert!(html.contains("<script>"),
        "HTML должна содержать inline JS для search");
    assert!(html.contains("getElementById('nova-search')"),
        "JS должна attachить к search input");
}

#[test]
fn html_uses_css_variables_for_dark_mode() {
    // Plan 45 Ф.31.3: CSS variables + prefers-color-scheme media query.
    let src = r#"
module m

export fn f() -> int => 1
"#;
    let tree = build_tree(src);
    let html = doc::render_html(&tree);
    assert!(html.contains(":root {"),
        "CSS должна использовать :root variables");
    assert!(html.contains("--bg:"),
        "CSS должна определять --bg variable");
    assert!(html.contains("@media (prefers-color-scheme: dark)"),
        "CSS должна иметь dark mode media query");
    assert!(html.contains("var(--bg)"),
        "CSS должна use var(--bg) (не hardcoded color)");
}

#[test]
fn html_anchor_format_lowercased_dash_separated() {
    let src = r#"
module myMod

export type MyType int
"#;
    let tree = build_tree(src);
    let html = doc::render_html(&tree);
    // Anchor must be lowercased.
    assert!(html.contains("id=\"mymod-mytype\""),
        "anchor must be lowercase dash-separated, got: {}",
        html.lines().filter(|l| l.contains("mytype")).collect::<Vec<_>>().join("\n"));
}
