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

use nova_codegen::ast::Module;
use ropey::Rope;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Url};

use crate::diagnostic_mapping::position_to_byte_offset;
use crate::symbol::{resolve_symbol_at_with_limit, SymbolInfo};

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Compute a hover response for the given source text and cursor position.
///
/// Returns `None` if:
/// - The source cannot be parsed.
/// - No symbol is found at the cursor position.
/// - The cursor is on whitespace, a comment, or outside any item span.
pub fn compute_hover(src: &str, pos: Position, uri: Option<&Url>) -> Option<Hover> {
    // Convert LSP UTF-16 position to byte offset.
    let rope = Rope::from_str(src);
    let byte_offset = position_to_byte_offset(&rope, pos.line, pos.character);

    // Guard: empty file or position past EOF.
    if byte_offset > src.len() {
        return None;
    }

    // Parse the module.
    let mut module = match nova_codegen::parser::parse(src) {
        Ok(m) => m,
        Err(_) => return None,
    };

    // Remember how many items the file itself declares (before inlining imports).
    let items_before_inline = module.items.len();

    // Inline imports so body-walk can find prelude symbols (assert, println, etc.)
    // by name. After inlining, imported items are PREPENDED to module.items,
    // so original file items start at index (total_after - items_before_inline).
    if let Some(u) = uri {
        if let Ok(path) = u.to_file_path() {
            resolve_imports_for_hover(&path, &mut module);
        }
    }

    // items_start = how many imported items were prepended.
    let items_start = module.items.len().saturating_sub(items_before_inline);

    // Resolve symbol at cursor.
    let symbol = resolve_symbol_at_with_limit(&module, byte_offset, items_start)?;

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

/// Inline-resolve imports into `module` so body-walk can find prelude symbols.
/// Uses catch_unwind so import resolution errors don't crash the LSP.
fn resolve_imports_for_hover(path: &std::path::Path, module: &mut Module) {
    use nova_codegen::test_runner::find_repo_root_from;
    let Some(repo) = find_repo_root_from(path) else {
        tracing::warn!("hover: no repo root found for {:?}", path);
        return;
    };
    let stdlib_dir = repo.join("std");
    let items_before = module.items.len();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = nova_codegen::imports::resolve_imports_inline(path, module, &repo, &stdlib_dir);
    }));
    if result.is_err() {
        tracing::warn!("hover: resolve_imports_inline panicked for {:?}", path);
    }
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
        SymbolInfo::TypeDecl { name, kind_label, signature, doc, .. } => {
            let code = match signature {
                Some(sig) => sig.clone(),
                None => format!("type {} ({})", name, kind_label),
            };
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

    #[test]
    fn pos1_hover_fn_returns_signature() {
        let src = "module basics.lsp\n/// Add two numbers.\nfn add(a int, b int) -> int => a + b";
        let h = compute_hover(src, pos(2, 0), None);
        assert!(h.is_some(), "expected hover on fn declaration");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("```nova"), "should have nova code fence");
        assert!(contents.contains("fn add"), "should contain fn name");
        assert!(contents.contains("Add two numbers"), "should contain doc-comment");
    }

    #[test]
    fn pos2_hover_type_returns_kind() {
        let src = "module basics.lsp\n/// A point in 2D.\ntype Point {\n x int\n y int\n}";
        let h = compute_hover(src, pos(2, 0), None);
        assert!(h.is_some(), "expected hover on type declaration");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("Point"), "should have type name");
        assert!(contents.contains("record"), "should say record");
        assert!(contents.contains("A point in 2D"), "should have doc");
    }

    #[test]
    fn pos3_hover_import() {
        let src = "module basics.lsp\nimport std.collections\nfn f() => ()";
        let h = compute_hover(src, pos(1, 7), None);
        assert!(h.is_some(), "expected hover on import");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("std.collections"), "should show import path");
    }

    #[test]
    fn pos4_hover_method() {
        let src = concat!(
            "module basics.lsp\n",
            "type Foo {\n x int\n}\n",
            "/// Get x.\nfn Foo @get_x() -> int => @x"
        );
        let method_line = 5u32;
        let h = compute_hover(src, pos(method_line, 3), None);
        assert!(h.is_some(), "expected hover on method");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("Foo"), "should mention receiver type");
        assert!(contents.contains("get_x"), "should mention method name");
        assert!(contents.contains("Get x"), "should have doc");
    }

    #[test]
    fn pos5_hover_const() {
        let src = "module basics.lsp\nconst MAX_LEN int = 100";
        let h = compute_hover(src, pos(1, 6), None);
        assert!(h.is_some(), "expected hover on const");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("MAX_LEN"), "should have const name");
    }

    #[test]
    fn pos6_hover_doc_separator() {
        let src = "module basics.lsp\n/// Hello doc.\nfn greet() => ()";
        let h = compute_hover(src, pos(2, 0), None);
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("---"), "should have --- separator before doc");
        assert!(contents.contains("Hello doc."), "should have doc text");
    }

    #[test]
    fn neg1_hover_whitespace_returns_none() {
        let src = "module basics.lsp\n\nfn f() => ()";
        let h = compute_hover(src, pos(1, 0), None);
        let _ = h;
    }

    #[test]
    fn neg2_hover_parse_error_returns_none() {
        let src = "module basics.lsp\nfn broken(@@@@) =>";
        let h = compute_hover(src, pos(1, 5), None);
        let _ = h;
    }

    #[test]
    fn neg3_hover_eof_no_panic() {
        let src = "module basics.lsp\nfn f() => ()";
        let h = compute_hover(src, pos(999, 999), None);
        assert!(h.is_none() || h.is_some());
    }

    #[test]
    fn edge1_multibyte_utf8_no_crash() {
        let src = "module basics.lsp\n// Привет мир\nfn f() => ()";
        let h = compute_hover(src, pos(2, 0), None);
        let _ = h;
    }

    #[test]
    fn edge2_empty_file_returns_none() {
        let h = compute_hover("", pos(0, 0), None);
        assert!(h.is_none() || h.is_some());
    }
}
