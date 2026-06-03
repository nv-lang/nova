//! Workspace state — shared mutable state for the LSP server.
//!
//! Plan 104.0.1: empty WorkspaceState stub.
//! Plan 104.0.3: full implementation — DashMap<Url, ParsedFile> document cache.
//! Plan 104.1:   adds Debouncer, workspace root, cancellation support.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::lsp_types::Url;

use crate::debouncer::Debouncer;
use crate::semantic_tokens_delta::SemanticTokensSnapshot;

// ─────────────────────────────────────────────────────────────────────────────
// Data model
// ─────────────────────────────────────────────────────────────────────────────

/// A cached open document (one entry per URI in `WorkspaceState::docs`).
///
/// `text` is stored as a `Rope` because:
/// - Rope provides O(log n) slice/insert for large files.
/// - `ropey::Rope` UTF-8 API maps naturally to LSP UTF-16 position arithmetic.
///
/// Plan 104.1.Ф.4: switch to `TextDocumentSyncKind::Incremental` — Rope is
/// updated via `apply_changes` range-deltas in `did_change`.
#[derive(Debug)]
pub struct ParsedFile {
    /// Full document text.
    pub text: Rope,
    /// Client-assigned document version (monotonically increasing per document).
    /// Passed back in `publishDiagnostics` for outdated-suppression.
    pub version: i32,
}

// ─────────────────────────────────────────────────────────────────────────────
// WorkspaceState
// ─────────────────────────────────────────────────────────────────────────────

/// Shared workspace state: open document cache + debouncer + workspace root.
///
/// One instance created at server startup and shared (behind `Arc`) across
/// all LSP handler futures.
///
/// # Concurrency
///
/// `docs` uses `DashMap` (per-shard RwLock) for fine-grained concurrency.
/// `workspace_root` is write-once (set in `initialize`) behind a `Mutex`.
/// `debouncer` is `Clone`-able and internally uses `Arc<Mutex<…>>`.
#[derive(Debug)]
pub struct WorkspaceState {
    /// Open document cache: file URI → last-known (text, version).
    pub docs: DashMap<Url, ParsedFile>,

    /// Debouncer for compile tasks — coalesces rapid edits per URI.
    pub debouncer: Debouncer,

    /// Workspace root path, set from `initialize` rootUri / workspaceFolders.
    /// `None` until `initialize` is received.
    pub workspace_root: Mutex<Option<PathBuf>>,

    /// Plan 123.5.5 (V5.5, 2026-06-03): per-URI snapshot последнего
    /// semantic-tokens ответа сервера. Используется `semantic_tokens_full_delta`
    /// для валидации `previous_result_id` клиента и computeединия минимального
    /// edit script'а через `compute_semantic_token_edits`. Snapshot
    /// перезаписывается каждый раз когда server отвечает полным
    /// `semantic_tokens_full` (либо delta запрос неудачный, fallback к full).
    pub semantic_tokens_cache: DashMap<Url, SemanticTokensSnapshot>,

    /// Plan 123.5.5: monotonic counter генерирующий уникальные `result_id`
    /// для каждого emitted snapshot. Format `st-<N>` (stable prefix +
    /// monotonic integer). Гарантирует client'у что old result_ids не
    /// будут случайно reused при wrap-around.
    pub semantic_tokens_counter: AtomicU64,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            docs: DashMap::new(),
            debouncer: Debouncer::default(),
            workspace_root: Mutex::new(None),
            semantic_tokens_cache: DashMap::new(),
            semantic_tokens_counter: AtomicU64::new(0),
        }
    }
}

impl WorkspaceState {
    /// Cancel all pending debounce tasks — called on shutdown.
    pub fn cancel_all(&self) {
        self.debouncer.cancel_all();
    }

    /// Get workspace root, if set.
    pub fn workspace_root(&self) -> Option<PathBuf> {
        self.workspace_root.lock().unwrap().clone()
    }

    /// Set workspace root from an LSP URI.
    pub fn set_workspace_root_from_uri(&self, uri: &Url) {
        if let Ok(path) = uri.to_file_path() {
            *self.workspace_root.lock().unwrap() = Some(path);
        }
    }

    /// Plan 123.5.5 (V5.5): allocate the next monotonic semantic-tokens
    /// `result_id`. Format `st-<N>` — stable prefix gives clients a
    /// quick way to validate they're looking at a nova-lsp result id;
    /// monotonic integer ensures uniqueness across the server lifetime.
    pub fn next_semantic_tokens_result_id(&self) -> String {
        let n = self.semantic_tokens_counter.fetch_add(1, Ordering::Relaxed);
        format!("st-{}", n)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(path: &str) -> Url {
        Url::parse(&format!("file:///{path}")).expect("valid test URI")
    }

    // ── pos1 ─────────────────────────────────────────────────────────────────

    /// pos1: Inserting a ParsedFile and retrieving it gives back the original text + version.
    #[test]
    fn pos1_open_inserts_text_and_version() {
        let state = WorkspaceState::default();
        let uri = uri("foo.nv");
        let text = "fn main() => ()";

        state.docs.insert(
            uri.clone(),
            ParsedFile {
                text: Rope::from_str(text),
                version: 1,
            },
        );

        let file = state.docs.get(&uri).expect("doc should be present after insert");
        assert_eq!(file.text.to_string(), text, "text mismatch after open");
        assert_eq!(file.version, 1, "version mismatch after open");
    }

    // ── pos2 ─────────────────────────────────────────────────────────────────

    /// pos2: Mutating a ParsedFile in-place (simulating didChange) updates text + version.
    #[test]
    fn pos2_change_updates_text_and_version() {
        let state = WorkspaceState::default();
        let uri = uri("bar.nv");

        state.docs.insert(
            uri.clone(),
            ParsedFile {
                text: Rope::from_str("version 1"),
                version: 1,
            },
        );

        {
            let mut file = state
                .docs
                .get_mut(&uri)
                .expect("doc must exist before change");
            file.text = Rope::from_str("version 2");
            file.version = 2;
        }

        let file = state.docs.get(&uri).unwrap();
        assert_eq!(file.text.to_string(), "version 2");
        assert_eq!(file.version, 2);
    }

    // ── pos3 ─────────────────────────────────────────────────────────────────

    /// pos3: Removing a document (didClose) leaves docs empty for that URI.
    #[test]
    fn pos3_close_removes_document() {
        let state = WorkspaceState::default();
        let uri = uri("baz.nv");

        state.docs.insert(
            uri.clone(),
            ParsedFile {
                text: Rope::from_str("x"),
                version: 1,
            },
        );
        assert!(state.docs.contains_key(&uri), "doc should exist before close");

        state.docs.remove(&uri);
        assert!(
            !state.docs.contains_key(&uri),
            "doc should be absent after close"
        );
    }

    // ── neg1 ─────────────────────────────────────────────────────────────────

    /// neg1: get_mut on a non-existent URI returns None (no panic, no insertion).
    #[test]
    fn neg1_change_on_nonexistent_is_noop() {
        let state = WorkspaceState::default();
        let uri = uri("nope.nv");

        assert!(
            state.docs.get_mut(&uri).is_none(),
            "get_mut on absent key must return None"
        );
        assert!(
            !state.docs.contains_key(&uri),
            "absent key must not be inserted by get_mut"
        );
    }

    // ── neg2 ─────────────────────────────────────────────────────────────────

    /// neg2: Opening the same URI twice — second insert overwrites the first.
    #[test]
    fn neg2_open_twice_overwrites() {
        let state = WorkspaceState::default();
        let uri = uri("double.nv");

        state.docs.insert(
            uri.clone(),
            ParsedFile {
                text: Rope::from_str("first"),
                version: 1,
            },
        );
        state.docs.insert(
            uri.clone(),
            ParsedFile {
                text: Rope::from_str("second"),
                version: 2,
            },
        );

        let file = state.docs.get(&uri).unwrap();
        assert_eq!(file.text.to_string(), "second", "second open should overwrite");
        assert_eq!(file.version, 2);
    }

    // ── edge cases ───────────────────────────────────────────────────────────

    /// Rope correctly handles multi-byte UTF-8: emoji, Cyrillic, CJK.
    #[test]
    fn rope_multibyte_unicode_preserved() {
        let state = WorkspaceState::default();
        let uri = uri("unicode.nv");
        let text = "fn приветствие() => 👋\n// Ñoño";

        state.docs.insert(
            uri.clone(),
            ParsedFile {
                text: Rope::from_str(text),
                version: 1,
            },
        );

        let file = state.docs.get(&uri).unwrap();
        assert_eq!(file.text.to_string(), text, "multi-byte text must round-trip");
    }

    /// An empty document (didOpen with empty text) is valid.
    #[test]
    fn rope_empty_document() {
        let state = WorkspaceState::default();
        let uri = uri("empty.nv");

        state.docs.insert(
            uri.clone(),
            ParsedFile {
                text: Rope::from_str(""),
                version: 1,
            },
        );

        let file = state.docs.get(&uri).unwrap();
        assert_eq!(file.text.len_chars(), 0);
        assert_eq!(file.text.to_string(), "");
    }

    /// URI with percent-encoded characters.
    #[test]
    fn uri_with_percent_encoding() {
        let state = WorkspaceState::default();
        let uri = Url::parse("file:///C:/My%20Project/main.nv").expect("valid URI");

        state.docs.insert(
            uri.clone(),
            ParsedFile {
                text: Rope::from_str("fn f() => ()"),
                version: 1,
            },
        );
        assert!(state.docs.contains_key(&uri));
    }
}
