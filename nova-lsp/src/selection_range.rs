//! `textDocument/selectionRange` — Plan 104.10 Ф.17.
//!
//! Purely **syntactic** smart-expand. For each requested position we parse the
//! buffer to an AST (no type-checking, no `expr_types`) and derive a chain of
//! *expanding* ranges from the real **AST node hierarchy**, not from a
//! bracket-matching or indentation heuristic. Concretely, the chain grows from
//! the innermost grammar construct containing the cursor outward to the whole
//! declaration:
//!
//! ```text
//! identifier / literal   (leaf Expr span)
//!   ⊂ enclosing expression (Call / Binary / Member / If / Match / … span)
//!     ⊂ enclosing statement (Let / Assign / Return / Expr-stmt / … span)
//!       ⊂ enclosing block   ({ … } Block span)
//!         ⊂ the fn / type / test / bench / lemma declaration (Item span)
//! ```
//!
//! Every level corresponds to a real span already stored on the AST node, so
//! each range is a strict superset of the previous one (`parent ⊃ child`), which
//! is exactly the invariant LSP requires of a [`SelectionRange`] chain.
//!
//! ## Algorithm
//!
//! 1. Parse once. On parse failure every position degrades to a minimal
//!    (empty) range at the cursor — graceful, never an error.
//! 2. For each position → byte offset, walk the AST collecting **every** node
//!    span that contains the offset. Because sibling spans in a well-formed AST
//!    do not overlap, the set of containing spans always lies on a single
//!    root-to-leaf path and therefore nests.
//! 3. De-duplicate coincident spans (e.g. `Stmt::Expr(e)` whose statement span
//!    equals the inner expression span), sort by width ascending, and keep only
//!    a strictly-nesting subsequence (defensive against parser-recovery spans
//!    that might not nest cleanly).
//! 4. Fold the innermost→outermost list into a linked [`SelectionRange`] whose
//!    `parent` pointers walk outward.
//!
//! A position that falls outside any code (blank line, trailing whitespace,
//! inside a discarded comment) collects no spans and yields a minimal range
//! `{ start == end == position }` with no parent — the LSP-mandated fallback so
//! the result array always has exactly one entry per input position.
//!
//! Offsets are UTF-8/UTF-16-safe via the shared `ropey` mapping
//! (`position_to_byte_offset` / `byte_offset_to_position`), so multi-byte
//! content before the cursor never skews a range boundary.

use nova_codegen::ast::{
    Block, ClosureBody, ElseBranch, Expr, ExprKind, FnBody, HandlerMethodBody, Item, MapElem,
    MatchArmBody, Module, SelectOp, Stmt, Trailing,
};
use nova_codegen::diag::Span;
use ropey::Rope;
use tower_lsp::lsp_types::{Position, Range, SelectionRange};

use crate::diagnostic_mapping::{byte_offset_to_position, position_to_byte_offset};

/// Compute a [`SelectionRange`] for every position in `positions`.
///
/// The returned vector has exactly `positions.len()` entries, index-aligned
/// with the input (LSP requirement). On parse failure — or for any position
/// outside code — the corresponding entry is a minimal empty range at the
/// cursor. Safe to run inside `run_with_large_stack`.
pub fn compute_selection_ranges(src: &str, positions: &[Position]) -> Vec<SelectionRange> {
    let rope = Rope::from_str(src);
    let module = nova_codegen::parser::parse(src).ok();

    positions
        .iter()
        .map(|pos| {
            let minimal = SelectionRange {
                range: Range { start: *pos, end: *pos },
                parent: None,
            };
            let Some(module) = module.as_ref() else {
                return minimal;
            };
            let offset = position_to_byte_offset(&rope, pos.line, pos.character);
            let mut c = Collector { target: offset, out: Vec::new() };
            c.collect_module(module);
            build_chain(&rope, c.out).unwrap_or(minimal)
        })
        .collect()
}

/// Turn a bag of containing spans into a nested [`SelectionRange`].
///
/// De-duplicates, sorts by width ascending, keeps a strictly-nesting
/// innermost→outermost subsequence, then links `parent` pointers outward.
/// Returns `None` when no span contained the cursor (caller substitutes the
/// minimal range).
fn build_chain(rope: &Rope, mut spans: Vec<Span>) -> Option<SelectionRange> {
    if spans.is_empty() {
        return None;
    }
    // Sort by (width asc, start asc) then dedup identical spans.
    spans.sort_by(|a, b| {
        let wa = a.end.saturating_sub(a.start);
        let wb = b.end.saturating_sub(b.start);
        wa.cmp(&wb).then(a.start.cmp(&b.start)).then(a.end.cmp(&b.end))
    });
    spans.dedup_by(|a, b| a.start == b.start && a.end == b.end);

    // Keep only a strictly-nesting chain: each kept span must fully contain the
    // previously kept (smaller) one and be strictly larger.
    let mut chain: Vec<Span> = Vec::with_capacity(spans.len());
    for sp in spans {
        match chain.last() {
            None => chain.push(sp),
            Some(prev) => {
                let contains = sp.start <= prev.start && sp.end >= prev.end;
                let strictly_larger = sp.start < prev.start || sp.end > prev.end;
                if contains && strictly_larger {
                    chain.push(sp);
                }
            }
        }
    }

    // Link parents from outermost (last) inward, so the returned node is the
    // innermost range with a `parent` chain reaching the outermost.
    let mut node: Option<Box<SelectionRange>> = None;
    for sp in chain.iter().rev() {
        node = Some(Box::new(SelectionRange {
            range: span_to_range(rope, *sp),
            parent: node.take(),
        }));
    }
    node.map(|b| *b)
}

fn span_to_range(rope: &Rope, span: Span) -> Range {
    let start = byte_offset_to_position(rope, span.start);
    // `span.end` is exclusive; map it directly so the range's end sits just past
    // the last byte (LSP end is exclusive too).
    let end = byte_offset_to_position(rope, span.end);
    Range { start, end }
}

/// Walks the AST accumulating every node span that contains `target`.
struct Collector {
    target: usize,
    out: Vec<Span>,
}

impl Collector {
    /// Record `span` iff it contains the cursor (inclusive at both ends so a
    /// cursor resting just after an identifier still selects it).
    fn push(&mut self, span: Span) {
        if span.start <= self.target && self.target <= span.end {
            self.out.push(span);
        }
    }

    // ── Top level ─────────────────────────────────────────────────────────────

    fn collect_module(&mut self, module: &Module) {
        for item in &module.items {
            self.collect_item(item);
        }
    }

    fn collect_item(&mut self, item: &Item) {
        self.push(item_span(item));
        match item {
            Item::Fn(fd) => self.walk_fn_body(&fd.body),
            Item::Type(_) => {}
            Item::Let(ld) => self.walk_expr(&ld.value),
            Item::Const(cd) => self.walk_expr(&cd.value),
            Item::Test(td) => self.walk_block(&td.body),
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
            Item::Lemma(ld) => self.walk_fn_body(&ld.body),
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

    fn walk_block(&mut self, block: &Block) {
        self.push(block.span);
        for s in &block.stmts {
            self.walk_stmt(s);
        }
        if let Some(t) = &block.trailing {
            self.walk_expr(t);
        }
    }

    fn walk_stmt(&mut self, stmt: &Stmt) {
        self.push(stmt_span(stmt));
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

    fn walk_expr(&mut self, expr: &Expr) {
        self.push(expr.span);
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

/// Span of an item's whole declaration (the outermost selection level).
fn item_span(item: &Item) -> Span {
    match item {
        Item::Fn(d) => d.span,
        Item::Type(d) => d.span,
        Item::Let(d) => d.span,
        Item::Const(d) => d.span,
        Item::Test(d) => d.span,
        Item::Bench(d) => d.span,
        Item::Lemma(d) => d.span,
    }
}

/// Span of a statement (some variants carry it inline, others in the wrapped
/// decl / expression).
fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Let(d) => d.span,
        Stmt::Const(d) => d.span,
        Stmt::Expr(e) => e.span,
        Stmt::Assign { span, .. }
        | Stmt::TupleAssign { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Throw { span, .. }
        | Stmt::Defer { span, .. }
        | Stmt::ConsumeScope { span, .. }
        | Stmt::AssertStatic { span, .. }
        | Stmt::Assume { span, .. }
        | Stmt::Apply { span, .. }
        | Stmt::Calc { span, .. }
        | Stmt::Reveal { span, .. } => *span,
        Stmt::Break(s) | Stmt::Continue(s) => *s,
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sr(src: &str, pos: Position) -> SelectionRange {
        compute_selection_ranges(src, &[pos]).into_iter().next().unwrap()
    }

    /// Collect a chain as a flat innermost→outermost Vec of ranges.
    fn flatten(mut node: SelectionRange) -> Vec<Range> {
        let mut out = vec![node.range];
        while let Some(p) = node.parent.take() {
            out.push(p.range);
            node = *p;
        }
        out
    }

    /// A range strictly contains another (superset, and larger).
    fn strictly_contains(outer: Range, inner: Range) -> bool {
        let outer_before = outer.start.line < inner.start.line
            || (outer.start.line == inner.start.line && outer.start.character <= inner.start.character);
        let outer_after = outer.end.line > inner.end.line
            || (outer.end.line == inner.end.line && outer.end.character >= inner.end.character);
        let strictly = outer.start != inner.start || outer.end != inner.end;
        outer_before && outer_after && strictly
    }

    fn pos(line: u32, ch: u32) -> Position {
        Position { line, character: ch }
    }

    /// Assert every parent strictly contains its child (LSP invariant).
    fn assert_nesting(chain: &[Range]) {
        for w in chain.windows(2) {
            let (inner, outer) = (w[0], w[1]);
            assert!(
                strictly_contains(outer, inner),
                "parent {outer:?} must strictly contain child {inner:?}"
            );
        }
    }

    // ── POS ──────────────────────────────────────────────────────────────────

    /// POS: cursor on an identifier inside `x + y` expands
    /// ident → binary expr → let-stmt → fn body block → fn decl.
    #[test]
    fn pos_expand_ident_to_fn() {
        let src = "\
module app.mod
fn main() {
  ro sum = alpha + beta
}
";
        // Line 2 (0-based): "  ro sum = alpha + beta"
        //                    01234567890123456789012
        // `alpha` starts at char 11; cursor at 13 lands inside it.
        let chain = flatten(sr(src, pos(2, 13)));
        assert!(
            chain.len() >= 4,
            "expected ident→expr→stmt→block→fn (≥4 levels), got {} levels: {chain:?}",
            chain.len()
        );
        assert_nesting(&chain);

        // Innermost must be the identifier `alpha` (line 2, chars 11..16).
        let inner = chain[0];
        assert_eq!(inner.start, pos(2, 11), "innermost starts at `alpha`");
        assert_eq!(inner.end, pos(2, 16), "innermost ends after `alpha`");

        // The binary expression `alpha + beta` must appear as an ancestor.
        assert!(
            chain.iter().any(|r| r.start == pos(2, 11) && r.end == pos(2, 23)),
            "binary expr `alpha + beta` must be a level, got {chain:?}"
        );

        // Outermost must be the whole fn decl: from `fn` (line 1) to `}` (line 3).
        let outer = *chain.last().unwrap();
        assert_eq!(outer.start.line, 1, "outermost starts on the `fn` line");
        assert_eq!(outer.end.line, 3, "outermost ends on the closing brace line");
    }

    /// POS: several positions in one request each get an independent chain,
    /// index-aligned with the input.
    #[test]
    fn pos_multiple_positions_one_request() {
        let src = "\
module app.mod
fn f() {
  ro a = 1
  ro b = 2
}
";
        // pos0: on `1` (line 2, char 9); pos1: on `b` (line 3, char 5).
        let out = compute_selection_ranges(src, &[pos(2, 9), pos(3, 5)]);
        assert_eq!(out.len(), 2, "one SelectionRange per input position");

        let c0 = flatten(out[0].clone());
        let c1 = flatten(out[1].clone());
        assert_nesting(&c0);
        assert_nesting(&c1);
        // Each innermost sits on its own line.
        assert_eq!(c0[0].start.line, 2, "first chain innermost on line 2");
        assert_eq!(c1[0].start.line, 3, "second chain innermost on line 3");
        // Both chains reach the same enclosing fn (line 1 → 4).
        assert_eq!(c0.last().unwrap().start.line, 1);
        assert_eq!(c1.last().unwrap().start.line, 1);
    }

    /// POS: cursor inside a nested `if` block expands through the block levels:
    /// inner statement → if-then block → fn body block → fn decl.
    #[test]
    fn pos_expand_through_nested_block() {
        let src = "\
module app.mod
fn main() {
  if true {
    ro z = 7
  }
}
";
        // Line 3: "    ro z = 7" — cursor on `7` at char 11.
        let chain = flatten(sr(src, pos(3, 11)));
        assert_nesting(&chain);
        // Expect at least: expr `7` → let-stmt → if-then block → fn block → fn.
        assert!(
            chain.len() >= 4,
            "nested block must yield ≥4 levels, got {chain:?}"
        );
        // A level matching the inner `{ … }` if-then block: line 2 → line 4.
        assert!(
            chain.iter().any(|r| r.start.line == 2 && r.end.line == 4),
            "if-then block level (lines 2..4) missing: {chain:?}"
        );
        // A level matching the outer fn body block/decl: reaching line 5.
        assert!(
            chain.iter().any(|r| r.end.line == 5),
            "outer fn level (ending line 5) missing: {chain:?}"
        );
    }

    // ── NEG ──────────────────────────────────────────────────────────────────

    /// NEG: a position outside any code (blank line) yields a minimal empty
    /// range at the cursor, with no parent.
    #[test]
    fn neg_position_outside_code_minimal_range() {
        let src = "\
module app.mod

fn main() => 42
";
        // Line 1 is blank — no AST node covers it.
        let p = pos(1, 0);
        let node = sr(src, p);
        assert_eq!(node.range.start, p, "minimal range starts at cursor");
        assert_eq!(node.range.end, p, "minimal range is empty (start == end)");
        assert!(node.parent.is_none(), "minimal range has no parent");
    }

    /// NEG: a parse failure degrades every position to the minimal range
    /// (no panic, no error).
    #[test]
    fn neg_parse_error_minimal_range() {
        let src = "module app.mod\nfn ( { { { unbalanced";
        let p = pos(1, 3);
        let node = sr(src, p);
        assert_eq!(node.range.start, p);
        assert_eq!(node.range.end, p);
        assert!(node.parent.is_none());
    }

    // ── EDGE ─────────────────────────────────────────────────────────────────

    /// EDGE: nested calls `f(g(x))` — cursor on `x` expands
    /// `x` → `g(x)` → `f(g(x))` → … each a strict superset.
    #[test]
    fn edge_nested_calls_expand() {
        let src = "\
module app.mod
fn main() {
  ro r = f(g(x))
}
";
        // Line 2: "  ro r = f(g(x))"
        //          0123456789012345
        // `f` at 9, `g` at 11, `x` at 13.
        let chain = flatten(sr(src, pos(2, 13)));
        assert_nesting(&chain);
        // Innermost `x`: chars 13..14.
        assert_eq!(chain[0].start, pos(2, 13), "innermost is `x`");
        assert_eq!(chain[0].end, pos(2, 14));
        // `g(x)`: chars 11..15.
        assert!(
            chain.iter().any(|r| r.start == pos(2, 11) && r.end == pos(2, 15)),
            "inner call `g(x)` must be a level, got {chain:?}"
        );
        // `f(g(x))`: chars 9..16.
        assert!(
            chain.iter().any(|r| r.start == pos(2, 9) && r.end == pos(2, 16)),
            "outer call `f(g(x))` must be a level, got {chain:?}"
        );
    }

    /// EDGE: multi-byte content before the cursor must not skew boundaries.
    /// A Cyrillic string precedes the target identifier on the same line.
    #[test]
    fn edge_multibyte_boundaries() {
        let src = "\
module app.mod
fn main() {
  ro s = \"Привет\" + tail
}
";
        // Line 2: `  ro s = "Привет" + tail`
        // UTF-16 columns: `"` at 9, `Привет` 6 units 10..16, `"` at 16,
        // space 17, `+` 18, space 19, `tail` 20..24.
        let chain = flatten(sr(src, pos(2, 21)));
        assert_nesting(&chain);
        // Innermost identifier `tail` occupies UTF-16 cols 20..24.
        assert_eq!(chain[0].start, pos(2, 20), "innermost `tail` start (UTF-16)");
        assert_eq!(chain[0].end, pos(2, 24), "innermost `tail` end (UTF-16)");
    }
}
