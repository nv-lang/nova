//! Plan 45 Ф.24.9 — `--scrape-examples` call-site scanner.
//!
//! Scans a workspace directory for `.nv` source files, extracts call expressions
//! whose callee name matches a documented item, and attaches the top-3 snippets
//! as `ScrapedExample` entries on the corresponding `DocItem`.
//!
//! Algorithm:
//! 1. Walk `workspace_root` recursively for `*.nv` files.
//! 2. For each file: read source text, scan for `<name>(` patterns where `name`
//!    matches a documented fn name (simple textual scan — no full parse required).
//! 3. Collect call-site: (file path, line, snippet = call line ± 1 context).
//! 4. Sort by (file, line) and keep top-3 per item.
//!
//! The textual scan is intentional: full AST parse of every file in the workspace
//! would be expensive; a regex-style scan over line-contents is sufficient for
//! documentation purposes (false positives = string literals containing `foo(` are
//! acceptable — this is for discoverability, not correctness).

use super::doctree::{DocTree, ItemKind, ScrapedExample};
use std::path::{Path, PathBuf};

const MAX_EXAMPLES: usize = 3;

/// Scan `workspace_root` for call-sites and populate `scraped_examples`
/// on fn/const items in `tree`. Re-export and type items are skipped.
pub fn scrape_examples(tree: &mut DocTree, workspace_root: &Path) {
    // Build name → item_index map for fast lookup.
    // Key: function name (without module prefix) — matches `foo(` patterns.
    // Only Fn items are eligible (and Const for completeness).
    let mut name_to_indices: std::collections::HashMap<String, Vec<(usize, usize)>> =
        std::collections::HashMap::new();
    for (mi, module) in tree.modules.iter().enumerate() {
        for (ii, item) in module.items.iter().enumerate() {
            if matches!(item.kind, ItemKind::Fn(_) | ItemKind::Const { .. }) {
                name_to_indices
                    .entry(item.name.clone())
                    .or_default()
                    .push((mi, ii));
            }
        }
    }
    if name_to_indices.is_empty() {
        return;
    }

    // Collect source files.
    let nv_files = collect_nv_files(workspace_root);

    // Per-item: accumulate (file, line, snippet).
    // We accumulate all, then truncate to MAX_EXAMPLES.
    let mut found: std::collections::HashMap<(usize, usize), Vec<ScrapedExample>> =
        std::collections::HashMap::new();

    for file_path in &nv_files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let display = file_path
            .strip_prefix(workspace_root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .replace('\\', "/");
        let lines: Vec<&str> = source.lines().collect();

        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            // Skip comment lines and doc-comment lines.
            if trimmed.starts_with("//") {
                continue;
            }
            for (name, indices) in &name_to_indices {
                // Match `name(` pattern — word boundary via preceding non-word char.
                // Simple check: find `name(` and verify the char before is non-ident.
                let pattern = format!("{}(", name);
                if let Some(pos) = line.find(pattern.as_str()) {
                    // Verify word boundary before the match.
                    let ok = if pos == 0 {
                        true
                    } else {
                        let prev = line.as_bytes()[pos - 1];
                        !prev.is_ascii_alphanumeric() && prev != b'_' && prev != b'.'
                    };
                    if ok {
                        let snippet = build_snippet(&lines, line_idx);
                        let example = ScrapedExample {
                            file: display.clone(),
                            line: (line_idx + 1) as u32,
                            snippet,
                        };
                        for &idx in indices {
                            // Only keep MAX_EXAMPLES per item.
                            let v = found.entry(idx).or_default();
                            if v.len() < MAX_EXAMPLES {
                                v.push(example.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    // Attach to tree.
    for ((mi, ii), examples) in found {
        tree.modules[mi].items[ii].scraped_examples = examples;
    }
}

/// Build a snippet: the call line + 1 line of trailing context (if available).
fn build_snippet(lines: &[&str], call_line: usize) -> String {
    let end = (call_line + 2).min(lines.len());
    lines[call_line..end]
        .iter()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Recursively collect all `.nv` files under `root`.
fn collect_nv_files(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    collect_nv_recursive(root, &mut result);
    result.sort(); // deterministic order
    result
}

fn collect_nv_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories and common non-source dirs.
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            collect_nv_recursive(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("nv") {
            out.push(path);
        }
    }
}
