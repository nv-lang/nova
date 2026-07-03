//! Plan 104.10 Ф.18 — workspace lifecycle integration tests.
//!
//! Exercises the real cache/index invalidation performed on external file
//! events (`workspace/didChangeWatchedFiles`) against a live `WorkspaceState`,
//! plus the `willRenameFiles` import-path rewrite end-to-end over on-disk files.
//! These assert the acceptance criterion "real invalidation, not restart the
//! LSP": a peer file changing on disk updates the index/diagnostics without the
//! editor buffer being touched, and a deletion leaves no dangling entries.

use std::fs;

use nova_lsp::state::WorkspaceState;
use nova_lsp::workspace_lifecycle::{
    apply_watched_event, classify_watch_uri, compute_rename_import_edits, RenamedFile,
    WatchTarget,
};
use tower_lsp::lsp_types::{FileChangeType, FileEvent, Url};

fn uri_of(path: &std::path::Path) -> Url {
    Url::from_file_path(path).expect("valid file path")
}

// ── POS: external peer-file change updates the workspace symbol index ─────────

/// POS: an external change to a peer file (not the open buffer) re-indexes its
/// symbols so `workspace/symbol` sees the new declaration — no buffer edit.
#[test]
fn pos_external_change_updates_index() {
    let dir = tempfile::tempdir().unwrap();
    let peer = dir.path().join("peer.nv");
    fs::write(&peer, "module app.peer\nfn original_fn() => ()\n").unwrap();
    let peer_uri = uri_of(&peer);

    let state = WorkspaceState::default();
    // Prime the index from the initial content.
    state
        .workspace_index
        .index_file(peer_uri.clone(), &fs::read_to_string(&peer).unwrap());
    assert_eq!(
        state.workspace_index.search("original_fn", 10).len(),
        1,
        "index primed with original symbol"
    );

    // Simulate an EXTERNAL edit (git pull / codegen): the file on disk now
    // declares a different symbol. The buffer was never opened in the editor.
    fs::write(&peer, "module app.peer\nfn renamed_fn() => ()\n").unwrap();
    let outcome = apply_watched_event(
        &state,
        &FileEvent::new(peer_uri.clone(), FileChangeType::CHANGED),
    );
    assert!(outcome.relevant, "a .nv change is a relevant event");

    // The index reflects the disk state without any buffer edit.
    assert_eq!(
        state.workspace_index.search("original_fn", 10).len(),
        0,
        "stale symbol gone after external change"
    );
    assert_eq!(
        state.workspace_index.search("renamed_fn", 10).len(),
        1,
        "new symbol indexed from disk after external change"
    );
}

// ── EDGE: deletion removes all traces ────────────────────────────────────────

/// EDGE: a delete event purges the symbol-index entry — no dangling references.
#[test]
fn edge_delete_removes_index_entries() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("gone.nv");
    fs::write(&f, "module app.gone\nfn doomed() => ()\n").unwrap();
    let f_uri = uri_of(&f);

    let state = WorkspaceState::default();
    state
        .workspace_index
        .index_file(f_uri.clone(), &fs::read_to_string(&f).unwrap());
    assert_eq!(state.workspace_index.search("doomed", 10).len(), 1);

    // The file is deleted on disk; the client forwards a DELETED event.
    fs::remove_file(&f).unwrap();
    let outcome =
        apply_watched_event(&state, &FileEvent::new(f_uri.clone(), FileChangeType::DELETED));
    assert!(outcome.relevant);
    assert_eq!(
        state.workspace_index.search("doomed", 10).len(),
        0,
        "deleted file's symbols must be gone (no dangling entries)"
    );
    assert_eq!(state.workspace_index.file_count(), 0, "no file entries remain");
}

// ── Resolved-cache invalidation on external change ───────────────────────────

/// POS: a `.nv` change invalidates the resolved-module cache entry for that
/// file so the next IDE request rebuilds against fresh content.
#[test]
fn pos_change_invalidates_resolved_entry() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("mod.nv");
    let src = "module app.mod\nfn f() => ()\n";
    fs::write(&f, src).unwrap();
    let f_uri = uri_of(&f);

    let state = WorkspaceState::default();
    let _ = state.get_or_build_resolved(&f_uri, 1, src);
    assert!(
        state.resolved_cache.contains_key(&f_uri),
        "resolved entry cached after build"
    );

    apply_watched_event(&state, &FileEvent::new(f_uri.clone(), FileChangeType::CHANGED));
    assert!(
        !state.resolved_cache.contains_key(&f_uri),
        "external change must evict the resolved-cache entry"
    );
}

// ── NEG: non-.nv watch event is ignored ──────────────────────────────────────

/// NEG: a watch event on a non-`.nv` file changes nothing.
#[test]
fn neg_non_nv_event_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let readme = dir.path().join("README.md");
    fs::write(&readme, "# docs\n").unwrap();
    let readme_uri = uri_of(&readme);
    assert_eq!(classify_watch_uri(&readme_uri), WatchTarget::Ignore);

    let state = WorkspaceState::default();
    let outcome = apply_watched_event(
        &state,
        &FileEvent::new(readme_uri, FileChangeType::CHANGED),
    );
    assert!(!outcome.relevant, "non-.nv event must be irrelevant (ignored)");
    assert_eq!(state.workspace_index.file_count(), 0);
}

// ── nova.toml manifest change clears search-path + resolved caches ────────────

/// A `nova.toml` change clears the resolved caches (dependency/search-path may
/// have changed) and is flagged as a manifest change.
#[test]
fn manifest_change_clears_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.nv");
    let src = "module app.a\nfn f() => ()\n";
    fs::write(&f, src).unwrap();
    let f_uri = uri_of(&f);
    let toml_uri = uri_of(&dir.path().join("nova.toml"));

    let state = WorkspaceState::default();
    let _ = state.get_or_build_resolved(&f_uri, 1, src);
    assert!(state.resolved_cache.contains_key(&f_uri));

    let outcome = apply_watched_event(
        &state,
        &FileEvent::new(toml_uri, FileChangeType::CHANGED),
    );
    assert!(outcome.manifest_changed, "nova.toml flagged as manifest change");
    assert!(
        state.resolved_cache.is_empty(),
        "manifest change clears all resolved builds"
    );
}

// ── willRename end-to-end over on-disk importers ─────────────────────────────

/// POS: renaming `foo.nv` → `bar.nv` produces a WorkspaceEdit updating the
/// `import app.foo` in a real importer file to `import app.bar`.
#[test]
fn pos_will_rename_updates_importer_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    fs::create_dir(&app).unwrap();
    let foo = app.join("foo.nv");
    let main = app.join("main.nv");
    fs::write(&foo, "module app.foo\nfn helper() => ()\n").unwrap();
    fs::write(&main, "module app.main\nimport app.foo\nfn main() => ()\n").unwrap();

    let old_uri = uri_of(&foo);
    let new_uri = uri_of(&app.join("bar.nv"));
    let main_uri = uri_of(&main);

    let renames = vec![RenamedFile {
        old_uri,
        new_uri,
        old_text: fs::read_to_string(&foo).unwrap(),
    }];
    let files = vec![(main_uri.clone(), fs::read_to_string(&main).unwrap())];

    let edit = compute_rename_import_edits(&renames, &files).expect("edit expected");
    let changes = edit.changes.expect("changes present");
    let edits = changes.get(&main_uri).expect("importer edited");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "bar");
}
