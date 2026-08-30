//! `textDocument/foldingRange` — Plan 104.10 Ф.16.
//!
//! Purely **syntactic**: parses the buffer to an AST (no type-checking, no
//! `expr_types`) and derives folding regions from the *spans of AST nodes*, not
//! from an indentation heuristic. Every foldable region corresponds to a real
//! grammar construct:
//!
//! - **fn / test / bench / lemma bodies** — the body `Block`'s `{ … }` span;
//! - **type declarations** — the `type Name { … }` span (record/sum/effect/
//!   protocol bodies);
//! - **nested `{ … }` blocks** — every `Block` reachable in the statement/
//!   expression tree (control-flow bodies, block-expressions, `with`/
//!   `supervised`/`detach`/`blocking`/`realtime`/`forbid` scopes, closures with
//!   block bodies, trailing DSL blocks, match/select arm blocks). Because the
//!   walk is recursive over the AST, nesting is represented faithfully: an outer
//!   block and an inner block each yield their own region, and the client nests
//!   them by line containment;
//! - **import groups** — a run of `import` statements on consecutive lines folds
//!   into a single [`FoldingRangeKind::Imports`] region;
//! - **multi-line doc-comments** — a `///`/`//!` run (the lexer merges it into a
//!   single [`DocBlock`] whose span covers all the lines) folds as a
//!   [`FoldingRangeKind::Comment`] region.
//!
//! A region is emitted **only** when it spans more than one line
//! (`end_line > start_line`); a single-line construct (`fn f() => 42`,
//! `type X alias int`, one `import`, one `///` line) yields nothing — matching
//! editor semantics where there is nothing to collapse.
//!
//! Line numbers are UTF-8/UTF-16-safe: the same `ropey`-based byte→`Position`
//! mapping the rest of the server uses (`byte_offset_to_position`) is applied to
//! the span boundaries, so multi-byte content before a brace does not skew the
//! reported line.
//!
//! **Known gap** ([M-104.10-folding-plain-comments]): runs of *plain* `//` line
//! comments are not folded — the lexer discards non-doc comments (they never
//! reach the AST), and re-deriving them would require a token-stream side
//! channel. Doc-comment runs (the AST-represented multi-line comments) *are*
//! folded. Tracked in `simplifications.md` / backlog.

use std::collections::HashSet;

use nova_codegen::ast::{
    Block, ClosureBody, ElseBranch, Expr, ExprKind, FnBody, HandlerMethodBody, Import, Item,
    MapElem, MatchArmBody, Module, SelectOp, Stmt, Trailing,
};
use nova_codegen::diag::Span;
use ropey::Rope;
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

use crate::diagnostic_mapping::byte_offset_to_position;

/// Compute all folding regions for `src`.
///
/// Returns an empty `Vec` on parse failure (graceful — the editor simply shows
/// no folds rather than an error). Safe to run inside `run_with_large_stack`.
pub fn compute_folding_ranges(src: &str) -> Vec<FoldingRange> {
    let module = match crate::compiler::parse_guarded(src) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    let rope = Rope::from_str(src);
    let mut c = Collector::new(&rope);
    c.collect_module(&module);
    c.finish()
}

/// Accumulates folding ranges while walking the AST, de-duplicating regions that
/// resolve to the same `(start_line, end_line, kind)` triple (e.g. a construct
/// whose node span and body span coincide).
struct Collector<'a> {
    rope: &'a Rope,
    out: Vec<FoldingRange>,
    seen: HashSet<(u32, u32, u8)>,
}

impl<'a> Collector<'a> {
    fn new(rope: &'a Rope) -> Self {
        Self { rope, out: Vec::new(), seen: HashSet::new() }
    }

    fn finish(self) -> Vec<FoldingRange> {
        self.out
    }

    /// The 0-based line of a byte offset in the buffer.
    fn line_of(&self, byte_offset: usize) -> u32 {
        byte_offset_to_position(self.rope, byte_offset).line
    }

    /// Emit a region for `span` with `kind`, iff it truly spans multiple lines.
    ///
    /// `start_line` is the line of the span's first byte (the `{`, `type`,
    /// `import`, or `///`). `end_line` is the line of the span's **last** byte
    /// (`span.end` is exclusive — it points just past the closing `}`/newline —
    /// so we probe `end - 1`, the last real character, to land on the closing
    /// brace's line rather than the following line).
    fn push_region(&mut self, span: Span, kind: FoldingRangeKind) {
        if span.end <= span.start {
            return;
        }
        let start_line = self.line_of(span.start);
        let end_line = self.line_of(span.end - 1);
        if end_line <= start_line {
            return;
        }
        let tag = match kind {
            FoldingRangeKind::Comment => 0u8,
            FoldingRangeKind::Imports => 1u8,
            FoldingRangeKind::Region => 2u8,
        };
        if !self.seen.insert((start_line, end_line, tag)) {
            return;
        }
        self.out.push(FoldingRange {
            start_line,
            start_character: None,
            end_line,
            end_character: None,
            kind: Some(kind),
            collapsed_text: None,
        });
    }

    // ── Top level ─────────────────────────────────────────────────────────────

    fn collect_module(&mut self, module: &Module) {
        // Inner module doc-comment (`//!` run).
        if let Some(doc) = &module.doc {
            self.push_region(doc.span, FoldingRangeKind::Comment);
        }
        self.collect_import_groups(&module.imports);
        for item in &module.items {
            self.collect_item(item);
        }
    }

    /// Fold each maximal run of imports whose lines are contiguous (adjacent or
    /// same line) into one `Imports` region. Spans are taken from the AST import
    /// nodes; a lone import (single line) produces nothing.
    fn collect_import_groups(&mut self, imports: &[Import]) {
        if imports.is_empty() {
            return;
        }
        // Sort by start offset — `Module.imports` is normally already in source
        // order, but folder-module flattening can interleave peers.
        let mut spans: Vec<(u32, u32, Span)> = imports
            .iter()
            .map(|imp| {
                let s = self.line_of(imp.span.start);
                let e = self.line_of(imp.span.end.saturating_sub(1).max(imp.span.start));
                (s, e, imp.span)
            })
            .collect();
        spans.sort_by_key(|(s, _, _)| *s);

        let mut group_start_line = spans[0].0;
        let mut group_start_off = spans[0].2.start;
        let mut group_end_line = spans[0].1;
        let mut group_end_off = spans[0].2.end;

        for (s, e, sp) in spans.iter().skip(1) {
            // Contiguous with the current group if it starts on the group's last
            // line, the next line, or before (overlap) — i.e. no blank-line gap.
            if *s <= group_end_line + 1 {
                if *e > group_end_line {
                    group_end_line = *e;
                    group_end_off = sp.end;
                }
            } else {
                self.flush_import_group(group_start_off, group_end_off);
                group_start_line = *s;
                group_start_off = sp.start;
                group_end_line = *e;
                group_end_off = sp.end;
            }
        }
        let _ = group_start_line;
        self.flush_import_group(group_start_off, group_end_off);
    }

    fn flush_import_group(&mut self, start_off: usize, end_off: usize) {
        self.push_region(Span::new(start_off, end_off), FoldingRangeKind::Imports);
    }

    fn collect_item(&mut self, item: &Item) {
        match item {
            Item::Fn(fd) => {
                if let Some(doc) = &fd.doc {
                    self.push_region(doc.span, FoldingRangeKind::Comment);
                }
                self.walk_fn_body(&fd.body);
            }
            Item::Type(td) => {
                if let Some(doc) = &td.doc {
                    self.push_region(doc.span, FoldingRangeKind::Comment);
                }
                // The whole `type Name { … }` declaration span is the body fold
                // (single-line forms like `type X alias int` self-filter).
                self.push_region(td.span, FoldingRangeKind::Region);
            }
            Item::Let(ld) => {
                self.walk_expr(&ld.value);
            }
            Item::Const(cd) => {
                if let Some(doc) = &cd.doc {
                    self.push_region(doc.span, FoldingRangeKind::Comment);
                }
                self.walk_expr(&cd.value);
            }
            Item::Test(td) => {
                self.walk_block(&td.body);
            }
            Item::Bench(bd) => {
                for s in &bd.setup {
                    self.walk_stmt(s);
                }
                self.walk_block(&bd.measure_body);
                for s in &bd.teardown {
                    self.walk_stmt(s);
                }
                for g in &bd.groups {
                    for case in &g.cases {
                        for s in &case.setup {
                            self.walk_stmt(s);
                        }
                        self.walk_block(&case.measure_body);
                        for s in &case.teardown {
                            self.walk_stmt(s);
                        }
                    }
                }
            }
            Item::Lemma(ld) => {
                self.walk_fn_body(&ld.body);
            }
        }
    }

    // ── Bodies / blocks / statements ──────────────────────────────────────────

    fn walk_fn_body(&mut self, body: &FnBody) {
        match body {
            FnBody::Block(b) => self.walk_block(b),
            FnBody::Expr(e) => self.walk_expr(e),
            FnBody::External => {}
        }
    }

    /// Emit the block's own `{ … }` region, then descend into its statements and
    /// trailing expression so nested blocks yield nested regions.
    fn walk_block(&mut self, block: &Block) {
        self.push_region(block.span, FoldingRangeKind::Region);
        for s in &block.stmts {
            self.walk_stmt(s);
        }
        if let Some(t) = &block.trailing {
            self.walk_expr(t);
        }
    }

    fn walk_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(ld) => self.walk_expr(&ld.value),
            Stmt::Const(cd) => self.walk_expr(&cd.value),
            Stmt::Expr(e) => self.walk_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target);
                self.walk_expr(value);
            }
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs {
                    self.walk_expr(e);
                }
                for e in rhs {
                    self.walk_expr(e);
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(e) = value {
                    self.walk_expr(e);
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
            Stmt::Throw { value, .. } => self.walk_expr(value),
            Stmt::Defer { body, .. } => self.walk_expr(body),
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(init);
                self.walk_block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.walk_expr(expr),
            Stmt::Apply { args, .. } => {
                for a in args {
                    self.walk_expr(a);
                }
            }
            Stmt::Calc { steps, .. } => {
                for step in steps {
                    self.walk_expr(&step.expr);
                }
            }
        }
    }

    // ── Expressions ───────────────────────────────────────────────────────────

    /// Descend into every sub-expression and block so that a `{ … }` appearing
    /// anywhere (statement, call argument, control-flow body, closure, trailing
    /// DSL block, operator operand, …) contributes a region. Pure leaves
    /// (literals, identifiers, paths) contribute nothing.
    fn walk_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            // Leaves — no sub-structure.
            ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::StrLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::UnitLit
            | ExprKind::CharLit(_)
            | ExprKind::NullPtrLit
            | ExprKind::HexBlobLit(_)
            | ExprKind::Ident(_)
            | ExprKind::Path(_)
            | ExprKind::SelfAccess => {}

            ExprKind::InterpolatedStr { parts } => {
                use nova_codegen::ast::InterpStrPart;
                for p in parts {
                    if let InterpStrPart::Expr { expr, .. } = p {
                        self.walk_expr(expr);
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                use nova_codegen::ast::ArrayElem;
                for e in elems {
                    match e {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                for e in elems {
                    match e {
                        MapElem::Pair(k, v) => {
                            self.walk_expr(k);
                            self.walk_expr(v);
                        }
                        MapElem::Spread(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value {
                        self.walk_expr(v);
                    }
                }
            }
            ExprKind::TupleLit(items) => {
                for e in items {
                    self.walk_expr(e);
                }
            }
            ExprKind::Member { obj, .. } => self.walk_expr(obj),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj);
                self.walk_expr(index);
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base),
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func);
                for a in args {
                    self.walk_expr(a.expr());
                }
                if let Some(t) = trailing {
                    self.walk_trailing(t);
                }
            }
            ExprKind::Try(e) | ExprKind::Bang(e) => self.walk_expr(e),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a);
                self.walk_expr(b);
            }
            ExprKind::CoalesceReturnFallback(inner) => {
                if let Some(e) = inner {
                    self.walk_expr(e);
                }
            }
            ExprKind::As(e, _) | ExprKind::Is(e, _) => self.walk_expr(e),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left);
                self.walk_expr(right);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand),
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond);
                self.walk_block(then);
                self.walk_else(else_.as_ref());
            }
            ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
                self.walk_expr(scrutinee);
                if let Some(g) = guard {
                    self.walk_expr(g);
                }
                self.walk_block(then);
                self.walk_else(else_.as_ref());
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.walk_expr(e),
                        MatchArmBody::Block(b) => self.walk_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, invariants, decreases, .. }
            | ExprKind::While { cond: iter, body, invariants, decreases } => {
                self.walk_expr(iter);
                self.walk_block(body);
                for inv in invariants {
                    self.walk_expr(inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(d);
                }
            }
            ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter);
                self.walk_block(body);
            }
            ExprKind::WhileLet { scrutinee, guard, body, invariants, decreases, .. } => {
                self.walk_expr(scrutinee);
                if let Some(g) = guard {
                    self.walk_expr(g);
                }
                self.walk_block(body);
                for inv in invariants {
                    self.walk_expr(inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(d);
                }
            }
            ExprKind::Loop { body, invariants, decreases } => {
                self.walk_block(body);
                for inv in invariants {
                    self.walk_expr(inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(d);
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => self.walk_expr(chan),
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan);
                            self.walk_expr(value);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g);
                    }
                    self.walk_block(&arm.body);
                }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body),
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(e) => self.walk_expr(e),
                ClosureBody::Block(b) => self.walk_block(b),
            },
            ExprKind::ClosureFull(sig) => self.walk_fn_body(&sig.body),
            ExprKind::With { bindings, body } => {
                for b in bindings {
                    self.walk_expr(&b.handler);
                }
                self.walk_block(body);
            }
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods {
                    match &m.body {
                        HandlerMethodBody::Expr(e) => self.walk_expr(e),
                        HandlerMethodBody::Block(b) => self.walk_block(b),
                    }
                }
            }
            ExprKind::Interrupt(opt) => {
                if let Some(e) = opt {
                    self.walk_expr(e);
                }
            }
            ExprKind::Forbid { body, .. } => self.walk_block(body),
            ExprKind::Realtime { body, .. } => self.walk_block(body),
            ExprKind::Range { start, end, .. } => {
                if let Some(e) = start {
                    self.walk_expr(e);
                }
                if let Some(e) = end {
                    self.walk_expr(e);
                }
            }
            ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
                self.walk_expr(range);
                self.walk_expr(body);
            }
            ExprKind::Block(b) => self.walk_block(b),
            ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::RefArg(e) => self.walk_expr(e),
            ExprKind::Supervised { body, cancel, .. } => {
                self.walk_block(body);
                if let Some(c) = cancel {
                    self.walk_expr(c);
                }
            }
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.walk_block(b),
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.walk_expr(tag);
                for a in args {
                    self.walk_expr(a);
                }
            }
        }
    }

    fn walk_else(&mut self, else_: Option<&ElseBranch>) {
        match else_ {
            Some(ElseBranch::Block(b)) => self.walk_block(b),
            Some(ElseBranch::If(e)) => self.walk_expr(e),
            None => {}
        }
    }

    fn walk_trailing(&mut self, t: &Trailing) {
        match t {
            Trailing::Block(b) => self.walk_block(b),
            Trailing::Fn(sig) => self.walk_fn_body(&sig.body),
            Trailing::LegacyBlockWithParams(tb) => self.walk_block(&tb.body),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(src: &str) -> Vec<FoldingRange> {
        compute_folding_ranges(src)
    }

    fn has_region(rs: &[FoldingRange], start: u32, end: u32, kind: FoldingRangeKind) -> bool {
        rs.iter()
            .any(|r| r.start_line == start && r.end_line == end && r.kind == Some(kind.clone()))
    }

    // ── POS ──────────────────────────────────────────────────────────────────

    /// POS: a multi-line fn body folds from its `{` line to its `}` line.
    #[test]
    fn pos_fn_body_region() {
        let src = "\
module app.mod
fn main() {
  ro x = 1
  ro y = 2
}
";
        let rs = ranges(src);
        // `{` on line 1, `}` on line 4.
        assert!(
            has_region(&rs, 1, 4, FoldingRangeKind::Region),
            "fn body must fold lines 1..4, got {rs:?}"
        );
    }

    /// POS: a run of consecutive imports folds into a single `Imports` region.
    #[test]
    fn pos_import_group_single_region() {
        let src = "\
module app.mod
import a.b
import c.d
import e.f
fn main() => ()
";
        let rs = ranges(src);
        let imports: Vec<_> = rs
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Imports))
            .collect();
        assert_eq!(imports.len(), 1, "3 adjacent imports → exactly one region, got {imports:?}");
        // imports on lines 1,2,3.
        assert_eq!(imports[0].start_line, 1);
        assert_eq!(imports[0].end_line, 3);
    }

    /// POS: nested blocks yield nested regions (outer fn body + inner `if` block).
    #[test]
    fn pos_nested_blocks_nested_regions() {
        let src = "\
module app.mod
fn main() {
  if true {
    ro z = 1
  }
}
";
        let rs = ranges(src);
        // Outer fn body: `{` line 1 → `}` line 5.
        assert!(has_region(&rs, 1, 5, FoldingRangeKind::Region), "outer fn body region missing: {rs:?}");
        // Inner if-block: `{` line 2 → `}` line 4.
        assert!(has_region(&rs, 2, 4, FoldingRangeKind::Region), "inner if-block region missing: {rs:?}");
        // Nesting is by line containment: 1..5 contains 2..4.
        let outer = rs.iter().find(|r| r.start_line == 1 && r.end_line == 5).unwrap();
        let inner = rs.iter().find(|r| r.start_line == 2 && r.end_line == 4).unwrap();
        assert!(
            outer.start_line < inner.start_line && inner.end_line < outer.end_line,
            "inner region must be strictly contained in outer"
        );
    }

    /// POS: a multi-line `type` declaration body folds.
    #[test]
    fn pos_type_body_region() {
        let src = "\
module app.mod
type User {
  ro name str
  ro age int
}
fn main() => ()
";
        let rs = ranges(src);
        assert!(
            has_region(&rs, 1, 4, FoldingRangeKind::Region),
            "type body must fold lines 1..4, got {rs:?}"
        );
    }

    /// POS: a multi-line doc-comment run folds as a `Comment` region.
    #[test]
    fn pos_multiline_doc_comment_region() {
        let src = "\
module app.mod
/// line one
/// line two
/// line three
fn main() => ()
";
        let rs = ranges(src);
        let comments: Vec<_> = rs
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
            .collect();
        assert!(!comments.is_empty(), "3-line doc-comment must produce a Comment region, got {rs:?}");
        // Doc run spans lines 1..3.
        let c = comments[0];
        assert_eq!(c.start_line, 1, "doc region starts at first `///`");
        assert!(c.end_line >= 3, "doc region reaches the last `///` line, got {c:?}");
    }

    /// POS: two import groups separated by a blank line fold as two regions.
    #[test]
    fn pos_two_import_groups() {
        let src = "\
module app.mod
import a.b
import c.d

import e.f
import g.h
fn main() => ()
";
        let rs = ranges(src);
        let imports: Vec<_> = rs
            .iter()
            .filter(|r| r.kind == Some(FoldingRangeKind::Imports))
            .collect();
        assert_eq!(imports.len(), 2, "blank-line-separated import runs → two regions, got {imports:?}");
    }

    // ── NEG ──────────────────────────────────────────────────────────────────

    /// NEG: a single-line fn (`=> expr`) produces no region.
    #[test]
    fn neg_single_line_fn_no_region() {
        let src = "\
module app.mod
fn main() => 42
";
        let rs = ranges(src);
        assert!(rs.is_empty(), "single-line fn must yield no folding region, got {rs:?}");
    }

    /// NEG: a lone import (single line) is not a foldable group.
    #[test]
    fn neg_single_import_no_region() {
        let src = "\
module app.mod
import a.b
fn main() => ()
";
        let rs = ranges(src);
        assert!(
            !rs.iter().any(|r| r.kind == Some(FoldingRangeKind::Imports)),
            "a single import is not a group, got {rs:?}"
        );
    }

    /// NEG: a single-line `type X alias int` is not foldable.
    #[test]
    fn neg_single_line_type_no_region() {
        let src = "\
module app.mod
type Id alias int
fn main() => ()
";
        let rs = ranges(src);
        assert!(rs.is_empty(), "single-line type alias must yield no region, got {rs:?}");
    }

    /// NEG: a parse error degrades to no folds (no panic).
    #[test]
    fn neg_parse_error_graceful() {
        let src = "module app.mod\nfn ( { { { unbalanced";
        let rs = ranges(src);
        // Must not panic; empty (or best-effort) is acceptable — assert no crash.
        let _ = rs;
    }

    // ── EDGE ─────────────────────────────────────────────────────────────────

    /// EDGE: multi-byte content before the braces must not skew line numbers.
    /// A Cyrillic string literal precedes the fn body; the `}` line must still be
    /// reported correctly (UTF-16 mapping is line-accurate).
    #[test]
    fn edge_multibyte_line_accuracy() {
        let src = "\
module app.mod
fn main() {
  ro s = \"Привет, мир — здравствуй\"
  ro t = \"ещё строка\"
}
";
        let rs = ranges(src);
        assert!(
            has_region(&rs, 1, 4, FoldingRangeKind::Region),
            "multi-byte body must still fold lines 1..4, got {rs:?}"
        );
    }

    /// EDGE: a trailing DSL block (`f() { … }`) folds via the trailing walk.
    #[test]
    fn edge_trailing_block_region() {
        let src = "\
module app.mod
fn main() {
  retry(3) {
    ro x = 1
  }
}
";
        let rs = ranges(src);
        // The trailing `{ … }` after retry(3) spans lines 2..4.
        assert!(
            has_region(&rs, 2, 4, FoldingRangeKind::Region),
            "trailing DSL block must fold, got {rs:?}"
        );
    }
}
