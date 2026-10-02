//! Registry 221.1 #1337/#1338, checker half: a user GENERIC sum
//! (`type Opt[T] enum Som(T) | Non`) is typed by the checker the way the
//! builtin `Option`/`Result` and, after #1260, a non-generic sum already were.
//!
//! Three gaps, one class -- the emitter re-derived what the checker never said:
//!
//! * `ro a = Som(3)` bound `a` to nothing (`variant_ctor_type` refused every
//!   generic sum), so a method call on `a` got no verdict and codegen typed the
//!   receiver on its own (`obj_ty="uint32_t"` for a must-consume payload).
//!   `generic_variant_ctor_type` solves the sum's own parameters from the
//!   constructor's ARGUMENTS -- the same rule as `Some(3)` and D88's table
//!   (`spec/decisions/03-syntax.md`: inference from the arguments, else the
//!   parameter's default): nothing is taken from a later use, and a parameter
//!   neither fixes (`Non`, a variant that does not mention `T`, no default)
//!   leaves the binding untyped, as before.
//! * `match b { Som(v) => v }` over `b Opt[int]` put no `v` in scope
//!   (`match_arm_bindings` answered only for non-generic sums), so the arm type
//!   never reached the channel. `generic_sum_payload` substitutes the
//!   scrutinee's type arguments into the variant's declared payload.
//! * `annotate_expected_concrete` stamped a match in a generic body with its
//!   DECLARATION spelling (`Opt[T]`, `T` as a plain `Named`), which the erased
//!   body lowered to the nonexistent `Nova_Opt____Nova_T_p`.
//!   `expected_args_closed` holds that gate to `rt_is_closed`.

use super::*;
use constraint_solver::{Constraint, Solver, Ty, VarGen};

impl<'a> TypeCheckCtx<'a> {
    /// The type of `Variant(args)` of the generic sum `td` (named `sum`): the
    /// sum's parameters solved from the argument types. `None` when an argument
    /// is untyped or not closed (a residual generic spelling), when the
    /// solution does not fix every parameter, or on any shape mismatch.
    pub(super) fn generic_variant_ctor_type(
        &self,
        td: &TypeDecl,
        sum: &str,
        variant: &str,
        args: &[CallArg],
        scope: &HashMap<String, TypeRef>,
        span: Span,
    ) -> Option<TypeRef> {
        let TypeDeclKind::Sum(variants) = &td.kind else { return None };
        let v = variants.iter().find(|v| v.name == variant)?;
        let SumVariantKind::Tuple(fields) = &v.kind else { return None };
        if fields.len() != args.len() {
            return None;
        }
        let mut gen = VarGen::new();
        let vars: HashMap<String, constraint_solver::TypeVar> =
            td.generics.iter().map(|g| (g.name.clone(), gen.fresh())).collect();
        let params: HashSet<String> = td.generics.iter().map(|g| g.name.clone()).collect();
        let mut cs = Vec::with_capacity(fields.len());
        for (field, arg) in fields.iter().zip(args) {
            let CallArg::Item(e) = arg else { return None };
            // #1653: a field that names no parameter fixes none -- `Tag(3, "t")` with
            // `Tag(u16, T)` asked `u16 == int` of the literal and lost `T` to the
            // failed solve; the field's own agreement is #1645's door
            // (`check_variant_ctor_payload`), which judges the literal against `u16`.
            if !typeref_mentions_any(field, &params) {
                continue;
            }
            let at = ResolvedType::from_type_ref(&self.infer_expr_type(e, scope)?);
            if !self.rt_is_closed(&at) {
                return None;
            }
            let ft = ResolvedType::from_type_ref(field);
            cs.push(Constraint::Eq(Self::ty_from_resolved_vars(&ft, &vars), Ty::from_resolved(&at)));
        }
        let sol = Solver::new().solve(&cs).ok()?;
        let mut targs = Vec::with_capacity(td.generics.len());
        for g in &td.generics {
            // D88: a parameter the arguments leave open takes its declared default.
            match sol.type_of(vars[&g.name]) {
                Some(rt) => targs.push(Self::resolved_to_typeref(&rt, span)?),
                None => targs.push(g.default.clone()?),
            }
        }
        Some(TypeRef::Named { path: vec![sum.to_string()], generics: targs, span })
    }

    /// The payload type of the one-field tuple variant `variant` of the generic
    /// sum `sum_name` instantiated at `args` (the scrutinee's own arguments).
    pub(super) fn generic_sum_payload(&self, sum_name: &str, variant: &str, args: &[TypeRef]) -> Option<TypeRef> {
        let td = self.types_get_here(sum_name)?;
        if td.generics.is_empty() || td.generics.len() != args.len() {
            return None;
        }
        let TypeDeclKind::Sum(variants) = &td.kind else { return None };
        let v = variants.iter().find(|v| v.name == variant)?;
        let SumVariantKind::Tuple(tys) = &v.kind else { return None };
        let [ty] = tys.as_slice() else { return None };
        let subst: HashMap<String, TypeRef> =
            td.generics.iter().map(|g| g.name.clone()).zip(args.iter().cloned()).collect();
        Some(subst_typeref(ty, &subst))
    }

    /// `annotate_expected_concrete`'s gate for an expected type WITH arguments:
    /// every argument closed -- no bare generic-parameter spelling at any depth.
    pub(super) fn expected_args_closed(&self, generics: &[TypeRef]) -> bool {
        generics.iter().all(|g| self.rt_is_closed(&ResolvedType::from_type_ref(g)))
    }
}
