//! Persistent on-disk cache for the workspace symbol / references index —
//! Plan 215.
//!
//! # Motivation
//!
//! `nova-lsp` re-derives its `workspace/symbol` and `textDocument/references`
//! indices from scratch on **every** server start (`server.rs::
//! run_initial_scan_with_progress`): every `.nv` file under the workspace root
//! (~3090 on the main Nova repo) is read from disk and re-parsed. Mature LSP
//! servers (rust-analyzer, gopls, clangd) instead persist the index across
//! restarts and only re-derive the files that actually changed since the last
//! run — this module is that persistence layer for nova-lsp. It closes the
//! `[M-104.10-persistent-index]` follow-up (`docs/plans/backlog-followups.md`).
//!
//! # Scope
//!
//! Only the **parse-based** indices (`symbols::WorkspaceIndex`,
//! `symbols::ReferencesIndex`) are cached here. The cold-start
//! *type-check* pass (`compiler::check_workspace`, run once at startup purely
//! to publish initial diagnostics) is **not** — correctly caching per-file
//! diagnostics would require a full reverse-dependency graph (a changed peer
//! file can change another file's diagnostics), which is the separate, still
//! -open `[M-104.10-dependent-invalidation]` follow-up. Re-deriving
//! diagnostics on every start is unchanged by this module.
//!
//! # Format
//!
//! One JSON file per workspace: `<root>/target/nova-lsp-cache/index-v1.json`
//! (mirrors the compiler's own build-cache precedent,
//! `nova-cli/src/build_cache.rs`'s `target/.nova-cache/`; `target/` is already
//! covered by the repo's top-level `.gitignore`, so no new ignore entry is
//! needed for a workspace that already builds with `nova build`). `serde_json`
//! is already a dependency (used for LSP capability options) — reusing it
//! here needs no new wire format or crate.
//!
//! [`PersistedIndex::format_version`] must equal [`CACHE_FORMAT_VERSION`] for
//! the cache to be trusted. Any mismatch, and any I/O or parse failure, is
//! treated as "no cache" — the caller falls back to a full re-index. This
//! must never panic or crash the server; a corrupt or foreign-version cache
//! file is exactly as safe as a missing one (see `pos_corrupt_json_is_none`
//! / `pos_wrong_version_is_none` below).
//!
//! # Invalidation
//!
//! Per-file freshness is `(mtime_nanos, size)` — the same fingerprint scheme
//! the compiler's build cache would use for a plain file (see
//! `build_cache.rs`'s doc comment on why content hashing was chosen there
//! instead: that cache is content-addressed because its *key* must be stable
//! across machines/checkouts; this cache is a purely-local, single-workspace
//! accelerator where a `stat()` per file is far cheaper than hashing file
//! content, and mtime+size is what rust-analyzer/gopls/clangd use for the
//! same purpose). A mismatch on either field means "reparse this file".

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::Range;

use crate::symbols::WorkspaceSymbolEntry;

/// Bump whenever [`PersistedIndex`] or [`CachedFile`]'s shape changes in a way
/// that isn't forward/backward compatible. A version mismatch is treated
/// exactly like a missing cache file (full fallback re-index — never a
/// crash).
pub const CACHE_FORMAT_VERSION: u32 = 1;

/// The cache directory for a given workspace root: `<root>/target/nova-lsp-cache/`.
/// Mirrors `nova-cli/src/build_cache.rs::cache_dir`'s `target/.nova-cache`
/// convention — `target/` is disposable/regenerable and already
/// `.gitignore`d at the repo root.
pub fn cache_dir(root: &Path) -> PathBuf {
    root.join("target").join("nova-lsp-cache")
}

/// The single index file inside [`cache_dir`]. Versioned by filename in
/// addition to the in-file `format_version` field, so a stale binary reading
/// a directory written by a newer/older schema doesn't even attempt to parse
/// a file it doesn't expect (belt-and-suspenders; `format_version` alone
/// already makes this safe).
fn cache_file(root: &Path) -> PathBuf {
    cache_dir(root).join(format!("index-v{CACHE_FORMAT_VERSION}.json"))
}

/// One file's contribution to the persisted index: enough to reinstall its
/// `WorkspaceIndex`/`ReferencesIndex` entries with zero re-parsing, plus the
/// on-disk fingerprint used to detect staleness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFile {
    /// Modification time, nanoseconds since `UNIX_EPOCH`.
    pub mtime_nanos: u64,
    /// File size in bytes.
    pub size: u64,
    /// This file's `workspace/symbol` entries (`WorkspaceIndex::export_file`).
    pub symbols: Vec<WorkspaceSymbolEntry>,
    /// This file's references contribution: `(identifier, occurrence ranges)`
    /// pairs (`ReferencesIndex::export_file`).
    pub refs: Vec<(String, Vec<Range>)>,
}

/// The whole persisted index: one workspace, one file, one JSON document.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PersistedIndex {
    /// Schema version this document was written with. Checked against
    /// [`CACHE_FORMAT_VERSION`] on load; a mismatch is a cache miss, not an
    /// error.
    pub format_version: u32,
    /// `file://` URI string → that file's cached contribution.
    pub files: HashMap<String, CachedFile>,
}

impl PersistedIndex {
    /// A fresh, empty index stamped with the current format version — the
    /// starting point for a scan that will `insert` a `CachedFile` per file
    /// as it's (re)indexed or reused from a warm hit.
    pub fn new() -> Self {
        PersistedIndex { format_version: CACHE_FORMAT_VERSION, files: HashMap::new() }
    }
}

/// `(mtime_nanos, size)` for `path`, or `None` if the file cannot be stat'd
/// (deleted between listing and stat, permission error, …) — the caller
/// treats that exactly like "no cache entry" (reparse).
pub fn file_fingerprint(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let nanos = mtime.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos() as u64;
    Some((nanos, meta.len()))
}

/// Load the persisted index for workspace `root`, or `None` on any miss:
/// file absent, unreadable, not valid JSON, or a `format_version` that
/// doesn't match [`CACHE_FORMAT_VERSION`]. Every branch here is a graceful
/// "cold start" fallback, never a panic — a corrupt or foreign cache file is
/// no worse than a missing one (Plan 215 acceptance: "кэш повреждён/несовместим
/// → молчаливый фолбэк на полную индексацию, не крэш").
pub fn load(root: &Path) -> Option<PersistedIndex> {
    let path = cache_file(root);
    let bytes = std::fs::read(&path).ok()?;
    let parsed: PersistedIndex = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                path = %path.display(), err = %e,
                "nova-lsp: index cache is not valid JSON — falling back to full re-index"
            );
            return None;
        }
    };
    if parsed.format_version != CACHE_FORMAT_VERSION {
        tracing::info!(
            found = parsed.format_version, expected = CACHE_FORMAT_VERSION,
            "nova-lsp: index cache format version mismatch — falling back to full re-index"
        );
        return None;
    }
    Some(parsed)
}

/// Persist `index` for workspace `root`. Best-effort: every failure (cannot
/// create the cache directory, cannot serialize, cannot write/rename) is
/// logged and swallowed — an unwritable cache degrades the *next* start back
/// to cold, it must never fail the *current* session. Written atomically
/// (temp file + rename) so a concurrent reader — or a crash mid-write — never
/// observes a half-written file, mirroring `nova-cli/src/build_cache.rs::store_c`.
pub fn save(root: &Path, index: &PersistedIndex) {
    let dir = cache_dir(root);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(dir = %dir.display(), err = %e, "nova-lsp: cannot create index cache dir");
        return;
    }
    let bytes = match serde_json::to_vec(index) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(err = %e, "nova-lsp: failed to serialize index cache");
            return;
        }
    };
    let final_path = cache_file(root);
    let tmp_path = dir.join(format!("index-v{CACHE_FORMAT_VERSION}.{}.tmp", std::process::id()));
    if let Err(e) = std::fs::write(&tmp_path, &bytes) {
        tracing::warn!(path = %tmp_path.display(), err = %e, "nova-lsp: failed to write index cache tmp file");
        let _ = std::fs::remove_file(&tmp_path);
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, &final_path) {
        tracing::warn!(path = %final_path.display(), err = %e, "nova-lsp: failed to install index cache");
        let _ = std::fs::remove_file(&tmp_path);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range, SymbolKind, Url};

    fn tmp_root(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("nova_lsp_p215_{}_{}_{}", tag, std::process::id(), nanos))
    }

    fn sample_entry() -> WorkspaceSymbolEntry {
        WorkspaceSymbolEntry {
            name: "compute".to_string(),
            kind: SymbolKind::FUNCTION,
            uri: Url::parse("file:///ws/a.nv").unwrap(),
            range: Range { start: Position { line: 0, character: 0 }, end: Position { line: 0, character: 7 } },
            container_name: None,
        }
    }

    // ── pos: round-trip ──────────────────────────────────────────────────────

    /// pos: an index written by `save` and read back by `load` reproduces the
    /// same symbols/refs/fingerprint — the basic contract the warm-start path
    /// depends on.
    #[test]
    fn pos_save_then_load_roundtrip() {
        let root = tmp_root("roundtrip");
        let mut idx = PersistedIndex::new();
        idx.files.insert(
            "file:///ws/a.nv".to_string(),
            CachedFile {
                mtime_nanos: 12345,
                size: 42,
                symbols: vec![sample_entry()],
                refs: vec![("compute".to_string(), vec![Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 7 },
                }])],
            },
        );
        save(&root, &idx);

        let loaded = load(&root).expect("must load what was just saved");
        assert_eq!(loaded.format_version, CACHE_FORMAT_VERSION);
        let entry = loaded.files.get("file:///ws/a.nv").expect("file entry present");
        assert_eq!(entry.mtime_nanos, 12345);
        assert_eq!(entry.size, 42);
        assert_eq!(entry.symbols.len(), 1);
        assert_eq!(entry.symbols[0].name, "compute");
        assert_eq!(entry.refs.len(), 1);
        assert_eq!(entry.refs[0].0, "compute");

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── pos: fingerprint sensitivity ─────────────────────────────────────────

    /// pos: `file_fingerprint` changes when file content (hence size and/or
    /// mtime) changes — the property the warm-start invalidation relies on.
    #[test]
    fn pos_fingerprint_changes_on_write() {
        let root = tmp_root("fingerprint");
        std::fs::create_dir_all(&root).unwrap();
        let f = root.join("a.nv");
        std::fs::write(&f, "fn f() => 0\n").unwrap();
        let fp1 = file_fingerprint(&f).expect("fingerprint of existing file");

        // Ensure the write below lands at a different size (mtime granularity
        // on some filesystems can coalesce two writes microseconds apart;
        // size is the fallback discriminator).
        std::fs::write(&f, "fn f() => 0\nfn g() => 1\n").unwrap();
        let fp2 = file_fingerprint(&f).expect("fingerprint after edit");

        assert_ne!(fp1, fp2, "changed file content must change the fingerprint");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// neg: `file_fingerprint` on a nonexistent path is `None` (no panic) —
    /// the caller treats this exactly like a cache miss (reparse, or skip a
    /// file that's been deleted since the directory listing).
    #[test]
    fn neg_fingerprint_missing_file_is_none() {
        let root = tmp_root("missing");
        let ghost = root.join("does_not_exist.nv");
        assert!(file_fingerprint(&ghost).is_none());
    }

    // ── neg: graceful fallback on a missing/corrupt/incompatible cache ───────

    /// neg: no cache file at all → `load` returns `None` (first-ever run).
    #[test]
    fn neg_no_cache_file_is_none() {
        let root = tmp_root("absent");
        assert!(load(&root).is_none(), "no cache file yet must be a clean miss");
    }

    /// neg: a cache file that exists but isn't valid JSON must fall back
    /// silently, not panic — Plan 215 acceptance criterion (corrupt/foreign
    /// cache → full reindex, never a crash).
    #[test]
    fn neg_corrupt_json_is_none() {
        let root = tmp_root("corrupt");
        let dir = cache_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(cache_file(&root), b"{ this is not json ][").unwrap();

        // Must not panic.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load(&root)));
        assert!(result.is_ok(), "load must not panic on corrupt JSON");
        assert!(result.unwrap().is_none(), "corrupt JSON must be treated as a cache miss");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// neg: a structurally-valid JSON document with a `format_version` from a
    /// different (older/newer) schema must be rejected, not force-parsed —
    /// otherwise a schema change could silently misinterpret old field
    /// layouts instead of safely falling back to a full reindex.
    #[test]
    fn neg_wrong_version_is_none() {
        let root = tmp_root("version");
        let dir = cache_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        // A well-formed document, but tagged with a version this binary does
        // not understand.
        let foreign = serde_json::json!({
            "format_version": CACHE_FORMAT_VERSION + 1000,
            "files": {}
        });
        std::fs::write(cache_file(&root), serde_json::to_vec(&foreign).unwrap()).unwrap();

        assert!(load(&root).is_none(), "version mismatch must be a cache miss");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// neg: an empty (zero-byte) cache file — e.g. truncated by a crash mid
    /// -write before atomic rename was introduced, or a foreign zero-length
    /// file — is a miss, not a panic.
    #[test]
    fn neg_empty_file_is_none() {
        let root = tmp_root("empty");
        let dir = cache_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(cache_file(&root), b"").unwrap();
        assert!(load(&root).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── edge: save is atomic / never leaves a torn file behind for readers ──

    /// edge: after `save`, no leftover `.tmp` file remains in the cache dir —
    /// the atomic temp+rename leaves exactly the final file.
    #[test]
    fn edge_save_leaves_no_tmp_file() {
        let root = tmp_root("atomic");
        save(&root, &PersistedIndex::new());
        let dir = cache_dir(&root);
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(entries.len(), 1, "exactly the final cache file, no tmp leftovers: {:?}",
            entries.iter().map(|e| e.file_name()).collect::<Vec<_>>());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// edge: saving twice (simulating two server sessions) overwrites cleanly
    /// — the second `load` sees the second `save`'s content, not a merge of
    /// both.
    #[test]
    fn edge_save_twice_overwrites() {
        let root = tmp_root("overwrite");
        let mut first = PersistedIndex::new();
        first.files.insert("file:///a.nv".to_string(), CachedFile {
            mtime_nanos: 1, size: 1, symbols: vec![], refs: vec![],
        });
        save(&root, &first);

        let mut second = PersistedIndex::new();
        second.files.insert("file:///b.nv".to_string(), CachedFile {
            mtime_nanos: 2, size: 2, symbols: vec![], refs: vec![],
        });
        save(&root, &second);

        let loaded = load(&root).unwrap();
        assert!(!loaded.files.contains_key("file:///a.nv"), "first save's content must not survive");
        assert!(loaded.files.contains_key("file:///b.nv"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
