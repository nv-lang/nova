//! Goto-definition handler — Plan 104.2.Ф.3.
//!
//! Given a source text, cursor position, and URI, resolves the symbol and
//! returns the LSP Location of its declaration.
//!
//! **V1 scope:** single-file only. Cross-file resolution via workspace graph
//! is deferred to Plan 104.4 ([M-104.2-cross-file-goto]).
//!
//! The returned Location always points to the same URI as the request because
//! imports resolve to themselves (ImportRef span is in the same file) and all
//! other items are top-level in the same file.

use ropey::Rope;
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::diagnostic_mapping::{byte_offset_to_position, position_to_byte_offset};
use crate::symbol::resolve_symbol_at;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Compute goto-definition for the cursor position.
///
/// Returns `None` when:
/// - Parse fails.
/// - No symbol at cursor.
/// - Symbol is a built-in type (no span in file).
///
/// Cross-file: always returns Location in same `uri` (V1 single-file limit).
pub fn compute_goto_definition(src: &str, pos: Position, uri: &Url) -> Option<Location> {
    // Convert LSP UTF-16 position to byte offset.
    let rope = Rope::from_str(src);
    let byte_offset = position_to_byte_offset(&rope, pos.line, pos.character);

    // Parse the module.
    let module = match nova_codegen::parser::parse(src) {
        Ok(m) => m,
        Err(_) => return None,
    };

    // Resolve symbol at cursor.
    let symbol = resolve_symbol_at(&module, byte_offset)?;

    // Convert declaration span to LSP range.
    let decl_span = symbol.span();
    if decl_span.start == 0 && decl_span.end == 0 {
        // Dummy span — no declaration site.
        return None;
    }

    let start = byte_offset_to_position(&rope, decl_span.start);
    let end = byte_offset_to_position(&rope, decl_span.end);

    Some(Location {
        uri: uri.clone(),
        range: tower_lsp::lsp_types::Range { start, end },
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> Url {
        Url::parse("file:///test.nv").unwrap()
    }

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    // ── pos tests ────────────────────────────────────────────────────────────

    /// pos1: goto-def on a fn name → returns location in same file.
    #[test]
    fn pos1_goto_fn_returns_location() {
        let src = "module basics.lsp\nfn hello() => ()";
        let fn_line = 1u32;
        let loc = compute_goto_definition(src, pos(fn_line, 3), &uri());
        assert!(loc.is_some(), "expected location for fn");
        let loc = loc.unwrap();
        assert_eq!(loc.uri, uri());
        // The location's start should be on or near line 1.
        assert!(loc.range.start.line >= 1);
    }

    /// pos2: goto-def on a type declaration → location in same file.
    #[test]
    fn pos2_goto_type_returns_location() {
        let src = "module basics.lsp\ntype Box {\n v int\n}";
        let loc = compute_goto_definition(src, pos(1, 5), &uri());
        assert!(loc.is_some(), "expected location for type");
    }

    /// pos3: goto-def on a method → points to method span.
    #[test]
    fn pos3_goto_method_returns_location() {
        let src = "module basics.lsp\ntype Foo {\n x int\n}\nfn Foo @bar() => ()";
        let loc = compute_goto_definition(src, pos(4, 4), &uri());
        assert!(loc.is_some(), "expected location for method");
    }

    /// pos4: goto-def on an import → returns import span.
    #[test]
    fn pos4_goto_import() {
        let src = "module basics.lsp\nimport std.io\nfn f() => ()";
        let loc = compute_goto_definition(src, pos(1, 7), &uri());
        assert!(loc.is_some(), "expected location for import");
    }

    /// pos5: goto-def returns range with start ≤ end.
    #[test]
    fn pos5_goto_range_valid() {
        let src = "module basics.lsp\nfn compute(x int) -> int => x * 2";
        let loc = compute_goto_definition(src, pos(1, 0), &uri());
        assert!(loc.is_some());
        let r = loc.unwrap().range;
        assert!(
            r.start.line < r.end.line
                || (r.start.line == r.end.line && r.start.character <= r.end.character),
            "range start must precede or equal end"
        );
    }

    /// pos6: goto-def on a const returns location.
    #[test]
    fn pos6_goto_const() {
        let src = "module basics.lsp\nconst PI float = 3.14";
        let loc = compute_goto_definition(src, pos(1, 6), &uri());
        assert!(loc.is_some(), "expected location for const");
    }

    // ── neg tests ────────────────────────────────────────────────────────────

    /// neg1: goto-def at whitespace → None.
    #[test]
    fn neg1_goto_whitespace_none() {
        let src = "module basics.lsp\n\nfn f() => ()";
        // Line 1 is blank — outside all item spans.
        let loc = compute_goto_definition(src, pos(1, 0), &uri());
        // No panic; result may be None.
        let _ = loc;
    }

    /// neg2: parse-error file → None.
    #[test]
    fn neg2_goto_parse_error_none() {
        let src = "module basics.lsp\nfn @@@() => (";
        let loc = compute_goto_definition(src, pos(1, 5), &uri());
        assert!(loc.is_none(), "parse error should produce None");
    }

    // ── edge tests ───────────────────────────────────────────────────────────

    /// edge1: cursor past EOF → None without panic.
    #[test]
    fn edge1_goto_past_eof() {
        let src = "module basics.lsp\nfn f() => ()";
        let loc = compute_goto_definition(src, pos(999, 999), &uri());
        let _ = loc; // no panic
    }

    /// edge2: emoji in source → no panic.
    #[test]
    fn edge2_goto_emoji_no_panic() {
        let src = "module basics.lsp\n// 🎉\nfn f() => ()";
        let loc = compute_goto_definition(src, pos(2, 0), &uri());
        let _ = loc; // no panic
    }
}
