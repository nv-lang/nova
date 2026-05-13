//! Plan 35 Ф.1 MVP: cross-file import resolution через inline AST expansion.
//!
//! Используется тремя compile pipelines (Plan 35 R31 — unified pipeline):
//! - `nova-cli::cmd_check` — type-check single file.
//! - `nova-cli::cmd_build` — compile single file → exe.
//! - `compiler-codegen::test_runner::codegen_to_c` — test compilation.
//!
//! Все три вызывают `resolve_imports_inline(...)` ДО передачи `Module` в
//! `types::check_module` или `CEmitter::emit_module`.

use crate::ast::{Import, Item, Module};
use crate::diag::byte_to_line_col;
use crate::parser;
use anyhow::{anyhow, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
/// **Limitations** (sub-plans 35.A-E):
///   - Нет visibility filter (is_export informational).
///   - Нет symbol mangling.
///   - Нет DCE.
///   - Нет signature/body 2-pass split.
///   - Wildcard `import X.*` не поддерживается.
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
    let mut visited: HashSet<PathBuf> = HashSet::new();

    let mut merged_items: Vec<Item> = Vec::new();
    // Plan 35 Ф.1 cycle detection (D29): in-progress DFS-stack — canonical
    // paths модулей currently being resolved. Если import упирается в
    // канон уже в стеке → cycle. visited — closed-set (diamond-dep dedup);
    // in_progress — open-set (cycle detect).
    let mut in_progress: HashSet<PathBuf> = HashSet::new();
    let mut import_chain: Vec<Vec<String>> = Vec::new(); // для error message

    // Plan 35 Ф.1 (D29): добавляем entry в in_progress + chain ДО resolve.
    // Если transitive import ссылается обратно на entry — cycle detected.
    // (Если entry сам не в visited — diamond-dep потом silent skip.)
    let entry_canon = entry_path.canonicalize().ok();
    if let Some(c) = &entry_canon {
        in_progress.insert(c.clone());
    }
    import_chain.push(module.name.clone());

    // Plan 35 sub-plan 35.A R27: auto-import `std.prelude` if exists.
    // D26 (08-runtime.md): prelude — auto-available имена (Option/Result/...).
    // Currently большая часть prelude hardcoded в type-checker'е/codegen'е;
    // R27 даёт миграционный путь — пользователь может расширять prelude
    // через `std/prelude.nv` файл (или future полная миграция hardcoded
    // в file-based). MVP: если файл существует — добавляем как import.
    // Skip prelude auto-import для самого prelude (избежать self-cycle).
    // Plan 42 Sub-plan 42.6: detect prelude self по обоих declaration
    // форматов (rev-1 legacy + rev-3 parent.X). Logic — в manifest helper.
    let is_prelude_self = crate::manifest::is_prelude_self_module(&module.name);
    let mut initial_imports = module.imports.clone();
    if !is_prelude_self {
        let prelude_path = stdlib_dir.join("prelude.nv");
        if prelude_path.exists() && prelude_path.is_file() {
            initial_imports.push(Import {
                path: vec!["std".into(), "prelude".into()],
                items: None,
                alias: None,
                is_export: false,
                span: crate::diag::Span::dummy(),
            });
        }
    }

    for imp in &initial_imports {
        resolve_one(
            imp,
            &entry_dir,
            repo,
            stdlib_dir,
            &mut visited,
            &mut in_progress,
            &mut import_chain,
            &mut merged_items,
            include_test_peers,
        )?;
    }

    // Entry done — promote из in_progress → visited.
    if let Some(c) = entry_canon {
        in_progress.remove(&c);
        visited.insert(c);
    }
    import_chain.pop();

    // Prepend merged items: imported сначала, потом user code.
    // Это важно для bootstrap single-pass codegen — typedef'ы должны
    // появиться ДО use-site.
    let mut new_items = merged_items;
    new_items.append(&mut module.items);
    module.items = new_items;

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
    entry_dir: &Path,
    repo: &Path,
    stdlib_dir: &Path,
    visited: &mut HashSet<PathBuf>,
    in_progress: &mut HashSet<PathBuf>,
    import_chain: &mut Vec<Vec<String>>,
    merged_items: &mut Vec<Item>,
    include_test_peers: bool,
) -> Result<()> {
    // Plan 42 правило H: `internal/` directory access check.
    // Module `<X>.internal.<Y>` доступен только из `<X>.*` (descendants).
    // Importing module = тот кто DIRECTLY делает import (последний в
    // chain), не entry. Например `folder_internal/public.nv` имеет
    // import internal/... — он descendant of folder_internal, разрешено.
    if let Some(internal_idx) = imp.path.iter().position(|s| s == "internal") {
        // Parent of `internal` = everything before it.
        let internal_parent = &imp.path[..internal_idx];
        // Importing module = the module currently being resolved (last in
        // chain). It's the file that wrote `import ...` line.
        let importing_module = import_chain.last().cloned().unwrap_or_default();
        // internal_parent должен быть prefix of importing_module.
        // Также важно: imported module's parent is the prefix until
        // `internal`. importing path должен **содержать** internal_parent
        // как prefix (descendant rule).
        let allowed = internal_parent.is_empty()
            || (internal_parent.len() <= importing_module.len()
                && importing_module
                    .iter()
                    .zip(internal_parent.iter())
                    .all(|(a, b)| a == b));
        if !allowed {
            return Err(anyhow!(
                "cannot import internal module '{}' from outside parent\n  \
                 internal module's parent: {}\n  \
                 importing module:         {}\n  \
                 hint: internal modules accessible only from descendants of `{}`",
                imp.path.join("."),
                internal_parent.join("."),
                importing_module.join("."),
                internal_parent.join("."),
            ));
        }
    }

    // Plan 42 Ф.2: resolve module to list of peer files (or single file
    // for legacy single-file modules).
    let resolved_paths = resolve_module_paths(&imp.path, entry_dir, repo, stdlib_dir, include_test_peers)
        .ok_or_else(|| {
            // Plan 42 правило L: diagnostic quality. Show importing module
            // (last in chain) + searched paths + suggestion.
            let importing = import_chain
                .last()
                .map(|m| m.join("."))
                .unwrap_or_else(|| "<unknown>".to_string());
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
        })?;

    // Use FIRST peer file's canonical path as module identity key. All peers
    // of one folder-module share single key (we promote ALL peers to visited
    // when done — diamond-dep dedup works correctly).
    let first_path = &resolved_paths[0];
    let canon = first_path.canonicalize()
        .map_err(|e| anyhow!("canonicalize {}: {}", first_path.display(), e))?;

    // D29: cycle = canon уже в in_progress.
    if in_progress.contains(&canon) {
        let mut chain_display: Vec<String> = import_chain.iter()
            .map(|p| p.join("."))
            .collect();
        chain_display.push(imp.path.join("."));
        return Err(anyhow!(
            "import cycle detected:\n  {}",
            chain_display.join(" → ")));
    }

    // Closed-set: diamond-dep dedup. Silent skip.
    if visited.contains(&canon) {
        return Ok(());
    }

    in_progress.insert(canon.clone());
    import_chain.push(imp.path.clone());

    // Plan 42 Ф.2: parse все peer files в alphabetical order (правило B).
    // Для each peer:
    //   1. Parse to Module.
    //   2. Recursively resolve its imports.
    //   3. Append its items в merged_items.
    // Peers share namespace через merge'нутый Module.items.
    let mut peer_canons: Vec<PathBuf> = Vec::new();
    for peer_path in &resolved_paths {
        let peer_canon = peer_path.canonicalize()
            .map_err(|e| anyhow!("canonicalize {}: {}", peer_path.display(), e))?;
        peer_canons.push(peer_canon);

        let peer_src = std::fs::read_to_string(peer_path)
            .map_err(|e| anyhow!("failed to read imported module {}: {}", peer_path.display(), e))?;
        let peer_path_str = peer_path.to_string_lossy().to_string();
        let peer_module = parser::parse(&peer_src)
            .map_err(|d| {
                let (line, col) = byte_to_line_col(&peer_src, d.span.start);
                anyhow!(
                    "in imported module '{}' ({}): {}:{}: {}",
                    imp.path.join("."), peer_path_str, line, col, d.message)
            })?;

        // Recursive: resolve transitive imports for THIS peer.
        for sub in &peer_module.imports {
            resolve_one(
                sub,
                entry_dir,
                repo,
                stdlib_dir,
                visited,
                in_progress,
                import_chain,
                merged_items,
                include_test_peers,
            )?;
        }

        // Merge items from this peer.
        for item in peer_module.items {
            match &item {
                Item::Type(_) | Item::Fn(_) | Item::Const(_) => {
                    merged_items.push(item);
                }
                Item::Test(_) | Item::Let(_) => {
                    // Test blocks / top-level let — игнорируем для imported.
                }
            }
        }
    }

    // Plan 35 sub-plan 35.A (R26): selective filter — syntax-only.
    let _ = imp.items.is_some();

    // Pop in_progress + chain; promote ALL peer canons в closed-set.
    in_progress.remove(&canon);
    for c in peer_canons {
        visited.insert(c);
    }
    import_chain.pop();
    Ok(())
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

/// Resolve `import a.b.c` к filesystem path.
/// Returns first existing path. Used для single-file modules.
fn resolve_import_path(
    parts: &[String],
    entry_dir: &Path,
    repo: &Path,
    stdlib_dir: &Path,
) -> Option<PathBuf> {
    if parts.is_empty() {
        return None;
    }
    let rel_path: PathBuf = parts.iter().collect();
    let with_ext = rel_path.with_extension("nv");

    let cand_local = entry_dir.join(&with_ext);
    if cand_local.exists() && cand_local.is_file() {
        return Some(cand_local);
    }

    let cand_repo = repo.join(&with_ext);
    if cand_repo.exists() && cand_repo.is_file() {
        return Some(cand_repo);
    }

    // explicit stdlib_dir search (path starts with `std`)
    if parts[0] == "std" && parts.len() >= 2 {
        let rel_inside_std: PathBuf = parts[1..].iter().collect();
        let cand_std = stdlib_dir.join(rel_inside_std.with_extension("nv"));
        if cand_std.exists() && cand_std.is_file() {
            return Some(cand_std);
        }
    }

    None
}

/// Plan 42 Ф.2: resolve module to **list** of peer files (folder-module)
/// или single file. Returns `Vec<PathBuf>` alphabetically sorted (правило B).
///
/// Resolution order:
/// 1. Try single-file `<...>/parts.nv` (legacy behaviour).
/// 2. If not found, try folder `<...>/parts/` — collect все `*.nv` файлы
///    в этой папке (non-recursive, alphabetical sort).
/// 3. Conflict (file exists AND folder with .nv files exists) → return
///    None and caller emits «ambiguous module».
///
/// Каждый search root (entry_dir / repo / stdlib_dir) проверяется в
/// порядке.
fn resolve_module_paths(
    parts: &[String],
    entry_dir: &Path,
    repo: &Path,
    stdlib_dir: &Path,
    include_test_peers: bool,
) -> Option<Vec<PathBuf>> {
    if parts.is_empty() {
        return None;
    }
    let rel_path: PathBuf = parts.iter().collect();

    // Candidate search roots — same order as resolve_import_path.
    let mut roots: Vec<PathBuf> = vec![
        entry_dir.to_path_buf(),
        repo.to_path_buf(),
    ];
    if parts[0] == "std" && parts.len() >= 2 {
        roots.push(stdlib_dir.to_path_buf());
    }

    for root in &roots {
        // Translate path: для stdlib_dir мы пропускаем первый `std` segment.
        let local_rel: PathBuf = if root == stdlib_dir && parts[0] == "std" {
            parts[1..].iter().collect()
        } else {
            rel_path.clone()
        };

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
                // Ambiguous — return None and let caller emit error.
                // Note: silent None is bad UX; caller currently emits
                // generic «cannot find» error. Improvement: thread Result.
                return None;
            }
        }

        if file_exists {
            return Some(vec![single_file]);
        }

        if folder_exists {
            // Collect все *.nv files (non-recursive), alphabetical sort.
            // Plan 42 правило F: filter `*_test.nv` peers если
            // !include_test_peers (build mode).
            let mut peers: Vec<PathBuf> = std::fs::read_dir(&folder)
                .ok()?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    if !p.is_file() {
                        return false;
                    }
                    if p.extension().and_then(|s| s.to_str()) != Some("nv") {
                        return false;
                    }
                    if !include_test_peers {
                        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                            if stem.ends_with("_test") {
                                return false;
                            }
                        }
                    }
                    true
                })
                .collect();
            if !peers.is_empty() {
                peers.sort();
                return Some(peers);
            }
            // Folder без .nv files (после filter) — namespace-container,
            // не module. Продолжаем поиск в других roots.
        }
    }

    None
}
