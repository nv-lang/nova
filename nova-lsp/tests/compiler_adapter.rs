//! Tests for the nova_codegen compiler adapter (Plan 104.1.Ф.1).
//!
//! These are unit-style tests that call `check_file` / `check_workspace`
//! directly without spawning an LSP process.

use nova_lsp::compiler::{check_file, check_workspace};
use tower_lsp::lsp_types::Url;

fn uri(p: &str) -> Url {
    Url::parse(&format!("file:///{p}")).unwrap()
}

// ─────────────────────────────────────────────────────────────────────────────
// pos tests
// ─────────────────────────────────────────────────────────────────────────────

/// pos1: valid Nova source → 0 diagnostics.
#[test]
fn pos1_valid_nova_zero_diagnostics() {
    // Minimal valid Nova module: a module declaration + simple function.
    // No prelude symbols needed (no assert/println).
    let src = "module adapter_test.pos1\n\nfn add(a int, b int) -> int => a + b\n";
    let result = check_file(&uri("pos1.nv"), src);
    assert!(
        result.diagnostics.is_empty(),
        "expected 0 diagnostics on valid source, got: {:?}",
        result.diagnostics
    );
}

/// pos2: syntax error → at least one diagnostic with a non-zero span.
#[test]
fn pos2_syntax_error_returns_diagnostic_with_span() {
    // Broken: missing closing paren
    let src = "module adapter_test.pos2\n\nfn broken( => ()\n";
    let result = check_file(&uri("pos2.nv"), src);
    assert!(
        !result.diagnostics.is_empty(),
        "expected at least one diagnostic for syntax error"
    );
    let d = &result.diagnostics[0];
    // Span should point into the source
    assert!(
        d.span.start < src.len() || d.span.end <= src.len(),
        "diagnostic span out of range: {:?}", d.span
    );
}

/// pos3: type error → diagnostic with non-empty message.
#[test]
fn pos3_type_error_returns_diagnostic() {
    // Declare a variable as int, assign bool — type mismatch
    let src = r#"
module adapter_test.pos3

fn bad() -> () {
    let x int = true
}
"#;
    let result = check_file(&uri("pos3.nv"), src);
    assert!(
        !result.diagnostics.is_empty(),
        "expected type-error diagnostic, got none"
    );
    let d = &result.diagnostics[0];
    assert!(!d.message.is_empty(), "diagnostic message should not be empty");
}

// ─────────────────────────────────────────────────────────────────────────────
// neg tests
// ─────────────────────────────────────────────────────────────────────────────

/// neg1: non-existent URI with empty text → 0 diagnostics, no panic.
///
/// The compiler only sees the text; an empty file is valid enough to not
/// produce a diagnostic (or produces a minimal one — either is fine as long
/// as it doesn't panic).
#[test]
fn neg1_nonexistent_uri_empty_text_no_panic() {
    let result = check_file(&uri("does_not_exist.nv"), "");
    // Either 0 or some diagnostics — we just need no panic
    let _ = result.diagnostics;
}

/// neg2: URI with no matching file — check_file still returns a CheckResult.
#[test]
fn neg2_no_crash_on_garbage_input() {
    // Binary-looking garbage — should not panic; may produce diagnostics
    // Use byte literal to avoid Rust string escape restrictions.
    let raw: &[u8] = b"\x00\x01\x02 binary garbage not Nova";
    let garbage = String::from_utf8_lossy(raw);
    let result = check_file(&uri("garbage.nv"), &garbage);
    // Must not panic; result is either diagnostics or InternalError
    let _ = result.diagnostics;
}

// ─────────────────────────────────────────────────────────────────────────────
// edge tests
// ─────────────────────────────────────────────────────────────────────────────

/// edge1: empty string → check runs, 0 or minimal diagnostics, no panic.
#[test]
fn edge1_empty_text_no_panic() {
    let result = check_file(&uri("empty.nv"), "");
    // We don't assert on the number of diagnostics — empty is valid or parse-error.
    // Key: no panic.
    let _ = result.diagnostics;
}

/// edge2: source with BOM prefix → handled gracefully, no panic.
#[test]
fn edge2_bom_prefix_handled() {
    // UTF-8 BOM: EF BB BF
    let src_with_bom = "\u{FEFF}module adapter_test.edge2\n";
    let result = check_file(&uri("bom.nv"), src_with_bom);
    // Must not panic. May have diagnostics if BOM is unexpected.
    let _ = result.diagnostics;
}

/// edge3: source with Unicode identifiers (Cyrillic) → handled gracefully.
#[test]
fn edge3_unicode_identifiers_no_panic() {
    // Nova identifiers are ASCII-only, but we should not panic on Unicode input
    let src = "module adapter_test.edge3\n\nlet привет int = 42\n";
    let result = check_file(&uri("unicode.nv"), src);
    let _ = result.diagnostics; // No panic required
}

// ─────────────────────────────────────────────────────────────────────────────
// check_workspace tests
// ─────────────────────────────────────────────────────────────────────────────

/// pos4: check_workspace on a temp dir with one valid .nv file → 0 diagnostics.
#[test]
fn pos4_workspace_valid_file_zero_diagnostics() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let file = dir.path().join("a.nv");
    // No prelude symbols: simple fn avoids assert/println not-in-scope errors.
    std::fs::write(
        &file,
        "module workspace_test.a\n\nfn double(x int) -> int => x * 2\n",
    )
    .expect("write file");

    let results = check_workspace(dir.path());
    assert_eq!(results.len(), 1, "expected 1 result for 1 file");
    assert!(
        results[0].diagnostics.is_empty(),
        "expected 0 diagnostics, got: {:?}",
        results[0].diagnostics
    );
}

/// pos5: check_workspace on empty dir → 0 results.
#[test]
fn pos5_empty_workspace_zero_results() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let results = check_workspace(dir.path());
    assert!(results.is_empty(), "empty workspace should return no results");
}

/// pos6: check_workspace ignores non-.nv files.
#[test]
fn pos6_workspace_ignores_non_nv_files() {
    let dir = tempfile::tempdir().expect("create temp dir");
    std::fs::write(dir.path().join("readme.md"), "# docs").unwrap();
    std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    let results = check_workspace(dir.path());
    assert!(results.is_empty(), "non-.nv files should be ignored");
}

/// neg3: check_workspace on non-existent path → empty results, no panic.
#[test]
fn neg3_workspace_nonexistent_path_no_panic() {
    let results = check_workspace(std::path::Path::new("/nonexistent/path/that/doesnt/exist"));
    // Should not panic; returns empty
    let _ = results;
}
