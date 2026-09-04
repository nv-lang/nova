//! Plan 262 Ф.А.1-bis (registry №531) — the ONE pass sequence every entry
//! point runs before `types::check_module` / `types::check_module_with_sig_table`.
//!
//! # Why this exists
//!
//! Before this module, each entry point (`nova check`, `nova build`,
//! `nova test`, the doc-test runner, nova-lsp) re-assembled its own list of
//! "passes to run before type-check" from memory. Every one of them got it
//! slightly wrong at some point:
//!
//! - `nova-lsp/src/compiler.rs` hardcoded `include_test_peers=false` (so a
//!   `*_test.nv` file's sibling `_test.nv` peers — where its own test
//!   helpers live — never merged in) and never called [`resolve_embeds`] at
//!   all (so `embed("path")` stayed an unresolved call, undefined
//!   identifier). Nine files that `nova check` passed with `rc=0` false-
//!   reddened in the editor (registry №531).
//! - `compiler-codegen/src/doc/test_runner.rs` (the doc-test runner) was
//!   missing `alpha_rename`/`number_exprs` until a fix landed the same
//!   morning this plan was written, which silently broke the checker's
//!   `resolved_types_buf` inference channel.
//!
//! Same root cause both times: **the pass list lived in the caller's head,
//! not in one place a compiler could check.** This module is that one
//! place. Every real entry point calls [`prepare_module_for_check`] (or its
//! `_with` extension point below) instead of listing passes itself. The
//! guard `scripts/guards/check-checker-entrypoints.sh` greps for direct
//! `resolve_imports_inline`/`resolve_embeds` calls made outside this file to
//! keep it that way.
//!
//! # What is deliberately NOT in here
//!
//! - **`desugar::desugar_module`** (MapLit `[k:v]` → block-expr). This is a
//!   pre-CODEGEN pass, not a pre-CHECK pass: `check_module` understands raw
//!   `ExprKind::MapLit` directly through its own `MapLitCtx`
//!   (`types/mod.rs`) and needs no desugaring to type it. Every real caller
//!   that runs `desugar_module` (`nova build`, `nova test`'s
//!   `codegen_to_c`, nova-lsp's field-cache IDE heuristics) already calls it
//!   **after** `check_module` succeeds, immediately before codegen-shaped
//!   passes (`callnorm`/`chain_norm`) — never before. Folding it in here
//!   would move it before type-check for every caller, which is untried and
//!   out of this fix's scope.
//! - **`check_module_path`** (D78 manifest/module-name-vs-path gate). This
//!   runs against a real on-disk file **before** parsing even starts (it
//!   validates the path, not the parsed `Module`), so it has nothing to do
//!   at the point this function is called. nova-lsp deliberately never runs
//!   it: an in-progress editor buffer is not required to already sit at its
//!   final on-disk path.
//! - **`infer_effects`/`lints::lint_module`**: these run **after**
//!   `check_module` (they need `ModuleEnv`), so they are downstream of what
//!   this module prepares, not upstream.
//!
//! # Legitimate per-entry-point differences (kept as parameters, not erased)
//!
//! - `include_test_peers`: `nova check` and `nova test` pass `true`
//!   unconditionally (safe even for non-test files — it only ever *adds*
//!   `*_test.nv` siblings to the merge, see `resolve_imports_inline_ex`'s own
//!   doc). `nova build` passes `false` (production artifact — test-only
//!   helper files must not leak into the compiled binary). nova-lsp now
//!   follows `nova check`'s `true` (an editor checking a `_test.nv` file
//!   needs the same siblings `nova check` would pull in, and `true` never
//!   regresses a non-test file).
//! - `between_embed_and_rename` (see [`prepare_module_for_check_with`]):
//!   `nova build` injects `Serialize`/`Deserialize` synthesized methods
//!   between `resolve_embeds` and `alpha_rename` (synthesized bodies must
//!   share the alpha-rename uniquify pass, so they cannot run after it, and
//!   the checker's own `synthesize_method` bridge makes this build-only —
//!   `check_module` on its own is correct without it). No other caller needs
//!   this extension point.

use std::path::{Path, PathBuf};

use crate::ast::Module;
use crate::diag::Diagnostic;
use crate::imports::ModuleSigTable;
use crate::lints::LintWarning;

/// Non-mutated results of [`prepare_module_for_check`], beyond the
/// in-place-updated `module`.
#[derive(Default)]
pub struct PreparedModule {
    /// Plan 162.2 cross-module signature table. `None` only when the sig-
    /// table collection pass itself failed AND the caller cares to
    /// distinguish that from "empty" (most callers treat a failure as an
    /// empty table and use it as `Some` anyway, matching pre-existing
    /// behaviour at every call site this replaces).
    pub sig_table: Option<ModuleSigTable>,
    /// Files pulled in via `embed(...)`/`embed_dir(...)` (Plan 186/210).
    /// Callers building a content-addressed cache key fold these into the
    /// fingerprint (an embedded file's edit must invalidate the cache).
    pub embed_files: Vec<PathBuf>,
    /// Non-fatal `embed_dir` warnings (`W_EMBED_DIR_*`), success path.
    pub embed_warnings: Vec<LintWarning>,
    /// `number_exprs`'s returned seed map — only `nova build`'s codegen
    /// path needs this (merged under the checker's own `resolved_types`
    /// after `check_module` runs); every other caller discards it, same as
    /// before this module existed.
    pub resolved_types_seed: std::collections::HashMap<crate::ast::ExprId, crate::types::ResolvedType>,
}

/// Why [`prepare_module_for_check`] could not finish. Both variants carry
/// enough to render a diagnostic the way each existing call site already
/// did (`anyhow` message for import errors, a `Diagnostic` list for embed
/// errors) — this enum does not change how any caller reports failure, only
/// removes the duplicated pass sequence leading up to it.
pub enum PrepareError {
    /// `resolve_imports_inline_ex` failed (import cycle, missing peer,
    /// unreadable file).
    Import(anyhow::Error),
    /// `resolve_embeds` failed (bad `embed(...)`/`embed_dir(...)` call).
    Embed(Vec<Diagnostic>),
}

/// Run every pass a parsed `Module` needs before
/// `check_module`/`check_module_with_sig_table`, in the one order every real
/// entry point agrees on (`nova check`'s `check_one_file`,
/// `nova-cli/src/main.rs:2349-2497`, is the reference this was extracted
/// from):
///
/// 1. `resolve_imports_inline_ex` — prelude + folder-module peers merged
///    into `module` (`include_test_peers` gates `*_test.nv` siblings, see
///    module docs above for which value each caller needs).
/// 2. `collect_all_signatures` — Plan 162.2 cross-module signature table,
///    collected AFTER step 1 so it sees imported items too. Best-effort: a
///    failure degrades to an empty table (matches every pre-existing call
///    site), not a hard error.
/// 3. `resolve_embeds` — `embed("path")`/`embed_dir("dir")` → `HexBlobLit`
///    (Plan 186/210). MUST run before type-check: the checker has no `fn
///    embed` and would report every call as an undefined identifier.
/// 4. `alpha_rename` — Plan 181/D347 same-scope rebind, populates
///    `module.rebind_shadows` (needed for the R2 `E_REBIND_LIVE_CONSUME`
///    check to fire at all).
/// 5. `number_exprs` — stamps a stable `ExprId` on every expr. Without this
///    the checker's own `resolved_types_buf` inference channel is inert
///    (every id reads back `ExprId::UNSET`), silently degrading inference
///    that depends on it.
///
/// This is the common case (no extension point needed). See
/// [`prepare_module_for_check_with`] for the one caller (`nova build`) that
/// needs to run an extra pass between steps 3 and 4.
pub fn prepare_module_for_check(
    entry_path: &Path,
    module: &mut Module,
    repo: &Path,
    stdlib_dir: &Path,
    include_test_peers: bool,
) -> Result<PreparedModule, PrepareError> {
    prepare_module_for_check_with(entry_path, module, repo, stdlib_dir, include_test_peers, |_| {})
}

/// Like [`prepare_module_for_check`], but runs `between_embed_and_rename` on
/// `module` after `resolve_embeds` (step 3) and before `alpha_rename` (step
/// 4). The only known legitimate need for this is `nova build`'s
/// `Serialize`/`Deserialize` synthesized-method injection — see the module
/// docs' "Legitimate per-entry-point differences" section for why it must
/// sit at exactly this position (before alpha-rename's uniquify pass, after
/// imports/embeds are resolved). Every other caller should use
/// [`prepare_module_for_check`] instead of reaching for this directly.
pub fn prepare_module_for_check_with(
    entry_path: &Path,
    module: &mut Module,
    repo: &Path,
    stdlib_dir: &Path,
    include_test_peers: bool,
    between_embed_and_rename: impl FnOnce(&mut Module),
) -> Result<PreparedModule, PrepareError> {
    // Registry 822: a missing standard library must be reported as OUR deficit,
    // before anything else. Without this the user got `undefined identifier
    // `println`` pointing at their own correct source, and went looking for a
    // mistake in a file that had none. It is checked HERE and not inside the
    // import resolver on purpose -- resolution is designed to work with no
    // stdlib at all (see `imports::prelude_deficit_message`), and this is the
    // boundary where a real compilation begins and the absence really is wrong.
    //
    // First, and not last: when the prelude is gone every later error is a
    // consequence, so reporting any of them ahead of the cause is what created
    // the defect in the first place.
    if let Some(msg) = crate::imports::prelude_deficit_message(module, stdlib_dir, entry_path) {
        return Err(PrepareError::Import(anyhow::anyhow!(msg)));
    }

    crate::imports::resolve_imports_inline_ex(
        entry_path, module, repo, stdlib_dir, include_test_peers,
    )
    .map_err(PrepareError::Import)?;

    let sig_table = crate::imports::collect_all_signatures(entry_path, module, repo, stdlib_dir)
        .unwrap_or_else(|_| ModuleSigTable::new());

    let (embed_files, embed_warnings) = crate::embed_resolve::resolve_embeds(module, entry_path, repo)
        .map_err(PrepareError::Embed)?;

    between_embed_and_rename(module);

    crate::alpha_rename::alpha_rename(module);
    let resolved_types_seed = crate::number_exprs::number_exprs(module);

    Ok(PreparedModule {
        sig_table: Some(sig_table),
        embed_files,
        embed_warnings,
        resolved_types_seed,
    })
}
