//! Multi-file workspace recheck tests (Plan 104.1.Ф.6).
//!
//! These tests use `check_workspace` directly (unit-style) rather than
//! spawning the LSP binary, since workspace root tracking requires
//! a full e2e process test which is covered by publish_workflow.rs.

use nova_lsp::compiler::check_workspace;
use std::fs;

// ─────────────────────────────────────────────────────────────────────────────
// helpers
// ─────────────────────────────────────────────────────────────────────────────

fn valid_nv(module_name: &str) -> String {
    format!("module {module_name}\n\nfn add(a int, b int) -> int => a + b\n")
}

fn broken_nv(module_name: &str) -> String {
    format!("module {module_name}\n\nfn bad( => ()\n")
}

// ─────────────────────────────────────────────────────────────────────────────
// pos tests
// ─────────────────────────────────────────────────────────────────────────────

/// pos1: 2-file workspace → results contain both files.
#[test]
fn pos1_two_file_workspace_returns_two_results() {
    let dir = tempfile::tempdir().expect("create temp dir");
    fs::write(dir.path().join("a.nv"), valid_nv("ws.a")).unwrap();
    fs::write(dir.path().join("b.nv"), valid_nv("ws.b")).unwrap();

    let results = check_workspace(dir.path());
    assert_eq!(results.len(), 2, "expected 2 results for 2 .nv files");
}

/// pos2: 5-file workspace → all 5 files checked, one broken → 1 has errors.
#[test]
fn pos2_five_file_workspace_one_broken() {
    let dir = tempfile::tempdir().expect("create temp dir");
    for i in 0..4 {
        fs::write(dir.path().join(format!("ok{i}.nv")), valid_nv(&format!("ws.ok{i}"))).unwrap();
    }
    fs::write(dir.path().join("broken.nv"), broken_nv("ws.broken")).unwrap();

    let results = check_workspace(dir.path());
    assert_eq!(results.len(), 5, "expected 5 results");

    let error_count = results.iter().filter(|r| !r.diagnostics.is_empty()).count();
    assert_eq!(error_count, 1, "expected exactly 1 file with errors");
}

/// pos3: subdirectory .nv files are scanned recursively.
#[test]
fn pos3_recursive_scan_finds_subdir_files() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("deep.nv"), valid_nv("ws.sub.deep")).unwrap();
    fs::write(dir.path().join("root.nv"), valid_nv("ws.root")).unwrap();

    let results = check_workspace(dir.path());
    assert_eq!(results.len(), 2, "expected 2 results (root + sub)");
}

/// pos4: target/ directory is skipped (build artifacts ignored).
#[test]
fn pos4_target_dir_skipped() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let target = dir.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("artifact.nv"), "this is not real Nova").unwrap();
    fs::write(dir.path().join("main.nv"), valid_nv("ws.main")).unwrap();

    let results = check_workspace(dir.path());
    // Should only find main.nv, not target/artifact.nv
    assert_eq!(results.len(), 1, "target/ dir should be skipped");
}

// ─────────────────────────────────────────────────────────────────────────────
// neg tests
// ─────────────────────────────────────────────────────────────────────────────

/// neg1: non-existent workspace root → empty results, no panic.
#[test]
fn neg1_nonexistent_root_returns_empty() {
    let results = check_workspace(std::path::Path::new("/nonexistent/path/that/does/not/exist"));
    assert!(results.is_empty(), "non-existent root should return empty results");
}

// ─────────────────────────────────────────────────────────────────────────────
// edge tests
// ─────────────────────────────────────────────────────────────────────────────

/// edge1: empty workspace → 0 results.
#[test]
fn edge1_empty_workspace_zero_results() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let results = check_workspace(dir.path());
    assert!(results.is_empty(), "empty workspace should return 0 results");
}

/// edge2: workspace with only non-.nv files → 0 results.
#[test]
fn edge2_no_nv_files_zero_results() {
    let dir = tempfile::tempdir().expect("create temp dir");
    fs::write(dir.path().join("README.md"), "# docs").unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    let results = check_workspace(dir.path());
    assert!(results.is_empty(), "non-.nv files should be ignored");
}

/// edge3: .nv file that cannot be read (no permissions) → skipped, no panic.
///
/// This test only makes sense on non-Windows or if we can deny read access.
/// On Windows, file permissions are more complex; we skip this test there.
#[test]
#[cfg(unix)]
fn edge3_unreadable_file_skipped_no_panic() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("create temp dir");
    let unreadable = dir.path().join("unreadable.nv");
    fs::write(&unreadable, valid_nv("ws.unreadable")).unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
    fs::write(dir.path().join("readable.nv"), valid_nv("ws.readable")).unwrap();

    let results = check_workspace(dir.path());
    // Unreadable file skipped; readable file returned
    assert_eq!(results.len(), 1, "unreadable file should be skipped");

    // Restore permissions for cleanup
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();
}
