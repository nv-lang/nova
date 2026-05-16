//! Plan 33.1 Р¤.3: Verification pipeline.
//!
//! РђР»РіРѕСЂРёС‚Рј РґР»СЏ РєР°Р¶РґРѕР№ С„СѓРЅРєС†РёРё СЃ РєРѕРЅС‚СЂР°РєС‚Р°РјРё:
//!
//! 1. Encode РїР°СЂР°РјРµС‚СЂС‹ РєР°Рє Var-С‹ (SMT-IR).
//! 2. Encode `requires` в†' assertions РІ backend.
//! 3. Encode body: РґР»СЏ straight-line `=> expr` body = symbolic value,
//!    РєРѕС‚РѕСЂРѕРµ Р·Р°РјРµРЅСЏРµС‚ `result` РІ ensures. Р"Р»СЏ block-body СЃ trailing вЂ"
//!    С‚Рѕ Р¶Рµ СЃР°РјРѕРµ.
//! 4. Р"Р»СЏ РєР°Р¶РґРѕРіРѕ `ensures Q`:
//!    - Substitute `result` в†' encoded_body_value РІ Q.
//!    - try_prove(Q): unsat в†' proven; sat в†' counterexample; unknown в†' fallback.
//! 5. Р РµР·СѓР»СЊС‚Р°С‚ per-fn в†' Р°РіСЂРµРіРёСЂСѓРµС‚СЃСЏ РІ pipeline-level diagnostics.
//!
//! Plan 33.1 РѕРіСЂР°РЅРёС‡РµРЅРёСЏ:
//! - Body РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ encodable (СЃРј. encode.rs `Unsupported` case'С‹).
//! - Block-bodies СЃРѕ statements (let, if-stmts) РќР• encoded; РёС… РєРѕРЅС‚СЂР°РєС‚С‹
//!   = `Unknown(NotAttempted)` (runtime fallback СЂР°Р±РѕС‚Р°РµС‚).
//! - Function calls РІ body РќР• encoded (composition РІ 33.2).

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use super::ir::*;
use super::encode;
use super::backend::{SmtBackend, try_prove};
use super::backend::trivial::TrivialBackend;

/// Р РµР·СѓР»СЊС‚Р°С‚ РІРµСЂРёС„РёРєР°С†РёРё РѕРґРЅРѕРіРѕ РєРѕРЅС‚СЂР°РєС‚Р°.
#[derive(Debug, Clone)]
pub enum VerifyResult {
    Proven,
    /// РљРѕРЅС‚СЂ-РїСЂРёРјРµСЂ (С„РѕСЂРјСѓР»Р° РѕРїСЂРѕРІРµСЂР¶РµРЅР°).
    Disproved(Model, String),
    /// SMT РЅРµ СЃРїСЂР°РІРёР»СЃСЏ вЂ" fallback to runtime.
    Unknown(String),
    /// Encoder РЅРµ СЃРјРѕРі РїРѕСЃС‚СЂРѕРёС‚СЊ SMT-IR (fall back to runtime).
    EncodingFailed(String),
    /// Ф.6.2 (Plan 33.6): не-ошибка, пользователь должен знать [W2402].
    Warning(String),
}

/// Р'С‹Р±РѕСЂ SMT backend'Р°.
///
/// Plan 33 Z3 milestone (V1 closure): РґРѕР±Р°РІР»РµРЅ `Z3`. РџРѕ СѓРјРѕР»С‡Р°РЅРёСЋ
/// `Trivial` (backward-compat + no external deps). Switch:
/// - CLI flag (nova check/test/compile): `--smt-backend=z3`.
/// - Env var: `NOVA_SMT_BACKEND=z3`.
///
/// Р•СЃР»Рё feature `z3-backend` РЅРµ compiled-in, `Z3` С‚РµСЂСЏРµС‚ СЃРјС‹СЃР» вЂ"
/// `create_backend` РїР°РґР°РµС‚ РѕР±СЂР°С‚РЅРѕ РЅР° trivial СЃ stderr-warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendChoice {
    Trivial,
    Z3,
}

impl BackendChoice {
    /// РџР°СЂСЃРёС‚ СЃС‚СЂРѕРєСѓ, used Рё РґР»СЏ CLI Рё РґР»СЏ env-var.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "trivial" | "default" | "" => Some(BackendChoice::Trivial),
            "z3" => Some(BackendChoice::Z3),
            _ => None,
        }
    }

    /// Backend РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ: СЃРјРѕС‚СЂРёРј `NOVA_SMT_BACKEND`, РёРЅР°С‡Рµ Trivial.
    pub fn from_env() -> Self {
        std::env::var("NOVA_SMT_BACKEND")
            .ok()
            .and_then(|s| Self::parse(&s))
            .unwrap_or(BackendChoice::Trivial)
    }
}

pub struct VerificationPipeline {
    timeout_ms: u32,
    backend: BackendChoice,
}

impl VerificationPipeline {
    pub fn new() -> Self {
        Self { timeout_ms: 2000, backend: BackendChoice::from_env() }
    }

    pub fn with_timeout(timeout_ms: u32) -> Self {
        Self { timeout_ms, backend: BackendChoice::from_env() }
    }

    /// Plan 33 Z3 milestone: СЏРІРЅС‹Р№ РІС‹Р±РѕСЂ backend'Р° (override env-var).
    pub fn with_backend(mut self, backend: BackendChoice) -> Self {
        self.backend = backend;
        self
    }

    /// РЎРѕР·РґР°С‚СЊ backend instance СЃРѕРіР»Р°СЃРЅРѕ РІС‹Р±РѕСЂСѓ. Falls back to trivial
    /// СЃ warning'РѕРј РµСЃР»Рё z3 РЅРµ compiled-in.
    pub(super) fn create_backend(&self) -> Box<dyn SmtBackend> {
        match self.backend {
            BackendChoice::Trivial => Box::new(TrivialBackend::new()),
            BackendChoice::Z3 => {
                #[cfg(feature = "z3-backend")]
                {
                    Box::new(super::backend::z3::Z3Backend::new(self.timeout_ms))
                }
                #[cfg(not(feature = "z3-backend"))]
                {
                    // User-friendly fallback (РЅРѕ РЅРµ silent вЂ" РїРёС€РµРј РІ stderr).
                    eprintln!(
                        "warning: --smt-backend=z3 requested, but binary built without \
                         `--features z3-backend`; falling back to trivial backend. \
                         Rebuild СЃ `cargo build --features z3-backend`."
                    );
                    Box::new(TrivialBackend::new())
                }
            }
        }
    }

    /// Verify РѕРґРЅСѓ С„СѓРЅРєС†РёСЋ: РІРѕР·РІСЂР°С‰Р°РµС‚ list of (Contract span, VerifyResult).
    /// Backend РІС‹Р±РёСЂР°РµС‚СЃСЏ С‡РµСЂРµР· `BackendChoice` (env-var / CLI flag).
    ///
    /// Plan 33.3 Р¤.9: РїСЂРёРЅРёРјР°РµС‚ `module` РґР»СЏ СЂР°Р·СЂРµС€РµРЅРёСЏ pure_view-РІС‹Р·РѕРІРѕРІ
    /// Рё СЂРµРіРёСЃС‚СЂР°С†РёРё axioms СЌС„С„РµРєС‚РѕРІ РІ SMT-scope СЌС‚РѕР№ fn.
    pub fn verify_fn(
        &self,
        module: &Module,
        fd: &FnDecl,
        inferred_pure: &std::collections::HashSet<String>,
    ) -> Vec<(Span, VerifyResult)> {
        if fd.contracts.is_empty() { return Vec::new(); }

        let mut backend = self.create_backend();

        // Plan 33.3 Р¤.9: СЂРµРµСЃС‚СЂ pure_view-ops РјРѕРґСѓР»СЏ. РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ
        // encoder'РѕРј РґР»СЏ РїРµСЂРµРІРѕРґР° `balance(id)` в†' UF `_view_Db_balance(id)`.
        let pure_views = collect_pure_views(module);
        let mut pure_fns = collect_pure_fns(module, inferred_pure);
        // Р¤.3: РїСЂРё РєРѕРґРёСЂРѕРІР°РЅРёРё С‚РµР»Р° С‚РµРєСѓС‰РµР№ fn СѓР±РёСЂР°РµРј РµС' body_expr РёР· РєРѕРЅС‚РµРєСЃС‚Р°,
        // С‡С‚РѕР±С‹ encoder РЅРµ РїС‹С‚Р°Р»СЃСЏ РёРЅР»Р°Р№РЅРёС‚СЊ СЂРµРєСѓСЂСЃРёРІРЅС‹Р№ РІС‹Р·РѕРІ: factorial(n-1)
        // СЃРѕРґРµСЂР¶РёС‚ factorial в†' body в†' factorial(n-1) в†' в€ћ.
        // UF-Р°РїРїР»РёРєР°С†РёСЏ (_pure_factorial(n-1)) РѕСЃС‚Р°С'С‚СЃСЏ вЂ" РґР»СЏ soundness
        // РґРѕСЃС‚Р°С‚РѕС‡РЅРѕ С‚РµР»Р°-Р°РєСЃРёРѕРјС‹ (body axiom), РєРѕС‚РѕСЂСѓСЋ Z3 instantiates РїРѕ trigger.
        if let Some(entry) = pure_fns.get_mut(&fd.name) {
            entry.body_expr = None;
        }
        let var_sorts: std::collections::HashMap<String, SortRef> = fd.params.iter()
            .map(|p| (p.name.clone(), type_to_sort(&p.ty))).collect();
        let ctx = super::encode::EncodeCtx { pure_views: &pure_views, pure_fns: &pure_fns, var_sorts };

        // Plan 33.3 Р¤.9: pre-declare РІСЃРµ pure_view UFs РІ backend'Рµ.
        // Р'РµР· СЌС‚РѕРіРѕ Z3 auto-declare'РёС‚ UF СЃ Int sorts РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ;
        // pre-decl РґР°С'С‚ РїСЂР°РІРёР»СЊРЅС‹Рµ sorts РёР· effect-СЃРёРіРЅР°С‚СѓСЂС‹ (РІР°Р¶РЅРѕ РґР»СЏ
        // soundness РєРѕРіРґР° args РЅРµ int'РѕРІС‹Рµ).
        for (op_name, sig) in &pure_views {
            let uf = super::encode::pure_view_uf_name(&sig.effect_name, op_name);
            backend.declare_function(&uf, &sig.param_sorts, sig.return_sort.clone());
        }

        // Plan 33.4 D.0.2: pre-declare UFs РґР»СЏ #pure fns + emit body axioms.
        for (fn_name, info) in &pure_fns {
            let uf = super::encode::pure_fn_uf_name(fn_name);
            backend.declare_function(&uf, &info.param_sorts, info.return_sort.clone());
        }
        // Body axioms: forall params. uf(params) == body_encoded.
        for item in &module.items {
            let Item::Fn(pfd) = item else { continue };
            if !matches!(pfd.purity, Purity::Pure) { continue; }
            let Some(info) = pure_fns.get(&pfd.name) else { continue };
            let body_expr = match &pfd.body {
                FnBody::Expr(e) => Some(e),
                FnBody::Block(b) if b.stmts.is_empty() => b.trailing.as_deref(),
                _ => None,
            };
            if let Some(body_e) = body_expr {
                if let Ok(body_term) = encode::encode_expr_with_ctx(body_e, &ctx) {
                    let param_vars: Vec<SmtTerm> = info.param_names.iter()
                        .map(|n| SmtTerm::Var(n.clone())).collect();
                    let uf_app = SmtTerm::App(encode::pure_fn_uf_name(&pfd.name), param_vars);
                    let eq_body = SmtTerm::eq(uf_app, body_term);
                    let binders: Vec<(String, SortRef)> = info.param_names.iter()
                        .zip(info.param_sorts.iter())
                        .map(|(n, s)| (n.clone(), s.clone()))
                        .collect();
                    let axiom = if binders.is_empty() { eq_body } else {
                        SmtTerm::Forall(binders, vec![], Box::new(eq_body))
                    };
                    backend.assert(Assertion {
                        formula: axiom,
                        label: Some(format!("pure_fn_body@{}", pfd.name)),
                    });
                }
            }
        }

        // 1. Declare params as Vars.
        for p in &fd.params {
            let sort = type_to_sort(&p.ty);
            backend.declare_var(&p.name, sort);
        }

        // Plan 33.3 Р¤.9: РґР»СЏ РєР°Р¶РґРѕРіРѕ СЌС„С„РµРєС‚Р° РІ СЃРёРіРЅР°С‚СѓСЂРµ fn СЂРµРіРёСЃС‚СЂРёСЂСѓРµРј
        // axioms РєР°Рє РіР»РѕР±Р°Р»СЊРЅС‹Рµ assertions (Forall'С‹). Z3 instantiate'РёС‚
        // РёС… С‡РµСЂРµР· trigger-based heuristics; TrivialBackend С…СЂР°РЅРёС‚ as-is.
        let fn_effects: std::collections::HashSet<String> = fd.effects.iter()
            .filter_map(|tr| match tr {
                TypeRef::Named { path, .. } => path.last().cloned(),
                _ => None,
            })
            .collect();
        for ax_info in collect_axioms(module) {
            if !fn_effects.contains(&ax_info.effect_name) { continue; }
            if let Some(formula) = encode_axiom(&ax_info, &pure_views) {
                backend.assert(Assertion {
                    formula,
                    label: Some(format!("axiom@{}.{}",
                        ax_info.effect_name, ax_info.axiom_name)),
                });
            }
        }

        // 2. Encode requires -> assertions.
        // Ф.1.2: если requires не encodable -> EncodingFailed для этого requires.
        let mut requires_failed = false;
        let mut req_failures: Vec<(Span, VerifyResult)> = Vec::new();
        for c in &fd.contracts {
            if matches!(c.kind, ContractKind::Requires) {
                match encode::encode_expr_with_ctx(&c.expr, &ctx) {
                    Ok(t) => backend.assert(Assertion {
                        formula: t,
                        label: Some(format!("requires@{}", c.span.start)),
                    }),
                    Err(super::encode::EncodingError::Unsupported(msg)) => {
                        // Ф.1.2: requires не encodable -> EncodingFailed с маркером E2401.
                        // Префикс "[CONTRACT_UNSUPPORTED]" отличает от "body not encodable".
                        req_failures.push((c.span, VerifyResult::EncodingFailed(
                            format!("[CONTRACT_UNSUPPORTED] {}", msg))));
                        requires_failed = true;
                    }
                }
            }
        }

        // D.1.2: frame axiom вЂ" РґР»СЏ РєР°Р¶РґРѕРіРѕ param NOT РІ modifies-СЃРїРёСЃРєРµ
        // assert _old_<x> == <x> (frame: Р·РЅР°С‡РµРЅРёРµ РЅРµ РёР·РјРµРЅРёР»РѕСЃСЊ).
        // Р­С‚Рѕ РїРѕР·РІРѕР»СЏРµС‚ Z3 reasoning'РѕРІР°С‚СЊ РЅР°Рґ `old(x)` РІ ensures:
        // РµСЃР»Рё x РЅРµ РІ modifies, С‚Рѕ old(x) == x С‚СЂРёРІРёР°Р»СЊРЅРѕ РІ pre-state.
        {
            let modifies_names: std::collections::HashSet<String> = fd.modifies.iter()
                .filter_map(|ft| match ft {
                    FrameTarget::Whole(e) => {
                        if let ExprKind::Ident(n) = &e.kind { Some(n.clone()) } else { None }
                    }
                    FrameTarget::Field { receiver, .. } => {
                        if let ExprKind::Ident(n) = &receiver.kind { Some(n.clone()) } else { None }
                    }
                    _ => None,
                })
                .collect();
            for p in &fd.params {
                if !modifies_names.contains(&p.name) {
                    // assert: _old_<x> == <x> (frame: param unchanged).
                    let var_term = SmtTerm::Var(p.name.clone());
                    let old_term = SmtTerm::Var(format!("_old_{}", p.name));
                    let frame_eq = SmtTerm::eq(old_term, var_term);
                    backend.assert(Assertion {
                        formula: frame_eq,
                        label: Some(format!("frame@{}", p.name)),
                    });
                }
            }
        }

        // 2.5. Ф.4.1: применить `apply lemma(args)` из тела fn.
        // Для каждого apply-statement в блоке: найти лемму в модуле,
        // замените lemma.params → args в каждом ensures, assert в backend.
        // Это даёт caller доступ к lemma.ensures как аксиоме SMT.
        for (lemma_name, args, apply_span) in collect_apply_stmts_in_body(&fd.body) {
            if let Some(lemma_ensures) = find_lemma_ensures(module, &lemma_name) {
                for (param_names, ensures_expr) in &lemma_ensures {
                    if param_names.len() != args.len() { continue; }
                    // Кодируем args в SMT.
                    let encoded_args: Vec<Option<SmtTerm>> = args.iter()
                        .map(|a| encode::encode_expr_with_ctx(a, &ctx).ok())
                        .collect();
                    if encoded_args.iter().any(|a| a.is_none()) { continue; }
                    // Кодируем ensures_expr лемммы.
                    if let Ok(ensures_term) = encode::encode_expr_with_ctx(ensures_expr, &ctx) {
                        // Подставляем: каждый param_name → encoded_arg.
                        let mut term = ensures_term;
                        for (pname, enc_arg) in param_names.iter().zip(encoded_args.iter()) {
                            if let Some(ea) = enc_arg {
                                term = term.substitute(pname, ea);
                            }
                        }
                        backend.assert(Assertion {
                            formula: term,
                            label: Some(format!("apply@{}:{}", lemma_name, apply_span.start)),
                        });
                    }
                }
            }
        }

        // 2.6. Ф.4.2: обработать `calc { ... }` из тела fn.
        // Для каждого calc-блока: каждый смежный шаг (e_i rel e_{i+1}) доказывается
        // и ассертируется в SMT-scope (как lemma: доказано → доступно для ensures).
        // Результат: SMT знает все промежуточные равенства/неравенства.
        let calc_step_results = verify_calc_stmts_in_body(&fd.body, &ctx, &mut *backend);

        // 3. Encode body value. РўРѕР»СЊРєРѕ РґР»СЏ `=> expr` С„РѕСЂРј
        // (block-bodies СЃ trailing-only С‚РѕР¶Рµ OK).
        // Ф.4.1: блок, содержащий только ghost `apply`-стейтменты, тоже считается
        // trailing-only -- apply стираются в runtime, не влияют на значение body.
        let body_val = match &fd.body {
            FnBody::Expr(e) => encode::encode_expr_with_ctx(e, &ctx).ok(),
            FnBody::Block(b) if block_has_only_ghost_stmts(b) => {
                b.trailing.as_ref().and_then(|e| encode::encode_expr_with_ctx(e, &ctx).ok())
            }
            _ => None,
        };

        // 4. Verify each ensures.
        // Ф.1.2: включаем failures от requires encoding.
        let mut results = calc_step_results; // Ф.4.2: calc шаги добавляются первыми
        results.extend(req_failures);
        for c in &fd.contracts {
            if !matches!(c.kind, ContractKind::Ensures) { continue; }
            if requires_failed {
                results.push((c.span, VerifyResult::EncodingFailed(
                    "requires-context failed to encode".into())));
                continue;
            }
            let encoded = match encode::encode_expr_with_ctx(&c.expr, &ctx) {
                Ok(t) => t,
                Err(super::encode::EncodingError::Unsupported(msg)) => {
                    // Ф.1.2: contract expr не encodable -> E2401 маркер.
                    results.push((c.span, VerifyResult::EncodingFailed(
                        format!("[CONTRACT_UNSUPPORTED] {}", msg))));
                    continue;
                }
            };
            // Substitute result в†' body_val (РµСЃР»Рё РµСЃС‚СЊ).
            let goal = if let Some(bv) = &body_val {
                encoded.substitute("result", bv)
            } else {
                // Body РЅРµ encoded в†' fallback.
                results.push((c.span, VerifyResult::EncodingFailed(
                    "function body not encodable (use runtime check)".into())));
                continue;
            };
            // РўР°РєР¶Рµ РїРѕРґРјРµРЅРёРј `old(x)` в†' Р·РЅР°С‡РµРЅРёРµ `x` РЅР° entry-state.
            // Р' 33.1 РЅРµС‚ mut params в†' СЃС‚Р°СЂС‹Рµ Р·РЅР°С‡РµРЅРёСЏ = С‚РµРєСѓС‰РёРµ Р·РЅР°С‡РµРЅРёСЏ.
            let goal = substitute_old(&goal);

            // try_prove(goal). `&mut *backend` С‡С‚РѕР±С‹ coerce Box<dyn> в†' &mut dyn.
            match try_prove(&mut *backend, goal) {
                SatResult::Unsat(_) => results.push((c.span, VerifyResult::Proven)),
                SatResult::Sat(model) => {
                    let cex = format_counterexample(&model);
                    results.push((c.span, VerifyResult::Disproved(model, cex)));
                }
                SatResult::Unknown(reason) => {
                    // Plan 33.3 Р¤.9.10: AI-friendly diagnostic вЂ" РєР°С‚РµРіРѕСЂРёР·РёСЂСѓРµРј
                    // reason + suggestions.
                    let msg = unknown_to_diag_message(reason);
                    results.push((c.span, VerifyResult::Unknown(msg)));
                }
            }
        }

        // D.1.5: verify ensures_fail clauses (Fail-path postconditions).
        // РњРѕРґРµР»СЊ (V1, conservative): РІРµСЂРёС„РёС†РёСЂСѓРµРј ensures_fail РЅРµР·Р°РІРёСЃРёРјРѕ,
        // РёСЃРїРѕР»СЊР·СѓСЏ С‚Рµ Р¶Рµ params + requires-assertions (entry state).
        // `result` РЅРµРґРѕСЃС‚СѓРїРµРЅ; `old(x)` в†' x (entry-state, РЅРµС‚ РјСѓС‚Р°Р±РµР»СЊРЅС‹С… params).
        for c in &fd.contracts {
            if !matches!(c.kind, ContractKind::EnsuresFail) { continue; }
            if requires_failed {
                results.push((c.span, VerifyResult::EncodingFailed(
                    "requires-context failed to encode".into())));
                continue;
            }
            let encoded = match encode::encode_expr_with_ctx(&c.expr, &ctx) {
                Ok(t) => t,
                Err(super::encode::EncodingError::Unsupported(msg)) => {
                    // Ф.1.2: contract expr не encodable -> E2401 маркер.
                    results.push((c.span, VerifyResult::EncodingFailed(
                        format!("[CONTRACT_UNSUPPORTED] {}", msg))));
                    continue;
                }
            };
            // `old(x)` в†' x (entry-state, params РЅРµРёР·РјРµРЅРЅС‹ РІ V1).
            let goal = substitute_old(&encoded);
            match try_prove(&mut *backend, goal) {
                SatResult::Unsat(_) => results.push((c.span, VerifyResult::Proven)),
                SatResult::Sat(model) => {
                    let cex = format_counterexample(&model);
                    results.push((c.span, VerifyResult::Disproved(model,
                        format!("ensures_fail РјРѕР¶РµС‚ РЅРµ РІС‹РїРѕР»РЅСЏС‚СЊСЃСЏ РЅР° Fail-РїСѓС‚Рё: {}", cex))));
                }
                SatResult::Unknown(reason) => {
                    results.push((c.span, VerifyResult::Unknown(
                        format!("ensures_fail: {}", unknown_to_diag_message(reason)))));
                }
            }
        }

        // Plan 33.4 D.0.4: verify decreases well-foundedness.
        if let Some(dec_expr) = &fd.decreases {
            if let Ok(dec_entry) = encode::encode_expr_with_ctx(dec_expr, &ctx) {
                // Step A: dec_entry >= 0 at entry (requires already in scope).
                let non_neg = SmtTerm::App(">=".into(), vec![
                    dec_entry.clone(),
                    SmtTerm::IntLit(0),
                ]);
                let dec_span = fd.span;
                match try_prove(&mut *backend, non_neg) {
                    SatResult::Unsat(_) => results.push((dec_span, VerifyResult::Proven)),
                    SatResult::Sat(model) => {
                        let cex = format_counterexample(&model);
                        results.push((dec_span, VerifyResult::Disproved(model,
                            format!("decreases measure may be negative: {}", cex))));
                    }
                    SatResult::Unknown(reason) => {
                        results.push((dec_span, VerifyResult::Unknown(
                            format!("decreases non-negative: {}", unknown_to_diag_message(reason)))));
                    }
                }
                // Step B: at each recursive call, dec_at_call < dec_entry.
                let recursive_calls = find_recursive_calls_in_body(&fd.body, &fd.name);
                for (call_span, call_args) in recursive_calls {
                    if call_args.len() != fd.params.len() { continue; }
                    // Substitute params into dec_entry to get dec_at_call.
                    let mut dec_at_call = dec_entry.clone();
                    for (param, arg_expr) in fd.params.iter().zip(call_args.iter()) {
                        match encode::encode_expr_with_ctx(arg_expr, &ctx) {
                            Ok(enc_arg) => {
                                dec_at_call = dec_at_call.substitute(&param.name, &enc_arg);
                            }
                            Err(_) => {
                                results.push((call_span, VerifyResult::EncodingFailed(
                                    format!("cannot encode recursive call arg for decreases check"))));
                                continue;
                            }
                        }
                    }
                    let decreasing = SmtTerm::App("<".into(), vec![
                        dec_at_call,
                        dec_entry.clone(),
                    ]);
                    match try_prove(&mut *backend, decreasing) {
                        SatResult::Unsat(_) => results.push((call_span, VerifyResult::Proven)),
                        SatResult::Sat(model) => {
                            let cex = format_counterexample(&model);
                            results.push((call_span, VerifyResult::Disproved(model,
                                format!("decreases measure may not decrease at recursive call: {}", cex))));
                        }
                        SatResult::Unknown(reason) => {
                            results.push((call_span, VerifyResult::Unknown(
                                format!("decreases at recursive call: {}", unknown_to_diag_message(reason)))));
                        }
                    }
                }
            }
        }

        // Plan 33.4 D.0.3: verify loop invariants at entry.
        // Р"Р»СЏ РєР°Р¶РґРѕРіРѕ С†РёРєР»Р° СЃ `invariant <expr>` РІ С‚РµР»Рµ fn:
        // РїС‹С‚Р°РµРјСЃСЏ РґРѕРєР°Р·Р°С‚СЊ С‡С‚Рѕ invariant РІС‹РїРѕР»РЅСЏРµС‚СЃСЏ РїСЂРё РІС…РѕРґРµ РІ С„СѓРЅРєС†РёСЋ
        // (РїСЂРё СѓСЃР»РѕРІРёРё requires). Р­С‚Рѕ partial check вЂ" РЅРµ РґРѕРєР°Р·С‹РІР°РµС‚
        // preservation (РїРѕР»РЅС‹Р№ havoc-based encoding вЂ" V2).
        let loop_invs = collect_loop_invariants_in_body(&fd.body);
        for (inv_span, inv_expr) in loop_invs {
            match encode::encode_expr_with_ctx(&inv_expr, &ctx) {
                Ok(inv_term) => {
                    match try_prove(&mut *backend, inv_term) {
                        SatResult::Unsat(_) => results.push((inv_span, VerifyResult::Proven)),
                        SatResult::Sat(model) => {
                            let cex = format_counterexample(&model);
                            results.push((inv_span, VerifyResult::Disproved(model,
                                format!("loop invariant may not hold at entry: {}", cex))));
                        }
                        SatResult::Unknown(reason) => {
                            results.push((inv_span, VerifyResult::Unknown(
                                format!("loop invariant at entry: {}", unknown_to_diag_message(reason)))));
                        }
                    }
                }
                Err(e) => {
                    // Ф.6.2 (Plan 33.6): loop invariant не encodable -- W2402.
                    // Runtime check (inject_loop_invariants) покрывает runtime часть,
                    // но пользователь должен знать что SMT не проверил invariant.
                    let reason_str = match &e {
                        super::encode::EncodingError::Unsupported(s) => s.clone(),
                    };
                    let msg = format!(
                        "loop invariant не удалось закодировать в SMT [W2402]: {}.\n  \
                         Runtime-proверка активна, SMT-доказательство пропущено.\n  \
                         Упростите invariant или используйте только int/bool/pure-fn.",
                        reason_str);
                    results.push((inv_span, VerifyResult::Warning(msg)));
                }
            }
        }

        // Ф.2 (Plan 33.5): Loop invariant preservation via havoc-based encoding.
        //
        // РђР»РіРѕСЂРёС‚Рј (Dafny/Verus standard):
        // 1. РЎРѕР±СЂР°С‚СЊ РІСЃРµ while-loops СЃ invariants РІ С‚РµР»Рµ fn.
        // 2. Р"Р»СЏ РєР°Р¶РґРѕРіРѕ С†РёРєР»Р°:
        //    a. Havoc: РґР»СЏ РєР°Р¶РґРѕР№ mutable var РІ С‚РµР»Рµ С†РёРєР»Р° вЂ" fresh SMT var
        //       (СЃРёРјРІРѕР»РёС‡РµСЃРєРѕРµ СЃРѕСЃС‚РѕСЏРЅРёРµ РїРѕСЃР»Рµ N РёС‚РµСЂР°С†РёР№).
        //    b. Assume invariant РЅР° havoc'd state + assume loop cond true.
        //    c. Symbolic exec body (V1: straight-line assignments only).
        //    d. Assert invariant РїРѕСЃР»Рµ body (РЅР° post-state).
        //    e. UNSAT в†' invariant preserved; SAT в†' counterexample.
        let loop_pres = collect_loop_preservation_targets(&fd.body);
        for lp in loop_pres {
            let res = verify_loop_preservation(&lp, &ctx, &mut *backend);
            results.extend(res);
        }

        // Р¤.1.3 (Plan 33.5): verify loop decreases.
        // V1 scope: simple `while <cond> decreases <m> { ... }` РіРґРµ body
        // СЃРѕРґРµСЂР¶РёС‚ РїСЂСЏРјРѕРµ decrement `var = var - 1` РёР»Рё `var = var + 1`
        // (РІ Р·Р°РІРёСЃРёРјРѕСЃС‚Рё РѕС‚ СѓР±С‹РІР°РЅРёСЏ РёР»Рё РІРѕР·СЂР°СЃС‚Р°РЅРёСЏ). Р"РѕРєР°Р·С‹РІР°РµРј:
        //   1. dec_pre >= 0 (non-negative РїСЂРё РІС…РѕРґРµ, РїРѕРґ requires).
        //   2. Р' РєР°Р¶РґРѕР№ РёС‚РµСЂР°С†РёРё dec_post < dec_pre
        //      (РёСЃРїРѕР»СЊР·СѓРµС‚ assignment-analysis: over-approx `var_post = var_pre - 1`).
        let loop_decs = collect_loop_decreases_in_body(&fd.body);
        for (dec_span, dec_expr, body_assignments) in loop_decs {
            match encode::encode_expr_with_ctx(&dec_expr, &ctx) {
                Ok(dec_pre) => {
                    // РџСЂРѕРІРµСЂСЏРµРј dec_pre >= 0 РїРѕРґ requires.
                    let non_neg = SmtTerm::App(">=".into(), vec![dec_pre.clone(), SmtTerm::IntLit(0)]);
                    match try_prove(&mut *backend, non_neg) {
                        SatResult::Unsat(_) => {
                            results.push((dec_span, VerifyResult::Proven));
                        }
                        SatResult::Sat(model) => {
                            let cex = format_counterexample(&model);
                            results.push((dec_span, VerifyResult::Disproved(model,
                                format!("loop decreases measure may be negative: {}", cex))));
                            continue;
                        }
                        SatResult::Unknown(_) => {} // fall through to decrease check
                    }
                    // РџСЂРѕРІРµСЂСЏРµРј С‡С‚Рѕ РјРµСЂР° СѓР±С‹РІР°РµС‚: РґР»СЏ РєР°Р¶РґРѕРіРѕ counter-assignment
                    // `var = var - k` в†' dec_post = dec_pre[var в†' var - k] < dec_pre.
                    // V1: С‚РѕР»СЊРєРѕ РѕРґРЅРѕ scalar decreases expression.
                    // Р•СЃР»Рё РІ body РЅР°Р№РґРµРЅРѕ РїСЂРѕСЃС‚РѕРµ decrement в†' encode РєР°Рє fresh var.
                    for (var_name, delta) in &body_assignments {
                        // dec_post: substitute var в†' (var - delta) РІ dec_expr.
                        let var_minus_delta = SmtTerm::App(
                            "-".into(),
                            vec![SmtTerm::Var(var_name.clone()), SmtTerm::IntLit(*delta)],
                        );
                        let dec_post = dec_pre.substitute(var_name, &var_minus_delta);
                        // prove dec_post < dec_pre (i.e. dec_pre - dec_post > 0)
                        let decreasing = SmtTerm::App("<".into(), vec![dec_post, dec_pre.clone()]);
                        match try_prove(&mut *backend, decreasing) {
                            SatResult::Unsat(_) => {
                                results.push((dec_span, VerifyResult::Proven));
                            }
                            SatResult::Sat(model) => {
                                let cex = format_counterexample(&model);
                                results.push((dec_span, VerifyResult::Disproved(model,
                                    format!("loop decreases measure may not decrease: {}", cex))));
                            }
                            SatResult::Unknown(reason) => {
                                results.push((dec_span, VerifyResult::Unknown(
                                    format!("loop decreases: {}", unknown_to_diag_message(reason)))));
                            }
                        }
                    }
                }
                Err(_) => {} // dec РЅРµ encodable вЂ" skip (РЅРµ Р»РѕРјР°РµРј СЃСѓС‰РµСЃС‚РІСѓСЋС‰РёРµ С‚РµСЃС‚С‹)
            }
        }

        let _ = self.timeout_ms; // РёСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РєРѕРіРґР° РґРѕР±Р°РІРёРј Z3-backend
        results
    }

    /// Ф.4.1: верификация тела леммы.
    ///
    /// Лемма = proven proof term: её `ensures` должны следовать из `requires`
    /// и тела (body). Модель проверки идентична verify_fn, но:
    /// - Лемма обязана верифицироваться (hard error если нет).
    /// - Нет decreases / loop invariants (V1 scope).
    /// - Нет effectful params.
    pub fn verify_lemma(
        &self,
        module: &Module,
        ld: &LemmaDecl,
        inferred_pure: &std::collections::HashSet<String>,
    ) -> Vec<(Span, VerifyResult)> {
        if ld.contracts.is_empty() { return Vec::new(); }

        let mut backend = self.create_backend();
        let pure_views = collect_pure_views(module);
        let pure_fns = collect_pure_fns(module, inferred_pure);
        let var_sorts: std::collections::HashMap<String, SortRef> = ld.params.iter()
            .map(|p| (p.name.clone(), type_to_sort(&p.ty))).collect();
        let ctx = super::encode::EncodeCtx { pure_views: &pure_views, pure_fns: &pure_fns, var_sorts };

        // Pre-declare pure_view UFs.
        for (op_name, sig) in &pure_views {
            let uf = super::encode::pure_view_uf_name(&sig.effect_name, op_name);
            backend.declare_function(&uf, &sig.param_sorts, sig.return_sort.clone());
        }
        // Pre-declare pure_fn UFs.
        for (fn_name, info) in &pure_fns {
            let uf = super::encode::pure_fn_uf_name(fn_name);
            backend.declare_function(&uf, &info.param_sorts, info.return_sort.clone());
        }

        // Объявляем параметры леммы как SMT переменные.
        for p in &ld.params {
            let sort = type_to_sort(&p.ty);
            backend.declare_var(&p.name, sort);
        }

        // Assert requires.
        let mut requires_failed = false;
        for c in &ld.contracts {
            if matches!(c.kind, ContractKind::Requires) {
                match encode::encode_expr_with_ctx(&c.expr, &ctx) {
                    Ok(t) => backend.assert(Assertion {
                        formula: t,
                        label: Some(format!("lemma_requires@{}", c.span.start)),
                    }),
                    Err(_) => { requires_failed = true; }
                }
            }
        }

        // Encode body value (лемма -- это доказательство, body = proof term).
        let body_val = match &ld.body {
            FnBody::Expr(e) => encode::encode_expr_with_ctx(e, &ctx).ok(),
            FnBody::Block(b) if block_has_only_ghost_stmts(b) => {
                b.trailing.as_ref().and_then(|e| encode::encode_expr_with_ctx(e, &ctx).ok())
            }
            _ => None,
        };

        // Verify each ensures clause.
        let mut results = Vec::new();
        for c in &ld.contracts {
            if !matches!(c.kind, ContractKind::Ensures) { continue; }
            if requires_failed {
                results.push((c.span, VerifyResult::EncodingFailed(
                    "lemma requires-context failed to encode".into())));
                continue;
            }
            let encoded = match encode::encode_expr_with_ctx(&c.expr, &ctx) {
                Ok(t) => t,
                Err(super::encode::EncodingError::Unsupported(msg)) => {
                    // Ф.1.2: contract expr не encodable -> E2401 маркер.
                    results.push((c.span, VerifyResult::EncodingFailed(
                        format!("[CONTRACT_UNSUPPORTED] {}", msg))));
                    continue;
                }
            };
            let goal = if let Some(bv) = &body_val {
                encoded.substitute("result", bv)
            } else {
                results.push((c.span, VerifyResult::EncodingFailed(
                    "lemma body not encodable".into())));
                continue;
            };
            let goal = substitute_old(&goal);
            match try_prove(&mut *backend, goal) {
                SatResult::Unsat(_) => results.push((c.span, VerifyResult::Proven)),
                SatResult::Sat(model) => {
                    let cex = format_counterexample(&model);
                    results.push((c.span, VerifyResult::Disproved(model, cex)));
                }
                SatResult::Unknown(reason) => {
                    results.push((c.span, VerifyResult::Unknown(
                        unknown_to_diag_message(reason))));
                }
            }
        }

        results
    }
}

/// Plan 33.3 Р¤.9: СЃРѕР±СЂР°С‚СЊ РІСЃРµ pure_view'С‹ РјРѕРґСѓР»СЏ РІ СЂРµРµСЃС‚СЂ.
pub(super) fn collect_pure_views(module: &Module) -> std::collections::HashMap<String, super::encode::PureViewSig> {
    use super::encode::PureViewSig;
    let mut out = std::collections::HashMap::new();
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        let TypeDeclKind::Effect(methods) = &td.kind else { continue };
        for m in methods {
            if !matches!(m.kind, EffectOpKind::PureView) { continue; }
            let return_sort = m.return_type.as_ref()
                .map(type_to_sort)
                .unwrap_or(SortRef::Int);
            let param_sorts: Vec<SortRef> = m.params.iter()
                .map(|p| type_to_sort(&p.ty)).collect();
            out.insert(m.name.clone(), PureViewSig {
                effect_name: td.name.clone(),
                arity: m.params.len(),
                return_sort,
                param_sorts,
            });
        }
    }
    out
}

/// Plan 33.4 D.0.2: СЃРѕР±СЂР°С‚СЊ РІСЃРµ #pure fn'С‹ РјРѕРґСѓР»СЏ РІ СЂРµРµСЃС‚СЂ РґР»СЏ encoder'Р°.
/// `inferred_pure` вЂ" РїСЂРµРґРІР°СЂРёС‚РµР»СЊРЅРѕ РІС‹С‡РёСЃР»РµРЅРЅС‹Р№ СЂРµР·СѓР»СЊС‚Р°С‚ SCC inference
/// (РїРµСЂРµРґР°С'С‚СЃСЏ СЃРЅР°СЂСѓР¶Рё С‡С‚РѕР±С‹ РЅРµ РїРµСЂРµСЃС‡РёС‚С‹РІР°С‚СЊ РЅР° РєР°Р¶РґСѓСЋ С„СѓРЅРєС†РёСЋ).
pub(super) fn collect_pure_fns(
    module: &Module,
    inferred_pure: &std::collections::HashSet<String>,
) -> std::collections::HashMap<String, encode::PureFnInfo> {

    let mut out = std::collections::HashMap::new();
    for item in &module.items {
        let Item::Fn(fd) = item else { continue };
        // Pure if explicitly annotated OR inferred via SCC (precomputed).
        let is_pure = matches!(fd.purity, Purity::Pure) || inferred_pure.contains(&fd.name);
        if !is_pure { continue; }
        let param_sorts = fd.params.iter().map(|p| type_to_sort(&p.ty)).collect();
        let return_sort = fd.return_type.as_ref().map(type_to_sort).unwrap_or(SortRef::Int);
        // Capture body for inlining: `=> expr` or empty-block-with-trailing.
        let body_expr = match &fd.body {
            FnBody::Expr(e) => Some(Box::new(e.clone())),
            FnBody::Block(b) if b.stmts.is_empty() => {
                b.trailing.as_ref().map(|e| Box::new(e.as_ref().clone()))
            }
            _ => None,
        };
        out.insert(fd.name.clone(), encode::PureFnInfo {
            param_names: fd.params.iter().map(|p| p.name.clone()).collect(),
            param_sorts,
            return_sort,
            body_expr,
        });
    }
    out
}

/// Р¤.3 (Plan 33.5): Tarjan SCC + purity inference.
///
/// РђР»РіРѕСЂРёС‚Рј:
/// 1. РџРѕСЃС‚СЂРѕРёС‚СЊ call-graph: РґР»СЏ РєР°Р¶РґРѕР№ Fn вЂ" РЅР°Р±РѕСЂ РІС‹Р·С‹РІР°РµРјС‹С… РёРјС'РЅ (РёР· body).
/// 2. Р--Р°РїСѓСЃС‚РёС‚СЊ Tarjan SCC.
/// 3. Topological order SCCs. Р"Р»СЏ РєР°Р¶РґРѕР№ SCC:
///    - Р•СЃР»Рё РІСЃРµ fn РІ SCC pure-eligible (РЅРµС‚ effects, РЅРµС‚ `with`, РЅРµС‚ IO,
///      РІСЃРµ РІС‹Р·РѕРІС‹ вЂ" С‚РѕР»СЊРєРѕ Рє СѓР¶Рµ-proven-pure РёР»Рё Рє fn С‚РѕР№ Р¶Рµ SCC) в†'
///      РїРѕРјРµС‚РёС‚СЊ РєР°Рє inferred-pure.
/// 4. РЇРІРЅРѕ `@effectful` fn в†' non-pure (РїРµСЂРµРѕРїСЂРµРґРµР»СЏРµС‚ inference).
///
/// Pure-eligibility (V1):
/// - FnBody::Expr РёР»Рё РїСЂРѕСЃС‚РѕР№ block Р±РµР· `with`/`interrupt`/IO-stmts.
/// - РЎРёРіРЅР°С‚СѓСЂР°: РЅРµС‚ implicit effects РїР°СЂР°РјРµС‚СЂРѕРІ (РЅРµС‚ `with E` handlers).
/// - Р'СЃРµ РІС‹Р·РѕРІС‹ РІ body в†' Рє already-pure РёР»Рё Рє fn С‚РѕР№ Р¶Рµ SCC.
/// - РќРµС‚ РІС‹Р·РѕРІРѕРІ Рє external fn (FnBody::External).
pub fn infer_pure_fns_scc(module: &Module) -> std::collections::HashSet<String> {
    use std::collections::{HashMap, HashSet};
    // РЁР°Рі 1: build call-graph.
    let mut fn_names: Vec<String> = Vec::new();
    let mut fn_body_map: HashMap<String, &FnBody> = HashMap::new();
    let mut fn_purity_explicit: HashMap<String, Purity> = HashMap::new();

    for item in &module.items {
        let Item::Fn(fd) = item else { continue };
        fn_names.push(fd.name.clone());
        fn_body_map.insert(fd.name.clone(), &fd.body);
        fn_purity_explicit.insert(fd.name.clone(), fd.purity);
    }

    // call-graph: fn_name в†' set of called fn_names (within module).
    let mut call_graph: HashMap<String, HashSet<String>> = HashMap::new();
    for name in &fn_names {
        let body = fn_body_map[name];
        let calls = collect_fn_calls_in_body(body, &fn_names);
        call_graph.insert(name.clone(), calls);
    }

    // РЁР°Рі 2: Tarjan SCC (iterative, С‡С‚РѕР±С‹ РЅРµ СѓРїРёСЂР°С‚СЊСЃСЏ РІ stack overflow).
    let sccs = tarjan_scc(&fn_names, &call_graph);

    // РЁР°Рі 3: topological order в†' РѕРїСЂРµРґРµР»СЏРµРј pure SCCs.
    // sccs СѓР¶Рµ РІ РѕР±СЂР°С‚РЅРѕРј С‚РѕРїРѕР»РѕРіРёС‡РµСЃРєРѕРј РїРѕСЂСЏРґРєРµ (Tarjan РІС‹РґР°С'С‚ SCC РІ
    // reverse topological order РІ СЃС‚Р°РЅРґР°СЂС‚РЅРѕР№ СЂРµР°Р»РёР·Р°С†РёРё).
    // РС‚РµСЂРёСЂСѓРµРј РѕС‚ С…РІРѕСЃС‚Р° Рє РіРѕР»РѕРІРµ С‡С‚РѕР±С‹ РёРґС‚Рё РѕС‚ Р»РёСЃС‚СЊРµРІ Рє РєРѕСЂРЅСЏРј.
    let mut proven_pure: HashSet<String> = HashSet::new();

    // РЎРЅР°С‡Р°Р»Р° РґРѕР±Р°РІРёРј СЏРІРЅРѕ Effectful'РЅС‹Рµ вЂ" РѕРЅРё non-pure РЅР°РІСЃРµРіРґР°.
    let explicitly_effectful: HashSet<String> = fn_purity_explicit.iter()
        .filter_map(|(name, p)| if matches!(p, Purity::Effectful) { Some(name.clone()) } else { None })
        .collect();

    for scc in &sccs {
        // РџСЂРѕРІРµСЂСЏРµРј pure-eligibility РІСЃРµР№ SCC.
        let eligible = scc.iter().all(|name| {
            // РЇРІРЅРѕ Effectful в†' non-pure.
            if explicitly_effectful.contains(name) { return false; }
            // External body в†' non-pure.
            if matches!(fn_body_map.get(name), Some(FnBody::External)) { return false; }
            // Р'СЃРµ РІС‹Р·РѕРІС‹ вЂ" РёР»Рё Рє proven_pure, РёР»Рё Рє fn РІ СЌС‚РѕР№ SCC.
            let empty_calls = HashSet::new();
            let calls = call_graph.get(name).unwrap_or(&empty_calls);
            let scc_set: HashSet<&String> = scc.iter().collect();
            calls.iter().all(|called| {
                proven_pure.contains(called) || scc_set.contains(called)
            }) &&
            // Body РЅРµ СЃРѕРґРµСЂР¶РёС‚ with/interrupt/effect calls.
            !body_has_effects(fn_body_map[name]) // Ф.3
        });

        if eligible {
            for name in scc {
                proven_pure.insert(name.clone());
            }
        }
    }

    proven_pure
}

/// Tarjan iterative SCC. Р'РѕР·РІСЂР°С‰Р°РµС‚ SCCs РІ РѕР±СЂР°С‚РЅРѕРј С‚РѕРїРѕР»РѕРіРёС‡РµСЃРєРѕРј РїРѕСЂСЏРґРєРµ.
fn tarjan_scc(
    nodes: &[String],
    graph: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> Vec<Vec<String>> {
    use std::collections::HashMap;

    let mut index_counter = 0usize;
    let mut stack: Vec<String> = Vec::new();
    let mut on_stack: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut index_map: HashMap<String, usize> = HashMap::new();
    let mut lowlink: HashMap<String, usize> = HashMap::new();
    let mut sccs: Vec<Vec<String>> = Vec::new();

    // Iterative DFS СЃ explicit stack frame.
    for start in nodes {
        if index_map.contains_key(start) { continue; }
        // DFS stack: (node, iterator over children, is_entry).
        let mut dfs_stack: Vec<(String, Vec<String>, usize)> = Vec::new();
        // init
        index_map.insert(start.clone(), index_counter);
        lowlink.insert(start.clone(), index_counter);
        index_counter += 1;
        stack.push(start.clone());
        on_stack.insert(start.clone());
        let children: Vec<String> = graph.get(start)
            .map(|s| s.iter().filter(|c| nodes.contains(c)).cloned().collect())
            .unwrap_or_default();
        dfs_stack.push((start.clone(), children, 0));

        'dfs: loop {
            let Some(frame) = dfs_stack.last_mut() else { break };
            let (node, children, child_idx) = frame;
            if *child_idx < children.len() {
                let child = children[*child_idx].clone();
                *child_idx += 1;
                if !index_map.contains_key(&child) {
                    // Recurse into child.
                    index_map.insert(child.clone(), index_counter);
                    lowlink.insert(child.clone(), index_counter);
                    index_counter += 1;
                    stack.push(child.clone());
                    on_stack.insert(child.clone());
                    let grandchildren: Vec<String> = graph.get(&child)
                        .map(|s| s.iter().filter(|c| nodes.contains(c)).cloned().collect())
                        .unwrap_or_default();
                    dfs_stack.push((child, grandchildren, 0));
                } else if on_stack.contains(&child) {
                    // Back edge.
                    let child_low = *index_map.get(&child).unwrap();
                    let node_low = lowlink.get_mut(node).unwrap();
                    if child_low < *node_low { *node_low = child_low; }
                }
            } else {
                // All children processed.
                let node = node.clone();
                dfs_stack.pop();
                if let Some(parent_frame) = dfs_stack.last_mut() {
                    let parent = &parent_frame.0;
                    let node_low = *lowlink.get(&node).unwrap();
                    let parent_low = lowlink.get_mut(parent).unwrap();
                    if node_low < *parent_low { *parent_low = node_low; }
                }
                // Check if node is root of SCC.
                if lowlink.get(&node) == index_map.get(&node) {
                    let mut scc = Vec::new();
                    loop {
                        let top = stack.pop().unwrap();
                        on_stack.remove(&top);
                        scc.push(top.clone());
                        if top == node { break; }
                    }
                    sccs.push(scc);
                }
            }
        }
    }

    sccs
}

/// Р¤.3: РїСЂРѕРІРµСЂРєР°, СЃРѕРґРµСЂР¶РёС‚ Р»Рё С‚РµР»Рѕ С„СѓРЅРєС†РёРё РІС‹Р·РѕРІС‹ Рє effects / with / IO.
/// V1: РёС‰РµС‚ `with`, `interrupt`, `ExprKind::With` РІ С‚РµР»Рµ.
fn body_has_effects(body: &FnBody) -> bool {
    match body {
        FnBody::External => true,
        FnBody::Expr(e) => expr_has_effects(e),
        FnBody::Block(b) => block_has_effects(b),
    }
}

fn block_has_effects(b: &Block) -> bool {
    b.stmts.iter().any(|s| stmt_has_effects(s))
        || b.trailing.as_ref().map_or(false, |e| expr_has_effects(e))
}

fn stmt_has_effects(s: &Stmt) -> bool {
    match s {
        Stmt::Expr(e) => expr_has_effects(e),
        Stmt::Let(ld) => expr_has_effects(&ld.value),
        Stmt::Assign { value, .. } => expr_has_effects(value),
        Stmt::Return { value: Some(v), .. } => expr_has_effects(v),
        Stmt::Throw { value, .. } => expr_has_effects(value),
        _ => false,
    }
}

fn expr_has_effects(e: &Expr) -> bool {
    match &e.kind {
        // with-blocks в†' effectful.
        ExprKind::With { .. } => true,
        // Interrupt в†' effectful.
        ExprKind::Interrupt(_) => true,
        // Spawn/Supervised в†' effectful.
        ExprKind::Spawn(_) | ExprKind::Supervised { .. } => true,
        // Recurse structurally.
        ExprKind::Binary { left, right, .. } => expr_has_effects(left) || expr_has_effects(right),
        ExprKind::Unary { operand, .. } => expr_has_effects(operand),
        ExprKind::Call { func, args, .. } => {
            expr_has_effects(func) || args.iter().any(|a| expr_has_effects(a.expr()))
        }
        ExprKind::If { cond, then, else_ } => {
            expr_has_effects(cond) || block_has_effects(then)
                || else_.as_ref().map_or(false, |eb| match eb {
                    ElseBranch::Block(b) => block_has_effects(b),
                    ElseBranch::If(ei) => expr_has_effects(ei),
                })
        }
        ExprKind::Block(b) => block_has_effects(b),
        ExprKind::While { cond, body, .. } => expr_has_effects(cond) || block_has_effects(body),
        ExprKind::For { iter, body, .. } => expr_has_effects(iter) || block_has_effects(body),
        ExprKind::Loop { body, .. } => block_has_effects(body),
        ExprKind::Match { scrutinee, arms } => {
            expr_has_effects(scrutinee) || arms.iter().any(|arm| match &arm.body {
                MatchArmBody::Expr(ae) => expr_has_effects(ae),
                MatchArmBody::Block(ab) => block_has_effects(ab),
            })
        }
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. }
        | ExprKind::As(obj, _) | ExprKind::Is(obj, _)
        | ExprKind::Try(obj) | ExprKind::Bang(obj) => expr_has_effects(obj),
        ExprKind::Coalesce(l, r) => expr_has_effects(l) || expr_has_effects(r),
        ExprKind::TupleLit(items) => items.iter().any(|i| expr_has_effects(i)),
        _ => false,
    }
}

/// Р¤.3: СЃРѕР±СЂР°С‚СЊ РІСЃРµ fn-calls РІ body, С„РёР»СЊС‚СЂСѓСЏ РїРѕ РёР·РІРµСЃС‚РЅС‹Рј РёРјРµРЅР°Рј РјРѕРґСѓР»СЏ.
fn collect_fn_calls_in_body(body: &FnBody, known_fns: &[String]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    match body {
        FnBody::Expr(e) => collect_fn_calls_in_expr(e, known_fns, &mut out),
        FnBody::Block(b) => collect_fn_calls_in_block(b, known_fns, &mut out),
        FnBody::External => {}
    }
    out
}

fn collect_fn_calls_in_block(b: &Block, known: &[String], out: &mut std::collections::HashSet<String>) {
    for s in &b.stmts {
        match s {
            Stmt::Expr(e) | Stmt::Return { value: Some(e), .. } | Stmt::Throw { value: e, .. } => {
                collect_fn_calls_in_expr(e, known, out);
            }
            Stmt::Let(ld) => collect_fn_calls_in_expr(&ld.value, known, out),
            Stmt::Assign { value, .. } => collect_fn_calls_in_expr(value, known, out),
            _ => {}
        }
    }
    if let Some(e) = &b.trailing { collect_fn_calls_in_expr(e, known, out); }
}

fn collect_fn_calls_in_expr(e: &Expr, known: &[String], out: &mut std::collections::HashSet<String>) {
    match &e.kind {
        ExprKind::Call { func, args, .. } => {
            // Direct function call: func = Ident("name").
            if let ExprKind::Ident(name) = &func.kind {
                if known.contains(name) {
                    out.insert(name.clone());
                }
            }
            collect_fn_calls_in_expr(func, known, out);
            for a in args { collect_fn_calls_in_expr(a.expr(), known, out); }
        }
        ExprKind::Binary { left, right, .. } => {
            collect_fn_calls_in_expr(left, known, out);
            collect_fn_calls_in_expr(right, known, out);
        }
        ExprKind::Unary { operand, .. } => collect_fn_calls_in_expr(operand, known, out),
        ExprKind::If { cond, then, else_ } => {
            collect_fn_calls_in_expr(cond, known, out);
            collect_fn_calls_in_block(then, known, out);
            match else_ {
                Some(ElseBranch::Block(b)) => collect_fn_calls_in_block(b, known, out),
                Some(ElseBranch::If(ei)) => collect_fn_calls_in_expr(ei, known, out),
                None => {}
            }
        }
        ExprKind::Block(b) => collect_fn_calls_in_block(b, known, out),
        ExprKind::While { cond, body, .. } => {
            collect_fn_calls_in_expr(cond, known, out);
            collect_fn_calls_in_block(body, known, out);
        }
        ExprKind::For { iter, body, .. } => {
            collect_fn_calls_in_expr(iter, known, out);
            collect_fn_calls_in_block(body, known, out);
        }
        ExprKind::Loop { body, .. } => collect_fn_calls_in_block(body, known, out),
        ExprKind::Match { scrutinee, arms } => {
            collect_fn_calls_in_expr(scrutinee, known, out);
            for arm in arms {
                match &arm.body {
                    MatchArmBody::Expr(ae) => collect_fn_calls_in_expr(ae, known, out),
                    MatchArmBody::Block(ab) => collect_fn_calls_in_block(ab, known, out),
                }
            }
        }
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. }
        | ExprKind::As(obj, _) | ExprKind::Is(obj, _)
        | ExprKind::Try(obj) | ExprKind::Bang(obj) => collect_fn_calls_in_expr(obj, known, out),
        ExprKind::Coalesce(l, r) => {
            collect_fn_calls_in_expr(l, known, out);
            collect_fn_calls_in_expr(r, known, out);
        }
        _ => {}
    }
}

/// Plan 33.4 D.0.4: РЅР°Р№С‚Рё РІСЃРµ СЃР°РјРѕ-СЂРµРєСѓСЂСЃРёРІРЅС‹Рµ РІС‹Р·РѕРІС‹ РІ С‚РµР»Рµ С„СѓРЅРєС†РёРё.
/// Р'РѕР·РІСЂР°С‰Р°РµС‚ Vec<(call_span, Vec<arg_expr>)>.
fn find_recursive_calls_in_body(body: &FnBody, fn_name: &str) -> Vec<(Span, Vec<Expr>)> {
    let mut results = Vec::new();
    match body {
        FnBody::Expr(e) => find_recursive_calls_in_expr(e, fn_name, &mut results),
        FnBody::Block(b) => find_recursive_calls_in_block(b, fn_name, &mut results),
        FnBody::External => {}
    }
    results
}

fn find_recursive_calls_in_block(b: &Block, fn_name: &str, out: &mut Vec<(Span, Vec<Expr>)>) {
    for s in &b.stmts {
        find_recursive_calls_in_stmt(s, fn_name, out);
    }
    if let Some(e) = &b.trailing {
        find_recursive_calls_in_expr(e, fn_name, out);
    }
}

fn find_recursive_calls_in_stmt(s: &Stmt, fn_name: &str, out: &mut Vec<(Span, Vec<Expr>)>) {
    match s {
        Stmt::Let(ld) => {
            find_recursive_calls_in_expr(&ld.value, fn_name, out);
        }
        Stmt::Expr(e) => find_recursive_calls_in_expr(e, fn_name, out),
        Stmt::Return { value: Some(v), .. } => find_recursive_calls_in_expr(v, fn_name, out),
        Stmt::Return { value: None, .. } => {}
        Stmt::Assign { value, .. } => find_recursive_calls_in_expr(value, fn_name, out),
        Stmt::Throw { value, .. } => find_recursive_calls_in_expr(value, fn_name, out),
        _ => {}
    }
}

fn find_recursive_calls_in_expr(e: &Expr, fn_name: &str, out: &mut Vec<(Span, Vec<Expr>)>) {
    match &e.kind {
        ExprKind::Call { func, args, trailing } => {
            // Check if this is a self-recursive call.
            if trailing.is_none() {
                if let ExprKind::Ident(name) = &func.kind {
                    if name == fn_name {
                        let arg_exprs: Vec<Expr> = args.iter()
                            .map(|a| a.expr().clone())
                            .collect();
                        out.push((e.span, arg_exprs));
                    }
                }
            }
            // Recurse into func and all args regardless.
            find_recursive_calls_in_expr(func, fn_name, out);
            for a in args {
                find_recursive_calls_in_expr(a.expr(), fn_name, out);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            find_recursive_calls_in_expr(left, fn_name, out);
            find_recursive_calls_in_expr(right, fn_name, out);
        }
        ExprKind::Unary { operand, .. } => {
            find_recursive_calls_in_expr(operand, fn_name, out);
        }
        ExprKind::If { cond, then, else_ } => {
            find_recursive_calls_in_expr(cond, fn_name, out);
            find_recursive_calls_in_block(then, fn_name, out);
            match else_ {
                Some(ElseBranch::Block(b)) => find_recursive_calls_in_block(b, fn_name, out),
                Some(ElseBranch::If(ei)) => find_recursive_calls_in_expr(ei, fn_name, out),
                None => {}
            }
        }
        ExprKind::Block(b) => find_recursive_calls_in_block(b, fn_name, out),
        ExprKind::Match { scrutinee, arms } => {
            find_recursive_calls_in_expr(scrutinee, fn_name, out);
            for arm in arms {
                match &arm.body {
                    MatchArmBody::Expr(ae) => find_recursive_calls_in_expr(ae, fn_name, out),
                    MatchArmBody::Block(ab) => find_recursive_calls_in_block(ab, fn_name, out),
                }
            }
        }
        _ => {}
    }
}

/// Ф.4.2: верифицировать calc-блоки в теле fn.
/// Для каждого смежного шага (e_i rel e_{i+1}): try_prove(e_i rel e_{i+1}).
/// Доказанные шаги ассертируются в backend как аксиомы (транзитивно усиливают SMT-scope).
fn verify_calc_stmts_in_body(
    body: &FnBody,
    ctx: &encode::EncodeCtx,
    backend: &mut dyn SmtBackend,
) -> Vec<(Span, VerifyResult)> {
    let mut results = Vec::new();
    let calcs = collect_calc_stmts_in_body(body);
    for (steps, _calc_span) in calcs {
        if steps.len() < 2 { continue; }
        for i in 0..steps.len() - 1 {
            let step_a = &steps[i];
            let step_b = &steps[i + 1];
            let rel = match step_b.rel {
                Some(r) => r,
                None => continue, // первый шаг (нет rel)
            };
            let enc_a = match encode::encode_expr_with_ctx(&step_a.expr, ctx) {
                Ok(t) => t,
                Err(_) => {
                    results.push((step_b.span, VerifyResult::EncodingFailed(
                        format!("calc step {} cannot be encoded", i))));
                    continue;
                }
            };
            let enc_b = match encode::encode_expr_with_ctx(&step_b.expr, ctx) {
                Ok(t) => t,
                Err(_) => {
                    results.push((step_b.span, VerifyResult::EncodingFailed(
                        format!("calc step {} cannot be encoded", i + 1))));
                    continue;
                }
            };
            let smt_op = rel.to_smt_op().to_string();
            let goal = SmtTerm::App(smt_op.clone(), vec![enc_a.clone(), enc_b.clone()]);
            match try_prove(backend, goal.clone()) {
                SatResult::Unsat(_) => {
                    // Доказано: добавляем как аксиому в scope.
                    backend.assert(Assertion {
                        formula: goal,
                        label: Some(format!("calc_step@{}:{}", i, step_b.span.start)),
                    });
                    results.push((step_b.span, VerifyResult::Proven));
                }
                SatResult::Sat(model) => {
                    let cex = format_counterexample(&model);
                    results.push((step_b.span, VerifyResult::Disproved(model,
                        format!("calc step {} {} {} failed: {}", i, smt_op, i + 1, cex))));
                }
                SatResult::Unknown(reason) => {
                    results.push((step_b.span, VerifyResult::Unknown(
                        format!("calc step: {}", unknown_to_diag_message(reason)))));
                }
            }
        }
    }
    results
}

/// Ф.4.2: собрать все `calc { ... }` из тела функции.
fn collect_calc_stmts_in_body(body: &FnBody) -> Vec<(Vec<CalcStep>, Span)> {
    let mut out = Vec::new();
    match body {
        FnBody::Block(b) => collect_calc_in_block(b, &mut out),
        FnBody::Expr(_) | FnBody::External => {}
    }
    out
}

fn collect_calc_in_block(b: &Block, out: &mut Vec<(Vec<CalcStep>, Span)>) {
    for s in &b.stmts {
        if let Stmt::Calc { steps, span } = s {
            out.push((steps.clone(), *span));
        }
    }
}

/// Ф.4.1: блок "ghost-only" -- все стейтменты ghost (`apply`).
/// Такой блок при верификации трактуется как trailing-only (apply стираются).
fn block_has_only_ghost_stmts(b: &Block) -> bool {
    b.stmts.iter().all(|s| matches!(s, Stmt::Apply { .. } | Stmt::Calc { .. }))
}

/// Ф.4.1: собрать все `apply lemma(args)` из тела функции.
/// Возвращает Vec<(lemma_name, args, span)>.
fn collect_apply_stmts_in_body(body: &FnBody) -> Vec<(String, Vec<Expr>, Span)> {
    let mut out = Vec::new();
    match body {
        FnBody::Block(b) => collect_apply_in_block(b, &mut out),
        FnBody::Expr(_) | FnBody::External => {}
    }
    out
}

fn collect_apply_in_block(b: &Block, out: &mut Vec<(String, Vec<Expr>, Span)>) {
    for s in &b.stmts {
        collect_apply_in_stmt(s, out);
    }
}

fn collect_apply_in_stmt(s: &Stmt, out: &mut Vec<(String, Vec<Expr>, Span)>) {
    match s {
        Stmt::Apply { lemma, args, span } => {
            out.push((lemma.clone(), args.clone(), *span));
        }
        Stmt::Let(d) => collect_apply_in_expr(&d.value, out),
        Stmt::Expr(e) => collect_apply_in_expr(e, out),
        _ => {}
    }
}

fn collect_apply_in_expr(e: &Expr, out: &mut Vec<(String, Vec<Expr>, Span)>) {
    use crate::ast::ElseBranch;
    match &e.kind {
        ExprKind::Block(b) => collect_apply_in_block(b, out),
        ExprKind::If { cond, then, else_: Some(el), .. } => {
            collect_apply_in_expr(cond, out);
            collect_apply_in_block(then, out);
            match el {
                ElseBranch::Block(b) => collect_apply_in_block(b, out),
                ElseBranch::If(ie) => collect_apply_in_expr(ie, out),
            }
        }
        ExprKind::If { cond, then, else_: None, .. } => {
            collect_apply_in_expr(cond, out);
            collect_apply_in_block(then, out);
        }
        _ => {}
    }
}

/// Ф.4.1: найти лемму в модуле и вернуть её ensures-клаузы как
/// Vec<(param_names, ensures_expr)>. None -- лемма не найдена.
fn find_lemma_ensures(module: &Module, name: &str) -> Option<Vec<(Vec<String>, Expr)>> {
    for item in &module.items {
        let Item::Lemma(ld) = item else { continue };
        if ld.name != name { continue; }
        let param_names: Vec<String> = ld.params.iter().map(|p| p.name.clone()).collect();
        let ensures: Vec<(Vec<String>, Expr)> = ld.contracts.iter()
            .filter(|c| matches!(c.kind, ContractKind::Ensures))
            .map(|c| (param_names.clone(), c.expr.clone()))
            .collect();
        return Some(ensures);
    }
    None
}

/// Plan 33.4 D.0.3: СЃРѕР±СЂР°С‚СЊ РІСЃРµ loop invariant'С‹ РёР· С‚РµР»Р° С„СѓРЅРєС†РёРё.
/// Р'РѕР·РІСЂР°С‰Р°РµС‚ Vec<(Span, Expr)> вЂ" span С†РёРєР»Р° + invariant expression.
fn collect_loop_invariants_in_body(body: &FnBody) -> Vec<(Span, Expr)> {
    let mut out = Vec::new();
    match body {
        FnBody::Expr(e) => collect_loop_invariants_in_expr(e, &mut out),
        FnBody::Block(b) => collect_loop_invariants_in_block(b, &mut out),
        FnBody::External => {}
    }
    out
}

fn collect_loop_invariants_in_block(b: &Block, out: &mut Vec<(Span, Expr)>) {
    for s in &b.stmts {
        collect_loop_invariants_in_stmt(s, out);
    }
    if let Some(e) = &b.trailing {
        collect_loop_invariants_in_expr(e, out);
    }
}

fn collect_loop_invariants_in_stmt(s: &Stmt, out: &mut Vec<(Span, Expr)>) {
    match s {
        Stmt::Expr(e) => collect_loop_invariants_in_expr(e, out),
        Stmt::Let(ld) => collect_loop_invariants_in_expr(&ld.value, out),
        Stmt::Return { value: Some(v), .. } => collect_loop_invariants_in_expr(v, out),
        Stmt::Assign { value, .. } => collect_loop_invariants_in_expr(value, out),
        _ => {}
    }
}

fn collect_loop_invariants_in_expr(e: &Expr, out: &mut Vec<(Span, Expr)>) {
    match &e.kind {
        ExprKind::While { body, invariants, .. }
        | ExprKind::For { body, invariants, .. }
        | ExprKind::Loop { body, invariants, .. } => {
            for inv in invariants {
                out.push((e.span, inv.clone()));
            }
            collect_loop_invariants_in_block(body, out);
        }
        ExprKind::WhileLet { body, invariants, .. } => {
            for inv in invariants {
                out.push((e.span, inv.clone()));
            }
            collect_loop_invariants_in_block(body, out);
        }
        ExprKind::If { cond, then, else_ } => {
            collect_loop_invariants_in_expr(cond, out);
            collect_loop_invariants_in_block(then, out);
            match else_ {
                Some(ElseBranch::Block(b)) => collect_loop_invariants_in_block(b, out),
                Some(ElseBranch::If(ei)) => collect_loop_invariants_in_expr(ei, out),
                None => {}
            }
        }
        ExprKind::Block(b) => collect_loop_invariants_in_block(b, out),
        ExprKind::Binary { left, right, .. } => {
            collect_loop_invariants_in_expr(left, out);
            collect_loop_invariants_in_expr(right, out);
        }
        ExprKind::Unary { operand, .. } => collect_loop_invariants_in_expr(operand, out),
        ExprKind::Match { scrutinee, arms } => {
            collect_loop_invariants_in_expr(scrutinee, out);
            for arm in arms {
                match &arm.body {
                    MatchArmBody::Expr(ae) => collect_loop_invariants_in_expr(ae, out),
                    MatchArmBody::Block(ab) => collect_loop_invariants_in_block(ab, out),
                }
            }
        }
        _ => {}
    }
}

/// Р¤.1.3 (Plan 33.5): СЃРѕР±СЂР°С‚СЊ РІСЃРµ `decreases` claus'С‹ РёР· С†РёРєР»РѕРІ.
///
/// Р'РѕР·РІСЂР°С‰Р°РµС‚ Vec<(Span, decreases_expr, assignments)> РіРґРµ:
/// - `decreases_expr` вЂ" РІС‹СЂР°Р¶РµРЅРёРµ РјРµСЂС‹ (РґРѕР»Р¶РЅРѕ СѓР±С‹РІР°С‚СЊ).
/// - `assignments` вЂ" Vec<(var_name, delta)> вЂ" РѕР±РЅР°СЂСѓР¶РµРЅРЅС‹Рµ `var = var В± delta`
///   РІ С‚РµР»Рµ С†РёРєР»Р° (V1: over-approximate, С‚РѕР»СЊРєРѕ straight-line assignment stmts).
///   `delta > 0` РѕР·РЅР°С‡Р°РµС‚ `var = var - delta` (РјРµСЂР° var СѓР±С‹РІР°РµС‚),
///   `delta < 0` вЂ" `var = var + delta` (РјРµСЂР° СЂР°СЃС‚С'С‚, delta negative в†' decrement positive).
fn collect_loop_decreases_in_body(body: &FnBody) -> Vec<(Span, Expr, Vec<(String, i64)>)> {
    let mut out = Vec::new();
    match body {
        FnBody::Expr(e) => collect_loop_decreases_in_expr(e, &mut out),
        FnBody::Block(b) => collect_loop_decreases_in_block(b, &mut out),
        FnBody::External => {}
    }
    out
}

fn collect_loop_decreases_in_block(b: &Block, out: &mut Vec<(Span, Expr, Vec<(String, i64)>)>) {
    for s in &b.stmts {
        if let Stmt::Expr(e) = s { collect_loop_decreases_in_expr(e, out); }
    }
    if let Some(e) = &b.trailing { collect_loop_decreases_in_expr(e, out); }
}

fn collect_loop_decreases_in_expr(e: &Expr, out: &mut Vec<(Span, Expr, Vec<(String, i64)>)>) {
    match &e.kind {
        ExprKind::While { body, decreases: Some(dec), .. }
        | ExprKind::Loop { body, decreases: Some(dec), .. } => {
            let assignments = extract_counter_assignments(body);
            out.push((e.span, *dec.clone(), assignments));
            collect_loop_decreases_in_block(body, out);
        }
        ExprKind::While { body, decreases: None, .. }
        | ExprKind::Loop { body, decreases: None, .. } => {
            collect_loop_decreases_in_block(body, out);
        }
        ExprKind::For { body, decreases: Some(dec), .. } => {
            let assignments = extract_counter_assignments(body);
            out.push((e.span, *dec.clone(), assignments));
            collect_loop_decreases_in_block(body, out);
        }
        ExprKind::For { body, decreases: None, .. } => {
            collect_loop_decreases_in_block(body, out);
        }
        ExprKind::WhileLet { body, decreases: Some(dec), .. } => {
            let assignments = extract_counter_assignments(body);
            out.push((e.span, *dec.clone(), assignments));
            collect_loop_decreases_in_block(body, out);
        }
        ExprKind::WhileLet { body, decreases: None, .. } => {
            collect_loop_decreases_in_block(body, out);
        }
        ExprKind::Block(b) => collect_loop_decreases_in_block(b, out),
        ExprKind::If { then, else_, .. } => {
            collect_loop_decreases_in_block(then, out);
            match else_ {
                Some(ElseBranch::Block(b)) => collect_loop_decreases_in_block(b, out),
                Some(ElseBranch::If(ei)) => collect_loop_decreases_in_expr(ei, out),
                None => {}
            }
        }
        _ => {}
    }
}

/// V1: РёР·РІР»РµС‡СЊ РїСЂРѕСЃС‚С‹Рµ counter-decrement assignments РёР· С‚РµР»Р° С†РёРєР»Р°.
///
/// РџР°С‚С‚РµСЂРЅС‹:
/// - `x = x - k` (AssignOp::Assign + BinOp::Sub) в†' delta = k
/// - `x -= k`    (AssignOp::Sub)                  в†' delta = k
/// - `x = x + k` where k < 0 в†' delta = -k (СЂРµРґРєРёР№)
/// Р'РѕР·РІСЂР°С‰Р°РµРј (var_name, delta) РіРґРµ delta > 0 РѕР·РЅР°С‡Р°РµС‚ СѓР±С‹РІР°РЅРёРµ РјРµСЂС‹.
fn extract_counter_assignments(body: &Block) -> Vec<(String, i64)> {
    let mut result = Vec::new();
    for s in &body.stmts {
        let Stmt::Assign { target, op: assign_op, value, .. } = s else { continue };
        let ExprKind::Ident(var_name) = &target.kind else { continue };
        let delta: i64 = match assign_op {
            // x -= k  в†'  delta = k (positive means decreasing)
            AssignOp::Sub => {
                let ExprKind::IntLit(k) = &value.kind else { continue };
                *k
            }
            // x += k  в†'  delta = -k  (positive k в†' increasing, negative delta = skip)
            AssignOp::Add => {
                let ExprKind::IntLit(k) = &value.kind else { continue };
                -*k
            }
            // x = x В± k
            AssignOp::Assign => {
                let ExprKind::Binary { left, op: bin_op, right } = &value.kind else { continue };
                let ExprKind::Ident(lvar) = &left.kind else { continue };
                if lvar != var_name { continue; }
                let ExprKind::IntLit(k) = &right.kind else { continue };
                match bin_op {
                    BinOp::Sub => *k,
                    BinOp::Add => -*k,
                    _ => continue,
                }
            }
            _ => continue,
        };
        if delta > 0 {
            result.push((var_name.clone(), delta));
        }
    }
    result
}

/// Р¤.2 (Plan 33.5): РґР°РЅРЅС‹Рµ РѕРґРЅРѕРіРѕ С†РёРєР»Р° СЃ invariants РґР»СЏ preservation check.
struct LoopPreservationTarget {
    span: Span,
    invariants: Vec<Expr>,
    cond: Option<Expr>,           // None РґР»СЏ `loop { }` (СѓСЃР»РѕРІРёРµ = true)
    body_assignments: Vec<(String, Expr)>, // (var_name, value_expr) РёР· body stmts
    havoc_vars: Vec<String>,      // vars РјСѓС‚РёСЂСѓРµРјС‹Рµ РІ С‚РµР»Рµ
}

/// Р¤.2: СЃРѕР±СЂР°С‚СЊ РІСЃРµ while/loop СЃ invariants РґР»СЏ havoc+preservation.
fn collect_loop_preservation_targets(body: &FnBody) -> Vec<LoopPreservationTarget> {
    let mut out = Vec::new();
    match body {
        FnBody::Expr(e) => collect_loop_preservation_in_expr(e, &mut out),
        FnBody::Block(b) => collect_loop_preservation_in_block(b, &mut out),
        FnBody::External => {}
    }
    out
}

fn collect_loop_preservation_in_block(b: &Block, out: &mut Vec<LoopPreservationTarget>) {
    for s in &b.stmts {
        if let Stmt::Expr(e) = s { collect_loop_preservation_in_expr(e, out); }
    }
    if let Some(e) = &b.trailing { collect_loop_preservation_in_expr(e, out); }
}

fn collect_loop_preservation_in_expr(e: &Expr, out: &mut Vec<LoopPreservationTarget>) {
    match &e.kind {
        ExprKind::While { cond, body, invariants, .. } if !invariants.is_empty() => {
            let assignments = extract_body_assignments(body);
            let havoc_vars: Vec<String> = assignments.iter().map(|(n, _)| n.clone()).collect();
            out.push(LoopPreservationTarget {
                span: e.span,
                invariants: invariants.clone(),
                cond: Some(*cond.clone()),
                body_assignments: assignments,
                havoc_vars,
            });
            collect_loop_preservation_in_block(body, out);
        }
        ExprKind::Loop { body, invariants, .. } if !invariants.is_empty() => {
            let assignments = extract_body_assignments(body);
            let havoc_vars: Vec<String> = assignments.iter().map(|(n, _)| n.clone()).collect();
            out.push(LoopPreservationTarget {
                span: e.span,
                invariants: invariants.clone(),
                cond: None,
                body_assignments: assignments,
                havoc_vars,
            });
            collect_loop_preservation_in_block(body, out);
        }
        ExprKind::While { body, .. } | ExprKind::Loop { body, .. } => {
            collect_loop_preservation_in_block(body, out);
        }
        ExprKind::For { body, invariants, .. } if !invariants.is_empty() => {
            // For-loops: treat body as is, no cond (iter is opaque in V1).
            let assignments = extract_body_assignments(body);
            let havoc_vars: Vec<String> = assignments.iter().map(|(n, _)| n.clone()).collect();
            out.push(LoopPreservationTarget {
                span: e.span,
                invariants: invariants.clone(),
                cond: None,
                body_assignments: assignments,
                havoc_vars,
            });
            collect_loop_preservation_in_block(body, out);
        }
        ExprKind::For { body, .. } => collect_loop_preservation_in_block(body, out),
        ExprKind::Block(b) => collect_loop_preservation_in_block(b, out),
        ExprKind::If { then, else_, .. } => {
            collect_loop_preservation_in_block(then, out);
            match else_ {
                Some(ElseBranch::Block(b)) => collect_loop_preservation_in_block(b, out),
                Some(ElseBranch::If(ei)) => collect_loop_preservation_in_expr(ei, out),
                None => {}
            }
        }
        _ => {}
    }
}

/// V1: РёР·РІР»РµС‡СЊ РїСЂСЏРјС‹Рµ assignments `Assign { target: Ident(x), op: Assign, value: e }`
/// РёР· С‚РµР»Р° Р±Р»РѕРєР° (С‚РѕР»СЊРєРѕ РїРµСЂРІС‹Р№ СѓСЂРѕРІРµРЅСЊ stmts). Compound `+=/-=` С‚РѕР¶Рµ СЃРѕР±РёСЂР°РµРј.
fn extract_body_assignments(body: &Block) -> Vec<(String, Expr)> {
    let mut result = Vec::new();
    for s in &body.stmts {
        let Stmt::Assign { target, op: assign_op, value, .. } = s else { continue };
        let ExprKind::Ident(var_name) = &target.kind else { continue };
        let synthetic_value: Expr = match assign_op {
            AssignOp::Assign => value.clone(),
            // x += k  в†'  synthetic: x + k
            AssignOp::Add => Expr {
                kind: ExprKind::Binary {
                    left: Box::new(target.clone()),
                    op: BinOp::Add,
                    right: Box::new(value.clone()),
                },
                span: value.span,
            },
            // x -= k  в†'  synthetic: x - k
            AssignOp::Sub => Expr {
                kind: ExprKind::Binary {
                    left: Box::new(target.clone()),
                    op: BinOp::Sub,
                    right: Box::new(value.clone()),
                },
                span: value.span,
            },
            _ => continue, // Mul/Div вЂ" skip in V1
        };
        result.push((var_name.clone(), synthetic_value));
    }
    result
}

/// Р¤.2: verify invariant preservation РґР»СЏ РѕРґРЅРѕРіРѕ С†РёРєР»Р°.
///
/// РђР»РіРѕСЂРёС‚Рј:
/// 1. Р"Р»СЏ РєР°Р¶РґРѕР№ havoc var вЂ" declare fresh `_havoc_<var>` РІ backend.
/// 2. push() scope.
/// 3. Assume invariants РЅР° havoc state (substitute var в†' _havoc_var).
/// 4. Assume cond РЅР° havoc state (РµСЃР»Рё РµСЃС‚СЊ).
/// 5. Encode body assignments: РґР»СЏ РєР°Р¶РґРѕРіРѕ `var = rhs` в†' compute `rhs` РЅР° havoc state.
/// 6. Assert invariants РЅР° post state (substitute var в†' post_val).
/// 7. check_sat (goal = negation of invariant в†' UNSAT = preserved).
/// 8. pop() scope.
fn verify_loop_preservation(
    lp: &LoopPreservationTarget,
    ctx: &encode::EncodeCtx<'_>,
    backend: &mut dyn SmtBackend,
) -> Vec<(Span, VerifyResult)> {
    let mut results = Vec::new();

    // Step 1: declare fresh havoc vars.
    let mut havoc_map: std::collections::HashMap<String, SmtTerm> = std::collections::HashMap::new();
    for var in &lp.havoc_vars {
        let havoc_name = format!("_havoc_{}", var);
        backend.declare_var(&havoc_name, SortRef::Int);
        havoc_map.insert(var.clone(), SmtTerm::Var(havoc_name));
    }

    // Helper: substitute havoc vars in a SmtTerm.
    let substitute_havoc = |mut t: SmtTerm| -> SmtTerm {
        for (var, havoc_var) in &havoc_map {
            t = t.substitute(var, havoc_var);
        }
        t
    };

    // Step 2: push scope.
    backend.push();

    // Step 3: assume invariants on havoc state.
    let mut inv_terms_havoc: Vec<SmtTerm> = Vec::new();
    for inv in &lp.invariants {
        match encode::encode_expr_with_ctx(inv, ctx) {
            Ok(inv_t) => {
                let inv_havoc = substitute_havoc(inv_t);
                inv_terms_havoc.push(inv_havoc.clone());
                // Assume (not(not inv)) = assume inv.
                backend.assert(Assertion {
                    formula: inv_havoc,
                    label: Some("inv_pre_havoc".into()),
                });
            }
            Err(_) => {
                // Invariant non-encodable в†' skip preservation for this loop.
                backend.pop();
                return results;
            }
        }
    }

    // Step 4: assume cond on havoc state.
    if let Some(cond_expr) = &lp.cond {
        if let Ok(cond_t) = encode::encode_expr_with_ctx(cond_expr, ctx) {
            let cond_havoc = substitute_havoc(cond_t);
            backend.assert(Assertion { formula: cond_havoc, label: Some("loop_cond_havoc".into()) });
        }
    }

    // Step 5: compute post-state for each assigned var.
    let mut post_map: std::collections::HashMap<String, SmtTerm> = std::collections::HashMap::new();
    for (var, rhs_expr) in &lp.body_assignments {
        match encode::encode_expr_with_ctx(rhs_expr, ctx) {
            Ok(rhs_t) => {
                // rhs С‡РёС‚Р°РµС‚ РёР· havoc state.
                let rhs_havoc = substitute_havoc(rhs_t);
                post_map.insert(var.clone(), rhs_havoc);
            }
            Err(_) => {
                // Cannot encode rhs в†' fall back, no preservation check.
                backend.pop();
                return results;
            }
        }
    }

    // Helper: substitute post vars in a SmtTerm (post-state).
    let substitute_post = |t: SmtTerm| -> SmtTerm {
        let mut result = t;
        // Non-assigned vars: havoc (still in havoc state post-loop).
        for (var, havoc_var) in &havoc_map {
            if !post_map.contains_key(var) {
                result = result.substitute(var, havoc_var);
            }
        }
        // Assigned vars: use post value.
        for (var, post_val) in &post_map {
            result = result.substitute(var, post_val);
        }
        result
    };

    // Step 6 + 7: for each invariant, try_prove(inv_post).
    // try_prove asserts NOT(goal) then checks SAT:
    //   UNSAT = goal proven (invariant holds after body).
    //   SAT   = counterexample (invariant NOT preserved).
    for inv in &lp.invariants {
        match encode::encode_expr_with_ctx(inv, ctx) {
            Ok(inv_t) => {
                let inv_post = substitute_post(inv_t);
                match try_prove(backend, inv_post) {
                    SatResult::Unsat(_) => {
                        results.push((lp.span, VerifyResult::Proven));
                    }
                    SatResult::Sat(model) => {
                        let cex = format_counterexample(&model);
                        results.push((lp.span, VerifyResult::Disproved(model,
                            format!("loop invariant not preserved: {}", cex))));
                    }
                    SatResult::Unknown(reason) => {
                        results.push((lp.span, VerifyResult::Unknown(
                            format!("loop invariant preservation: {}", unknown_to_diag_message(reason)))));
                    }
                }
            }
            Err(_) => {} // non-encodable invariant в†' skip
        }
    }

    // Step 8: pop scope.
    backend.pop();

    results
}

/// Plan 33.3 Р¤.9 / Plan 33.4 P1-5: РјРµС‚Р°РґР°РЅРЅС‹Рµ РѕРґРЅРѕРіРѕ axiom'Р° СЃ СЂРµРµСЃС‚СЂР°РјРё РґР»СЏ encoding.
pub(super) struct AxiomInfo<'a> {
    pub(super) effect_name: String,
    pub(super) axiom_name: String,
    pub(super) binders: &'a [crate::ast::BinderDef],
    pub(super) formula: &'a crate::ast::Expr,
    pub(super) is_generic: bool,
}

/// Plan 33.3 Р¤.9: СЃРѕР±СЂР°С‚СЊ РІСЃРµ axiom'С‹ РјРѕРґСѓР»СЏ.
fn collect_axioms(module: &Module) -> Vec<AxiomInfo<'_>> {
    let mut out = Vec::new();
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        if !matches!(td.kind, TypeDeclKind::Effect(_)) { continue }
        for ax in &td.axioms {
            out.push(AxiomInfo {
                effect_name: td.name.clone(),
                axiom_name: ax.name.clone(),
                binders: ax.binders.as_slice(),
                formula: &ax.formula,
                is_generic: !ax.generics.is_empty(),
            });
        }
    }
    out
}

/// Plan 33.3 Р¤.9: encode axiom РІ SMT-Forall.
///
/// Р'РёРЅРґРµСЂС‹ РїСЂРёРѕР±СЂРµС‚Р°СЋС‚ sort РёР· РџР•Р Р'РћР"Рћ pure_view-РІС‹Р·РѕРІР° РІ С„РѕСЂРјСѓР»Рµ,
/// РіРґРµ РѕРЅРё РёСЃРїРѕР»СЊР·СѓСЋС‚СЃСЏ РєР°Рє Р°СЂРіСѓРјРµРЅС‚. Р­С‚Рѕ СЌРІСЂРёСЃС‚РёРєР° V1; СЏРІРЅС‹Рµ type
/// ascriptions РІ binders вЂ" future work.
/// РљРѕРЅРІРµСЂС‚РёСЂСѓРµС‚ TypeRef РІ SortRef РґР»СЏ SMT (V1: int/bool/str в†' СЃРѕРѕС‚РІРµС‚СЃС‚РІСѓСЋС‰РёР№ sort,
/// РѕСЃС‚Р°Р»СЊРЅРѕРµ в†' Int РєР°Рє fallback).
fn type_ref_to_sort(ty: &crate::ast::TypeRef) -> SortRef {
    if let crate::ast::TypeRef::Named { path, .. } = ty {
        if let Some(name) = path.last() {
            return match name.as_str() {
                "int" | "i32" | "i64" | "u32" | "u64" | "usize" => SortRef::Int,
                "bool" => SortRef::Bool,
                "str" => SortRef::Str,
                _ => SortRef::Int,
            };
        }
    }
    SortRef::Int
}

pub(super) fn encode_axiom(
    ax: &AxiomInfo,
    pure_views: &std::collections::HashMap<String, super::encode::PureViewSig>,
) -> Option<SmtTerm> {
    // Generic axioms вЂ" V2; РїРѕРєР° РЅРµ РїРѕРґРґРµСЂР¶РёРІР°СЋС‚СЃСЏ РІ SMT encoding.
    if ax.is_generic {
        return None;
    }
    // Plan 33.4 P1-5: binders С‡РµСЂРµР· BinderDef.
    let binder_names: Vec<String> = ax.binders.iter().map(|bd| bd.name.clone()).collect();
    let mut binder_sorts: std::collections::HashMap<String, SortRef> = std::collections::HashMap::new();
    // Р•СЃР»Рё Сѓ binder СЏРІРЅС‹Р№ С‚РёРї (Typed) вЂ" РёСЃРїРѕР»СЊР·СѓРµРј РµРіРѕ; Generic/Untyped вЂ" РІС‹РІРѕРґРёРј РёР· usage.
    for bd in ax.binders {
        if let crate::ast::BinderType::Typed(ty) = &bd.kind {
            let sort = type_ref_to_sort(ty);
            binder_sorts.insert(bd.name.clone(), sort);
        }
        // Generic Рё Untyped вЂ" РѕСЃС‚Р°РІР»СЏРµРј РґР»СЏ infer_binder_sorts.
    }
    infer_binder_sorts(ax.formula, &binder_names, pure_views, &mut binder_sorts);
    // Encode body.
    static EMPTY_FNS: std::sync::OnceLock<std::collections::HashMap<String, super::encode::PureFnInfo>> = std::sync::OnceLock::new();
    let empty_fns = EMPTY_FNS.get_or_init(std::collections::HashMap::new);
    let ctx = super::encode::EncodeCtx { pure_views, pure_fns: empty_fns, var_sorts: std::collections::HashMap::new() };
    let body = super::encode::encode_expr_with_ctx(ax.formula, &ctx).ok()?;
    // Build binders Vec вЂ" СЏРІРЅС‹Р№ РёР»Рё inferred sort, default Int.
    let binders: Vec<(String, SortRef)> = binder_names.iter()
        .map(|n| (n.clone(), binder_sorts.remove(n).unwrap_or(SortRef::Int)))
        .collect();
    if binders.is_empty() {
        Some(body)
    } else {
        Some(SmtTerm::Forall(binders, vec![], Box::new(body)))
    }
}

/// Walks `formula` Рё РґР»СЏ РєР°Р¶РґРѕР№ СЃСЃС‹Р»РєРё РЅР° binder РІ pure_view-arg
/// Р·Р°РїРёСЃС‹РІР°РµС‚ sort РїР°СЂР°РјРµС‚СЂР° РІ `out`.
pub(super) fn infer_binder_sorts(
    e: &crate::ast::Expr,
    binders: &[String],
    pure_views: &std::collections::HashMap<String, super::encode::PureViewSig>,
    out: &mut std::collections::HashMap<String, SortRef>,
) {
    use crate::ast::ExprKind::*;
    let binders_set: std::collections::HashSet<&String> = binders.iter().collect();
    match &e.kind {
        Call { func, args, .. } => {
            if let Ident(name) = &func.kind {
                if let Some(sig) = pure_views.get(name) {
                    for (i, a) in args.iter().enumerate() {
                        if i < sig.param_sorts.len() {
                            if let Ident(arg_name) = &a.expr().kind {
                                if binders_set.contains(&arg_name.to_string()) {
                                    out.entry(arg_name.clone())
                                        .or_insert_with(|| sig.param_sorts[i].clone());
                                }
                            }
                        }
                        infer_binder_sorts(a.expr(), binders, pure_views, out);
                    }
                    return;
                }
            }
            for a in args {
                infer_binder_sorts(a.expr(), binders, pure_views, out);
            }
        }
        Binary { left, right, .. } => {
            infer_binder_sorts(left, binders, pure_views, out);
            infer_binder_sorts(right, binders, pure_views, out);
        }
        Unary { operand, .. } => {
            infer_binder_sorts(operand, binders, pure_views, out);
        }
        If { cond, then, else_ } => {
            infer_binder_sorts(cond, binders, pure_views, out);
            if let Some(t) = &then.trailing {
                infer_binder_sorts(t, binders, pure_views, out);
            }
            if let Some(crate::ast::ElseBranch::Block(b)) = else_ {
                if let Some(t) = &b.trailing {
                    infer_binder_sorts(t, binders, pure_views, out);
                }
            } else if let Some(crate::ast::ElseBranch::If(ie)) = else_ {
                infer_binder_sorts(ie, binders, pure_views, out);
            }
        }
        _ => {}
    }
}

impl Default for VerificationPipeline {
    fn default() -> Self { Self::new() }
}

/// Plan 33.3 Р¤.9.5: РїСЂРѕРІРµСЂРєР° consistency axiom'РѕРІ РјРѕРґСѓР»СЏ.
///
/// Р"Р»СЏ РєР°Р¶РґРѕРіРѕ СЌС„С„РµРєС‚Р° СЃ axioms СЃРѕР·РґР°С'С‚СЃСЏ РёР·РѕР»РёСЂРѕРІР°РЅРЅС‹Р№ backend, РІ РЅС'Рј
/// РѕР±СЉСЏРІР»СЏСЋС‚СЃСЏ РІСЃРµ pure_view UFs СЌС„С„РµРєС‚Р°, asserted РІСЃРµ axioms, Р·Р°С‚РµРј
/// `check_sat`. Р•СЃР»Рё UNSAT вЂ" axioms together implication False в†'
/// **compile error** В«axioms inconsistentВ».
///
/// SAT РёР»Рё Unknown вЂ" OK. TrivialBackend РІСЃРµРіРґР° РґР°С'С‚ Unknown РґР»СЏ
/// quantified-axioms (РЅРµС‚ reasoning'Р° РЅР°Рґ Forall), С‡С‚Рѕ С‚СЂР°РєС‚СѓРµС‚СЃСЏ РєР°Рє
/// В«РЅРµ РґРѕРєР°Р·Р°РЅРѕ inconsistentВ» вЂ" silent fallback.
///
/// Р'РѕР·РІСЂР°С‰Р°РµС‚ diagnostic'Рё (РїСѓСЃС‚РѕР№ Vec РµСЃР»Рё РІСЃС' consistent).
pub fn check_axiom_consistency(module: &Module) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let pure_views = collect_pure_views(module);

    // Р"СЂСѓРїРїРёСЂСѓРµРј axioms РїРѕ effect-name.
    let mut axioms_by_effect: std::collections::HashMap<String, Vec<(&crate::ast::TypeDecl, Vec<&crate::ast::EffectAxiom>)>>
        = std::collections::HashMap::new();
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        if !matches!(td.kind, TypeDeclKind::Effect(_)) { continue }
        if td.axioms.is_empty() { continue }
        let entry = axioms_by_effect.entry(td.name.clone()).or_default();
        let axiom_refs: Vec<&crate::ast::EffectAxiom> = td.axioms.iter().collect();
        entry.push((td, axiom_refs));
    }

    let pipeline = VerificationPipeline::new();

    for (_effect_name, effect_group) in &axioms_by_effect {
        for (td, axiom_refs) in effect_group {
            let mut backend = pipeline.create_backend();

            // Pre-declare Р'РЎР• pure_view UFs РјРѕРґСѓР»СЏ (РјРѕРіСѓС‚ СЃСЃС‹Р»Р°С‚СЊСЃСЏ cross-effect
            // РІ С„РѕСЂРјСѓР»Р°С… вЂ" V1 РѕРіСЂР°РЅРёС‡РёРІР°РµС‚ one-effect-axioms, РЅРѕ Р±РµР·РѕРїР°СЃРЅРµРµ
            // pre-decl'РёС‚СЊ РІСЃС').
            for (op_name, sig) in &pure_views {
                let uf = super::encode::pure_view_uf_name(&sig.effect_name, op_name);
                backend.declare_function(&uf, &sig.param_sorts, sig.return_sort.clone());
            }

            // Assert РІСЃРµ axioms СЌС„С„РµРєС‚Р°.
            let mut some_encoded = false;
            for ax in axiom_refs {
                let info = AxiomInfo {
                    effect_name: td.name.clone(),
                    axiom_name: ax.name.clone(),
                    binders: &ax.binders,
                    formula: &ax.formula,
                    is_generic: !ax.generics.is_empty(),
                };
                if let Some(formula) = encode_axiom(&info, &pure_views) {
                    backend.assert(Assertion {
                        formula,
                        label: Some(format!("axiom@{}.{}", td.name, ax.name)),
                    });
                    some_encoded = true;
                }
            }

            // Р•СЃР»Рё РЅРё РѕРґРёРЅ axiom РЅРµ encoded вЂ" РЅРµС‡РµРіРѕ РїСЂРѕРІРµСЂСЏС‚СЊ.
            if !some_encoded { continue; }

            // check_sat. Unsat в†' inconsistent.
            match backend.check_sat() {
                SatResult::Unsat(_) => {
                    diagnostics.push(Diagnostic::new(
                        format!(
                            "axioms of effect `{}` are inconsistent: their conjunction \
                             entails `false`. Z3 cannot find any model satisfying all \
                             axioms simultaneously. Suggestions:\n  \
                             1. Review axiom bodies for unintended contradiction \
                                (e.g. `balance(id) >= 0` AND `balance(id) < 0`);\n  \
                             2. If axioms intentionally over-constrain (impossible \
                                effect), mark effect `#trusted` and split into \
                                consistent subset.",
                            td.name,
                        ),
                        td.span,
                    ));
                }
                _ => {
                    // SAT РёР»Рё Unknown вЂ" axioms consistent (РёР»Рё TrivialBackend
                    // РЅРµ reasoning'СѓРµС‚ вЂ" silent OK).
                }
            }
        }
    }

    diagnostics
}

/// Substitute `_old_<x>` → `<x>` (33.1: no mut, snapshot trivial).
pub(super) fn substitute_old(t: &SmtTerm) -> SmtTerm {
    match t {
        SmtTerm::Var(n) if n.starts_with("_old_") => {
            SmtTerm::Var(n.strip_prefix("_old_").unwrap().to_string())
        }
        SmtTerm::App(op, args) => SmtTerm::App(
            op.clone(),
            args.iter().map(substitute_old).collect(),
        ),
        _ => t.clone(),
    }
}

pub(super) fn type_to_sort(ty: &TypeRef) -> SortRef {
    match ty {
        TypeRef::Named { path, .. } if path.len() == 1 => match path[0].as_str() {
            "int" | "i32" | "i64" | "money" | "nat" => SortRef::Int,
            "bool" => SortRef::Bool,
            "str" => SortRef::Str,
            "f32" => SortRef::F32,
            "f64" => SortRef::F64,
            other => SortRef::Named(other.into()),
        },
        _ => SortRef::Named("opaque".into()),
    }
}

pub(super) fn format_counterexample(model: &Model) -> String {
    if model.bindings.is_empty() {
        return "values not extracted (TrivialBackend); enable Z3 \
                backend for full counterexample".into();
    }
    let mut parts = Vec::new();
    for (name, val) in &model.bindings {
        let v = match val {
            ModelValue::Int(n) => n.to_string(),
            ModelValue::Bool(b) => b.to_string(),
            ModelValue::Str(s) => format!("\"{}\"", s),
            ModelValue::Unknown => "?".into(),
        };
        parts.push(format!("{} = {}", name, v));
    }
    parts.join(", ")
}

pub(super) fn unknown_to_diag_message(reason: UnknownReason) -> String {
    match reason {
        UnknownReason::Timeout => {
            "SMT solver hit timeout. \
             Suggestions: (1) simplify the contract into smaller steps via \
             intermediate `assert_static`; (2) increase `#verify_timeout(N)`; \
             (3) mark `#unverified` if verification is intentionally complex.".into()
        }
        UnknownReason::NonLinearArithmetic => {
            "non-linear arithmetic in contract (e.g. `x * y`, `x / y`). \
             Trivial backend supports only LIA; Z3 backend can handle non-linear \
             via NIA. Suggestions: (1) rewrite in linear form via intermediate \
             variables; (2) wait for Z3 backend; (3) `#unverified`.".into()
        }
        UnknownReason::UnsupportedTheory(s) => {
            format!("unsupported SMT theory: {}. Suggestion: rewrite in supported \
                     theory (LIA/EUF/arrays) or mark `#unverified`.", s)
        }
        UnknownReason::BackendError(s) => {
            format!("SMT backend internal error: {}. This is a bug -- please report.", s)
        }
        UnknownReason::NotAttempted(s) => {
            format!("{}\n  AI-friendly hint: contract is beyond TrivialBackend \
                     capabilities (only reflexive ensures, constant folding, \
                     impl-shortcuts). Add intermediate `assert_static`, or \
                     mark `#unverified`, or wait for Z3 backend.", s)
        }
    }
}

/// Entry-point: РїСЂРѕРІРµСЂРёС‚СЊ РІСЃРµ С„СѓРЅРєС†РёРё РјРѕРґСѓР»СЏ. Р--Р°РїРѕР»РЅСЏРµС‚ diagnostics
/// СЃ warning'Р°РјРё/errors СЃРѕРіР»Р°СЃРЅРѕ verify_mode.
///
/// РўР°РєР¶Рµ РІРѕР·РІСЂР°С‰Р°РµС‚ map `(fn_name в†' set of proven contract span)`,
/// РєРѕС‚РѕСЂР°СЏ РёСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ codegen'РѕРј РґР»СЏ **zero-cost release** вЂ"
/// proven РєРѕРЅС‚СЂР°РєС‚С‹ РЅРµ emit'СЏС‚СЃСЏ РІ release СЃР±РѕСЂРєРµ.
pub fn verify_module(module: &Module) -> ModuleVerifyReport {
    let pipeline = VerificationPipeline::new();
    let cache_dir = std::env::var("NOVA_CACHE_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join("target"));
    let cache = super::cache::ContractCache::new(&cache_dir);
    let module_name = module.name.join(".");
    let mut report = ModuleVerifyReport::default();

    // Plan 33.3 Р¤.9.5: РїСЂРѕРІРµСЂРєР° consistency axiom'РѕРІ РґРѕ per-fn verify.
    // Р•СЃР»Рё axioms СЌС„С„РµРєС‚Р° inconsistent (Z3 в†' UNSAT) в†' compile-error,
    // skip РІСЃРµС… РѕСЃС‚Р°Р»СЊРЅС‹С… verify'РµРІ (Р»СЋР±Р°СЏ formula С‚СЂРёРІРёР°Р»СЊРЅРѕ РґРѕРєР°Р·СѓРµРјР°
    // РїРѕРґ inconsistent assumptions).
    let inconsistency_errors = check_axiom_consistency(module);
    let has_inconsistent_axioms = !inconsistency_errors.is_empty();
    for e in inconsistency_errors {
        report.errors.push(e);
    }
    if has_inconsistent_axioms {
        return report;
    }

    // Plan 33.4 P0-1 (Р¤.9.7 V1): РІРµСЂРёС„РёРєР°С†РёСЏ `with #verify E = handler` bindings.
    for diag in super::handler_exec::verify_handlers(module) {
        report.errors.push(diag);
    }
    if !report.errors.is_empty() {
        return report;
    }

    // Р¤.3: РІС‹С‡РёСЃР»СЏРµРј SCC purity РѕРґРёРЅ СЂР°Р· РЅР° РјРѕРґСѓР»СЊ, С‡С‚РѕР±С‹ РЅРµ РїРµСЂРµСЃС‡РёС‚С‹РІР°С‚СЊ
    // СЂРµРєСѓСЂСЃРёРІРЅС‹Р№ РѕР±С…РѕРґ AST РЅР° РєР°Р¶РґСѓСЋ С„СѓРЅРєС†РёСЋ (overhead + СЂРёСЃРє stack overflow).
    let inferred_pure = infer_pure_fns_scc(module);

    // Plan 33.3 Ф.13: #must_verify_module -- все функции MustVerify.
    let module_strict = module.attrs.iter().any(|a| matches!(a.kind, ModuleAttrKind::MustVerifyModule));

    for item in &module.items {
        if let Item::Fn(fd) = item {
            if fd.contracts.is_empty() { continue; }
            // Plan 33.3 Ф.13: #trusted external fn -- контракты axioms, SMT-verify пропускается.
            if fd.is_trusted && fd.is_external { continue; }
            // Skip Fail-functions вЂ" ContractCtx СѓР¶Рµ РІС‹РґР°Р» error.
            // Mut-РїР°СЂР°РјРµС‚СЂС‹ в†' РїСЂРѕРїСѓСЃС‚РёС‚СЊ (33.2). РЎРµР№С‡Р°СЃ РґРµС‚РµРєС‚РёРј С‡РµСЂРµР·
            // РѕС‚СЃСѓС‚СЃС‚РІРёРµ РІ С‚РёРїР°С….
            let contracts_repr: String = fd.contracts.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>().join("|");
            let body_repr = format!("{:?}", fd.body);
            let ck = super::cache::cache_key(&module_name, &fd.name, &contracts_repr, &body_repr);
            if let Some(super::cache::CachedResult::Proven) = cache.lookup(ck) {
                for c in &fd.contracts { if matches!(c.kind, ContractKind::Ensures) { report.proven.push((fd.name.clone(), c.span)); } }
                continue;
            }
                        let effective_mode = if module_strict && matches!(fd.verify_mode, VerifyMode::Default) { VerifyMode::MustVerify } else { fd.verify_mode };
let t0 = std::time::Instant::now();
            let results = pipeline.verify_fn(module, fd, &inferred_pure);
            let elapsed_ms = t0.elapsed().as_millis() as u64;
            for (span, vr) in results {
                match vr {
                    VerifyResult::Proven => {
                        report.proven.push((fd.name.clone(), span));
                    }
                    VerifyResult::Disproved(_, cex) => {
                        // Plan 33.3 Р¤.9.10: AI-friendly format.
                        // Р'РєР»СЋС‡Р°РµС‚: fn name, counterexample values (РёР»Рё hint),
                        // suggestions РґР»СЏ РёСЃРїСЂР°РІР»РµРЅРёСЏ.
                        let msg = format!(
                            "contract violation in `{}`:\n  counterexample: {}\n  \
                             suggestions:\n    1. Add `requires` precondition restricting input;\n    \
                             2. Fix function body to match `ensures`;\n    \
                             3. Weaken `ensures` to actual behavior;\n    \
                             4. Mark `#unverified` if intentional disprove",
                            fd.name, cex);
                        match effective_mode {
                            VerifyMode::MustVerify => report.errors.push(
                                Diagnostic::new(msg, span)),
                            _ => report.warnings.push(
                                Diagnostic::new(msg, span)),
                        }
                    }
                    VerifyResult::Unknown(reason) => {
                        // РџРѕ D24 / Plan 33.1: default вЂ" runtime fallback РІ debug,
                        // РІ release РєРѕРЅС‚СЂР°РєС‚ СЃС‚РёСЂР°РµС‚СЃСЏ СЃ warning (РёР»Рё error РµСЃР»Рё
                        // `#verify`).
                        match effective_mode {
                            VerifyMode::MustVerify => {
                                // Plan 33.3 Р¤.9.10: AI-friendly format СЃ
                                // РєР°С‚РµРіРѕСЂРёР·РёСЂРѕРІР°РЅРЅС‹Рј reason + suggestions
                                // (reason СѓР¶Рµ СЃРѕРґРµСЂР¶РёС‚ hint РёР· unknown_to_diag_message).
                                let msg = format!(
                                    "`#verify` failed for `{}`:\n  {}",
                                    fd.name, reason);
                                report.errors.push(Diagnostic::new(msg, span));
                            }
                            _ => {
                                // Ф.6.2 (Plan 33.6): Default + Unverified mode.
                                // NotAttempted (TrivialBackend не пробовал) -- silent: это нормально.
                                // Остальные причины (Timeout, NonLinear, UnsupportedTheory,
                                // BackendError) -- W2402: реальная проблема, пользователь должен знать.
                                if !reason.contains("NotAttempted") {
                                    let msg = format!(
                                        "fn '{}': SMT-верификация не завершена [W2402]: {}\n  \
                                         Если это ожидаемо -- пометьте `#unverified` \
                                         для явного подтверждения.",
                                        fd.name, reason);
                                    report.warnings.push(Diagnostic::new(msg, span));
                                }
                                // NotAttempted → silent (TrivialBackend нормально не пробует сложные)
                            }
                        }
                    }
                    VerifyResult::EncodingFailed(reason) => {
                        // Ф.1.2 (Plan 33.6): EncodingFailed без #unverified -> compile error E2401.
                        // С #unverified -> warning W2401 (осознанный отказ от SMT).
                        // "body not encodable" -- TrivialBackend limitation, не E2401 (оставляем silent).
                        // E2401 только если контракт (requires/ensures expr) не encodable,
                        // не когда body не encodable (TrivialBackend limitation).
                        let is_contract_unsupported = reason.starts_with("[CONTRACT_UNSUPPORTED]");
                        let display_reason = reason.trim_start_matches("[CONTRACT_UNSUPPORTED] ");
                        match fd.verify_mode {
                            VerifyMode::MustVerify | VerifyMode::Default if is_contract_unsupported => {
                                let extra = if matches!(fd.verify_mode, VerifyMode::Default) {
                                    " hint: пометьте fn как #unverified для runtime-fallback,\
                                     или вынесите логику в #pure helper."
                                } else { "" };
                                let msg = format!(
                                    "контракт fn '{}' содержит конструкцию \
                                     не поддерживаемую SMT-encoder'ом [E2401]: {}. {}",
                                    fd.name, display_reason, extra);
                                report.errors.push(Diagnostic::new(msg, span));
                            }
                            VerifyMode::Unverified => {
                                let msg = format!(
                                    "fn '{}' помечена #unverified: SMT verification пропущен [W2401]",
                                    fd.name);
                                report.warnings.push(Diagnostic::new(msg, span));
                            }
                            VerifyMode::MustVerify => {
                                // Ф.6.2: MustVerify + body limitation (не CONTRACT_UNSUPPORTED).
                                // Тело fn не encodable (control flow, FFI, etc.) -- W2402.
                                // Пользователь ожидал верификацию, должен знать что body пропущен.
                                let msg = format!(
                                    "fn '{}' помечена `#verify`: тело fn содержит конструкцию \
                                     не поддерживаемую SMT-encoder'ом тела [W2402]: {}\n  \
                                     Верификация контракта возможна только если тело прямолинейно. \
                                     Используйте `#trusted` если верификация тела не нужна.",
                                    fd.name, display_reason);
                                report.warnings.push(Diagnostic::new(msg, span));
                            }
                            _ => {} // Default/Unverified + body limitation -- silent (TrivialBackend норм.)
                        }
                    }
                    VerifyResult::Warning(msg) => {
                        // Ф.6.2 (Plan 33.6): W2402 от verify_fn (loop invariant не encodable, etc.)
                        report.warnings.push(Diagnostic::new(msg, span));
                    }
                }
            }
        }
        // Ф.4.1: верифицируем все леммы модуля.
        // Лемма -- это proven proof term: failure = hard error (always MustVerify).
        if let Item::Lemma(ld) = item {
            if ld.contracts.is_empty() { continue; }
            let results = pipeline.verify_lemma(module, ld, &inferred_pure);
            for (span, vr) in results {
                match vr {
                    VerifyResult::Proven => {
                        report.proven.push((ld.name.clone(), span));
                    }
                    VerifyResult::Disproved(_, cex) => {
                        report.errors.push(Diagnostic::new(
                            format!("lemma `{}` не доказана:\n  counterexample: {}\n  \
                                     Лемма должна быть доказуема -- проверьте requires/ensures/body.",
                                ld.name, cex),
                            span));
                    }
                    VerifyResult::Unknown(reason) => {
                        report.errors.push(Diagnostic::new(
                            format!("lemma `{}` не удалось верифицировать:\n  {}\n  \
                                     Лемма требует полной верификации (не runtime fallback).",
                                ld.name, reason),
                            span));
                    }
                    VerifyResult::EncodingFailed(reason) => {
                        report.errors.push(Diagnostic::new(
                            format!("lemma `{}`: тело или контракт не encodable: {}\n  \
                                     Только int/bool/str/record/binary-ops/if поддерживается в V1.",
                                ld.name, reason),
                            span));
                    }
                    VerifyResult::Warning(msg) => {
                        report.warnings.push(Diagnostic::new(msg, span));
                    }
                }
            }
        }
    }
    report
}

/// Aggregated РѕС‚С‡С'С‚ РїРѕ РІРµСЂРёС„РёРєР°С†РёРё РјРѕРґСѓР»СЏ.
#[derive(Debug, Default)]
pub struct ModuleVerifyReport {
    /// Р"РѕРєР°Р·Р°РЅРЅС‹Рµ РєРѕРЅС‚СЂР°РєС‚С‹ вЂ" `(fn_name, span)`. РСЃРїРѕР»СЊР·СѓСЋС‚СЃСЏ codegen'РѕРј
    /// РІ release-СЃР±РѕСЂРєРµ РґР»СЏ СЃС‚РёСЂР°РЅРёСЏ runtime-check'Р°.
    pub proven: Vec<(String, Span)>,
    /// Errors вЂ" РІС‹РґР°СЋС‚СЃСЏ РїРѕСЃР»Рµ verify (РЅР°РїСЂРёРјРµСЂ `#verify` failed).
    pub errors: Vec<Diagnostic>,
    /// Warnings вЂ" counterexamples РґР»СЏ РєРѕРЅС‚СЂР°РєС‚РѕРІ Р±РµР· `#verify`.
    pub warnings: Vec<Diagnostic>,
}



