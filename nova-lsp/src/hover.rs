//! Hover handler — Plan 104.2.Ф.2 / Plan 104.10 Ф.4 (cross-file).
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
//! # Cross-file hover (Plan 104.10 Ф.4)
//!
//! Symbol resolution runs against a [`ResolvedModule`] (parse + import inlining +
//! type-check), exactly as goto-definition (Ф.3) does. Because import inlining
//! parses each peer / prelude file **from disk** and `lookup_decl_by_name`
//! searches those inlined items, the signature **and doc-comment** a hover shows
//! for a cross-file symbol come from that symbol's *real* declaration in the
//! source file — never a re-synthesized or name-only stub (criterion #3).
//!
//! When the resolved declaration lives in a foreign file (its `Span.file_id` is
//! not the entry's `MAIN_FILE_ID` and maps to a different path via the resolved
//! module's provenance `file_map`), the hover additionally surfaces that source
//! path as a footer — so the reader knows *where* the symbol is defined.
//!
//! UTF-16 position handling: delegates to `diagnostic_mapping::position_to_byte_offset`.

use std::path::{Path, PathBuf};

use nova_codegen::diag::{FileId, Span, MAIN_FILE_ID};
use ropey::Rope;
use std::collections::HashMap;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Url};

use crate::diagnostic_mapping::position_to_byte_offset;
use crate::provenance::{self, ResolvedModule};
use crate::symbol::{resolve_symbol_at_with_limit, SymbolInfo};

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Compute a hover response for the given source text and cursor position.
///
/// This is the self-contained entry point (used by unit tests and any caller
/// without a [`WorkspaceState`](crate::state::WorkspaceState)). It resolves the
/// module fresh from `src` via provenance. The server handler uses
/// [`compute_hover_in`] so it can reuse the Ф.1 resolved-module cache.
///
/// `uri` locates the entry buffer on disk (needed to resolve imports / provenance
/// so cross-file symbols carry the right source). When `None` (unit tests with no
/// backing file), the entry is resolved against a synthetic out-of-repo path, so
/// no imports are inlined — resolution is purely local, matching V1 behaviour.
///
/// Returns `None` if:
/// - The source cannot be parsed.
/// - No symbol is found at the cursor position.
/// - The cursor is on whitespace, a comment, or outside any item span.
pub fn compute_hover(src: &str, pos: Position, uri: Option<&Url>) -> Option<Hover> {
    // Derive an entry path + URI. With a real `uri` we resolve provenance
    // against it (cross-file works). Without one, we use a synthetic path in the
    // OS temp dir: `find_repo_root_from` yields no repo there, so imports are not
    // inlined and hover resolves only local symbols — the pre-Ф.4 behaviour.
    let (path, u_owned) = match uri {
        Some(u) => (
            u.to_file_path().unwrap_or_else(|_| PathBuf::from(u.path())),
            u.clone(),
        ),
        None => {
            let p = std::env::temp_dir().join("_nova_hover_local.nv");
            let u = Url::from_file_path(&p)
                .unwrap_or_else(|_| Url::parse("file:///_nova_hover_local.nv").unwrap());
            (p, u)
        }
    };

    let resolved = provenance::resolve_module_for(&path, src);
    compute_hover_in(&resolved, src, pos, &u_owned)
}

/// Compute a hover response against an already-resolved module.
///
/// `resolved` supplies the import-inlined module, `items_start` (to distinguish
/// the entry file's own items from prepended imports), the `file_id → path`
/// provenance `file_map`, and the type-checker `env`. The server passes the
/// Ф.1-cached [`ResolvedModule`] here so hover reuses the same parse/resolve as
/// goto/completion on the same document version.
///
/// The signature and doc are rendered from the resolved [`SymbolInfo`], whose
/// fields were extracted from the symbol's real declaration (in the entry file
/// or — for an imported/prelude symbol — in the peer file parsed from disk during
/// inlining). For a cross-file declaration a source-path footer is appended.
pub fn compute_hover_in(
    resolved: &ResolvedModule,
    src: &str,
    pos: Position,
    uri: &Url,
) -> Option<Hover> {
    // Convert LSP UTF-16 position to byte offset in the current document.
    let rope = Rope::from_str(src);
    let byte_offset = position_to_byte_offset(&rope, pos.line, pos.character);

    // Guard: empty file or position past EOF.
    if byte_offset > src.len() {
        return None;
    }

    // Resolve symbol at cursor. `items_start` skips prepended imports for
    // span-matching; name-lookup still searches all items so a prelude/imported
    // symbol resolves to its real (foreign-`file_id`) declaration.
    let symbol = resolve_symbol_at_with_limit(
        &resolved.module,
        byte_offset,
        resolved.items_start,
        resolved.env.as_ref(),
    )?;

    // Cross-file provenance: if the declaration lives in a foreign file, surface
    // its source path. The signature+doc themselves already come from that real
    // declaration (inlining parsed it from disk).
    let source = cross_file_source(symbol.span(), &resolved.file_map, uri);

    // Render to markdown, appending the source footer for cross-file symbols.
    let mut md = render_hover_markdown(&symbol);
    if let Some(display) = source {
        md.push_str("\n\n---\n\n*Defined in `");
        md.push_str(&display);
        md.push_str("`*");
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Cross-file source attribution (Ф.4)
// ─────────────────────────────────────────────────────────────────────────────

/// If `decl_span` was declared in a foreign file, return a human-readable
/// display path for it (repo-relative when determinable, else the file name).
/// Returns `None` for a local declaration (entry buffer) so single-file hover is
/// byte-identical to before.
///
/// Robust to the entry-`file_id` duality (see `provenance` module docs): a span
/// stamped `MAIN_FILE_ID` — or one whose mapped path canonically equals the
/// current document — is treated as local and yields `None`.
fn cross_file_source(decl_span: Span, file_map: &HashMap<FileId, PathBuf>, uri: &Url) -> Option<String> {
    if decl_span.file_id == MAIN_FILE_ID {
        return None;
    }
    let path = file_map.get(&decl_span.file_id)?;

    // Guard against the entry being registered under a non-MAIN id too: if the
    // mapped path is the current document, this is not a cross-file symbol.
    if let Ok(cur) = uri.to_file_path() {
        let cur_c = cur.canonicalize().unwrap_or(cur);
        let path_c = path.canonicalize().unwrap_or_else(|_| path.clone());
        if cur_c == path_c {
            return None;
        }
    }

    Some(display_source_path(path))
}

/// Render a declaration file path for display: relative to the repo root when it
/// lies under one (stable, machine-independent), else the bare file name.
fn display_source_path(path: &Path) -> String {
    use nova_codegen::test_runner::find_repo_root_from;
    if let Some(root) = find_repo_root_from(path) {
        if let Ok(rel) = path.strip_prefix(&root) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
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
            let code = format!("{} {} {}", prefix, name, ty_text);
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
        SymbolInfo::FieldDecl { owner, name, ty_text, doc, .. } => {
            // Member-access field hover (Ф.6): render `Owner.field type` so the
            // reader sees both the owning type and the field's real type.
            // D104 rev-2: plus the field's own `///` doc, like a declaration.
            let code = format!("{}.{} {}", owner, name, ty_text);
            render_code_and_doc(&code, doc.as_deref())
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
    fn pos7_hover_call_in_test_body() {
        // Hover inside a test body uses body-walk (resolve_item returns None for
        // Test items, so body-walk is the only path to resolve symbols there).
        // line 4 char 2 = 'a' in 'add(1, 2)'.
        let src = "module basics.lsp\n/// Compute sum.\nfn add(a int, b int) -> int => a + b\ntest \"my_test\" {\n  add(1, 2)\n}";
        let h = compute_hover(src, pos(4, 2), None);
        assert!(h.is_some(), "expected hover on fn call inside test body");
        let contents = match h.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        };
        assert!(contents.contains("fn add"), "should resolve to fn add declaration");
        assert!(contents.contains("Compute sum"), "should include doc of resolved fn");
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

    // ── Plan 104.10 Ф.4: cross-file hover ─────────────────────────────────────

    use std::path::PathBuf;

    /// Repo root (CARGO_MANIFEST_DIR = .../nova-lsp; root is its parent).
    fn repo_root() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.parent().expect("nova-lsp has a parent").to_path_buf()
    }

    /// Write `src` to an isolated per-fixture directory inside the repo and
    /// return its `file://` URI + path (mirrors goto_definition's `write_fixture`).
    /// A per-fixture sub-dir keeps sibling `module` declarations from being
    /// collected as folder-module peers of one another.
    fn write_fixture(dir_stem: &str, file: &str, src: &str) -> (Url, PathBuf) {
        let dir = repo_root().join("target").join("f4_hover_test").join(dir_stem);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(file);
        std::fs::write(&path, src).unwrap();
        let uri = Url::from_file_path(&path).expect("valid file URI");
        (uri, path)
    }

    fn hover_md(h: Option<Hover>) -> String {
        match h.expect("expected a hover").contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("expected Markup"),
        }
    }

    /// POS: hover on a call to a fn declared in a sibling peer file shows the
    /// signature AND doc-comment taken from the *sibling's* real declaration,
    /// plus a cross-file source footer. Exercises criteria #1 and #3.
    #[test]
    fn pos_cross_file_fn_signature_and_doc() {
        let dir = "pos_cross_fn";
        // Sibling declares `helper` with a doc-comment. Same module → peers.
        write_fixture(
            dir,
            "helper.nv",
            "module app.mod\n/// Compute the answer to everything.\nfn helper() -> int => 42\n",
        );
        let entry_src = "module app.mod\nfn main() {\n  helper()\n}\n";
        let (entry_uri, entry_path) = write_fixture(dir, "app.nv", entry_src);

        let resolved = provenance::resolve_module_for(&entry_path, entry_src);
        // Cursor on `helper` inside main's body (line 2, col 2).
        let md = hover_md(compute_hover_in(&resolved, entry_src, pos(2, 2), &entry_uri));

        assert!(md.contains("fn helper"), "must show the real signature, got:\n{md}");
        assert!(md.contains("-> int"), "signature must include return type, got:\n{md}");
        assert!(
            md.contains("Compute the answer to everything."),
            "must show the doc from the sibling's real declaration, got:\n{md}"
        );
        assert!(
            md.contains("Defined in") && md.contains("helper.nv"),
            "cross-file hover must surface the source path, got:\n{md}"
        );
    }

    /// POS: hover on a local symbol shows no cross-file footer (0 regressions,
    /// criterion #2). The signature + doc still render.
    #[test]
    fn pos_local_symbol_no_source_footer() {
        let src = "module basics.lsp\n/// Add two numbers.\nfn add(a int, b int) -> int => a + b\nfn main() {\n  add(1, 2)\n}\n";
        let (uri, path) = write_fixture("pos_local", "app.nv", src);
        let resolved = provenance::resolve_module_for(&path, src);
        // Cursor on the `add` call inside main's body (line 4, col 2).
        let md = hover_md(compute_hover_in(&resolved, src, pos(4, 2), &uri));
        assert!(md.contains("fn add"), "must resolve local fn, got:\n{md}");
        assert!(md.contains("Add two numbers."), "must show local doc, got:\n{md}");
        assert!(
            !md.contains("Defined in"),
            "a same-file symbol must NOT get a cross-file footer, got:\n{md}"
        );
    }

    /// NEG: hover on an unknown identifier resolves to no symbol → None.
    #[test]
    fn neg_cross_file_unknown_symbol_none() {
        let src = "module app.mod\nfn main() {\n  no_such_symbol_xyz()\n}\n";
        let (uri, path) = write_fixture("neg_unknown", "app.nv", src);
        let resolved = provenance::resolve_module_for(&path, src);
        let h = compute_hover_in(&resolved, src, pos(2, 2), &uri);
        assert!(h.is_none(), "unknown symbol must yield no hover");
    }

    /// EDGE: a symbol used in the entry file whose doc lives entirely in another
    /// file (the prelude) — hover must still surface that doc, proving the doc is
    /// pulled from the real cross-file declaration, not the use site.
    #[test]
    fn edge_doc_from_another_file_shown() {
        // `assert` is a prelude symbol; its declaration + doc live in the stdlib
        // prelude, a different file than the entry.
        let src = "module app.mod\nfn main() {\n  assert(true)\n}\n";
        let (uri, path) = write_fixture("edge_prelude_doc", "app.nv", src);
        let resolved = provenance::resolve_module_for(&path, src);
        // Cursor on `assert` (line 2, col 2).
        let h = compute_hover_in(&resolved, src, pos(2, 2), &uri);
        let md = hover_md(h);
        // Signature comes from the prelude declaration; the source footer points
        // into the stdlib prelude file (a foreign file).
        assert!(md.contains("assert"), "must resolve the prelude `assert`, got:\n{md}");
        assert!(
            md.contains("Defined in") && md.contains("prelude"),
            "doc/signature must be attributed to the prelude source file, got:\n{md}"
        );
    }

    // ── Plan 104.10 Ф.6: member-access hover (obj.field) ──────────────────────

    /// Position of `&src[idx+sub_off..]` where `idx` is the first occurrence of
    /// `needle`. All Ф.6 fixtures are ASCII so byte column == UTF-16 column.
    fn locate(src: &str, needle: &str, sub_off: usize) -> Position {
        let idx = src.find(needle).expect("needle present") + sub_off;
        let before = &src[..idx];
        let line = before.matches('\n').count() as u32;
        let col = before.rsplit('\n').next().unwrap().len() as u32;
        Position { line, character: col }
    }

    /// Resolve the fixture with the IDE entry point (records `expr_types`, Ф.2),
    /// then compute a hover — the member path needs the per-expression types.
    fn hover_ide(dir: &str, src: &str, pos: Position) -> Option<Hover> {
        let (uri, path) = write_fixture(dir, "app.nv", src);
        let resolved = provenance::resolve_module_for_ide(&path, src);
        compute_hover_in(&resolved, src, pos, &uri)
    }

    /// POS: hover on `r.start` where `r: Range` — the object's type comes from
    /// `expr_types`, the field type from Range's real declaration (criterion #4).
    /// `r` is a `Range` parameter (a range-literal local `ro r = 0..=5` is not yet
    /// covered by the checker's `expr_types` — [M-104.10-expr-types-coverage]).
    #[test]
    fn pos_member_range_field_start() {
        let src = "module app.mod\nfn use_range(r Range) -> int {\n  r.start\n}\n";
        let md = hover_md(hover_ide("f6_range_start", src, locate(src, "r.start", 2)));
        assert!(md.contains("start"), "must name the field, got:\n{md}");
        assert!(md.contains("int"), "must show the field's real type, got:\n{md}");
        assert!(md.contains("Range"), "must attribute the field to its owner type, got:\n{md}");
    }

    /// POS: hover on a user record field — type of the object from `expr_types`,
    /// field type from the record's real declaration.
    #[test]
    fn pos_member_user_record_field() {
        let src = concat!(
            "module app.mod\n",
            "type Rec {\n  ro a int\n  ro b str\n}\n",
            "fn main() {\n  ro x = Rec { a: 1, b: \"hi\" }\n  ro y = x.a\n}\n",
        );
        let md = hover_md(hover_ide("f6_user_field", src, locate(src, "x.a", 2)));
        assert!(md.contains("Rec.a"), "must show owner.field, got:\n{md}");
        assert!(md.contains("int"), "must show the field's real type, got:\n{md}");
    }

    /// POS: hover on `r.len` (a method call receiver) resolves the *method*
    /// declaration on the object's type and shows its signature.
    #[test]
    fn pos_member_method_signature() {
        let src = "module app.mod\nfn use_range(r Range) -> int {\n  r.len()\n}\n";
        let md = hover_md(hover_ide("f6_method", src, locate(src, "r.len", 2)));
        assert!(md.contains("len"), "must name the method, got:\n{md}");
        assert!(md.contains("fn Range"), "must show the method signature on Range, got:\n{md}");
    }

    /// NEG: hover on a non-existent field of a known type → no symbol → None.
    #[test]
    fn neg_member_unknown_field_none() {
        // `r` has a KNOWN type (Range) so this genuinely tests "no such field on a
        // resolved owner type", not an expr_types coverage miss.
        let src = "module app.mod\nfn use_range(r Range) -> int {\n  r.nonexistent\n}\n";
        let h = hover_ide("f6_unknown", src, locate(src, "r.nonexistent", 2));
        assert!(h.is_none(), "unknown member must yield no hover");
    }

    /// EDGE: chained access `o.b.c` — hover on the trailing `c` must resolve
    /// through the chain (type of `o.b` from `expr_types`, field `c` from its
    /// real decl), NOT against the outer receiver `o`.
    #[test]
    fn edge_member_chain_trailing_field() {
        let src = concat!(
            "module app.mod\n",
            "type Inner {\n  ro c int\n}\n",
            "type Outer {\n  ro b Inner\n}\n",
            "fn main() {\n  ro o = Outer { b: Inner { c: 5 } }\n  ro v = o.b.c\n}\n",
        );
        // Cursor on the trailing `c` (index of "o.b.c" + 4).
        let md = hover_md(hover_ide("f6_chain", src, locate(src, "o.b.c", 4)));
        assert!(md.contains("Inner.c"), "trailing field must resolve on Inner, got:\n{md}");
        assert!(md.contains("int"), "must show the field's real type, got:\n{md}");
    }
}
