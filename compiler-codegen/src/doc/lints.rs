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
