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
    let is_prelude_self = module.name.iter().map(|s| s.as_str()).collect::<Vec<_>>() == ["std", "prelude"];
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
) -> Result<()> {
    let resolved = resolve_import_path(&imp.path, entry_dir, repo, stdlib_dir)
        .ok_or_else(|| anyhow!(
            "cannot find module '{}' — searched:\n  {}/{}.nv\n  {}/{}.nv\n  {}/{}.nv",
            imp.path.join("."),
            entry_dir.display(), imp.path.join("/"),
            repo.display(), imp.path.join("/"),
            stdlib_dir.display(), imp.path.iter().skip(1).cloned().collect::<Vec<_>>().join("/")))?;

    let canon = resolved.canonicalize()
        .map_err(|e| anyhow!("canonicalize {}: {}", resolved.display(), e))?;

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

    let imp_src = std::fs::read_to_string(&resolved)
        .map_err(|e| anyhow!("failed to read imported module {}: {}", resolved.display(), e))?;
    let imp_path_str = resolved.to_string_lossy().to_string();
    let imp_module = parser::parse(&imp_src)
        .map_err(|d| {
            let (line, col) = byte_to_line_col(&imp_src, d.span.start);
            anyhow!(
                "in imported module '{}' ({}): {}:{}: {}",
                imp.path.join("."), imp_path_str, line, col, d.message)
        })?;

    // Recursive: resolve transitive imports.
    for sub in &imp_module.imports {
        resolve_one(
            sub,
            entry_dir,
            repo,
            stdlib_dir,
            visited,
            in_progress,
            import_chain,
            merged_items,
        )?;
    }

    // Plan 35 sub-plan 35.A (R26): selective filter — syntax-only.
    let _ = imp.items.is_some();

    // Merge items: Type, Fn, Const. Skip Test и top-level Let.
    for item in imp_module.items {
        match &item {
            Item::Type(_) | Item::Fn(_) | Item::Const(_) => {
                merged_items.push(item);
            }
            Item::Test(_) | Item::Let(_) => {
                // Test blocks и top-level let — игнорируем для imported.
            }
        }
    }

    // Pop in_progress + chain; promote canon в closed-set.
    in_progress.remove(&canon);
    visited.insert(canon);
    import_chain.pop();
    Ok(())
}

/// Resolve `import a.b.c` к filesystem path.
/// Returns first existing path.
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
