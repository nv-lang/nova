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

pub struct VerificationPipeline {
    timeout_ms: u32,
}

impl VerificationPipeline {
    pub fn new() -> Self {
        Self { timeout_ms: 2000 }
    }

    pub fn with_timeout(timeout_ms: u32) -> Self {
        Self { timeout_ms }
    }

    /// Verify одну функцию: возвращает list of (Contract span, VerifyResult).
    /// Trivial backend используется по умолчанию (без libz3).
    pub fn verify_fn(&self, fd: &FnDecl) -> Vec<(Span, VerifyResult)> {
        if fd.contracts.is_empty() { return Vec::new(); }

        let mut backend = TrivialBackend::new();

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

            // try_prove(goal).
            match try_prove(&mut backend, goal) {
                SatResult::Unsat(_) => results.push((c.span, VerifyResult::Proven)),
                SatResult::Sat(model) => {
                    let cex = format_counterexample(&model);
                    results.push((c.span, VerifyResult::Disproved(model, cex)));
                }
                SatResult::Unknown(reason) => {
                    let msg = match reason {
                        UnknownReason::Timeout => "SMT timeout".into(),
                        UnknownReason::NonLinearArithmetic => "nonlinear arithmetic".into(),
                        UnknownReason::UnsupportedTheory(s) => format!("unsupported theory: {}", s),
                        UnknownReason::BackendError(s) => format!("backend error: {}", s),
                        UnknownReason::NotAttempted(s) => s,
                    };
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
        return "(no specific values — trivial sat)".into();
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
                        let msg = format!(
                            "contract not satisfied in `{}`: counterexample {}",
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
                                let msg = format!(
                                    "`#must_verify` failed for `{}`: SMT returned unknown ({})",
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
                                "`#must_verify` failed for `{}`: encoder cannot represent contract ({})",
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
