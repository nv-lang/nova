//! Plan 33.1 Ф.3 (D24): Contract verification pipeline.
//!
//! Engine-agnostic SMT-backend через trait `SmtBackend`. Множественные
//! реализации:
//! - `TrivialBackend` (built-in, без внешних зависимостей) — доказывает
//!   тавтологии и простые impls через symbolic substitution.
//! - `Z3Backend` (опционально, feature `z3-backend`) — полный SMT через
//!   libz3.
//!
//! Pipeline:
//! 1. Encode Nova-выражение контракта в SMT-IR (`encode.rs`).
//! 2. Передать в backend через trait.
//! 3. Результат — `Proven` / `Disproved(counterexample)` / `Unknown(reason)`.
//!
//! Plan 33.1 scope: только straight-line код без mut/циклов/composition.
//! Полный pipeline (frame conditions, loops, decreases) — 33.2/33.3.

pub mod ir;
pub mod encode;
pub mod backend;
pub mod pipeline;
pub mod handler_exec;
pub mod cache;
pub mod suggest;

pub use ir::{Formula, SmtTerm, SortRef, SatResult, Model, UnsatCore, UnknownReason};
pub use backend::SmtBackend;
pub use backend::trivial::TrivialBackend;
pub use pipeline::{VerifyResult, VerificationPipeline, verify_module};
pub use handler_exec::verify_handlers;
