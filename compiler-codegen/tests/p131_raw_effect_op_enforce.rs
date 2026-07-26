// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 221.1 №131 (D62 ENFORCED) — Rust-level correctness gate for the raw
//! effect-op enforcement: `Effect.op(...)` called DIRECTLY in a body (no
//! intervening named fn) is a "direct effect" (D28 §Правило вывода п.1),
//! mandatory in `export fn` signatures — `[E_RAW_EFFECT_OP_UNDECLARED]`
//! (`types/mod.rs` `check_raw_effect_op_declared`, called from
//! `check_capabilities_at`'s "1. Effect-op call" branch). UNLIKE
//! `E_UNDECLARED_TRANSITIVE_EFFECT` (Plan 197, see `strict_effects_flag.rs`),
//! this check is UNCONDITIONAL — not gated behind `--strict-effects` (same
//! footing as `E_BANG_REQUIRES_FAIL`, №113). Private fn instead get the
//! effect silently auto-added by `infer_effects` (D28) —
//! `collect_raw_effect_ops_in_fn` / `raw_effect_op_head`, mirroring the
//! pre-existing `Fail`-on-throw push.
//!
//! Companion `.nv` fixtures: `spec_tests/conformance/d62_raw_effect_op_*`.

fn parse(src: &str) -> nova_codegen::ast::Module {
    nova_codegen::parser::parse(src).expect("fixture must parse")
}

fn check(src: &str) -> Result<(), Vec<nova_codegen::Diagnostic>> {
    let module = parse(src);
    nova_codegen::types::check_module(&module).map(|_| ())
}

fn has_code(errs: &[nova_codegen::Diagnostic], code: &str) -> bool {
    errs.iter().any(|d| d.message.contains(code))
}

// ─── export fn, raw op, undeclared — must fail unconditionally ───

const EXPORT_UNDECLARED_SRC: &str = r#"
module t

type Log1 effect {
    info(msg str) -> ()
}

export fn bad(s str) -> int {
    Log1.info(s)
    1
}
"#;

#[test]
fn export_raw_op_undeclared_errors() {
    let errs = check(EXPORT_UNDECLARED_SRC).expect_err("must fail — raw op undeclared in export fn");
    assert!(
        has_code(&errs, "E_RAW_EFFECT_OP_UNDECLARED"),
        "expected E_RAW_EFFECT_OP_UNDECLARED, got: {:?}",
        errs
    );
}

// ─── export fn, raw op, declared directly — clean ───

const EXPORT_DECLARED_SRC: &str = r#"
module t

type Log1 effect {
    info(msg str) -> ()
}

export fn ok(s str) Log1 -> int {
    Log1.info(s)
    1
}
"#;

#[test]
fn export_raw_op_declared_never_flagged() {
    assert!(check(EXPORT_DECLARED_SRC).is_ok(), "{:?}", check(EXPORT_DECLARED_SRC).err());
}

// ─── export fn, raw op inside a LOCAL `with` — discharged, no sig needed ───

const EXPORT_WITH_SCOPE_SRC: &str = r#"
module t

type Log1 effect {
    info(msg str) -> ()
}

export fn ok(s str) -> int {
    with Log1 = effect Log1 { info(m) -> () => () } {
        Log1.info(s)
    }
    1
}
"#;

#[test]
fn export_raw_op_in_with_scope_never_flagged() {
    assert!(check(EXPORT_WITH_SCOPE_SRC).is_ok(), "{:?}", check(EXPORT_WITH_SCOPE_SRC).err());
}

// ─── private fn, raw op, undeclared — exempt from the hard-error gate
//     (D28 auto-inference territory, NOT this check's job) ───

const PRIVATE_UNDECLARED_SRC: &str = r#"
module t

type Log1 effect {
    info(msg str) -> ()
}

fn helper(s str) -> int {
    Log1.info(s)
    1
}
"#;

#[test]
fn private_raw_op_undeclared_never_hard_errors() {
    // check_module alone (no infer_effects) — mirrors the `is_export` gate:
    // private fn must NEVER trip [E_RAW_EFFECT_OP_UNDECLARED].
    if let Err(errs) = check(PRIVATE_UNDECLARED_SRC) {
        assert!(
            !has_code(&errs, "E_RAW_EFFECT_OP_UNDECLARED"),
            "private fn must be exempt from the export-only hard-error gate: {:?}",
            errs
        );
    }
}

// ─── D28 auto-inference: infer_effects silently adds the raw-used effect
//     to a PRIVATE fn's signature (mirrors the pre-existing Fail-on-throw
//     push) ───

#[test]
fn private_raw_op_gets_auto_inferred() {
    let mut module = parse(PRIVATE_UNDECLARED_SRC);
    nova_codegen::types::infer_effects(&mut module);
    let f = module.items.iter().find_map(|it| match it {
        nova_codegen::ast::Item::Fn(f) if f.name == "helper" => Some(f),
        _ => None,
    }).expect("helper must exist");
    let has_log1 = f.effects.iter().any(|e| matches!(
        e, nova_codegen::ast::TypeRef::Named { path, .. } if path.last().map(String::as_str) == Some("Log1")
    ));
    assert!(has_log1, "infer_effects must auto-add `Log1` to a private fn's raw-op-using signature; got effects: {:?}", f.effects);
}

// ─── export fn already declares Fail — Fail itself is never mistaken for
//     a "registered effect type" by this raw-op check (defensive; `Fail`
//     has no `type Fail effect {...}` decl, `effect_decls.contains_key`
//     naturally excludes it — this just documents/pins the intent) ───

const EXPORT_NO_EFFECT_TYPE_COLLISION_SRC: &str = r#"
module t

type Log1 effect {
    info(msg str) -> ()
}

export fn ok(x int) -> int {
    x + 1
}
"#;

#[test]
fn export_pure_fn_never_flagged() {
    assert!(check(EXPORT_NO_EFFECT_TYPE_COLLISION_SRC).is_ok());
}

// ─── handler-literal op body (`effect X { op() => ... }`) — a raw op of a
//     DIFFERENT effect used inside the handler body is NOT this construct's
//     obligation (no enclosing fn-signature to check against); it belongs
//     to the LEXICAL with-establishment context (Ф-D-class reasoning,
//     opus design-note 222.20-Ф.2 probe p_effect_in_effect: "Time из
//     лексического окружения main"). Must stay clean regardless of whether
//     the enclosing scope declares the inner effect. ───

const HANDLER_LIT_INNER_RAW_OP_SRC: &str = r#"
module t

type Aeff effect {
    a_op() -> int
}

type Beff effect {
    b_op() -> int
}

export fn ok() -> int {
    with Beff = effect Beff { b_op() -> int => 7 } {
        with Aeff = effect Aeff { a_op() -> int => Beff.b_op() } {
            Aeff.a_op()
        }
    }
}
"#;

#[test]
fn raw_op_inside_handler_literal_body_not_hard_gated() {
    assert!(
        check(HANDLER_LIT_INNER_RAW_OP_SRC).is_ok(),
        "{:?}", check(HANDLER_LIT_INNER_RAW_OP_SRC).err()
    );
}
