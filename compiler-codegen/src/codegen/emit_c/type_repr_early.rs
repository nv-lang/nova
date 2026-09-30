//! Registry 221.1 #761 -- the C representation of a NEWTYPE (`type Row int`)
//! or an ALIAS (`type Row alias int`) is known BEFORE the first type body is
//! emitted, not when the emitter happens to reach its declaration.
//!
//! The representation lives in `type_aliases` (`Row` -> `nova_int`), and every
//! lowering of the name reads it there (`resolved_named_to_c`). The entry used
//! to be written only by `emit_type_decl`, i.e. in ITEM ORDER -- the order of
//! the declarations in a file and, across a folder module, the SORTED order of
//! its peer files. Anything lowered earlier met the name unregistered and fell
//! to the heap-record default `Nova_Row*`. One newtype then had two
//! representations inside one program:
//!
//!   * a sum declared ABOVE `Row` (same file, or a peer that sorts first)
//!     stored its arm payload as `Nova_Row*`, and a pattern binding of that
//!     arm (`ToRow(r)`) took the same type;
//!   * `Option[Row]` in a signature, lowered after `Row` was reached, became
//!     `NovaOpt_nova_int`;
//!   * `Some(r)` built from that binding became `NovaOpt_Nova_Row_p`, and the
//!     assignment into the `NovaOpt_nova_int` result was refused by clang.
//!
//! Carina's self-build hit exactly this when `ConstRow` moved from
//! `sem/callables.nv` (3rd) to `sem/defs.nv` (7th): `DefTarget.DefConst` and
//! `Resolution.Const` sat above it, and `const_row`/`const_of` return
//! `Option[ConstRow]` through `Some(row)` out of the arm. The integrator's four
//! probes stayed green because none of them took the payload from a PATTERN
//! BINDING of the arm -- a fresh `Some(7 as Row)` is typed from the value, not
//! from the sum's stored payload type.
//!
//! The fix answers the question where it is asked: a newtype's representation
//! is a property of its declaration, so it is registered for every declaration
//! up front, before any consumer is lowered. Declaration order stops mattering.
//! `emit_type_decl` still emits the `typedef` text and re-inserts the same
//! value; this pass only makes the value known early.
//!
//! What is left to the late path, deliberately:
//!   * generic newtypes (their own branch, per-instance or shared typedef);
//!   * runtime-backed newtypes and `RUNTIME_DEFINED_TYPES` (the header owns
//!     their C type);
//!   * a name declared more than once with DIFFERENT lowerings, or colliding
//!     across modules (bare-name keying would pick one arbitrarily);
//!   * a newtype whose inner type names a local newtype/alias not resolvable
//!     here, or a local named tuple (its alias is written at emission) -- the
//!     early answer could differ from the late one, so none is given.
//!   Chains (`type A B`, `type B int`) are resolved by iterating to a fixpoint.

use std::collections::{HashMap, HashSet};

use super::CEmitter;
use crate::ast::{Item, Module, TypeDecl, TypeDeclKind, TypeRef};

impl CEmitter {
    /// Register the C representation of every non-generic newtype and alias of
    /// `module` in `type_aliases`, independent of declaration order (#761).
    pub(super) fn preregister_type_repr_aliases(&mut self, module: &Module) {
        let mut candidates: Vec<(&TypeDecl, &TypeRef)> = Vec::new();
        let mut named_tuples: HashSet<String> = HashSet::new();
        for item in &module.items {
            let Item::Type(t) = item else { continue };
            if matches!(t.kind, TypeDeclKind::NamedTuple(_)) {
                named_tuples.insert(t.name.clone());
                continue;
            }
            if !t.generics.is_empty()
                || super::RUNTIME_DEFINED_TYPES.contains(&t.name.as_str())
                || Self::debt_is_runtime_backed_newtype(t.name.as_str())
                || self.colliding_type_names.contains(&t.name)
                || self.type_aliases.contains_key(&t.name)
            {
                continue;
            }
            match &t.kind {
                TypeDeclKind::Newtype(inner) | TypeDeclKind::Alias(inner) => candidates.push((t, inner)),
                _ => {}
            }
        }
        let mut open: HashSet<String> = candidates.iter().map(|(t, _)| t.name.clone()).collect();
        // Names given NO early answer stay blocking: a newtype over them would be
        // lowered through the heap-record default, which is the defect itself.
        let mut failed: HashSet<String> = HashSet::new();
        loop {
            let mut resolved: HashMap<String, Option<String>> = HashMap::new();
            for (t, inner) in &candidates {
                if !open.contains(&t.name) { continue; }
                let mut names = HashSet::new();
                let mut vtables = HashSet::new();
                Self::collect_typeref_names(inner, &mut names, &mut vtables);
                if names.iter().any(|n| open.contains(n) || failed.contains(n) || named_tuples.contains(n)) {
                    continue;
                }
                let c = self.type_ref_to_c(inner).ok().filter(|c| !c.is_empty());
                // Two declarations of one name must agree, or neither is registered.
                match resolved.get(&t.name) {
                    Some(prev) if *prev != c => { resolved.insert(t.name.clone(), None); }
                    Some(_) => {}
                    None => { resolved.insert(t.name.clone(), c); }
                }
            }
            if resolved.is_empty() { break; }
            for (name, c) in resolved {
                open.remove(&name);
                match c {
                    Some(c) => { self.type_aliases.insert(name, c); }
                    None => { failed.insert(name); }
                }
            }
        }
    }
}
