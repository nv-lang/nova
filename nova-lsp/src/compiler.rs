//! Thin adapter between nova_codegen and the LSP server.
//!
//! Provides `check_file` and `check_workspace` — the only two entry points
//! the LSP needs.  Everything compiler-internal stays behind this boundary
//! so that API changes in nova_codegen only affect this file.
//!
//! # Panic safety
//!
//! Both functions wrap the compiler invocation in `std::panic::catch_unwind`.
//! If the compiler panics (e.g., on a pre-existing internal bug), the function
//! returns a synthetic `InternalError` diagnostic instead of crashing the server.
//!
//! # Stack size
//!
//! nova_codegen's recursive passes (type-checker, SCC inference) need a large
//! stack on Windows.  Callers must run `check_file` / `check_workspace` inside
//! `tokio::task::spawn_blocking` **and** use the large-stack wrapper
//! `run_with_large_stack`.

use std::path::{Path, PathBuf};

use nova_codegen::ast::Module;
use nova_codegen::diag::{Diagnostic, Span};
use nova_codegen::imports::ModuleSigTable;
use nova_codegen::manifest::resolve_std_path;
use nova_codegen::test_runner::find_repo_root_from;
use tower_lsp::lsp_types::Url;

use crate::perf::PerfTimer;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// One file's worth of diagnostics, tagged with the originating URI.
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// The file that produced these diagnostics.
    pub file_uri: Url,
    /// Zero or more compiler diagnostics for that file.
    pub diagnostics: Vec<Diagnostic>,
    /// Full source text at the time of the check (used for span → LSP range
    /// conversion in diagnostic_mapping).
    pub source: String,
}

/// Check a single file given its URI and current text content.
///
/// Returns one `CheckResult` for the file.  Parse errors and type errors are
/// both accumulated; the result is never empty (may contain zero diagnostics
/// on a clean file).
///
/// Panics inside the compiler are caught and returned as a single
/// `InternalError` diagnostic so the server stays up.
pub fn check_file(uri: &Url, text: &str) -> CheckResult {
    check_file_with_root(uri, text, None)
}

/// Like [`check_file`], but with an explicit LSP workspace root (from
/// `initialize`).
///
/// The root is used as a **degraded-mode fallback** ([M-104.10-degraded-cu-red]):
/// when the file has no ancestor `nova.toml` (or is an unsaved/untitled buffer
/// with no path at all), the resolver still needs *somewhere* to find the
/// stdlib for prelude injection and sibling folder-module peers. Passing the
/// workspace root lets `print`/`Vec` and peer symbols resolve instead of
/// false-reddening.
pub fn check_file_with_root(uri: &Url, text: &str, workspace_root: Option<&Path>) -> CheckResult {
    let source = text.to_string();
    let source_clone = source.clone();
    let path = uri.to_file_path().ok();
    let root = workspace_root.map(|p| p.to_path_buf());
    let t = PerfTimer::start("check_file");
    let diagnostics = run_with_large_stack(move || {
        check_source(&source_clone, path.as_deref(), root.as_deref())
    });
    t.finish();
    CheckResult { file_uri: uri.clone(), diagnostics, source }
}

/// Check all `.nv` files under `workspace_root`.
///
/// Returns one `CheckResult` per file found.  Files that cannot be read are
/// skipped with a warning log.  The workspace root itself is not checked (it
/// is not a `.nv` file).
///
/// V1 strategy: **full workspace recheck** — every file is re-parsed and
/// type-checked independently.  Per-module incremental dep-graph is V2.
pub fn check_workspace(workspace_root: &Path) -> Vec<CheckResult> {
    let t = PerfTimer::start("check_workspace");
    let nv_files = collect_nv_files(workspace_root);
    tracing::debug!(files = nv_files.len(), root = %workspace_root.display(), "workspace scan");
    let mut results = Vec::with_capacity(nv_files.len());

    for path in nv_files {
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(path = %path.display(), err = %e, "failed to read .nv file; skipping");
                continue;
            }
        };

        let uri = match path_to_uri(&path) {
            Some(u) => u,
            None => {
                tracing::warn!(path = %path.display(), "failed to convert path to URI; skipping");
                continue;
            }
        };

        let source_clone = source.clone();
        let path_clone = path.clone();
        let root_clone = workspace_root.to_path_buf();
        let diagnostics = run_with_large_stack(move || {
            check_source(&source_clone, Some(&path_clone), Some(&root_clone))
        });
        results.push(CheckResult { file_uri: uri, diagnostics, source });
    }

    t.finish();
    results
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Run the compiler passes on `src`, returning any diagnostics.
///
/// Wraps the whole pipeline in `catch_unwind`; on panic returns a synthetic
/// `InternalError` diagnostic.
fn check_source(src: &str, path: Option<&Path>, workspace_root: Option<&Path>) -> Vec<Diagnostic> {
    // Wrap in AssertUnwindSafe because Diagnostic / Module are not UnwindSafe.
    // This is acceptable: we only read the panic value (discarded) and return
    // a fixed synthetic diagnostic — we never re-use any poisoned state.
    let src_owned = src.to_string();
    let path_owned = path.map(|p| p.to_path_buf());
    let root_owned = workspace_root.map(|p| p.to_path_buf());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check_source_inner(&src_owned, path_owned.as_deref(), root_owned.as_deref())
    }));

    match result {
        Ok(diags) => diags,
        Err(payload) => {
            let msg = panic_message(&payload);
            tracing::error!(panic = %msg, "compiler panicked during check; returning InternalError");
            vec![Diagnostic::new(
                format!("nova-lsp: internal compiler error — {msg}"),
                Span::new(0, 0),
            )]
        }
    }
}

/// The actual parse + type-check pipeline (no panic catching).
///
/// Public so that `rename.rs` can use it for the atomic post-rename check.
///
/// The pipeline mirrors `nova check` (`cmd_check`) exactly so LSP diagnostics
/// are byte-parity with the CLI ([M-104.10-lsp-cmd-check-drift]):
///
/// 1. parse
/// 2. resolve imports (prelude + folder-module peers merged into the module)
///    **and** collect the cross-module signature table (Plan 162.2); import
///    errors are surfaced, not swallowed ([M-104.10-import-diag-swallowed]),
///    and degraded contexts fall back to a best-effort root
///    ([M-104.10-degraded-cu-red])
/// 3. `alpha_rename` (Plan 181/D347) over the fully-assembled module — same as
///    `nova check`, so `module.rebind_shadows` is populated and R2
///    `E_REBIND_LIVE_CONSUME` fires in the IDE (without it R2 early-returns →
///    the diagnostic would never appear in the editor)
/// 4. `number_exprs` over the fully-assembled module (post-inline, pre-check)
/// 5. `check_module_with_sig_table` (162.2 suppression) — identical to
///    `nova check`, so transitively-imported symbols do not false-red.
pub fn check_source_inner(
    src: &str,
    path: Option<&Path>,
    workspace_root: Option<&Path>,
) -> Vec<Diagnostic> {
    // Step 1: parse
    let mut module = match nova_codegen::parser::parse(src) {
        Ok(m) => m,
        Err(diag) => return vec![diag],
    };

    // Step 2: resolve imports + collect the signature table. Import errors go
    // into `import_diags` (surfaced first as the real root cause).
    let mut import_diags: Vec<Diagnostic> = Vec::new();
    let sig_table = resolve_for_check(path, workspace_root, &mut module, &mut import_diags);

    // Step 3: same-scope re-binding alpha-rename (Plan 181/D347) — parity with
    // `cmd_check` (nova-cli/src/main.rs:2125). Populates `module.rebind_shadows`
    // so the consume-checker fires R2 `E_REBIND_LIVE_CONSUME` in the IDE (it
    // early-returns on an empty map) and B1's distinct obligation keys agree
    // with the CLI. No-op for a module without a same-scope rebind.
    nova_codegen::alpha_rename::alpha_rename(&mut module);

    // Step 4: number every expr of the fully-assembled module (post-inline,
    // pre-check) — parity with `cmd_check` / `test_runner` so the checker sees
    // the same ExprId-stamped AST.
    let _ = nova_codegen::number_exprs::number_exprs(&mut module);

    // Step 4: type-check. Use the signature table when available (Plan 162.2)
    // so symbols from transitively-imported modules are not reported as unknown.
    let check_result = match sig_table {
        Some(st) => nova_codegen::types::check_module_with_sig_table(&module, st),
        None => nova_codegen::types::check_module(&module),
    };
    let mut type_diags = check_result.err().unwrap_or_default();

    // Import errors (real root cause) precede type errors so the user sees the
    // actual reason instead of downstream "unknown type" noise.
    import_diags.append(&mut type_diags);
    import_diags
}

/// Resolve imports and collect the Plan 162.2 signature table for `module`,
/// pushing any import-resolution error into `import_diags`.
///
/// Returns the signature table when a resolution context could be established
/// (so the caller uses `check_module_with_sig_table`), or `None` for a truly
/// context-less single-file check (untitled buffer with no workspace root).
///
/// # Degraded-mode fallback ([M-104.10-degraded-cu-red])
///
/// The repo root is chosen best-effort:
///  1. nearest ancestor `nova.toml` (authoritative — identical to `cmd_check`);
///  2. the LSP workspace root (from `initialize`);
///  3. the entry file's own directory (folder-module fallback — resolves
///     sibling peers even for a file outside any repo).
///
/// This guarantees `module.peer_files` is populated (prelude + peers) instead
/// of leaving the checker to see only the entry file, which previously made
/// prelude symbols (`print`/`Vec`) and peer symbols false-red.
fn resolve_for_check(
    path: Option<&Path>,
    workspace_root: Option<&Path>,
    module: &mut Module,
    import_diags: &mut Vec<Diagnostic>,
) -> Option<ModuleSigTable> {
    // The entry path we resolve peers/prelude against. For an unsaved/untitled
    // buffer (no path), use a scratch path inside the workspace root so the
    // resolver can still locate the stdlib for prelude injection.
    let entry: PathBuf = match path {
        Some(p) => p.to_path_buf(),
        None => workspace_root?.join("__nova_lsp_unsaved__.nv"),
    };

    let repo: PathBuf = find_repo_root_from(&entry)
        .or_else(|| workspace_root.map(|r| r.to_path_buf()))
        .or_else(|| entry.parent().map(|p| p.to_path_buf()))?;
    let stdlib_dir = resolve_std_path(&repo);

    // Merge prelude + folder-module peers into `module` (populates peer_files).
    // Guarded so a malformed peer cannot crash the whole check (matches the
    // resolve guard in `provenance.rs`).
    let resolve_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nova_codegen::imports::resolve_imports_inline(&entry, module, &repo, &stdlib_dir)
    }));
    match resolve_res {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            // [M-104.10-import-diag-swallowed]: surface the real import cause
            // (cycle / missing / unreadable peer) instead of discarding it, so
            // the user sees why — not downstream "unknown type" noise.
            import_diags.push(Diagnostic::new(
                format!("import resolution: {e}"),
                Span::new(0, 0),
            ));
        }
        Err(_) => {
            import_diags.push(Diagnostic::new(
                "import resolution panicked".to_string(),
                Span::new(0, 0),
            ));
        }
    }

    // Plan 162.2: cross-module signature table, collected AFTER imports are
    // merged so it sees all imported items. Non-fatal on failure (empty table).
    let sig_table = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nova_codegen::imports::collect_all_signatures(&entry, module, &repo, &stdlib_dir)
            .unwrap_or_else(|_| ModuleSigTable::new())
    }))
    .unwrap_or_else(|_| ModuleSigTable::new());

    Some(sig_table)
}

/// Collect all `.nv` files recursively under `root`.
fn collect_nv_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_nv_files_rec(root, &mut files);
    files
}

fn collect_nv_files_rec(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(dir = %dir.display(), err = %e, "cannot read dir");
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip target/ and hidden dirs to avoid scanning build artefacts.
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_nv_files_rec(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("nv") {
            out.push(path);
        }
    }
}

/// Convert a filesystem path to a `file://` URI.
fn path_to_uri(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

/// Extract a human-readable message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Run `f` on a new thread with a 64 MiB stack.
///
/// nova_codegen's recursive passes blow the default Windows stack (1 MiB).
/// We spawn a dedicated thread rather than relying on tokio's `spawn_blocking`
/// threadpool stack size, which is platform-default.
///
/// **Must be called from within `tokio::task::spawn_blocking`** (the spawned
/// thread is synchronous and will block until `f` completes).
pub fn run_with_large_stack<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name("nova-check".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn nova-check thread")
        .join()
        .unwrap_or_else(|_| panic!("nova-check thread panicked (already caught above)"))
}
