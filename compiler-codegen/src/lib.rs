//! Nova bootstrap compiler library.
//!
//! Точка входа для интеграционных тестов. CLI находится в `main.rs`.

pub mod argbind;
pub mod ast;
pub mod callnorm;
pub mod codegen;
pub mod desugar;
pub mod diag;
pub mod doc;
pub mod git_cache;
pub mod imports;
pub mod interp;
pub mod lexer;
pub mod lints;
pub mod lockfile;
pub mod manifest;
pub mod parser;
pub mod perf_timer;
pub mod resolver;
pub mod semver;
pub mod test_runner;
pub mod types;
pub mod verify;

pub use diag::{Diagnostic, Span};
