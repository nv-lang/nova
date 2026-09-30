//! Registry 221.1 #895, second carrier: a STATIC type-set blanket
//! (`fn[T SignedInts] T.from_ordinal(i int) -> T`, the form D310 spells out as
//! `fn[T SignedInts] T.parse(...)`) called on a primitive (`i64.from_ordinal(3)`).
//!
//! The Path-call check returned early for every primitive receiver with fewer
//! than two own overloads, so the call was accepted without an answer: no
//! callee, no type in the channel. The blanket is registered under its typevar
//! (`"T"`), never under `"i64"`, and the emitter then died on
//! "Path call return type unknown". This resolves the candidate here, where the
//! set bound is known, and writes both answers into the channel; the emitter
//! mono's the callee the channel names.
//!
//! The membership predicate is the one `blanket_set_bound_violation` (#930)
//! applies to the instance half: every bound must be a declared type-set that
//! lists the receiver. A protocol bound is not judged there and is not taken
//! here either — such a candidate is skipped, never guessed.

use super::*;

impl<'a> TypeCheckCtx<'a> {
    /// The static blanket named `method` whose set bounds admit the primitive
    /// `concrete`, with its return type under `T := concrete`.
    fn static_set_blanket(&self, concrete: &str, method: &str) -> Option<(&'a FnDecl, TypeRef)> {
        for (recv_key, methods) in self.sig.method_table.iter() {
            let Some(overloads) = methods.get(method) else { continue };
            for f in overloads {
                let Some(recv) = f.receiver.as_ref() else { continue };
                if !matches!(recv.kind, ReceiverKind::Static) {
                    continue;
                }
                let Some(recv_g) = f.generics.iter().find(|g| &g.name == recv_key) else { continue };
                let admits = !recv_g.bounds.is_empty() && recv_g.bounds.iter().all(|b| {
                    let TypeRef::Named { path, generics, span } = b else { return false };
                    if !generics.is_empty() || path.len() != 1 {
                        return false;
                    }
                    let Some(td) = self.types_get_for_file(&path[0], span.file_id) else { return false };
                    let TypeDeclKind::TypeSet(members) = &td.kind else { return false };
                    members.iter().any(|m| matches!(m, TypeRef::Named { path: mp, generics: mg, .. }
                        if mg.is_empty() && mp.len() == 1 && mp[0] == concrete))
                });
                if !admits {
                    continue;
                }
                let span = f.span;
                let mut subst = HashMap::new();
                subst.insert(recv_key.clone(), TypeRef::Named {
                    path: vec![concrete.to_string()], generics: vec![], span,
                });
                let ret = f.return_type.as_ref()
                    .map(|r| subst_typeref(r, &subst))
                    .unwrap_or(TypeRef::Unit(span));
                return Some((*f, ret));
            }
        }
        None
    }

    /// Record callee and return type of `prim.method(..)` resolved to a static
    /// set-blanket. `true` when the channel was written.
    pub(super) fn record_static_set_blanket_call(
        &self,
        concrete: &str,
        method: &str,
        call_id: crate::ast::ExprId,
    ) -> bool {
        if !call_id.is_set() {
            return false;
        }
        let Some((f, ret)) = self.static_set_blanket(concrete, method) else { return false };
        self.resolved_callees.borrow_mut().insert(call_id, f.span);
        self.resolved_types_buf.borrow_mut().insert(call_id, ResolvedType::from_type_ref(&ret));
        true
    }
}
