//! Registry 221.1 #1616 (D488): how a call passes an argument is decided by the function
//! the call resolved to, never by a same-named function of another module.
//!
//! Before: the call site read a free function's parameter conventions from two tables keyed
//! by the BARE name -- `free_fn_inout_params` (a `mut` parameter is passed by pointer) and
//! `free_fn_byref_params` (a large `ro` value by address, R3). The first was filled by every
//! forward declaration, the last one written winning: with `backups.stamp(ms int)` and
//! `respond.stamp(mut r Rec)` in one unit, `backups`'s own call `stamp(ms)` reached the right
//! C symbol and passed `&ms` -- the address of the argument as its value, a different result
//! on every run. The definition side was right all along: it reads its own parameters.
//!
//! The conventions are now recorded per DECLARATION (its span, the identity the checker's
//! `resolved_callees` channel carries), and a call takes those of the declaration it
//! resolved to. Without a channel entry the bare name is used only when every free function
//! of that name passes its arguments the same way; when they differ there is no right
//! guess, and the emitter stops with an internal error instead of choosing one.

use super::CEmitter;
use crate::ast::{ExprId, FnDecl};

impl CEmitter {
    /// Record a free function's `mut`-pointer flags under its declaration (all of them,
    /// all-`false` included: "this declaration passes nothing by pointer" is an answer).
    pub(super) fn record_free_fn_inout(&mut self, f: &FnDecl, flags: Vec<bool>) {
        let decls = self.free_fn_inout_params.entry(f.name.clone()).or_default();
        if let Some(slot) = decls.iter_mut().find(|(sp, _)| *sp == f.span) {
            slot.1 = flags;
        } else {
            decls.push((f.span, flags));
        }
    }

    /// The `mut`-pointer flags of the callee of this call: the resolved declaration's, or
    /// the name's when all its declarations agree. `None` -- nothing passed by pointer.
    pub(super) fn callee_inout_flags(&self, name: &str, call_id: ExprId) -> Option<Vec<bool>> {
        let decls = self.free_fn_inout_params.get(name)?;
        let flags = match self.resolved_callees.get(&call_id) {
            // The checker named the declaration: its flags, or none when the name's
            // entries are other functions (a generic callee, another unit's function).
            Some(decl) => decls.iter().find(|(sp, _)| sp == decl).map(|(_, f)| f.clone())?,
            None => Self::agreed(name, decls.iter().map(|(_, f)| f))?,
        };
        flags.iter().any(|b| *b).then_some(flags)
    }

    /// The large-`ro` by-address flags (R3) of the callee of this call. The table holds a
    /// name only when it has ONE free declaration in the unit (a second one poisons it), so
    /// a resolved declaration other than that one has none of them.
    pub(super) fn callee_byref_flags(&self, name: &str, call_id: ExprId) -> Option<&Vec<(String, bool)>> {
        let flags = self.free_fn_byref_params.get(name)?;
        match (self.resolved_callees.get(&call_id), self.free_fn_byref_decl.get(name)) {
            (Some(decl), Some(own)) if decl != own => None,
            _ => Some(flags),
        }
    }

    /// One convention for every declaration of `name`, or `None` when none passes
    /// anything specially; two that differ are an internal error -- no guess is right.
    fn agreed<'f>(name: &str, mut all: impl Iterator<Item = &'f Vec<bool>>) -> Option<Vec<bool>> {
        let first = all.next()?.clone();
        for other in all {
            if *other != first {
                panic!(
                    "internal compiler error (registry 221.1 #1616): a call to `{name}` has no \
                     resolved callee, and the free functions named `{name}` in this compile \
                     unit pass their parameters differently (a `mut` parameter by pointer in \
                     one, by value in another). Choosing either would pass the argument the \
                     wrong way for the other; the call must take its callee from the \
                     checker's resolved-callee channel."
                );
            }
        }
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::CEmitter;

    #[test]
    fn declarations_that_agree_give_their_convention() {
        let a = vec![false, true];
        let b = vec![false, true];
        assert_eq!(CEmitter::agreed("f", [&a, &b].into_iter()), Some(vec![false, true]));
        assert_eq!(CEmitter::agreed("f", std::iter::empty()), None);
    }

    #[test]
    #[should_panic(expected = "#1616")]
    fn declarations_that_differ_are_no_guess() {
        let by_value = vec![false];
        let by_pointer = vec![true];
        CEmitter::agreed("stamp", [&by_value, &by_pointer].into_iter());
    }
}
