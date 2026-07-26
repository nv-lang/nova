//! LSP Backend — implements the `LanguageServer` trait from tower-lsp.
//!
//! Plan 104.0.1: skeleton (initialize/initialized/shutdown stubs).
//! Plan 104.0.2: lifecycle handlers — shutdown_requested guard.
//! Plan 104.0.3: textDocument/did* handlers — document cache population.
//! Plan 104.1.Ф.4: TextDocumentSyncKind::Incremental — apply range edits.
//! Plan 104.1.Ф.5: publishDiagnostics — debounced background recompile.
//! Plan 104.1.Ф.6: multi-file workspace recheck on every didChange.
//! Plan 104.4: documentSymbol, workspaceSymbol, references handlers.
//! Plan 104.5: code_action — ≥25 quick-fixes via compute_code_actions.
//! Plan 104.6: rename + format-on-save handlers.

use std::sync::Arc;
use std::time::Duration;

use ropey::Rope;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::lsp_types::{notification, request};
use tower_lsp::{Client, LanguageServer};

use crate::code_actions::compute_code_actions_with_stdlib;
use crate::compiler::{
    check_file_with_root, check_open_documents, check_workspace, collect_nv_paths,
    run_with_large_stack,
};
use crate::completion;
use crate::diagnostic_mapping::to_lsp;
use crate::document_highlight::compute_document_highlights;
use crate::format::{format_document, format_range, on_type_format};
use crate::goto_definition::compute_goto_definition_in;
use crate::hover::compute_hover_in;
use crate::incremental::apply_changes;
use crate::index_cache;
use crate::rename::{prepare_rename, RenameDoc, compute_rename};
use crate::semantic_tokens_delta::{build_delta_response, SemanticTokensSnapshot};
use crate::signature_help::compute_signature_help_in;
use crate::state::{ParsedFile, WorkspaceState};
use crate::symbols::{
    collect_nv_files, compute_document_symbols, entries_to_workspace_symbols,
    symbol_at_position,
};
use crate::workspace_lifecycle::{
    apply_watched_event, classify_watch_uri, compute_rename_import_edits, RenamedFile,
    WatchTarget,
};

// ─────────────────────────────────────────────────────────────────────────────
// Backend
// ─────────────────────────────────────────────────────────────────────────────

/// The LSP backend.
///
/// Holds:
/// - `client`: tower-lsp handle for server-initiated notifications.
/// - `state`: shared workspace state (open documents, debouncer, workspace
///   root, and — LSP write-after-destroyed fix — the `shutting_down` flag
///   every background task checks; see `WorkspaceState::shutting_down` doc).
pub struct Backend {
    pub(crate) client: Client,
    pub(crate) state: Arc<WorkspaceState>,
}

impl Backend {
    /// Construct a new Backend.  Called once by `LspService::new`.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(WorkspaceState::default()),
        }
    }

    /// Plan 104.10 Ф.20: run `nova test <file> [--filter <test>]` for the
    /// run-test lens and report the outcome to the user.
    ///
    /// Real execution (no stub): spawns the `nova` binary (`NOVA_BIN`, else a
    /// sibling of the running `nova-lsp` executable, else `nova` on `PATH`),
    /// working from the workspace root so relative test resolution matches the
    /// CLI. `--quiet` keeps the captured output focused on the failure/summary
    /// lines; the full stdout/stderr is logged via `window/logMessage` and a
    /// one-line pass/fail summary is surfaced via `window/showMessage`.
    async fn run_nova_test(&self, file: &str, test: Option<&str>) {
        let bin = nova_binary();
        let label = match test {
            Some(t) => format!("{file} (filter: {t})"),
            None => file.to_string(),
        };
        self.client
            .show_message(MessageType::INFO, format!("Running nova test — {label}…"))
            .await;

        let mut cmd = tokio::process::Command::new(&bin);
        cmd.arg("test").arg(file);
        if let Some(t) = test {
            cmd.arg("--filter").arg(t);
        }
        cmd.arg("--quiet");
        if let Some(root) = self.state.workspace_root() {
            cmd.current_dir(root);
        }

        match cmd.output().await {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!(
                            "nova test `{label}` exited {}\n── stdout ──\n{stdout}\n── stderr ──\n{stderr}",
                            out.status
                        ),
                    )
                    .await;
                let summary = last_meaningful_line(&stdout)
                    .or_else(|| last_meaningful_line(&stderr))
                    .unwrap_or_else(|| match out.status.code() {
                        Some(c) => format!("exited with code {c}"),
                        None => "terminated by signal".to_string(),
                    });
                let ty = if out.status.success() {
                    MessageType::INFO
                } else {
                    MessageType::ERROR
                };
                self.client
                    .show_message(ty, format!("nova test — {summary}"))
                    .await;
            }
            Err(e) => {
                self.client
                    .show_message(
                        MessageType::ERROR,
                        format!(
                            "nova test failed to launch ({}): {e}. Set NOVA_BIN to the `nova` binary if it is not on PATH.",
                            bin.display()
                        ),
                    )
                    .await;
            }
        }
    }

    /// Schedule a debounced recompile for `uri`. Thin wrapper around
    /// [`schedule_recheck_for`] (a free function, Plan 213 Ф.2) so the
    /// debounced `did_change_watched_files` burst handler — which has no
    /// `&Backend` — can trigger the same recheck logic.
    fn schedule_recheck(&self, uri: Url, version: i32) {
        schedule_recheck_for(self.client.clone(), Arc::clone(&self.state), uri, version);
    }
}

/// Schedule a debounced recompile for `uri` (free-function body of
/// `Backend::schedule_recheck`).
///
/// Strategy:
/// - If workspace root is set: recheck every **currently open** document
///   (Plan 213 Ф.1/Ф.2 — previously this called `check_workspace(&root)`, a
///   full re-parse + type-check of **every** `.nv` file under the workspace
///   root on every debounced edit; on the main Nova repo that is 3000+ files,
///   each with its own import/prelude resolution, and was the primary cause
///   of the LSP burning ~27 CPU-hours/day. `check_open_documents` reuses the
///   same per-file resolution machinery but only over open buffers —
///   typically single digits). Publishes diagnostics for every open document.
/// - Otherwise: single-file check via `check_file`.
///
/// V2 (future): per-module dep-graph to avoid rechecking unrelated open
/// buffers too — see docs/plans/213-nova-lsp-performance.md Ф.4.
fn schedule_recheck_for(client: Client, state: Arc<WorkspaceState>, uri: Url, version: i32) {
    let workspace_root = state.workspace_root();
    // Clone the (cheap, `Arc`-backed) debouncer handle out first: calling
    // `.schedule()` directly on `state.debouncer` would hold a borrow of
    // `state` for the receiver while the `move` closure argument tries to
    // move the whole `state` into itself in the same expression (E0505).
    let debouncer = state.debouncer.clone();

    debouncer.schedule(uri.clone(), move |token| async move {
            if token.is_cancelled() || state.is_shutting_down() {
                return;
            }

            if let Some(root) = workspace_root {
                // ── Open-documents recheck (Plan 213 Ф.1) ─────────────────────
                tracing::debug!(root = %root.display(), "open-documents recheck triggered");

                let open_docs: Vec<(Url, String)> = state
                    .docs
                    .iter()
                    .map(|e| (e.key().clone(), e.value().text.to_string()))
                    .collect();
                let root_clone = root.clone();
                let results = tokio::task::spawn_blocking(move || {
                    run_with_large_stack(move || check_open_documents(&open_docs, &root_clone))
                })
                .await;

                if token.is_cancelled() || state.is_shutting_down() {
                    return;
                }

                match results {
                    Ok(check_results) => {
                        for cr in check_results {
                            if token.is_cancelled() || state.is_shutting_down() {
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
                                "publishing open-document diagnostics"
                            );
                            client.publish_diagnostics(cr.file_uri, lsp_diags, ver).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "open-documents recheck spawn_blocking failed");
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

                if token.is_cancelled() || state.is_shutting_down() {
                    return;
                }

                let uri_clone = uri.clone();
                // This branch runs only when no workspace root is set, so the
                // check falls back to path-based resolution (nearest nova.toml /
                // folder-module peers) inside `check_source_inner`.
                let result = tokio::task::spawn_blocking(move || {
                    run_with_large_stack(move || check_file_with_root(&uri_clone, &text, None))
                })
                .await;

                if token.is_cancelled() || state.is_shutting_down() {
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

impl Backend {
    /// Publish empty diagnostics for a URI (used on didClose to clear the editor).
    async fn publish_empty_diagnostics(&self, uri: Url) {
        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }

    // ── Plan 104.10 Ф.18: workspace-lifecycle helpers ───────────────────────

    /// Collect every candidate importer file: open documents (unsaved overlay)
    /// plus workspace `.nv` files on disk not currently open. Used by
    /// `willRenameFiles` to find dependents.
    fn collect_workspace_files(&self) -> Vec<(Url, String)> {
        let mut files: Vec<(Url, String)> = Vec::new();
        for entry in self.state.docs.iter() {
            files.push((entry.key().clone(), entry.value().text.to_string()));
        }
        if let Some(root) = self.state.workspace_root() {
            let open: std::collections::HashSet<Url> =
                files.iter().map(|(u, _)| u.clone()).collect();
            for (uri, text) in collect_nv_files(&root) {
                if !open.contains(&uri) {
                    files.push((uri, text));
                }
            }
        }
        files
    }

    /// Recompute diagnostics for currently-open documents (used after an external
    /// change / rename so results refresh without the user editing the buffer).
    /// Thin wrapper around [`recheck_open_documents_for`] (Plan 213 Ф.2 free
    /// function) so the debounced watch-batch handler in
    /// `did_change_watched_files` — which has no `&Backend` — can trigger the
    /// same logic.
    async fn recheck_open_documents(&self) {
        recheck_open_documents_for(&self.client, &self.state).await;
    }

    /// Push server→client refresh notifications so already-rendered semantic
    /// tokens / code lenses / inlay hints re-pull after a background reindex
    /// (Ф.18 sub-feature 4). Thin wrapper around [`refresh_client_hints_for`]
    /// (Plan 213 Ф.2 free function).
    async fn refresh_client_hints(&self) {
        refresh_client_hints_for(&self.client, &self.state).await;
    }
}

/// Free-function body of `Backend::recheck_open_documents` (Plan 213 Ф.2).
///
/// When a workspace root is set, one recheck republishes diagnostics for
/// every open document ([`schedule_recheck_for`]'s open-documents strategy,
/// Plan 213 Ф.1), so a single open document suffices as the trigger.
/// Otherwise every open document is rechecked individually.
async fn recheck_open_documents_for(client: &Client, state: &Arc<WorkspaceState>) {
    if state.is_shutting_down() {
        return;
    }
    if state.workspace_root().is_some() {
        if let Some(entry) = state.docs.iter().next() {
            let uri = entry.key().clone();
            let version = entry.value().version;
            drop(entry);
            schedule_recheck_for(client.clone(), Arc::clone(state), uri, version);
        }
        return;
    }
    let open: Vec<(Url, i32)> = state
        .docs
        .iter()
        .map(|e| (e.key().clone(), e.value().version))
        .collect();
    for (uri, version) in open {
        schedule_recheck_for(client.clone(), Arc::clone(state), uri, version);
    }
}

/// Free-function body of `Backend::refresh_client_hints` (Plan 213 Ф.2).
/// Each notification is a server→client *request*; a non-responsive client
/// must not stall the caller, so each is bounded by a short timeout (a real
/// client answers in milliseconds). Errors are swallowed (a client lacking
/// the capability is fine).
async fn refresh_client_hints_for(client: &Client, state: &WorkspaceState) {
    if state.is_shutting_down() {
        return;
    }
    let st = client.semantic_tokens_refresh();
    if let Ok(Err(e)) = tokio::time::timeout(Duration::from_secs(2), st).await {
        tracing::debug!(err = %e, "semanticTokens/refresh not honoured");
    }
    let cl = client.code_lens_refresh();
    if let Ok(Err(e)) = tokio::time::timeout(Duration::from_secs(2), cl).await {
        tracing::debug!(err = %e, "codeLens/refresh not honoured");
    }
    let ih = client.inlay_hint_refresh();
    if let Ok(Err(e)) = tokio::time::timeout(Duration::from_secs(2), ih).await {
        tracing::debug!(err = %e, "inlayHint/refresh not honoured");
    }
}

/// Fixed debounce key (Plan 213 Ф.2) for coalescing `didChangeWatchedFiles`
/// bursts. Not a real document URI — just a stable map key so the per-URI
/// `Debouncer` mechanism (already used for interactive edits) collapses a
/// rapid run of watcher notifications into one apply-pass + one recheck,
/// matching how gopls/rust-analyzer absorb git-checkout / branch-switch
/// storms.
fn watch_batch_key() -> Url {
    Url::parse("nova-lsp-internal://watch-batch").expect("static URL is valid")
}

impl Backend {
    /// Run the cold initial workspace scan wrapped in a `$/progress` token so the
    /// IDE shows a determinate spinner (begin → report → end) instead of looking
    /// hung. Indexes every `.nv` file for `workspace/symbol`, publishes initial
    /// diagnostics, and refreshes push-based hints on completion.
    async fn run_initial_scan_with_progress(&self) {
        let Some(root) = self.state.workspace_root() else { return };
        // LSP write-after-destroyed fix: bail out at every yield point once
        // `shutdown` has been received — see `WorkspaceState::shutting_down`.
        if self.state.is_shutting_down() {
            return;
        }

        // A unique progress token (server-initiated). Per LSP the server must ask
        // the client to create it first; we do so but do not hard-block on the
        // acknowledgement (a naive client may not answer — after a short grace we
        // proceed so the scan is never gated on the client).
        let token = NumberOrString::String(format!(
            "nova-lsp/initial-scan/{}",
            self.state.next_semantic_tokens_result_id()
        ));
        let create = self
            .client
            .send_request::<request::WorkDoneProgressCreate>(WorkDoneProgressCreateParams {
                token: token.clone(),
            });
        let _ = tokio::time::timeout(Duration::from_millis(500), create).await;

        if self.state.is_shutting_down() {
            return;
        }

        // begin
        self.send_progress(
            &token,
            WorkDoneProgress::Begin(WorkDoneProgressBegin {
                title: "Nova: indexing workspace".to_string(),
                cancellable: Some(false),
                message: Some("scanning files".to_string()),
                percentage: Some(0),
            }),
        )
        .await;

        // Plan 215: warm-start the workspace/symbol + references index (Ф.12)
        // from the persistent on-disk cache instead of unconditionally
        // re-parsing every `.nv` file. `index_cache::load` degrades to `None`
        // on first run / a corrupt or format-mismatched cache file — this
        // path then behaves exactly like the old unconditional full scan
        // below (every file lands in `to_reindex`), so there's no unsafe
        // "trust the cache" branch here, only a fast path that skips
        // re-parsing files whose (mtime, size) fingerprint is unchanged.
        let scan_t0 = std::time::Instant::now();
        let persisted = index_cache::load(&root).unwrap_or_default();
        let had_cache = !persisted.files.is_empty();

        // Path-only listing (no content read yet — Plan 213 Ф.1's filtered
        // walk) so a warm start never pays the I/O cost of reading files it's
        // about to skip.
        let paths = collect_nv_paths(&root);
        let total_files = paths.len();

        let mut new_persisted = index_cache::PersistedIndex::new();
        let mut to_reindex: Vec<std::path::PathBuf> = Vec::new();
        let mut warm_hits = 0usize;

        for path in &paths {
            let Some(uri) = Url::from_file_path(path).ok() else { continue };
            if let Some(fp) = index_cache::file_fingerprint(path) {
                let key = uri.as_str().to_string();
                if let Some(entry) = persisted.files.get(&key) {
                    if (entry.mtime_nanos, entry.size) == fp {
                        // Unchanged since the cache was written — install the
                        // pre-computed entries directly, no parse.
                        self.state.workspace_index.install_file(uri.clone(), entry.symbols.clone());
                        self.state.references_index.install_file(uri.clone(), entry.refs.clone());
                        new_persisted.files.insert(key, entry.clone());
                        warm_hits += 1;
                        continue;
                    }
                }
            }
            to_reindex.push(path.clone());
        }

        tracing::info!(
            total_files,
            warm_hits,
            stale = to_reindex.len(),
            had_cache,
            fingerprint_pass_ms = scan_t0.elapsed().as_millis(),
            "nova-lsp: index cache warm-start check"
        );

        // The index already reflects every warm-cache hit — answer
        // workspace/symbol and references requests immediately instead of
        // blocking on the stale-file reindex loop below (which may still
        // have thousands of files left after a first-ever cold start).
        self.state.references_index.mark_primed();

        if self.state.is_shutting_down() {
            self.end_progress_on_shutdown(&token).await;
            return;
        }

        self.send_progress(
            &token,
            WorkDoneProgress::Report(WorkDoneProgressReport {
                cancellable: Some(false),
                message: Some(format!(
                    "{warm_hits}/{total_files} warm from cache, reindexing {} stale",
                    to_reindex.len()
                )),
                percentage: Some(20),
            }),
        )
        .await;

        // Reindex the stale set. Plan 215 "open documents first": a file
        // already open in the editor (`state.docs`) is skipped here entirely
        // — `did_open`/`did_change` already indexed it from the live buffer
        // (the source of truth; reading the on-disk copy here could clobber
        // unsaved edits — same principle as `workspace_lifecycle::
        // apply_watched_event`) — so an open document never waits behind
        // this loop's position at all, warm or cold.
        //
        // CPU-scromness (Plan 213 Ф.2 precedent: the owner has previously
        // disabled the LSP entirely over background CPU usage): yield to the
        // tokio scheduler after every file, and actively sleep every
        // `REINDEX_SLEEP_EVERY` files, so a large stale set (e.g. after a
        // `git pull` touching hundreds of files, or a first-ever cold start)
        // never monopolizes a core against concurrent interactive requests.
        const REINDEX_SLEEP_EVERY: usize = 64;
        const REINDEX_SLEEP: Duration = Duration::from_millis(10);

        let total_stale = to_reindex.len().max(1);
        let mut processed = 0usize;
        for (i, path) in to_reindex.iter().enumerate() {
            if self.state.is_shutting_down() {
                tracing::info!("nova-lsp: initial scan aborted — shutdown requested");
                self.end_progress_on_shutdown(&token).await;
                return;
            }
            let Some(uri) = Url::from_file_path(path).ok() else { continue };
            if self.state.docs.contains_key(&uri) {
                continue; // already fresh via the open-document path
            }
            let Ok(src) = std::fs::read_to_string(path) else { continue };
            self.state.workspace_index.index_file(uri.clone(), &src);
            self.state.references_index.index_file(uri.clone(), &src);

            if let Some((mtime_nanos, size)) = index_cache::file_fingerprint(path) {
                new_persisted.files.insert(
                    uri.as_str().to_string(),
                    index_cache::CachedFile {
                        mtime_nanos,
                        size,
                        symbols: self.state.workspace_index.export_file(&uri),
                        refs: self.state.references_index.export_file(&uri),
                    },
                );
            }

            if i % 16 == 0 {
                let pct = 20 + ((i * 60) / total_stale) as u32;
                self.send_progress(
                    &token,
                    WorkDoneProgress::Report(WorkDoneProgressReport {
                        cancellable: Some(false),
                        message: Some(format!("reindexed {}/{} stale files", i, to_reindex.len())),
                        percentage: Some(pct.min(80)),
                    }),
                )
                .await;
            }

            processed += 1;
            tokio::task::yield_now().await;
            if processed % REINDEX_SLEEP_EVERY == 0 {
                tokio::time::sleep(REINDEX_SLEEP).await;
            }
        }

        // Best-effort persist for the next server start. A write failure
        // (read-only filesystem, disk full, …) only degrades the *next*
        // start back to cold — never this session.
        index_cache::save(&root, &new_persisted);

        tracing::info!(
            total_files,
            warm_hits,
            reindexed = to_reindex.len(),
            total_elapsed_ms = scan_t0.elapsed().as_millis(),
            "nova-lsp: workspace index ready"
        );

        if self.state.is_shutting_down() {
            self.end_progress_on_shutdown(&token).await;
            return;
        }

        // Cold type-check pass → publish initial diagnostics.
        self.send_progress(
            &token,
            WorkDoneProgress::Report(WorkDoneProgressReport {
                cancellable: Some(false),
                message: Some("type-checking".to_string()),
                percentage: Some(92),
            }),
        )
        .await;

        let root_clone = root.clone();
        let results = tokio::task::spawn_blocking(move || {
            run_with_large_stack(move || check_workspace(&root_clone))
        })
        .await;
        if let Ok(check_results) = results {
            for cr in check_results {
                if self.state.is_shutting_down() {
                    self.end_progress_on_shutdown(&token).await;
                    return;
                }
                let rope = Rope::from_str(&cr.source);
                let lsp_diags: Vec<Diagnostic> = cr
                    .diagnostics
                    .iter()
                    .map(|d| to_lsp(d, &rope, &cr.file_uri))
                    .collect();
                self.client
                    .publish_diagnostics(cr.file_uri, lsp_diags, None)
                    .await;
            }
        }

        if self.state.is_shutting_down() {
            self.end_progress_on_shutdown(&token).await;
            return;
        }

        // end
        self.send_progress(
            &token,
            WorkDoneProgress::End(WorkDoneProgressEnd {
                message: Some("workspace ready".to_string()),
            }),
        )
        .await;

        // Fresh index/expr-types → refresh push-based hints.
        self.refresh_client_hints().await;
    }

    /// Send one `$/progress` notification for `token`.
    async fn send_progress(&self, token: &NumberOrString, value: WorkDoneProgress) {
        self.client
            .send_notification::<notification::Progress>(ProgressParams {
                token: token.clone(),
                value: ProgressParamsValue::WorkDone(value),
            })
            .await;
    }

    /// Best-effort `WorkDoneProgress::End` for a token whose scan is bailing
    /// out early (`is_shutting_down()`) — without this, `run_initial_scan_
    /// with_progress`'s early `return`s leave the client's progress spinner
    /// open forever (found 2026-07-26: owner's "indexing workspace: type-
    /// checking" never clears, even across a clean client restart, because
    /// the KILLED instance's token was never closed — only the NEW instance's
    /// own cycle completes cleanly, and old orphaned tokens accumulate).
    /// Safe to call here: `shutdown()` gives in-flight tasks a ~100ms grace
    /// window before the transport actually dies (LSP `exit` comes later),
    /// so this notification still has a live stream to write to.
    async fn end_progress_on_shutdown(&self, token: &NumberOrString) {
        self.send_progress(
            token,
            WorkDoneProgress::End(WorkDoneProgressEnd {
                message: Some("cancelled — shutting down".to_string()),
            }),
        )
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

        // Plan 104.10 Ф.9: seed inlay-hint config from initializationOptions
        // (both kinds default on; the client may disable either).
        if let Some(opts) = &params.initialization_options {
            let cfg = crate::inlay_hints::InlayHintConfig::from_settings(opts);
            self.state.set_inlay_config(cfg);
            tracing::info!(?cfg, "inlay-hint config from initializationOptions");
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
                // Plan 104.5: code_action_provider — ≥25 quick-fixes
                // (extends Plan 114 Ф.7.2 E_KW_REMOVED_LET / E_KW_REMOVED_READONLY).
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::REFACTOR,
                            CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                        ]),
                        resolve_provider: Some(false),
                        work_done_progress_options: Default::default(),
                    },
                )),
                // Plan 123.5.1 (V5.1): field-cache code-lens над method
                // headers ("N caches inserted") + hover provider over
                // `@field` showing cache info.
                // Plan 104.10 Ф.20: navigation lenses (run-test / references /
                // implementations) in addition to the Plan 123.5.1 field-cache
                // lens. `resolve_provider = false` — titles/counts are computed
                // eagerly from the real indexes (Ф.12 refs, Ф.19 impl scan).
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                // Plan 104.10 Ф.20: server-side commands. `nova.runTest` is the
                // executeCommand target of the run-test lens — it shells out to
                // `nova test <file> --filter <name>` and reports the outcome via
                // `window/showMessage`. The references/implementations lenses use
                // the client-side `editor.action.showReferences` (no server
                // round-trip) so they are not listed here.
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![crate::code_lens::CMD_RUN_TEST.to_string()],
                    work_done_progress_options: Default::default(),
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Plan 104.2: goto-definition handler.
                definition_provider: Some(OneOf::Left(true)),
                // Plan 104.2: signature-help for function/method calls.
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    work_done_progress_options: Default::default(),
                }),
                // Plan 123.5.2 (V5.2, 2026-06-02): semantic tokens
                // for `@<field>` reads that field_cache analysis decides
                // to CSE/cache. Colors them differently from plain
                // field accesses. Legend defines the custom modifier
                // "cached" alongside standard "property" type.
                // Plan 123.5.5 (V5.5, 2026-06-03): advertise delta
                // support so clients send `semanticTokens/full/delta`
                // after the first full request — bandwidth-saving для
                // typical incremental edits.
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: Default::default(),
                            legend: SemanticTokensLegend {
                                // Plan 104.10 Ф.10: full legend (superset of the
                                // Plan 123.5.2 single-PROPERTY legend).
                                token_types:
                                    crate::semantic_tokens::semantic_token_legend_types(),
                                token_modifiers:
                                    crate::semantic_tokens::semantic_token_legend_modifiers(),
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Delta {
                                delta: Some(true),
                            }),
                        },
                    ),
                ),
                // Plan 104.3: completion provider — keywords, identifiers, methods, imports.
                // Trigger chars: "." (method), " " (keyword/ident), ":" (type annotation).
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        " ".to_string(),
                        ":".to_string(),
                    ]),
                    // Plan 104.10 Ф.13: lazy resolve — the initial list omits
                    // heavy detail/documentation; the client fetches them per
                    // item via completionItem/resolve.
                    resolve_provider: Some(true),
                    work_done_progress_options: Default::default(),
                    completion_item: None,
                    all_commit_characters: None,
                }),
                // Plan 104.4: document symbols, workspace symbols, references.
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                // Plan 104.10 Ф.15: documentHighlight — occurrences of the
                // symbol under the cursor in the current file (read/write kind),
                // resolved semantically via the Ф.7 scope resolver.
                document_highlight_provider: Some(OneOf::Left(true)),
                // Plan 104.10 Ф.19: typeDefinition (type of the expression under
                // the cursor → its `type` declaration, via Ф.2 expr_types) and
                // implementation (protocol → implementing types; method → its
                // implementations, via the AST #impl / method registry).
                type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                // Plan 104.10 Ф.16: foldingRange — syntactic AST-walk yielding
                // regions for fn/type bodies, nested `{ }` blocks, import groups
                // and multi-line doc-comments (not an indentation heuristic).
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                // Plan 104.10 Ф.17: selectionRange — smart-expand chains derived
                // from the AST node hierarchy (ident → expr → stmt → block → fn),
                // each range a strict superset of the previous. Parse-only.
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                // Plan 104.10 Ф.9: inlay hints — type hints for un-annotated
                // `ro x = expr` bindings (`: T` from Ф.2 expr_types) and
                // parameter-name hints before call arguments (`a:`/`b:` from the
                // resolved callee). Both toggleable via config (default on);
                // `resolve_provider=false` — labels are computed eagerly.
                inlay_hint_provider: Some(OneOf::Right(
                    InlayHintServerCapabilities::Options(InlayHintOptions {
                        work_done_progress_options: Default::default(),
                        resolve_provider: Some(false),
                    }),
                )),
                // Plan 104.6: rename + format-on-save.
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                    first_trigger_character: "\n".to_string(),
                    more_trigger_character: Some(vec!["}".to_string()]),
                }),
                // Plan 104.10 Ф.18: workspace lifecycle — advertise
                // willRenameFiles / didRenameFiles for `*.nv` so the editor asks
                // us for an import-fixup WorkspaceEdit on rename. File *watching*
                // is registered dynamically in `initialized()` (needs the client
                // to support dynamic registration; static advertisement here is
                // insufficient for didChangeWatchedFiles).
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: None,
                    file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                        will_rename: Some(nv_file_operation_registration()),
                        did_rename: Some(nv_file_operation_registration()),
                        ..Default::default()
                    }),
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "nova-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    /// Plan 104.10 Ф.18: on `initialized`, (1) dynamically register file
    /// watchers so the client forwards external `*.nv` / `nova.toml` changes via
    /// `workspace/didChangeWatchedFiles`, and (2) run the cold initial workspace
    /// scan under a `$/progress` token (spinner instead of an apparent hang),
    /// then push semanticTokens/codeLens refreshes so already-rendered hints
    /// re-pull the freshly-indexed data.
    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("nova-lsp ready");

        // (1) Dynamic watcher registration — `**/*.nv` and `**/nova.toml`.
        let registration = Registration {
            id: "nova-lsp-watch-nv".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                watchers: vec![
                    FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/*.nv".to_string()),
                        kind: None, // create | change | delete (default 7)
                    },
                    FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/nova.toml".to_string()),
                        kind: None,
                    },
                ],
            })
            .ok(),
        };
        // Register in the background: a client without dynamic-registration
        // support (or a slow one) must never block the initial scan below. The
        // server still functions if registration is rejected (just no
        // external-change reaction). Log, never fail.
        {
            let client = self.client.clone();
            let state = Arc::clone(&self.state);
            tokio::spawn(async move {
                if state.is_shutting_down() {
                    return;
                }
                if let Err(e) = client.register_capability(vec![registration]).await {
                    tracing::warn!(err = %e, "didChangeWatchedFiles dynamic registration rejected");
                }
            });
        }

        // (2) Cold initial workspace scan with progress + refresh.
        if self.state.workspace_root().is_some() {
            self.run_initial_scan_with_progress().await;
        }
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("nova-lsp shutdown");
        // LSP write-after-destroyed fix: flip the shared flag every
        // background task checks (cold scan, debounced recheck, watch
        // batch, hint refresh) — see `WorkspaceState::shutting_down` doc.
        self.state.mark_shutting_down();
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

        self.state.docs.insert(uri.clone(), ParsedFile { text: text.clone(), version });
        tracing::debug!(uri = %uri, version, "document opened and cached");

        // Plan 104.4: invalidate document symbol cache + update workspace index.
        // Plan 104.10 Ф.12: refresh the incremental references index from the
        // open-buffer text (source of truth for open files).
        self.state.document_symbol_cache.invalidate(&uri);
        {
            let src = text.to_string();
            self.state.workspace_index.index_file(uri.clone(), &src);
            self.state.references_index.index_file(uri.clone(), &src);
        }

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

        // Plan 104.4: invalidate document symbol cache + re-index workspace symbols.
        // Plan 104.10 Ф.12: re-index the references occurrences incrementally.
        self.state.document_symbol_cache.invalidate(&uri);
        if let Some(doc) = self.state.docs.get(&uri) {
            let src = doc.text.to_string();
            drop(doc);
            self.state.workspace_index.index_file(uri.clone(), &src);
            self.state.references_index.index_file(uri.clone(), &src);
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
        // Plan 104.4: evict from symbol caches.
        self.state.document_symbol_cache.invalidate(&uri);
        // Plan 104.10 Ф.1: evict the resolved-module cache (bounds memory to
        // open documents).
        self.state.invalidate_resolved(&uri);
        // Note: we do NOT remove from workspace_index on close — the file still
        // exists on disk; its symbols remain searchable (consistent with gopls).
        tracing::debug!(uri = %uri, "document closed and evicted from cache");

        // Clear diagnostics in the editor (LSP convention: empty list on close).
        self.publish_empty_diagnostics(uri).await;
    }

    // ── Plan 104.10 Ф.18: Workspace lifecycle ───────────────────────────────

    /// `workspace/didChangeWatchedFiles` — react to *external* file changes
    /// (git checkout/pull, edits outside the editor, codegen output).
    ///
    /// Plan 213 Ф.2: a git checkout / branch switch / build can deliver
    /// **many** separate notifications in rapid succession (one per touched
    /// file, or chunked batches) — previously each notification synchronously
    /// applied its events (disk read + parse per file, directly on the async
    /// task, never `spawn_blocking`) and then unconditionally triggered
    /// `invalidate_all_resolved()` + a full recheck, with no coalescing across
    /// notifications. Now: events are only cheaply classified here (no I/O),
    /// buffered into `state.pending_watch_events`, and the actual apply-pass +
    /// recheck is scheduled on `state.watch_debouncer` (400ms, separate from
    /// the 200ms interactive-edit debouncer so a burst of N notifications
    /// inside that window collapses into exactly ONE apply-pass + ONE recheck
    /// — the parse/apply work itself also moves to `spawn_blocking` so it
    /// never blocks the async runtime.
    ///
    /// Each buffered event is applied to the caches by [`apply_watched_event`]
    /// (real invalidation of the Ф.1 resolved cache and the Ф.12 symbol index
    /// — not a server restart). A `.nv` change additionally invalidates every
    /// open document's resolved build (reverse-dependency superset,
    /// `[M-104.10-watch-reverse-deps]`). If anything relevant changed,
    /// diagnostics for open documents are recomputed and semanticTokens/codeLens
    /// refreshes are pushed so stale hints re-pull.
    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        // Cheap filter (no I/O): drop events the watchers should not even have
        // sent (defensive — see `classify_watch_uri` doc comment).
        let relevant: Vec<FileEvent> = params
            .changes
            .into_iter()
            .filter(|e| {
                let target = classify_watch_uri(&e.uri);
                if target == WatchTarget::Ignore {
                    // NEG: a watch event on a non-.nv / non-manifest file is ignored.
                    tracing::trace!(uri = %e.uri, "watched-file event ignored (not .nv/nova.toml)");
                }
                target != WatchTarget::Ignore
            })
            .collect();
        if relevant.is_empty() {
            return;
        }

        {
            let mut pending = self.state.pending_watch_events.lock().unwrap();
            pending.extend(relevant);
        }

        let client = self.client.clone();
        let state = Arc::clone(&self.state);
        self.state
            .watch_debouncer
            .schedule(watch_batch_key(), move |token| async move {
                if token.is_cancelled() || state.is_shutting_down() {
                    return;
                }

                let events: Vec<FileEvent> = {
                    let mut pending = state.pending_watch_events.lock().unwrap();
                    std::mem::take(&mut *pending)
                };
                if events.is_empty() {
                    return;
                }

                // Apply on a blocking thread: disk reads + parsing (symbol /
                // references indexing) must never block the async runtime,
                // especially for a large burst (e.g. a branch switch touching
                // thousands of files across std/examples/spec_tests).
                let state_for_apply = Arc::clone(&state);
                let (any_relevant, any_nv) = tokio::task::spawn_blocking(move || {
                    let mut any_relevant = false;
                    let mut any_nv = false;
                    for event in &events {
                        let target = classify_watch_uri(&event.uri);
                        if target == WatchTarget::Nv {
                            any_nv = true;
                        }
                        let outcome = apply_watched_event(&state_for_apply, event);
                        any_relevant |= outcome.relevant;
                        tracing::debug!(uri = %event.uri, ?target, "watched-file event applied");
                    }
                    (any_relevant, any_nv)
                })
                .await
                .unwrap_or((false, false));

                if !any_relevant || token.is_cancelled() || state.is_shutting_down() {
                    return;
                }

                // A changed peer file may invalidate any open importer's cached
                // build. Clear the whole resolved cache (correct superset;
                // rebuilt lazily).
                if any_nv {
                    state.invalidate_all_resolved();
                }

                // Recompute diagnostics for open documents so external changes
                // surface without the user touching the buffer, then refresh
                // push-based hints.
                recheck_open_documents_for(&client, &state).await;
                refresh_client_hints_for(&client, &state).await;
            });
    }

    /// `workspace/willRenameFiles` — return a `WorkspaceEdit` that rewrites the
    /// `import` paths of every dependent file so they keep resolving to the
    /// renamed `.nv` file (Nova imports are path-based).
    ///
    /// Only `.nv` renames are considered. The pre-rename text is taken from the
    /// open buffer if present, else read from disk (the old path still exists —
    /// willRename fires *before* the rename). See `[M-104.10-file-rename-imports]`
    /// for the precise scope boundary.
    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        // Build the RenamedFile set (only .nv, only where we can read old text).
        let mut renames: Vec<RenamedFile> = Vec::new();
        for f in &params.files {
            let (Ok(old_uri), Ok(new_uri)) = (Url::parse(&f.old_uri), Url::parse(&f.new_uri))
            else {
                continue;
            };
            if classify_watch_uri(&old_uri) != WatchTarget::Nv {
                continue;
            }
            // Old text: prefer the open buffer overlay, else disk.
            let old_text = if let Some(doc) = self.state.docs.get(&old_uri) {
                doc.text.to_string()
            } else if let Ok(path) = old_uri.to_file_path() {
                match std::fs::read_to_string(&path) {
                    Ok(t) => t,
                    Err(_) => String::new(),
                }
            } else {
                String::new()
            };
            renames.push(RenamedFile { old_uri, new_uri, old_text });
        }
        if renames.is_empty() {
            return Ok(None);
        }

        // Candidate importers: all open documents + all workspace .nv files.
        let files = self.collect_workspace_files();

        let edit = run_with_large_stack(move || compute_rename_import_edits(&renames, &files));
        Ok(edit)
    }

    /// `workspace/didRenameFiles` — the rename has now happened. Purge the old
    /// URI from every cache and index the file at its new URI, then invalidate
    /// dependent resolved builds and refresh. Complements `willRenameFiles`
    /// (which only produced the import-path edit).
    async fn did_rename_files(&self, params: RenameFilesParams) {
        let mut any_nv = false;
        for f in &params.files {
            let (Ok(old_uri), Ok(new_uri)) = (Url::parse(&f.old_uri), Url::parse(&f.new_uri))
            else {
                continue;
            };
            if classify_watch_uri(&old_uri) != WatchTarget::Nv {
                continue;
            }
            any_nv = true;
            // Old location is gone: evict its caches/index entries.
            self.state.workspace_index.remove_file(&old_uri);
            self.state.references_index.remove_file(&old_uri);
            self.state.invalidate_resolved(&old_uri);
            self.state.document_symbol_cache.invalidate(&old_uri);
            // Index the file at its new location from disk (if present).
            if let Ok(path) = new_uri.to_file_path() {
                if let Ok(src) = std::fs::read_to_string(&path) {
                    self.state.workspace_index.index_file(new_uri.clone(), &src);
                    self.state.references_index.index_file(new_uri.clone(), &src);
                }
            }
        }
        if any_nv {
            self.state.invalidate_all_resolved();
            self.recheck_open_documents().await;
            self.refresh_client_hints().await;
        }
    }

    // ── Plan 104.6: Rename + Format-on-save ─────────────────────────────────

    /// `textDocument/prepareRename` — validate that the cursor is on a
    /// renameable identifier and return the current word span.
    ///
    /// Returns an error if the cursor is on a keyword, comment, string literal,
    /// or whitespace.
    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri.clone();
        let pos = params.position;

        let Some(doc) = self.state.docs.get(&uri) else {
            return Err(tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
                message: "document not open".into(),
                data: None,
            });
        };
        let text = doc.text.to_string();
        drop(doc);

        let response = run_with_large_stack(move || prepare_rename(&text, pos))?;
        Ok(Some(response))
    }

    /// `textDocument/rename` — cross-file rename with atomic post-check.
    ///
    /// Returns a `WorkspaceEdit` with `documentChanges` if rename is valid.
    /// Returns an error if the new name is invalid or post-rename type-check
    /// introduces errors (D296 atomic rename contract).
    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let new_name = params.new_name.clone();

        // Collect all open documents.
        let mut docs: Vec<RenameDoc> = Vec::new();
        for entry in self.state.docs.iter() {
            docs.push(RenameDoc {
                uri: entry.key().clone(),
                text: entry.value().text.to_string(),
                version: Some(entry.value().version),
            });
        }

        // Also collect workspace files not currently open.
        if let Some(root) = self.state.workspace_root() {
            let open_uris: std::collections::HashSet<_> =
                self.state.docs.iter().map(|e| e.key().clone()).collect();

            if let Ok(nv_files) = collect_nv_files_for_rename(&root) {
                for path in nv_files {
                    if let Some(file_uri) = tower_lsp::lsp_types::Url::from_file_path(&path).ok() {
                        if !open_uris.contains(&file_uri) {
                            if let Ok(text) = std::fs::read_to_string(&path) {
                                docs.push(RenameDoc {
                                    uri: file_uri,
                                    text,
                                    version: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Find old_name + cursor byte offset in the primary doc. The byte offset
        // (word start) is fed to `compute_rename` so it can classify the symbol's
        // scope from the AST (Plan 104.10 Ф.7), not just the bare word text.
        let (old_name, cursor_byte) = {
            let Some(doc) = self.state.docs.get(&uri) else {
                return Err(tower_lsp::jsonrpc::Error {
                    code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
                    message: "document not open".into(),
                    data: None,
                });
            };
            let text = doc.text.to_string();
            drop(doc);
            let pos = params.text_document_position.position;
            let line_starts = crate::rename::compute_line_starts(&text);
            let line_idx = pos.line as usize;
            let line_start = line_starts.get(line_idx).copied().unwrap_or(0);
            // Convert UTF-16 col to byte offset (simplified: ASCII lines).
            let col_byte = pos.character as usize;
            let byte_off = line_start + col_byte;
            let (ws, we) = crate::rename::word_at(&text, byte_off);
            if ws == we {
                return Err(tower_lsp::jsonrpc::Error {
                    code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
                    message: "cursor is not on an identifier".into(),
                    data: None,
                });
            }
            (text[ws..we].to_string(), ws)
        };

        tracing::info!(old_name = %old_name, new_name = %new_name, "rename requested");

        let primary_uri = uri.clone();
        let edit = run_with_large_stack(move || {
            compute_rename(&docs, &primary_uri, cursor_byte, &old_name, &new_name)
        })?;
        Ok(Some(edit))
    }

    /// `textDocument/formatting` — invoke `nova fmt` and return text edits.
    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text.to_string();
        drop(doc);

        let edits = run_with_large_stack(move || format_document(&text, None));
        Ok(Some(edits))
    }

    /// `textDocument/rangeFormatting` — format only a range.
    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri.clone();
        let range = params.range;
        let Some(doc) = self.state.docs.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text.to_string();
        drop(doc);

        let edits = run_with_large_stack(move || format_range(&text, range, None));
        Ok(Some(edits))
    }

    /// `textDocument/onTypeFormatting` — auto-indent/close on trigger characters.
    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        let trigger = params.ch.clone();

        let Some(doc) = self.state.docs.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text.to_string();
        drop(doc);

        let edits = run_with_large_stack(move || on_type_format(&text, pos, &trigger));
        Ok(Some(edits))
    }

    /// Plan 104.3: textDocument/completion handler.
    ///
    /// Computes context-aware completions for the cursor position:
    /// - Keyword + snippet completions (context: top-level / fn-body / type-body)
    /// - In-scope identifier completions (scope walk)
    /// - Method-dot completions (type-driven after `.`)
    /// - Import path completions (`import std.*`)
    ///
    /// Performance: ≤200ms target (runs in run_with_large_stack).
    /// Cancellation: if document changed since request, returns empty (LSP re-requests).
    async fn completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;

        // Get document text.
        let src = match self.state.docs.get(&uri) {
            Some(doc) => doc.text.to_string(),
            None => {
                tracing::warn!(uri = %uri, "completion: document not in cache");
                return Ok(None);
            }
        };

        // Convert LSP position (line, character UTF-16) to byte offset.
        let offset = match lsp_position_to_byte_offset(&src, pos) {
            Some(o) => o,
            None => {
                tracing::warn!(uri = %uri, pos = ?pos, "completion: position out of range");
                return Ok(None);
            }
        };

        tracing::debug!(uri = %uri, line = pos.line, char = pos.character, offset, "completion request");

        // Plan 104.10 Ф.5: type-driven method completion + FS-sourced import
        // completion. Resolve the document's on-disk path (enables receiver-type
        // inference) and the workspace stdlib index (enables import completion).
        let doc_path = uri.to_file_path().ok();
        let stdlib = doc_path.as_deref().and_then(|p| self.state.stdlib_index(p));

        // Run completion synchronously in large-stack thread (compiler API uses recursion).
        let src_clone = src.clone();
        let items = run_with_large_stack(move || match doc_path.as_deref() {
            Some(path) => {
                completion::completion_for_doc(path, &src_clone, offset, stdlib.as_deref())
            }
            None => completion::completion_for(&src_clone, offset),
        });

        if items.is_empty() {
            return Ok(None);
        }

        tracing::debug!(uri = %uri, count = items.len(), "completion: returning items");
        Ok(Some(CompletionResponse::Array(items)))
    }

    /// `completionItem/resolve` — Plan 104.10 Ф.13.
    ///
    /// The initial completion list is kept lightweight: keyword/snippet/prelude
    /// items ship without `detail`/`documentation`, and method/import items ship
    /// without `documentation`. When the client focuses an item it sends it back
    /// here; [`completion::resolve_completion_item`] re-derives the heavy fields
    /// from the `data` descriptor. Pure and cheap (no recompile, no I/O), so it
    /// runs directly. An item lacking a recognised `data` payload is returned
    /// unchanged (graceful).
    async fn completion_resolve(&self, params: CompletionItem) -> Result<CompletionItem> {
        tracing::debug!(label = %params.label, "completionItem/resolve request");
        Ok(completion::resolve_completion_item(params))
    }

    // ── Plan 104.4: symbols + references ─────────────────────────────────────

    /// `textDocument/documentSymbol` — outline for VSCode sidebar.
    ///
    /// Returns a hierarchical `DocumentSymbol` list: functions, types (with
    /// nested fields/variants/protocol-methods), tests, consts, lets.
    /// Methods are nested under their receiver type when declared in the same
    /// file.  Falls back to empty list on parse failure (graceful).
    ///
    /// Cache per-URI, invalidated on `didChange`/`didOpen`.
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.clone();

        // Check cache first.
        if let Some(cached) = self.state.document_symbol_cache.get(&uri) {
            let list = (*cached).clone();
            return Ok(if list.is_empty() {
                None
            } else {
                Some(DocumentSymbolResponse::Nested(list))
            });
        }

        // Compute symbols (parse only — no typecheck, ≤50ms).
        let src = match self.state.docs.get(&uri) {
            Some(doc) => doc.text.to_string(),
            None => return Ok(None),
        };

        let symbols = run_with_large_stack(move || compute_document_symbols(&src));

        // Populate cache.
        self.state.document_symbol_cache.insert(uri, symbols.clone());

        Ok(if symbols.is_empty() {
            None
        } else {
            Some(DocumentSymbolResponse::Nested(symbols))
        })
    }

    /// `workspace/symbol` — Ctrl+T project-wide symbol search.
    ///
    /// Substring + case-insensitive matching against the workspace index.
    /// Returns at most 100 results.  Empty query returns up to 100 symbols
    /// from any file.
    ///
    /// Index is built incrementally on `didOpen`/`didChange`; initial scan of
    /// the workspace root happens at first `documentSymbol` request or when the
    /// index is empty.
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = &params.query;
        const LIMIT: usize = 100;

        // Guard: very long queries never match anything useful.
        if query.len() > 1000 {
            return Ok(None);
        }

        // If workspace index is empty and we have a root, do a one-shot scan.
        if self.state.workspace_index.file_count() == 0 {
            if let Some(root) = self.state.workspace_root() {
                let files = collect_nv_files(&root);
                for (uri, src) in &files {
                    self.state.workspace_index.index_file(uri.clone(), src);
                }
            }
        }

        let entries = self.state.workspace_index.search(query, LIMIT);
        let symbols = entries_to_workspace_symbols(entries);

        Ok(if symbols.is_empty() { None } else { Some(symbols) })
    }

    /// `textDocument/references` — Shift+F12 find all usages.
    ///
    /// Plan 104.10 Ф.12: answered from the **incremental references index**
    /// (`name → [(uri, span)]`) instead of the V1 per-request full-filesystem
    /// scan. The index is kept current on `didOpen`/`didChange` (open-buffer
    /// overlay), external watched-file events, and rename; the whole workspace
    /// is cold-scanned once in the background (`initialized`). `includeDeclaration`
    /// is honoured by dropping the occurrence overlapping the declaration span.
    async fn references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let position = params.text_document_position.position;
        let include_decl = params.context.include_declaration;

        // Get text for the file under cursor.
        let src = match self.state.docs.get(&uri) {
            Some(doc) => doc.text.to_string(),
            None => return Ok(None),
        };

        // Extract identifier at position.
        let symbol_name = match symbol_at_position(&src, position) {
            Some(name) => name,
            None => return Ok(None),
        };

        // Lazy cold-prime: if the background `initialized` scan has not run
        // (client sent no `initialized`, or none-workspace-root edge), fill the
        // index from disk ONCE. Open documents are already indexed with their
        // buffer text, so we skip them to avoid clobbering unsaved edits with a
        // stale disk read. After this, all requests answer from memory.
        if !self.state.references_index.is_primed() {
            if let Some(root) = self.state.workspace_root() {
                for (file_uri, text) in collect_nv_files(&root) {
                    if !self.state.docs.contains_key(&file_uri) {
                        self.state.references_index.index_file(file_uri, &text);
                    }
                }
            }
            self.state.references_index.mark_primed();
        }

        tracing::debug!(symbol = %symbol_name, "references: querying incremental index");

        // Declaration location (first occurrence in the cursor file) — only
        // needed to honour `includeDeclaration = false`.
        let declaration_loc = if include_decl {
            None
        } else {
            let decl_src = src.clone();
            let decl_uri = uri.clone();
            let decl_name = symbol_name.clone();
            run_with_large_stack(move || find_decl_location(&decl_uri, &decl_src, &decl_name))
        };

        let locs = self.state.references_index.find(
            &symbol_name,
            declaration_loc.as_ref(),
            include_decl,
        );

        tracing::debug!(count = locs.len(), symbol = %symbol_name, "references: found");

        Ok(if locs.is_empty() { None } else { Some(locs) })
    }

    /// `textDocument/documentHighlight` — Plan 104.10 Ф.15.
    ///
    /// Highlights every occurrence of the symbol under the cursor **in the
    /// current file**, tagged read/write. The occurrence set is scoped by the
    /// same AST resolver the Ф.7 rename uses (`resolve_highlight_scope`), so a
    /// same-named local in a sibling function is never highlighted — semantic
    /// resolution, not a word regex.
    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri.clone();
        let pos = params.text_document_position_params.position;
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        drop(doc);

        // Contained so a parser panic degrades to no highlights, never a crash.
        let highlights = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || compute_document_highlights(&src, pos))
        })) {
            Ok(h) => h,
            Err(_) => None,
        };
        Ok(highlights)
    }

    /// Plan 104.10 Ф.16: `textDocument/foldingRange` — syntactic folding regions
    /// derived from AST node spans (fn/type bodies, nested `{ }` blocks, import
    /// groups, multi-line doc-comments). Parse-only; no type-check.
    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        drop(doc);

        // Contained so a parser panic degrades to no folds, never a crash.
        let ranges = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || crate::folding_range::compute_folding_ranges(&src))
        })) {
            Ok(r) => r,
            Err(_) => Vec::new(),
        };
        Ok(Some(ranges))
    }

    /// Plan 104.10 Ф.17: `textDocument/selectionRange` — smart-expand.
    ///
    /// For each requested position returns a chain of expanding ranges derived
    /// from the AST hierarchy (identifier → enclosing expression → statement →
    /// block → declaration); each `parent` is a strict superset of its child.
    /// Parse-only; no type-check. A position outside code (or a parse failure)
    /// degrades to a minimal empty range at the cursor.
    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        drop(doc);
        let positions = params.positions;

        // Contained so a parser panic degrades to minimal ranges, never a crash.
        let ranges = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || {
                crate::selection_range::compute_selection_ranges(&src, &positions)
            })
        })) {
            Ok(r) => r,
            Err(_) => Vec::new(),
        };
        Ok(Some(ranges))
    }

    /// Plan 104.10 Ф.9: `textDocument/inlayHint` — type hints for un-annotated
    /// `ro x = expr` bindings (`: T` from the Ф.2 `expr_types` map) and
    /// parameter-name hints before call arguments (`a:`/`b:` from the resolved
    /// callee). Uses the Ф.1 resolved-module cache (build once per doc version).
    /// Both kinds are individually toggleable via config (default on); a disabled
    /// kind is skipped in the compute. A parser/checker panic degrades to no
    /// hints, never a crash.
    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.clone();
        let range = params.range;
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        let version = doc.version;
        drop(doc);

        let cfg = self.state.inlay_config();
        if !cfg.type_hints && !cfg.parameter_hints {
            return Ok(Some(Vec::new()));
        }

        let state = Arc::clone(&self.state);
        let uri_clone = uri.clone();
        let hints = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || {
                let resolved = state.get_or_build_resolved(&uri_clone, version, &src);
                crate::inlay_hints::compute_inlay_hints_in(&resolved, &src, range, cfg)
            })
        })) {
            Ok(h) => h,
            Err(_) => Vec::new(),
        };
        Ok(Some(hints))
    }

    /// Plan 104.10 Ф.9: `workspace/didChangeConfiguration` — re-read the inlay-hint
    /// toggles and push an `inlayHint/refresh` so already-rendered hints re-pull
    /// with the new setting (no edit required).
    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        let cfg = crate::inlay_hints::InlayHintConfig::from_settings(&params.settings);
        self.state.set_inlay_config(cfg);
        tracing::info!(?cfg, "inlay-hint config updated via didChangeConfiguration");
        // Ask the client to re-request inlay hints for visible editors.
        let _ = self.client.inlay_hint_refresh().await;
    }

    /// Plan 104.5: code_action — ≥25 quick-fix providers.
    ///
    /// Dispatches to `compute_code_actions` (code_actions.rs) for all
    /// diagnostics in `params.context.diagnostics`.  Supports:
    /// - Plan 101 generic errors (8 fixes)
    /// - Plan 100 consume/mutability errors (7 fixes)
    /// - General fixes: protocol-embed, kw-removed, extension imports (7 fixes)
    /// - Auto-import suggestions (3 fixes)
    ///
    /// Also includes Plan 123.5.3 diagnostic-independent actions (pure annotation).
    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let mut actions: Vec<CodeActionOrCommand> = Vec::new();

        // ── Plan 123.5.3: #pure annotation suggestions (diagnostic-independent) ─
        if let Some(doc) = self.state.docs.get(&uri) {
            let src = doc.text.to_string();
            drop(doc);
            let range = params.range;
            let pure_actions = run_with_large_stack(move ||
                compute_pure_annotation_actions(&src, range)
            );
            if let Some(edits) = pure_actions {
                for (insert_range, label) in edits {
                    let mut changes = std::collections::HashMap::new();
                    changes.insert(
                        uri.clone(),
                        vec![TextEdit {
                            range: insert_range,
                            new_text: "#pure\n".to_string(),
                        }],
                    );
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: label,
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: None,
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            document_changes: None,
                            change_annotations: None,
                        }),
                        command: None,
                        is_preferred: Some(false),
                        disabled: None,
                        data: None,
                    }));
                }
            }
        }

        // ── Plan 104.10 Ф.11: source.organizeImports (diagnostic-independent) ──
        // Offer only when the client's `only` filter admits the
        // `source.organizeImports` kind (hierarchical prefix match).
        if code_action_only_admits(
            params.context.only.as_deref(),
            &CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
        ) {
            if let Some(doc) = self.state.docs.get(&uri) {
                let src = doc.text.to_string();
                drop(doc);
                let uri_clone = uri.clone();
                let organize = run_with_large_stack(move || {
                    crate::organize_imports::compute_organize_imports(&uri_clone, &src)
                });
                if let Some(action) = organize {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }
        }

        // ── Plan 104.5: compute_code_actions for all diagnostics ──────────────
        if !params.context.diagnostics.is_empty() {
            let src = self.state.docs.get(&uri).map(|d| d.text.to_string())
                .unwrap_or_default();
            let rope = ropey::Rope::from_str(&src);
            // Plan 104.10 Ф.5: resolve add-import quick-fixes from the workspace
            // stdlib search-path index (no hardcoded type/protocol → module).
            let stdlib = uri
                .to_file_path()
                .ok()
                .and_then(|p| self.state.stdlib_index(&p));
            let ca = compute_code_actions_with_stdlib(
                &uri,
                &src,
                &rope,
                &params.context.diagnostics,
                stdlib.as_deref(),
            );
            actions.extend(ca);
        }

        Ok(if actions.is_empty() { None } else { Some(actions) })
    }

    /// Plan 104.10 Ф.20 + Plan 123.5.1: code lenses.
    ///
    /// Combines the Ф.20 navigation lenses — `▶ Run test` over `test` blocks,
    /// `N references` over every `fn`/`type` (Ф.12 index), `N implementations`
    /// over every `protocol` (Ф.19 scan) — with the Plan 123.5.1 field-cache
    /// lens ("N cache(s)" over method headers).
    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        let version = doc.version;
        drop(doc);

        // Ф.20: prime the references index once on a cold workspace so cross-file
        // reference counts are correct (mirrors the `references` handler). Open
        // documents are already indexed with their buffer text, so skip them.
        if !self.state.references_index.is_primed() {
            if let Some(root) = self.state.workspace_root() {
                for (file_uri, text) in collect_nv_files(&root) {
                    if !self.state.docs.contains_key(&file_uri) {
                        self.state.references_index.index_file(file_uri, &text);
                    }
                }
            }
            self.state.references_index.mark_primed();
        }

        // Ф.20 navigation lenses — computed off the Ф.1 resolved cache (imports
        // inlined, so cross-file implementers resolve) + the Ф.12 references
        // index. Contained so a resolver/scan panic degrades to just the
        // field-cache lenses rather than failing the request.
        let file_path = uri.to_file_path().ok();
        let state = Arc::clone(&self.state);
        let uri_nav = uri.clone();
        let src_nav = src.clone();
        let mut lenses: Vec<CodeLens> =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_with_large_stack(move || {
                    let resolved = state.get_or_build_resolved(&uri_nav, version, &src_nav);
                    crate::code_lens::compute_navigation_lenses(
                        &src_nav,
                        &uri_nav,
                        file_path.as_deref(),
                        &resolved,
                        &state.references_index,
                    )
                })
            })) {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!("code_lens: navigation-lens computation panicked");
                    Vec::new()
                }
            };

        // Plan 123.5.1: append the field-cache lenses (best-effort; skipped when
        // the file does not type-check).
        if let Some(fc) = run_with_large_stack(move || compute_field_cache_lenses(&src)) {
            lenses.extend(fc);
        }

        Ok(if lenses.is_empty() { None } else { Some(lenses) })
    }

    /// Plan 104.10 Ф.20: `workspace/executeCommand`.
    ///
    /// Currently the enabler for the run-test lens: `nova.runTest` receives
    /// `[file_path, test_name]` and shells out to `nova test <file> --filter
    /// <name>`, reporting the outcome to the user via `window/showMessage` (full
    /// output goes to the trace log). Unknown commands degrade to `Ok(None)`.
    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            crate::code_lens::CMD_RUN_TEST => {
                let file = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let test = params
                    .arguments
                    .get(1)
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let Some(file) = file else {
                    self.client
                        .show_message(
                            MessageType::ERROR,
                            "nova.runTest: missing file-path argument",
                        )
                        .await;
                    return Ok(None);
                };
                self.run_nova_test(&file, test.as_deref()).await;
                Ok(None)
            }
            other => {
                tracing::warn!(command = %other, "executeCommand: unknown command");
                Ok(None)
            }
        }
    }

    /// Plan 104.2: hover handler — symbol type + doc-comment.
    ///
    /// Priority:
    /// 1. Symbol hover (Plan 104.2): resolves fn/type/var/import and renders
    ///    type + doc-comment in a `nova` fenced code block.
    /// 2. Field-cache hover (Plan 123.5.1): for `@field` accesses that the
    ///    field_cache analyzer would cache — shows cache classification.
    ///
    /// If neither returns a result, returns `Ok(None)`.
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        let version = doc.version;
        drop(doc);

        // Plan 104.2 / 104.10 Ф.4: primary symbol hover, resolved cross-file via
        // the Ф.1 resolved-module cache (built once per doc version, shared with
        // goto/completion). A foreign-file symbol shows the real signature+doc
        // from its source declaration plus a source-path footer.
        let src2 = src.clone();
        let uri2 = uri.clone();
        let state = Arc::clone(&self.state);
        let symbol_hover = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || {
                let resolved = state.get_or_build_resolved(&uri2, version, &src2);
                compute_hover_in(&resolved, &src2, pos, &uri2)
            })
        })) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("hover panicked: {:?}", e.downcast_ref::<&str>().unwrap_or(&"?"));
                None
            }
        };
        if symbol_hover.is_some() {
            return Ok(symbol_hover);
        }

        // Plan 123.5.1 fallback: field-cache hover for @field accesses.
        let field_hover = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || compute_field_cache_hover(&src, pos))
        })) {
            Ok(h) => h,
            Err(_) => None,
        };
        Ok(field_hover)
    }

    /// Plan 104.10 Ф.3: cross-file goto-definition.
    ///
    /// Resolves the symbol under the cursor and returns the [`Location`] of its
    /// declaration in the *right* file — the declaration `Span`'s `file_id` is
    /// mapped back to its source file via the resolved module's provenance
    /// `file_map` (built from real `peer_files`, never a textual re-scan). The
    /// target range is computed in that file's own UTF-16 coordinates: from the
    /// in-memory buffer for the current document, and from disk for a peer file
    /// (whose spans were parsed from disk during import inlining).
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        let version = doc.version;
        drop(doc);

        let state = Arc::clone(&self.state);
        let uri_clone = uri.clone();
        let location = run_with_large_stack(move || {
            // Reuse the Ф.1 resolved-module cache (build once per doc version).
            let resolved = state.get_or_build_resolved(&uri_clone, version, &src);
            compute_goto_definition_in(&resolved, &src, pos, &uri_clone)
        });
        Ok(location.map(GotoDefinitionResponse::Scalar))
    }

    /// Plan 104.10 Ф.19: `textDocument/typeDefinition` — jump to the declaration
    /// of the *type* of the expression under the cursor (via Ф.2 `expr_types`).
    async fn goto_type_definition(
        &self,
        params: request::GotoTypeDefinitionParams,
    ) -> Result<Option<request::GotoTypeDefinitionResponse>> {
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        let version = doc.version;
        drop(doc);

        let state = Arc::clone(&self.state);
        let uri_clone = uri.clone();
        let location = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || {
                let resolved = state.get_or_build_resolved(&uri_clone, version, &src);
                crate::type_definition::compute_type_definition_in(&resolved, &src, pos, &uri_clone)
            })
        })) {
            Ok(l) => l,
            Err(_) => None,
        };
        Ok(location.map(GotoDefinitionResponse::Scalar))
    }

    /// Plan 104.10 Ф.19: `textDocument/implementation` — for a protocol under the
    /// cursor, all implementing types; for a method, all its implementations.
    /// Driven by the AST `#impl` / method registry (not a hardcoded table).
    async fn goto_implementation(
        &self,
        params: request::GotoImplementationParams,
    ) -> Result<Option<request::GotoImplementationResponse>> {
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        let version = doc.version;
        drop(doc);

        let state = Arc::clone(&self.state);
        let uri_clone = uri.clone();
        let locations = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || {
                let resolved = state.get_or_build_resolved(&uri_clone, version, &src);
                crate::type_definition::compute_implementation_in(&resolved, &src, pos, &uri_clone)
            })
        })) {
            Ok(l) => l,
            Err(_) => None,
        };
        Ok(locations.map(GotoDefinitionResponse::Array))
    }

    /// Plan 104.2: signature-help handler.
    ///
    /// Triggered by `(` and `,` (per server capabilities).
    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> Result<Option<SignatureHelp>> {
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        let version = doc.version;
        drop(doc);

        // Plan 104.10 Ф.8: dispatch the active overload by the receiver's real
        // type via the Ф.1 resolved-module cache (imports inlined) + Ф.2
        // `expr_types`. Contained so a checker/resolver panic degrades to no
        // signature rather than crashing the request.
        let state = Arc::clone(&self.state);
        let uri_clone = uri.clone();
        let help = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || {
                let resolved = state.get_or_build_resolved(&uri_clone, version, &src);
                compute_signature_help_in(&resolved, &src, pos)
            })
        })) {
            Ok(h) => h,
            Err(_) => None,
        };
        Ok(help)
    }

    /// Plan 123.5.2 (V5.2, 2026-06-02): semantic tokens for cached
    /// `@<field>` reads. Highlight only the reads the analyzer would
    /// fold into a cache local at codegen — gives the developer a
    /// visual signal that an optimization is being applied.
    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        let version = doc.version;
        drop(doc);

        // Plan 104.10 Ф.10: full semantic pass (keywords, literals, comments,
        // and per-identifier classification) driven off the Ф.1 resolved cache.
        let state = Arc::clone(&self.state);
        let uri_clone = uri.clone();
        let data = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || {
                let resolved = state.get_or_build_resolved(&uri_clone, version, &src);
                crate::semantic_tokens::compute_semantic_tokens(&src, &resolved)
            })
        })) {
            Ok(d) => d,
            Err(_) => Vec::new(),
        };
        // Plan 123.5.5 (V5.5): cache snapshot и assign monotonic `result_id`
        // для последующих delta requests от клиента.
        let result_id = self.state.next_semantic_tokens_result_id();
        self.state.semantic_tokens_cache.insert(
            uri.clone(),
            SemanticTokensSnapshot {
                result_id: result_id.clone(),
                tokens: data.clone(),
            },
        );
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: Some(result_id),
            data,
        })))
    }

    /// Plan 123.5.5 (V5.5, 2026-06-03): incremental semantic-tokens delta.
    /// Client passes back the `previous_result_id` it last received; if it
    /// matches our cached snapshot, we compute a minimal edit script via
    /// `compute_semantic_token_edits`; otherwise we fallback к a full
    /// re-response (per LSP spec — server is free to return either variant).
    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        let uri = params.text_document.uri.clone();
        let prev_result_id = params.previous_result_id;
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        let version = doc.version;
        drop(doc);

        // Plan 104.10 Ф.10: recompute the full token set, then diff against the
        // cached snapshot to produce a minimal edit script (Plan 123.5.5).
        let state = Arc::clone(&self.state);
        let uri_clone = uri.clone();
        let new_tokens = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_large_stack(move || {
                let resolved = state.get_or_build_resolved(&uri_clone, version, &src);
                crate::semantic_tokens::compute_semantic_tokens(&src, &resolved)
            })
        })) {
            Ok(t) => t,
            Err(_) => Vec::new(),
        };

        // Look up the cached snapshot. If `previous_result_id` matches,
        // compute delta; otherwise fallback к polite full response — the
        // client will re-sync через the returned snapshot.
        let cached = self.state.semantic_tokens_cache.get(&uri)
            .map(|r| r.value().clone());
        let new_result_id = self.state.next_semantic_tokens_result_id();
        let (response, updated_snap) = build_delta_response(
            cached.as_ref(),
            &prev_result_id,
            new_tokens,
            new_result_id,
        );
        self.state.semantic_tokens_cache.insert(uri, updated_snap);
        Ok(Some(response))
    }
}

/// Plan 104.10 Ф.11: does the client's `only` code-action filter admit `kind`?
///
/// Per the LSP spec, `CodeActionContext.only` matching is **hierarchical by
/// prefix**: a requested kind `"source"` admits `"source.organizeImports"`, and
/// an exact `"source.organizeImports"` admits it too. `None` (no filter) admits
/// everything. An empty list admits nothing.
fn code_action_only_admits(only: Option<&[CodeActionKind]>, kind: &CodeActionKind) -> bool {
    match only {
        None => true,
        Some(kinds) => kinds.iter().any(|requested| {
            let req = requested.as_str();
            let k = kind.as_str();
            k == req || k.starts_with(&format!("{req}."))
        }),
    }
}

/// Plan 104.10 Ф.18: file-operation registration options matching every `.nv`
/// file — used for the `willRenameFiles` / `didRenameFiles` server capability so
/// the client only asks us about Nova-source renames.
fn nv_file_operation_registration() -> FileOperationRegistrationOptions {
    FileOperationRegistrationOptions {
        filters: vec![FileOperationFilter {
            scheme: Some("file".to_string()),
            pattern: FileOperationPattern {
                glob: "**/*.nv".to_string(),
                matches: Some(FileOperationPatternKind::File),
                options: None,
            },
        }],
    }
}

/// Plan 123.5.2 (V5.2): semantic token legend — token types this
/// server emits. Single-element vec: standard LSP `property` type.
/// Public so unit tests can verify the legend stays stable across
/// edits.
pub fn cached_field_semantic_token_types() -> Vec<SemanticTokenType> {
    vec![SemanticTokenType::PROPERTY]
}

/// Plan 123.5.2 (V5.2): semantic token modifier legend. Indices
/// emitted in tokens are bit positions in this list — must match
/// the order returned to the client at initialize-time.
pub fn cached_field_semantic_token_modifiers() -> Vec<SemanticTokenModifier> {
    // Standard "readonly" approximates cached-folded semantics for
    // editors that map LSP modifiers to TextMate scopes без custom
    // theme support.  Custom modifier "cached" added for clients that
    // do honor non-standard modifiers (VS Code, Helix).
    vec![
        SemanticTokenModifier::READONLY,
        SemanticTokenModifier::new("cached"),
    ]
}

/// Plan 123.5.2 (V5.2): bit position of the "cached" modifier in the
/// legend returned by `cached_field_semantic_token_modifiers`.
const CACHED_MOD_BIT: u32 = (1 << 0) | (1 << 1); // readonly + cached

/// Plan 123.5.2 (V5.2): compute LSP-encoded semantic tokens for every
/// `@<field>` read in `src` that field_cache analysis says would be
/// CSE'd / cached. Delta-encoded per LSP spec.
///
/// Returns `None` when parsing/type-check fails (silent fallback —
/// editor keeps existing syntax highlighting without inflicting
/// errors).
pub fn compute_field_cache_semantic_tokens(src: &str) -> Option<Vec<SemanticToken>> {
    let mut module = nova_codegen::parser::parse(src).ok()?;
    // Plan 181 (D347): alpha-rename before the pipeline so field-cache analysis
    // sees the same unique-named AST as the real build. No-op without a rebind.
    nova_codegen::alpha_rename::alpha_rename(&mut module);
    if nova_codegen::types::check_module(&module).is_err() { return None; }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module, &std::collections::HashMap::new());
    // Plan 184 (Р7): IDE-analysis path — no binary is produced, so the
    // value-root guard is irrelevant here; the empty map preserves the
    // pre-184 hoisting shape the field-cache report inspects.
    nova_codegen::chain_norm::normalize_chains_module(
        &mut module, &std::collections::HashMap::new());
    let cfg = nova_codegen::field_cache::FieldCacheConfig::from_env_or_default();
    let report = nova_codegen::field_cache::analyze_module(&module, &cfg);

    // For each FnCacheInfo, build set of "cached" field names; then
    // scan src for `@<name>` reads within fn span and emit tokens.
    use std::collections::HashMap as Map;
    let mut cached_per_fn: Vec<(usize, usize, std::collections::HashSet<String>)> = Vec::new();
    for info in &report.per_fn {
        let mut set: std::collections::HashSet<String> = Default::default();
        for f in &info.ro_caches { set.insert(f.clone()); }
        for f in &info.mut_caches { set.insert(f.clone()); }
        for f in &info.licm_hoists { set.insert(f.clone()); }
        // chain_caches store path components — take the root.
        for p in &info.chain_caches {
            if let Some(root) = p.first() { set.insert(root.clone()); }
        }
        cached_per_fn.push((info.span.start as usize, info.span.end as usize, set));
    }
    if cached_per_fn.is_empty() { return Some(Vec::new()); }

    // Build a line-offset table once to convert byte offsets into LSP
    // (line, character) coordinates.
    let line_starts = compute_line_starts(src);

    let mut raw: Vec<(u32, u32, u32)> = Vec::new(); // (line, char, length)
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut prev_offset_to_fn: Map<usize, usize> = Map::new();
    while i < bytes.len() {
        if bytes[i] == b'@' && i + 1 < bytes.len()
            && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b'_')
        {
            // Extract field name.
            let mut j = i + 1;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
            {
                j += 1;
            }
            let name = match std::str::from_utf8(&bytes[i + 1..j]) {
                Ok(s) => s.to_string(),
                Err(_) => { i = j; continue; }
            };
            // Locate enclosing fn whose span covers `i`, AND which has
            // `name` in cached set.
            for (start, end, set) in &cached_per_fn {
                if i >= *start && i <= *end && set.contains(&name) {
                    let (line, col) = byte_to_line_col(&line_starts, i);
                    // Length covers `@` + name.
                    raw.push((line as u32, col as u32, (j - i) as u32));
                    prev_offset_to_fn.insert(i, *start);
                    break;
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    if raw.is_empty() { return Some(Vec::new()); }
    // Sort by (line, char) for deterministic delta encoding.
    raw.sort();
    // Delta-encode per LSP spec: each token's deltaLine/deltaStart are
    // relative to the previous emitted token.
    let mut out: Vec<SemanticToken> = Vec::with_capacity(raw.len());
    let mut prev_line: u32 = 0;
    let mut prev_char: u32 = 0;
    for (line, ch, len) in raw {
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 { ch - prev_char } else { ch };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length: len,
            token_type: 0,                 // index 0 = PROPERTY in legend.
            token_modifiers_bitset: CACHED_MOD_BIT, // readonly | cached
        });
        prev_line = line;
        prev_char = ch;
    }
    Some(out)
}

/// Plan 123.5.3 (V5.3): for every analytically-pure-but-unannotated
/// method whose decl span intersects `range`, return the insertion
/// site (zero-length Range at the line of `fn` keyword) and a human
/// label. Used by LSP code_action handler.
pub fn compute_pure_annotation_actions(
    src: &str,
    range: Range,
) -> Option<Vec<(Range, String)>> {
    let mut module = nova_codegen::parser::parse(src).ok()?;
    // Plan 181 (D347): alpha-rename before the pipeline so field-cache analysis
    // sees the same unique-named AST as the real build. No-op without a rebind.
    nova_codegen::alpha_rename::alpha_rename(&mut module);
    if nova_codegen::types::check_module(&module).is_err() { return None; }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module, &std::collections::HashMap::new());
    // Plan 184 (Р7): IDE-analysis path — no binary is produced, so the
    // value-root guard is irrelevant here; the empty map preserves the
    // pre-184 hoisting shape the field-cache report inspects.
    nova_codegen::chain_norm::normalize_chains_module(
        &mut module, &std::collections::HashMap::new());
    let candidates = nova_codegen::field_cache::pure_annotation_candidates(&module);
    if candidates.is_empty() { return Some(Vec::new()); }

    let line_starts = compute_line_starts(src);
    // Convert request range (LSP positions) → byte range.
    let req_start_byte = position_to_byte_offset_via_starts(src, &line_starts, range.start)?;
    let req_end_byte = position_to_byte_offset_via_starts(src, &line_starts, range.end)
        .unwrap_or(req_start_byte);

    let mut actions: Vec<(Range, String)> = Vec::new();
    let bytes = src.as_bytes();
    for (type_name, fn_name, span) in candidates {
        let s = span.start as usize;
        let e = span.end as usize;
        // Skip when invocation range outside this fn decl.
        if req_end_byte < s || req_start_byte > e { continue; }
        // Insertion point = line start of `fn` keyword. Walk back from
        // span.start to start of containing line. We insert `#pure\n`
        // at column 0 of that line; the editor preserves following
        // indent.
        let (line, _) = byte_to_line_col(&line_starts, s);
        let insert = Range {
            start: Position { line: line as u32, character: 0 },
            end: Position { line: line as u32, character: 0 },
        };
        let _ = bytes; // suppress unused; reserved for indent detection в V5.4.
        actions.push((
            insert,
            format!("Plan 123 V5.3: add `#pure` to {}.{}", type_name, fn_name),
        ));
    }
    Some(actions)
}

/// Convert an LSP position to byte offset given precomputed line-starts.
/// Treats character as byte-offset (V5.3 fixtures are pure ASCII).
fn position_to_byte_offset_via_starts(
    src: &str,
    line_starts: &[usize],
    pos: Position,
) -> Option<usize> {
    let line_idx = pos.line as usize;
    let line_start = *line_starts.get(line_idx)?;
    let next_line_start = line_starts.get(line_idx + 1).copied().unwrap_or(src.len());
    let target = line_start + pos.character as usize;
    if target > next_line_start { return Some(next_line_start); }
    Some(target.min(src.len()))
}

/// Compute byte offsets of each line start in `src`.
fn compute_line_starts(src: &str) -> Vec<usize> {
    let mut out = vec![0usize];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' { out.push(i + 1); }
    }
    out
}

/// Convert byte offset to (line, character-in-line) — both 0-indexed.
/// Character is byte-based; UTF-16 conversion happens at the LSP
/// boundary (V5.2 fixtures are pure ASCII so this is exact).
fn byte_to_line_col(line_starts: &[usize], byte: usize) -> (usize, usize) {
    let line = match line_starts.binary_search(&byte) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let line_start = line_starts.get(line).copied().unwrap_or(0);
    (line, byte - line_start)
}

/// Plan 104.10 Ф.20: locate the `nova` CLI binary for the run-test lens.
///
/// Resolution order: `NOVA_BIN` env override → a sibling of the running
/// `nova-lsp` executable (the normal `cargo build` layout puts `nova` and
/// `nova-lsp` in the same `target/<profile>/` dir) → bare `nova` (resolved via
/// `PATH` by the OS at spawn time).
fn nova_binary() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("NOVA_BIN") {
        if !p.is_empty() {
            return std::path::PathBuf::from(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let name = if cfg!(windows) { "nova.exe" } else { "nova" };
            let cand = dir.join(name);
            if cand.exists() {
                return cand;
            }
        }
    }
    std::path::PathBuf::from("nova")
}

/// Last non-blank line of `s`, trimmed — used to surface a `nova test` run's
/// summary/failure line in a `window/showMessage`. `None` if `s` is all blank.
fn last_meaningful_line(s: &str) -> Option<String> {
    s.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Plan 123.5.1: compute code-lens list для source text.
pub fn compute_field_cache_lenses(src: &str) -> Option<Vec<CodeLens>> {
    let mut module = nova_codegen::parser::parse(src).ok()?;
    // Best-effort pipeline (skip if type-check fails).
    // Plan 181 (D347): alpha-rename first so field-cache analysis sees the same
    // unique-named AST as the real build. No-op without a rebind.
    nova_codegen::alpha_rename::alpha_rename(&mut module);
    if nova_codegen::types::check_module(&module).is_err() { return None; }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module, &std::collections::HashMap::new());
    // Plan 184 (Р7): IDE-analysis path — no binary is produced, so the
    // value-root guard is irrelevant here; the empty map preserves the
    // pre-184 hoisting shape the field-cache report inspects.
    nova_codegen::chain_norm::normalize_chains_module(
        &mut module, &std::collections::HashMap::new());
    let cfg = nova_codegen::field_cache::FieldCacheConfig::from_env_or_default();
    let report = nova_codegen::field_cache::analyze_module(&module, &cfg);

    let mut lenses: Vec<CodeLens> = Vec::new();
    for info in &report.per_fn {
        // Map Span to LSP Range (line/col).
        let span = info.span;
        let (line, col) = span_to_line_col(src, span.start as usize);
        let range = Range {
            start: Position { line: line as u32, character: col as u32 },
            end: Position { line: line as u32, character: (col + 1) as u32 },
        };
        let total = info.total();
        let title = format!(
            "{} cache(s): ro={} mut={} licm={} pure={} chain={}",
            total,
            info.ro_caches.len(),
            info.mut_caches.len(),
            info.licm_hoists.len(),
            info.pure_caches.len(),
            info.chain_caches.len(),
        );
        lenses.push(CodeLens {
            range,
            command: Some(Command {
                title,
                command: "nova-lsp.fieldCache.show".to_string(),
                arguments: None,
            }),
            data: None,
        });
    }
    Some(lenses)
}

/// Plan 123.5.1: hover info над `@field` access.
pub fn compute_field_cache_hover(src: &str, pos: Position) -> Option<Hover> {
    // Compute byte-offset at pos.
    let byte_off = position_to_byte_offset(src, pos)?;
    // Find `@<name>` token at pos: look backward for `@`.
    let bytes = src.as_bytes();
    let mut at_start = byte_off;
    while at_start > 0 && bytes[at_start - 1].is_ascii_alphanumeric() {
        at_start -= 1;
    }
    if at_start == 0 || bytes[at_start - 1] != b'@' {
        return None;
    }
    let at_marker = at_start - 1;
    let mut name_end = byte_off;
    while name_end < bytes.len() && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_') {
        name_end += 1;
    }
    let field_name = std::str::from_utf8(&bytes[at_start..name_end]).ok()?.to_string();
    if field_name.is_empty() { return None; }

    // Parse module + analyze.
    let mut module = nova_codegen::parser::parse(src).ok()?;
    // Plan 181 (D347): alpha-rename before the pipeline so field-cache analysis
    // sees the same unique-named AST as the real build. No-op without a rebind.
    nova_codegen::alpha_rename::alpha_rename(&mut module);
    if nova_codegen::types::check_module(&module).is_err() { return None; }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module, &std::collections::HashMap::new());
    // Plan 184 (Р7): IDE-analysis path — no binary is produced, so the
    // value-root guard is irrelevant here; the empty map preserves the
    // pre-184 hoisting shape the field-cache report inspects.
    nova_codegen::chain_norm::normalize_chains_module(
        &mut module, &std::collections::HashMap::new());
    let cfg = nova_codegen::field_cache::FieldCacheConfig::from_env_or_default();
    let report = nova_codegen::field_cache::analyze_module(&module, &cfg);

    // Find any fn whose ro_caches OR mut_caches OR licm_hoists OR
    // chain_caches contain field_name AND whose span covers the hover
    // position.
    for info in &report.per_fn {
        let fn_start = info.span.start as usize;
        let fn_end = info.span.end as usize;
        if at_marker < fn_start || at_marker > fn_end { continue; }
        let cached_as = if info.ro_caches.iter().any(|f| f == &field_name) {
            Some(format!("D217 ro cache: `_at_{}`", field_name))
        } else if info.mut_caches.iter().any(|f| f == &field_name) {
            Some(format!("D217 mut first-region cache: `_at_{}`", field_name))
        } else if info.licm_hoists.iter().any(|f| f == &field_name) {
            Some(format!("D218 LICM loop hoist: `_at_{}_loop`", field_name))
        } else if info.chain_caches.iter().any(|p| p.first() == Some(&field_name)) {
            Some(format!("D217 V4 chain cache (root)"))
        } else {
            None
        };
        if let Some(info_str) = cached_as {
            return Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(format!(
                    "**Plan 123 field-cache (V1-V7):**\n\n@{} — {}",
                    field_name, info_str
                ))),
                range: None,
            });
        }
    }
    Some(Hover {
        contents: HoverContents::Scalar(MarkedString::String(format!(
            "**Plan 123 field-cache:**\n\n@{} — not cached (below threshold or excluded)",
            field_name
        ))),
        range: None,
    })
}

fn span_to_line_col(src: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut col = 0usize;
    for (i, c) in src.char_indices() {
        if i >= byte_offset { break; }
        if c == '\n' { line += 1; col = 0; } else { col += 1; }
    }
    (line, col)
}

fn position_to_byte_offset(src: &str, pos: Position) -> Option<usize> {
    let mut current_line = 0u32;
    let mut current_col = 0u32;
    for (i, c) in src.char_indices() {
        if current_line == pos.line && current_col == pos.character {
            return Some(i);
        }
        if c == '\n' {
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
    }
    if current_line == pos.line && current_col == pos.character {
        return Some(src.len());
    }
    None
}

// Plan 114 Ф.7.2 helpers superseded by Plan 104.5 compute_code_actions (code_actions.rs).

// ─────────────────────────────────────────────────────────────────────────────
// Plan 104.3 — Position conversion helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Convert an LSP Position (line, character in UTF-16 units) to a byte offset
/// in the UTF-8 source text.
///
/// LSP positions use UTF-16 code unit counts. For ASCII content (most Nova
/// source), UTF-16 character count == UTF-8 character count == byte count.
/// For multi-byte chars we walk line-by-line to find the exact byte offset.
fn lsp_position_to_byte_offset(src: &str, pos: Position) -> Option<usize> {
    let target_line = pos.line as usize;
    let target_char = pos.character as usize; // UTF-16 units

    // Find the byte offset of the start of target_line.
    let line_start = if target_line == 0 {
        0
    } else {
        let mut current_line = 0usize;
        let mut found = None;
        for (i, ch) in src.char_indices() {
            if ch == '\n' {
                current_line += 1;
                if current_line == target_line {
                    found = Some(i + 1);
                    break;
                }
            }
        }
        match found {
            Some(o) => o,
            None => return Some(src.len()),
        }
    };

    // Walk UTF-16 units along the target line.
    byte_offset_on_line(src, line_start, target_char)
}

/// Walk UTF-16 units from `line_start` byte offset by `utf16_col` units
/// and return the byte offset.
fn byte_offset_on_line(src: &str, line_start: usize, utf16_col: usize) -> Option<usize> {
    let rest = src.get(line_start..)?;
    let mut utf16_count = 0usize;
    let mut byte_pos = line_start;
    for ch in rest.chars() {
        if ch == '\n' {
            break;
        }
        if utf16_count >= utf16_col {
            return Some(byte_pos);
        }
        // Each char takes 1 or 2 UTF-16 units (surrogate pairs for > U+FFFF).
        utf16_count += if (ch as u32) > 0xFFFF { 2 } else { 1 };
        byte_pos += ch.len_utf8();
    }
    Some(byte_pos.min(src.len()))
}

/// Plan 104.4: find the declaration `Location` of `symbol_name` in `src`.
///
/// Uses the first word-boundary occurrence of the name as a heuristic.
/// This is sufficient for `includeDeclaration` filtering in V1; a proper
/// implementation would use type-check resolution (V2).
fn find_decl_location(uri: &Url, src: &str, symbol_name: &str) -> Option<Location> {
    use crate::symbols::find_word_occurrences;
    let rope = ropey::Rope::from_str(src);
    let occs = find_word_occurrences(src, symbol_name);
    let (start, end) = occs.into_iter().next()?;
    let range = crate::diagnostic_mapping::span_to_range(&rope, start, end);
    Some(Location { uri: uri.clone(), range })
}

/// Plan 104.6: collect .nv files under `root` for cross-file rename.
///
/// Plan 213 Ф.1: delegates to [`crate::compiler::collect_nv_paths`] — the
/// shared, filtered walk (target/vendor dirs + nested-repository-root guard)
/// that replaced this function's own copy of the recursion.
fn collect_nv_files_for_rename(root: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    Ok(crate::compiler::collect_nv_paths(root))
}

// ─────────────────────────────────────────────────────────────────────────────
// LSP write-after-destroyed fix — shutdown-gate tests
// ─────────────────────────────────────────────────────────────────────────────
//
// 2026-07-20: the owner reported a repeated client-side error, "Cannot call
// write after a stream was destroyed", when sending `textDocument/didClose`.
// Root cause (see `WorkspaceState::shutting_down` doc + the fix commit):
// background tasks (chiefly `run_initial_scan_with_progress`, Plan 215) kept
// sending client notifications/requests with no awareness of `shutdown`,
// which — via tower-lsp's own `Server::serve()` `join!` semantics — kept the
// whole process alive until the client's grace period expired and it
// force-killed the process, destroying its own transport mid-teardown.
//
// These tests drain the *real* `ClientSocket` loopback (the exact channel
// every `client.*` call writes into — not a mock) to prove background paths
// actually stop producing messages once `WorkspaceState::mark_shutting_down`
// has been called, instead of merely asserting on the flag itself.
#[cfg(test)]
mod shutdown_gate_tests {
    use super::*;
    use futures::StreamExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;
    use tower_lsp::LspService;

    /// Build a `(Client, receive-count handle)` pair backed by a real
    /// `ClientSocket` loopback, continuously drained in the background (like
    /// an always-connected client that never answers) so every `client.*`
    /// call's *send* completes without blocking on a response, and each
    /// arrival is counted.
    ///
    /// Also drives the service through a real `initialize` call: tower-lsp's
    /// `Client::send_request`/`send_notification` silently no-op outside
    /// `State::Initialized`/`State::ShutDown` (`service/client.rs`), so an
    /// un-initialized service would make every assertion below vacuous.
    async fn client_with_drain() -> (Client, Arc<AtomicUsize>) {
        let captured: Arc<StdMutex<Option<Client>>> = Arc::new(StdMutex::new(None));
        let captured2 = Arc::clone(&captured);
        let (mut service, socket) = LspService::new(move |client| {
            *captured2.lock().unwrap() = Some(client.clone());
            Backend::new(client)
        });
        let client = captured.lock().unwrap().clone().expect("init closure ran synchronously");

        {
            use tower::{Service, ServiceExt};
            let init = tower_lsp::jsonrpc::Request::build("initialize")
                .params(serde_json::json!({ "capabilities": {} }))
                .id(1i64)
                .finish();
            let _ = service.ready().await.unwrap().call(init).await;
            // `service`/`client` share the same `Arc<ServerState>` (set up in
            // `LspService::build`), so the `Initialized` transition above is
            // visible through `client` regardless of `service`'s lifetime —
            // dropping it here is fine, no need to keep it around.
        }

        let received = Arc::new(AtomicUsize::new(0));
        let received2 = Arc::clone(&received);
        tokio::spawn(async move {
            let mut socket = socket;
            while socket.next().await.is_some() {
                received2.fetch_add(1, Ordering::SeqCst);
            }
        });
        (client, received)
    }

    /// Spawn `refresh_client_hints_for(client, state)` as an owned,
    /// `'static` task (it takes `&Client`/`&WorkspaceState`, which aren't
    /// `'static` themselves — the async block owns clones so it can be
    /// spawned) and don't await it: each inner call is wrapped in a 2s
    /// timeout waiting for a response nobody sends in this test, but the
    /// *send* into the loopback channel we're asserting on completes near-
    /// instantly, so the test doesn't need to wait out those timeouts.
    fn spawn_refresh(client: &Client, state: &Arc<WorkspaceState>) {
        let client = client.clone();
        let state = Arc::clone(state);
        tokio::spawn(async move {
            refresh_client_hints_for(&client, &state).await;
        });
    }

    /// neg: once `shutting_down` is set, `refresh_client_hints_for` must not
    /// send anything further through the real client channel — proving the
    /// gate actually stops outgoing traffic, not just that a flag got set.
    #[tokio::test]
    async fn neg_refresh_client_hints_sends_nothing_after_shutdown() {
        let (client, received) = client_with_drain().await;
        let state = Arc::new(WorkspaceState::default());

        // Before shutdown: the 3 refresh requests are sent (fire-and-forget
        // from the drain's point of view — nobody answers them, but the
        // *send* into the loopback channel happens immediately; we don't
        // await the call itself so the test isn't gated on its 2s timeouts).
        spawn_refresh(&client, &state);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let before = received.load(Ordering::SeqCst);
        assert!(before > 0, "expected ≥1 client request before shutdown, got {before}");

        // After shutdown: must send nothing further.
        state.mark_shutting_down();
        spawn_refresh(&client, &state);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let after = received.load(Ordering::SeqCst);
        assert_eq!(after, before, "shutting_down must suppress every further client send");
    }

    /// neg: `recheck_open_documents_for` (used by both `did_rename_files`
    /// and the watched-files batch handler) must likewise send nothing once
    /// `shutting_down` is set, even with an open document and a workspace
    /// root present (the code path that would otherwise schedule a real
    /// recheck + `publish_diagnostics`).
    #[tokio::test]
    async fn neg_recheck_open_documents_sends_nothing_after_shutdown() {
        let (client, received) = client_with_drain().await;
        let state = Arc::new(WorkspaceState::default());
        state.docs.insert(
            Url::parse("file:///shutdown_gate_test.nv").unwrap(),
            ParsedFile { text: Rope::from_str("fn f() => ()"), version: 1 },
        );
        state.mark_shutting_down();

        recheck_open_documents_for(&client, &state).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            received.load(Ordering::SeqCst),
            0,
            "recheck_open_documents_for must not touch the client once shutting_down is set"
        );
    }

    /// pos: `WorkspaceState::is_shutting_down` reflects `mark_shutting_down`
    /// — the flag semantics every gate above relies on.
    #[test]
    fn pos_mark_shutting_down_flips_flag() {
        let state = WorkspaceState::default();
        assert!(!state.is_shutting_down());
        state.mark_shutting_down();
        assert!(state.is_shutting_down());
    }
}
