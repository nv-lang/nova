// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 197 — `--strict-effects` (EXPERIMENTAL, opt-in). Rust-level
//! correctness gate for the two diagnostics behind the flag:
//! `E_UNDECLARED_TRANSITIVE_EFFECT` and `E_EFFECT_ERASED_IN_FN_TYPE`. See
//! `spec/decisions/04-effects.md` D62 for the underlying (unconditional)
//! language semantics; `nova-cli/src/main.rs` for the `--strict-effects`
//! flag → `NOVA_STRICT_EFFECTS` env var plumbing;
//! `compiler-codegen/src/strict_effects.rs` for the implementation.
//!
//! The D89 `EXPECT_*` marker system (`test_runner.rs::parse_expect`) has no
//! per-file CLI-flag support, so the human-readable pos/neg `.nv` fixtures
//! under `spec_tests/strict_effects/` cannot be driven through `nova test`
//! directly — this file is the PRIMARY correctness gate instead, calling
//! `nova_codegen::types::check_module` directly on the SAME snippets (mirror
//! of the fixtures, kept inline so this file has zero I/O dependency).
//! `scripts/guards/strict_effects_smoke.sh` is a secondary CLI-level smoke test
//! against a built `nova` binary, driving the actual fixture files.
//!
//! `NOVA_STRICT_EFFECTS` is process-global state (env var), read by
//! `strict_effects::strict_effects_enabled()`. Every test here that touches
//! it goes through `check_with_flag`, which holds `ENV_LOCK` for the
//! duration — required because `cargo test` runs `#[test]` fns in this file
//! concurrently on a thread pool by default, and an unguarded env var would
//! race across threads. (This file is its own test BINARY per cargo
//! convention — `tests/*.rs` files compile to separate processes — so this
//! lock only needs to cover races WITHIN this file, not against `--lib`
//! unit tests or other integration-test files.)

use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn parse(src: &str) -> nova_codegen::ast::Module {
    nova_codegen::parser::parse(src).expect("fixture must parse")
}

/// Run `check_module` with `NOVA_STRICT_EFFECTS` set per `on`, holding
/// `ENV_LOCK` for the duration (see module doc) and restoring the env var
/// (removed — matches the CLI's "flag absent" default) afterward.
fn check_with_flag(src: &str, on: bool) -> Result<(), Vec<nova_codegen::Diagnostic>> {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if on {
        std::env::set_var("NOVA_STRICT_EFFECTS", "1");
    } else {
        std::env::remove_var("NOVA_STRICT_EFFECTS");
    }
    let module = parse(src);
    let result = nova_codegen::types::check_module(&module).map(|_| ());
    std::env::remove_var("NOVA_STRICT_EFFECTS");
    result
}

fn has_code(errs: &[nova_codegen::Diagnostic], code: &str) -> bool {
    errs.iter().any(|d| d.message.contains(code))
}

// ─── E_UNDECLARED_TRANSITIVE_EFFECT (mirrors spec_tests/strict_effects/*transitive*) ───

const TRANSITIVE_UNDECLARED_SRC: &str = r#"
module t

type Counter effect {
    next() -> int
}

fn tick() Counter -> int =>
    Counter.next()

fn caller_bad() -> int =>
    tick()
"#;

#[test]
fn transitive_undeclared_silent_without_flag() {
    // Byte-identical-behavior guarantee: this is the pre-existing, unsound
    // gap (Plan 197 brief, repro (b)) — must stay silent without the flag.
    let r = check_with_flag(TRANSITIVE_UNDECLARED_SRC, false);
    assert!(r.is_ok(), "must compile clean without --strict-effects: {:?}", r.err());
}

#[test]
fn transitive_undeclared_errors_with_flag() {
    let errs = check_with_flag(TRANSITIVE_UNDECLARED_SRC, true)
        .expect_err("must fail under --strict-effects");
    assert!(
        has_code(&errs, "E_UNDECLARED_TRANSITIVE_EFFECT"),
        "expected E_UNDECLARED_TRANSITIVE_EFFECT, got: {:?}",
        errs
    );
}

const TRANSITIVE_DECLARED_SRC: &str = r#"
module t

type Counter effect {
    next() -> int
}

fn tick() Counter -> int =>
    Counter.next()

fn caller_ok() Counter -> int =>
    tick()
"#;

#[test]
fn transitive_declared_directly_never_flagged() {
    // Positive control — must be clean in BOTH modes (no flag-dependent
    // behavior change for a fn that already declares the effect directly).
    assert!(check_with_flag(TRANSITIVE_DECLARED_SRC, false).is_ok());
    assert!(check_with_flag(TRANSITIVE_DECLARED_SRC, true).is_ok());
}

const TRANSITIVE_HANDLED_SRC: &str = r#"
module t

type Counter effect {
    next() -> int
}

fn tick() Counter -> int =>
    Counter.next()

fn caller_handled() -> int {
    with Counter = effect Counter {
        next() => 42
    } {
        tick()
    }
}
"#;

#[test]
fn transitive_lexically_handled_never_flagged() {
    // D62 "Альтернатива через with" — a lexically-enclosing `with Counter =
    // …` handler discharges the obligation locally; `caller_handled` needs
    // no `Counter` in its own signature. Positive control, both modes.
    assert!(check_with_flag(TRANSITIVE_HANDLED_SRC, false).is_ok());
    assert!(check_with_flag(TRANSITIVE_HANDLED_SRC, true).is_ok());
}

// ─── E_EFFECT_ERASED_IN_FN_TYPE (mirrors spec_tests/strict_effects/*erasure*) ───

const ERASURE_SRC: &str = r#"
module t

type Counter effect {
    next() -> int
}

fn tick() Counter -> int =>
    Counter.next()

fn bad_erasure() -> int {
    ro f fn() -> int = tick
    f()
}
"#;

#[test]
fn erasure_silent_without_flag() {
    // Byte-identical-behavior guarantee: this is the pre-existing, unsound
    // gap (Plan 197 brief, repro (a): `ro f fn() -> int = tick`) — must stay
    // silent without the flag.
    let r = check_with_flag(ERASURE_SRC, false);
    assert!(r.is_ok(), "must compile clean without --strict-effects: {:?}", r.err());
}

#[test]
fn erasure_errors_with_flag() {
    let errs = check_with_flag(ERASURE_SRC, true).expect_err("must fail under --strict-effects");
    assert!(
        has_code(&errs, "E_EFFECT_ERASED_IN_FN_TYPE"),
        "expected E_EFFECT_ERASED_IN_FN_TYPE, got: {:?}",
        errs
    );
}

const ERASURE_WIDENING_SRC: &str = r#"
module t

type Counter effect {
    next() -> int
}

fn tick() Counter -> int =>
    Counter.next()

fn good_widening() Counter -> int {
    ro f fn() Counter -> int = tick
    f()
}
"#;

#[test]
fn erasure_widening_never_flagged() {
    // Positive control — dest fn-type's effect-row COVERS the source's
    // effects (Counter ⊇ Counter): ordinary assignment, not erasure. Both
    // modes clean.
    assert!(check_with_flag(ERASURE_WIDENING_SRC, false).is_ok());
    assert!(check_with_flag(ERASURE_WIDENING_SRC, true).is_ok());
}

const ERASURE_PURE_SRC: &str = r#"
module t

fn pure_stub() -> int => 7

fn ok_no_erasure() -> int {
    ro g fn() -> int = pure_stub
    g()
}
"#;

#[test]
fn erasure_pure_into_pure_never_flagged() {
    // Positive control — empty ⊆ empty, nothing to erase. Both modes clean.
    assert!(check_with_flag(ERASURE_PURE_SRC, false).is_ok());
    assert!(check_with_flag(ERASURE_PURE_SRC, true).is_ok());
}

// ─── Fail is out of scope for E_UNDECLARED_TRANSITIVE_EFFECT (D62 §Правило 2) ───

const FAIL_TRANSITIVE_SRC: &str = r#"
module t

fn parses(s str) Fail -> int {
    throw RuntimeError.Generic("bad")
}

fn caller_no_fail() -> int =>
    parses("x")
"#;

#[test]
fn fail_transitivity_not_gated_by_strict_effects() {
    // D62 §Правило 2: `Fail` transitivity is strict and UNCONDITIONAL —
    // pre-existing, separate concern, deliberately excluded from
    // `E_UNDECLARED_TRANSITIVE_EFFECT` (see `check_transitive_effect_strict`
    // doc in types/mod.rs). This asserts our new check does not pile a
    // SECOND, redundant diagnostic on top of whatever the existing Fail
    // machinery does (parse may itself reject/accept this snippet
    // independently — we only assert our own code stays silent on `Fail`).
    if let Err(errs) = check_with_flag(FAIL_TRANSITIVE_SRC, true) {
        assert!(
            !has_code(&errs, "E_UNDECLARED_TRANSITIVE_EFFECT"),
            "E_UNDECLARED_TRANSITIVE_EFFECT must never fire for `Fail` (D62 §Правило 2 \
             is unconditional, out of this flag's scope): {:?}",
            errs
        );
    }
}
