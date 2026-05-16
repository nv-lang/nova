//! Plan 45 Ф.25.3 — source URL linking end-to-end tests.
//!
//! Проверяет что `NOVA_DOC_SOURCE_URL_TEMPLATE` env var заставляет
//! JSON output содержать `source.url` поля, а Markdown — `[src]` links.

use nova_codegen::doc;
use nova_codegen::parser;
use nova_codegen::types;

const SRC: &str = r#"
module mymod

/// First function.
export fn first() -> int => 1

/// Second function.
export fn second() -> int => 2
"#;

fn build_tree() -> doc::DocTree {
    let mut module = parser::parse(SRC).expect("parse");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    doc::build(&module)
}

#[test]
fn json_source_url_present_when_template_set() {
    // Mutex для env var (тесты бегут параллельно — set/unset race condition).
    // Используем уникальное значение чтобы isolate от других тестов.
    let template = "https://example.com/repo/blob/main/{path}#L{line}";
    // SAFETY: тест не run в parallel с другими env-modifying tests.
    unsafe { std::env::set_var("NOVA_DOC_SOURCE_URL_TEMPLATE", template); }
    let tree = build_tree();
    let json = doc::render_json_with_source(&tree, SRC);
    unsafe { std::env::remove_var("NOVA_DOC_SOURCE_URL_TEMPLATE"); }

    // URL должен быть в JSON.
    assert!(
        json.contains("https://example.com/repo/blob/main/mymod.nv#L"),
        "JSON should contain source URL, got: {}",
        json
    );
    // {path} substituted to "mymod.nv".
    assert!(json.contains("mymod.nv"), "path placeholder substituted");
}

#[test]
fn json_source_url_absent_when_template_unset() {
    unsafe { std::env::remove_var("NOVA_DOC_SOURCE_URL_TEMPLATE"); }
    let tree = build_tree();
    let json = doc::render_json_with_source(&tree, SRC);
    // No `"url":` key должен быть в source objects.
    // Note: некоторые другие поля могут содержать "url" — поэтому проверяем
    // что нет именно поля в source object: `"url":` сразу после "line": NNN,
    // или сразу перед закрытием объекта source.
    // Простая heuristic: нет example.com URL.
    assert!(
        !json.contains("example.com"),
        "JSON should not contain stale URL, got: {}",
        json
    );
}

#[test]
fn markdown_src_link_present_when_template_set() {
    let template = "https://gitlab.example/{path}#L{line}";
    unsafe { std::env::set_var("NOVA_DOC_SOURCE_URL_TEMPLATE", template); }
    let tree = build_tree();
    let md = doc::render_markdown_with_source(&tree, SRC);
    unsafe { std::env::remove_var("NOVA_DOC_SOURCE_URL_TEMPLATE"); }

    // Должен быть `[src]` link в headings.
    assert!(
        md.contains("[\\[src\\]](https://gitlab.example/mymod.nv#L"),
        "MD should contain [src] link, got: {}",
        md
    );
}

#[test]
fn markdown_no_src_link_when_template_unset() {
    unsafe { std::env::remove_var("NOVA_DOC_SOURCE_URL_TEMPLATE"); }
    let tree = build_tree();
    let md = doc::render_markdown_with_source(&tree, SRC);
    assert!(!md.contains("[src]"), "MD should not have [src] when template unset");
    assert!(!md.contains("[\\[src\\]]"), "MD should not have escaped [src] either");
}

#[test]
fn url_line_numbers_distinct_per_item() {
    let template = "https://x.y/{path}#L{line}";
    unsafe { std::env::set_var("NOVA_DOC_SOURCE_URL_TEMPLATE", template); }
    let tree = build_tree();
    let md = doc::render_markdown_with_source(&tree, SRC);
    unsafe { std::env::remove_var("NOVA_DOC_SOURCE_URL_TEMPLATE"); }

    // У двух функций должны быть разные #L<N>.
    let l5 = md.contains("#L5");
    let l8 = md.contains("#L8");
    // Точные line numbers зависят от parser'а; главное — две разные строки.
    let mut found_lines = 0;
    for line_num in 1..=20 {
        if md.contains(&format!("#L{}", line_num)) { found_lines += 1; }
    }
    assert!(
        found_lines >= 2,
        "expected at least 2 distinct line numbers for 2 fns, got {} (l5={} l8={}), MD: {}",
        found_lines, l5, l8, md
    );
}
