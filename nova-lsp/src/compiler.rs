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

use nova_codegen::diag::{Diagnostic, Span};
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
    let source = text.to_string();
    let source_clone = source.clone();
    let t = PerfTimer::start("check_file");
    let diagnostics = run_with_large_stack(move || check_source(&source_clone));
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
        let diagnostics = run_with_large_stack(move || check_source(&source_clone));
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
fn check_source(src: &str) -> Vec<Diagnostic> {
    // Wrap in AssertUnwindSafe because Diagnostic / Module are not UnwindSafe.
    // This is acceptable: we only read the panic value (discarded) and return
    // a fixed synthetic diagnostic — we never re-use any poisoned state.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check_source_inner(src)
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
fn check_source_inner(src: &str) -> Vec<Diagnostic> {
    // Step 1: parse
    let module = match nova_codegen::parser::parse(src) {
        Ok(m) => m,
        Err(diag) => return vec![diag],
    };

    // Step 2: type-check
    match nova_codegen::types::check_module(&module) {
        Ok(_) => vec![],
        Err(diags) => diags,
    }
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
