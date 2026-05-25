//! LSP Backend — implements the `LanguageServer` trait from tower-lsp.
//!
//! Plan 104.0.1: skeleton (initialize/initialized/shutdown stubs).
//! Plan 104.0.2: lifecycle handlers — shutdown_requested guard.
//! Plan 104.0.3: textDocument/did* handlers — document cache population.
//! Plan 104.1+: diagnostics, hover, completion, quick-fixes (gate: Plan 91/100).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ropey::Rope;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::state::{ParsedFile, WorkspaceState};

// ─────────────────────────────────────────────────────────────────────────────
// Backend
// ─────────────────────────────────────────────────────────────────────────────

/// The LSP backend.
///
/// Holds:
/// - `client`: tower-lsp handle for server-initiated notifications
///   (e.g., `publishDiagnostics`, `window/showMessage`).
/// - `state`: shared workspace state (open documents, compiler cache).
///   Wrapped in `Arc` for cheap cloning into `tokio::spawn` tasks.
/// - `shutdown_requested`: set to `true` when the client calls `shutdown`,
///   used for deciding the process exit code in the `exit` notification path.
// `client` is used starting in Plan 104.1 (publishDiagnostics).
#[allow(dead_code)]
pub struct Backend {
    pub(crate) client: Client,
    pub(crate) state: Arc<WorkspaceState>,
    shutdown_requested: Arc<AtomicBool>,
}

impl Backend {
    /// Construct a new Backend.  Called once by `LspService::new`.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(WorkspaceState::default()),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LanguageServer impl
// ─────────────────────────────────────────────────────────────────────────────

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    // ── Lifecycle ────────────────────────────────────────────────────────────

    /// Respond to `initialize` with our server capabilities.
    ///
    /// Duplicate calls are rejected by tower-lsp's middleware with
    /// `InvalidRequest` (-32600) before this handler runs.
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("initialize");
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(PositionEncodingKind::UTF16),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Future capabilities (uncomment as sub-plans land):
                // 104.2: hover_provider, definition_provider, signature_help_provider
                // 104.3: completion_provider
                // 104.4: document_symbol_provider, workspace_symbol_provider
                // 104.5: code_action_provider
                // 104.6: rename_provider, document_formatting_provider
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "nova-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("nova-lsp ready");
        // Plan 104.1: trigger initial workspace file scan here.
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("nova-lsp shutdown");
        self.shutdown_requested.store(true, Ordering::Relaxed);
        // Plan 104.1: cancel background recheck workers here.
        Ok(())
    }

    // ── textDocument/did* ────────────────────────────────────────────────────
    //
    // V1 sync strategy: TextDocumentSyncKind::FULL — the editor re-sends the
    // entire document content on every change.  No incremental patch needed.
    // Plan 104.6 V2 will switch to Incremental + apply Rope range-edits.

    /// Cache a newly opened document.
    ///
    /// Per LSP spec, `didOpen` is sent exactly once per document (before any
    /// `didChange`).  A duplicate open (same URI) is a protocol violation by
    /// the client, but we handle it defensively: log a warning and overwrite.
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let text = Rope::from_str(&params.text_document.text);

        if self.state.docs.contains_key(&uri) {
            tracing::warn!(
                uri = %uri,
                "didOpen on already-open document; overwriting cached text"
            );
        }

        self.state.docs.insert(uri.clone(), ParsedFile { text, version });
        tracing::debug!(uri = %uri, version, "document opened and cached");

        // Plan 104.1: schedule background type-check + publishDiagnostics here.
    }

    /// Update the cached text for an already-open document.
    ///
    /// V1 (Full sync): `content_changes` always contains exactly one entry with
    /// the full document text and no `range` field.  We take the last entry as
    /// a safety net in case the client sends multiple.
    ///
    /// If the document isn't in the cache (client violated the LSP lifecycle by
    /// sending `didChange` before `didOpen`), we log a warning and ignore it.
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        let Some(change) = params.content_changes.into_iter().last() else {
            tracing::warn!(uri = %uri, "didChange with empty content_changes; ignoring");
            return;
        };

        match self.state.docs.get_mut(&uri) {
            Some(mut file) => {
                // Full sync: replace the entire document text.
                file.text = Rope::from_str(&change.text);
                file.version = version;
                tracing::debug!(uri = %uri, version, "document updated");
                // Plan 104.1: schedule debounced recheck here.
            }
            None => {
                tracing::warn!(
                    uri = %uri,
                    version,
                    "didChange on unopened document; ignoring"
                );
            }
        }
    }

    /// Remove a closed document from the cache.
    ///
    /// After `didClose` the editor is responsible for the file; we no longer
    /// need to keep its text in memory.
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.state.docs.remove(&uri);
        tracing::debug!(uri = %uri, "document closed and evicted from cache");
        // Plan 104.1: cancel any pending recheck for this URI here.
    }
}
