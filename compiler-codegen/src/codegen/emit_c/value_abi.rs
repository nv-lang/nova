//! D488 rule 2 (owner, 2026-10-01; plan 172.15 Ф.1-бис): how a `ro` value is passed is
//! decided by its SIZE -- up to three machine words inclusive by copy, larger by a pointer
//! to the caller's place. The threshold is ONE named constant of the compiler for every
//! place that decides the passing (a `ro` parameter, a `ro @` receiver, a `-> ro T`
//! result); a number in an expression is not a threshold (D488 «Правило»). A build key,
//! if one ever appears, is one for the whole program: the oracle and Carina must pass a
//! value the same way, since Carina calls the functions of the shell the oracle emits.
//!
//! Before: `param_is_auto_byref` compared `s > 16` -- the SysV register boundary, which
//! the owner's decision of 2026-08-08 (plan 172.15 п.4) replaced with «at least 24 bytes,
//! as in Swift».

/// Three machine words on a 64-bit target: a `ro` value of at most this many bytes is
/// passed by copy, a larger one by a hidden read-only pointer (D488 rule 2).
pub(super) const VALUE_BYREF_THRESHOLD_BYTES: i64 = 3 * 8;

/// Registry 221.1 #1598 (D488 rule 2): the `ro @` receiver of a value record (`NovaValue_X` /
/// `NovaTuple_X`) of at most `VALUE_BYREF_THRESHOLD_BYTES` is passed BY COPY; a larger one and
/// every `mut @` stay a pointer. Before, the receiver was a pointer at any size ("receiver ABI
/// A6" of plan 172.4) while a `ro` PARAMETER already followed the size rule -- two rules for
/// one decision. This is the one door both sides of the ABI ask: the definition
/// (`receiver_c_type`) and every call that builds the receiver argument.
///
/// Pinned to a pointer whatever the size: a type that must not be copied -- `#no_copy`,
/// `consume`, `#zero_on_move` (a copy would not be zeroed), `#share` (its value is shared) --
/// recorded in `recv_pinned_c` where the type is emitted. The answer per C type is remembered
/// on first ask, so a definition and a call can never disagree even if a size were learned
/// late; an unknown size answers "pointer".
impl super::CEmitter {
    pub(super) fn recv_by_copy(&self, c_ty: &str, recv_mutable: bool) -> bool {
        if recv_mutable {
            return false;
        }
        let base = c_ty.trim_start_matches("const ").trim().trim_end_matches('*');
        if !Self::is_value_struct(base) {
            return false;
        }
        if let Some(v) = self.recv_by_copy_memo.borrow().get(base) {
            return *v;
        }
        let v = !self.recv_pinned_c.contains(base)
            && self
                .value_struct_size_align(base, 0)
                .map_or(false, |(size, _)| size <= VALUE_BYREF_THRESHOLD_BYTES);
        self.recv_by_copy_memo.borrow_mut().insert(base.to_string(), v);
        v
    }

    /// Is `nova_self` of the method being emitted a POINTER to the receiver? Read from its
    /// registered C type (`receiver_c_type` decided it, #1598: a small `ro @` value record is
    /// a copy now). The receiver type's NAME used to answer "pointer" for every value record;
    /// it is asked only when `nova_self` has no registered type.
    pub(super) fn self_receiver_is_pointer(&self) -> bool {
        match self.var_types.get("nova_self") {
            Some(c) => Self::is_value_struct_ptr(c) || Self::is_primitive_mut_recv_ptr(c),
            None => self
                .current_receiver_type
                .as_deref()
                .map(|t| t != "str" && self.value_record_names.contains(t))
                .unwrap_or(false),
        }
    }

    /// A receiver that must stay a pointer although the method is not `mut @`: a fluent
    /// `-> @` returns the receiver itself (its chain must reach the caller's binding, the
    /// #1395 carrier), and `consume @` takes the receiver over. Read from the declaration.
    pub(super) fn recv_forced_ptr_decl(f: &crate::ast::FnDecl) -> bool {
        f.returns_receiver || f.receiver.as_ref().map_or(false, |r| r.consume)
    }

    /// Registry 221.1 #1664: is `obj`, whose C type is a STARRED value struct
    /// (`NovaValue_X*`), the PLACE of a value record rather than a raw `*T`? A
    /// fluent `-> @` returns the receiver's place as `NovaValue_X*` (D409, #1598),
    /// and its `.write(v)` / `.read()` are the record's methods, not the pointer
    /// intrinsics -- `f.bump().write(5)` was printed as a store through the
    /// pointer. The C type cannot tell them apart (`*mut Pt` is `NovaValue_Pt*`
    /// too); the checker's type of the expression can: a record, not a pointer.
    pub(super) fn starred_value_is_place(&self, obj: &crate::ast::Expr, obj_ty: &str) -> bool {
        Self::is_value_struct_ptr(obj_ty)
            && matches!(self.resolved_types.get(&obj.id), Some(crate::types::ResolvedType::Named { .. }))
    }

    /// The same, for a call that holds only the registered signature.
    pub(super) fn sig_recv_forced_ptr(&self, sig: &super::MethodSig) -> bool {
        sig.recv_consume
            || sig.fn_span.map_or(false, |sp| self.fluent_recv_fn_spans.contains(&sp))
    }
}
