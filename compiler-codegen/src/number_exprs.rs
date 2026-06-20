//! Plan 172.1 U.4.1 — assign a stable [`ExprId`] to every `Expr` in a module.
//!
//! Runs AFTER parse + import-inlining, BEFORE type-checking. The checker then
//! annotates `ModuleEnv.resolved_types: ExprId → ResolvedType` and codegen READS
//! that annotation across `desugar` instead of re-deriving the type
//! (`infer_expr_c_type`, compiler-conventions §0/§1).
//!
//! Why a dedicated numbering pass (not a span key): parser/desugar synthesize
//! multiple distinct `Expr`s at ONE span (for-invariant wrapper; map-literal
//! lowering), so spans collide and cannot key per-`Expr` annotations. See
//! [`crate::ast::ExprId`].
//!
//! Completeness is compiler-enforced: every `match` over `ExprKind`/`Stmt`/`Item`
//! is exhaustive (no `_` arm), mirroring the authoritative traversal in
//! [`crate::desugar`]. A node left `UNSET` (e.g. spec-only lemma bodies, mirrored
//! from desugar) simply carries no annotation — the producer skips `!is_set()`
//! ids and codegen falls back, so partial numbering is sound, never wrong.

use crate::ast::*;
use std::collections::HashMap;

/// Assign sequential [`ExprId`]s (1..N) to every `Expr` in `module`, in
/// deterministic pre-order, AND seed the per-Expr resolved-type table for the
/// context-free LITERAL kinds (Plan 172.1 U.4.1 part 2 — the trivial producer;
/// the checker annotates non-literal exprs in U.4.2+). Returns `ExprId →
/// ResolvedType` for the seeded literals. Mirrors `desugar::desugar_module`'s
/// reach (`module.items` + `peer_files`).
pub fn number_exprs(module: &mut Module) -> HashMap<ExprId, crate::types::ResolvedType> {
    let mut n = Numberer { next: 1, lits: HashMap::new() };
    for item in &mut module.items {
        n.item(item);
    }
    // peer_files carry their own item copies for per-peer name resolution
    // (Plan 42.4); number them too so any consumer reading those copies sees
    // numbered exprs (distinct ids from module.items — distinct Expr instances).
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            n.item(item);
        }
    }
    n.lits
}

struct Numberer {
    /// Next id to hand out. Starts at 1 — `ExprId::UNSET` (0) is reserved for
    /// post-numbering synthesis (desugar/codegen scaffolding).
    next: u32,
    /// Plan 172.1 U.4.1 part 2: resolved-type seed for context-free literals
    /// (ExprId → ResolvedType), consumed by codegen via `infer_expr_c_type`.
    lits: HashMap<ExprId, crate::types::ResolvedType>,
}

impl Numberer {
    fn expr(&mut self, e: &mut Expr) {
        e.id = ExprId(self.next);
        self.next += 1;
        self.seed_literal(e);
        self.children(e);
    }

    /// Plan 172.1 U.4.1 part 2: record the resolved type of a LITERAL expr (the
    /// context-free trivial producer). Mirrors `infer_expr_c_type`'s literal arms
    /// (int/f64/bool/str/char/unit/`void*`): `null ptr`→opaque `Ptr`. Non-literals
    /// are produced by the checker (U.4.2+) — skipped here (no annotation → codegen
    /// falls back, sound).
    fn seed_literal(&mut self, e: &Expr) {
        use crate::types::ResolvedType as R;
        let rt = match &e.kind {
            ExprKind::IntLit(_) => R::Scalar { width: 64, signed: true, wide_default: true },
            ExprKind::FloatLit(_) => R::Float { width: 64 },
            ExprKind::BoolLit(_) => R::Bool,
            ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => R::Str,
            ExprKind::CharLit(_) => R::Named { name: "char".to_string(), args: Vec::new() },
            ExprKind::UnitLit => R::Unit,
            ExprKind::NullPtrLit => R::Ptr,
            _ => return,
        };
        self.lits.insert(e.id, rt);
    }

    fn item(&mut self, item: &mut Item) {
        match item {
            Item::Fn(f) => match &mut f.body {
                FnBody::Expr(e) => self.expr(e),
                FnBody::Block(b) => self.block(b),
                FnBody::External => {}
            },
            Item::Const(c) => self.expr(&mut c.value),
            Item::Let(l) => self.expr(&mut l.value),
            Item::Test(t) => self.block(&mut t.body),
            Item::Bench(b) => {
                for s in &mut b.setup {
                    self.stmt(s);
                }
                self.block(&mut b.measure_body);
                for s in &mut b.teardown {
                    self.stmt(s);
                }
            }
            // Mirror desugar: Type has no exprs; Lemma body is spec-only
            // (erased in codegen) — left UNSET, which is sound (no annotation).
            Item::Type(_) => {}
            Item::Lemma(_) => {}
        }
    }

    fn block(&mut self, b: &mut Block) {
        for s in &mut b.stmts {
            self.stmt(s);
        }
        if let Some(t) = &mut b.trailing {
            self.expr(t);
        }
    }

    fn stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Let(d) => self.expr(&mut d.value),
            Stmt::Const(d) => self.expr(&mut d.value),
            Stmt::Expr(e) => self.expr(e),
            Stmt::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.expr(v);
                }
            }
            Stmt::Throw { value, .. } => self.expr(value),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. }
            | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. }
            | Stmt::DeferWithResult { body, .. } => self.expr(body),
            Stmt::ConsumeScope { init, body, .. } => {
                self.expr(init);
                self.block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.expr(expr),
            // Spec-only proof statements (lemma bodies) — mirror desugar's skip.
            Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs {
                    self.expr(e);
                }
                for e in rhs {
                    self.expr(e);
                }
            }
        }
    }

    fn children(&mut self, e: &mut Expr) {
        match &mut e.kind {
            ExprKind::MapLit { elems, .. } => {
                for me in elems.iter_mut() {
                    match me {
                        MapElem::Pair(k, v) => {
                            self.expr(k);
                            self.expr(v);
                        }
                        MapElem::Spread(e) => self.expr(e),
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for el in elems.iter_mut() {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => self.expr(x),
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                for x in elems.iter_mut() {
                    self.expr(x);
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields.iter_mut() {
                    if let Some(v) = &mut f.value {
                        self.expr(v);
                    }
                }
            }
            ExprKind::Call { func, args, trailing } => {
                self.expr(func);
                for a in args.iter_mut() {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => self.expr(x),
                        CallArg::Named { value, .. } => self.expr(value),
                    }
                }
                if let Some(t) = trailing {
                    self.trailing(t);
                }
            }
            ExprKind::TurboFish { base, .. } => self.expr(base),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.expr(x),
            ExprKind::Coalesce(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.expr(x),
            ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Member { obj, .. } => self.expr(obj),
            ExprKind::Index { obj, index } => {
                self.expr(obj);
                self.expr(index);
            }
            ExprKind::If { cond, then, else_ } => {
                self.expr(cond);
                self.block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.block(b),
                        ElseBranch::If(x) => self.expr(x),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.expr(scrutinee);
                self.block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.block(b),
                        ElseBranch::If(x) => self.expr(x),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in arms.iter_mut() {
                    if let Some(g) = &mut arm.guard {
                        self.expr(g);
                    }
                    match &mut arm.body {
                        MatchArmBody::Expr(x) => self.expr(x),
                        MatchArmBody::Block(b) => self.block(b),
                    }
                }
            }
            ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
                self.expr(iter);
                self.block(body);
            }
            ExprKind::While { cond, body, .. } => {
                self.expr(cond);
                self.block(body);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.expr(scrutinee);
                self.block(body);
            }
            ExprKind::Loop { body, .. } => self.block(body),
            ExprKind::Block(b) => self.block(b),
            ExprKind::Spawn(x) => self.expr(x),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.block(b),
            ExprKind::Supervised { body, cancel } => {
                self.block(body);
                if let Some(c) = cancel {
                    self.expr(c);
                }
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.block(body);
            }
            ExprKind::Throw(x) => self.expr(x),
            ExprKind::Interrupt(opt) => {
                if let Some(x) = opt {
                    self.expr(x);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.expr(s);
                }
                if let Some(en) = end {
                    self.expr(en);
                }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts.iter_mut() {
                    if let InterpStrPart::Expr { expr: x, spec: _ } = p {
                        self.expr(x);
                    }
                }
            }
            ExprKind::TaggedTemplate { args, .. } => {
                for x in args.iter_mut() {
                    self.expr(x);
                }
            }
            ExprKind::Lambda { body, .. } => self.expr(body),
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(x) => self.expr(x),
                ClosureBody::Block(b) => self.block(b),
            },
            ExprKind::ClosureFull(sb) => match &mut sb.body {
                FnBody::Expr(x) => self.expr(x),
                FnBody::Block(b) => self.block(b),
                FnBody::External => {}
            },
            ExprKind::With { bindings, body } => {
                for b in bindings.iter_mut() {
                    self.expr(&mut b.handler);
                }
                self.block(body);
            }
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods.iter_mut() {
                    match &mut m.body {
                        HandlerMethodBody::Expr(x) => self.expr(x),
                        HandlerMethodBody::Block(b) => self.block(b),
                    }
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms.iter_mut() {
                    match &mut arm.op {
                        SelectOp::Recv { chan, .. } => self.expr(chan),
                        SelectOp::Send { chan, value } => {
                            self.expr(chan);
                            self.expr(value);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &mut arm.guard {
                        self.expr(g);
                    }
                    self.block(&mut arm.body);
                }
            }
            ExprKind::Forall { body, .. } | ExprKind::Exists { body, .. } => {
                self.expr(body);
            }
            // Leaves — no sub-expressions.
            ExprKind::Ident(_)
            | ExprKind::Path(_)
            | ExprKind::SelfAccess
            | ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_)
            | ExprKind::CharLit(_)
            | ExprKind::UnitLit
            | ExprKind::NullPtrLit => {}
        }
    }

    fn trailing(&mut self, t: &mut Trailing) {
        match t {
            Trailing::Block(b) => self.block(b),
            Trailing::LegacyBlockWithParams(tb) => self.block(&mut tb.body),
            Trailing::Fn(sb) => match &mut sb.body {
                FnBody::Expr(x) => self.expr(x),
                FnBody::Block(b) => self.block(b),
                FnBody::External => {}
            },
        }
    }
}
