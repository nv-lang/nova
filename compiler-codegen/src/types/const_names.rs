//! Registry 221.1 #1488: the TYPE of a module-level named value -- a `const`
//! (annotated or not) or a module-level `ro NAME = expr` -- read by its bare
//! name in an expression.
//!
//! Before: `infer_expr_type` knew a name only from the local scope, the
//! resolved-type channel and the fn table, so `MESSAGE` in
//! `stream.write(MESSAGE)` had NO type in the checker. An untyped name is
//! accepted at every typed position permissively, so no door that decides
//! by the value's type ever ran on it: `#coerce` recorded no verdict and the
//! rewrite spliced no `.bytes()` (CC-FAIL since #1452 removed the codegen
//! guesses that used to cover it), and `ro x int = MESSAGE` passed the check.
//!
//! Now the name resolves to the declaration the READING file sees: its own
//! module's value first; otherwise another module's value only when this file
//! imports that name selectively (the CU is merged, two modules' same-named
//! values are two values). The type is the annotation, else the
//! value's own type -- except an untyped numeric/bool/char literal value,
//! which stays context-adaptive (D44/D55: such a value takes the type of its
//! position, exactly as before this fix).

use super::*;

/// One module-level named value, as `TypeCheckCtx::build` collected it.
pub(super) struct ModuleValue<'a> {
    pub module: &'a [String],
    pub file_id: crate::diag::FileId,
    pub file_private: bool,
    pub ty: Option<&'a TypeRef>,
    pub value: &'a Expr,
}

/// Name -> every module-level value of that name in the merged CU.
pub(super) fn collect_module_values(module: &Module) -> HashMap<String, Vec<ModuleValue<'_>>> {
    let peer_mod: HashMap<crate::diag::FileId, &[String]> =
        module.peer_files.iter().map(|p| (p.file_id, p.module_name.as_slice())).collect();
    let mod_of = |sp: Span| peer_mod.get(&sp.file_id).copied().unwrap_or(&[]);
    let mut out: HashMap<String, Vec<ModuleValue<'_>>> = HashMap::new();
    for item in &module.items {
        let (name, mv) = match item {
            Item::Const(cd) if !cd.name.contains('.') => (
                cd.name.clone(),
                ModuleValue {
                    module: mod_of(cd.span),
                    file_id: cd.span.file_id,
                    file_private: cd.file_private,
                    ty: cd.ty.as_ref(),
                    value: &cd.value,
                },
            ),
            Item::Let(ld) if !ld.is_ghost => {
                // An UPPER_CASE binder parses as a unit variant pattern (as in
                // `check_module`'s `env.consts` registration).
                let name = match &ld.pattern {
                    Pattern::Ident { name, .. } => name.clone(),
                    Pattern::Variant { path, kind: VariantPatternKind::Unit, .. } if path.len() == 1 => path[0].clone(),
                    _ => continue,
                };
                (
                    name,
                    ModuleValue {
                        module: mod_of(ld.span),
                        file_id: ld.span.file_id,
                        file_private: false,
                        ty: ld.ty.as_ref(),
                        value: &ld.value,
                    },
                )
            }
            _ => continue,
        };
        out.entry(name).or_default().push(mv);
    }
    out
}

impl<'a> TypeCheckCtx<'a> {
    /// The assignability walk of one module-level value: its annotated position
    /// (immutable -- a `const` and a module `ro` are both read-only bindings),
    /// then the value itself. Anything else is not a module value: no-op.
    pub(super) fn f1_check_module_value(&self, item: &Item, errors: &mut Vec<Diagnostic>) {
        let (name, ty, value, file_id) = match item {
            Item::Const(cd) => (cd.name.as_str(), cd.ty.as_ref(), &cd.value, cd.span.file_id),
            Item::Let(ld) if !ld.is_ghost => {
                let name = match &ld.pattern {
                    Pattern::Ident { name, .. } => name.as_str(),
                    Pattern::Variant { path, kind: VariantPatternKind::Unit, .. } if path.len() == 1 => path[0].as_str(),
                    _ => "_",
                };
                (name, ld.ty.as_ref(), &ld.value, ld.span.file_id)
            }
            _ => return,
        };
        let _file_scope = self.enter_file(file_id);
        let gs: GenericScope = HashMap::new();
        let mut scope: HashMap<String, TypeRef> = HashMap::new();
        if let Some(ann) = ty {
            self.f1_check_assign_let(value, ann, name, false, &gs, &scope, errors);
        }
        self.f1_expr(value, &gs, &mut scope, errors);
    }

    fn module_of_file(&self, file_id: crate::diag::FileId) -> &'a [String] {
        self.module_value_files.get(&file_id).copied().unwrap_or(&[])
    }

    /// The declaration a bare `name` read at `at` denotes, if it is a
    /// module-level value: the reading module's own one, else an imported one.
    fn module_value_decl(&self, name: &str, at: Span) -> Option<Vec<&ModuleValue<'a>>> {
        let all = self.module_values.get(name)?;
        let visible: Vec<&ModuleValue<'a>> =
            all.iter().filter(|v| !v.file_private || v.file_id == at.file_id).collect();
        let here = self.module_of_file(at.file_id);
        let own: Vec<&ModuleValue<'a>> = visible.iter().copied().filter(|v| v.module == here).collect();
        if !own.is_empty() {
            return Some(own);
        }
        // Another module's value only through a selective import of THIS file
        // (`import a.b.{NAME}`), matched by the module's last segment. Measured: a
        // CU-wide fallback typed std's locals (`n`, `j`, missing from the checker's
        // scope) by a same-named module `ro` of an unrelated test file.
        let imports = self.file_fn_imports.get(&at.file_id)?;
        let imported: Vec<&ModuleValue<'a>> = visible
            .into_iter()
            .filter(|v| imports.iter().any(|(path, item)| item == name && path.last() == v.module.last()))
            .collect();
        Some(imported)
    }

    /// The type of one module-level value, or `None` when it has none to give:
    /// unannotated with an untyped numeric/bool/char literal value (adaptive),
    /// or a value the checker cannot type.
    fn module_value_type(&self, v: &ModuleValue<'_>) -> Option<TypeRef> {
        if let Some(t) = v.ty {
            return Some(t.clone());
        }
        if is_untyped_const_expr(v.value) && !matches!(v.value.kind, ExprKind::StrLit(_)) {
            return None;
        }
        // `const A = B` reads another value: bounded, a cycle is reported elsewhere.
        if self.module_value_depth.get() > 8 {
            return None;
        }
        self.module_value_depth.set(self.module_value_depth.get() + 1);
        let t = self.infer_expr_type(v.value, &HashMap::new());
        self.module_value_depth.set(self.module_value_depth.get() - 1);
        t
    }

    /// `infer_expr_type`'s answer for a bare name that is not a local: the
    /// type of the module-level value it denotes, when that is unambiguous.
    pub(super) fn module_value_ident_type(&self, name: &str, at: Span) -> Option<TypeRef> {
        let decls = self.module_value_decl(name, at)?;
        let mut types = decls.iter().map(|v| self.module_value_type(v));
        let first = types.next()??;
        for t in types {
            if typeref_display(&t?) != typeref_display(&first) {
                return None;
            }
        }
        Some(first)
    }

    /// The qualified spellings of the same thing, `Q.NAME`: an associated
    /// constant of a type (`Type.K`, D200) or a value of an imported module
    /// (`m.K`, the import prefix naming that module). `None` when `Q` is
    /// neither, or the value has no type to give (see `module_value_type`).
    pub(super) fn qualified_value_type(&self, q: &str, name: &str, scope: &HashMap<String, TypeRef>) -> Option<TypeRef> {
        if scope.contains_key(q) {
            return None;
        }
        if let Some(td) = self.types_get_here(q) {
            let ac = td.assoc_consts.iter().find(|a| a.name == name)?;
            return self.module_value_type(&ModuleValue {
                module: &[],
                file_id: ac.span.file_id,
                file_private: false,
                ty: ac.ty.as_ref(),
                value: &ac.value,
            });
        }
        let last = self.import_prefix_to_module_last.get(q)?;
        let decls: Vec<&ModuleValue<'a>> = self
            .module_values
            .get(name)?
            .iter()
            .filter(|v| !v.file_private && v.module.last() == Some(last))
            .collect();
        let mut types = decls.iter().map(|v| self.module_value_type(v));
        let first = types.next()??;
        for t in types {
            if typeref_display(&t?) != typeref_display(&first) {
                return None;
            }
        }
        Some(first)
    }

    /// A path the parser folded from an uppercase head (`CR.name`,
    /// `Type.K.field`, `m.K`): typed as the member chain it spells, but only
    /// when its head is a module value, a type's associated constant or an
    /// import prefix -- every other path (a variant, a static) stays untyped.
    pub(super) fn value_path_type(&self, parts: &[String], span: Span, scope: &HashMap<String, TypeRef>) -> Option<TypeRef> {
        let [head, second, ..] = parts else { return None };
        if scope.contains_key(head) {
            return None;
        }
        let head_is_value = self.module_values.contains_key(head.as_str())
            || self.import_prefix_to_module_last.contains_key(head.as_str())
            || self.types_get_here(head).is_some_and(|td| td.assoc_consts.iter().any(|a| &a.name == second));
        if !head_is_value {
            return None;
        }
        let mut e = Expr::new(ExprKind::Ident(head.clone()), span);
        for p in &parts[1..] {
            e = Expr::new(ExprKind::Member { obj: Box::new(e), name: p.clone() }, span);
        }
        self.infer_expr_type(&e, scope)
    }

    /// D55 amend: a name bound to an untyped literal (`const N = "x"`) is still
    /// an untyped constant for the single-wrapper newtype rule, as the literal is.
    pub(super) fn is_untyped_module_value(&self, expr: &Expr) -> bool {
        let ExprKind::Ident(name) = &expr.kind else { return false };
        self.module_value_decl(name, expr.span)
            .is_some_and(|d| !d.is_empty() && d.iter().all(|v| v.ty.is_none() && is_untyped_const_expr(v.value)))
    }
}
