//! Plan 45 Ф.6 — intra-doc-link resolver (production-grade after Ф.21.7+Ф.25.1).
//!
//! Распознаём ссылки формата `[Name]`, `[Name.method]`, `[mod.Name]`
//! в doc-text'е (summary / description / sections). Игнорируем
//! markdown links формата `[text](url)` (распознаются по `(` сразу
//! после `]`).
//!
//! **Production capabilities:**
//! - Cross-module resolution (Ф.21.7): workspace mode видит items из всех модулей.
//! - Ambiguous-link warnings (Ф.25.1): collision short_name → DocWarning через
//!   tree.warnings (раньше silent None).
//! - Back-links (Ф.23.23): после resolve строится inverted map → DocItem.linked_from.
//!
//! Алгоритм:
//! 1. Собираем все item-IDs из `tree.modules`.
//! 2. Строим индекс: `short_name -> full_id` (e.g., `Range` →
//!    `std.collections.range::Range`). При коллизии short_name —
//!    ссылка считается ambiguous (target_id = None).
//! 3. Для каждого item обходим summary/description/sections, находим
//!    кандидаты `[...]`, пытаемся резолвить.
//!
//! Поведение:
//! - Match по full path (`mod.Name`) приоритетнее short name.
//! - `Type.method` — ищем item с id `*::Type.method`.
//! - Broken/ambiguous link — `target_id = None`, попадает в output
//!   с `null`-target'ом (consumer'ы могут warn).

use super::doctree::*;
use std::collections::HashMap;

pub fn resolve_intra_doc_links(tree: &mut DocTree) {
    let mut by_short: HashMap<String, Vec<String>> = HashMap::new();
    let mut by_full: HashMap<String, String> = HashMap::new();

    for m in &tree.modules {
        for it in &m.items {
            // Full id, e.g. "std.x::Range" → key "std.x::Range".
            by_full.insert(it.id.clone(), it.id.clone());
            // Short key: имя item'а (после `::`).
            let short = item_short_key(&it.id);
            by_short.entry(short).or_default().push(it.id.clone());
        }
    }

    let mut found: Vec<DocLink> = Vec::new();
    let mut warnings: Vec<DocWarning> = Vec::new();
    for m in &tree.modules {
        // Module-level links.
        let module_id = format!("<module:{}>", m.path.join("."));
        let texts: Vec<&str> = [m.summary.as_deref(), m.description.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        for text in texts {
            for cand in extract_link_candidates(text) {
                found.push(resolve_one(None, cand, &by_short, &by_full, &module_id, &mut warnings));
            }
        }
        for it in &m.items {
            let mut item_texts: Vec<&str> = Vec::new();
            item_texts.extend(it.summary.as_deref());
            item_texts.extend(it.description.as_deref());
            for v in it.sections.values() {
                item_texts.push(v.as_str());
            }
            for text in item_texts {
                for cand in extract_link_candidates(text) {
                    found.push(resolve_one(Some(it.id.clone()), cand, &by_short, &by_full, &it.id, &mut warnings));
                }
            }
        }
    }

    // Дедупликация: одинаковые (from_id, text) — оставляем один.
    found.sort_by(|a, b| {
        a.from_id
            .cmp(&b.from_id)
            .then(a.text.cmp(&b.text))
    });
    found.dedup_by(|a, b| a.from_id == b.from_id && a.text == b.text);

    // Plan 45 Ф.23.23: populate back-links (item.linked_from).
    // Build inverted map: target_id → Vec<from_id>.
    let mut back: HashMap<String, Vec<String>> = HashMap::new();
    for l in &found {
        if let (Some(from), Some(target)) = (&l.from_id, &l.target_id) {
            back.entry(target.clone()).or_default().push(from.clone());
        }
    }
    for m in &mut tree.modules {
        for it in &mut m.items {
            if let Some(mut froms) = back.remove(&it.id) {
                froms.sort();
                froms.dedup();
                it.linked_from = froms;
            }
        }
    }

    tree.links = found;
    tree.warnings.extend(warnings);
    tree.warnings.sort();
    tree.warnings.dedup();
}

fn item_short_key(id: &str) -> String {
    // `module.path::Name` или `module.path::Type.method` →
    // `Name` или `Type.method`.
    match id.rsplit_once("::") {
        Some((_, rest)) => rest.to_string(),
        None => id.to_string(),
    }
}

fn resolve_one(
    from_id: Option<String>,
    text: String,
    by_short: &HashMap<String, Vec<String>>,
    by_full: &HashMap<String, String>,
    warn_source_id: &str,
    warnings: &mut Vec<DocWarning>,
) -> DocLink {
    let target_id = resolve_text(&text, by_short, by_full, warn_source_id, warnings);
    // Plan 45 Ф.30.1: external crate-doc fallback. Если target_id None и текст
    // имеет префикс из NOVA_DOC_EXTERN_LINKS map — emit external URL.
    let target_url = if target_id.is_none() {
        resolve_external_url(&text)
    } else {
        None
    };
    DocLink { from_id, text, target_id, target_url }
}

/// Plan 45 Ф.30.1 — resolve external crate-doc URL по env var
/// `NOVA_DOC_EXTERN_LINKS`.
///
/// Format env var: `prefix1=template1;prefix2=template2`
/// где template содержит `{path}` placeholder.
///
/// Example: `std=https://docs.nova-lang.org/std/{path};myorg.lib=https://myorg.dev/docs/{path}`
///
/// Lookup: для текста `std.io.println`, prefix match `std` → URL
/// `https://docs.nova-lang.org/std/io.println` (path = всё после prefix.).
fn resolve_external_url(text: &str) -> Option<String> {
    let env = std::env::var("NOVA_DOC_EXTERN_LINKS").ok()?;
    if env.is_empty() {
        return None;
    }
    // Parse env: prefix=template;prefix=template
    for entry in env.split(';') {
        let entry = entry.trim();
        if entry.is_empty() { continue; }
        let (prefix, template) = match entry.split_once('=') {
            Some((p, t)) => (p.trim(), t.trim()),
            None => continue,
        };
        if prefix.is_empty() || template.is_empty() {
            continue;
        }
        // Match: text начинается с `prefix.` или равен prefix.
        let prefix_dot = format!("{}.", prefix);
        let path = if text == prefix {
            String::new()
        } else if let Some(rest) = text.strip_prefix(&prefix_dot) {
            rest.to_string()
        } else {
            continue;
        };
        return Some(template.replace("{path}", &path));
    }
    None
}

fn resolve_text(
    text: &str,
    by_short: &HashMap<String, Vec<String>>,
    by_full: &HashMap<String, String>,
    warn_source_id: &str,
    warnings: &mut Vec<DocWarning>,
) -> Option<String> {
    // 1. Full id match — текст уже `mod.path::Name`.
    if let Some(id) = by_full.get(text) {
        return Some(id.clone());
    }
    // 2. Short match по последнему сегменту.
    if let Some(ids) = by_short.get(text) {
        if ids.len() == 1 {
            return Some(ids[0].clone());
        }
        // Plan 45 Ф.25.1: ambiguous short link раньше silent → None.
        // Теперь — warning перечисляющий candidates, чтобы автор знал что
        // надо disambiguate через full path.
        let mut candidates: Vec<String> = ids.clone();
        candidates.sort();
        warnings.push(DocWarning {
            rule: "ambiguous-link".to_string(),
            item_id: warn_source_id.to_string(),
            message: format!(
                "intra-doc link `[{}]` is ambiguous between {} items: [{}]; use full path to disambiguate",
                text,
                candidates.len(),
                candidates.join(", ")
            ),
        });
        return None;
    }
    // 3. `mod.path.Name` форма (без `::`) — попробуем сопоставить.
    if let Some((module, name)) = text.rsplit_once('.') {
        let synthetic = format!("{}::{}", module, name);
        if let Some(id) = by_full.get(&synthetic) {
            return Some(id.clone());
        }
    }
    None
}

/// Извлечь кандидаты `[X]` из markdown-текста.
/// Игнорирует:
/// - `[text](url)` — обычная markdown ссылка;
/// - `[X][label]` — reference-style;
/// - содержимое внутри backtick-fence'ов (```...```);
/// - inline `code` блоков.
fn extract_link_candidates(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut in_inline_code = false;
    let mut in_fenced = false;

    while i < bytes.len() {
        // Detect ```fence``` toggles.
        if !in_inline_code
            && i + 2 < bytes.len()
            && &bytes[i..i + 3] == b"```"
        {
            in_fenced = !in_fenced;
            i += 3;
            continue;
        }
        if in_fenced {
            i += 1;
            continue;
        }
        if bytes[i] == b'`' {
            in_inline_code = !in_inline_code;
            i += 1;
            continue;
        }
        if in_inline_code {
            i += 1;
            continue;
        }
        if bytes[i] == b'[' {
            // Skip `#[...]` — Plan 45 Ф.3 doc-attr syntax, не intra-doc link.
            if i > 0 && bytes[i - 1] == b'#' {
                let mut k = i + 1;
                while k < bytes.len() && bytes[k] != b']' && bytes[k] != b'\n' {
                    k += 1;
                }
                i = if k < bytes.len() { k + 1 } else { k };
                continue;
            }
            // Найти закрывающую `]`. Скипуем nested `[]` для простоты.
            let mut j = i + 1;
            let mut depth = 1;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    b'\n' => {
                        depth = 0;
                        break;
                    }
                    _ => {}
                }
                j += 1;
            }
            // j указывает на позицию после `]` (или конец).
            if depth == 0 && j > i + 1 {
                let inner = &text[i + 1..j - 1];
                let next = bytes.get(j).copied();
                let is_md_link = matches!(next, Some(b'(') | Some(b'['));
                if !is_md_link && is_plausible_identifier(inner) {
                    out.push(inner.to_string());
                }
                // Ref-style `[text][label]` — also skip the `[label]` part
                // so it's not picked up as a separate link candidate.
                if next == Some(b'[') {
                    let mut k = j + 1;
                    while k < bytes.len() && bytes[k] != b']' && bytes[k] != b'\n' {
                        k += 1;
                    }
                    i = if k < bytes.len() { k + 1 } else { k };
                    continue;
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    out
}

fn is_plausible_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Допустимы буквы/цифры/`_`/`.`/`::`. Не разрешаем пробелы.
    s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
        && s.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple() {
        let cands = extract_link_candidates("See [Range] for details.");
        assert_eq!(cands, vec!["Range".to_string()]);
    }

    #[test]
    fn skips_markdown_link() {
        let cands = extract_link_candidates("See [text](http://example.com).");
        assert!(cands.is_empty());
    }

    #[test]
    fn skips_ref_style() {
        let cands = extract_link_candidates("See [text][label].");
        assert!(cands.is_empty());
    }

    #[test]
    fn skips_inside_code() {
        let cands = extract_link_candidates("Use `[X]` literally.");
        assert!(cands.is_empty());
    }

    #[test]
    fn skips_inside_fence() {
        let cands = extract_link_candidates("```nova\n[X] code\n```\n[Real]");
        assert_eq!(cands, vec!["Real".to_string()]);
    }

    #[test]
    fn dotted_path() {
        let cands = extract_link_candidates("[Type.method] reference.");
        assert_eq!(cands, vec!["Type.method".to_string()]);
    }

    #[test]
    fn rejects_spaces() {
        let cands = extract_link_candidates("[two words] x");
        assert!(cands.is_empty());
    }
}
