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
    pub fn verify_fn(&self, fd: &FnDecl) -> Vec<(Span, VerifyResult)> {
        if fd.contracts.is_empty() { return Vec::new(); }

        let mut backend = self.create_backend();

        // 1. Declare params as Vars.
        for p in &fd.params {
            let sort = type_to_sort(&p.ty);
            backend.declare_var(&p.name, sort);
        }

        // 2. Encode requires → assertions.
        let mut requires_failed = false;
        for c in &fd.contracts {
            if matches!(c.kind, ContractKind::Requires) {
                match encode::encode_expr(&c.expr) {
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
            FnBody::Expr(e) => encode::encode_expr(e).ok(),
            FnBody::Block(b) if b.stmts.is_empty() => {
                b.trailing.as_ref().and_then(|e| encode::encode_expr(e).ok())
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
            let encoded = match encode::encode_expr(&c.expr) {
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

impl Default for VerificationPipeline {
    fn default() -> Self { Self::new() }
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
    for item in &module.items {
        if let Item::Fn(fd) = item {
            if fd.contracts.is_empty() { continue; }
            // Skip Fail-functions — ContractCtx уже выдал error.
            // Mut-параметры → пропустить (33.2). Сейчас детектим через
            // отсутствие в типах.
            let results = pipeline.verify_fn(fd);
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
