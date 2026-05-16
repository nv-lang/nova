//! Plan 45 Ф.31.4 — multi-page HTML output integration tests.

use nova_codegen::doc;
use nova_codegen::parser;
use nova_codegen::types;

fn build_workspace(files: &[(&str, &str)]) -> doc::DocTree {
    let mut modules: Vec<nova_codegen::ast::Module> = Vec::new();
    for (_path, src) in files {
        let mut module = parser::parse(src).expect("parse");
        let _ = types::check_module(&module);
        types::infer_effects(&mut module);
        modules.push(module);
    }
    doc::build_workspace(&modules)
}

#[test]
fn multipage_has_index_and_per_module() {
    let module_a = r#"
module foo

/// Module foo.
export fn a() -> int => 1
"#;
    let module_b = r#"
module bar

/// Module bar.
export fn b() -> int => 2
"#;
    let tree = build_workspace(&[("foo", module_a), ("bar", module_b)]);
    let pages = doc::render_html_multipage(&tree);
    assert!(pages.contains_key("index.html"), "index.html должен быть");
    assert!(pages.contains_key("foo.html"), "per-module foo.html должен быть");
    assert!(pages.contains_key("bar.html"), "per-module bar.html должен быть");
    // Plan 45 Ф.31.6: + sitemap.xml.
    assert_eq!(pages.len(), 4, "expected 4 pages (index + 2 modules + sitemap)");
}

#[test]
fn multipage_index_lists_all_modules() {
    let tree = build_workspace(&[
        ("alpha", "module alpha\nexport fn a() -> int => 1\n"),
        ("beta", "module beta\nexport fn b() -> int => 2\n"),
    ]);
    let pages = doc::render_html_multipage(&tree);
    let index = pages.get("index.html").expect("index.html");
    assert!(index.contains("href=\"alpha.html\""));
    assert!(index.contains("href=\"beta.html\""));
    assert!(index.contains("API documentation"));
}

#[test]
fn multipage_per_module_lists_items_and_navigates() {
    let tree = build_workspace(&[
        ("mymod", "module mymod\nexport fn item_one() -> int => 1\nexport fn item_two() -> int => 2\n"),
    ]);
    let pages = doc::render_html_multipage(&tree);
    let page = pages.get("mymod.html").expect("mymod.html");
    // Sidebar содержит items.
    assert!(page.contains("item_one"));
    assert!(page.contains("item_two"));
    // Main содержит rendered items.
    assert!(page.matches("item_one").count() >= 2,
        "item_one должен быть в sidebar AND main, got count: {}",
        page.matches("item_one").count());
    // Link back на index.
    assert!(page.contains("href=\"index.html\""));
}

#[test]
fn multipage_marks_current_module_in_sidebar() {
    let tree = build_workspace(&[
        ("aaa", "module aaa\nexport fn x() -> int => 1\n"),
        ("bbb", "module bbb\nexport fn y() -> int => 2\n"),
    ]);
    let pages = doc::render_html_multipage(&tree);
    let aaa_page = pages.get("aaa.html").expect("aaa.html");
    // Current module marker.
    assert!(aaa_page.contains("(here)"),
        "current module должен быть помечен (here) в sidebar");
}

#[test]
fn multipage_all_pages_have_search_and_css() {
    let tree = build_workspace(&[
        ("m", "module m\nexport fn f() -> int => 1\n"),
    ]);
    let pages = doc::render_html_multipage(&tree);
    for (filename, html) in &pages {
        // Plan 45 Ф.31.6: sitemap.xml — XML, not HTML; skip CSS/search check.
        if filename.ends_with(".xml") { continue; }
        assert!(html.contains("id=\"nova-search\""),
            "search box must be in {}", filename);
        assert!(html.contains("<style>"),
            "CSS must be in {}", filename);
        assert!(html.contains("@media (prefers-color-scheme: dark)"),
            "dark mode CSS must be in {}", filename);
    }
}

#[test]
fn sitemap_xml_present_in_multipage() {
    // Plan 45 Ф.31.6 — sitemap.xml generated в multipage output.
    let tree = build_workspace(&[
        ("foo", "module foo\nexport fn a() -> int => 1\n"),
        ("bar", "module bar\nexport fn b() -> int => 2\n"),
    ]);
    let pages = doc::render_html_multipage(&tree);
    let sitemap = pages.get("sitemap.xml").expect("sitemap.xml должен быть");
    assert!(sitemap.contains("<?xml version=\"1.0\""));
    assert!(sitemap.contains("<urlset"));
    assert!(sitemap.contains("sitemaps.org/schemas/sitemap/0.9"));
    // Index.html — priority 1.0.
    assert!(sitemap.contains("index.html"));
    assert!(sitemap.contains("<priority>1.0</priority>"));
    // Module pages — priority 0.8.
    assert!(sitemap.contains("foo.html"));
    assert!(sitemap.contains("bar.html"));
    assert!(sitemap.contains("<priority>0.8</priority>"));
}

#[test]
fn sitemap_uses_absolute_urls_when_env_set() {
    static SITEMAP_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = SITEMAP_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { std::env::set_var("NOVA_DOC_SITE_URL", "https://docs.example.com/api"); }
    let tree = build_workspace(&[
        ("m", "module m\nexport fn f() -> int => 1\n"),
    ]);
    let pages = doc::render_html_multipage(&tree);
    unsafe { std::env::remove_var("NOVA_DOC_SITE_URL"); }
    let sitemap = pages.get("sitemap.xml").expect("sitemap.xml");
    assert!(sitemap.contains("https://docs.example.com/api/index.html"),
        "absolute URL должен use base, got: {}", sitemap);
    assert!(sitemap.contains("https://docs.example.com/api/m.html"));
}

#[test]
fn multipage_deterministic_output() {
    let tree = build_workspace(&[
        ("m1", "module m1\nexport fn a() -> int => 1\n"),
        ("m2", "module m2\nexport fn b() -> int => 2\n"),
    ]);
    let first = doc::render_html_multipage(&tree);
    let second = doc::render_html_multipage(&tree);
    assert_eq!(first.len(), second.len());
    for (k, v) in &first {
        let v2 = second.get(k).expect("same keys");
        assert_eq!(v, v2, "deterministic для page {}", k);
    }
}
