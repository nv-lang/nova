//! SMT backend trait + implementations.
//!
//! Plan 33.1 Ф.3 (D24, D96): engine-agnostic verification.
//! Дизайн фиксирует **trait**, конкретный движок — выбор реализации.
//!
//! Реализации:
//! - `trivial::TrivialBackend` — built-in, без внешних deps. Доказывает
//!   тавтологии и простые symbolic substitutions.
//! - `z3::Z3Backend` (feature `z3-backend`, не активен сейчас) — полный
//!   SMT через libz3.

pub mod trivial;

#[cfg(feature = "z3-backend")]
pub mod z3_ffi;
#[cfg(feature = "z3-backend")]
pub mod z3;

#[cfg(feature = "z3-backend")]
pub use z3::Z3Backend;

use super::ir::{Assertion, Formula, SatResult, SortRef};

/// Backend для check-sat запросов. Engine-agnostic.
///
/// Lifecycle:
/// 1. `new(timeout_ms)` — создать backend.
/// 2. `declare_var(name, sort)` — объявить переменные.
/// 3. `assert(formula)` — добавить assumptions (requires, encoded body).
/// 4. `check_sat()` — попытка decide.
/// 5. `push` / `pop` — для перебора нескольких ensures по push/pop.
pub trait SmtBackend: Send {
    /// Backend name для diagnostic. Возвращает константу.
    fn name(&self) -> &'static str;

    /// Declare a variable of given sort.
    fn declare_var(&mut self, name: &str, sort: SortRef);

    /// Add assertion (assumption). Label опционально (для unsat-core).
    fn assert(&mut self, assertion: Assertion);

    /// Save current scope.
    fn push(&mut self);

    /// Restore last saved scope (drops assertions added after last push).
    fn pop(&mut self);

    /// Check satisfiability of all current assertions.
    fn check_sat(&mut self) -> SatResult;
}

/// Helper: prove that formula `goal` holds given current assertions.
/// Идёт по стандартной схеме: `assert (not goal); check_sat`.
/// `unsat` → proven. `sat` → counterexample. `unknown` → пропуск.
pub fn try_prove<B: SmtBackend + ?Sized>(backend: &mut B, goal: Formula) -> SatResult {
    backend.push();
    let neg = Formula::App("not".into(), vec![goal]);
    backend.assert(Assertion { formula: neg, label: Some("goal-neg".into()) });
    let result = backend.check_sat();
    backend.pop();
    result
}
