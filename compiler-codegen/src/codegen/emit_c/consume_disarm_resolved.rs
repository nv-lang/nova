//! Registry 221.1 #1100: D432 §4 disarm points ask the checker's RESOLVED
//! callee of THIS call, not the set of declarations under its name.
//!
//! Before this file the auto-`@cleanup` flag of a binding was disarmed at a
//! call argument whose position was `consume` on AT LEAST ONE overload of the
//! callee's name (a union by name). A second, never-called overload
//! `fn use(consume x Boom)` then disarmed `g` in `use(g, 1)`, although the
//! call resolved to `fn use(x Boom, tag int)`, which takes `g` by view:
//! nobody ran the cleanup, a silent leak. The receiver form (`X.m()`, point 2)
//! had the same shape: "some overload of `m` on `X`'s type consumes its
//! receiver".
//!
//! Now the answer comes from the 196 channel (`resolved_callees[call.id]`,
//! the declaration `Span` the checker picked) through the table built here
//! from every declaration of the CU. When the channel has no entry for the
//! call, the declarations under the name are asked, and only a UNANIMOUS
//! "consumes" disarms (one declaration, or all of them consume there).
//! Declarations that disagree without a resolved callee are the AMBIGUOUS
//! cell: the flag stays armed, see `resolved_consume_positions`.

use super::CEmitter;
use crate::ast::{Expr, ExprKind, FnDecl, Item, Module};
use crate::diag::Span;
use std::collections::{HashMap, HashSet};

/// Per-declaration consume modes and the declarations under each name.
#[derive(Default)]
pub(super) struct DeclConsumeModes {
    /// declaration span -> (receiver is `consume`, consume param positions).
    by_decl: HashMap<Span, (bool, HashSet<usize>)>,
    /// free-fn name -> its declarations.
    free: HashMap<String, Vec<Span>>,
    /// `(receiver type, method)` -> its declarations (positions exclude `@`).
    method: HashMap<(String, String), Vec<Span>>,
}

impl DeclConsumeModes {
    /// A free fn of this name has a `consume` position on some declaration
    /// (the variant-ctor arm's "a colliding free fn wins" test).
    pub(super) fn free_fn_has_consume_position(&self, name: &str) -> bool {
        self.free.get(name).map_or(false, |ds| {
            ds.iter().any(|d| self.by_decl.get(d).map_or(false, |m| !m.1.is_empty()))
        })
    }
}

impl CEmitter {
    /// Pre-pass: record every declaration's consume modes (module + peers).
    pub(super) fn collect_decl_consume_modes(&mut self, module: &Module) {
        let t = &mut self.decl_consume_modes;
        let mut add = |f: &FnDecl| {
            let positions: HashSet<usize> = f.params.iter().enumerate()
                .filter(|(_, p)| p.consume).map(|(i, _)| i).collect();
            let recv_consume = f.receiver.as_ref().map_or(false, |r| r.consume);
            t.by_decl.insert(f.span, (recv_consume, positions));
            match &f.receiver {
                None => t.free.entry(f.name.clone()).or_default().push(f.span),
                Some(r) => t.method.entry((r.type_name.clone(), f.name.clone()))
                    .or_default().push(f.span),
            }
        };
        let items = module.items.iter()
            .chain(module.peer_files.iter().flat_map(|pf| pf.items_here.iter()));
        for item in items {
            if let Item::Fn(f) = item { add(f); }
        }
    }

    /// The `(receiver type, method)` key of a Member callee (`recv.m(..)`).
    fn consume_method_key(&self, obj: &Expr, method: &str) -> (String, String) {
        let recv_ty = match &obj.kind {
            ExprKind::Ident(n) => self.var_types.get(n).cloned()
                .unwrap_or_else(|| self.infer_expr_c_type(obj)),
            _ => self.infer_expr_c_type(obj),
        };
        let t = self.debt_strip_nova_trim_start(&recv_ty);
        let t = t.strip_prefix("NovaValue_").map(|s| s.to_string()).unwrap_or(t);
        (t, method.to_string())
    }

    /// The consume modes of the callee of `e` (a Call): the checker's resolved
    /// declaration when the channel has one, otherwise EVERY declaration under
    /// the name (the caller then acts only on a unanimous answer). Empty = no
    /// known declaration (intrinsic, extern of another CU, value callee).
    fn callee_consume_modes(&self, e: &Expr) -> Vec<&(bool, HashSet<usize>)> {
        let ExprKind::Call { func, .. } = &e.kind else { return Vec::new() };
        let t = &self.decl_consume_modes;
        if let Some(m) = self.resolved_callees.get(&e.id).and_then(|d| t.by_decl.get(d)) {
            return vec![m];
        }
        let decls = match &func.kind {
            ExprKind::Ident(name) => t.free.get(name),
            ExprKind::Path(path) => path.last().and_then(|name| t.free.get(name)),
            ExprKind::Member { obj, name } => t.method.get(&self.consume_method_key(obj, name)),
            _ => None,
        };
        decls.map_or(Vec::new(), |ds| ds.iter().filter_map(|d| t.by_decl.get(d)).collect())
    }

    /// D432 §4 point 3: the argument positions of `e` that pass ownership to
    /// the callee this call resolves to.
    /// Without a resolved callee a position counts only when EVERY declaration
    /// consumes there; declarations that disagree leave the flag armed.
    pub(super) fn resolved_consume_positions(&self, e: &Expr) -> Option<HashSet<usize>> {
        let modes = self.callee_consume_modes(e);
        let (first, rest) = modes.split_first()?;
        let out: HashSet<usize> = first.1.iter().copied()
            .filter(|i| rest.iter().all(|m| m.1.contains(i))).collect();
        (!out.is_empty()).then_some(out)
    }

    /// D432 §4 point 2: `X.m(..)` resolves to a declaration with a `consume`
    /// receiver.
    pub(super) fn resolved_consumes_receiver(&self, e: &Expr) -> bool {
        let modes = self.callee_consume_modes(e);
        !modes.is_empty() && modes.iter().all(|m| m.0)
    }
}
