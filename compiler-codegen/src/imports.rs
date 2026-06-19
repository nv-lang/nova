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

    /// Insert or replace the signatures for a module.
    pub fn insert(&mut self, sigs: ModuleSignatures) {
        self.table.insert(sigs.module_name.clone(), sigs);
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
    let entry_dir = entry_path.parent().unwrap_or(repo).to_path_buf();
    let mut table = ModuleSigTable::new();
    let mut visited: HashSet<Vec<String>> = HashSet::new();
    let mut in_progress: HashSet<Vec<String>> = HashSet::new();

    // Seed the table with the entry module's own items.
    let entry_sigs = collect_module_signatures_from_items(&module.items, module.name.clone());
    table.insert(entry_sigs);

    // Build import work-list from the entry module's declared imports.
    let mut import_work: Vec<(Import, PathBuf)> = Vec::new();
    for imp in &module.imports {
        import_work.push((imp.clone(), entry_path.to_path_buf()));
    }

    // Add prelude import using the same heuristic as resolve_imports_inline_ex
    // (simple default: look for std/prelude.nv in stdlib_dir).
    let is_prelude_self = crate::manifest::is_prelude_self_module(&module.name);
    let has_no_prelude = module
        .attrs
        .iter()
        .any(|a| matches!(a.kind, crate::ast::ModuleAttrKind::NoPrelude));
    if !is_prelude_self && !has_no_prelude {
        let prelude_path = stdlib_dir.join("prelude.nv");
        if prelude_path.exists() && prelude_path.is_file() {
            import_work.push((
                Import {
                    path: vec!["std".into(), "prelude".into()],
                    items: None,
                    alias: None,
                    is_export: false,
                    span: crate::diag::Span::dummy(),
                    doc_attrs: Vec::new(),
                    anchor: crate::ast::ImportAnchor::Package,
                },
                entry_path.to_path_buf(),
            ));
        }
    }

    // Mark entry as in-progress so transitive re-imports of entry early-return.
    in_progress.insert(module.name.clone());

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

    in_progress.remove(&module.name);
    visited.insert(module.name.clone());

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
    let dep_root: Option<PathBuf> = if rel_root.is_some() || imp.path.is_empty() {
        None
    } else {
        match lookup_dependency(importer_path, &imp.path[0]) {
            DepLookup::PathDep(root) if imp.path.len() >= 2 => Some(root),
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

    // Determine module key from the first peer file.
    let first_path = &resolved_paths[0];
    let module_key: Vec<String> = read_module_decl(first_path).unwrap_or_else(|| {
        let canon = first_path.canonicalize().unwrap_or_else(|_| first_path.clone());
        vec![canon.to_string_lossy().to_string()]
    });

    // Cycle guard and dedup.
    if in_progress.contains(&module_key) || visited.contains(&module_key) {
        return;
    }

    in_progress.insert(module_key.clone());

    for peer_path in &resolved_paths {
        let peer_src = match std::fs::read_to_string(peer_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let peer_module = match crate::parser::parse(&peer_src) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !cfg_active(&peer_module) {
            continue;
        }

        // Extract and insert signatures for this peer.
        let peer_sigs =
            collect_module_signatures_from_items(&peer_module.items, peer_module.name.clone());
        table.insert(peer_sigs);

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
    if !module_nv.exists() { return vec![]; }
    let src = match std::fs::read_to_string(&module_nv) { Ok(s) => s, Err(_) => return vec![] };
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
    let entry_canon_for_peer = entry_path.canonicalize().unwrap_or_else(|_| entry_path.to_path_buf());
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
    // Entry key = его declared module name (module.name).
    let entry_key = module.name.clone();
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

    // Plan 35 sub-plan 35.A R27: auto-import `std.prelude` if exists.
    // D26 (08-runtime.md): prelude — auto-available имена (Option/Result/...).
    // Currently большая часть prelude hardcoded в type-checker'е/codegen'е;
    // R27 даёт миграционный путь — пользователь может расширять prelude
    // через `std/prelude.nv` файл (или future полная миграция hardcoded
    // в file-based). MVP: если файл существует — добавляем как import.
    // Skip prelude auto-import для самого prelude (избежать self-cycle).
    // Plan 42 Sub-plan 42.6: detect prelude self по обоих declaration
    // форматов (rev-1 legacy + rev-3 parent.X). Logic — в manifest helper.
    //
    // **Plan 62.F / D174 (Plan 107):** prelude opt-out атрибуты. Logic:
    //   - `#no_prelude` (NoPrelude) → НЕ auto-import'им вообще.
    //   - `#prelude(a, b, ...)` (PartialPrelude) → auto-import только
    //     перечисленных sub-modules. Empty list → compile error (D174).
    //   - default → full `std.prelude` facade (как раньше).
    // Inline-формы `no_prelude`/`partial_prelude`/`allow_prelude_shadow`
    // удалены (D174) — parser эмитит hard error с migration hint.
    let is_prelude_self = crate::manifest::is_prelude_self_module(&module.name);
    let has_no_prelude = module.attrs.iter()
        .any(|a| matches!(a.kind, crate::ast::ModuleAttrKind::NoPrelude));
    let partial_prelude_names: Option<Vec<String>> = module.attrs.iter()
        .find_map(|a| if let crate::ast::ModuleAttrKind::PartialPrelude(names) = &a.kind {
            Some(names.clone())
        } else { None });
    // Plan 81 Ф.10: prelude auto-imports collected separately from the
    // entry's own (and sibling peers') `import` statements — prelude is
    // resolved once and shared by every entry-group peer (see below).
    let mut prelude_imports: Vec<Import> = Vec::new();
    if !is_prelude_self && !has_no_prelude {
        if let Some(names) = partial_prelude_names {
            // D174: пустой список — compile error (parser уже отклоняет #prelude(),
            // но defensive check для надёжности в случае прямого AST использования).
            if names.is_empty() {
                return Err(anyhow!(
                    "empty prelude list `#prelude()` is not allowed (D174, Plan 107); \
                     use `#no_prelude` to disable prelude auto-import\n  \
                     in module `{}`",
                    module.name.join(".")
                ));
            }
            // Plan 62.F: `partial_prelude(a, b, ...)` — auto-import только
            // перечисленных sub-modules. Валидируем имена против реальных
            // файлов `std/prelude/<name>.nv`. Bad name → compile error.
            let prelude_subdir = stdlib_dir.join("prelude");
            for name in &names {
                let sub_path = prelude_subdir.join(format!("{}.nv", name));
                if !sub_path.exists() || !sub_path.is_file() {
                    let importing = module.name.join(".");
                    return Err(anyhow!(
                        "`partial_prelude({})`: unknown prelude sub-module `{}`\n  \
                         in module `{}`\n  \
                         expected file: {}\n  \
                         valid sub-modules (Plan 62): core, runtime, errors, \
                         collections, protocols, effects\n  \
                         hint: check spelling or remove from list (D26, Plan 62.F)",
                        names.join(", "),
                        name,
                        importing,
                        sub_path.display(),
                    ));
                }
                prelude_imports.push(Import {
                    path: vec!["std".into(), "prelude".into(), name.clone()],
                    items: None,
                    alias: None,
                    is_export: false,
                    span: crate::diag::Span::dummy(),
                    doc_attrs: Vec::new(),
                    anchor: crate::ast::ImportAnchor::Package,
                });
            }
            // D174: empty list — defensive path (unreachable after parser check above).
        } else {
            // Default: full prelude facade.
            //
            // Plan 62.F.bis Ф.1 (edition versioning, 2026-05-18):
            // если в `nova.toml` указано `[package].edition = "<X>"`, и
            // соответствующий `std/prelude/<sanitized>.nv` существует —
            // используем его вместо rolling `std/prelude.nv` facade.
            // Sanitization: `.` → `_` (например `2026.05` → `2026_05.nv`).
            //
            // Fallback chain (resolver-side):
            //   1. edition pin: `std/prelude/<sanitized>.nv` — если найден,
            //      import path = `["std", "prelude", "<sanitized>"]`.
            //   2. rolling facade: `std/prelude.nv` — backward-compat
            //      default (нет edition в манифесте, или edition pin не
            //      найден на диске).
            //
            // Безопасность: pin даёт stable prelude content на edition
            // — даже если будущие версии compiler'а изменят `std/prelude.nv`,
            // package с `edition = "2026.05"` видит фиксированный snapshot.
            // Soft-fail (edition specified, но файла нет): silently fall back
            // на rolling facade — не блокируем build, но user может явно
            // указать edition без файла (например для будущего pin).
            let mut edition_pin_used = false;
            if let Some(manifest) = crate::manifest::find_manifest(entry_path) {
                if let Some(edition) = &manifest.edition {
                    let sanitized = crate::manifest::sanitize_edition(edition);
                    if !sanitized.is_empty() {
                        let pin_path = stdlib_dir.join("prelude").join(format!("{}.nv", sanitized));
                        if pin_path.exists() && pin_path.is_file() {
                            prelude_imports.push(Import {
                                path: vec!["std".into(), "prelude".into(), sanitized.clone()],
                                items: None,
                                alias: None,
                                is_export: false,
                                span: crate::diag::Span::dummy(),
                                doc_attrs: Vec::new(),
                                anchor: crate::ast::ImportAnchor::Package,
                            });
                            edition_pin_used = true;
                        }
                    }
                }
            }
            if !edition_pin_used {
                let prelude_path = stdlib_dir.join("prelude.nv");
                if prelude_path.exists() && prelude_path.is_file() {
                    prelude_imports.push(Import {
                        path: vec!["std".into(), "prelude".into()],
                        items: None,
                        alias: None,
                        is_export: false,
                        span: crate::diag::Span::dummy(),
                        doc_attrs: Vec::new(),
                        anchor: crate::ast::ImportAnchor::Package,
                    });
                }
            }
        }
    }

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
        let entry_canon = entry_path.canonicalize().ok();
        let target = current_target_os();
        if let Ok(entries) = std::fs::read_dir(&entry_dir) {
            let mut sib_paths: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension().and_then(|s| s.to_str()) == Some("nv")
                })
                .filter(|p| {
                    // Exclude the entry file itself.
                    match (p.canonicalize().ok(), &entry_canon) {
                        (Some(pc), Some(ec)) => &pc != ec,
                        _ => p.as_path() != entry_path,
                    }
                })
                .filter(|p| {
                    // Mirror `resolve_module_paths` peer filters: `_test`
                    // peers only in test mode; OS-suffix peers only for the
                    // current target.
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        let core = stem.strip_suffix("_test").unwrap_or(stem);
                        if !include_test_peers && core != stem {
                            return false;
                        }
                        if !peer_active_for_target(core, target) {
                            return false;
                        }
                    }
                    true
                })
                .filter(|p| {
                    // Sibling = declares the SAME module path as the entry.
                    read_module_decl(p).as_deref() == Some(module.name.as_slice())
                })
                .collect();
            // Alphabetical → deterministic file_id assignment.
            sib_paths.sort();
            for sp in sib_paths {
                let src = std::fs::read_to_string(&sp).map_err(|e| {
                    anyhow!("failed to read entry-folder peer {}: {}", sp.display(), e)
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
                let canon = sp.canonicalize().unwrap_or(sp);
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
        );
        if let Err(e) = res {
            import_errors.push(format!("{}", e));
            in_progress = in_progress_snap;
            import_chain = import_chain_snap;
            visited = visited_snap;
        }
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

    // Entry done — promote из in_progress → visited.
    // Plan 162 Ф.4: entry's exports not cached (entry is never dedup'd as
    // an import by others in the same resolve call — it's the root module).
    in_progress.remove(&entry_key);
    visited.insert(entry_key, vec![]);
    import_chain.pop();

    // Prepend merged items: imported сначала, потом user code (entry +
    // sibling peers). Это важно для bootstrap single-pass codegen —
    // typedef'ы должны появиться ДО use-site.
    let mut new_items = merged_items;
    new_items.append(&mut module.items);
    for sib in &mut siblings {
        new_items.append(&mut sib.module.items);
    }
    module.items = new_items;

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
            let dir_canon = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            let pkg_canon = pkg_root.canonicalize().unwrap_or_else(|_| pkg_root.clone());
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
        match lookup_dependency(importer_path, &imp.path[0]) {
            DepLookup::NotADep => None,
            DepLookup::PathDep(root) => {
                // Импорт из зависимости — всегда полным путём
                // `<dep>.<module>...` (минимум 2 сегмента).
                if imp.path.len() < 2 {
                    return Err(anyhow!(
                        "импорт из зависимости `{}` требует путь к модулю: \
                         `import {}.<module>...`\n  \
                         importing file: {}\n  \
                         hint: голое имя пакета не адресует модуль (D29)",
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
            let ip_c = ip.canonicalize().unwrap_or_else(|_| ip.clone());
            let rp_c = rp.canonicalize().unwrap_or_else(|_| rp.clone());
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
        let importer_canon = importer_path.canonicalize()
            .unwrap_or_else(|_| importer_path.to_path_buf());
        let owner_canon = owner_dir.canonicalize()
            .unwrap_or_else(|_| owner_dir.clone());
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

    // Plan 42.14 Ф.3 ([M11]): cycle detection по DECLARED MODULE NAME,
    // не canonical PathBuf. Symlink / case-insensitive FS могли дать
    // разные пути для same module → false-negative cycle. Module name
    // (parent.X) — стабильный логический identity.
    //
    // Читаем `module X.Y` declaration из первого peer (lightweight —
    // только первая non-comment строка, без полного parse).
    let first_path = &resolved_paths[0];
    let module_key: Vec<String> = read_module_decl(first_path)
        .unwrap_or_else(|| {
            // Fallback: если decl не прочитался — canonical path string
            // как single-element key (всё равно уникален).
            let canon = first_path.canonicalize()
                .unwrap_or_else(|_| first_path.clone());
            vec![canon.to_string_lossy().to_string()]
        });

    // Plan 162 Ф.2: cycle guard — когда модуль уже находится в стеке
    // DFS (in_progress), это цикл импортов. Вместо stack-overflow или
    // ошибки — ранний возврат Ok(()), позволяя циклу завершиться с теми
    // декларациями, которые уже собраны. Это «collect-first» guard:
    // сигнатуры уже в merged_items (из предыдущих итераций); тела
    // разрешаются после полного сбора. Межмодульные циклы разрешены
    // (D29 rev-5, Plan 162), как peer-циклы в Rule D (Plan 42).
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

    // Closed-set: diamond-dep dedup. When a module is already in visited
    // (items already merged into merged_items), skip the recursive resolve
    // to avoid duplicating items. However, still populate visible_acc with
    // the module's exported names filtered by this import's selector — this
    // is needed when user code has an explicit `import X` and X was already
    // loaded transitively (e.g. via prelude.core importing std.unicode).
    // Plan 162 Ф.4: fixes regression where std.unicode free functions
    // (is_alphabetic etc.) were invisible to explicit user imports because
    // prelude.core had already added std.unicode to visited.
    if let Some(module_exports) = visited.get(&module_key) {
        for exported_name in module_exports {
            if import_selects(imp, exported_name) {
                visible_acc.insert(exported_name.clone());
            }
        }
        return Ok(());
    }

    in_progress.insert(module_key.clone());
    import_chain.push(imp.path.clone());

    // Plan 162 Ф.4: collect all exportable names from this module (across
    // all peers) to cache in visited map. Used by the dedup path above.
    let mut module_exports_cache: Vec<String> = Vec::new();

    // Plan 42 Ф.2: parse все peer files в alphabetical order (правило B).
    // Для each peer:
    //   1. Parse to Module.
    //   2. Recursively resolve its imports.
    //   3. Append its items в merged_items.
    // Peers share namespace через merge'нутый Module.items.
    for peer_path in &resolved_paths {
        let peer_canon = peer_path.canonicalize()
            .map_err(|e| anyhow!("canonicalize {}: {}", peer_path.display(), e))?;

        let peer_src = std::fs::read_to_string(peer_path)
            .map_err(|e| anyhow!("failed to read imported module {}: {}", peer_path.display(), e))?;
        let peer_path_str = peer_path.to_string_lossy().to_string();

        // Plan 42 Sub-plan 42.4 шаг 2: allocate unique FileId для этого peer
        // и parse с этим file_id. Все tokens/spans peer'а получат этот id,
        // type-checker (шаг 3) использует для per-peer name resolution.
        let peer_file_id = *next_file_id;
        *next_file_id += 1;

        let peer_module = parser::parse_with_file_id(&peer_src, peer_file_id)
            .map_err(|d| {
                let (line, col) = byte_to_line_col(&peer_src, d.span.start);
                anyhow!(
                    "in imported module '{}' ({}): {}:{}: {}",
                    imp.path.join("."), peer_path_str, line, col, d.message)
            })?;

        // Plan 42.12 Ф.2: проверка module-level `#cfg(feature/target_os)`.
        // Если peer объявил inactive cfg — skip целиком (не merge items,
        // не register peer_file, не recurse imports).
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

        // Регистрируем PeerFile (snapshot до recursive resolve + merge).
        // Plan 42.15: imported_item_names заполняется ниже после resolve.
        // is_entry_module = false — это peer ИМПОРТИРОВАННОГО модуля,
        // его items_here НЕ должны протекать в entry's shared_decls.
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

        // Plan 42.15: accumulator имён items видимых ЭТОМУ peer'у через
        // его прямые imports. Передаётся в resolve_one для каждого sub —
        // resolve_one пишет туда имена items которые sub притащил.
        let mut peer_visible: HashSet<String> = HashSet::new();

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
    // closed-set. Все peers folder-module share один module_key (declared
    // name) — diamond-dep dedup работает естественно.
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
    let src = std::fs::read_to_string(path).ok()?;
    scan_module_decl(&src)
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
    let parent = match path.parent() {
        Some(p) => p,
        None => return false,
    };
    let target = current_target_os();
    let entries: Vec<PathBuf> = match std::fs::read_dir(parent) {
        Ok(it) => it
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                if !p.is_file() {
                    return false;
                }
                if p.extension().and_then(|s| s.to_str()) != Some("nv") {
                    return false;
                }
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    let core_stem = stem.strip_suffix("_test").unwrap_or(stem);
                    if !peer_active_for_target(core_stem, target) {
                        return false;
                    }
                }
                true
            })
            .collect(),
        Err(_) => return false,
    };
    if entries.len() < 2 {
        return false;
    }
    let mut decls: Vec<Vec<String>> = Vec::with_capacity(entries.len());
    for entry in &entries {
        let src = match std::fs::read_to_string(entry) {
            Ok(s) => s,
            Err(_) => return false,
        };
        match scan_module_decl(&src) {
            Some(d) => decls.push(d),
            None => return false,
        }
    }
    let first = &decls[0];
    decls.iter().all(|d| d == first)
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
#[derive(Debug)]
pub(crate) enum ResolveErr {
    /// Не найдено — caller emit'ит «cannot find module» с suggestions.
    NotFound,
    /// `X.nv` и `X/` (с direct .nv) сосуществуют — ambiguous.
    Ambiguous { file: PathBuf, folder: PathBuf },
    /// Plan 81 Ф.4: путь импорта не совпадает по регистру с именем
    /// файла/папки на диске. На case-insensitive ФС (Windows, macOS
    /// default) такой импорт резолвится, но код непортируем на Linux.
    CaseMismatch { requested: String, actual: String },
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
    let canon = std::fs::canonicalize(path).ok()?;
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
fn package_root_of(file: &Path) -> Option<PathBuf> {
    let mut dir = file.parent()?;
    loop {
        if dir.join("nova.toml").is_file() {
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
fn lookup_dependency(importer_path: &Path, dep_name: &str) -> DepLookup {
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
    match &dep.source {
        crate::manifest::DepSource::Path(rel) => {
            let dep_dir = pkg_dir.join(rel);
            if !dep_dir.is_dir() {
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
    if !dep_toml.is_file() {
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

fn resolve_module_paths(
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

        let file_exists = single_file.is_file();
        let folder_exists = folder.is_dir();

        if file_exists && folder_exists {
            // Check folder has direct .nv files — only then it's ambiguous.
            // If folder только contains sub-folders without direct .nv,
            // we treat it as namespace-container (rule E).
            let has_direct_nv = std::fs::read_dir(&folder)
                .ok()
                .map(|entries| {
                    entries.filter_map(|e| e.ok()).any(|e| {
                        e.path().extension().and_then(|s| s.to_str()) == Some("nv")
                    })
                })
                .unwrap_or(false);
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
                if let Ok(entries) = std::fs::read_dir(&folder) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let p = entry.path();
                        if !p.is_file() {
                            continue;
                        }
                        if p.extension().and_then(|s| s.to_str()) != Some("nv") {
                            continue;
                        }
                        any_peer = true;
                        let declared = match extract_declared_module(&p) {
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
            return Ok(vec![single_file]);
        }

        if folder_exists {
            // Collect все *.nv files (non-recursive), alphabetical sort.
            // Plan 42 правило F: filter `*_test.nv` peers если
            // !include_test_peers (build mode).
            // Plan 42.12 Ф.1: filter peers по filename suffix vs current target.
            let target = current_target_os();
            let entries = match std::fs::read_dir(&folder) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let mut peers: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    if !p.is_file() {
                        return false;
                    }
                    if p.extension().and_then(|s| s.to_str()) != Some("nv") {
                        return false;
                    }
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        // Strip `_test` suffix first для test-peer filter.
                        let core_stem = stem.strip_suffix("_test").unwrap_or(stem);
                        if !include_test_peers && core_stem != stem {
                            // `_test` peer, build mode → skip.
                            return false;
                        }
                        // Target filter: применяем к stem БЕЗ `_test` suffix'а
                        // (чтобы `tls_windows_test.nv` правильно ассоциировался
                        // с windows target).
                        if !peer_active_for_target(core_stem, target) {
                            return false;
                        }
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
    let content = std::fs::read_to_string(path).ok()?;
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
}
