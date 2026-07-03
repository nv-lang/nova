//! nova-lsp library root.
//!
//! Exposes internal modules so that integration tests in `tests/` can access
//! them without spawning a child process.
//!
//! The binary entry point is `src/main.rs`.

pub mod code_actions;
pub mod compiler;
pub mod completion;
pub mod debouncer;
pub mod diagnostic_mapping;
pub mod document_highlight;
pub mod folding_range;
pub mod format;
pub mod goto_definition;
pub mod hover;
pub mod incremental;
pub mod perf;
pub mod provenance;
pub mod rename;
pub mod selection_range;
pub mod semantic_tokens_delta;
pub mod server;
pub mod signature_help;
pub mod state;
pub mod stdlib_index;
pub mod symbol;
pub mod symbols;
pub mod type_definition;
pub mod workspace_lifecycle;
