//! LSP Backend — implements the `LanguageServer` trait from tower-lsp.
//!
//! Plan 104.0.1: skeleton (initialize/initialized/shutdown stubs).
//! Plan 104.0.2: lifecycle handlers — shutdown_requested guard, exit notification.
//! Plan 104.0.3: textDocument/did* handlers connected to WorkspaceState.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::state::WorkspaceState;

/// The LSP backend.
///
/// Holds:
/// - `client`: tower-lsp handle for sending server-initiated notifications
///   (e.g., `publishDiagnostics`, `window/showMessage`).
/// - `state`: shared workspace state (open documents, compiler cache).
///   Wrapped in `Arc` so it can be cheaply cloned into background tasks.
/// - `shutdown_requested`: set to `true` when the client calls `shutdown`.
///   Used in `exit` to decide the exit code (0 if clean, 1 if premature).
// `client` and `state` are used starting in Plan 104.0.3 (document handlers)
// and 104.1 (publishDiagnostics).  Suppress the dead_code lint in the meantime.
#[allow(dead_code)]
pub struct Backend {
    pub(crate) client: Client,
    pub(crate) state: Arc<WorkspaceState>,
    shutdown_requested: Arc<AtomicBool>,
}

impl Backend {
    /// Construct a new Backend. Called once by `LspService::new`.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(WorkspaceState::default()),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    /// Respond to the `initialize` request with our server capabilities.
    ///
    /// Duplicate `initialize` calls (after the server is already initialized)
    /// are rejected by tower-lsp's middleware with `InvalidRequest` (-32600)
    /// before this handler is even called — so this method only runs once.
    ///
    /// V1 capabilities (Plan 104.0):
    /// - `positionEncoding`: UTF-16 (LSP 3.17 default).
    /// - `textDocumentSync`: Full — entire document re-sent on every change.
    ///   Incremental sync arrives in Plan 104.6 V2.
    ///
    /// Extended capabilities are added as sub-plans (104.1–104.6) land.
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("initialize");
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(PositionEncodingKind::UTF16),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Future capabilities (uncomment as sub-plans land):
                // Plan 104.2: hover_provider, definition_provider, signature_help_provider
                // Plan 104.3: completion_provider
                // Plan 104.4: document_symbol_provider, workspace_symbol_provider, references_provider
                // Plan 104.5: code_action_provider
                // Plan 104.6: rename_provider, document_formatting_provider
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "nova-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    /// Called after the client acknowledges our `initialize` response.
    /// This is the right place to start background work (e.g., workspace scan).
    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("nova-lsp ready");
        // Plan 104.1: trigger initial workspace file scan here.
        // Plan 104.1: register didChangeWatchedFiles capability here.
    }

    /// Called when the editor initiates a clean shutdown.
    ///
    /// The server MUST respond (with `null` per LSP spec) before the client
    /// sends the `exit` notification.  We set `shutdown_requested = true` so
    /// that `exit` can produce the correct exit code (0 for clean, 1 otherwise).
    async fn shutdown(&self) -> Result<()> {
        tracing::info!("nova-lsp shutdown");
        self.shutdown_requested.store(true, Ordering::Relaxed);
        // Plan 104.1: cancel and join background recheck workers here.
        Ok(())
    }
}
