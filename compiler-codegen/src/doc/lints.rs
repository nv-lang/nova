//! Plan 45 Ф.23.12 — Style-guide lints (§11.5 catalog).
//!
//! Реализует 8 дополнительных lint правил поверх существующих
//! (broken-links + missing-summary из --check mode).
//!
//! Все lints — additive: `nova doc --check` агрегирует и exit 1 при любом
//! **Error**-severity нарушении. Plan 71 ввёл `Severity::Warning` для
//! `public-missing-stability` в default-режиме — этот lint emit'ит warning
//! (не блокирует CI), error только под `enforce-stability = true` в
//! `nova.toml [lib]`.

use crate::doc::doctree::*;
use std::path::Path;

/// Plan 71 / D127: severity-уровень lint'а.
///
/// Mapped в `nova doc --check` exit code: `Error` → exit 1, `Warning` →
/// печать в stderr без блокировки CI (если нет `--strict`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Блокирует CI — exit 1.
    Error,
    /// Печатается в stderr, exit 0 (если только нет `--strict` поверх).
    Warning,
}

impl Severity {
    /// Строковое имя для `--format json` output и CLI таблицы.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// Plan 71 / D127: конфигурация lint-pipeline'а.
///
/// Загружается из `nova.toml [lib]` (см. [`crate::manifest::Manifest`]).
/// `nova-cli` строит `LintConfig` per-invocation:
/// - `strict_stability` ← `manifest.enforce_stability` (или `false` если manifest не найден).
/// - `fixture_dirs` ← `LintConfig::default_fixture_dirs()` (hardcoded
///   `nova_tests / tests / examples / bench` для V1; manifest-config —
///   V2 если будет user-request).
#[derive(Debug, Clone)]
pub struct LintConfig {
    /// `true` — повышает severity `public-missing-stability` до `Error`.
    /// `false` (default) — оставляет `Warning`. Source:
    /// `[lib] enforce-stability = true` в `nova.toml`.
    pub strict_stability: bool,
    /// Path component names, маркирующие файл как
    /// test/example/bench fixture. Для таких файлов
    /// `public-missing-stability` skip'ается *полностью* — даже как
    /// warning. Default: `["nova_tests", "tests", "examples", "bench"]`.
    ///
    /// Match — по любому path-сегменту в абсолютном пути
    /// `DocModule.source_paths`. Это robust к layout'ам:
    /// `<repo>/nova_tests/foo.nv`, `<workspace>/tests/bar/baz.nv`,
    /// `<root>/bench/corpus/perf.nv` — все определяются как fixtures.
    pub fixture_dirs: Vec<String>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl LintConfig {
    /// Default-конфиг: lenient mode (strict_stability=false) +
    /// canonical fixture-dirs из Plan 71.
    pub fn with_defaults() -> Self {
        Self {
            strict_stability: false,
            fixture_dirs: Self::default_fixture_dirs(),
        }
    }

    /// Canonical fixture-dirs hardcoded for V1 (Plan 71).
    /// Поменять через manifest config — V2 (cм. Plan 71 open question №2).
    pub fn default_fixture_dirs() -> Vec<String> {
        vec![
            "nova_tests".to_string(),
            "tests".to_string(),
            "examples".to_string(),
            "bench".to_string(),
        ]
    }

    /// Проверить, является ли модуль fixture'ом (test/example/bench).
    ///
    /// Возвращает `true` если **любой** из `source_paths` модуля
    /// содержит сегмент из `fixture_dirs` в своём path-component
    /// списке. Folder-modules: достаточно, чтобы один peer был под
    /// fixture-dir (peers одного folder'а структурно делят родителя,
    /// так что обычно все peers либо все fixtures, либо все нет;
    /// `any` устойчив к edge case'ам).
    ///
    /// Пустой `source_paths` (synthesized DocTree) → `false`
    /// (fallthrough к строгому пути; для prod-кода irrelevant).
    pub fn is_fixture_module(&self, source_paths: &[std::path::PathBuf]) -> bool {
        if source_paths.is_empty() {
            return false;
        }
        let fixture_set: std::collections::HashSet<&str> =
            self.fixture_dirs.iter().map(String::as_str).collect();
        source_paths
            .iter()
            .any(|p| path_has_component(p, &fixture_set))
    }
}

/// Helper: возвращает `true` если у `p` есть Normal-сегмент, имя которого
/// (как `&str`) присутствует в `set`. Robust ко всем path-форматам:
/// абсолютный/relative, Windows backslash/Unix slash, `..` / `.` сегменты
/// (которые мы игнорируем — Plan 71 не enforce'ит paths за пределы repo).
fn path_has_component(p: &Path, set: &std::collections::HashSet<&str>) -> bool {
    use std::path::Component;
    for c in p.components() {
        if let Component::Normal(os) = c {
            if let Some(s) = os.to_str() {
                if set.contains(s) {
                    return true;
                }
            }
        }
    }
    false
}

/// Один lint-нарушение.
#[derive(Debug, Clone)]
pub struct DocLintViolation {
    /// Item ID (или module path для module-level nits).
    pub item_id: String,
    /// Код правила: "imperative-mood", "section-order", etc.
    pub rule: &'static str,
    /// Человекочитаемое объяснение.
    pub message: String,
    /// Plan 71 / D127: severity. `Error` блокирует CI; `Warning` —
    /// печатается, но `nova doc --check` exit 0. Все правила кроме
    /// `public-missing-stability` всегда `Error` (исторический default).
    /// `public-missing-stability` — `Warning` по default, `Error` если
    /// `LintConfig.strict_stability == true`.
    pub severity: Severity,
}

/// Запускает все §11.5 lints на DocTree. Возвращает список нарушений.
///
/// Plan 71: signature changed — теперь принимает `&LintConfig` для
/// контроля `public-missing-stability` severity + fixture-skip. Caller'ы
/// без специфической конфигурации могут использовать
/// `&LintConfig::default()`.
pub fn run_lints(tree: &DocTree, config: &LintConfig) -> Vec<DocLintViolation> {
    let mut out = Vec::new();
    for m in &tree.modules {
        // Plan 71 / D127: test/example/bench exemption — skip
        // public-missing-stability полностью для fixtures.
        let is_fixture = config.is_fixture_module(&m.source_paths);
        lint_module(m, config, is_fixture, &mut out);
        for it in &m.items {
            lint_item(it, config, is_fixture, &mut out);
        }
    }
    out
}

fn lint_module(
    m: &DocModule,
    config: &LintConfig,
    is_fixture: bool,
    out: &mut Vec<DocLintViolation>,
) {
    // Rule 7: public module без stability tier.
    // Plan 71 / D127:
    //   - fixture path → skip полностью (не emit'им даже warning),
    //   - strict_stability=true → severity=Error,
    //   - strict_stability=false → severity=Warning.
    if !m.hide_doc && m.stability.is_none() && !is_fixture {
        let severity = if config.strict_stability {
            Severity::Error
        } else {
            Severity::Warning
        };
        out.push(DocLintViolation {
            item_id: m.path.join("."),
            rule: "public-missing-stability",
            message: "public module has no stability tier (#stable / #unstable / #experimental)".to_string(),
            severity,
        });
    }
}

fn lint_item(
    it: &DocItem,
    config: &LintConfig,
    is_fixture: bool,
    out: &mut Vec<DocLintViolation>,
) {
    let id = &it.id;

    // Rule 8: summary слишком длинная (> 120 символов).
    if let Some(s) = &it.summary {
        if s.len() > 120 {
            out.push(DocLintViolation {
                item_id: id.clone(),
                rule: "summary-too-long",
                message: format!("summary is {} chars (max 120); move details to description", s.len()),
                severity: Severity::Error,
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
                    severity: Severity::Error,
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
                severity: Severity::Error,
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
                severity: Severity::Error,
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
                    severity: Severity::Error,
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
                severity: Severity::Error,
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
                    severity: Severity::Error,
                });
            }
        }
    }

    // Rule 7: exported item без stability.
    // Plan 71 / D127:
    //   - fixture path → skip полностью (не emit'им даже warning),
    //   - strict_stability=true → severity=Error,
    //   - strict_stability=false → severity=Warning.
    if it.visibility == Visibility::Export && it.stability.is_none() && !is_fixture {
        let severity = if config.strict_stability {
            Severity::Error
        } else {
            Severity::Warning
        };
        out.push(DocLintViolation {
            item_id: id.clone(),
            rule: "public-missing-stability",
            message: "exported item has no stability tier (#stable / #unstable / #experimental)".to_string(),
            severity,
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
                severity: Severity::Error,
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
                severity: Severity::Error,
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
