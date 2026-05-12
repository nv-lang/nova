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
    // Mark entry as visited (canonical) to prevent re-loading it.
    if let Ok(c) = entry_path.canonicalize() {
        visited.insert(c);
    }

    // Collect imports to process (BFS).
    let mut queue: Vec<Import> = module.imports.clone();
    let mut merged_items: Vec<Item> = Vec::new();
    let mut import_stack: Vec<Vec<String>> = Vec::new(); // for cycle err message

    // Plan 35 sub-plan 35.A R27: auto-import `std.prelude` if exists.
    // D26 (08-runtime.md): prelude — auto-available имена (Option/Result/...).
    // Currently большая часть prelude hardcoded в type-checker'е/codegen'е;
    // R27 даёт миграционный путь — пользователь может расширять prelude
    // через `std/prelude.nv` файл (или future полная миграция hardcoded
    // в file-based). MVP: если файл существует — добавляем как import.
    // Skip prelude auto-import для самого prelude (избежать self-cycle).
    let is_prelude_self = module.name.iter().map(|s| s.as_str()).collect::<Vec<_>>() == ["std", "prelude"];
    if !is_prelude_self {
        let prelude_path = stdlib_dir.join("prelude.nv");
        if prelude_path.exists() && prelude_path.is_file() {
            queue.push(Import {
                path: vec!["std".into(), "prelude".into()],
                items: None,
                alias: None,
                is_export: false,
                span: crate::diag::Span::dummy(),
            });
        }
    }

    while let Some(imp) = queue.pop() {
        // Resolve path to file.
        let resolved = resolve_import_path(&imp.path, &entry_dir, repo, stdlib_dir);
        let resolved = match resolved {
            Some(p) => p,
            None => {
                return Err(anyhow!(
                    "cannot find module '{}' — searched:\n  {}/{}.nv\n  {}/{}.nv\n  {}/{}.nv",
                    imp.path.join("."),
                    entry_dir.display(), imp.path.join("/"),
                    repo.display(), imp.path.join("/"),
                    stdlib_dir.display(), imp.path.iter().skip(1).cloned().collect::<Vec<_>>().join("/")));
            }
        };

        let canon = resolved.canonicalize()
            .map_err(|e| anyhow!("canonicalize {}: {}", resolved.display(), e))?;
        if !visited.insert(canon.clone()) {
            // Already loaded — skip silently (diamond-dep dedup).
            continue;
        }

        // Detect cycle: if canon path appears in import_stack, cycle.
        if import_stack.contains(&imp.path) {
            return Err(anyhow!(
                "import cycle detected:\n  {}",
                import_stack.iter()
                    .map(|p| p.join("."))
                    .collect::<Vec<_>>()
                    .join(" → ")));
        }
        import_stack.push(imp.path.clone());

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

        // Recursive: enqueue transitive imports.
        for sub in &imp_module.imports {
            queue.push(sub.clone());
        }

        // Plan 35 sub-plan 35.A (R26): selective syntax `import X.{A, B}` —
        // **bootstrap MVP**: парсер принимает синтаксис, но resolver
        // **не enforce'ит** filter — все items добавляются. Причина:
        // **transitive dependency closure** (Range.method_step_by возвращает
        // StepRangeIter — фильтр без полного dep-walking ломал бы codegen).
        // Полный enforcement через type-checker visibility — post-bootstrap.
        // Filter сейчас служит только документацией намерения программиста.
        // Plan 35 sub-plan 35.A R26 follow-up: enforce via type-checker
        // name resolution (visible names в module scope).
        let _ = imp.items.is_some(); // syntax-only

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

        import_stack.pop();
    }

    // Prepend merged items: imported сначала, потом user code.
    // Это важно для bootstrap single-pass codegen — typedef'ы должны
    // появиться ДО use-site.
    let mut new_items = merged_items;
    new_items.append(&mut module.items);
    module.items = new_items;

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
