//! Registry 221.1 #1395 -- the C lowering of `Self` BY POSITION.
//!
//! The spec gives `Self` inside a method one meaning -- the type itself -- and
//! gives the pointer carrier to exactly one form:
//!
//!   * `spec/decisions/02-types.md` (D326 R1): "`-> @` has a concrete type:
//!     `-> ref Self` for a stack (value) type, `-> Self` for a heap type";
//!   * same file, the `-> Self` row: "`-> Self` (static Self type, D182) --
//!     owned-by-caller; does not inherit the receiver's mutability".
//!
//! The `"Self"` arm of `resolved_named_to_c` used to answer `receiver_c_type`
//! for EVERY instance-method `Self` -- a value type's receiver carrier
//! (`NovaValue_X*` / `NovaTuple_X*`, D226) in a parameter, in `-> Self`, in an
//! annotation and in a generic argument alike -- while the consumers (direct
//! call, synthesized `@clone`, the `<` wrapper) typed those positions by the
//! checker, where `Self` is already the value type. Now the arm answers the
//! value form (`self_value_c`), and the carrier is kept for the `-> @` return
//! alone (`fluent_ret_c`).
//!
//! `-> @` is recognised by its TypeRef: the parser (`parse_fn`, D132) stands a
//! synthesized `Self` for the `@` token, so its span is the `@` token's span.
//! `collect_fluent_ret_spans` records those spans once per module from the
//! `returns_receiver` flag -- the source of truth -- and `type_ref_to_c` asks
//! `fluent_ret_c` before the generic lowering. A span carries its file id, so
//! a span match is exact.

use crate::ast::{Item, Module, TypeRef};

use super::CEmitter;

impl CEmitter {
    /// The value form of a receiver C type: the pointer carrier of a value
    /// type loses its `*`; every other receiver type (heap `Nova_X*`,
    /// primitives, builtin sums, slices) is already the type itself.
    pub(super) fn value_form_of_receiver_c(c: String) -> String {
        if Self::is_value_struct_ptr(&c) {
            c[..c.len() - 1].to_string()
        } else {
            c
        }
    }

    /// `Self` of an instance method in any position but `-> @`.
    pub(super) fn self_value_c(&self, recv: &str) -> String {
        Self::value_form_of_receiver_c(self.receiver_c_type(recv, false))
    }

    /// The return C type of a method whose declared return is `Self`:
    /// the receiver carrier for `-> @`, the value otherwise.
    pub(super) fn self_ret_c(&self, recv: &str, recv_mutable: bool, fluent: bool) -> String {
        if fluent {
            self.receiver_c_type(recv, recv_mutable)
        } else {
            self.self_value_c(recv)
        }
    }

    /// `Some(carrier)` when `ty` is the synthesized `Self` of a `-> @` and an
    /// instance receiver is in scope. A `Self` already bound by a
    /// substitution (generic mono instance, protocol default body) is left to
    /// that binding, as before.
    pub(super) fn fluent_ret_c(&self, ty: &TypeRef) -> Option<String> {
        let TypeRef::Named { path, generics, span } = ty else { return None };
        if !generics.is_empty() || path.len() != 1 || path[0] != "Self" {
            return None;
        }
        if !self.fluent_ret_spans.contains(span)
            || self.current_receiver_is_static
            || self.subst_c("Self").is_some()
            || self.type_subst_overrides.borrow().contains_key("Self")
        {
            return None;
        }
        let recv = self.current_receiver_type.as_ref()?;
        Some(self.receiver_c_type(recv, false))
    }

    /// Record the span of every `-> @` return TypeRef in `module`.
    pub(super) fn collect_fluent_ret_spans(&mut self, module: &Module) {
        let peers = module.peer_files.iter().flat_map(|pf| pf.items_here.iter());
        for item in module.items.iter().chain(peers) {
            if let Item::Fn(f) = item {
                if f.returns_receiver {
                    if let Some(TypeRef::Named { span, .. }) = &f.return_type {
                        self.fluent_ret_spans.insert(*span);
                    }
                }
            }
        }
    }
}
