//! Plan 45 Ф.3 / D105 — `propagate_stability` pass.
//!
//! Заполняет `DocItem.deprecation` и `DocItem.stability` из:
//! 1. Standard sections `# Deprecated` / `# Since` (D107).
//! 2. Inline doc-attr маркеры `#[deprecated("...")]` / `#[stable]` /
//!    `#[unstable]` / `#[experimental]` / `#[since("X")]` в начале
//!    description'а (D105 doc-attr alternate syntax).
//!
//! MVP: derive `Stability::Stable` если `# Since >= 1.0.0`, иначе —
//! не trigger'им stability (consumer'у решать). `#stable`/`#unstable`/
//! `#experimental` doc-attrs дают явный override.
//!
//! Этот pass — не error-emitting. Если sections содержат невалидный
//! version-string, поле просто остаётся `None`.

use super::doctree::*;

pub fn propagate_stability(tree: &mut DocTree) {
    for m in &mut tree.modules {
        for it in &mut m.items {
            derive_for_item(it);
        }
    }
}

fn derive_for_item(it: &mut DocItem) {
    // 1. Inline doc-attrs в начале description (если есть).
    let attrs = extract_inline_attrs(it.description.as_deref().unwrap_or(""));
    // Если attrs найдены — убираем их из description, чтобы не
    // дублировались в выводе.
    if attrs.has_any() {
        if let Some(desc) = &it.description {
            let stripped = strip_leading_attrs(desc);
            it.description = if stripped.is_empty() {
                None
            } else {
                Some(stripped)
            };
        }
    }

    // 2. Deprecation — приоритет: inline-attr > # Deprecated section.
    if let Some(note) = attrs.deprecated {
        let since = attrs.since.clone();
        it.deprecation = Some(Deprecation { note, since });
    } else if let Some(dep_text) = it.sections.get("deprecated") {
        let since = extract_since_from_note(dep_text);
        it.deprecation = Some(Deprecation {
            note: dep_text.clone(),
            since,
        });
    }

    // 3. Stability — приоритет: inline-attr > derived from # Since.
    if let Some(tier) = attrs.tier {
        it.stability = Some(Stability {
            tier,
            since: attrs.since.clone(),
        });
    } else if let Some(since_text) = it.sections.get("since") {
        let since_str = since_text.trim().to_string();
        let tier = if is_post_1_0(&since_str) {
            StabilityTier::Stable
        } else {
            StabilityTier::Unstable
        };
        it.stability = Some(Stability {
            tier,
            since: Some(since_str),
        });
    }
}

#[derive(Debug, Default)]
struct ExtractedAttrs {
    deprecated: Option<String>,
    tier: Option<StabilityTier>,
    since: Option<String>,
}

impl ExtractedAttrs {
    fn has_any(&self) -> bool {
        self.deprecated.is_some() || self.tier.is_some() || self.since.is_some()
    }
}

/// Удаляет ведущие `#[...]` attr-строки + последующие пустые строки.
fn strip_leading_attrs(desc: &str) -> String {
    let mut lines: Vec<&str> = desc.lines().collect();
    while let Some(first) = lines.first() {
        let t = first.trim();
        if t.starts_with("#[") && t.ends_with(']') {
            lines.remove(0);
        } else if t.is_empty() {
            lines.remove(0);
        } else {
            break;
        }
    }
    lines.join("\n").trim_end().to_string()
}

/// Распознаём строки в начале description'а:
/// - `#[deprecated("text")]` или `#[deprecated = "text"]`
/// - `#[stable]` / `#[unstable]` / `#[experimental]`
/// - `#[since("X")]` или `#[since = "X"]`
///
/// Каждая такая строка — отдельный line (terminated by `\n`).
/// Stops at first non-attr line.
fn extract_inline_attrs(desc: &str) -> ExtractedAttrs {
    let mut out = ExtractedAttrs::default();
    for line in desc.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !t.starts_with("#[") || !t.ends_with(']') {
            break; // First non-attr line — stop.
        }
        let inner = &t[2..t.len() - 1]; // content between `#[` and `]`
        if let Some(arg) = inner.strip_prefix("deprecated") {
            let val = parse_attr_arg(arg).unwrap_or_default();
            out.deprecated = Some(val);
        } else if let Some(arg) = inner.strip_prefix("since") {
            let val = parse_attr_arg(arg).unwrap_or_default();
            out.since = Some(val);
        } else if inner == "stable" {
            out.tier = Some(StabilityTier::Stable);
        } else if inner == "unstable" {
            out.tier = Some(StabilityTier::Unstable);
        } else if inner == "experimental" {
            out.tier = Some(StabilityTier::Experimental);
        }
        // unknown — silently ignored (forward-compat).
    }
    out
}

/// Парсит `("text")` или `= "text"` или `("text", key="val")` (только
/// первый positional). Возвращает строковое значение без кавычек.
fn parse_attr_arg(s: &str) -> Option<String> {
    let s = s.trim();
    let inner = if let Some(rest) = s.strip_prefix('(') {
        rest.strip_suffix(')')?
    } else if let Some(rest) = s.strip_prefix('=') {
        rest.trim()
    } else {
        return None;
    };
    let inner = inner.trim();
    // Берём содержимое первой строки в кавычках.
    let q_start = inner.find('"')?;
    let after = &inner[q_start + 1..];
    let q_end = after.find('"')?;
    Some(after[..q_end].to_string())
}

/// Эвристика: `# Deprecated\nUse X instead. Since 0.5.0.` → `Some("0.5.0")`.
fn extract_since_from_note(note: &str) -> Option<String> {
    let lower = note.to_ascii_lowercase();
    let idx = lower.find("since ")?;
    let after = &note[idx + 6..];
    let end = after
        .find(|c: char| c.is_whitespace() || c == '.' && !looks_like_version_dot(after))
        .unwrap_or(after.len());
    let ver = after[..end].trim_end_matches('.').to_string();
    if ver.is_empty() {
        None
    } else {
        Some(ver)
    }
}

fn looks_like_version_dot(_s: &str) -> bool {
    // Сейчас просто возвращаем true чтобы `.` в `0.5.0` не обрывал
    // парсинг; имена для MVP — упрощены.
    true
}

/// SemVer ≥ 1.0.0 → stable. Не парсим полностью, просто смотрим major.
fn is_post_1_0(v: &str) -> bool {
    let v = v.trim().trim_start_matches('v');
    let major = v.split('.').next().unwrap_or("0");
    matches!(major.parse::<u32>(), Ok(n) if n >= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn since_post_1_0_is_stable() {
        assert!(is_post_1_0("1.0.0"));
        assert!(is_post_1_0("2.5.1"));
        assert!(is_post_1_0("v1.0"));
        assert!(!is_post_1_0("0.9.0"));
        assert!(!is_post_1_0("0.1"));
    }

    #[test]
    fn inline_deprecated_with_string() {
        let attrs = extract_inline_attrs("#[deprecated(\"use add instead\")]\n\nThe rest.");
        assert_eq!(attrs.deprecated.as_deref(), Some("use add instead"));
    }

    #[test]
    fn inline_stable() {
        let attrs = extract_inline_attrs("#[stable]\n\nDescription.");
        assert_eq!(attrs.tier, Some(StabilityTier::Stable));
    }

    #[test]
    fn inline_attrs_stop_at_non_attr() {
        let attrs = extract_inline_attrs("Description.\n#[stable]");
        // Первая строка — не attr, остановились.
        assert_eq!(attrs.tier, None);
    }

    #[test]
    fn parse_attr_arg_paren_string() {
        assert_eq!(
            parse_attr_arg("(\"hello\")").as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn parse_attr_arg_eq_string() {
        assert_eq!(
            parse_attr_arg("= \"world\"").as_deref(),
            Some("world")
        );
    }
}
