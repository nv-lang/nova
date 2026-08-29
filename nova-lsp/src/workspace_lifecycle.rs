//! Workspace lifecycle — Plan 104.10 Ф.18 (D-block, Ред.2).
//!
//! Pure, unit-testable core of the four workspace-lifecycle sub-features wired
//! into `server.rs`:
//!
//! 1. **`workspace/didChangeWatchedFiles`** — react to *external* file changes
//!    (git checkout/pull, edits outside the editor, codegen output). The server
//!    registers watchers for `**/*.nv` and `**/nova.toml` via dynamic
//!    `client/registerCapability`; each incoming event is classified here
//!    ([`classify_watch_uri`]) and applied to the caches ([`apply_watched_event`]).
//!    Without this, the Ф.1 resolved-module cache and the Ф.12 workspace symbol
//!    index silently go stale after any change the editor did not originate.
//!
//! 2. **`workspace/willRenameFiles`** — when a `.nv` file is renamed, Nova's
//!    imports are *path-based* (`import a.b.foo` loads `a/b/foo.nv`), so a rename
//!    changes which import path resolves to the file. [`compute_rename_import_edits`]
//!    returns a `WorkspaceEdit` rewriting the affected `import` paths in every
//!    dependent file — using the real parsed AST import spans, not a blind regex.
//!
//! 3. **`$/progress`** — the cold initial workspace scan is wrapped in a work-done
//!    progress token (begin/report/end) so the IDE shows a spinner instead of
//!    appearing hung. (Wired in `server.rs`; this module holds the classification
//!    and cache logic only.)
//!
//! 4. **`{semanticTokens,codeLens}/refresh`** — after a background reindex the
//!    server pushes refresh notifications so already-rendered hints re-pull fresh
//!    data. (Wired in `server.rs`.)
//!
//! # Scope-out markers
//!
//! - `[M-104.10-file-rename-imports]` — the import-path rewrite matches an import
//!   whose *final segment* equals the renamed file's old stem AND whose dotted
//!   path is a suffix of the file's on-disk path segments. This resolves the
//!   common single-file-module rename precisely without running the full import
//!   resolver. It intentionally does **not** cover: folder-module *peer* renames
//!   (the folder — hence the module identity — is unchanged, so no edit is
//!   correct), renames that collide on a shared leaf name across two unrelated
//!   directories (both are rewritten — genuinely ambiguous without the resolver),
//!   and `as`-alias / selective-`{…}` re-spelling. Full resolver-verified path
//!   matching is deferred.
//! - `[M-104.10-watch-reverse-deps]` — a `.nv` watch event invalidates the
//!   changed file's own resolved-cache entry *and* every other open document's
//!   entry (a correct superset of the reverse-dependency set: any open doc might
//!   import the changed file). This never leaves a stale cache; it is coarser
//!   than a precise module-graph reverse-dep walk, which is deferred.

use std::collections::HashMap;
use std::path::PathBuf;

use ropey::Rope;
use tower_lsp::lsp_types::{
    FileChangeType, FileEvent, TextEdit, Url, WorkspaceEdit,
};

use crate::diagnostic_mapping::span_to_range;
use crate::state::WorkspaceState;

// ─────────────────────────────────────────────────────────────────────────────
// (1) didChangeWatchedFiles — event classification + cache application
// ─────────────────────────────────────────────────────────────────────────────

/// What a watched-file URI refers to, for dispatch in the handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchTarget {
    /// A Nova source file (`*.nv`) — index/diagnostics/resolved-cache relevant.
    Nv,
    /// The `nova.toml` manifest — search paths / dependencies may have changed.
    NovaToml,
    /// Anything else — ignored (NEG: a watch event on a non-`.nv` file).
    Ignore,
}

/// Classify a watched-file URI. Only `*.nv` files and `nova.toml` are relevant;
/// everything else is [`WatchTarget::Ignore`] (the watchers are scoped to those
/// two globs, but a client may still forward broader events — we filter
/// defensively rather than trust the glob).
pub fn classify_watch_uri(uri: &Url) -> WatchTarget {
    let path = match uri.to_file_path() {
        Ok(p) => p,
        // Non-file URIs (untitled:, etc.) are never watched workspace files.
        Err(_) => return WatchTarget::Ignore,
    };
    match path.file_name().and_then(|n| n.to_str()) {
        Some("nova.toml") => WatchTarget::NovaToml,
        _ => {
            if path.extension().and_then(|e| e.to_str()) == Some("nv") {
                WatchTarget::Nv
            } else {
                WatchTarget::Ignore
            }
        }
    }
}

/// Outcome of applying a single watched-file event, returned so the handler can
/// decide whether a re-check / refresh is warranted and log precisely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WatchApplyOutcome {
    /// The event touched Nova state (index/resolved cache) — a recheck+refresh
    /// is worthwhile.
    pub relevant: bool,
    /// The manifest changed — search-path/stdlib caches were cleared.
    pub manifest_changed: bool,
}

/// Apply one external file event to the shared workspace caches — the real
/// invalidation that keeps the server from serving stale data after an
/// out-of-editor change (git pull, codegen, manual edit).
///
/// Behaviour by [`WatchTarget`]:
/// - **`Nv` created/changed:** if the file is **not** an open document (an open
///   buffer is the source of truth and is maintained by `did_change`), re-index
///   its symbols from disk and drop its resolved-cache entry. Reverse-dependent
///   resolved entries are invalidated by [`WorkspaceState::invalidate_all_resolved`]
///   in the caller (see `[M-104.10-watch-reverse-deps]`).
/// - **`Nv` deleted:** remove its symbol-index entry, resolved-cache entry, and
///   document-symbol cache entry — no dangling references remain (EDGE).
/// - **`NovaToml`:** clear the stdlib/search-path index cache and all resolved
///   caches (dependency graph may have changed).
/// - **`Ignore`:** no-op (NEG).
pub fn apply_watched_event(state: &WorkspaceState, event: &FileEvent) -> WatchApplyOutcome {
    match classify_watch_uri(&event.uri) {
        WatchTarget::Ignore => WatchApplyOutcome::default(),
        WatchTarget::NovaToml => {
            // Manifest change: search paths / dependencies may differ. Drop the
            // per-directory stdlib index and every resolved module so the next
            // request rebuilds against the new configuration.
            state.stdlib_index_cache.clear();
            state.invalidate_all_resolved();
            WatchApplyOutcome { relevant: true, manifest_changed: true }
        }
        WatchTarget::Nv => {
            let is_open = state.docs.contains_key(&event.uri);
            if event.typ == FileChangeType::DELETED {
                // Purge every trace of the deleted file (Ф.12: references
                // occurrences too — EDGE deleted file → entries removed).
                state.workspace_index.remove_file(&event.uri);
                state.references_index.remove_file(&event.uri);
                state.invalidate_resolved(&event.uri);
                state.document_symbol_cache.invalidate(&event.uri);
                return WatchApplyOutcome { relevant: true, manifest_changed: false };
            }
            // Created / Changed. An open buffer already owns the latest text
            // (maintained incrementally by did_change); re-indexing from disk
            // would clobber unsaved edits, so we skip the disk read for open
            // docs but still invalidate the resolved cache below.
            if !is_open {
                if let Ok(path) = event.uri.to_file_path() {
                    if let Ok(src) = std::fs::read_to_string(&path) {
                        state.workspace_index.index_file(event.uri.clone(), &src);
                        // Ф.12: keep the references index fresh on external edits.
                        state.references_index.index_file(event.uri.clone(), &src);
                    }
                }
            }
            state.invalidate_resolved(&event.uri);
            state.document_symbol_cache.invalidate(&event.uri);
            WatchApplyOutcome { relevant: true, manifest_changed: false }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// (2) willRenameFiles — path-based import rewrite
// ─────────────────────────────────────────────────────────────────────────────

/// One `.nv` file the client is about to rename, with its pre-rename content
/// (taken from the open buffer if any, else read from disk) so we can parse its
/// module structure before the file moves.
pub struct RenamedFile {
    /// URI of the file at its original location.
    pub old_uri: Url,
    /// URI of the file at its new location.
    pub new_uri: Url,
    /// The file's text at the old location (buffer overlay or disk).
    pub old_text: String,
}

/// Compute the `WorkspaceEdit` that keeps `import` paths valid across a batch of
/// `.nv` file renames.
///
/// For each renamed file we derive its old/new *stem* (filename without `.nv`).
/// A dependent file's `import` is rewritten iff its final path segment equals the
/// old stem **and** its dotted path is a suffix of the renamed file's on-disk
/// path segments — i.e. the import genuinely resolves to this file by Nova's
/// path-based module rule. The edit replaces only that final segment, preserving
/// the anchor (`./`, `../`), any `as`-alias, and selective `{…}` lists.
///
/// Returns `None` when nothing needs changing (so the handler can answer `null`).
/// See `[M-104.10-file-rename-imports]` for the precise scope boundary.
pub fn compute_rename_import_edits(
    renames: &[RenamedFile],
    workspace_files: &[(Url, String)],
) -> Option<WorkspaceEdit> {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

    for rn in renames {
        let (Some(old_stem), Some(new_stem)) = (uri_stem(&rn.old_uri), uri_stem(&rn.new_uri))
        else {
            continue;
        };
        // Same leaf name (e.g. moved between directories) → no import-path leaf
        // rewrite. Directory-move import fixups are out of scope (marker).
        if old_stem == new_stem {
            continue;
        }
        let file_segments = uri_path_segments(&rn.old_uri);

        for (uri, text) in workspace_files {
            // A file never rewrites its own imports for its own rename.
            if uri == &rn.old_uri {
                continue;
            }
            let module = match crate::compiler::parse_guarded(text) {
                Ok(m) => m,
                Err(_) => continue, // un-parseable importer → skip gracefully
            };
            let rope = Rope::from_str(text);
            for imp in &module.imports {
                if imp.path.last().map(String::as_str) != Some(old_stem.as_str()) {
                    continue;
                }
                if !path_is_suffix(&imp.path, &file_segments) {
                    continue;
                }
                if let Some((s, e)) =
                    last_segment_byte_range(text, imp.span.start, imp.span.end, &old_stem)
                {
                    let range = span_to_range(&rope, s, e);
                    changes
                        .entry(uri.clone())
                        .or_default()
                        .push(TextEdit { range, new_text: new_stem.clone() });
                }
            }
        }
    }

    if changes.is_empty() {
        return None;
    }
    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

/// The filename stem (without the `.nv` extension) of a `file://` URI.
fn uri_stem(uri: &Url) -> Option<String> {
    let path = uri.to_file_path().ok()?;
    path.file_stem().and_then(|s| s.to_str()).map(str::to_string)
}

/// The path components of a `file://` URI with the final component's extension
/// stripped — e.g. `/home/u/app/foo.nv` → `["home","u","app","foo"]`. Used to
/// verify an import path resolves to this file by suffix match (root-agnostic).
fn uri_path_segments(uri: &Url) -> Vec<String> {
    let Ok(path) = uri.to_file_path() else { return Vec::new() };
    let mut segs: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(str::to_string),
            _ => None,
        })
        .collect();
    if let Some(last) = segs.last_mut() {
        if let Some(stem) = PathBuf::from(&*last)
            .file_stem()
            .and_then(|s| s.to_str())
        {
            *last = stem.to_string();
        }
    }
    segs
}

/// True iff `needle` is a (non-empty) suffix of `haystack` (segment-wise). An
/// `import a.b.foo` for a file `…/a/b/foo.nv` has segments `[a,b,foo]` which is a
/// suffix of the file's path components — the check that keeps us from rewriting
/// a same-leaf import in an unrelated directory.
fn path_is_suffix(needle: &[String], haystack: &[String]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    let tail = &haystack[haystack.len() - needle.len()..];
    tail == needle
}

/// Locate the byte range of the *final* path segment (`stem`) inside an import
/// statement spanning `[span_start, span_end)` in `src`. Scans only the path
/// region — up to the first `{`, newline, or ` as ` — and returns the last
/// whole-word occurrence of `stem` there, so `import a.b.foo`, `import a.b.foo
/// as x`, and `import a.b.foo.{X}` all target the `foo` segment.
fn last_segment_byte_range(
    src: &str,
    span_start: usize,
    span_end: usize,
    stem: &str,
) -> Option<(usize, usize)> {
    let end = span_end.min(src.len());
    if span_start >= end {
        return None;
    }
    let region = &src[span_start..end];

    // Trim the region at the first `{` (selective list), newline, or ` as `
    // (alias) — the path proper never extends past these.
    let mut region_end = region.len();
    if let Some(i) = region.find('{') {
        region_end = region_end.min(i);
    }
    if let Some(i) = region.find('\n') {
        region_end = region_end.min(i);
    }
    if let Some(i) = region.find('\r') {
        region_end = region_end.min(i);
    }
    if let Some(i) = region.find(" as ") {
        region_end = region_end.min(i);
    }
    let path_region = &region[..region_end];

    // Find the last whole-word occurrence of `stem`.
    let bytes = path_region.as_bytes();
    let sb = stem.as_bytes();
    if sb.is_empty() {
        return None;
    }
    let mut found: Option<usize> = None;
    let mut i = 0usize;
    while i + sb.len() <= bytes.len() {
        if &bytes[i..i + sb.len()] == sb {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_idx = i + sb.len();
            let after_ok = after_idx >= bytes.len() || !is_ident_byte(bytes[after_idx]);
            if before_ok && after_ok {
                found = Some(i);
            }
        }
        i += 1;
    }
    let rel = found?;
    Some((span_start + rel, span_start + rel + sb.len()))
}

/// True for bytes that may appear inside a path-segment identifier.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    fn uri(p: &str) -> Url {
        // Absolute path so `to_file_path` succeeds on both platforms.
        #[cfg(windows)]
        let full = format!("file:///C:/{}", p);
        #[cfg(not(windows))]
        let full = format!("file:///{}", p);
        Url::parse(&full).unwrap()
    }

    // ── classify (NEG on non-.nv) ─────────────────────────────────────────────

    #[test]
    fn classify_nv_toml_and_ignore() {
        assert_eq!(classify_watch_uri(&uri("ws/app/foo.nv")), WatchTarget::Nv);
        assert_eq!(classify_watch_uri(&uri("ws/nova.toml")), WatchTarget::NovaToml);
        // NEG: a non-.nv file (readme, target artifact) is ignored.
        assert_eq!(classify_watch_uri(&uri("ws/README.md")), WatchTarget::Ignore);
        assert_eq!(classify_watch_uri(&uri("ws/app/foo.txt")), WatchTarget::Ignore);
        assert_eq!(classify_watch_uri(&uri("ws/foo.nvx")), WatchTarget::Ignore);
    }

    // ── willRename: import-path rewrite ───────────────────────────────────────

    #[test]
    fn rename_rewrites_importer_last_segment() {
        // File app/foo.nv renamed → app/bar.nv; importer writes `import app.foo`.
        let old_uri = uri("proj/app/foo.nv");
        let new_uri = uri("proj/app/bar.nv");
        let importer_uri = uri("proj/app/main.nv");
        let importer_src = "module app.main\nimport app.foo\nfn main() => ()\n";

        let renames = vec![RenamedFile {
            old_uri: old_uri.clone(),
            new_uri,
            old_text: "module app.foo\nfn f() => ()\n".to_string(),
        }];
        let files = vec![(importer_uri.clone(), importer_src.to_string())];

        let edit = compute_rename_import_edits(&renames, &files).expect("expected an edit");
        let changes = edit.changes.expect("changes present");
        let edits = changes.get(&importer_uri).expect("importer edited");
        assert_eq!(edits.len(), 1, "exactly one import segment rewritten");
        assert_eq!(edits[0].new_text, "bar");
        // The edited range must cover exactly `foo` on line 1 (0-indexed).
        let te = &edits[0];
        assert_eq!(te.range.start.line, 1);
        // `import app.` is 11 chars → `foo` starts at column 11.
        assert_eq!(te.range.start, Position { line: 1, character: 11 });
        assert_eq!(te.range.end, Position { line: 1, character: 14 });
    }

    #[test]
    fn rename_preserves_alias_and_selective() {
        let old_uri = uri("proj/app/foo.nv");
        let new_uri = uri("proj/app/bar.nv");
        let importer_uri = uri("proj/app/main.nv");
        // `as` alias and selective list must not confuse segment targeting.
        let src = "module app.main\nimport app.foo as fee\nimport app.foo.{A, B}\n";

        let renames = vec![RenamedFile {
            old_uri,
            new_uri,
            old_text: "module app.foo\n".to_string(),
        }];
        let files = vec![(importer_uri.clone(), src.to_string())];
        let edit = compute_rename_import_edits(&renames, &files).unwrap();
        let edits = edit.changes.unwrap().remove(&importer_uri).unwrap();
        assert_eq!(edits.len(), 2, "both imports of foo rewritten");
        for te in &edits {
            assert_eq!(te.new_text, "bar");
        }
    }

    #[test]
    fn rename_no_match_in_unrelated_dir_returns_none() {
        // Importer references `other.foo`, but the renamed file lives under
        // `app/`, so the path-suffix guard rejects the rewrite.
        let old_uri = uri("proj/app/foo.nv");
        let new_uri = uri("proj/app/bar.nv");
        let importer_uri = uri("proj/app/main.nv");
        let src = "module app.main\nimport other.foo\n";

        let renames = vec![RenamedFile {
            old_uri,
            new_uri,
            old_text: "module app.foo\n".to_string(),
        }];
        let files = vec![(importer_uri, src.to_string())];
        assert!(
            compute_rename_import_edits(&renames, &files).is_none(),
            "unrelated same-leaf import must NOT be rewritten"
        );
    }

    #[test]
    fn rename_same_stem_is_noop() {
        // A pure directory move (stem unchanged) yields no import-leaf edit.
        let old_uri = uri("proj/app/foo.nv");
        let new_uri = uri("proj/lib/foo.nv");
        let importer_uri = uri("proj/app/main.nv");
        let src = "module app.main\nimport app.foo\n";
        let renames = vec![RenamedFile {
            old_uri,
            new_uri,
            old_text: "module app.foo\n".to_string(),
        }];
        let files = vec![(importer_uri, src.to_string())];
        assert!(compute_rename_import_edits(&renames, &files).is_none());
    }

    #[test]
    fn last_segment_range_handles_relative_and_multiseg() {
        // Single-segment relative import `import foo`.
        let src = "import foo\n";
        let (s, e) = last_segment_byte_range(src, 0, src.len(), "foo").unwrap();
        assert_eq!(&src[s..e], "foo");
        // Multi-segment: pick the LAST `foo` (the leaf), not an earlier one.
        let src2 = "import foo.bar.foo\n";
        let (s2, e2) = last_segment_byte_range(src2, 0, src2.len(), "foo").unwrap();
        assert_eq!(&src2[s2..e2], "foo");
        assert_eq!(s2, "import foo.bar.".len(), "leaf foo, not the first one");
    }

    #[test]
    fn path_suffix_semantics() {
        let hay = vec!["a".into(), "b".into(), "foo".into()];
        assert!(path_is_suffix(&["foo".to_string()], &hay));
        assert!(path_is_suffix(&["b".to_string(), "foo".to_string()], &hay));
        assert!(path_is_suffix(&["a".to_string(), "b".to_string(), "foo".to_string()], &hay));
        assert!(!path_is_suffix(&["x".to_string(), "foo".to_string()], &hay));
        assert!(!path_is_suffix(&[], &hay));
    }
}
