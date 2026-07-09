// Plan 173.3 (D415): data-race-freedom — `#share` type-attribute predicate.
//
// This module computes whether a TYPE is safe to alias across a fiber
// boundary ("share"). It is deliberately NOT a protocol (D415 §0 — an
// empty-marker protocol would be trivially satisfied by every type, so the
// existing protocol machinery — `#impl(P)` opt-in, `verify_impl_protocols`
// — is not reused here). It mirrors the STRUCTURE of Plan 126 auto-derive
// (`protocols/auto_derive.rs`: memberwise recursive eligibility) without
// being routed through it.
//
// **The rule (hardcoded — but names ZERO types, D415 §0 / criterion 3):**
// 1. `#share`-attributed type → `true` (audited vouch — author asserts real
//    internal synchronization the auto-inference can't see; only escape).
// 2. Poison base — a raw pointer type (`*T`, any pointee modifier) → `false`.
//    Any type transitively embedding one (without its own `#share` vouch)
//    is not share. This subsumes Plan 118.3's `E_POINTER_CROSS_FIBER`.
// 3. Primitive scalars / `()` → `true` (by-value, no aliasable interior).
// 4. Record / NamedTuple → `true` iff EVERY field is share (memberwise).
// 5. Sum → `true` iff every variant's payload elements are share.
// 6. Array / FixedArray / Tuple → `true` iff the element type(s) are share.
// 7. Opaque / Newtype / Effect / Protocol / TypeSet with NO `#share` vouch
//    and no fields to recurse into → `false` (unknown internals — can't
//    prove share; matches "opaque without vouch ⇒ not share").
// 8. Unresolved / generic type-parameter names → `false` (V1 conservative;
//    no share-bound propagation through generics yet — see D415 §7 Q3/Q7).
// 9. `Func` (closure/fn-pointer type) → `false` (may capture non-share state).

use crate::ast::{TypeDecl, TypeDeclKind, TypeRef};
use std::collections::HashSet;

/// Query interface needed by the share predicate — separates the pure rule
/// from `TypeCheckCtx`/`CapabilityCtx` wiring (unit-testable in isolation,
/// mirrors `auto_derive::DeriveQuery`).
pub trait ShareQuery {
    /// Lookup a type declaration by (bare) name. `None` — unknown / unresolved
    /// (generic type-param, or a name the registry doesn't carry — conservative
    /// `false`, see rule 8).
    fn lookup_type(&self, name: &str) -> Option<&TypeDecl>;
}

const NOVA_PRIMITIVES: &[&str] = crate::protocols::auto_derive::NOVA_PRIMITIVES;

/// Top-level entry point: is `ty` share (safe to alias across a fiber
/// boundary)? `visited` guards against runaway recursion on a (structurally
/// impossible, but defensively guarded) self-referential value-type cycle —
/// mirrors `auto_derive::AutoDeriveCtx`'s cycle guard.
pub fn is_share_type<Q: ShareQuery>(query: &Q, ty: &TypeRef) -> bool {
    let mut visited: HashSet<String> = HashSet::new();
    is_share_type_rec(query, ty, &mut visited)
}

fn is_share_type_rec<Q: ShareQuery>(
    query: &Q,
    ty: &TypeRef,
    visited: &mut HashSet<String>,
) -> bool {
    match ty {
        // Rule 2 (poison base): raw pointer of ANY pointee modifier. This is
        // the ONLY hardcoded structural rule (no type name involved) — it is
        // itself the audited-escape point: a type embedding one can only
        // regain `share` via its OWN explicit `#share` vouch (checked before
        // recursion reaches the field, in the Named-type branch below).
        TypeRef::Pointer(_, _) => false,
        // Binding-modifier wrappers are transparent to share-ness at the
        // TYPE level (D415 §7 Q5: `ro`/D246 axis is orthogonal — capture-check
        // uses `ro` directly as a capture-kind, not through this predicate).
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Unsafe(inner, _) => {
            is_share_type_rec(query, inner, visited)
        }
        TypeRef::Unit(_) => true,
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
            is_share_type_rec(query, inner, visited)
        }
        TypeRef::Tuple(elems, _) => elems.iter().all(|e| is_share_type_rec(query, e, visited)),
        // Rule 9: fn-pointer / closure type — may close over non-share state
        // invisible at the type level. Conservative `false`.
        TypeRef::Func { .. } => false,
        // Anonymous protocol-type-in-position — existential, unknown concrete
        // layout. Conservative `false` (same reasoning as unresolved Named).
        TypeRef::Protocol { .. } => false,
        // Plan 184 `ref T` — non-relocatable alias to storage. Aliasing IS the
        // whole point of `ref`, so treat like a pointer for share purposes:
        // share iff the referent itself is share (a `ref` to a `#share` type,
        // e.g. `ref Mutex[..]`-shaped code, is fine; a `ref` to a plain `mut`
        // record is exactly the poison case).
        TypeRef::Ref(inner, _) => is_share_type_rec(query, inner, visited),
        TypeRef::Named { path, generics, .. } => {
            let name = match path.last() {
                Some(n) => n.as_str(),
                None => return false,
            };
            // Rule 3: primitives.
            if NOVA_PRIMITIVES.contains(&name) {
                return true;
            }
            // Option[T] / Result[T,E] / []T-via-Vec sugar etc. are ordinary
            // Named generics — if the registry doesn't carry a TypeDecl for
            // them (prelude types may be registry-only), fall back to a
            // structural rule for the well-known container shapes so
            // `Option[Mutex]`-shaped code isn't spuriously poisoned; anything
            // else with no TypeDecl is rule 8 (conservative `false`).
            if let Some(td) = query.lookup_type(name) {
                return is_share_type_decl(query, td, generics, visited);
            }
            match name {
                "Option" | "Result" => generics.iter().all(|g| is_share_type_rec(query, g, visited)),
                _ => false, // rule 8: unresolved / generic type-param.
            }
        }
    }
}

/// Share-ness of a resolved `TypeDecl` (rules 1, 4-7). `generics` are the
/// use-site type-arguments (unused at V1 — no per-field generic-param
/// substitution; a generic type's OWN field declarations are checked
/// as-declared, so a field of the bare type-parameter `T` hits rule 8
/// conservatively until a `Share`-bound generic system exists, D415 §7 Q3/Q7).
fn is_share_type_decl<Q: ShareQuery>(
    query: &Q,
    td: &TypeDecl,
    _generics: &[TypeRef],
    visited: &mut HashSet<String>,
) -> bool {
    // Rule 1: audited vouch always wins — short-circuits field recursion.
    if td.attrs.contains(&crate::ast::TypeAttr::Share) {
        return true;
    }
    if !visited.insert(td.name.clone()) {
        // Cycle guard (defensive — see module doc). Treat as NOT share:
        // can't prove it without assuming the very fact being derived.
        return false;
    }
    let result = match &td.kind {
        TypeDeclKind::Record(fields) => fields
            .iter()
            .all(|f| is_share_type_rec(query, &f.ty, visited)),
        TypeDeclKind::NamedTuple(fields) => fields
            .iter()
            .all(|f| is_share_type_rec(query, &f.ty, visited)),
        TypeDeclKind::Sum(variants) => variants.iter().all(|v| match &v.kind {
            crate::ast::SumVariantKind::Unit => true,
            crate::ast::SumVariantKind::Tuple(tys) => {
                tys.iter().all(|t| is_share_type_rec(query, t, visited))
            }
            crate::ast::SumVariantKind::Record(fields) => fields
                .iter()
                .all(|f| is_share_type_rec(query, &f.ty, visited)),
        }),
        TypeDeclKind::Newtype(inner) => is_share_type_rec(query, inner, visited),
        TypeDeclKind::Alias(inner) => is_share_type_rec(query, inner, visited),
        // Rule 7: no fields to recurse into, no vouch → not share.
        TypeDeclKind::Opaque
        | TypeDeclKind::Effect(_)
        | TypeDeclKind::Protocol { .. }
        | TypeDeclKind::TypeSet(_) => false,
    };
    visited.remove(&td.name);
    result
}
