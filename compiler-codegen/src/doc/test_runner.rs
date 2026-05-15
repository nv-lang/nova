//! Plan 45 Ф.7+ — doc-test runner.
//!
//! Принимает `Vec<DocTest>` из collector'а и для каждого:
//! - `ignore` → SKIPPED.
//! - `compile_fail` → парсим + type-check, ожидаем error.
//! - `no_run` → парсим + type-check, ожидаем success.
//! - `should_panic` → парсим + type-check + run, ожидаем runtime error.
//! - `must_verify` → SMT verify (Plan 33) — отложено, помечается как
//!   SKIPPED со специальной причиной (`must_verify not yet wired`).
//! - Иначе — полный pipeline: parse → typecheck → run main.
//!
//! Каждый doc-test обёрнут в синтетический module + `fn main` block:
//!
//! ```nova
//! module __doctest__
//! <full_source как тело main'а>
//! ```
//!
//! Это позволяет писать примеры как тело функции (`let x = 1; assert(...)`)
//! без верхнего-уровня import'ов (если они не указаны через hidden `# `).

use super::doctree::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocTestOutcome {
    Passed,
    Failed(String),
    Skipped(String),
}

#[derive(Debug, Clone)]
pub struct DocTestResult {
    pub id: String,
    pub outcome: DocTestOutcome,
}

pub struct DocTestSummary {
    pub results: Vec<DocTestResult>,
}

impl DocTestSummary {
    pub fn passed(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.outcome == DocTestOutcome::Passed)
            .count()
    }
    pub fn failed(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r.outcome, DocTestOutcome::Failed(_)))
            .count()
    }
    pub fn skipped(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r.outcome, DocTestOutcome::Skipped(_)))
            .count()
    }
    pub fn all_passed(&self) -> bool {
        self.failed() == 0
    }
}

pub fn run_doc_tests(tests: &[DocTest]) -> DocTestSummary {
    let mut results = Vec::with_capacity(tests.len());
    for t in tests {
        let outcome = run_one(t);
        results.push(DocTestResult {
            id: t.id.clone(),
            outcome,
        });
    }
    DocTestSummary { results }
}

fn run_one(t: &DocTest) -> DocTestOutcome {
    let modifiers = &t.modifiers;
    if modifiers.contains(&DocTestModifier::Ignore) {
        return DocTestOutcome::Skipped("ignore modifier".to_string());
    }
    if modifiers.contains(&DocTestModifier::MustVerify) {
        // SMT verification — Plan 33; doc-test runner вызывает SMT
        // pipeline отдельно (Plan 45 Ф.7.B). MVP: skip.
        return DocTestOutcome::Skipped("must_verify not yet wired".to_string());
    }

    let synthetic = wrap_source(&t.full_source);
    // 1. Parse.
    let parse_result = crate::parser::parse(&synthetic);
    let compile_fail = modifiers.contains(&DocTestModifier::CompileFail);

    let mut module = match parse_result {
        Ok(m) => m,
        Err(d) => {
            if compile_fail {
                return DocTestOutcome::Passed;
            }
            return DocTestOutcome::Failed(format!("parse error: {}", d.message));
        }
    };

    // 2. Type-check.
    if let Err(errs) = crate::types::check_module(&module) {
        if compile_fail {
            return DocTestOutcome::Passed;
        }
        let msg = errs
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return DocTestOutcome::Failed(format!("type-check error: {}", msg));
    }

    if compile_fail {
        // Ожидали ошибку — не получили.
        return DocTestOutcome::Failed("compile_fail: expected error, got success".to_string());
    }

    if modifiers.contains(&DocTestModifier::NoRun) {
        return DocTestOutcome::Passed;
    }

    // 3. Execute.
    crate::callnorm::normalize_module(&mut module);
    let mut interp = crate::interp::Interpreter::new();
    if let Err(d) = interp.load_module(&module) {
        return DocTestOutcome::Failed(format!("load error: {}", d.message));
    }
    let run_result = interp.run_main();
    let should_panic = modifiers.contains(&DocTestModifier::ShouldPanic);
    match (run_result, should_panic) {
        (Ok(_), false) => DocTestOutcome::Passed,
        (Ok(_), true) => {
            DocTestOutcome::Failed("should_panic: expected panic, got success".to_string())
        }
        (Err(_), true) => DocTestOutcome::Passed,
        (Err(d), false) => DocTestOutcome::Failed(format!("runtime error: {}", d.message)),
    }
}

/// Обернуть исходник doc-test'а в синтетический module + main.
///
/// Простая стратегия: если `source` уже содержит верхнеуровневые
/// declarations (`fn`/`type`/`let`/`module`/`import`) — оставляем как
/// есть, добавляя только префиксный `module __doctest__`. Иначе —
/// обворачиваем тело в `fn main() => { <source> }`.
fn wrap_source(source: &str) -> String {
    if has_top_level_decl(source) {
        return format!("module __doctest__\n{}", source);
    }
    // Тело main'а: оборачиваем в block-expression.
    format!(
        "module __doctest__\n\nfn main() -> () => {{\n{}\n}}",
        source
    )
}

fn has_top_level_decl(source: &str) -> bool {
    // Грубая эвристика: ищем строку, начинающуюся с keyword'а
    // верхнеуровневой декларации (вне `///` doc-комментариев).
    for line in source.lines() {
        let t = line.trim_start();
        if t.starts_with("///") || t.starts_with("//") || t.is_empty() {
            continue;
        }
        for kw in &["module ", "import ", "fn ", "type ", "export ", "const "] {
            if t.starts_with(kw) {
                return true;
            }
        }
        // Первая non-comment строка не была декларацией — это body.
        return false;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Span;

    fn make_test(source: &str, modifiers: Vec<DocTestModifier>) -> DocTest {
        DocTest {
            id: "test::doc_test_1".to_string(),
            from_id: None,
            index: 1,
            modifiers,
            visible_source: source.to_string(),
            full_source: source.to_string(),
        }
    }

    #[test]
    fn passes_trivial_body() {
        let t = make_test("let _ = 1\n", vec![]);
        let s = run_doc_tests(std::slice::from_ref(&t));
        assert_eq!(s.results[0].outcome, DocTestOutcome::Passed, "{:?}", s.results[0].outcome);
    }

    #[test]
    fn ignore_skips() {
        let t = make_test("garbage", vec![DocTestModifier::Ignore]);
        let s = run_doc_tests(std::slice::from_ref(&t));
        assert!(matches!(s.results[0].outcome, DocTestOutcome::Skipped(_)));
    }

    #[test]
    fn no_run_passes_when_compiles() {
        let t = make_test("let x = 1\n", vec![DocTestModifier::NoRun]);
        let s = run_doc_tests(std::slice::from_ref(&t));
        assert_eq!(s.results[0].outcome, DocTestOutcome::Passed);
    }

    #[test]
    fn compile_fail_passes_when_fails() {
        let t = make_test(
            "let x: int = \"not an int\"\n",
            vec![DocTestModifier::CompileFail],
        );
        let s = run_doc_tests(std::slice::from_ref(&t));
        assert_eq!(s.results[0].outcome, DocTestOutcome::Passed);
    }

    #[test]
    fn compile_fail_fails_when_compiles() {
        let t = make_test("let x = 1\n", vec![DocTestModifier::CompileFail]);
        let s = run_doc_tests(std::slice::from_ref(&t));
        assert!(matches!(s.results[0].outcome, DocTestOutcome::Failed(_)));
    }

    #[test]
    fn must_verify_skipped() {
        let t = make_test("let x = 1\n", vec![DocTestModifier::MustVerify]);
        let s = run_doc_tests(std::slice::from_ref(&t));
        assert!(matches!(s.results[0].outcome, DocTestOutcome::Skipped(_)));
    }

    #[test]
    fn wraps_body_correctly() {
        let wrapped = wrap_source("let x = 1\n");
        assert!(wrapped.contains("fn main"));
        assert!(wrapped.contains("let x = 1"));
    }

    #[test]
    fn top_level_decl_not_wrapped_in_main() {
        let wrapped = wrap_source("fn helper() -> int => 42\n");
        // Не должно быть обёртки в main — оставлено как есть.
        assert!(!wrapped.contains("fn main"));
        assert!(wrapped.contains("fn helper"));
    }

    // Suppress dead-code warning for Span import.
    #[allow(dead_code)]
    fn _force_use() -> Span {
        Span { start: 0, end: 0, file_id: 0 }
    }
}
