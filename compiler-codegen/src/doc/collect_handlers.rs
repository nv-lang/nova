//! Plan 45 Ф.26.2 / Ф.23.4 — handler matrix pass (textual scanner).
//!
//! **Что делает.** Сканирует source текст каждого fn body на pattern
//! `handler <EffectName>` и регистрирует callers как `HandlerRef` в
//! `ItemKind::Effect.handlers`. Это даёт "Effect → who handles me"
//! cross-reference, уникальное обещание Plan 45 §19.4 (Nova-unique vs
//! Go/Rust/TS — у них нет effect handlers).
//!
//! **Почему текстовый scan, не AST visitor.** Аналогично `scraper.rs`
//! (Ф.24.9 для call-sites): full AST visitor across all expression variants
//! — ~250 LOC + сильная coupling к AST schema. Текстовый scan — robust к
//! AST refactor'ам, дёшев, deterministic. False positives (например, строка
//! `"handler Foo"` в string literal) допустимы — это edge case в documentation
//! tooling, не code-correctness.
//!
//! **Алгоритм.**
//! 1. Build effect_name → effect_id index (full и short) из DocTree.
//! 2. Для каждой fn в DocTree.modules ищем pattern `handler <Name>` в source slice.
//!    Source slice = `source[fn.source_span.start..fn.source_span.end]`.
//! 3. Match name → effect_id. Push HandlerRef в effect's handlers list.
//! 4. Sort by (caller_item_id), dedup.
//!
//! **Source availability.** Нужен текст source. API:
//! - `collect_handlers_with_source(tree, source)` — single-file mode.
//! - Workspace mode — нужны per-file sources; MVP передаёт concatenated
//!   sources (best-effort fallback).

use super::doctree::*;

/// Plan 45 Ф.26.2 — entry для single-file mode.
pub fn collect_handlers_with_source(tree: &mut DocTree, source: &str) {
    // Build effect index.
    let (by_full, by_short) = build_effect_index(tree);
    if by_full.is_empty() {
        return;
    }
    // Scan каждый fn-item source slice.
    let mut collected: Vec<((usize, usize), HandlerRef)> = Vec::new();
    for mi in 0..tree.modules.len() {
        for ii in 0..tree.modules[mi].items.len() {
            let it = &tree.modules[mi].items[ii];
            if !matches!(it.kind, ItemKind::Fn(_)) {
                continue;
            }
            let start = it.source_span.start;
            let end = it.source_span.end;
            if start >= source.len() || end > source.len() || start >= end {
                continue;
            }
            let body = &source[start..end];
            for handler_name in find_handler_literals(body) {
                if let Some(target) = resolve_effect(&handler_name, &by_full, &by_short) {
                    collected.push((target, HandlerRef {
                        caller_item_id: it.id.clone(),
                        kind: "inline".to_string(),
                    }));
                }
            }
        }
    }
    // Apply, sort, dedup.
    for ((mi, ii), href) in collected {
        if let Some(m) = tree.modules.get_mut(mi) {
            if let Some(it) = m.items.get_mut(ii) {
                if let ItemKind::Effect { handlers, .. } = &mut it.kind {
                    handlers.push(href);
                }
            }
        }
    }
    for m in &mut tree.modules {
        for it in &mut m.items {
            if let ItemKind::Effect { handlers, .. } = &mut it.kind {
                handlers.sort();
                handlers.dedup();
            }
        }
    }
}

/// Workspace mode — без точных per-fn sources scan невозможен.
/// MVP: noop (handlers остаются empty). Caller вызывает
/// `collect_handlers_with_source` per-file если хочет workspace coverage.
pub fn collect_handlers_workspace(_tree: &mut DocTree) {
    // Plan 45.A: workspace-mode handler matrix требует per-file sources.
    // Currently CLI workspace pipeline передаёт concatenated sources, но
    // span'ы относительные к каждому файлу — нужен sources map. Out-of-scope.
}

fn build_effect_index(
    tree: &DocTree,
) -> (
    std::collections::BTreeMap<String, (usize, usize)>,
    std::collections::BTreeMap<String, Vec<(usize, usize)>>,
) {
    let mut by_full: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    let mut by_short: std::collections::BTreeMap<String, Vec<(usize, usize)>> =
        std::collections::BTreeMap::new();
    for (mi, m) in tree.modules.iter().enumerate() {
        for (ii, it) in m.items.iter().enumerate() {
            if let ItemKind::Effect { .. } = &it.kind {
                by_full.insert(it.id.clone(), (mi, ii));
                let short = it.id.rsplit("::").next().unwrap_or(&it.id).to_string();
                by_short.entry(short).or_default().push((mi, ii));
            }
        }
    }
    (by_full, by_short)
}

/// Извлекает имена effects из pattern'ов `handler <Name>` в source.
/// Возвращает Vec<String> — каждый Name это либо `"Foo"` либо `"mod.Foo"`.
///
/// **Pattern.** `handler` keyword, затем whitespace, затем identifier
/// (possibly with `.` для qualified path). Word-boundary check на старте
/// (избегает `subhandler`).
fn find_handler_literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let kw = b"handler";
    let mut i = 0;
    while i + kw.len() <= bytes.len() {
        if &bytes[i..i + kw.len()] == kw {
            // Word-boundary check before: must not be preceded by alnum/_.
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            // Word-boundary check after: must be whitespace before name.
            let after_ok = i + kw.len() < bytes.len() && bytes[i + kw.len()].is_ascii_whitespace();
            if before_ok && after_ok {
                // Skip whitespace и читаем identifier (with optional dots).
                let mut j = i + kw.len();
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                let name_start = j;
                while j < bytes.len() && (is_ident_char(bytes[j]) || bytes[j] == b'.') {
                    j += 1;
                }
                if j > name_start {
                    let name = source[name_start..j].to_string();
                    // Skip if next non-ws char is `{` или `(`. `handler X {` — это
                    // handler literal. `handler X for Y` — declaration form (но
                    // declarations не поддерживаются в Nova currently).
                    let mut k = j;
                    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if k < bytes.len() && bytes[k] == b'{' {
                        out.push(name);
                    }
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn resolve_effect(
    name: &str,
    by_full: &std::collections::BTreeMap<String, (usize, usize)>,
    by_short: &std::collections::BTreeMap<String, Vec<(usize, usize)>>,
) -> Option<(usize, usize)> {
    // Form like "mod.path.Name" → попробуем как full "mod.path::Name".
    if let Some(dot_idx) = name.rfind('.') {
        let module = &name[..dot_idx];
        let short = &name[dot_idx + 1..];
        let full = format!("{}::{}", module, short);
        if let Some(t) = by_full.get(&full) {
            return Some(*t);
        }
        // Fallback: try short.
        if let Some(matches) = by_short.get(short) {
            if matches.len() == 1 {
                return Some(matches[0]);
            }
        }
        return None;
    }
    // Pure short name.
    if let Some(matches) = by_short.get(name) {
        if matches.len() == 1 {
            return Some(matches[0]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_inline_handler() {
        let src = "fn main() { with x = handler Store { Set(v) { } get() => 0 } { Store.Set(1) } }";
        let names = find_handler_literals(src);
        assert_eq!(names, vec!["Store".to_string()]);
    }

    #[test]
    fn find_qualified_handler() {
        let src = "fn main() { with x = handler std.io.Fs { open() => 0 } { } }";
        let names = find_handler_literals(src);
        assert_eq!(names, vec!["std.io.Fs".to_string()]);
    }

    #[test]
    fn ignores_handler_substring() {
        // "subhandler" не должен matchнуть.
        let src = "let subhandler = 1\nlet handler_var = 2";
        let names = find_handler_literals(src);
        assert!(names.is_empty(), "ident `subhandler` не должен match'нуть keyword");
    }

    #[test]
    fn ignores_handler_without_brace() {
        // `handler X for Y` — это пока что не Nova syntax, должен skip.
        let src = "handler X for Y { }";
        let names = find_handler_literals(src);
        // `X` is matched потому что after `X` идёт `for`, не `{`. Should be empty.
        assert!(names.is_empty(), "handler без `{{` не должен match'нуть");
    }

    #[test]
    fn multiple_handlers() {
        let src = "
            fn f() {
                with a = handler Store { } { }
                with b = handler Logger { } { }
            }
        ";
        let names = find_handler_literals(src);
        assert_eq!(names, vec!["Store".to_string(), "Logger".to_string()]);
    }
}
