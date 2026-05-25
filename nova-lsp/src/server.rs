//! LSP Backend — implements the `LanguageServer` trait from tower-lsp.
//!
//! Plan 104.0.1: skeleton with stubs; enough for `cargo build` + smoke tests.
//! Plan 104.0.2: fills in all lifecycle handlers + duplicate-initialize guard.
//! Plan 104.0.3: connects textDocument/did* handlers to WorkspaceState.

use std::sync::Arc;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::state::WorkspaceState;

/// The LSP backend.
///
/// Holds:
/// - `client`: tower-lsp client handle for sending server-initiated
///   notifications (e.g., `publishDiagnostics`, window/showMessage).
/// - `state`: shared workspace state (open documents, compiler cache).
///   Wrapped in `Arc` so it can be cloned across `tokio::spawn` tasks.
// Fields are used starting in Plan 104.0.2 (lifecycle) and 104.0.3 (state).
#[allow(dead_code)]
pub struct Backend {
    pub(crate) client: Client,
    pub(crate) state: Arc<WorkspaceState>,
}

impl Backend {
    /// Construct a new Backend. Called once by `LspService::new`.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(WorkspaceState::default()),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    /// Respond to the `initialize` request with our server capabilities.
    ///
    /// V1 capabilities (Plan 104.0):
    /// - `positionEncoding`: UTF-16 (LSP default; editors assume this unless
    ///    negotiated otherwise via `clientCapabilities.general.positionEncodings`)
    /// - `textDocumentSync`: Full — re-send entire document on every change
    ///    (incremental sync is Plan 104.6 V2)
    ///
    /// Extended capabilities (hover, completion, etc.) are added as the
    /// corresponding sub-plans (104.1 – 104.6) land.
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("initialize received");
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(PositionEncodingKind::UTF16),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Plan 104.1+: diagnostics (via publishDiagnostics — push, not pull)
                // Plan 104.2+: hover, definition, signatureHelp
                // Plan 104.3+: completion
                // Plan 104.4+: documentSymbol, workspaceSymbol, references
                // Plan 104.5+: codeAction
                // Plan 104.6+: rename, formatting
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "nova-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    /// Called after the client acknowledges our `initialize` response.
    /// Good place for server-side startup work (e.g., workspace scan).
    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("nova-lsp ready");
        // Plan 104.1: trigger initial workspace scan here.
    }

    /// Called when the editor requests a clean shutdown.
    /// Must respond before the client sends the `exit` notification.
    async fn shutdown(&self) -> Result<()> {
        tracing::info!("nova-lsp shutdown requested");
        // Plan 104.1: cancel pending background recheck workers here.
        Ok(())
    }
}
