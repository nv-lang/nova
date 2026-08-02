//! `source.organizeImports` code action — Plan 104.10 Ф.11.
//!
//! Produces a single [`CodeAction`] of kind [`CodeActionKind::SOURCE_ORGANIZE_IMPORTS`]
//! whose [`WorkspaceEdit`] rewrites the file's leading import block so that:
//!
//! 1. **Unused imports are removed.** An imported binding is "used" iff its
//!    in-scope name (selective item alias/name, module alias, or — for a bare
//!    `import a.b.c` — the last path segment `c`) occurs as an identifier token
//!    anywhere in the file *outside* the import block. Detection is a purely
//!    **textual name-scan** (comments and string literals are stripped first) —
//!    no compiler re-invocation, matching the `code_actions.rs` text-only /
//!    ≤1ms performance contract.
//!    The name-scan is *whole-word textual*, not type-aware: it can
//!    conservatively keep a genuinely-unused import whose binding name coincides
//!    with an unrelated identifier used elsewhere. It never removes a *used*
//!    import (removal requires a total absence of the token). Tracked as
//!    `[M-104.10-organize-imports-namescan]` in `docs/dev/simplifications.md` /
//!    `docs/backlog-followups.md` (P3, IDE-quality follow-up).
//! 2. **Selective imports are pruned per-item.** `import a.b.{Foo, Bar}` with
//!    only `Foo` used becomes `import a.b.{Foo}`; if *no* item is used the whole
//!    statement is dropped.
//! 3. **`export import` re-exports are never removed.** A re-export is part of
//!    the module's public API surface (D29/D288); it is not "unused" merely
//!    because the current file's bodies do not reference it. Re-exports are kept
//!    verbatim (items are not pruned either).
//! 4. **The surviving imports are sorted** by anchor (absolute before relative)
//!    then by dotted path.
//!
//! # Granularity — "unit with leading trivia"
//!
//! Each import statement's *unit* is the physical line(s) it occupies plus any
//! contiguous immediately-preceding attribute (`#…`) or comment (`//…`) lines.
//! Sorting moves the unit as a whole, so a doc-attr / doc-comment travels with
//! the import it annotates rather than being orphaned. Blank lines between
//! imports are collapsed.
//!
//! # Safety
//!
//! If the region spanned by the import block contains a non-blank line that is
//! neither part of an import unit nor trivia (i.e. real code interleaved with
//! imports — not legal top-of-file Nova, but possible in a malformed buffer),
//! the action is suppressed (returns `None`) rather than risk destroying code.
//! Likewise a parse failure or an empty import list yields `None` (no-op).

use std::collections::HashSet;

use nova_codegen::ast::{Import, ImportAnchor};
use ropey::Rope;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Range, TextEdit, WorkspaceEdit,
};

use crate::diagnostic_mapping::byte_offset_to_position;

/// Compute the `source.organizeImports` action for `src`, or `None` when there
/// is nothing to do (no imports, parse failure, interleaved code, or the block
/// is already organized so the edit would be a no-op).
pub fn compute_organize_imports(
    uri: &tower_lsp::lsp_types::Url,
    src: &str,
) -> Option<CodeAction> {
    let module = nova_codegen::parser::parse(src).ok()?;
    if module.imports.is_empty() {
        return None; // NEG: empty import list → no-op.
    }

    // Order imports by source position (folder-module flattening can interleave,
    // though a raw single-file parse is normally already in order).
    let mut imports: Vec<&Import> = module.imports.iter().collect();
    imports.sort_by_key(|imp| imp.span.start);

    // ── Compute each import's unit (line range + leading trivia) ──────────────
    let mut units: Vec<Unit> = Vec::with_capacity(imports.len());
    for imp in &imports {
        let stmt_start = imp.span.start.min(src.len());
        let stmt_end = imp.span.end.min(src.len());
        let unit_start = extend_over_leading_trivia(src, line_start(src, stmt_start));
        let unit_end = line_end(src, stmt_end.saturating_sub(1).max(stmt_start));
        units.push(Unit { imp, stmt_start, stmt_end, unit_start, unit_end });
    }

    let block_start = units.iter().map(|u| u.unit_start).min()?;
    let block_end = units.iter().map(|u| u.unit_end).max()?;

    // ── Safety: reject if real code is interleaved inside the block ───────────
    if interleaved_code_in_block(src, block_start, block_end, &units) {
        return None;
    }

    // ── Name-scan: identifiers used outside the import block ──────────────────
    let used = collect_used_identifiers(src, &units);

    // ── Decide, per import, what survives ─────────────────────────────────────
    let mut kept: Vec<Kept> = Vec::new();
    for u in &units {
        let imp = u.imp;

        // Re-exports are public API — never removed, never pruned.
        if imp.is_export {
            kept.push(Kept { key: sort_key(imp), text: src[u.unit_start..u.unit_end].to_string() });
            continue;
        }

        match &imp.items {
            Some(items) => {
                // Selective import — prune unused items.
                let kept_items: Vec<&nova_codegen::ast::ImportItem> = items
                    .iter()
                    .filter(|it| used.contains(introduced_item_name(it)))
                    .collect();
                if kept_items.is_empty() {
                    continue; // all items unused → drop statement.
                }
                let text = if kept_items.len() == items.len() {
                    src[u.unit_start..u.unit_end].to_string()
                } else {
                    reconstruct_selective(src, u, &kept_items)?
                };
                kept.push(Kept { key: sort_key(imp), text });
            }
            None => {
                // Whole-module import (with or without alias) — keep iff used.
                let name = imp
                    .alias
                    .as_deref()
                    .or_else(|| imp.path.last().map(|s| s.as_str()))
                    .unwrap_or("");
                if used.contains(name) {
                    kept.push(Kept { key: sort_key(imp), text: src[u.unit_start..u.unit_end].to_string() });
                }
            }
        }
    }

    // ── Sort surviving imports ────────────────────────────────────────────────
    kept.sort_by(|a, b| a.key.cmp(&b.key));

    let new_block = kept.iter().map(|k| k.text.as_str()).collect::<Vec<_>>().join("\n");
    let old_block = &src[block_start..block_end];
    if new_block == old_block {
        return None; // Already organized → no-op (don't offer a null edit).
    }

    let rope = Rope::from_str(src);
    let range = Range {
        start: byte_offset_to_position(&rope, block_start),
        end: byte_offset_to_position(&rope, block_end),
    };
    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text: new_block }]);

    Some(CodeAction {
        title: "Organize imports".to_string(),
        kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal
// ─────────────────────────────────────────────────────────────────────────────

/// One import statement together with the physical range of its unit.
struct Unit<'a> {
    imp: &'a Import,
    /// Byte range of the `import`/`export` statement proper (no trivia).
    stmt_start: usize,
    stmt_end: usize,
    /// Byte range of the full unit (leading trivia lines + statement line(s)).
    unit_start: usize,
    unit_end: usize,
}

/// A surviving import, ready to be sorted and re-emitted.
struct Kept {
    key: (u8, String, String),
    text: String,
}

/// The in-scope name introduced by a selective-import item (`alias` if present,
/// else the imported `name`).
fn introduced_item_name(it: &nova_codegen::ast::ImportItem) -> &str {
    it.alias.as_deref().unwrap_or(it.name.as_str())
}

/// Sort key: absolute imports (anchor rank 0) before relative (rank 1), then by
/// dotted path (with anchor prefix), then by the full statement text as a
/// stable tie-breaker.
fn sort_key(imp: &Import) -> (u8, String, String) {
    let (rank, prefix) = match &imp.anchor {
        ImportAnchor::Package => (0u8, String::new()),
        ImportAnchor::Relative { up } => {
            let p = if *up == 0 { "./".to_string() } else { "../".repeat(*up as usize) };
            (1u8, p)
        }
    };
    let path = format!("{}{}", prefix, imp.path.join("."));
    let mut tie = String::new();
    for it in imp.items.iter().flatten() {
        tie.push_str(&it.name);
        if let Some(a) = &it.alias {
            tie.push_str(" as ");
            tie.push_str(a);
        }
        tie.push(',');
    }
    if let Some(a) = &imp.alias {
        tie.push_str(" as ");
        tie.push_str(a);
    }
    (rank, path, tie)
}

/// Rebuild a selective import keeping only `kept_items`, preserving the original
/// path/anchor prefix and any leading trivia verbatim. Returns `None` if the
/// statement text does not contain a `{ … }` list (should not happen for a
/// parsed selective import).
fn reconstruct_selective(
    src: &str,
    u: &Unit,
    kept_items: &[&nova_codegen::ast::ImportItem],
) -> Option<String> {
    let stmt = &src[u.stmt_start..u.stmt_end];
    let brace_rel = stmt.find('{')?;
    let close_rel = stmt.rfind('}')?;
    if close_rel < brace_rel {
        return None;
    }
    let prefix = &stmt[..brace_rel]; // "…import a.b.{"  up to and incl. nothing after '{'? see below
    let suffix = &stmt[close_rel + 1..]; // after '}' (usually empty)
    let inner = kept_items
        .iter()
        .map(|it| src[it.span.start.min(src.len())..it.span.end.min(src.len())].trim())
        .collect::<Vec<_>>()
        .join(", ");
    // Leading trivia + indentation before the statement, preserved verbatim.
    let lead = &src[u.unit_start..u.stmt_start];
    Some(format!("{lead}{prefix}{{{inner}}}{suffix}"))
}

/// Extend `line_start_byte` upward over any contiguous preceding lines that are
/// pure trivia (attribute `#…` or comment `//…`), so a doc-attr / doc-comment
/// stays attached to the import it annotates. Stops at a blank line, the module
/// declaration, another import, or start-of-file.
fn extend_over_leading_trivia(src: &str, line_start_byte: usize) -> usize {
    let mut start = line_start_byte;
    while start > 0 {
        let prev_nl = start - 1; // the '\n' ending the previous line
        let prev_start = src[..prev_nl].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prev = src[prev_start..prev_nl].trim();
        if prev.is_empty() {
            break;
        }
        let is_trivia = prev.starts_with('#') || prev.starts_with("//");
        if !is_trivia {
            break;
        }
        start = prev_start;
    }
    start
}

/// True if any line in `[block_start, block_end]` is non-blank yet lies outside
/// every import unit and is not trivia — i.e. real code interleaved with the
/// imports, which we refuse to reorganize.
fn interleaved_code_in_block(
    src: &str,
    block_start: usize,
    block_end: usize,
    units: &[Unit],
) -> bool {
    let mut pos = block_start;
    while pos < block_end {
        let ls = line_start(src, pos);
        let le = line_end(src, pos);
        let line = src[ls..le].trim();
        let covered = units.iter().any(|u| ls < u.unit_end && le > u.unit_start);
        if !covered && !line.is_empty() {
            let is_trivia = line.starts_with('#') || line.starts_with("//");
            if !is_trivia {
                return true;
            }
        }
        // Advance to the next line (skip the newline).
        pos = if le < src.len() { le + 1 } else { le };
        if pos <= ls {
            break; // guard against non-advance
        }
    }
    false
}

/// Collect the set of identifier tokens that occur in `src` *outside* the import
/// units and outside the leading `module …` declaration line. Comments (`//…`)
/// and string literals (`"…"`) are skipped so a name mentioned only in a string
/// or comment does not count as a use.
fn collect_used_identifiers(src: &str, units: &[Unit]) -> HashSet<String> {
    // Mask out import-unit bytes and the `module …` declaration line with spaces.
    let mut masked: Vec<u8> = src.as_bytes().to_vec();
    for u in units {
        for b in masked.iter_mut().take(u.unit_end.min(src.len())).skip(u.unit_start) {
            *b = b' ';
        }
    }
    // Mask the module declaration line (first line whose trimmed text starts
    // with "module ") so `module a.b` doesn't mark import `b` as used.
    let mut off = 0usize;
    while off < src.len() {
        let le = line_end(src, off);
        if src[off..le].trim_start().starts_with("module ") {
            for b in masked.iter_mut().take(le).skip(off) {
                *b = b' ';
            }
            break;
        }
        off = if le < src.len() { le + 1 } else { le };
    }

    let s = String::from_utf8_lossy(&masked);
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut set = HashSet::new();
    let mut i = 0;
    while i < len {
        let c = bytes[i];
        // Line comment.
        if c == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // String literal (with backslash escapes).
        if c == b'"' {
            i += 1;
            while i < len && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }
        // Identifier.
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            set.insert(s[start..i].to_string());
            continue;
        }
        i += 1;
    }
    set
}

/// Byte offset of the start of the line containing `off`.
fn line_start(src: &str, off: usize) -> usize {
    let off = off.min(src.len());
    src[..off].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Byte offset of the end of the line containing `off` (the position of the next
/// `\n`, or end-of-string) — exclusive of the newline itself.
fn line_end(src: &str, off: usize) -> usize {
    let off = off.min(src.len());
    src[off..].find('\n').map(|i| off + i).unwrap_or(src.len())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> tower_lsp::lsp_types::Url {
        tower_lsp::lsp_types::Url::parse("file:///t.nv").unwrap()
    }

    /// Apply the produced edit to `src` and return the new document text.
    fn apply(src: &str) -> Option<String> {
        let action = compute_organize_imports(&uri(), src)?;
        let edit = action.edit?;
        let changes = edit.changes?;
        let edits = changes.get(&uri())?.clone();
        assert_eq!(edits.len(), 1, "organize emits exactly one TextEdit");
        let te = &edits[0];
        // Translate the LSP range back to byte offsets and splice.
        let rope = Rope::from_str(src);
        let start = crate::diagnostic_mapping::position_to_byte_offset(
            &rope, te.range.start.line, te.range.start.character);
        let end = crate::diagnostic_mapping::position_to_byte_offset(
            &rope, te.range.end.line, te.range.end.character);
        let mut out = String::new();
        out.push_str(&src[..start]);
        out.push_str(&te.new_text);
        out.push_str(&src[end..]);
        Some(out)
    }

    // ── POS ───────────────────────────────────────────────────────────────────

    /// POS: an unused import is removed; a used one is kept.
    #[test]
    fn pos_removes_unused_keeps_used() {
        let src = "\
module app.main
import a.b.{Used}
import c.d.{Unused}
fn main() {
  ro x = Used
}
";
        let out = apply(src).expect("should offer an edit");
        assert!(out.contains("import a.b.{Used}"), "used import kept:\n{out}");
        assert!(!out.contains("Unused"), "unused import removed:\n{out}");
    }

    /// POS: imports are sorted by dotted path.
    #[test]
    fn pos_sorts_imports() {
        let src = "\
module app.main
import zeta.mod.{Z}
import alpha.mod.{A}
fn main() {
  ro x = Z
  ro y = A
}
";
        let out = apply(src).expect("should offer an edit");
        let a = out.find("alpha.mod").unwrap();
        let z = out.find("zeta.mod").unwrap();
        assert!(a < z, "alpha must sort before zeta:\n{out}");
    }

    /// POS: a selective import keeps used items and prunes unused ones.
    #[test]
    fn pos_prunes_unused_items() {
        let src = "\
module app.main
import a.b.{Keep, Drop}
fn main() {
  ro x = Keep
}
";
        let out = apply(src).expect("should offer an edit");
        assert!(out.contains("import a.b.{Keep}"), "kept item survives:\n{out}");
        assert!(!out.contains("Drop"), "unused item pruned:\n{out}");
    }

    /// POS: a bare whole-module import is used via its last-segment namespace.
    #[test]
    fn pos_whole_module_namespace_use() {
        let src = "\
module app.main
import std.collections
import std.unusedmod
fn main() {
  ro m = collections.new_map()
}
";
        let out = apply(src).expect("should offer an edit");
        assert!(out.contains("import std.collections"), "namespace-used kept:\n{out}");
        assert!(!out.contains("unusedmod"), "unused namespace dropped:\n{out}");
    }

    // ── NEG ───────────────────────────────────────────────────────────────────

    /// NEG: a file with no imports yields no action (no-op).
    #[test]
    fn neg_no_imports_no_action() {
        let src = "\
module app.main
fn main() {
  ro x = 1
}
";
        assert!(compute_organize_imports(&uri(), src).is_none(), "no imports → no action");
    }

    /// NEG: an already-organized block (sorted, all used) yields no action.
    #[test]
    fn neg_already_organized_no_action() {
        let src = "\
module app.main
import alpha.mod.{A}
import zeta.mod.{Z}
fn main() {
  ro x = A
  ro y = Z
}
";
        assert!(
            compute_organize_imports(&uri(), src).is_none(),
            "already organized → no-op"
        );
    }

    // ── EDGE ──────────────────────────────────────────────────────────────────

    /// EDGE: `export import` (re-export) is preserved even though the current
    /// file's bodies never reference it.
    #[test]
    fn edge_reexport_preserved() {
        let src = "\
module app.facade
export import std.io.{File, Reader}
import c.d.{Unused}
fn main() {
  ro x = 1
}
";
        let out = apply(src).expect("should offer an edit (drops the unused import)");
        assert!(
            out.contains("export import std.io.{File, Reader}"),
            "re-export must be preserved verbatim:\n{out}"
        );
        assert!(!out.contains("Unused"), "the plain unused import is still removed:\n{out}");
    }

    /// EDGE: a re-export's items are NOT pruned even if unreferenced locally.
    #[test]
    fn edge_reexport_items_not_pruned() {
        let src = "\
module app.facade
export import std.io.{File, Reader, Writer}
fn main() {
  ro x = File
}
";
        // File is referenced but Reader/Writer are not — still, none is pruned.
        assert!(
            compute_organize_imports(&uri(), src).is_none(),
            "re-export items must not be pruned → nothing to do → no-op"
        );
    }

    /// EDGE: real code interleaved with imports suppresses the action (safety).
    #[test]
    fn edge_interleaved_code_suppressed() {
        let src = "\
module app.main
import a.b.{A}
fn helper() => 1
import c.d.{C}
fn main() {
  ro x = A
  ro y = C
}
";
        assert!(
            compute_organize_imports(&uri(), src).is_none(),
            "interleaved code must suppress organize to avoid destroying it"
        );
    }
}
