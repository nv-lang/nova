//! Registry 221.1 #1611 (D491): a value of a numeric type does not change its KIND
//! (integer <-> float) implicitly, and a float value does not narrow (`f64` -> `f32`).
//!
//! Before: `cat_compatible_rt` holds `Scalar` and `Float` compatible both ways -- it is the
//! category test that also serves literals, generic matching and inference fallbacks -- and
//! the only value check after it was `would_narrow_into`, which knows integers only. So with
//! `d f64` the program `ro n int = d` checked green and the C compiler truncated `2.5` to `2`
//! in every position: declaration, argument, return, field, element (`push`), channel send,
//! `x = y`. `int` went into `f64` the same way, and `f64` into `f32` lost precision -- all
//! without a word.
//!
//! The judgement is on the DIRECT types (`from_type_ref`, no alias resolve); it feeds the one
//! `Compat::Narrowing` verdict, so every position refuses it, and `numeric_change_why` is the
//! one text of the reason all of them print.
//!
//! Registry 221.1 #1608 (D491, owner's decision 2026-10-02) widened the rule to every change
//! of numeric type: a widening inside a kind (`u8` -> `int`, `f32` -> `f64`) and `int` <->
//! `i64` are refused like a narrowing; `value_type_changes` is the one predicate.

use super::ResolvedType;

/// D491: does a NON-literal value of `from` change its numeric TYPE going into a position
/// of `to`? Any change: of kind (integer <-> float, #1611), of float width (`f32` <-> `f64`),
/// of integer type (`changes_int_type_into`, #1608). Literals never reach here: their rule
/// is D489 (`literal_exact.rs`).
pub(super) fn value_type_changes(from: &ResolvedType, to: &ResolvedType) -> bool {
    match (from.peel_view(), to.peel_view()) {
        (ResolvedType::Scalar { .. }, ResolvedType::Float { .. })
        | (ResolvedType::Float { .. }, ResolvedType::Scalar { .. }) => true,
        (ResolvedType::Float { width: f }, ResolvedType::Float { width: t }) => t != f,
        _ => from.changes_int_type_into(to),
    }
}

/// D489 + #1608: an expression built only of numeric literals and arithmetic -- `60 *
/// 1_000_000_000`, `-(2 * 3_600)` -- has no type of its own, like the literals it is made of:
/// it takes the type of the other operand or of its position. Before D491 nothing asked,
/// because every integer type mixed with every other; the new operator and arm checks would
/// otherwise refuse `abs < 60 * 1_000_000_000` with `abs i64` (measured: about twenty places
/// in std/src/time).
pub(super) fn is_const_number_expr(e: &crate::ast::Expr) -> bool {
    use crate::ast::{BinOp as B, ExprKind as K, UnOp};
    match &e.kind {
        K::IntLit(_) | K::FloatLit(_) => true,
        K::Unary { op: UnOp::Neg, operand } => is_const_number_expr(operand),
        K::Binary { op, left, right } => {
            matches!(op, B::Add | B::Sub | B::Mul | B::Div | B::Mod
                | B::BitAnd | B::BitOr | B::BitXor | B::Shl | B::Shr)
                && is_const_number_expr(left)
                && is_const_number_expr(right)
        }
        _ => false,
    }
}

/// The reason clause of every `E_IMPLICIT_NARROWING` text: one rule, one sentence (D491).
pub(super) fn numeric_change_why(_from: &str, _to: &str) -> &'static str {
    "a value does not change its numeric type implicitly (D491)"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(width: u8, signed: bool) -> ResolvedType {
        ResolvedType::Scalar { width, signed, wide_default: false }
    }

    #[test]
    fn every_numeric_change_is_refused() {
        let (f32t, f64t) = (ResolvedType::Float { width: 32 }, ResolvedType::Float { width: 64 });
        assert!(value_type_changes(&f64t, &s(64, true))); // f64 -> i64 (#1611)
        assert!(value_type_changes(&s(64, true), &f64t)); // i64 -> f64 (#1611)
        assert!(value_type_changes(&f64t, &f32t)); // f64 -> f32 (#1611)
        assert!(value_type_changes(&f32t, &f64t)); // f32 -> f64: widening, #1608
        assert!(value_type_changes(&s(8, false), &s(64, true))); // u8 -> i64, #1608
        assert!(!value_type_changes(&f64t, &f64t));
        assert!(!value_type_changes(&s(8, false), &s(8, false)));
        assert!(!value_type_changes(&ResolvedType::Str, &f64t));
    }

    #[test]
    fn the_reason_names_the_rule() {
        assert!(numeric_change_why("u8", "int").contains("D491"));
    }
}
