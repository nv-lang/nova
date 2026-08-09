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
/// **Cost warning (Plan 213 Ф.1 diagnosis):** this is an `O(workspace size)`
/// operation — every `.nv` file under `root` is independently re-parsed +
/// import-resolved + type-checked, with no cross-file caching between calls.
/// On the main Nova repo this is 3000+ files (std/examples/spec_tests/
/// nova_tests). It is appropriate for the **one-time** cold startup scan
/// (`run_initial_scan_with_progress`) and for tests against small temp
/// workspaces, but must never be called on the interactive per-edit path —
/// `schedule_recheck` uses [`check_open_documents`] instead, which is
/// `O(open buffers)`. See docs/plans/213-nova-lsp-performance.md.
///
/// Each file is checked on the *same* thread as the caller (no per-file
/// thread spawn — Plan 213 Ф.1 found the previous per-file
/// `run_with_large_stack` call here spawned one 64 MiB-stack OS thread per
/// file, i.e. 3000+ thread creations per invocation, on top of the O(n) parse
/// cost). Callers that need a large stack (recursive compiler passes) must
/// wrap the *whole* `check_workspace` call in `run_with_large_stack`, as
/// `run_initial_scan_with_progress` already does.
pub fn check_workspace(workspace_root: &Path) -> Vec<CheckResult> {
    let t = PerfTimer::start("check_workspace");
    let nv_files = collect_nv_paths(workspace_root);
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

        let diagnostics = check_source(&source, Some(&path), Some(workspace_root));
        results.push(CheckResult { file_uri: uri, diagnostics, source });
    }

    t.finish();
    results
}

/// Check only `docs` (a caller-supplied set of `(uri, text)` pairs), each
/// resolved against `workspace_root` for prelude/peer/nova.toml lookup — Plan
/// 213 Ф.2's replacement for the interactive per-edit recheck path.
///
/// Previously `schedule_recheck` called [`check_workspace`] (the entire
/// repository — 3000+ files) on **every** debounced edit to **any** open
/// document. This is the incremental fix: only currently-open buffers are
/// rechecked, each via the same per-file resolution machinery
/// (`check_source` → nearest `nova.toml` / folder-module peers / prelude) used
/// by [`check_file_with_root`]. Cost is `O(open documents)`, typically single
/// digits, instead of `O(workspace size)`.
///
/// A real per-module dependency graph (rechecking only the edited file's
/// module + its reverse-dependents) is deferred — see
/// `[M-104.10-dependent-invalidation]` in `state.rs` and
/// docs/plans/213-nova-lsp-performance.md Ф.4. Rechecking *all* open buffers
/// on every edit is the documented interim step endorsed by that plan.
pub fn check_open_documents(docs: &[(Url, String)], workspace_root: &Path) -> Vec<CheckResult> {
    let t = PerfTimer::start("check_open_documents");
    let mut results = Vec::with_capacity(docs.len());
    for (uri, source) in docs {
        let path = uri.to_file_path().ok();
        let diagnostics = check_source(source, path.as_deref(), Some(workspace_root));
        results.push(CheckResult {
            file_uri: uri.clone(),
            diagnostics,
            source: source.clone(),
        });
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
/// 2. `nova_codegen::check_pipeline::prepare_module_for_check` (Plan 262
///    Ф.А.1-bis, registry №531) — resolve imports **with `*_test.nv` peers**
///    + collect the cross-module signature table (Plan 162.2) + resolve
///    `embed(...)`/`embed_dir(...)` + `alpha_rename` (Plan 181/D347) +
///    `number_exprs`, in the one order `nova check` runs them. Import/embed
///    errors are surfaced, not swallowed ([M-104.10-import-diag-swallowed]),
///    and degraded contexts fall back to a best-effort root
///    ([M-104.10-degraded-cu-red]).
/// 3. `check_module_with_sig_table` (162.2 suppression) — identical to
///    `nova check`, so transitively-imported symbols do not false-red.
///
/// Before this used [`prepare_module_for_check`], this function called
/// `resolve_imports_inline` (hardcoded `include_test_peers=false`) and never
/// called `resolve_embeds` at all — two of the passes `nova check` runs were
/// silently missing, so files whose test helpers live in a sibling
/// `*_test.nv` peer (`undefined identifier` for those helpers) or that use
/// `embed(...)` (`undefined identifier embed`) false-reddened in the editor
/// while `nova check` passed them with `rc=0` (registry №531). Both passes
/// now come from the same function `nova check` uses, so the two pipelines
/// cannot drift apart pass-by-pass again — a guard
/// (`scripts/guards/check-checker-entrypoints.sh`) greps for direct
/// `resolve_imports_inline`/`resolve_embeds` calls made outside
/// `check_pipeline.rs` to keep it that way.
///
/// [`prepare_module_for_check`]: nova_codegen::check_pipeline::prepare_module_for_check
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

    // Step 2: prepare (resolve imports + sig-table + embeds + alpha_rename +
    // number_exprs) — see the pipeline doc above. Errors go into
    // `import_diags` (surfaced first as the real root cause).
    let mut import_diags: Vec<Diagnostic> = Vec::new();
    let sig_table = resolve_for_check(path, workspace_root, &mut module, &mut import_diags);

    // Step 3: type-check. Use the signature table when available (Plan 162.2)
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
/// pushing any import/embed-resolution error into `import_diags`.
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
///
/// `include_test_peers=true` is passed unconditionally to
/// `prepare_module_for_check` — same choice `nova check` makes
/// (nova-cli/src/main.rs, [M-tls-cert-modes-test-undefined-helpers]): it can
/// only ever *add* `*_test.nv` siblings to the merged compile unit, so it is
/// safe for a non-test file too and is what makes a `_test.nv` file's own
/// sibling test-helper peers resolve in the editor (registry №531).
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

    // Guarded so a malformed peer / embed / cycle cannot crash the whole
    // check (matches the resolve guard `provenance.rs` uses for its own,
    // narrower import-only call).
    let prepare_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nova_codegen::check_pipeline::prepare_module_for_check(
            &entry, module, &repo, &stdlib_dir, /* include_test_peers */ true,
        )
    }));

    match prepare_res {
        Ok(Ok(prepared)) => {
            for w in &prepared.embed_warnings {
                import_diags.push(w.diag.clone());
            }
            prepared.sig_table
        }
        Ok(Err(nova_codegen::check_pipeline::PrepareError::Import(e))) => {
            // [M-104.10-import-diag-swallowed]: surface the real import cause
            // (cycle / missing / unreadable peer) instead of discarding it, so
            // the user sees why — not downstream "unknown type" noise.
            import_diags.push(Diagnostic::new(
                format!("import resolution: {e}"),
                Span::new(0, 0),
            ));
            None
        }
        Ok(Err(nova_codegen::check_pipeline::PrepareError::Embed(diags))) => {
            import_diags.extend(diags);
            None
        }
        Err(_) => {
            import_diags.push(Diagnostic::new(
                "import resolution panicked".to_string(),
                Span::new(0, 0),
            ));
            None
        }
    }
}

/// Directory names that are never part of a Nova module graph: build output
/// and vendored third-party trees. Skipping them by name (in addition to the
/// dotdir skip below) avoids walking — and `read_dir`-syscalling — tens of
/// thousands of unrelated files on every workspace scan (Plan 213 Ф.1
/// diagnosis: `compiler-codegen/vcpkg_installed/` alone is a large vendored
/// include/lib tree with zero `.nv` files but a deep directory structure).
const SKIP_DIR_NAMES: &[&str] = &[
    "target",
    "target_alt",
    "target_test",
    "vcpkg_installed",
    "node_modules",
];

/// Collect all `.nv` files recursively under `root`, shared by every LSP
/// entry point that needs a workspace-wide file list (`check_workspace`,
/// `symbols::collect_nv_files`, rename's `collect_nv_files_for_rename`) —
/// Plan 213 Ф.1/Ф.2 consolidated three near-duplicate walkers into this one so
/// the filtering below is applied everywhere consistently.
///
/// Filters, beyond the `.nv` extension check:
/// - dot-directories (`.git`, `.vscode`, …) and [`SKIP_DIR_NAMES`] are never
///   descended into;
/// - a subdirectory that itself contains a `.git` entry (dir or worktree
///   pointer file) is treated as a **distinct repository root** and is not
///   descended into, even though it is reachable under the open workspace
///   folder. This matters for a "fleet" setup where several Nova `git
///   worktree` checkouts are opened as sibling folders and the owner
///   sometimes opens their common parent directory in the editor — without
///   this guard, `d:/…/nova-lspfix`, `d:/…/nova-206`, etc. would each be
///   walked and fully rechecked as if they were part of *this* workspace's
///   module graph (`[M-213-nested-worktree-scan]`).
pub fn collect_nv_paths(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_nv_paths_rec(root, root, &mut files);
    files
}

fn collect_nv_paths_rec(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
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
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || SKIP_DIR_NAMES.contains(&name) {
                continue;
            }
            // Repository-boundary guard: never descend into another nested
            // git root (see doc comment above). The top-level `root` itself
            // is exempt (it commonly *is* a git checkout).
            if path != root && path.join(".git").exists() {
                tracing::debug!(dir = %path.display(), "skipping nested repository root");
                continue;
            }
            collect_nv_paths_rec(&path, root, out);
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
///
/// Plan 213 Ф.2: the spawned thread's OS scheduling priority is lowered
/// (best-effort, Windows-only — no-op elsewhere) so background type-checking
/// never competes with the editor's own UI thread for CPU time when the
/// machine is under load. This is a background aide, not the foreground
/// workload.
pub fn run_with_large_stack<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name("nova-check".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            worker_priority::lower_current_thread_priority();
            f()
        })
        .expect("spawn nova-check thread")
        .join()
        .unwrap_or_else(|_| panic!("nova-check thread panicked (already caught above)"))
}

/// Best-effort OS thread-priority lowering for `nova-check` worker threads
/// (Plan 213 Ф.2). Windows-only real implementation (kernel32 is always
/// linked on Windows targets — no new crate dependency needed); a no-op stub
/// on other platforms keeps the call site portable.
#[cfg(windows)]
mod worker_priority {
    // Minimal raw FFI surface — avoids pulling in a full `windows`/`winapi`
    // dependency for two calls.
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> isize;
        fn SetThreadPriority(h_thread: isize, n_priority: i32) -> i32;
    }

    /// `THREAD_PRIORITY_BELOW_NORMAL` (winbase.h) — one step below the
    /// process's normal priority class, enough to yield to the editor's own
    /// threads under contention without starving the check entirely.
    const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;

    pub fn lower_current_thread_priority() {
        // Safety: both calls are simple, argument-free (besides the returned
        // handle) Win32 APIs; failure is silently ignored (best-effort — a
        // check that runs at normal priority is still correct, just not as
        // considerate of the foreground UI).
        unsafe {
            let handle = GetCurrentThread();
            SetThreadPriority(handle, THREAD_PRIORITY_BELOW_NORMAL);
        }
    }
}

#[cfg(not(windows))]
mod worker_priority {
    pub fn lower_current_thread_priority() {}
}
