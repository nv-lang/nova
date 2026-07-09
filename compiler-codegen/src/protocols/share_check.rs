// Plan 173.3 (D415): data-race-freedom — `#share` type-attribute predicates.
//
// This module computes whether a TYPE is safe to alias across a fiber
// boundary ("share"). It is deliberately NOT a protocol (D415 §0 — an
// empty-marker protocol would be trivially satisfied by every type, so the
// existing protocol machinery — `#impl(P)` opt-in, `verify_impl_protocols`
// — is not reused here). It mirrors the STRUCTURE of Plan 126 auto-derive
// (`protocols/auto_derive.rs`: memberwise recursive eligibility) without
// being routed through it.
//
// **Two predicates, one axis (D415 §1).** The `#share` axis is "can this be
// aliased from another fiber"; what that requires depends on the ACCESS the
// alias grants — mirroring Rust's `T: Sync ⇔ &T: Send` split between shared
// (read) views and exclusive (write) access:
//
// - [`is_alias_read_safe`] — safe to alias for READ (a `ro`/immutable view
//   from another fiber). Primitives and deep-immutable aggregates qualify
//   memberwise; the audited `#share` vouch qualifies unconditionally.
// - [`is_mut_alias_safe`] — safe to alias for MUTATION (an outer `mut`
//   binding captured by a `spawn`/`parallel for` body). ONLY internal
//   synchronization makes this safe: the audited `#share` vouch
//   (`Mutex`/`RwLock`/`Atomic*`/…, or a user lock-free type), or an
//   aggregate whose every mutation path bottoms out in such a type. A bare
//   `mut int` accumulator — THE motivating race of Plan 173.3 §1 — is NOT
//   mut-alias-safe (nothing synchronizes the cell), even though `int` is
//   read-alias-safe.
//
// **Poison base (D415 §1, hardcoded — but names ZERO types, §3-compliant):**
// a raw pointer (`*T`, any pointee modifier) and — its binding-level twin —
// a directly writable cell (`mut` field / `mut` scalar binding) with no
// audited synchronization around it. Any type transitively containing one
// is not share; the ONLY escape is the type's own `#share` vouch (analogous
// to Rust `UnsafeCell: !Sync` + `unsafe impl Sync`).
//
// **Conservative directions (V1):** an unresolved type name (generic
// type-param, registry-miss) / fn-type / anonymous-protocol / opaque-без-
// vouch → NOT share (can't prove it). Effect/Protocol/TypeSet decls → NOT
// share (no concrete instance identity).

use crate::ast::{TypeDecl, TypeDeclKind, TypeRef};
use std::collections::HashSet;

/// Query interface needed by the share predicates — separates the pure rule
/// from `TypeCheckCtx`/`CapabilityCtx` wiring (unit-testable in isolation,
/// mirrors `auto_derive::DeriveQuery`).
pub trait ShareQuery {
    /// Lookup a type declaration by (bare) name. `None` — unknown / unresolved
    /// (generic type-param, or a name the registry doesn't carry) → both
    /// predicates conservatively answer `false`.
    fn lookup_type(&self, name: &str) -> Option<&TypeDecl>;
}

const NOVA_PRIMITIVES: &[&str] = crate::protocols::auto_derive::NOVA_PRIMITIVES;

/// Access level an alias grants — selects which memberwise rule applies.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    /// Shared read-only view (`ro` capture / immutable field).
    Read,
    /// Exclusive-shaped write access (`mut` binding capture / `mut` field).
    Mut,
}

/// Is `ty` safe to alias from another fiber for READ access?
/// (`ro` deep-immutable capture — D246; immutable fields memberwise.)
pub fn is_alias_read_safe<Q: ShareQuery>(query: &Q, ty: &TypeRef) -> bool {
    let mut visited = HashSet::new();
    share_rec(query, ty, Access::Read, &mut visited)
}

/// Is `ty` safe to alias from another fiber for MUTATION — i.e. may an
/// outer `mut` binding of this type be captured by a `spawn`/`parallel for`
/// body? True ONLY when every mutation path is internally synchronized
/// (audited `#share` vouch, transitively).
pub fn is_mut_alias_safe<Q: ShareQuery>(query: &Q, ty: &TypeRef) -> bool {
    let mut visited = HashSet::new();
    share_rec(query, ty, Access::Mut, &mut visited)
}

fn share_rec<Q: ShareQuery>(
    query: &Q,
    ty: &TypeRef,
    access: Access,
    visited: &mut HashSet<String>,
) -> bool {
    match ty {
        // Poison base: raw pointer of ANY pointee modifier — never share by
        // structure; only a containing type's own `#share` vouch (checked in
        // the Named branch before recursion reaches the field) escapes.
        TypeRef::Pointer(_, _) => false,
        // Binding-modifier wrappers are transparent at the TYPE level; the
        // ACCESS distinction is carried by the `access` parameter (the
        // capture-check derives it from the binding's `mut`-ness, D415 §7 Q5).
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Unsafe(inner, _) => {
            share_rec(query, inner, access, visited)
        }
        TypeRef::Unit(_) => true,
        // Scalars: read-aliasing is safe; a writable scalar cell aliased
        // across fibers is THE race (unsynchronized load/store) — not
        // mut-alias-safe.
        TypeRef::Named { path, .. }
            if path.len() == 1 && NOVA_PRIMITIVES.contains(&path[0].as_str()) =>
        {
            access == Access::Read
        }
        // Containers ([]T = Vec, fixed arrays): reading concurrently is safe
        // iff elements are read-safe; mutating (push/set) is unsynchronized →
        // never mut-alias-safe.
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
            access == Access::Read && share_rec(query, inner, Access::Read, visited)
        }
        TypeRef::Tuple(elems, _) => {
            access == Access::Read
                && elems.iter().all(|e| share_rec(query, e, Access::Read, visited))
        }
        // fn-pointer / closure type — may close over non-share state
        // invisible at the type level. Conservative `false`.
        TypeRef::Func { .. } => false,
        // Anonymous protocol-type — existential, unknown concrete layout.
        TypeRef::Protocol { .. } => false,
        // Plan 184 `ref T` — non-relocatable alias to storage; aliasing IS
        // its point. Same access level flows through to the referent.
        TypeRef::Ref(inner, _) => share_rec(query, inner, access, visited),
        TypeRef::Named { path, generics, .. } => {
            let name = match path.last() {
                Some(n) => n.as_str(),
                None => return false,
            };
            if let Some(td) = query.lookup_type(name) {
                return share_decl(query, td, access, visited);
            }
            // Registry-miss fallback for the well-known prelude containers
            // (their TypeDecl may be registry-only): Option/Result payloads
            // follow the read rule like tuples.
            match name {
                "Option" | "Result" => {
                    access == Access::Read
                        && generics.iter().all(|g| share_rec(query, g, Access::Read, visited))
                }
                _ => false, // unresolved / generic type-param — conservative.
            }
        }
    }
}

/// Share-ness of a resolved `TypeDecl`. Use-site generics are NOT
/// substituted at V1 — a field of a bare type-parameter `T` hits the
/// unresolved-name rule conservatively until a `Share`-bound generic system
/// exists (D415 §7 Q3/Q7).
fn share_decl<Q: ShareQuery>(
    query: &Q,
    td: &TypeDecl,
    access: Access,
    visited: &mut HashSet<String>,
) -> bool {
    // Audited vouch always wins, for BOTH access levels — the author asserts
    // real internal synchronization (its `mut` methods are safe to call
    // through a cross-fiber alias). Short-circuits field recursion.
    if td.attrs.contains(&crate::ast::TypeAttr::Share) {
        return true;
    }
    if !visited.insert(td.name.clone()) {
        // Cycle guard (defensive): can't prove share without assuming the
        // very fact being derived → not share.
        return false;
    }
    let result = match &td.kind {
        // Memberwise (auto-derive, D415 §1): an immutable field is a read
        // view (needs read-alias safety); a `mut` field is a writable cell
        // reachable through the alias (needs mut-alias safety — i.e. an
        // audited-sync type — REGARDLESS of the outer access level, because
        // a heap record aliased read-only still exposes its `mut` fields to
        // whoever holds the OTHER, mutable alias).
        TypeDeclKind::Record(fields) => fields.iter().all(|f| {
            let field_access = if f.mutable { Access::Mut } else { Access::Read };
            share_rec(query, &f.ty, field_access, visited)
        }),
        // Named tuples / sums: payloads are immutable views (no in-place
        // payload mutation surface) → read rule.
        TypeDeclKind::NamedTuple(fields) => fields
            .iter()
            .all(|f| share_rec(query, &f.ty, Access::Read, visited)),
        TypeDeclKind::Sum(variants) => variants.iter().all(|v| match &v.kind {
            crate::ast::SumVariantKind::Unit => true,
            crate::ast::SumVariantKind::Tuple(tys) => {
                tys.iter().all(|t| share_rec(query, t, Access::Read, visited))
            }
            crate::ast::SumVariantKind::Record(fields) => fields
                .iter()
                .all(|f| share_rec(query, &f.ty, Access::Read, visited)),
        }),
        // Newtype/Alias: transparent — same access level flows through.
        TypeDeclKind::Newtype(inner) => share_rec(query, inner, access, visited),
        TypeDeclKind::Alias(inner) => share_rec(query, inner, access, visited),
        // No fields to recurse into + no vouch → can't prove share.
        TypeDeclKind::Opaque
        | TypeDeclKind::Effect(_)
        | TypeDeclKind::Protocol { .. }
        | TypeDeclKind::TypeSet(_) => false,
    };
    visited.remove(&td.name);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{RecordField, SumVariant, SumVariantKind, TypeAttr, TypeDeclKind};
    use crate::diag::Span;
    use std::collections::HashMap;

    struct MockQuery {
        types: HashMap<String, TypeDecl>,
    }
    impl ShareQuery for MockQuery {
        fn lookup_type(&self, name: &str) -> Option<&TypeDecl> {
            self.types.get(name)
        }
    }

    fn named(name: &str) -> TypeRef {
        TypeRef::Named { path: vec![name.to_string()], generics: vec![], span: Span::dummy() }
    }

    fn field(name: &str, ty: TypeRef, mutable: bool) -> RecordField {
        RecordField {
            name: name.to_string(),
            ty,
            mutable,
            ..RecordField::default()
        }
    }

    fn record(name: &str, fields: Vec<RecordField>, share: bool) -> TypeDecl {
        TypeDecl {
            name: name.to_string(),
            kind: TypeDeclKind::Record(fields),
            attrs: if share { vec![TypeAttr::Share] } else { vec![] },
            ..TypeDecl::default()
        }
    }

    #[test]
    fn primitive_read_ok_mut_race() {
        let q = MockQuery { types: HashMap::new() };
        assert!(is_alias_read_safe(&q, &named("int")));
        assert!(!is_mut_alias_safe(&q, &named("int"))); // THE motivating race
    }

    #[test]
    fn pointer_is_poison_both_ways() {
        let q = MockQuery { types: HashMap::new() };
        let ptr = TypeRef::Pointer(Box::new(named("int")), Span::dummy());
        assert!(!is_alias_read_safe(&q, &ptr));
        assert!(!is_mut_alias_safe(&q, &ptr));
    }

    #[test]
    fn audited_vouch_wins_for_mut() {
        let mut types = HashMap::new();
        types.insert("Mutex".to_string(), record("Mutex", vec![], true));
        let q = MockQuery { types };
        assert!(is_mut_alias_safe(&q, &named("Mutex")));
    }

    #[test]
    fn immutable_record_read_ok_mut_ok() {
        let mut types = HashMap::new();
        types.insert(
            "Point".to_string(),
            record(
                "Point",
                vec![field("x", named("int"), false), field("y", named("int"), false)],
                false,
            ),
        );
        let q = MockQuery { types };
        assert!(is_alias_read_safe(&q, &named("Point")));
        // No writable paths → mut binding capture is benign (pointer copy).
        assert!(is_mut_alias_safe(&q, &named("Point")));
    }

    #[test]
    fn mut_scalar_field_poisons() {
        let mut types = HashMap::new();
        types.insert(
            "Ctr".to_string(),
            record("Ctr", vec![field("n", named("int"), true)], false),
        );
        let q = MockQuery { types };
        assert!(!is_alias_read_safe(&q, &named("Ctr")));
        assert!(!is_mut_alias_safe(&q, &named("Ctr")));
    }

    #[test]
    fn mut_mutex_field_ok() {
        let mut types = HashMap::new();
        types.insert("Mutex".to_string(), record("Mutex", vec![], true));
        types.insert(
            "Shared".to_string(),
            record("Shared", vec![field("mu", named("Mutex"), true)], false),
        );
        let q = MockQuery { types };
        assert!(is_mut_alias_safe(&q, &named("Shared")));
    }

    #[test]
    fn vec_read_ok_mut_race() {
        let q = MockQuery { types: HashMap::new() };
        let vec_int = TypeRef::Array(Box::new(named("int")), Span::dummy());
        assert!(is_alias_read_safe(&q, &vec_int));
        assert!(!is_mut_alias_safe(&q, &vec_int));
    }

    #[test]
    fn unknown_name_conservative_false() {
        let q = MockQuery { types: HashMap::new() };
        assert!(!is_alias_read_safe(&q, &named("Whatever")));
        assert!(!is_mut_alias_safe(&q, &named("T")));
    }

    #[test]
    fn sum_of_share_payloads_read_ok() {
        let mut types = HashMap::new();
        types.insert(
            "E".to_string(),
            TypeDecl {
                name: "E".to_string(),
                kind: TypeDeclKind::Sum(vec![
                    SumVariant {
                        name: "A".into(),
                        kind: SumVariantKind::Unit,
                        discriminant: None,
                        span: Span::dummy(),
                        serde_attrs: vec![],
                    },
                    SumVariant {
                        name: "B".into(),
                        kind: SumVariantKind::Tuple(vec![named("int")]),
                        discriminant: None,
                        span: Span::dummy(),
                        serde_attrs: vec![],
                    },
                ]),
                ..TypeDecl::default()
            },
        );
        let q = MockQuery { types };
        assert!(is_alias_read_safe(&q, &named("E")));
    }
}
