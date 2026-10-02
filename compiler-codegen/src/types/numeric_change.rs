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
//! The judgement is on the DIRECT types (`from_type_ref`, no alias resolve), the footing of
//! `would_narrow_into`; both feed the one `Compat::Narrowing` verdict, so every position that
//! prints integer narrowing refuses this too, and `numeric_change_why` is the one text of the
//! reason all of them print. Widening inside a kind (`u8` -> `int`, `f32` -> `f64`) is also
//! D491's, registry 221.1 #1608 -- not this rule.

use super::ResolvedType;

/// Does a NON-literal value of `from` change its numeric kind, or narrow as a float, going
/// into a position of `to`? Literals never reach here: their rule is D489 (`literal_exact.rs`).
pub(super) fn float_change_refused(from: &ResolvedType, to: &ResolvedType) -> bool {
    match (from.peel_view(), to.peel_view()) {
        (ResolvedType::Scalar { .. }, ResolvedType::Float { .. })
        | (ResolvedType::Float { .. }, ResolvedType::Scalar { .. }) => true,
        (ResolvedType::Float { width: f }, ResolvedType::Float { width: t }) => t < f,
        _ => false,
    }
}

/// The reason clause of every `E_IMPLICIT_NARROWING` text, chosen by the displayed type
/// names of the value (`from`) and the position (`to`).
pub(super) fn numeric_change_why(from: &str, to: &str) -> &'static str {
    // A displayed type may carry its view (`ro f64`); the type name is the last word.
    let is_float = |s: &str| matches!(s.rsplit(' ').next(), Some("f32" | "f64"));
    match (is_float(from), is_float(to)) {
        (true, false) | (false, true) => {
            "a value does not change its numeric kind implicitly, integer and float alike (D491)"
        }
        (true, true) => "implicit float narrowing loses precision (D491)",
        (false, false) => "implicit int narrowing loses range (D54)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(width: u8, signed: bool) -> ResolvedType {
        ResolvedType::Scalar { width, signed, wide_default: false }
    }

    #[test]
    fn kind_change_and_float_narrowing_are_refused() {
        let (f32t, f64t) = (ResolvedType::Float { width: 32 }, ResolvedType::Float { width: 64 });
        assert!(float_change_refused(&f64t, &s(64, true))); // f64 -> int
        assert!(float_change_refused(&s(64, true), &f64t)); // int -> f64
        assert!(float_change_refused(&s(8, false), &f32t)); // u8 -> f32
        assert!(float_change_refused(&f64t, &f32t)); // f64 -> f32
        assert!(!float_change_refused(&f32t, &f64t)); // widening: #1608, not here
        assert!(!float_change_refused(&f64t, &f64t));
        assert!(!float_change_refused(&s(8, false), &s(64, true))); // int widening: #1608
        assert!(!float_change_refused(&ResolvedType::Str, &f64t));
    }

    #[test]
    fn the_reason_names_the_change() {
        assert!(numeric_change_why("f64", "int").contains("numeric kind"));
        assert!(numeric_change_why("int", "ro f64").contains("numeric kind"));
        assert!(numeric_change_why("f64", "f32").contains("float narrowing"));
        assert!(numeric_change_why("int", "u8").contains("int narrowing"));
    }
}
