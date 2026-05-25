//! LSP Backend — implements the `LanguageServer` trait from tower-lsp.
//!
//! Plan 104.0.1: skeleton (initialize/initialized/shutdown stubs).
//! Plan 104.0.2: lifecycle handlers — shutdown_requested guard.
//! Plan 104.0.3: textDocument/did* handlers — document cache population.
//! Plan 104.1.Ф.4: TextDocumentSyncKind::Incremental — apply range edits.
//! Plan 104.1.Ф.5: publishDiagnostics — debounced background recompile.
//! Plan 104.1.Ф.6: multi-file workspace recheck on every didChange.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ropey::Rope;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::compiler::{check_file, check_workspace, run_with_large_stack};
use crate::diagnostic_mapping::to_lsp;
use crate::incremental::apply_changes;
use crate::state::{ParsedFile, WorkspaceState};

// ─────────────────────────────────────────────────────────────────────────────
// Backend
// ─────────────────────────────────────────────────────────────────────────────

/// The LSP backend.
///
/// Holds:
/// - `client`: tower-lsp handle for server-initiated notifications.
/// - `state`: shared workspace state (open documents, debouncer, workspace root).
/// - `shutdown_requested`: set to `true` when the client calls `shutdown`.
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

    /// Schedule a debounced recompile for `uri`.
    ///
    /// Strategy (V1):
    /// - If workspace root is set: full workspace recheck via `check_workspace`.
    ///   Publishes diagnostics for every .nv file found.
    /// - Otherwise: single-file check via `check_file`.
    ///
    /// V2 (future): per-module dep-graph to avoid rechecking unrelated files.
    fn schedule_recheck(&self, uri: Url, version: i32) {
        let client = self.client.clone();
        let state = Arc::clone(&self.state);
        let workspace_root = self.state.workspace_root();

        self.state.debouncer.schedule(uri.clone(), move |token| async move {
            if token.is_cancelled() {
                return;
            }

            if let Some(root) = workspace_root {
                // ── Full workspace recheck ────────────────────────────────────
                tracing::debug!(root = %root.display(), "workspace recheck triggered");

                let root_clone = root.clone();
                let results = tokio::task::spawn_blocking(move || {
                    run_with_large_stack(move || check_workspace(&root_clone))
                })
                .await;

                if token.is_cancelled() {
                    return;
                }

                match results {
                    Ok(check_results) => {
                        for cr in check_results {
                            if token.is_cancelled() {
                                return;
                            }
                            let rope = Rope::from_str(&cr.source);
                            let lsp_diags: Vec<Diagnostic> = cr
                                .diagnostics
                                .iter()
                                .map(|d| to_lsp(d, &rope, &cr.file_uri))
                                .collect();

                            // Version only applies to the changed file.
                            let ver = if cr.file_uri == uri { Some(version) } else { None };

                            tracing::debug!(
                                file = %cr.file_uri,
                                count = lsp_diags.len(),
                                "publishing workspace diagnostics"
                            );
                            client.publish_diagnostics(cr.file_uri, lsp_diags, ver).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "workspace recheck spawn_blocking failed");
                    }
                }
            } else {
                // ── Single-file check (no workspace root) ─────────────────────
                let text = match state.docs.get(&uri) {
                    Some(f) => f.text.to_string(),
                    None => {
                        tracing::warn!(uri = %uri, "recheck: document not in cache; skipping");
                        return;
                    }
                };

                if token.is_cancelled() {
                    return;
                }

                let uri_clone = uri.clone();
                let result = tokio::task::spawn_blocking(move || {
                    run_with_large_stack(move || check_file(&uri_clone, &text))
                })
                .await;

                if token.is_cancelled() {
                    return;
                }

                match result {
                    Ok(check_result) => {
                        let rope = Rope::from_str(&check_result.source);
                        let lsp_diags: Vec<Diagnostic> = check_result
                            .diagnostics
                            .iter()
                            .map(|d| to_lsp(d, &rope, &check_result.file_uri))
                            .collect();

                        tracing::debug!(
                            uri = %uri,
                            count = lsp_diags.len(),
                            "publishing single-file diagnostics"
                        );
                        client
                            .publish_diagnostics(uri.clone(), lsp_diags, Some(version))
                            .await;
                    }
                    Err(e) => {
                        tracing::error!(uri = %uri, err = %e, "spawn_blocking failed");
                    }
                }
            }
        });
    }

    /// Publish empty diagnostics for a URI (used on didClose to clear the editor).
    async fn publish_empty_diagnostics(&self, uri: Url) {
        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LanguageServer impl
// ─────────────────────────────────────────────────────────────────────────────

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    // ── Lifecycle ────────────────────────────────────────────────────────────

    /// Respond to `initialize` with our server capabilities.
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("initialize");

        // Extract workspace root from initialize params.
        #[allow(deprecated)] // root_uri is deprecated in LSP 3.17 but widely used
        if let Some(root_uri) = &params.root_uri {
            self.state.set_workspace_root_from_uri(root_uri);
            tracing::info!(root = %root_uri, "workspace root set");
        } else if let Some(folders) = &params.workspace_folders {
            if let Some(first) = folders.first() {
                self.state.set_workspace_root_from_uri(&first.uri);
                tracing::info!(root = %first.uri, "workspace root set from folders");
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(PositionEncodingKind::UTF16),
                // Plan 104.1.Ф.4: switch to Incremental sync.
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        will_save: None,
                        will_save_wait_until: None,
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(false),
                        })),
                    },
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
        // TODO: trigger initial workspace file scan here (Plan 104.1.Ф.6).
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("nova-lsp shutdown");
        self.shutdown_requested.store(true, Ordering::Relaxed);
        // Cancel all pending recheck workers.
        self.state.cancel_all();
        // Give in-flight tasks a moment to terminate.
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    // ── textDocument/did* ────────────────────────────────────────────────────

    /// Cache a newly opened document and schedule an immediate recheck.
    ///
    /// Per LSP spec, `didOpen` is sent exactly once per document (before any
    /// `didChange`).  A duplicate open is handled defensively: log + overwrite.
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

        // Immediate recheck on open (no debounce — user just opened the file).
        self.schedule_recheck(uri, version);
    }

    /// Apply incremental changes to the cached text and schedule a debounced recheck.
    ///
    /// Plan 104.1.Ф.4: handles TextDocumentSyncKind::Incremental changes.
    /// Each `ContentChangeEvent` carries a `range` + `text`; we apply them
    /// to the Rope in order.  A missing `range` means full text refresh.
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        if params.content_changes.is_empty() {
            tracing::warn!(uri = %uri, "didChange with empty content_changes; ignoring");
            return;
        }

        match self.state.docs.get_mut(&uri) {
            Some(mut file) => {
                apply_changes(&mut file.text, &params.content_changes);
                file.version = version;
                tracing::debug!(uri = %uri, version, "document updated (incremental)");
            }
            None => {
                tracing::warn!(
                    uri = %uri,
                    version,
                    "didChange on unopened document; inserting from full content"
                );
                // Recover: take the last change as a full text if possible.
                if let Some(last) = params.content_changes.last() {
                    self.state.docs.insert(
                        uri.clone(),
                        ParsedFile {
                            text: Rope::from_str(&last.text),
                            version,
                        },
                    );
                }
            }
        }

        // Debounced recheck — coalesces rapid edits.
        self.schedule_recheck(uri, version);
    }

    /// Handle didSave — trigger a recheck immediately (no debounce on save).
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        tracing::debug!(uri = %uri, "didSave — triggering immediate recheck");

        let version = self.state.docs.get(&uri).map(|f| f.version).unwrap_or(0);
        self.schedule_recheck(uri, version);
    }

    /// Remove a closed document from the cache and clear its diagnostics.
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.state.docs.remove(&uri);
        tracing::debug!(uri = %uri, "document closed and evicted from cache");

        // Clear diagnostics in the editor (LSP convention: empty list on close).
        self.publish_empty_diagnostics(uri).await;
    }
}
