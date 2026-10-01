//! Registry 221.1 #1547: the `as` rules of D54 (spec/decisions/03-syntax.md), judged
//! by the CHECKER -- `nova check` refuses what is not a conversion, before any C.
//!
//! Before: the table of forbidden pairs lived in the emitter
//! (`check_as_cast_allowed`, keyed by C type names), so `nova check` passed every
//! cast and only `nova build` refused a few; pairs D54 has no rule for (`'A' as
//! f64`, `f as char`, `true as bool`, `s as str`) and `u8 as char` were accepted
//! outright; `unsafe { c as u8 }` was `E_UNSAFE_UNUSED` (the cast did not count as
//! the operation the block gates).
//!
//! The rule, by source and target (scalars only; a newtype, a sum, `any`, a pointer
//! target and a type parameter are other doors and pass through here untouched):
//!
//! * integer -> integer, integer <-> float, float -> float: legal (D54, amendment
//!   2026-09-04: low bits, one operation, `int as uint` included);
//! * `bool` -> integer or float: legal, 1/0 (amendment 2026-10-01 p.1);
//! * `char` value -> `i32 u32 int uint i64 u64`: legal (p.2); -> `i8 u8 i16 u16`:
//!   forbidden; a `char` LITERAL -> any integer it fits, else `E_LIT_OUT_OF_RANGE`;
//! * an integer LITERAL -> `char`: legal for a Unicode scalar (Plan 14 F.7);
//! * FORBIDDEN (the table of D54; allowed inside `unsafe { }` for a value, which
//!   then counts as the block's operation): integer value -> `char`, `char` value ->
//!   a narrow integer, a number or `char` -> `bool`, an opaque pointer `*()` -> a
//!   non-integer or a narrow integer (`E_PTR_CAST_INVALID_TARGET`);
//! * NO RULE, an error everywhere ("произвольные типы без явного правила -- ошибка"):
//!   `char` <-> float, float -> `char`, `bool` -> `bool`/`char`, `char` -> `char`,
//!   anything <-> `str` (the canon is a method on the source: `s.to_int()`,
//!   `v.to_str()`).
//!
//! Also here: unary minus is defined for SIGNED types only (spec/conversions.md);
//! `-x as u8` reads `-(x as u8)` (amendment 2026-10-01 p.3), a minus on `u8`.

use super::*;

/// Where a forbidden-table cast stands: refused at once, or only outside `unsafe`.
pub(super) enum CastVerdict {
    Legal,
    /// An error wherever it stands.
    Refused(String),
    /// Legal inside `unsafe { }` (and then the block's operation), refused outside.
    UnsafeOnly(String),
}

fn is_int(n: &str) -> bool {
    matches!(n, "int" | "uint" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64")
}
fn is_float(n: &str) -> bool {
    matches!(n, "f32" | "f64")
}
fn is_unsigned(n: &str) -> bool {
    matches!(n, "uint" | "u8" | "u16" | "u32" | "u64")
}
/// The integer targets that hold every code point (amendment 2026-10-01 p.2).
fn holds_any_char(n: &str) -> bool {
    matches!(n, "i32" | "u32" | "int" | "uint" | "i64" | "u64")
}
/// The inclusive range of an integer type (`int`/`uint` are 64-bit, D129/D130).
fn int_range(n: &str) -> (i128, i128) {
    match n {
        "i8" => (i8::MIN as i128, i8::MAX as i128),
        "i16" => (i16::MIN as i128, i16::MAX as i128),
        "i32" => (i32::MIN as i128, i32::MAX as i128),
        "u8" => (0, u8::MAX as i128),
        "u16" => (0, u16::MAX as i128),
        "u32" => (0, u32::MAX as i128),
        "u64" | "uint" => (0, u64::MAX as i128),
        _ => (i64::MIN as i128, i64::MAX as i128),
    }
}

/// The scalar name of a type, or "*()" for the opaque pointer.
fn scalar_name(t: &TypeRef) -> Option<String> {
    match t.strip_modifiers() {
        TypeRef::Named { path, generics, .. } if path.len() == 1 && generics.is_empty() => {
            let n = path[0].as_str();
            (is_int(n) || is_float(n) || matches!(n, "bool" | "char" | "str")).then(|| n.to_string())
        }
        TypeRef::Pointer(inner, _) if matches!(inner.strip_modifiers(), TypeRef::Unit(_)) => Some("*()".to_string()),
        _ => None,
    }
}

/// What the operand of `as` is, as far as D54 cares.
enum Src {
    CharLit(u32),
    IntLit(i128),
    Value(String),
}

fn forbidden(src: &str, tgt: &str, hint: &str) -> String {
    format!("[E_AS_CAST_FORBIDDEN] `as`-cast `{src} as {tgt}` is forbidden (D54): {hint}")
}
fn no_rule(src: &str, tgt: &str, hint: &str) -> String {
    format!("[E_AS_CAST_NO_RULE] `as`-cast `{src} as {tgt}` has no rule in D54 -- a cast between types without an explicit rule is an error: {hint}")
}

/// D54 for one cast. `None` source or target: not a scalar cast, not judged here.
fn cast_verdict(src: &Src, tgt: &str) -> CastVerdict {
    use CastVerdict::*;
    let (sname, lit_char, lit_int) = match src {
        Src::CharLit(c) => ("char".to_string(), Some(*c), None),
        Src::IntLit(v) => ("int".to_string(), None, Some(*v)),
        Src::Value(n) => (n.clone(), None, None),
    };
    let s = sname.as_str();
    if s == "*()" {
        return match tgt {
            "int" | "i64" | "u64" | "uint" => Legal,
            "str" | "bool" | "f32" | "f64" | "char" | "i8" | "i16" | "i32" | "u8" | "u16" | "u32" => UnsafeOnly(format!(
                "[E_PTR_CAST_INVALID_TARGET] `*() as {tgt}` is forbidden: an opaque pointer converts only to a 64-bit integer (`as u64`, `as i64`, `as int`)"
            )),
            _ => Legal,
        };
    }
    if s == "str" || tgt == "str" {
        let hint = if s == "str" { "parse with a method on the source -- `s.to_int()?`, `s.to_f64()?`, `s.to_bool()?`, `s.to_char()?`" } else { "use `v.to_str()` (a method on the source)" };
        return Refused(no_rule(s, tgt, hint));
    }
    if tgt == "bool" {
        return if s == "bool" {
            Refused(no_rule(s, tgt, "the value already is a `bool` -- drop the cast"))
        } else if is_float(s) {
            UnsafeOnly(forbidden(s, tgt, "use `f != 0.0`"))
        } else {
            UnsafeOnly(forbidden(s, tgt, "use explicit comparison (`n != 0`)"))
        };
    }
    if s == "bool" {
        return if is_int(tgt) || is_float(tgt) { Legal } else { Refused(no_rule(s, tgt, "`bool` converts to a number only (`true` -> 1)")) };
    }
    if tgt == "char" {
        if let Some(v) = lit_int {
            return if v < 0 || v > 0x10FFFF {
                Refused(format!("[E_LIT_OUT_OF_RANGE] `{v} as char`: code point 0x{v:X} is outside U+0..=U+10FFFF"))
            } else if (0xD800..=0xDFFF).contains(&v) {
                Refused(format!("[E_LIT_OUT_OF_RANGE] `{v} as char`: U+{v:04X} is a surrogate (U+D800..=U+DFFF), not a Unicode scalar value"))
            } else {
                Legal
            };
        }
        return if is_int(s) {
            UnsafeOnly(forbidden(s, tgt, "the checked form is `n.to_char()?` (Result[char, CharError]); `unsafe { n as char }` after a range check"))
        } else if s == "char" {
            Refused(no_rule(s, tgt, "the value already is a `char` -- drop the cast"))
        } else {
            Refused(no_rule(s, tgt, "a float has no code point -- convert to an integer first"))
        };
    }
    if s == "char" {
        if is_float(tgt) {
            return Refused(no_rule(s, tgt, "convert through an integer: `c as u32 as f64`"));
        }
        if !is_int(tgt) {
            return Legal;
        }
        if let Some(c) = lit_char {
            let (lo, hi) = int_range(tgt);
            let v = c as i128;
            return if v >= lo && v <= hi {
                Legal
            } else {
                Refused(format!("[E_LIT_OUT_OF_RANGE] `'\\u{{{c:X}}}' as {tgt}`: code point {c} does not fit `{tgt}` ({lo}..={hi})"))
            };
        }
        return if holds_any_char(tgt) {
            Legal
        } else {
            UnsafeOnly(forbidden(s, tgt, "a `char` value converts only to `i32`/`u32`/`int`/`uint`/`i64`/`u64`; the checked form is `(c as u32).to_u8()`"))
        };
    }
    Legal
}

impl<'a> TypeCheckCtx<'a> {
    /// D54 for `inner as cast_ty` (registry #1547). A forbidden-table cast is
    /// recorded for the unsafe pass, which refuses it outside `unsafe { }` and
    /// counts it as the block's operation inside.
    pub(super) fn check_as_cast(&self, e: &Expr, inner: &Expr, cast_ty: &TypeRef, scope: &HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        let Some(tgt) = scalar_name(cast_ty) else { return };
        let src = match &inner.kind {
            ExprKind::CharLit(c) => Src::CharLit(*c),
            ExprKind::IntLit(v) => Src::IntLit(*v as i128),
            ExprKind::Unary { op: UnOp::Neg, operand } if matches!(operand.kind, ExprKind::IntLit(_)) => {
                let ExprKind::IntLit(v) = operand.kind else { return };
                Src::IntLit(-(v as i128))
            }
            _ => match self.infer_expr_type(inner, scope).as_ref().and_then(scalar_name) {
                Some(n) => Src::Value(n),
                None => return,
            },
        };
        match cast_verdict(&src, &tgt) {
            CastVerdict::Legal => {}
            CastVerdict::Refused(msg) => errors.push(Diagnostic::new(msg, e.span)),
            CastVerdict::UnsafeOnly(msg) => {
                if e.id.is_set() {
                    self.unsafe_gated_casts.borrow_mut().insert(e.id, msg);
                } else {
                    errors.push(Diagnostic::new(msg, e.span));
                }
            }
        }
    }

    /// Unary minus on an UNSIGNED operand (spec/conversions.md: "unary minus is
    /// defined for signed types only"). `-x as u8` is `-(x as u8)` (D54 amendment
    /// 2026-10-01 p.3) and lands here; `(-x) as u8` is a minus on the signed `x`.
    pub(super) fn check_neg_unsigned(&self, e: &Expr, operand: &Expr, scope: &HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        if matches!(operand.kind, ExprKind::IntLit(_) | ExprKind::FloatLit(_)) {
            return;
        }
        let Some(n) = self.infer_expr_type(operand, scope).as_ref().and_then(scalar_name) else { return };
        if is_unsigned(&n) {
            errors.push(Diagnostic::new(
                format!("[E_NEG_UNSIGNED] unary minus on `{n}`: it is defined for signed types only (spec/conversions.md) -- `-x as {n}` reads `-(x as {n})` (D54); negate first: `(-x) as {n}`"),
                e.span,
            ));
        }
    }
}
