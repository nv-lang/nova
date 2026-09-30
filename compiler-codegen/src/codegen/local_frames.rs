//! Registry 221.1 #1397: is a name read here a LOCAL binding, or the module
//! value / free function of the same name?
//!
//! The Ident arm of `emit_expr` resolved a read by NAME alone -- a lazy
//! module `ro`, a module-private `const`, a colliding export, a free fn -- so
//! a parameter, closure parameter, `ro`/`mut` or pattern binder with the same
//! name as a module value read the MODULE value, silently (`plus_one(10)`
//! printed 4 for 11 when `ro k = three()` existed). The emitter's `var_types`
//! is flat and holds module values beside locals, so it cannot tell.
//!
//! Each emitted body (fn, test, closure) pushes a frame: the names it reads,
//! split by the scope-aware `collect_truly_free_idents` into read-as-bound and
//! read-as-free. A read is local when some enclosing frame reads the name ONLY
//! as bound. A name read both ways in one body (a module value read before a
//! local of the same name is introduced) keeps the module resolution -- so this
//! never answers "local" for a read that is in fact global.
//!
//! Kept out of emit_c.rs (arch-ratchet precedent: `assoc_ro.rs`).

use std::cell::RefCell;
use std::collections::HashSet;
use super::emit_c::CEmitter;

thread_local! {
    /// Names the collector saw read as BOUND while a frame is being computed.
    static BOUND_READS: RefCell<Option<HashSet<String>>> = const { RefCell::new(None) };
}

/// Called by the collector's Ident arm for a read of a name that is bound in scope.
pub(crate) fn record_bound_read(name: &str) {
    BOUND_READS.with(|r| if let Some(set) = r.borrow_mut().as_mut() { set.insert(name.to_string()); });
}

impl CEmitter {
    /// A frame for a body whose parameters are `params`: (read-as-bound, read-as-free).
    pub(crate) fn local_frame(
        params: &[&str],
        walk: impl FnOnce(&mut HashSet<String>, &mut HashSet<String>),
    ) -> (HashSet<String>, HashSet<String>) {
        let mut bound: HashSet<String> = params.iter().map(|p| p.to_string()).collect();
        let mut free = HashSet::new();
        BOUND_READS.with(|r| *r.borrow_mut() = Some(HashSet::new()));
        walk(&mut bound, &mut free);
        (BOUND_READS.with(|r| r.borrow_mut().take()).unwrap_or_default(), free)
    }

    /// Is a read of `name` here a local binding? See the module doc.
    pub(crate) fn is_local_read(&self, name: &str) -> bool {
        self.local_frames.iter().any(|(bound, free)| bound.contains(name) && !free.contains(name))
    }
}
