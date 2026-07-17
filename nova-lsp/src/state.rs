//! Workspace state — shared mutable state for the LSP server.
//!
//! Plan 104.0.1: empty WorkspaceState stub.
//! Plan 104.0.3: full implementation — DashMap<Url, ParsedFile> document cache.
//! Plan 104.1:   adds Debouncer, workspace root, cancellation support.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::lsp_types::{FileEvent, Url};

use std::path::Path;

use crate::debouncer::Debouncer;
use crate::provenance::{self, ResolvedModule};
use crate::semantic_tokens_delta::SemanticTokensSnapshot;
use crate::stdlib_index::StdlibIndex;
use crate::symbols::{DocumentSymbolCache, ReferencesIndex, WorkspaceIndex};

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

/// A cached, fully-resolved module for one open document (Plan 104.10 Ф.1).
///
/// Holds the parsed + import-inlined + type-checked [`ResolvedModule`] behind an
/// `Arc` so many concurrent IDE requests (hover/goto/completion) share one build
/// without cloning the `Module`. `version` is the document version this build
/// was produced from — a request supplying a newer version forces a rebuild.
#[derive(Clone)]
pub struct CachedResolved {
    /// Shared, immutable resolved module (parse + imports + env).
    pub resolved: Arc<ResolvedModule>,
    /// Document version this build corresponds to.
    pub version: i32,
}

// `ResolvedModule` (Module + env) is intentionally not `Debug`; a manual impl
// keeps `WorkspaceState`'s derived `Debug` working while avoiding dumping the
// whole AST. Only the discriminating `version` is shown.
impl std::fmt::Debug for CachedResolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedResolved")
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
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

    /// Debouncer for compile tasks — coalesces rapid interactive edits per URI
    /// (200ms, gopls/rust-analyzer default — kept snappy for typing).
    pub debouncer: Debouncer,

    /// Plan 213 Ф.2: separate, longer-delay debouncer for
    /// `workspace/didChangeWatchedFiles` bursts (git checkout / branch switch
    /// / build can each deliver hundreds to thousands of notifications in a
    /// few milliseconds). 400ms — long enough to absorb a realistic burst
    /// without feeling laggy for the rare single external edit. Kept separate
    /// from `debouncer` (interactive edits) so the two delays can be tuned
    /// independently and neither workload starves the other's key space.
    pub watch_debouncer: Debouncer,

    /// Plan 213 Ф.2: events buffered between `did_change_watched_files`
    /// notification arrivals and the debounced apply-pass. Drained (taken)
    /// by the scheduled work closure each time `watch_debouncer` fires, so a
    /// burst of notifications inside the debounce window contributes to ONE
    /// apply-pass instead of one per notification.
    pub pending_watch_events: Mutex<Vec<FileEvent>>,

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

    /// Plan 104.4: per-URI document symbol cache.
    /// Invalidated on `didChange`/`didOpen`.  Populated lazily on first
    /// `textDocument/documentSymbol` request after each edit.
    pub document_symbol_cache: DocumentSymbolCache,

    /// Plan 104.4: project-wide symbol index for `workspace/symbol`.
    /// Updated incrementally: per-file re-index on `didChange`/`didOpen`.
    pub workspace_index: WorkspaceIndex,

    /// Plan 104.10 Ф.12: incremental references index (`name → [(uri, span)]`).
    /// Replaces the V1 per-request full-filesystem scan for
    /// `textDocument/references`. Updated on `didOpen`/`didChange`, external
    /// watched-file events, and rename; cold-scanned once in the background.
    pub references_index: ReferencesIndex,

    /// Plan 104.10 Ф.1: per-URI cache of the fully-resolved module
    /// (parse + import inlining + type-check with `expr_types`). Populated
    /// lazily by [`WorkspaceState::get_or_build_resolved`] on the first IDE
    /// request after each edit and reused (cache hit by version) by every
    /// subsequent hover/goto/completion on the same document version. Only
    /// open documents are cached; entries are evicted on `didClose`.
    pub resolved_cache: DashMap<Url, CachedResolved>,

    /// Plan 104.10 Ф.1: monotonic count of `ResolvedModule` builds performed by
    /// [`WorkspaceState::get_or_build_resolved`]. Used by tests to assert
    /// cache hits (no rebuild on a repeated same-version request) and rebuilds
    /// (after a version bump). Incremented once per actual build.
    pub resolved_build_count: AtomicU64,

    /// Plan 104.10 Ф.5: per-stdlib-directory cache of the filesystem search-path
    /// [`StdlibIndex`] (import-completion module tree + type/protocol → module
    /// resolution for add-import quick-fixes). Keyed by the resolved stdlib dir
    /// so all documents in one workspace share a single FS walk. Built lazily on
    /// first use ([`WorkspaceState::stdlib_index`]).
    pub stdlib_index_cache: DashMap<PathBuf, Arc<StdlibIndex>>,

    /// Plan 104.10 Ф.9: inlay-hint toggles (type hints / parameter-name hints),
    /// both default **on**. Set from the client's `initializationOptions` at
    /// `initialize` and updated on `workspace/didChangeConfiguration`.
    pub inlay_config: Mutex<crate::inlay_hints::InlayHintConfig>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            docs: DashMap::new(),
            debouncer: Debouncer::default(),
            watch_debouncer: Debouncer::new(Duration::from_millis(400)),
            pending_watch_events: Mutex::new(Vec::new()),
            workspace_root: Mutex::new(None),
            semantic_tokens_cache: DashMap::new(),
            semantic_tokens_counter: AtomicU64::new(0),
            document_symbol_cache: DocumentSymbolCache::default(),
            workspace_index: WorkspaceIndex::default(),
            references_index: ReferencesIndex::default(),
            resolved_cache: DashMap::new(),
            resolved_build_count: AtomicU64::new(0),
            stdlib_index_cache: DashMap::new(),
            inlay_config: Mutex::new(crate::inlay_hints::InlayHintConfig::default()),
        }
    }
}

impl WorkspaceState {
    /// Cancel all pending debounce tasks — called on shutdown.
    pub fn cancel_all(&self) {
        self.debouncer.cancel_all();
        self.watch_debouncer.cancel_all();
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

    /// Plan 104.10 Ф.9: current inlay-hint configuration (cheap `Copy`).
    pub fn inlay_config(&self) -> crate::inlay_hints::InlayHintConfig {
        *self.inlay_config.lock().unwrap()
    }

    /// Plan 104.10 Ф.9: replace the inlay-hint configuration (from
    /// `initializationOptions` / `didChangeConfiguration`).
    pub fn set_inlay_config(&self, cfg: crate::inlay_hints::InlayHintConfig) {
        *self.inlay_config.lock().unwrap() = cfg;
    }

    /// Plan 104.10 Ф.1: return the fully-resolved module for `uri` at document
    /// `version`, building and caching it on demand.
    ///
    /// - **Cache hit:** an existing entry whose `version` matches is returned
    ///   directly (a cheap `Arc` clone) — no re-parse / re-resolve / re-check.
    /// - **Cache miss / stale:** build a fresh [`ResolvedModule`] via
    ///   [`provenance::resolve_module_for_ide`] (which records `expr_types`,
    ///   Ф.2), store it under the current `version`, and return it.
    ///
    /// Every actual build bumps `resolved_build_count` so tests can distinguish
    /// hits from rebuilds.
    ///
    /// # Concurrency
    ///
    /// Thread-safe via `DashMap` + `Arc`. Two threads racing on the same
    /// uncached `(uri, version)` may each build once (both results are valid and
    /// equivalent); the later `insert` wins and both callers receive a usable
    /// `Arc`. We deliberately do not hold a shard lock across the (potentially
    /// slow) build to avoid blocking unrelated URIs on the same shard.
    ///
    /// # Dependency invalidation
    ///
    /// This keys only on the document's own `uri` + `version`. Editing a file
    /// `A` does not yet invalidate cached importers of `A`; a stale importer
    /// entry survives until that importer is itself edited (version bump) or
    /// closed. See marker `[M-104.10-dependent-invalidation]` — full reverse
    /// dependency invalidation (module-graph, zls-style) is deferred.
    pub fn get_or_build_resolved(
        &self,
        uri: &Url,
        version: i32,
        src: &str,
    ) -> Arc<ResolvedModule> {
        // Fast path: a build for this exact version already exists. The read
        // guard is scoped to this `if let` and dropped before any `insert`,
        // so we never deadlock by inserting into a shard we still hold read.
        if let Some(entry) = self.resolved_cache.get(uri) {
            if entry.version == version {
                return entry.resolved.clone();
            }
        }

        // Miss or stale: build outside any map lock. Unsaved / non-file URIs
        // (no on-disk path) degrade to the URI's raw path; `resolve_module_for`
        // canonicalizes best-effort and never panics on a missing file.
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.path()));
        let resolved = Arc::new(provenance::resolve_module_for_ide(&path, src));
        self.resolved_build_count.fetch_add(1, Ordering::Relaxed);

        self.resolved_cache.insert(
            uri.clone(),
            CachedResolved { resolved: resolved.clone(), version },
        );
        resolved
    }

    /// Plan 104.10 Ф.1: evict the resolved-module cache entry for `uri`
    /// (called on `didClose` to bound memory to open documents).
    pub fn invalidate_resolved(&self, uri: &Url) {
        self.resolved_cache.remove(uri);
    }

    /// Plan 104.10 Ф.18: evict **every** resolved-module cache entry.
    ///
    /// Used on external file changes (`workspace/didChangeWatchedFiles`) and
    /// manifest edits: because nova-lsp does not (yet) maintain a precise
    /// module-graph reverse-dependency map, a changed peer file could invalidate
    /// any importer's cached build. Clearing the whole cache is the correct
    /// (never-stale) superset — entries are cheap, bounded to open documents, and
    /// rebuilt lazily on the next request. See `[M-104.10-watch-reverse-deps]`.
    pub fn invalidate_all_resolved(&self) {
        self.resolved_cache.clear();
    }

    /// Plan 104.10 Ф.5: the filesystem [`StdlibIndex`] for the workspace that
    /// contains `doc_path`, built once per stdlib directory and shared. Resolves
    /// the repo root from `doc_path` (nearest `nova.toml` workspace), then the
    /// stdlib dir (`resolve_std_path`), builds/caches the index. `None` when
    /// `doc_path` is not inside a Nova workspace.
    pub fn stdlib_index(&self, doc_path: &Path) -> Option<Arc<StdlibIndex>> {
        let repo = nova_codegen::test_runner::find_repo_root_from(doc_path)?;
        let stdlib_dir = nova_codegen::manifest::resolve_std_path(&repo);
        if let Some(idx) = self.stdlib_index_cache.get(&stdlib_dir) {
            return Some(idx.clone());
        }
        let idx = Arc::new(StdlibIndex::build(&stdlib_dir, "std"));
        self.stdlib_index_cache
            .insert(stdlib_dir.clone(), idx.clone());
        Some(idx)
    }

    /// Plan 104.10 Ф.1: number of `ResolvedModule` builds performed so far.
    /// Used by tests to assert cache hits vs rebuilds.
    pub fn resolved_build_count(&self) -> u64 {
        self.resolved_build_count.load(Ordering::Relaxed)
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

    // ─────────────────────────────────────────────────────────────────────────
    // Plan 104.10 Ф.1 — resolved-module symbol cache
    // ─────────────────────────────────────────────────────────────────────────

    use std::sync::Arc;

    /// Locate the repo root so cache tests resolve a real file with real imports
    /// (mirrors `provenance.rs` test setup). CARGO_MANIFEST_DIR = .../nova-lsp;
    /// the repo root is its parent.
    fn repo_root() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.parent().expect("nova-lsp has a parent").to_path_buf()
    }

    /// Write `src` to an isolated per-fixture directory inside the repo (so
    /// `find_repo_root_from` + stdlib resolution work) and return its `file://`
    /// URI plus the on-disk path. Each fixture gets its own sub-directory so
    /// sibling files declaring the same `module` name are not collected as
    /// folder-module peers of one another.
    fn write_fixture(stem: &str, src: &str) -> (Url, PathBuf) {
        let dir = repo_root().join("target").join("f1_cache_test").join(stem);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{stem}.nv"));
        std::fs::write(&path, src).unwrap();
        let uri = Url::from_file_path(&path).expect("valid file URI");
        (uri, path)
    }

    /// A small valid Nova source that transitively imports the prelude (so the
    /// build performs real import inlining + type-check, not a trivial parse).
    const CACHE_SRC: &str = "module basics.lsp\nimport std.collections\nfn f() => ()\n";

    /// POS: two same-version requests → the second is a cache hit (no rebuild),
    /// and both hand back the very same `Arc` (shared, not re-cloned build).
    #[test]
    fn f1_pos_cache_hit_same_version() {
        let state = WorkspaceState::default();
        let (uri, _path) = write_fixture("f1_hit", CACHE_SRC);

        let first = state.get_or_build_resolved(&uri, 1, CACHE_SRC);
        assert_eq!(state.resolved_build_count(), 1, "first request must build once");

        let second = state.get_or_build_resolved(&uri, 1, CACHE_SRC);
        assert_eq!(
            state.resolved_build_count(),
            1,
            "same-version request must be a cache hit (no rebuild)"
        );
        assert!(
            Arc::ptr_eq(&first, &second),
            "cache hit must return the same shared Arc"
        );
        // The cached build is the FULL resolved module (Ф.1 crit #5): entry
        // provenance present.
        assert!(
            first.file_map.contains_key(&nova_codegen::diag::MAIN_FILE_ID),
            "cached ResolvedModule must carry provenance for the entry file"
        );
    }

    /// POS: a version bump invalidates the entry → the next request rebuilds.
    #[test]
    fn f1_pos_rebuild_on_version_bump() {
        let state = WorkspaceState::default();
        let (uri, _path) = write_fixture("f1_bump", CACHE_SRC);

        let _v1 = state.get_or_build_resolved(&uri, 1, CACHE_SRC);
        assert_eq!(state.resolved_build_count(), 1);

        // didChange bumps the version; the next request must rebuild.
        let _v2 = state.get_or_build_resolved(&uri, 2, CACHE_SRC);
        assert_eq!(
            state.resolved_build_count(),
            2,
            "a newer document version must force a rebuild"
        );

        // And the fresh version is now the cached one (a repeat is a hit).
        let _v2b = state.get_or_build_resolved(&uri, 2, CACHE_SRC);
        assert_eq!(state.resolved_build_count(), 2, "repeat at v2 is a cache hit");
    }

    /// POS: `didClose` eviction (`invalidate_resolved`) removes the entry.
    #[test]
    fn f1_pos_close_evicts_entry() {
        let state = WorkspaceState::default();
        let (uri, _path) = write_fixture("f1_close", CACHE_SRC);

        let _ = state.get_or_build_resolved(&uri, 1, CACHE_SRC);
        assert!(state.resolved_cache.contains_key(&uri), "entry present after build");

        state.invalidate_resolved(&uri);
        assert!(
            !state.resolved_cache.contains_key(&uri),
            "entry must be evicted after didClose"
        );

        // A request after eviction rebuilds (build_count increments again).
        let before = state.resolved_build_count();
        let _ = state.get_or_build_resolved(&uri, 1, CACHE_SRC);
        assert_eq!(
            state.resolved_build_count(),
            before + 1,
            "post-eviction request must rebuild"
        );
    }

    /// NEG: a request on a never-seen URI builds without panicking and yields a
    /// usable resolved module (entry provenance present).
    #[test]
    fn f1_neg_uncached_uri_builds_without_panic() {
        let state = WorkspaceState::default();
        let (uri, _path) = write_fixture("f1_uncached", CACHE_SRC);

        let resolved = state.get_or_build_resolved(&uri, 7, CACHE_SRC);
        assert!(
            resolved.file_map.contains_key(&nova_codegen::diag::MAIN_FILE_ID),
            "fresh build must map the entry file"
        );
        assert_eq!(state.resolved_build_count(), 1);
    }

    /// EDGE: two threads racing on the same uncached `(uri, version)` both get a
    /// valid result with no data-race / panic. At most one build per thread; the
    /// cache ends with a single coherent entry.
    #[test]
    fn f1_edge_concurrent_same_uri() {
        let state = Arc::new(WorkspaceState::default());
        let (uri, _path) = write_fixture("f1_concurrent", CACHE_SRC);

        let handles: Vec<_> = (0..2)
            .map(|_| {
                let state = Arc::clone(&state);
                let uri = uri.clone();
                std::thread::spawn(move || {
                    let r = state.get_or_build_resolved(&uri, 1, CACHE_SRC);
                    // Every racer gets a valid, fully-resolved module.
                    assert!(r.file_map.contains_key(&nova_codegen::diag::MAIN_FILE_ID));
                })
            })
            .collect();
        for h in handles {
            h.join().expect("no panic / data-race in concurrent build");
        }

        // Both threads may have raced to build (≤ one build each); the final
        // cache holds exactly one entry at the requested version.
        let count = state.resolved_build_count();
        assert!((1..=2).contains(&count), "build_count in 1..=2, got {count}");
        let entry = state.resolved_cache.get(&uri).expect("entry cached after race");
        assert_eq!(entry.version, 1);
    }

    /// PERF (Ф.1 crit #4): a cache hit is ≤10ms and strictly cheaper than the
    /// cold build. Uses a large (~1000-line) source so the cold path pays real
    /// parse + import-inline + type-check cost, while the hit is an `Arc` clone.
    #[test]
    fn f1_perf_cache_hit_under_10ms() {
        // ~1000 lines of trivial functions + a prelude-importing header.
        let mut src = String::from("module basics.lsp\nimport std.collections\n");
        for i in 0..1000 {
            src.push_str(&format!("fn f{i}() => ()\n"));
        }
        let (uri, _path) = write_fixture("f1_perf", &src);

        // Cold build.
        let cold = crate::perf::PerfTimer::start("f1_cold_build");
        let _first = state_perf_build(&uri, 1, &src);
        let cold_ms = cold.finish();

        // Warm hit (same version).
        let warm = crate::perf::PerfTimer::start("f1_warm_hit");
        let _second = STATE_FOR_PERF.with(|s| s.get_or_build_resolved(&uri, 1, &src));
        let warm_ms = warm.finish();

        assert!(warm_ms <= 10, "cache hit must be ≤10ms, was {warm_ms}ms");
        assert!(
            warm_ms <= cold_ms,
            "cache hit ({warm_ms}ms) must not exceed cold build ({cold_ms}ms)"
        );
    }

    // A thread-local `WorkspaceState` so the cold build and the warm hit in the
    // perf test share one cache instance.
    thread_local! {
        static STATE_FOR_PERF: WorkspaceState = WorkspaceState::default();
    }

    fn state_perf_build(uri: &Url, version: i32, src: &str) -> Arc<ResolvedModule> {
        STATE_FOR_PERF.with(|s| s.get_or_build_resolved(uri, version, src))
    }
}
