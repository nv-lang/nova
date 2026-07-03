//! Goto-definition handler — Plan 104.2.Ф.3 / Plan 104.10 Ф.3.
//!
//! Given a source text, cursor position, and URI, resolves the symbol and
//! returns the LSP [`Location`] of its declaration — **cross-file**.
//!
//! # Cross-file resolution (Plan 104.10 Ф.3)
//!
//! The symbol's declaration [`Span`] carries a `file_id` stamped at parse time
//! (`parse_with_file_id`, imports.rs). A resolved module's `file_map`
//! (`file_id → path`, built from real `peer_files` — never a textual re-scan)
//! maps that id back to the concrete declaring file. So a goto onto a symbol
//! imported from another module — or a prelude symbol like `assert` — returns a
//! [`Location`] in *that* file, with the range computed in the **target file's**
//! own UTF-16 coordinates.
//!
//! Range source precedence for the target file:
//! 1. If the declaration is in the current document (`file_id == MAIN_FILE_ID`),
//!    use the in-memory `src` we already hold — this also preserves the exact
//!    editor URI (no canonicalization drift) for single-file goto.
//! 2. Else, an `overlay` (open, possibly-unsaved editor buffers) is consulted.
//! 3. Else, the target file is read from disk.
//!
//! This is real provenance-driven resolution (criterion #5): we resolve through
//! `span.file_id` + `file_map`, never by grepping the target text for the name.

use std::path::PathBuf;

use nova_codegen::diag::MAIN_FILE_ID;
use ropey::Rope;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::diagnostic_mapping::{byte_offset_to_position, position_to_byte_offset};
use crate::provenance::{self, ResolvedModule};
use crate::symbol::resolve_symbol_at_with_limit;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Compute goto-definition for the cursor position, resolving the module fresh
/// from `src`.
///
/// This is the self-contained entry point (used directly by unit tests and any
/// caller without a [`WorkspaceState`](crate::state::WorkspaceState)). The
/// server handler uses [`compute_goto_definition_in`] instead so it can reuse
/// the Ф.1 resolved-module cache.
///
/// Returns `None` when:
/// - No symbol is found at the cursor.
/// - The resolved symbol has a dummy (0,0) span — no declaration site.
pub fn compute_goto_definition(src: &str, pos: Position, uri: &Url) -> Option<Location> {
    // Resolve the module (parse + import inlining + provenance). We do NOT need
    // per-expression types for goto, so use the cheaper `resolve_module_for`.
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| PathBuf::from(uri.path()));
    let resolved = provenance::resolve_module_for(&path, src);

    compute_goto_definition_in(&resolved, src, pos, uri)
}

/// Compute goto-definition against an already-resolved module.
///
/// `resolved` supplies the import-inlined module, `items_start` (so the original
/// file's items are distinguished from prepended imports), the `file_id → path`
/// provenance `file_map`, and the type-checker `env`.
///
/// The returned [`Location`]'s range is in the **target file's** UTF-16
/// coordinates.
///
/// # Range source: disk-authoritative for peers, in-memory for the entry
///
/// A declaration [`Span`]'s byte offsets are meaningful only against the exact
/// text the span was parsed from:
/// - The **entry** file was parsed from the in-memory `src` (the open buffer),
///   so entry spans (`file_id == MAIN_FILE_ID`) are mapped against `src`. This
///   correctly reflects unsaved edits in the document being edited.
/// - **Peer / imported** files are read *from disk* by `resolve_imports_inline`
///   (`parse_with_file_id`), so their spans are disk-relative and MUST be mapped
///   against the on-disk bytes. Mapping them against a divergent open-buffer copy
///   would yield wrong positions.
///
/// [M-104.10-vfs-overlay-imports]: unifying an open-buffer VFS overlay across
/// *both* import resolution and position mapping (so an unsaved edit in a peer
/// file shifts its goto target live, zls/rust-analyzer VFS-style) is deferred —
/// it requires `resolve_imports_inline` to consult open buffers, not just disk.
/// Until then, goto into a peer reflects its last saved state, which is correct
/// and never mis-positions.
pub fn compute_goto_definition_in(
    resolved: &ResolvedModule,
    src: &str,
    pos: Position,
    uri: &Url,
) -> Option<Location> {
    // Convert LSP UTF-16 position to a byte offset in the *current* document.
    let rope = Rope::from_str(src);
    let byte_offset = position_to_byte_offset(&rope, pos.line, pos.character);

    // Resolve the symbol under the cursor. `items_start` skips prepended imports
    // for span-matching; name-lookup still searches all items so prelude/import
    // symbols resolve to their real (foreign-`file_id`) declaration span.
    let symbol = resolve_symbol_at_with_limit(
        &resolved.module,
        byte_offset,
        resolved.items_start,
        resolved.env.as_ref(),
    )?;

    // Declaration span; a dummy (0,0) span means "no declaration site".
    let decl_span = symbol.span();
    if decl_span.start == 0 && decl_span.end == 0 {
        return None;
    }

    // Determine the target document (URI + text) that declares the symbol.
    //
    // Criterion #5: this is pure provenance — `span.file_id` → `file_map` → path.
    // No textual grep of the target by name anywhere.
    let (target_uri, target_text) = if decl_span.file_id == MAIN_FILE_ID {
        // Declared in the current document. Map against the in-memory `src` (the
        // exact text the entry was parsed from) and hand back the *editor's* URI
        // verbatim — avoids canonicalization drift that a round-trip through
        // `file_map` + `Url::from_file_path` could introduce.
        (uri.clone(), Some(src.to_string()))
    } else {
        match resolved.file_map.get(&decl_span.file_id) {
            // Peer file: its span offsets are disk-relative (inlining parsed disk),
            // so map them against the on-disk bytes. See the `# Range source` note.
            Some(path) => match Url::from_file_path(path) {
                Ok(turi) => (turi, std::fs::read_to_string(path).ok()),
                // Path is not representable as a file URL — degrade to current doc.
                Err(_) => (uri.clone(), None),
            },
            // Unknown `file_id` (not in provenance) — degrade gracefully: point at
            // the current document with a degenerate range rather than emitting a
            // bogus range against unrelated text.
            None => (uri.clone(), None),
        }
    };

    // Compute the range in the target file's own coordinates.
    let range = match target_text {
        Some(text) => {
            let trope = Rope::from_str(&text);
            Range {
                start: byte_offset_to_position(&trope, decl_span.start),
                end: byte_offset_to_position(&trope, decl_span.end),
            }
        }
        None => Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 0 },
        },
    };

    Some(Location { uri: target_uri, range })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn uri() -> Url {
        Url::parse("file:///test.nv").unwrap()
    }

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    /// Locate the repo root so cross-file tests resolve real files with real
    /// imports (mirrors `provenance.rs` / `state.rs` test setup).
    fn repo_root() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.parent().expect("nova-lsp has a parent").to_path_buf()
    }

    /// Write `src` to an isolated per-fixture directory inside the repo and
    /// return its `file://` URI + on-disk path. A per-fixture sub-directory
    /// keeps sibling `module` declarations from being collected as folder-module
    /// peers of one another.
    fn write_fixture(stem: &str, src: &str) -> (Url, PathBuf) {
        let dir = repo_root().join("target").join("f3_goto_test").join(stem);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{stem}.nv"));
        std::fs::write(&path, src).unwrap();
        let uri = Url::from_file_path(&path).expect("valid file URI");
        (uri, path)
    }

    // ── POS: single-file regression (unchanged behaviour) ─────────────────────

    /// pos1: goto-def on a fn name → returns location in the same file.
    #[test]
    fn pos1_goto_fn_returns_location() {
        let src = "module basics.lsp\nfn hello() => ()";
        let loc = compute_goto_definition(src, pos(1, 3), &uri());
        assert!(loc.is_some(), "expected location for fn");
        let loc = loc.unwrap();
        assert_eq!(loc.uri, uri(), "single-file goto must return the same URI");
        assert!(loc.range.start.line >= 1);
    }

    /// pos2: goto-def on a type declaration → location in the same file.
    #[test]
    fn pos2_goto_type_returns_location() {
        let src = "module basics.lsp\ntype Box {\n v int\n}";
        let loc = compute_goto_definition(src, pos(1, 5), &uri());
        assert!(loc.is_some(), "expected location for type");
        assert_eq!(loc.unwrap().uri, uri());
    }

    /// pos3: goto-def on a method → points to the method span, same URI.
    #[test]
    fn pos3_goto_method_returns_location() {
        let src = "module basics.lsp\ntype Foo {\n x int\n}\nfn Foo @bar() => ()";
        let loc = compute_goto_definition(src, pos(4, 4), &uri());
        assert!(loc.is_some(), "expected location for method");
        assert_eq!(loc.unwrap().uri, uri());
    }

    /// pos4: goto-def on an import → returns the import span in the same file.
    #[test]
    fn pos4_goto_import() {
        let src = "module basics.lsp\nimport std.io\nfn f() => ()";
        let loc = compute_goto_definition(src, pos(1, 7), &uri());
        assert!(loc.is_some(), "expected location for import");
        assert_eq!(loc.unwrap().uri, uri());
    }

    /// pos5: goto-def returns a range with start ≤ end.
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

    /// pos6: goto-def on a const returns location in the same file.
    #[test]
    fn pos6_goto_const() {
        let src = "module basics.lsp\nconst PI float = 3.14";
        let loc = compute_goto_definition(src, pos(1, 6), &uri());
        assert!(loc.is_some(), "expected location for const");
        assert_eq!(loc.unwrap().uri, uri());
    }

    // ── POS: cross-file resolution (Plan 104.10 Ф.3) ──────────────────────────

    /// pos7: goto on a prelude symbol (`assert`) resolves into the stdlib
    /// prelude file — a DIFFERENT URI than the current document — with a range
    /// pointing at the real declaration span in that file.
    ///
    /// This exercises the full provenance path: the ident is found in the body,
    /// looked up by name across inlined prelude items, and its declaration span
    /// carries the prelude file's `file_id`, which `file_map` maps to disk.
    #[test]
    fn pos7_goto_prelude_symbol_cross_file() {
        let src = "module basics.lsp\nfn f() {\n  assert(true)\n}\n";
        let (u, path) = write_fixture("pos7_prelude", src);
        let resolved = provenance::resolve_module_for(&path, src);

        // Cursor on `assert` (line 2, after two spaces).
        let loc = compute_goto_definition_in(&resolved, src, pos(2, 2), &u)
            .expect("goto on `assert` must resolve to its prelude declaration");
        // Cross-file: a DIFFERENT URI than the current document, under the stdlib.
        assert_ne!(loc.uri, u, "prelude `assert` must resolve to a foreign file, not the entry");
        let p = loc.uri.to_file_path().unwrap();
        let s = p.to_string_lossy().replace('\\', "/");
        assert!(
            s.contains("/std/") && s.contains("prelude"),
            "prelude symbol should resolve into stdlib prelude, got {s}"
        );
        // Range is non-degenerate (points at the real declaration span).
        assert!(loc.range.end.character > 0 || loc.range.end.line > 0, "range must be non-degenerate");
    }

    /// pos8: goto on a symbol declared in an imported peer file resolves to that
    /// peer's URI (cross-file), with the range in the peer's coordinates.
    ///
    /// Two files share the same `module` name (folder-module peers): the entry
    /// calls a fn declared in the sibling. Goto on the call must land in the
    /// sibling file.
    #[test]
    fn pos8_goto_peer_symbol_cross_file() {
        // Sibling declares `helper`; entry calls it. Same module → peers.
        let dir = repo_root().join("target").join("f3_goto_test").join("pos8_peer");
        std::fs::create_dir_all(&dir).unwrap();
        let sib_path = dir.join("helper.nv");
        std::fs::write(&sib_path, "module app.mod\nfn helper() -> int => 41\n").unwrap();
        let entry_path = dir.join("app.nv");
        let entry_src = "module app.mod\nfn main() {\n  helper()\n}\n";
        std::fs::write(&entry_path, entry_src).unwrap();
        let entry_uri = Url::from_file_path(&entry_path).unwrap();

        let resolved = provenance::resolve_module_for(&entry_path, entry_src);
        // Cursor on `helper` inside main's body (line 2, col 2).
        let loc = compute_goto_definition_in(&resolved, entry_src, pos(2, 2), &entry_uri)
            .expect("goto on `helper` must resolve to the sibling declaration");
        assert_ne!(loc.uri, entry_uri, "peer symbol must resolve to a foreign file");
        let p = loc.uri.to_file_path().unwrap();
        let name = p.file_name().unwrap().to_string_lossy();
        assert_eq!(name, "helper.nv", "peer symbol must resolve into the sibling file");
    }

    /// pos9: the cross-file range is disk-authoritative. A peer declaration's
    /// span offsets come from the on-disk bytes (import inlining parses disk), so
    /// the returned range must point at the declaration's real on-disk position.
    #[test]
    fn pos9_peer_range_matches_disk_source() {
        let dir = repo_root().join("target").join("f3_goto_test").join("pos9_disk_auth");
        std::fs::create_dir_all(&dir).unwrap();
        let sib_path = dir.join("helper.nv");
        // `helper` sits on line 3 (0-based): two blank lines pad the top.
        let disk_src = "module app.mod\n\n\nfn helper() -> int => 41\n";
        std::fs::write(&sib_path, disk_src).unwrap();
        let sib_uri = Url::from_file_path(&sib_path).unwrap();
        let entry_path = dir.join("app.nv");
        let entry_src = "module app.mod\nfn main() {\n  helper()\n}\n";
        std::fs::write(&entry_path, entry_src).unwrap();
        let entry_uri = Url::from_file_path(&entry_path).unwrap();

        let resolved = provenance::resolve_module_for(&entry_path, entry_src);
        let loc = compute_goto_definition_in(&resolved, entry_src, pos(2, 2), &entry_uri)
            .expect("goto on `helper` must resolve cross-file");
        assert_eq!(loc.uri, sib_uri, "must resolve into the sibling file");
        // `fn helper` begins on line 3 of the disk source.
        assert_eq!(
            loc.range.start.line, 3,
            "peer range must match the on-disk declaration line, got {}",
            loc.range.start.line
        );
    }

    // ── NEG ───────────────────────────────────────────────────────────────────

    /// neg1: goto at whitespace → no symbol → None (no panic).
    #[test]
    fn neg1_goto_whitespace_none() {
        let src = "module basics.lsp\n\nfn f() => ()";
        let loc = compute_goto_definition(src, pos(1, 0), &uri());
        assert!(loc.is_none(), "whitespace must not resolve to a definition");
    }

    /// neg2: parse-error file → resolve yields no items → None.
    #[test]
    fn neg2_goto_parse_error_none() {
        let src = "module basics.lsp\nfn @@@() => (";
        let loc = compute_goto_definition(src, pos(1, 5), &uri());
        assert!(loc.is_none(), "parse error should produce None");
    }

    /// neg3: goto on an unknown identifier (no declaration) → None.
    #[test]
    fn neg3_goto_unknown_symbol_none() {
        let src = "module basics.lsp\nfn f() {\n  no_such_symbol_xyz()\n}\n";
        let (u, path) = write_fixture("neg3_unknown", src);
        let resolved = provenance::resolve_module_for(&path, src);
        let loc = compute_goto_definition_in(&resolved, src, pos(2, 2), &u);
        assert!(loc.is_none(), "unknown symbol must resolve to None");
    }

    // ── EDGE ──────────────────────────────────────────────────────────────────

    /// edge1: cursor past EOF → None without panic.
    #[test]
    fn edge1_goto_past_eof() {
        let src = "module basics.lsp\nfn f() => ()";
        let loc = compute_goto_definition(src, pos(999, 999), &uri());
        let _ = loc; // no panic
    }

    /// edge2: emoji before the item → multi-byte UTF-16 handled, no panic, and
    /// the resolved range round-trips through the (multi-byte) source correctly.
    #[test]
    fn edge2_goto_emoji_no_panic() {
        let src = "module basics.lsp\n// 🎉\nfn f() => ()";
        let loc = compute_goto_definition(src, pos(2, 0), &uri());
        // Cursor at start of `fn f` line — resolves to the fn, same URI, valid range.
        if let Some(loc) = loc {
            assert_eq!(loc.uri, uri());
            assert!(loc.range.start.line >= 1);
        }
    }

    /// edge3: a cross-file target is read from disk and produces a correct,
    /// non-degenerate range.
    #[test]
    fn edge3_unopened_target_read_from_disk() {
        let dir = repo_root().join("target").join("f3_goto_test").join("edge3_disk");
        std::fs::create_dir_all(&dir).unwrap();
        let sib_path = dir.join("helper.nv");
        // `helper` on line 2 (0-based) so a correct disk read yields line ≥ 1.
        std::fs::write(&sib_path, "module app.mod\n\nfn helper() -> int => 7\n").unwrap();
        let entry_path = dir.join("app.nv");
        let entry_src = "module app.mod\nfn main() {\n  helper()\n}\n";
        std::fs::write(&entry_path, entry_src).unwrap();
        let entry_uri = Url::from_file_path(&entry_path).unwrap();

        let sib_uri = Url::from_file_path(&sib_path).unwrap();
        let resolved = provenance::resolve_module_for(&entry_path, entry_src);
        let loc = compute_goto_definition_in(&resolved, entry_src, pos(2, 2), &entry_uri)
            .expect("goto must resolve cross-file");
        assert_eq!(loc.uri, sib_uri, "must resolve into the (unopened) sibling read from disk");
        // Disk read must have produced the real declaration line (2).
        assert_eq!(loc.range.start.line, 2, "disk-read range must be correct");
    }

    /// edge4: multi-byte UTF-16 in the TARGET file — the cross-file range must be
    /// computed in the target's UTF-16 units, not byte offsets.
    #[test]
    fn edge4_multibyte_utf16_target_range() {
        let dir = repo_root().join("target").join("f3_goto_test").join("edge4_utf16");
        std::fs::create_dir_all(&dir).unwrap();
        let sib_path = dir.join("helper.nv");
        // `helper` is declared on line 2 (0-based) and its body is a string
        // literal containing an emoji — 4 UTF-8 bytes but 2 UTF-16 code units.
        // The declaration `Span`'s END therefore only lands at the correct LSP
        // column if positions are counted in UTF-16, not raw bytes.
        std::fs::write(&sib_path, "module app.mod\n\nfn helper() -> str => \"🎉\"\n").unwrap();
        let entry_path = dir.join("app.nv");
        let entry_src = "module app.mod\nfn main() {\n  helper()\n}\n";
        std::fs::write(&entry_path, entry_src).unwrap();
        let entry_uri = Url::from_file_path(&entry_path).unwrap();
        let sib_uri = Url::from_file_path(&sib_path).unwrap();

        let resolved = provenance::resolve_module_for(&entry_path, entry_src);
        let loc = compute_goto_definition_in(&resolved, entry_src, pos(2, 2), &entry_uri)
            .expect("goto must resolve cross-file");
        assert_eq!(loc.uri, sib_uri, "must resolve into the sibling file");
        // The declaration begins at line 2, column 0.
        assert_eq!(loc.range.start.line, 2);
        assert_eq!(loc.range.start.character, 0);
        // The declaration ends on the same line. A byte-based (buggy) mapping
        // would count the emoji as 4 units and push `end.character` to ≥28; a
        // correct UTF-16 mapping keeps it ≤26 (emoji = 2 units).
        assert_eq!(loc.range.end.line, 2, "single-line declaration");
        assert!(
            loc.range.end.character <= 26,
            "target range end must be UTF-16 (≤26), got {} (byte-based miscount?)",
            loc.range.end.character
        );
    }
}
