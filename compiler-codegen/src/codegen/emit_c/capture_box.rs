//! Registry 221.1 #1559: the heap box of a captured `mut` local (or of a by-pointer
//! source -- an in-out parameter, a value record passed by pointer) is DECLARED at
//! the top of the enclosing C function, not where the first capture stands.
//!
//! Before: the first closure (or escaping handler) capturing `name` emitted
//! `T* _box_name = nova_alloc(..)` at its own call site and registered the box in
//! `var_boxed`, which lives for the whole function. A `with` (and every other
//! construct that opens a C block) put that declaration in a nested C block, so a
//! second capture under a SIBLING `with`, or a read of `name` after the block,
//! named `_box_name` out of its C scope -- clang refused the generated C
//! (`use of undeclared identifier '_box_at'`): a record parameter captured by two
//! lambdas, each under its own `with`, blocked claude-limits; a `mut` scalar or a
//! `mut` record local fails the same way.
//!
//! Now: the declaration is hoisted (`T* _box_name = NULL;`) with the detach boxes'
//! anchor (`hoist_box_decl`, #240), the first capture site allocates and copies
//! as before, a later site whose path may not have run the first one allocates
//! only while the box is still NULL, and a read of `name` falls back to the local
//! while the box is NULL (`(*(box ? box : &local))`, the shape of #790 for the
//! detach boxes). While the box is NULL the local is the only storage; once a
//! capture ran, the box is.

use super::*;

impl CEmitter {
    /// The anchor `hoist_box_decl` inserts at: the current end of `self.out`, the
    /// current indent, and the text just before the anchor. The text is checked at
    /// every hoist -- an anchor taken in another output buffer (a nested C function
    /// emitted through a swapped `self.out`) does not match there, and the hoist
    /// falls back to the old inline declaration instead of inserting into the wrong
    /// function.
    pub(super) fn box_hoist_anchor(&self) -> Option<(usize, usize, String, usize)> {
        let end = self.out.len();
        let mut start = end.saturating_sub(120);
        while !self.out.is_char_boundary(start) {
            start += 1;
        }
        Some((end, self.indent, self.out[start..].to_string(), 0))
    }

    /// #1559: the box of the captured `name`, declared at the function's anchor.
    /// `ptr_src`: the local is already a pointer to the box's type (an in-out
    /// parameter, a value record by pointer), so its value is `(*local)` and the
    /// fallback of a NULL box is the local itself.
    pub(super) fn capture_box(&mut self, name: &str, box_ty: &str, ptr_src: bool) -> String {
        let local = Self::mangle_field_name(name);
        let copy_src = if ptr_src { format!("(*{local})") } else { local.clone() };
        if let Some(existing) = self.var_boxed.get(name).cloned() {
            // A box first made on another path (a sibling `with`, an untaken branch)
            // may still be NULL here.
            if self.lazy_box_fallback.contains_key(&existing) {
                self.line(&format!(
                    "if (!{existing}) {{ {existing} = ({box_ty}*)nova_alloc(sizeof({box_ty})); *{existing} = {copy_src}; }}"
                ));
            }
            return existing;
        }
        let bv = format!("_box_{name}");
        // Without an anchor the declaration stands here, inline, as before #1559,
        // and is never NULL past this point.
        if self.hoist_box_decl(box_ty, &bv) {
            let fallback = if ptr_src { local } else { format!("&{local}") };
            self.lazy_box_fallback.insert(bv.clone(), fallback);
        } else {
            self.lazy_box_fallback.remove(&bv);
        }
        self.line(&format!("{bv} = ({box_ty}*)nova_alloc(sizeof({box_ty}));"));
        self.line(&format!("*{bv} = {copy_src};"));
        self.var_boxed.insert(name.to_string(), bv.clone());
        bv
    }
}
