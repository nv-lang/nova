//! `textDocument/documentHighlight` — Plan 104.10 Ф.15.
//!
//! Highlights every occurrence of the symbol under the cursor **in the current
//! file only**, tagging each as read or write access.
//!
//! # Semantic resolution (not a word regex)
//!
//! The occurrence set is scoped by the **same AST resolver the Ф.7 rename uses**
//! ([`crate::rename::resolve_highlight_scope`]):
//!
//! - A **local** binding (`let`/param/`for`/pattern) highlights only inside its
//!   declaring function's byte span. A same-named local in a *sibling* function
//!   is never highlighted — the false positive a blind `\bword\b` regex would
//!   produce, and exactly what the plan forbids.
//! - A **top-level** symbol (free fn / type / const / field) highlights across
//!   the whole file, but occurrences inside a function that *locally shadows*
//!   the name are skipped (that function's `x` is a different `x`).
//!
//! Within the resolved scope, occurrences are found by a byte-accurate word scan
//! that skips string and comment text; for a *local* symbol, member-access
//! positions (`obj.x`) are also excluded, since a local variable is never
//! written as `.x` — that `x` is a field of another value. Read vs write is then
//! decided from **AST binding/assignment sites** (`collect_write_offsets`), not
//! from surrounding characters.
//!
//! # Residual — [M-104.10-highlight-lexical-occurrences]
//!
//! Like the Ф.7 rename it mirrors, the within-scope occurrence scan is textual
//! (word-boundary + string/comment/`.`-member exclusion), not a per-occurrence
//! resolve of every candidate back to its declaration `Span`. Consequences (all
//! rare, all also present in rename, tracked in `simplifications.md`/backlog):
//! a record/named-argument **field label** that happens to share the symbol's
//! spelling (`Point { x: … }` when highlighting a local `x`) is highlighted as a
//! read; a `..rest` array-pattern slice bind (no AST span) is reported as a read
//! rather than a write. Scope-correctness (the tested property) is exact; these
//! are read/write-classification edge cases on textual coincidences.

use std::collections::HashSet;

use tower_lsp::lsp_types::{DocumentHighlight, DocumentHighlightKind, Position};

use nova_codegen::ast::{
    ArrayElem, ArrayPatternElem, Block, ClosureBody, ElseBranch, Expr, ExprKind,
    FnBody, FnDecl, Item, MatchArmBody, Pattern, Stmt, VariantPatternKind,
};
use nova_codegen::diag::Span;

use crate::rename::{
    byte_range_to_lsp_range, compute_line_starts, is_in_comment, is_in_string,
    is_valid_identifier, resolve_highlight_scope, word_at, HighlightScope,
};

/// Compute document highlights for the symbol at `pos` in `text`.
///
/// Returns `None` when the cursor is not on a highlightable identifier (keyword,
/// whitespace, punctuation, string/comment text) or nothing resolves.
pub fn compute_document_highlights(text: &str, pos: Position) -> Option<Vec<DocumentHighlight>> {
    let line_starts = compute_line_starts(text);
    let byte_off = position_to_byte(text, &line_starts, pos)?;

    // Word under the cursor.
    let (ws, we) = word_at(text, byte_off);
    if ws == we {
        return None;
    }
    let name = &text[ws..we];

    // Reject non-identifiers, reserved keywords, and cursors inside literals /
    // comments — a keyword or string cursor highlights nothing (plan NEG test).
    if !is_valid_identifier(name) {
        return None;
    }
    if nova_codegen::lexer::is_reserved_keyword(name) {
        return None;
    }
    if is_in_string(text, ws) || is_in_comment(text, ws) {
        return None;
    }

    // Semantic scope from the Ф.7 resolver (AST-derived, never a whole-file regex).
    let scope = resolve_highlight_scope(text, ws, name);

    // AST-derived write sites (bindings + assignment targets) for read/write kind.
    let writes = collect_write_offsets(text, name);

    // `exclude_members` is set for a *local* value symbol: a leading `.` marks a
    // field/method access on another value (`obj.x`), never the local itself.
    let (accept_range, exclude_members): (Box<dyn Fn(usize) -> bool>, bool) = match &scope {
        HighlightScope::Local { range } => {
            let (lo, hi) = *range;
            (Box::new(move |a: usize| a >= lo && a < hi), true)
        }
        HighlightScope::TopLevel { shadows } => {
            let shadows = shadows.clone();
            (
                Box::new(move |a: usize| !shadows.iter().any(|(lo, hi)| a >= *lo && a < *hi)),
                false,
            )
        }
    };

    let mut highlights = Vec::new();
    for (start, end) in word_occurrences(text, name) {
        if !accept_range(start) {
            continue;
        }
        if is_in_string(text, start) || is_in_comment(text, start) {
            continue;
        }
        if exclude_members && preceded_by_member_dot(text, start) {
            continue;
        }
        let kind = if writes.contains(&start) {
            DocumentHighlightKind::WRITE
        } else {
            DocumentHighlightKind::READ
        };
        highlights.push(DocumentHighlight {
            range: byte_range_to_lsp_range(text, &line_starts, start, end),
            kind: Some(kind),
        });
    }

    if highlights.is_empty() {
        None
    } else {
        Some(highlights)
    }
}

/// True if the identifier starting at `start` is immediately preceded by a `.`
/// (ignoring inline horizontal whitespace) — i.e. it is a `obj.field` /
/// `obj.method` access, not a standalone value reference.
fn preceded_by_member_dot(text: &str, start: usize) -> bool {
    let bytes = text.as_bytes();
    let mut i = start;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b' ' | b'\t' => continue,
            b'.' => {
                // Exclude range operator `..` (e.g. slice `a..b`) — the char
                // before the dot being another `.` means this is not a member
                // access. A single `.` immediately before is member access.
                return !(i > 0 && bytes[i - 1] == b'.');
            }
            _ => return false,
        }
    }
    false
}

// ─────────────────────────────────────────────────────────────────────────────
// Occurrence scan
// ─────────────────────────────────────────────────────────────────────────────

/// All word-boundary occurrences of `name` in `text` as `(start, end)` byte
/// ranges. Boundaries use Nova's identifier-char rule so `count` does not match
/// inside `counter`.
fn word_occurrences(text: &str, name: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let n = name.len();
    if n == 0 || text.len() < n {
        return out;
    }
    let mut from = 0usize;
    while let Some(off) = text[from..].find(name) {
        let abs = from + off;
        let end = abs + n;
        if boundary_ok(text, abs) && boundary_ok_end(text, end) {
            out.push((abs, end));
        }
        from = abs + 1;
    }
    out
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// True if the char immediately before byte `at` is not an identifier char.
fn boundary_ok(text: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }
    let mut p = at - 1;
    while p > 0 && !text.is_char_boundary(p) {
        p -= 1;
    }
    !text[p..].chars().next().map_or(false, is_ident_char)
}

/// True if the char at byte `at` (first char after the match) is not an ident char.
fn boundary_ok_end(text: &str, at: usize) -> bool {
    if at >= text.len() {
        return true;
    }
    let mut p = at;
    while p < text.len() && !text.is_char_boundary(p) {
        p += 1;
    }
    !text[p..].chars().next().map_or(false, is_ident_char)
}

// ─────────────────────────────────────────────────────────────────────────────
// Write-site collection (AST-derived) → read/write kind
// ─────────────────────────────────────────────────────────────────────────────

/// Byte offsets at which `name` is *written*: every binding site (`let`/`const`/
/// param / `for` / pattern / `consume` binding) and every assignment target.
/// Empty on a parse failure (all occurrences degrade to reads).
fn collect_write_offsets(text: &str, name: &str) -> HashSet<usize> {
    let mut out = HashSet::new();
    let Ok(module) = crate::compiler::parse_guarded(text) else {
        return out;
    };
    for item in &module.items {
        match item {
            Item::Fn(fd) => collect_fn_writes(fd, name, text, &mut out),
            Item::Test(td) => walk_block_writes(&td.body, name, text, &mut out),
            Item::Let(ld) => {
                add_pattern_writes(&ld.pattern, name, text, &mut out);
                walk_expr_writes(&ld.value, name, text, &mut out);
            }
            Item::Const(cd) => {
                if cd.name == name {
                    add_span_write(text, name, cd.span, &mut out);
                }
                walk_expr_writes(&cd.value, name, text, &mut out);
            }
            _ => {}
        }
    }
    out
}

fn collect_fn_writes(fd: &FnDecl, name: &str, text: &str, out: &mut HashSet<usize>) {
    for p in &fd.params {
        if p.name == name {
            add_span_write(text, name, p.span, out);
        }
        if let Some(def) = &p.default {
            walk_expr_writes(def, name, text, out);
        }
    }
    match &fd.body {
        FnBody::Block(b) => walk_block_writes(b, name, text, out),
        FnBody::Expr(e) => walk_expr_writes(e, name, text, out),
        FnBody::External => {}
    }
}

/// Record the first word-boundary occurrence of `name` inside `span` as a write.
/// Used where the AST stores only a name + an enclosing span (params, `mut`
/// patterns, `const`/`consume` bindings) rather than the bare identifier span.
fn add_span_write(text: &str, name: &str, span: Span, out: &mut HashSet<usize>) {
    if let Some(off) = find_word_in_range(text, name, span.start, span.end) {
        out.insert(off);
    }
}

fn find_word_in_range(text: &str, name: &str, lo: usize, hi: usize) -> Option<usize> {
    let hi = hi.min(text.len());
    if lo >= hi || !text.is_char_boundary(lo) || !text.is_char_boundary(hi) {
        return None;
    }
    let hay = &text[lo..hi];
    let n = name.len();
    let mut from = 0usize;
    while let Some(off) = hay[from..].find(name) {
        let abs = lo + from + off;
        if boundary_ok(text, abs) && boundary_ok_end(text, abs + n) {
            return Some(abs);
        }
        from += off + 1;
    }
    None
}

fn add_pattern_writes(pat: &Pattern, name: &str, text: &str, out: &mut HashSet<usize>) {
    match pat {
        Pattern::Ident { name: n, span, .. } => {
            if n == name {
                add_span_write(text, name, *span, out);
            }
        }
        Pattern::Binding { name: n, inner, span } => {
            if n == name {
                add_span_write(text, name, *span, out);
            }
            add_pattern_writes(inner, name, text, out);
        }
        Pattern::Tuple(ps, _) => {
            for p in ps {
                add_pattern_writes(p, name, text, out);
            }
        }
        Pattern::Variant { kind, .. } => {
            if let VariantPatternKind::Tuple { patterns, .. } = kind {
                for p in patterns {
                    add_pattern_writes(p, name, text, out);
                }
            }
        }
        Pattern::Record { fields, .. } => {
            for f in fields {
                match &f.pattern {
                    Some(sub) => add_pattern_writes(sub, name, text, out),
                    // Shorthand `{ x, .. }` binds the field name itself.
                    None => {
                        if f.name == name {
                            add_span_write(text, name, f.span, out);
                        }
                    }
                }
            }
        }
        Pattern::Array { elems, .. } => {
            for e in elems {
                match e {
                    ArrayPatternElem::Item(p) => add_pattern_writes(p, name, text, out),
                    // `..rest` bind has no AST span — see the residual marker.
                    ArrayPatternElem::RestBind(_) | ArrayPatternElem::Rest => {}
                }
            }
        }
        Pattern::Or { alternatives, .. } => {
            for p in alternatives {
                add_pattern_writes(p, name, text, out);
            }
        }
        Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
    }
}

fn walk_block_writes(block: &Block, name: &str, text: &str, out: &mut HashSet<usize>) {
    for s in &block.stmts {
        walk_stmt_writes(s, name, text, out);
    }
    if let Some(t) = &block.trailing {
        walk_expr_writes(t, name, text, out);
    }
}

fn walk_stmt_writes(stmt: &Stmt, name: &str, text: &str, out: &mut HashSet<usize>) {
    match stmt {
        Stmt::Let(ld) => {
            add_pattern_writes(&ld.pattern, name, text, out);
            walk_expr_writes(&ld.value, name, text, out);
        }
        Stmt::Const(cd) => {
            if cd.name == name {
                add_span_write(text, name, cd.span, out);
            }
            walk_expr_writes(&cd.value, name, text, out);
        }
        Stmt::Expr(e) => walk_expr_writes(e, name, text, out),
        Stmt::Assign { target, value, .. } => {
            // The assignment target `x = …` / `x += …` is a write iff it is the
            // bare identifier; `obj.f = …` / `arr[i] = …` write a field/element,
            // and the name inside those is a read (handled by recursion).
            add_assign_target_write(target, name, text, out);
            walk_expr_writes(target, name, text, out);
            walk_expr_writes(value, name, text, out);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                add_assign_target_write(e, name, text, out);
                walk_expr_writes(e, name, text, out);
            }
            for e in rhs {
                walk_expr_writes(e, name, text, out);
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                walk_expr_writes(e, name, text, out);
            }
        }
        Stmt::Throw { value, .. } => walk_expr_writes(value, name, text, out),
        Stmt::Defer { body, .. } => walk_expr_writes(body, name, text, out),
        Stmt::ConsumeScope { binding, init, body, span, .. } => {
            if binding == name {
                add_span_write(text, name, *span, out);
            }
            walk_expr_writes(init, name, text, out);
            walk_block_writes(body, name, text, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            walk_expr_writes(expr, name, text, out)
        }
        Stmt::Apply { args, .. } => {
            for a in args {
                walk_expr_writes(a, name, text, out);
            }
        }
        Stmt::Calc { steps, .. } => {
            for s in steps {
                walk_expr_writes(&s.expr, name, text, out);
            }
        }
        _ => {}
    }
}

/// If `target` is the bare identifier `name`, record its span as a write.
fn add_assign_target_write(target: &Expr, name: &str, text: &str, out: &mut HashSet<usize>) {
    if let ExprKind::Ident(n) = &target.kind {
        if n == name {
            add_span_write(text, name, target.span, out);
        }
    }
}

fn walk_expr_writes(expr: &Expr, name: &str, text: &str, out: &mut HashSet<usize>) {
    match &expr.kind {
        ExprKind::For { pattern, iter, body, .. }
        | ExprKind::ParallelFor { pattern, iter, body, .. } => {
            add_pattern_writes(pattern, name, text, out);
            walk_expr_writes(iter, name, text, out);
            walk_block_writes(body, name, text, out);
        }
        ExprKind::IfLet { pattern, scrutinee, guard, then, else_, .. } => {
            add_pattern_writes(pattern, name, text, out);
            walk_expr_writes(scrutinee, name, text, out);
            if let Some(g) = guard {
                walk_expr_writes(g, name, text, out);
            }
            walk_block_writes(then, name, text, out);
            walk_else_writes(else_, name, text, out);
        }
        ExprKind::WhileLet { pattern, scrutinee, guard, body, .. } => {
            add_pattern_writes(pattern, name, text, out);
            walk_expr_writes(scrutinee, name, text, out);
            if let Some(g) = guard {
                walk_expr_writes(g, name, text, out);
            }
            walk_block_writes(body, name, text, out);
        }
        ExprKind::If { cond, then, else_, .. } => {
            walk_expr_writes(cond, name, text, out);
            walk_block_writes(then, name, text, out);
            walk_else_writes(else_, name, text, out);
        }
        ExprKind::While { cond, body, .. } => {
            walk_expr_writes(cond, name, text, out);
            walk_block_writes(body, name, text, out);
        }
        ExprKind::Loop { body, .. } => walk_block_writes(body, name, text, out),
        ExprKind::Block(b) => walk_block_writes(b, name, text, out),
        ExprKind::Match { scrutinee, arms, .. } => {
            walk_expr_writes(scrutinee, name, text, out);
            for arm in arms {
                add_pattern_writes(&arm.pattern, name, text, out);
                if let Some(g) = &arm.guard {
                    walk_expr_writes(g, name, text, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => walk_expr_writes(e, name, text, out),
                    MatchArmBody::Block(b) => walk_block_writes(b, name, text, out),
                }
            }
        }
        ExprKind::ClosureLight { params, body, .. } => {
            for p in params {
                if p.name == name {
                    add_span_write(text, name, p.span, out);
                }
            }
            match body {
                ClosureBody::Expr(e) => walk_expr_writes(e, name, text, out),
                ClosureBody::Block(b) => walk_block_writes(b, name, text, out),
            }
        }
        ExprKind::ClosureFull(sig_body) => {
            for p in &sig_body.params {
                if p.name == name {
                    add_span_write(text, name, p.span, out);
                }
            }
            match &sig_body.body {
                FnBody::Block(b) => walk_block_writes(b, name, text, out),
                FnBody::Expr(e) => walk_expr_writes(e, name, text, out),
                FnBody::External => {}
            }
        }
        ExprKind::Call { func, args, .. } => {
            walk_expr_writes(func, name, text, out);
            for a in args {
                walk_expr_writes(a.expr(), name, text, out);
            }
        }
        ExprKind::Member { obj, .. } => walk_expr_writes(obj, name, text, out),
        ExprKind::Index { obj, index } => {
            walk_expr_writes(obj, name, text, out);
            walk_expr_writes(index, name, text, out);
        }
        ExprKind::Binary { left, right, .. } => {
            walk_expr_writes(left, name, text, out);
            walk_expr_writes(right, name, text, out);
        }
        ExprKind::Unary { operand, .. } => walk_expr_writes(operand, name, text, out),
        ExprKind::TupleLit(elems) => {
            for e in elems {
                walk_expr_writes(e, name, text, out);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for e in elems {
                match e {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => {
                        walk_expr_writes(x, name, text, out)
                    }
                }
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    walk_expr_writes(v, name, text, out);
                }
            }
        }
        ExprKind::TurboFish { base, .. } => walk_expr_writes(base, name, text, out),
        ExprKind::Forbid { body, .. } | ExprKind::Blocking(body) => {
            walk_block_writes(body, name, text, out)
        }
        ExprKind::Supervised { body, cancel, .. } => {
            walk_block_writes(body, name, text, out);
            if let Some(c) = cancel {
                walk_expr_writes(c, name, text, out);
            }
        }
        ExprKind::Try(inner)
        | ExprKind::Bang(inner)
        | ExprKind::Spawn(inner)
        | ExprKind::Throw(inner) => walk_expr_writes(inner, name, text, out),
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => {
            walk_expr_writes(inner, name, text, out)
        }
        ExprKind::Coalesce(a, b) => {
            walk_expr_writes(a, name, text, out);
            walk_expr_writes(b, name, text, out);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(e) = start {
                walk_expr_writes(e, name, text, out);
            }
            if let Some(e) = end {
                walk_expr_writes(e, name, text, out);
            }
        }
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            walk_expr_writes(range, name, text, out);
            walk_expr_writes(body, name, text, out);
        }
        _ => {}
    }
}

fn walk_else_writes(else_: &Option<ElseBranch>, name: &str, text: &str, out: &mut HashSet<usize>) {
    match else_ {
        Some(ElseBranch::Block(b)) => walk_block_writes(b, name, text, out),
        Some(ElseBranch::If(e)) => walk_expr_writes(e, name, text, out),
        None => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Position → byte
// ─────────────────────────────────────────────────────────────────────────────

/// Convert an LSP `Position` (line + UTF-16 column) to a byte offset in `text`.
/// Returns `None` if the line is out of range.
fn position_to_byte(text: &str, line_starts: &[usize], pos: Position) -> Option<usize> {
    let line_idx = pos.line as usize;
    let line_start = *line_starts.get(line_idx)?;
    let next = line_starts.get(line_idx + 1).copied().unwrap_or(text.len());
    let line = &text[line_start..next];

    let mut remaining = pos.character as usize;
    for (byte_idx, ch) in line.char_indices() {
        if remaining == 0 {
            return Some(line_start + byte_idx);
        }
        let w = ch.len_utf16();
        if remaining < w {
            return Some(line_start + byte_idx);
        }
        remaining -= w;
    }
    Some(next.min(text.len()))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, col: u32) -> Position {
        Position { line, character: col }
    }

    /// Byte offset of the Nth (0-based) occurrence of `needle`.
    fn nth(text: &str, needle: &str, n: usize) -> usize {
        let mut start = 0;
        for _ in 0..n {
            let off = text[start..].find(needle).expect("occurrence");
            start += off + needle.len();
        }
        start + text[start..].find(needle).expect("occurrence")
    }

    /// Position at a given byte offset.
    fn pos_at(text: &str, byte: usize) -> Position {
        let ls = compute_line_starts(text);
        crate::rename::byte_to_lsp_position(text, &ls, byte)
    }

    // ── POS: local variable → all in-scope occurrences highlighted ────────────

    #[test]
    fn pos_local_var_all_occurrences() {
        let src = "fn main() {\n  ro count = 1\n  ro y = count + count\n}\n";
        let hs = compute_document_highlights(src, pos_at(src, nth(src, "count", 0)))
            .expect("highlights");
        // decl + two uses = 3.
        assert_eq!(hs.len(), 3, "all three `count` in the fn are highlighted");
    }

    #[test]
    fn pos_read_vs_write_kind_distinguished() {
        // `count` is bound (write) then read twice, then reassigned (write).
        let src = "fn main() {\n  mut count = 1\n  ro y = count + count\n  count = 2\n}\n";
        let hs = compute_document_highlights(src, pos_at(src, nth(src, "count", 0)))
            .expect("highlights");
        let writes = hs
            .iter()
            .filter(|h| h.kind == Some(DocumentHighlightKind::WRITE))
            .count();
        let reads = hs
            .iter()
            .filter(|h| h.kind == Some(DocumentHighlightKind::READ))
            .count();
        assert_eq!(writes, 2, "the `let` binding and the `count = 2` assignment are writes");
        assert_eq!(reads, 2, "the two uses in `count + count` are reads");
    }

    #[test]
    fn pos_cursor_on_use_site_resolves_binding() {
        // Cursor on a *use* still highlights the whole set.
        let src = "fn f() {\n  ro x = 1\n  ro y = x + 1\n}\n";
        let hs = compute_document_highlights(src, pos_at(src, nth(src, "x", 1)))
            .expect("highlights");
        assert_eq!(hs.len(), 2, "decl + use");
    }

    #[test]
    fn pos_top_level_fn_decl_and_call_sites() {
        let src = "fn helper() => ()\nfn user() {\n  helper()\n}\n";
        let hs = compute_document_highlights(src, pos_at(src, nth(src, "helper", 0)))
            .expect("highlights");
        assert_eq!(hs.len(), 2, "decl + call site");
    }

    // ── NEG: same-named local in a sibling scope must NOT be highlighted ───────

    #[test]
    fn neg_same_name_other_scope_not_highlighted() {
        let src = "fn a() {\n  ro x = 1\n  ro y = x + 1\n}\nfn b() {\n  ro x = 2\n  ro z = x + x\n}\n";
        // Cursor on a's `x`.
        let hs = compute_document_highlights(src, pos_at(src, nth(src, "x", 0)))
            .expect("highlights");
        // Only a's two `x` (decl + use); b's three are a different symbol.
        assert_eq!(hs.len(), 2, "b's `x` must not be highlighted (semantic scope)");
        // Every highlight is before `fn b`.
        let b_start = src.find("fn b").unwrap() as u32;
        for h in &hs {
            let line_start = compute_line_starts(src)[h.range.start.line as usize];
            assert!((line_start as u32) < b_start);
        }
    }

    #[test]
    fn neg_top_level_shadowing_fn_skipped() {
        let src = "fn helper() => ()\nfn user() {\n  helper()\n}\nfn shad() {\n  ro helper = 5\n  ro y = helper + 1\n}\n";
        let hs = compute_document_highlights(src, pos_at(src, nth(src, "helper", 0)))
            .expect("highlights");
        // decl + user() call; shad's two `helper` refer to its local.
        assert_eq!(hs.len(), 2, "shadowing fn's occurrences are excluded");
    }

    #[test]
    fn neg_member_field_not_highlighted_for_local() {
        // Local `x` and a same-named field access `p.x` in the same fn.
        let src = "fn f() {\n  ro x = 1\n  ro y = p.x + x\n}\n";
        let hs = compute_document_highlights(src, pos_at(src, nth(src, "x", 0)))
            .expect("highlights");
        // decl `x` + the standalone `x` use; the `p.x` member access is excluded.
        assert_eq!(hs.len(), 2, "member access `p.x` is not the local `x`");
    }

    #[test]
    fn neg_cursor_on_keyword_empty() {
        let src = "fn hello() => ()\n";
        // cursor on `fn`.
        assert!(compute_document_highlights(src, pos(0, 0)).is_none());
    }

    #[test]
    fn neg_cursor_in_string_empty() {
        let src = "fn f() {\n  ro s = \"count\"\n}\n";
        let byte = src.find("count").unwrap() + 1; // inside the string literal
        assert!(compute_document_highlights(src, pos_at(src, byte)).is_none());
    }

    #[test]
    fn neg_cursor_on_whitespace_empty() {
        let src = "fn   f() => ()\n";
        assert!(compute_document_highlights(src, pos(0, 2)).is_none());
    }

    // ── EDGE: parse failure degrades gracefully (no panic) ────────────────────

    #[test]
    fn edge_parse_error_no_panic() {
        let src = "fn broken(@@@ => x x x\n";
        // Must not panic; result is best-effort (Some or None).
        let _ = compute_document_highlights(src, pos(0, 3));
    }

    #[test]
    fn edge_for_loop_binding_is_write() {
        let src = "fn f() {\n  for i in 0..3 {\n    ro y = i + 1\n  }\n}\n";
        let hs = compute_document_highlights(src, pos_at(src, nth(src, "i", 0)))
            .expect("highlights");
        let writes = hs
            .iter()
            .filter(|h| h.kind == Some(DocumentHighlightKind::WRITE))
            .count();
        assert_eq!(hs.len(), 2, "loop binding + use");
        assert_eq!(writes, 1, "the `for i` binding is a write");
    }
}
