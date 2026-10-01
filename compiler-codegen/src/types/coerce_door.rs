//! Registry 221.1 #1451/#1452: ONE door for a `#coerce` pair (D429), from the
//! checker's verdict to the rewritten AST.
//!
//! Before: the checker ACCEPTED `str` at a `[]u8` position by the value's TYPE
//! (`assignable`'s fallback), while three separate places MATERIALIZED it by the
//! expression's FORM -- the rewrite pass (`try_coerce_leaf`: a literal or a bare
//! name only), a codegen call-arg pre-pass (only where it could find the callee's
//! C signature) and a codegen target-type arm (a literal only). Every form outside
//! their union -- a call, a field, an `if`/`match` value in `let`/`return`; any
//! non-leaf at a protocol receiver or in a `[][]u8` element -- was accepted and
//! then handed to C as `nova_str` (#1451, CC-FAIL). And none of them asked whether
//! the POSITION is writable, so the `ro []u8` view of a `str` was bound to a
//! `mut`/`consume` parameter or a `mut` local and written through (#1452: SIGSEGV
//! on a literal in `.rodata`, silent corruption of a heap `str`).
//!
//! Now: `coerce_verdict` is the only decision. `assignable` asks it and, at a
//! definite position, records the verdict per `ExprId` (`coerce_sites_buf` ->
//! `ModuleEnv.coerce_sites`); a speculative overload probe does not record
//! (`coerce_probe`). The rewrite pass splices exactly the recorded expressions
//! (`splice_checker_coerce`), whatever their form. A view-lane pair (`ro` result,
//! D429 R2) at a mutable position is `E_READONLY_COERCE` (D429 R6: `ro O` does not
//! match a `mut`/`consume` position), for every pair of that lane, not only `str`.

use super::*;

/// The checker's verdict for one expression at one typed position.
#[derive(Debug, Clone)]
pub struct CoerceSite {
    /// The pair's method: the rewrite turns `x` into `x.method()` (D429 R7).
    pub method: String,
    /// I, as the user wrote it (for the diagnostic).
    pub input: String,
    /// O's canonical key (`coerce_type_key`).
    pub output: String,
    /// View lane (non-`consume` receiver, `ro` result) vs finalize lane (D429 R2).
    pub is_view: bool,
}

/// Holds `coerce_probe_depth` raised for the lifetime of a speculative
/// `assignable` (overload filtering); see `TypeCheckCtx::coerce_probe`.
pub(super) struct CoerceProbe<'c>(&'c std::cell::Cell<u32>);

impl Drop for CoerceProbe<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

impl<'a> TypeCheckCtx<'a> {
    /// THE decision: does `expr` reach `expected` through a declared `#coerce`
    /// pair? Concrete pairs first, generic patterns only on a miss (D429 R5').
    /// `Err` is R3' (two patterns give the same pair here). The caller has
    /// already established that there is no exact match (R5).
    pub(super) fn coerce_verdict(
        &self,
        expr: &Expr,
        expected: &TypeRef,
        scope: &HashMap<String, TypeRef>,
    ) -> Option<Result<CoerceSite, String>> {
        if let Some(input_name) = self.coerce_expr_input_name(expr, scope) {
            if let Some(pairs) = self.coerce_pairs.get(&input_name) {
                let exp_key = coerce_type_key(expected);
                if let Some(p) = pairs.iter().find(|p| p.output_key == exp_key) {
                    return Some(Ok(CoerceSite {
                        method: p.method_name.clone(),
                        input: input_name,
                        output: exp_key,
                        is_view: !p.is_finalize,
                    }));
                }
            }
        }
        self.generic_coerce_lookup(expr, expected, scope)
    }

    /// Record an accepted coercion in the channel -- unless this `assignable`
    /// is a speculative probe, whose position may not be the one finally taken.
    pub(super) fn note_coerce_site(&self, expr: &Expr, site: CoerceSite) {
        if self.coerce_probe_depth.get() == 0 && expr.id.is_set() {
            self.coerce_sites_buf.borrow_mut().insert(expr.id, site);
        }
    }

    /// Mark the enclosing scope as speculative: `assignable` verdicts inside do
    /// not reach the channel. Overload filtering asks every candidate; only the
    /// chosen one is checked again, definitely, and that check records.
    pub(super) fn coerce_probe(&self) -> CoerceProbe<'_> {
        self.coerce_probe_depth.set(self.coerce_probe_depth.get() + 1);
        CoerceProbe(&self.coerce_probe_depth)
    }

    /// A definite position that does not go through `assignable` (plain
    /// assignment `x = v`, an operator's single matching overload): the same
    /// verdict, recorded the same way. `None` when `value` matches directly or
    /// no pair applies.
    pub(super) fn materialize_coerce(
        &self,
        value: &Expr,
        expected: &TypeRef,
        gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
    ) -> Option<CoerceSite> {
        if !matches!(self.assignable_direct(value, expected, gs, gs, scope), Compat::Bad { .. }) {
            return None;
        }
        let site = self.coerce_verdict(value, expected, scope)?.ok()?;
        self.note_coerce_site(value, site.clone());
        Some(site)
    }

    /// #1452: a view-lane coercion (`I -> ro O`) recorded for `value` must not
    /// land in a MUTABLE position -- a `mut`/`consume` parameter, a `mut` local,
    /// an assignment to one (D429 R6, D55 "Str -> `ro []u8`"). Called right after
    /// the position's own `assignable`, which is what recorded the verdict.
    pub(super) fn check_coerce_view_into_mut(
        &self,
        value: &Expr,
        target_is_mut: bool,
        position: &str,
        errors: &mut Vec<Diagnostic>,
    ) {
        if !target_is_mut || !value.id.is_set() {
            return;
        }
        let Some(site) = self.coerce_sites_buf.borrow().get(&value.id).cloned() else { return };
        if !site.is_view {
            return;
        }
        let CoerceSite { method, input, output, .. } = site;
        let gesture = match &value.kind {
            ExprKind::Ident(n) => format!("{n}.{method}().clone()"),
            ExprKind::StrLit(lit) if lit.chars().count() <= 24 && !lit.contains('"') => {
                format!("\"{lit}\".{method}().clone()")
            }
            _ => format!("(...).{method}().clone()"),
        };
        errors.push(Diagnostic::new(
            format!(
                "[E_READONLY_COERCE] a `{input}` value is accepted as `{output}` only as the \
                 read-only view `#coerce {input} @{method}() -> ro {output}` (D429 R2/R6), and \
                 {position} is writable: a write through it would change the `{input}`'s own \
                 immutable bytes. Pass an owned copy explicitly -- `{gesture}` -- or make \
                 the position read-only."
            ),
            value.span,
        ));
    }
}

/// #1452: does a parameter hold WRITABLE content -- `mut`/`consume` mode over a
/// type that is not itself `ro` (`mut b ro []u8` is a read-only view)?
pub(super) fn param_is_writable(param: &Param) -> bool {
    (param.is_mut || param.consume) && !param.ty.is_readonly()
}

/// How a parameter's position is named in the #1452 diagnostic.
pub(super) fn param_position(param: &Param) -> String {
    let mode = if param.consume { "consume" } else { "mut" };
    format!("the `{mode}` parameter `{}`", param.name)
}

impl<'a> TypeCheckCtx<'a> {
    /// `target = value` (plain assignment): the target's declared type is the
    /// expected type, exactly as for a `let`; the content is writable unless that
    /// type is `ro`.
    pub(super) fn coerce_assignment(
        &self,
        target: &Expr,
        value: &Expr,
        gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let Some(target_ty) = self.infer_expr_type(target, scope) else { return };
        if self.materialize_coerce(value, &target_ty, gs, scope).is_none() {
            return;
        }
        let what = match &target.kind {
            ExprKind::Ident(n) => format!("the assignment to `{n}`"),
            _ => "the assignment target".to_string(),
        };
        self.check_coerce_view_into_mut(value, !target_ty.is_readonly(), &what, errors);
    }

    /// A method called on a PROTOCOL receiver (`fn go(mut w Sink) { w.put(s) }`) has
    /// no `FnDecl` for the call-argument check to bind against, so its arguments
    /// reach no `assignable` at all. The `#coerce` door is applied to them here from
    /// the protocol's own declaration (embeds included): one method of that name, no
    /// method-level generics, a parameter type that does not mention `Self`.
    pub(super) fn coerce_protocol_method_args(
        &self,
        proto_name: &str,
        method_name: &str,
        args: &[CallArg],
        gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let mut found: Vec<&EffectMethod> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        self.collect_protocol_methods(proto_name, method_name, &mut seen, &mut found);
        let [m] = found.as_slice() else { return };
        if !m.generics.is_empty() {
            return;
        }
        let Ok(bindings) = crate::argbind::bind_call_args(&m.params, args) else { return };
        let self_only: HashSet<String> = std::iter::once("Self".to_string()).collect();
        for (pi, binding) in bindings.iter().enumerate() {
            let ai = match binding {
                crate::argbind::ArgBinding::Positional(i) | crate::argbind::ArgBinding::Named(i) => *i,
                _ => continue,
            };
            let (Some(param), Some(arg)) = (m.params.get(pi), args.get(ai)) else { continue };
            if param.is_variadic || typeref_mentions_any(&param.ty, &self_only) {
                continue;
            }
            if self.materialize_coerce(arg.expr(), &param.ty, gs, scope).is_some() {
                self.check_coerce_view_into_mut(
                    arg.expr(),
                    param_is_writable(param),
                    &param_position(param),
                    errors,
                );
            }
        }
    }

    fn collect_protocol_methods(
        &self,
        proto_name: &str,
        method_name: &str,
        seen: &mut HashSet<String>,
        out: &mut Vec<&'a EffectMethod>,
    ) {
        if !seen.insert(proto_name.to_string()) {
            return;
        }
        let Some(td) = self.types_get_here(proto_name) else { return };
        let TypeDeclKind::Protocol { methods, embeds } = &td.kind else { return };
        out.extend(methods.iter().filter(|m| m.name.trim_start_matches('@') == method_name));
        for e in embeds {
            if let TypeRef::Named { path, .. } = e {
                if let Some(emb) = path.last() {
                    self.collect_protocol_methods(emb, method_name, seen, out);
                }
            }
        }
    }

    /// A returned `if`/`match`/block whose OWN type reaches the declared return
    /// through a pair is coerced whole (`(if c { s } else { t }).bytes()`), as in a
    /// `let` -- not leaf by leaf, which left the construct's own C value typed as
    /// the source (`nova_str`) around converted leaves. `true` when recorded.
    pub(super) fn coerce_return_container(
        &self,
        e: &Expr,
        ret: &TypeRef,
        gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
    ) -> bool {
        matches!(
            e.kind,
            ExprKind::If { .. } | ExprKind::IfLet { .. } | ExprKind::Match { .. } | ExprKind::Block(_)
        ) && self.materialize_coerce(e, ret, gs, scope).is_some()
    }
}

impl MapLitAnnotator<'_> {
    /// #1451: the materialization half of the door -- `e` becomes `e.method()`
    /// exactly when the checker accepted `e` through a `#coerce` pair (the
    /// channel), whatever `e`'s form. Called post-order from `walk_expr`, after
    /// `e`'s own children were walked, so the spliced call is never revisited.
    pub(super) fn splice_checker_coerce(&mut self, e: &mut Expr) {
        if !e.id.is_set() {
            return;
        }
        let Some(site) = self.coerce_sites.get(&e.id) else { return };
        let method = site.method.clone();
        Self::splice_coerce_call(e, method);
    }
}
