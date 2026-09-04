//! Plan 35 Ф.1 MVP: cross-file import resolution через inline AST expansion.
//!
//! Используется тремя compile pipelines (Plan 35 R31 — unified pipeline):
//! - `nova-cli::cmd_check` — type-check single file.
//! - `nova-cli::cmd_build` — compile single file → exe.
//! - `compiler-codegen::test_runner::codegen_to_c` — test compilation.
//!
//! Все три вызывают `resolve_imports_inline(...)` ДО передачи `Module` в
//! `types::check_module` или `CEmitter::emit_module`.

use crate::ast::{Import, Item, Module, PeerFile};
use crate::diag::{byte_to_line_col, FileId, MAIN_FILE_ID};
use crate::parser;
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// ─── Plan 162.1 Step 1: ModuleSigTable ───────────────────────────────────────

/// A single function signature extracted from a module during signature-only
/// collection pass (Plan 162.1). Contains only the name and owning module;
/// body and type information are intentionally omitted — this is a lightweight
/// pre-pass data structure used for disambiguation before full resolve.
#[derive(Debug, Clone)]
pub struct FnSig {
    /// The function name as declared (`fn foo`) — not mangled.
    pub name: String,
    /// The declared module path that owns this function (e.g. `["std", "net"]`).
    pub module_name: Vec<String>,
}

/// Signatures collected from a single module during the signature-only pass.
/// Populated by [`collect_module_signatures_from_items`] and stored in
/// [`ModuleSigTable`].
#[derive(Debug, Clone)]
pub struct ModuleSignatures {
    /// Names of all `type` declarations in this module.
    pub type_names: Vec<String>,
    /// All `fn` declarations in this module (name + owning module).
    pub fn_sigs: Vec<FnSig>,
    /// The declared module path (same as the key in [`ModuleSigTable`]).
    pub module_name: Vec<String>,
}

/// Cross-module signature table built by [`collect_all_signatures`].
///
/// Maps declared module name (`Vec<String>`) to [`ModuleSignatures`].
/// The table is populated by a signature-only pre-pass that walks the same
/// import graph as [`resolve_imports_inline_ex`] but does not merge items or
/// mutate the [`Module`]. Callers can use [`ModuleSigTable::find_fn_modules`]
/// / [`ModuleSigTable::find_type_modules`] to answer "which module(s) define
/// symbol X?" before committing to a full resolve.
#[derive(Debug, Default, Clone)]
pub struct ModuleSigTable {
    table: HashMap<Vec<String>, ModuleSignatures>,
}

impl ModuleSigTable {
    /// Create an empty table.
    pub fn new() -> Self {
        Self { table: HashMap::new() }
    }

    /// Insert or replace the signatures for a module, keyed by `key`.
    ///
    /// **Plan 202 Ф.1 (D78 rev-4):** `key` MUST be the module's canonical
    /// **path** identity ([`canonical_module_key`]), not its declaration —
    /// see the doc on [`canonical_module_key`] for why. Before Plan 202 this
    /// keyed by `sigs.module_name` (the declaration); two physically distinct
    /// modules sharing a declaration (legal since D78 rev-4 — research
    /// 2026-07-13 §2а) would silently overwrite each other's signatures here.
    /// `sigs.module_name` is retained on [`ModuleSignatures`] as informational
    /// (identity-check / display) — no longer the lookup key.
    pub fn insert(&mut self, key: Vec<String>, sigs: ModuleSignatures) {
        self.table.insert(key, sigs);
    }

    /// Return all modules that declare a function named `fn_name`.
    /// Returns an empty vec if no module declares it.
    pub fn find_fn_modules(&self, fn_name: &str) -> Vec<&ModuleSignatures> {
        self.table
            .values()
            .filter(|sigs| sigs.fn_sigs.iter().any(|f| f.name == fn_name))
            .collect()
    }

    /// Return all modules that declare a type named `type_name`.
    /// Returns an empty vec if no module declares it.
    pub fn find_type_modules(&self, type_name: &str) -> Vec<&ModuleSignatures> {
        self.table
            .values()
            .filter(|sigs| sigs.type_names.iter().any(|t| t == type_name))
            .collect()
    }

    /// Iterate over all module signatures in the table.
    pub fn iter(&self) -> impl Iterator<Item = (&Vec<String>, &ModuleSignatures)> {
        self.table.iter()
    }

    /// Number of modules in the table.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// True if the table has no entries.
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

/// Extract [`ModuleSignatures`] from a parsed item list.
///
/// Only `Item::Type` and `Item::Fn` items contribute to the signature table;
/// `Item::Const`, `Item::Let`, `Item::Test`, etc. are skipped because they
/// are not needed for cross-module disambiguation in Plan 162.
pub fn collect_module_signatures_from_items(
    items: &[Item],
    module_name: Vec<String>,
) -> ModuleSignatures {
    let mut type_names = Vec::new();
    let mut fn_sigs = Vec::new();
    for item in items {
        match item {
            Item::Type(t) => {
                type_names.push(t.name.clone());
            }
            Item::Fn(f) => {
                fn_sigs.push(FnSig {
                    name: f.name.clone(),
                    module_name: module_name.clone(),
                });
            }
            _ => {}
        }
    }
    ModuleSignatures { type_names, fn_sigs, module_name }
}

/// Plan 172.1 U.1.2 — the implicitly-imported "prelude" package.
///
/// `prelude` is the one package every module gets **without** writing
/// `import`. It is *not* special-cased anywhere downstream: the [`Import`]
/// nodes returned by [`compute_prelude_imports`] flow through the exact same
/// resolver as a user-written `import`. There is no "prelude path" — only this
/// single named description of *which* package is implicit. Per
/// `docs/dev/compiler-conventions.md` §2, the *location* of the package (the std
/// search-path) is configurable (Plan 172.1 U.1.1), but *what is implicit* is
/// described here in one place.
const PRELUDE_PACKAGE: [&str; 2] = ["std", "prelude"];

/// Build a single prelude [`Import`] for `PRELUDE_PACKAGE` (optionally with a
/// trailing sub-module segment, e.g. `std.prelude.core` for `#prelude(core)`
/// or `std.prelude.<edition>` for an edition pin). Centralizes the `Import`
/// boilerplate that was previously duplicated across the two prelude-injection
/// sites (sig pre-pass + authoritative resolve).
fn prelude_import(sub_module: Option<&str>) -> Import {
    let mut path: Vec<String> = PRELUDE_PACKAGE.iter().map(|s| s.to_string()).collect();
    if let Some(name) = sub_module {
        path.push(name.to_string());
    }
    Import {
        path,
        items: None,
        alias: None,
        is_export: false,
        span: crate::diag::Span::dummy(),
        doc_attrs: Vec::new(),
        anchor: crate::ast::ImportAnchor::Package,
    }
}

/// Plan 172.1 U.1.2 — compute the prelude [`Import`]s a module gets implicitly.
///
/// Single source for both [`collect_all_signatures`] (the signature pre-pass)
/// and [`resolve_imports_inline_ex`] (the authoritative resolve), which
/// previously duplicated this logic — the pre-pass diverged by only ever
/// injecting the default facade, ignoring `#prelude(..)` and edition pins.
/// Unifying them makes the signature table see exactly the same prelude set
/// as the real resolve.
///
/// Honors (D26 / D174, Plan 62.F / Plan 107):
///   - `#no_prelude` → no implicit prelude.
///   - `#prelude(a, b, ...)` → only the named `std/prelude/<name>.nv`
///     sub-modules (validated against disk; empty/unknown → `Err`).
///   - `[package].edition = "X"` pin → `std/prelude/<sanitized>.nv` snapshot
///     facade when present, else falls back to the rolling facade.
///   - default → the rolling `std/prelude.nv` facade.
///
/// Prelude-self modules (the prelude files themselves) get nothing, to avoid a
/// self-import cycle.
fn compute_prelude_imports(
    module: &Module,
    stdlib_dir: &Path,
    entry_path: &Path,
) -> Result<Vec<Import>> {
    let _t = crate::perf_timer::PerfTimer::new("imports-prelude-compute");
    let is_prelude_self = crate::manifest::is_prelude_self_module(&module.name);
    let has_no_prelude = module
        .attrs
        .iter()
        .any(|a| matches!(a.kind, crate::ast::ModuleAttrKind::NoPrelude));
    if is_prelude_self || has_no_prelude {
        return Ok(Vec::new());
    }
    let partial_prelude_names: Option<Vec<String>> = module.attrs.iter().find_map(|a| {
        if let crate::ast::ModuleAttrKind::PartialPrelude(names) = &a.kind {
            Some(names.clone())
        } else {
            None
        }
    });
    let mut prelude_imports: Vec<Import> = Vec::new();
    if let Some(names) = partial_prelude_names {
        // D174: empty `#prelude()` is rejected by the parser; defensive check
        // here in case the AST is constructed directly.
        if names.is_empty() {
            return Err(anyhow!(
                "empty prelude list `#prelude()` is not allowed (D174, Plan 107); \
                 use `#no_prelude` to disable prelude auto-import\n  \
                 in module `{}`",
                module.name.join(".")
            ));
        }
        // Plan 62.F: auto-import only the listed sub-modules, validated against
        // real `std/prelude/<name>.nv` files. Bad name → compile error.
        let prelude_subdir = stdlib_dir.join("prelude");
        for name in &names {
            let sub_path = prelude_subdir.join(format!("{}.nv", name));
            if !crate::source_index::is_file(&sub_path) {
                return Err(anyhow!(
                    "`partial_prelude({})`: unknown prelude sub-module `{}`\n  \
                     in module `{}`\n  \
                     expected file: {}\n  \
                     valid sub-modules (Plan 62): core, runtime, errors, \
                     collections, protocols, effects\n  \
                     hint: check spelling or remove from list (D26, Plan 62.F)",
                    names.join(", "),
                    name,
                    module.name.join("."),
                    sub_path.display(),
                ));
            }
            prelude_imports.push(prelude_import(Some(name)));
        }
    } else {
        // Default facade, with optional edition pin (Plan 62.F.bis Ф.1):
        // `[package].edition = "X"` → `std/prelude/<sanitized>.nv` snapshot
        // when present (sanitization: `.` → `_`), else the rolling facade.
        // Soft-fail: edition specified but file absent → fall back silently.
        let mut edition_pin_used = false;
        // Registry 822: remember every path actually looked at, so the refusal
        // below can list them the way the import resolver lists its own.
        let mut searched: Vec<std::path::PathBuf> = Vec::new();
        if let Some(manifest) = crate::manifest::find_manifest(entry_path) {
            if let Some(edition) = &manifest.edition {
                let sanitized = crate::manifest::sanitize_edition(edition);
                if !sanitized.is_empty() {
                    let pin_path = stdlib_dir
                        .join("prelude")
                        .join(format!("{}.nv", sanitized));
                    searched.push(pin_path.clone());
                    if crate::source_index::is_file(&pin_path) {
                        prelude_imports.push(prelude_import(Some(&sanitized)));
                        edition_pin_used = true;
                    }
                }
            }
        }
        if !edition_pin_used {
            let prelude_path = stdlib_dir.join("prelude.nv");
            searched.push(prelude_path.clone());
            if crate::source_index::is_file(&prelude_path) {
                prelude_imports.push(prelude_import(None));
            } else {
                // Registry 822 -- the SILENCE was the defect, and the irony is that
                // the `#prelude(...)` branch twenty lines above already refuses
                // loudly, naming the expected file and offering a hint. It is the
                // DEFAULT branch -- the one every ordinary program takes -- that used
                // to fall through without a word. The user then got `undefined
                // identifier `println`` pointing at their own perfectly correct
                // source: the compiler blaming the author for the absence of the
                // compiler's own half.
                //
                // Measured against the loud neighbour on the SAME deficit: a missing
                // `std.time.duration` is answered with `cannot find module ...
                // searched: <paths>`. One deficit, two opposite reactions. They are
                // the same reaction now, and deliberately in the same shape.
                //
                // The trigger is not exotic. Our own quickstart warns that without the
                // leading dot in `. ./setup-env.ps1` the variables never get set, so
                // this is the EXPECTED beginner mistake, met in the first five minutes.
                let env_set = std::env::var_os("NOVA_STD_PATH").is_some();
                // Absolute for display, and this is the whole point of the row: a
                // relative `std\prelude.nv` with no stated base is the same kind of
                // half-answer the silent fall-through was. Display only -- the
                // resolution itself is untouched.
                let abs = |p: &std::path::Path| -> String {
                    if p.is_absolute() {
                        p.display().to_string()
                    } else {
                        std::env::current_dir()
                            .map(|c| c.join(p).display().to_string())
                            .unwrap_or_else(|_| p.display().to_string())
                    }
                };
                let searched_lines = searched
                    .iter()
                    .map(|p| format!("     {}", abs(p)))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(anyhow!(
                    "cannot find the standard library prelude\n  \
                     searched:\n{}\n  \
                     std package root resolved to: {}\n  \
                     env NOVA_STD_PATH is {}\n  \
                     hint: the std root is taken from, highest first: (1) env \
                     NOVA_STD_PATH, (2) `std = \"...\"` under [workspace] or [package] \
                     in nova.toml, (3) <project root>/std\n  \
                     without it every name the prelude provides (`println`, `Option`, \
                     ...) resolves to nothing, and the error would land on your source \
                     instead of here",
                    searched_lines,
                    abs(stdlib_dir),
                    if env_set { "set" } else { "NOT set" },
                ));
            }
        }
    }
    Ok(prelude_imports)
}

/// Signature-only pre-pass over the full import graph.
///
/// Walks the same import graph as [`resolve_imports_inline_ex`] (same path
/// resolution rules, same cycle detection, same peer-file expansion) but
/// instead of merging items into the [`Module`], it only parses each file and
/// extracts [`ModuleSignatures`] into a [`ModuleSigTable`].
///
/// The [`Module`] is **not mutated** — this function is a pure read-only scan.
/// Call this before [`resolve_imports_inline_ex`] to obtain a lookup table
/// that can answer "which module defines symbol X?" cheaply.
///
/// # Errors
/// Returns an error only for hard I/O or parse failures. Cycle detection
/// uses the same early-return guard as the main resolver (cycles are allowed
/// per D29 rev-5).
pub fn collect_all_signatures(
    entry_path: &Path,
    module: &Module,
    repo: &Path,
    stdlib_dir: &Path,
) -> Result<ModuleSigTable> {
    crate::imports_stats::note_sig_call();
    let entry_dir = entry_path.parent().unwrap_or(repo).to_path_buf();
    let mut table = ModuleSigTable::new();
    let mut visited: HashSet<Vec<String>> = HashSet::new();
    let mut in_progress: HashSet<Vec<String>> = HashSet::new();

    // Plan 202 Ф.1 (D78 rev-4): key by canonical PATH identity, not
    // declaration — see `canonical_module_key` doc. Mirrors the entry_key
    // used by `resolve_imports_inline_ex` below so both registries agree on
    // module identity (no second decl-keyed window, D78 rev-4 §Свойства п.4).
    let entry_key = canonical_module_key(std::slice::from_ref(&entry_path.to_path_buf()));

    // Seed the table with the entry module's own items.
    let entry_sigs = collect_module_signatures_from_items(&module.items, module.name.clone());
    table.insert(entry_key.clone(), entry_sigs);

    // Build import work-list from the entry module's declared imports.
    let mut import_work: Vec<(Import, PathBuf)> = Vec::new();
    for imp in &module.imports {
        import_work.push((imp.clone(), entry_path.to_path_buf()));
    }

    // Plan 172.1 U.1.2: prelude auto-import via the SINGLE shared source
    // ([`compute_prelude_imports`]) — identical to the authoritative resolve,
    // so the signature table sees exactly the prelude set the real resolve
    // will, including `#prelude(..)` partials and edition pins.
    for imp in compute_prelude_imports(module, stdlib_dir, entry_path)? {
        import_work.push((imp, entry_path.to_path_buf()));
    }

    // Mark entry as in-progress so transitive re-imports of entry early-return.
    in_progress.insert(entry_key.clone());

    for (imp, importer) in &import_work {
        collect_sigs_one(
            imp,
            importer,
            &entry_dir,
            repo,
            stdlib_dir,
            &mut table,
            &mut visited,
            &mut in_progress,
        );
    }

    in_progress.remove(&entry_key);
    visited.insert(entry_key);

    Ok(table)
}

/// Recursive helper for [`collect_all_signatures`].
///
/// Resolves a single import to its peer files, parses each peer, extracts
/// signatures, and recurses into transitive imports. Does NOT mutate any
/// `Module` — only writes to `table`, `visited`, and `in_progress`.
///
/// Errors are silently swallowed (soft-fail): a signature-only pass is
/// best-effort; hard errors will surface again during the full resolve.
fn collect_sigs_one(
    imp: &Import,
    importer_path: &Path,
    entry_dir: &Path,
    repo: &Path,
    stdlib_dir: &Path,
    table: &mut ModuleSigTable,
    visited: &mut HashSet<Vec<String>>,
    in_progress: &mut HashSet<Vec<String>>,
) {
    // Resolve relative import root (mirrors resolve_one logic).
    let rel_root: Option<PathBuf> = match &imp.anchor {
        crate::ast::ImportAnchor::Package => None,
        crate::ast::ImportAnchor::Relative { up } => {
            let base = match importer_path.parent() {
                Some(b) => b,
                None => return,
            };
            let mut dir = base.to_path_buf();
            for _ in 0..*up {
                match dir.parent() {
                    Some(p) => dir = p.to_path_buf(),
                    None => return,
                }
            }
            Some(dir)
        }
    };

    // Resolve dep root (mirrors resolve_one; errors are soft-fail).
    // Plan 202 Ф.2: bare dep-name import (len==1) also valid when the dep
    // declares root peers — mirrors `resolve_one`'s dep-lookup branch.
    let dep_root: Option<PathBuf> = if rel_root.is_some() || imp.path.is_empty() {
        None
    } else {
        match lookup_dependency(importer_path, &imp.path[0], entry_dir) {
            DepLookup::PathDep(root) if imp.path.len() >= 2 => Some(root),
            DepLookup::PathDep(root)
                if collect_root_peers(&root, &imp.path[0], false).is_some() =>
            {
                Some(root)
            }
            _ => None,
        }
    };

    let resolved_paths = match resolve_module_paths(
        &imp.path,
        entry_dir,
        repo,
        stdlib_dir,
        false,
        rel_root.as_deref(),
        dep_root.as_deref(),
    ) {
        Ok(p) => p,
        Err(_) => return, // soft-fail
    };

    if resolved_paths.is_empty() {
        return;
    }

    // Plan 202 Ф.1 (D78 rev-4): canonical-path identity, not declaration —
    // see `canonical_module_key` doc (research 2026-07-13 §2а: two physically
    // distinct modules forced to the same 2-segment decl by D78 rev-3 must
    // NOT be treated as "the same module" here).
    let module_key: Vec<String> = canonical_module_key(&resolved_paths);

    // Cycle guard and dedup.
    if in_progress.contains(&module_key) || visited.contains(&module_key) {
        return;
    }

    in_progress.insert(module_key.clone());

    for peer_path in &resolved_paths {
        let peer_src = {
            let _t = crate::perf_timer::PerfTimer::new("imports-peer-io");
            match crate::source_index::file_text(peer_path) {
                Some(s) => s,
                None => continue,
            }
        };
        crate::imports_stats::note_parse(peer_path, peer_src.len(), true);
        let peer_module = {
            let _tp = crate::perf_timer::PerfTimer::new("imports-peer-parse");
            match crate::parser::parse(&peer_src) {
                Ok(m) => m,
                Err(_) => continue,
            }
        };
        if !cfg_active(&peer_module) {
            continue;
        }

        // Extract and insert signatures for this peer.
        let peer_sigs =
            collect_module_signatures_from_items(&peer_module.items, peer_module.name.clone());
        table.insert(module_key.clone(), peer_sigs);

        // Recurse into this peer's transitive imports.
        for sub in &peer_module.imports {
            collect_sigs_one(
                sub,
                peer_path,
                entry_dir,
                repo,
                stdlib_dir,
                table,
                visited,
                in_progress,
            );
        }
    }

    in_progress.remove(&module_key);
    visited.insert(module_key);
}

// ─── End Plan 162.1 Step 1 ───────────────────────────────────────────────────

/// Plan 35 Ф.1 MVP: cross-file resolve через inline AST expansion.
///
/// Walks `module.imports` recursively (BFS), loads each imported `.nv` file,
/// parses, recursively resolves transitive imports. `Item::Type`,
/// `Item::Fn`, `Item::Const` из всех imported modules merge'ятся в текущий
/// `module.items`.
///
/// **Cycle detection:** visited set по canonical path. Cycle → error.
///
/// **Load paths** (в порядке поиска):
///   1. `<entry_dir>/<path/parts>.nv` — same-package import
///   2. `<repo>/<path/parts>.nv`     — repo-root import (для `std.X.Y` это `<repo>/std/X/Y.nv`)
///   3. `<stdlib_dir>/<X/Y>.nv`      — explicit stdlib (если path начинается с `std.`)
///
/// **Limitations** (sub-plans 35.A-E / Plan 81):
///   - Нет symbol mangling (Plan 81 Ф.3).
///   - Нет DCE.
///   - Нет signature/body 2-pass split.
///   - Wildcard `import X.*` не поддерживается.
/// D174 / Plan 107 Ф.3: pre-scan `_module.nv` рядом с entry-файлом
/// для early prelude opt-out decision до полного resolve.
///
/// Использует `crate::parser::parse` (публичный API). `parse_module_attrs`
/// приватен для parser-модуля и недоступен снаружи.
///
/// Soft-fail: любая ошибка (файл не найден, parse error) → пустой вектор.
/// Быстрый путь: raw-text check перед полным parse.
fn preload_module_nv_prelude_attrs(entry_path: &Path) -> Vec<crate::ast::ModuleAttr> {
    let dir = match entry_path.parent() { Some(d) => d, None => return vec![] };
    let module_nv = dir.join("_module.nv");
    if !crate::source_index::exists(&module_nv) { return vec![]; }
    let src = match crate::source_index::file_text(&module_nv) { Some(s) => s, None => return vec![] };
    // Fast path: skip full parse если нет prelude-управляющих атрибутов в тексте.
    if !src.contains("#no_prelude") && !src.contains("#prelude") { return vec![]; }
    // Full parse через публичный API.
    match crate::parser::parse(&src) {
        Ok(module) => module.attrs.into_iter()
            .filter(|a| matches!(a.kind,
                crate::ast::ModuleAttrKind::NoPrelude |
                crate::ast::ModuleAttrKind::PartialPrelude(_)))
            .collect(),
        Err(_) => vec![],
    }
}

/// Plan 159 Ф.4 (restored by Plan 169.2.1, D303): char Unicode-aware method
/// selectors hosted in `std.unicode` (`std/unicode/category.nv`, `char @<name>`).
/// These are the ONLY providers of these selectors on a `char` receiver in the
/// whole stdlib — verified by a stdlib-wide scan: no other type declares a
/// method with any of these names. So a syntactic appearance of `expr.<name>()`
/// is an unambiguous signal that `std.unicode` bodies are needed, even when the
/// user never wrote `import std.unicode`.
///
/// This list breaks the historic `[M-152.3b-char-methods-no-import]` blocker
/// WITHOUT a `prelude → std.unicode` import (which would re-cycle through
/// `std.collections → prelude` → stack overflow). Instead the import is injected
/// into the *user's entry module* (the normal, cycle-free import path), and
/// Plan 159 Ф.1 reachability DCE strips every table the program does not touch
/// — so the no-import ergonomics cost nothing for programs that never call them.
///
/// **Plan 169.2.1 (D303):** Plan 162 Ф.4 had replaced this auto-injection by
/// hosting the char @methods in `prelude.core` + `core.nv import std.unicode`,
/// but that forced every partial `#prelude(core, …)` to pull the whole unicode
/// folder-module (incl. `normalize.nv::cps_to_str`'s `consume sb`), tripping
/// D133 (type-check, before DCE) when `collections` was absent → plan107 failed.
/// 169.2.1 moves the methods back to `std.unicode` and restores THIS injection,
/// keeping `core` unicode-free. Re-exporting the methods through the prelude
/// facade instead is NOT viable: the method names collide with the same-named
/// free functions (`general_category(cp int)` etc.), so a facade re-export would
/// leak those free functions into the global namespace and break the opt-in
/// boundary pinned by `plan152_3/neg/n_char_unicode_opt_in.nv`.
/// [M-runtime-folder-run-ice-vec-ident] (Plan 172.13 batch 4): the checker
/// accepts `[]T` inherent methods (`.len()`, `.append()`, `.new()`, …) in ANY
/// module regardless of import — Vec's inherent methods are visible via the
/// global `TypeMethodMap` / built-in-type signature registry (Plan 162,
/// `[M-159-lazy-module-resolution]`), by DESIGN independent from whether
/// `std.collections.vec`'s ACTUAL Nova-body declarations are merged into
/// THIS compile unit's `merged_items`. For a normal (prelude-having) module
/// this is moot — prelude's default facade always merges the real
/// `std.collections.vec` too, so the two views coincide. A `#no_prelude`
/// module that never explicitly imports `std.collections.vec` (bootstrap
/// runtime helpers like `std/runtime/{read_buffer,write_buffer,
/// string_builder}.nv`, which only need e.g. `std.runtime.numeric`) is where
/// the views diverge: the checker is satisfied (TypeMethodMap knows `.len()`
/// exists), but `merged_items` never gained Vec's real body, so codegen's
/// C-lowering (`infer_expr_c_type` — a SEPARATE re-derivation from the
/// checker's channel, reading actual `FnDecl`s) cannot find `.len()`'s real
/// declared return type → `nova: internal error … [P67-LEGACY] Ident 'Vec'
/// not in var_types` / `method call '.len' return type unknown`. Mirrors
/// `CHAR_UNICODE_METHOD_SELECTORS` / `needs_unicode_injection` below 1:1 —
/// same auto-inject-into-the-user-entry-group mechanism, same
/// over-injection-is-harmless argument (Plan 159 Ф.1 reachability DCE strips
/// anything unused). List = every INSTANCE `@method` Vec actually declares in
/// `std/collections/vec/*.nv` (static ctors `new`/`with_capacity`/`from`/
/// `default`/`filled` are compiler intrinsics, not Nova bodies — excluded,
/// they need no merged declaration to lower).
const VEC_INHERENT_METHOD_SELECTORS: &[&str] = &[
    "append", "append_zero", "binary_search_by", "binary_search_by_key", "cap",
    "clear", "concat", "contains", "copy_from", "copy_within", "dedup",
    "dedup_by", "dedup_by_key", "drain", "equal", "extend", "fill",
    "fill_with", "first", "first_n", "get", "index", "index_of", "insert",
    "insert_slice", "is_empty", "is_sorted_by", "iter", "last", "last_n",
    "len", "partition", "plus", "pop", "position", "ptr", "push", "remove",
    "reserve", "resize", "resize_with", "retain", "reverse", "rotate_left",
    "rotate_right", "rposition", "slice", "sort_by", "sort_by_key",
    "sort_unstable_by", "sort_unstable_by_key", "splice", "split_at",
    "split_first", "split_last", "swap", "swap_remove", "truncate",
];

/// Mirrors [`needs_unicode_injection`]: true iff some item uses a Vec
/// inherent-method selector (`expr.foo()` receiver-call form) AND
/// `std.collections.vec` is not already imported.
fn needs_vec_injection(entry_items: &[Item], sibling_items: &[&[Item]]) -> bool {
    let mut used: HashSet<String> = HashSet::new();
    crate::lints::collect_used_names(entry_items, &mut used);
    for items in sibling_items {
        crate::lints::collect_used_names(items, &mut used);
    }
    VEC_INHERENT_METHOD_SELECTORS
        .iter()
        .any(|m| used.contains(&format!("@method:{}", m)))
}

/// True iff `imp` resolves to the `std.collections.vec` module (directly, or
/// via the `std.collections` folder that re-exports it). Used to avoid
/// double-injecting when the user already imported it.
fn import_targets_std_collections_vec(imp: &Import) -> bool {
    imp.path.len() >= 2 && imp.path[0] == "std" && imp.path[1] == "collections"
}

const CHAR_UNICODE_METHOD_SELECTORS: &[&str] = &[
    "is_alphabetic",
    "is_numeric",
    "is_alphanumeric",
    "is_whitespace",
    "is_uppercase",
    "is_lowercase",
    "is_control",
    "general_category",
    "to_uppercase",
    "to_lowercase",
];

/// Plan 159 Ф.4 (restored by 169.2.1): decide whether `std.unicode` must be
/// auto-injected into the entry module's import list. Returns true iff (a) some
/// item references a char-Unicode method selector (syntactic over-approximation
/// — collisions are impossible, see `CHAR_UNICODE_METHOD_SELECTORS`), AND
/// (b) `std.unicode` is not already imported by the entry or one of its sibling
/// peers.
///
/// G0-conservative: over-injection is harmless (Ф.1 DCE strips unused tables);
/// under-injection would be a hard error (undefined symbol), so the scan errs
/// toward injecting. Names are collected via the existing `collect_used_names`
/// AST walk (lints.rs); the walk additionally tags value-receiver method calls
/// `expr.foo()` as `@method:foo`, which is what this fn matches against (so the
/// bare free-function form `foo()` does NOT trigger injection).
fn needs_unicode_injection(entry_items: &[Item], sibling_items: &[&[Item]]) -> bool {
    let mut used: HashSet<String> = HashSet::new();
    crate::lints::collect_used_names(entry_items, &mut used);
    for items in sibling_items {
        crate::lints::collect_used_names(items, &mut used);
    }
    // Match ONLY the value-receiver method-call form `expr.<name>()`, recorded
    // by lints::collect_expr as `@method:<name>` (Plan 159 Ф.4). The bare
    // free-function form `<name>(...)` (recorded as a plain `Ident`) deliberately
    // does NOT trigger injection — those free functions stay opt-in behind
    // `import std.unicode` (pinned by plan152_3/neg/n_char_unicode_opt_in.nv).
    CHAR_UNICODE_METHOD_SELECTORS
        .iter()
        .any(|m| used.contains(&format!("@method:{}", m)))
}

/// True iff `imp` resolves to the `std.unicode` folder-module (either the
/// folder itself or any of its peers, e.g. `std.unicode.category`). Used to
/// avoid double-injecting when the user already imported it.
fn import_targets_std_unicode(imp: &Import) -> bool {
    imp.path.len() >= 2 && imp.path[0] == "std" && imp.path[1] == "unicode"
}

/// [M-assoc-const-out-of-body-syntax] (D200 AMEND, Plan 114.4 окно №66):
/// relocate parsed `const Type.NAME <Тип> = <значение>` module-level decls
/// (dotted `ConstDecl.name`, produced by `parser::parse_const_decl`) into the
/// matching `TypeDecl.assoc_consts` entry, exactly mirroring what the
/// (D200-retracted) in-body `type X { const NAME = v }` form used to
/// populate directly. Everything downstream — namespace-access type
/// inference, `E_CONST_INSTANCE_ACCESS` instance-access rejection, emit_c's
/// `.rodata` `Type_NAME` symbol emission — reads `TypeDecl.assoc_consts`
/// unconditionally and needs no further change.
///
/// Runs once over the fully-flattened item list (post import-inline), so it
/// transparently covers both a single-file `type` + `const Type.NAME` in the
/// same file AND a folder-module split across peer files (type in one peer,
/// const in another) — by this point both live in the same flat `Vec<Item>`.
fn attach_out_of_body_assoc_consts(items: &mut Vec<Item>) -> Result<()> {
    // Первый проход: вытащить все qualified `Item::Const("Type.NAME", ...)`
    // из плоского списка (retain оставляет всё остальное на месте, порядок
    // typedef'ов/fn'ов не трогаем).
    let mut pending: Vec<crate::ast::ConstDecl> = Vec::new();
    items.retain(|it| {
        if let Item::Const(cd) = it {
            if let Some(dot) = cd.name.find('.') {
                if dot > 0 && dot + 1 < cd.name.len() {
                    pending.push(cd.clone());
                    return false;
                }
            }
        }
        true
    });
    for cd in pending {
        // `split_once` — ровно один `.` ожидается на этом синтаксисе (V1,
        // T-independent). T-dependent `Box[int].SIZE` — followup, парсер
        // сюда такую форму не пропускает (see parse_const_decl doc-comment).
        let (type_name, const_name) = cd.name.split_once('.').unwrap();
        let (type_name, const_name) = (type_name.to_string(), const_name.to_string());
        let target = items.iter_mut().find_map(|it| match it {
            Item::Type(td) if td.name == type_name => Some(td),
            _ => None,
        });
        match target {
            Some(td) => {
                td.assoc_consts.push(crate::ast::AssocConst {
                    name: const_name,
                    ty: cd.ty,
                    value: cd.value,
                    span: cd.span,
                    is_export: cd.is_export,
                    // Plan 157 (D200 amend): carry the `ro`-vs-`const` flavor
                    // through unchanged — everything else about this entry
                    // (namespace access, export, instance-access rejection)
                    // is shared verbatim between the two.
                    is_lazy_ro: cd.is_lazy_ro,
                });
            }
            None => {
                let kw = if cd.is_lazy_ro { "ro" } else { "const" };
                return Err(anyhow!(
                    "[E_CONST_UNKNOWN_TYPE] `{} {}.{}` — unknown type `{}` \
                     (D200 out-of-body associated const/ro requires an already \
                     declared type in this compile unit; T-dependent generic \
                     receivers like `Box[int].SIZE` are not yet supported, \
                     [M-assoc-const-out-of-body-syntax] followup)",
                    kw, type_name, const_name, type_name,
                ));
            }
        }
    }
    Ok(())
}

pub fn resolve_imports_inline(
    entry_path: &Path,
    module: &mut Module,
    repo: &Path,
    stdlib_dir: &Path,
) -> Result<()> {
    resolve_imports_inline_ex(entry_path, module, repo, stdlib_dir, false)
}

/// Plan 42 правило F: `_test.nv` peers test-only.
/// `include_test_peers=true` (test mode): включает `*_test.nv` файлы
/// в folder-module collection.
/// `include_test_peers=false` (build mode): фильтрует их.
pub fn resolve_imports_inline_ex(
    entry_path: &Path,
    module: &mut Module,
    repo: &Path,
    stdlib_dir: &Path,
    include_test_peers: bool,
) -> Result<()> {
    crate::imports_stats::note_resolve_call();
    let entry_dir = entry_path.parent().unwrap_or(repo).to_path_buf();
    // Plan 42.14 Ф.3 ([M11]): cycle detection keyed by declared module
    // name (Vec<String>), не canonical PathBuf — symlink-safe.
    // Plan 162 Ф.4: visited is now a map from module_key → exported_names.
    // When a module is dedup-skipped (already in visited), we still populate
    // visible_acc from the cached exported_names so that explicit `import X`
    // in user code works even if X was already loaded via prelude.
    let mut visited: HashMap<Vec<String>, Vec<String>> = HashMap::new();

    let mut merged_items: Vec<Item> = Vec::new();

    // Plan 42 Sub-plan 42.4 шаг 2 (2026-05-14): per-peer attribution.
    // Entry's PeerFile регистрируется первым (file_id = MAIN_FILE_ID = 0).
    // imports + items_here — копия entry's pre-merge state.
    //
    // Note: entry parsed parent caller'ом через `parser::parse(src)` который
    // использует MAIN_FILE_ID, так что entry's spans уже file_id=0. Сейчас
    // лишь регистрируем PeerFile для type-checker'а.
    let entry_canon_for_peer = crate::source_index::canonicalize(entry_path)
        .unwrap_or_else(|| entry_path.to_path_buf());
    let entry_peer_file = PeerFile {
        path: entry_canon_for_peer,
        file_id: MAIN_FILE_ID,
        imports: module.imports.clone(),
        items_here: module.items.clone(),
        // Plan 42.15: заполнится после resolve entry's imports.
        imported_item_names: HashSet::new(),
        // Plan 42.15: entry — часть компилируемого module.
        is_entry_module: true,
        // Plan 81 Ф.1: declared module name для group-isolation.
        module_name: module.name.clone(),
    };
    // Local counter для file_id (entry = 0, peers начинают с 1).
    // Используем Vec<PeerFile> чтобы collect peers через resolve_one,
    // потом append в module.peer_files после всех resolves.
    let mut peer_files: Vec<PeerFile> = vec![entry_peer_file];
    let mut next_file_id: FileId = 1;
    // Plan 35 Ф.1 cycle detection (D29) + Plan 42.14 Ф.3 ([M11]):
    // in-progress DFS-stack — declared module names (Vec<String>)
    // currently being resolved. Если import упирается в module name
    // уже в стеке → cycle. visited — closed-set (diamond-dep dedup);
    // in_progress — open-set (cycle detect).
    let mut in_progress: HashSet<Vec<String>> = HashSet::new();
    let mut import_chain: Vec<Vec<String>> = Vec::new(); // для error message

    // Plan 35 Ф.1 (D29): добавляем entry в in_progress + chain ДО resolve.
    // Если transitive import ссылается обратно на entry — cycle detected.
    // Plan 202 Ф.1 (D78 rev-4): entry_key — canonical PATH identity
    // (`canonical_module_key`), NOT the declared module name. Two physically
    // distinct modules forced to the same 2-segment decl by D78 rev-3 (research
    // 2026-07-13 §2а) must resolve to DIFFERENT keys here, or the second one's
    // exports silently vanish (`[M-d78-duplicate-decl-module-swallow]`).
    // `import_chain` stays decl-based — it is purely a display trail for error
    // messages, not an identity key.
    let entry_key = canonical_module_key(std::slice::from_ref(&entry_path.to_path_buf()));
    in_progress.insert(entry_key.clone());
    import_chain.push(module.name.clone());

    // D174 / Plan 107 Ф.3: pre-scan _module.nv для prelude inheritance.
    // inherited_attrs merge происходит ПОСЛЕ prelude decision (end of fn),
    // поэтому early pre-scan нужен специально для NoPrelude / PartialPrelude.
    // Soft-fail: любые ошибки fs/parse → vec![] (не прерывают compile).
    let module_nv_prelude_attrs = preload_module_nv_prelude_attrs(entry_path);
    // entry-file wins: добавляем только те attrs из _module.nv, чей
    // discriminant отсутствует в уже объявленных attrs entry-файла.
    for attr in module_nv_prelude_attrs {
        if !module.attrs.iter().any(|a| {
            std::mem::discriminant(&a.kind) == std::mem::discriminant(&attr.kind)
        }) {
            module.attrs.push(attr);
        }
    }

    // Plan 35 sub-plan 35.A R27 / Plan 62.F / D174 (Plan 107): auto-import the
    // implicit `std.prelude` package. The full policy (`#no_prelude`,
    // `#prelude(a,b,..)` partials, edition pins, default facade) lives in the
    // single shared source [`compute_prelude_imports`] (Plan 172.1 U.1.2), used
    // identically by the signature pre-pass — there is no separate "prelude
    // path", only this one description of which package is implicit.
    //
    // Plan 81 Ф.10: prelude auto-imports are collected separately from the
    // entry's own (and sibling peers') `import` statements — prelude is
    // resolved once and shared by every entry-group peer (see below).
    let prelude_imports: Vec<Import> = compute_prelude_imports(module, stdlib_dir, entry_path)?;

    // Plan 42.10: accumulate module-level attrs from `_module.nv` peers
    // of imported folder-modules. Applied to entry's module.attrs at end.
    let mut inherited_attrs: Vec<crate::ast::ModuleAttr> = Vec::new();

    // Plan 81 Ф.10: entry-folder-module peer collection.
    //
    // The caller parses only the entry FILE (`parser::parse` → one
    // `Module`, `MAIN_FILE_ID`). If that file is a peer of a folder-module,
    // its sibling peers must also be compiled — they share the module's
    // namespace and the entry alone is incomplete. `resolve_one` collects
    // peers for *imported* folder-modules; here we do the equivalent for
    // the *entry* folder-module.
    //
    // A file in `entry_dir` is a sibling peer iff it declares the **same**
    // `module` path as the entry. This condition is false for every
    // single-file entry and every `_use.nv` test entry (each declares a
    // unique per-file module), so this branch is inert for all current
    // entry shapes — zero regression.
    //
    // Each sibling gets a distinct `file_id` (per-peer diagnostics +
    // per-peer import isolation), is registered as a `PeerFile` with
    // `is_entry_module = true` (it *is* part of the compiled module), and
    // its items — **including `Item::Test`** — are merged into
    // `module.items` (an entry folder-module's own tests must run, unlike
    // imported peers whose tests are skipped).
    struct SiblingPeer {
        path: PathBuf,
        file_id: FileId,
        module: Module,
    }
    let mut siblings: Vec<SiblingPeer> = Vec::new();
    {
        let entry_canon = crate::source_index::canonicalize(entry_path);
        let target = current_target_os();
        // [M-d376-slow-suffix-folder-module-peer-merge]: `_slow` siblings
        // merge iff EITHER (a) the entry ITSELF is a `_slow` file (the shape
        // `nova check`'s SlowLane-based walker / internal tests use for a
        // dedicated slow-only group), OR (b) the whole `nova test` run
        // opted into slow tests (`--slow`/`--include-slow`) — needed
        // because `nova test`'s actual walker (`walk_nv_selected`) groups a
        // folder-module's peers under ONE alphabetically-first
        // representative regardless of slow/non-slow composition, so that
        // representative is (almost) never itself `_slow`; see
        // `test_run_include_slow`'s doc-comment for the full reasoning.
        // Otherwise (plain `nova test`, no slow flag) `_slow` siblings are
        // excluded — a `_slow.nv` peer's `Item::Test` would otherwise be
        // merged into, and run as part of, every other entry's default CU.
        let entry_is_slow = entry_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(crate::test_runner::is_slow_file_stem)
            .unwrap_or(false)
            || crate::test_runner::test_run_include_slow();
        {
            // План 252: имена + объявления соседей — из кэша по каталогу.
            // Отбор по объявлению стоит ПЕРВЫМ: в каталогах вроде
            // `spec_tests/conformance/neg` (568 файлов) он отсекает почти всё
            // до дорогой `canonicalize` в следующем фильтре.
            let mut sib_paths: Vec<PathBuf> = dir_module_decls(&entry_dir)
                .iter()
                .filter(|(_, decl)| decl.as_deref() == Some(module.name.as_slice()))
                .map(|(p, _)| p.clone())
                .filter(|p| {
                    // Exclude the entry file itself.
                    match (crate::source_index::canonicalize(p), &entry_canon) {
                        (Some(pc), Some(ec)) => &pc != ec,
                        _ => p.as_path() != entry_path,
                    }
                })
                .filter(|p| {
                    // Mirror `resolve_module_paths` peer filters: `_test`
                    // peers only in test mode; `_slow` peers only when the
                    // entry itself is `_slow`; OS-suffix peers only for the
                    // current target.
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        return peer_file_included(stem, include_test_peers, entry_is_slow, target);
                    }
                    true
                })
                .collect();
            // Alphabetical → deterministic file_id assignment.
            sib_paths.sort();
            for sp in sib_paths {
                let src = crate::source_index::file_text(&sp).ok_or_else(|| {
                    anyhow!("failed to read entry-folder peer {}", sp.display())
                })?;
                // Skip peer that requires a specific SMT backend not currently active.
                // (Same logic as test_runner's REQUIRES_SMT_BACKEND check, but applied
                // before parsing so a file with unsupported syntax gated on z3v2 etc.
                // doesn't cause a parse error when included as a folder-module peer.)
                if let Some(required) = crate::test_runner::parse_smt_backend_requirement(&src) {
                    let actual = std::env::var("NOVA_SMT_BACKEND")
                        .ok()
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "trivial".to_string());
                    if actual != required {
                        continue;
                    }
                }
                let fid = next_file_id;
                next_file_id += 1;
                crate::imports_stats::note_parse(&sp, src.len(), false);
                let sib_mod = parser::parse_with_file_id(&src, fid).map_err(|d| {
                    let (line, col) = byte_to_line_col(&src, d.span.start);
                    anyhow!(
                        "in entry-folder peer '{}' ({}): {}:{}: {}",
                        module.name.join("."),
                        sp.display(),
                        line,
                        col,
                        d.message
                    )
                })?;
                // Plan 42.12: inactive `#cfg` peer — skip entirely.
                if !cfg_active(&sib_mod) {
                    continue;
                }
                let canon = crate::source_index::canonicalize(&sp).unwrap_or(sp);
                siblings.push(SiblingPeer { path: canon, file_id: fid, module: sib_mod });
            }
        }
    }

    // Plan 42.10 + Ф.10: `_module.nv` config peer of the entry folder —
    // propagate its module-level attrs (Forbid / Cfg / Doc) onto the
    // compiled module, exactly as `resolve_one` does for imported peers.
    for sib in &siblings {
        let is_module_config = sib.path.file_stem()
            .and_then(|s| s.to_str())
            .map_or(false, |stem| stem == "_module");
        if is_module_config {
            for attr in &sib.module.attrs {
                inherited_attrs.push(attr.clone());
            }
        }
    }

    // Register sibling PeerFiles (snapshot of items before merge;
    // `imported_item_names` filled after import resolution below).
    for sib in &siblings {
        peer_files.push(PeerFile {
            path: sib.path.clone(),
            file_id: sib.file_id,
            imports: sib.module.imports.clone(),
            items_here: sib.module.items.clone(),
            imported_item_names: HashSet::new(),
            is_entry_module: true,
            module_name: sib.module.name.clone(),
        });
    }

    // [M-imports-entry-folder-module-self-cycle-empty-exports] fix: seed
    // `visited` with the entry module's own export surface RIGHT NOW —
    // `module.items` (+ each sibling's items) is fully parsed already, no
    // recursion needed to know it. This makes `resolve_one`'s `visited`
    // check (now ordered before the `in_progress` cycle guard, see there)
    // answer correctly for any file transitively reached during THIS same
    // resolve that plainly `import`s the entry module back — previously
    // that hit the `in_progress` guard instead and got an empty
    // `visible_acc` (entry_key stays in `in_progress` for the whole
    // function, including the `pending_peer_preludes` drain below — see
    // the entry_key comment above). Mirrors, per-file, the export-name
    // filter `resolve_one`'s own peer loop applies to every other module
    // (`module_has_exports` / `is_export` — Plan 81 Ф.1).
    //
    // [M-imports-order-dependent-cycle]: `exported_names_from_items` is now
    // a top-level fn (defined above `resolve_one`) — the SAME entry-only
    // trick below is generalized there to ANY in-progress (non-entry)
    // module, not just the entry.
    let mut entry_export_names: Vec<String> = exported_names_from_items(&module.items);
    for sib in &siblings {
        entry_export_names.extend(exported_names_from_items(&sib.module.items));
    }
    visited.insert(entry_key.clone(), entry_export_names);

    // Plan 81 Ф.10: per-peer visible-name accumulators.
    //   index 0      — entry's own imports.
    //   index 1      — prelude (auto-import; shared by ALL entry-group
    //                  peers — resolved once, the `visited` set prevents
    //                  re-resolution so each peer cannot re-derive it).
    //   index 2 + i  — sibling `siblings[i]`'s own imports.
    // Rule C: a peer sees only its OWN imports — accumulators are NOT
    // shared between peers; prelude (index 1) is the one deliberate
    // exception, mirroring how the entry receives prelude auto-import.
    let mut visible_accs: Vec<HashSet<String>> =
        vec![HashSet::new(); 2 + siblings.len()];

    // Build the import work-list: (import, importer-file path, acc index).
    // Order: entry's own imports, then each sibling's, then prelude last —
    // keeps `merged_items` in «imported-then-prelude» order (identical to
    // pre-Ф.10 for single-file entries: no siblings → entry imports then
    // prelude).
    let mut import_work: Vec<(Import, PathBuf, usize)> = Vec::new();
    for imp in &module.imports {
        import_work.push((imp.clone(), entry_path.to_path_buf(), 0));
    }
    for (si, sib) in siblings.iter().enumerate() {
        for imp in &sib.module.imports {
            import_work.push((imp.clone(), sib.path.clone(), 2 + si));
        }
    }

    // Plan 159 Ф.4 (restored by Plan 169.2.1, D303) — no-import char Unicode
    // methods (closes `[M-152.3b-char-methods-no-import]`). If the entry-group
    // references a char-Unicode method selector (`'A'.is_alphabetic()` etc.) but
    // never imported `std.unicode`, inject that import here — into the *user*
    // entry group, NOT the prelude facade. Injecting into the user group is the
    // ordinary cycle-free path (the prelude→unicode→collections→prelude cycle is
    // never entered). Bodies then merge normally; Plan 159 Ф.1 reachability DCE
    // strips every Unicode table the program does not actually touch, so a
    // program that never calls a char-Unicode method pays nothing. Skipped for
    // `std.unicode` itself (its peers `module std.unicode`) to avoid self-import,
    // and skipped when the user already imported `std.unicode` anywhere in the
    // entry group.
    //
    // Plan 169.2.1: this replaces the Plan 162 Ф.4 approach (char @methods hosted
    // in prelude.core + core.nv import std.unicode), which forced partial
    // `#prelude(core,…)` to pull the whole unicode folder-module and tripped D133
    // on `cps_to_str`'s `consume sb` (plan107). `core` is now unicode-free again.
    {
        let is_unicode_self = module.name.len() >= 2
            && module.name[0] == "std"
            && module.name[1] == "unicode";
        let already_imports_unicode = module.imports.iter().any(import_targets_std_unicode)
            || siblings
                .iter()
                .any(|s| s.module.imports.iter().any(import_targets_std_unicode));
        if !is_unicode_self && !already_imports_unicode {
            let sibling_items: Vec<&[Item]> =
                siblings.iter().map(|s| s.module.items.as_slice()).collect();
            if needs_unicode_injection(&module.items, &sibling_items) {
                let inject = Import {
                    path: vec!["std".into(), "unicode".into()],
                    items: None,
                    alias: None,
                    is_export: false,
                    span: crate::diag::Span::dummy(),
                    doc_attrs: Vec::new(),
                    anchor: crate::ast::ImportAnchor::Package,
                };
                // Acc index 0 (entry's own visible-name accumulator): the
                // injected names behave exactly as if the entry had written
                // the import itself.
                import_work.push((inject, entry_path.to_path_buf(), 0));
            }
        }
    }

    // [M-runtime-folder-run-ice-vec-ident]: same auto-inject mechanism as the
    // std.unicode block above, targeting `std.collections.vec` — see
    // `VEC_INHERENT_METHOD_SELECTORS` doc for the full rationale.
    {
        // Note: `std/collections/vec/*.nv` self-declares `module
        // collections.vec` (no `std` prefix — a pre-existing stdlib
        // module-naming inconsistency; imports still spell the full
        // `std.collections.vec`, reconciled by the resolver's path mapping).
        let is_vec_self = module.name.len() >= 2
            && module.name[0] == "collections"
            && module.name[1] == "vec";
        let already_imports_vec = module.imports.iter().any(import_targets_std_collections_vec)
            || siblings
                .iter()
                .any(|s| s.module.imports.iter().any(import_targets_std_collections_vec));
        if !is_vec_self && !already_imports_vec {
            let sibling_items: Vec<&[Item]> =
                siblings.iter().map(|s| s.module.items.as_slice()).collect();
            if needs_vec_injection(&module.items, &sibling_items) {
                let inject = Import {
                    path: vec!["std".into(), "collections".into(), "vec".into()],
                    items: None,
                    alias: None,
                    is_export: false,
                    span: crate::diag::Span::dummy(),
                    doc_attrs: Vec::new(),
                    anchor: crate::ast::ImportAnchor::Package,
                };
                import_work.push((inject, entry_path.to_path_buf(), 0));
            }
        }
    }

    for imp in &prelude_imports {
        import_work.push((imp.clone(), entry_path.to_path_buf(), 1));
    }

    // Plan 81 Ф.8.2: multi-error recovery. Резолв НЕ прерывается на
    // первой ошибке импорта — собираем все и репортим разом. Между
    // top-level импортами восстанавливаем cycle-detection state
    // (`in_progress` / `import_chain` / `visited`) из снапшота, если
    // `resolve_one` упал, не сбалансировав push/pop — иначе ложные
    // cycle-ошибки на последующих импортах. `merged_items` / `peer_files`
    // могут остаться частичными — это безвредно: при наличии ошибок
    // дальнейший пайплайн (type-check) не запускается.
    // [M-per-file-check-no-prelude-protocol-scope] (Plan 172.13 batch 4):
    // deferred peer-prelude requests queued by nested `resolve_one` calls
    // (a peer needing ITS OWN implicit prelude — see the doc on `resolve_one`'s
    // `pending_peer_preludes` parameter). Drained in a SEPARATE pass below,
    // strictly AFTER this loop, so every top-level import target is already
    // fully `visited` (no `in_progress` ancestor left for a legitimate
    // prelude→…→peer cycle to hit).
    let mut pending_peer_preludes: Vec<(Import, PathBuf)> = Vec::new();
    let mut import_errors: Vec<String> = Vec::new();
    for (imp, importer, acc_idx) in &import_work {
        let in_progress_snap = in_progress.clone();
        let import_chain_snap = import_chain.clone();
        let visited_snap = visited.clone();
        let res = resolve_one(
            imp,
            importer,
            &entry_dir,
            repo,
            stdlib_dir,
            &mut visited,
            &mut in_progress,
            &mut import_chain,
            &mut merged_items,
            &mut peer_files,
            &mut next_file_id,
            include_test_peers,
            &mut inherited_attrs,
            &mut visible_accs[*acc_idx],
            &mut pending_peer_preludes,
        );
        if let Err(e) = res {
            import_errors.push(format!("{}", e));
            in_progress = in_progress_snap;
            import_chain = import_chain_snap;
            visited = visited_snap;
        }
    }
    // Drain deferred peer-prelude requests. Best-effort (soft-fail, mirroring
    // the signature pre-pass): these are supplementary (the peer's own
    // protocol/bound resolution), not user-authored imports — a failure here
    // must not turn into a top-level import error for code the user never
    // wrote. The resolved items land in the SAME shared `merged_items` (global
    // registries like `self.types`/`self.sig` used by BOUND/PROTOCOL checks
    // are not scoped by `visible_acc`), so a throwaway acc is correct — we
    // only need the declarations present, not attributed to any one file's
    // visible-name set. `visited`-dedup still applies, so requesting the same
    // prelude sub-module from many peers resolves it at most once.
    {
        let mut extra_errors: Vec<String> = Vec::new();
        for (pi, importer) in &pending_peer_preludes {
            let in_progress_snap = in_progress.clone();
            let import_chain_snap = import_chain.clone();
            let visited_snap = visited.clone();
            let mut throwaway_visible: HashSet<String> = HashSet::new();
            let mut throwaway_pending: Vec<(Import, PathBuf)> = Vec::new();
            let res = resolve_one(
                pi,
                importer,
                &entry_dir,
                repo,
                stdlib_dir,
                &mut visited,
                &mut in_progress,
                &mut import_chain,
                &mut merged_items,
                &mut peer_files,
                &mut next_file_id,
                include_test_peers,
                &mut inherited_attrs,
                &mut throwaway_visible,
                &mut throwaway_pending,
            );
            if res.is_err() {
                extra_errors.push(format!("{:?}", res));
                in_progress = in_progress_snap;
                import_chain = import_chain_snap;
                visited = visited_snap;
            }
        }
        let _ = extra_errors; // soft-fail: best-effort supplementary resolve only.
    }
    if !import_errors.is_empty() {
        return Err(anyhow!(
            "{} import error(s):\n\n{}",
            import_errors.len(),
            import_errors.join("\n\n"),
        ));
    }

    // Plan 81 Ф.10: write per-peer `imported_item_names`. Each entry-group
    // peer sees the names brought by its OWN imports plus prelude (index 1).
    let prelude_visible = visible_accs[1].clone();
    if let Some(entry_pf) = peer_files.iter_mut().find(|p| p.file_id == MAIN_FILE_ID) {
        let mut s = std::mem::take(&mut visible_accs[0]);
        s.extend(prelude_visible.iter().cloned());
        entry_pf.imported_item_names = s;
    }
    for (si, sib) in siblings.iter().enumerate() {
        if let Some(pf) = peer_files.iter_mut().find(|p| p.file_id == sib.file_id) {
            let mut s = std::mem::take(&mut visible_accs[2 + si]);
            s.extend(prelude_visible.iter().cloned());
            pf.imported_item_names = s;
        }
    }

    // Entry done — drop it from the open-set (DFS-stack bookkeeping only:
    // nothing after this point calls `resolve_one` again in this function,
    // so it's inert either way). `visited` already holds the entry's real
    // export names, seeded up front (see `entry_export_names` above) —
    // [M-imports-entry-folder-module-self-cycle-empty-exports]: do NOT
    // overwrite that cache with `vec![]` here, that was the bug (the old
    // "entry's exports not cached" comment/invariant this replaced was
    // false — a file transitively reached via CU auto-injection, not a
    // direct importer, CAN and does get dedup'd against the entry's own
    // module_key within this same resolve call).
    in_progress.remove(&entry_key);
    import_chain.pop();

    // Prepend merged items: imported сначала, потом user code (entry +
    // sibling peers). Это важно для bootstrap single-pass codegen —
    // typedef'ы должны появиться ДО use-site.
    //
    // Plan 221.1 №99 (`[M-entry-value-embed-forward-decl-order]`): entry's
    // own items merge into the SAME alphabetical position it would occupy
    // as an ordinary sibling — NOT unconditionally first. `siblings` is
    // already filename-sorted (`sib_paths.sort()` above); before this fix
    // the entry ALWAYS jumped the queue ahead of every one of its self-
    // collected siblings regardless of filename, so an entry needing a
    // BY-VALUE embed from an alphabetically-EARLIER sibling (e.g. nova-http's
    // `server.nv` embedding `multipart.nv`'s `MultipartLimits` — a `value`
    // record, `m` < `s`) got that dependency's C struct body emitted AFTER
    // its own embedding struct — a hard C error (`field has incomplete type
    // 'NovaValue_MultipartLimits'`) — codegen's `emit_module` walks
    // `module.items` in this exact order to emit type declarations, and a
    // by-value field needs its type's COMPLETE body already emitted, not
    // just forward-declared. The SAME pair compiled fine whenever the file
    // needing the embed was itself just an ordinary (non-entry) peer or
    // import target — `resolve_one`'s peer collection for an IMPORTED
    // folder-module sorts ALL its peers alphabetically with no special
    // treatment for any one of them (only the entry's OWN self-collected
    // siblings, handled here, ever special-cased "self" ahead of the
    // group) — which is why this never surfaced in the nova-http monolith
    // (server.nv was always reached as an ordinary peer there, never as
    // literally the file passed to `nova build`/`nova test`).
    let mut new_items = merged_items;
    let entry_file_name = entry_path.file_name();
    let entry_insert_at = siblings.iter()
        .position(|s| s.path.file_name() > entry_file_name)
        .unwrap_or(siblings.len());
    for sib in siblings[..entry_insert_at].iter_mut() {
        new_items.append(&mut sib.module.items);
    }
    new_items.append(&mut module.items);
    for sib in siblings[entry_insert_at..].iter_mut() {
        new_items.append(&mut sib.module.items);
    }
    module.items = new_items;

    // [M-assoc-const-out-of-body-syntax] (D200 AMEND, окно №66): fold
    // module-level `const Type.NAME <Тип> = <значение>` decls (parser
    // yields them as plain `Item::Const` with a dotted qualified name —
    // see `parser::parse_const_decl`) into their target `TypeDecl.assoc_consts`
    // — the SAME const-table field the (now-retracted) in-body form
    // populated, so every downstream consumer (namespace-access resolve,
    // `E_CONST_INSTANCE_ACCESS`, emit_c `.rodata` emission) needs zero
    // changes. Runs HERE (post item-flatten, covers single-file AND
    // folder-module cross-peer-file cases — type in one peer, const in
    // another) — all three canonical pipelines call
    // `resolve_imports_inline[_ex]` before check/emit (module doc-comment
    // above), so this is the one universal point.
    attach_out_of_body_assoc_consts(&mut module.items)?;

    // Plan 42 Sub-plan 42.4 шаг 2: переносим собранные PeerFile в module.
    // Type-checker (шаг 3) использует это для per-peer name resolution.
    module.peer_files = peer_files;

    // Plan 42.10: merge inherited attrs из `_module.nv` peers импортированных
    // folder-modules. CapabilityCtx (types/mod.rs) применит #forbid attrs
    // ко всем functions module — независимо от того, defined ли они в
    // entry или imported. Doc и Cfg attrs тоже пропагируются (consumer —
    // Plan 45 nova doc и cfg_active filter уже handled).
    for attr in inherited_attrs {
        module.attrs.push(attr);
    }

    Ok(())
}

/// [M-imports-order-dependent-cycle] (generalizes the entry-only
/// `[M-imports-entry-folder-module-self-cycle-empty-exports]` fix,
/// 2026-07-20): a module's exported-name SURFACE is computable from its own
/// parsed `items` alone — it needs no resolved imports (D291: cross-module
/// cycles are allowed; "collect-signatures-first, lazy bodies" is the
/// stated architecture, but the actual `visited`/`module_exports_cache`
/// population previously happened only AFTER a module's own imports had
/// been (possibly cycle-guard-truncated) recursed into — not actually
/// "signatures first"). Used both to seed the CU entry's export cache
/// (`resolve_imports_inline_ex`) and, per this fix, to seed/extend ANY
/// in-progress module's provisional export cache in `resolve_one` below,
/// as soon as each of its (peer) files is parsed — BEFORE recursing into
/// that file's own imports. Mirrors, per-file, the export-name filter
/// `resolve_one`'s own peer loop applies to every other module
/// (`module_has_exports` / `is_export` — Plan 81 Ф.1).
pub(crate) fn exported_names_from_items(items: &[Item]) -> Vec<String> {
    let module_has_exports = items.iter().any(|item| match item {
        Item::Fn(f) => f.is_export,
        Item::Type(t) => t.is_export,
        Item::Const(c) => c.is_export,
        _ => false,
    });
    let mut names = Vec::new();
    for item in items {
        let (name, is_export) = match item {
            Item::Type(t) => (Some(t.name.clone()), t.is_export),
            Item::Fn(f) => (Some(f.name.clone()), f.is_export),
            Item::Const(c) => (Some(c.name.clone()), c.is_export),
            _ => (None, false),
        };
        if let Some(n) = name {
            if !module_has_exports || is_export {
                names.push(n);
            }
        }
    }
    names
}

/// Plan 35 Ф.1 cycle detection (D29): DFS-recursive resolve.
/// Поддерживает два множества:
///   - `visited`: closed-set (модули уже полностью обработаны) — для
///     diamond-dep dedup (silent skip).
///   - `in_progress`: open-set (модули currently being resolved в DFS-стеке)
///     — для cycle detection (error при повторном visit'е).
///   - `import_chain`: parallel vec для error-message (full cycle path).
fn resolve_one(
    imp: &Import,
    // Plan 42.17 Ф.4: путь importing-файла (entry или peer, который
    // написал этот `import`). Нужен для Rule H filesystem-containment.
    importer_path: &Path,
    entry_dir: &Path,
    repo: &Path,
    stdlib_dir: &Path,
    visited: &mut HashMap<Vec<String>, Vec<String>>,
    in_progress: &mut HashSet<Vec<String>>,
    import_chain: &mut Vec<Vec<String>>,
    merged_items: &mut Vec<Item>,
    peer_files: &mut Vec<PeerFile>,
    next_file_id: &mut FileId,
    include_test_peers: bool,
    // Plan 42.10: collect module-level attrs from `_module.nv` peers
    // for propagation into entry's module.attrs.
    inherited_attrs: &mut Vec<crate::ast::ModuleAttr>,
    // Plan 42.15: accumulator имён items, ставших видимыми через ЭТОТ
    // import (после rename). Caller — владелец import'а (peer/entry) —
    // передаёт свой `imported_item_names`. Транзитивные sub-imports
    // получают свой временный acc (не протекают в caller).
    visible_acc: &mut HashSet<String>,
    // [M-per-file-check-no-prelude-protocol-scope] (Plan 172.13 batch 4):
    // deferred peer-prelude requests — see the call site below (peer loop)
    // for the full rationale. Collected here, NOT resolved inline, and
    // drained by the caller (`resolve_imports_inline_ex`) ONLY after the
    // top-level `import_work` loop fully completes — i.e. with every
    // top-level import target's `module_key` already fully `visited`
    // (not merely `in_progress`). Resolving inline (nested, while some
    // ancestor peer's own module_key is still `in_progress`) risked a
    // legitimate prelude→…→that-ancestor cycle hitting the `in_progress`
    // cycle-guard (line ~1366) and returning EARLY — silently truncating
    // `module_exports_cache` for `std.prelude` itself (a one-shot `visited`
    // cache), which then starved the ENTRY's own top-level prelude import of
    // names it legitimately re-exports (observed: `assert` going missing —
    // "undefined identifier `assert`" — in a large sibling-merged CU where
    // an unrelated sibling's `import std.collections.vec` reached this peer
    // loop while `vec`'s own module_key was still `in_progress`).
    pending_peer_preludes: &mut Vec<(Import, PathBuf)>,
) -> Result<()> {
    // Plan 42 правило H (`internal/` boundary) — проверяется НИЖЕ, после
    // resolve в filesystem paths. Plan 42.17 Ф.4 перевёл его с хрупкого
    // import-path-string prefix на filesystem-containment: re-export /
    // alias больше не обходят boundary.

    // Plan 42 Ф.2: resolve module to list of peer files (or single file
    // for legacy single-file modules).
    // Plan 84: относительный импорт (`./` / `../`) — root резолва =
    // директория importing-файла, поднятая на `up` уровней; строго в
    // пределах своего пакета (директория ближайшего `nova.toml`).
    let rel_root: Option<PathBuf> = match &imp.anchor {
        crate::ast::ImportAnchor::Package => None,
        crate::ast::ImportAnchor::Relative { up } => {
            let importing = import_chain.last()
                .map(|m| m.join("."))
                .unwrap_or_else(|| "<entry>".to_string());
            let prefix_str = if *up == 0 {
                "./".to_string()
            } else {
                "../".repeat(*up as usize)
            };
            let base = importer_path.parent().ok_or_else(|| anyhow!(
                "relative import `{}{}`: importing file has no parent directory",
                prefix_str, imp.path.join("."),
            ))?;
            let pkg_root = package_root_of(importer_path)
                .unwrap_or_else(|| repo.to_path_buf());
            let mut dir = base.to_path_buf();
            for _ in 0..*up {
                match dir.parent() {
                    Some(p) => dir = p.to_path_buf(),
                    None => return Err(anyhow!(
                        "relative import `{}{}` выходит за границу файловой системы\n  \
                         importing module: {}\n  \
                         hint: слишком много `../`",
                        prefix_str, imp.path.join("."), importing,
                    )),
                }
            }
            let dir_canon = crate::source_index::canonicalize(&dir).unwrap_or_else(|| dir.clone());
            let pkg_canon = crate::source_index::canonicalize(&pkg_root).unwrap_or_else(|| pkg_root.clone());
            if !dir_canon.starts_with(&pkg_canon) {
                return Err(anyhow!(
                    "relative import `{}{}` выходит за границу пакета\n  \
                     importing module: {}\n  \
                     package root:     {}\n  \
                     hint: относительный импорт (`./` / `../`) не может выйти за \
                     корень своего пакета — для межпакетных ссылок используйте \
                     полный путь от корня (Plan 84 / D29)",
                    prefix_str, imp.path.join("."), importing, pkg_canon.display(),
                ));
            }
            Some(dir)
        }
    };

    // Plan 03.1 Ф.3: межпакетный резолв. Если первый сегмент import-пути
    // — объявленная `[dependencies]`-зависимость пакета импортирующего
    // файла, резолв идёт в дереве этой зависимости (а не через repo-root).
    // Относительный импорт (Plan 84) границу пакета не пересекает — для
    // него dep-резолв неприменим (`rel_root.is_some()` ⇒ пропуск).
    let dep_root: Option<PathBuf> = if rel_root.is_some() || imp.path.is_empty() {
        None
    } else {
        match lookup_dependency(importer_path, &imp.path[0], entry_dir) {
            DepLookup::NotADep => None,
            DepLookup::PathDep(root) => {
                // Plan 202 Ф.2 (D78 rev-4 "root peers"): a BARE dependency
                // name (`import tls`, no `.module` suffix) is legal iff
                // the dependency itself declares root peers (`.nv` files
                // directly in ITS source_root declaring the single-segment
                // `module <dep_name>`) — this is the Ф.2 fix for the
                // historic `tls.tls` stutter (research 2026-07-13 §7):
                // `import tls.{TlsStream}` instead of
                // `import tls.tls.{TlsStream}`. `imp.path[0]` is already
                // validated == the dependency's `[package].name` by
                // `lookup_dependency`'s `NameMismatch` check above, so
                // `root` (== dependency's source_root) + `imp.path[0]` is
                // enough — no manifest re-parse needed. Falls through to
                // the original hard error for a dependency with no root
                // peers (a bare dep name still doesn't address anything).
                if imp.path.len() < 2
                    && collect_root_peers(&root, &imp.path[0], include_test_peers).is_none()
                {
                    return Err(anyhow!(
                        "импорт из зависимости `{}` требует путь к модулю: \
                         `import {}.<module>...`\n  \
                         importing file: {}\n  \
                         hint: голое имя пакета не адресует модуль, если \
                         зависимость не объявляет root peers (D78 rev-4 §7)",
                        imp.path[0], imp.path[0], importer_path.display(),
                    ));
                }
                Some(root)
            }
            DepLookup::GitError(msg) => return Err(anyhow!(
                "{}\n  \
                 importing file: {}",
                msg, importer_path.display(),
            )),
            DepLookup::RegistryDep(ver) => return Err(anyhow!(
                "зависимость `{}` задана registry-версией `{}`, но registry \
                 ещё нет\n  \
                 importing file: {}\n  \
                 hint: используйте `{{ path = \"...\" }}`; registry — \
                 Plan 03.3",
                imp.path[0], ver, importer_path.display(),
            )),
            DepLookup::InvalidDep(raw) => return Err(anyhow!(
                "некорректная запись `[dependencies]` для `{}`: {}\n  \
                 importing file: {}\n  \
                 hint: ожидается `{{ path = \"...\" }}` либо \
                 `{{ git = \"...\", rev|tag|branch = \"...\" }}`",
                imp.path[0], raw, importer_path.display(),
            )),
            DepLookup::PathMissing(p) => return Err(anyhow!(
                "path-зависимость `{}` указывает на несуществующую \
                 директорию\n  \
                 expected:       {}\n  \
                 importing file: {}\n  \
                 hint: проверьте `path` в `[dependencies]`",
                imp.path[0], p, importer_path.display(),
            )),
            DepLookup::ReplacePathMissing(p) => return Err(anyhow!(
                "[E_REPLACE_PATH_MISSING] [replace] `{}` указывает на \
                 несуществующий путь\n  \
                 expected:       {}\n  \
                 importing file: {}\n  \
                 hint: проверьте `path` в `[replace]` (nova.toml или \
                 nova.override.toml) — на несуществующий override-путь \
                 компилятор НЕ откатывается тихо на git/декларированный \
                 источник; либо исправьте путь, либо уберите override",
                imp.path[0], p, importer_path.display(),
            )),
            DepLookup::NoManifest(p) => return Err(anyhow!(
                "path-зависимость `{}`: директория не содержит `nova.toml`\n  \
                 directory:      {}\n  \
                 importing file: {}\n  \
                 hint: зависимость должна быть Nova-пакетом — со своим \
                 `nova.toml` и `[package].name`",
                imp.path[0], p, importer_path.display(),
            )),
            DepLookup::NameMismatch { key, actual } => return Err(anyhow!(
                "имя зависимости `{}` не совпадает с `[package].name` = `{}` \
                 в её `nova.toml`\n  \
                 importing file: {}\n  \
                 hint: ключ в `[dependencies]` должен совпадать с именем \
                 пакета зависимости (Plan 03.1 §3.2)",
                key, actual, importer_path.display(),
            )),
            DepLookup::ConfigError(msg) => return Err(anyhow!("{}", msg)),
        }
    };

    let resolved_paths = resolve_module_paths(&imp.path, entry_dir, repo, stdlib_dir, include_test_peers, rel_root.as_deref(), dep_root.as_deref())
        .map_err(|err| {
            // Plan 42 правило L: diagnostic quality. Plan 42.08 Ф.2: ambiguous
            // case теперь явно диагностируется.
            let importing = import_chain
                .last()
                .map(|m| m.join("."))
                .unwrap_or_else(|| "<unknown>".to_string());
            match err {
                ResolveErr::Ambiguous { file, folder } => anyhow!(
                    "ambiguous module '{}': both single-file and folder-module exist\n  \
                     imported from: module `{}`\n  \
                     file:   {}\n  \
                     folder: {}\n  \
                     hint: remove one or rename to resolve conflict (D29 rev-3)",
                    imp.path.join("."),
                    importing,
                    file.display(),
                    folder.display(),
                ),
                ResolveErr::NotFound => {
                    // Plan 84: для относительного импорта — сообщение про
                    // конкретную директорию, не про candidate-roots.
                    if let Some(rr) = &rel_root {
                        let prefix_str = match &imp.anchor {
                            crate::ast::ImportAnchor::Relative { up } if *up == 0 =>
                                "./".to_string(),
                            crate::ast::ImportAnchor::Relative { up } =>
                                "../".repeat(*up as usize),
                            crate::ast::ImportAnchor::Package => String::new(),
                        };
                        anyhow!(
                            "cannot find module `{}{}` (relative import)\n  \
                             imported from: module `{}`\n  \
                             searched in:   {}\n  \
                             hint: модуль не найден в этой директории — \
                             проверьте имя и число `../`",
                            prefix_str,
                            imp.path.join("."),
                            importing,
                            rr.join(imp.path.iter().collect::<PathBuf>()).display(),
                        )
                    } else if let Some(dr) = &dep_root {
                        // Plan 03.1 Ф.3: импорт из зависимости не нашёлся —
                        // сообщение про дерево зависимости, не про
                        // candidate-roots текущего пакета.
                        anyhow!(
                            "cannot find module `{}` in dependency `{}`\n  \
                             imported from: module `{}`\n  \
                             searched in:   {}\n  \
                             hint: проверьте, что модуль существует в дереве \
                             зависимости `{}` (полный путь импорта — `{}`)",
                            imp.path[1..].join("."),
                            imp.path[0],
                            importing,
                            dr.join(imp.path[1..].iter().collect::<PathBuf>()).display(),
                            imp.path[0],
                            imp.path.join("."),
                        )
                    } else {
                    let suggestion = suggest_module_name(
                        &imp.path,
                        entry_dir,
                        repo,
                        stdlib_dir,
                    );
                    anyhow!(
                        "cannot find module '{}'\n  \
                         imported from: module `{}`\n  \
                         searched:\n  \
                         \x20  {} (single-file or folder)\n  \
                         \x20  {} (single-file or folder)\n  \
                         \x20  {} (stdlib){}",
                        imp.path.join("."),
                        importing,
                        entry_dir.join(imp.path.iter().collect::<PathBuf>()).display(),
                        repo.join(imp.path.iter().collect::<PathBuf>()).display(),
                        if imp.path[0] == "std" && imp.path.len() >= 2 {
                            stdlib_dir.join(imp.path[1..].iter().collect::<PathBuf>())
                                .display()
                                .to_string()
                        } else {
                            "<n/a>".to_string()
                        },
                        suggestion,
                    )
                    }
                }
                ResolveErr::CaseMismatch { requested, actual } => anyhow!(
                    "module path case mismatch: import declares `{}` but on \
                     disk the name is `{}`\n  \
                     imported from: module `{}`\n  \
                     hint: module paths must match file/folder names \
                     case-sensitively (Plan 81 Ф.4) — code that resolves on \
                     Windows/macOS would fail on Linux. Fix the import to \
                     `{}`.",
                    requested,
                    actual,
                    importing,
                    actual,
                ),
                ResolveErr::FileOrphan { head, module_path, orphans } => {
                    let dir = head
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<?>".to_string());
                    let target_seg = module_path
                        .rsplit('.')
                        .next()
                        .unwrap_or(module_path.as_str())
                        .to_string();
                    let head_str = head.display().to_string();
                    let suffix = if orphans.len() == 1 { "ий" } else { "ие" };
                    let orphans_list = orphans
                        .iter()
                        .map(|p| format!("    - {} — объявляет `module {}`", p.display(), module_path))
                        .collect::<Vec<_>>()
                        .join("\n");
                    anyhow!(
                        "[E_MODULE_FILE_ORPHAN] module `{module_path}`: у файлового \
                         head-модуля есть осиротевш{suffix} co-equal peer-файл(ы) — \
                         они не подключаются к резолву этого импорта\n  \
                         imported from: module `{importing}`\n  \
                         head-файл (единственный, чьё имя совпадает с последним \
                         сегментом импорта): {head_str}\n  \
                         осиротевш{suffix} файл(ы) в той же директории `{dir}`, \
                         объявляющие тот же `module {module_path}`:\n{orphans_list}\n  \
                         почему: `module {module_path}` — файловый модуль (D78 \
                         «файл ИЛИ папка»); его каноничный (головной) файл — \
                         единственный `.nv`, чьё ИМЯ совпадает с последним \
                         сегментом `{target_seg}` пути импорта. Альтернативная \
                         легальная форма — выделенная папка-модуль \
                         `{dir}/{target_seg}/`, содержащая ВСЕ co-equal peer-файлы \
                         модуля — но такой папки здесь нет: файлы лежат прямо в \
                         `{dir}`, рядом с head-файлом, а не в `{target_seg}/`.\n  \
                         следствие: типы/функции/методы, объявленные в \
                         осиротевш{suffix} файл(ах), невидимы для `{importing}` и \
                         для любого другого импортёра `{module_path}` — \
                         единственное исключение — если осиротевший файл САМ \
                         является compile-entry (тогда его видит отдельный \
                         entry-sibling scan). Типичный downstream-симптом — \
                         честный, но неверный [E_UNKNOWN_METHOD] на методе, \
                         объявленном только в осиротевшем файле.\n  \
                         fix:\n  \
                         \x20 - сделай папку-модуль `{dir}/{target_seg}/` и \
                         перенеси head-файл И все осиротевшие peer-файлы внутрь \
                         неё (D78, прецеденты std/time/civil, std/collections/vec) \
                         — рекомендуемый путь;\n  \
                         \x20 - либо поправь `module`-декларацию осиротевшего \
                         файла, если она была скопирована по ошибке;\n  \
                         \x20 - либо перенеси осиротевший файл в директорию, \
                         действительно соответствующую его собственному модулю."
                    )
                }
            }
        })?;

    // Plan 03.1 Ф.3: ужесточение repo-root looseness (§3.2). Если импорт
    // НЕ относительный и НЕ через объявленную `[dependencies]`-зависимость,
    // но резолвится в файл ДРУГОГО пакета (иной `package_root_of`), — это
    // неявный межпакетный импорт через repo-root candidate. Запрещаем:
    // межпакетные ссылки обязаны идти через `[dependencies]` (explicit
    // dependency-граф). `std` — исключение (неявный stdlib-пакет).
    if rel_root.is_none()
        && dep_root.is_none()
        && imp.path.first().map(|s| s != "std").unwrap_or(false)
    {
        if let (Some(ip), Some(rp)) = (
            package_root_of(importer_path),
            package_root_of(&resolved_paths[0]),
        ) {
            let ip_c = crate::source_index::canonicalize(&ip).unwrap_or_else(|| ip.clone());
            let rp_c = crate::source_index::canonicalize(&rp).unwrap_or_else(|| rp.clone());
            if ip_c != rp_c {
                let importing = import_chain.last()
                    .map(|m| m.join("."))
                    .unwrap_or_else(|| "<entry>".to_string());
                return Err(anyhow!(
                    "import `{}` пересекает границу пакета без объявления в \
                     `[dependencies]`\n  \
                     importing package: {}\n  \
                     resolved package:  {}\n  \
                     importing module:  {}\n  \
                     hint: межпакетные импорты должны быть объявлены в \
                     `[dependencies]` (Plan 03.1 §3.2) — workspace-членство \
                     само по себе не делает пакет импортируемым; для модулей \
                     своего пакета используйте путь от его корня",
                    imp.path.join("."),
                    ip_c.display(),
                    rp_c.display(),
                    importing,
                ));
            }
        }
    }

    // Plan 84 Ф.3: peer-collision — относительный импорт, резолвящийся в
    // модуль самого импортирующего файла (self-import либо peer того же
    // folder-модуля). Peers делят namespace — импорт избыточен и почти
    // наверняка ошибка. Диагностируем ДО cycle/mismatch-ошибок.
    if rel_root.is_some() {
        if let (Some(imp_mod), Some(res_mod)) = (
            extract_declared_module(importer_path),
            extract_declared_module(&resolved_paths[0]),
        ) {
            if imp_mod == res_mod {
                return Err(anyhow!(
                    "relative import резолвится в модуль `{}` — это модуль \
                     самого импортирующего файла\n  \
                     importing file: {}\n  \
                     hint: файл уже принадлежит этому модулю; peer-файлы \
                     folder-модуля делят namespace — импорт между ними не \
                     нужен (Plan 84 / D29)",
                    res_mod,
                    importer_path.display(),
                ));
            }
        }
    }

    // Plan 42 правило H + Plan 42.17 Ф.4: `internal/` boundary —
    // **filesystem-containment** check. `<owner>/internal/...` импортируем
    // ТОЛЬКО из файлов физически под `<owner>/`. Проверяем по реальному
    // пути importing-файла (`importer_path`) против реального пути
    // resolved `internal/`-модуля — не по строке import-path. Re-export
    // (`export import`) и alias обойти boundary не могут: проверяется
    // фактическое расположение файлов, а не путь, по которому дошли.
    if let Some(owner_dir) = find_internal_owner_dir(&resolved_paths[0]) {
        let importer_canon = crate::source_index::canonicalize(importer_path)
            .unwrap_or_else(|| importer_path.to_path_buf());
        let owner_canon = crate::source_index::canonicalize(&owner_dir)
            .unwrap_or_else(|| owner_dir.clone());
        if !importer_canon.starts_with(&owner_canon) {
            let importing = import_chain.last()
                .map(|m| m.join("."))
                .unwrap_or_else(|| "<entry>".to_string());
            return Err(anyhow!(
                "cannot import internal module '{}' from outside its owner\n  \
                 internal module:  {}\n  \
                 owner directory:  {}\n  \
                 importing file:   {}\n  \
                 importing module: {}\n  \
                 hint: `internal/` modules are accessible only from files \
                 under `{}` (Plan 42 rule H)",
                imp.path.join("."),
                resolved_paths[0].display(),
                owner_canon.display(),
                importer_canon.display(),
                importing,
                owner_canon.display(),
            ));
        }
    }

    // Plan 42.14 Ф.3 ([M11], history): cycle detection изначально керилась
    // по DECLARED MODULE NAME. Plan 202 Ф.1 (D78 rev-4, research 2026-07-13
    // §2а): декларация — усечённая `parent.target` форма (D29 rev-3), НЕ
    // уникальна по построению — два физически разных модуля, чьи пути
    // случайно дают одинаковый `(parent, target)`, обязаны совпасть по
    // decl (иначе `E_D78_MODULE_PATH_MISMATCH`), но это РАЗНЫЕ модули.
    // Кеинг по decl тихо ГЛОТАЛ экспорты второго (`visited`-dedup считал
    // его уже resolved) — `[M-d78-duplicate-decl-module-swallow]`.
    // `canonical_module_key` — identity по canonical filesystem path
    // (symlink/case-insensitive-safe через `Path::canonicalize`, как и
    // раньше использовалось в fallback-ветке здесь). Декларация остаётся
    // identity-check файла (`E_D78_MODULE_PATH_MISMATCH`, manifest.rs) —
    // не routing/registry key.
    let module_key: Vec<String> = canonical_module_key(&resolved_paths);

    // [M-imports-entry-folder-module-self-cycle-empty-exports] fix: check
    // the closed-set (`visited`) BEFORE the open-set (`in_progress`) guard.
    // For every module OTHER than the entry, membership in these two sets
    // is mutually exclusive by construction (`resolve_one` always does
    // `in_progress.remove(&module_key); visited.insert(module_key, ...)`
    // atomically at the end, never leaving both set simultaneously) — so
    // this reorder is a strict no-op for them, whatever branch fires first
    // fires identically to before.
    //
    // The ONE deliberate exception is the entry module: `entry_key` is
    // pre-seeded into `visited` (with its real, already-known export names
    // — the entry's own AST is fully parsed before any import resolution
    // starts, so its export surface needs no recursion to compute) AND kept
    // in `in_progress` for the whole resolve (root of the DFS). Before this
    // fix, a file transitively reached via CU auto-injection (e.g. `.ptr()`
    // → `needs_vec_injection` → `std.collections.vec` → its peer's own
    // implicit prelude, deferred to `pending_peer_preludes` → …→ a module
    // that plainly `import`s the entry module back) hit the `in_progress`
    // guard below FIRST and returned with an EMPTY `visible_acc` for the
    // entry's exports — even though the entry is not "still being
    // resolved" in any meaningful sense (its items were sitting fully
    // parsed in `module.items` the whole time). Checking `visited` first
    // lets that lookup succeed instead, exactly like the diamond-dep dedup
    // branch below already does for any other module.
    if let Some(module_exports) = visited.get(&module_key) {
        // Closed-set (or, for the entry, the pre-seeded export cache):
        // items already merged (or, for the entry, always available) — skip
        // the recursive resolve, just populate visible_acc with the
        // module's exported names filtered by this import's selector. This
        // is also needed when user code has an explicit `import X` and X
        // was already loaded transitively (e.g. via prelude.core importing
        // std.unicode — Plan 162 Ф.4: fixes regression where std.unicode
        // free functions were invisible to explicit user imports because
        // prelude.core had already added std.unicode to visited).
        for exported_name in module_exports {
            if import_selects(imp, exported_name) {
                visible_acc.insert(exported_name.clone());
            }
        }
        return Ok(());
    }

    // Plan 162 Ф.2: cycle guard — когда модуль уже находится в стеке
    // DFS (in_progress), это цикл импортов. Вместо stack-overflow или
    // ошибки — ранний возврат Ok(()), позволяя циклу завершиться с теми
    // декларациями, которые уже собраны. Это «collect-first» guard:
    // сигнатуры уже в merged_items (из предыдущих итераций); тела
    // разрешаются после полного сбора. Межмодульные циклы разрешены
    // (D29 rev-5, Plan 162), как peer-циклы в Rule D (Plan 42).
    //
    // Note: this guard now only ever fires for a module that is genuinely
    // mid-resolution (its own items are still being iterated in the peer
    // loop below, `module_exports_cache` incomplete) — the entry module
    // short-circuits via the `visited` check above instead, so a real
    // two-way cycle (A imports B, B imports A, neither is the entry) still
    // hits exactly this branch with an empty `visible_acc`, unchanged.
    //
    // Предыдущее поведение (Plan 35 Ф.1 / D29 pre-rev5): Err("import cycle
    // detected") — оставлено ниже в виде legacy-комментария; удалить
    // можно после Ф.3 (method-resolution-by-type) когда cycle-semantics
    // полностью valидированы через тесты.
    if in_progress.contains(&module_key) {
        // Plan 162 Ф.2: cycle detected → early Ok(()) (cycle guard).
        // Позволяем циклу разрешиться: декларации уже собраны.
        return Ok(());
    }

    in_progress.insert(module_key.clone());
    import_chain.push(imp.path.clone());

    // Plan 162 Ф.4: collect all exportable names from this module (across
    // all peers) to cache in visited map. Used by the dedup path above.
    let mut module_exports_cache: Vec<String> = Vec::new();

    // [M-imports-multipeer-cycle-partial-exports] fix (221.1 Ф.2 #14,
    // 2026-07-23 — residual gap of [M-imports-order-dependent-cycle]):
    // parsing+export-seeding and recursion+merge used to happen in ONE
    // combined pass over `resolved_paths` — per peer, seed THIS peer's
    // exports into `visited[module_key]`, THEN immediately recurse into
    // THIS peer's own imports. For a multi-peer folder-module, that meant
    // a cyclic back-edge reached from an EARLY peer's recursion could see
    // the provisional `visited[module_key]` cache with only the peers
    // parsed SO FAR — any name declared only in a LATER (alphabetically
    // greater) peer was still missing, and a legal D291 cross-module cycle
    // closing at exactly that moment got a truncated `visible_acc` → false
    // "undefined identifier" (live precedent: `server`(12 peers)↔`servernet`
    // in nova-http, `serialize_response`). Fixed per D291's own stated
    // "collect-signatures-first, lazy bodies" architecture: split into TWO
    // passes over `resolved_paths` (same alphabetical order, Plan 42 rule
    // B, both passes) —
    //   PASS 1 (below): parse every peer AND seed `visited[module_key]`
    //     with ALL of them (`exported_names_from_items` needs only parsed
    //     items, no recursion) — so by the time PASS 2 starts, the
    //     provisional export cache is COMPLETE for this module, not
    //     partial-by-peer-order.
    //   PASS 2 (further below): register each `PeerFile`, recurse into each
    //     peer's own imports, and merge its items — any cyclic back-edge
    //     hit during PASS 2's recursion now sees the full cache from PASS 1.
    // `next_file_id` allocation stays in PASS 1, still walked in the same
    // alphabetical `resolved_paths` order as before — this module's own
    // peers get contiguous, ascending file_ids exactly as before (Sub-plan
    // 42.4 §3's per-peer name-resolution invariant only needs peer files to
    // each have their OWN stable id, not any particular interleaving with
    // recursively-discovered modules' ids — PASS 2's recursion now simply
    // allocates ids for transitively-imported modules AFTER all of THIS
    // module's own peer ids, instead of interleaved between them).
    struct ParsedPeer {
        peer_path: PathBuf,
        peer_canon: PathBuf,
        peer_file_id: FileId,
        peer_module: Module,
    }
    let mut parsed_peers: Vec<ParsedPeer> = Vec::with_capacity(resolved_paths.len());

    // ─── PASS 1: parse every peer, seed the FULL provisional export cache ───
    for peer_path in &resolved_paths {
        let peer_canon = {
            let _t = crate::perf_timer::PerfTimer::new("imports-peer-canon");
            // План 252 Ф.2: канонизация — по разу на файл (шаг 1), не на
            // каждый резолв. Текст ошибки сохранён: единственная причина
            // отказа здесь — путь недоступен, и его же печатал `io::Error`.
            match crate::source_index::canonicalize(peer_path) {
                Some(p) => p,
                None => {
                    // Отказ — редчайший путь; текст ошибки берём у ОС ровно
                    // так же, как раньше.
                    return Err(peer_path
                        .canonicalize()
                        .map_err(|e| {
                            anyhow!("canonicalize {}: {}", peer_path.display(), e)
                        })
                        .err()
                        .unwrap_or_else(|| {
                            anyhow!("canonicalize {}: unavailable", peer_path.display())
                        }));
                }
            }
        };

        let peer_src = {
            let _t = crate::perf_timer::PerfTimer::new("imports-peer-io");
            crate::source_index::file_text(peer_path)
                .ok_or_else(|| anyhow!("failed to read imported module {}", peer_path.display()))?
        };
        let peer_path_str = peer_path.to_string_lossy().to_string();

        // Plan 42 Sub-plan 42.4 шаг 2: allocate unique FileId для этого peer
        // и parse с этим file_id. Все tokens/spans peer'а получат этот id,
        // type-checker (шаг 3) использует для per-peer name resolution.
        let peer_file_id = *next_file_id;
        *next_file_id += 1;

        crate::imports_stats::note_parse(peer_path, peer_src.len(), false);
        let peer_module = {
            let _tp = crate::perf_timer::PerfTimer::new("imports-peer-parse");
            parser::parse_with_file_id(&peer_src, peer_file_id)
                .map_err(|d| {
                    let (line, col) = byte_to_line_col(&peer_src, d.span.start);
                    anyhow!(
                        "in imported module '{}' ({}): {}:{}: {}",
                        imp.path.join("."), peer_path_str, line, col, d.message)
                })?
        };

        // Plan 42.12 Ф.2: проверка module-level `#cfg(feature/target_os)`.
        // Если peer объявил inactive cfg — skip целиком (не merge items,
        // не register peer_file, не recurse imports, НЕ участвует в
        // export-seeding — неактивный peer не даёт экспортов).
        if !cfg_active(&peer_module) {
            continue;
        }

        // Plan 42.10: `_module.nv` peer — special module-config файл.
        // Его module-level attrs (Forbid / Cfg / Doc) пропагируются на
        // entry's module.attrs — applied ко всему compiled module.
        let is_module_config = peer_path.file_stem()
            .and_then(|s| s.to_str())
            .map_or(false, |stem| stem == "_module");
        if is_module_config {
            for attr in &peer_module.attrs {
                inherited_attrs.push(attr.clone());
            }
        }

        // [M-imports-order-dependent-cycle] (2026-07-20, generalizes
        // [M-imports-entry-folder-module-self-cycle-empty-exports]), now
        // completed by [M-imports-multipeer-cycle-partial-exports] above:
        // seed/extend `visited[module_key]` with THIS peer's own export
        // surface RIGHT NOW — `peer_module.items` is fully parsed already,
        // no recursion needed to know it (`exported_names_from_items` reads
        // only items, per D291's stated "collect-signatures-first" design).
        // Done for EVERY peer in PASS 1, BEFORE PASS 2 recurses into ANY of
        // this module's peers' own imports — so a cyclic back-edge reached
        // from elsewhere in the DFS (e.g. a sibling top-level import of a
        // module that mutually imports THIS one — the fmt_buf↔
        // string_builder / Ф.4R Ш1 finding, and the multi-peer
        // server↔servernet finding) sees the COMPLETE name set via the
        // `visited` check (ordered before the `in_progress` guard) instead
        // of hitting the guard and getting an empty/partial `visible_acc`.
        // `module_key` is already in `in_progress` (inserted above, before
        // this peer loop) — same "in both sets at once" precedent the
        // entry-exports fix established; harmless no-op for any module
        // never re-entered mid-resolve. Extends rather than overwrites:
        // accumulates across ALL peers of this module (PASS 1 order is
        // Plan 42 rule B — alphabetical — deterministic on every run). The
        // final, complete `module_exports_cache` computed in PASS 2 (peer
        // loop's end) replaces this provisional entry once the whole
        // module finishes — same content, just recomputed via the merge
        // loop's identical `module_has_exports`/`is_export` filter.
        {
            let _t = crate::perf_timer::PerfTimer::new("imports-exports-scan");
            let peer_export_names = exported_names_from_items(&peer_module.items);
            visited.entry(module_key.clone())
                .or_insert_with(Vec::new)
                .extend(peer_export_names);
        }

        parsed_peers.push(ParsedPeer {
            peer_path: peer_path.clone(),
            peer_canon,
            peer_file_id,
            peer_module,
        });
    }

    // ─── PASS 2: register PeerFiles, recurse into imports, merge items ───
    // Peers share namespace через merge'нутый Module.items.
    for parsed in parsed_peers {
        let peer_path = &parsed.peer_path;
        let peer_canon = parsed.peer_canon;
        let peer_file_id = parsed.peer_file_id;
        let peer_module = parsed.peer_module;

        // Регистрируем PeerFile (snapshot до recursive resolve + merge).
        // Plan 42.15: imported_item_names заполняется ниже после resolve.
        // is_entry_module = false — это peer ИМПОРТИРОВАННОГО модуля,
        // его items_here НЕ должны протекать в entry's shared_decls.
        {
            let _t = crate::perf_timer::PerfTimer::new("imports-peerfile-clone");
            peer_files.push(PeerFile {
                path: peer_canon,
                file_id: peer_file_id,
                imports: peer_module.imports.clone(),
                items_here: peer_module.items.clone(),
                imported_item_names: HashSet::new(),
                is_entry_module: false,
                // Plan 81 Ф.1: declared module name для group-isolation.
                module_name: peer_module.name.clone(),
            });
        }

        // Plan 42.15: accumulator имён items видимых ЭТОМУ peer'у через
        // его прямые imports. Передаётся в resolve_one для каждого sub —
        // resolve_one пишет туда имена items которые sub притащил.
        let mut peer_visible: HashSet<String> = HashSet::new();

        // [M-per-file-check-no-prelude-protocol-scope] (Plan 172.13 batch 4):
        // this peer needs its OWN implicit prelude, decided by ITS OWN
        // `#no_prelude`/`#prelude(..)` attrs — previously prelude was computed
        // ONCE at the top level from the ENTRY module's attrs only (see
        // `prelude_imports` in `resolve_imports_inline_ex`), so a `#no_prelude`
        // entry (e.g. std/runtime/string/core.nv) that imports a normal,
        // prelude-expecting peer (std.collections.vec) starved that peer of
        // the prelude protocols/bounds it references (`Iter`/`AsSlice`/`Next`
        // from std/prelude/{collections,protocols}.nv) — E_BOUND_UNKNOWN /
        // E_IMPL_UNKNOWN_PROTOCOL, per-file-check only (whole-CU builds hide
        // it because some OTHER non-`#no_prelude` entry in the same run
        // already pulled prelude in). `compute_prelude_imports` short-circuits
        // to empty for prelude-self modules and `#no_prelude` peers, so this
        // is a no-op for the already-correct common case. NOT resolved here
        // inline (see `pending_peer_preludes` doc on `resolve_one`'s
        // parameter) — merely queued; the caller drains the queue once every
        // top-level import target is fully `visited`, so a legitimate
        // prelude→…→this-peer cycle can never hit the `in_progress` guard
        // mid-resolution and truncate prelude's own export cache.
        if let Ok(imps) = compute_prelude_imports(&peer_module, stdlib_dir, peer_path) {
            for pi in imps {
                pending_peer_preludes.push((pi, peer_path.clone()));
            }
        }

        // Recursive: resolve transitive imports for THIS peer.
        for sub in &peer_module.imports {
            // Plan 42.15: re-export. Если peer делает `export import X`
            // (sub.is_export) — items притащенные `sub` re-export'ятся:
            // они видны не только этому peer'у, но и caller'у (тому кто
            // импортировал ЭТОТ folder-module). Собираем в отдельный acc
            // и потом мержим в caller's visible_acc если is_export.
            let mut sub_visible: HashSet<String> = HashSet::new();
            resolve_one(
                sub,
                peer_path,
                entry_dir,
                repo,
                stdlib_dir,
                visited,
                in_progress,
                import_chain,
                merged_items,
                peer_files,
                next_file_id,
                include_test_peers,
                inherited_attrs,
                &mut sub_visible,
                pending_peer_preludes,
            )?;
            // Items всегда видны самому peer'у.
            for n in &sub_visible {
                peer_visible.insert(n.clone());
            }
            // `export import` — re-export: items видны caller'у, НО через
            // селективный фильтр самого caller'а (Plan 42.17 Ф.6): если
            // caller написал `import F.{a}` — он получает только `a` из
            // re-export'ов F, не другие re-exported items.
            // Note: rename caller'а к re-exported items НЕ применяется —
            // re-exported item уже в merged_items под именем re-export'а,
            // переименовать его здесь без рассинхрона с codegen-scope
            // нельзя. Rename работает для прямых (не re-exported) imports.
            if sub.is_export {
                for n in &sub_visible {
                    if import_selects(imp, n) {
                        visible_acc.insert(n.clone());
                    }
                }
            }
        }

        // Plan 42.15: записываем собранные visible-имена в PeerFile.
        // Находим PeerFile по file_id (он был push'нут выше; recursive
        // resolve_one мог push'нуть ещё peer_files, ищем по id).
        if let Some(pf) = peer_files.iter_mut().find(|p| p.file_id == peer_file_id) {
            pf.imported_item_names = peer_visible;
        }

        // Plan 42.09: selective rename map. Если import имеет
        // `.{A as B}` — после merge item с name `A` переименовывается
        // в `B` в merged scope.
        let rename_map: std::collections::HashMap<String, String> =
            if let Some(items) = &imp.items {
                items.iter()
                    .filter_map(|it| it.alias.as_ref().map(|a| (it.name.clone(), a.clone())))
                    .collect()
            } else {
                std::collections::HashMap::new()
            };
        // Plan 81 Ф.1: opt-in visibility enforcement. Если хотя бы один
        // item в модуле помечен `export` — только exported items видны
        // caller'у (как Rust `pub` / TS `export`). Если ни один — всё
        // видно (backward-compat с std/, external fn и legacy-модулями
        // у которых нет явного export-аннотации).
        let module_has_exports = peer_module.items.iter().any(|item| match item {
            Item::Fn(f) => f.is_export,
            Item::Type(t) => t.is_export,
            Item::Const(c) => c.is_export,
            _ => false,
        });
        // Merge items from this peer (with optional rename).
        // Plan 42.15: имена merged items пишутся в `visible_acc` —
        // caller (peer/entry который написал `imp`) получает их в свой
        // visible scope. Это и есть «import притащил эти имена».
        let _tmerge = crate::perf_timer::PerfTimer::new("imports-merge");
        for item in peer_module.items {
            // Plan 81 Ф.1: извлекаем is_export вместе с именем.
            let (name, is_export) = match &item {
                Item::Type(t) => (Some(t.name.clone()), t.is_export),
                Item::Fn(f) => (Some(f.name.clone()), f.is_export),
                Item::Const(c) => (Some(c.name.clone()), c.is_export),
                // Plan 152.4: module-level `ro NAME = EXPR` lazy-static global —
                // a private (no `export` on `let`) runtime binding. Extract its
                // binder name so it can be carried across the module boundary:
                // an imported fn in the same module reads it via the lazy getter
                // emitted by emit_module. Single named binder (Ident, or a
                // single-segment unit Variant for the UPPER_CASE form), non-ghost.
                Item::Let(l) if !l.is_ghost => {
                    let n = match &l.pattern {
                        crate::ast::Pattern::Ident { name, .. } => Some(name.clone()),
                        crate::ast::Pattern::Variant {
                            path,
                            kind: crate::ast::VariantPatternKind::Unit,
                            ..
                        } if path.len() == 1 => Some(path[0].clone()),
                        _ => None,
                    };
                    (n, false)
                }
                // Plan 57: bench не экспортируется (как test/lemma). ghost let —
                // spec-only, не emit'ится в codegen.
                Item::Test(_) | Item::Bench(_) | Item::Let(_) | Item::Lemma(_) => (None, false),
            };
            match (&item, name) {
                (Item::Type(_) | Item::Fn(_) | Item::Const(_), Some(item_name)) => {
                    // Codegen completeness: ВСЕ items merge'атся в
                    // merged_items (inline expansion — exported fn может
                    // вызывать приватный helper из того же модуля).
                    // is_export + selective list влияют на visibility,
                    // но НЕ на codegen-scope.
                    let final_name = if let Some(new_name) = rename_map.get(&item_name) {
                        let renamed = rename_item(item, new_name.clone());
                        merged_items.push(renamed);
                        new_name.clone()
                    } else {
                        merged_items.push(item);
                        item_name.clone()
                    };
                    // Plan 81 Ф.1: виден caller'у если модуль не использует
                    // явную экспорт-аннотацию (!module_has_exports) ИЛИ
                    // сам item помечен export (is_export). Приватные items
                    // в export-аннотированных модулях остаются в merged_items
                    // для codegen (inline expansion), но НЕ входят в
                    // visible_acc → type-checker их не видит снаружи.
                    // Plan 42.15: selective filter (`import X.{A}`) применяется
                    // поверх visibility. Матч по оригинальному item_name;
                    // в scope кладётся final_name (renamed при alias).
                    if !module_has_exports || is_export {
                        // Plan 162 Ф.4: cache exportable names (unfiltered)
                        // for the dedup path in visited map.
                        module_exports_cache.push(item_name.clone());
                        if import_selects(imp, &item_name) {
                            visible_acc.insert(final_name);
                        }
                    }
                }
                (Item::Let(_), Some(_)) => {
                    // Plan 152.4: module-level `ro NAME = EXPR` lazy-static
                    // global. Merge into `merged_items` for codegen completeness
                    // (an exported fn from this module may read it — e.g.
                    // `ccc_of` reads `ccc_map`); the lazy getter is emitted in
                    // emit_module's §1b1-moved pass over the merged items. Not
                    // added to `visible_acc` — `let` has no `export`, so it stays
                    // module-private (only same-module peers reference it).
                    merged_items.push(item);
                }
                _ => {
                    // Test blocks / ghost let — игнорируем для imported.
                }
            }
        }
    }

    // Plan 42.14 Ф.3: pop in_progress + chain; promote module_key в
    // closed-set. Plan 202 Ф.1: все peers folder-module share один
    // module_key (canonical directory path) — diamond-dep dedup работает
    // естественно, и БЕЗ путаницы при decl-дубле из другой директории.
    // Plan 162 Ф.4: store collected exportable names alongside the key so
    // dedup-skipped imports can still populate visible_acc.
    in_progress.remove(&module_key);
    visited.insert(module_key, module_exports_cache);
    import_chain.pop();
    Ok(())
}

/// Plan 42.17 Ф.6: видит ли селективный список `imp` имя `name`?
/// `import X` (без `.{...}`) — видит всё. `import X.{a, b}` — только
/// `a`/`b`. Матч по ОРИГИНАЛЬНОМУ имени item'а; `alias` — это что
/// кладётся в scope (`final_name`), не критерий отбора.
fn import_selects(imp: &Import, name: &str) -> bool {
    match &imp.items {
        None => true,
        Some(sel) => sel.iter().any(|it| it.name == name),
    }
}

/// Plan 42.17 Ф.4: если `path` лежит внутри `.../<owner>/internal/...`,
/// возвращает `.../<owner>/` — owner-директорию для Rule H containment
/// check. None если `internal` сегмента в пути нет.
///
/// Spec D29 rev-3.1: «берётся **первый** internal сегмент» — поэтому при
/// nested `internal/` берём самый ВЕРХНИЙ. `internal` на самом верху
/// (нет родителя) → None.
fn find_internal_owner_dir(path: &Path) -> Option<PathBuf> {
    let mut cur = path;
    let mut internal_dir: Option<&Path> = None;
    while let Some(parent) = cur.parent() {
        if parent.file_name().map(|n| n == "internal").unwrap_or(false) {
            // Перезаписываем — итоговое значение = самый верхний `internal`.
            internal_dir = Some(parent);
        }
        cur = parent;
    }
    internal_dir.and_then(|d| d.parent()).map(|p| p.to_path_buf())
}

/// Plan 42.17 Ф.3: единый сканер `module a.b` декларации из исходника —
/// заменяет три копипаст-сканера (`read_module_decl` + два folder-module
/// detector'а в `test_runner.rs`).
///
/// Lightweight: первая значимая строка, без полного parse. Пропускает
/// blank / `//` / `#`-attr строки (Plan 42.16 — module-level атрибуты
/// идут ПЕРЕД `module`). Nova не имеет block-комментариев (`/* */`) —
/// лексер обрабатывает только `//`, поэтому отдельная их обработка не
/// нужна. Первая non-skip строка не `module ...` → `None`.
///
/// Возвращает имя модуля как сегменты: `module encoding.hex` →
/// `["encoding", "hex"]`. Trailing-комментарий после декларации
/// отбрасывается (`module a.b // note` → `["a", "b"]`).
pub fn scan_module_decl(src: &str) -> Option<Vec<String>> {
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("module ") {
            let decl = rest.trim().split_whitespace().next().unwrap_or("");
            if decl.is_empty() {
                return None;
            }
            return Some(decl.split('.').map(|s| s.to_string()).collect());
        }
        // Первая значимая строка не `module` — декларации нет.
        return None;
    }
    None
}

/// Plan 42.14 Ф.3 ([M11]): cycle-detection key — declared module name
/// (не canonical path). Тонкая обёртка над `scan_module_decl`.
fn read_module_decl(path: &Path) -> Option<Vec<String>> {
    // План 252 Ф.2 шаг 1: читается ТОЛЬКО заголовок. Чтение файла целиком
    // ради одной строки `module` и стоило 2459 с в замере Ф.1.
    let (head, truncated) = crate::source_index::header_text(path)?;
    match scan_module_decl(&head) {
        Some(d) => Some(d),
        // Заголовок оборван, а объявление не найдено — единственный случай,
        // когда ответ мог бы разойтись с полным чтением. Дочитываем.
        None if truncated => scan_module_decl(&crate::source_index::file_text(path)?),
        None => None,
    }
}

/// План 252: объявления `module` всех обычных `.nv` каталога, посчитанные
/// один раз на каталог и сверяемые свежим снимком (`dir_derived`).
///
/// Зачем: `resolve_module_paths` и entry-sibling-скан спрашивают «что
/// объявляет вон тот сосед» для КАЖДОГО файла каталога и делают это на
/// каждый импорт каждого компилируемого файла. По отдельности это N чтений
/// (или, с кэшем содержимого, N вызовов `stat`); здесь — один `read_dir`.
/// Порядок — как у [`crate::source_index::nv_files`] (сортировка по пути).
fn dir_module_decls(dir: &Path) -> std::sync::Arc<Vec<(PathBuf, Option<Vec<String>>)>> {
    crate::source_index::derived(dir, "module-decls", || {
        crate::source_index::nv_files(dir)
            .iter()
            .map(|p| (p.clone(), read_module_decl(p)))
            .collect()
    })
}

/// То же для «точечной» формы объявления (`std.prelude.core`), которую
/// использует [`extract_declared_module`] — у неё СВОЙ сканер, поэтому
/// результат отдельный, а не производный от [`dir_module_decls`].
fn dir_declared_dotted(dir: &Path) -> std::sync::Arc<Vec<(PathBuf, Option<String>)>> {
    crate::source_index::derived(dir, "module-decls-dotted", || {
        crate::source_index::nv_files(dir)
            .iter()
            .map(|p| (p.clone(), extract_declared_module(p)))
            .collect()
    })
}

/// Plan 202 Ф.1 (D78 rev-4): module-registry identity key — canonical
/// **filesystem path**, NOT declaration.
///
/// `resolved_paths` is whatever [`resolve_module_paths`] returned for one
/// import target: either a single file (single-file module) or every peer
/// `.nv` file of a folder-module. Every peer of a given physical
/// folder-module resolves to the SAME key (all peers share one canonical
/// directory); a genuine single-file module keys off the file itself. This
/// is stable regardless of WHICH peer happens to be `resolved_paths[0]`
/// (alphabetical sort order is irrelevant — folder identity is derived from
/// the parent directory, not from any one file in it).
///
/// Two DIFFERENT physical modules whose D29 rev-3 `parent.target`
/// declarations happen to coincide (research 2026-07-13 §2а — e.g.
/// `src/a/neg/x.nv` and `src/b/neg/x.nv`, both forced to declare `module
/// neg.x`) get DIFFERENT keys here, because they live under different
/// canonical directories. This is the fix for
/// `[M-d78-duplicate-decl-module-swallow]`: before Plan 202, the registry
/// keyed by declaration, so the second same-decl module's `visited`/
/// `in_progress` entry silently deduped against the first and its exports
/// vanished. The declaration remains a pure identity-check
/// (`E_D78_MODULE_PATH_MISMATCH`, `manifest.rs`) — never a routing/registry
/// key (D78 rev-3 «Свойства» п.4, unchanged by this fix).
///
/// `canonicalize()` resolves symlinks and (on case-insensitive filesystems)
/// normalizes to the on-disk casing, so two spellings of the same physical
/// path collapse to one key — this mirrors the fallback branch this
/// function replaces (used historically only when the decl-scan failed).
pub(crate) fn canonical_module_key(resolved_paths: &[PathBuf]) -> Vec<String> {
    let _t = crate::perf_timer::PerfTimer::new("imports-canonkey");
    debug_assert!(!resolved_paths.is_empty(), "caller must guard empty resolved_paths");
    if resolved_paths.is_empty() {
        return Vec::new();
    }
    // План 252 Ф.2: ключ зависит ТОЛЬКО от `resolved_paths[0]` (см. doc выше:
    // якорь берётся от него одного), поэтому считается по разу на файл, а не
    // на каждый импорт. Это перенос `imports-canonkey` в шаг 1 алгоритма.
    let head = &resolved_paths[0];
    crate::source_index::derived_for_path(head, "canonical-module-key", || {
        let anchor: PathBuf = if is_peer_group_member(head) {
            head.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| head.clone())
        } else {
            head.clone()
        };
        let canon = crate::source_index::canonicalize(&anchor).unwrap_or(anchor);
        vec![canon.to_string_lossy().to_string()]
    })
    .as_ref()
    .clone()
}

/// Plan 42 D29 rev-3 / Plan 81 Ф.10: is `path` a peer of a folder-module?
///
/// Folder-module = every `.nv` file in `path`'s parent directory declares
/// the **same** `module X`. A single-file module is the opposite: each
/// file declares its own unique module. Lightweight — scans only the
/// first `module` line of each peer (no full parse), and filters
/// OS-suffix peers (`_windows.nv` …) inactive for the current target so
/// they do not skew the detection.
///
/// Canonical detector (Plan 42.17 Ф.3 consolidation). Used by
/// `manifest::check_module_path` — so `nova check` / `nova build` validate
/// a folder-module *entry* against the folder-module D29 rule rather than
/// the single-file rule — and by the test-runner directory walk.
pub fn is_folder_module_peer(path: &Path) -> bool {
    let _t = crate::perf_timer::PerfTimer::new("imports-folder-detect");
    let parent = match path.parent() {
        Some(p) => p,
        None => return false,
    };
    // План 252: вердикт зависит ТОЛЬКО от содержимого каталога (имена его
    // `.nv`-файлов + их строки `module`), поэтому кэшируется целиком и
    // сверяется свежим снимком каталога — один `read_dir` вместо N чтений.
    // `current_target_os()` в ключ не входит: он неизменен в пределах
    // процесса (переменная окружения читается один раз).
    *crate::source_index::derived(parent, "folder-module-peer", || {
        compute_is_folder_module_peer(parent)
    })
}

fn compute_is_folder_module_peer(parent: &Path) -> bool {
    let target = current_target_os();
    // Пустой список (каталога нет / в нём нет `.nv`) даёт пустой `decls` и
    // тот же `false`, что старая ветка `Err(_) => return false`.
    let listing = crate::source_index::nv_files(parent);
    let entries: Vec<PathBuf> = {
        listing
            .iter()
            .cloned()
            .filter(|p| {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    // [M-d376-slow-suffix-folder-module-peer-merge]: peel
                    // `_slow` too (canonical order) before the OS-target
                    // check — this detector never gated on test/slow mode,
                    // only classifies peer-group membership, so `_test` and
                    // `_slow` peers both stay unconditionally in scope here;
                    // only the OS-suffix classification needed the fix.
                    if !peer_active_for_target(peer_core_stem(stem), target) {
                        return false;
                    }
                }
                true
            })
            .collect()
    };
    let folder_name = match parent.file_name().and_then(|s| s.to_str()) {
        Some(n) => n,
        None => return false,
    };
    // Read all peer declarations.
    let mut decls: Vec<Vec<String>> = Vec::with_capacity(entries.len());
    for entry in &entries {
        let src = match crate::source_index::file_text(entry) {
            Some(s) => s,
            None => return false,
        };
        match scan_module_decl(&src) {
            Some(d) => decls.push(d),
            None => return false,
        }
    }
    // All files must agree on the same declaration.
    let first = match decls.first() {
        Some(d) => d,
        None => return false,
    };
    if !decls.iter().all(|d| d == first) {
        return false;
    }
    // Peer-module: declaration's last segment == folder name (not file name).
    first.last().map(|s| s.as_str()) == Some(folder_name)
}

/// Plan 202 Ф.1/Ф.2: does `path` share a physical peer-group identity with
/// its siblings — either the existing folder-module form
/// ([`is_folder_module_peer`]) or the NEW root-peer form (D78 rev-4: a
/// `.nv` file directly in a package's `source_root`, declaring the
/// single-segment `module <package>` — peer of the package's root module)?
///
/// Used ONLY by [`canonical_module_key`] to decide whether to anchor
/// registry identity on the shared parent directory or on the file itself.
/// Deliberately kept SEPARATE from `is_folder_module_peer` (which drives
/// D78 declaration-shape validation via `manifest::check_module_path` and
/// must keep its existing folder-name-based contract untouched — root-peer
/// declaration validation is the independent
/// `manifest::expected_root_peer_decl` check). `is_folder_module_peer`'s
/// folder-name heuristic can miss a root-peer group when the OS directory
/// name differs from the package name (the common case — e.g. directory
/// `nova-tls/` for `[package] name = "tls"`), which is exactly why this
/// wrapper exists.
fn is_peer_group_member(path: &Path) -> bool {
    if is_folder_module_peer(path) {
        return true;
    }
    let Some(parent) = path.parent() else { return false; };
    let Some(decl) = read_module_decl(path) else { return false; };
    if decl.len() != 1 {
        return false;
    }
    let Some(manifest) = crate::manifest::find_manifest(path) else { return false; };
    if decl[0] != manifest.package_name {
        return false;
    }
    match (
        crate::source_index::canonicalize(parent),
        crate::source_index::canonicalize(&manifest.source_root),
    ) {
        (Some(p), Some(r)) => p == r,
        _ => false,
    }
}

/// Plan 202 Ф.2 (D78 rev-4 "root peers"): collect `.nv` files directly in
/// `source_root` that declare the single-segment `module <package_name>` —
/// the peers of the package's root module (aliases Rust's `lib.rs`).
/// Mirrors the peer-collection filters used elsewhere in this file
/// (`_test.nv` peers only in test mode, OS-suffix peers only for the
/// current target), scanning `source_root` itself instead of a named
/// subfolder. Returns `None` (caller falls through to the generic
/// single-file/folder candidate search) if no file declares the root-peer
/// form — an ordinary package with no root peers is completely untouched.
fn collect_root_peers(
    source_root: &Path,
    package_name: &str,
    include_test_peers: bool,
) -> Option<Vec<PathBuf>> {
    let target = current_target_os();
    // План 252: перечисление через кэш с проверкой отпечатка. Отсутствующий
    // каталог даёт пустой список и тот же `None`, что старое `.ok()?`.
    let listing = dir_module_decls(source_root);
    let root_decl = [package_name.to_string()];
    let mut peers: Vec<PathBuf> = listing
        .iter()
        .filter(|(_, decl)| decl.as_deref() == Some(&root_decl[..]))
        .map(|(p, _)| p.clone())
        .filter(|p| {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                // [M-d376-slow-suffix-folder-module-peer-merge]: this
                // collects root-peers for an IMPORTED package (never for
                // compiling that package's own entry — that path is the
                // separate entry-sibling scan in `resolve_imports_inline_ex`,
                // which has its own `_slow`-aware predicate) — so `_slow`
                // peers are always excluded here, same as `include_test_peers
                // = false` would exclude `_test` peers.
                if !peer_file_included(stem, include_test_peers, false, target) {
                    return false;
                }
            }
            true
        })
        .collect();
    if peers.is_empty() {
        return None;
    }
    peers.sort();
    Some(peers)
}

/// Plan 42.09: rename item (Type/Fn/Const) при selective re-import.
/// `import X.{A as B}` → A in module X становится B в importing module.
fn rename_item(item: Item, new_name: String) -> Item {
    match item {
        Item::Type(mut t) => {
            t.name = new_name;
            Item::Type(t)
        }
        Item::Fn(mut f) => {
            f.name = new_name;
            Item::Fn(f)
        }
        Item::Const(mut c) => {
            c.name = new_name;
            Item::Const(c)
        }
        other => other,
    }
}

/// Plan 42 правило L: suggest module name через scan parent dir.
/// Если в parent dir есть похожие .nv files или folders — предложить
/// «did you mean ...?». Возвращает «\n  hint: ...» string или empty.
fn suggest_module_name(
    parts: &[String],
    entry_dir: &Path,
    repo: &Path,
    _stdlib_dir: &Path,
) -> String {
    if parts.is_empty() {
        return String::new();
    }
    // Scan parent dir of expected path в entry_dir / repo.
    let target = parts.last().cloned().unwrap_or_default();
    let parent_parts = &parts[..parts.len() - 1];
    let parent_rel: PathBuf = parent_parts.iter().collect();
    let mut candidates: Vec<String> = Vec::new();
    for root in [entry_dir, repo] {
        let dir = root.join(&parent_rel);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        candidates.push(name.to_string());
                    }
                } else if path.extension().and_then(|s| s.to_str()) == Some("nv") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        candidates.push(stem.to_string());
                    }
                }
            }
        }
    }
    // Cheap similar-name match: case-insensitive substring or prefix.
    let target_lower = target.to_lowercase();
    let close: Vec<String> = candidates
        .iter()
        .filter(|c| {
            let cl = c.to_lowercase();
            cl == target_lower || cl.starts_with(&target_lower) || target_lower.starts_with(&cl)
        })
        .cloned()
        .collect();
    if close.is_empty() {
        return String::new();
    }
    let suggestion = close
        .iter()
        .take(3)
        .map(|c| {
            let mut p = parent_parts.to_vec();
            p.push(c.clone());
            p.join(".")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("\n  hint: did you mean `{}`?", suggestion)
}

/// Plan 42.12 Ф.2: enabled features set (через `NOVA_FEATURES=foo,bar` env
/// или `--features` CLI flag). Empty if нет features.
pub fn enabled_features() -> HashSet<String> {
    if let Ok(s) = std::env::var("NOVA_FEATURES") {
        s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
    } else {
        HashSet::new()
    }
}

/// Plan 42.14 Ф.1: рекурсивная оценка одного `#cfg` predicate.
/// `any` — OR, `all` — AND, `not` — negation.
pub fn eval_cfg_predicate(
    pred: &crate::ast::CfgPredicate,
    target: &str,
    features: &HashSet<String>,
) -> bool {
    use crate::ast::CfgPredicate as P;
    match pred {
        P::Feature(name) => features.contains(name),
        P::TargetOs(os) => match os.as_str() {
            "windows" => target == "windows",
            "linux" => target == "linux",
            "macos" => target == "macos",
            "unix" | "posix" => target == "linux" || target == "macos" || target == "unix",
            _ => false, // unknown target = never matches
        },
        P::Any(preds) => preds.iter().any(|p| eval_cfg_predicate(p, target, features)),
        P::All(preds) => preds.iter().all(|p| eval_cfg_predicate(p, target, features)),
        P::Not(inner) => !eval_cfg_predicate(inner, target, features),
    }
}

/// Plan 42.12 Ф.2 + 42.14 Ф.1: peer module active при current target/features?
/// Проверяет все `#cfg` атрибуты — если хоть один inactive → peer inactive.
/// (AND semantic между разными `#cfg` атрибутами; внутри одного — `any/all/not`.)
fn cfg_active(module: &Module) -> bool {
    let target = current_target_os();
    let features = enabled_features();
    for attr in &module.attrs {
        if let crate::ast::ModuleAttrKind::Cfg(pred) = &attr.kind {
            if !eval_cfg_predicate(pred, target, &features) {
                return false;
            }
        }
    }
    true
}

/// Plan 42.12 Ф.1: target OS для filename suffix filtering.
/// Default — host OS (cfg!(target_os) at compile time of nova-codegen).
/// Override через `NOVA_TARGET_OS` env var (Ф.1 minimal — без CLI flag).
pub fn current_target_os() -> &'static str {
    // Override через env var — валидируем против известных значений и
    // возвращаем `&'static str` literal (без Box::leak: невалидное имя
    // никогда не матчится, "unknown" честнее утёкшей мусорной строки).
    if let Ok(t) = std::env::var("NOVA_TARGET_OS") {
        return match t.as_str() {
            "windows" => "windows",
            "linux" => "linux",
            "macos" => "macos",
            "unix" | "posix" => "unix",
            _ => "unknown",
        };
    }
    if cfg!(target_os = "windows") { "windows" }
    else if cfg!(target_os = "linux") { "linux" }
    else if cfg!(target_os = "macos") { "macos" }
    else if cfg!(target_family = "unix") { "unix" }
    else { "unknown" }
}

/// Plan 42.12 Ф.1: filename suffix filter для peer files.
/// Returns Some(target) если filename имеет recognized suffix (`_windows.nv`,
/// `_linux.nv`, `_macos.nv`, `_unix.nv`, `_posix.nv`); None если нет suffix.
fn file_target_suffix(stem: &str) -> Option<&'static str> {
    // Order matters: check more specific suffixes first.
    // `_test` тоже может быть в stem'е — мы фильтруем после _test stripping
    // в caller, так что здесь работаем с already-stripped stem.
    if stem.ends_with("_windows") { Some("windows") }
    else if stem.ends_with("_linux") { Some("linux") }
    else if stem.ends_with("_macos") { Some("macos") }
    else if stem.ends_with("_unix") { Some("unix") }
    else if stem.ends_with("_posix") { Some("posix") }
    else { None }
}

/// Public wrapper для test_runner walker.
pub fn peer_active_for_target_pub(stem: &str, target: &str) -> bool {
    peer_active_for_target(stem, target)
}

/// Plan 42.12 Ф.1: peer file active для current target?
/// - Без suffix → активен всегда.
/// - С suffix → активен если target matches:
///   - `_windows` ↔ windows
///   - `_linux` ↔ linux
///   - `_macos` ↔ macos
///   - `_unix` ↔ linux OR macos (POSIX-like, без bsd для simplicity)
///   - `_posix` ↔ linux OR macos (синоним _unix)
fn peer_active_for_target(stem: &str, target: &str) -> bool {
    match file_target_suffix(stem) {
        None => true,
        Some("windows") => target == "windows",
        Some("linux") => target == "linux",
        Some("macos") => target == "macos",
        Some("unix") | Some("posix") => target == "linux" || target == "macos" || target == "unix",
        Some(_) => true,
    }
}

/// [M-d376-slow-suffix-folder-module-peer-merge]: shared peer-inclusion
/// predicate. Every folder/root-peer scan in this file used to re-derive
/// the same `_test`-strip + OS-target check inline (5 call sites) and NONE
/// of them knew about D376's `_slow` suffix — a `*_slow.nv` peer sitting
/// beside a folder-module (co-equal peer files, e.g. nova-tls's `src/`
/// root-peers) was pulled into every other entry's compile-unit and its
/// `Item::Test` ran on every plain `nova test`, defeating the slow-lane
/// entirely for folder-modules (the discovery walker,
/// `test_runner::walk_nv_filtered_ex`, already excludes `_slow` entries
/// correctly — this predicate is the peer-merge-side counterpart, reusing
/// `test_runner::is_slow_file_stem` as the single source of truth for what
/// "_slow" means rather than re-deriving it here).
///
/// Peels suffixes in the canonical outermost-to-innermost order
/// `<core>[_<os>][_test][_slow]` (`_slow` peeled FIRST, matching
/// `walk_nv_filtered_ex`), then `_test`, then resolves the OS-suffix on
/// whatever core remains.
///
/// - `include_test_peers`: Plan 42 правило F gate for `_test`-suffixed peers.
/// - `include_slow_peers`: D376 gate for `_slow`-suffixed peers. Pass `true`
///   only when the CU's own entry file is itself `_slow` (its module peers
///   then merge exactly as before this fix — "peers merge as usual"); pass
///   `false` for every *import/library* resolution path (a `_slow` file is
///   a self-contained slow-lane entry, never a legitimate dependency of
///   someone else's compile-unit).
///
/// Peel both the `_slow` (outermost) and `_test` peer suffixes, in the
/// canonical order, leaving `core[_<os>]` — the part `peer_active_for_target`
/// classifies. Factored out of [`peer_file_included`] so call sites that
/// classify peer-group *membership* unconditionally (e.g.
/// `is_folder_module_peer`, which never gated on test/slow mode) can reach
/// the correctly-peeled core for the OS-suffix check without adopting
/// `peer_file_included`'s inclusion/exclusion gating. Fixes a latent gap
/// found alongside D376: an OS-suffixed slow file (`repro_windows_slow.nv`)
/// previously reached `peer_active_for_target` with `_slow` still attached
/// (only `_test` was stripped), so `_windows`/`_linux`/etc gating silently
/// no-op'd for any peer combining an OS suffix with `_slow`.
fn peer_core_stem(stem: &str) -> &str {
    let stem_no_slow = crate::test_runner::strip_slow_suffix(stem);
    stem_no_slow.strip_suffix("_test").unwrap_or(stem_no_slow)
}

/// Returns `true` iff the peer should be INCLUDED in the scan.
fn peer_file_included(
    stem: &str,
    include_test_peers: bool,
    include_slow_peers: bool,
    target: &str,
) -> bool {
    let stem_no_slow = crate::test_runner::strip_slow_suffix(stem);
    if !include_slow_peers && stem_no_slow != stem {
        return false;
    }
    if !include_test_peers && stem_no_slow.strip_suffix("_test").unwrap_or(stem_no_slow) != stem_no_slow {
        return false;
    }
    peer_active_for_target(peer_core_stem(stem), target)
}

/// Plan 42 Ф.2: resolve module to **list** of peer files (folder-module)
/// или single file. Returns `Vec<PathBuf>` alphabetically sorted (правило B).
///
/// Plan 42.08 Ф.2: возвращает `ResolveErr::Ambiguous` если `X.nv` И `X/`
/// (с direct .nv) сосуществуют — раньше silent None → generic "cannot find".
///
/// Plan 42.12 Ф.1: filter peer files по filename suffix vs current target.
///
/// Resolution order:
/// 1. Try single-file `<...>/parts.nv` (legacy behaviour).
/// 2. If not found, try folder `<...>/parts/` — collect все `*.nv` файлы
///    в этой папке (non-recursive, alphabetical sort).
/// 3. Conflict (file exists AND folder with .nv files exists) → `Err(Ambiguous)`.
///
/// Каждый search root (entry_dir / repo / stdlib_dir) проверяется в
/// порядке.
#[derive(Debug, Clone)]
pub(crate) enum ResolveErr {
    /// Не найдено — caller emit'ит «cannot find module» с suggestions.
    NotFound,
    /// `X.nv` и `X/` (с direct .nv) сосуществуют — ambiguous.
    Ambiguous { file: PathBuf, folder: PathBuf },
    /// Plan 81 Ф.4: путь импорта не совпадает по регистру с именем
    /// файла/папки на диске. На case-insensitive ФС (Windows, macOS
    /// default) такой импорт резолвится, но код непортируем на Linux.
    CaseMismatch { requested: String, actual: String },
    /// [M-module-file-submodule-split-silent-orphan]: import резолвится в
    /// единственный файл `head` (файловый модуль, D78 «файл ИЛИ папка»),
    /// но в ТОЙ ЖЕ директории лежат ещё `.nv`-файл(ы), объявляющие ТОТ ЖЕ
    /// `module <parts>` — co-equal peers, разбросанные напрямую в общей
    /// родительской папке вместо выделенной папки-модуля `<Y>/`.
    ///
    /// До Plan 202-диагностики такие peer-файлы либо (a) молча выпадали из
    /// любого резолва этого импорта извне — их декларации были невидимы
    /// ЛЮБОМУ импортёру, кроме случая когда peer-файл сам являлся
    /// compile-entry (через отдельный entry-sibling scan в
    /// `resolve_imports_inline_ex`) — реальный кейс-баг
    /// `std/src/time/{duration,timestamp,monotonic}.nv`; либо (b), после
    /// временного маскирующего фикса `[M-blanket-crossmodule-scattered-peer-drop]`
    /// (откачен), молча подмешивались в резолв без диагностики. Оба
    /// поведения тихие; это variant делает их ГРОМКИМ, actionable
    /// compile-error вместо того чтобы либо теряться, либо мёржиться без
    /// следа.
    FileOrphan {
        /// Головной файл, в который резолвится импорт (единственный файл,
        /// чьё ИМЯ совпадает с последним сегментом импортируемого пути).
        head: PathBuf,
        /// Запрошенный путь модуля (dotted, как в `import`/`module`).
        module_path: String,
        /// Sibling-файл(ы) в той же директории, объявляющие тот же
        /// `module_path`, но не входящие в резолв (alphabetically sorted).
        orphans: Vec<PathBuf>,
    },
}

/// Plan 81 Ф.4: сверка регистра резолвнутого пути с запрошенным.
///
/// На case-insensitive ФС `import Foo.Bar` находит `foo/bar.nv`.
/// Канонизируем путь (на Windows `canonicalize` возвращает реальный
/// регистр диска) и сверяем последние `parts.len()` компонент с
/// запрошенными сегментами. `is_file` — у файла последний компонент
/// несёт расширение `.nv`, у папки — нет.
///
/// Возвращает `Some((requested, actual))` при расхождении; `None` —
/// если совпало или проверить нельзя (canonicalize не удался, путь
/// короче запрошенного — консервативно: не ошибка).
fn verify_case(path: &Path, parts: &[String], is_file: bool) -> Option<(String, String)> {
    let _t = crate::perf_timer::PerfTimer::new("imports-verify-case");
    // План 252 Ф.2 шаг 3: имя на диске берётся ИЗ ЗАПИСИ ИНДЕКСА, а не
    // добывается `fs::canonicalize` на каждый импорт. Индекс хранит имена
    // ровно так, как их вернул `read_dir`, — то же, что показывает
    // `canonicalize` на пути без символических ссылок.
    if let Some(found) = verify_case_from_index(path, parts, is_file) {
        return found.0;
    }
    verify_case_via_canonicalize(path, parts, is_file)
}

/// `Some(ответ)` — индекс дал имена всех сравниваемых сегментов и ни один из
/// каталогов цепочки не содержит символических ссылок. `None` — вопрос
/// индексу не адресуется (снимок выключен, путь вне индекса, есть ссылка):
/// [INV-TODO: №523] вызывающий обязан спросить `fs::canonicalize`, как раньше.
#[allow(clippy::type_complexity)]
fn verify_case_from_index(
    path: &Path,
    parts: &[String],
    is_file: bool,
) -> Option<(Option<(String, String)>,)> {
    let mut cur = path.to_path_buf();
    let mut on_disk: Vec<String> = Vec::with_capacity(parts.len());
    for _ in 0..parts.len() {
        let parent = cur.parent()?;
        if crate::source_index::dir_has_symlink(parent) {
            return None;
        }
        on_disk.push(crate::source_index::on_disk_name(&cur)?);
        cur = parent.to_path_buf();
    }
    on_disk.reverse();
    for (i, part) in parts.iter().enumerate() {
        let d = &on_disk[i];
        let actual: &str = if is_file && i == parts.len() - 1 {
            d.strip_suffix(".nv").unwrap_or(d)
        } else {
            d.as_str()
        };
        if actual != part {
            return Some((Some((part.clone(), actual.to_string())),));
        }
    }
    Some((None,))
}

fn verify_case_via_canonicalize(
    path: &Path,
    parts: &[String],
    is_file: bool,
) -> Option<(String, String)> {
    let canon = crate::source_index::canonicalize(path)?;
    let comps: Vec<String> = canon
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(str::to_string),
            _ => None,
        })
        .collect();
    if comps.len() < parts.len() {
        return None;
    }
    let tail = &comps[comps.len() - parts.len()..];
    for (i, part) in parts.iter().enumerate() {
        let on_disk = &tail[i];
        let actual: &str = if is_file && i == parts.len() - 1 {
            on_disk.strip_suffix(".nv").unwrap_or(on_disk)
        } else {
            on_disk.as_str()
        };
        if actual != part {
            return Some((part.clone(), actual.to_string()));
        }
    }
    None
}

/// Plan 84: корень пакета, содержащего `file` — директория ближайшего
/// `nova.toml` на уровне `file` или выше. Это граница для относительных
/// импортов: цепочка `../` не может подняться выше этой директории.
/// `pub(crate)`: переиспользуется build-пайплайном (`test_runner.rs`) для
/// FFI-агрегации по объявленным зависимостям (Plan 03.1 ext-dep FFI
/// propagation) — то же самое понятие «директория с nova.toml», что и для
/// резолва `.nv`-импортов, но нужна снаружи модуля.
pub(crate) fn package_root_of(file: &Path) -> Option<PathBuf> {
    let mut dir = file.parent()?;
    loop {
        if crate::source_index::is_file(&dir.join("nova.toml")) {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Plan 03.1 Ф.3: результат поиска первого сегмента import-пути среди
/// объявленных `[dependencies]` пакета импортирующего файла.
enum DepLookup {
    /// Имя не объявлено как зависимость — обычный intra-package резолв.
    NotADep,
    /// `path`- либо `git`-зависимость: source root дерева зависимости
    /// (для `git` — внутри checkout'а в кэше, Plan 03.1 Ф.2).
    PathDep(PathBuf),
    /// `git`-зависимость не материализовалась (clone/fetch/checkout
    /// упали либо пин не резолвится). Сообщение готово к показу.
    GitError(String),
    /// registry-версия — registry появится в Plan 03.3.
    RegistryDep(String),
    /// Запись `[dependencies]` синтаксически некорректна.
    InvalidDep(String),
    /// `path`-зависимость указывает на несуществующую директорию.
    PathMissing(String),
    /// **Plan 204 дофикс №2:** активный `[replace]`-override корневого
    /// пакета указывает на несуществующий путь — честная ошибка
    /// (`E_REPLACE_PATH_MISSING`), НЕ тихий откат на git/declared источник.
    ReplacePathMissing(String),
    /// Директория `path`-зависимости не содержит `nova.toml`.
    NoManifest(String),
    /// Имя ключа в `[dependencies]` ≠ `[package].name` зависимости.
    NameMismatch { key: String, actual: String },
    /// `[dependencies]` пакета содержит ошибку конфигурации
    /// (зарезервированное имя `std`, дубль имени). Сообщение готово к показу.
    ConfigError(String),
}

/// Plan 03.1 Ф.3: ищет `dep_name` среди `[dependencies]` пакета, которому
/// принадлежит `importer_path` (директория ближайшего `nova.toml`).
///
/// - `std` — никогда не зависимость (неявный stdlib-пакет, как Rust `std`).
/// - Для `path`-deps возвращает source root дерева зависимости.
/// - Валидирует `[dependencies]` целиком: имя `std` зарезервировано,
///   дубли имён запрещены (§3.2) — ошибка возвращается независимо от
///   того, какой именно `dep_name` ищется.
/// Plan 204 дофикс №2/№3 (D420 go-scope): true if `pkg_dir` (a package root
/// found via `package_root_of`) IS the root/main package of the CURRENT
/// build session — the package that owns `entry_dir` (the top-level
/// compiled file's directory).
///
/// Used by `lookup_dependency` for two things: (1) whether a dependency's
/// OWN `[replace]` section is honored for ITS OWN edges — only when it IS
/// root (a non-root package's own `[replace]` stays inert, Go-module
/// semantics, see `W_REPLACE_IN_DEPENDENCY`); (2) as a shortcut to reuse the
/// already-parsed `manifest` instead of reparsing root's `nova.toml` when
/// `pkg_dir` already IS root. It does **not** gate whether root's
/// `[replace]` table applies — дофикс №3 made root's `[replace]` apply to
/// ANY same-named package anywhere in the graph, direct or transitive (real
/// Go/Cargo semantics — see the `root_override` lookup below).
fn is_root_package(pkg_dir: &Path, entry_dir: &Path) -> bool {
    let Some(root) = find_root_package_dir(entry_dir) else {
        return false;
    };
    let a = crate::source_index::canonicalize(pkg_dir).unwrap_or_else(|| pkg_dir.to_path_buf());
    let b = crate::source_index::canonicalize(&root).unwrap_or_else(|| root.to_path_buf());
    a == b
}

/// Directory of the nearest `nova.toml` at or above `dir` — mirrors
/// `package_root_of`'s rule but starts from a DIRECTORY (`entry_dir` is
/// already one) instead of a file.
fn find_root_package_dir(dir: &Path) -> Option<PathBuf> {
    let mut d = dir.to_path_buf();
    loop {
        if crate::source_index::is_file(&d.join("nova.toml")) {
            return Some(d);
        }
        if !d.pop() {
            return None;
        }
    }
}

fn lookup_dependency(importer_path: &Path, dep_name: &str, entry_dir: &Path) -> DepLookup {
    if dep_name == "std" {
        return DepLookup::NotADep;
    }
    let Some(pkg_dir) = package_root_of(importer_path) else {
        return DepLookup::NotADep;
    };
    let toml = pkg_dir.join("nova.toml");
    let Some(manifest) = crate::manifest::parse_manifest(&toml, &pkg_dir) else {
        return DepLookup::NotADep;
    };
    // Plan 203 (D78 rev-4 root-peer self-reference, cross-package fix):
    // a package's OWN subfolder file absolutely self-referencing its own
    // root peers (`import <own_package_name>.{...}`, matching
    // `manifest.package_name`) must resolve against the package's OWN
    // `source_root` regardless of which OUTER entry file started this
    // resolution session — `entry_dir`/`repo` in `resolve_one` stay fixed
    // to the top-level entry for the whole session (by design, Plan 84
    // relative-import anchor semantics), so they only happen to contain
    // this package's own root when compiling FROM WITHIN it. A package
    // consumed TRANSITIVELY via another package's `[dependencies]`
    // couldn't otherwise resolve its own root-peer self-references: e.g.
    // nova-http's `src/server/server.nv` (`module http.server`) doing
    // `import http.{Method, ...}` broke once `http.server` itself was
    // pulled in by an external consumer's `[dependencies] http = ...`,
    // because the root-peer detection in `resolve_module_paths` only
    // checked `entry_dir`/`repo` — both belonging to the OUTER consumer.
    // Treating "own package name" like a reflexive dependency reuses the
    // already-correct external-dep root-peer codepath (`DepLookup::PathDep`
    // below) instead of adding a parallel resolution mechanism.
    if manifest.package_name == dep_name {
        return DepLookup::PathDep(manifest.source_root.clone());
    }
    // Валидация `[dependencies]` целиком (§3.2) — до поиска конкретной
    // записи: ошибка конфигурации должна сорвать любой импорт пакета.
    let mut seen: HashSet<&str> = HashSet::new();
    for d in &manifest.dependencies {
        if d.name == "std" {
            return DepLookup::ConfigError(format!(
                "`std` — зарезервированное имя (неявный stdlib-пакет); \
                 нельзя объявлять его в `[dependencies]`\n  \
                 nova.toml: {}",
                toml.display(),
            ));
        }
        if !seen.insert(d.name.as_str()) {
            return DepLookup::ConfigError(format!(
                "зависимость `{}` объявлена в `[dependencies]` дважды\n  \
                 nova.toml: {}",
                d.name, toml.display(),
            ));
        }
    }
    let Some(dep) = manifest.dependencies.iter().find(|d| d.name == dep_name) else {
        return DepLookup::NotADep;
    };
    // Plan 204 дофикс №3 (M-187 diamond fix, real Cargo `[patch]`/Go-module
    // semantics): [replace] declared in the build ROOT's manifest overrides
    // ANY occurrence of a package with this NAME anywhere in the graph — the
    // root's own direct edge AND any transitively-reached package's edge for
    // a same-named dependency (e.g. `http`'s own `tls = {git=...}`) — not
    // only `dep`'s own owning manifest when that manifest happens to be root.
    // Real Go `replace` in the main module's go.mod is exactly this: it
    // substitutes a module path wherever it's required in the whole build
    // list, not just among the main module's own direct requires. Дофикс №2
    // implemented a narrower reading (`is_root_package(pkg_dir, ...)` — only
    // fired when the manifest OWNING this specific edge was root) which left
    // a transitively-reached same-named package (`tls` pulled in via `http`'s
    // OWN `[dependencies]`, git-sourced) unable to pick up root's override —
    // two physically distinct `tls` copies loaded into one compile unit
    // (`examples/nova.toml`'s direct path `tls` + `http`'s nested git `tls`)
    // → the checker's canonical-path decl registry (D78 rev-4) sees them as
    // two unrelated packages declaring the identical `TlsStream.connect` →
    // `E_METHOD_REDEFINITION`, NOT a dedup (`[M-187-weather-live-tls-diamond-
    // blocked]`). A dependency's OWN `[replace]` section stays inert when
    // reached transitively (Go-scope, unchanged — see `W_REPLACE_IN_DEPENDENCY`
    // / `lockfile::collect_replace_scope_warnings`); only the BUILD ROOT's
    // `[replace]` table is ever consulted, now regardless of which manifest
    // declares the specific edge being looked up.
    //
    // A `Path` override's `rel` is always resolved relative to the manifest
    // that DECLARED the `[replace]` entry — i.e. root — never relative to
    // `pkg_dir` (which may be a transitively-reached package at a different
    // depth in the tree; joining against it would silently resolve to the
    // wrong directory whenever root and `pkg_dir` don't share a parent).
    let is_root = is_root_package(&pkg_dir, entry_dir);
    let root_dir = find_root_package_dir(entry_dir);
    // №336: `nova check`/`nova test` не зовут `lockfile::sync`/`load_pins`
    // нигде на своём пути (в отличие от `cmd_build`) — без этого git+
    // version-зависимости резолвились бы «вживую» (максимальный тег) на
    // КАЖДЫЙ прогон, полностью игнорируя закоммиченный `nova.lock.toml`.
    // Единая точка входа резолва зависимости — здесь; засеиваем
    // lock-таблицу коммитами КОРНЕВОГО (не транзитивного) пакета —
    // именно его lock описывает весь граф. Мемоизировано — дешёво звать
    // на каждый lookup.
    if let Some(rd) = root_dir.as_deref() {
        if let Err(e) = crate::lockfile::ensure_pins_loaded(rd) {
            return DepLookup::GitError(format!("nova.lock.toml: {}", e));
        }
    }
    let root_manifest = if is_root {
        None // already have it as `manifest` below — avoid a redundant reparse.
    } else {
        root_dir
            .as_deref()
            .and_then(|rd| crate::manifest::parse_manifest(&rd.join("nova.toml"), rd))
    };
    let root_override: Option<crate::manifest::DepSource> = if is_root {
        manifest.replace.get(dep_name).cloned()
    } else {
        root_manifest
            .as_ref()
            .and_then(|rm| rm.replace.get(dep_name).cloned())
    };
    let (effective, base_dir): (crate::manifest::DepSource, &Path) = match &root_override {
        Some(src) => (
            src.clone(),
            if is_root {
                pkg_dir.as_path()
            } else {
                root_dir.as_deref().unwrap_or(pkg_dir.as_path())
            },
        ),
        None => (dep.source.clone(), pkg_dir.as_path()),
    };
    match &effective {
        crate::manifest::DepSource::Path(rel) => {
            let dep_dir = base_dir.join(rel);
            if !crate::source_index::is_dir(&dep_dir) {
                // Plan 204 дофикс №2/№3: честная ошибка (не тихий откат на
                // git/declared источник) когда путь пришёл из АКТИВНОГО
                // корневого [replace]-override.
                if root_override.is_some() {
                    return DepLookup::ReplacePathMissing(dep_dir.display().to_string());
                }
                return DepLookup::PathMissing(dep_dir.display().to_string());
            }
            finalize_dep_pkg(&dep_dir, dep_name)
        }
        crate::manifest::DepSource::Git { url, pin } => {
            // Plan 03.1 Ф.2: материализуем git-зависимость в кэше и
            // дальше резолвим её как обычный пакет на диске.
            match crate::git_cache::resolve_git_dep(url, pin, None) {
                Ok(res) => finalize_dep_pkg(&res.checkout, dep_name),
                Err(e) => DepLookup::GitError(format!(
                    "git-зависимость `{}`: {}",
                    dep_name, e,
                )),
            }
        }
        crate::manifest::DepSource::Registry(v) => DepLookup::RegistryDep(v.clone()),
        crate::manifest::DepSource::Invalid(raw) => DepLookup::InvalidDep(raw.clone()),
    }
}

/// Plan 03.1 Ф.2/Ф.3: довести каталог зависимости (path-каталог либо
/// git-checkout) до `DepLookup`: проверить наличие `nova.toml`, разобрать
/// его и сверить `[package].name` с именем-ключом зависимости.
fn finalize_dep_pkg(dep_dir: &Path, dep_name: &str) -> DepLookup {
    let dep_toml = dep_dir.join("nova.toml");
    if !crate::source_index::is_file(&dep_toml) {
        return DepLookup::NoManifest(dep_dir.display().to_string());
    }
    let Some(dep_manifest) = crate::manifest::parse_manifest(&dep_toml, dep_dir) else {
        return DepLookup::NoManifest(dep_dir.display().to_string());
    };
    if dep_manifest.package_name != dep_name {
        return DepLookup::NameMismatch {
            key: dep_name.to_string(),
            actual: dep_manifest.package_name,
        };
    }
    DepLookup::PathDep(dep_manifest.source_root)
}

/// Plan 03.1 (ext-dep native/FFI propagation): manifest-директории ВСЕХ
/// объявленных `[dependencies]` пакета `pkg_dir` (директория его
/// `nova.toml`) — `path`-зависимость на диске, `git`-зависимость через
/// `git_cache` (offline-aware, с кэшем/lock-пином). Недоступная/битая
/// зависимость молча пропускается: диагностика уже даётся на этапе резолва
/// `.nv`-импорта, если зависимость реально используется модулем; здесь цель
/// иная — собрать `[ffi]`-секции ВСЕХ объявленных зависимостей для
/// build-пайплайна (§3.2: explicit dependency graph — зависимость
/// объявлена → её native-артефакты (`.c`/`.lib` из `[ffi]`/
/// `[ffi.staticlib]`) линкуются в бинарь импортёра, симметрично тому как её
/// `.nv`-модули резолвятся в компиляцию).
///
/// **Plan 193 Ф.2 gate-3 fix (2026-07-12):** НЕ переиспользует
/// `lookup_dependency`/`finalize_dep_pkg`'s `PathDep(source_root)` — тот
/// возвращает `[lib] src`-resolved `source_root` (для `.nv` module-path
/// resolution), которая расходится с `manifest_dir` (nova.toml-директория)
/// для non-trivial `[lib] src` (напр. nova-tls: `src = "src"` →
/// `source_root` = `<pkg>/src`, но `nova.toml` живёт в `<pkg>/`). Каждый
/// caller здесь (`[ffi]`-merge в test_runner.rs) делает
/// `dep_root.join("nova.toml")` — нужна именно `manifest_dir`, иначе
/// dep-`[ffi]` молча пропадает (dep_toml не находится → `continue`) и
/// build падает hard CC/link-FAIL вместо честного merge/detect-and-degrade
/// SKIP. Резолвит `manifest_dir` независимо (path join / git checkout),
/// с той же валидацией имени пакета что `finalize_dep_pkg`.
///
/// **Честный маркер v1:** только ПРЯМЫЕ зависимости — транзитивные
/// зависимости зависимостей не обходятся (см. docs/plans/03.1-*.md).
///
/// **Plan 204 дофикс №2 (go-scope invariant):** unconditionally honors
/// `effective_source` (i.e. `[replace]`) for `pkg_dir`'s OWN dependencies —
/// this is safe ONLY because every current caller passes the ROOT package
/// of the build (`test_runner.rs`: `package_root_of(opts.nv_file)`, the
/// entry file). If a future caller ever invokes this for a non-root
/// (transitively-reached) package, it would need the same root-check as
/// `lookup_dependency`'s `is_root_package` — `[replace]` must never apply
/// to a dependency's OWN manifest (Go module semantics; see D420 §2-3).
pub fn resolved_dependency_roots(pkg_dir: &Path) -> Vec<PathBuf> {
    let toml = pkg_dir.join("nova.toml");
    let Some(manifest) = crate::manifest::parse_manifest(&toml, pkg_dir) else {
        return Vec::new();
    };
    // №336: та же засветка lock-таблицы, что и в `lookup_dependency` —
    // `pkg_dir` здесь ВСЕГДА корневой пакет сборки (см. doc выше), лок
    // рядом с ним описывает весь граф. Best-effort как и остальная
    // функция (недоступная зависимость и так молча пропускается ниже) —
    // громкая диагностика битого лока живёт в `lookup_dependency`.
    let _ = crate::lockfile::ensure_pins_loaded(pkg_dir);
    let mut roots = Vec::new();
    for d in &manifest.dependencies {
        if d.name == "std" {
            continue;
        }
        // Plan 204: honor [replace] override, same as lookup_dependency.
        let effective = manifest.effective_source(d);
        let dep_dir = match &effective {
            crate::manifest::DepSource::Path(rel) => {
                let dir = pkg_dir.join(rel);
                if crate::source_index::is_dir(&dir) { Some(dir) } else { None }
            }
            crate::manifest::DepSource::Git { url, pin } => {
                crate::git_cache::resolve_git_dep(url, pin, None)
                    .ok()
                    .map(|r| r.checkout)
            }
            crate::manifest::DepSource::Registry(_) | crate::manifest::DepSource::Invalid(_) => {
                None
            }
        };
        let Some(dep_dir) = dep_dir else { continue };
        let dep_toml = dep_dir.join("nova.toml");
        let Some(dep_manifest) = crate::manifest::parse_manifest(&dep_toml, &dep_dir) else {
            continue;
        };
        if dep_manifest.package_name != d.name {
            continue;
        }
        roots.push(dep_dir);
    }
    roots
}

/// План 252 Ф.2 шаг 3 — **КАРТА МОДУЛЕЙ**: ключ запроса → разрешённые пути.
///
/// **Почему ключ такой, а не «имя модуля».** Шаг 3 плана описан как «один
/// поиск по ключу — имени модуля». Глобальная карта «имя → путь» тут была бы
/// НЕ ускорением, а сменой семантики импорта, что тот же раздел запрещает:
///
/// * резолв идёт **по пути, а не по объявлению**. Объявление `module` —
///   чистая сверка тождества (D78 rev-3 «Свойства» п.4, `check_module_path`),
///   никогда не ключ маршрутизации;
/// * одно и то же имя модуля законно объявляют РАЗНЫЕ физические модули
///   (D78 rev-4, исследование 2026-07-13 §2а: `src/a/neg/x.nv` и
///   `src/b/neg/x.nv` оба обязаны объявлять `module neg.x`). Карта по имени
///   схлопнула бы их — ровно дефект `[M-d78-duplicate-decl-module-swallow]`,
///   починенный планом 202;
/// * один и тот же `import X.Y` из разных файлов резолвится в РАЗНОЕ:
///   якорь `./`/`../`, корень пакета-зависимости и `entry_dir` входят в
///   ответ. Имя модуля этого не выражает.
///
/// Поэтому ключ карты — весь запрос резолва (сегменты импорта + якоря +
/// режим peer'ов), а значение — его ответ. Свойство, которого требует
/// приёмка, при этом выполняется буквально: повторный импорт — один поиск в
/// хэш-таблице и НОЛЬ обращений к ФС.
type ModPathKey = (
    Vec<String>,
    PathBuf,
    PathBuf,
    PathBuf,
    bool,
    Option<PathBuf>,
    Option<PathBuf>,
);
type ModPathVal = std::sync::Arc<Result<Vec<PathBuf>, ResolveErr>>;

fn modpath_map() -> &'static std::sync::Mutex<HashMap<ModPathKey, ModPathVal>> {
    static MAP: std::sync::OnceLock<std::sync::Mutex<HashMap<ModPathKey, ModPathVal>>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Сбросить карту модулей. Симметрично [`crate::source_index::reset`] — обе
/// живут ровно один прогон.
pub fn reset_module_map() {
    if let Ok(mut g) = modpath_map().lock() {
        g.clear();
    }
}

fn resolve_module_paths(
    parts: &[String],
    entry_dir: &Path,
    repo: &Path,
    stdlib_dir: &Path,
    include_test_peers: bool,
    rel_root: Option<&Path>,
    dep_root: Option<&Path>,
) -> Result<Vec<PathBuf>, ResolveErr> {
    // План 252 Ф.0: под-таймер (вложен в `imports-resolve`, время учтено
    // дважды — доли читать относительно родителя).
    let _t = crate::perf_timer::PerfTimer::new("imports-modpaths");
    crate::source_index::note_import_resolve();
    if !crate::source_index::snapshot_enabled() {
        return resolve_module_paths_inner(
            parts, entry_dir, repo, stdlib_dir, include_test_peers, rel_root, dep_root,
        );
    }
    let key: ModPathKey = (
        parts.to_vec(),
        entry_dir.to_path_buf(),
        repo.to_path_buf(),
        stdlib_dir.to_path_buf(),
        include_test_peers,
        rel_root.map(|p| p.to_path_buf()),
        dep_root.map(|p| p.to_path_buf()),
    );
    if let Ok(g) = modpath_map().lock() {
        if let Some(v) = g.get(&key) {
            return (**v).clone();
        }
    }
    let val: ModPathVal = std::sync::Arc::new(resolve_module_paths_inner(
        parts, entry_dir, repo, stdlib_dir, include_test_peers, rel_root, dep_root,
    ));
    if let Ok(mut g) = modpath_map().lock() {
        return (**g.entry(key).or_insert(val)).clone();
    }
    (*val).clone()
}

fn resolve_module_paths_inner(
    parts: &[String],
    entry_dir: &Path,
    repo: &Path,
    stdlib_dir: &Path,
    include_test_peers: bool,
    // Plan 84: для относительного импорта (`./` / `../`) caller передаёт
    // вычисленную директорию-root; `None` — обычный candidate-поиск.
    rel_root: Option<&Path>,
    // Plan 03.1 Ф.3: для импорта из объявленной `[dependencies]`-зависимости
    // caller передаёт source root дерева зависимости; первый сегмент
    // import-пути (имя пакета) при этом отбрасывается. `None` — обычный
    // intra-package резолв.
    dep_root: Option<&Path>,
) -> Result<Vec<PathBuf>, ResolveErr> {
    if parts.is_empty() {
        return Err(ResolveErr::NotFound);
    }
    let rel_path: PathBuf = parts.iter().collect();

    // Candidate search roots. Plan 84: для относительного импорта —
    // единственный root = вычисленная caller'ом директория (без
    // candidate-поиска и без std-special-case). Plan 03.1 Ф.3: для
    // импорта из зависимости — единственный root = source root дерева
    // зависимости (первый сегмент import-пути — имя пакета — отброшен).
    let roots: Vec<PathBuf> = if let Some(rr) = rel_root {
        vec![rr.to_path_buf()]
    } else if let Some(dr) = dep_root {
        vec![dr.to_path_buf()]
    } else {
        let mut rs = vec![entry_dir.to_path_buf(), repo.to_path_buf()];
        if parts[0] == "std" && parts.len() >= 2 {
            rs.push(stdlib_dir.to_path_buf());
        }
        rs
    };

    for root in &roots {
        // Plan 202 Ф.2 (D78 rev-4 "root peers"): `import <package>` (bare,
        // single segment) matching THIS candidate root's OWN package name
        // addresses the root peers directly in its source_root — `.nv`
        // files declaring the single-segment `module <package>` (peers of
        // one another, D78 rev-4 §7). Checked BEFORE the generic
        // single-file/folder candidate search below so it takes priority —
        // root peers is the newer, more specific meaning of a bare
        // package-name import; falls through untouched (`None`) for any
        // ordinary package with no root peers, or when `root` isn't itself
        // a package root (no `nova.toml`) — zero regression for every
        // existing single-segment import (`import vec_iter`, …).
        if parts.len() == 1 && rel_root.is_none() && dep_root.is_none() {
            if let Some(m) = crate::manifest::parse_manifest(&root.join("nova.toml"), root) {
                if m.package_name == parts[0] {
                    if let Some(peers) =
                        collect_root_peers(&m.source_root, &m.package_name, include_test_peers)
                    {
                        return Ok(peers);
                    }
                }
            }
        }

        // Plan 203 (D78 rev-4 root peers, dep_root fix): a bare single-
        // segment import addressed THROUGH `dep_root` (external
        // `[dependencies]` OR the reflexive self-package-name case added by
        // `lookup_dependency`, both mean `root == dep_root == the target
        // package's own source_root` and `parts[0] == that package's own
        // name`) must resolve via the SAME filtered `collect_root_peers`
        // used above — NOT the generic single-file/folder search below.
        // The generic path computes `local_rel = parts[1..]` (empty for a
        // single segment) and treats `root` itself as the "folder", listing
        // EVERY direct `.nv` file in source_root unfiltered by module
        // declaration — for a "mixed root" package (root peers coexisting
        // with independent single-file modules, D78 rev-4 §7 "смешанный
        // корень", e.g. `spec_tests/conformance/d78_root_peers/util.nv`)
        // this wrongly pulls the independent modules in too, double-
        // defining their items (`redefinition of 'nova_fn_...'` at
        // codegen). `collect_root_peers` already does the correct
        // decl-filtered collection (mirrors the `rel_root.is_none() &&
        // dep_root.is_none()` branch above, which never had this bug
        // because it re-derives `m.source_root` from a real `nova.toml`
        // lookup instead of trusting `root` blindly).
        if parts.len() == 1 && dep_root.is_some() {
            if let Some(peers) = collect_root_peers(root, &parts[0], include_test_peers) {
                return Ok(peers);
            }
        }

        // Translate path: для stdlib_dir пропускаем первый `std` segment;
        // Plan 03.1 Ф.3: для dep_root пропускаем первый сегмент (имя
        // пакета-зависимости) — файлы лежат от source root зависимости.
        let local_rel: PathBuf = if root == stdlib_dir && parts[0] == "std" {
            parts[1..].iter().collect()
        } else if dep_root.is_some() {
            parts[1..].iter().collect()
        } else {
            rel_path.clone()
        };

        // Plan 03.1 Ф.3: `verify_case` сверяет с диском ТОЛЬКО сегменты,
        // реально соответствующие компонентам пути. Для stdlib и для
        // импорта из зависимости первый сегмент (`std` / имя пакета) —
        // логический, не имя директории, и в `local_rel` он отброшен.
        let strip_first =
            (root == stdlib_dir && parts[0] == "std") || dep_root.is_some();
        let verify_parts: &[String] =
            if strip_first { &parts[1..] } else { &parts[..] };

        let single_file = root.join(local_rel.with_extension("nv"));
        let folder = root.join(&local_rel);

        let file_exists = crate::source_index::is_file(&single_file);
        let folder_exists = crate::source_index::is_dir(&folder);

        if file_exists && folder_exists {
            // Check folder has direct .nv files — only then it's ambiguous.
            // If folder только contains sub-folders without direct .nv,
            // we treat it as namespace-container (rule E).
            // План 252: перечисление через кэш с проверкой отпечатка.
            let has_direct_nv = !crate::source_index::nv_files(&folder).is_empty();
            if has_direct_nv {
                // Plan 62.A: разрешённый pattern — facade file `X.nv` +
                // child-namespace folder `X/<sub>.nv` (where каждый sub
                // declares `module X.<sub>`, not `module X`). В этом случае
                // file — parent-module facade, folder peers — child
                // modules, не peers of file. Это специально для
                // splittable prelude design (Plan 62 §«Splittable
                // structure»), но general-purpose: применимо к любому
                // `<X>.nv` + `<X>/<sub>.nv` case.
                //
                // Detection: peek все direct .nv в folder; если ВСЕ
                // declare `module <parent>.<X>.<...>` (т.е. их declared
                // path starts with file's full path + один сегмент), —
                // это child-namespace case, не ambiguity.
                //
                // Если хоть один peer declares `module <X>` или `module
                // <parent>.<X>` (same path как file), — реальная
                // ambiguity, error как раньше.
                let file_module_full = parts.join(".");
                let file_module_prefix = format!("{}.", file_module_full);
                // Plan 62 cleanup (2026-05-19): rev-3 strict `parent.target`
                // means sub-modules в `X/` declare `module <X>.<sub>` (2 seg)
                // — НЕ полный `<parent_of_X>.<X>.<sub>` (3+ seg).
                // file's target (folder name) — last segment of parts.
                // Accept peer as sub-module if its declared form is either:
                //   - full path `<parent>.<X>.<sub>` (legacy rev-1 / facade)
                //   - short path `<X>.<sub>` (rev-3 strict)
                // Conflict (ambiguity) if peer declares `<X>` alone, или
                // `<parent>.<X>` (i.e. same path как file — peer of file).
                let file_target = parts.last().cloned().unwrap_or_default();
                let short_prefix = format!("{}.", file_target);
                let mut all_children = true;
                let mut any_peer = false;
                {
                    // План 252: объявления соседей — из кэша по каталогу.
                    for (_p, decl) in dir_declared_dotted(&folder).iter() {
                        any_peer = true;
                        let declared = match decl.clone() {
                            Some(d) => d,
                            None => {
                                // Не удалось извлечь module declaration —
                                // consideredambiguous (старое поведение).
                                all_children = false;
                                break;
                            }
                        };
                        // Detect peer-of-file (ambiguity) — declared is
                        // exactly file_module_full (e.g. `std.prelude`) or
                        // exactly `<X>` (e.g. `prelude`).
                        if declared == file_module_full || declared == file_target {
                            all_children = false;
                            break;
                        }
                        // Accept sub-module форм: either full prefix
                        // `<parent>.<X>.` или short prefix `<X>.`.
                        let is_full_child = declared.starts_with(&file_module_prefix);
                        let is_short_child = declared.starts_with(&short_prefix);
                        if !is_full_child && !is_short_child {
                            all_children = false;
                            break;
                        }
                    }
                }
                if !any_peer || !all_children {
                    // Plan 42.08 Ф.2: ambiguous → return explicit ResolveErr
                    // вместо silent None. Caller emit'ит clear «ambiguous module
                    // X: <file> vs <folder>» вместо generic «cannot find».
                    return Err(ResolveErr::Ambiguous {
                        file: single_file.clone(),
                        folder: folder.clone(),
                    });
                }
                // All peers — child modules. Fall through: return file as
                // single resolved path (folder peers resolve через explicit
                // `import X.<sub>` paths).
            }
        }

        if file_exists {
            // Plan 81 Ф.4: сверка регистра пути с диском.
            if let Some((requested, actual)) =
                verify_case(&single_file, verify_parts, true)
            {
                return Err(ResolveErr::CaseMismatch { requested, actual });
            }
            // [M-module-file-submodule-split-silent-orphan]: этот import
            // резолвится в единственный `single_file` (файловый модуль —
            // головной файл `<Y>.nv`). Если в ТОЙ ЖЕ директории лежат ещё
            // `.nv`-файлы, объявляющие ТОТ ЖЕ `module <verify_parts>`, —
            // это co-equal peers, разбросанные напрямую в общей папке
            // вместо выделенной папки-модуля `<Y>/` (D78). Такие peers
            // никогда не попадут в этот и любой другой внешний резолв
            // импорта (только entry-sibling scan видит их, когда peer сам
            // является compile-entry) — тихое сиротение вместо ошибки.
            // Раньше это либо молча теряло декларации (pre-Plan-202), либо
            // (после отменённого маскирующего фикса) молча подмешивало их
            // без диагностики. Громкая ошибка вместо обоих тихих исходов.
            if let Some(dir) = single_file.parent() {
                {
                    let target = current_target_os();
                    // План 252: имена + объявления соседей — из кэша по
                    // каталогу (один `read_dir` на проверку вместо N чтений).
                    let listing = dir_module_decls(dir);
                    let mut orphans: Vec<PathBuf> = listing
                        .iter()
                        .filter(|(p, _)| p != &single_file)
                        .filter(|(_, decl)| decl.as_deref() == Some(verify_parts))
                        .map(|(p, _)| p.clone())
                        .filter(|p| {
                            // [M-d376-slow-suffix-folder-module-peer-merge]:
                            // external import resolve (per comment above) —
                            // never the entry-sibling scan — so `_slow`
                            // peers are always excluded, same treatment as
                            // `_test` peers in build mode.
                            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                                return peer_file_included(stem, include_test_peers, false, target);
                            }
                            true
                        })
                        .collect();
                    if !orphans.is_empty() {
                        orphans.sort();
                        return Err(ResolveErr::FileOrphan {
                            head: single_file.clone(),
                            module_path: verify_parts.join("."),
                            orphans,
                        });
                    }
                }
            }
            return Ok(vec![single_file]);
        }

        if folder_exists {
            // Collect все *.nv files (non-recursive), alphabetical sort.
            // Plan 42 правило F: filter `*_test.nv` peers если
            // !include_test_peers (build mode).
            // Plan 42.12 Ф.1: filter peers по filename suffix vs current target.
            // [M-d376-slow-suffix-folder-module-peer-merge]: this resolves an
            // IMPORTED folder-module (`import X.Y` style) — external
            // consumer, never the package's own compile-entry (that's the
            // entry-sibling scan in `resolve_imports_inline_ex`) — so
            // `_slow` peers are always excluded, mirroring how `_test` peers
            // are excluded in build mode.
            let target = current_target_os();
            // План 252: перечисление через кэш с проверкой отпечатка.
            let mut peers: Vec<PathBuf> = crate::source_index::nv_files(&folder)
                .iter()
                .cloned()
                .filter(|p| {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        return peer_file_included(stem, include_test_peers, false, target);
                    }
                    true
                })
                .collect();
            if !peers.is_empty() {
                // Plan 81 Ф.4: сверка регистра пути с диском (папка).
                if let Some((requested, actual)) =
                    verify_case(&folder, verify_parts, false)
                {
                    return Err(ResolveErr::CaseMismatch { requested, actual });
                }
                peers.sort();
                return Ok(peers);
            }
            // Folder без .nv files (после filter) — namespace-container,
            // не module. Продолжаем поиск в других roots.
        }
    }

    Err(ResolveErr::NotFound)
}

/// Plan 62.A: lightweight extraction of `module X.Y.Z` declaration из
/// .nv file без полного парсинга. Использован в `resolve_module_paths`
/// для disambiguating file+folder coexistence (facade + child-namespace
/// pattern).
///
/// Возвращает declared module path как dotted string (e.g.
/// `"std.prelude.core"`) или `None` если:
///   - файл не читается,
///   - module declaration не найден в первых ~50 non-comment lines,
///   - syntax не распознан.
///
/// Скан: skip blank lines, line/block comments, attrs (`#stable(...)`).
/// Останавливается на первой строке начинающейся с `module `.
fn extract_declared_module(path: &Path) -> Option<String> {
    // План 252 Ф.2 шаг 1: только заголовок; при обрыве без находки — дочитать
    // (см. `read_module_decl`), чтобы ответ совпадал с полным чтением.
    let (head, truncated) = crate::source_index::header_text(path)?;
    match extract_declared_module_from(&head) {
        Some(d) => Some(d),
        None if truncated => {
            extract_declared_module_from(&crate::source_index::file_text(path)?)
        }
        None => None,
    }
}

fn extract_declared_module_from(content: &str) -> Option<String> {
    let mut in_block_comment = false;
    let mut lines_seen = 0;
    for raw_line in content.lines() {
        lines_seen += 1;
        if lines_seen > 200 {
            // module declaration MUST быть в первых ~200 lines (typically
            // в первых 30). Не нашли — bail.
            return None;
        }
        let line = raw_line.trim();
        if in_block_comment {
            if let Some(idx) = line.find("*/") {
                let rest = &line[idx + 2..].trim_start();
                if rest.is_empty() {
                    in_block_comment = false;
                    continue;
                }
                in_block_comment = false;
                // continue parsing rest of line
                if let Some(name) = try_parse_module_decl(rest) {
                    return Some(name);
                }
                continue;
            }
            continue;
        }
        if line.is_empty() || line.starts_with("//") || line.starts_with("///") {
            continue;
        }
        if line.starts_with("/*") {
            if line.contains("*/") {
                // Single-line block comment.
                continue;
            }
            in_block_comment = true;
            continue;
        }
        // Skip attrs (lines starting with `#`).
        if line.starts_with('#') {
            continue;
        }
        if let Some(name) = try_parse_module_decl(line) {
            return Some(name);
        }
        // Первый non-comment non-attr line не "module ..." — bail.
        return None;
    }
    None
}

/// Helper: если строка начинается с `module `, извлечь path как dotted
/// string. Path = sequence of `[A-Za-z_][A-Za-z0-9_]*` separated by `.`,
/// terminated whitespace/EOL/comment.
fn try_parse_module_decl(line: &str) -> Option<String> {
    let rest = line.strip_prefix("module ")?.trim_start();
    let mut path = String::new();
    let mut started_segment = false;
    for ch in rest.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            path.push(ch);
            started_segment = true;
        } else if ch == '.' && started_segment {
            path.push('.');
            started_segment = false;
        } else {
            break;
        }
    }
    if path.is_empty() || path.ends_with('.') {
        None
    } else {
        Some(path)
    }
}

#[cfg(test)]
mod entry_folder_module_tests {
    //! Plan 81 Ф.10: when the compiled entry file is itself a peer of a
    //! folder-module, `resolve_imports_inline_ex` must collect the sibling
    //! peers, register them with distinct `file_id`s, merge their items,
    //! and resolve each peer's imports into ITS OWN visible scope
    //! (Rule C — per-peer import isolation).
    use super::*;

    /// Unique scratch directory under the OS temp dir.
    fn unique_tmp(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "nova_p81_{}_{}_{}",
            tag,
            std::process::id(),
            nanos
        ))
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create_dir_all");
        }
        std::fs::write(path, content).expect("write fixture file");
    }

    #[test]
    fn entry_folder_module_collects_siblings_with_per_peer_isolation() {
        // proj/m/app.nv  — entry peer (`fn main`), uses sibling's `helper`.
        // proj/m/lib.nv  — sibling peer, imports `dep` and uses `dep_fn`.
        // proj/dep.nv    — a separate single-file module.
        let root = unique_tmp("f10");
        let proj = root.join("proj");
        let app = proj.join("m").join("app.nv");
        let lib = proj.join("m").join("lib.nv");
        let dep = proj.join("dep.nv");

        write_file(&app, "module m\n\nfn main() -> int => helper()\n");
        write_file(
            &lib,
            "module m\n\nimport dep.{dep_fn}\n\nfn helper() -> int => dep_fn()\n",
        );
        write_file(&dep, "module dep\n\nexport fn dep_fn() -> int => 7\n");

        let src = std::fs::read_to_string(&app).expect("read entry");
        let mut module = parser::parse(&src).expect("entry parses");
        // Nonexistent stdlib dir → prelude auto-import is skipped, keeping
        // this test hermetic (no dependency on the real std/ tree).
        let stdlib = root.join("no_stdlib");

        resolve_imports_inline_ex(&app, &mut module, &proj, &stdlib, false)
            .expect("entry-folder-module resolves");

        // Exactly two entry-group peers: app (MAIN_FILE_ID) + lib (sibling).
        let entry_peers: Vec<&PeerFile> = module
            .peer_files
            .iter()
            .filter(|p| p.is_entry_module)
            .collect();
        assert_eq!(
            entry_peers.len(),
            2,
            "expected entry + 1 sibling peer, got {}",
            entry_peers.len()
        );

        // The sibling got a distinct, non-MAIN file_id.
        let sib = module
            .peer_files
            .iter()
            .find(|p| p.is_entry_module && p.file_id != MAIN_FILE_ID)
            .expect("sibling peer registered");
        assert!(
            sib.path.ends_with("lib.nv"),
            "sibling peer should be lib.nv, got {}",
            sib.path.display()
        );
        assert_eq!(sib.module_name, vec!["m".to_string()]);

        // Sibling items AND the sibling's imported items are merged into
        // `module.items` for codegen completeness.
        let fn_names: HashSet<String> = module
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Fn(f) => Some(f.name.clone()),
                _ => None,
            })
            .collect();
        assert!(fn_names.contains("main"), "entry's `main` present");
        assert!(fn_names.contains("helper"), "sibling's `helper` merged");
        assert!(
            fn_names.contains("dep_fn"),
            "sibling's imported `dep_fn` merged for codegen"
        );

        // Rule C — per-peer import isolation: `dep_fn` is visible to the
        // SIBLING (it wrote `import dep.{dep_fn}`), but NOT to the entry
        // (which imported nothing).
        assert!(
            sib.imported_item_names.contains("dep_fn"),
            "sibling must see its own import `dep_fn`"
        );
        let entry_pf = module
            .peer_files
            .iter()
            .find(|p| p.file_id == MAIN_FILE_ID)
            .expect("entry peer present");
        assert!(
            !entry_pf.imported_item_names.contains("dep_fn"),
            "entry must NOT see the sibling's import (Rule C isolation)"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn single_file_entry_collects_no_siblings() {
        // A lone file whose directory contains another `.nv` declaring a
        // DIFFERENT module must NOT be treated as a folder-module — the
        // Ф.10 detection branch stays inert (zero-regression guarantee).
        let root = unique_tmp("f10solo");
        let proj = root.join("proj");
        let solo = proj.join("solo.nv");
        let other = proj.join("other.nv");

        write_file(&solo, "module solo\n\nfn main() -> int => 0\n");
        write_file(&other, "module other\n\nfn unrelated() -> int => 1\n");

        let src = std::fs::read_to_string(&solo).expect("read entry");
        let mut module = parser::parse(&src).expect("entry parses");
        let stdlib = root.join("no_stdlib");

        resolve_imports_inline_ex(&solo, &mut module, &proj, &stdlib, false)
            .expect("single-file entry resolves");

        assert_eq!(
            module.peer_files.len(),
            1,
            "single-file entry must register exactly one peer (itself)"
        );
        assert!(module.peer_files[0].is_entry_module);
        let fn_names: HashSet<String> = module
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Fn(f) => Some(f.name.clone()),
                _ => None,
            })
            .collect();
        assert!(
            !fn_names.contains("unrelated"),
            "a file declaring a different module must not be pulled in"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Plan 204 дофикс №2 (D420 go-scope): `[replace]` declared inside a
    /// DEPENDENCY's own manifest (reached transitively — `b`, a `[dependencies]`
    /// entry of the root `app`) must be IGNORED when `b`'s own files resolve
    /// their OWN imports (`c`). `b`'s `[replace] c = { path = "<nonexistent>" }`
    /// points at a directory that DOESN'T EXIST — if the bug were still present
    /// (`lookup_dependency` honoring ANY manifest's `effective_source`, not
    /// just the root's), this would hard-fail with `ReplacePathMissing`.
    /// After the fix, `b`'s declared `c = { path = "../c_real" }` is used
    /// instead, resolution succeeds, and `c`'s real `c_fn` (returning `1`,
    /// not the fake's hypothetical value) is merged in.
    #[test]
    fn nested_dependency_replace_is_ignored_root_scope_only() {
        let root = unique_tmp("p204scope");
        let proj = root.join("proj");
        // `proj/` — общий корень для соседних пакетов, связанных `path = "../b"`.
        // Без маркера репозитория проверка D420 (переехавшая в `sync`/`load_pins`
        // с №460) считает такую связь выходом за границу репы и валит разрешение
        // до того, как тест доберётся до своего предмета — области видимости
        // `[replace]`. Настоящий git не нужен: проверяется наличие `.git`.
        std::fs::create_dir_all(proj.join(".git")).unwrap();

        let app_dir = proj.join("app");
        write_file(
            &app_dir.join("nova.toml"),
            "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nb = { path = \"../b\" }\n",
        );
        write_file(
            &app_dir.join("app.nv"),
            "module app\n\nimport b.core.{b_fn}\n\nfn main() -> int => b_fn()\n",
        );

        let b_dir = proj.join("b");
        write_file(
            &b_dir.join("nova.toml"),
            "[package]\nname = \"b\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nc = { path = \"../c_real\" }\n\
             [replace]\nc = { path = \"../c_fake_nonexistent\" }\n",
        );
        write_file(
            &b_dir.join("core.nv"),
            "module b.core\n\nimport c.{c_fn}\n\nexport fn b_fn() -> int => c_fn()\n",
        );

        // NOTE: `proj/c_fake_nonexistent` is deliberately never created —
        // if `b`'s [replace] were (wrongly) honored, this would hard-error
        // (`E_REPLACE_PATH_MISSING` / legacy `PathMissing`).
        let c_real_dir = proj.join("c_real");
        write_file(
            &c_real_dir.join("nova.toml"),
            "[package]\nname = \"c\"\n[lib]\nsrc = \".\"\n",
        );
        write_file(
            &c_real_dir.join("c.nv"),
            "module c\n\nexport fn c_fn() -> int => 1\n",
        );

        let app_nv = app_dir.join("app.nv");
        let src = std::fs::read_to_string(&app_nv).expect("read entry");
        let mut module = parser::parse(&src).expect("entry parses");
        let stdlib = root.join("no_stdlib");

        resolve_imports_inline_ex(&app_nv, &mut module, &proj, &stdlib, false).expect(
            "resolution must succeed — b's OWN [replace] must be ignored \
             (not root), falling back to b's declared `c = path(../c_real)`",
        );

        let fn_names: HashSet<String> = module
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Fn(f) => Some(f.name.clone()),
                _ => None,
            })
            .collect();
        assert!(fn_names.contains("b_fn"), "b's b_fn merged");
        assert!(
            fn_names.contains("c_fn"),
            "c's REAL c_fn merged (via b's declared source, not b's ignored replace)"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Plan 204 дофикс №3 (M-187 diamond fix) — mirror image of the test
    /// above: proves root's `[replace]` DOES reach a same-named dependency
    /// found ANYWHERE in the graph, not only root's own direct edge. Root
    /// (`app`) does not even depend on `c` directly — only `b` (root's own
    /// dependency) does, with a declared `c` source that DOES NOT EXIST on
    /// disk. Without дофикс №3, `b`'s own files resolving `import c.*` would
    /// use `b`'s broken declared source and hard-fail (`PathMissing`); with
    /// the fix, root's `[replace] c = { path = "../c_real" }` is consulted
    /// regardless of which manifest owns the `c` edge (real Cargo `[patch]`/
    /// Go-module `replace` semantics — see `lookup_dependency`'s
    /// `root_override`), so resolution succeeds via the real `c` package.
    #[test]
    fn root_replace_overrides_transitive_same_named_dep() {
        let root = unique_tmp("p204scope3");
        let proj = root.join("proj");
        // См. соседний тест: `proj/` обязан выглядеть репозиторием, иначе D420
        // отвергает `path = "../b"` раньше, чем проверяется предмет теста.
        std::fs::create_dir_all(proj.join(".git")).unwrap();

        let app_dir = proj.join("app");
        write_file(
            &app_dir.join("nova.toml"),
            "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nb = { path = \"../b\" }\n\
             [replace]\nc = { path = \"../c_real\" }\n",
        );
        write_file(
            &app_dir.join("app.nv"),
            "module app\n\nimport b.core.{b_fn}\n\nfn main() -> int => b_fn()\n",
        );

        let b_dir = proj.join("b");
        write_file(
            &b_dir.join("nova.toml"),
            "[package]\nname = \"b\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nc = { path = \"../c_fake_nonexistent\" }\n",
        );
        write_file(
            &b_dir.join("core.nv"),
            "module b.core\n\nimport c.{c_fn}\n\nexport fn b_fn() -> int => c_fn()\n",
        );

        // NOTE: `proj/c_fake_nonexistent` is deliberately never created — if
        // root's `[replace]` did NOT reach this transitive edge (the дофикс
        // №2 bug this test guards against regressing), this would hard-error
        // (`PathMissing`).
        let c_real_dir = proj.join("c_real");
        write_file(
            &c_real_dir.join("nova.toml"),
            "[package]\nname = \"c\"\n[lib]\nsrc = \".\"\n",
        );
        write_file(
            &c_real_dir.join("c.nv"),
            "module c\n\nexport fn c_fn() -> int => 1\n",
        );

        let app_nv = app_dir.join("app.nv");
        let src = std::fs::read_to_string(&app_nv).expect("read entry");
        let mut module = parser::parse(&src).expect("entry parses");
        let stdlib = root.join("no_stdlib");

        resolve_imports_inline_ex(&app_nv, &mut module, &proj, &stdlib, false).expect(
            "resolution must succeed — root's [replace] `c` must reach b's \
             transitive `c` edge (M-187 diamond fix), not just root's own edges",
        );

        let fn_names: HashSet<String> = module
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Fn(f) => Some(f.name.clone()),
                _ => None,
            })
            .collect();
        assert!(fn_names.contains("b_fn"), "b's b_fn merged");
        assert!(
            fn_names.contains("c_fn"),
            "c's REAL c_fn merged via root's [replace] override, not b's broken declared path"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// [M-d376-slow-suffix-folder-module-peer-merge]: symmetric counterpart
    /// of `entry_folder_module_collects_siblings_with_per_peer_isolation`
    /// for D376's `_slow` suffix. A `*_slow.nv` peer sitting beside a
    /// folder-module (co-equal peer files, e.g. nova-tls's `src/`
    /// root-peers) must NOT be merged into a plain (non-`_slow`) entry's
    /// compile-unit — before this fix, the entry-sibling scan only knew
    /// about `_test`, so a `_slow` peer's items (and its `Item::Test`, were
    /// it a test) were pulled into every other entry in the same
    /// folder-module and effectively ran on every default `nova test`,
    /// defeating the slow-lane (Plan 156 D376) for folder-modules — the
    /// discovery walker (`test_runner::walk_nv_filtered_ex`,
    /// `plan156_slow_lane_tests::walk_nv_filtered_slow_lanes`) already
    /// excluded `_slow` correctly; only this peer-merge side lagged.
    #[test]
    fn entry_folder_module_excludes_slow_peer_for_non_slow_entry() {
        let root = unique_tmp("d376a");
        let proj = root.join("proj");
        let app = proj.join("m").join("app.nv");
        let helper = proj.join("m").join("helper.nv");
        let helper_slow = proj.join("m").join("helper_slow.nv");

        write_file(&app, "module m\n\nfn main() -> int => 0\n");
        write_file(&helper, "module m\n\nfn helper() -> int => 1\n");
        write_file(
            &helper_slow,
            "module m\n\nfn heavy_slow_check() -> int => 2\n",
        );

        let src = std::fs::read_to_string(&app).expect("read entry");
        let mut module = parser::parse(&src).expect("entry parses");
        let stdlib = root.join("no_stdlib");

        resolve_imports_inline_ex(&app, &mut module, &proj, &stdlib, false)
            .expect("non-slow entry resolves");

        let entry_peers: Vec<&PeerFile> = module
            .peer_files
            .iter()
            .filter(|p| p.is_entry_module)
            .collect();
        assert_eq!(
            entry_peers.len(),
            2,
            "expected entry + helper.nv ONLY — helper_slow.nv must be excluded, got {:?}",
            entry_peers.iter().map(|p| p.path.clone()).collect::<Vec<_>>()
        );
        assert!(
            !entry_peers.iter().any(|p| p.path.ends_with("helper_slow.nv")),
            "helper_slow.nv must NOT be registered as a peer of a non-slow entry"
        );

        let fn_names: HashSet<String> = module
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Fn(f) => Some(f.name.clone()),
                _ => None,
            })
            .collect();
        assert!(fn_names.contains("helper"), "normal sibling still merges");
        assert!(
            !fn_names.contains("heavy_slow_check"),
            "`_slow` sibling's items must NOT be merged into a non-slow entry's CU \
             (this is the D376 peer-merge bug: a slow peer used to be compiled AND \
             run on every ordinary `nova test`)"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// [M-d376-slow-suffix-folder-module-peer-merge]: when the compiled
    /// entry is ITSELF a `_slow` file (the shape a `--include-slow` /
    /// `--slow-only` run compiles), its own module peers — including OTHER
    /// `_slow` siblings — merge exactly as before this fix ("peers merge as
    /// usual"); the exclusion in the sibling test above applies only when a
    /// `_slow` file is a peer of SOMEONE ELSE'S (non-slow) entry.
    #[test]
    fn entry_folder_module_includes_slow_peers_when_entry_is_itself_slow() {
        let root = unique_tmp("d376b");
        let proj = root.join("proj");
        let app_slow = proj.join("m").join("app_slow.nv");
        let helper = proj.join("m").join("helper.nv");
        let other_slow = proj.join("m").join("other_slow.nv");

        write_file(&app_slow, "module m\n\nfn main() -> int => 0\n");
        write_file(&helper, "module m\n\nfn helper() -> int => 1\n");
        write_file(&other_slow, "module m\n\nfn other_task() -> int => 2\n");

        let src = std::fs::read_to_string(&app_slow).expect("read entry");
        let mut module = parser::parse(&src).expect("entry parses");
        let stdlib = root.join("no_stdlib");

        resolve_imports_inline_ex(&app_slow, &mut module, &proj, &stdlib, false)
            .expect("slow entry resolves");

        let entry_peers: Vec<&PeerFile> = module
            .peer_files
            .iter()
            .filter(|p| p.is_entry_module)
            .collect();
        assert_eq!(
            entry_peers.len(),
            3,
            "slow entry must collect ALL module peers incl. other _slow siblings, got {:?}",
            entry_peers.iter().map(|p| p.path.clone()).collect::<Vec<_>>()
        );

        let fn_names: HashSet<String> = module
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Fn(f) => Some(f.name.clone()),
                _ => None,
            })
            .collect();
        assert!(fn_names.contains("helper"), "normal sibling merges");
        assert!(
            fn_names.contains("other_task"),
            "another `_slow` sibling merges normally when the entry itself is `_slow`"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
