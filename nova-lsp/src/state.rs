//! Workspace state — shared mutable state for the LSP server.
//!
//! Plan 104.0.1: skeleton with empty WorkspaceState.
//! Full implementation (DashMap<Url, ParsedFile> + Rope) lands in Plan 104.0.3.

/// Shared workspace state: open document cache + compiler context.
///
/// One instance is created at server startup and shared (behind Arc) across
/// all LSP handler futures. Wrapping individual fields in DashMap / RwLock
/// rather than putting a single Mutex on the whole struct gives fine-grained
/// parallelism for concurrent didOpen/didChange events.
///
/// Plan 104.0.1: empty stub.
/// Plan 104.0.3: adds `docs: DashMap<Url, ParsedFile>`.
/// Plan 104.1:   adds compiler diagnostics cache + background recheck worker.
#[derive(Debug, Default)]
pub struct WorkspaceState;
