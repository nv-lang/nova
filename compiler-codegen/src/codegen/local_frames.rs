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
//! read-as-free (`free_idents::frame_reads`). A read is local when some enclosing frame reads the name ONLY
//! as bound. A name read both ways in one body (a module value read before a
//! local of the same name is introduced) keeps the module resolution -- so this
//! never answers "local" for a read that is in fact global.
//!
//! Kept out of emit_c.rs (arch-ratchet precedent: `assoc_ro.rs`).

use std::collections::HashSet;
use super::emit_c::CEmitter;

impl CEmitter {
    /// #1441: a capture body (handler op, closure) begins. Its reads that are bound where they stand -- a local
    /// declared before the read, possibly shadowing a capture of the same name -- read the LOCAL, not the captured
    /// box (`var_boxed`); and with captures unpacked as C locals of the same names, the body goes into its own C
    /// block so such a local is a legal C shadow and a read before it still sees the capture.
    pub(crate) fn enter_capture_body(&mut self, params: &[&str], brace: bool, walk: impl FnOnce(&mut HashSet<String>, &mut HashSet<String>)) -> (HashSet<crate::ast::ExprId>, bool) {
        let ids = crate::free_idents::bound_read_ids(params, walk);
        let saved = self.local_read_ids.clone();
        self.local_read_ids.extend(ids);
        if brace { self.line("{"); }
        (saved, brace)
    }

    pub(crate) fn leave_capture_body(&mut self, (saved, brace): (HashSet<crate::ast::ExprId>, bool)) {
        if brace { self.line("}"); }
        self.local_read_ids = saved;
    }

    /// #1441: does handler op `m` read `name` free somewhere -- before a local of that name is declared?
    pub(crate) fn read_before_bound(m: &crate::ast::HandlerMethod, name: &str) -> bool {
        let (mut bound, mut free): (HashSet<String>, HashSet<String>) = (m.params.iter().map(|p| p.name.clone()).collect(), HashSet::new());
        match &m.body {
            crate::ast::HandlerMethodBody::Expr(e) => crate::free_idents::collect_truly_free_idents(e, &mut bound, &mut free),
            crate::ast::HandlerMethodBody::Block(b) => crate::free_idents::collect_truly_free_idents_block(b, &mut bound, &mut free),
        }
        free.contains(name)
    }

    /// #1421: what a handler literal or closure nested in a body reads -- minus its own params and locals -- is
    /// read by the body that holds it: a capture of a capture. `collect_idents_expr` stopped at such a node, so an
    /// op body whose nested literal alone read `seen` never captured it, and the inner literal wrote `&seen` for
    /// a name that is no C local there.
    pub(crate) fn collect_idents_nested(expr: &crate::ast::Expr, out: &mut Vec<String>) {
        use crate::ast::{ClosureBody, ExprKind, FnBody, HandlerMethodBody};
        let (mut inner, mut own) = (Vec::new(), HashSet::new());
        match &expr.kind {
            ExprKind::HandlerLit { methods, .. } => for m in methods {
                let (names, mut bound): (_, HashSet<String>) = (Self::collect_idents_in_handler_method(m), m.params.iter().map(|p| p.name.clone()).collect());
                match &m.body {
                    HandlerMethodBody::Expr(e) => Self::collect_bound_names_expr(e, &mut bound),
                    HandlerMethodBody::Block(b) => Self::collect_bound_names_block(b, &mut bound),
                }
                out.extend(names.into_iter().filter(|n| !bound.contains(n)));
            },
            ExprKind::ClosureLight { params, body } => {
                match body {
                    ClosureBody::Expr(e) => { Self::collect_idents_expr(e, &mut inner); Self::collect_bound_names_expr(e, &mut own) }
                    ClosureBody::Block(b) => { Self::collect_idents_block(b, &mut inner); Self::collect_bound_names_block(b, &mut own) }
                }
                own.extend(params.iter().map(|p| p.name.clone()));
            }
            ExprKind::ClosureFull(c) => {
                match &c.body {
                    FnBody::Expr(e) => { Self::collect_idents_expr(e, &mut inner); Self::collect_bound_names_expr(e, &mut own) }
                    FnBody::Block(b) => { Self::collect_idents_block(b, &mut inner); Self::collect_bound_names_block(b, &mut own) }
                    FnBody::External => {}
                }
                own.extend(c.params.iter().map(|p| p.name.clone()));
            }
            _ => {}
        }
        out.extend(inner.into_iter().filter(|n| !own.contains(n)));
    }

    /// Is a read of `name` here a local binding? See the module doc.
    pub(crate) fn is_local_read(&self, name: &str) -> bool {
        self.local_frames.iter().any(|(bound, free)| bound.contains(name) && !free.contains(name))
    }

    /// #1410: a module value's C type -- the flat `var_types` entry AND the value's own record.
    pub(crate) fn note_module_value(&mut self, name: &str, ty_c: &str) {
        self.var_types.insert(name.to_string(), ty_c.to_string());
        self.module_value_tys.insert(name.to_string(), ty_c.to_string());
    }

    /// An emitted fn/test body begins: the module values' own types (#1410), then the body's frame (#1397).
    pub(crate) fn enter_body(&mut self, frame: (HashSet<String>, HashSet<String>)) {
        self.reset_module_value_types();
        self.local_frames.push(frame);
    }

    /// An emitted fn/test body ends: its frame goes, and its locals give the module values' names back.
    pub(crate) fn leave_body(&mut self) {
        self.local_frames.pop();
        self.reset_module_value_types();
    }

    /// #1410: `var_types` is ONE flat table for module values and the locals of every body, keyed by the bare
    /// name, and a body's local never gave its entry back: after a std fn with `ro p = ...` of a value type, a
    /// module `ro p Pt` read as `p.y` in a later fn was typed by the dead local (`.` on a pointer, CC-FAIL). Every
    /// fn and test body now starts and ends with the module values' own types -- a local lives in its body only.
    /// Named limit: a body that reads a module value AFTER declaring a local of the same name (the mixed case of
    /// #1397) still sees the local's type there.
    pub(crate) fn reset_module_value_types(&mut self) {
        for (n, t) in &self.module_value_tys {
            if self.var_types.get(n) != Some(t) { self.var_types.insert(n.clone(), t.clone()); }
        }
    }

    /// #1399: is a read of `name` in `file_id` a MODULE value with a C symbol of its own (a lazy `ro`, a
    /// module-private or qualified `const`) rather than a local? Such a read is not a closure capture: the body
    /// reads the symbol, and the capture's env init wrote the bare source name -- undeclared in C.
    pub(crate) fn is_module_value_read(&self, file_id: crate::diag::FileId, name: &str) -> bool {
        !self.is_local_read(name) && (self.lazy_const_sym(file_id, name).is_some() || self.init_dep_sym(file_id, name) != name)
    }
}
