// SPDX-License-Identifier: MIT OR Apache-2.0
//! #1870: operation environments, including the post-check rewrite fallback.
use super::*;

fn named(name: &str) -> TypeRef {
    TypeRef::Named { path: vec![name.into()], generics: vec![], span: Span::default() }
}

fn method(ty: Option<TypeRef>) -> HandlerMethod {
    HandlerMethod {
        name: "pass".into(),
        params: vec![crate::ast::HandlerMethodParam {
            name: "s".into(), ty, span: Span::default(),
        }],
        ret_ty: Some(named("int")),
        body: HandlerMethodBody::Expr(Expr::new(ExprKind::UnitLit, Span::default())),
        span: Span::default(),
    }
}

#[test]
fn typed_parameter_shadows_outer_and_sibling_locals_die() {
    let outer = HashMap::from([("s".into(), named("str")), ("state".into(), named("int"))]);
    let mut first = handler_method_scope(&outer, &method(Some(named("Signal"))));
    assert_eq!(typeref_display(first.get("s").unwrap()), "Signal");
    first.insert("s".into(), named("int"));
    first.insert("local".into(), named("int"));
    let next = handler_method_scope(&outer, &method(Some(named("Signal"))));
    assert_eq!(typeref_display(next.get("s").unwrap()), "Signal");
    assert_eq!(typeref_display(next.get("state").unwrap()), "int");
    assert!(!next.contains_key("local"));
    assert_eq!(typeref_display(outer.get("s").unwrap()), "str");
}

#[test]
fn untyped_parameter_hides_outer_type() {
    let outer = HashMap::from([("s".into(), named("int"))]);
    let local = handler_method_scope(&outer, &method(None));
    assert!(!local.contains_key("s"));
    assert!(local.contains_key(&local_shadow_key("s")));
}

#[test]
fn annotator_does_not_sum_lift_sibling_parameter_or_outer_capture() {
    // Empty channels deliberately exercise the annotator's var_types fallback.
    // A real int local must still lift; sibling Signal values must not.
    let mut module = crate::parser::parse(r#"module t
type Signal enum Kill | Other(int)
type Probe effect {
    fn seed() -> int
    fn pass(s Signal) -> int
    fn captured() -> int
}
fn take(s Signal) -> int => 1
fn make(s Signal) -> Effect[Probe] => effect Probe {
    seed() -> int {
        mut s = 0
        return take(s)
    }
    pass(s Signal) -> int => take(s)
    captured() -> int => take(s)
}
"#).unwrap();
    annotate_map_literals(&mut module, &ModuleEnv::default());
    let make = module.items.iter().find_map(|i| match i {
        Item::Fn(f) if f.name == "make" => Some(f), _ => None,
    }).unwrap();
    let FnBody::Expr(handler) = &make.body else { panic!("handler expression") };
    let ExprKind::HandlerLit { methods, .. } = &handler.kind else { panic!("handler literal") };
    let HandlerMethodBody::Block(seed) = &methods[0].body else { panic!("seed block") };
    let Stmt::Return { value: Some(value), .. } = &seed.stmts[1] else { panic!("seed return") };
    let ExprKind::Call { args, .. } = &value.kind else { panic!("seed call") };
    assert!(matches!(args[0].expr().kind, ExprKind::Call { .. }), "lawful int sum-lift retained");
    for method in &methods[1..] {
        let HandlerMethodBody::Expr(call) = &method.body else { panic!("expression body") };
        let ExprKind::Call { args, .. } = &call.kind else { panic!("call") };
        assert!(matches!(&args[0].expr().kind, ExprKind::Ident(n) if n == "s"),
            "{} must pass the Signal unchanged", method.name);
    }
}
