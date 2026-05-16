//! Plan 45 Ф.27.2 — render_expr coverage tests.
//!
//! Verify что contract expressions с different ExprKind variants
//! рендерятся правильно (не `...` placeholder для common cases).

use nova_codegen::doc;
use nova_codegen::parser;
use nova_codegen::types;

fn extract_contract_exprs(src: &str, fn_name: &str) -> Vec<String> {
    let mut module = parser::parse(src).expect("parse");
    let _ = types::check_module(&module);
    types::infer_effects(&mut module);
    let tree = doc::build(&module);
    for m in &tree.modules {
        for it in &m.items {
            if it.name == fn_name {
                if let doc::ItemKind::Fn(sig) = &it.kind {
                    return sig.contracts.iter().map(|c| c.expr.clone()).collect();
                }
            }
        }
    }
    Vec::new()
}

#[test]
fn binary_expression_in_requires() {
    let src = r#"
module x

/// Positive only.
export fn pos(x int) -> int
    requires x > 0
    => x
"#;
    let exprs = extract_contract_exprs(src, "pos");
    assert!(exprs.iter().any(|e| e == "x > 0"),
        "expected `x > 0` exactly, got: {:?}", exprs);
}

#[test]
fn arithmetic_in_ensures() {
    let src = r#"
module x

/// Increment.
export fn inc(x int) -> int
    ensures result == x + 1
    => x + 1
"#;
    let exprs = extract_contract_exprs(src, "inc");
    assert!(exprs.iter().any(|e| e == "result == x + 1"),
        "expected `result == x + 1`, got: {:?}", exprs);
}

#[test]
fn function_call_in_requires() {
    let src = r#"
module x

export fn pos(x int) -> bool => x > 0

export fn safe(y int) -> int
    requires pos(y)
    => y
"#;
    let exprs = extract_contract_exprs(src, "safe");
    assert!(exprs.iter().any(|e| e == "pos(y)"),
        "expected `pos(y)`, got: {:?}", exprs);
}

#[test]
fn complex_combined_predicate() {
    let src = r#"
module x

/// Combined precondition.
export fn check(a int, b int) -> int
    requires a > 0 && b < 100
    => a + b
"#;
    let exprs = extract_contract_exprs(src, "check");
    let combined = exprs.iter().find(|e| e.contains("&&"))
        .expect("expected expression with &&");
    // Должны видеть ОБА side, не `...`.
    assert!(combined.contains("a > 0"),
        "left side `a > 0` отсутствует: {}", combined);
    assert!(combined.contains("b < 100"),
        "right side `b < 100` отсутствует: {}", combined);
}

#[test]
fn negation_in_predicate() {
    let src = r#"
module x

export fn pos(x int) -> bool => x > 0

export fn neg_check(y int) -> int
    requires !pos(y)
    => y
"#;
    let exprs = extract_contract_exprs(src, "neg_check");
    assert!(exprs.iter().any(|e| e.contains("!pos(y)") || e.contains("! pos(y)")),
        "expected `!pos(y)`, got: {:?}", exprs);
}

#[test]
fn no_dot_dot_dot_in_simple_contracts() {
    // Regression: simple expressions не должны fall'ить в `...` placeholder.
    let src = r#"
module x

export fn f(a int, b int) -> int
    requires a >= 0
    requires b <= 100
    ensures result == a * b
    => a * b
"#;
    let exprs = extract_contract_exprs(src, "f");
    for e in &exprs {
        assert!(!e.contains("..."), "contract expression имеет `...` placeholder: {}", e);
    }
    assert_eq!(exprs.len(), 3, "expected 3 contracts (2 requires + 1 ensures)");
}
