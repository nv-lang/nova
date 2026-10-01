//! Registry 221.1 #1338, emitter half: the payload layout of a user GENERIC
//! sum's instance, from the type the CHECKER gave the scrutinee.
//!
//! A pattern binding reads its C type from `sum_schemas[<instance>]`
//! (`pattern_bind_typed`) or `sum_schema_registry` (`collect_pattern_inner_bindings`).
//! Both are filled only when the instance's struct is EMITTED, which happens
//! after the bodies that first name it are drained. A `match b { Som(v) => .. }`
//! over `b Opt[int]` in a function emitted before that point missed the
//! instance, fell back to `find_variant_compat("Som")` -- the erased template,
//! payload `void*` -- and the program read an `int` as a pointer (a segfault, or
//! `p.x` on the `void*` printed as `0`). Whether it worked depended on the order
//! of declarations elsewhere in the file (a signature naming `Opt[int]` earlier
//! registered the instance in time).
//!
//! `ensure_channel_sum_schema` takes the scrutinee's `resolved_types` entry --
//! `Opt[int]` -- and registers the instance's layout from the template, each
//! payload lowered through `resolved_type_to_c` after substituting the
//! checker's type arguments. Registration is idempotent and the later struct
//! emission writes the same entry again; nothing is re-derived from a C name.

use super::CEmitter;
use crate::ast::{SumVariantKind, TypeDeclKind};
use crate::types::ResolvedType as R;

impl CEmitter {
    /// See the module doc. A miss (no channel entry, not a generic user sum,
    /// an argument the printer cannot lower) leaves everything unchanged.
    pub(super) fn ensure_channel_sum_schema(&mut self, scrutinee: &crate::ast::Expr) {
        if let Some(rt) = scrutinee.id.is_set().then(|| self.resolved_types.get(&scrutinee.id).cloned()).flatten() {
            self.ensure_sum_schema_rt(&rt, 0);
        }
    }

    fn ensure_sum_schema_rt(&mut self, rt: &R, depth: usize) {
        let rt = match rt { R::Readonly(inner) => inner.as_ref(), other => other };
        let R::Named { name, args, .. } = rt else { return };
        // A generic body's own `Opt[T]` is no instance -- lowering it would queue `Opt____Nova_T_p`.
        if args.is_empty() || depth > 8 || args.iter().any(|a| self.rt_is_erased_stub(a, true)) {
            return;
        }
        // Nested instances first: `Opt[Opt[int]]`'s payload is itself matched.
        for a in args {
            self.ensure_sum_schema_rt(a, depth + 1);
        }
        let Some(tpl) = self.generic_type_templates.get(name).cloned() else { return };
        let TypeDeclKind::Sum(variants) = &tpl.kind else { return };
        if tpl.generics.len() != args.len() {
            return;
        }
        let Ok(c) = self.resolved_type_to_c(rt) else { return };
        let mangled = c.trim_end_matches('*').to_string();
        let key = Self::debt_strip_nova_prefix(&mangled).to_string();
        if self.sum_schemas.contains_key(&key) {
            return;
        }
        let subst: std::collections::HashMap<&str, &R> =
            tpl.generics.iter().map(|g| g.name.as_str()).zip(args.iter()).collect();
        let mut schema = std::collections::HashMap::new();
        for v in variants {
            let tys: Vec<&crate::ast::TypeRef> = match &v.kind {
                SumVariantKind::Unit => Vec::new(),
                SumVariantKind::Tuple(tys) => tys.iter().collect(),
                SumVariantKind::Record(fields) => fields.iter().map(|f| &f.ty).collect(),
            };
            let mut field_c = Vec::with_capacity(tys.len());
            for ty in tys {
                let Ok(fc) = self.resolved_type_to_c(&subst_rt(&R::from_type_ref(ty), &subst)) else { return };
                field_c.push(fc);
            }
            schema.insert(v.name.clone(), field_c);
        }
        let order: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
        self.sum_schema_registry.register_user_sum(
            &key,
            &schema,
            &mangled,
            super::super::sum_schema_registry::SumAbi::PointerErrorLike,
            &order,
        );
        self.sum_schemas.insert(key, schema);
    }
}

/// The template's payload type with the sum's own parameters replaced by the
/// checker's arguments (a parameter is a bare `Named` or a `TypeParam` leaf).
fn subst_rt(rt: &R, subst: &std::collections::HashMap<&str, &R>) -> R {
    match rt {
        R::TypeParam(n) => subst.get(n.as_str()).map(|r| (*r).clone()).unwrap_or_else(|| rt.clone()),
        R::Named { name, module, args } if module.is_empty() && args.is_empty() => {
            subst.get(name.as_str()).map(|r| (*r).clone()).unwrap_or_else(|| rt.clone())
        }
        R::Named { name, module, args } => R::Named {
            name: name.clone(),
            module: module.clone(),
            args: args.iter().map(|a| subst_rt(a, subst)).collect(),
        },
        R::Array(inner) => R::Array(Box::new(subst_rt(inner, subst))),
        R::FixedArray(n, inner) => R::FixedArray(*n, Box::new(subst_rt(inner, subst))),
        R::Readonly(inner) => R::Readonly(Box::new(subst_rt(inner, subst))),
        R::TypedPtr(m, inner) => R::TypedPtr(m.clone(), Box::new(subst_rt(inner, subst))),
        R::Tuple(items) => R::Tuple(items.iter().map(|i| subst_rt(i, subst)).collect()),
        R::Func { params, ret, effects } => R::Func {
            params: params.iter().map(|p| subst_rt(p, subst)).collect(),
            ret: Box::new(subst_rt(ret, subst)),
            effects: effects.clone(),
        },
        other => other.clone(),
    }
}
