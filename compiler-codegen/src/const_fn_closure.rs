//! Plan 114.4.4.4 V4.3: closure-returning const fn specialization.
//!
//! Allows pattern:
//! ```nova
//! fn make_adder(const n int) -> const fn(int) -> int =>
//!     |x| x + n   // captures const param n
//!
//! ro adder5 = make_adder(5)   // ⇒ Ident("make_adder__closure_0")
//! adder5(3) == 8              // calls specialized fn (x int) -> int { x + 5 }
//! ```
//!
//! Strategy:
//! - Detect fully-const fn whose body is a single closure literal
//!   (`FnBody::Expr(Lambda | ClosureLight | ClosureFull)`).
//! - At each Call site `host_fn(LITERAL_ARGS)`:
//!   - Generate specialized top-level fn `<host>__closure_<idx>` где
//!     params = closure params (типы выведены из closure annotation OR
//!     host fn's declared return-type `fn(P1,..) -> R`), body =
//!     closure body с host's const params substituted с literal values,
//!     return type = host's return-type's return (or closure's explicit).
//!   - Memoize per (host_fn_name, const_args) — identical calls reuse.
//!   - Replace Call expression с `Ident(spec_name)` — Nova treats bare
//!     fn name as fn pointer (см. tests basics/functions.nv).
//!
//! Pipeline placement: внутри `rewrite_const_fn_calls` ДО main walker
//! (так что walker не пытается evaluate'ить эти Calls через
//! eval_call, которое не умеет closure ConstValue).
//!
//! V1 limitations:
//! - Closure body validated по обычным const fn body rules (с
//!   расширенным param scope: host const params + closure params).
//!   Это значит body может: literal-арифметика, control flow,
//!   calls to other const fns, intrinsics. Не может: side effects,
//!   spawn/handler/defer/throw.
//! - Closure return type выводится из host fn's `-> fn(...) -> R`
//!   declaration. Если host fn declared без `fn(..)` return type —
//!   reject с `E_CONST_FN_CLOSURE_NO_RET_TYPE`.
//! - Generic closure-returning const fn — V2 followup
//!   `[M-114.4.4-closure-generic]`.
//! - First-class use of closure-returning fn name (`ro f = make_adder`)
//!   rejected: value per-call-different, нет single trampoline. Friendly
//!   error `E_CONST_FN_CLOSURE_FIRST_CLASS`.

use std::collections::HashMap;

use crate::ast::{
    ArrayElem, Block, CallArg, ClosureBody, ElseBranch, Expr, ExprKind, FnBody, FnDecl, Item,
    LambdaParam, MapElem, MatchArmBody, Module, Param, Stmt, Trailing, TypeRef,
};
use crate::const_fn_eval::{ConstValue, try_literal_to_value};
use crate::diag::{Diagnostic, Span};

pub fn specialize_closure_returning_const_fns(module: &mut Module) -> Vec<Diagnostic> {
    let mut errors: Vec<Diagnostic> = Vec::new();

    // Step 1 — collect closure-returning fully-const fns.
    // Plan 114.4.4 V4.4 Ф.2 [M-114.4.4-closure-captures-outer]: also accept
    // Block body where stmts = all `Stmt::Const` decls + trailing = closure
    // literal. Captured outer const decls evaluated при специализации с
    // host const params substituted, results merged в subst map для closure
    // body.
    let mut closure_fns: HashMap<String, FnDecl> = HashMap::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            let all_const = !fd.params.is_empty() && fd.params.iter().all(|p| p.is_const);
            let is_fully = (all_const || fd.params.is_empty()) && fd.return_is_const;
            if !is_fully { continue; }
            if has_closure_returning_body(&fd.body) {
                closure_fns.insert(fd.name.clone(), fd.clone());
            }
        }
    }
    if closure_fns.is_empty() {
        return errors;
    }

    // Step 2 — walk module, rewrite Call sites + collect specs.
    let mut specs: HashMap<(String, Vec<ConstValue>), String> = HashMap::new();
    let mut spec_counter: usize = 0;
    for item in &mut module.items {
        rewrite_item(item, &closure_fns, &mut specs, &mut spec_counter, &mut errors);
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            rewrite_item(item, &closure_fns, &mut specs, &mut spec_counter, &mut errors);
        }
    }

    // Step 3 — generate specialized FnDecls.
    let mut sorted_specs: Vec<((&String, &Vec<ConstValue>), &String)> = specs
        .iter()
        .map(|((n, a), s)| ((n, a), s))
        .collect();
    sorted_specs.sort_by(|a, b| a.1.cmp(b.1));
    let mut specialized_items: Vec<Item> = Vec::new();
    for ((host_name, const_args), spec_name) in sorted_specs {
        let host = &closure_fns[host_name];
        match generate_closure_spec(host, const_args, spec_name) {
            Ok(fd) => specialized_items.push(Item::Fn(fd)),
            Err(d) => errors.push(d),
        }
    }
    module.items.extend(specialized_items);

    // Step 4 — detect leftover bare-Ident references к closure-returning
    // fn names → friendly error (V1 не supports first-class use).
    let closure_names: std::collections::HashSet<String> = closure_fns.keys().cloned().collect();
    {
        let mut ctx = FirstClassRejectCtx {
            closure_fns: &closure_names,
            errors: &mut errors,
        };
        for item in &module.items {
            ctx.visit_item(item);
        }
        for pf in &module.peer_files {
            for item in &pf.items_here {
                ctx.visit_item(item);
            }
        }
    }

    errors
}

fn is_closure_literal(e: &Expr) -> bool {
    matches!(
        &e.kind,
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_)
    )
}

/// Plan 114.4.4 V4.4 Ф.2: body форма допускает либо
/// `FnBody::Expr(closure)` либо `FnBody::Block { stmts: all_const_decls,
/// trailing: closure_literal }` для outer-captures pattern.
fn has_closure_returning_body(body: &FnBody) -> bool {
    match body {
        FnBody::Expr(e) => is_closure_literal(e),
        FnBody::Block(b) => {
            // V1 outer captures: stmts must be all Stmt::Const.
            let stmts_ok = b.stmts.iter().all(|s| matches!(s, Stmt::Const(_)));
            let trailing_ok = b.trailing.as_ref()
                .map(|t| is_closure_literal(t))
                .unwrap_or(false);
            stmts_ok && trailing_ok
        }
        FnBody::External => false,
    }
}

/// Extract outer const decls + closure expr из body. Caller'у выдаём
/// (Vec<(name, span, value_expr)>, closure_expr).
fn extract_outer_consts_and_closure<'a>(
    body: &'a FnBody,
) -> Option<(Vec<(String, Span, &'a Expr)>, &'a Expr)> {
    match body {
        FnBody::Expr(e) if is_closure_literal(e) => Some((Vec::new(), e)),
        FnBody::Block(b) => {
            let mut consts = Vec::new();
            for s in &b.stmts {
                if let Stmt::Const(cd) = s {
                    consts.push((cd.name.clone(), cd.span, &cd.value));
                } else {
                    return None;
                }
            }
            let trailing = b.trailing.as_ref()?;
            if !is_closure_literal(trailing) { return None; }
            Some((consts, trailing))
        }
        _ => None,
    }
}

/// Substitute identifiers in expr (in-place) according to subst map. Used
/// для outer const RHS evaluation: host param refs → literals, prior outer
/// const refs → their evaluated values.
fn subst_then_eval(e: &Expr, subst: &HashMap<String, ConstValue>) -> Option<ConstValue> {
    let mut cloned = e.clone();
    subst_expr(&mut cloned, subst);
    try_literal_to_value(&cloned)
}

// =========================================================================
// Step 3 — Specialization (clone closure body, substitute const params).
// =========================================================================

fn generate_closure_spec(
    host: &FnDecl,
    const_args: &[ConstValue],
    spec_name: &str,
) -> Result<FnDecl, Diagnostic> {
    // Build substitution map: host const param name → literal value.
    let mut subst: HashMap<String, ConstValue> = HashMap::new();
    for (p, v) in host.params.iter().zip(const_args.iter()) {
        subst.insert(p.name.clone(), v.clone());
    }

    // Plan 114.4.4 V4.4 Ф.2 [M-114.4.4-closure-captures-outer]: extract
    // outer const decls (Stmt::Const before trailing closure). Evaluate
    // each в порядке declaration с current subst map (host params + prior
    // outer consts), результат добавляем в map.
    let (outer_consts, closure_expr) = extract_outer_consts_and_closure(&host.body)
        .ok_or_else(|| Diagnostic::new(
            "[E_CONST_FN_CLOSURE_BODY] body должен быть closure literal или \
             Block с Stmt::Const decls + trailing closure (V4.3).".to_string(),
            host.span,
        ))?;
    for (name, span, value_expr) in &outer_consts {
        let v = subst_then_eval(value_expr, &subst).ok_or_else(|| Diagnostic::new(
            format!(
                "[E_CONST_FN_CLOSURE_OUTER_NOT_CONST] outer const `{}` в \
                 closure-returning fn `{}` body не constexpr после substitution \
                 host params (V4.4 Ф.2). Body должен использовать только literal \
                 arithmetic / const refs к host params / previous outer consts.",
                name, host.name
            ),
            *span,
        ))?;
        subst.insert(name.clone(), v);
    }

    // Extract closure params, body, return type. Param types выводим из
    // host fn's `-> const fn(P1,..) -> R` declaration.
    let host_ret_func = extract_func_signature(&host.return_type, host.span)?;
    let (closure_params, closure_body, closure_ret_explicit) =
        extract_closure_parts_from_expr(closure_expr, host.span, host_ret_func.0.clone())?;

    // Derive return type: closure's explicit annotation (Lambda/ClosureFull)
    // или host fn's declared `fn(..) -> R` second component.
    let return_type = closure_ret_explicit.or(host_ret_func.1);

    // Build specialized FnDecl.
    let mut spec = host.clone();
    spec.name = spec_name.to_string();
    spec.params = closure_params;
    spec.return_type = return_type;
    spec.return_is_const = false;
    spec.body = match closure_body {
        ClosureBodyForm::Expr(mut e) => {
            subst_expr(&mut e, &subst);
            FnBody::Expr(e)
        }
        ClosureBodyForm::Block(mut b) => {
            subst_block(&mut b, &subst);
            FnBody::Block(b)
        }
    };
    Ok(spec)
}

enum ClosureBodyForm {
    Expr(Expr),
    Block(Block),
}

/// Extract closure components from a closure literal Expr.
/// Returns (closure_params_as_FnParams, closure_body, closure_return_type_explicit).
fn extract_closure_parts_from_expr(
    e: &Expr,
    _host_span: Span,
    expected_param_types: Vec<TypeRef>,
) -> Result<(Vec<Param>, ClosureBodyForm, Option<TypeRef>), Diagnostic> {
    let span = e.span;
    match &e.kind {
        ExprKind::Lambda { params, return_type, body, .. } => {
            let fn_params = lambda_params_to_fn_params(params, &expected_param_types, span)?;
            Ok((fn_params, ClosureBodyForm::Expr((**body).clone()), return_type.clone()))
        }
        ExprKind::ClosureLight { params, body } => {
            let fn_params = closure_light_params_to_fn_params(
                params, &expected_param_types, span,
            )?;
            let body_form = match body {
                ClosureBody::Expr(e) => ClosureBodyForm::Expr((**e).clone()),
                ClosureBody::Block(b) => ClosureBodyForm::Block(b.clone()),
            };
            Ok((fn_params, body_form, None))
        }
        ExprKind::ClosureFull(sb) => {
            let body_form = match &sb.body {
                FnBody::Expr(e) => ClosureBodyForm::Expr(e.clone()),
                FnBody::Block(b) => ClosureBodyForm::Block(b.clone()),
                FnBody::External => {
                    return Err(Diagnostic::new(
                        "[E_CONST_FN_CLOSURE_EXTERNAL] external closure body not allowed (V4.3)."
                            .to_string(),
                        span,
                    ));
                }
            };
            Ok((sb.params.clone(), body_form, sb.return_type.clone()))
        }
        _ => Err(Diagnostic::new(
            "[E_CONST_FN_CLOSURE_BODY] expected closure literal body (V4.3)."
                .to_string(),
            span,
        )),
    }
}

fn lambda_params_to_fn_params(
    params: &[LambdaParam],
    expected_types: &[TypeRef],
    span: crate::diag::Span,
) -> Result<Vec<Param>, Diagnostic> {
    if !expected_types.is_empty() && expected_types.len() != params.len() {
        return Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_CLOSURE_ARITY] closure param count {} не совпадает \
                 с host fn's return type `fn(..)` arity {} (V4.3).",
                params.len(), expected_types.len()
            ),
            span,
        ));
    }
    let mut out = Vec::with_capacity(params.len());
    for (i, lp) in params.iter().enumerate() {
        let ty = lp.ty.clone().or_else(|| expected_types.get(i).cloned());
        let Some(ty) = ty else {
            return Err(Diagnostic::new(
                format!(
                    "[E_CONST_FN_CLOSURE_PARAM_TYPE] cannot infer type для closure param \
                     `{}` (V4.3). Annotate host fn's return type `-> const fn(T1, ..) -> R` \
                     или явно укажи type в lambda param.",
                    lp.name
                ),
                lp.span,
            ));
        };
        out.push(Param {
            name: lp.name.clone(),
            ty,
            span: lp.span,
            is_variadic: false,
            default: None,
            consume: false,
            is_mut: false,
            is_const: false,
        });
    }
    Ok(out)
}

fn closure_light_params_to_fn_params(
    params: &[crate::ast::ClosureLightParam],
    expected_types: &[TypeRef],
    span: crate::diag::Span,
) -> Result<Vec<Param>, Diagnostic> {
    if expected_types.is_empty() {
        return Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_CLOSURE_NO_RET_TYPE] untyped closure (`|x| body`) requires \
                 host fn's return type `-> const fn(T1, ..) -> R` для type inference \
                 (V4.3 limitation)."
            ),
            span,
        ));
    }
    if expected_types.len() != params.len() {
        return Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_CLOSURE_ARITY] closure param count {} не совпадает \
                 с host fn's return type `fn(..)` arity {} (V4.3).",
                params.len(), expected_types.len()
            ),
            span,
        ));
    }
    let mut out = Vec::with_capacity(params.len());
    for (i, lp) in params.iter().enumerate() {
        out.push(Param {
            name: lp.name.clone(),
            ty: expected_types[i].clone(),
            span: lp.span,
            is_variadic: false,
            default: None,
            consume: false,
            is_mut: false,
            is_const: false,
        });
    }
    Ok(out)
}

fn extract_func_signature(
    ret: &Option<TypeRef>,
    span: crate::diag::Span,
) -> Result<(Vec<TypeRef>, Option<TypeRef>), Diagnostic> {
    let Some(tr) = ret else {
        return Ok((Vec::new(), None));
    };
    let stripped = tr.strip_readonly();
    if let TypeRef::Func { params, return_type, .. } = stripped {
        Ok((params.clone(), return_type.as_ref().map(|b| (**b).clone())))
    } else {
        Err(Diagnostic::new(
            format!(
                "[E_CONST_FN_CLOSURE_NO_RET_TYPE] closure-returning const fn должен \
                 declare return type `-> const fn(T1, ..) -> R` (V4.3). \
                 Текущая declaration не fn-type."
            ),
            span,
        ))
    }
}

// =========================================================================
// Substitution (closure body — replace Ident(const_param) с literal).
// Borrowed pattern from const_fn_mono.rs.
// =========================================================================

fn subst_block(b: &mut Block, subst: &HashMap<String, ConstValue>) {
    for s in &mut b.stmts {
        subst_stmt(s, subst);
    }
    if let Some(t) = &mut b.trailing {
        subst_expr(t, subst);
    }
}

fn subst_stmt(s: &mut Stmt, subst: &HashMap<String, ConstValue>) {
    match s {
        Stmt::Let(d) => subst_expr(&mut d.value, subst),
        Stmt::Const(d) => subst_expr(&mut d.value, subst),
        Stmt::Expr(e) => subst_expr(e, subst),
        Stmt::Assign { target, value, .. } => {
            subst_expr(target, subst);
            subst_expr(value, subst);
        }
        Stmt::Return { value: Some(v), .. } => subst_expr(v, subst),
        Stmt::Throw { value, .. } => subst_expr(value, subst),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            subst_expr(body, subst);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            subst_expr(init, subst);
            subst_block(body, subst);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            subst_expr(expr, subst);
        }
        _ => {}
    }
}

fn subst_expr(e: &mut Expr, subst: &HashMap<String, ConstValue>) {
    if let ExprKind::Ident(name) = &e.kind {
        if let Some(v) = subst.get(name) {
            let span = e.span;
            *e = v.to_literal_expr(span);
            return;
        }
    }
    match &mut e.kind {
        ExprKind::Unary { operand, .. } => subst_expr(operand, subst),
        ExprKind::Binary { left, right, .. } => {
            subst_expr(left, subst);
            subst_expr(right, subst);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => subst_expr(x, subst),
        ExprKind::Try(x) | ExprKind::Bang(x) => subst_expr(x, subst),
        ExprKind::Coalesce(a, b) => { subst_expr(a, subst); subst_expr(b, subst); }
        ExprKind::Member { obj, .. } => subst_expr(obj, subst),
        ExprKind::Index { obj, index } => {
            subst_expr(obj, subst);
            subst_expr(index, subst);
        }
        ExprKind::TurboFish { base, .. } => subst_expr(base, subst),
        ExprKind::Call { func, args, trailing } => {
            subst_expr(func, subst);
            for a in args {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => subst_expr(x, subst),
                    CallArg::Named { value, .. } => subst_expr(value, subst),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => subst_block(b, subst),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(e) => subst_expr(e, subst),
                        FnBody::Block(b) => subst_block(b, subst),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => subst_block(&mut tb.body, subst),
                }
            }
        }
        ExprKind::TupleLit(items) => for x in items { subst_expr(x, subst); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => subst_expr(x, subst), }
        },
        ExprKind::MapLit { elems, .. } => for el in elems {
            match el {
                MapElem::Pair(k, v) => { subst_expr(k, subst); subst_expr(v, subst); }
                MapElem::Spread(s) => subst_expr(s, subst),
            }
        },
        ExprKind::RecordLit { fields, .. } => for f in fields {
            if let Some(v) = &mut f.value { subst_expr(v, subst); }
        },
        ExprKind::InterpolatedStr { parts } => for p in parts {
            if let crate::ast::InterpStrPart::Expr(ex) = p { subst_expr(ex, subst); }
        },
        ExprKind::If { cond, then, else_ } => {
            subst_expr(cond, subst);
            subst_block(then, subst);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => subst_block(b, subst),
                    ElseBranch::If(ie) => subst_expr(ie, subst),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            subst_expr(scrutinee, subst);
            for arm in arms {
                if let Some(g) = &mut arm.guard { subst_expr(g, subst); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => subst_expr(e, subst),
                    MatchArmBody::Block(b) => subst_block(b, subst),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            subst_expr(iter, subst);
            subst_block(body, subst);
        }
        ExprKind::While { cond, body, .. } => {
            subst_expr(cond, subst);
            subst_block(body, subst);
        }
        ExprKind::Loop { body, .. } => subst_block(body, subst),
        ExprKind::Block(b) => subst_block(b, subst),
        _ => {}
    }
}

// =========================================================================
// Step 2 — Rewriter (find Calls to closure-returning fn, replace с Ident
// to specialized fn).
// =========================================================================

fn rewrite_item(
    item: &mut Item,
    closure_fns: &HashMap<String, FnDecl>,
    specs: &mut HashMap<(String, Vec<ConstValue>), String>,
    spec_counter: &mut usize,
    errors: &mut Vec<Diagnostic>,
) {
    match item {
        Item::Fn(fd) => {
            // Skip rewriting inside closure-returning fn bodies — those
            // remain template (но dropped в retain).
            if closure_fns.contains_key(&fd.name) { return; }
            for p in &mut fd.params {
                if let Some(def) = &mut p.default {
                    rewrite_expr(def, closure_fns, specs, spec_counter, errors);
                }
            }
            match &mut fd.body {
                FnBody::Expr(e) => rewrite_expr(e, closure_fns, specs, spec_counter, errors),
                FnBody::Block(b) => rewrite_block(b, closure_fns, specs, spec_counter, errors),
                FnBody::External => {}
            }
        }
        Item::Const(c) => rewrite_expr(&mut c.value, closure_fns, specs, spec_counter, errors),
        Item::Let(l) => rewrite_expr(&mut l.value, closure_fns, specs, spec_counter, errors),
        Item::Test(t) => rewrite_block(&mut t.body, closure_fns, specs, spec_counter, errors),
        Item::Bench(b) => {
            for s in &mut b.setup { rewrite_stmt(s, closure_fns, specs, spec_counter, errors); }
            rewrite_block(&mut b.measure_body, closure_fns, specs, spec_counter, errors);
            for s in &mut b.teardown { rewrite_stmt(s, closure_fns, specs, spec_counter, errors); }
        }
        Item::Type(td) => {
            for ac in &mut td.assoc_consts {
                rewrite_expr(&mut ac.value, closure_fns, specs, spec_counter, errors);
            }
        }
        Item::Lemma(_) => {}
    }
}

fn rewrite_block(
    b: &mut Block,
    closure_fns: &HashMap<String, FnDecl>,
    specs: &mut HashMap<(String, Vec<ConstValue>), String>,
    spec_counter: &mut usize,
    errors: &mut Vec<Diagnostic>,
) {
    for s in &mut b.stmts {
        rewrite_stmt(s, closure_fns, specs, spec_counter, errors);
    }
    if let Some(t) = &mut b.trailing {
        rewrite_expr(t, closure_fns, specs, spec_counter, errors);
    }
}

fn rewrite_stmt(
    s: &mut Stmt,
    closure_fns: &HashMap<String, FnDecl>,
    specs: &mut HashMap<(String, Vec<ConstValue>), String>,
    spec_counter: &mut usize,
    errors: &mut Vec<Diagnostic>,
) {
    match s {
        Stmt::Let(d) => rewrite_expr(&mut d.value, closure_fns, specs, spec_counter, errors),
        Stmt::Const(d) => rewrite_expr(&mut d.value, closure_fns, specs, spec_counter, errors),
        Stmt::Expr(e) => rewrite_expr(e, closure_fns, specs, spec_counter, errors),
        Stmt::Assign { target, value, .. } => {
            rewrite_expr(target, closure_fns, specs, spec_counter, errors);
            rewrite_expr(value, closure_fns, specs, spec_counter, errors);
        }
        Stmt::Return { value: Some(v), .. } => rewrite_expr(v, closure_fns, specs, spec_counter, errors),
        Stmt::Throw { value, .. } => rewrite_expr(value, closure_fns, specs, spec_counter, errors),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            rewrite_expr(body, closure_fns, specs, spec_counter, errors);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            rewrite_expr(init, closure_fns, specs, spec_counter, errors);
            rewrite_block(body, closure_fns, specs, spec_counter, errors);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            rewrite_expr(expr, closure_fns, specs, spec_counter, errors);
        }
        _ => {}
    }
}

fn rewrite_expr(
    e: &mut Expr,
    closure_fns: &HashMap<String, FnDecl>,
    specs: &mut HashMap<(String, Vec<ConstValue>), String>,
    spec_counter: &mut usize,
    errors: &mut Vec<Diagnostic>,
) {
    // Recurse first.
    match &mut e.kind {
        ExprKind::Unary { operand, .. } => rewrite_expr(operand, closure_fns, specs, spec_counter, errors),
        ExprKind::Binary { left, right, .. } => {
            rewrite_expr(left, closure_fns, specs, spec_counter, errors);
            rewrite_expr(right, closure_fns, specs, spec_counter, errors);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => rewrite_expr(x, closure_fns, specs, spec_counter, errors),
        ExprKind::Member { obj, .. } => rewrite_expr(obj, closure_fns, specs, spec_counter, errors),
        ExprKind::Index { obj, index } => {
            rewrite_expr(obj, closure_fns, specs, spec_counter, errors);
            rewrite_expr(index, closure_fns, specs, spec_counter, errors);
        }
        ExprKind::TurboFish { base, .. } => rewrite_expr(base, closure_fns, specs, spec_counter, errors),
        ExprKind::Try(x) | ExprKind::Bang(x) => rewrite_expr(x, closure_fns, specs, spec_counter, errors),
        ExprKind::Coalesce(a, b) => {
            rewrite_expr(a, closure_fns, specs, spec_counter, errors);
            rewrite_expr(b, closure_fns, specs, spec_counter, errors);
        }
        ExprKind::TupleLit(items) => for x in items { rewrite_expr(x, closure_fns, specs, spec_counter, errors); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => rewrite_expr(x, closure_fns, specs, spec_counter, errors), }
        },
        ExprKind::RecordLit { fields, .. } => for f in fields {
            if let Some(v) = &mut f.value { rewrite_expr(v, closure_fns, specs, spec_counter, errors); }
        },
        ExprKind::If { cond, then, else_ } => {
            rewrite_expr(cond, closure_fns, specs, spec_counter, errors);
            rewrite_block(then, closure_fns, specs, spec_counter, errors);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_block(b, closure_fns, specs, spec_counter, errors),
                    ElseBranch::If(ie) => rewrite_expr(ie, closure_fns, specs, spec_counter, errors),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_expr(scrutinee, closure_fns, specs, spec_counter, errors);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_expr(g, closure_fns, specs, spec_counter, errors); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => rewrite_expr(e, closure_fns, specs, spec_counter, errors),
                    MatchArmBody::Block(b) => rewrite_block(b, closure_fns, specs, spec_counter, errors),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            rewrite_expr(iter, closure_fns, specs, spec_counter, errors);
            rewrite_block(body, closure_fns, specs, spec_counter, errors);
        }
        ExprKind::While { cond, body, .. } => {
            rewrite_expr(cond, closure_fns, specs, spec_counter, errors);
            rewrite_block(body, closure_fns, specs, spec_counter, errors);
        }
        ExprKind::Loop { body, .. } => rewrite_block(body, closure_fns, specs, spec_counter, errors),
        ExprKind::Block(b) => rewrite_block(b, closure_fns, specs, spec_counter, errors),
        ExprKind::Call { func, args, trailing } => {
            rewrite_expr(func, closure_fns, specs, spec_counter, errors);
            for a in args {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => rewrite_expr(x, closure_fns, specs, spec_counter, errors),
                    CallArg::Named { value, .. } => rewrite_expr(value, closure_fns, specs, spec_counter, errors),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => rewrite_block(b, closure_fns, specs, spec_counter, errors),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(e) => rewrite_expr(e, closure_fns, specs, spec_counter, errors),
                        FnBody::Block(b) => rewrite_block(b, closure_fns, specs, spec_counter, errors),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => rewrite_block(&mut tb.body, closure_fns, specs, spec_counter, errors),
                }
            }
        }
        _ => {}
    }
    // Now check Call against closure_fns.
    let mut do_rewrite: Option<(String, Vec<ConstValue>)> = None;
    if let ExprKind::Call { func, args, trailing: None } = &e.kind {
        if let ExprKind::Ident(name) = &func.kind {
            if closure_fns.contains_key(name) {
                let mut arg_vals = Vec::with_capacity(args.len());
                let mut ok = true;
                for a in args {
                    match a {
                        CallArg::Item(ae) => {
                            if let Some(v) = try_literal_to_value(ae) {
                                arg_vals.push(v);
                            } else {
                                ok = false;
                                break;
                            }
                        }
                        _ => { ok = false; break; }
                    }
                }
                if !ok {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_CONST_FN_CLOSURE_NON_CONST_ARG] call to closure-returning \
                             const fn `{}` has non-constexpr argument — all args must be \
                             literals (D199 V4.3).",
                            name
                        ),
                        e.span,
                    ));
                } else {
                    do_rewrite = Some((name.clone(), arg_vals));
                }
            }
        }
    }
    if let Some((host_name, const_args)) = do_rewrite {
        let key = (host_name.clone(), const_args);
        let counter_val = &mut *spec_counter;
        let spec_name = specs.entry(key).or_insert_with(|| {
            let n = format!("{}__closure_{}", host_name, counter_val);
            *counter_val += 1;
            n
        }).clone();
        let span = e.span;
        *e = Expr { kind: ExprKind::Ident(spec_name), span };
    }
}

// =========================================================================
// Step 4 — Friendly error для leftover bare-Ident refs к closure-returning
// fns (first-class use не supported в V1).
// =========================================================================

struct FirstClassRejectCtx<'a, 'b> {
    closure_fns: &'a std::collections::HashSet<String>,
    errors: &'b mut Vec<Diagnostic>,
}

impl<'a, 'b> FirstClassRejectCtx<'a, 'b> {
    fn check(&mut self, e: &Expr) {
        if let ExprKind::Ident(name) = &e.kind {
            if self.closure_fns.contains(name) {
                self.errors.push(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_CLOSURE_FIRST_CLASS] closure-returning const fn `{}` \
                         cannot be used as first-class value (V4.3). Каждый вызов \
                         producewает разный specialized closure — нет single trampoline. \
                         Use direct call: `make_adder(LITERAL)` returns specialized fn ref.",
                        name
                    ),
                    e.span,
                ));
            }
        }
    }
    fn visit_item(&mut self, item: &Item) {
        match item {
            Item::Fn(fd) => {
                let all_const = !fd.params.is_empty() && fd.params.iter().all(|p| p.is_const);
                let is_fully = (all_const || fd.params.is_empty()) && fd.return_is_const;
                if is_fully { return; }
                match &fd.body {
                    FnBody::Expr(e) => self.visit_expr(e),
                    FnBody::Block(b) => self.visit_block(b),
                    FnBody::External => {}
                }
            }
            Item::Const(c) => self.visit_expr(&c.value),
            Item::Let(l) => { self.check(&l.value); self.visit_expr(&l.value); }
            Item::Test(t) => self.visit_block(&t.body),
            Item::Bench(b) => {
                for s in &b.setup { self.visit_stmt(s); }
                self.visit_block(&b.measure_body);
                for s in &b.teardown { self.visit_stmt(s); }
            }
            Item::Type(_) | Item::Lemma(_) => {}
        }
    }
    fn visit_block(&mut self, b: &Block) {
        for s in &b.stmts { self.visit_stmt(s); }
        if let Some(t) = &b.trailing { self.visit_expr(t); }
    }
    fn visit_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let(d) => { self.check(&d.value); self.visit_expr(&d.value); }
            Stmt::Const(_) => {}
            Stmt::Expr(e) => self.visit_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.visit_expr(target);
                self.check(value);
                self.visit_expr(value);
            }
            Stmt::Return { value: Some(v), .. } => { self.check(v); self.visit_expr(v); }
            Stmt::Return { value: None, .. } => {}
            Stmt::Throw { value, .. } => self.visit_expr(value),
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.visit_expr(body);
            }
            Stmt::ConsumeScope { init, body, .. } => {
                self.visit_expr(init);
                self.visit_block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.visit_expr(expr);
            }
            _ => {}
        }
    }
    fn visit_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Call { func, args, trailing } => {
                if !matches!(&func.kind, ExprKind::Ident(_)) {
                    self.visit_expr(func);
                }
                for a in args {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => {
                            self.check(x);
                            self.visit_expr(x);
                        }
                        CallArg::Named { value, .. } => {
                            self.check(value);
                            self.visit_expr(value);
                        }
                    }
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => self.visit_block(b),
                        Trailing::Fn(sb) => match &sb.body {
                            FnBody::Expr(e) => self.visit_expr(e),
                            FnBody::Block(b) => self.visit_block(b),
                            FnBody::External => {}
                        },
                        Trailing::LegacyBlockWithParams(tb) => self.visit_block(&tb.body),
                    }
                }
            }
            ExprKind::Unary { operand, .. } => self.visit_expr(operand),
            ExprKind::Binary { left, right, .. } => {
                self.visit_expr(left); self.visit_expr(right);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.visit_expr(x),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.visit_expr(x),
            ExprKind::Coalesce(a, b) => { self.visit_expr(a); self.visit_expr(b); }
            ExprKind::Member { obj, .. } => self.visit_expr(obj),
            ExprKind::Index { obj, index } => { self.visit_expr(obj); self.visit_expr(index); }
            ExprKind::TurboFish { base, .. } => self.visit_expr(base),
            ExprKind::TupleLit(items) => for x in items { self.visit_expr(x); },
            ExprKind::ArrayLit(elems) => for el in elems {
                match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => self.visit_expr(x), }
            },
            ExprKind::RecordLit { fields, .. } => for f in fields {
                if let Some(v) = &f.value { self.visit_expr(v); }
            },
            ExprKind::If { cond, then, else_ } => {
                self.visit_expr(cond);
                self.visit_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.visit_block(b),
                        ElseBranch::If(ie) => self.visit_expr(ie),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.visit_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.visit_expr(g); }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.visit_expr(e),
                        MatchArmBody::Block(b) => self.visit_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, .. } => { self.visit_expr(iter); self.visit_block(body); }
            ExprKind::While { cond, body, .. } => { self.visit_expr(cond); self.visit_block(body); }
            ExprKind::Loop { body, .. } => self.visit_block(body),
            ExprKind::Block(b) => self.visit_block(b),
            _ => {}
        }
    }
}
