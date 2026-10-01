//! Registry 221.1 #1535: a LITERAL pattern has the type of the scrutinee.
//!
//! Before: the checker took a literal pattern as it came and left the comparison to
//! the emitter. `'a'` over a `str` and `"a"` over an `int` reached clang
//! (`scr == 97LL`, `scr.len == ...`); `'a'` over an `int` and `97` over a `char`
//! compiled and matched by the code point -- a `char` and an `int` mixed silently,
//! which an expression refuses (D128: `char` is distinct from `int`; D405: no
//! implicit conversion between named types). The integrator's ruling on the
//! question of `p274-carina-strarm` (2026-10-01): the literal of a pattern has the
//! scrutinee's type, and a `char`/`int` mix in a pattern is refused as in an
//! expression. A literal pattern is a comparison by value (`syntax.ru.md`, the
//! pattern table), so it passes the type door a literal in an expression passes.
//!
//! Judged in `match` arms (an `if <pattern> = <expr>` takes no literal pattern):
//! the pattern itself, each alternative of `p | q`, the inner pattern of a
//! binding `x @ p` -- against a scalar scrutinee (an integer, a float, `bool`,
//! `char`, `str`). An integer literal stands in a float position, as in an
//! expression (D44). A literal nested in a variant, a tuple or a record is judged
//! by that pattern's own door, not here.

use super::*;

/// The scalar a scrutinee type is, for this rule.
fn scalar_of(t: &TypeRef) -> Option<&str> {
    match t.strip_modifiers() {
        TypeRef::Named { path, generics, .. } if path.len() == 1 && generics.is_empty() => {
            let n = path[0].as_str();
            matches!(n, "int" | "uint" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
                | "f32" | "f64" | "bool" | "char" | "str")
                .then_some(n)
        }
        _ => None,
    }
}

fn is_float(n: &str) -> bool {
    matches!(n, "f32" | "f64")
}

/// May this literal stand as a pattern over a scrutinee of scalar `s`?
fn literal_fits(l: &Literal, s: &str) -> bool {
    match l {
        Literal::Int(_) => !matches!(s, "bool" | "char" | "str"),
        Literal::Float(_) => is_float(s),
        Literal::Bool(_) => s == "bool",
        Literal::Char(_) => s == "char",
        Literal::Str(_) => s == "str",
        Literal::Unit => true,
    }
}

/// The literal as written and what it is, for the message.
fn literal_shown(l: &Literal) -> (String, &'static str) {
    match l {
        Literal::Int(v) => (v.to_string(), "an integer"),
        Literal::Float(v) => (v.to_string(), "a float"),
        Literal::Bool(b) => (b.to_string(), "a `bool`"),
        Literal::Char(c) => (
            char::from_u32(*c).map(|ch| format!("'{ch}'")).unwrap_or_else(|| format!("'\\u{{{c:X}}}'")),
            "a `char`",
        ),
        Literal::Str(s) => (format!("\"{s}\""), "a `str`"),
        Literal::Unit => ("()".to_string(), "the unit"),
    }
}

/// #1593 (D55, amendment 2026-10-02): a literal of the right kind still has to hold its
/// exact value in the scrutinee's type -- the same rule as every other typed position
/// (`literal_exact.rs`): an integer over a float only if exact, a fraction over `f32`
/// not past infinity, an integer over a sized integer within its range.
fn literal_value_fault(l: &Literal, s: &str) -> Option<String> {
    match l {
        Literal::Int(v) if is_float(s) => {
            super::literal_exact::int_literal_into_float(*v as i128, if s == "f32" { 32 } else { 64 })
        }
        Literal::Float(f) if is_float(s) => {
            super::literal_exact::float_literal_into_float(*f, if s == "f32" { 32 } else { 64 })
        }
        Literal::Int(v) => {
            if s == "uint" && *v < 0 {
                return Some(format!("[E_LIT_OUT_OF_RANGE] {v} < uint.MIN (0)"));
            }
            // A hex literal past i64::MAX is stored wrapped; an unsigned target reads it back.
            let v128 = if matches!(s, "u8" | "u16" | "u32" | "u64") && *v < 0 { (*v as u64) as i128 } else { *v as i128 };
            super::lit_range_check(v128, s).map(|m| format!("[E_LIT_OUT_OF_RANGE] {m}"))
        }
        _ => None,
    }
}

impl<'a> TypeCheckCtx<'a> {
    /// #1535: every literal of `pattern` reachable without a structural pattern
    /// (itself, `|` alternatives, the inner pattern of `x @ p`) has the type of the
    /// scalar scrutinee.
    pub(super) fn check_pattern_literal_type(&self, pattern: &Pattern, scrut: Option<&TypeRef>, errors: &mut Vec<Diagnostic>) {
        let Some(s) = scrut.and_then(scalar_of) else { return };
        self.pattern_literal_walk(pattern, s, errors);
    }

    fn pattern_literal_walk(&self, pattern: &Pattern, s: &str, errors: &mut Vec<Diagnostic>) {
        match pattern {
            Pattern::Literal(l, span) => {
                if !literal_fits(l, s) {
                    let (text, what) = literal_shown(l);
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_PATTERN_LITERAL_TYPE] the pattern `{text}` is {what} literal, the scrutinee is `{s}` -- a literal pattern has the scrutinee's type, and `char`, integers, floats, `bool` and `str` do not mix in a pattern as they do not in an expression (D128, D405)"
                        ),
                        *span,
                    ));
                } else if let Some(msg) = literal_value_fault(l, s) {
                    errors.push(Diagnostic::new(msg, *span));
                }
            }
            Pattern::Or { alternatives, .. } => {
                for p in alternatives {
                    self.pattern_literal_walk(p, s, errors);
                }
            }
            Pattern::Binding { inner, .. } => self.pattern_literal_walk(inner, s, errors),
            _ => {}
        }
    }
}
