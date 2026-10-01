//! Registry 221.1 #1473 / D216 amendment 2026-09-20, PART 1: a raw-pointer
//! ADDRESS-ARITHMETIC operation outside `unsafe { }` is `E_UNSAFE_REQUIRED`.
//!
//! The amendment made ACCESS through a valid pointer safe (`p.read()`,
//! `p.write(v)`, `p*.field`) and kept `unsafe` exactly where the type stops
//! answering: «адресная арифметика (`read_at`, `write_at`, `offset`, `dist`,
//! `copy_from`/`copy_to`, невыровненный и volatile доступ)»; `view_at` is
//! address arithmetic too («живёт там же, где `read_at`», D216 амендмент
//! 2026-09-25 п. 3), and so are the indexed consume forms.
//!
//! Before this module the oracle only COUNTED these names for
//! `E_UNSAFE_UNUSED` (the D216 §21 «known gap»): `s.ptr().read_at(0)` outside
//! `unsafe` passed `check`, built and printed — while the Carina window
//! refused it by the spec (novac/fixtures/unsafe_block/neg_2.nv sat in
//! novac/divergences.allow).
//!
//! The judgement is by the RECEIVER'S TYPE from the checker's channel, not by
//! the method name: `read_at`/`offset`/`view_at` are also ordinary method
//! names on other types, and a name-only rule would demand `unsafe` there.
//! An unrecorded receiver type is not flagged (no false positive); the
//! fixtures pin the shapes that must be recorded.

use super::ResolvedType;
use crate::ast::ExprId;
use crate::diag::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};

/// Methods of the `*T` family that are address arithmetic (D216 part 1).
/// Access (`read`, `write`, `read_consume`, `write_consume`, `view`) is NOT here.
pub(super) fn is_address_arithmetic(name: &str) -> bool {
    matches!(
        name,
        "read_at" | "write_at"
            | "read_consume_at" | "write_consume_at"
            | "view_at"
            | "offset" | "dist"
            | "copy_from" | "copy_from_nonoverlapping"
            | "copy_to" | "copy_to_nonoverlapping"
            | "read_unaligned" | "write_unaligned"
            | "read_volatile" | "write_volatile"
    )
}

fn is_raw_pointer(t: &ResolvedType) -> bool {
    match t {
        ResolvedType::TypedPtr(..) | ResolvedType::Ptr => true,
        ResolvedType::Readonly(inner) => is_raw_pointer(inner),
        _ => false,
    }
}

/// Ids of the expressions the checker typed as a raw pointer — handed to the
/// unsafe-context walk, which has no type inference of its own.
pub(super) fn pointer_typed_exprs(types: &HashMap<ExprId, ResolvedType>) -> HashSet<ExprId> {
    types.iter().filter(|(_, t)| is_raw_pointer(t)).map(|(id, _)| *id).collect()
}

pub(super) fn required(method: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        format!(
            "[E_UNSAFE_REQUIRED] raw-pointer `.{}()` is address arithmetic: it requires an \
             `unsafe {{ }}` block (D216 amendment 2026-09-20, part 1 -- access through a valid \
             pointer is safe, arithmetic on its address is not). Wrap the call in \
             `unsafe {{ ... }}` or mark the enclosing fn `#unsafe`.",
            method
        ),
        span,
    )
}
