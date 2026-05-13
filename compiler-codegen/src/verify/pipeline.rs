//! Plan 33.1 Ф.3: Verification pipeline.
//!
//! Алгоритм для каждой функции с контрактами:
//!
//! 1. Encode параметры как Var-ы (SMT-IR).
//! 2. Encode `requires` → assertions в backend.
//! 3. Encode body: для straight-line `=> expr` body = symbolic value,
//!    которое заменяет `result` в ensures. Для block-body с trailing —
//!    то же самое.
//! 4. Для каждого `ensures Q`:
//!    - Substitute `result` → encoded_body_value в Q.
//!    - try_prove(Q): unsat → proven; sat → counterexample; unknown → fallback.
//! 5. Результат per-fn → агрегируется в pipeline-level diagnostics.
//!
//! Plan 33.1 ограничения:
//! - Body должен быть encodable (см. encode.rs `Unsupported` case'ы).
//! - Block-bodies со statements (let, if-stmts) НЕ encoded; их контракты
//!   = `Unknown(NotAttempted)` (runtime fallback работает).
//! - Function calls в body НЕ encoded (composition в 33.2).

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use super::ir::*;
use super::encode;
use super::backend::{SmtBackend, try_prove};
use super::backend::trivial::TrivialBackend;

/// Результат верификации одного контракта.
#[derive(Debug, Clone)]
pub enum VerifyResult {
    Proven,
    /// Контр-пример (формула опровержена).
    Disproved(Model, String),
    /// SMT не справился — fallback to runtime.
    Unknown(String),
    /// Encoder не смог построить SMT-IR (fall back to runtime).
    EncodingFailed(String),
}

/// Выбор SMT backend'а.
///
/// Plan 33 Z3 milestone (V1 closure): добавлен `Z3`. По умолчанию
/// `Trivial` (backward-compat + no external deps). Switch:
/// - CLI flag (nova check/test/compile): `--smt-backend=z3`.
/// - Env var: `NOVA_SMT_BACKEND=z3`.
///
/// Если feature `z3-backend` не compiled-in, `Z3` теряет смысл —
/// `create_backend` падает обратно на trivial с stderr-warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendChoice {
    Trivial,
    Z3,
}

impl BackendChoice {
    /// Парсит строку, used и для CLI и для env-var.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "trivial" | "default" | "" => Some(BackendChoice::Trivial),
            "z3" => Some(BackendChoice::Z3),
            _ => None,
        }
    }

    /// Backend по умолчанию: смотрим `NOVA_SMT_BACKEND`, иначе Trivial.
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

    /// Plan 33 Z3 milestone: явный выбор backend'а (override env-var).
    pub fn with_backend(mut self, backend: BackendChoice) -> Self {
        self.backend = backend;
        self
    }

    /// Создать backend instance согласно выбору. Falls back to trivial
    /// с warning'ом если z3 не compiled-in.
    fn create_backend(&self) -> Box<dyn SmtBackend> {
        match self.backend {
            BackendChoice::Trivial => Box::new(TrivialBackend::new()),
            BackendChoice::Z3 => {
                #[cfg(feature = "z3-backend")]
                {
                    Box::new(super::backend::z3::Z3Backend::new(self.timeout_ms))
                }
                #[cfg(not(feature = "z3-backend"))]
                {
                    // User-friendly fallback (но не silent — пишем в stderr).
                    eprintln!(
                        "warning: --smt-backend=z3 requested, but binary built without \
                         `--features z3-backend`; falling back to trivial backend. \
                         Rebuild с `cargo build --features z3-backend`."
                    );
                    Box::new(TrivialBackend::new())
                }
            }
        }
    }

    /// Verify одну функцию: возвращает list of (Contract span, VerifyResult).
    /// Backend выбирается через `BackendChoice` (env-var / CLI flag).
    ///
    /// Plan 33.3 Ф.9: принимает `module` для разрешения pure_view-вызовов
    /// и регистрации axioms эффектов в SMT-scope этой fn.
    pub fn verify_fn(&self, module: &Module, fd: &FnDecl) -> Vec<(Span, VerifyResult)> {
        if fd.contracts.is_empty() { return Vec::new(); }

        let mut backend = self.create_backend();

        // Plan 33.3 Ф.9: реестр pure_view-ops модуля. Используется
        // encoder'ом для перевода `balance(id)` → UF `_view_Db_balance(id)`.
        let pure_views = collect_pure_views(module);
        let ctx = super::encode::EncodeCtx { pure_views: &pure_views };

        // Plan 33.3 Ф.9: pre-declare все pure_view UFs в backend'е.
        // Без этого Z3 auto-declare'ит UF с Int sorts по умолчанию;
        // pre-decl даёт правильные sorts из effect-сигнатуры (важно для
        // soundness когда args не int'овые).
        for (op_name, sig) in &pure_views {
            let uf = super::encode::pure_view_uf_name(&sig.effect_name, op_name);
            backend.declare_function(&uf, &sig.param_sorts, sig.return_sort.clone());
        }

        // 1. Declare params as Vars.
        for p in &fd.params {
            let sort = type_to_sort(&p.ty);
            backend.declare_var(&p.name, sort);
        }

        // Plan 33.3 Ф.9: для каждого эффекта в сигнатуре fn регистрируем
        // axioms как глобальные assertions (Forall'ы). Z3 instantiate'ит
        // их через trigger-based heuristics; TrivialBackend хранит as-is.
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

        // 2. Encode requires → assertions.
        let mut requires_failed = false;
        for c in &fd.contracts {
            if matches!(c.kind, ContractKind::Requires) {
                match encode::encode_expr_with_ctx(&c.expr, &ctx) {
                    Ok(t) => backend.assert(Assertion {
                        formula: t,
                        label: Some(format!("requires@{}", c.span.start)),
                    }),
                    Err(_) => {
                        // Если requires не encoded — мы не можем проверить
                        // никакой ensures (контекст неполон). Все ensures →
                        // EncodingFailed.
                        requires_failed = true;
                    }
                }
            }
        }

        // 3. Encode body value. Только для `=> expr` форм
        // (block-bodies с trailing-only тоже OK).
        let body_val = match &fd.body {
            FnBody::Expr(e) => encode::encode_expr_with_ctx(e, &ctx).ok(),
            FnBody::Block(b) if b.stmts.is_empty() => {
                b.trailing.as_ref().and_then(|e| encode::encode_expr_with_ctx(e, &ctx).ok())
            }
            _ => None,
        };

        // 4. Verify each ensures.
        let mut results = Vec::new();
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
                    results.push((c.span, VerifyResult::EncodingFailed(msg)));
                    continue;
                }
            };
            // Substitute result → body_val (если есть).
            let goal = if let Some(bv) = &body_val {
                encoded.substitute("result", bv)
            } else {
                // Body не encoded → fallback.
                results.push((c.span, VerifyResult::EncodingFailed(
                    "function body not encodable (use runtime check)".into())));
                continue;
            };
            // Также подменим `old(x)` → значение `x` на entry-state.
            // В 33.1 нет mut params → старые значения = текущие значения.
            let goal = substitute_old(&goal);

            // try_prove(goal). `&mut *backend` чтобы coerce Box<dyn> → &mut dyn.
            match try_prove(&mut *backend, goal) {
                SatResult::Unsat(_) => results.push((c.span, VerifyResult::Proven)),
                SatResult::Sat(model) => {
                    let cex = format_counterexample(&model);
                    results.push((c.span, VerifyResult::Disproved(model, cex)));
                }
                SatResult::Unknown(reason) => {
                    // Plan 33.3 Ф.9.10: AI-friendly diagnostic — категоризируем
                    // reason + suggestions.
                    let msg = unknown_to_diag_message(reason);
                    results.push((c.span, VerifyResult::Unknown(msg)));
                }
            }
        }
        let _ = self.timeout_ms; // используется когда добавим Z3-backend
        results
    }
}

/// Plan 33.3 Ф.9: собрать все pure_view'ы модуля в реестр.
fn collect_pure_views(module: &Module) -> std::collections::HashMap<String, super::encode::PureViewSig> {
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

/// Plan 33.3 Ф.9: метаданные одного axiom'а с реестрами для encoding.
struct AxiomInfo<'a> {
    effect_name: String,
    axiom_name: String,
    binders: &'a [String],
    /// Маппинг binder-name → sort. Выводится из usage в pure_view-вызовах
    /// внутри formula (V1 эвристика).
    formula: &'a crate::ast::Expr,
}

/// Plan 33.3 Ф.9: собрать все axiom'ы модуля.
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
            });
        }
    }
    out
}

/// Plan 33.3 Ф.9: encode axiom в SMT-Forall.
///
/// Биндеры приобретают sort из ПЕРВОГО pure_view-вызова в формуле,
/// где они используются как аргумент. Это эвристика V1; явные type
/// ascriptions в binders — future work.
fn encode_axiom(
    ax: &AxiomInfo,
    pure_views: &std::collections::HashMap<String, super::encode::PureViewSig>,
) -> Option<SmtTerm> {
    // Инфер sorts для binders.
    let mut binder_sorts: std::collections::HashMap<String, SortRef> = std::collections::HashMap::new();
    infer_binder_sorts(ax.formula, ax.binders, pure_views, &mut binder_sorts);
    // Encode body.
    let ctx = super::encode::EncodeCtx { pure_views };
    let body = super::encode::encode_expr_with_ctx(ax.formula, &ctx).ok()?;
    // Build binders Vec — для каждого ax.binder возьмём inferred sort,
    // default Int если не выведен.
    let binders: Vec<(String, SortRef)> = ax.binders.iter()
        .map(|n| (n.clone(), binder_sorts.remove(n).unwrap_or(SortRef::Int)))
        .collect();
    if binders.is_empty() {
        Some(body)
    } else {
        Some(SmtTerm::Forall(binders, Box::new(body)))
    }
}

/// Walks `formula` и для каждой ссылки на binder в pure_view-arg
/// записывает sort параметра в `out`.
fn infer_binder_sorts(
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

/// Plan 33.3 Ф.9.5: проверка consistency axiom'ов модуля.
///
/// Для каждого эффекта с axioms создаётся изолированный backend, в нём
/// объявляются все pure_view UFs эффекта, asserted все axioms, затем
/// `check_sat`. Если UNSAT — axioms together implication False →
/// **compile error** «axioms inconsistent».
///
/// SAT или Unknown — OK. TrivialBackend всегда даёт Unknown для
/// quantified-axioms (нет reasoning'а над Forall), что трактуется как
/// «не доказано inconsistent» — silent fallback.
///
/// Возвращает diagnostic'и (пустой Vec если всё consistent).
pub fn check_axiom_consistency(module: &Module) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let pure_views = collect_pure_views(module);

    // Группируем axioms по effect-name.
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

            // Pre-declare ВСЕ pure_view UFs модуля (могут ссылаться cross-effect
            // в формулах — V1 ограничивает one-effect-axioms, но безопаснее
            // pre-decl'ить всё).
            for (op_name, sig) in &pure_views {
                let uf = super::encode::pure_view_uf_name(&sig.effect_name, op_name);
                backend.declare_function(&uf, &sig.param_sorts, sig.return_sort.clone());
            }

            // Assert все axioms эффекта.
            let mut some_encoded = false;
            for ax in axiom_refs {
                let info = AxiomInfo {
                    effect_name: td.name.clone(),
                    axiom_name: ax.name.clone(),
                    binders: &ax.binders,
                    formula: &ax.formula,
                };
                if let Some(formula) = encode_axiom(&info, &pure_views) {
                    backend.assert(Assertion {
                        formula,
                        label: Some(format!("axiom@{}.{}", td.name, ax.name)),
                    });
                    some_encoded = true;
                }
            }

            // Если ни один axiom не encoded — нечего проверять.
            if !some_encoded { continue; }

            // check_sat. Unsat → inconsistent.
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
                    // SAT или Unknown — axioms consistent (или TrivialBackend
                    // не reasoning'ует — silent OK).
                }
            }
        }
    }

    diagnostics
}

/// Substitute `_old_<x>` → `<x>` (33.1: no mut, snapshot trivial).
fn substitute_old(t: &SmtTerm) -> SmtTerm {
    match t {
        SmtTerm::Var(n) if n.starts_with("_old_") => {
            // strip `_old_` prefix
            SmtTerm::Var(n.strip_prefix("_old_").unwrap().to_string())
        }
        SmtTerm::App(op, args) => SmtTerm::App(
            op.clone(),
            args.iter().map(substitute_old).collect(),
        ),
        _ => t.clone(),
    }
}

fn type_to_sort(ty: &TypeRef) -> SortRef {
    match ty {
        TypeRef::Named { path, .. } if path.len() == 1 => match path[0].as_str() {
            "int" | "i32" | "i64" | "money" | "nat" => SortRef::Int,
            "bool" => SortRef::Bool,
            "str" => SortRef::Str,
            other => SortRef::Named(other.into()),
        },
        _ => SortRef::Named("opaque".into()),
    }
}

fn format_counterexample(model: &Model) -> String {
    if model.bindings.is_empty() {
        // Plan 33.3 Ф.9.10: AI-friendly hint когда модель пуста
        // (TrivialBackend часто эту дорогу — конкретные значения не
        // вычисляет, только symbolic disprove).
        return "values not extracted (TrivialBackend); enable Z3 \
                backend для full counterexample".into();
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

/// Plan 33.3 Ф.9.10: AI-friendly hint при unknown verify-result.
/// Категоризирует причину (timeout, nonlinear, unsupported theory) +
/// предлагает actions.
fn unknown_to_diag_message(reason: UnknownReason) -> String {
    match reason {
        UnknownReason::Timeout => {
            "SMT solver hit timeout. \
             Suggestions: (1) simplify the contract into smaller steps via \
             intermediate `assert_static`; (2) increase `#verify_timeout(N)`; \
             (3) mark `#unverified` if проверка intentionally сложна.".into()
        }
        UnknownReason::NonLinearArithmetic => {
            "non-linear arithmetic in contract (e.g. `x * y`, `x / y`). \
             Trivial backend supports only LIA; Z3 backend can handle non-linear \
             через NIA. Suggestions: (1) rewrite в linear form через intermediate \
             variables; (2) wait для Z3 backend; (3) `#unverified`.".into()
        }
        UnknownReason::UnsupportedTheory(s) => {
            format!("unsupported SMT theory: {}. Suggestion: rewrite в supported \
                     theory (LIA/EUF/arrays) или mark `#unverified`.", s)
        }
        UnknownReason::BackendError(s) => {
            format!("SMT backend internal error: {}. Это bug — please report.", s)
        }
        UnknownReason::NotAttempted(s) => {
            format!("{}\n  AI-friendly hint: контракт за пределами TrivialBackend \
                     capabilities (только reflexive ensures, constant folding, \
                     impl-shortcuts). Add intermediate `assert_static`, или \
                     mark `#unverified`, или wait для Z3 backend.", s)
        }
    }
}

/// Entry-point: проверить все функции модуля. Заполняет diagnostics
/// с warning'ами/errors согласно verify_mode.
///
/// Также возвращает map `(fn_name → set of proven contract span)`,
/// которая используется codegen'ом для **zero-cost release** —
/// proven контракты не emit'ятся в release сборке.
pub fn verify_module(module: &Module) -> ModuleVerifyReport {
    let pipeline = VerificationPipeline::new();
    let mut report = ModuleVerifyReport::default();

    // Plan 33.3 Ф.9.5: проверка consistency axiom'ов до per-fn verify.
    // Если axioms эффекта inconsistent (Z3 → UNSAT) → compile-error,
    // skip всех остальных verify'ев (любая formula тривиально доказуема
    // под inconsistent assumptions).
    let inconsistency_errors = check_axiom_consistency(module);
    let has_inconsistent_axioms = !inconsistency_errors.is_empty();
    for e in inconsistency_errors {
        report.errors.push(e);
    }
    if has_inconsistent_axioms {
        return report;
    }

    for item in &module.items {
        if let Item::Fn(fd) = item {
            if fd.contracts.is_empty() { continue; }
            // Skip Fail-functions — ContractCtx уже выдал error.
            // Mut-параметры → пропустить (33.2). Сейчас детектим через
            // отсутствие в типах.
            let results = pipeline.verify_fn(module, fd);
            for (span, vr) in results {
                match vr {
                    VerifyResult::Proven => {
                        report.proven.push((fd.name.clone(), span));
                    }
                    VerifyResult::Disproved(_, cex) => {
                        // Plan 33.3 Ф.9.10: AI-friendly format.
                        // Включает: fn name, counterexample values (или hint),
                        // suggestions для исправления.
                        let msg = format!(
                            "contract violation in `{}`:\n  counterexample: {}\n  \
                             suggestions:\n    1. Add `requires` precondition restricting input;\n    \
                             2. Fix function body to match `ensures`;\n    \
                             3. Weaken `ensures` to actual behavior;\n    \
                             4. Mark `#unverified` if intentional disprove",
                            fd.name, cex);
                        match fd.verify_mode {
                            VerifyMode::MustVerify => report.errors.push(
                                Diagnostic::new(msg, span)),
                            _ => report.warnings.push(
                                Diagnostic::new(msg, span)),
                        }
                    }
                    VerifyResult::Unknown(reason) => {
                        // По D24 / Plan 33.1: default — runtime fallback в debug,
                        // в release контракт стирается с warning (или error если
                        // `#must_verify`).
                        match fd.verify_mode {
                            VerifyMode::MustVerify => {
                                // Plan 33.3 Ф.9.10: AI-friendly format с
                                // категоризированным reason + suggestions
                                // (reason уже содержит hint из unknown_to_diag_message).
                                let msg = format!(
                                    "`#must_verify` failed for `{}`:\n  {}",
                                    fd.name, reason);
                                report.errors.push(Diagnostic::new(msg, span));
                            }
                            _ => {
                                // Default + Unverified — silently runtime-fallback.
                                // (Не пушим warning чтобы не засорять output —
                                // в большинстве случаев trivial backend NotAttempted.)
                            }
                        }
                    }
                    VerifyResult::EncodingFailed(reason) => {
                        // Аналогично Unknown.
                        if matches!(fd.verify_mode, VerifyMode::MustVerify) {
                            let msg = format!(
                                "`#must_verify` failed for `{}`:\n  encoder cannot represent contract: {}\n  \
                                 hint: Plan 33.1 encoder поддерживает только int/bool/str/record/binary-ops/if/old. \
                                 Sum types / arrays / quantifiers — ждут Z3 backend.",
                                fd.name, reason);
                            report.errors.push(Diagnostic::new(msg, span));
                        }
                    }
                }
            }
        }
    }
    report
}

/// Aggregated отчёт по верификации модуля.
#[derive(Debug, Default)]
pub struct ModuleVerifyReport {
    /// Доказанные контракты — `(fn_name, span)`. Используются codegen'ом
    /// в release-сборке для стирания runtime-check'а.
    pub proven: Vec<(String, Span)>,
    /// Errors — выдаются после verify (например `#must_verify` failed).
    pub errors: Vec<Diagnostic>,
    /// Warnings — counterexamples для контрактов без `#must_verify`.
    pub warnings: Vec<Diagnostic>,
}
