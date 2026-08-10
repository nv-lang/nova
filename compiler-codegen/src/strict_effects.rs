//! Plan 197 — `--strict-effects` (experimental, opt-in, owner-approved).
//!
//! Off by default: `nova check`/`build`/`test` behavior is BYTE-IDENTICAL to
//! before this module existed unless the flag is passed (verified by the
//! conformance gate this change ran — see the Plan-197 report). No language
//! semantics change — this is tooling-only strictness, same spirit as
//! `Nova.toml`'s (currently unimplemented) `transit_effects = "error"` lint
//! knob described in spec/decisions/04-effects.md D62 §Правило 1.
//!
//! Two NEW, additive diagnostics, both gated behind [`strict_effects_enabled`]:
//!
//! 1. **`E_UNDECLARED_TRANSITIVE_EFFECT`** (implemented in
//!    `types/mod.rs::CapabilityCtx::check_transitive_effect_strict`, called
//!    from the existing `check_callee_effects` — reuses the already-built
//!    `with_handler_stack` / `declared_effects` capability-walk state instead
//!    of re-deriving handler-scope tracking here). Calling a function/method
//!    whose signature carries effect `E` from a function that neither
//!    declares `E` in its own signature nor has an enclosing
//!    `with E = … { }` handler in lexical scope. D62 §Правило 1 already
//!    specifies this as a warning by default; `--strict-effects` is the
//!    CLI-flag stand-in for `transit_effects = "error"` (no `Nova.toml`
//!    lint-config reader exists yet). `Fail` is EXCLUDED — D62 §Правило 2
//!    makes `Fail` transitivity strict and UNCONDITIONAL already (a
//!    separate, pre-existing concern, out of this flag's scope).
//!
//! 2. **`E_EFFECT_ERASED_IN_FN_TYPE`** (this module). Assigning / passing /
//!    returning a function VALUE into a `fn(...) EffectRow -> T` target whose
//!    effect-row is not a superset of the source function's own declared
//!    effects. Catches "effect erasure via fn-type coercion": calling
//!    through the narrower fn-type value bypasses the effect/handler
//!    obligation entirely — nothing ties the call back to the original,
//!    more-effectful declaration. Sub-effecting (fewer effects on the value
//!    than the target declares) is always fine — ordinary covariant
//!    widening.
//!
//! Both diagnostics are purely SYNTACTIC (V1 — conservative, same style as
//! the existing `E_UNSAFE_FN_PTR_COERCION` check in `types/mod.rs`, which is
//! explicitly documented as "conservative — fires only when detectable
//! without full type-inference"). `E_EFFECT_ERASED_IN_FN_TYPE` recognizes a
//! DIRECT named-function reference (`Ident` for a free fn, `Type.method` for
//! a static/instance method reference) at four syntactic "value flows into a
//! Func-typed slot" positions:
//!   - `let`/`const` with an explicit `fn(...) -> T` annotation,
//!   - a call argument (only when the callee resolves to exactly ONE
//!     overload — ambiguous multi-overload calls are skipped to avoid
//!     false positives from guessing the wrong overload's param types),
//!   - `return <expr>` / the function's own trailing tail-expression,
//!   - a `ClosureFull`'s own trailing tail-expression (its own return type).
//!
//! NOT tracked in V1 (documented false-negative gaps, never false
//! positives): closures/lambdas as the erased VALUE itself (only their
//! declared return-type-erasure, not effect-inference of their body),
//! re-assignment through a `mut` fn-typed binding (`Stmt::Assign`), and
//! nested (non-tail) block trailing expressions used as an outer
//! if/match-as-expression value.

use crate::ast::{
    ArrayElem, Block, ElseBranch, Expr, ExprKind, FnBody, FnDecl, Item, MatchArmBody,
    Module, Stmt, Trailing, TypeRef,
};
use crate::diag::Diagnostic;
use crate::sig_registry::SigRegistry;
use std::collections::HashSet;

/// Env-var escape hatch — same idiom as `NOVA_FIELD_CACHE*` (`field_cache.rs`
/// `from_env_or_default`) and the CLI's `--no-field-cache` family
/// (`nova-cli/src/main.rs`). `nova-cli` sets `NOVA_STRICT_EFFECTS=1` before
/// dispatch when `--strict-effects` is passed; read here at check-time so
/// NEITHER of the ~99 `check_module*` call-sites across the workspace need a
/// new parameter threaded through (§0 "no drive-by refactors" — this flag is
/// experimental and may be retracted).
pub fn strict_effects_enabled() -> bool {
    match std::env::var("NOVA_STRICT_EFFECTS") {
        Ok(v) => v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on"),
        Err(_) => false,
    }
}

/// Entry point for `E_EFFECT_ERASED_IN_FN_TYPE` — called once from
/// `check_module_impl` (types/mod.rs) right after `CapabilityCtx::check_module`
/// (which independently drives `E_UNDECLARED_TRANSITIVE_EFFECT` via its own
/// `check_callee_effects` hook). No-op unless [`strict_effects_enabled`].
pub fn check_effect_erasure(module: &Module, sig: &SigRegistry, errors: &mut Vec<Diagnostic>) {
    if !strict_effects_enabled() {
        return;
    }
    for item in &module.items {
        match item {
            Item::Fn(f) => check_fn(f, sig, errors),
            Item::Const(c) => {
                check_fn_type_target(c.ty.as_ref(), &c.value, sig, errors);
                walk_expr(&c.value, None, sig, errors);
            }
            Item::Let(d) => {
                check_fn_type_target(d.ty.as_ref(), &d.value, sig, errors);
                walk_expr(&d.value, None, sig, errors);
            }
            _ => {}
        }
    }
}

fn check_fn(f: &FnDecl, sig: &SigRegistry, errors: &mut Vec<Diagnostic>) {
    let ret_ty = f.return_type.as_ref();
    match &f.body {
        FnBody::Expr(e) => {
            check_fn_type_target(ret_ty, e, sig, errors);
            walk_expr(e, ret_ty, sig, errors);
        }
        FnBody::Block(b) => {
            if let Some(t) = &b.trailing {
                check_fn_type_target(ret_ty, t, sig, errors);
            }
            walk_block(b, ret_ty, sig, errors);
        }
        FnBody::External => {}
    }
}

fn walk_block(b: &Block, ret_ty: Option<&TypeRef>, sig: &SigRegistry, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        walk_stmt(s, ret_ty, sig, errors);
    }
    if let Some(t) = &b.trailing {
        walk_expr(t, ret_ty, sig, errors);
    }
}

fn walk_stmt(s: &Stmt, ret_ty: Option<&TypeRef>, sig: &SigRegistry, errors: &mut Vec<Diagnostic>) {
    match s {
        Stmt::Expr(e) => walk_expr(e, ret_ty, sig, errors),
        Stmt::Let(d) => {
            check_fn_type_target(d.ty.as_ref(), &d.value, sig, errors);
            walk_expr(&d.value, ret_ty, sig, errors);
        }
        Stmt::Const(c) => {
            check_fn_type_target(c.ty.as_ref(), &c.value, sig, errors);
            walk_expr(&c.value, ret_ty, sig, errors);
        }
        Stmt::Assign { target, value, .. } => {
            walk_expr(target, ret_ty, sig, errors);
            walk_expr(value, ret_ty, sig, errors);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                check_fn_type_target(ret_ty, v, sig, errors);
                walk_expr(v, ret_ty, sig, errors);
            }
        }
        Stmt::Throw { value, .. } => walk_expr(value, ret_ty, sig, errors),
        Stmt::Defer { body, .. } => walk_expr(body, ret_ty, sig, errors),
        Stmt::ConsumeScope { init, body, .. } => {
            walk_expr(init, ret_ty, sig, errors);
            for s in &body.stmts {
                walk_stmt(s, ret_ty, sig, errors);
            }
            if let Some(t) = &body.trailing {
                walk_expr(t, ret_ty, sig, errors);
            }
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                walk_expr(e, ret_ty, sig, errors);
            }
            for e in rhs {
                walk_expr(e, ret_ty, sig, errors);
            }
        }
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::AssertStatic { .. } | Stmt::Assume { .. }
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
    }
}

fn walk_expr(e: &Expr, ret_ty: Option<&TypeRef>, sig: &SigRegistry, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Call { func, args, trailing } => {
            // Call-argument erasure check: only when the callee resolves to
            // exactly ONE overload (see module doc — ambiguous overloads
            // skipped to avoid guessing the wrong param types).
            if let Some(callee) = resolve_named_fn(func, sig) {
                for (i, a) in args.iter().enumerate() {
                    if let Some(param) = callee.params.get(i) {
                        check_fn_type_target(Some(&param.ty), a.expr(), sig, errors);
                    }
                }
            }
            walk_expr(func, ret_ty, sig, errors);
            for a in args {
                walk_expr(a.expr(), ret_ty, sig, errors);
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => walk_block(b, ret_ty, sig, errors),
                    Trailing::LegacyBlockWithParams(tb) => walk_block(&tb.body, ret_ty, sig, errors),
                    Trailing::Fn(sb) => {
                        let inner_ret = sb.return_type.as_ref();
                        match &sb.body {
                            FnBody::Expr(e) => {
                                check_fn_type_target(inner_ret, e, sig, errors);
                                walk_expr(e, inner_ret, sig, errors);
                            }
                            FnBody::Block(b) => walk_block(b, inner_ret, sig, errors),
                            FnBody::External => {}
                        }
                    }
                }
            }
        }
        ExprKind::Block(b) => walk_block(b, ret_ty, sig, errors),
        ExprKind::If { cond, then, else_ } => {
            walk_expr(cond, ret_ty, sig, errors);
            walk_block(then, ret_ty, sig, errors);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_block(b, ret_ty, sig, errors),
                    ElseBranch::If(e) => walk_expr(e, ret_ty, sig, errors),
                }
            }
        }
        ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
            walk_expr(scrutinee, ret_ty, sig, errors);
            if let Some(g) = guard {
                walk_expr(g, ret_ty, sig, errors);
            }
            walk_block(then, ret_ty, sig, errors);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_block(b, ret_ty, sig, errors),
                    ElseBranch::If(e) => walk_expr(e, ret_ty, sig, errors),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr(scrutinee, ret_ty, sig, errors);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    walk_expr(g, ret_ty, sig, errors);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => walk_expr(e, ret_ty, sig, errors),
                    MatchArmBody::Block(b) => walk_block(b, ret_ty, sig, errors),
                }
            }
        }
        ExprKind::While { cond, body, .. } => {
            walk_expr(cond, ret_ty, sig, errors);
            walk_block(body, ret_ty, sig, errors);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            walk_expr(scrutinee, ret_ty, sig, errors);
            if let Some(g) = guard {
                walk_expr(g, ret_ty, sig, errors);
            }
            walk_block(body, ret_ty, sig, errors);
        }
        ExprKind::For { iter, body, .. } => {
            walk_expr(iter, ret_ty, sig, errors);
            walk_block(body, ret_ty, sig, errors);
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr(iter, ret_ty, sig, errors);
            walk_block(body, ret_ty, sig, errors);
        }
        ExprKind::Loop { body, .. } => walk_block(body, ret_ty, sig, errors),
        ExprKind::Binary { left, right, .. } => {
            walk_expr(left, ret_ty, sig, errors);
            walk_expr(right, ret_ty, sig, errors);
        }
        ExprKind::Unary { operand, .. } => walk_expr(operand, ret_ty, sig, errors),
        ExprKind::Try(inner) | ExprKind::Bang(inner) | ExprKind::RefArg(inner) => {
            walk_expr(inner, ret_ty, sig, errors)
        }
        ExprKind::Coalesce(a, b) => {
            walk_expr(a, ret_ty, sig, errors);
            walk_expr(b, ret_ty, sig, errors);
        }
        ExprKind::As(e, _) | ExprKind::Is(e, _) => walk_expr(e, ret_ty, sig, errors),
        ExprKind::Member { obj, .. } => walk_expr(obj, ret_ty, sig, errors),
        ExprKind::Index { obj, index } => {
            walk_expr(obj, ret_ty, sig, errors);
            walk_expr(index, ret_ty, sig, errors);
        }
        ExprKind::TurboFish { base, .. } => walk_expr(base, ret_ty, sig, errors),
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => walk_expr(e, ret_ty, sig, errors),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            let pairs = crate::ast::MapElem::cloned_pairs(&elems);
            for (k, v) in pairs.iter() {
                walk_expr(k, ret_ty, sig, errors);
                walk_expr(v, ret_ty, sig, errors);
            }
        }
        ExprKind::TupleLit(elems) => {
            for e in elems {
                walk_expr(e, ret_ty, sig, errors);
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    walk_expr(v, ret_ty, sig, errors);
                }
            }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            walk_expr(tag, ret_ty, sig, errors);
            for a in args {
                walk_expr(a, ret_ty, sig, errors);
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let crate::ast::InterpStrPart::Expr { expr: e, .. } = p {
                    walk_expr(e, ret_ty, sig, errors);
                }
            }
        }
        // Closures/lambdas — own scope, own (usually inferred) return type.
        // V1 does not infer a closure-light/Lambda body's effect usage, so
        // there is no target to erasure-check against; still recurse for
        // NESTED erasure sites inside the body (call-args / lets / returns
        // of THEIR OWN return, which for Lambda/ClosureLight has no
        // annotation to check against — pass `ret_ty = None`).
        ExprKind::Lambda { body, .. } => walk_expr(body, None, sig, errors),
        ExprKind::ClosureLight { body, .. } => match body {
            crate::ast::ClosureBody::Expr(e) => walk_expr(e, None, sig, errors),
            crate::ast::ClosureBody::Block(b) => walk_block(b, None, sig, errors),
        },
        ExprKind::ClosureFull(sb) => {
            let inner_ret = sb.return_type.as_ref();
            match &sb.body {
                FnBody::Expr(e) => {
                    check_fn_type_target(inner_ret, e, sig, errors);
                    walk_expr(e, inner_ret, sig, errors);
                }
                FnBody::Block(b) => {
                    if let Some(t) = &b.trailing {
                        check_fn_type_target(inner_ret, t, sig, errors);
                    }
                    walk_block(b, inner_ret, sig, errors);
                }
                FnBody::External => {}
            }
        }
        ExprKind::With { bindings, body } => {
            for b in bindings {
                walk_expr(&b.handler, ret_ty, sig, errors);
            }
            walk_block(body, ret_ty, sig, errors);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            walk_block(body, ret_ty, sig, errors);
        }
        ExprKind::Realtime { body, .. } => walk_block(body, ret_ty, sig, errors),
        ExprKind::Spawn(e) | ExprKind::Throw(e) | ExprKind::Interrupt(Some(e)) => {
            walk_expr(e, ret_ty, sig, errors)
        }
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            walk_block(body, ret_ty, sig, errors);
            if let Some(c) = cancel {
                walk_expr(c, ret_ty, sig, errors);
            }
            if let Some(dl) = deadline {
                walk_expr(&dl.expr, ret_ty, sig, errors);
            }
            if let Some(oh) = on_timeout {
                walk_expr(oh, ret_ty, sig, errors);
            }
        }
        ExprKind::Select { arms } => {
            for a in arms {
                match &a.op {
                    crate::ast::SelectOp::Recv { chan, .. } => walk_expr(chan, ret_ty, sig, errors),
                    crate::ast::SelectOp::Send { chan, value } => {
                        walk_expr(chan, ret_ty, sig, errors);
                        walk_expr(value, ret_ty, sig, errors);
                    }
                    crate::ast::SelectOp::Default => {}
                }
                if let Some(g) = &a.guard {
                    walk_expr(g, ret_ty, sig, errors);
                }
                walk_block(&a.body, ret_ty, sig, errors);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                walk_expr(s, ret_ty, sig, errors);
            }
            if let Some(en) = end {
                walk_expr(en, ret_ty, sig, errors);
            }
        }
        _ => {}
    }
}

/// Resolve `e` to a SINGLE unambiguous named-function declaration: a free-fn
/// `Ident`, or a `Type.method` / `Path(["Type","method"])` static/method
/// reference. Returns `None` for anything else (dynamic dispatch,
/// multi-overload names, closures, ...) — conservative, false-negative
/// direction only (see module doc).
fn resolve_named_fn<'a>(e: &Expr, sig: &'a SigRegistry) -> Option<&'a FnDecl> {
    match &e.kind {
        ExprKind::Ident(name) => {
            let v = sig.free_fns(name)?;
            if v.len() == 1 { Some(v[0]) } else { None }
        }
        ExprKind::Member { obj, name } => {
            let ExprKind::Ident(type_name) = &obj.kind else { return None };
            let v = sig.method_overloads(type_name, name)?;
            if v.len() == 1 { Some(v[0]) } else { None }
        }
        ExprKind::Path(parts) if parts.len() == 2 => {
            let v = sig.method_overloads(&parts[0], &parts[1])?;
            if v.len() == 1 { Some(v[0]) } else { None }
        }
        _ => None,
    }
}

/// Human-readable label for the erased-value diagnostic message.
fn fn_ref_label(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Ident(n) => n.clone(),
        ExprKind::Member { obj, name } => match &obj.kind {
            ExprKind::Ident(t) => format!("{}.{}", t, name),
            _ => name.clone(),
        },
        ExprKind::Path(parts) => parts.join("."),
        _ => "<fn>".to_string(),
    }
}

/// Last-path-segment effect names (mirrors the convention used throughout
/// `types/mod.rs::CapabilityCtx`, e.g. `path.last()` in `check_callee_effects`).
fn effect_names(effects: &[TypeRef]) -> HashSet<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            TypeRef::Named { path, .. } => path.last().cloned(),
            _ => None,
        })
        .collect()
}

/// Core check: does a function-value flowing into a `dest` fn-type slot
/// erase an effect the source function actually carries? `dest = None`
/// (no annotation / no return-type) — nothing to check against, skip.
/// `dest` not a `TypeRef::Func` — not a fn-typed slot, skip. `value` not a
/// resolvable single-overload named-fn reference — skip (V1 scope, see
/// module doc).
fn check_fn_type_target(
    dest: Option<&TypeRef>,
    value: &Expr,
    sig: &SigRegistry,
    errors: &mut Vec<Diagnostic>,
) {
    let Some(TypeRef::Func { effects: dest_effects, .. }) = dest else { return };
    let Some(src) = resolve_named_fn(value, sig) else { return };
    let dest_names = effect_names(dest_effects);
    let src_names = effect_names(&src.effects);
    let mut missing: Vec<&String> = src_names.difference(&dest_names).collect();
    if missing.is_empty() {
        return;
    }
    missing.sort();
    let missing_str = missing
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(Diagnostic::new(
        format!(
            "[E_EFFECT_ERASED_IN_FN_TYPE] `{}` carries effect(s) `{}` that the target \
             fn-type's effect-row does not cover (--strict-effects). Calling through this \
             erased binding would skip the handler/declaration obligation `{}` carries at \
             its original declaration (D62 sub-effecting rule: fewer effects on the value \
             than the target declares is fine — covariant widening; MORE is unsound \
             erasure). Hint: add `{}` to the target fn-type's effect-row, or discharge it \
             locally before erasing (e.g. wrap the call in `with {} = handler {{ … }}`).",
            fn_ref_label(value),
            missing_str,
            missing_str,
            missing_str,
            missing[0]
        ),
        value.span,
    ));
}
