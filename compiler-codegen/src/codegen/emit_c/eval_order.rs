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

/// Whether evaluating an operand can be observed independently of the C
/// expression which eventually contains it.  This is deliberately separate
/// from `may_write_statements`: ordinary calls often emit no C statements,
/// but their effects still must not be reordered by the C compiler.
pub(super) fn may_have_effects(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::HexBlobLit(_) | ExprKind::BoolLit(_) | ExprKind::UnitLit
        | ExprKind::CharLit(_) | ExprKind::NullPtrLit | ExprKind::Ident(_)
        | ExprKind::Path(_) | ExprKind::SelfAccess
        | ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_)
        | ExprKind::HandlerLit { .. } | ExprKind::ProtocolLit { .. } => false,
        ExprKind::Member { obj, .. } | ExprKind::TurboFish { base: obj, .. }
        | ExprKind::As(obj, _) | ExprKind::Is(obj, _)
        | ExprKind::Unary { operand: obj, .. } => may_have_effects(obj),
        ExprKind::Index { obj, index } => may_have_effects(obj) || may_have_effects(index),
        // A call is observable even when its codegen is a plain C expression.
        ExprKind::Call { .. } => true,
        ExprKind::Binary { left, right, .. } => may_have_effects(left) || may_have_effects(right),
        // Unlisted forms can execute user code or emit runtime work.
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

/// `None` is a context-typed constant, not an independently typed operand.
/// Materializing it with `infer_expr_c_type` can freeze the fallback
/// `Option[int]` before the neighboring operand supplies its payload type.
pub(super) fn is_none_value(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Ident(name) => name == "None",
        ExprKind::Path(parts) => parts.last().is_some_and(|name| name == "None"),
        _ => false,
    }
}

/// Whether an ordinary binary expression needs explicit sequencing. A pure
/// literal/contextual `None` has no observable evaluation to order. Otherwise,
/// an effect on either side requires the peer value to be snapshotted too: the
/// effect may change a place or a pure expression that reads it.
pub(super) fn needs_binary_sequencing(left: &Expr, right: &Expr) -> bool {
    let left_effect = may_have_effects(left);
    let right_effect = may_have_effects(right);
    let left_value = !is_literal(left) && !is_none_value(left);
    let right_value = !is_literal(right) && !is_none_value(right);

    (left_effect && (right_effect || right_value)) || (right_effect && left_value)
}

/// Moved into a temporary when a later operand may write statements: not a
/// place, not a literal, not a closure value.
///
/// Statement-emitting forms are not generally moved: their generated C type
/// is not always independently inferable (`V.deserialize(sub)?` in a
/// monomorphized body inferred the error type, measured on
/// std/src/encoding/serde, 2026-10-02). `??` is the narrow exception: its
/// result type is the Option/Result payload, and it must be emitted at its
/// source position before a later call operand is hoisted.
fn is_hoistable(e: &Expr) -> bool {
    !is_place(e)
        && !is_literal(e)
        && matches!(
            &e.kind,
            ExprKind::Call { .. } | ExprKind::Binary { .. } | ExprKind::Unary { .. }
                | ExprKind::Member { .. } | ExprKind::Index { .. } | ExprKind::As(..)
                | ExprKind::Is(..) | ExprKind::TurboFish { .. } | ExprKind::Coalesce(..)
        )
}

impl super::CEmitter {
    /// Return the result type of a C call after call emission has selected its
    /// concrete overload. `infer_expr_c_type` is the fallback, but for
    /// overloaded pointer-receiver methods it can pick a same-name sibling
    /// with a different arity/return type (e.g. ro `peek() -> int` vs mut
    /// `peek(v) -> ()`). A sequencing temporary must use the callee that was
    /// actually emitted, not the speculative name-only inference.
    pub(super) fn emitted_call_result_c_type(
        &self,
        expr: &Expr,
        emitted: &str,
    ) -> Option<String> {
        if !matches!(&expr.kind, ExprKind::Call { .. }) {
            return None;
        }
        let c_name = emitted.trim().trim_start_matches('(')
            .split('(').next()?.trim();
        if let Some(sig) = self.method_overloads.values()
            .flat_map(|sigs| sigs.iter())
            .find(|sig| sig.c_name == c_name)
        {
            return Some(sig.return_c_type.clone());
        }
        if expr.id.is_set() {
            if let Some(span) = self.resolved_callees.get(&expr.id) {
                if let Some(ret) = self.fn_ret_by_span.get(span) {
                    return Some(ret.clone());
                }
            }
        }
        // Do not re-resolve a free function by its source name here:
        // free_fn_c_name_at_call intentionally fails loud when a duplicate
        // module name has no resolved-callee channel (registry #1090). In that
        // case retain infer_expr_c_type's result rather than manufacture a C
        // symbol from an ambiguous name.
        None
    }

    /// #1627, the door for a call. Operands in evaluation order: the receiver
    /// of a method call, then the arguments. When an operand after the first
    /// emits statements or has observable effects, every hoistable operand up
    /// to and including the last such one is evaluated, in order, into a
    /// temporary (the technique of `emit_expr_with_reconsume_disarm`, which
    /// hoists consuming calls' arguments the same way), and the call is rebuilt
    /// over those names.
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
        let Some(last) = ops.iter().rposition(|o| {
            may_write_statements(o) || may_have_effects(o)
        }) else { return Ok(None) };
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
        assert!(is_hoistable(&e(ExprKind::Coalesce(
            Box::new(id("maybe_value")),
            Box::new(e(ExprKind::IntLit(0))),
        ))));
    }

    #[test]
    fn ordinary_calls_are_observable_even_without_statement_emission() {
        let effect_call = call("effect", vec![]);
        assert!(!may_write_statements(&effect_call));
        assert!(super::may_have_effects(&effect_call));
        assert!(!super::may_have_effects(&id("value")));
        assert!(!super::needs_binary_sequencing(&effect_call, &e(ExprKind::IntLit(5))));
        assert!(super::needs_binary_sequencing(&effect_call, &id("value")));
        assert!(super::needs_binary_sequencing(&effect_call, &call("effect2", vec![])));
    }

    #[test]
    fn a_single_effect_against_a_literal_needs_no_typed_temporary() {
        let none = e(ExprKind::Ident("None".into()));
        let literal = e(ExprKind::IntLit(5));
        assert!(!super::needs_binary_sequencing(&call("effect", vec![]), &literal));
        assert!(!super::needs_binary_sequencing(&call("effect", vec![]), &none));
        assert!(super::needs_binary_sequencing(&call("effect", vec![]), &id("value")));
        assert!(super::needs_binary_sequencing(&call("effect", vec![]), &call("other", vec![])));
    }
}
