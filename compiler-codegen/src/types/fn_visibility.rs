//! Registry 221.1 #1097, checker half: WHICH same-named free fn a bare name
//! denotes in a given file, when several modules of the compile unit declare it.
//!
//! `sig.fn_decls` is keyed by bare name across the whole CU. The call path
//! already narrows it twice — `priv(file)` visibility, then the caller's own
//! module shadowing every other module (№534). What it did not do is look at the
//! caller's imports: `import m.beta.{tag}` next to an unrelated, unimported
//! `m.alpha.tag(int)` somewhere in the CU left two type-compatible candidates,
//! the call went unrecorded, and the emitter had to guess by parameter C-types —
//! the same guess that collapsed `alpha.tag` and `beta.tag` into one symbol.
//! `narrow_by_fn_imports` keeps the candidates the caller's FILE imports by this
//! very name; with no such import nothing changes (strangler: only a narrowing).
//!
//! A bare name used as a VALUE (`ro f = tag`) had no callee record at all, so the
//! emitter took the thunk, the signature and the target by name.
//! `record_fn_value_callee` writes the same answer the call path would give into
//! `resolved_callees` under the Ident's own `ExprId`; the emitter reads it
//! (`codegen/emit_c/decl_module_symbol.rs`). Every consumer of that map looks up
//! CALL ids, so an Ident id is invisible to them.

use super::*;

impl<'a> TypeCheckCtx<'a> {
    /// Candidates the caller's file imports by `name` (`import path.{name}`),
    /// matched by declaring module or by the declaring file's path (relative
    /// imports). Returns `cands` unchanged when no candidate is imported so.
    pub(super) fn narrow_by_fn_imports<'b>(
        &self,
        caller: crate::diag::FileId,
        name: &str,
        cands: Vec<&'b FnDecl>,
    ) -> Vec<&'b FnDecl> {
        if cands.len() < 2 {
            return cands;
        }
        let Some(imports) = self.file_fn_imports.get(&caller) else { return cands };
        let modules = self.file_modules.borrow();
        let imported = |c: &FnDecl| {
            imports.iter().any(|(path, item)| {
                item == name
                    && (modules.get(&c.span.file_id).map_or(false, |m| m == path)
                        || self
                            .file_paths
                            .get(&c.span.file_id)
                            .map_or(false, |p| Self::path_matches_import(p, path)))
            })
        };
        let narrowed: Vec<&'b FnDecl> = cands.iter().copied().filter(|c| imported(c)).collect();
        if narrowed.is_empty() { cands } else { narrowed }
    }

    /// For a bare Ident naming a free fn with several declarations: the one it
    /// denotes in its file (file-private visibility, own module, then imports),
    /// recorded as `resolved_callees[ident.id]` when exactly one remains.
    pub(super) fn record_fn_value_callee(&self, e: &Expr, scope: &HashMap<String, TypeRef>) {
        let ExprKind::Ident(name) = &e.kind else { return };
        if scope.contains_key(name) {
            return;
        }
        let Some(decls) = self.sig.fn_decls.get(name) else { return };
        let caller = e.span.file_id;
        let visible: Vec<&FnDecl> = decls
            .iter()
            .copied()
            .filter(|f| f.receiver.is_none() && f.generics.is_empty())
            .filter(|f| !f.file_private || f.span.file_id == caller)
            .collect();
        if visible.len() < 2 {
            return;
        }
        let own: Vec<&FnDecl> = {
            let modules = self.file_modules.borrow();
            let caller_mod = modules.get(&caller);
            visible
                .iter()
                .copied()
                .filter(|c| {
                    c.span.file_id == caller
                        || (caller_mod.is_some() && modules.get(&c.span.file_id) == caller_mod)
                })
                .collect()
        };
        let pick = if own.is_empty() { self.narrow_by_fn_imports(caller, name, visible) } else { own };
        if let [only] = pick.as_slice() {
            self.resolved_callees.borrow_mut().insert(e.id, only.span);
        }
    }
}
