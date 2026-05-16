//! Plan 45 Ф.23.12 — Style-guide lints (§11.5 catalog).
//!
//! Реализует 8 дополнительных lint правил поверх существующих
//! (broken-links + missing-summary из --check mode).
//!
//! Все lints — additive: `nova doc --check` агрегирует и exit 1 при любом.

use crate::doc::doctree::*;

/// Один lint-нарушение.
#[derive(Debug, Clone)]
pub struct DocLintViolation {
    /// Item ID (или module path для module-level nits).
    pub item_id: String,
    /// Код правила: "imperative-mood", "section-order", etc.
    pub rule: &'static str,
    /// Человекочитаемое объяснение.
    pub message: String,
}

/// Запускает все §11.5 lints на DocTree. Возвращает список нарушений.
pub fn run_lints(tree: &DocTree) -> Vec<DocLintViolation> {
    let mut out = Vec::new();
    for m in &tree.modules {
        lint_module(m, &mut out);
        for it in &m.items {
            lint_item(it, &mut out);
        }
    }
    out
}

fn lint_module(m: &DocModule, out: &mut Vec<DocLintViolation>) {
    // Rule 7: public module без stability tier.
    if !m.hide_doc && m.stability.is_none() {
        out.push(DocLintViolation {
            item_id: m.path.join("."),
            rule: "public-missing-stability",
            message: "public module has no stability tier (#stable / #unstable / #experimental)".to_string(),
        });
    }
}

fn lint_item(it: &DocItem, out: &mut Vec<DocLintViolation>) {
    let id = &it.id;

    // Rule 8: summary слишком длинная (> 120 символов).
    if let Some(s) = &it.summary {
        if s.len() > 120 {
            out.push(DocLintViolation {
                item_id: id.clone(),
                rule: "summary-too-long",
                message: format!("summary is {} chars (max 120); move details to description", s.len()),
            });
        }

        // Plan 45 Ф.26.4: Rule 1 (style-guide §11.5 №2) — summary-not-sentence.
        // Summary должно быть полным грамматическим предложением: capital first letter +
        // оканчивается на `.`, `!`, `?`.
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            let starts_with_capital = trimmed.chars().next()
                .map(|c| c.is_uppercase() || !c.is_alphabetic()) // не alphabetic (digit/symbol) тоже OK
                .unwrap_or(true);
            let ends_with_terminator = trimmed.ends_with('.')
                || trimmed.ends_with('!')
                || trimmed.ends_with('?');
            if !starts_with_capital || !ends_with_terminator {
                let reason = if !starts_with_capital && !ends_with_terminator {
                    "should start with capital letter AND end with `.`, `!`, or `?`"
                } else if !starts_with_capital {
                    "should start with capital letter"
                } else {
                    "should end with `.`, `!`, or `?`"
                };
                out.push(DocLintViolation {
                    item_id: id.clone(),
                    rule: "summary-not-sentence",
                    message: format!("summary {} (style-guide §11.5 №1)", reason),
                });
            }
        }

        // Rule 1: summary не в imperative mood (не начинается с глагола-инфинитива).
        // Эвристика: первое слово — known non-verb (The, A, An, This, Returns...).
        let first_word = s.split_whitespace().next().unwrap_or("");
        let non_imperative = ["The", "A", "An", "This", "Returns", "Gets", "Provides", "Represents", "Implements", "Wraps"];
        if non_imperative.iter().any(|&w| first_word == w) {
            out.push(DocLintViolation {
                item_id: id.clone(),
                rule: "imperative-mood",
                message: format!("summary should start with an imperative verb, not '{}'", first_word),
            });
        }
    }

    // Plan 45 Ф.26.4 / §11.5 №2 — unknown-section.
    // Canonical sections list — те же что в Rule 2 section-order ниже.
    // Если в `it.sections` есть key не из этого списка — это unknown.
    const CANONICAL_SECTIONS: &[&str] = &[
        "examples", "errors", "panics", "safety", "effects",
        "contracts", "since", "see also", "deprecated",
    ];
    for key in it.sections.keys() {
        if !CANONICAL_SECTIONS.contains(&key.as_str()) {
            out.push(DocLintViolation {
                item_id: id.clone(),
                rule: "unknown-section",
                message: format!(
                    "section `# {}` is not in canonical catalog (allowed: {})",
                    key,
                    CANONICAL_SECTIONS.join(", ")
                ),
            });
        }
    }

    // Rule 5: public fn без examples секции.
    if it.visibility == Visibility::Export {
        if let ItemKind::Fn(_) = &it.kind {
            if !it.sections.contains_key("examples") {
                out.push(DocLintViolation {
                    item_id: id.clone(),
                    rule: "examples-missing",
                    message: "public function has no # Examples section".to_string(),
                });
            }
        }
    }

    // Rule 6: deprecated без since или note.
    if let Some(dep) = &it.deprecation {
        if dep.since.is_none() && dep.note.is_empty() {
            out.push(DocLintViolation {
                item_id: id.clone(),
                rule: "deprecated-incomplete",
                message: "deprecated item missing both `since` version and deprecation note".to_string(),
            });
        }
        // Plan 45 Ф.26.4 / §11.5 №6 — deprecated-overdue.
        // Если `#deprecated(until = "X.Y")` и текущая версия (env NOVA_VERSION или
        // CARGO_PKG_VERSION) ≥ X.Y → lint error. Используется CI как gate перед release.
        if let Some(until) = &dep.until {
            let current = std::env::var("NOVA_VERSION").ok()
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
            if version_at_or_above(&current, until) {
                out.push(DocLintViolation {
                    item_id: id.clone(),
                    rule: "deprecated-overdue",
                    message: format!(
                        "deprecation `until = \"{}\"` reached (current = `{}`); item should be removed",
                        until, current
                    ),
                });
            }
        }
    }

    // Rule 7: exported item без stability.
    if it.visibility == Visibility::Export && it.stability.is_none() {
        out.push(DocLintViolation {
            item_id: id.clone(),
            rule: "public-missing-stability",
            message: "exported item has no stability tier (#stable / #unstable / #experimental)".to_string(),
        });
    }

    // Rule 2: section order violation.
    // Canonical order: examples < errors < panics < safety < effects < contracts < since < see also < deprecated.
    const ORDER: &[&str] = &["examples", "errors", "panics", "safety", "effects", "contracts", "since", "see also", "deprecated"];
    let present: Vec<usize> = ORDER.iter().enumerate()
        .filter(|(_, k)| it.sections.contains_key(**k))
        .map(|(i, _)| i)
        .collect();
    for w in present.windows(2) {
        if w[0] > w[1] {
            out.push(DocLintViolation {
                item_id: id.clone(),
                rule: "section-order",
                message: format!("section '{}' appears before '{}' (canonical order: {})",
                    ORDER[w[1]], ORDER[w[0]],
                    ORDER.iter().filter(|k| it.sections.contains_key(**k)).cloned().collect::<Vec<_>>().join(" < ")),
            });
            break;
        }
    }

    // Rule 3: markdown-subset — no raw HTML in description.
    for text in [it.description.as_deref()].into_iter().flatten() {
        if text.contains("<script") || text.contains("<iframe") || text.contains("<style") {
            out.push(DocLintViolation {
                item_id: id.clone(),
                rule: "markdown-subset",
                message: "description contains disallowed raw HTML tags (<script>, <iframe>, <style>)".to_string(),
            });
        }
    }
}

/// Plan 45 Ф.26.4 — version comparison для `deprecated-overdue` lint.
/// Semver-ish: split на `.`, parse как u32, compare lexicographically.
/// Префикс `v` stripped (e.g., `v1.2.3` → `1.2.3`).
/// Если parse fails — return false (conservative: не flag'аем как overdue).
fn version_at_or_above(current: &str, until: &str) -> bool {
    let cur = parse_version(current);
    let unt = parse_version(until);
    match (cur, unt) {
        (Some(c), Some(u)) => c >= u,
        _ => false,
    }
}

fn parse_version(s: &str) -> Option<Vec<u32>> {
    let s = s.trim().trim_start_matches('v');
    // Take prefix up to first non-numeric/dot (e.g., `1.0.0-rc1` → `1.0.0`).
    let prefix: String = s.chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let parts: Result<Vec<u32>, _> = prefix.split('.')
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<u32>())
        .collect();
    parts.ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse_basic() {
        assert_eq!(parse_version("1.2.3"), Some(vec![1, 2, 3]));
        assert_eq!(parse_version("v1.0"), Some(vec![1, 0]));
        assert_eq!(parse_version("0.5.1-rc1"), Some(vec![0, 5, 1]));
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("notaversion"), None);
    }

    #[test]
    fn version_compare_basic() {
        assert!(version_at_or_above("1.2.3", "1.2.3"));
        assert!(version_at_or_above("1.2.4", "1.2.3"));
        assert!(version_at_or_above("2.0.0", "1.99.99"));
        assert!(!version_at_or_above("1.2.2", "1.2.3"));
        assert!(!version_at_or_above("0.9.0", "1.0.0"));
    }

    #[test]
    fn version_compare_different_lengths() {
        // 1.0 vs 1.0.0: vec![1,0] < vec![1,0,0] lexicographically.
        // Это разумно (1.0.0 — более specific, считается later release).
        assert!(version_at_or_above("1.0.0", "1.0"));
        assert!(!version_at_or_above("1.0", "1.0.0"));
    }

    #[test]
    fn version_unparseable_returns_false() {
        // Conservative: malformed input не flag'ит как overdue.
        assert!(!version_at_or_above("garbage", "1.0.0"));
        assert!(!version_at_or_above("1.0.0", "garbage"));
    }
}
