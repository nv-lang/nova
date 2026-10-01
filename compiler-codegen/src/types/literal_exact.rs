//! Registry 221.1 #1593: a literal takes the type of its position only if its EXACT value
//! is representable there (D55, amendment 2026-10-02, owner's decision).
//!
//! Before: an integer literal in a float position was accepted whatever its size --
//! `ro a f32 = 16777217` held `16777216`, `ro a f64 = 9007199254740993` held `...992`, and
//! a float literal past `f32.MAX` became `inf` -- all without a word. The written number
//! was silently replaced by another one.
//!
//! The kind of a literal is set by its FORM (D55 table): an integer literal goes into a
//! float type only if the float holds it exactly, else `E_LIT_INEXACT`; a decimal fraction
//! (a point or an exponent) rounds to the nearest float -- that is the nature of a decimal
//! fraction -- and only overflowing to infinity is an error, `E_LIT_OUT_OF_RANGE`. The rule
//! holds in every position that knows the type; the callers here are the `assignable` arms
//! for literals (declaration, argument, return, field, element, constant, payload) and the
//! pattern-literal rule (`pattern_literal_rules.rs`), and the literal operand of a comparison
//! or an arithmetic operator against a typed operand (`check_literal_operand` below):
//! `x == 300` with `x u8` is `E_LIT_OUT_OF_RANGE`, not a comparison that is always false.

/// `Some(diagnostic)` when the integer literal `val` is not exactly representable in the
/// float of `width` bits (32 or 64); the text carries its own code.
pub(super) fn int_literal_into_float(val: i128, width: u8) -> Option<String> {
    let (back, ty, near) = if width == 32 {
        let f = val as f32;
        (f as i128, "f32", format!("{}", f as f64))
    } else {
        let f = val as f64;
        (f as i128, "f64", format!("{f}"))
    };
    (back != val).then(|| {
        format!(
            "[E_LIT_INEXACT] the integer literal {val} is not exactly representable in `{ty}` \
             (it would become {near}) -- a literal takes the type of its position only if its \
             exact value fits (D55, amendment 2026-10-02); write the fraction form `{val}.0` if \
             rounding is meant, or use a wider type"
        )
    })
}

/// `Some(diagnostic)` when the float literal `f` overflows the float of `width` bits to an
/// infinity (rounding a decimal fraction is legal; overflow is not).
pub(super) fn float_literal_into_float(f: f64, width: u8) -> Option<String> {
    let (inf, ty, max) = if width == 32 {
        ((f as f32).is_infinite(), "f32", format!("{:e}", f32::MAX))
    } else {
        (f.is_infinite(), "f64", format!("{:e}", f64::MAX))
    };
    inf.then(|| {
        format!(
            "[E_LIT_OUT_OF_RANGE] the float literal {f:e} overflows `{ty}` to infinity \
             (|{ty}| <= {max}) -- D55, amendment 2026-10-02"
        )
    })
}

/// The diagnostic text of a `Compat::OutOfRange` message: the integer-range messages are
/// bare suffixes (`300 > u8.MAX (255)`), the float ones above carry their own code.
pub(super) fn literal_diag(msg: &str) -> String {
    if msg.starts_with("[E_") { msg.to_string() } else { format!("[E_LIT_OUT_OF_RANGE] {msg}") }
}

use super::*;

/// A literal that takes its type from the other operand: an integer or a float literal,
/// possibly negated.
fn is_number_literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) => true,
        ExprKind::Unary { op: UnOp::Neg, operand } => matches!(operand.kind, ExprKind::IntLit(_) | ExprKind::FloatLit(_)),
        _ => false,
    }
}

impl<'a> TypeCheckCtx<'a> {
    /// #1593: the second operand of a comparison or an arithmetic operator is a typed
    /// position too (D55, amendment 2026-10-02): a literal there takes the other operand's
    /// type only if its exact value fits -- judged by the same `assignable` arms as an
    /// annotated declaration. Only the out-of-range / inexact verdict is reported here;
    /// a kind mismatch (`x == 'a'` with `x u8`) stays with the existing type checks.
    pub(super) fn check_literal_operand(
        &self,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        use BinOp::*;
        if !matches!(op, Add | Sub | Mul | Div | Mod | Eq | Neq | Lt | Le | Gt | Ge) {
            return;
        }
        for (lit, other) in [(left, right), (right, left)] {
            if !is_number_literal(lit) || is_number_literal(other) {
                continue;
            }
            let Some(ty) = self.infer_expr_type(other, scope) else { continue };
            let rt = ResolvedType::from_type_ref(&ty);
            if !matches!(rt, ResolvedType::Scalar { .. } | ResolvedType::Float { .. }) {
                continue;
            }
            // The best-effort inference can be wrong (a binding of a match whose arms widen
            // `u32` and an `int` sentinel to `int`, D129: inference reads the first arm and
            // says `u32`). Judge only when the channel -- the type codegen reads -- says the
            // same type; an operand the channel does not know is not judged.
            let agreed = other.id.is_set()
                && self.resolved_types_buf.borrow().get(&other.id).is_some_and(|ch| *ch == rt);
            if !agreed {
                continue;
            }
            if let Compat::OutOfRange { msg } = self.assignable(lit, &ty, gs, gs, scope) {
                errors.push(Diagnostic::new(literal_diag(&msg), lit.span));
            }
        }
    }
}
