//! Workspace state — shared mutable state for the LSP server.
//!
//! Plan 104.0.1: empty WorkspaceState stub.
//! Plan 104.0.3: full implementation — DashMap<Url, ParsedFile> document cache.
//! Plan 104.1:   adds compiler diagnostics cache + background recheck channel.

use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::lsp_types::Url;

// ─────────────────────────────────────────────────────────────────────────────
// Data model
// ─────────────────────────────────────────────────────────────────────────────

/// A cached open document (one entry per URI in `WorkspaceState::docs`).
///
/// `text` is stored as a `Rope` rather than a plain `String` because:
/// - Rope provides O(log n) slice/insert, important for large files.
/// - `ropey::Rope` is UTF-8–aware; its character-index API maps naturally
///   to LSP UTF-16 position arithmetic (Plan 104.2+ hover / goto-def).
///
/// V1 (Plan 104.0.3): the Rope is rebuilt entirely on each `didChange` (Full
///   sync).  Incremental edits (TextDocumentSyncKind::Incremental) arrive in
///   Plan 104.6 V2 when we switch to passing `range` deltas into `Rope::remove`
///   / `Rope::insert`.
#[derive(Debug)]
pub struct ParsedFile {
    /// Full document text.
    pub text: Rope,
    /// Client-assigned document version (monotonically increasing per document).
    /// Used for version conflict detection (Plan 104.1+).
    pub version: i32,
}

// ─────────────────────────────────────────────────────────────────────────────
// WorkspaceState
// ─────────────────────────────────────────────────────────────────────────────

/// Shared workspace state: open document cache + (Plan 104.1+) compiler cache.
///
/// One instance is created at server startup and shared (behind `Arc`) across
/// all LSP handler futures.
///
/// # Concurrency
///
/// Fields use `DashMap` rather than `Mutex<HashMap>` to allow concurrent reads
/// with minimal contention: `didOpen`, `didChange`, and `didClose` events can
/// arrive in rapid succession (e.g., when the editor saves multiple files).
/// `DashMap` uses per-shard `RwLock`, so concurrent reads to _different_ shards
/// proceed in parallel, and writes only lock a single shard.
#[derive(Debug, Default)]
pub struct WorkspaceState {
    /// Open document cache: file URI → last-known (text, version).
    ///
    /// Populated by `didOpen`, updated by `didChange`, cleaned up by `didClose`.
    /// In Plan 104.1, hover / diagnostic recheck also reads this map.
    pub docs: DashMap<Url, ParsedFile>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
//
// These tests exercise the data-model layer directly (WorkspaceState + ParsedFile)
// without spawning a process.  They are complementary to the integration tests
// in `tests/document_cache.rs`, which verify the LSP protocol handlers.
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

        // Simulate didChange (full sync: replace whole text)
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
    ///
    /// This mirrors the didChange-on-unopened-file path in server.rs: if the
    /// document isn't in the cache, we skip silently rather than crashing.
    #[test]
    fn neg1_change_on_nonexistent_is_noop() {
        let state = WorkspaceState::default();
        let uri = uri("nope.nv");

        // Should not panic
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
    ///
    /// DashMap::insert returns the old value; the server logs a warning and
    /// overwrites (see server.rs did_open).  This test verifies the overwrite
    /// semantic at the data-model level.
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
    ///
    /// This matters for LSP position encoding (UTF-16 column numbers vs
    /// UTF-8 byte offsets).  Ropey is UTF-8 native and exposes both
    /// char-index and byte-index APIs; Plan 104.2 will use char_to_utf16_cu().
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

    /// URI with percent-encoded characters (e.g., spaces, Cyrillic paths).
    #[test]
    fn uri_with_percent_encoding() {
        // Windows paths with spaces or non-ASCII dirs are percent-encoded by editors
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
