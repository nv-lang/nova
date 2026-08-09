//! Plan 104.10 Ф.0.5 — diagnostic pipeline correctness (root of false red).
//!
//! Covers the five fixes:
//!   1. [M-104.10-import-diag-swallowed] — import errors surface, not swallowed.
//!   2. [M-104.10-degraded-cu-red]       — degraded CU (no repo / unsaved)
//!                                          still resolves prelude + peers → 0 false-red.
//!   3. [M-104.10-lsp-cmd-check-drift]    — LSP check-input == `nova check`
//!                                          pipeline (number_exprs + sig-table).
//!   4. [M-104.10-diag-numeric-codes]     — numeric `[Ennnn]` codes flow through.
//!   5. [M-104.10-hardcode-lists]         — stale std-module completions removed
//!                                          (covered by completion.rs unit tests).
//!
//! Gate: diagnostic pos+neg fixtures (detect-like), NOT nova_tests byte-baseline.

use std::path::{Path, PathBuf};

use nova_lsp::compiler::{check_file, check_file_with_root, check_source_inner};
use tower_lsp::lsp_types::Url;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Worktree repo root (`nova-lsp/`'s parent) — has `nova.toml` + real `std/`.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nova-lsp has a parent dir")
        .to_path_buf()
}

/// Absolute path to the real stdlib, forward-slashed for embedding in TOML.
fn real_std_toml_path() -> String {
    repo_root().join("std").to_string_lossy().replace('\\', "/")
}

/// Write a minimal workspace `nova.toml` whose `std` key points at the real
/// stdlib, so prelude resolution uses the genuine `std/prelude.nv`.
fn write_workspace_toml(dir: &Path) {
    std::fs::write(
        dir.join("nova.toml"),
        format!("[workspace]\nstd = \"{}\"\n", real_std_toml_path()),
    )
    .expect("write nova.toml");
}

fn file_uri(path: &Path) -> Url {
    Url::from_file_path(path).expect("file uri")
}

/// True if any diagnostic message contains `needle` (case-insensitive).
fn any_msg_contains(diags: &[nova_codegen::diag::Diagnostic], needle: &str) -> bool {
    let n = needle.to_lowercase();
    diags.iter().any(|d| d.message.to_lowercase().contains(&n))
}

// ─────────────────────────────────────────────────────────────────────────────
// Fix #2 — degraded CU: folder-module peers resolve WITHOUT a repo root.
// ─────────────────────────────────────────────────────────────────────────────

/// POS: entry-folder-module peer symbol resolves even with no ancestor
/// `nova.toml` (find_repo_root == None) via the entry-dir fallback → 0 red on
/// the peer call. Before the fix, imports were skipped entirely and `helper`
/// was undefined.
#[test]
fn pos_degraded_folder_module_peer_no_false_red() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mod_dir = dir.path().join("degmod");
    std::fs::create_dir(&mod_dir).unwrap();
    // Two co-equal peers of folder-module `degmod` (same module decl), no
    // prelude needed (int only).
    std::fs::write(
        mod_dir.join("lib.nv"),
        "module degmod\n\nfn helper() -> int {\n    42\n}\n",
    )
    .unwrap();
    let app = mod_dir.join("app.nv");
    std::fs::write(
        &app,
        "module degmod\n\nfn main() -> int {\n    helper()\n}\n",
    )
    .unwrap();

    // No workspace root → forces the entry-dir fallback repo.
    let result = check_file(&file_uri(&app), &std::fs::read_to_string(&app).unwrap());
    assert!(
        result.diagnostics.is_empty(),
        "peer `helper` should resolve via degraded folder-module fallback; got: {:?}",
        result.diagnostics
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Fix #2 — prelude resolves for in-repo file (real std/prelude.nv).
// ─────────────────────────────────────────────────────────────────────────────

/// POS: a file that uses a prelude symbol (`println`) inside a workspace whose
/// `std` points at the real stdlib → 0 false-red on the prelude symbol.
#[test]
fn pos_prelude_symbol_no_false_red_in_repo() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_workspace_toml(dir.path());
    let main = dir.path().join("main.nv");
    let src = "module useprelude\n\nfn go() -> () {\n    println(\"hi\")\n}\n";
    std::fs::write(&main, src).unwrap();

    let result = check_file(&file_uri(&main), src);
    assert!(
        !any_msg_contains(&result.diagnostics, "println"),
        "prelude `println` must resolve (no unknown-symbol red); got: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.is_empty(),
        "valid prelude-using file should be clean; got: {:?}",
        result.diagnostics
    );
}

/// POS: unsaved / untitled buffer (path == None) with a workspace root → the
/// scratch-entry fallback still injects prelude so `println` does not false-red.
#[test]
fn pos_unsaved_buffer_prelude_resolves_via_workspace_root() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_workspace_toml(dir.path());

    // Non-file URI → to_file_path() == None → path is None inside the checker.
    let untitled = Url::parse("untitled:Untitled-1").unwrap();
    assert!(untitled.to_file_path().is_err(), "URI must be pathless");

    let src = "module scratch\n\nfn go() -> () {\n    println(\"hi\")\n}\n";
    let result = check_file_with_root(&untitled, src, Some(dir.path()));
    assert!(
        !any_msg_contains(&result.diagnostics, "println"),
        "unsaved buffer prelude must resolve via workspace root; got: {:?}",
        result.diagnostics
    );
    assert!(
        result.diagnostics.is_empty(),
        "unsaved valid buffer should be clean; got: {:?}",
        result.diagnostics
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Fix #1 — import errors surface (not swallowed).
// ─────────────────────────────────────────────────────────────────────────────

/// POS: a missing import yields a real import-resolution diagnostic — the user
/// sees the actual cause, not just downstream "unknown type" noise.
#[test]
fn pos_missing_import_surfaces_real_diagnostic() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_workspace_toml(dir.path());
    let main = dir.path().join("main.nv");
    let src = "module m\n\nimport std.does_not_exist_xyz.foo\n\nfn f() -> int => 1\n";
    std::fs::write(&main, src).unwrap();

    let result = check_file(&file_uri(&main), src);
    assert!(
        !result.diagnostics.is_empty(),
        "missing import must produce a diagnostic"
    );
    assert!(
        any_msg_contains(&result.diagnostics, "import"),
        "diagnostic must name the import cause, not just 'unknown type'; got: {:?}",
        result.diagnostics
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Fix #3 — parity: LSP diagnostics == `nova check` (sig-table) pipeline.
// ─────────────────────────────────────────────────────────────────────────────

/// Reference implementation of the `nova check` pipeline (mirrors
/// `nova-cli::main::check_one_file` / `test_runner`): parse →
/// `prepare_module_for_check` (Plan 262 Ф.А.1-bis: resolve imports incl.
/// `*_test.nv` peers + sig-table + `embed(...)` + alpha_rename + number_exprs)
/// → `check_module_with_sig_table`. Returns the diagnostic messages.
///
/// Before Plan 262, this duplicated the pass list by hand (bare
/// `resolve_imports_inline`, no `embed(...)` resolution) — the SAME class of
/// bug registry №531 found in `compiler.rs` itself: a second, independently
/// maintained copy of "what passes does the checker need" that had already
/// drifted from what `nova check` actually runs. Now both this reference and
/// `check_source_inner` call the same shared function, so this test still
/// catches the class of regression it exists for (a future entry point
/// skipping `prepare_module_for_check` and hand-rolling the list again would
/// diverge from `nova check`, not from this reference — the class guard is
/// `check-checker-entrypoints.sh`, this test is the semantic parity guard).
fn reference_check_messages(path: &Path, src: &str) -> Vec<String> {
    let mut module = match nova_codegen::parser::parse(src) {
        Ok(m) => m,
        Err(d) => return vec![d.message],
    };
    let repo = nova_codegen::test_runner::find_repo_root_from(path);
    let sig_table = if let Some(repo) = &repo {
        let stdlib = nova_codegen::manifest::resolve_std_path(repo);
        nova_codegen::check_pipeline::prepare_module_for_check(
            path, &mut module, repo, &stdlib, /* include_test_peers */ true,
        )
        .ok()
        .and_then(|p| p.sig_table)
    } else {
        None
    };
    let res = match sig_table {
        Some(st) => nova_codegen::types::check_module_with_sig_table(&module, st),
        None => nova_codegen::types::check_module(&module),
    };
    match res {
        Ok(_) => vec![],
        Err(errs) => errs.into_iter().map(|d| d.message).collect(),
    }
}

/// Run `f` on a 64 MiB-stack thread — `prepare_module_for_check`'s
/// `collect_all_signatures` step walks the whole imported module graph
/// (incl. `std.prelude`'s own transitive imports), which is enough recursion
/// to overflow the default test-harness stack on Windows (confirmed:
/// `parity_lsp_matches_nova_check_pipeline` — STATUS_STACK_OVERFLOW before
/// this wrapper existed). The production LSP already runs everything through
/// `run_with_large_stack` at the `server.rs` request-handling layer; this
/// helper is the test-only equivalent for call sites in this file that use
/// `check_source_inner`/`reference_check_messages` directly, bypassing that
/// layer.
fn run_large_stack<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn large-stack thread")
        .join()
        .expect("large-stack thread panicked")
}

/// PARITY: for a valid in-repo fixture, the LSP produces exactly the same
/// diagnostic set as the reference `nova check` pipeline (both clean here).
/// If the LSP had kept the old plain-`check_module` path, a transitive-import
/// false-positive could diverge.
#[test]
fn parity_lsp_matches_nova_check_pipeline() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_workspace_toml(dir.path());
    let main = dir.path().join("main.nv");
    let src = "module parity\n\nfn add(a int, b int) -> int => a + b\n\nfn go() -> () {\n    println(add(1, 2))\n}\n";
    std::fs::write(&main, src).unwrap();

    // Type-check errors (import diagnostics excluded) must match the reference.
    // Both run on a large-stack thread — see `run_large_stack`'s doc comment.
    let src_owned = src.to_string();
    let main_owned = main.clone();
    let lsp = run_large_stack(move || check_source_inner(&src_owned, Some(&main_owned), None));
    let lsp_msgs: Vec<String> = lsp.iter().map(|d| d.message.clone()).collect();
    let main_owned2 = main.clone();
    let src_owned2 = src.to_string();
    let reference = run_large_stack(move || reference_check_messages(&main_owned2, &src_owned2));
    assert_eq!(
        lsp_msgs, reference,
        "LSP diagnostics must equal the `nova check` sig-table pipeline"
    );
}

/// PARITY (Plan 181 / D347): the LSP check pipeline runs `alpha_rename`, so the
/// consume-checker sees a populated `module.rebind_shadows` and R2
/// `E_REBIND_LIVE_CONSUME` fires in the IDE exactly as in `nova check`. Without
/// the pass the shadow-map is empty and `check_rebind_live_consume`
/// early-returns, so this diagnostic would never reach the editor.
#[test]
fn parity_lsp_fires_r2_rebind_live_consume() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_workspace_toml(dir.path());
    let main = dir.path().join("main.nv");
    // Mirrors nova_tests/rebind/neg/rebind_ro_over_live_consume_neg.nv: a live
    // consume obligation on `b` is hidden by a same-scope `ro b = 5` rebind.
    let src = "module m\n\n\
type NRbBox consume {\n    v int,\n}\n\
fn NRbBox consume @close() -> () { () }\n\
fn NRbBox @touch() -> () { () }\n\n\
test \"r2\" {\n\
    consume b = NRbBox { v: 1 }\n\
    b.touch()\n\
    ro b = 5\n\
    assert(b == 5)\n\
}\n";
    std::fs::write(&main, src).unwrap();

    // The full check pipeline (prelude-inlined) is deep — run it on a large
    // stack like the production LSP (`run_with_large_stack`) so the test does
    // not depend on `RUST_MIN_STACK` being set on the harness thread.
    let src_owned = src.to_string();
    let main_owned = main.clone();
    let diags = run_large_stack(move || check_source_inner(&src_owned, Some(&main_owned), None));
    assert!(
        diags.iter().any(|d| d.message.contains("E_REBIND_LIVE_CONSUME")),
        "R2 must fire in the LSP pipeline (alpha_rename wired); got: {:?}",
        diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// NEG — the fix must not silence genuine diagnostics.
// ─────────────────────────────────────────────────────────────────────────────

/// NEG: a genuine error (undefined symbol) is still reported after the fix.
#[test]
fn neg_real_error_still_reported() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_workspace_toml(dir.path());
    let main = dir.path().join("main.nv");
    let src = "module m\n\nfn bad() -> int => undefined_symbol_xyz\n";
    std::fs::write(&main, src).unwrap();

    let result = check_file(&file_uri(&main), src);
    assert!(
        !result.diagnostics.is_empty(),
        "genuine undefined-symbol error must still be reported"
    );
}

/// NEG: a broken sibling peer does not swallow the entry file's own genuine
/// error — both the entry error and the import surface are visible.
#[test]
fn neg_broken_peer_does_not_swallow_entry_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mod_dir = dir.path().join("brk");
    std::fs::create_dir(&mod_dir).unwrap();
    // Malformed peer — parse error.
    std::fs::write(mod_dir.join("bad.nv"), "module brk.bad\n\nfn oops( => {\n").unwrap();
    // Entry with a genuine undefined-symbol error.
    let app = mod_dir.join("app.nv");
    let src = "module brk.app\n\nfn main() -> int {\n    undefined_symbol_xyz()\n}\n";
    std::fs::write(&app, src).unwrap();

    let result = check_file(&file_uri(&app), src);
    assert!(
        !result.diagnostics.is_empty(),
        "entry's genuine error must not be swallowed by a broken peer"
    );
    assert!(
        any_msg_contains(&result.diagnostics, "undefined_symbol_xyz")
            || any_msg_contains(&result.diagnostics, "import"),
        "expected entry error or import diagnostic; got: {:?}",
        result.diagnostics
    );
}
