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
    /// Plan 33.14: каждая VC прогоняется через Z3 И CVC5, расхождения
    /// определённых ответов поднимают compile-error. CI-only режим.
    CrossCheck,
}

impl BackendChoice {
    /// РџР°СЂСЃРёС‚ СЃС‚СЂРѕРєСѓ, used Рё РґР»СЏ CLI Рё РґР»СЏ env-var.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "trivial" | "default" | "" => Some(BackendChoice::Trivial),
            "z3" => Some(BackendChoice::Z3),
            "crosscheck" | "cross-check" => Some(BackendChoice::CrossCheck),
            _ => None,
        }
    }

    /// Backend РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ: СЃРјРѕС‚СЂРёРј `NOVA_SMT_BACKEND`, РёРЅР°С‡Рµ Trivial.
    ///
    /// Plan 33.14: `NOVA_CROSSCHECK=1` имеет приоритет над `NOVA_SMT_BACKEND`
    /// и принудительно включает cross-check режим.
    pub fn from_env() -> Self {
        if let Ok(v) = std::env::var("NOVA_CROSSCHECK") {
            let v = v.trim().to_ascii_lowercase();
            if v == "1" || v == "true" || v == "yes" || v == "on" {
                return BackendChoice::CrossCheck;
            }
        }
        std::env::var("NOVA_SMT_BACKEND")
            .ok()
            .and_then(|s| Self::parse(&s))
            .unwrap_or(BackendChoice::Trivial)
    }
}

/// Plan 33.14: одноразовые предупреждения для деградации cross-check'а.
fn crosscheck_warn_once(which: u8) {
    use std::sync::Once;
    static CVC5_MISSING: Once = Once::new();
    static Z3_MISSING: Once = Once::new();
    match which {
        0 => CVC5_MISSING.call_once(|| {
            eprintln!(
                "warning: NOVA_CROSSCHECK задан, но бинарник `cvc5` не найден \
                 (env NOVA_CVC5 / PATH); cross-check вырождается в «только Z3». \
                 Установите cvc5 для полной проверки."
            );
        }),
        _ => Z3_MISSING.call_once(|| {
            eprintln!(
                "warning: NOVA_CROSSCHECK задан, но бинарь собран без \
                 `--features z3-backend`; cross-check недоступен, fallback на \
                 trivial backend. Пересоберите с `cargo build --features z3-backend`."
            );
        }),
    }
}

pub struct VerificationPipeline {
    pub timeout_ms: u32,
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
            // Plan 33.14: cross-check режим Z3 ↔ CVC5.
            BackendChoice::CrossCheck => {
                #[cfg(feature = "z3-backend")]
                {
                    if super::backend::cvc5::cvc5_available() {
                        Box::new(super::crosscheck::CrossCheckBackend::new(self.timeout_ms))
                    } else {
                        // cvc5 нет — деградируем до Z3, чтобы компиляция
                        // не ломалась (cross-check просто не сработает).
                        crosscheck_warn_once(0);
                        Box::new(super::backend::z3::Z3Backend::new(self.timeout_ms))
                    }
                }
                #[cfg(not(feature = "z3-backend"))]
                {
                    crosscheck_warn_once(1);
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
        let trusted_fns = collect_trusted_fns(module);
        let ctx = super::encode::EncodeCtx { pure_views: &pure_views, pure_fns: &pure_fns, trusted_fns: &trusted_fns, var_sorts };

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
        // Plan 33.9 Ф.5: #opaque fns are skipped here — their body axiom is only
        // injected when `reveal fn_name` is encountered in the verifying fn body.
        for item in &module.items {
            let Item::Fn(pfd) = item else { continue };
            if !matches!(pfd.purity, Purity::Pure) { continue; }
            let Some(info) = pure_fns.get(&pfd.name) else { continue };
            // Skip opaque fns: body withheld until reveal.
            if info.is_opaque { continue; }
            let body_expr = match &pfd.body {
                FnBody::Expr(e) => Some(e),
                FnBody::Block(b) if b.stmts.is_empty() => b.trailing.as_deref(),
                _ => None,
            };
            if let Some(body_e) = body_expr {
                if let Ok(body_term) = encode::encode_expr_with_ctx(body_e, &ctx) {
                    let param_vars: Vec<SmtTerm> = info.param_names.iter()
                        .map(|n| SmtTerm::Var(n.clone())).collect();
                    let uf_app = SmtTerm::App(encode::pure_fn_uf_name(&pfd.name), param_vars.clone());
                    let eq_body = SmtTerm::eq(uf_app.clone(), body_term);
                    let binders: Vec<(String, SortRef)> = info.param_names.iter()
                        .zip(info.param_sorts.iter())
                        .map(|(n, s)| (n.clone(), s.clone()))
                        .collect();
                    // Trigger on UF application to guide instantiation.
                    let patterns = if binders.is_empty() { vec![] } else { vec![vec![uf_app]] };
                    let axiom = if binders.is_empty() { eq_body } else {
                        SmtTerm::Forall(binders, patterns, Box::new(eq_body))
                    };
                    backend.assert(Assertion {
                        formula: axiom,
                        label: Some(format!("pure_fn_body@{}", pfd.name)),
                    });
                }
            }
        }

        // Ф.4.2 (Plan 33.6): declare UFs для #trusted external fn + inject ensures axioms.
        // ensures(params, result) → (forall (params result) (=> true ensures)).
        // "result" — специальная переменная, обозначает возвращаемое значение UF.
        for (fn_name, info) in &trusted_fns {
            let uf = encode::trusted_fn_uf_name(fn_name);
            backend.declare_function(&uf, &info.param_sorts, info.return_sort.clone());
            // Build axiom: forall params, result. ensures_expr
            // Substitution: "result" → uf(params) в ensures.
            let param_vars: Vec<SmtTerm> = info.param_names.iter()
                .map(|n| SmtTerm::Var(n.clone())).collect();
            let uf_app = SmtTerm::App(uf.clone(), param_vars);
            let result_var = "_trusted_result".to_string();
            // Binders: params + _trusted_result
            let mut binders: Vec<(String, SortRef)> = info.param_names.iter()
                .zip(info.param_sorts.iter())
                .map(|(n, s)| (n.clone(), s.clone()))
                .collect();
            binders.push((result_var.clone(), info.return_sort.clone()));
            // Substitute "result" → _trusted_result in ensures, then assert forall
            let result_binding = SmtTerm::eq(SmtTerm::Var(result_var.clone()), uf_app);
            for ensures_expr in &info.ensures_exprs {
                // Encode ensures with result → _trusted_result substitution in ctx.
                let mut ctx_with_result = ctx.clone();
                ctx_with_result.var_sorts.insert(result_var.clone(), info.return_sort.clone());
                if let Ok(ensures_term) = encode::encode_expr_with_ctx(ensures_expr, &ctx_with_result) {
                    // Replace "result" var with _trusted_result in encoded term.
                    let ensures_subst = ensures_term.substitute("result", &SmtTerm::Var(result_var.clone()));
                    let body = SmtTerm::App("and".into(), vec![result_binding.clone(), ensures_subst]);
                    let axiom = SmtTerm::Forall(binders.clone(), vec![], Box::new(body));
                    backend.assert(Assertion {
                        formula: axiom,
                        label: Some(format!("trusted_fn_ensures@{}", fn_name)),
                    });
                }
            }
        }

        // 1. Declare params as Vars.
        // Ф.7.2 (Plan 33.6): + declare `_old_<param>` для каждого param как
        // entry-snapshot (V4 close). Раньше substitute_old делал
        // тривиальную подстановку `_old_x → x` — unsound для mut params.
        // Теперь `_old_<x>` — отдельная SMT var, frame axiom (далее) ассертит
        // `_old_x == x` для non-modifies params. Для modifies-params (когда
        // появятся в Nova) — frame axiom не applies, `_old_<x>` представляет
        // entry-state значение независимо от current `x`.
        for p in &fd.params {
            let sort = type_to_sort(&p.ty);
            backend.declare_var(&p.name, sort.clone());
            backend.declare_var(&format!("_old_{}", p.name), sort);
            // Plan 33.8 Ф.1.4: `nat` — неотрицательный `int`. Без аксиомы
            // `nat >= 0` верификатор считал бы nat-параметр любым целым
            // (включая отрицательные) → ложные counterexample'ы и unsound
            // рассуждение о свойствах, опирающихся на неотрицательность.
            if type_ref_is_nat(&p.ty) {
                for vname in [p.name.clone(), format!("_old_{}", p.name)] {
                    backend.assert(Assertion {
                        formula: SmtTerm::App(">=".into(), vec![
                            SmtTerm::Var(vname),
                            SmtTerm::IntLit(0),
                        ]),
                        label: Some("nat_nonneg".into()),
                    });
                }
            }
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
        let mut apply_warnings: Vec<(Span, VerifyResult)> = Vec::new();
        for (lemma_name, mut args, apply_span) in collect_apply_stmts_in_body(&fd.body) {
            // Ф.31.1 (Plan 33.6): apply к несуществующей лемме → E2405 (был silent skip).
            // Программист написал `apply foo(x)` но `lemma foo` не объявлена.
            // Проверка: lemma либо есть с contracts, либо вообще нет в модуле.
            let lemma_exists = module.items.iter().any(|item| {
                matches!(item, Item::Lemma(ld) if ld.name == lemma_name)
            });
            if !lemma_exists {
                apply_warnings.push((apply_span, VerifyResult::EncodingFailed(format!(
                    "[CONTRACT_UNSUPPORTED] `apply {}` ссылается на несуществующую лемму [E2405]: \
                     объявите `lemma {}(...) ensures ...` или удалите apply.",
                    lemma_name, lemma_name))));
                continue;
            }
            if let Some(lemma_ensures) = find_lemma_ensures(module, &lemma_name) {
                for (param_names, ensures_expr) in &lemma_ensures {
                    // Ф.13.1 (Plan 33.6): apply auto-inference. Если args.is_empty()
                    // и lemma имеет params — пытаемся auto-fill через **name-based
                    // matching**: lemma param `x` → fn param `x` если ровно 1 такой.
                    if args.is_empty() && !param_names.is_empty() {
                        let mut auto_args: Vec<crate::ast::Expr> = Vec::new();
                        let mut auto_ok = true;
                        for pname in param_names {
                            let matches: Vec<&crate::ast::Param> = fd.params.iter()
                                .filter(|p| p.name == *pname).collect();
                            if matches.len() == 1 {
                                auto_args.push(crate::ast::Expr {
                                    kind: ExprKind::Ident(pname.clone()),
                                    span: apply_span,
                                });
                            } else {
                                auto_ok = false;
                                break;
                            }
                        }
                        if auto_ok {
                            args = auto_args;
                        } else {
                            let suggested = format!("apply {}({})", lemma_name, param_names.join(", "));
                            apply_warnings.push((apply_span, VerifyResult::Warning(format!(
                                "`apply {}` auto-inference не удалась [W2402]: \
                                 не найдено уникальных matching params в scope.\n  \
                                 hint: `{}`",
                                lemma_name, suggested))));
                            continue;
                        }
                    }
                    if param_names.len() != args.len() {
                        // Ф.11.3 (Plan 33.6): arity mismatch — suggest correct call
                        // (UX hint via W2402).
                        let _ = (ensures_expr,);
                        let suggested = format!("apply {}({})", lemma_name, param_names.join(", "));
                        apply_warnings.push((apply_span, VerifyResult::Warning(format!(
                            "`apply {}` имеет {} args, ожидалось {} [W2402]:\n  \
                             hint: `{}`",
                            lemma_name, args.len(), param_names.len(), suggested))));
                        continue;
                    }
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

        // 2.55. Plan 33.9 Ф.5: inject body axioms for `reveal fn_name` statements.
        // For each reveal in fn body: find the opaque pure fn, emit
        //   forall params. _pure_fn_X(params) == body  :pattern { _pure_fn_X(params) }
        // This makes Z3 aware of the fn body specifically in this verification scope.
        // Fuel chain: if fn has #fuel(n), emit n unrolled axioms instead of single body forall.
        for (reveal_name, _reveal_span) in collect_reveal_stmts_in_body(&fd.body) {
            let Some(info) = pure_fns.get(&reveal_name) else { continue };
            // Only opaque fns get reveal-injected axioms; non-opaque already have body axiom.
            if !info.is_opaque { continue; }
            // Find the fn declaration to get its body.
            let pfd_opt = module.items.iter().find_map(|it| {
                if let Item::Fn(pfd) = it { if pfd.name == reveal_name { return Some(pfd); } }
                None
            });
            let Some(pfd) = pfd_opt else { continue };
            let fuel = info.fuel.unwrap_or(0);
            if fuel == 0 {
                // Standard reveal: single body forall with trigger.
                emit_opaque_body_axiom(pfd, info, &ctx, &mut *backend);
            } else {
                // Fuel chain: emit n levels of unrolled axioms.
                emit_fuel_chain_axioms(pfd, info, fuel, &ctx, &mut *backend);
            }
        }

        // 2.6. Ф.4.2: обработать `calc { ... }` из тела fn.
        // Для каждого calc-блока: каждый смежный шаг (e_i rel e_{i+1}) доказывается
        // и ассертируется в SMT-scope (как lemma: доказано → доказано → доступно для ensures).
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
        // Ф.11.3 (Plan 33.6): apply arity-mismatch warnings.
        results.extend(apply_warnings);
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

            // Ф.16.1 (Plan 33.6): если ensures содержит `exists`, используем
            // try_prove_with_witness для extract witness в info-note.
            // Plan 33.9 Ф.4: save a clone for opaque-UF check after goal is consumed.
            let goal_for_opaque_check = goal.clone();
            let exists_var = find_exists_var(&c.expr);
            let proof_result = if let Some(var_name) = &exists_var {
                let (proof, witness) = super::backend::try_prove_with_witness(
                    &mut *backend, goal, var_name);
                if let (SatResult::Unsat(_), Some(w)) = (&proof, &witness) {
                    let w_str = match w {
                        ModelValue::Int(n) => n.to_string(),
                        ModelValue::Bool(b) => b.to_string(),
                        ModelValue::Str(s) => format!("\"{}\"", s),
                        ModelValue::Unknown => "?".into(),
                    };
                    // Emit info-note как Warning (информационный, не error).
                    results.push((c.span, VerifyResult::Warning(format!(
                        "proven via witness: {} = {} [info]",
                        var_name, w_str))));
                }
                proof
            } else {
                try_prove(&mut *backend, goal)
            };
            // try_prove(goal). `&mut *backend` С‡С‚РѕР±С‹ coerce Box<dyn> в†' &mut dyn.
            match proof_result {
                SatResult::Unsat(_) => results.push((c.span, VerifyResult::Proven)),
                SatResult::Sat(model) => {
                    let cex = format_counterexample(&model);
                    results.push((c.span, VerifyResult::Disproved(model, cex)));
                }
                SatResult::Unknown(reason) => {
                    // Plan 33.3 Ф.9.10: AI-friendly diagnostic — категоризируем
                    // reason + suggestions.
                    // Plan 33.9 Ф.4: upgrade NotAttempted → UnsupportedTheory when
                    // the goal or body_val contains an opaque UF call. TrivialBackend
                    // cannot discharge opaque-fn contracts; user needs `reveal X` or Z3.
                    let upgraded_reason = if matches!(&reason, UnknownReason::NotAttempted(_)) {
                        let opaque_in_goal = term_references_opaque_uf(&goal_for_opaque_check, &pure_fns);
                        let opaque_in_body = body_val.as_ref()
                            .and_then(|bv| term_references_opaque_uf(bv, &pure_fns));
                        let opaque_name = opaque_in_goal.or(opaque_in_body);
                        if let Some(fname) = opaque_name {
                            let has_reveal = collect_reveal_stmts_in_body(&fd.body)
                                .iter().any(|(n, _)| n == &fname);
                            if has_reveal {
                                UnknownReason::UnsupportedTheory(format!(
                                    "opaque fn `{}` was revealed but TrivialBackend cannot \
                                     discharge reveal-injected axioms (quantified formulas). \
                                     Use Z3 backend: `--smt-backend=z3` or \
                                     `NOVA_SMT_BACKEND=z3`.", fname))
                            } else {
                                UnknownReason::UnsupportedTheory(format!(
                                    "opaque fn `{}` called without `reveal {}`. \
                                     Add `reveal {}` before calling it in a `#verify` fn, \
                                     or use Z3 backend: `--smt-backend=z3`.",
                                    fname, fname, fname))
                            }
                        } else {
                            reason
                        }
                    } else {
                        reason
                    };
                    let msg = unknown_to_diag_message(upgraded_reason);
                    results.push((c.span, VerifyResult::Unknown(msg)));
                }
            }
        }

        // D.1.5: verify ensures_fail clauses (Fail-path postconditions).
        // Модель (V1, conservative): верифицируем ensures_fail независимо,
        // используя те же params + requires-assertions (entry state).
        // `result` недоступен; `old(x)` → x (entry-state, нет мутабельных params).
        // Ф.34.1 (Plan 33.6): если fn не имеет Fail в effects — ensures_fail
        // unreachable, не верифицируем (W2402 эмитится в verify_module pass).
        let has_fail_effect_local = fd.effects.iter().any(|e| {
            matches!(e, crate::ast::TypeRef::Named { path, .. }
                if path.len() == 1 && path[0] == "Fail")
        });
        for c in &fd.contracts {
            if !matches!(c.kind, ContractKind::EnsuresFail) { continue; }
            if !has_fail_effect_local { continue; } // Ф.34.1 skip — обработано lint'ом.
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

        // Plan 33.8 Ф.6.5: рекурсивная функция без `decreases` — завершимость
        // НЕ верифицируется. `ensures` доказывается лишь как partial
        // correctness; индукция по рекурсии без well-founded меры
        // потенциально unsound. Предупреждаем (раньше — молчаливый пропуск).
        if fd.decreases.is_none() {
            if let Some((call_span, _)) = find_recursive_calls_in_body(&fd.body, &fd.name).first() {
                results.push((*call_span, VerifyResult::Warning(
                    format!("рекурсивная функция `{}` без `decreases` [W2402]: \
                             завершимость НЕ верифицирована — `ensures` доказан \
                             только как partial correctness. Добавьте \
                             `decreases <мера>` для проверки well-foundedness, \
                             либо `#unverified`.", fd.name))));
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
                Err(e) => {
                    // Ф.7.3 (Plan 33.6): decreases expr не encodable → W2402 вместо silent skip.
                    let reason = match e {
                        super::encode::EncodingError::Unsupported(s) => s,
                    };
                    let msg = format!(
                        "loop decreases expr не закодирован в SMT [W2402]: {}.\n  \
                         well-foundedness НЕ проверена. Упростите decreases или используйте \
                         #unverified для явного отказа.",
                        reason);
                    results.push((dec_span, VerifyResult::Warning(msg)));
                }
            }
        }

        // Plan 33.7 Ф.4: `#nooverflow` — для каждой BV-арифм. операции в теле
        // генерируется overflow VC. Если не доказано → compile error (E2410).
        if fd.no_overflow {
            let bv_ops = collect_bv_arith_ops_in_body(&fd.body, &ctx);
            for (op_span, op, lhs_t, rhs_t) in bv_ops {
                // Width + signedness — из первого BV-sorted Var внутри терма
                // (после V2.3-subst операнды могут быть App-выражениями).
                // Литералы знаковость не несут — fallback на ширину BitVecLit.
                let bv_sort = bv_sort_in_term(&lhs_t, &ctx.var_sorts)
                    .or_else(|| bv_sort_in_term(&rhs_t, &ctx.var_sorts));
                let (width, is_signed) = match bv_sort {
                    Some((width, signed)) => (width, signed),
                    None => {
                        // Fallback: ширина из BitVecLit, знаковость unsigned.
                        match (bv_lit_width(&lhs_t), bv_lit_width(&rhs_t)) {
                            (Some(w), _) | (_, Some(w)) => (w, false),
                            _ => continue,
                        }
                    }
                };
                let vc: Option<SmtTerm> = match op {
                    crate::ast::BinOp::Add => Some(SmtTerm::App(
                        if is_signed { "bvadd_no_overflow_s" } else { "bvadd_no_overflow_u" }.into(),
                        vec![lhs_t, rhs_t],
                    )),
                    crate::ast::BinOp::Sub => Some(SmtTerm::App(
                        if is_signed { "bvsub_no_underflow_s" } else { "bvsub_no_underflow_u" }.into(),
                        vec![lhs_t, rhs_t],
                    )),
                    crate::ast::BinOp::Mul => Some(SmtTerm::App(
                        if is_signed { "bvmul_no_overflow_s" } else { "bvmul_no_overflow_u" }.into(),
                        vec![lhs_t, rhs_t],
                    )),
                    _ => None,
                };
                if let Some(vc_term) = vc {
                    let op_name = match op {
                        crate::ast::BinOp::Add => "addition",
                        crate::ast::BinOp::Sub => "subtraction",
                        crate::ast::BinOp::Mul => "multiplication",
                        _ => "arithmetic",
                    };
                    let ty_prefix = if is_signed { "i" } else { "u" };
                    match try_prove(&mut *backend, vc_term) {
                        SatResult::Unsat(_) => {
                            results.push((op_span, VerifyResult::Proven));
                        }
                        SatResult::Sat(model) => {
                            let cex = format_counterexample(&model);
                            results.push((op_span, VerifyResult::Disproved(model,
                                format!("#nooverflow: {} may overflow ({}{}): {}",
                                    op_name, ty_prefix, width, cex))));
                        }
                        SatResult::Unknown(reason) => {
                            results.push((op_span, VerifyResult::Unknown(
                                format!("#nooverflow {} check: {}", op_name, unknown_to_diag_message(reason)))));
                        }
                    }
                }
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
        let trusted_fns = collect_trusted_fns(module);
        let ctx = super::encode::EncodeCtx { pure_views: &pure_views, pure_fns: &pure_fns, trusted_fns: &trusted_fns, var_sorts };

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
        let mut req_failures: Vec<(Span, VerifyResult)> = Vec::new();
        for c in &ld.contracts {
            if matches!(c.kind, ContractKind::Requires) {
                match encode::encode_expr_with_ctx(&c.expr, &ctx) {
                    Ok(t) => backend.assert(Assertion {
                        formula: t,
                        label: Some(format!("lemma_requires@{}", c.span.start)),
                    }),
                    Err(e) => {
                        // Ф.8.2 (Plan 33.6): lemma requires encoding fail →
                        // EncodingFailed (NOT silent flag). Без этого лемма
                        // становилась vacuously proven (Z3 без контекста доказывает что угодно).
                        let reason = match e {
                            super::encode::EncodingError::Unsupported(s) => s,
                        };
                        req_failures.push((c.span, VerifyResult::EncodingFailed(
                            format!("[CONTRACT_UNSUPPORTED] lemma requires: {}", reason))));
                        requires_failed = true;
                    }
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
        // Ф.8.2 (Plan 33.6): эмитим req_failures (E2401) первыми — пользователь
        // увидит точную причину почему requires не encoded.
        results.extend(req_failures);
        for c in &ld.contracts {
            if !matches!(c.kind, ContractKind::Ensures) { continue; }
            if requires_failed {
                results.push((c.span, VerifyResult::EncodingFailed(
                    "[CONTRACT_UNSUPPORTED] lemma ensures skipped: requires not encoded".into())));
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
            is_opaque: fd.is_opaque,
            fuel: fd.fuel,
        });
    }
    out
}

/// Ф.4.2 (Plan 33.6): собрать все `#trusted external fn` модуля в реестр для encoder'а.
pub(super) fn collect_trusted_fns(module: &Module) -> std::collections::HashMap<String, encode::TrustedFnInfo> {
    let mut out = std::collections::HashMap::new();
    for item in &module.items {
        let Item::Fn(fd) = item else { continue };
        if !fd.is_trusted || !fd.is_external { continue; }
        let param_sorts = fd.params.iter().map(|p| type_to_sort(&p.ty)).collect();
        let return_sort = fd.return_type.as_ref().map(type_to_sort).unwrap_or(SortRef::Int);
        let ensures_exprs: Vec<_> = fd.contracts.iter()
            .filter(|c| matches!(c.kind, ContractKind::Ensures))
            .map(|c| c.expr.clone())
            .collect();
        out.insert(fd.name.clone(), encode::TrustedFnInfo {
            param_names: fd.params.iter().map(|p| p.name.clone()).collect(),
            param_sorts,
            return_sort,
            ensures_exprs,
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

        loop {
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
                    // Ф.3.6 (Plan 33.6): pinpoint failing calc step с pretty-print
                    // previous/expected expressions.
                    let cex = format_counterexample(&model);
                    let prev_pretty = enc_a.pretty();
                    let expected_pretty = enc_b.pretty();
                    let rel_human = match rel {
                        crate::ast::CalcRel::Eq => "==",
                        crate::ast::CalcRel::Lt => "<",
                        crate::ast::CalcRel::Le => "<=",
                        crate::ast::CalcRel::Gt => ">",
                        crate::ast::CalcRel::Ge => ">=",
                    };
                    let msg = format!(
                        "calc proof step {} failed:\n  \
                         previous (step {}): {}\n  \
                         expected ({} step {}): {}\n  \
                         cannot prove: {} {} {}\n  \
                         counterexample: {}",
                        i + 1, i, prev_pretty,
                        rel_human, i + 1, expected_pretty,
                        prev_pretty, rel_human, expected_pretty,
                        cex);
                    results.push((step_b.span, VerifyResult::Disproved(model, msg)));
                }
                SatResult::Unknown(reason) => {
                    results.push((step_b.span, VerifyResult::Unknown(
                        format!("calc step {}→{}: {}", i, i + 1, unknown_to_diag_message(reason)))));
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
    b.stmts.iter().all(|s| matches!(s, Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. }))
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

/// Plan 33.9 Ф.4: check whether an SMT term references any `_pure_fn_X` UF
/// where fn `X` is marked `is_opaque` in `pure_fns`. Used to upgrade
/// `Unknown(NotAttempted)` → `Unknown(UnsupportedTheory)` so TrivialBackend
/// emits W2402 instead of staying silent when opaque calls are unresolvable.
fn term_references_opaque_uf(
    term: &SmtTerm,
    pure_fns: &std::collections::HashMap<String, encode::PureFnInfo>,
) -> Option<String> {
    match term {
        SmtTerm::App(op, args) => {
            // Check if this App is an opaque UF call: op starts with "_pure_fn_"
            // and the corresponding fn is is_opaque.
            if let Some(fn_name) = op.strip_prefix("_pure_fn_") {
                // Strip possible fuel suffix: _pure_fn_X_fuel0 → X
                let base_name = fn_name.split("_fuel").next().unwrap_or(fn_name);
                if let Some(info) = pure_fns.get(base_name) {
                    if info.is_opaque {
                        return Some(base_name.to_string());
                    }
                }
            }
            // Recurse into args.
            for a in args {
                if let Some(name) = term_references_opaque_uf(a, pure_fns) {
                    return Some(name);
                }
            }
            None
        }
        SmtTerm::Forall(_, patterns, body) => {
            for pat_group in patterns {
                for p in pat_group {
                    if let Some(name) = term_references_opaque_uf(p, pure_fns) {
                        return Some(name);
                    }
                }
            }
            term_references_opaque_uf(body, pure_fns)
        }
        _ => None,
    }
}

/// Plan 33.9 Ф.6: собрать все `reveal name` statements из fn body.
/// Возвращает (name, span). Used для lints (dup reveal, reveal-non-opaque).
fn collect_reveal_stmts_in_body(body: &FnBody) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    if let FnBody::Block(b) = body { collect_reveal_in_block(b, &mut out); }
    out
}

fn collect_reveal_in_block(b: &Block, out: &mut Vec<(String, Span)>) {
    for s in &b.stmts {
        if let Stmt::Reveal { name, span } = s {
            out.push((name.clone(), *span));
        } else if let Stmt::Let(d) = s {
            collect_reveal_in_expr(&d.value, out);
        } else if let Stmt::Expr(e) = s {
            collect_reveal_in_expr(e, out);
        }
    }
}

fn collect_reveal_in_expr(e: &Expr, out: &mut Vec<(String, Span)>) {
    use crate::ast::ElseBranch;
    match &e.kind {
        ExprKind::Block(b) => collect_reveal_in_block(b, out),
        ExprKind::If { cond, then, else_: Some(el), .. } => {
            collect_reveal_in_expr(cond, out);
            collect_reveal_in_block(then, out);
            match el {
                ElseBranch::Block(b) => collect_reveal_in_block(b, out),
                ElseBranch::If(ie) => collect_reveal_in_expr(ie, out),
            }
        }
        ExprKind::If { cond, then, else_: None, .. } => {
            collect_reveal_in_expr(cond, out);
            collect_reveal_in_block(then, out);
        }
        _ => {}
    }
}

/// Plan 33.9 Ф.5: emit body axiom for an opaque fn triggered by `reveal`.
///
/// Asserts:  forall params. _pure_fn_X(params) == body  :pattern { _pure_fn_X(params) }
///
/// The trigger on the UF application prevents matching loops: Z3 only instantiates
/// the forall when it sees the UF applied to ground terms, not infinitely.
fn emit_opaque_body_axiom(
    pfd: &FnDecl,
    info: &encode::PureFnInfo,
    ctx: &encode::EncodeCtx,
    backend: &mut dyn SmtBackend,
) {
    let body_expr = match &pfd.body {
        FnBody::Expr(e) => Some(e as &Expr),
        FnBody::Block(b) if b.stmts.is_empty() => b.trailing.as_deref(),
        _ => None,
    };
    let Some(body_e) = body_expr else { return };
    let Ok(body_term) = encode::encode_expr_with_ctx(body_e, ctx) else { return };
    let param_vars: Vec<SmtTerm> = info.param_names.iter()
        .map(|n| SmtTerm::Var(n.clone())).collect();
    let uf_name = encode::pure_fn_uf_name(&pfd.name);
    let uf_app = SmtTerm::App(uf_name, param_vars);
    let eq_body = SmtTerm::eq(uf_app.clone(), body_term);
    let binders: Vec<(String, SortRef)> = info.param_names.iter()
        .zip(info.param_sorts.iter())
        .map(|(n, s)| (n.clone(), s.clone()))
        .collect();
    // Trigger on UF app: Z3 instantiates only when it encounters _pure_fn_X(args).
    let patterns = if binders.is_empty() { vec![] } else { vec![vec![uf_app]] };
    let axiom = if binders.is_empty() { eq_body } else {
        SmtTerm::Forall(binders, patterns, Box::new(eq_body))
    };
    backend.assert(Assertion {
        formula: axiom,
        label: Some(format!("reveal_body@{}", pfd.name)),
    });
}

/// Plan 33.9 Ф.5: emit fuel-chain axioms for `#opaque #fuel(n)` fn after `reveal`.
///
/// Strategy (Dafny-style fuel chain):
///   _pure_fn_X_fuel0  — UF, no axiom (cutoff at depth 0)
///   _pure_fn_X_fuel1  — body where recursive calls → _pure_fn_X_fuel0
///   _pure_fn_X_fuel2  — body where recursive calls → _pure_fn_X_fuel1
///   ...
///   _pure_fn_X        — body where recursive calls → _pure_fn_X_fuel(n-1)
///
/// Body is encoded normally (recursive call → `_pure_fn_X(args)` UF), then
/// we apply `substitute_uf_name` to redirect recursive calls to the right level.
fn emit_fuel_chain_axioms(
    pfd: &FnDecl,
    info: &encode::PureFnInfo,
    fuel: u32,
    ctx: &encode::EncodeCtx,
    backend: &mut dyn SmtBackend,
) {
    let body_expr = match &pfd.body {
        FnBody::Expr(e) => Some(e as &Expr),
        FnBody::Block(b) if b.stmts.is_empty() => b.trailing.as_deref(),
        _ => None,
    };
    let Some(body_e) = body_expr else {
        // No body → fall back to plain reveal axiom.
        emit_opaque_body_axiom(pfd, info, ctx, backend);
        return;
    };

    let base_uf = encode::pure_fn_uf_name(&pfd.name);
    let param_sorts = &info.param_sorts;
    let return_sort = &info.return_sort;
    let param_names = &info.param_names;

    // Declare intermediate fuel-level UFs: _pure_fn_X_fuel0 .. _pure_fn_X_fuel(n-1).
    for k in 0..fuel {
        let fuel_uf = format!("{}_fuel{}", base_uf, k);
        backend.declare_function(&fuel_uf, param_sorts, return_sort.clone());
    }

    // Encode body once (recursive self-call → _pure_fn_X, because that's what the
    // encoder emits for opaque/non-body-inlined calls). Then use substitute_uf_name
    // to redirect the recursive calls to the appropriate fuel level.
    let Ok(base_body_term) = encode::encode_expr_with_ctx(body_e, ctx) else {
        emit_opaque_body_axiom(pfd, info, ctx, backend);
        return;
    };

    // Emit axiom for each fuel level (1..=fuel) and the top-level base_uf.
    // Level k means: _pure_fn_X_fuel(k) = body[ _pure_fn_X → _pure_fn_X_fuel(k-1) ]
    // The final level uses base_uf itself (no suffix).
    for level in 1..=(fuel as usize) {
        let axiom_uf = if level == fuel as usize {
            base_uf.clone()
        } else {
            format!("{}_fuel{}", base_uf, level)
        };
        let cutoff_uf = format!("{}_fuel{}", base_uf, level - 1);
        // Redirect recursive calls in the body: _pure_fn_X → _pure_fn_X_fuel(level-1).
        let body_at_level = substitute_uf_name(&base_body_term, &base_uf, &cutoff_uf);

        let param_vars: Vec<SmtTerm> = param_names.iter()
            .map(|n| SmtTerm::Var(n.clone())).collect();
        let uf_app = SmtTerm::App(axiom_uf.clone(), param_vars);
        let eq_body = SmtTerm::eq(uf_app.clone(), body_at_level);
        let binders: Vec<(String, SortRef)> = param_names.iter()
            .zip(param_sorts.iter())
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect();
        let patterns = if binders.is_empty() { vec![] } else { vec![vec![uf_app]] };
        let axiom = if binders.is_empty() { eq_body } else {
            SmtTerm::Forall(binders, patterns, Box::new(eq_body))
        };
        backend.assert(Assertion {
            formula: axiom,
            label: Some(format!("fuel_chain@{}:level{}", pfd.name, level)),
        });
    }
}

/// Plan 33.9 Ф.5: substitute all `SmtTerm::App(from_op, args)` →
/// `SmtTerm::App(to_op, args)` recursively (op-name substitution only).
fn substitute_uf_name(term: &SmtTerm, from_op: &str, to_op: &str) -> SmtTerm {
    match term {
        SmtTerm::App(op, args) => {
            let new_args: Vec<SmtTerm> = args.iter()
                .map(|a| substitute_uf_name(a, from_op, to_op))
                .collect();
            let new_op = if op == from_op { to_op.to_string() } else { op.clone() };
            SmtTerm::App(new_op, new_args)
        }
        SmtTerm::Forall(binders, patterns, body) => {
            let new_body = substitute_uf_name(body, from_op, to_op);
            let new_patterns: Vec<Vec<SmtTerm>> = patterns.iter()
                .map(|pv| pv.iter().map(|p| substitute_uf_name(p, from_op, to_op)).collect())
                .collect();
            SmtTerm::Forall(binders.clone(), new_patterns, Box::new(new_body))
        }
        _ => term.clone(),
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
    for s in &b.stmts { collect_loop_decreases_in_stmt(s, out); }
    if let Some(e) = &b.trailing { collect_loop_decreases_in_expr(e, out); }
}

/// Plan 33.8 Ф.6.4: спуск во все stmt-формы, не только `Stmt::Expr` —
/// иначе цикл в `let x = while...` / `return while...` пропускался и его
/// well-foundedness не проверялась молча.
fn collect_loop_decreases_in_stmt(s: &Stmt, out: &mut Vec<(Span, Expr, Vec<(String, i64)>)>) {
    match s {
        Stmt::Expr(e) => collect_loop_decreases_in_expr(e, out),
        Stmt::Let(ld) => collect_loop_decreases_in_expr(&ld.value, out),
        Stmt::Return { value: Some(v), .. } => collect_loop_decreases_in_expr(v, out),
        Stmt::Assign { value, .. } => collect_loop_decreases_in_expr(value, out),
        _ => {}
    }
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
    /// Plan 33.8 Ф.2.1: тело цикла вне sound-envelope havoc-модели
    /// (составное `*=`/`/=`, присваивание во вложенном if/блоке/цикле,
    /// повторное присваивание). Если true — preservation fail-safe
    /// возвращает Warning, а не Proven.
    model_incomplete: bool,
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
    for s in &b.stmts { collect_loop_preservation_in_stmt(s, out); }
    if let Some(e) = &b.trailing { collect_loop_preservation_in_expr(e, out); }
}

/// Plan 33.8 Ф.6.4: спуск во все stmt-формы, не только `Stmt::Expr` —
/// иначе цикл в `let x = while...` / `return while...` пропускался и его
/// invariant preservation не проверялась молча.
fn collect_loop_preservation_in_stmt(s: &Stmt, out: &mut Vec<LoopPreservationTarget>) {
    match s {
        Stmt::Expr(e) => collect_loop_preservation_in_expr(e, out),
        Stmt::Let(ld) => collect_loop_preservation_in_expr(&ld.value, out),
        Stmt::Return { value: Some(v), .. } => collect_loop_preservation_in_expr(v, out),
        Stmt::Assign { value, .. } => collect_loop_preservation_in_expr(value, out),
        _ => {}
    }
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
                model_incomplete: loop_body_model_incomplete(body),
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
                model_incomplete: loop_body_model_incomplete(body),
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
                model_incomplete: loop_body_model_incomplete(body),
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

/// Plan 33.8 Ф.2.1: выходит ли тело цикла за пределы sound-envelope
/// havoc-модели `verify_loop_preservation`.
///
/// Модель `extract_body_assignments` корректна ТОЛЬКО для тела, которое —
/// плоский список first-level `Stmt::Assign` (op Assign/Add/Sub, target —
/// Ident, каждая переменная присваивается не более одного раза). Всё
/// прочее — составное `*=`/`/=`, присваивание во вложенном if/блоке/цикле/
/// match, повторное присваивание — havoc-набор не покрывает, и инвариант
/// «доказался» бы на устаревшем значении немоделированной переменной.
/// В таких случаях preservation обязана fail-safe вернуть Warning.
fn loop_body_model_incomplete(body: &Block) -> bool {
    let mut first_level_assigned: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for s in &body.stmts {
        match s {
            Stmt::Assign { target, op, value, .. } => {
                let simple_op = matches!(op, AssignOp::Assign | AssignOp::Add | AssignOp::Sub);
                let ExprKind::Ident(name) = &target.kind else { return true };
                if !simple_op { return true; }
                if !first_level_assigned.insert(name.clone()) { return true; }
                // rhs с присваиванием через block-выражение — вне модели.
                if expr_has_assignment(value) { return true; }
            }
            // `let` создаёт свежий локал — безопасен для soundness (худший
            // случай — свободная переменная → imprecision → Unknown, не
            // ложный Proven); но его значение может прятать присваивание.
            Stmt::Let(ld) => {
                if expr_has_assignment(&ld.value) { return true; }
            }
            // Любой иной stmt: если несёт присваивание где-либо в поддереве
            // (вложенный if/цикл/match) — вне модели.
            other => {
                if stmt_has_assignment(other) { return true; }
            }
        }
    }
    if let Some(t) = &body.trailing {
        if expr_has_assignment(t) { return true; }
    }
    false
}

/// Plan 33.8 Ф.2.1: содержит ли блок `Stmt::Assign` где-либо в поддереве.
fn block_has_assignment(b: &Block) -> bool {
    b.stmts.iter().any(stmt_has_assignment)
        || b.trailing.as_deref().map_or(false, expr_has_assignment)
}

fn stmt_has_assignment(s: &Stmt) -> bool {
    match s {
        Stmt::Assign { .. } => true,
        Stmt::Let(ld) => expr_has_assignment(&ld.value),
        Stmt::Expr(e) => expr_has_assignment(e),
        Stmt::Return { value, .. } => value.as_ref().map_or(false, expr_has_assignment),
        Stmt::Throw { value, .. } => expr_has_assignment(value),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => expr_has_assignment(body),
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => expr_has_assignment(expr),
        Stmt::Break(_) | Stmt::Continue(_)
        | Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => false,
    }
}

fn else_branch_has_assignment(eb: &Option<ElseBranch>) -> bool {
    match eb {
        Some(ElseBranch::Block(b)) => block_has_assignment(b),
        Some(ElseBranch::If(ei)) => expr_has_assignment(ei),
        None => false,
    }
}

fn expr_has_assignment(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Block(b) => block_has_assignment(b),
        ExprKind::If { cond, then, else_ } => {
            expr_has_assignment(cond)
                || block_has_assignment(then)
                || else_branch_has_assignment(else_)
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            expr_has_assignment(scrutinee)
                || block_has_assignment(then)
                || else_branch_has_assignment(else_)
        }
        ExprKind::While { cond, body, .. } => {
            expr_has_assignment(cond) || block_has_assignment(body)
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            expr_has_assignment(scrutinee) || block_has_assignment(body)
        }
        ExprKind::Loop { body, .. } => block_has_assignment(body),
        ExprKind::For { iter, body, .. } => {
            expr_has_assignment(iter) || block_has_assignment(body)
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            expr_has_assignment(iter) || block_has_assignment(body)
        }
        ExprKind::Match { scrutinee, arms } => {
            expr_has_assignment(scrutinee)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().map_or(false, expr_has_assignment)
                        || match &arm.body {
                            MatchArmBody::Expr(ae) => expr_has_assignment(ae),
                            MatchArmBody::Block(b) => block_has_assignment(b),
                        }
                })
        }
        ExprKind::Binary { left, right, .. } => {
            expr_has_assignment(left) || expr_has_assignment(right)
        }
        ExprKind::Unary { operand, .. } => expr_has_assignment(operand),
        ExprKind::Call { func, args, .. } => {
            expr_has_assignment(func)
                || args.iter().any(|a| expr_has_assignment(a.expr()))
        }
        // Прочие ExprKind (литералы, Ident, Member, Index, As, лямбды и т.п.)
        // не несут `Stmt::Assign` в реалистичном теле цикла. Экзотический
        // случай (assignment внутри closure/spawn в теле цикла) — V2.
        _ => false,
    }
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

    // Plan 33.8 Ф.2.1: fail-safe — тело цикла вне sound-envelope havoc-модели.
    // Возвращаем Warning (W2402) вместо ложного Proven: havoc-набор не
    // покрывает составные/вложенные/повторные присваивания, и инвариант
    // «доказался» бы на устаревшем значении немоделированной переменной.
    if lp.model_incomplete {
        results.push((lp.span, VerifyResult::Warning(
            "сохранение инварианта цикла НЕ проверено [W2402]: тело цикла \
             содержит присваивание вне модели верификатора (составное \
             `*=`/`/=`, присваивание во вложенном if/блоке/цикле/match, \
             либо повторное присваивание одной переменной).\n  \
             Упростите тело цикла или пометьте функцию #unverified.".into())));
        return results;
    }

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
            Err(e) => {
                // Ф.7.3 (Plan 33.6): invariant не encodable → W2402 (раньше silent return).
                let reason = match e {
                    super::encode::EncodingError::Unsupported(s) => s,
                };
                let msg = format!(
                    "loop invariant preservation НЕ проверена [W2402]: invariant \
                     не закодирован в SMT: {}.\n  \
                     Упростите invariant или используйте #unverified.",
                    reason);
                results.push((lp.span, VerifyResult::Warning(msg)));
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
            Err(e) => {
                // Ф.7.3 (Plan 33.6): body rhs не encodable → W2402 (раньше silent return).
                let reason = match e {
                    super::encode::EncodingError::Unsupported(s) => s,
                };
                let msg = format!(
                    "loop invariant preservation НЕ проверена [W2402]: body assignment \
                     `{} := ...` не закодирован в SMT: {}.\n  \
                     Упростите rhs или используйте #unverified.",
                    var, reason);
                results.push((lp.span, VerifyResult::Warning(msg)));
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
            Err(e) => {
                // Ф.7.3 (Plan 33.6): non-encodable invariant in post-check → W2402.
                let reason = match e {
                    super::encode::EncodingError::Unsupported(s) => s,
                };
                let msg = format!(
                    "loop invariant post-state НЕ проверен [W2402]: invariant не \
                     закодирован в SMT (post): {}.",
                    reason);
                results.push((lp.span, VerifyResult::Warning(msg)));
            }
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
                // Unbounded integer (Nova `int` = i64) — Z3 Int sort (global decision 33.1).
                "int" | "i64" | "money" | "nat" => SortRef::Int,
                // Plan 33.7: sized integer types → BitVec sort.
                // V2: signed=true для i8/i16/i32, false для u8/u16/u32/u64.
                "u8"           => SortRef::BitVec { width: 8,  signed: false },
                "i8"           => SortRef::BitVec { width: 8,  signed: true },
                "u16"          => SortRef::BitVec { width: 16, signed: false },
                "i16"          => SortRef::BitVec { width: 16, signed: true },
                "u32"          => SortRef::BitVec { width: 32, signed: false },
                "i32"          => SortRef::BitVec { width: 32, signed: true },
                "u64" | "i32u" | "usize" => SortRef::BitVec { width: 64, signed: false },
                "bool" => SortRef::Bool,
                "str"  => SortRef::Str,
                "f32"  => SortRef::F32,
                "f64"  => SortRef::F64,
                _      => SortRef::Int,
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
    static EMPTY_TRUSTED: std::sync::OnceLock<std::collections::HashMap<String, super::encode::TrustedFnInfo>> = std::sync::OnceLock::new();
    let empty_fns = EMPTY_FNS.get_or_init(std::collections::HashMap::new);
    let empty_trusted = EMPTY_TRUSTED.get_or_init(std::collections::HashMap::new);
    let ctx = super::encode::EncodeCtx { pure_views, pure_fns: empty_fns, trusted_fns: empty_trusted, var_sorts: std::collections::HashMap::new() };
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
/// Ф.7.4 (Plan 33.6): возвращает (errors, warnings).
/// errors — inconsistent axioms (Unsat). warnings — Unknown под Trivial (W2402).
pub fn check_axiom_consistency(module: &Module) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut warnings: Vec<Diagnostic> = Vec::new();
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
                SatResult::Sat(_) => {
                    // axioms consistent — реально проверено backend'ом.
                }
                SatResult::Unknown(reason) => {
                    // Ф.7.4 (Plan 33.6): Unknown под Trivial или Z3 timeout —
                    // axiom consistency НЕ проверена, эмит W2402 чтобы user знал.
                    let reason_str = match reason {
                        UnknownReason::NotAttempted(s) => format!(
                            "TrivialBackend не reasoning'ует ({})", s),
                        UnknownReason::Timeout => "SMT timeout".to_string(),
                        UnknownReason::NonLinearArithmetic => "non-linear arithmetic".to_string(),
                        UnknownReason::UnsupportedTheory(s) => format!("unsupported theory: {}", s),
                        UnknownReason::BackendError(s) => format!("backend error: {}", s),
                    };
                    let msg = format!(
                        "axiom consistency для effect `{}` НЕ проверена [W2402]: {}.\n  \
                         Используйте Z3 backend (`cargo build --features z3-backend` + \
                         `NOVA_SMT_BACKEND=z3`) для полной проверки.",
                        td.name, reason_str);
                    warnings.push(Diagnostic::new(msg, td.span));
                }
            }
        }
    }

    (diagnostics, warnings)
}

/// Ф.7.2 (Plan 33.6 / V4 close): после introduction `_old_<x>` как
/// отдельных SMT vars (с frame axiom для non-modifies params),
/// substitute_old превращается в no-op identity-fn — preserved для
/// API compat, но больше не делает unsound подстановку `_old_x → x`.
pub(super) fn substitute_old(t: &SmtTerm) -> SmtTerm {
    t.clone()
}

/// Ф.16.1 (Plan 33.6): найти первую `exists`-var в AST-expression.
/// Используется для witness extraction в proven ensures.
pub(super) fn find_exists_var(e: &crate::ast::Expr) -> Option<String> {
    use crate::ast::ExprKind::*;
    match &e.kind {
        Exists { var, .. } => Some(var.clone()),
        Binary { left, right, .. } => {
            find_exists_var(left).or_else(|| find_exists_var(right))
        }
        Unary { operand, .. } => find_exists_var(operand),
        Call { func, args, .. } => {
            find_exists_var(func).or_else(|| {
                args.iter().find_map(|a| find_exists_var(a.expr()))
            })
        }
        If { cond, then, else_ } => {
            find_exists_var(cond)
                .or_else(|| then.trailing.as_ref().and_then(|t| find_exists_var(t)))
                .or_else(|| match else_ {
                    Some(crate::ast::ElseBranch::Block(b)) => {
                        b.trailing.as_ref().and_then(|t| find_exists_var(t))
                    }
                    Some(crate::ast::ElseBranch::If(e2)) => find_exists_var(e2),
                    None => None,
                })
        }
        _ => None,
    }
}

/// Plan 33.8 Ф.1.4: тип `nat` (неотрицательный int). `type_to_sort`
/// мэпит `nat` в `SortRef::Int`, теряя неотрицательность — её
/// восстанавливает аксиома `nat >= 0` в `verify_fn`.
fn type_ref_is_nat(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Named { path, .. } if path.len() == 1 && path[0] == "nat")
}

pub(super) fn type_to_sort(ty: &TypeRef) -> SortRef {
    match ty {
        TypeRef::Named { path, .. } if path.len() == 1 => match path[0].as_str() {
            "int" | "i64" | "money" | "nat" => SortRef::Int,
            // Plan 33.7: sized integer types → BitVec.
            // V2: signed=true для i8/i16/i32, false для u8/u16/u32/u64.
            "u8"           => SortRef::BitVec { width: 8,  signed: false },
            "i8"           => SortRef::BitVec { width: 8,  signed: true },
            "u16"          => SortRef::BitVec { width: 16, signed: false },
            "i16"          => SortRef::BitVec { width: 16, signed: true },
            "u32"          => SortRef::BitVec { width: 32, signed: false },
            "i32"          => SortRef::BitVec { width: 32, signed: true },
            "u64" | "usize"            => SortRef::BitVec { width: 64, signed: false },
            "bool" => SortRef::Bool,
            "str"  => SortRef::Str,
            "f32"  => SortRef::F32,
            "f64"  => SortRef::F64,
            other  => SortRef::Named(other.into()),
        },
        _ => SortRef::Named("opaque".into()),
    }
}

pub(super) fn format_counterexample(model: &Model) -> String {
    if model.bindings.is_empty() {
        return "values not extracted (TrivialBackend); enable Z3 \
                backend for full counterexample".into();
    }
    // Ф.3.1 (Plan 33.6): minimisation + ranking.
    // 1. Filter out internal Skolem vars (_old_*, _havoc_*, _trusted_*, _entry_*)
    //    из top-level diagnostic — они для internal reasoning, user их не понимает.
    // 2. Sort: parameters first (no _ prefix), intermediate vars last (_ prefix).
    // 3. Cap output: первые 10 binding'ов; остальные — суммарный счёт.
    let mut user_facing: Vec<(&String, &ModelValue)> = model.bindings.iter()
        .filter(|(name, _)| !name.starts_with("_skolem_"))
        .collect();
    user_facing.sort_by(|(a, _), (b, _)| {
        let a_internal = a.starts_with('_');
        let b_internal = b.starts_with('_');
        match (a_internal, b_internal) {
            (false, true) => std::cmp::Ordering::Less,    // params first
            (true, false) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        }
    });
    let total = user_facing.len();
    let mut parts = Vec::new();
    for (name, val) in user_facing.iter().take(10) {
        let v = match val {
            ModelValue::Int(n) => n.to_string(),
            ModelValue::Bool(b) => b.to_string(),
            ModelValue::Str(s) => format!("\"{}\"", s),
            ModelValue::Unknown => "?".into(),
        };
        parts.push(format!("{} = {}", name, v));
    }
    if total > 10 {
        parts.push(format!("... ({} more bindings)", total - 10));
    }
    parts.join(", ")
}

pub(super) fn unknown_to_diag_message(reason: UnknownReason) -> String {
    match reason {
        UnknownReason::Timeout => {
            "SMT solver hit timeout. \
             Suggestions: (1) extract sub-expressions into `#pure` helper \
             functions (composition is verified); (2) increase \
             `#verify_timeout(N)`; (3) mark `#unverified` if verification is \
             intentionally complex.".into()
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
                     impl-shortcuts). Use the Z3 backend \
                     (`REQUIRES_SMT_BACKEND z3`), or mark `#unverified`.", s)
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

    // Ф.12.3 (Plan 33.6): reset module-scoped subsumption cache.
    // try_prove_module_cached использует thread-local cache, который live
    // на весь verify_module call. Stats emit'ятся в конце.
    super::subsumption_cache::reset_module_cache();

    // Plan 33.3 Р¤.9.5: РїСЂРѕРІРµСЂРєР° consistency axiom'РѕРІ РґРѕ per-fn verify.
    // Р•СЃР»Рё axioms СЌС„С„РµРєС‚Р° inconsistent (Z3 в†' UNSAT) в†' compile-error,
    // skip РІСЃРµС… РѕСЃС‚Р°Р»СЊРЅС‹С… verify'РµРІ (Р»СЋР±Р°СЏ formula С‚СЂРёРІРёР°Р»СЊРЅРѕ РґРѕРєР°Р·СѓРµРјР°
    // РїРѕРґ inconsistent assumptions).
    let (inconsistency_errors, inconsistency_warnings) = check_axiom_consistency(module);
    let has_inconsistent_axioms = !inconsistency_errors.is_empty();
    for e in inconsistency_errors {
        report.errors.push(e);
    }
    // Ф.7.4: W2402 axiom consistency under Trivial backend — НЕ ломаем компиляцию.
    for w in inconsistency_warnings {
        report.warnings.push(w);
    }
    if has_inconsistent_axioms {
        return report;
    }

    // Plan 33.4 P0-1 (Р¤.9.7 V1): РІРµСЂРёС„РёРєР°С†РёСЏ `with #verify E = handler` bindings.
    // Ф.7.5 (Plan 33.6): verify_handlers возвращает (errors, warnings).
    let (handler_errors, handler_warnings) = super::handler_exec::verify_handlers(module);
    for diag in handler_errors {
        report.errors.push(diag);
    }
    for w in handler_warnings {
        report.warnings.push(w);
    }
    if !report.errors.is_empty() {
        return report;
    }

    // Р¤.3: РІС‹С‡РёСЃР»СЏРµРј SCC purity РѕРґРёРЅ СЂР°Р· РЅР° РјРѕРґСѓР»СЊ, С‡С‚РѕР±С‹ РЅРµ РїРµСЂРµСЃС‡РёС‚С‹РІР°С‚СЊ
    // СЂРµРєСѓСЂСЃРёРІРЅС‹Р№ РѕР±С…РѕРґ AST РЅР° РєР°Р¶РґСѓСЋ С„СѓРЅРєС†РёСЋ (overhead + СЂРёСЃРє stack overflow).
    let inferred_pure = infer_pure_fns_scc(module);

    // Plan 33.3 Ф.13: #must_verify_module -- все функции MustVerify.
    let module_strict = module.attrs.iter().any(|a| matches!(a.kind, ModuleAttrKind::MustVerifyModule));

    // Ф.3.4 (Plan 33.6): #proof_budget module-level.
    let (module_timeout_ms, module_vc_count_max) = module.attrs.iter()
        .find_map(|a| if let ModuleAttrKind::ProofBudget { timeout_ms, vc_count_max } = &a.kind {
            Some((*timeout_ms, *vc_count_max))
        } else { None })
        .unwrap_or((None, None));
    let mut module_vc_used: u32 = 0;

    for item in &module.items {
        if let Item::Fn(fd) = item {
            if fd.contracts.is_empty() { continue; }
            // Plan 33.14: контекст для атрибуции cross-check-расхождений.
            super::crosscheck::set_current_fn(Some(fd.name.clone()));
            // Plan 33.3 Ф.13: #trusted external fn -- контракты axioms, SMT-verify пропускается.
            if fd.is_trusted && fd.is_external { continue; }
            // Ф.4.1 (Plan 33.6): #unverified fn внутри #must_verify_module → conflict error.
            if module_strict && matches!(fd.verify_mode, VerifyMode::Unverified) {
                let span = fd.contracts.first().map(|c| c.span).unwrap_or(fd.span);
                let msg = format!(
                    "fn '{}' помечена `#unverified` внутри `#must_verify_module` [E2403]: \
                     нельзя отказаться от верификации в strict-модуле. \
                     Уберите `#unverified` или перенесите fn в другой модуль.",
                    fd.name);
                report.errors.push(Diagnostic::new(msg, span));
                continue;
            }
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
            // Ф.3.4: per-fn timeout = fn-level override > module budget > pipeline default.
            let fn_timeout_ms = fd.verify_timeout_ms
                .map(|ms| ms as u32)
                .or(module_timeout_ms)
                .unwrap_or(pipeline.timeout_ms);
            // Ф.3.4: vc_count_max budget check.
            if let Some(max_vc) = module_vc_count_max {
                let fn_vc_count = fd.contracts.len() as u32;
                if module_vc_used + fn_vc_count > max_vc {
                    let span = fd.contracts.first().map(|c| c.span).unwrap_or(fd.span);
                    let msg = format!(
                        "fn '{}': #proof_budget vc_count_max={} превышен \
                         (использовано {}, попытка добавить {}) [BudgetExceeded]; \
                         увеличьте vc_count_max или вынесите fn в другой модуль.",
                        fd.name, max_vc, module_vc_used, fn_vc_count);
                    report.errors.push(Diagnostic::new(msg, span));
                    continue;
                }
                module_vc_used += fn_vc_count;
            }
let t0 = std::time::Instant::now();
            // Ф.3.4: если timeout отличается от default — создаём временный pipeline.
            let results = if fn_timeout_ms != pipeline.timeout_ms {
                VerificationPipeline::with_timeout(fn_timeout_ms).verify_fn(module, fd, &inferred_pure)
            } else {
                pipeline.verify_fn(module, fd, &inferred_pure)
            };
            let _elapsed_ms = t0.elapsed().as_millis() as u64;
            for (span, vr) in results {
                match vr {
                    VerifyResult::Proven => {
                        report.proven.push((fd.name.clone(), span));
                    }
                    VerifyResult::Disproved(_, cex) => {
                        // Plan 33.3 Р¤.9.10: AI-friendly format.
                        // Ф.3.3 (Plan 33.6): pattern-aware suggested fixes из AST.
                        let pattern_suggestions = super::suggest::suggest_fixes(fd);
                        let pattern_block = if pattern_suggestions.is_empty() {
                            String::new()
                        } else {
                            let mut s = String::from("\n  pattern-aware hints:\n");
                            for (i, h) in pattern_suggestions.iter().enumerate() {
                                s.push_str(&format!("    {}. {}\n", i + 1, h));
                            }
                            s
                        };
                        let msg = format!(
                            "contract violation in `{}`:\n  counterexample: {}\n  \
                             suggestions:\n    1. Add `requires` precondition restricting input;\n    \
                             2. Fix function body to match `ensures`;\n    \
                             3. Weaken `ensures` to actual behavior;\n    \
                             4. Mark `#unverified` if intentional disprove{}",
                            fd.name, cex, pattern_block);
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
            // Plan 33.14: контекст для атрибуции cross-check-расхождений.
            super::crosscheck::set_current_fn(Some(ld.name.clone()));
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
                        // Ф.8.2 (Plan 33.6): [CONTRACT_UNSUPPORTED] префикс → E2401.
                        if reason.starts_with("[CONTRACT_UNSUPPORTED]") {
                            let display = reason.trim_start_matches("[CONTRACT_UNSUPPORTED] ");
                            report.errors.push(Diagnostic::new(
                                format!("lemma `{}` контракт не поддерживается SMT-encoder'ом [E2401]: {}",
                                    ld.name, display),
                                span));
                        } else {
                            report.errors.push(Diagnostic::new(
                                format!("lemma `{}`: тело или контракт не encodable: {}\n  \
                                         Только int/bool/str/record/binary-ops/if поддерживается в V1.",
                                    ld.name, reason),
                                span));
                        }
                    }
                    VerifyResult::Warning(msg) => {
                        report.warnings.push(Diagnostic::new(msg, span));
                    }
                }
            }
        }
    }
    // Ф.12.3 (Plan 33.6): emit cache stats как info-warning если есть hits.
    let (hits, misses) = super::subsumption_cache::module_cache_stats();
    if hits + misses > 0 && hits > 0 {
        let rate = (hits as f64) / ((hits + misses) as f64) * 100.0;
        // Не push в report.warnings — это noise для каждого compile.
        // V2: emit только под NOVA_VERIFY_STATS=1 env flag.
        if std::env::var("NOVA_VERIFY_STATS").is_ok() {
            eprintln!("[verify] cache hits={} misses={} hit-rate={:.1}%", hits, misses, rate);
        }
    }
    // Ф.17.3 (Plan 33.6): dead lemma tracking. Lemma defined but not applied.
    let mut applied_lemmas: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            for (lemma_name, _, _) in collect_apply_stmts_in_body(&fd.body) {
                applied_lemmas.insert(lemma_name);
            }
        }
    }
    for item in &module.items {
        if let Item::Lemma(ld) = item {
            if !applied_lemmas.contains(&ld.name) {
                report.warnings.push(Diagnostic::new(
                    format!("lemma `{}` определена, но не применена нигде в модуле [W2402]: \
                             dead code — удалите лемму или добавьте `apply {}(...)` в caller.",
                        ld.name, ld.name),
                    ld.span));
            }
            // Ф.24.2 (Plan 33.6): lemma без params — suspicious.
            if ld.params.is_empty() {
                report.warnings.push(Diagnostic::new(
                    format!("lemma `{}` без параметров [W2402]:\n  \
                             lemma без params обычно бесполезна — она доказывает constant\n  \
                             statement который Z3 derive'ит сам. Возможно, забыли add params.",
                        ld.name),
                    ld.span));
            }
            // Ф.31.3 (Plan 33.6): lemma c `requires false` — vacuously true,
            // но apply никогда не активирует precondition → лемма бесполезна.
            // Detect: какой-либо contract.kind == Requires с body = BoolLit(false).
            let has_false_requires = ld.contracts.iter().any(|c| {
                matches!(c.kind, crate::ast::ContractKind::Requires)
                    && matches!(c.expr.kind, ExprKind::BoolLit(false))
            });
            if has_false_requires {
                report.warnings.push(Diagnostic::new(
                    format!("lemma `{}` имеет `requires false` [W2402]:\n  \
                             precondition `false` никогда не выполняется → `apply` бесполезен\n  \
                             (никогда не активирует ensures). Удалите лемму или поправьте requires.",
                        ld.name),
                    ld.span));
            }
            // Ф.32.1 (Plan 33.6): lemma c identical requires ↔ ensures (текстуально
            // эквивалентные expression) — лемма не добавляет новой информации.
            // `lemma foo(x) requires x >= 0 ensures x >= 0` → tautology.
            let requires_exprs: Vec<&Expr> = ld.contracts.iter()
                .filter(|c| matches!(c.kind, crate::ast::ContractKind::Requires))
                .map(|c| &c.expr).collect();
            let ensures_exprs: Vec<&Expr> = ld.contracts.iter()
                .filter(|c| matches!(c.kind, crate::ast::ContractKind::Ensures))
                .map(|c| &c.expr).collect();
            // Если каждый ensures совпадает с каким-нибудь requires текстуально
            // (через pretty-print, ignore Span) — лемма tautological.
            use crate::ast::pretty::print_expr;
            if !ensures_exprs.is_empty() && !requires_exprs.is_empty() {
                let req_strs: std::collections::HashSet<String> =
                    requires_exprs.iter().map(|e| print_expr(e)).collect();
                let all_ensures_in_requires = ensures_exprs.iter()
                    .all(|e| req_strs.contains(&print_expr(e)));
                if all_ensures_in_requires {
                    report.warnings.push(Diagnostic::new(
                        format!("lemma `{}` имеет ensures идентичные requires [W2402]:\n  \
                                 лемма не добавляет новой информации (tautological refinement).\n  \
                                 Удалите или поменяйте ensures на содержательное утверждение.",
                            ld.name),
                        ld.span));
                }
            }
            // Ф.32.2 (Plan 33.6): lemma name collision с обычной fn в том же модуле.
            // Программист скорее всего ошибся (две декларации с одним именем).
            let fn_collision = module.items.iter().any(|item| {
                matches!(item, Item::Fn(fd) if fd.name == ld.name)
            });
            if fn_collision {
                report.warnings.push(Diagnostic::new(
                    format!("lemma `{}` имеет то же имя что обычная fn в модуле [W2402]:\n  \
                             apply `{}(...)` будет ссылаться на лемму, fn-вызов — на функцию.\n  \
                             Переименуйте один из них для clarity.",
                        ld.name, ld.name),
                    ld.span));
            }
            // Ф.46.1 (Plan 33.6): lemma самоприменение (self-apply) — infinite proof.
            // `lemma foo(x) { apply foo(x); ... }` — proof depends on itself.
            // Error (не warning): proof is unsound by construction.
            let self_applies = collect_apply_stmts_in_body(&ld.body);
            for (lemma_applied, _, sp) in &self_applies {
                if lemma_applied == &ld.name {
                    report.errors.push(Diagnostic::new(
                        format!("lemma `{}` применяет саму себя через `apply {}(...)` [E2408]:\n  \
                                 self-application делает proof unsound (proves what it assumes).\n  \
                                 Используйте strong induction через `apply lemma(x-1)` или удалите apply.",
                            ld.name, ld.name),
                        *sp));
                }
            }
            // Ф.48.1 (Plan 33.6): unused lemma param. Param должен встречаться
            // в каком-то contract либо body. Скорее всего программист забыл.
            {
                let mut used_names: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                collect_used_idents_in_body(&ld.body, &mut used_names);
                for c in &ld.contracts {
                    collect_used_idents_in_expr(&c.expr, &mut used_names);
                }
                for p in &ld.params {
                    if p.name.starts_with('_') { continue; } // intentional skip
                    if !used_names.contains(&p.name) {
                        report.warnings.push(Diagnostic::new(
                            format!("lemma `{}`: param `{}` не используется в contracts/body [W2402]:\n  \
                                     удалите или переименуйте в `_{}` для intentional skip.",
                                ld.name, p.name, p.name),
                            ld.span));
                    }
                }
            }
            // Ф.40.1 (Plan 33.6): lemma body == ensures expression — body просто
            // повторяет ensures. Это не bug, но прозрачно: SMT доказывает результат
            // body == ensures тавтологически. Лучше пустой body или body содержащий
            // calc/apply hints. Detect: body это Expr и canon(body) == canon(any ensures).
            use crate::ast::pretty::print_expr as pe;
            if let crate::ast::FnBody::Expr(body_expr) = &ld.body {
                let body_str = pe(body_expr);
                for c in &ld.contracts {
                    if matches!(c.kind, ContractKind::Ensures) {
                        // Если body буквально совпадает с ensures expr.
                        if body_str == pe(&c.expr) {
                            report.warnings.push(Diagnostic::new(
                                format!("lemma `{}`: body буквально совпадает с ensures [W2402]:\n  \
                                         body не добавляет proof-information — SMT доказывает\n  \
                                         тавтологически. Используйте `calc {{ }}` или `apply other_lemma`\n  \
                                         для нетривиальной reasoning chain.",
                                    ld.name),
                                ld.span));
                            break;
                        }
                    }
                }
            }
        }
        // Ф.26.3 (Plan 33.6): axiom без binders + `=> true` — vacuous.
        if let Item::Type(td) = item {
            for ax in &td.axioms {
                if ax.binders.is_empty() {
                    if let ExprKind::BoolLit(true) = ax.formula.kind {
                        report.warnings.push(Diagnostic::new(
                            format!("axiom `{}.{}`: vacuous `=> true` без binders [W2402]:\n  \
                                     добавьте binders или удалите — axiom тривиально true.",
                                td.name, ax.name),
                            ax.span));
                    }
                }
            }
            // Ф.28.3 (Plan 33.6): empty effect — methods.is_empty() и axioms.is_empty().
            if let TypeDeclKind::Effect(methods) = &td.kind {
                if methods.is_empty() && td.axioms.is_empty() {
                    report.warnings.push(Diagnostic::new(
                        format!("effect `{}` пустой (без methods и axioms) [W2402]:\n  \
                                 удалите или добавьте methods/axioms — пустой effect noop.",
                            td.name),
                        td.span));
                }
            }
        }
    }
    // Ф.19.2 + Ф.19.3 (Plan 33.6): detection trivial/redundant contracts.
    // Ф.21.2 (Plan 33.6): contradictory ensures detection.
    for item in &module.items {
        if let Item::Fn(fd) = item {
            // Ф.21.2: собрать literal-eq ensures: result == LIT.
            let mut result_eq_lit: Vec<(i64, Span)> = Vec::new();
            for c in &fd.contracts {
                match c.kind {
                    ContractKind::Requires => {
                        // Ф.19.3: `requires true` — no-op.
                        if matches!(c.expr.kind, ExprKind::BoolLit(true)) {
                            report.warnings.push(Diagnostic::new(
                                format!("fn `{}`: `requires true` тривиально true [W2402]: \
                                         удалите — это no-op.", fd.name),
                                c.span));
                        }
                        // Ф.22.1 (Plan 33.6): `requires false` — vacuous fn.
                        // Ф.26.2 (defer): для #trusted external blocked parser-level
                        // earlier; нет смысла добавлять additional check.
                        if matches!(c.expr.kind, ExprKind::BoolLit(false)) {
                            report.warnings.push(Diagnostic::new(
                                format!("fn `{}`: vacuous fn — `requires false` [W2402]:\n  \
                                         эта fn никогда не может быть вызвана (precondition \
                                         невыполним). Возможно, опечатка или TODO-stub. \
                                         Используйте `panic` body вместо `requires false`.",
                                    fd.name),
                                c.span));
                        }
                    }
                    ContractKind::Ensures => {
                        // Ф.19.2: `ensures x == x` (left структурно == right) → trivial reflexive.
                        if let ExprKind::Binary { op: BinOp::Eq, left, right } = &c.expr.kind {
                            if format!("{:?}", left.kind) == format!("{:?}", right.kind) {
                                report.warnings.push(Diagnostic::new(
                                    format!("fn `{}`: self-referential ensures [W2402]: \
                                             `expr == expr` тривиально true и не несёт смысла. \
                                             Возможно, опечатка — проверьте намерение.",
                                        fd.name),
                                    c.span));
                            }
                            // Ф.21.2: collect `result == IntLit`.
                            if let (ExprKind::Ident(name), ExprKind::IntLit(n)) =
                                (&left.kind, &right.kind)
                            {
                                if name == "result" {
                                    result_eq_lit.push((*n, c.span));
                                }
                            }
                            if let (ExprKind::IntLit(n), ExprKind::Ident(name)) =
                                (&left.kind, &right.kind)
                            {
                                if name == "result" {
                                    result_eq_lit.push((*n, c.span));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Ф.34.1 (Plan 33.6): ensures_fail на fn без Fail effect → W2402.
            // `ensures_fail` имеет смысл только если fn может бросить Fail.
            // Если в effects нет Fail — ensures_fail unreachable, дезинформирует.
            let has_fail_effect = fd.effects.iter().any(|e| {
                matches!(e, crate::ast::TypeRef::Named { path, .. }
                    if path.len() == 1 && path[0] == "Fail")
            });
            if !has_fail_effect {
                for c in &fd.contracts {
                    if matches!(c.kind, ContractKind::EnsuresFail) {
                        report.warnings.push(Diagnostic::new(
                            format!("fn `{}`: `ensures_fail` без `Fail` effect [W2402]:\n  \
                                     эта fn не может бросить Fail (нет в signature effects),\n  \
                                     значит ensures_fail unreachable. Добавьте `Fail` в effects\n  \
                                     или удалите ensures_fail.",
                                fd.name),
                            c.span));
                    }
                }
            }
            // Ф.33.1 (Plan 33.6): fn c ensures идентичными requires → W2402.
            // Аналог Ф.32.1 для fn: если каждый ensures (без `result`/`old`)
            // встречается как requires — fn tautologically refines (бесполезен).
            // Skip если ensures упоминает `result` или `old(...)` — там может быть
            // переименование, ensures осмысленный.
            {
                use crate::ast::pretty::print_expr;
                // Walker: проверяет наличие Ident("result") или Call("old", ...) в Expr.
                // Покрывает основные cases в contracts (Binary/Unary/Call/Ident).
                // Для редких (Block/If/Match в contract'е) consvervatively вернёт false
                // — это OK, потенциально ложный negative безопаснее false-positive warning.
                fn refs_result_or_old(e: &Expr) -> bool {
                    match &e.kind {
                        ExprKind::Ident(n) => n == "result",
                        ExprKind::Call { func, args, .. } => {
                            if let ExprKind::Ident(n) = &func.kind {
                                if n == "old" { return true; }
                            }
                            refs_result_or_old(func) || args.iter().any(|a| match a {
                                crate::ast::CallArg::Item(e) | crate::ast::CallArg::Spread(e)
                                    => refs_result_or_old(e),
                                _ => false,
                            })
                        }
                        ExprKind::Binary { left, right, .. } =>
                            refs_result_or_old(left) || refs_result_or_old(right),
                        ExprKind::Unary { operand, .. } => refs_result_or_old(operand),
                        _ => false,
                    }
                }
                let mut req_strs: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut ensures_pure: Vec<&Contract> = Vec::new();
                for c in &fd.contracts {
                    match c.kind {
                        ContractKind::Requires => {
                            req_strs.insert(print_expr(&c.expr));
                        }
                        ContractKind::Ensures => {
                            if !refs_result_or_old(&c.expr) {
                                ensures_pure.push(c);
                            }
                        }
                        _ => {}
                    }
                }
                if !ensures_pure.is_empty() && !req_strs.is_empty() {
                    let all_tautological = ensures_pure.iter()
                        .all(|c| req_strs.contains(&print_expr(&c.expr)));
                    if all_tautological {
                        let span = ensures_pure[0].span;
                        report.warnings.push(Diagnostic::new(
                            format!("fn `{}`: ensures (без result/old) идентичны requires [W2402]:\n  \
                                     fn tautologically refines — ensures не несёт новой информации\n  \
                                     callerу. Возможно, опечатка — ensures должен ссылаться на result.",
                                fd.name),
                            span));
                    }
                }
            }
            // Ф.21.2: если 2+ ensures с разными литералами → contradictory error.
            if result_eq_lit.len() >= 2 {
                let first_val = result_eq_lit[0].0;
                let conflicting: Vec<(i64, Span)> = result_eq_lit.iter()
                    .filter(|(n, _)| *n != first_val).copied().collect();
                if !conflicting.is_empty() {
                    let span = conflicting[0].1;
                    let conflicts_str: Vec<String> = result_eq_lit.iter()
                        .map(|(n, _)| n.to_string()).collect();
                    report.errors.push(Diagnostic::new(
                        format!("fn `{}`: contradictory ensures [E2404]: \
                                 multiple `result == LIT` ensures с разными литералами: [{}]. \
                                 Они одновременно невыполнимы — fn никогда не пройдёт verify.",
                            fd.name, conflicts_str.join(", ")),
                        span));
                }
            }
            // Ф.22.2 (Plan 33.6): redundant requires — tightness check.
            // Собрать lower bounds для каждой Var: (var → Vec<(literal, span)>).
            let mut lower_bounds: std::collections::HashMap<String, Vec<(i64, Span)>> =
                std::collections::HashMap::new();
            let mut upper_bounds: std::collections::HashMap<String, Vec<(i64, Span)>> =
                std::collections::HashMap::new();
            for c in &fd.contracts {
                if !matches!(c.kind, ContractKind::Requires) { continue; }
                if let ExprKind::Binary { op, left, right } = &c.expr.kind {
                    if let (ExprKind::Ident(name), ExprKind::IntLit(n)) =
                        (&left.kind, &right.kind)
                    {
                        match op {
                            BinOp::Ge => lower_bounds.entry(name.clone()).or_default().push((*n, c.span)),
                            BinOp::Le => upper_bounds.entry(name.clone()).or_default().push((*n, c.span)),
                            _ => {}
                        }
                    }
                }
            }
            for (var, bounds) in &lower_bounds {
                if bounds.len() < 2 { continue; }
                let max_bound = bounds.iter().map(|(n, _)| *n).max().unwrap();
                for (n, sp) in bounds {
                    if *n < max_bound {
                        report.warnings.push(Diagnostic::new(
                            format!("fn `{}`: redundant `requires {} >= {}` [W2402]:\n  \
                                     уже есть строжайшая `requires {} >= {}` — удалите эту.",
                                fd.name, var, n, var, max_bound),
                            *sp));
                    }
                }
            }
            for (var, bounds) in &upper_bounds {
                if bounds.len() < 2 { continue; }
                let min_bound = bounds.iter().map(|(n, _)| *n).min().unwrap();
                for (n, sp) in bounds {
                    if *n > min_bound {
                        report.warnings.push(Diagnostic::new(
                            format!("fn `{}`: redundant `requires {} <= {}` [W2402]:\n  \
                                     уже есть строжайшая `requires {} <= {}` — удалите эту.",
                                fd.name, var, n, var, min_bound),
                            *sp));
                    }
                }
            }
            // Ф.23.1 (Plan 33.6): contradictory requires — max(lower) > min(upper) для одной Var.
            for var in lower_bounds.keys() {
                let max_low = lower_bounds.get(var).unwrap().iter()
                    .map(|(n, _)| *n).max().unwrap();
                if let Some(ub) = upper_bounds.get(var) {
                    let min_up = ub.iter().map(|(n, _)| *n).min().unwrap();
                    if max_low > min_up {
                        let span = ub.iter().find(|(n, _)| *n == min_up).map(|(_, s)| *s)
                            .unwrap_or(fd.span);
                        report.errors.push(Diagnostic::new(
                            format!("fn `{}`: contradictory requires для `{}` [E2405]:\n  \
                                     `requires {} >= {}` и `requires {} <= {}` несовместимы.\n  \
                                     fn никогда не может быть вызвана с валидными аргументами.",
                                fd.name, var, var, max_low, var, min_up),
                            span));
                    }
                }
            }
            // Ф.23.3 (Plan 33.6): unused param detection.
            let mut used_names: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            collect_used_idents_in_body(&fd.body, &mut used_names);
            for p in &fd.params {
                if p.name.starts_with('_') { continue; } // intentional skip
                if !used_names.contains(&p.name) {
                    report.warnings.push(Diagnostic::new(
                        format!("fn `{}`: param `{}` не используется в теле [W2402]:\n  \
                                 удалите или переименуйте в `_{}` для intentional skip.",
                            fd.name, p.name, p.name),
                        fd.span));
                }
            }
            // Ф.25.1 (Plan 33.6): duplicate ensures/requires detection.
            // Canonical form через простой кастомный walker (без spans).
            fn canon_expr(e: &Expr) -> String {
                match &e.kind {
                    ExprKind::Ident(n) => format!("Ident({})", n),
                    ExprKind::IntLit(n) => format!("Int({})", n),
                    ExprKind::BoolLit(b) => format!("Bool({})", b),
                    ExprKind::StrLit(s) => format!("Str({:?})", s),
                    ExprKind::Binary { op, left, right } => {
                        format!("Bin({:?},{},{})", op, canon_expr(left), canon_expr(right))
                    }
                    ExprKind::Unary { op, operand } => {
                        format!("Un({:?},{})", op, canon_expr(operand))
                    }
                    ExprKind::Member { obj, name } => {
                        format!("Mem({},{})", canon_expr(obj), name)
                    }
                    ExprKind::Call { func, args, .. } => {
                        let a: Vec<String> = args.iter().map(|x| canon_expr(x.expr())).collect();
                        format!("Call({},[{}])", canon_expr(func), a.join(","))
                    }
                    _ => format!("{:?}", e.kind),
                }
            }
            let mut seen_keys: std::collections::HashMap<String, Vec<Span>> =
                std::collections::HashMap::new();
            for c in &fd.contracts {
                let kind_str = match c.kind {
                    ContractKind::Requires => "requires",
                    ContractKind::Ensures => "ensures",
                    ContractKind::EnsuresFail => "ensures_fail",
                };
                let key = format!("{}|{}", kind_str, canon_expr(&c.expr));
                seen_keys.entry(key).or_default().push(c.span);
            }
            for (key, spans) in &seen_keys {
                if spans.len() >= 2 {
                    let kind_label = key.split('|').next().unwrap_or("contract");
                    for sp in spans.iter().skip(1) {
                        report.warnings.push(Diagnostic::new(
                            format!("fn `{}`: duplicate `{}` clause [W2402]:\n  \
                                     2+ idential `{}` — удалите дубликат.",
                                fd.name, kind_label, kind_label),
                            *sp));
                    }
                }
            }
            // Ф.24.1 (Plan 33.6): loop без invariant в fn с ensures — W2402 hint.
            // Включаем для всех fn с contracts (не только #verify), потому что ensures
            // часто требует loop preservation reasoning.
            if !fd.contracts.is_empty() {
                let mut loops_no_inv: Vec<Span> = Vec::new();
                collect_loops_no_invariant(&fd.body, &mut loops_no_inv);
                for sp in loops_no_inv {
                    report.warnings.push(Diagnostic::new(
                        format!("fn `{}`: loop без `invariant` clause [W2402]:\n  \
                                 verify ограничен — добавьте `invariant <cond>` для proper\n  \
                                 preservation reasoning. (loop с contracts без invariant\n  \
                                 проходит проверку только через runtime fallback)",
                            fd.name),
                        sp));
                }
            }
        }
    }
    // Ф.27.1 (Plan 33.6): `#verify` fn без contracts — noop.
    for item in &module.items {
        if let Item::Fn(fd) = item {
            if matches!(fd.verify_mode, VerifyMode::MustVerify) && fd.contracts.is_empty() {
                report.warnings.push(Diagnostic::new(
                    format!("fn `{}` помечена `#verify` но не имеет contracts [W2402]:\n  \
                             `#verify` без requires/ensures — noop. Удалите `#verify`\n  \
                             или добавьте contracts.",
                        fd.name),
                    fd.span));
            }
            // Ф.42.4 (Plan 33.6): `#verify` fn с только requires (без ensures) —
            // доказывает только что входы валидны, callerу никакой гарантии не даёт.
            // Полезно redirecting attention — добавь ensures либо удали `#verify`.
            if matches!(fd.verify_mode, VerifyMode::MustVerify) && !fd.contracts.is_empty() {
                let has_ensures = fd.contracts.iter().any(|c|
                    matches!(c.kind, ContractKind::Ensures | ContractKind::EnsuresFail));
                let has_requires = fd.contracts.iter().any(|c|
                    matches!(c.kind, ContractKind::Requires));
                if has_requires && !has_ensures {
                    report.warnings.push(Diagnostic::new(
                        format!("fn `{}` имеет только `requires` без `ensures` [W2402]:\n  \
                                 verify-fn без ensures не даёт callerу гарантий —\n  \
                                 только проверяет валидность входов. Добавьте `ensures result <условие>`\n  \
                                 либо удалите `#verify` (runtime требования проверятся compile-time).",
                            fd.name),
                        fd.span));
                }
            }
            // Ф.47.1 (Plan 33.6): readability hint — contract clause с 5+ AND-conjuncts.
            // Recursive count: `(a && b && c && d && e)` → 5 conjuncts. Подсказка:
            // разделите на несколько `requires <a>` / `requires <b>` clauses для clarity.
            fn count_and_conjuncts(e: &Expr) -> usize {
                match &e.kind {
                    ExprKind::Binary { op: BinOp::And, left, right } =>
                        count_and_conjuncts(left) + count_and_conjuncts(right),
                    _ => 1,
                }
            }
            // Ф.50.2 (Plan 33.6): парный для OR — long disjunction.
            // 5+ disjuncts через `||` — readability concern. Suggest pattern match,
            // table lookup или enum-based dispatch.
            fn count_or_disjuncts(e: &Expr) -> usize {
                match &e.kind {
                    ExprKind::Binary { op: BinOp::Or, left, right } =>
                        count_or_disjuncts(left) + count_or_disjuncts(right),
                    _ => 1,
                }
            }
            for c in &fd.contracts {
                let n = count_and_conjuncts(&c.expr);
                if n >= 5 {
                    let kind_str = match c.kind {
                        ContractKind::Requires => "requires",
                        ContractKind::Ensures => "ensures",
                        ContractKind::EnsuresFail => "ensures_fail",
                    };
                    report.warnings.push(Diagnostic::new(
                        format!("fn `{}`: `{}` clause содержит {} AND-conjunct'ов [W2402]:\n  \
                                 для readability разделите на несколько `{}` clauses\n  \
                                 (каждый conjunct — отдельная строка). Diagnostic messages\n  \
                                 при failure тогда укажут который именно conjunct провален.",
                            fd.name, kind_str, n, kind_str),
                        c.span));
                }
                let m = count_or_disjuncts(&c.expr);
                if m >= 5 {
                    let kind_str = match c.kind {
                        ContractKind::Requires => "requires",
                        ContractKind::Ensures => "ensures",
                        ContractKind::EnsuresFail => "ensures_fail",
                    };
                    report.warnings.push(Diagnostic::new(
                        format!("fn `{}`: `{}` clause содержит {} OR-disjunct'ов [W2402]:\n  \
                                 для readability рассмотрите pattern match (`match x {{ A | B => true, _ => false }}`)\n  \
                                 либо table lookup. Long disjunction обычно сигнал что должен\n  \
                                 быть enum или set membership check.",
                            fd.name, kind_str, m),
                        c.span));
                }
            }
            // Ф.28.1 (Plan 33.6): duplicate `apply lemma(args)` detection.
            let applies = collect_apply_stmts_in_body(&fd.body);
            let mut seen_applies: std::collections::HashMap<String, Vec<Span>> =
                std::collections::HashMap::new();
            for (lemma_name, args, sp) in &applies {
                let args_canon: Vec<String> = args.iter().map(|a| format!("{:?}", a.kind)).collect();
                let key = format!("{}({})", lemma_name, args_canon.join(","));
                seen_applies.entry(key).or_default().push(*sp);
            }
            for (key, spans) in &seen_applies {
                if spans.len() >= 2 {
                    for sp in spans.iter().skip(1) {
                        report.warnings.push(Diagnostic::new(
                            format!("fn `{}`: duplicate `apply {}` [W2402]:\n  \
                                     повтор apply того же lemma с теми же args не даёт\n  \
                                     extra info. Удалите дубликат.",
                                fd.name, key),
                            *sp));
                    }
                }
            }
            // Ф.49.1 (Plan 33.6): apply к лемме с `requires false` → W2402.
            // Lemma vacuous (Ф.31.3 ловит на declaration side); здесь warning на
            // call-site чтобы программист увидел useless apply.
            for (lemma_name, _, sp) in &applies {
                let lemma_has_false_req = module.items.iter().any(|it| {
                    if let Item::Lemma(ld) = it {
                        ld.name == *lemma_name && ld.contracts.iter().any(|c| {
                            matches!(c.kind, ContractKind::Requires)
                                && matches!(c.expr.kind, ExprKind::BoolLit(false))
                        })
                    } else { false }
                });
                if lemma_has_false_req {
                    report.warnings.push(Diagnostic::new(
                        format!("fn `{}`: `apply {}(...)` к vacuous лемме [W2402]:\n  \
                                 лemma `{}` имеет `requires false` — apply никогда\n  \
                                 не активирует precondition. Удалите apply или fix lemma.",
                            fd.name, lemma_name, lemma_name),
                        *sp));
                }
            }
            // Plan 33.9 Ф.6.1: `reveal X` где X не помечена `#opaque` → W2403.
            // Reveal на non-opaque fn бесполезен (body уже видим verifier'у).
            // Plan 33.9 Ф.6.3: duplicate `reveal X; reveal X;` → W2402.
            let reveals = collect_reveal_stmts_in_body(&fd.body);
            let mut seen_reveals: std::collections::HashMap<String, Vec<Span>> =
                std::collections::HashMap::new();
            for (name, sp) in &reveals {
                seen_reveals.entry(name.clone()).or_default().push(*sp);
                // Ф.6.1: проверка opaque target.
                let target_is_opaque = module.items.iter().any(|it| {
                    matches!(it, Item::Fn(fd) if fd.name == *name && fd.is_opaque)
                });
                let target_exists = module.items.iter().any(|it| {
                    matches!(it, Item::Fn(fd) if fd.name == *name)
                });
                if !target_exists {
                    report.errors.push(Diagnostic::new(
                        format!("fn `{}`: `reveal {}` ссылается на несуществующую fn [E2411]:\n  \
                                 объявите `#opaque #pure fn {}(...)` или удалите reveal.",
                            fd.name, name, name),
                        *sp));
                } else if !target_is_opaque {
                    report.warnings.push(Diagnostic::new(
                        format!("fn `{}`: `reveal {}` на non-opaque fn [W2403]:\n  \
                                 fn `{}` не помечена `#opaque` — reveal бесполезен\n  \
                                 (body уже видим verifier'у). Удалите reveal или добавьте\n  \
                                 `#opaque` к `{}`.",
                            fd.name, name, name, name),
                        *sp));
                }
            }
            // Ф.6.3: dup reveal detection.
            for (name, spans) in &seen_reveals {
                if spans.len() >= 2 {
                    for sp in spans.iter().skip(1) {
                        report.warnings.push(Diagnostic::new(
                            format!("fn `{}`: duplicate `reveal {}` [W2402]:\n  \
                                     повторный reveal того же fn в одном scope не даёт\n  \
                                     extra info. Удалите дубликат.",
                                fd.name, name),
                            *sp));
                    }
                }
            }
        }
    }
    // Plan 33.9 Ф.6.2: `#opaque` fn never reveal'ится никем в module → W2403.
    // Также Ф.6.4: `#fuel(0)` явный — эквивалентно отсутствию (noise).
    let mut all_reveals: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            for (n, _) in collect_reveal_stmts_in_body(&fd.body) {
                all_reveals.insert(n);
            }
        }
    }
    for item in &module.items {
        if let Item::Fn(fd) = item {
            // Ф.6.2: dead opaque.
            if fd.is_opaque && !all_reveals.contains(&fd.name) {
                report.warnings.push(Diagnostic::new(
                    format!("fn `{}`: `#opaque` но никогда не reveal'ится в модуле [W2403]:\n  \
                             dead opaque — никто не доказывает с раскрытым body. Удалите\n  \
                             `#opaque` (body будет inline'ится как обычно) или добавьте\n  \
                             `reveal {}` в caller fn.",
                        fd.name, fd.name),
                    fd.span));
            }
            // Ф.6.4: explicit #fuel(0).
            if let Some(0) = fd.fuel {
                report.warnings.push(Diagnostic::new(
                    format!("fn `{}`: `#fuel(0)` явный — эквивалентно отсутствию fuel [W2403]:\n  \
                             default unfolding depth для opaque и так 0. Удалите\n  \
                             `#fuel(0)` либо укажите `#fuel(N)` где N >= 1.",
                        fd.name),
                    fd.span));
            }
        }
    }
    // Ф.27.3 (Plan 33.6): effect с #pure methods, но без axioms — suggest.
    for item in &module.items {
        if let Item::Type(td) = item {
            if let TypeDeclKind::Effect(methods) = &td.kind {
                let has_pure_view = methods.iter()
                    .any(|m| matches!(m.kind, EffectOpKind::PureView));
                if has_pure_view && td.axioms.is_empty() {
                    report.warnings.push(Diagnostic::new(
                        format!("effect `{}` имеет #pure methods но не имеет axioms [W2402]:\n  \
                                 handler нельзя verify относительно behavior без axiom.\n  \
                                 Добавьте `axiom <name>(...) => <pure-formula>`.",
                            td.name),
                        td.span));
                }
            }
        }
    }
    // Plan 33.14 Ф.4+Ф.5: слить накопленные cross-check-расхождения.
    // Непустой список — soundness-критичная находка (Z3 и CVC5 дали
    // противоположные definite-ответы): поднимаем compile-error. Это и
    // есть merge-gate для CI-job `contracts-crosscheck`. В обычном режиме
    // (без NOVA_CROSSCHECK) коллектор всегда пуст — drain это no-op.
    super::crosscheck::set_current_fn(None);
    let disagreements = super::crosscheck::take_disagreements();
    for d in &disagreements {
        let span = d
            .fn_name
            .as_ref()
            .and_then(|name| {
                module.items.iter().find_map(|it| match it {
                    Item::Fn(fd) if &fd.name == name => Some(fd.span),
                    Item::Lemma(ld) if &ld.name == name => Some(ld.span),
                    _ => None,
                })
            })
            .unwrap_or_else(crate::diag::Span::dummy);
        report
            .errors
            .push(Diagnostic::new(super::crosscheck::format_one(d), span));
    }
    report
}

/// Ф.24.1: walker для сбора spans loops без invariants в body.
fn collect_loops_no_invariant(body: &FnBody, out: &mut Vec<Span>) {
    match body {
        FnBody::Expr(e) => collect_loops_in_expr(e, out),
        FnBody::Block(b) => {
            for s in &b.stmts { collect_loops_in_stmt(s, out); }
            if let Some(e) = &b.trailing { collect_loops_in_expr(e, out); }
        }
        FnBody::External => {}
    }
}

fn collect_loops_in_stmt(s: &Stmt, out: &mut Vec<Span>) {
    match s {
        Stmt::Let(l) => collect_loops_in_expr(&l.value, out),
        Stmt::Expr(e) => collect_loops_in_expr(e, out),
        Stmt::Assign { value, .. } => collect_loops_in_expr(value, out),
        _ => {}
    }
}

fn collect_loops_in_expr(e: &Expr, out: &mut Vec<Span>) {
    match &e.kind {
        ExprKind::While { body, invariants, .. } => {
            if invariants.is_empty() { out.push(e.span); }
            for s in &body.stmts { collect_loops_in_stmt(s, out); }
            if let Some(t) = &body.trailing { collect_loops_in_expr(t, out); }
        }
        ExprKind::For { body, invariants, .. } => {
            if invariants.is_empty() { out.push(e.span); }
            for s in &body.stmts { collect_loops_in_stmt(s, out); }
            if let Some(t) = &body.trailing { collect_loops_in_expr(t, out); }
        }
        ExprKind::If { then, else_, .. } => {
            for s in &then.stmts { collect_loops_in_stmt(s, out); }
            if let Some(t) = &then.trailing { collect_loops_in_expr(t, out); }
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => {
                        for s in &b.stmts { collect_loops_in_stmt(s, out); }
                        if let Some(t) = &b.trailing { collect_loops_in_expr(t, out); }
                    }
                    ElseBranch::If(e2) => collect_loops_in_expr(e2, out),
                }
            }
        }
        _ => {}
    }
}

/// Ф.23.3: walker для сбора всех используемых Ident в body.
fn collect_used_idents_in_body(body: &FnBody, out: &mut std::collections::HashSet<String>) {
    match body {
        FnBody::Expr(e) => collect_used_idents_in_expr(e, out),
        FnBody::Block(b) => {
            for s in &b.stmts { collect_used_idents_in_stmt(s, out); }
            if let Some(e) = &b.trailing { collect_used_idents_in_expr(e, out); }
        }
        FnBody::External => {}
    }
}

fn collect_used_idents_in_stmt(s: &Stmt, out: &mut std::collections::HashSet<String>) {
    match s {
        Stmt::Let(l) => collect_used_idents_in_expr(&l.value, out),
        Stmt::Expr(e) => collect_used_idents_in_expr(e, out),
        Stmt::Assign { value, target, .. } => {
            collect_used_idents_in_expr(value, out);
            collect_used_idents_in_expr(target, out);
        }
        Stmt::Return { value: Some(v), .. } => collect_used_idents_in_expr(v, out),
        _ => {}
    }
}

fn collect_used_idents_in_expr(e: &Expr, out: &mut std::collections::HashSet<String>) {
    match &e.kind {
        ExprKind::Ident(n) => { out.insert(n.clone()); }
        ExprKind::Binary { left, right, .. } => {
            collect_used_idents_in_expr(left, out);
            collect_used_idents_in_expr(right, out);
        }
        ExprKind::Unary { operand, .. } => collect_used_idents_in_expr(operand, out),
        ExprKind::Call { func, args, .. } => {
            collect_used_idents_in_expr(func, out);
            for a in args { collect_used_idents_in_expr(a.expr(), out); }
        }
        ExprKind::If { cond, then, else_ } => {
            collect_used_idents_in_expr(cond, out);
            for s in &then.stmts { collect_used_idents_in_stmt(s, out); }
            if let Some(t) = &then.trailing { collect_used_idents_in_expr(t, out); }
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => {
                        for s in &b.stmts { collect_used_idents_in_stmt(s, out); }
                        if let Some(t) = &b.trailing { collect_used_idents_in_expr(t, out); }
                    }
                    ElseBranch::If(e2) => collect_used_idents_in_expr(e2, out),
                }
            }
        }
        ExprKind::Member { obj, .. } => collect_used_idents_in_expr(obj, out),
        _ => {}
    }
}

/// V2.3: рекурсивно найти первый BV-sorted Var в терме → (width, signed).
/// Используется для overflow-VC после subst (операнды могут быть App).
fn bv_sort_in_term(
    t: &SmtTerm,
    var_sorts: &std::collections::HashMap<String, SortRef>,
) -> Option<(u32, bool)> {
    match t {
        SmtTerm::Var(n) => match var_sorts.get(n) {
            Some(SortRef::BitVec { width, signed }) => Some((*width, *signed)),
            _ => None,
        },
        SmtTerm::App(_, args) => args.iter().find_map(|a| bv_sort_in_term(a, var_sorts)),
        _ => None,
    }
}

/// Найти ширину BitVecLit в терме (для fallback когда нет BV-Var).
fn bv_lit_width(t: &SmtTerm) -> Option<u32> {
    match t {
        SmtTerm::BitVecLit(_, w) => Some(*w),
        SmtTerm::App(_, args) => args.iter().find_map(bv_lit_width),
        _ => None,
    }
}

/// V2.3: scope обхода — карта sorts (для BV-детекции) + карта subst
/// (let-имя → SmtTerm его значения). Subst позволяет переписать
/// VC в терминах fn-параметров (уже declared в backend), избегая
/// undeclared-var в Z3.
#[derive(Clone)]
struct BvScope {
    sorts: std::collections::HashMap<String, SortRef>,
    /// let-имя → закодированный SmtTerm значения (через subst подставляется).
    subst: std::collections::HashMap<String, SmtTerm>,
}

/// Plan 33.7 Ф.4: собрать все BV-арифметические операции в теле fn.
/// Возвращает Vec<(span, op, lhs_SmtTerm, rhs_SmtTerm)>.
/// Только Add/Sub/Mul где хотя бы один операнд имеет BV-сорт.
///
/// V2.3: рекурсия в блочные тела (let-bindings, вложенные блоки,
/// if/else-блоки). `let x u32 = E` регистрирует `x` как BV-sorted и
/// запоминает subst `x → encode(E)` — последующие `x + y` детектятся и
/// VC переписывается в `(E) + y` (только fn-параметры, declared в backend).
fn collect_bv_arith_ops_in_body(
    body: &FnBody,
    ctx: &encode::EncodeCtx,
) -> Vec<(Span, crate::ast::BinOp, SmtTerm, SmtTerm)> {
    let mut out = Vec::new();
    let mut scope = BvScope {
        sorts: ctx.var_sorts.clone(),
        subst: std::collections::HashMap::new(),
    };
    match body {
        FnBody::Expr(e) => collect_bv_arith_in_expr(e, ctx, &mut scope, &mut out),
        FnBody::Block(b) => collect_bv_arith_in_block(b, ctx, &mut scope, &mut out),
        FnBody::External => {}
    }
    out
}

fn collect_bv_arith_in_block(
    b: &Block,
    ctx: &encode::EncodeCtx,
    scope: &mut BvScope,
    out: &mut Vec<(Span, crate::ast::BinOp, SmtTerm, SmtTerm)>,
) {
    for s in &b.stmts { collect_bv_arith_in_stmt(s, ctx, scope, out); }
    if let Some(t) = &b.trailing { collect_bv_arith_in_expr(t, ctx, scope, out); }
}

fn collect_bv_arith_in_stmt(
    s: &Stmt,
    ctx: &encode::EncodeCtx,
    scope: &mut BvScope,
    out: &mut Vec<(Span, crate::ast::BinOp, SmtTerm, SmtTerm)>,
) {
    match s {
        Stmt::Let(ld) => {
            collect_bv_arith_in_expr(&ld.value, ctx, scope, out);
            // V2.3: `let x u32 = E` с явной BV-аннотацией → регистрируем
            // `x` как BV-sorted и subst `x → encode(E, с текущим subst)`.
            if let (crate::ast::Pattern::Ident { name, .. }, Some(ty)) = (&ld.pattern, &ld.ty) {
                let sort = type_to_sort(ty);
                if matches!(sort, SortRef::BitVec { .. }) {
                    let ext_ctx = bv_scope_ctx(ctx, scope);
                    if let Ok(mut val_t) = encode::encode_expr_with_ctx(&ld.value, &ext_ctx) {
                        // Развернуть subst внутри val_t (вложенные let).
                        for (k, v) in &scope.subst {
                            val_t = val_t.substitute(k, v);
                        }
                        scope.sorts.insert(name.clone(), sort);
                        scope.subst.insert(name.clone(), val_t);
                    }
                }
            }
        }
        Stmt::Expr(e) => collect_bv_arith_in_expr(e, ctx, scope, out),
        Stmt::Assign { value, .. } => collect_bv_arith_in_expr(value, ctx, scope, out),
        Stmt::Return { value: Some(v), .. } => collect_bv_arith_in_expr(v, ctx, scope, out),
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_bv_arith_in_expr(expr, ctx, scope, out);
        }
        Stmt::Throw { value, .. } | Stmt::Defer { body: value, .. } | Stmt::ErrDefer { body: value, .. }
        | Stmt::OkDefer { body: value, .. } | Stmt::DeferWithResult { body: value, .. } => {
            collect_bv_arith_in_expr(value, ctx, scope, out);
        }
        _ => {}
    }
}

/// Построить EncodeCtx с расширенным var_sorts из BvScope.
fn bv_scope_ctx<'a>(ctx: &encode::EncodeCtx<'a>, scope: &BvScope) -> encode::EncodeCtx<'a> {
    encode::EncodeCtx {
        pure_views: ctx.pure_views,
        pure_fns: ctx.pure_fns,
        trusted_fns: ctx.trusted_fns,
        var_sorts: scope.sorts.clone(),
    }
}

fn collect_bv_arith_in_expr(
    e: &Expr,
    ctx: &encode::EncodeCtx,
    scope: &mut BvScope,
    out: &mut Vec<(Span, crate::ast::BinOp, SmtTerm, SmtTerm)>,
) {
    match &e.kind {
        ExprKind::Binary { op, left, right } => {
            collect_bv_arith_in_expr(left, ctx, scope, out);
            collect_bv_arith_in_expr(right, ctx, scope, out);
            if !matches!(op, crate::ast::BinOp::Add | crate::ast::BinOp::Sub | crate::ast::BinOp::Mul) {
                return;
            }
            let lhs_is_bv = is_expr_bv_sorted(left, &scope.sorts);
            let rhs_is_bv = is_expr_bv_sorted(right, &scope.sorts);
            if !lhs_is_bv && !rhs_is_bv { return; }
            let ext_ctx = bv_scope_ctx(ctx, scope);
            let Ok(mut lhs_t) = encode::encode_expr_with_ctx(left, &ext_ctx) else { return };
            let Ok(mut rhs_t) = encode::encode_expr_with_ctx(right, &ext_ctx) else { return };
            // V2.3: подставить let-bound переменные их значениями, чтобы VC
            // содержал только fn-параметры (declared в backend).
            for (k, v) in &scope.subst {
                lhs_t = lhs_t.substitute(k, v);
                rhs_t = rhs_t.substitute(k, v);
            }
            out.push((e.span, *op, lhs_t, rhs_t));
        }
        ExprKind::If { cond, then, else_ } => {
            collect_bv_arith_in_expr(cond, ctx, scope, out);
            let mut then_scope = scope.clone();
            collect_bv_arith_in_block(then, ctx, &mut then_scope, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => {
                        let mut else_scope = scope.clone();
                        collect_bv_arith_in_block(b, ctx, &mut else_scope, out);
                    }
                    ElseBranch::If(e2) => collect_bv_arith_in_expr(e2, ctx, scope, out),
                }
            }
        }
        ExprKind::Block(b) => {
            let mut blk_scope = scope.clone();
            collect_bv_arith_in_block(b, ctx, &mut blk_scope, out);
        }
        ExprKind::Call { func, args, .. } => {
            collect_bv_arith_in_expr(func, ctx, scope, out);
            for a in args { collect_bv_arith_in_expr(a.expr(), ctx, scope, out); }
        }
        ExprKind::Unary { operand, .. } => collect_bv_arith_in_expr(operand, ctx, scope, out),
        _ => {}
    }
}

fn is_expr_bv_sorted(e: &Expr, sorts: &std::collections::HashMap<String, SortRef>) -> bool {
    match &e.kind {
        ExprKind::Ident(n) => matches!(sorts.get(n), Some(SortRef::BitVec { .. })),
        ExprKind::Binary { left, right, .. } => {
            is_expr_bv_sorted(left, sorts) || is_expr_bv_sorted(right, sorts)
        }
        ExprKind::As(_, ty) => matches!(type_to_sort(ty), SortRef::BitVec { .. }),
        ExprKind::IntLit(_) => false, // bare int literal — BV only in context
        _ => false,
    }
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



