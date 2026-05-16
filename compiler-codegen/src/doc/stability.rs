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
    // Plan 45 Ф.25.1: собираем warnings отдельно, чтобы не нарушать
    // mutable-borrow цикл по modules.
    let mut warnings: Vec<DocWarning> = Vec::new();
    for m in &mut tree.modules {
        // Ф.22.1 / D105: snapshot module-level stability/deprecation
        // ДО mutable-borrow на items, чтобы propagate'ить на items
        // без явного override.
        let module_stability = m.stability.clone();
        let module_deprecation = m.deprecation.clone();
        for it in &mut m.items {
            derive_for_item(it, &mut warnings);
            // Propagate module → item только если item не имеет
            // явного override (set'нутого collector'ом или derive_for_item'ом).
            if it.stability.is_none() {
                if let Some(s) = &module_stability {
                    it.stability = Some(s.clone());
                }
            }
            if it.deprecation.is_none() {
                if let Some(d) = &module_deprecation {
                    it.deprecation = Some(d.clone());
                }
            }
        }
    }
    tree.warnings.extend(warnings);
    tree.warnings.sort();
    tree.warnings.dedup();
}

fn derive_for_item(it: &mut DocItem, warnings: &mut Vec<DocWarning>) {
    // 1. Inline doc-attrs в начале description (если есть).
    let attrs = extract_inline_attrs(it.description.as_deref().unwrap_or(""), &it.id, warnings);
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
        it.deprecation = Some(Deprecation { note, since, until: None });
    } else if let Some(dep_text) = it.sections.get("deprecated") {
        let since = extract_since_from_note(dep_text);
        it.deprecation = Some(Deprecation {
            note: dep_text.clone(),
            since,
            until: None,
        });
    }

    // 3. Stability — приоритет: real parser attr (already in collector)
    // > markdown-inline `#[stable]` > derived from `# Since` section.
    // Ф.22.2: не overwrite уже установленный stability (real-attr хранит
    // feature/note, которые markdown-inline теряет).
    if it.stability.is_none() {
        if let Some(tier) = attrs.tier {
            it.stability = Some(Stability {
                tier,
                since: attrs.since.clone(),
                feature: None,
                note: None,
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
                feature: None,
                note: None,
            });
        }
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
fn extract_inline_attrs(desc: &str, item_id: &str, warnings: &mut Vec<DocWarning>) -> ExtractedAttrs {
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
            match parse_attr_arg(arg) {
                Some(val) => out.deprecated = Some(val),
                None => {
                    // Plan 45 Ф.25.1: было silent unwrap_or_default(), теперь warning.
                    warnings.push(DocWarning {
                        rule: "malformed-stability-attr".to_string(),
                        item_id: item_id.to_string(),
                        message: format!(
                            "malformed #[deprecated{}] — expected #[deprecated(\"reason\")] or #[deprecated = \"reason\"]; ignored",
                            arg
                        ),
                    });
                }
            }
        } else if let Some(arg) = inner.strip_prefix("since") {
            match parse_attr_arg(arg) {
                Some(val) => out.since = Some(val),
                None => {
                    warnings.push(DocWarning {
                        rule: "malformed-stability-attr".to_string(),
                        item_id: item_id.to_string(),
                        message: format!(
                            "malformed #[since{}] — expected #[since(\"X.Y.Z\")] or #[since = \"X.Y.Z\"]; ignored",
                            arg
                        ),
                    });
                }
            }
        } else if inner == "stable" {
            out.tier = Some(StabilityTier::Stable);
        } else if inner == "unstable" {
            out.tier = Some(StabilityTier::Unstable);
        } else if inner == "experimental" {
            out.tier = Some(StabilityTier::Experimental);
        } else {
            // Plan 45 Ф.25.1: было silent skip, теперь warning. Forward-compat
            // сохраняется (мы продолжаем работу), но автор узнаёт о неизвестном attr.
            warnings.push(DocWarning {
                rule: "unknown-doc-attr".to_string(),
                item_id: item_id.to_string(),
                message: format!(
                    "unknown doc-attribute #[{}] — not recognized by this Nova version; ignored",
                    inner
                ),
            });
        }
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
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[deprecated(\"use add instead\")]\n\nThe rest.", "test", &mut w);
        assert_eq!(attrs.deprecated.as_deref(), Some("use add instead"));
        assert!(w.is_empty());
    }

    #[test]
    fn inline_stable() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[stable]\n\nDescription.", "test", &mut w);
        assert_eq!(attrs.tier, Some(StabilityTier::Stable));
        assert!(w.is_empty());
    }

    #[test]
    fn inline_attrs_stop_at_non_attr() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("Description.\n#[stable]", "test", &mut w);
        // Первая строка — не attr, остановились.
        assert_eq!(attrs.tier, None);
        assert!(w.is_empty());
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

    #[test]
    fn malformed_deprecated_emits_warning() {
        // Plan 45 Ф.25.1: malformed attr раньше silent unwrap_or_default(),
        // теперь — warning + skip.
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[deprecated()]\n\nDesc.", "test::foo", &mut w);
        assert!(attrs.deprecated.is_none());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].rule, "malformed-stability-attr");
        assert_eq!(w[0].item_id, "test::foo");
        assert!(w[0].message.contains("malformed #[deprecated"));
    }

    #[test]
    fn unknown_attr_emits_warning() {
        // Plan 45 Ф.25.1: unknown attr раньше silent skip,
        // теперь — warning + skip (forward-compat сохраняется).
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[future_thing]\n\nDesc.", "test::bar", &mut w);
        assert!(attrs.tier.is_none());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].rule, "unknown-doc-attr");
        assert_eq!(w[0].item_id, "test::bar");
        assert!(w[0].message.contains("future_thing"));
    }

    #[test]
    fn malformed_since_emits_warning() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[since]\n", "test::baz", &mut w);
        assert!(attrs.since.is_none());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].rule, "malformed-stability-attr");
    }
}
