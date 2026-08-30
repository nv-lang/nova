//! Source provenance — Plan 104.10 Ф.0 (D378).
//!
//! Restores the `file_id → path` mapping that the compiler already builds while
//! resolving imports, but which nova-lsp previously discarded. This is the
//! foundation for cross-file goto-definition (Ф.3), cross-file hover (Ф.4),
//! type-driven completion (Ф.5) and rename (Ф.7): once a symbol's declaration
//! `Span` carries a `file_id`, we can map that `file_id` back to the concrete
//! file that declared it and emit a `Location` in the *right* document.
//!
//! **Provenance is real, not textual.** The map is built by walking
//! `Module.peer_files` (populated by `resolve_imports_inline`), where each
//! `PeerFile` carries `(file_id, path)`. We never re-scan files by name to
//! guess where a symbol came from.
//!
//! # Entry-file `file_id` duality
//!
//! The entry buffer is parsed via `parser::parse(src)`, which stamps every span
//! with `MAIN_FILE_ID` (= 0). `resolve_imports_inline` then registers the entry
//! as a `PeerFile` — also with `MAIN_FILE_ID` (see `imports.rs:608`). So today
//! both agree on `0`. To stay robust if that ever changes (Q-104-5), we map
//! *both* `MAIN_FILE_ID` and whatever `file_id` the entry `PeerFile` reports to
//! the entry path — both point at the same file, so this is always safe.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nova_codegen::ast::Module;
use nova_codegen::diag::{FileId, Span, MAIN_FILE_ID};
use nova_codegen::types::ModuleEnv;
use ropey::Rope;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::diagnostic_mapping::byte_offset_to_position;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// A parsed + import-resolved module together with the provenance needed to
/// attribute spans back to their source files.
pub struct ResolvedModule {
    /// The module after `resolve_imports_inline` (imported items PREPENDED).
    pub module: Module,
    /// Index into `module.items` where the entry file's own items begin.
    /// Items in `[0, items_start)` were prepended by import inlining.
    pub items_start: usize,
    /// `file_id → path`, built by walking `module.peer_files`. Always contains
    /// at least the entry file (mapped from both `MAIN_FILE_ID` and the entry
    /// `PeerFile`'s reported id).
    pub file_map: HashMap<FileId, PathBuf>,
    /// Type-checker environment for the entry module. `None` if checking
    /// failed. Ф.1/Ф.2 enrich this (cache / `expr_types`).
    pub env: Option<ModuleEnv>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Parse `src`, inline its imports, and build the `file_id → path` provenance
/// map by walking the resulting `peer_files`.
///
/// Robust to failure at every step:
/// - Parse error → `module` has no items, `file_map` maps the entry only,
///   `env` is `None`. Never panics.
/// - Import resolution panic → caught (`catch_unwind`, as in `hover.rs`); the
///   entry mapping is still present.
///
/// `path` is the on-disk location of the entry buffer (used both as the entry
/// provenance target and to locate the repo root / stdlib for import resolution).
///
/// The `env` is built with the plain `check_module` (no per-expression type
/// recording). For IDE requests that need `expr_types` (hover/type-driven
/// completion, Ф.2), use [`resolve_module_for_ide`] instead.
pub fn resolve_module_for(path: &Path, src: &str) -> ResolvedModule {
    resolve_module_impl(path, src, /* record_expr_types = */ false)
}

/// Like [`resolve_module_for`], but records per-expression types in the returned
/// `env` (via `check_module_with_expr_types`, Ф.2). This is the variant the
/// Ф.1 symbol cache builds, so downstream IDE features (hover, type-driven
/// completion, typeDefinition) get a populated `ModuleEnv::expr_types`.
///
/// The extra recording is opt-in precisely so the plain compile path stays
/// zero-overhead; here we deliberately pay for it because the result is cached
/// per open document and reused across many requests.
pub fn resolve_module_for_ide(path: &Path, src: &str) -> ResolvedModule {
    resolve_module_impl(path, src, /* record_expr_types = */ true)
}

/// Shared implementation of [`resolve_module_for`] / [`resolve_module_for_ide`].
/// `record_expr_types` selects the checker entry point used to build `env`.
fn resolve_module_impl(path: &Path, src: &str, record_expr_types: bool) -> ResolvedModule {
    // Entry path, canonicalized to match how `peer_files` stores paths.
    let entry_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Parse. On failure we still return a usable (empty) module so callers
    // degrade gracefully instead of crashing.
    let mut module = match crate::compiler::parse_guarded(src) {
        Ok(m) => m,
        Err(_) => {
            let mut file_map = HashMap::new();
            file_map.insert(MAIN_FILE_ID, entry_path.clone());
            return ResolvedModule {
                module: empty_module(),
                items_start: 0,
                file_map,
                env: None,
            };
        }
    };

    // Remember how many items the file itself declares (before inlining imports).
    let items_before_inline = module.items.len();

    // Inline imports so downstream walks can find prelude/peer symbols. Errors
    // and panics are contained; on failure `peer_files` may hold only the entry
    // (or nothing), which the entry-fallback mapping below still covers.
    resolve_imports_inline_guarded(path, &mut module);

    // Imported items are PREPENDED, so the entry file's own items start after them.
    let items_start = module.items.len().saturating_sub(items_before_inline);

    // Build provenance from the REAL peer_files (not a textual re-scan).
    let mut file_map: HashMap<FileId, PathBuf> = HashMap::new();
    for pf in &module.peer_files {
        file_map.insert(pf.file_id, pf.path.clone());
    }

    // Entry duality (see module docs / Q-104-5): guarantee the entry path is
    // reachable from BOTH the parser's MAIN_FILE_ID and the entry PeerFile id.
    // `parse(src)` stamps entry spans with MAIN_FILE_ID, so this mapping must
    // exist even if `resolve_imports_inline` registered the entry under a
    // different id (or never ran at all).
    file_map.entry(MAIN_FILE_ID).or_insert_with(|| entry_path.clone());

    // Plan 181 (D347) / Plan 262 Ф.А.1-bis: same-scope re-binding alpha-rename
    // (and `number_exprs`, needed for hover/typeDefinition's expr-type lookups
    // to work at all) already ran as the last two steps inside
    // `prepare_module_for_check` (`resolve_imports_inline_guarded` above) —
    // parity with `nova check` so the checker sees the same unique-named,
    // ExprId-stamped AST (`module.rebind_shadows` populated; R2/B1 consistent
    // with the CLI). No separate call needed here.

    // Type-check the entry module for downstream type resolution (Ф.4/Ф.5).
    // Contained so a checker panic never takes down the request. When
    // `record_expr_types` is set (Ф.1 IDE cache), use the expr-type-recording
    // entry point so `env.expr_types` is populated (Ф.2).
    let env = check_module_guarded(&module, record_expr_types);

    ResolvedModule { module, items_start, file_map, env }
}

/// Map a declaration `Span` to an LSP `Location`.
///
/// - If `span.file_id` is present in `file_map` and yields a valid `file://`
///   URL → that file's URI, with the range computed in the *target* file's
///   coordinates (read from disk).
/// - Otherwise → `fallback_uri` (the current document), with a best-effort
///   range. This is the graceful-degradation path for unknown / synthetic
///   `file_id`s; it never panics.
///
/// Range coordinates are UTF-16 LSP positions. For a cross-file target the
/// source is read from disk here; the full document-cache wiring (unsaved
/// buffers, open-document overlay) lands in Ф.3.
pub fn span_to_location(span: Span, file_map: &HashMap<FileId, PathBuf>, fallback_uri: &Url) -> Location {
    // Resolve the target URI from provenance, if any.
    let target = file_map.get(&span.file_id).and_then(|p| Url::from_file_path(p).ok().map(|u| (u, p.clone())));

    match target {
        Some((uri, path)) => {
            let range = range_in_file(&path, span, fallback_uri);
            Location { uri, range }
        }
        None => {
            // Unknown file_id → point at the current document. We cannot read
            // its bytes here reliably (may be unsaved), so emit a zero-width
            // range at the span start line/col best-effort: without the source
            // we fall back to a degenerate range. Ф.3 overlays open-doc text.
            Location { uri: fallback_uri.clone(), range: degenerate_range() }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Prepare `module` for the checker, mirroring `compiler.rs::check_source_inner`:
/// resolve the repo root + stdlib from `path`, then call
/// `prepare_module_for_check` (Plan 262 Ф.А.1-bis) under `catch_unwind` so a
/// resolver/embed panic (malformed import, cycle, unreadable peer, bad
/// `embed(...)`) degrades to "entry-only" instead of crashing.
///
/// Before this used the shared function, this called the bare
/// `resolve_imports_inline` (no `*_test.nv` peers, no `embed(...)` resolution)
/// — the same gap registry №531 found in `compiler.rs`'s diagnostics path.
/// Hover/goto-definition/type-driven completion built on this `ResolvedModule`
/// could therefore show a wrong or missing type for a symbol defined in a
/// sibling `*_test.nv` peer, or for an `embed(...)` call's synthesized type.
fn resolve_imports_inline_guarded(path: &Path, module: &mut Module) {
    use nova_codegen::test_runner::find_repo_root_from;
    let Some(repo) = find_repo_root_from(path) else {
        tracing::warn!("provenance: no repo root found for {:?}", path);
        return;
    };
    let stdlib_dir = nova_codegen::manifest::resolve_std_path(repo.as_ref());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = nova_codegen::check_pipeline::prepare_module_for_check(
            path, module, &repo, &stdlib_dir, /* include_test_peers */ true,
        );
    }));
    if result.is_err() {
        tracing::warn!("provenance: prepare_module_for_check panicked for {:?}", path);
    }
}

/// Run the type checker under `catch_unwind`, returning the env on success.
/// When `record_expr_types` is set, use `check_module_with_expr_types` (Ф.2)
/// so the returned `ModuleEnv::expr_types` is populated; otherwise the plain
/// zero-overhead `check_module`.
fn check_module_guarded(module: &Module, record_expr_types: bool) -> Option<ModuleEnv> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if record_expr_types {
            // Plan 104.10 Ф.5: lenient IDE check — keep the `env` (with
            // `expr_types`) even when the buffer has type errors. Interactive
            // completion/hover fire on momentarily-invalid buffers (a dangling
            // `.`, a half-typed call); the receiver's type is still known and
            // must remain available. `Some` for any module that parsed.
            Some(nova_codegen::types::check_module_with_expr_types_ide(module))
        } else {
            nova_codegen::types::check_module(module).ok()
        }
    }));
    result.ok().flatten()
}

/// Compute the UTF-16 range of `span` inside the file at `path`, reading its
/// bytes from disk. On any read failure, degrade to a zero-width range so the
/// `Location`'s URI is still correct (better than dropping the result).
fn range_in_file(path: &Path, span: Span, _fallback_uri: &Url) -> Range {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let rope = Rope::from_str(&text);
            let start = byte_offset_to_position(&rope, span.start);
            let end = byte_offset_to_position(&rope, span.end);
            Range { start, end }
        }
        Err(_) => degenerate_range(),
    }
}

/// An empty `Module` used as the degraded value on a parse error. `Module`
/// does not derive `Default`, so we spell out its (all-empty) fields.
fn empty_module() -> Module {
    Module {
        name: Vec::new(),
        imports: Vec::new(),
        items: Vec::new(),
        attrs: Vec::new(),
        doc_attrs: Vec::new(),
        span: Span::default(),
        peer_files: Vec::new(),
        doc: None,
        // Plan 181 (D347): empty until `alpha_rename` runs — no rebind here.
        rebind_shadows: Default::default(),
        consume_reuse_spans: std::collections::HashSet::new(),
    }
}

/// A zero-width range at document start — the graceful-degradation range used
/// when the target source is unavailable.
fn degenerate_range() -> Range {
    Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 0 },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Locate the repo root so tests can resolve a real file with real imports.
    fn repo_root() -> PathBuf {
        // CARGO_MANIFEST_DIR = .../nova-lsp ; repo root is its parent.
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.parent().expect("nova-lsp has a parent").to_path_buf()
    }

    /// A tiny valid Nova source that imports the prelude transitively (every
    /// module auto-imports `std.prelude` unless `#no_prelude`).
    const SRC_WITH_IMPORT: &str = "module basics.lsp\nimport std.collections\nfn f() => ()\n";

    fn write_temp(name: &str, src: &str) -> PathBuf {
        // Place each temp file inside the repo (so `find_repo_root_from`
        // succeeds and stdlib resolution works), but in its OWN sub-directory:
        // sibling files that declare the same `module` name in one directory are
        // collected as folder-module peers, which would cross-contaminate the
        // independent test fixtures. A per-fixture dir keeps them isolated.
        let stem = name.strip_suffix(".nv").unwrap_or(name);
        let dir = repo_root().join("target").join("prov_test").join(stem);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, src).unwrap();
        path
    }

    /// Test-only wrapper: runs `resolve_module_for` on a large-stack thread.
    ///
    /// Plan 262 Ф.А.1-bis: `resolve_module_for` now goes through
    /// `prepare_module_for_check`, which adds `collect_all_signatures` (walks
    /// the fully-imported module graph, incl. `std.prelude`'s own transitive
    /// imports) on top of the plain import-inline this used to do. That is
    /// enough recursion to overflow the DEFAULT test-harness stack on Windows
    /// (confirmed: `neg2_no_imports_still_maps_entry` — STATUS_STACK_OVERFLOW
    /// before this wrapper existed). Every real (non-test) caller already runs
    /// through `run_with_large_stack` at the `server.rs` request-handling
    /// layer (hover/goto-definition/completion/etc. — see that module), so
    /// this wrapper only closes the gap for the unit tests below, which call
    /// `resolve_module_for` directly on the harness's own thread.
    fn resolve_module_for_test(path: &Path, src: &str) -> ResolvedModule {
        let path = path.to_path_buf();
        let src = src.to_string();
        crate::compiler::run_with_large_stack(move || resolve_module_for(&path, &src))
    }

    // ── POS ──────────────────────────────────────────────────────────────────

    /// POS: a file with an import yields ≥2 provenance entries (entry + at least
    /// one imported peer/prelude file), built from real peer_files.
    #[test]
    fn pos1_file_with_import_maps_multiple_files() {
        let path = write_temp("pos1.nv", SRC_WITH_IMPORT);
        let resolved = resolve_module_for_test(&path, SRC_WITH_IMPORT);
        assert!(
            resolved.file_map.len() >= 2,
            "expected ≥2 file_map entries (entry + prelude/import), got {}: {:?}",
            resolved.file_map.len(),
            resolved.file_map
        );
        // Entry is always mapped from MAIN_FILE_ID.
        assert!(resolved.file_map.contains_key(&MAIN_FILE_ID), "entry MAIN_FILE_ID must be mapped");
    }

    /// POS: a span from a foreign file_id resolves to a DIFFERENT URI than the
    /// entry / fallback.
    #[test]
    fn pos2_foreign_file_id_yields_different_uri() {
        let path = write_temp("pos2.nv", SRC_WITH_IMPORT);
        let resolved = resolve_module_for_test(&path, SRC_WITH_IMPORT);
        let fallback = Url::from_file_path(&path).unwrap();

        // Find a peer whose id is NOT the entry and whose path differs.
        let foreign = resolved
            .file_map
            .iter()
            .find(|(id, p)| **id != MAIN_FILE_ID && p.as_path() != path.as_path())
            .map(|(id, _)| *id);

        let Some(foreign_id) = foreign else {
            panic!("expected at least one foreign file_id in {:?}", resolved.file_map);
        };
        let span = Span::with_file(0, 1, foreign_id);
        let loc = span_to_location(span, &resolved.file_map, &fallback);
        assert_ne!(loc.uri, fallback, "foreign span must resolve to a different URI");
    }

    /// POS: a span with the entry's file_id resolves to the entry URI itself.
    #[test]
    fn pos3_entry_file_id_yields_same_uri() {
        let path = write_temp("pos3.nv", SRC_WITH_IMPORT);
        let resolved = resolve_module_for_test(&path, SRC_WITH_IMPORT);
        let fallback = Url::from_file_path(&path).unwrap();

        let span = Span::with_file(0, 1, MAIN_FILE_ID);
        let loc = span_to_location(span, &resolved.file_map, &fallback);
        // The entry path is canonicalized in the map; compare canonical URIs.
        let entry_canon = path.canonicalize().unwrap_or(path.clone());
        let expected = Url::from_file_path(&entry_canon).unwrap();
        assert_eq!(loc.uri, expected, "entry span must resolve to the entry URI");
    }

    // ── NEG ──────────────────────────────────────────────────────────────────

    /// NEG: an unknown file_id falls back to the current URI without panicking.
    #[test]
    fn neg1_unknown_file_id_falls_back() {
        let path = write_temp("neg1.nv", SRC_WITH_IMPORT);
        let resolved = resolve_module_for_test(&path, SRC_WITH_IMPORT);
        let fallback = Url::parse("file:///unsaved.nv").unwrap();

        let span = Span::with_file(0, 1, 9_999_999);
        let loc = span_to_location(span, &resolved.file_map, &fallback);
        assert_eq!(loc.uri, fallback, "unknown file_id must fall back to current URI");
    }

    /// NEG: a file with NO imports still yields ≥1 provenance entry (the entry).
    #[test]
    fn neg2_no_imports_still_maps_entry() {
        // `#no_prelude` avoids the auto prelude import so peer_files is minimal.
        let src = "#no_prelude\nmodule basics.lsp\nfn f() => ()\n";
        let path = write_temp("neg2.nv", src);
        let resolved = resolve_module_for_test(&path, src);
        assert!(
            !resolved.file_map.is_empty(),
            "no-import file must still map the entry"
        );
        assert!(resolved.file_map.contains_key(&MAIN_FILE_ID));
    }

    // ── EDGE ─────────────────────────────────────────────────────────────────

    /// EDGE: a broken import (resolution likely panics/errors) is contained by
    /// catch_unwind — the entry mapping survives and nothing panics.
    #[test]
    fn edge1_broken_import_no_panic() {
        let src = "module basics.lsp\nimport this.module.does.not.exist.anywhere\nfn f() => ()\n";
        let path = write_temp("edge1.nv", src);
        let resolved = resolve_module_for_test(&path, src);
        assert!(
            resolved.file_map.contains_key(&MAIN_FILE_ID),
            "entry must be mapped even when import resolution fails"
        );
    }

    /// EDGE: a parse-error source degrades to an entry-only map without panic.
    #[test]
    fn edge2_parse_error_entry_only() {
        let src = "module basics.lsp\nfn broken(@@@@) =>";
        let path = write_temp("edge2.nv", src);
        let resolved = resolve_module_for_test(&path, src);
        assert!(resolved.file_map.contains_key(&MAIN_FILE_ID));
        assert_eq!(resolved.items_start, 0, "parse-error module has no entry items");
    }
}
