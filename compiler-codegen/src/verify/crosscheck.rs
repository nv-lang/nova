//! Plan 33.14 Ф.3 + Ф.4: cross-check режим — Z3 ↔ CVC5.
//!
//! Вторая линия защиты soundness (в дополнение к Plan 33.8 Ф.7
//! regression-suite). Каждая verification condition прогоняется через
//! **два независимых пути**:
//!  - Z3 — через FFI-backend (`backend::z3`);
//!  - CVC5 — через текстовый SMT-LIB v2 и подпроцесс (`backend::cvc5`).
//!
//! Текстовый путь — это ещё и **второй независимый кодировщик**
//! (`smtlib`), не разделяющий код с Z3-FFI-трансляцией. Расхождение
//! `Proven` vs `Disproved` означает баг кодирования либо баг решателя.
//!
//! ## Что считается расхождением
//!
//! Gate срабатывает **только** на definite-disagreement: один путь
//! сказал `Proven` (unsat), другой — `Disproved` (sat). Любой `Unknown`
//! / timeout — норма (решатели имеют разные перф-профили), не ошибка.
//!
//! ## Где расхождения собираются
//!
//! `CrossCheckBackend` создаётся per-fn (как и `Z3Backend`), поэтому
//! расхождения копятся в **процесс-глобальном** коллекторе. `verify_module`
//! в конце сливает его и при непустом списке поднимает compile-error —
//! это и есть merge-gate (Ф.5): корпус контрактов под `NOVA_CROSSCHECK=1`
//! обязан быть зелёным.

use std::cell::RefCell;
use std::sync::Mutex;

use super::ir::{Model, ModelValue, SatResult, SmtTerm};

// ─────────────────────────────────────────────────────────────────────
// Расхождение и процесс-глобальный коллектор
// ─────────────────────────────────────────────────────────────────────

/// Одно definite-расхождение между Z3 и CVC5.
#[derive(Debug, Clone)]
pub struct Disagreement {
    /// Функция, при верификации которой найдено расхождение.
    pub fn_name: Option<String>,
    /// Verification condition (человекочитаемо).
    pub vc: String,
    /// Вердикт Z3.
    pub z3_verdict: String,
    /// Вердикт CVC5.
    pub cvc5_verdict: String,
    /// Контрпример решателя, сказавшего `Disproved` (если извлёкся).
    pub counterexample: Option<String>,
    /// SMT-LIB-скрипт, отданный cvc5 — для воспроизведения вручную.
    pub smtlib_script: String,
}

static DISAGREEMENTS: Mutex<Vec<Disagreement>> = Mutex::new(Vec::new());

/// Записать найденное расхождение в глобальный коллектор.
///
/// Дополнительно, если задан `NOVA_CROSSCHECK_LOG`, дописывает строку в
/// указанный файл. `nova test` компилирует каждый `.nv` отдельным
/// процессом, и in-process коллектор виден только своему процессу;
/// файл же аккумулирует расхождения всего прогона корпуса — на нём
/// строится airtight CI-gate (Ф.5). Append на POSIX атомарен для
/// коротких строк.
pub fn record_disagreement(d: Disagreement) {
    if let Ok(path) = std::env::var("NOVA_CROSSCHECK_LOG") {
        if !path.trim().is_empty() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path.trim())
            {
                let _ = writeln!(
                    f,
                    "DISAGREEMENT fn={} z3={} cvc5={} vc={}",
                    d.fn_name.as_deref().unwrap_or("?"),
                    d.z3_verdict,
                    d.cvc5_verdict,
                    d.vc
                );
            }
        }
    }
    if let Ok(mut g) = DISAGREEMENTS.lock() {
        g.push(d);
    }
}

/// Слить и очистить все накопленные расхождения.
pub fn take_disagreements() -> Vec<Disagreement> {
    match DISAGREEMENTS.lock() {
        Ok(mut g) => std::mem::take(&mut *g),
        Err(_) => Vec::new(),
    }
}

/// Сколько расхождений накоплено (не очищает).
pub fn disagreement_count() -> usize {
    DISAGREEMENTS.lock().map(|g| g.len()).unwrap_or(0)
}

thread_local! {
    /// Имя верифицируемой сейчас функции — для атрибуции расхождений.
    static CURRENT_FN: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Установить контекст текущей функции (вызывается `verify_module`).
pub fn set_current_fn(name: Option<String>) {
    CURRENT_FN.with(|c| *c.borrow_mut() = name);
}

fn current_fn() -> Option<String> {
    CURRENT_FN.with(|c| c.borrow().clone())
}

// ─────────────────────────────────────────────────────────────────────
// Классификация и сравнение вердиктов
// ─────────────────────────────────────────────────────────────────────

/// Огрублённый вердикт для сравнения двух решателей.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CcVerdict {
    /// `unsat` — контракт доказан.
    Proven,
    /// `sat` — найден контрпример.
    Disproved,
    /// `unknown` / timeout / ошибка backend'а.
    Indeterminate,
}

/// Огрубить `SatResult` до `CcVerdict`.
pub fn verdict_of(r: &SatResult) -> CcVerdict {
    match r {
        SatResult::Unsat(_) => CcVerdict::Proven,
        SatResult::Sat(_) => CcVerdict::Disproved,
        SatResult::Unknown(_) => CcVerdict::Indeterminate,
    }
}

/// Definite-disagreement: один путь `Proven`, другой — `Disproved`.
/// Любая комбинация с `Indeterminate` — НЕ расхождение (это норма).
pub fn is_disagreement(z3: CcVerdict, cvc5: CcVerdict) -> bool {
    matches!(
        (z3, cvc5),
        (CcVerdict::Proven, CcVerdict::Disproved) | (CcVerdict::Disproved, CcVerdict::Proven)
    )
}

/// Человекочитаемая метка вердикта (с причиной для `Unknown`).
pub fn verdict_label(r: &SatResult) -> String {
    match r {
        SatResult::Unsat(_) => "Proven (unsat)".to_string(),
        SatResult::Sat(_) => "Disproved (sat)".to_string(),
        SatResult::Unknown(reason) => format!("Unknown ({:?})", reason),
    }
}

/// Pretty-print VC: захваченная формула — это `not(goal)` из `try_prove`;
/// разворачиваем обратно к `goal`.
fn pretty_vc(captured: &SmtTerm) -> String {
    if let SmtTerm::App(op, args) = captured {
        if op == "not" && args.len() == 1 {
            return args[0].pretty();
        }
    }
    captured.pretty()
}

/// Краткий counterexample из модели — для diff-репорта.
/// Используется только в `CrossCheckBackend` (feature `z3-backend`).
#[cfg_attr(not(feature = "z3-backend"), allow(dead_code))]
fn format_model(model: &Model) -> Option<String> {
    let mut parts: Vec<String> = model
        .bindings
        .iter()
        // Служебные `_field_*`-константы (uf_…) в counterexample не нужны.
        .filter(|(name, _)| !name.starts_with("uf_"))
        .map(|(name, val)| {
            let v = match val {
                ModelValue::Int(n) => n.to_string(),
                ModelValue::Bool(b) => b.to_string(),
                ModelValue::Str(s) => format!("\"{}\"", s),
                ModelValue::Unknown => "?".to_string(),
            };
            format!("{} = {}", name, v)
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    parts.sort();
    if parts.len() > 12 {
        let extra = parts.len() - 12;
        parts.truncate(12);
        parts.push(format!("… (+{} ещё)", extra));
    }
    Some(parts.join(", "))
}

// ─────────────────────────────────────────────────────────────────────
// Diff-репорт
// ─────────────────────────────────────────────────────────────────────

/// Сформировать diff-репорт по одному расхождению (для compile-error).
pub fn format_one(d: &Disagreement) -> String {
    let mut s = String::new();
    s.push_str("CROSS-CHECK DISAGREEMENT [E2412]: Z3 и CVC5 разошлись\n");
    if let Some(fname) = &d.fn_name {
        s.push_str(&format!("  fn:   {}\n", fname));
    }
    s.push_str(&format!("  VC:   {}\n", d.vc));
    s.push_str(&format!("  Z3:   {}\n", d.z3_verdict));
    s.push_str(&format!("  CVC5: {}\n", d.cvc5_verdict));
    if let Some(cex) = &d.counterexample {
        s.push_str(&format!("  counterexample: {}\n", cex));
    }
    s.push_str(
        "  Один из решателей или один из путей кодирования (Z3-FFI либо\n  \
         SMT-LIB-текст) даёт неверный ответ. Это soundness-критично:\n  \
         верификатор мог объявить ложный `Proven`. Расследуйте VC\n  \
         вручную; SMT-LIB-скрипт ниже воспроизводит запрос для cvc5.\n",
    );
    // Скрипт — с отступом, чтобы выделялся в выводе компилятора.
    s.push_str("  --- SMT-LIB (cvc5) ---\n");
    for line in d.smtlib_script.lines() {
        s.push_str("  ");
        s.push_str(line);
        s.push('\n');
    }
    s.push_str("  --- end SMT-LIB ---");
    s
}

/// Сводный репорт по всем расхождениям (для CLI / лога).
pub fn format_report(disagreements: &[Disagreement]) -> String {
    if disagreements.is_empty() {
        return "cross-check: 0 расхождений — Z3 и CVC5 согласны.".to_string();
    }
    let mut s = format!(
        "cross-check: НАЙДЕНО {} definite-расхождений Z3 ↔ CVC5:\n\n",
        disagreements.len()
    );
    for (i, d) in disagreements.iter().enumerate() {
        s.push_str(&format!("[{}/{}] ", i + 1, disagreements.len()));
        s.push_str(&format_one(d));
        s.push_str("\n\n");
    }
    s
}

// ─────────────────────────────────────────────────────────────────────
// CrossCheckBackend — требует Z3 (feature `z3-backend`)
// ─────────────────────────────────────────────────────────────────────

#[cfg(feature = "z3-backend")]
pub use cc_backend::CrossCheckBackend;

#[cfg(feature = "z3-backend")]
mod cc_backend {
    use super::*;
    use crate::verify::backend::cvc5::Cvc5Backend;
    use crate::verify::backend::z3::Z3Backend;
    use crate::verify::backend::SmtBackend;
    use crate::verify::ir::{Assertion, ModelValue, SatResult, SmtTerm, SortRef};

    /// `SmtBackend`, прогоняющий каждый запрос через Z3 И CVC5.
    ///
    /// Все мутирующие вызовы зеркалятся в оба backend'а. `check_sat`
    /// спрашивает оба, сравнивает, при definite-расхождении пишет
    /// `Disagreement` в глобальный коллектор. Возвращает результат Z3 —
    /// cross-check **наблюдает**, но не меняет поведение верификации.
    pub struct CrossCheckBackend {
        z3: Z3Backend,
        cvc5: Cvc5Backend,
        /// Последняя формула, помеченная `goal-neg` (из `try_prove`).
        last_goal: Option<SmtTerm>,
    }

    impl CrossCheckBackend {
        pub fn new(timeout_ms: u32) -> Self {
            CrossCheckBackend {
                z3: Z3Backend::new(timeout_ms),
                cvc5: Cvc5Backend::new(timeout_ms),
                last_goal: None,
            }
        }
    }

    impl SmtBackend for CrossCheckBackend {
        fn name(&self) -> &'static str {
            "crosscheck(z3+cvc5)"
        }

        fn declare_var(&mut self, name: &str, sort: SortRef) {
            self.z3.declare_var(name, sort.clone());
            self.cvc5.declare_var(name, sort);
        }

        fn declare_function(
            &mut self,
            name: &str,
            param_sorts: &[SortRef],
            return_sort: SortRef,
        ) {
            self.z3.declare_function(name, param_sorts, return_sort.clone());
            self.cvc5.declare_function(name, param_sorts, return_sort);
        }

        fn assert(&mut self, assertion: Assertion) {
            // try_prove ассертит `not goal` под этой меткой прямо перед
            // check_sat — захватываем для diff-репорта.
            if assertion.label.as_deref() == Some("goal-neg") {
                self.last_goal = Some(assertion.formula.clone());
            }
            self.z3.assert(assertion.clone());
            self.cvc5.assert(assertion);
        }

        fn push(&mut self) {
            self.z3.push();
            self.cvc5.push();
        }

        fn pop(&mut self) {
            self.z3.pop();
            self.cvc5.pop();
        }

        fn get_witness(&mut self, var_name: &str) -> Option<ModelValue> {
            // Witness берём из Z3 — он primary для остального pipeline'а.
            self.z3.get_witness(var_name)
        }

        fn check_sat(&mut self) -> SatResult {
            let z3_result = self.z3.check_sat();
            let cvc5_result = self.cvc5.check_sat();

            let z3v = verdict_of(&z3_result);
            let cvc5v = verdict_of(&cvc5_result);

            if is_disagreement(z3v, cvc5v) {
                let vc = self
                    .last_goal
                    .as_ref()
                    .map(pretty_vc)
                    .unwrap_or_else(|| "<прямой check-sat>".to_string());
                // Контрпример — у того, кто сказал Disproved (sat).
                let counterexample = match (&z3_result, &cvc5_result) {
                    (SatResult::Sat(m), _) => format_model(m),
                    (_, SatResult::Sat(m)) => format_model(m),
                    _ => None,
                };
                record_disagreement(Disagreement {
                    fn_name: current_fn(),
                    vc,
                    z3_verdict: verdict_label(&z3_result),
                    cvc5_verdict: verdict_label(&cvc5_result),
                    counterexample,
                    smtlib_script: self.cvc5.last_script().to_string(),
                });
            }

            // Возвращаем вердикт Z3: cross-check не должен менять исход
            // верификации, только фиксировать расхождения.
            z3_result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::ir::{Model, UnknownReason, UnsatCore};
    use std::collections::HashMap;

    fn proven() -> SatResult {
        SatResult::Unsat(UnsatCore::default())
    }
    fn disproved() -> SatResult {
        SatResult::Sat(Model { bindings: HashMap::new() })
    }
    fn indet() -> SatResult {
        SatResult::Unknown(UnknownReason::Timeout)
    }

    #[test]
    fn agreement_is_not_disagreement() {
        assert!(!is_disagreement(verdict_of(&proven()), verdict_of(&proven())));
        assert!(!is_disagreement(verdict_of(&disproved()), verdict_of(&disproved())));
        assert!(!is_disagreement(verdict_of(&indet()), verdict_of(&indet())));
    }

    #[test]
    fn unknown_either_side_is_not_disagreement() {
        // «один Unknown — OK» (Plan 33.14 Ф.4).
        assert!(!is_disagreement(verdict_of(&proven()), verdict_of(&indet())));
        assert!(!is_disagreement(verdict_of(&indet()), verdict_of(&disproved())));
        assert!(!is_disagreement(verdict_of(&disproved()), verdict_of(&indet())));
        assert!(!is_disagreement(verdict_of(&indet()), verdict_of(&proven())));
    }

    #[test]
    fn proven_vs_disproved_is_disagreement() {
        assert!(is_disagreement(verdict_of(&proven()), verdict_of(&disproved())));
        assert!(is_disagreement(verdict_of(&disproved()), verdict_of(&proven())));
    }

    #[test]
    fn verdict_labels() {
        assert_eq!(verdict_label(&proven()), "Proven (unsat)");
        assert_eq!(verdict_label(&disproved()), "Disproved (sat)");
        assert!(verdict_label(&indet()).starts_with("Unknown"));
    }

    #[test]
    fn collector_record_take_roundtrip() {
        // Очистить возможный мусор от других тестов того же процесса.
        let _ = take_disagreements();
        record_disagreement(Disagreement {
            fn_name: Some("foo".into()),
            vc: "x > 0".into(),
            z3_verdict: "Proven (unsat)".into(),
            cvc5_verdict: "Disproved (sat)".into(),
            counterexample: Some("x = -1".into()),
            smtlib_script: "(check-sat)".into(),
        });
        let drained = take_disagreements();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].fn_name.as_deref(), Some("foo"));
        // Повторный take — пусто.
        assert_eq!(take_disagreements().len(), 0);
    }

    #[test]
    fn pretty_vc_unwraps_negation() {
        // try_prove передаёт `not(goal)`; репорт должен показать goal.
        let goal = SmtTerm::App(">".into(), vec![SmtTerm::Var("x".into()), SmtTerm::IntLit(0)]);
        let captured = SmtTerm::not(goal.clone());
        assert_eq!(pretty_vc(&captured), goal.pretty());
    }

    #[test]
    fn report_formatting() {
        let d = Disagreement {
            fn_name: Some("withdraw".into()),
            vc: "(acc.balance >= 0)".into(),
            z3_verdict: "Proven (unsat)".into(),
            cvc5_verdict: "Disproved (sat)".into(),
            counterexample: Some("balance = -1".into()),
            smtlib_script: "(set-logic ALL)\n(check-sat)".into(),
        };
        let one = format_one(&d);
        assert!(one.contains("CROSS-CHECK DISAGREEMENT"));
        assert!(one.contains("withdraw"));
        assert!(one.contains("Proven"));
        assert!(one.contains("Disproved"));
        assert!(one.contains("balance = -1"));
        assert!(one.contains("(check-sat)"));

        assert!(format_report(&[]).contains("0 расхождений"));
        assert!(format_report(&[d]).contains("НАЙДЕНО 1"));
    }

    #[test]
    fn current_fn_threadlocal() {
        set_current_fn(Some("bar".into()));
        assert_eq!(current_fn().as_deref(), Some("bar"));
        set_current_fn(None);
        assert_eq!(current_fn(), None);
    }
}
