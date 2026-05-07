//! Nova bootstrap compiler library.
//!
//! Точка входа для интеграционных тестов. CLI находится в `main.rs`.

pub mod ast;
pub mod codegen;
pub mod diag;
pub mod interp;
pub mod lexer;
pub mod lints;
pub mod parser;
pub mod types;

pub use diag::{Diagnostic, Span};
