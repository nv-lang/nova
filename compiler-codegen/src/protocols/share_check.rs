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
// **Failure-path explanation ([M-173.3-share-leakage-explain]):** both
// predicates are thin wrappers over [`read_alias_failure`] /
// [`mut_alias_failure`], which return the PATH of the first refusal
// ([`ShareFailure`]: field-segment chain + failing type + [`ShareReason`]).
// The E_CONCURRENT_MUT_CAPTURE diagnostic renders it as
// "`Outer.inner.buf` (`[]u8`): a writable cell …" so that adding a non-share
// `mut` field deep in a type no longer strips share-ness SILENTLY (D415 §7
// Q8 — leakage diagnostics).
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
use std::fmt;

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

/// Why a specific point in the type refused share-ness
/// ([M-173.3-share-leakage-explain]). `Display` yields the short English
/// clause embedded in E_CONCURRENT_MUT_CAPTURE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareReason {
    /// `*T` — the D415 §1 poison base; no synchronization is expressible
    /// through a raw pointer, only the containing type's `#share` vouch
    /// escapes.
    RawPointer,
    /// A directly writable cell (`mut` binding / `mut` field of a scalar or
    /// container) with no audited synchronization — concurrent load/store
    /// through the alias is THE race.
    WritableCell,
    /// fn/closure type — may close over non-share state invisible at the
    /// type level.
    FnType,
    /// Anonymous protocol-type — existential, concrete layout unknown.
    ProtocolType,
    /// Unresolved name (generic type-param / registry miss) — share-ness
    /// cannot be proven.
    Unresolved(String),
    /// Declaration with no memberwise structure and no `#share` vouch
    /// (opaque / effect / protocol / type-set).
    NoStructure(String),
    /// Recursive cycle through the named type without a `#share` vouch.
    Cycle(String),
}

impl fmt::Display for ShareReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShareReason::RawPointer => write!(
                f,
                "a raw pointer (D415 poison base — no synchronization is \
                 expressible through it; only the containing type's own \
                 `#share` vouch escapes)"
            ),
            ShareReason::WritableCell => {
                write!(f, "a writable cell with no audited synchronization")
            }
            ShareReason::FnType => {
                write!(f, "a fn/closure type (may close over non-share state)")
            }
            ShareReason::ProtocolType => {
                write!(f, "an anonymous protocol-type (concrete layout unknown)")
            }
            ShareReason::Unresolved(n) => write!(
                f,
                "an unresolved type name `{}` (generic type-param or \
                 registry miss) — share-ness cannot be proven",
                n
            ),
            ShareReason::NoStructure(n) => write!(
                f,
                "`{}` has no memberwise structure and no `#share` vouch",
                n
            ),
            ShareReason::Cycle(n) => write!(
                f,
                "a recursive cycle through `{}` without a `#share` vouch",
                n
            ),
        }
    }
}

/// The FIRST refusal point of a share predicate — feeds the per-field
/// explanation in E_CONCURRENT_MUT_CAPTURE ([M-173.3-share-leakage-explain]).
#[derive(Debug, Clone)]
pub struct ShareFailure {
    /// Pre-formatted path segments from the root type to the refusal point:
    /// `.field` (record/named-tuple field), `.0` (tuple element), `[..]`
    /// (container element), `[0]` (Option/Result payload),
    /// `::Variant.0` / `::Variant.field` (sum payload). Empty — the root
    /// type itself refused.
    pub path: Vec<String>,
    /// Rendered type at the refusal point (`render_type_ref`; for
    /// decl-level refusals — the declaration name).
    pub ty: String,
    pub reason: ShareReason,
}

impl ShareFailure {
    /// `Outer.inner.buf`-style chain for the diagnostic; `root` is the
    /// rendered type of the captured binding.
    pub fn chain(&self, root: &str) -> String {
        format!("{}{}", root, self.path.concat())
    }
}

/// Is `ty` safe to alias from another fiber for READ access?
/// (`ro` deep-immutable capture — D246; immutable fields memberwise.)
pub fn is_alias_read_safe<Q: ShareQuery>(query: &Q, ty: &TypeRef) -> bool {
    read_alias_failure(query, ty).is_none()
}

/// Is `ty` safe to alias from another fiber for MUTATION — i.e. may an
/// outer `mut` binding of this type be captured by a `spawn`/`parallel for`
/// body? True ONLY when every mutation path is internally synchronized
/// (audited `#share` vouch, transitively).
pub fn is_mut_alias_safe<Q: ShareQuery>(query: &Q, ty: &TypeRef) -> bool {
    mut_alias_failure(query, ty).is_none()
}

/// Explain-variant of [`is_alias_read_safe`]: `None` = safe, `Some` = the
/// first refusal path.
pub fn read_alias_failure<Q: ShareQuery>(query: &Q, ty: &TypeRef) -> Option<ShareFailure> {
    let mut visited = HashSet::new();
    let mut at = Vec::new();
    share_rec(query, ty, Access::Read, &mut visited, &mut at).err()
}

/// Explain-variant of [`is_mut_alias_safe`]: `None` = safe, `Some` = the
/// first refusal path.
pub fn mut_alias_failure<Q: ShareQuery>(query: &Q, ty: &TypeRef) -> Option<ShareFailure> {
    let mut visited = HashSet::new();
    let mut at = Vec::new();
    share_rec(query, ty, Access::Mut, &mut visited, &mut at).err()
}

/// Build a refusal at the current path for a TypeRef-shaped point.
fn refuse(at: &[String], ty: &TypeRef, reason: ShareReason) -> Result<(), ShareFailure> {
    Err(ShareFailure {
        path: at.to_vec(),
        ty: crate::types::render_type_ref(ty),
        reason,
    })
}

fn share_rec<Q: ShareQuery>(
    query: &Q,
    ty: &TypeRef,
    access: Access,
    visited: &mut HashSet<String>,
    at: &mut Vec<String>,
) -> Result<(), ShareFailure> {
    match ty {
        // Poison base: raw pointer of ANY pointee modifier — never share by
        // structure; only a containing type's own `#share` vouch (checked in
        // the Named branch before recursion reaches the field) escapes.
        TypeRef::Pointer(_, _) => refuse(at, ty, ShareReason::RawPointer),
        // Binding-modifier wrappers are transparent at the TYPE level; the
        // ACCESS distinction is carried by the `access` parameter (the
        // capture-check derives it from the binding's `mut`-ness, D415 §7 Q5).
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Uninit(inner, _) => {
            share_rec(query, inner, access, visited, at)
        }
        TypeRef::Unit(_) => Ok(()),
        // Scalars: read-aliasing is safe; a writable scalar cell aliased
        // across fibers is THE race (unsynchronized load/store) — not
        // mut-alias-safe.
        TypeRef::Named { path, .. }
            if path.len() == 1 && NOVA_PRIMITIVES.contains(&path[0].as_str()) =>
        {
            if access == Access::Read {
                Ok(())
            } else {
                refuse(at, ty, ShareReason::WritableCell)
            }
        }
        // Containers ([]T = Vec, fixed arrays): reading concurrently is safe
        // iff elements are read-safe; mutating (push/set) is unsynchronized →
        // never mut-alias-safe.
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
            if access != Access::Read {
                return refuse(at, ty, ShareReason::WritableCell);
            }
            at.push("[..]".to_string());
            let r = share_rec(query, inner, Access::Read, visited, at);
            at.pop();
            r
        }
        TypeRef::Tuple(elems, _) => {
            if access != Access::Read {
                return refuse(at, ty, ShareReason::WritableCell);
            }
            for (i, e) in elems.iter().enumerate() {
                at.push(format!(".{}", i));
                let r = share_rec(query, e, Access::Read, visited, at);
                at.pop();
                r?;
            }
            Ok(())
        }
        // fn-pointer / closure type — may close over non-share state
        // invisible at the type level. Conservative refusal.
        TypeRef::Func { .. } => refuse(at, ty, ShareReason::FnType),
        // Anonymous protocol-type — existential, unknown concrete layout.
        TypeRef::Protocol { .. } => refuse(at, ty, ShareReason::ProtocolType),
        // Plan 184 `ref T` — non-relocatable alias to storage; aliasing IS
        // its point. Same access level flows through to the referent.
        TypeRef::Ref(inner, _) => share_rec(query, inner, access, visited, at),
        TypeRef::Named { path: tpath, generics, .. } => {
            let name = match tpath.last() {
                Some(n) => n.as_str(),
                None => return refuse(at, ty, ShareReason::Unresolved(String::new())),
            };
            if let Some(td) = query.lookup_type(name) {
                return share_decl(query, td, access, visited, at);
            }
            // Registry-miss fallback for the well-known prelude containers
            // (their TypeDecl may be registry-only): Option/Result payloads
            // follow the read rule like tuples.
            match name {
                "Option" | "Result" => {
                    if access != Access::Read {
                        return refuse(at, ty, ShareReason::WritableCell);
                    }
                    for (i, g) in generics.iter().enumerate() {
                        at.push(format!("[{}]", i));
                        let r = share_rec(query, g, Access::Read, visited, at);
                        at.pop();
                        r?;
                    }
                    Ok(())
                }
                // unresolved / generic type-param — conservative.
                _ => refuse(at, ty, ShareReason::Unresolved(name.to_string())),
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
    at: &mut Vec<String>,
) -> Result<(), ShareFailure> {
    // Audited vouch always wins, for BOTH access levels — the author asserts
    // real internal synchronization (its `mut` methods are safe to call
    // through a cross-fiber alias). Short-circuits field recursion.
    if td.attrs.contains(&crate::ast::TypeAttr::Share) {
        return Ok(());
    }
    if !visited.insert(td.name.clone()) {
        // Cycle guard (defensive): can't prove share without assuming the
        // very fact being derived → not share.
        return Err(ShareFailure {
            path: at.clone(),
            ty: td.name.clone(),
            reason: ShareReason::Cycle(td.name.clone()),
        });
    }
    let result = match &td.kind {
        // Memberwise (auto-derive, D415 §1): an immutable field is a read
        // view (needs read-alias safety); a `mut` field is a writable cell
        // reachable through the alias (needs mut-alias safety — i.e. an
        // audited-sync type — REGARDLESS of the outer access level, because
        // a heap record aliased read-only still exposes its `mut` fields to
        // whoever holds the OTHER, mutable alias).
        TypeDeclKind::Record(fields) => {
            let mut r = Ok(());
            for f in fields {
                let field_access = if f.mutable { Access::Mut } else { Access::Read };
                at.push(format!(".{}", f.name));
                r = share_rec(query, &f.ty, field_access, visited, at);
                at.pop();
                if r.is_err() {
                    break;
                }
            }
            r
        }
        // Named tuples / sums: payloads are immutable views (no in-place
        // payload mutation surface) → read rule.
        TypeDeclKind::NamedTuple(fields) => {
            let mut r = Ok(());
            for f in fields {
                at.push(format!(".{}", f.name));
                r = share_rec(query, &f.ty, Access::Read, visited, at);
                at.pop();
                if r.is_err() {
                    break;
                }
            }
            r
        }
        TypeDeclKind::Sum(variants) => {
            let mut r = Ok(());
            'variants: for v in variants {
                match &v.kind {
                    crate::ast::SumVariantKind::Unit => {}
                    crate::ast::SumVariantKind::Tuple(tys) => {
                        for (i, t) in tys.iter().enumerate() {
                            at.push(format!("::{}.{}", v.name, i));
                            r = share_rec(query, t, Access::Read, visited, at);
                            at.pop();
                            if r.is_err() {
                                break 'variants;
                            }
                        }
                    }
                    crate::ast::SumVariantKind::Record(fields) => {
                        for f in fields {
                            at.push(format!("::{}.{}", v.name, f.name));
                            r = share_rec(query, &f.ty, Access::Read, visited, at);
                            at.pop();
                            if r.is_err() {
                                break 'variants;
                            }
                        }
                    }
                }
            }
            r
        }
        // Newtype/Alias: transparent — same access level flows through.
        TypeDeclKind::Newtype(inner) => share_rec(query, inner, access, visited, at),
        TypeDeclKind::Alias(inner) => share_rec(query, inner, access, visited, at),
        // No fields to recurse into + no vouch → can't prove share.
        TypeDeclKind::Opaque
        | TypeDeclKind::Effect(_)
        | TypeDeclKind::Protocol { .. }
        | TypeDeclKind::TypeSet(_) => Err(ShareFailure {
            path: at.clone(),
            ty: td.name.clone(),
            reason: ShareReason::NoStructure(td.name.clone()),
        }),
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
                        doc: None,
                    },
                    SumVariant {
                        name: "B".into(),
                        kind: SumVariantKind::Tuple(vec![named("int")]),
                        discriminant: None,
                        span: Span::dummy(),
                        serde_attrs: vec![],
                        doc: None,
                    },
                ]),
                ..TypeDecl::default()
            },
        );
        let q = MockQuery { types };
        assert!(is_alias_read_safe(&q, &named("E")));
    }

    // ---- failure-path tests ([M-173.3-share-leakage-explain]) ----

    #[test]
    fn failure_root_primitive_mut() {
        let q = MockQuery { types: HashMap::new() };
        let f = mut_alias_failure(&q, &named("int")).expect("must refuse");
        assert!(f.path.is_empty());
        assert_eq!(f.ty, "int");
        assert_eq!(f.reason, ShareReason::WritableCell);
        assert_eq!(f.chain("int"), "int");
    }

    #[test]
    fn failure_mut_scalar_field_path() {
        let mut types = HashMap::new();
        types.insert(
            "Ctr".to_string(),
            record("Ctr", vec![field("n", named("int"), true)], false),
        );
        let q = MockQuery { types };
        let f = mut_alias_failure(&q, &named("Ctr")).expect("must refuse");
        assert_eq!(f.path, vec![".n".to_string()]);
        assert_eq!(f.ty, "int");
        assert_eq!(f.reason, ShareReason::WritableCell);
        assert_eq!(f.chain("Ctr"), "Ctr.n");
    }

    #[test]
    fn failure_pointer_field_path() {
        let mut types = HashMap::new();
        types.insert(
            "RawCell".to_string(),
            record(
                "RawCell",
                vec![field(
                    "p",
                    TypeRef::Pointer(Box::new(named("int")), Span::dummy()),
                    false,
                )],
                false,
            ),
        );
        let q = MockQuery { types };
        let f = mut_alias_failure(&q, &named("RawCell")).expect("must refuse");
        assert_eq!(f.path, vec![".p".to_string()]);
        assert_eq!(f.ty, "* int");
        assert_eq!(f.reason, ShareReason::RawPointer);
        assert_eq!(f.chain("RawCell"), "RawCell.p");
    }

    #[test]
    fn failure_nested_two_segment_path() {
        // type Inner { mut buf []u8 }  type Outer { inner Inner } —
        // the refusal path must walk BOTH segments: Outer.inner.buf.
        let mut types = HashMap::new();
        types.insert(
            "Inner".to_string(),
            record(
                "Inner",
                vec![field(
                    "buf",
                    TypeRef::Array(Box::new(named("u8")), Span::dummy()),
                    true,
                )],
                false,
            ),
        );
        types.insert(
            "Outer".to_string(),
            record("Outer", vec![field("inner", named("Inner"), false)], false),
        );
        let q = MockQuery { types };
        let f = mut_alias_failure(&q, &named("Outer")).expect("must refuse");
        assert_eq!(f.path, vec![".inner".to_string(), ".buf".to_string()]);
        assert_eq!(f.ty, "[]u8");
        assert_eq!(f.reason, ShareReason::WritableCell);
        assert_eq!(f.chain("Outer"), "Outer.inner.buf");
    }

    #[test]
    fn failure_unresolved_generic_param() {
        let q = MockQuery { types: HashMap::new() };
        let f = mut_alias_failure(&q, &named("T")).expect("must refuse");
        assert!(f.path.is_empty());
        assert_eq!(f.reason, ShareReason::Unresolved("T".to_string()));
    }

    #[test]
    fn failure_container_element_read_path() {
        // ro [](*int) — read-alias fails INSIDE the element, path `[..]`.
        let q = MockQuery { types: HashMap::new() };
        let vec_ptr = TypeRef::Array(
            Box::new(TypeRef::Pointer(Box::new(named("int")), Span::dummy())),
            Span::dummy(),
        );
        let f = read_alias_failure(&q, &vec_ptr).expect("must refuse");
        assert_eq!(f.path, vec!["[..]".to_string()]);
        assert_eq!(f.reason, ShareReason::RawPointer);
    }
}
