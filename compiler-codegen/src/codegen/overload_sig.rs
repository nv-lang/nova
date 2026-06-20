// Plan 172.1 U.6.3.a — shared overload-signature helpers.
//
// These four functions were byte-for-byte duplicated (the doc comments literally
// said "Identical to preempt_keep::…") between `may_gc.rs` and `preempt_keep.rs`,
// both of which build an over-approximate call graph and need the same source-level
// overload-disambiguation signatures. De-duplicated into one `pub(crate)` module
// (the bodies were confirmed functionally identical by a per-fn diff before the move;
// `may_gc::typeref_is_function` is a may_gc-only extra and stays there).
//
// PURELY SOURCE-SYNTACTIC: these never resolve types — they render a stable string
// from AST shape only, conservative (unknown → `?` / `None`), so the call-graph
// over-approximation stays sound. They EMIT NOTHING into the generated C.

use crate::ast::{Expr, ExprKind, Param, TypeRef};

/// Render a parameter list to a stable signature string. Unknown / generic
/// types render as `?`. Used both for node keys and for overload matching.
pub(crate) fn param_sig(params: &[Param]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(params.len());
    for p in params {
        parts.push(typeref_sig(&p.ty));
    }
    parts.join(",")
}

/// Stable string form of a source TypeRef, sufficient to distinguish
/// concrete scalar overloads (`f32` vs `f64` vs `int`). Anything we cannot
/// render to a concrete primitive name renders to `?` (conservative — two
/// `?` overloads collapse, which only adds edges → KEEP).
pub(crate) fn typeref_sig(t: &TypeRef) -> String {
    match t {
        // Strip compile-time modifier wrappers — they do not change the
        // overload identity for our purposes (`ro f32` ≡ `f32` for dispatch).
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Unsafe(inner, _) => {
            typeref_sig(inner)
        }
        TypeRef::Named { path, generics, .. } => {
            let name = path.join(".");
            if generics.is_empty() {
                name
            } else {
                // Generic application: render head + args. Concrete scalars
                // never have generics, so this never collapses the f32/f64
                // scalar overloads we care about.
                format!("{}<{}>", name, generics.iter().map(typeref_sig).collect::<Vec<_>>().join(","))
            }
        }
        TypeRef::Array(inner, _) => format!("[]{}", typeref_sig(inner)),
        TypeRef::FixedArray(n, inner, _) => format!("[{}]{}", n, typeref_sig(inner)),
        TypeRef::Tuple(elems, _) => {
            format!("({})", elems.iter().map(typeref_sig).collect::<Vec<_>>().join(","))
        }
        TypeRef::Unit(_) => "()".to_string(),
        // Pointers / funcs / protocols / anything else: not needed to
        // distinguish the scalar-forwarder overloads; collapse to `?`.
        _ => "?".to_string(),
    }
}

/// Best-effort, purely source-syntactic approximation of an expression's
/// TYPE signature, for overload disambiguation of resolved calls. Returns
/// `Some(sig)` only when source-evident (literal / `as T` cast / a call whose
/// resolved callee has a single, concrete, non-generic return type). `None`
/// means "unknown" — callers then edge to ALL candidate overloads
/// (conservative superset). NEVER wrong-narrows: an uncertain arg yields
/// `None` (KEEP-leaning), never a guessed concrete type.
pub(crate) fn approx_arg_sig_lit(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::As(_, ty) => Some(typeref_sig(ty)),
        ExprKind::IntLit(_) => Some("int".to_string()),
        ExprKind::FloatLit(_) => Some("f64".to_string()),
        ExprKind::BoolLit(_) => Some("bool".to_string()),
        ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => Some("str".to_string()),
        ExprKind::CharLit(_) => Some("char".to_string()),
        _ => None,
    }
}

/// Whether a candidate's param signature is COMPATIBLE with the approximated
/// argument signatures. `None` arg = unknown → matches anything. Differing
/// arity → no match. A known arg sig must equal the param sig (or the param
/// sig is `?` / generic, which matches anything).
pub(crate) fn overload_compatible(param_sig_str: &str, approx_args: &[Option<String>]) -> bool {
    let param_parts: Vec<&str> = if param_sig_str.is_empty() {
        Vec::new()
    } else {
        param_sig_str.split(',').collect()
    };
    if param_parts.len() != approx_args.len() {
        return false;
    }
    for (pp, aa) in param_parts.iter().zip(approx_args.iter()) {
        match aa {
            None => continue, // unknown arg → compatible (conservative)
            Some(a) => {
                if *pp == "?" || pp.contains('<') {
                    continue; // generic / unrendered param → compatible
                }
                if pp != a {
                    return false;
                }
            }
        }
    }
    true
}
