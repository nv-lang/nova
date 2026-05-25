//! nova-lsp library root.
//!
//! Exposes internal modules so that integration tests in `tests/` can access
//! them without spawning a child process.
//!
//! The binary entry point is `src/main.rs`.

pub mod compiler;
pub mod debouncer;
pub mod diagnostic_mapping;
pub mod incremental;
pub mod perf;
pub mod server;
pub mod state;
