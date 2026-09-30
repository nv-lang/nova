//! Registry 221.1 #895, second carrier: emitting a STATIC type-set blanket
//! call on a primitive (`i64.from_ordinal(3)` against
//! `fn[T SignedInts] T.from_ordinal(i int) -> T`).
//!
//! The blanket is registered under its typevar (`("T", "from_ordinal")`), so the
//! Path-form static dispatch, keyed by the receiver (`("i64", ..)`), never saw it
//! and fell through to "Path call return type unknown". The checker now names
//! the callee in `resolved_callees` (`types/static_blanket.rs`, where the set
//! bound is judged); this reads that answer and mono's the declaration for
//! `T := <primitive>` — the static twin of the instance blanket dispatch
//! (`Nova_<concrete>_method_<m>`), named `Nova_<prim>_static_<m>`.

use super::CEmitter;

impl CEmitter {
    pub(super) fn try_static_set_blanket_call(
        &mut self,
        recv_seg: &str,
        method: &str,
        args: &[crate::ast::CallArg],
        call_id: crate::ast::ExprId,
    ) -> Result<Option<String>, String> {
        let Some(prim_c) = Self::primitive_name_to_c(recv_seg) else { return Ok(None) };
        let Some(span) = self.resolved_callees.get(&call_id).copied() else { return Ok(None) };
        let decl = self.mono_method_decls.iter()
            .find(|((tv, m), fd)| m == method && fd.span == span
                && fd.receiver.as_ref().map_or(false, |r| &r.type_name == tv
                    && matches!(r.kind, crate::ast::ReceiverKind::Static)))
            .map(|((tv, _), fd)| (tv.clone(), fd.clone()));
        let Some((tvname, fn_decl)) = decl else { return Ok(None) };
        let mono_name = format!("Nova_{}_static_{}", recv_seg, method);
        let type_subst = vec![(tvname, prim_c.to_string())];
        self.register_mono_method_instance(&fn_decl, type_subst, &mono_name, recv_seg);
        let mut arg_strs = Vec::with_capacity(args.len());
        for a in args {
            arg_strs.push(self.emit_expr(a.expr())?);
        }
        Ok(Some(format!("{}({})", mono_name, arg_strs.join(", "))))
    }
}
