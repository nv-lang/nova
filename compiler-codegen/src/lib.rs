//! Nova bootstrap compiler library.
//!
//! Точка входа для интеграционных тестов. CLI находится в `main.rs`.

pub mod argbind;
pub mod ast;
pub mod callnorm;
pub mod chain_norm;
pub mod codegen;
pub mod const_fn_closure;
pub mod const_fn_eval;
pub mod const_fn_mono;
pub mod const_fn_trampoline;
pub mod desugar;
pub mod diag;
pub mod escape_analyze;
pub mod doc;
pub mod effect_surface;
pub mod field_cache;
pub mod git_cache;
pub mod imports;
pub mod interp;
pub mod lexer;
pub mod lints;
pub mod lockfile;
pub mod manifest;
pub mod parser;
pub mod perf_timer;
pub mod protocols;
pub mod resolver;
pub mod semver;
pub mod sig_registry;
pub mod test_runner;
pub mod types;
pub mod verify;

pub use diag::{Diagnostic, Span};

/// Plan 172.1 U.1.4 (§0 «никаких параллельных списков»): ЕДИНЫЙ источник набора
/// namespace-имён, для которых `Name.method(...)` — квалифицированный вызов в
/// пространстве имён типа (namespace-тип `type Name` без тела + static `fn
/// Name.method`, идиома nv-coding-style §23) ИЛИ intrinsic-неймспейс
/// (gc/fibers/runtime/bench — лоуэрятся в C-runtime). Раньше этот список был
/// ЗАХАРДКОЖЕН ДВАЖДЫ и hand-synced: types/mod.rs `is_intrinsic_namespace` ↔
/// codegen emit_c.rs inline `matches!`. Теперь — одна функция, оба слоя зовут её.
///
/// ВНИМАНИЕ (§3): сам список имён остаётся хардкодом — это **промежуточный**
/// шаг. Конечная цель (U.1.5/U.1.6) — резолвить «namespace-тип ли это» из
/// `.nv`-объявлений (`type Name`) / реестра, как для любого пользовательского
/// типа, и удалить этот список. Здесь он лишь дедуплицирован (drift убран).
pub fn is_intrinsic_namespace(name: &str) -> bool {
    matches!(
        name,
        "gc" | "fibers" | "runtime" | "Channel" | "ChanReader"
        | "ChanWriter" | "Time" | "Monotonic" | "CancelToken"
        | "StringBuilder" | "WriteBuffer" | "ReadBuffer"
        | "f64" | "f32" | "int" | "u8" | "u16" | "u32" | "u64"
        | "i8" | "i16" | "i32" | "i64" | "Self" | "Duration"
    )
}
