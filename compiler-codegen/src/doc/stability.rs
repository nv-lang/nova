//! Plan 45 Ф.3 / D105 — `propagate_stability` pass.
//!
//! Заполняет `DocItem.deprecation` и `DocItem.stability` из:
//! 1. Standard sections `# Deprecated` / `# Since` (D107).
//! 2. Inline doc-attr маркеры в начале description'а (D105 doc-attr alternate syntax).
//!    Поддерживаются ОБА формата:
//!    - D96 canonical: `#stable` / `#unstable` / `#experimental` / `#deprecated("...")`
//!      / `#since("X")` / `#stable(since = "X")` — без скобок для bare attrs
//!    - Legacy bracket form: `#[stable]` / `#[deprecated("...")]` etc. — backward compat
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
        // D96/D105 Ф.22.1: process inline attrs from //! inner-doc description
        // (parallel path to derive_for_item — //! content is raw text, not
        // AST attrs, so the collector cannot pick it up).
        derive_for_module(m, &mut warnings);

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

fn derive_for_module(m: &mut DocModule, warnings: &mut Vec<DocWarning>) {
    if m.stability.is_some() {
        return; // real attr already set by collector
    }
    let module_id = m.path.join(".");
    let attrs = extract_inline_attrs(m.description.as_deref().unwrap_or(""), &module_id, warnings);
    if attrs.has_any() {
        if let Some(desc) = &m.description {
            let stripped = strip_leading_attrs(desc);
            m.description = if stripped.is_empty() { None } else { Some(stripped) };
        }
        if let Some(tier) = attrs.tier {
            m.stability = Some(Stability { tier, since: attrs.since.clone(), feature: None, note: None });
        }
        if let Some(note) = attrs.deprecated {
            m.deprecation = Some(Deprecation { note, since: attrs.since, until: None });
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

/// Возвращает `true` если строка — inline doc-attr в любом из двух форматов:
/// - D96 canonical: `#name` или `#name(...)` или `#name = "..."`
/// - Legacy bracket: `#[name]` или `#[name(...)]` или `#[name = "..."]`
fn is_attr_line(t: &str) -> bool {
    if !t.starts_with('#') {
        return false;
    }
    // Legacy bracket form: #[...]
    if t.starts_with("#[") && t.ends_with(']') {
        return true;
    }
    // D96 canonical form: #name или #name(...) — второй символ должен быть буквой
    t.len() >= 2 && t.chars().nth(1).map_or(false, |c| c.is_ascii_alphabetic())
}

/// Извлекает `(inner_name, rest_arg)` из attr-строки любого формата.
/// - `#[stable]`       → ("stable", "")
/// - `#[since("1.0")]` → ("since", "(\"1.0\")")
/// - `#stable`         → ("stable", "")
/// - `#stable(since = "1.0")` → ("stable", "(since = \"1.0\")")
/// - `#deprecated("msg")` → ("deprecated", "(\"msg\")")
fn parse_attr_line(t: &str) -> Option<(&str, &str)> {
    if t.starts_with("#[") && t.ends_with(']') {
        // Legacy: #[name...] → strip #[ and ]
        let inner = &t[2..t.len() - 1];
        let name_end = inner
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(inner.len());
        let name = &inner[..name_end];
        let rest = &inner[name_end..];
        Some((name, rest))
    } else if t.starts_with('#') {
        // D96 canonical: #name...
        let after = &t[1..];
        let name_end = after
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(after.len());
        let name = &after[..name_end];
        let rest = &after[name_end..];
        Some((name, rest))
    } else {
        None
    }
}

/// Удаляет ведущие attr-строки (оба формата) + последующие пустые строки.
fn strip_leading_attrs(desc: &str) -> String {
    let mut lines: Vec<&str> = desc.lines().collect();
    while let Some(first) = lines.first() {
        let t = first.trim();
        if is_attr_line(t) || t.is_empty() {
            lines.remove(0);
        } else {
            break;
        }
    }
    lines.join("\n").trim_end().to_string()
}

/// Распознаём строки в начале description'а. Поддерживаем ОБА формата:
///
/// D96 canonical (spec):
/// - `#stable` / `#unstable` / `#experimental`
/// - `#stable(since = "X")` / `#deprecated("msg")`
/// - `#since("X.Y.Z")`
///
/// Legacy bracket (backward compat):
/// - `#[stable]` / `#[unstable]` / `#[experimental]`
/// - `#[deprecated("text")]` / `#[since("X")]`
///
/// Каждая такая строка — отдельный line. Stops at first non-attr line.
fn extract_inline_attrs(desc: &str, item_id: &str, warnings: &mut Vec<DocWarning>) -> ExtractedAttrs {
    let mut out = ExtractedAttrs::default();
    for line in desc.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !is_attr_line(t) {
            break; // First non-attr line — stop.
        }
        let (name, rest) = match parse_attr_line(t) {
            Some(pair) => pair,
            None => break,
        };
        match name {
            "deprecated" => {
                if rest.is_empty() {
                    // bare #deprecated без аргумента — валидно, пустая note
                    out.deprecated = Some(String::new());
                } else {
                    match parse_attr_arg(rest) {
                        Some(val) => out.deprecated = Some(val),
                        None => {
                            // Plan 45 Ф.25.1: malformed → warning
                            warnings.push(DocWarning {
                                rule: "malformed-stability-attr".to_string(),
                                item_id: item_id.to_string(),
                                message: format!(
                                    "malformed #deprecated{} — expected #deprecated(\"reason\") or #deprecated = \"reason\"; ignored",
                                    rest
                                ),
                            });
                        }
                    }
                }
            }
            "since" => {
                match parse_attr_arg(rest) {
                    Some(val) => out.since = Some(val),
                    None => {
                        warnings.push(DocWarning {
                            rule: "malformed-stability-attr".to_string(),
                            item_id: item_id.to_string(),
                            message: format!(
                                "malformed #since{} — expected #since(\"X.Y.Z\") or #since = \"X.Y.Z\"; ignored",
                                rest
                            ),
                        });
                    }
                }
            }
            "stable" => {
                out.tier = Some(StabilityTier::Stable);
                // Опциональный since-аргумент: #stable(since = "1.0")
                if !rest.is_empty() {
                    if let Some(since_val) = parse_named_arg(rest, "since") {
                        out.since = Some(since_val);
                    }
                }
            }
            "unstable" => {
                out.tier = Some(StabilityTier::Unstable);
                // Опциональный feature-аргумент: #unstable(feature = "xyz") — игнорируем здесь,
                // real parser attr хранит feature; для markdown-inline достаточно tier.
            }
            "experimental" => {
                out.tier = Some(StabilityTier::Experimental);
            }
            "hide_doc" => {
                // Plan 45 Ф.3 / D105: #hide_doc — не stability, но валидный attr.
                // Обрабатывается в collector'е по real-attr. Здесь просто пропускаем
                // без warning чтобы не дублировать диагностику.
            }
            _ => {
                // Plan 45 Ф.25.1: unknown attr → warning, forward-compat сохраняется.
                warnings.push(DocWarning {
                    rule: "unknown-doc-attr".to_string(),
                    item_id: item_id.to_string(),
                    message: format!(
                        "unknown doc-attribute #{} — not recognized by this Nova version; ignored",
                        name
                    ),
                });
            }
        }
    }
    out
}

/// Парсит именованный аргумент `(key = "val")` или `(key="val")`.
/// Например: `(since = "1.0.0")` → Some("1.0.0") для key="since".
fn parse_named_arg(s: &str, key: &str) -> Option<String> {
    let s = s.trim();
    let inner = s.strip_prefix('(')?.strip_suffix(')')?;
    // ищем key = "..."
    let needle = format!("{} = ", key);
    let needle2 = format!("{}=", key);
    let val_start = if let Some(pos) = inner.find(&needle) {
        pos + needle.len()
    } else if let Some(pos) = inner.find(&needle2) {
        pos + needle2.len()
    } else {
        return None;
    };
    let rest = inner[val_start..].trim();
    let q_start = rest.find('"')?;
    let after = &rest[q_start + 1..];
    let q_end = after.find('"')?;
    Some(after[..q_end].to_string())
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

    // ── Legacy bracket form: #[...] — backward compat ────────────────────────

    #[test]
    fn bracket_deprecated_with_string() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[deprecated(\"use add instead\")]\n\nThe rest.", "test", &mut w);
        assert_eq!(attrs.deprecated.as_deref(), Some("use add instead"));
        assert!(w.is_empty());
    }

    #[test]
    fn bracket_stable() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[stable]\n\nDescription.", "test", &mut w);
        assert_eq!(attrs.tier, Some(StabilityTier::Stable));
        assert!(w.is_empty());
    }

    #[test]
    fn bracket_unstable() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[unstable]\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.tier, Some(StabilityTier::Unstable));
        assert!(w.is_empty());
    }

    #[test]
    fn bracket_experimental() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[experimental]\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.tier, Some(StabilityTier::Experimental));
        assert!(w.is_empty());
    }

    #[test]
    fn bracket_since() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[since(\"0.3.0\")]\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.since.as_deref(), Some("0.3.0"));
        assert!(w.is_empty());
    }

    // ── D96 canonical form: #name — spec-correct ─────────────────────────────

    #[test]
    fn bare_stable_d96() {
        // D96/D105 canonical: #stable без скобок
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#stable\n\nDescription.", "test", &mut w);
        assert_eq!(attrs.tier, Some(StabilityTier::Stable));
        assert!(w.is_empty());
    }

    #[test]
    fn bare_unstable_d96() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#unstable\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.tier, Some(StabilityTier::Unstable));
        assert!(w.is_empty());
    }

    #[test]
    fn bare_experimental_d96() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#experimental\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.tier, Some(StabilityTier::Experimental));
        assert!(w.is_empty());
    }

    #[test]
    fn stable_with_since_d96() {
        // #stable(since = "1.0") — D96 с named arg
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#stable(since = \"1.0\")\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.tier, Some(StabilityTier::Stable));
        assert_eq!(attrs.since.as_deref(), Some("1.0"));
        assert!(w.is_empty());
    }

    #[test]
    fn deprecated_d96() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#deprecated(\"use foo instead\")\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.deprecated.as_deref(), Some("use foo instead"));
        assert!(w.is_empty());
    }

    #[test]
    fn bare_deprecated_d96() {
        // #deprecated без аргумента — валидно
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#deprecated\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.deprecated.as_deref(), Some(""));
        assert!(w.is_empty());
    }

    #[test]
    fn since_d96() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#since(\"0.5.0\")\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.since.as_deref(), Some("0.5.0"));
        assert!(w.is_empty());
    }

    #[test]
    fn hide_doc_d96_no_warning() {
        // #hide_doc — валидный attr, не stability, не должен давать warning
        let mut w = Vec::new();
        let _ = extract_inline_attrs("#hide_doc\n\nDesc.", "test", &mut w);
        assert!(w.is_empty());
    }

    #[test]
    fn multiple_attrs_d96() {
        // Несколько attrs подряд
        let mut w = Vec::new();
        let attrs = extract_inline_attrs(
            "#stable\n#since(\"1.0\")\n\nDescription.",
            "test",
            &mut w,
        );
        assert_eq!(attrs.tier, Some(StabilityTier::Stable));
        assert_eq!(attrs.since.as_deref(), Some("1.0"));
        assert!(w.is_empty());
    }

    #[test]
    fn mixed_bracket_and_canonical() {
        // Bracket form и canonical не должны смешиваться в одном doc,
        // но парсер должен принимать оба без паники.
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#stable\n\nDesc.", "test", &mut w);
        assert_eq!(attrs.tier, Some(StabilityTier::Stable));
        let attrs2 = extract_inline_attrs("#[stable]\n\nDesc.", "test", &mut w);
        assert_eq!(attrs2.tier, Some(StabilityTier::Stable));
        assert!(w.is_empty());
    }

    // ── Stop at non-attr ──────────────────────────────────────────────────────

    #[test]
    fn inline_attrs_stop_at_non_attr() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("Description.\n#stable", "test", &mut w);
        // Первая строка — не attr, остановились.
        assert_eq!(attrs.tier, None);
        assert!(w.is_empty());
    }

    // ── parse_attr_arg ────────────────────────────────────────────────────────

    #[test]
    fn parse_attr_arg_paren_string() {
        assert_eq!(parse_attr_arg("(\"hello\")").as_deref(), Some("hello"));
    }

    #[test]
    fn parse_attr_arg_eq_string() {
        assert_eq!(parse_attr_arg("= \"world\"").as_deref(), Some("world"));
    }

    // ── parse_named_arg ───────────────────────────────────────────────────────

    #[test]
    fn parse_named_arg_since() {
        assert_eq!(
            parse_named_arg("(since = \"1.0.0\")", "since").as_deref(),
            Some("1.0.0")
        );
    }

    #[test]
    fn parse_named_arg_no_spaces() {
        assert_eq!(
            parse_named_arg("(since=\"0.5\")", "since").as_deref(),
            Some("0.5")
        );
    }

    // ── Warnings ─────────────────────────────────────────────────────────────

    #[test]
    fn malformed_deprecated_emits_warning() {
        // Plan 45 Ф.25.1: malformed attr → warning + skip.
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#deprecated()\n\nDesc.", "test::foo", &mut w);
        assert!(attrs.deprecated.is_none());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].rule, "malformed-stability-attr");
        assert_eq!(w[0].item_id, "test::foo");
        assert!(w[0].message.contains("malformed #deprecated"));
    }

    #[test]
    fn malformed_bracket_deprecated_emits_warning() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[deprecated()]\n\nDesc.", "test::foo", &mut w);
        assert!(attrs.deprecated.is_none());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].rule, "malformed-stability-attr");
    }

    #[test]
    fn unknown_attr_emits_warning() {
        // Plan 45 Ф.25.1: unknown attr → warning + skip.
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#future_thing\n\nDesc.", "test::bar", &mut w);
        assert!(attrs.tier.is_none());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].rule, "unknown-doc-attr");
        assert_eq!(w[0].item_id, "test::bar");
        assert!(w[0].message.contains("future_thing"));
    }

    #[test]
    fn unknown_bracket_attr_emits_warning() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[future_thing]\n\nDesc.", "test::bar", &mut w);
        assert!(attrs.tier.is_none());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].rule, "unknown-doc-attr");
        assert!(w[0].message.contains("future_thing"));
    }

    #[test]
    fn malformed_since_emits_warning() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#since\n", "test::baz", &mut w);
        assert!(attrs.since.is_none());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].rule, "malformed-stability-attr");
    }

    #[test]
    fn malformed_bracket_since_emits_warning() {
        let mut w = Vec::new();
        let attrs = extract_inline_attrs("#[since]\n", "test::baz", &mut w);
        assert!(attrs.since.is_none());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].rule, "malformed-stability-attr");
    }

    // ── strip_leading_attrs ───────────────────────────────────────────────────

    #[test]
    fn strip_canonical_attr() {
        let result = strip_leading_attrs("#stable\n\nDescription here.");
        assert_eq!(result, "Description here.");
    }

    #[test]
    fn strip_bracket_attr() {
        let result = strip_leading_attrs("#[stable]\n\nDescription here.");
        assert_eq!(result, "Description here.");
    }

    #[test]
    fn strip_multiple_attrs() {
        let result = strip_leading_attrs("#stable\n#since(\"1.0\")\n\nDescription.");
        assert_eq!(result, "Description.");
    }
}
