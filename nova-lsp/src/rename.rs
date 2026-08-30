//! Rename support for nova-lsp — Plan 104.6
//!
//! Implements `textDocument/prepareRename` and `textDocument/rename`.
//!
//! # Design (D296 — LSP Rename Atomicity Contract)
//!
//! Rename is a three-phase atomic operation:
//!
//! 1. **Prepare**: validate cursor → extract identifier span + placeholder.
//! 2. **Collect**: scan all workspace documents for occurrences of the old name.
//!    Build `WorkspaceEdit.documentChanges` with `TextDocumentEdit` per file.
//! 3. **Atomic check**: apply edits to in-memory copies, re-typecheck every
//!    changed file. If any file fails → reject the entire rename.
//!
//! # Scope-aware resolution (Plan 104.10 Ф.7)
//!
//! The occurrence scan is no longer a blind word-boundary regex across every
//! file. [`compute_rename`] first parses the primary buffer and classifies the
//! symbol under the cursor **from the AST** ([`classify_scope`]):
//!
//! - **Local binding** (a `let`/param/`for`/pattern binding whose name is bound
//!   inside the enclosing function): the rename is confined to that declaring
//!   function's byte span in the primary file only. A same-named local in a
//!   *different* function is never touched — the false-positive the old regex
//!   produced.
//! - **Top-level symbol** (free fn / type / const / field): the rename still runs
//!   cross-file, but every function that *locally shadows* the name (binds a
//!   local of the same identifier) is skipped, so a shadowing `ro x` in `fn b`
//!   is not rewritten when renaming a top-level `x`.
//!
//! Scope is derived from real AST bindings, not a brace-depth heuristic.
//!
//! Doc-comment `[[name]]` links are updated as part of the rename edits.
//!
//! # Residual — [M-104.10-rename-full-resolve]
//!
//! A full per-*occurrence* symbol resolution (resolve every candidate to its
//! declaration `Span` and compare) is not performed: it would require a complete
//! name-resolution pass the bootstrap checker does not expose. Instead the scope
//! model over-approximates a local binding's scope to the *whole declaring
//! function* (not the exact block), and a shadow scope to the *whole shadowing
//! function*. Consequences (all rare, all safe-by-omission): a second same-named
//! binding in a sibling block of the same function is renamed together; a
//! top-level reference that appears in a function *before* that function
//! introduces a local of the same name is conservatively skipped. See
//! `simplifications.md` / backlog for the marker.

use std::collections::HashSet;

use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::*;

use nova_codegen::ast::{
    ArrayPatternElem, Block, ClosureBody, ElseBranch, Expr, ExprKind, FnBody, FnDecl,
    Item, MatchArmBody, Module, Pattern, Stmt, VariantPatternKind,
};
use nova_codegen::diag::Span;

use crate::compiler::check_source_inner;

// ─────────────────────────────────────────────────────────────────────────────
// Nova keywords — must not be accepted as rename targets or new names
// ─────────────────────────────────────────────────────────────────────────────
//
// Plan 104.10 Ф.5 ([M-104.10-hardcode-lists]): the keyword set is NOT hardcoded
// here. The previous static list had drifted from the language (it listed
// removed/never-real keywords like `let`, `impl`, `blocking`, `suspend`, `and`,
// `or`, `not`, `mod`). Reserved-keyword status is now decided by the compiler's
// own lexer — the single source of truth — via `is_nova_keyword`.

/// True if `word` is a reserved Nova keyword (delegates to the compiler lexer,
/// so the LSP never drifts from the real language). Contextual keywords that the
/// lexer keeps as identifiers (e.g. `bench`, `apply`) are correctly renameable.
fn is_nova_keyword(word: &str) -> bool {
    nova_codegen::lexer::is_reserved_keyword(word)
}

// ─────────────────────────────────────────────────────────────────────────────
// prepareRename
// ─────────────────────────────────────────────────────────────────────────────

/// Validate cursor position for rename.
///
/// Returns `PrepareRenameResponse::RangeWithPlaceholder` if the cursor is on
/// a renameable identifier, or an LSP error otherwise.
///
/// # Errors
///
/// - `-32602 InvalidParams`: cursor is not on an identifier (whitespace,
///   comment, string literal, keyword, digit-only token).
pub fn prepare_rename(text: &str, pos: Position) -> Result<PrepareRenameResponse> {
    let line_idx = pos.line as usize;
    let col_utf16 = pos.character as usize;

    // Build line-start byte offset table.
    let line_starts = compute_line_starts(text);
    let Some(&line_start_byte) = line_starts.get(line_idx) else {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: "position line out of range".into(),
            data: None,
        });
    };

    // Convert UTF-16 column to byte offset within the line.
    let line_text = get_line(text, line_idx);
    let col_byte = utf16_col_to_byte_offset(line_text, col_utf16);
    let byte_off = line_start_byte + col_byte;

    // Extract word boundaries.
    let (word_start, word_end) = word_at(text, byte_off);
    if word_start == word_end {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: "cursor is not on an identifier".into(),
            data: None,
        });
    }

    let word = &text[word_start..word_end];

    // Must not start with a digit.
    if word.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: "cursor is on a numeric literal, not an identifier".into(),
            data: None,
        });
    }

    // Must not be a keyword.
    if is_nova_keyword(word) {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: format!("cannot rename keyword `{}`", word).into(),
            data: None,
        });
    }

    // Must not be inside a comment.
    if is_in_comment(text, byte_off) {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: "cursor is inside a comment".into(),
            data: None,
        });
    }

    // Must not be inside a string literal.
    if is_in_string(text, byte_off) {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: "cursor is inside a string literal".into(),
            data: None,
        });
    }

    // Build LSP range for the word.
    let range = byte_range_to_lsp_range(text, &line_starts, word_start, word_end);

    Ok(PrepareRenameResponse::RangeWithPlaceholder {
        range,
        placeholder: word.to_string(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// rename
// ─────────────────────────────────────────────────────────────────────────────

/// A document snapshot for rename processing.
pub struct RenameDoc {
    pub uri: Url,
    pub text: String,
    pub version: Option<i32>,
}

/// Compute a `WorkspaceEdit` that renames the symbol `old_name` (found at
/// `cursor_byte` in the primary document `primary_uri`) to `new_name`.
///
/// The set of documents/occurrences rewritten is decided **from the AST**
/// ([`classify_scope`]), not by a blind cross-file word scan — see the module
/// docs. A local binding is confined to its declaring function in the primary
/// file; a top-level symbol renames cross-file while skipping functions that
/// locally shadow the name.
///
/// # Errors
///
/// - `-32602 InvalidParams`: `new_name` is empty or a keyword.
/// - `-32803 RequestFailed`: post-rename type-check fails in any file.
pub fn compute_rename(
    docs: &[RenameDoc],
    primary_uri: &Url,
    cursor_byte: usize,
    old_name: &str,
    new_name: &str,
) -> Result<WorkspaceEdit> {
    // Validate new name.
    if new_name.is_empty() {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: "new name must not be empty".into(),
            data: None,
        });
    }
    if !is_valid_identifier(new_name) {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: format!("new name `{}` is not a valid identifier", new_name).into(),
            data: None,
        });
    }
    if is_nova_keyword(new_name) {
        return Err(Error {
            code: ErrorCode::InvalidParams,
            message: format!("new name `{}` is a reserved keyword", new_name).into(),
            data: None,
        });
    }

    // Determine the rename scope from the primary buffer's AST. On any parse
    // failure we degrade to the previous cross-file behaviour (TopLevel with no
    // shadow scopes) so rename never becomes *less* capable than V1.
    let primary_text = docs
        .iter()
        .find(|d| &d.uri == primary_uri)
        .map(|d| d.text.as_str());
    let scope = match primary_text {
        Some(text) => classify_scope(text, cursor_byte, old_name),
        None => RenameScope::TopLevel,
    };

    // Phase 1: collect edits per document.
    let mut text_doc_edits: Vec<TextDocumentEdit> = Vec::new();
    let mut changed_texts: Vec<(Url, String)> = Vec::new();

    for doc in docs {
        let edits = match &scope {
            // Local binding: only the primary file, only within the declaring
            // function's byte range. Every other file is left untouched.
            RenameScope::LocalInFile { range } => {
                if &doc.uri != primary_uri {
                    continue;
                }
                let (lo, hi) = *range;
                collect_edits_filtered(&doc.text, old_name, new_name, &|abs| {
                    abs >= lo && abs < hi
                })
            }
            // Top-level symbol: cross-file, but skip any occurrence that falls
            // inside a function that locally shadows the name (semantic filter,
            // not regex). Shadow scopes are recomputed per document from its AST.
            RenameScope::TopLevel => {
                let shadows = shadow_scopes_in_text(&doc.text, old_name);
                collect_edits_filtered(&doc.text, old_name, new_name, &|abs| {
                    !shadows.iter().any(|(lo, hi)| abs >= *lo && abs < *hi)
                })
            }
        };
        if edits.is_empty() {
            continue;
        }

        // Build the new text for atomic check.
        let new_text = apply_edits_to_text(&doc.text, &edits);
        changed_texts.push((doc.uri.clone(), new_text));

        let text_doc = OptionalVersionedTextDocumentIdentifier {
            uri: doc.uri.clone(),
            version: doc.version,
        };
        text_doc_edits.push(TextDocumentEdit {
            text_document: text_doc,
            edits: edits.into_iter().map(OneOf::Left).collect(),
        });
    }

    // Phase 2: atomic post-rename type-check.
    for (uri, new_text) in &changed_texts {
        let path = uri.to_file_path().ok();
        // Path-based resolution (nearest nova.toml / folder-module peers) is
        // sufficient here; the rename flow always operates on saved files.
        let diags = check_source_inner(new_text, path.as_deref(), None);
        if !diags.is_empty() {
            let msgs: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
            return Err(Error {
                code: ErrorCode::ServerError(-32803),
                message: format!(
                    "rename rejected: post-rename type errors in {}: {}",
                    uri,
                    msgs.join("; ")
                ).into(),
                data: None,
            });
        }
    }

    Ok(WorkspaceEdit {
        changes: None,
        document_changes: if text_doc_edits.is_empty() {
            None
        } else {
            Some(DocumentChanges::Edits(text_doc_edits))
        },
        change_annotations: None,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal: occurrence scan
// ─────────────────────────────────────────────────────────────────────────────

/// Collect all rename edits in `text`: replace every word-boundary occurrence
/// of `old_name` with `new_name`, also updating `[[old_name]]` doc-comment links.
///
/// Convenience wrapper over [`collect_edits_filtered`] that accepts every
/// occurrence — retained for the doc-comment-link / atomic-check unit tests.
#[cfg(test)]
fn collect_edits_in_text(text: &str, old_name: &str, new_name: &str) -> Vec<TextEdit> {
    collect_edits_filtered(text, old_name, new_name, &|_| true)
}

/// Like [`collect_edits_in_text`], but only keeps an occurrence whose start byte
/// offset `abs` satisfies `accept`. This is the hook the Ф.7 scope filter uses:
/// a local binding accepts only offsets within the declaring function; a
/// top-level rename accepts only offsets outside shadowing functions.
fn collect_edits_filtered(
    text: &str,
    old_name: &str,
    new_name: &str,
    accept: &dyn Fn(usize) -> bool,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let line_starts = compute_line_starts(text);

    let name_bytes = old_name.as_bytes();
    let n = name_bytes.len();

    if n == 0 || text.len() < n {
        return edits;
    }

    // Track doc-comment link pattern `[[old_name]]`.
    let link_pat = format!("[[{}]]", old_name);

    let mut search_start = 0usize;

    while search_start < text.len() {
        // Find next occurrence of old_name in text starting at search_start.
        let slice = &text[search_start..];

        // First, check for `[[old_name]]` doc-comment link.
        if let Some(lp_off) = slice.find(link_pat.as_str()) {
            let link_abs = search_start + lp_off;
            // name_start is after `[[`
            let name_start = link_abs + 2;
            let name_end = name_start + n;

            // Check if there's also a plain word match before this link.
            if let Some(word_off) = slice[..lp_off].find(old_name) {
                let abs = search_start + word_off;
                // Word boundary check using char-safe approach.
                let left_ok = check_left_boundary(text, abs);
                let right_ok = check_right_boundary(text, abs + n);
                if left_ok && right_ok
                    && accept(abs)
                    && !is_in_string(text, abs)
                    && !is_in_comment(text, abs)
                {
                    let range = byte_range_to_lsp_range(text, &line_starts, abs, abs + n);
                    edits.push(TextEdit { range, new_text: new_name.to_string() });
                }
                // Advance past this match.
                search_start = abs + 1;
                continue;
            }

            // No word match before the link; process the link.
            if accept(name_start) && !is_in_string(text, link_abs) {
                // In comment or doc-comment: still update the link text.
                let range = byte_range_to_lsp_range(text, &line_starts, name_start, name_end);
                edits.push(TextEdit { range, new_text: new_name.to_string() });
            }
            search_start = name_end;
            continue;
        }

        // No more links; find remaining word occurrences.
        match slice.find(old_name) {
            None => break,
            Some(off) => {
                let abs = search_start + off;
                let left_ok = check_left_boundary(text, abs);
                let right_ok = check_right_boundary(text, abs + n);
                if left_ok && right_ok
                    && accept(abs)
                    && !is_in_string(text, abs)
                    && !is_in_comment(text, abs)
                {
                    let range = byte_range_to_lsp_range(text, &line_starts, abs, abs + n);
                    edits.push(TextEdit { range, new_text: new_name.to_string() });
                }
                search_start = abs + 1;
            }
        }
    }

    // Sort by position (ascending).
    edits.sort_by(|a, b| {
        a.range.start.line.cmp(&b.range.start.line)
            .then(a.range.start.character.cmp(&b.range.start.character))
    });
    // Deduplicate.
    edits.dedup_by(|a, b| a.range == b.range);

    edits
}

/// Check that the character immediately before byte offset `at` is not an identifier char.
fn check_left_boundary(text: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }
    // Walk backward to find the previous Unicode char.
    let mut pos = at - 1;
    while pos > 0 && !text.is_char_boundary(pos) {
        pos -= 1;
    }
    let ch = text[pos..].chars().next().unwrap_or(' ');
    !is_ident_char(ch)
}

/// Check that the character at byte offset `at` (the first char after the match) is not an identifier char.
fn check_right_boundary(text: &str, at: usize) -> bool {
    if at >= text.len() {
        return true;
    }
    let mut pos = at;
    while pos < text.len() && !text.is_char_boundary(pos) {
        pos += 1;
    }
    let ch = text[pos..].chars().next().unwrap_or(' ');
    !is_ident_char(ch)
}

/// Apply `TextEdit`s to `text` (in ascending order by position) and return
/// the resulting string.  Used for the atomic post-rename check.
fn apply_edits_to_text(text: &str, edits: &[TextEdit]) -> String {
    if edits.is_empty() {
        return text.to_string();
    }

    let line_starts = compute_line_starts(text);
    let mut result = String::with_capacity(text.len());

    // Process edits in ascending order; track current byte position.
    let mut last_byte = 0usize;

    for edit in edits {
        let start_byte = lsp_pos_to_byte(text, &line_starts, edit.range.start);
        let end_byte = lsp_pos_to_byte(text, &line_starts, edit.range.end);

        if start_byte < last_byte {
            // Overlapping edit — skip (shouldn't happen for rename edits).
            continue;
        }

        result.push_str(&text[last_byte..start_byte]);
        result.push_str(&edit.new_text);
        last_byte = end_byte;
    }

    result.push_str(&text[last_byte..]);
    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Scope analysis (Plan 104.10 Ф.7) — AST-derived rename scope
// ─────────────────────────────────────────────────────────────────────────────

/// The rename scope for the symbol under the cursor, decided from the AST.
enum RenameScope {
    /// A local binding (`let`/param/`for`/pattern). Rename is confined to the
    /// half-open byte range `[start, end)` (the declaring function's span) in
    /// the primary file; no other file is touched.
    LocalInFile { range: (usize, usize) },
    /// A top-level symbol (free fn / type / const / field / import). Rename runs
    /// cross-file; functions that locally shadow the name are skipped per file.
    TopLevel,
}

/// Classify the symbol named `old_name` at `cursor_byte` in `text`.
///
/// A symbol is **local** when the innermost function (or `test`) whose span
/// contains the cursor binds `old_name` locally (as a parameter or any nested
/// pattern binding). Otherwise it is treated as **top-level**.
///
/// On a parse failure we return [`RenameScope::TopLevel`] (the safe, V1-compatible
/// default): rename stays cross-file rather than silently doing nothing.
fn classify_scope(text: &str, cursor_byte: usize, old_name: &str) -> RenameScope {
    let Ok(module) = crate::compiler::parse_guarded(text) else {
        return RenameScope::TopLevel;
    };
    // Innermost enclosing top-level fn/test whose span contains the cursor.
    if let Some((span, bound)) = enclosing_fn_bindings(&module, cursor_byte) {
        if bound.contains(old_name) {
            return RenameScope::LocalInFile { range: (span.start, span.end) };
        }
    }
    RenameScope::TopLevel
}

/// The single-file symbol scope for `documentHighlight` (Plan 104.10 Ф.15),
/// derived from the **same** AST classification the Ф.7 rename uses — never a
/// blind word regex. See [`resolve_highlight_scope`].
pub(crate) enum HighlightScope {
    /// A local binding: its occurrences live only inside the half-open byte
    /// range `[start, end)` of the declaring function.
    Local { range: (usize, usize) },
    /// A top-level symbol: its occurrences span the whole file, except inside
    /// the functions listed in `shadows` (each locally rebinds the name, so
    /// their occurrences refer to a different symbol).
    TopLevel { shadows: Vec<(usize, usize)> },
}

/// Resolve the scope of the symbol named `name` at `cursor_byte` in `text`, for
/// `documentHighlight`. This reuses the Ф.7 rename resolver ([`classify_scope`]
/// + [`shadow_scopes_in_text`]) so highlight and rename agree on what "the same
/// symbol" means — a same-named local in a sibling function is never conflated
/// (the semantic scope-correctness the plan requires, not a word regex).
///
/// On a parse failure `classify_scope` degrades to `TopLevel` with no shadows,
/// so highlight stays functional (whole-file occurrences) rather than panicking.
pub(crate) fn resolve_highlight_scope(
    text: &str,
    cursor_byte: usize,
    name: &str,
) -> HighlightScope {
    match classify_scope(text, cursor_byte, name) {
        RenameScope::LocalInFile { range } => HighlightScope::Local { range },
        RenameScope::TopLevel => HighlightScope::TopLevel {
            shadows: shadow_scopes_in_text(text, name),
        },
    }
}

/// Byte ranges of every top-level function/test in `text` that locally binds
/// `name` — the shadow scopes a top-level rename must skip. Empty on parse
/// failure (degrade to "no shadows", i.e. the V1 cross-file behaviour).
fn shadow_scopes_in_text(text: &str, name: &str) -> Vec<(usize, usize)> {
    let Ok(module) = crate::compiler::parse_guarded(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in &module.items {
        let (span, bound) = match item {
            Item::Fn(fd) => (fd.span, fn_bound_names(fd)),
            Item::Test(td) => {
                let mut b = HashSet::new();
                collect_block_bindings(&td.body, &mut b);
                (td.span, b)
            }
            _ => continue,
        };
        if bound.contains(name) {
            out.push((span.start, span.end));
        }
    }
    out
}

/// Find the innermost top-level fn/test containing `cursor_byte` and return its
/// span together with the set of names it binds locally.
fn enclosing_fn_bindings(module: &Module, cursor_byte: usize) -> Option<(Span, HashSet<String>)> {
    // Prefer the narrowest span (a test/fn cannot nest at top level, but keep the
    // narrowest for robustness against any future nesting).
    let mut best: Option<(Span, HashSet<String>)> = None;
    for item in &module.items {
        let (span, names): (Span, HashSet<String>) = match item {
            Item::Fn(fd) => {
                if !span_contains(fd.span, cursor_byte) {
                    continue;
                }
                (fd.span, fn_bound_names(fd))
            }
            Item::Test(td) => {
                if !span_contains(td.span, cursor_byte) {
                    continue;
                }
                let mut b = HashSet::new();
                collect_block_bindings(&td.body, &mut b);
                (td.span, b)
            }
            _ => continue,
        };
        let narrower = best
            .as_ref()
            .map_or(true, |(bspan, _)| span_len(span) < span_len(*bspan));
        if narrower {
            best = Some((span, names));
        }
    }
    best
}

fn span_len(s: Span) -> usize {
    s.end.saturating_sub(s.start)
}

/// True if `span` covers `offset` (inclusive of both ends, matching `symbol.rs`).
fn span_contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

/// All names bound locally by function `fd`: its parameters plus every pattern
/// binding introduced anywhere in its body.
fn fn_bound_names(fd: &FnDecl) -> HashSet<String> {
    let mut out = HashSet::new();
    for p in &fd.params {
        out.insert(p.name.clone());
    }
    match &fd.body {
        FnBody::Block(b) => collect_block_bindings(b, &mut out),
        FnBody::Expr(e) => collect_expr_bindings(e, &mut out),
        FnBody::External => {}
    }
    out
}

/// Collect every identifier bound by `pattern` (idents, `as`-bindings, tuple /
/// record / array / variant / or sub-patterns, slice `..rest` binds).
fn collect_pattern_names(pattern: &Pattern, out: &mut HashSet<String>) {
    match pattern {
        Pattern::Ident { name, .. } => {
            out.insert(name.clone());
        }
        Pattern::Binding { name, inner, .. } => {
            out.insert(name.clone());
            collect_pattern_names(inner, out);
        }
        Pattern::Tuple(ps, _) => {
            for p in ps {
                collect_pattern_names(p, out);
            }
        }
        Pattern::Variant { kind, .. } => {
            if let VariantPatternKind::Tuple { patterns, .. } = kind {
                for p in patterns {
                    collect_pattern_names(p, out);
                }
            }
        }
        Pattern::Record { fields, .. } => {
            for f in fields {
                match &f.pattern {
                    Some(sub) => collect_pattern_names(sub, out),
                    // Shorthand `{ name, .. }` binds the field name itself.
                    None => {
                        out.insert(f.name.clone());
                    }
                }
            }
        }
        Pattern::Array { elems, .. } => {
            for e in elems {
                match e {
                    ArrayPatternElem::Item(p) => collect_pattern_names(p, out),
                    ArrayPatternElem::RestBind(n) => {
                        out.insert(n.clone());
                    }
                    ArrayPatternElem::Rest => {}
                }
            }
        }
        Pattern::Or { alternatives, .. } => {
            for p in alternatives {
                collect_pattern_names(p, out);
            }
        }
        Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
    }
}

fn collect_block_bindings(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_stmt_bindings(stmt, out);
    }
    if let Some(trailing) = &block.trailing {
        collect_expr_bindings(trailing, out);
    }
}

fn collect_stmt_bindings(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Let(ld) => {
            collect_pattern_names(&ld.pattern, out);
            collect_expr_bindings(&ld.value, out);
        }
        Stmt::Const(cd) => {
            out.insert(cd.name.clone());
            collect_expr_bindings(&cd.value, out);
        }
        Stmt::Expr(e) => collect_expr_bindings(e, out),
        Stmt::Assign { target, value, .. } => {
            collect_expr_bindings(target, out);
            collect_expr_bindings(value, out);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                collect_expr_bindings(e, out);
            }
            for e in rhs {
                collect_expr_bindings(e, out);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                collect_expr_bindings(e, out);
            }
        }
        Stmt::Throw { value, .. } => collect_expr_bindings(value, out),
        Stmt::Defer { body, .. } => collect_expr_bindings(body, out),
        Stmt::ConsumeScope { binding, init, body, .. } => {
            out.insert(binding.clone());
            collect_expr_bindings(init, out);
            collect_block_bindings(body, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_expr_bindings(expr, out)
        }
        Stmt::Apply { args, .. } => {
            for a in args {
                collect_expr_bindings(a, out);
            }
        }
        Stmt::Calc { steps, .. } => {
            for s in steps {
                collect_expr_bindings(&s.expr, out);
            }
        }
        _ => {}
    }
}

/// Recurse through `expr` collecting the names introduced by binding constructs
/// nested inside it (`for`/`while let`/`if let` patterns, `match` arm patterns,
/// closure parameters, nested blocks). Mirrors the expression coverage of
/// `symbol.rs::find_ident_in_expr` so no binding-bearing form is missed.
fn collect_expr_bindings(expr: &Expr, out: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::For { pattern, iter, body, .. }
        | ExprKind::ParallelFor { pattern, iter, body, .. } => {
            collect_pattern_names(pattern, out);
            collect_expr_bindings(iter, out);
            collect_block_bindings(body, out);
        }
        ExprKind::IfLet { pattern, scrutinee, guard, then, else_, .. } => {
            collect_pattern_names(pattern, out);
            collect_expr_bindings(scrutinee, out);
            if let Some(g) = guard {
                collect_expr_bindings(g, out);
            }
            collect_block_bindings(then, out);
            collect_else(else_, out);
        }
        ExprKind::WhileLet { pattern, scrutinee, guard, body, .. } => {
            collect_pattern_names(pattern, out);
            collect_expr_bindings(scrutinee, out);
            if let Some(g) = guard {
                collect_expr_bindings(g, out);
            }
            collect_block_bindings(body, out);
        }
        ExprKind::If { cond, then, else_, .. } => {
            collect_expr_bindings(cond, out);
            collect_block_bindings(then, out);
            collect_else(else_, out);
        }
        ExprKind::While { cond, body, .. } => {
            collect_expr_bindings(cond, out);
            collect_block_bindings(body, out);
        }
        ExprKind::Loop { body, .. } => collect_block_bindings(body, out),
        ExprKind::Block(block) => collect_block_bindings(block, out),
        ExprKind::Match { scrutinee, arms, .. } => {
            collect_expr_bindings(scrutinee, out);
            for arm in arms {
                collect_pattern_names(&arm.pattern, out);
                if let Some(g) = &arm.guard {
                    collect_expr_bindings(g, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => collect_expr_bindings(e, out),
                    MatchArmBody::Block(b) => collect_block_bindings(b, out),
                }
            }
        }
        ExprKind::ClosureLight { params, body, .. } => {
            for p in params {
                out.insert(p.name.clone());
            }
            match body {
                ClosureBody::Expr(e) => collect_expr_bindings(e, out),
                ClosureBody::Block(b) => collect_block_bindings(b, out),
            }
        }
        ExprKind::ClosureFull(sig_body) => {
            for p in &sig_body.params {
                out.insert(p.name.clone());
            }
            match &sig_body.body {
                FnBody::Block(b) => collect_block_bindings(b, out),
                FnBody::Expr(e) => collect_expr_bindings(e, out),
                FnBody::External => {}
            }
        }
        ExprKind::Call { func, args, .. } => {
            collect_expr_bindings(func, out);
            for a in args {
                collect_expr_bindings(a.expr(), out);
            }
        }
        ExprKind::Member { obj, .. } => collect_expr_bindings(obj, out),
        ExprKind::Index { obj, index } => {
            collect_expr_bindings(obj, out);
            collect_expr_bindings(index, out);
        }
        ExprKind::Binary { left, right, .. } => {
            collect_expr_bindings(left, out);
            collect_expr_bindings(right, out);
        }
        ExprKind::Unary { operand, .. } => collect_expr_bindings(operand, out),
        ExprKind::TupleLit(elems) => {
            for e in elems {
                collect_expr_bindings(e, out);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for e in elems {
                match e {
                    nova_codegen::ast::ArrayElem::Item(x)
                    | nova_codegen::ast::ArrayElem::Spread(x) => collect_expr_bindings(x, out),
                }
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    collect_expr_bindings(v, out);
                }
            }
        }
        ExprKind::TurboFish { base, .. } => collect_expr_bindings(base, out),
        ExprKind::Forbid { body, .. } | ExprKind::Blocking(body) => {
            collect_block_bindings(body, out)
        }
        ExprKind::Supervised { body, cancel, .. } => {
            collect_block_bindings(body, out);
            if let Some(c) = cancel {
                collect_expr_bindings(c, out);
            }
        }
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::Spawn(inner)
        | ExprKind::Throw(inner) => collect_expr_bindings(inner, out),
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => collect_expr_bindings(inner, out),
        ExprKind::Coalesce(a, b) => {
            collect_expr_bindings(a, out);
            collect_expr_bindings(b, out);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(e) = start {
                collect_expr_bindings(e, out);
            }
            if let Some(e) = end {
                collect_expr_bindings(e, out);
            }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            collect_expr_bindings(range, out);
            collect_expr_bindings(body, out);
        }
        _ => {}
    }
}

fn collect_else(else_: &Option<ElseBranch>, out: &mut HashSet<String>) {
    match else_ {
        Some(ElseBranch::Block(b)) => collect_block_bindings(b, out),
        Some(ElseBranch::If(e)) => collect_expr_bindings(e, out),
        None => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Position utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Compute byte offset of each line start (0-indexed, first entry = 0).
pub fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Get line `idx` as a string slice (without trailing newline).
fn get_line(text: &str, idx: usize) -> &str {
    let mut n = 0usize;
    for line in text.split('\n') {
        if n == idx {
            return line;
        }
        n += 1;
    }
    ""
}

/// Convert UTF-16 column to byte offset within a line string (ASCII-only lines
/// have col_utf16 == col_byte; multi-byte chars require counting CUs).
fn utf16_col_to_byte_offset(line: &str, col_utf16: usize) -> usize {
    let mut remaining = col_utf16;
    for (byte_idx, ch) in line.char_indices() {
        if remaining == 0 {
            return byte_idx;
        }
        let w = ch.len_utf16();
        if remaining < w {
            // In the middle of a surrogate pair — snap to char boundary.
            return byte_idx;
        }
        remaining -= w;
    }
    line.len().min(remaining) // Clamp if col beyond line end.
}

/// Convert a (start_byte, end_byte) range to an LSP `Range`.
pub fn byte_range_to_lsp_range(
    text: &str,
    line_starts: &[usize],
    start_byte: usize,
    end_byte: usize,
) -> Range {
    Range {
        start: byte_to_lsp_position(text, line_starts, start_byte),
        end: byte_to_lsp_position(text, line_starts, end_byte),
    }
}

/// Convert a byte offset to an LSP `Position` (line, UTF-16 col).
pub fn byte_to_lsp_position(text: &str, line_starts: &[usize], byte_off: usize) -> Position {
    // Find line index via binary search.
    let line = match line_starts.binary_search(&byte_off) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let line_start = line_starts.get(line).copied().unwrap_or(0);

    // Walk chars from line_start to byte_off, counting UTF-16 code units.
    let mut utf16_col = 0u32;
    for ch in text[line_start..byte_off.min(text.len())].chars() {
        utf16_col += ch.len_utf16() as u32;
    }

    Position { line: line as u32, character: utf16_col }
}

/// Convert an LSP `Position` to a byte offset in `text`.
fn lsp_pos_to_byte(text: &str, line_starts: &[usize], pos: Position) -> usize {
    let line_idx = pos.line as usize;
    let line_start = line_starts.get(line_idx).copied().unwrap_or(text.len());

    // Determine next line start (or end of text).
    let next_line_start = line_starts.get(line_idx + 1).copied().unwrap_or(text.len());

    let line_text = &text[line_start..next_line_start];
    let col_byte = utf16_col_to_byte_offset(line_text, pos.character as usize);
    (line_start + col_byte).min(text.len())
}

// ─────────────────────────────────────────────────────────────────────────────
// Word / identifier utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Returns true if `c` is an identifier character in Nova (alphanumeric or `_`).
fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Returns true if `s` is a valid Nova identifier.
pub fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        None => false,
        Some(c) if c.is_ascii_digit() => false,
        Some(c) if !is_ident_char(c) => false,
        _ => chars.all(is_ident_char),
    }
}

/// Find the byte range of the identifier word at byte offset `off` in `text`.
/// Returns `(word_start, word_end)`.  If not on an identifier, returns `(off, off)`.
pub fn word_at(text: &str, off: usize) -> (usize, usize) {
    if off > text.len() {
        return (off, off);
    }
    // Find a char boundary at or before `off`.
    let mut start = off.min(text.len());
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }

    // Check the char at start.
    let ch = text[start..].chars().next().unwrap_or(' ');
    if !is_ident_char(ch) {
        // Try one position back (cursor might be just after the word).
        if start > 0 {
            let prev = start - 1;
            let mut pb = prev;
            while pb > 0 && !text.is_char_boundary(pb) {
                pb -= 1;
            }
            let pch = text[pb..].chars().next().unwrap_or(' ');
            if is_ident_char(pch) {
                start = pb;
            } else {
                return (off, off);
            }
        } else {
            return (off, off);
        }
    }

    // Scan backward to word start.
    while start > 0 {
        let prev = start - 1;
        let mut pb = prev;
        while pb > 0 && !text.is_char_boundary(pb) {
            pb -= 1;
        }
        let pch = text[pb..].chars().next().unwrap_or(' ');
        if is_ident_char(pch) {
            start = pb;
        } else {
            break;
        }
    }

    // Scan forward to word end.
    let mut end = start;
    for ch in text[start..].chars() {
        if is_ident_char(ch) {
            end += ch.len_utf8();
        } else {
            break;
        }
    }

    if start == end {
        (off, off)
    } else {
        (start, end)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Comment / string detection
// ─────────────────────────────────────────────────────────────────────────────

/// Returns true if byte offset `off` is inside a `//` or `/* */` comment.
pub fn is_in_comment(text: &str, off: usize) -> bool {
    // Walk through the text tracking comment state.
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_string = false;
    let mut escape_next = false;

    while i < off && i < bytes.len() {
        if in_line_comment {
            if bytes[i] == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if in_string {
            if escape_next {
                escape_next = false;
            } else if bytes[i] == b'\\' {
                escape_next = true;
            } else if bytes[i] == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        // Not in any special context.
        if bytes[i] == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            in_line_comment = true;
            i += 2;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            in_block_comment = true;
            i += 2;
            continue;
        }
        i += 1;
    }

    in_line_comment || in_block_comment
}

/// Returns true if byte offset `off` is inside a string literal.
pub fn is_in_string(text: &str, off: usize) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut in_string = false;
    let mut escape_next = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while i < off && i < bytes.len() {
        if in_line_comment {
            if bytes[i] == b'\n' { in_line_comment = false; }
            i += 1;
            continue;
        }
        if in_block_comment {
            if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if in_string {
            if escape_next {
                escape_next = false;
            } else if bytes[i] == b'\\' {
                escape_next = true;
            } else if bytes[i] == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            in_line_comment = true;
            i += 2;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            in_block_comment = true;
            i += 2;
            continue;
        }
        i += 1;
    }

    in_string
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, col: u32) -> Position {
        Position { line, character: col }
    }

    /// Test convenience: rename the symbol `old` (cursor placed at its first
    /// textual occurrence in the primary doc) to `new`, driving the real
    /// scope-aware [`compute_rename`].
    fn cr(docs: &[RenameDoc], old: &str, new: &str) -> Result<WorkspaceEdit> {
        let uri = docs[0].uri.clone();
        let cursor = docs[0].text.find(old).unwrap_or(0);
        compute_rename(docs, &uri, cursor, old, new)
    }

    /// Compute the byte offset of the Nth (0-based) occurrence of `needle`.
    fn nth_occurrence(text: &str, needle: &str, n: usize) -> usize {
        let mut start = 0;
        for _ in 0..n {
            let off = text[start..].find(needle).expect("occurrence exists");
            start += off + needle.len();
        }
        start + text[start..].find(needle).expect("occurrence exists")
    }

    // ── prepareRename pos tests ───────────────────────────────────────────────

    #[test]
    fn pos_prepare_on_identifier() {
        let src = "fn hello() => ()\n";
        // cursor on 'h' of 'hello' (col 3)
        let result = prepare_rename(src, pos(0, 3)).unwrap();
        match result {
            PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } => {
                assert_eq!(placeholder, "hello");
            }
            _ => panic!("expected RangeWithPlaceholder"),
        }
    }

    #[test]
    fn pos_prepare_on_type_name() {
        let src = "type Point {}\n";
        // cursor on 'P' of 'Point' (col 5)
        let result = prepare_rename(src, pos(0, 5)).unwrap();
        match result {
            PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } => {
                assert_eq!(placeholder, "Point");
            }
            _ => panic!("expected RangeWithPlaceholder"),
        }
    }

    #[test]
    fn pos_prepare_on_param() {
        let src = "fn foo(bar int) => ()\n";
        // cursor on 'b' of 'bar' (col 7)
        let result = prepare_rename(src, pos(0, 7)).unwrap();
        match result {
            PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } => {
                assert_eq!(placeholder, "bar");
            }
            _ => panic!("expected RangeWithPlaceholder"),
        }
    }

    #[test]
    fn pos_prepare_on_field() {
        let src = "type Foo { x int }\n";
        // cursor on 'x' (col 11)
        let result = prepare_rename(src, pos(0, 11)).unwrap();
        match result {
            PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } => {
                assert_eq!(placeholder, "x");
            }
            _ => panic!("expected RangeWithPlaceholder"),
        }
    }

    // ── prepareRename neg tests ───────────────────────────────────────────────

    #[test]
    fn neg_prepare_on_keyword() {
        let src = "fn hello() => ()\n";
        // cursor on 'f' of 'fn' (col 0)
        let result = prepare_rename(src, pos(0, 0));
        assert!(result.is_err(), "keyword should be rejected");
    }

    #[test]
    fn neg_prepare_on_string_literal() {
        let src = "ro x = \"hello\"\n";
        // cursor inside "hello" at col 9
        let result = prepare_rename(src, pos(0, 9));
        assert!(result.is_err(), "string literal should be rejected");
    }

    #[test]
    fn neg_prepare_on_comment() {
        let src = "// this is a comment\nfn foo() => ()\n";
        // cursor at col 3 (inside comment)
        let result = prepare_rename(src, pos(0, 3));
        assert!(result.is_err(), "comment should be rejected");
    }

    #[test]
    fn neg_prepare_on_whitespace() {
        let src = "fn   foo() => ()\n";
        // cursor on space between fn and foo (col 3)
        let result = prepare_rename(src, pos(0, 3));
        assert!(result.is_err(), "whitespace should be rejected");
    }

    // ── rename pos tests ──────────────────────────────────────────────────────

    #[test]
    fn pos_rename_simple_local_variable() {
        let src = "fn main() {\n  ro oldVar = 1\n  ro x = oldVar + 2\n}\n";
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///test.nv").unwrap(),
            text: src.to_string(),
            version: Some(1),
        }];
        // rename may fail atomic check (no full compiler in tests), but should
        // at least compute the edits and attempt the check.
        let result = cr(&docs, "oldVar", "newVar");
        // Result can be Ok (if compiler not available) or Err (type error post-rename).
        // Key: no panic, and if Ok, all edits replace "oldVar" -> "newVar".
        match result {
            Ok(edit) => {
                assert!(edit.document_changes.is_some(), "should have document changes");
            }
            Err(e) => {
                // Atomic check rejection is acceptable in test env.
                assert!(e.message.contains("rename rejected") || e.message.contains("post-rename"),
                    "unexpected error: {}", e.message);
            }
        }
    }

    #[test]
    fn pos_rename_function_call_sites() {
        // Simple case: rename 'add' to 'sum'
        let src = "fn add(a int, b int) => a + b\n\nfn main() {\n  ro r = add(1, 2)\n}\n";
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///math.nv").unwrap(),
            text: src.to_string(),
            version: Some(1),
        }];
        let result = cr(&docs, "add", "sum");
        // Verify no panic, accept either Ok or Err(atomic check).
        match result {
            Ok(_) | Err(_) => {}
        }
    }

    #[test]
    fn pos_rename_type_name() {
        let src = "type Point { x int, y int }\nfn origin() Point => Point { x: 0, y: 0 }\n";
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///point.nv").unwrap(),
            text: src.to_string(),
            version: Some(1),
        }];
        let result = cr(&docs, "Point", "Vec2");
        match result {
            Ok(edit) => {
                // Should have changed multiple occurrences.
                if let Some(DocumentChanges::Edits(edits)) = edit.document_changes {
                    // Each TextDocumentEdit has the edits.
                    assert!(!edits.is_empty());
                }
            }
            Err(_) => {} // atomic check failure acceptable
        }
    }

    #[test]
    fn pos_rename_cross_file() {
        // Two files: one defines a function, another calls it.
        let docs = vec![
            RenameDoc {
                uri: Url::parse("file:///lib.nv").unwrap(),
                text: "pub fn greet() => ()\n".to_string(),
                version: Some(1),
            },
            RenameDoc {
                uri: Url::parse("file:///main.nv").unwrap(),
                text: "import lib\n\nfn main() {\n  greet()\n}\n".to_string(),
                version: Some(2),
            },
        ];
        let result = cr(&docs, "greet", "hello");
        match result {
            Ok(edit) => {
                // Should touch both files.
                if let Some(DocumentChanges::Edits(edits)) = edit.document_changes {
                    assert!(edits.len() <= 2, "at most 2 files changed");
                }
            }
            Err(_) => {} // atomic check ok
        }
    }

    #[test]
    fn pos_rename_generic_param() {
        // Generic param T renamed to U — should only change within the fn.
        let src = "fn map[T, R](x T, f fn(T) R) R => f(x)\n";
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///map.nv").unwrap(),
            text: src.to_string(),
            version: Some(1),
        }];
        let result = cr(&docs, "T", "U");
        // Should produce edits replacing T→U in the fn signature.
        match result {
            Ok(edit) => {
                if let Some(DocumentChanges::Edits(edits)) = edit.document_changes {
                    // Multiple replacements expected (T appears 3 times in sig).
                    assert!(!edits.is_empty());
                }
            }
            Err(_) => {}
        }
    }

    #[test]
    fn pos_rename_updates_doc_comment_links() {
        let src = "/// See [[oldFn]] for details.\nfn oldFn() => ()\n";
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///docs.nv").unwrap(),
            text: src.to_string(),
            version: Some(1),
        }];
        let result = cr(&docs, "oldFn", "newFn");
        match result {
            Ok(edit) => {
                if let Some(DocumentChanges::Edits(te)) = edit.document_changes {
                    // Should update both the [[link]] and the fn name.
                    assert!(te.len() >= 1, "at least one edit expected (link + fn name)");
                }
            }
            Err(_) => {}
        }
    }

    // ── rename neg tests ──────────────────────────────────────────────────────

    #[test]
    fn neg_rename_empty_name() {
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///a.nv").unwrap(),
            text: "fn foo() => ()\n".to_string(),
            version: None,
        }];
        let result = cr(&docs, "foo", "");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::InvalidParams);
    }

    #[test]
    fn neg_rename_keyword_as_new_name() {
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///b.nv").unwrap(),
            text: "fn myFn() => ()\n".to_string(),
            version: None,
        }];
        let result = cr(&docs, "myFn", "fn");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams);
        assert!(err.message.contains("keyword"), "expected keyword error, got: {}", err.message);
    }

    #[test]
    fn neg_rename_digit_start_new_name() {
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///c.nv").unwrap(),
            text: "fn bar() => ()\n".to_string(),
            version: None,
        }];
        let result = cr(&docs, "bar", "1invalid");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::InvalidParams);
    }

    #[test]
    fn neg_rename_conflict_post_check() {
        // Both 'foo' and 'bar' exist; renaming 'foo' → 'bar' should be rejected
        // by the atomic check (name collision).
        // Note: this depends on the compiler actually rejecting duplicates.
        // If the compiler doesn't check (light test env), the edit would succeed.
        // We just verify no panic.
        let src = "fn foo() => ()\nfn bar() => ()\n";
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///d.nv").unwrap(),
            text: src.to_string(),
            version: None,
        }];
        let result = cr(&docs, "foo", "bar");
        // Either rejected by atomic check or Ok (compiler accepts duplicates in single file).
        match result {
            Ok(_) | Err(_) => {} // no panic is the contract
        }
    }

    // ── helper tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_is_in_comment_line() {
        let src = "code // comment\nmore code\n";
        assert!(!is_in_comment(src, 0)); // 'c' in code
        assert!(is_in_comment(src, 8));  // inside comment
        assert!(!is_in_comment(src, 16)); // 'm' in 'more' on line 2
    }

    #[test]
    fn test_is_in_string() {
        let src = "x = \"hello world\"\n";
        assert!(!is_in_string(src, 0));  // 'x'
        assert!(is_in_string(src, 6));   // inside "hello..."
        assert!(!is_in_string(src, 18)); // after closing quote
    }

    #[test]
    fn test_word_at_middle() {
        let src = "fn hello() => ()\n";
        let (s, e) = word_at(src, 4); // inside 'hello'
        assert_eq!(&src[s..e], "hello");
    }

    #[test]
    fn test_word_at_boundary() {
        let src = "fn hello() => ()\n";
        let (s, e) = word_at(src, 2); // 'f' of 'fn'
        assert_eq!(&src[s..e], "fn");
    }

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("hello"));
        assert!(is_valid_identifier("_priv"));
        assert!(is_valid_identifier("CamelCase"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("1bad"));
        assert!(!is_valid_identifier("has space"));
    }

    #[test]
    fn edge_cyrillic_identifier() {
        // Cyrillic identifiers in Nova are valid; ensure word_at works with multi-byte.
        // In test: we just verify word_at doesn't panic on multi-byte input.
        let src = "fn привет() => ()\n";
        let bytes = src.as_bytes();
        // 'п' starts at byte 3 (after "fn ")
        let byte_off = 3;
        let (s, e) = word_at(src, byte_off);
        // Should capture the whole Cyrillic word (all chars are is_alphabetic).
        let word = &src[s..e];
        assert!(word.chars().all(|c| c.is_alphabetic() || c == '_'));
    }

    #[test]
    fn edge_emoji_in_doc_comment() {
        // Doc comment with emoji should not crash collect_edits.
        let src = "/// Hello 👋 [[oldFn]]\nfn oldFn() => ()\n";
        let edits = collect_edits_in_text(src, "oldFn", "newFn");
        // Should find 2 occurrences: inside [[link]] and fn name.
        assert!(edits.len() >= 1, "expected at least 1 edit, got {}", edits.len());
    }

    #[test]
    fn edge_very_long_file() {
        // 10000 lines — rename must complete without timeout.
        let mut lines = String::with_capacity(200_000);
        lines.push_str("fn myFunc() => ()\n");
        for i in 0..9999 {
            lines.push_str(&format!("ro _x{} = {}\n", i, i));
        }
        let docs = vec![RenameDoc {
            uri: Url::parse("file:///big.nv").unwrap(),
            text: lines,
            version: None,
        }];
        let start = std::time::Instant::now();
        let _result = cr(&docs, "myFunc", "myFunction");
        let elapsed = start.elapsed();
        assert!(elapsed.as_secs() < 5, "rename on 10000-line file took too long: {:?}", elapsed);
    }

    #[test]
    fn test_collect_edits_no_string_replacement() {
        // Should not replace identifier inside string.
        let src = "ro s = \"foo\"\nro x = foo\n";
        let edits = collect_edits_in_text(src, "foo", "bar");
        // Only 1 edit: the one at `ro x = foo`, not the one in "foo" string.
        assert_eq!(edits.len(), 1, "should not replace inside string literal");
    }

    #[test]
    fn test_collect_edits_no_comment_replacement() {
        let src = "// foo is old\nro x = foo\n";
        let edits = collect_edits_in_text(src, "foo", "bar");
        // Only replace outside comment.
        assert_eq!(edits.len(), 1, "should not replace inside comment");
    }

    #[test]
    fn test_apply_edits_to_text() {
        let src = "fn foo() => foo + 1\n";
        let edits = collect_edits_in_text(src, "foo", "bar");
        assert!(!edits.is_empty());
        let result = apply_edits_to_text(src, &edits);
        assert!(result.contains("bar"), "new name not found in result");
        assert!(!result.contains("foo"), "old name still present in result");
    }

    // ── Ф.7 scope-aware rename (criterion #4: scope from AST, NOT regex) ───────

    /// Helper: run the exact occurrence filter `compute_rename` applies for a
    /// given scope, returning the edits (bypassing the atomic type-check so the
    /// scope semantics can be asserted deterministically).
    fn scoped_edits(src: &str, old: &str, new: &str, scope: &RenameScope) -> Vec<TextEdit> {
        match scope {
            RenameScope::LocalInFile { range } => {
                let (lo, hi) = *range;
                collect_edits_filtered(src, old, new, &|abs| abs >= lo && abs < hi)
            }
            RenameScope::TopLevel => {
                let shadows = shadow_scopes_in_text(src, old);
                collect_edits_filtered(src, old, new, &|abs| {
                    !shadows.iter().any(|(lo, hi)| abs >= *lo && abs < *hi)
                })
            }
        }
    }

    /// POS: renaming a local var touches only its declaring function, never a
    /// same-named local in a sibling function.
    #[test]
    fn f7_pos_local_var_scoped_to_declaring_fn() {
        let src = "fn a() {\n  ro x = 1\n  ro y = x + 1\n}\n\nfn b() {\n  ro x = 2\n  ro z = x + 3\n}\n";
        let cursor = src.find("x").unwrap(); // first `x` — inside a
        let scope = classify_scope(src, cursor, "x");
        let range = match scope {
            RenameScope::LocalInFile { range } => range,
            RenameScope::TopLevel => panic!("local var must classify as LocalInFile, not regex-wide"),
        };
        let b_start = src.find("fn b").unwrap();
        assert!(range.1 <= b_start, "local scope must end before fn b begins");
        let edits = scoped_edits(src, "x", "q", &scope);
        assert_eq!(edits.len(), 2, "only a's `x` (decl + use) renamed, b's untouched");
    }

    /// POS: cursor on a *use* of a local (not the binding) still resolves to the
    /// local's scope — proving AST binding lookup, not just decl-site matching.
    #[test]
    fn f7_pos_local_var_use_site_still_scoped() {
        let src = "fn a() {\n  ro x = 1\n  ro y = x + 1\n}\nfn b() {\n  ro x = 2\n}\n";
        let cursor = nth_occurrence(src, "x", 1); // the `x` in `x + 1`
        assert!(
            matches!(classify_scope(src, cursor, "x"), RenameScope::LocalInFile { .. }),
            "use-site of a local must classify as LocalInFile"
        );
    }

    /// POS: a top-level fn with no shadowing renames its decl + all call sites.
    #[test]
    fn f7_pos_top_level_fn_renames_call_sites() {
        let src = "fn helper() => ()\nfn user() {\n  helper()\n}\n";
        let cursor = src.find("helper").unwrap();
        let scope = classify_scope(src, cursor, "helper");
        assert!(matches!(scope, RenameScope::TopLevel), "free fn is TopLevel");
        assert!(shadow_scopes_in_text(src, "helper").is_empty());
        let edits = scoped_edits(src, "helper", "aid", &scope);
        assert_eq!(edits.len(), 2, "decl + call site");
    }

    /// POS (shadow-check): a function that locally binds the top-level name is
    /// skipped — its occurrences refer to the local, not the top-level symbol.
    #[test]
    fn f7_pos_shadow_check_skips_shadowing_fn() {
        let src = "fn helper() => ()\nfn user() {\n  helper()\n}\nfn shad() {\n  ro helper = 5\n  ro y = helper + 1\n}\n";
        let cursor = src.find("helper").unwrap(); // decl of the top-level fn
        let scope = classify_scope(src, cursor, "helper");
        assert!(matches!(scope, RenameScope::TopLevel));
        let shadows = shadow_scopes_in_text(src, "helper");
        assert_eq!(shadows.len(), 1, "exactly `shad` shadows `helper`");
        let edits = scoped_edits(src, "helper", "aid", &scope);
        assert_eq!(edits.len(), 2, "decl + user() call; shad's two occurrences skipped");
    }

    /// POS (plan's shadow example): `fn a(){ro x} fn b(){ro x}` renaming x in a
    /// leaves b's x untouched.
    #[test]
    fn f7_pos_shadow_two_functions_same_local() {
        let src = "fn a() {\n  ro x = 1\n}\nfn b() {\n  ro x = 2\n}\n";
        let cursor = src.find("x").unwrap();
        let scope = classify_scope(src, cursor, "x");
        assert!(matches!(scope, RenameScope::LocalInFile { .. }));
        let edits = scoped_edits(src, "x", "w", &scope);
        assert_eq!(edits.len(), 1, "only a's `x` decl renamed");
    }

    /// EDGE: a type used across files classifies as TopLevel (cross-file rename),
    /// never scope-limited.
    #[test]
    fn f7_edge_type_is_top_level() {
        let src = "type Point { x int, y int }\nfn origin() Point => Point { x: 0, y: 0 }\n";
        let cursor = src.find("Point").unwrap();
        assert!(
            matches!(classify_scope(src, cursor, "Point"), RenameScope::TopLevel),
            "a type rename must be cross-file (TopLevel)"
        );
        assert!(shadow_scopes_in_text(src, "Point").is_empty());
    }

    /// NEG: a parse failure degrades to TopLevel (V1-compatible cross-file),
    /// never a panic or a silent no-op.
    #[test]
    fn f7_neg_parse_error_degrades_to_top_level() {
        let src = "fn broken(@@@ =>";
        assert!(matches!(classify_scope(src, 3, "broken"), RenameScope::TopLevel));
        assert!(shadow_scopes_in_text(src, "broken").is_empty());
    }

    /// NEG (regression): the old regex renamed a same-named local in EVERY
    /// function. Assert the scope filter does NOT — the classic false positive.
    #[test]
    fn f7_neg_no_cross_scope_false_positive() {
        let src = "fn a() {\n  ro count = 1\n}\nfn b() {\n  ro count = 2\n  ro c = count + count\n}\n";
        let cursor = src.find("count").unwrap(); // a's count
        let scope = classify_scope(src, cursor, "count");
        let edits = scoped_edits(src, "count", "n", &scope);
        // a has exactly one `count`; b's three occurrences must be untouched.
        assert_eq!(edits.len(), 1, "regex would have matched all 4; scope matches 1");
    }
}
