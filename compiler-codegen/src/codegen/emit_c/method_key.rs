//! Registry 221.1 #1413 -- the `(Type, method)` key of a call's receiver when
//! the receiver's type is a COLLIDING name (D381: one simple name declared in
//! two modules of the CU).
//!
//! Such a type's C base is module-qualified (`Nova_<mod>_Node`,
//! `qualify_type_base`), while `method_overloads` is keyed by the SIMPLE name
//! (`("Node", "kind_of")`). The return-type inference of a method call without
//! a checker channel (`infer_call_ret_c`) stripped `Nova_` off the receiver's C
//! type and looked up `("<mod>_Node", "kind_of")` -- a miss -- and then fell
//! through to the lookup by the BARE METHOD NAME, which answers with whichever
//! type registered that name last. In Carina that was `Interner.kind_of ->
//! TypeKind` for `Node.kind_of -> NodeKind`: `ro k = c.kind_of()` was declared
//! `Nova_TypeKind* k`, silently, until `TypeKind` got `#impl(Equal)` and `==`
//! on `k` dispatched to TypeKind's `@equal`.

use super::CEmitter;

impl CEmitter {
    /// The simple Nova name behind a receiver's C base, for the
    /// `method_overloads` key: `<mod>_Node` -> `Node` when that is the
    /// module-qualified base of a colliding type; anything else unchanged.
    pub(super) fn method_key_type_name(&self, c_base: &str) -> String {
        if self.colliding_type_names.is_empty() {
            return c_base.to_string();
        }
        for name in &self.colliding_type_names {
            if c_base.len() > name.len()
                && c_base.ends_with(name.as_str())
                && c_base.as_bytes()[c_base.len() - name.len() - 1] == b'_'
                && self
                    .emit_file_module
                    .values()
                    .any(|m| self.qualify_type_base(name, m) == c_base)
            {
                return name.clone();
            }
        }
        c_base.to_string()
    }
}
