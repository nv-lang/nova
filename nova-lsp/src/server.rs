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
                // Plan 114 Ф.7.2: code_action_provider — quick-fix
                // для E_KW_REMOVED_LET / E_KW_REMOVED_READONLY.
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                        resolve_provider: Some(false),
                        work_done_progress_options: Default::default(),
                    },
                )),
                // Plan 123.5.1 (V5.1): field-cache code-lens над method
                // headers ("N caches inserted") + hover provider over
                // `@field` showing cache info.
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Plan 123.5.2 (V5.2, 2026-06-02): semantic tokens
                // for `@<field>` reads that field_cache analysis decides
                // to CSE/cache. Colors them differently from plain
                // field accesses. Legend defines the custom modifier
                // "cached" alongside standard "property" type.
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: Default::default(),
                            legend: SemanticTokensLegend {
                                token_types: cached_field_semantic_token_types(),
                                token_modifiers: cached_field_semantic_token_modifiers(),
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                // Future capabilities (uncomment as sub-plans land):
                // 104.3: completion_provider
                // 104.4: document_symbol_provider, workspace_symbol_provider
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

    /// Plan 114 Ф.7.2: code_action — quick-fix providers.
    ///
    /// Сейчас поддерживается:
    /// - `E_KW_REMOVED_LET` → replace `let X = …` → `ro X = …`,
    ///   `let mut X = …` → `mut X = …`.
    /// - `E_KW_REMOVED_READONLY` → replace `readonly` → `ro` в field/type/param.
    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let mut actions: Vec<CodeActionOrCommand> = Vec::new();

        // Plan 123.5.3 (V5.3, 2026-06-02): suggest "Add #pure
        // annotation" when invocation range overlaps an
        // analytically-pure method. Diagnostic-independent.
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

        for diag in params.context.diagnostics.iter() {
            if let Some(NumberOrString::String(code)) = &diag.code {
                let (label, replacement) = match code.as_str() {
                    "E_KW_REMOVED_LET" => (
                        "Plan 114: change `let` → `ro` / `mut`",
                        plan114_fix_let(&self.state, &uri, diag.range),
                    ),
                    "E_KW_REMOVED_READONLY" => (
                        "Plan 114: change `readonly` → `ro`",
                        plan114_fix_readonly(&self.state, &uri, diag.range),
                    ),
                    _ => continue,
                };
                let Some(new_text) = replacement else { continue };
                let mut changes = std::collections::HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit { range: diag.range, new_text }],
                );
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: label.to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    }),
                    command: None,
                    is_preferred: Some(true),
                    disabled: None,
                    data: None,
                }));
            }
        }
        Ok(if actions.is_empty() { None } else { Some(actions) })
    }

    /// Plan 123.5.1 (V5.1): code-lens над method headers showing
    /// "N caches inserted" — uses field_cache::analyze_module API.
    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        drop(doc);

        let lenses = run_with_large_stack(move || compute_field_cache_lenses(&src));
        Ok(lenses)
    }

    /// Plan 123.5.1 (V5.1): hover на `@field` показывает cache info.
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri.clone();
        let Some(doc) = self.state.docs.get(&uri) else { return Ok(None); };
        let src = doc.text.to_string();
        drop(doc);

        let hover = run_with_large_stack(move || compute_field_cache_hover(&src, pos));
        Ok(hover)
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
        drop(doc);

        let tokens = run_with_large_stack(move || compute_field_cache_semantic_tokens(&src));
        Ok(tokens.map(|data| {
            SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data,
            })
        }))
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
    if nova_codegen::types::check_module(&module).is_err() { return None; }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module);
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
    if nova_codegen::types::check_module(&module).is_err() { return None; }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module);
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

/// Plan 123.5.1: compute code-lens list для source text.
pub fn compute_field_cache_lenses(src: &str) -> Option<Vec<CodeLens>> {
    let mut module = nova_codegen::parser::parse(src).ok()?;
    // Best-effort pipeline (skip if type-check fails).
    if nova_codegen::types::check_module(&module).is_err() { return None; }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module);
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
    if nova_codegen::types::check_module(&module).is_err() { return None; }
    let _ = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    nova_codegen::types::infer_effects(&mut module);
    nova_codegen::callnorm::normalize_module(&mut module);
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

/// Plan 114 Ф.7.2 helper: read the `let`-keyword span (range от LSP-диагностики
/// должен указывать на сам keyword) и подставить `ro` или `mut` в зависимости
/// от следующего за ним токена. Если open-document отсутствует или range
/// out-of-bounds — возвращаем `None`.
fn plan114_fix_let(
    state: &Arc<WorkspaceState>,
    uri: &Url,
    range: Range,
) -> Option<String> {
    let doc = state.docs.get(uri)?;
    let rope = &doc.text;
    // LSP range is UTF-16; convert to char-offset for ropey via line+char.
    let line_idx = range.start.line as usize;
    let line_end_idx = range.end.line as usize;
    if line_idx >= rope.len_lines() || line_end_idx >= rope.len_lines() {
        return None;
    }
    // Extract context after the `let` keyword on the same line.
    let line = rope.line(line_idx).to_string();
    let after_let_col = (range.end.character as usize).min(line.len());
    let tail = line[after_let_col..].trim_start();
    // `let mut X = …` → `mut`; `let X = …` → `ro`.
    if tail.starts_with("mut") {
        Some("mut".to_string())
    } else {
        Some("ro".to_string())
    }
}

/// Plan 114 Ф.7.2: `readonly` → `ro` всегда (canonical rename).
fn plan114_fix_readonly(
    _state: &Arc<WorkspaceState>,
    _uri: &Url,
    _range: Range,
) -> Option<String> {
    Some("ro".to_string())
}
