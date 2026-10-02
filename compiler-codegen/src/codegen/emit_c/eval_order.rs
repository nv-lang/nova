//! Registry 221.1 #1627: the order of evaluation survives a form that writes
//! statements.
//!
//! An operand is emitted into a C expression STRING, and that string runs only
//! where the enclosing C expression is finally placed. Some forms also write C
//! STATEMENTS into `out` while being emitted (`??`, `?`, a `match` or a block
//! used as a value -- №1147 counted 27 such arms, so the question is asked of
//! the emission, not of the node's name: "did `out` grow?"). Those statements
//! land BEFORE the statement that will hold the enclosing expression, i.e.
//! before every operand emitted earlier as a string. Two wrongs followed:
//!
//! * `mark("f", 1) + (maybe(mark("g", 2)) ?? 0)` printed `g` before `f` -- the
//!   left operand's effect ran after the right operand's (D484: evaluation is
//!   left to right and observable);
//! * `false && (maybe(mark("never", 5)) ?? 0) > 3` evaluated `never` -- the
//!   right operand of `&&`/`||` ran although the left one decided (D46).
//!
//! The two doors here are the donor's (rustc_mir_build `as_operand`/`as_temp`:
//! an operand evaluated earlier is materialized into a temporary before the
//! next one is built; the right side of `&&`/`||` is lowered to a branch):
//!
//! * `spill_before(at, c, c_ty)` -- the operand `c` was emitted before `at`;
//!   statements were written after `at`. `c` is moved into a temporary
//!   declared AT `at`, so it is evaluated before them. A constant has no
//!   effect and is left alone.
//! * `lazy_right(at, op, l, r)` -- the statements written after `at` belong
//!   to the right side of a short-circuit operator: they are cut and placed
//!   inside the branch that needs them.
//!
//! The caller snapshots `self.out.len()` between the operands and asks these
//! doors only when `out` grew -- every program without such a form keeps its C
//! byte for byte.

use crate::ast::{BinOp, CallArg, Expr, ExprKind};

/// May emitting `e` write C statements? A SYNTACTIC over-approximation: the
/// truth lives in the emission (see the module doc), but a call must decide
/// BEFORE its operands are emitted, because an operand cannot be emitted
/// twice. Erring toward "yes" only adds a temporary; erring toward "no"
/// reorders the program -- so every kind not named as a pure leaf or a pure
/// wrapper answers "yes". A closure's body is not evaluated where it stands.
pub(super) fn may_write_statements(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::HexBlobLit(_) | ExprKind::BoolLit(_) | ExprKind::UnitLit
        | ExprKind::CharLit(_) | ExprKind::NullPtrLit | ExprKind::Ident(_)
        | ExprKind::Path(_) | ExprKind::SelfAccess => false,
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_)
        | ExprKind::HandlerLit { .. } | ExprKind::ProtocolLit { .. } => false,
        ExprKind::Member { obj, .. } | ExprKind::TurboFish { base: obj, .. }
        | ExprKind::As(obj, _) | ExprKind::Is(obj, _)
        | ExprKind::Unary { operand: obj, .. } => may_write_statements(obj),
        ExprKind::Index { obj, index } => may_write_statements(obj) || may_write_statements(index),
        ExprKind::Binary { left, right, .. } => may_write_statements(left) || may_write_statements(right),
        ExprKind::Call { func, args, trailing } => {
            trailing.is_some()
                || may_write_statements(func)
                || args.iter().any(|a| may_write_statements(a.expr()))
        }
        // `??`, `?`, `!!`, match/if/block values, loops, aggregate literals
        // (built field by field), interpolation (built part by part), `throw`,
        // effect scopes -- and any kind not listed above.
        _ => true,
    }
}

/// A place: reading it has no effect, and a `mut` parameter needs its
/// address -- it is never moved into a temporary.
pub(super) fn is_place(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Ident(_) | ExprKind::SelfAccess | ExprKind::Path(_) => true,
        ExprKind::Member { obj, .. } => is_place(obj),
        ExprKind::Index { obj, index } => is_place(obj) && (is_place(index) || is_literal(index)),
        _ => false,
    }
}

pub(super) fn is_literal(e: &Expr) -> bool {
    matches!(
        &e.kind,
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_) | ExprKind::HexBlobLit(_)
            | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_) | ExprKind::NullPtrLit
    )
}

/// Moved into a temporary when a later operand may write statements: not a
/// place, not a literal, not a closure value.
///
/// A form that itself writes statements (`??`, `?`, a `match` value, ...) is
/// not moved either: its effects run in those statements, in order, when it is
/// emitted, and its value is a temporary or a field of one. Moving it would
/// only add a copy -- typed by `infer_expr_c_type`, which answered the error
/// type for `V.deserialize(sub)?` in a monomorphized body (measured on
/// std/src/encoding/serde, 2026-10-02).
fn is_hoistable(e: &Expr) -> bool {
    !is_place(e)
        && !is_literal(e)
        && matches!(
            &e.kind,
            ExprKind::Call { .. } | ExprKind::Binary { .. } | ExprKind::Unary { .. }
                | ExprKind::Member { .. } | ExprKind::Index { .. } | ExprKind::As(..)
                | ExprKind::Is(..) | ExprKind::TurboFish { .. }
        )
}

impl super::CEmitter {
    /// #1627, the door for a call. Operands in evaluation order: the receiver
    /// of a method call, then the arguments. When an operand after the first
    /// may write statements, every hoistable operand up to and including the
    /// last such one is evaluated, in order, into a temporary (the technique
    /// of `emit_expr_with_reconsume_disarm`, which hoists consuming calls'
    /// arguments the same way), and the call is rebuilt over those names.
    /// `None` -- nothing to reorder, the call is emitted as it was.
    pub(super) fn order_operands(&mut self, e: &Expr) -> Result<Option<Expr>, String> {
        if let ExprKind::Index { obj, index } = &e.kind {
            return self.order_index_operands(e, obj, index);
        }
        let ExprKind::Call { func, args, trailing } = &e.kind else { return Ok(None) };
        let recv: Option<&Expr> = match &func.kind {
            ExprKind::Member { obj, .. } if is_hoistable(obj) && !matches!(obj.kind, ExprKind::TurboFish { .. }) => Some(obj),
            _ => None,
        };
        let offset = usize::from(recv.is_some());
        let mut ops: Vec<&Expr> = Vec::with_capacity(args.len() + offset);
        if let Some(r) = recv {
            ops.push(r);
        }
        for a in args {
            ops.push(a.expr());
        }
        let Some(last) = ops.iter().rposition(|o| may_write_statements(o)) else { return Ok(None) };
        if last == 0 {
            return Ok(None);
        }
        let mut hoisted: Vec<Option<Expr>> = vec![None; ops.len()];
        for (i, op) in ops.iter().enumerate().take(last + 1) {
            if is_hoistable(op) {
                hoisted[i] = Some(self.hoist_operand(op)?);
            }
        }
        // Nothing moved: the call is unchanged, and rebuilding it would only
        // bring it back to this door (the `emit_expr` wrapper asks again).
        if hoisted.iter().all(Option::is_none) {
            return Ok(None);
        }
        let new_func = match (&func.kind, hoisted.first().cloned().flatten().filter(|_| recv.is_some())) {
            (ExprKind::Member { name, .. }, Some(obj)) => Box::new(Expr {
                kind: ExprKind::Member { obj: Box::new(obj), name: name.clone() },
                span: func.span,
                id: func.id,
                debug_only: func.debug_only,
            }),
            _ => func.clone(),
        };
        let new_args: Vec<CallArg> = args
            .iter()
            .enumerate()
            .map(|(ai, a)| match hoisted[ai + offset].clone() {
                None => a.clone(),
                Some(t) => match a {
                    CallArg::Item(_) => CallArg::Item(t),
                    CallArg::Spread(_) => CallArg::Spread(t),
                    CallArg::Named { name, .. } => CallArg::Named { name: name.clone(), value: t },
                },
            })
            .collect();
        Ok(Some(Expr {
            kind: ExprKind::Call { func: new_func, args: new_args, trailing: trailing.clone() },
            span: e.span,
            id: e.id,
            debug_only: e.debug_only,
        }))
    }

    /// Evaluate `op` now into a temporary and return the name as an expression.
    fn hoist_operand(&mut self, op: &Expr) -> Result<Expr, String> {
        let c = self.emit_expr(op)?;
        let ty = self.infer_expr_c_type(op);
        if ty.trim().is_empty() || ty.trim() == "void" {
            return Err(format!(
                "internal: #1627 evaluation order -- an operand precedes a form that \
                 writes statements, and its C type is unknown (`{}`)",
                ty
            ));
        }
        // A value-record fluent `-> @` call yields a pointer to its receiver
        // (`NovaValue_X*`) while its Nova type is the value: reading it is a
        // dereference (Р5), the same conversion a `ro` binding applies.
        let c = if self.is_fluent_value_ptr_for_target(op, &ty) { format!("(*({}))", c) } else { c };
        let tmp = self.fresh_tmp();
        self.line(&format!("{} {} = {};", ty, tmp, c));
        self.var_types.insert(tmp.clone(), ty);
        Ok(Expr::new(ExprKind::Ident(tmp), op.span))
    }

    /// #1627, the door for `obj[index]`: the index arm emits the index BEFORE
    /// the base, so statements the index writes ran before a base with an
    /// effect. Such a base is evaluated first, into a temporary.
    fn order_index_operands(&mut self, e: &Expr, obj: &Expr, index: &Expr) -> Result<Option<Expr>, String> {
        if !may_write_statements(index) || !is_hoistable(obj) {
            return Ok(None);
        }
        let base = self.hoist_operand(obj)?;
        Ok(Some(Expr {
            kind: ExprKind::Index { obj: Box::new(base), index: Box::new(index.clone()) },
            span: e.span,
            id: e.id,
            debug_only: e.debug_only,
        }))
    }

    /// True when `c` is a C constant: evaluating it later changes nothing.
    fn c_is_constant(c: &str) -> bool {
        let t = c.trim().trim_start_matches('(').trim_end_matches(')').trim();
        if t == "NULL" || t.starts_with("_nova_strlit_") {
            return true;
        }
        let digits = t.strip_prefix('-').unwrap_or(t);
        digits.chars().next().map_or(false, |ch| ch.is_ascii_digit())
            && digits.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_')
    }

    /// #1627: `c` was emitted before `at`, and statements were written to `out`
    /// after `at`. Declare a temporary holding `c` at `at` and return its name.
    pub(super) fn spill_before(&mut self, at: usize, c: &str, c_ty: &str) -> Result<String, String> {
        if Self::c_is_constant(c) {
            return Ok(c.to_string());
        }
        let ty = c_ty.trim();
        if ty.is_empty() || ty == "void" {
            // Fail loud: without a type the operand cannot be evaluated early,
            // and leaving it would silently reorder the program again.
            return Err(format!(
                "internal: #1627 evaluation order -- the operand `{}` precedes a form \
                 that writes statements, and its C type is unknown (`{}`)",
                c, c_ty
            ));
        }
        let tmp = self.fresh_tmp();
        let indent = "    ".repeat(self.indent);
        self.out.insert_str(at, &format!("{}{} {} = {};\n", indent, ty, tmp, c));
        Ok(tmp)
    }

    /// #1627: the statements written to `out` after `at` were built by the
    /// right operand `r` of `&&`, `||` or `implies`; `l` is the left operand.
    /// They are moved into the branch that evaluates the right side, and the
    /// result is a `nova_bool` temporary.
    pub(super) fn lazy_right(&mut self, at: usize, op: &BinOp, l: &str, r: &str) -> String {
        let tail = self.out.split_off(at);
        let tmp = self.fresh_tmp();
        let (init, cond) = match op {
            BinOp::And => (format!("({})", l), tmp.clone()),
            BinOp::Or => (format!("({})", l), format!("!{}", tmp)),
            // `a implies b` == `!a || b`.
            _ => (format!("(!({}))", l), format!("!{}", tmp)),
        };
        self.line(&format!("nova_bool {} = {};", tmp, init));
        self.line(&format!("if ({}) {{", cond));
        for line in tail.lines() {
            if line.is_empty() {
                self.out.push('\n');
            } else {
                self.out.push_str("    ");
                self.out.push_str(line);
                self.out.push('\n');
            }
        }
        self.indent += 1;
        self.line(&format!("{} = ({});", tmp, r));
        self.indent -= 1;
        self.line("}");
        tmp
    }
}

#[cfg(test)]
mod tests {
    use super::{is_hoistable, may_write_statements};
    use crate::ast::{CallArg, Expr, ExprKind};
    use crate::diag::Span;

    fn e(kind: ExprKind) -> Expr {
        Expr::new(kind, Span::default())
    }
    fn id(n: &str) -> Expr {
        e(ExprKind::Ident(n.to_string()))
    }
    fn call(f: &str, args: Vec<Expr>) -> Expr {
        e(ExprKind::Call {
            func: Box::new(id(f)),
            args: args.into_iter().map(CallArg::Item).collect(),
            trailing: None,
        })
    }

    #[test]
    fn constants_are_not_spilled_and_effects_are() {
        let c = |s: &str| super::super::CEmitter::c_is_constant(s);
        for k in ["5", "(5)", "-3", "1.5", "0x1FULL", "_nova_strlit_3", "NULL"] {
            assert!(c(k), "{} is a constant", k);
        }
        for k in ["mark(x)", "a", "a.b", "(*p)", "x[i]"] {
            assert!(!c(k), "{} is not a constant", k);
        }
    }

    #[test]
    fn statement_forms_are_seen_through_calls_but_not_into_closures() {
        let coalesce = e(ExprKind::Coalesce(Box::new(call("maybe", vec![])), Box::new(e(ExprKind::IntLit(0)))));
        assert!(may_write_statements(&coalesce));
        assert!(may_write_statements(&call("f", vec![coalesce.clone()])), "nested in an argument");
        assert!(!may_write_statements(&call("f", vec![id("x"), e(ExprKind::IntLit(1))])));
        assert!(!may_write_statements(&id("x")));
        // A kind the predicate does not name answers "yes" (over-approximation).
        assert!(may_write_statements(&e(ExprKind::Block(crate::ast::Block { stmts: vec![], trailing: None, span: Span::default(), is_unsafe: false }))));
    }

    #[test]
    fn places_and_literals_stay_in_place() {
        assert!(!is_hoistable(&id("x")), "a place keeps its address for `mut`");
        assert!(!is_hoistable(&e(ExprKind::IntLit(1))));
        assert!(is_hoistable(&call("f", vec![])));
    }
}
