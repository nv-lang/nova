//! Registry 221.1 #1343 (D84): a monomorph of a GENERIC free function belongs
//! to the declaration the checker chose, never to its bare name.
//!
//! `mono_fn_decls` is keyed by the bare name, so of two generic overloads
//! (`pick[T](consume b Bag[T])` / `pick[T consume](consume b Bag[T])`, or any
//! pair differing by parameter mode, parameter types or arity) the second
//! registration overwrote the first: every call monomorphized the surviving
//! body under one symbol, and the program silently ran the other overload.
//!
//! The same three rules as #1097 (`decl_module_symbol.rs`), keyed by the
//! declaration's span: every generic declaration of a name is kept
//! (`note_generic_free_fn`); a call takes the declaration the checker resolved
//! it to (`resolved_callees`, `mono_fn_decl_for_call`); and a name with two or
//! more generic declarations gives each one its own symbol base
//! (`mono_fn_base_c_name`), so `pick#0[int]` and `pick#1[int]` never share a
//! C symbol. The worklist drain then takes the declaration registered for the
//! INSTANCE (`mono_fn_decl_for_instance`), not the one registered for the
//! name. A name with one generic declaration is byte-identical to before.

use std::collections::HashMap;

use super::CEmitter;
use crate::ast::{ExprId, FnDecl};
use crate::diag::{FileId, Span};

/// Every generic free-fn declaration per name, in registration order, and the
/// declaration each monomorph instance was registered from.
#[derive(Default)]
pub(super) struct GenericOverloads {
    by_name: HashMap<String, Vec<FnDecl>>,
    by_instance: HashMap<String, FnDecl>,
}

impl CEmitter {
    /// Register generic free fn `f`: the bare-name entry `mono_fn_decls` keeps
    /// for its other readers, plus `f` itself among the name's declarations.
    pub(super) fn note_generic_free_fn(&mut self, f: &FnDecl) {
        self.mono_fn_decls.insert(f.name.clone(), f.clone());
        let decls = self.generic_overloads.by_name.entry(f.name.clone()).or_default();
        if !decls.iter().any(|d| d.span == f.span) {
            decls.push(f.clone());
        }
    }

    /// The generic declarations of `name` when there are two or more of them.
    fn generic_overload_set(&self, name: &str) -> Option<&[FnDecl]> {
        self.generic_overloads.by_name.get(name).filter(|d| d.len() > 1).map(|d| d.as_slice())
    }

    /// The FnDecl to monomorphize for the call `call_id` to generic `name`: the
    /// declaration the checker resolved the call to when the name has several
    /// generic declarations; else the caller file's own `priv(file)` generic
    /// (Facet-B D307 §1/§3 — the only `priv(file)` decl a caller may reference);
    /// else the bare-name entry.
    pub(super) fn mono_fn_decl_for_call(
        &self, caller_fid: FileId, name: &str, call_id: Option<ExprId>,
    ) -> Option<FnDecl> {
        let resolved: Option<Span> = call_id.and_then(|id| self.resolved_callees.get(&id).copied());
        if let (Some(decls), Some(sp)) = (self.generic_overload_set(name), resolved) {
            if let Some(d) = decls.iter().find(|d| d.span == sp) {
                return Some(d.clone());
            }
        }
        self.file_priv_free_fn_decls
            .get(&(caller_fid, name.to_string()))
            .filter(|d| !d.generics.is_empty())
            .cloned()
            .or_else(|| self.mono_fn_decls.get(name).cloned())
    }

    /// Symbol base of the monomorphs of generic declaration `f`. One generic
    /// declaration of the name: the by-name base, as before. Several: each
    /// declaration's own base (`decl_base_c_name`, #1097) plus its position
    /// among the name's generic declarations, so two overloads instantiated at
    /// the same type arguments stay two C functions.
    pub(super) fn mono_fn_base_c_name(&self, f: &FnDecl) -> String {
        match self.generic_overload_set(&f.name) {
            Some(decls) => {
                let idx = decls.iter().position(|d| d.span == f.span).unwrap_or(0);
                format!("{}__ov{}", self.decl_base_c_name(f), idx)
            }
            None => self.free_fn_c_name(&f.name),
        }
    }

    /// Remember which declaration monomorph `mono_name` instantiates.
    pub(super) fn note_mono_fn_instance(&mut self, mono_name: &str, f: &FnDecl) {
        self.generic_overloads.by_instance.insert(mono_name.to_string(), f.clone());
    }

    /// The declaration monomorph `mono_name` was registered from; the bare-name
    /// entry only for an instance registered without one.
    pub(super) fn mono_fn_decl_for_instance(&self, name: &str, mono_name: &str) -> Option<FnDecl> {
        self.generic_overloads
            .by_instance
            .get(mono_name)
            .or_else(|| self.mono_fn_decls.get(name))
            .cloned()
    }
}
