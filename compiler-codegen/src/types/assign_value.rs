//! Registry 221.1 #1611: `target = value` is judged against the target's type by the same
//! `assignable` a declaration uses.
//!
//! Before: a plain assignment had NO type check -- `coerce_assignment` applies a `#coerce`
//! pair and nothing else, and the only value check was an integer-narrowing compare for a
//! bare-name target. So `mut n = 0` then `n = "s"`, or `p.n = "s"` with `n int`, checked
//! green and the C compiler refused the program; and with `d f64`, `p.n = d` was accepted
//! and the C compiler truncated `2.5` to `2` without a word (D491: a value does not change
//! its numeric kind implicitly).
//!
//! The target's type is its inferred type -- a binding's declared type, a field's type, an
//! element's type. When inference is silent the assignment stays unjudged, as before.
//! A compound assignment (`+=`) is not judged here: its right side is an operand, and
//! operands are D405/D491's operator rule (registry 221.1 #1608).
//!
//! The same row found three more positions that no check reached at all, closed here by
//! the same `assignable`: a tuple literal's elements against a tuple type, a map
//! literal's keys and values against `HashMap[K, V]`, and the body of a closure with a
//! declared `-> R` (its tail and every `return`), its parameters in scope.

use std::collections::HashMap;

use super::*;

impl<'a> TypeCheckCtx<'a> {
    pub(super) fn check_assignment_value(
        &self,
        target: &Expr,
        value: &Expr,
        gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let Some(target_ty) = self.infer_expr_type(target, scope) else { return };
        let what = match &target.kind {
            ExprKind::Ident(n) => format!("`{n}`"),
            _ => "the assignment target".to_string(),
        };
        let shown = typeref_display(&target_ty);
        match self.assignable(value, &target_ty, gs, gs, scope) {
            Compat::Ok | Compat::Unknown => {}
            Compat::Bad { found } => errors.push(Diagnostic::new(
                format!("[E7301] cannot assign value of type `{found}` to {what} of type `{shown}`"),
                value.span,
            )),
            Compat::OutOfRange { msg } => {
                errors.push(Diagnostic::new(literal_exact::literal_diag(&msg), value.span))
            }
            Compat::Narrowing { from, to } => errors.push(Diagnostic::new(
                format!(
                    "[E_IMPLICIT_NARROWING] cannot assign value of type `{from}` to {what} of \
                     type `{to}` — {}; use an explicit `... as {to}` cast",
                    numeric_change::numeric_change_why(&from, &to),
                ),
                value.span,
            )),
            Compat::CoerceConflict { msg } => errors.push(Diagnostic::new(msg, value.span)),
            Compat::RecordLit { faults } => {
                errors.extend(faults.into_iter().map(|(m, s)| Diagnostic::new(m, s)))
            }
        }
    }

    /// A tuple literal against a tuple type, a map literal against `HashMap[K, V]`:
    /// `Some(verdict)` element by element; `None` for any other shape (the caller goes on).
    pub(super) fn composite_literal_compat(
        &self,
        expr: &Expr,
        expected: &TypeRef,
        expr_gs: &GenericScope,
        exp_gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
    ) -> Option<Compat> {
        let mut ty = expected;
        while let TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) = ty {
            ty = inner;
        }
        let pairs: Vec<(&Expr, &TypeRef)> = match (&expr.kind, ty) {
            (ExprKind::TupleLit(items), TypeRef::Tuple(tys, _)) if items.len() == tys.len() => {
                items.iter().zip(tys.iter()).collect()
            }
            (ExprKind::MapLit { elems, .. }, TypeRef::Named { path, generics, .. })
                if path.last().map(String::as_str) == Some("HashMap") && generics.len() == 2 =>
            {
                let mut v = Vec::new();
                for el in elems {
                    match el {
                        MapElem::Pair(k, x) => {
                            v.push((k, &generics[0]));
                            v.push((x, &generics[1]));
                        }
                        MapElem::Spread(_) => return None,
                    }
                }
                v
            }
            _ => return None,
        };
        for (item, item_ty) in pairs {
            match self.assignable(item, item_ty, expr_gs, exp_gs, scope) {
                Compat::Ok | Compat::Unknown => {}
                Compat::Bad { found } => {
                    // Name the whole literal when it is typed, the element otherwise.
                    let whole = self.infer_expr_type(expr, scope).map(|t| typeref_display(&t));
                    return Some(Compat::Bad { found: whole.unwrap_or(found) });
                }
                other => return Some(other),
            }
        }
        Some(Compat::Ok)
    }

    /// A closure with a declared `-> R`: its tail and every `return` against `R`, by the
    /// check a named function's body gets (`check_return_compat_*`), parameters in scope.
    pub(super) fn check_closure_return(
        &self,
        sb: &FnSigBody,
        gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let Some(ret) = &sb.return_type else { return };
        let mut inner = scope.clone();
        for p in &sb.params {
            inner.insert(p.name.clone(), p.ty.clone());
        }
        match &sb.body {
            FnBody::Expr(e) => self.check_return_compat_tail(e, ret, gs, &inner, errors),
            FnBody::Block(b) => {
                if let Some(t) = &b.trailing {
                    self.check_return_compat_tail(t, ret, gs, &inner, errors);
                }
                self.check_return_compat_in_block(b, ret, gs, &inner, errors);
            }
            FnBody::External => {}
        }
    }
}
