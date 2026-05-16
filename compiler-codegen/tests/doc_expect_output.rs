//! Plan 45 Р¤.24.8 вЂ” integration tests for `expect_output` doc-test modifier.
//!
//! Verifies that:
//! 1. `// Output: <text>` lines are parsed from fenced blocks.
//! 2. Runner captures stdout and diffs against expected.
//! 3. Drift detection: wrong expected в†’ Failed with clear message.

use nova_codegen::doc::doctree::DocTestModifier;

fn make_tree_with_source(src: &str) -> (nova_codegen::doc::DocTree, String) {
    let mut module = nova_codegen::parser::parse(src)
        .unwrap_or_else(|d| panic!("parse: {}", d.message));
    let _ = nova_codegen::types::check_module(&module);
    nova_codegen::types::infer_effects(&mut module);
    let tree = nova_codegen::doc::build(&module);
    (tree, src.to_string())
}

#[test]
fn expect_output_pass() {
    let src = r#"
module doc_tests.expect_output_pass

/// Adds two numbers.
///
/// ```nova,expect_output
/// println(add(2, 3))
/// // Output: 5
/// ```
export fn add(a int, b int) -> int => a + b
"#;
    let (tree, source) = make_tree_with_source(src);
    assert_eq!(tree.doc_tests.len(), 1);
    let dt = &tree.doc_tests[0];
    assert!(dt.modifiers.contains(&DocTestModifier::ExpectOutput));
    assert_eq!(dt.expected_output.as_deref(), Some("5"));

    let summary = nova_codegen::doc::test_runner::run_doc_tests_with_source(
        &tree.doc_tests,
        Some(&source),
    );
    assert!(summary.all_passed(), "expect_output test should pass: {:?}", summary.results[0].outcome);
}

#[test]
fn expect_output_multiline() {
    let src = r#"
module doc_tests.expect_output_multi

/// Print multiple lines.
///
/// ```nova,expect_output
/// println("line1")
/// println("line2")
/// // Output: line1
/// // Output: line2
/// ```
export fn demo() -> () => ()
"#;
    let (tree, source) = make_tree_with_source(src);
    assert_eq!(tree.doc_tests.len(), 1);
    let dt = &tree.doc_tests[0];
    assert_eq!(dt.expected_output.as_deref(), Some("line1\nline2"));

    let summary = nova_codegen::doc::test_runner::run_doc_tests_with_source(
        &tree.doc_tests,
        Some(&source),
    );
    assert!(summary.all_passed(), "multiline expect_output should pass: {:?}", summary.results[0].outcome);
}

#[test]
fn expect_output_drift_detected() {
    let src = r#"
module doc_tests.expect_output_drift

/// Wrong expected output.
///
/// ```nova,expect_output
/// println(42)
/// // Output: 999
/// ```
export fn demo() -> () => ()
"#;
    let (tree, source) = make_tree_with_source(src);
    let summary = nova_codegen::doc::test_runner::run_doc_tests_with_source(
        &tree.doc_tests,
        Some(&source),
    );
    assert!(!summary.all_passed(), "drift should be detected");
    match &summary.results[0].outcome {
        nova_codegen::doc::test_runner::DocTestOutcome::Failed(msg) => {
            assert!(msg.contains("expect_output mismatch"), "wrong failure message: {}", msg);
        }
        other => panic!("expected Failed, got {:?}", other),
    }
}

#[test]
fn expect_output_no_annotation_passes_normally() {
    // expect_output without any // Output: annotations в†’ expected = None.
    // Runner should still run but not check output (graceful).
    let src = r#"
module doc_tests.expect_output_no_ann

/// No output annotation.
///
/// ```nova,expect_output
/// println("hello")
/// ```
export fn demo() -> () => ()
"#;
    let (tree, source) = make_tree_with_source(src);
    let dt = &tree.doc_tests[0];
    assert!(dt.expected_output.is_none());

    let summary = nova_codegen::doc::test_runner::run_doc_tests_with_source(
        &tree.doc_tests,
        Some(&source),
    );
    // With no expected_output, runner runs normally вЂ” no output check.
    // The test just verifies no panic/compile error.
    assert!(summary.all_passed(), "no-annotation expect_output should pass: {:?}", summary.results[0].outcome);
}
