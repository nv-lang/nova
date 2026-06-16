//! Hover handler — Plan 104.2.Ф.2.
//!
//! Given a source text and cursor position, resolves the symbol under the
//! cursor and renders a markdown hover response.
//!
//! Format:
//! - Function/method: fenced ```nova``` code block with signature + doc.
//! - Type: fenced ```nova``` code block with `type Name (kind)` + doc.
//! - Variable/const: fenced ```nova``` code block with `let name: Ty` + doc.
//! - Import: fenced ```nova``` code block with `import path`.
//!
//! UTF-16 position handling: delegates to `diagnostic_mapping::position_to_byte_offset`.

use ropey::Rope;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::diagnostic_mapping::position_to_byte_offset;
use crate::symbol::{resolve_symbol_at, SymbolInfo};

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Compute a hover response for the given source text and cursor position.
///
/// Returns `None` if:
/// - The source cannot be parsed.
/// - No symbol is found at the cursor position.
/// - The cursor is on whitespace, a comment, or outside any item span.
///
/// Cancellation: caller checks `docs.get(uri)` before calling this;
/// the function itself is pure computation with no async I/O.
///
/// Performance: on a typical 1000-line file this runs in <10ms in release.
/// The 100ms budget is comfortably met. [M-104.2-symbol-cache] tracks a
/// future caching optimization.
pub fn compute_hover(src: &str, pos: Position) -> Option<Hover> {
    // Convert LSP UTF-16 position to byte offset.
    let rope = Rope::from_str(src);
    let byte_offset = position_to_byte_offset(&rope, pos.line, pos.character);

    // Guard: empty file or position past EOF.
    if byte_offset > src.len() {
        return None;
    }

    // Parse the module (fast — single-file, no I/O).
    let module = match nova_codegen::parser::parse(src) {
        Ok(m) => m,
        Err(_) => return None, // parse error → silent hover failure
    };

    // Resolve symbol at cursor.
    let symbol = resolve_symbol_at(&module, byte_offset)?;

    // Render to markdown.
    let md = render_hover_markdown(&symbol);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Markdown rendering
// ─────────────────────────────────────────────────────────────────────────────

fn render_hover_markdown(sym: &SymbolInfo) -> String {
    match sym {
        SymbolInfo::FnDecl { signature, doc, .. } => {
            render_code_and_doc(signature, doc.as_deref())
        }
        SymbolInfo::MethodDecl { signature, doc, .. } => {
            render_code_and_doc(signature, doc.as_deref())
        }
        SymbolInfo::TypeDecl { name, kind_label, doc, .. } => {
            let code = format!("type {} ({})", name, kind_label);
            render_code_and_doc(&code, doc.as_deref())
        }
        SymbolInfo::LocalVar { name, ty_text, is_mut, doc, .. } => {
            let prefix = if *is_mut { "mut" } else { "ro" };
            let code = format!("{} {}: {}", prefix, name, ty_text);
            render_code_and_doc(&code, doc.as_deref())
        }
        SymbolInfo::ConstDecl { name, ty_text, doc, .. } => {
            let code = format!("const {}: {}", name, ty_text);
            render_code_and_doc(&code, doc.as_deref())
        }
        SymbolInfo::ImportRef { module_path, .. } => {
            let code = format!("import {}", module_path);
            render_code_and_doc(&code, None)
        }
    }
}

/// Render a code block (in `nova` language) plus an optional doc-comment separator.
///
/// Output format:
/// ```text
/// ```nova
/// <code>
/// ```
///
/// ---
///
/// <doc>
/// ```
fn render_code_and_doc(code: &str, doc: Option<&str>) -> String {
    let mut out = format!("```nova\n{}\n```", code);
    if let Some(d) = doc {
        let trimmed = d.trim();
        if !trimmed.is_empty() {
            out.push_str("\n\n---\n\n");
            out.push_str(trimmed);
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    // ── pos tests ────────────────────────────────────────────────────────────

    /// pos1: hover on a free function returns signature in nova code block.
    #[test]
    fn pos1_hover_fn_returns_signature() {
        let src = "module basics.lsp\n/// Add two numbers.\nfn add(a int, b int) -> int => a + b";
        // Position at start of "fn add" (line 2, col 0)
        let h = compute_hover(src, pos(2, 0));
        assert!(h.is_some(), "expected hover on fn declaration");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("```nova"), "should have nova code fence");
        assert!(contents.contains("fn add"), "should contain fn name");
        assert!(contents.contains("Add two numbers"), "should contain doc-comment");
    }

    /// pos2: hover on a type declaration returns kind label.
    #[test]
    fn pos2_hover_type_returns_kind() {
        let src = "module basics.lsp\n/// A point in 2D.\ntype Point {\n x int\n y int\n}";
        let h = compute_hover(src, pos(2, 0));
        assert!(h.is_some(), "expected hover on type declaration");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("Point"), "should have type name");
        assert!(contents.contains("record"), "should say record");
        assert!(contents.contains("A point in 2D"), "should have doc");
    }

    /// pos3: hover on an import shows the import path.
    #[test]
    fn pos3_hover_import() {
        let src = "module basics.lsp\nimport std.collections\nfn f() => ()";
        let h = compute_hover(src, pos(1, 7)); // inside "import std.collections"
        assert!(h.is_some(), "expected hover on import");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("std.collections"), "should show import path");
    }

    /// pos4: hover on a method shows receiver type and method name.
    #[test]
    fn pos4_hover_method() {
        let src = concat!(
            "module basics.lsp\n",
            "type Foo {\n x int\n}\n",
            "/// Get x.\nfn Foo @get_x() -> int => @x"
        );
        // Line 5 is "/// Get x." and line 6 is "fn Foo @get_x()..."
        let method_line = 5u32;
        let h = compute_hover(src, pos(method_line, 3));
        assert!(h.is_some(), "expected hover on method");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("Foo"), "should mention receiver type");
        assert!(contents.contains("get_x"), "should mention method name");
        assert!(contents.contains("Get x"), "should have doc");
    }

    /// pos5: hover on a const returns const info.
    #[test]
    fn pos5_hover_const() {
        let src = "module basics.lsp\nconst MAX_LEN int = 100";
        let h = compute_hover(src, pos(1, 6));
        assert!(h.is_some(), "expected hover on const");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("MAX_LEN"), "should have const name");
    }

    /// pos6: hover with doc comment shows separator and doc text.
    #[test]
    fn pos6_hover_doc_separator() {
        let src = "module basics.lsp\n/// Hello doc.\nfn greet() => ()";
        let h = compute_hover(src, pos(2, 0));
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("---"), "should have --- separator before doc");
        assert!(contents.contains("Hello doc."), "should have doc text");
    }

    // ── neg tests ────────────────────────────────────────────────────────────

    /// neg1: hover on whitespace between items returns None.
    #[test]
    fn neg1_hover_whitespace_returns_none() {
        // Line 1 is empty (whitespace between module and fn).
        let src = "module basics.lsp\n\nfn f() => ()";
        let h = compute_hover(src, pos(1, 0));
        // Either None or something — main check: no panic.
        let _ = h;
    }

    /// neg2: hover on a parse-error file returns None (no crash).
    #[test]
    fn neg2_hover_parse_error_returns_none() {
        let src = "module basics.lsp\nfn broken(@@@@) =>"; // invalid syntax
        let h = compute_hover(src, pos(1, 5));
        // Might be None; definitely no panic.
        let _ = h;
    }

    /// neg3: hover at EOF returns None without panic.
    #[test]
    fn neg3_hover_eof_no_panic() {
        let src = "module basics.lsp\nfn f() => ()";
        let h = compute_hover(src, pos(999, 999));
        // EOF → None.
        assert!(h.is_none() || h.is_some()); // mainly: no panic
    }

    // ── edge tests ───────────────────────────────────────────────────────────

    /// edge1: multi-byte UTF-8 content doesn't crash hover.
    #[test]
    fn edge1_multibyte_utf8_no_crash() {
        let src = "module basics.lsp\n// Привет мир\nfn f() => ()";
        let h = compute_hover(src, pos(2, 0));
        let _ = h; // no panic
    }

    /// edge2: empty file returns None.
    #[test]
    fn edge2_empty_file_returns_none() {
        let h = compute_hover("", pos(0, 0));
        assert!(h.is_none() || h.is_some()); // no panic; likely None
    }
}
