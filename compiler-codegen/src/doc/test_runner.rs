//! Plan 45 Ф.7 + Ф.21.1 — doc-test runner.
//!
//! Принимает `Vec<DocTest>` из collector'а и для каждого:
//! - `ignore` → SKIPPED.
//! - `compile_fail` → парсим + type-check, ожидаем error.
//! - `no_run` → парсим + type-check, ожидаем success.
//! - `should_panic` → парсим + type-check + run, ожидаем runtime error.
//! - `must_verify` → SMT verify (Plan 33) — wiring см. `Ф.21.4`.
//! - Иначе — полный pipeline: parse → typecheck → run main.
//!
//! **Ф.21.1 — Crate-scope для doc-tests.** Rustdoc автоматически даёт
//! doc-test'у `use crate::*` scope. У нас аналог: `run_doc_tests_with_source`
//! принимает оригинальный source документируемого файла, **встраивает**
//! его перед test-body. Это позволяет doc-test'у вызывать любые items
//! документируемого модуля (`assert(double(3) == 6)` работает в doc'е
//! `fn double`).
//!
//! Конфликт `fn main`: если оригинальный файл содержит `fn main`, она
//! автоматически переименовывается в `__orig_main` (textual rewrite),
//! чтобы оставить `fn main` доступной для wrapped test body.

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
    run_doc_tests_with_source(tests, None)
}

/// Plan 45 Ф.21.1: prod-grade entry — каждый test получает crate-scope
/// (items документируемого модуля), как в rustdoc.
pub fn run_doc_tests_with_source(
    tests: &[DocTest],
    original_source: Option<&str>,
) -> DocTestSummary {
    let mut results = Vec::with_capacity(tests.len());
    for t in tests {
        let outcome = run_one(t, original_source);
        results.push(DocTestResult {
            id: t.id.clone(),
            outcome,
        });
    }
    DocTestSummary { results }
}

fn run_one(t: &DocTest, original_source: Option<&str>) -> DocTestOutcome {
    let modifiers = &t.modifiers;
    if modifiers.contains(&DocTestModifier::Ignore) {
        return DocTestOutcome::Skipped("ignore modifier".to_string());
    }
    let synthetic = wrap_source(&t.full_source, original_source, t.test_handlers.as_deref());
    // 1. Parse.
    let parse_result = crate::parser::parse(&synthetic);
    let compile_fail = modifiers.contains(&DocTestModifier::CompileFail);
    let must_verify = modifiers.contains(&DocTestModifier::MustVerify);

    let mut module = match parse_result {
        Ok(m) => m,
        Err(d) => {
            if compile_fail {
                return DocTestOutcome::Passed;
            }
            // Plan 45 Ф.21.8: render diagnostic с span/snippet через
            // существующую Diagnostic.render. Path помечен `<doc-test>`
            // (синтетический source).
            return DocTestOutcome::Failed(format!(
                "parse error:\n{}",
                d.render(&synthetic, "<doc-test>")
            ));
        }
    };

    // 2. Type-check.
    if let Err(errs) = crate::types::check_module(&module) {
        if compile_fail {
            return DocTestOutcome::Passed;
        }
        // Plan 45 Ф.21.8: rendered diagnostics с span/snippet — берём
        // первый error (тот, что наиболее indicative) + count'ом
        // дополнительных.
        let first = errs.first().expect("Err(_) implies non-empty");
        let rendered = first.render(&synthetic, "<doc-test>");
        let extra = if errs.len() > 1 {
            format!("\n  (+{} more error{})", errs.len() - 1, if errs.len() == 2 { "" } else { "s" })
        } else {
            String::new()
        };
        return DocTestOutcome::Failed(format!("type-check error:\n{}{}", rendered, extra));
    }

    if compile_fail {
        // Ожидали ошибку — не получили.
        return DocTestOutcome::Failed("compile_fail: expected error, got success".to_string());
    }

    if must_verify {
        // Plan 45 Ф.21.4: SMT verification через Plan 33 pipeline.
        // Запускаем `verify_module` на синтетическом модуле; success
        // ⇔ no errors. Counterexamples (warnings) — игнорируем (это
        // hint'ы для контрактов без `#verify` — не наш кейс).
        let report = crate::verify::pipeline::verify_module(&module);
        if !report.errors.is_empty() {
            // Ф.21.8: rendered diagnostics для SMT-failures.
            let first = &report.errors[0];
            let rendered = first.render(&synthetic, "<doc-test>");
            let extra = if report.errors.len() > 1 {
                format!("\n  (+{} more)", report.errors.len() - 1)
            } else {
                String::new()
            };
            return DocTestOutcome::Failed(format!("must_verify failed:\n{}{}", rendered, extra));
        }
        return DocTestOutcome::Passed;
    }

    if modifiers.contains(&DocTestModifier::NoRun) {
        return DocTestOutcome::Passed;
    }

    // 3. Execute.
    crate::callnorm::normalize_module(&mut module);
    let mut interp = crate::interp::Interpreter::new();
    if let Err(d) = interp.load_module(&module) {
        return DocTestOutcome::Failed(format!(
            "load error:\n{}",
            d.render(&synthetic, "<doc-test>")
        ));
    }

    let expect_output = modifiers.contains(&DocTestModifier::ExpectOutput);
    // Plan 45 Ф.24.8: activate output capture before run_main when expect_output.
    if expect_output {
        crate::interp::stdlib::capture_output_start();
    }

    let run_result = interp.run_main();
    let should_panic = modifiers.contains(&DocTestModifier::ShouldPanic);

    // Collect captured output before deciding outcome.
    let actual_output = if expect_output {
        Some(crate::interp::stdlib::capture_output_finish())
    } else {
        None
    };

    let base_outcome = match (run_result, should_panic) {
        (Ok(_), false) => DocTestOutcome::Passed,
        (Ok(_), true) => {
            DocTestOutcome::Failed("should_panic: expected panic, got success".to_string())
        }
        (Err(_), true) => DocTestOutcome::Passed,
        (Err(d), false) => DocTestOutcome::Failed(format!(
            "runtime error:\n{}",
            d.render(&synthetic, "<doc-test>")
        )),
    };

    // Plan 45 Ф.24.8: if base run passed and expect_output + expected annotation present — diff.
    // If no // Output: annotations found (expected_output = None) — skip output check,
    // just verify the test ran without error (useful for documenting side-effectful examples).
    if expect_output {
        if let DocTestOutcome::Passed = base_outcome {
            if let Some(expected) = t.expected_output.as_deref() {
                let actual = actual_output.unwrap_or_default();
                let actual_norm = actual.replace("\r\n", "\n");
                let expected_norm = expected.replace("\r\n", "\n");
                if actual_norm == expected_norm {
                    return DocTestOutcome::Passed;
                } else {
                    return DocTestOutcome::Failed(format!(
                        "expect_output mismatch:\n  expected: {:?}\n  actual:   {:?}",
                        expected_norm, actual_norm
                    ));
                }
            }
        }
    }

    base_outcome
}

/// Обернуть исходник doc-test'а.
///
/// **Ф.21.1**: если предоставлен `original_source` документируемого
/// файла, используем его как base (рустдок-style `use crate::*`):
/// - Берём оригинальный source как есть.
/// - Переименовываем `fn main` (если есть) → `__orig_main` (textual rewrite).
/// - Добавляем test wrapped в новый `fn main`.
/// Test получает доступ ко всем exports + imports оригинального модуля.
///
/// **Fallback** (None): synthetic `module __doctest__` без scope (для
/// unit-тестов и backward-compat).
/// Plan 45 Ф.23.7 / D106: wrap doc-test source. If `handlers` is set —
/// wraps `fn main` body in `with <handler>` block.
fn wrap_source(test_source: &str, original_source: Option<&str>, handlers: Option<&str>) -> String {
    let test_part = if has_top_level_decl(test_source) {
        test_source.to_string()
    } else if let Some(h) = handlers {
        // Injet `with <handler>` around body.
        format!("fn main() -> () => {{\n    with {} = {} {{\n{}\n    }}\n}}", h, h, test_source)
    } else {
        format!("fn main() -> () => {{\n{}\n}}", test_source)
    };
    match original_source {
        Some(orig) => {
            let cleaned = rename_main_in_source(orig);
            format!("{}\n\n{}\n", cleaned, test_part)
        }
        None => format!("module __doctest__\n\n{}\n", test_part),
    }
}

/// Textual rewrite `fn main(` → `fn __orig_main(` (+ `export fn main(`
/// variant). Per-line — robust для стандартного Nova formatting. Не
/// затрагивает строки, начинающиеся с whitespace (тело других функций).
fn rename_main_in_source(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 32);
    let mut first = true;
    for line in src.lines() {
        if !first {
            out.push('\n');
        }
        first = false;
        if let Some(rest) = line.strip_prefix("fn main(") {
            out.push_str("fn __orig_main(");
            out.push_str(rest);
        } else if let Some(rest) = line.strip_prefix("export fn main(") {
            out.push_str("export fn __orig_main(");
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
    }
    out
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
            test_handlers: None,
            expected_output: None,
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
    fn must_verify_passes_trivial() {
        // Ф.21.4: must_verify wiring к Plan 33 SMT. Trivial-test без
        // контрактов → verify_module не возвращает errors → Passed.
        let t = make_test("let x = 1\n", vec![DocTestModifier::MustVerify]);
        let s = run_doc_tests(std::slice::from_ref(&t));
        assert_eq!(s.results[0].outcome, DocTestOutcome::Passed,
            "must_verify on trivial test should pass (no contracts → no SMT failures): {:?}",
            s.results[0].outcome);
    }

    #[test]
    fn wraps_body_correctly() {
        let wrapped = wrap_source("let x = 1\n", None, None);
        assert!(wrapped.contains("fn main"));
        assert!(wrapped.contains("let x = 1"));
    }

    #[test]
    fn top_level_decl_not_wrapped_in_main() {
        let wrapped = wrap_source("fn helper() -> int => 42\n", None, None);
        // Не должно быть обёртки в main — оставлено как есть.
        assert!(!wrapped.contains("fn main"));
        assert!(wrapped.contains("fn helper"));
    }

    #[test]
    fn wrap_with_original_source_injects_module() {
        let orig = "module my.mod\n\nexport fn double(x int) -> int => x * 2\n";
        let wrapped = wrap_source("let r = double(3)\n", Some(orig), None);
        assert!(wrapped.contains("module my.mod"));
        assert!(wrapped.contains("fn double"));
        assert!(wrapped.contains("fn main"));
        assert!(wrapped.contains("let r = double(3)"));
    }

    #[test]
    fn rename_main_handles_both_forms() {
        let s = "fn main() => println(\"hi\")\nfn helper() -> int => 1\n";
        let r = rename_main_in_source(s);
        assert!(r.starts_with("fn __orig_main()"));
        assert!(r.contains("fn helper")); // helper untouched

        let s2 = "export fn main(args []str) -> int => 0\n";
        let r2 = rename_main_in_source(s2);
        assert!(r2.starts_with("export fn __orig_main("));
    }

    #[test]
    fn rename_main_no_main_unchanged() {
        let s = "module x\n\nexport fn other() => ()\n";
        assert_eq!(rename_main_in_source(s), s.trim_end());
    }

    // Suppress dead-code warning for Span import.
    #[allow(dead_code)]
    fn _force_use() -> Span {
        Span { start: 0, end: 0, file_id: 0 }
    }
}
