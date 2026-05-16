//! Plan 33.6 Ф.2.1: Handler verification — выделено из pipeline.rs.
//!
//! Содержит:
//! - `verify_handlers` — entry-point для `with #verify E = handler` bindings.
//! - Symbolic exec V2: `rewrite_post_in_expr`, `verify_post_axiom_with_handler`.
//! - Liskov verification: `collect_effect_method_contracts`, `verify_liskov_method`.
//! - AST substitution helpers: `subst_ident_in_expr`, `subst_call_arg`, `extract_block_assignments`.
//! - Collect helpers: `collect_verify_bindings_block`, `collect_verify_bindings_expr`.

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use super::ir::*;
use super::encode;
use super::backend::try_prove;
use super::pipeline::{
    VerificationPipeline, VerifyResult, AxiomInfo,
    collect_pure_views, collect_pure_fns, collect_trusted_fns, encode_axiom, infer_binder_sorts,
    format_counterexample, unknown_to_diag_message, substitute_old, type_to_sort,
    infer_pure_fns_scc,
};

// ─────────────────────────────────────────────────────────────────────────────
// Entry-point
// ─────────────────────────────────────────────────────────────────────────────

/// Plan 33.4 P0-1 (Ф.9.7 V1): верификация `with #verify E = handler` bindings.
///
/// V1 scope:
/// - Только статические axiom'ы (без `post(Action)` — те требуют V2 symbolic exec).
/// - Для статических axioms: кодируем pure_view impl handler'а как UF с body-axiom,
///   затем пытаемся доказать axiom формулу в этом контексте.
/// - Для `post(...)` axioms: честно выдаём Unknown("post-axiom verification requires V2").
///
/// Ф.7.5 (Plan 33.6): возвращает (errors, warnings).
/// errors — Disproved axioms (compile-blocking). warnings — Unknown/EncodingFailed (W2402).
pub fn verify_handlers(module: &Module) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut warnings: Vec<Diagnostic> = Vec::new();
    let pure_views = collect_pure_views(module);

    let mut axioms_by_effect: std::collections::HashMap<String, &[crate::ast::EffectAxiom]>
        = std::collections::HashMap::new();
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        if !matches!(td.kind, TypeDeclKind::Effect(_)) { continue }
        if !td.axioms.is_empty() {
            axioms_by_effect.insert(td.name.clone(), &td.axioms);
        }
    }

    let mut verify_bindings: Vec<(String, Span, &crate::ast::Expr)> = Vec::new();
    for item in &module.items {
        match item {
            Item::Fn(fd) => match &fd.body {
                FnBody::Block(b) => collect_verify_bindings_block(b, &mut verify_bindings),
                FnBody::Expr(e) => collect_verify_bindings_expr(e, &mut verify_bindings),
                FnBody::External => {}
            }
            Item::Test(t) => collect_verify_bindings_block(&t.body, &mut verify_bindings),
            _ => {}
        }
    }

    let pipeline = VerificationPipeline::new();
    let inferred_pure = infer_pure_fns_scc(module);

    for (effect_name, binding_span, handler_expr) in verify_bindings {

        let methods = match &handler_expr.kind {
            ExprKind::HandlerLit { methods, .. } => methods,
            _ => continue,
        };

        if let Some(axioms) = axioms_by_effect.get(&effect_name) {
        for ax in axioms.iter() {
            let ax_info = AxiomInfo {
                effect_name: effect_name.clone(),
                axiom_name: ax.name.clone(),
                binders: &ax.binders,
                formula: &ax.formula,
                is_generic: !ax.generics.is_empty(),
            };

            if axiom_formula_has_post(&ax.formula) {
                let result = verify_post_axiom_with_handler(
                    &pipeline, &ax_info, &pure_views, methods, &effect_name, module, &inferred_pure,
                );
                match result {
                    VerifyResult::Proven => {}
                    VerifyResult::Disproved(_, cex) => {
                        diagnostics.push(Diagnostic::new(
                            format!(
                                concat!(
                                    "#verify handler for effect {}: ",
                                    "post-axiom {} is NOT satisfied.\n  ",
                                    "counterexample: {}\n  ",
                                    "suggestion: fix handler action or view."
                                ),
                                effect_name, ax.name, cex,
                            ),
                            binding_span,
                        ));
                    }
                    VerifyResult::Unknown(reason) => {
                        // Ф.1.3 (Plan 33.6): #verify handler + Unknown = compile error E2402.
                        diagnostics.push(Diagnostic::new(
                            format!(
                                "#verify handler для эффекта {}: post-axiom '{}' не удалось верифицировать [E2402].\
                                 \n  причина: {}\
                                 \n  V1 symbolic exec поддерживает только линейные handler bodies (без if/match/loop/FFI).\
                                 \n  используйте #trusted чтобы пропустить верификацию.",
                                effect_name, ax.name, reason,
                            ),
                            binding_span,
                        ));
                    }
                    VerifyResult::EncodingFailed(reason) => {
                        // Ф.1.3: encoding failed тоже = error для #verify handler.
                        diagnostics.push(Diagnostic::new(
                            format!(
                                "#verify handler для эффекта {}: post-axiom '{}' не encodable [E2402]: {}",
                                effect_name, ax.name, reason,
                            ),
                            binding_span,
                        ));
                    }
                    VerifyResult::Warning(msg) => {
                        // W2402 от verify — forwarding как warning.
                        diagnostics.push(Diagnostic::new(msg, binding_span));
                    }
                }
                continue;
            }

            let result = verify_static_axiom_with_handler(
                &pipeline, &ax_info, &pure_views, methods, &effect_name, module, &inferred_pure,
            );

            match result {
                VerifyResult::Proven => {}
                VerifyResult::Disproved(_, cex) => {
                    diagnostics.push(Diagnostic::new(
                        format!(
                            "`#verify` handler for effect `{}`: axiom `{}` is NOT satisfied.\n  \
                             counterexample: {}\n  \
                             suggestion: fix handler's pure_view implementation or weaken axiom.",
                            effect_name, ax.name, cex,
                        ),
                        binding_span,
                    ));
                }
                VerifyResult::Unknown(reason) => {
                    // Ф.7.5 (Plan 33.6): static-axiom check Unknown — emit W2402
                    // (раньше silent discard через `let _`).
                    let msg = format!(
                        "`#verify` handler for effect `{}`: static axiom `{}` check returned Unknown [W2402]: {}\n  \
                         Используйте Z3 backend для полной проверки (NOVA_SMT_BACKEND=z3).",
                        effect_name, ax.name, reason);
                    warnings.push(Diagnostic::new(msg, binding_span));
                }
                VerifyResult::EncodingFailed(reason) => {
                    // Ф.7.5: тоже не silent — это либо bug в нашем encoder'е, либо
                    // unsupported конструкция в axiom — пользователь должен знать.
                    let msg = format!(
                        "`#verify` handler for effect `{}`: axiom `{}` failed to encode [W2402]: {}",
                        effect_name, ax.name, reason);
                    warnings.push(Diagnostic::new(msg, binding_span));
                }
                VerifyResult::Warning(msg) => {
                    diagnostics.push(Diagnostic::new(msg, binding_span));
                }
            }
        }
        }

        // Ф.5.2: Liskov-верификация.
        let effect_methods_with_contracts = collect_effect_method_contracts(module, &effect_name);
        for (em_name, em_params, em_contracts) in &effect_methods_with_contracts {
            if em_contracts.is_empty() { continue; }
            let Some(handler_method) = methods.iter().find(|m| m.name == *em_name) else { continue };
            let (liskov_errs, liskov_warns) = verify_liskov_method(
                &pipeline, em_name, em_params, em_contracts,
                handler_method, &pure_views, module, &inferred_pure,
                binding_span,
            );
            diagnostics.extend(liskov_errs);
            warnings.extend(liskov_warns);
        }
    }

    (diagnostics, warnings)
}

// ─────────────────────────────────────────────────────────────────────────────
// Liskov verification
// ─────────────────────────────────────────────────────────────────────────────

fn collect_effect_method_contracts(
    module: &Module,
    effect_name: &str,
) -> Vec<(String, Vec<Param>, Vec<Contract>)> {
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        if td.name != effect_name { continue; }
        let TypeDeclKind::Effect(methods) = &td.kind else { continue };
        return methods.iter()
            .filter(|m| !m.contracts.is_empty())
            .map(|m| (m.name.clone(), m.params.clone(), m.contracts.clone()))
            .collect();
    }
    Vec::new()
}

/// Ф.5.2: Liskov-верификация одного handler-метода.
fn verify_liskov_method(
    pipeline: &VerificationPipeline,
    method_name: &str,
    effect_params: &[Param],
    effect_contracts: &[Contract],
    handler_method: &crate::ast::HandlerMethod,
    pure_views: &std::collections::HashMap<String, super::encode::PureViewSig>,
    module: &Module,
    inferred_pure: &std::collections::HashSet<String>,
    binding_span: Span,
) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut warnings: Vec<Diagnostic> = Vec::new();
    let pure_fns = collect_pure_fns(module, inferred_pure);
    let trusted_fns = collect_trusted_fns(module);
    let var_sorts: std::collections::HashMap<String, SortRef> = effect_params.iter()
        .map(|p| (p.name.clone(), type_to_sort(&p.ty))).collect();
    let ctx = super::encode::EncodeCtx { pure_views, pure_fns: &pure_fns, trusted_fns: &trusted_fns, var_sorts };
    let mut backend = pipeline.create_backend();

    for (op_name, sig) in pure_views {
        let uf = super::encode::pure_view_uf_name(&sig.effect_name, op_name);
        backend.declare_function(&uf, &sig.param_sorts, sig.return_sort.clone());
    }
    for (fn_name, info) in &pure_fns {
        let uf = super::encode::pure_fn_uf_name(fn_name);
        backend.declare_function(&uf, &info.param_sorts, info.return_sort.clone());
    }

    for p in effect_params {
        backend.declare_var(&p.name, type_to_sort(&p.ty));
    }

    let mut requires_failed = false;
    for c in effect_contracts {
        if !matches!(c.kind, ContractKind::Requires) { continue; }
        match encode::encode_expr_with_ctx(&c.expr, &ctx) {
            Ok(t) => backend.assert(Assertion {
                formula: t,
                label: Some(format!("liskov_requires@{}", c.span.start)),
            }),
            Err(e) => {
                // Ф.8.1 (Plan 33.6): silent skip → W2402.
                let reason = match e {
                    super::encode::EncodingError::Unsupported(s) => s,
                };
                let msg = format!(
                    "Liskov requires для handler method `{}` не закодирован в SMT [W2402]: {}.\n  \
                     Verification handler не проверена.",
                    method_name, reason);
                warnings.push(Diagnostic::new(msg, binding_span));
                requires_failed = true;
            }
        }
    }

    let body_val = match &handler_method.body {
        crate::ast::HandlerMethodBody::Expr(e) => encode::encode_expr_with_ctx(e, &ctx).ok(),
        crate::ast::HandlerMethodBody::Block(b) if b.stmts.is_empty() => {
            b.trailing.as_ref().and_then(|e| encode::encode_expr_with_ctx(e, &ctx).ok())
        }
        _ => None,
    };

    for c in effect_contracts {
        if !matches!(c.kind, ContractKind::Ensures) { continue; }
        if requires_failed { continue; }
        let encoded = match encode::encode_expr_with_ctx(&c.expr, &ctx) {
            Ok(t) => t,
            Err(e) => {
                // Ф.8.1 (Plan 33.6): silent skip → W2402.
                let reason = match e {
                    super::encode::EncodingError::Unsupported(s) => s,
                };
                let msg = format!(
                    "Liskov ensures для handler method `{}` не закодирован в SMT [W2402]: {}.",
                    method_name, reason);
                warnings.push(Diagnostic::new(msg, binding_span));
                continue;
            }
        };
        let goal = if let Some(bv) = &body_val {
            encoded.substitute("result", bv)
        } else {
            // Ф.8.1 (Plan 33.6): handler body не encodable → W2402.
            let msg = format!(
                "Liskov check для handler method `{}` пропущен [W2402]: \
                 handler body не encodable в SMT (вероятно, complex statements).",
                method_name);
            warnings.push(Diagnostic::new(msg, binding_span));
            continue;
        };
        let goal = substitute_old(&goal);
        // Ф.10.4 (Plan 33.6): сохраняем pretty-print ensures-выражения
        // для position-aware diagnostic.
        let clause_pretty = format!("{:?}", c.expr.kind).chars().take(80).collect::<String>();
        match try_prove(&mut *backend, goal) {
            SatResult::Unsat(_) => {}
            SatResult::Sat(model) => {
                let cex = format_counterexample(&model);
                diagnostics.push(Diagnostic::new(
                    format!(
                        "`#verify` handler method `{}` нарушает контракт эффекта @ position {}:\n  \
                         clause: {}\n  \
                         counterexample: {}\n  \
                         Liskov: handler.{} должен удовлетворять ensures эффекта при его requires.\n  \
                         hint: fix handler body или ослабьте именно этот ensures в effect-decl.",
                        method_name, c.span.start, clause_pretty, cex, method_name,
                    ),
                    binding_span,
                ));
            }
            SatResult::Unknown(reason) => {
                // Ф.8.1 (Plan 33.6): Z3 Unknown → W2402.
                let msg = format!(
                    "Liskov check для handler method `{}` вернул Unknown [W2402]: {}.\n  \
                     Используйте Z3 backend для полной проверки.",
                    method_name, unknown_to_diag_message(reason));
                warnings.push(Diagnostic::new(msg, binding_span));
            }
        }
    }

    (diagnostics, warnings)
}

// ─────────────────────────────────────────────────────────────────────────────
// Static axiom verification
// ─────────────────────────────────────────────────────────────────────────────

fn verify_static_axiom_with_handler(
    pipeline: &VerificationPipeline,
    ax: &AxiomInfo,
    pure_views: &std::collections::HashMap<String, super::encode::PureViewSig>,
    methods: &[crate::ast::HandlerMethod],
    effect_name: &str,
    module: &Module,
    inferred_pure: &std::collections::HashSet<String>,
) -> VerifyResult {
    let pure_fns = collect_pure_fns(module, inferred_pure);
    let trusted_fns = collect_trusted_fns(module);
    let ctx = super::encode::EncodeCtx { pure_views, pure_fns: &pure_fns, trusted_fns: &trusted_fns, var_sorts: std::collections::HashMap::new() };

    let mut backend = pipeline.create_backend();

    for (op_name, sig) in pure_views {
        let uf = super::encode::pure_view_uf_name(&sig.effect_name, op_name);
        backend.declare_function(&uf, &sig.param_sorts, sig.return_sort.clone());
    }

    for method in methods {
        let Some(sig) = pure_views.get(&method.name) else { continue };
        if sig.effect_name != effect_name { continue }

        let body_expr = match &method.body {
            crate::ast::HandlerMethodBody::Expr(e) => e,
            crate::ast::HandlerMethodBody::Block(_) => continue,
        };

        if let Ok(body_term) = encode::encode_expr_with_ctx(body_expr, &ctx) {
            let param_names: Vec<String> = method.params.iter()
                .map(|p| p.name.clone())
                .collect();
            let param_args: Vec<SmtTerm> = param_names.iter()
                .map(|n| SmtTerm::Var(n.clone()))
                .collect();

            let uf_name = super::encode::pure_view_uf_name(effect_name, &method.name);
            let uf_app = SmtTerm::App(uf_name, param_args);
            let eq_body = SmtTerm::eq(uf_app, body_term);

            let binders: Vec<(String, SortRef)> = param_names.iter()
                .zip(sig.param_sorts.iter())
                .map(|(n, s)| (n.clone(), s.clone()))
                .collect();
            let handler_axiom = if binders.is_empty() {
                eq_body
            } else {
                SmtTerm::Forall(binders, vec![], Box::new(eq_body))
            };

            backend.assert(Assertion {
                formula: handler_axiom,
                label: Some(format!("handler_body@{}.{}", effect_name, method.name)),
            });
        }
    }

    let Some(ax_formula) = encode_axiom(ax, pure_views) else {
        return VerifyResult::Unknown("axiom not encodable (generic or unsupported)".into());
    };

    match try_prove(&mut *backend, ax_formula) {
        SatResult::Unsat(_) => VerifyResult::Proven,
        SatResult::Sat(model) => {
            let cex = format_counterexample(&model);
            VerifyResult::Disproved(model, cex)
        }
        SatResult::Unknown(reason) => {
            VerifyResult::Unknown(unknown_to_diag_message(reason))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ф.6: post(Action(args))(view(args)) symbolic exec V2
// ─────────────────────────────────────────────────────────────────────────────

fn subst_ident_in_expr(
    expr: &crate::ast::Expr,
    name: &str,
    replacement: &crate::ast::Expr,
) -> crate::ast::Expr {
    use crate::ast::ExprKind::*;
    let new_kind = match &expr.kind {
        Ident(n) if n == name => return replacement.clone(),
        Binary { op, left, right } => Binary {
            op: op.clone(),
            left: Box::new(subst_ident_in_expr(left, name, replacement)),
            right: Box::new(subst_ident_in_expr(right, name, replacement)),
        },
        Unary { op, operand } => Unary {
            op: op.clone(),
            operand: Box::new(subst_ident_in_expr(operand, name, replacement)),
        },
        Call { func, args, trailing } => Call {
            func: Box::new(subst_ident_in_expr(func, name, replacement)),
            args: args.iter().map(|a| subst_call_arg(a, name, replacement)).collect(),
            trailing: trailing.clone(),
        },
        Member { obj, name: field } => Member {
            obj: Box::new(subst_ident_in_expr(obj, name, replacement)),
            name: field.clone(),
        },
        Index { obj, index } => Index {
            obj: Box::new(subst_ident_in_expr(obj, name, replacement)),
            index: Box::new(subst_ident_in_expr(index, name, replacement)),
        },
        _ => return expr.clone(),
    };
    crate::ast::Expr { kind: new_kind, span: expr.span }
}

fn subst_call_arg(
    arg: &crate::ast::CallArg,
    name: &str,
    replacement: &crate::ast::Expr,
) -> crate::ast::CallArg {
    match arg {
        crate::ast::CallArg::Item(e) =>
            crate::ast::CallArg::Item(subst_ident_in_expr(e, name, replacement)),
        crate::ast::CallArg::Spread(e) =>
            crate::ast::CallArg::Spread(subst_ident_in_expr(e, name, replacement)),
        crate::ast::CallArg::Named { name: n, value } =>
            crate::ast::CallArg::Named {
                name: n.clone(),
                value: subst_ident_in_expr(value, name, replacement),
            },
    }
}

fn extract_block_assignments(block: &crate::ast::Block) -> Vec<(String, crate::ast::Expr)> {
    let mut result = Vec::new();
    for stmt in &block.stmts {
        match stmt {
            crate::ast::Stmt::Assign { target, value, .. } => {
                if let crate::ast::ExprKind::Ident(var) = &target.kind {
                    result.push((var.clone(), value.clone()));
                }
            }
            // Ф.11.5 (Plan 33.6): if-branching MVP — если оба branches содержат
            // одинаковые assignments к одной var с if-expression в value,
            // эмитим объединённую assignment через ite.
            crate::ast::Stmt::Expr(e) => {
                if let crate::ast::ExprKind::If { cond, then, else_ } = &e.kind {
                    let then_asgns = extract_block_assignments(then);
                    let else_asgns = if let Some(crate::ast::ElseBranch::Block(b)) = else_ {
                        extract_block_assignments(b)
                    } else {
                        Vec::new()
                    };
                    // Для каждого var присвоенного в обоих ветках — emit ite(cond, then_val, else_val).
                    for (var, then_val) in &then_asgns {
                        if let Some((_, else_val)) = else_asgns.iter().find(|(v, _)| v == var) {
                            // Build synthetic if-expr: `if cond { then_val } else { else_val }`.
                            let ite_expr = crate::ast::Expr {
                                kind: crate::ast::ExprKind::If {
                                    cond: cond.clone(),
                                    then: crate::ast::Block {
                                        stmts: Vec::new(),
                                        trailing: Some(Box::new(then_val.clone())),
                                        span: then_val.span,
                                    },
                                    else_: Some(crate::ast::ElseBranch::Block(crate::ast::Block {
                                        stmts: Vec::new(),
                                        trailing: Some(Box::new(else_val.clone())),
                                        span: else_val.span,
                                    })),
                                },
                                span: e.span,
                            };
                            result.push((var.clone(), ite_expr));
                        }
                    }
                }
                // Ф.13.2 (Plan 33.6): match с constant patterns — конвертируем в nested ite.
                if let crate::ast::ExprKind::Match { scrutinee, arms } = &e.kind {
                    extract_match_assignments(scrutinee, arms, &mut result, e.span);
                }
            }
            _ => {}
        }
    }
    result
}

/// Ф.13.2 (Plan 33.6): match handler-branching с constant patterns.
/// Конвертирует `match scrutinee { lit1 => block1; lit2 => block2; _ => block_def }`
/// → nested ite по pattern == lit.
fn extract_match_assignments(
    scrutinee: &crate::ast::Expr,
    arms: &[crate::ast::MatchArm],
    result: &mut Vec<(String, crate::ast::Expr)>,
    match_span: crate::diag::Span,
) {
    // Собрать (cond, assignments) per arm.
    let mut arm_data: Vec<(Option<crate::ast::Expr>, Vec<(String, crate::ast::Expr)>)> = Vec::new();
    for arm in arms {
        let asgns = match &arm.body {
            crate::ast::MatchArmBody::Block(b) => extract_block_assignments(b),
            crate::ast::MatchArmBody::Expr(_) => continue, // expr arm: no assignment
        };
        if asgns.is_empty() { continue; }
        let cond = match &arm.pattern {
            crate::ast::Pattern::Literal(lit, _) => {
                let lit_expr_kind = match lit {
                    crate::ast::Literal::Int(n) => crate::ast::ExprKind::IntLit(*n),
                    crate::ast::Literal::Bool(b) => crate::ast::ExprKind::BoolLit(*b),
                    _ => continue, // skip Str/Float/Char patterns (not encodable easily)
                };
                let lit_expr = crate::ast::Expr {
                    kind: lit_expr_kind,
                    span: arm.span,
                };
                Some(crate::ast::Expr {
                    kind: crate::ast::ExprKind::Binary {
                        op: crate::ast::BinOp::Eq,
                        left: Box::new(scrutinee.clone()),
                        right: Box::new(lit_expr),
                    },
                    span: arm.span,
                })
            }
            crate::ast::Pattern::Wildcard(_) => None, // catch-all else
            _ => continue, // unsupported pattern (Variant/Record/Tuple/...)
        };
        arm_data.push((cond, asgns));
    }

    // Найти все vars присваиваемые хотя бы в одной arm.
    let mut vars: Vec<String> = Vec::new();
    for (_, asgns) in &arm_data {
        for (v, _) in asgns {
            if !vars.contains(v) { vars.push(v.clone()); }
        }
    }

    // Для каждой var построить nested ite.
    for var in &vars {
        // Find arms where var is assigned + collect (cond, val) pairs.
        let mut branches: Vec<(crate::ast::Expr, crate::ast::Expr)> = Vec::new(); // cond → val
        let mut default_val: Option<crate::ast::Expr> = None;
        for (cond, asgns) in &arm_data {
            if let Some((_, val)) = asgns.iter().find(|(v, _)| v == var) {
                match cond {
                    Some(c) => branches.push((c.clone(), val.clone())),
                    None => default_val = Some(val.clone()),
                }
            }
        }
        // Если есть branches и default, строим nested ite.
        let Some(default) = default_val else { continue; }; // V1: требуется wildcard
        let mut acc = default;
        for (cond, val) in branches.into_iter().rev() {
            acc = crate::ast::Expr {
                kind: crate::ast::ExprKind::If {
                    cond: Box::new(cond.clone()),
                    then: crate::ast::Block {
                        stmts: Vec::new(),
                        trailing: Some(Box::new(val.clone())),
                        span: val.span,
                    },
                    else_: Some(crate::ast::ElseBranch::Block(crate::ast::Block {
                        stmts: Vec::new(),
                        trailing: Some(Box::new(acc.clone())),
                        span: acc.span,
                    })),
                },
                span: match_span,
            };
        }
        result.push((var.clone(), acc));
    }
}

fn parse_post_call(
    expr: &crate::ast::Expr,
) -> Option<(String, Vec<crate::ast::Expr>, String, Vec<crate::ast::Expr>)> {
    use crate::ast::ExprKind::*;
    let Call { func: outer_func, args: view_args, .. } = &expr.kind else { return None };
    let Call { func: post_ident, args: action_wrap_args, .. } = &outer_func.kind else { return None };
    let Ident(post_name) = &post_ident.kind else { return None };
    if post_name != "post" { return None; }
    if action_wrap_args.len() != 1 { return None; }
    let action_call_expr = action_wrap_args[0].expr();
    let Call { func: action_func, args: action_args, .. } = &action_call_expr.kind else { return None };
    let Ident(action_name) = &action_func.kind else { return None };
    if view_args.len() != 1 { return None; }
    let view_call_expr = view_args[0].expr();
    let Call { func: view_func, args: view_args2, .. } = &view_call_expr.kind else { return None };
    let Ident(view_name) = &view_func.kind else { return None };

    Some((
        action_name.clone(),
        action_args.iter().map(|a| a.expr().clone()).collect(),
        view_name.clone(),
        view_args2.iter().map(|a| a.expr().clone()).collect(),
    ))
}

fn rewrite_post_in_expr(
    formula: &crate::ast::Expr,
    methods: &[crate::ast::HandlerMethod],
) -> Result<(crate::ast::Expr, bool), String> {
    use crate::ast::ExprKind::*;

    if let Some((action_name, action_args, view_name, view_args)) = parse_post_call(formula) {
        let action_method = methods.iter().find(|m| m.name == action_name)
            .ok_or_else(|| format!("handler method `{}` not found", action_name))?;
        let action_block = match &action_method.body {
            crate::ast::HandlerMethodBody::Block(b) => b,
            crate::ast::HandlerMethodBody::Expr(_) =>
                return Err(format!("action `{}` has Expr body, expected Block", action_name)),
        };

        let view_method = methods.iter().find(|m| m.name == view_name)
            .ok_or_else(|| format!("handler method `{}` not found", view_name))?;
        let view_expr = match &view_method.body {
            crate::ast::HandlerMethodBody::Expr(e) => e.clone(),
            crate::ast::HandlerMethodBody::Block(_) =>
                return Err(format!("view `{}` has Block body, expected Expr", view_name)),
        };

        let assignments = extract_block_assignments(action_block);

        let mut result_expr = view_expr;
        for (var, new_val) in &assignments {
            result_expr = subst_ident_in_expr(&result_expr, var, new_val);
        }

        let action_params: Vec<String> = action_method.params.iter()
            .map(|p| p.name.clone()).collect();
        for (param, arg_expr) in action_params.iter().zip(action_args.iter()) {
            result_expr = subst_ident_in_expr(&result_expr, param, arg_expr);
        }

        let view_params: Vec<String> = view_method.params.iter()
            .map(|p| p.name.clone()).collect();
        for (param, arg_expr) in view_params.iter().zip(view_args.iter()) {
            result_expr = subst_ident_in_expr(&result_expr, param, arg_expr);
        }

        return Ok((result_expr, true));
    }

    let mut changed = false;
    let new_kind = match &formula.kind {
        Binary { op, left, right } => {
            let (l, cl) = rewrite_post_in_expr(left, methods)?;
            let (r, cr) = rewrite_post_in_expr(right, methods)?;
            changed = cl || cr;
            Binary { op: op.clone(), left: Box::new(l), right: Box::new(r) }
        }
        Unary { op, operand } => {
            let (o, c) = rewrite_post_in_expr(operand, methods)?;
            changed = c;
            Unary { op: op.clone(), operand: Box::new(o) }
        }
        Call { func, args, trailing } => {
            let (f, cf) = rewrite_post_in_expr(func, methods)?;
            let mut new_args = Vec::with_capacity(args.len());
            let mut ca = false;
            for arg in args {
                let (e, c) = rewrite_post_in_expr(arg.expr(), methods)?;
                ca = ca || c;
                new_args.push(match arg {
                    crate::ast::CallArg::Item(_) => crate::ast::CallArg::Item(e),
                    crate::ast::CallArg::Spread(_) => crate::ast::CallArg::Spread(e),
                    crate::ast::CallArg::Named { name, .. } =>
                        crate::ast::CallArg::Named { name: name.clone(), value: e },
                });
            }
            changed = cf || ca;
            Call { func: Box::new(f), args: new_args, trailing: trailing.clone() }
        }
        _ => return Ok((formula.clone(), false)),
    };

    Ok((crate::ast::Expr { kind: new_kind, span: formula.span }, changed))
}

fn verify_post_axiom_with_handler(
    pipeline: &VerificationPipeline,
    ax: &AxiomInfo,
    pure_views: &std::collections::HashMap<String, super::encode::PureViewSig>,
    methods: &[crate::ast::HandlerMethod],
    effect_name: &str,
    module: &Module,
    inferred_pure: &std::collections::HashSet<String>,
) -> VerifyResult {
    let rewritten_formula = match rewrite_post_in_expr(ax.formula, methods) {
        Ok((expr, true)) => expr,
        Ok((_, false)) => {
            return VerifyResult::Unknown(
                "post-axiom: no post(...) term found during rewrite".into(),
            );
        }
        Err(reason) => {
            return VerifyResult::Unknown(format!(
                "post-axiom `{}`: symbolic rewrite not applicable — {}", ax.axiom_name, reason
            ));
        }
    };

    let ax_rewritten = AxiomInfo {
        effect_name: ax.effect_name.clone(),
        axiom_name: ax.axiom_name.clone(),
        binders: ax.binders,
        formula: &rewritten_formula,
        is_generic: ax.is_generic,
    };

    let pure_fns = collect_pure_fns(module, inferred_pure);
    let trusted_fns = collect_trusted_fns(module);
    let ctx = super::encode::EncodeCtx { pure_views, pure_fns: &pure_fns, trusted_fns: &trusted_fns, var_sorts: std::collections::HashMap::new() };
    let mut backend = pipeline.create_backend();

    for (op_name, sig) in pure_views {
        let uf = super::encode::pure_view_uf_name(&sig.effect_name, op_name);
        backend.declare_function(&uf, &sig.param_sorts, sig.return_sort.clone());
    }

    let Some(ax_formula) = encode_axiom(&ax_rewritten, pure_views) else {
        let binder_names: Vec<String> = ax.binders.iter().map(|bd| bd.name.clone()).collect();
        let body = match super::encode::encode_expr_with_ctx(&rewritten_formula, &ctx) {
            Ok(t) => t,
            Err(e) => return VerifyResult::EncodingFailed(format!("{:?}", e)),
        };
        let mut binder_sorts: std::collections::HashMap<String, SortRef> = Default::default();
        infer_binder_sorts(&rewritten_formula, &binder_names, pure_views, &mut binder_sorts);
        let binders: Vec<(String, SortRef)> = binder_names.iter()
            .map(|n| (n.clone(), binder_sorts.remove(n).unwrap_or(SortRef::Int)))
            .collect();
        let formula_term = if binders.is_empty() {
            body
        } else {
            SmtTerm::Forall(binders, vec![], Box::new(body))
        };
        return match try_prove(&mut *backend, formula_term) {
            SatResult::Unsat(_) => VerifyResult::Proven,
            SatResult::Sat(model) => {
                let cex = format_counterexample(&model);
                VerifyResult::Disproved(model, cex)
            }
            SatResult::Unknown(reason) => VerifyResult::Unknown(unknown_to_diag_message(reason)),
        };
    };

    match try_prove(&mut *backend, ax_formula) {
        SatResult::Unsat(_) => VerifyResult::Proven,
        SatResult::Sat(model) => {
            let cex = format_counterexample(&model);
            VerifyResult::Disproved(model, cex)
        }
        SatResult::Unknown(reason) => VerifyResult::Unknown(unknown_to_diag_message(reason)),
    }
}

fn axiom_formula_has_post(e: &crate::ast::Expr) -> bool {
    use crate::ast::ExprKind::*;
    match &e.kind {
        Call { func, args, .. } => {
            if let Ident(name) = &func.kind {
                if name == "post" { return true; }
            }
            if axiom_formula_has_post(func) { return true; }
            args.iter().any(|a| axiom_formula_has_post(a.expr()))
        }
        Binary { left, right, .. } => {
            axiom_formula_has_post(left) || axiom_formula_has_post(right)
        }
        Unary { operand, .. } => axiom_formula_has_post(operand),
        If { cond, then, else_ } => {
            if axiom_formula_has_post(cond) { return true; }
            if let Some(t) = &then.trailing { if axiom_formula_has_post(t) { return true; } }
            match else_ {
                Some(crate::ast::ElseBranch::Block(b)) => {
                    if let Some(t) = &b.trailing { axiom_formula_has_post(t) } else { false }
                }
                Some(crate::ast::ElseBranch::If(ie)) => axiom_formula_has_post(ie),
                None => false,
            }
        }
        _ => false,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Collect helpers
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn collect_verify_bindings_block<'a>(
    b: &'a crate::ast::Block,
    out: &mut Vec<(String, Span, &'a crate::ast::Expr)>,
) {
    for s in &b.stmts {
        match s {
            Stmt::Expr(e) => collect_verify_bindings_expr(e, out),
            Stmt::Let(ld) => collect_verify_bindings_expr(&ld.value, out),
            Stmt::Assign { value, .. } => collect_verify_bindings_expr(value, out),
            _ => {}
        }
    }
    if let Some(t) = &b.trailing { collect_verify_bindings_expr(t, out); }
}

pub(super) fn collect_verify_bindings_expr<'a>(
    e: &'a crate::ast::Expr,
    out: &mut Vec<(String, Span, &'a crate::ast::Expr)>,
) {
    use crate::ast::ExprKind::*;
    match &e.kind {
        With { bindings, body } => {
            for b in bindings {
                if !matches!(b.verification, HandlerVerification::Verify) { continue }
                let eff_name = match &b.effect {
                    TypeRef::Named { path, .. } => path.last().cloned().unwrap_or_default(),
                    _ => String::new(),
                };
                if eff_name.is_empty() { continue }
                out.push((eff_name, b.span, &b.handler));
            }
            collect_verify_bindings_block(body, out);
        }
        Block(b) => collect_verify_bindings_block(b, out),
        If { cond, then, else_ } => {
            collect_verify_bindings_expr(cond, out);
            collect_verify_bindings_block(then, out);
            match else_ {
                Some(crate::ast::ElseBranch::Block(b)) => collect_verify_bindings_block(b, out),
                Some(crate::ast::ElseBranch::If(ie)) => collect_verify_bindings_expr(ie, out),
                None => {}
            }
        }
        Call { func, args, .. } => {
            collect_verify_bindings_expr(func, out);
            for a in args { collect_verify_bindings_expr(a.expr(), out); }
        }
        Binary { left, right, .. } => {
            collect_verify_bindings_expr(left, out);
            collect_verify_bindings_expr(right, out);
        }
        _ => {}
    }
}
