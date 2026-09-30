//! Plan 157 (§157, 221.1 bug-sweep; D200 amend, `spec/decisions/02-types.md`):
//! codegen for associated **ro**-values — `ro Type.NAME [Тип] = expr`,
//! declared out-of-body exactly like `const Type.NAME` (D200) but with a
//! `ro`-flavored (non-constexpr) initializer, e.g. `ro BigInt.ZERO BigInt =
//! { sign: Zero, limbs: []u32.new() }` (the owner's own proposed form —
//! `const` cannot hold a heap-allocated `Vec` field, strict-constexpr RHS).
//!
//! Kept in its OWN file (arch-ratchet precedent: `mono_method_registry.rs`)
//! specifically so this genuinely-new emission pass does not grow
//! `emit_c.rs` itself — the call site there (`emit_module`) is a single
//! line. `type_ref_to_c` / `infer_expr_c_type` / `emit_lazy_const` were
//! promoted from private to `pub(crate)` on `CEmitter` (emit_c.rs) so this
//! sibling module can reuse them verbatim — no logic duplicated, no new
//! emission machinery invented (§0/196 channel-first: this is pure REUSE of
//! the existing module-level `ro NAME = EXPR` lazy-static-global machine,
//! Plan 152.4, only keyed by the qualified `Type_NAME` symbol instead of a
//! bare name).

use crate::ast::{AssocConst, ConstDecl, Expr, Item, Module};
use std::collections::HashSet;
use super::emit_c::CEmitter;

impl CEmitter {
    // D184 amendment 2026-09-30: `ro Type.NAME` is no longer emitted here in a
    // loop of its own. It joined the bare module-level `ro` in ONE dependency
    // graph (`emit_module`, emit_c.rs), because a read between the two kinds
    // is an edge like any other and two separate loops could not order it. The
    // symbol convention is unchanged: `Type_NAME` is both the Nova-level key
    // and the C-name qualifier (`[M-157-assoc-ro-lazy-read]`).

    /// #1412: module `const` and `const Type.NAME` (deferred by `emit_type_decl` into
    /// `pending_assoc_consts`) in ONE pass, in dependency order -- keys and edges as the module-`ro`
    /// order (D184 amend): a `const Type.NAME` may read a module `const` and hold a type declared
    /// after its own, and used to be emitted inside its type's declaration, ahead of both.
    pub(crate) fn emit_consts_ordered(&mut self, module: &Module, skip: impl Fn(&ConstDecl) -> bool) -> Result<(), String> {
        let assoc = std::mem::take(&mut self.pending_assoc_consts);
        let mut nodes: Vec<(crate::diag::FileId, String, &Expr, Result<&ConstDecl, &(String, AssocConst)>)> = Vec::new();
        for item in &module.items {
            if let Item::Const(c) = item {
                if !skip(c) { nodes.push((c.span.file_id, c.name.clone(), &c.value, Ok(c))); }
            }
        }
        for a in &assoc { nodes.push((a.1.span.file_id, format!("{}.{}", a.0, a.1.name), &a.1.value, Err(a))); }
        let deps: Vec<(String, String, Vec<String>)> = nodes.iter().map(|(fid, k, v, _)| {
            let mut f = HashSet::new();
            crate::free_idents::collect_truly_free_idents(v, &mut HashSet::new(), &mut f);
            (self.init_dep_sym(*fid, k), String::new(), f.iter().map(|r| self.init_dep_sym(*fid, r)).collect())
        }).collect();
        for i in Self::topo_sort_const_inits(&deps) {
            match nodes[i].3 { Ok(c) => self.emit_const_decl(c)?, Err((tn, ac)) => self.emit_assoc_const_entry(tn, ac)? }
        }
        Ok(())
    }

    /// [fix #1361] Non-lazy (strict) assoc `const Type.NAME` entry, called
    /// from `emit_type_decl`'s (emit_c.rs) `assoc_consts` loop for every
    /// entry NOT marked `is_lazy_ro`. Registry #1361 / backlog
    /// `[M-assoc-const-arraylit-codegen-gap]`: `emit_const_expr`/
    /// `_typed` (emit_c.rs) have no `ExprKind::ArrayLit` arm — an assoc
    /// `const Type.NAME []T = [...]` RHS fell into their safety-net
    /// `_ => Err(...)` and hard-failed codegen even though the checker
    /// accepted it, while the byte-identical array literal on a MODULE-level
    /// `const NAME []T = [...]` already compiles: `emit_const_decl`
    /// (emit_c.rs) reacts to that SAME `Err` by desugaring into
    /// `emit_lazy_const`'s eager-init-at-startup global (Plan 14 Ф.2), whose
    /// `Nova_Vec____`/`_NovaFixArr_` branch (реестр №998/№1003/№1008) builds
    /// the array via `emit_expr_with_target_type` instead. This entry point
    /// gives assoc `const` the SAME two-step behaviour, reusing BOTH
    /// existing machines verbatim (no new emission code, no ArrayLit arm
    /// added anywhere): try true constexpr `.rodata` first — the D200
    /// intent, and still what every scalar/record assoc const emits,
    /// byte-identical — falling back to the lazy-static-global path (same
    /// `Type_NAME` symbol as both the Nova-level key and the C-name
    /// qualifier, exactly the convention `ro Type.NAME` uses in
    /// `emit_module`'s ordered module-value loop) only when the RHS isn't
    /// constexpr-representable.
    pub(super) fn emit_assoc_const_entry(&mut self, type_name: &str, ac: &AssocConst) -> Result<(), String> {
        let symbol = format!("{}_{}", type_name, ac.name);
        let ty_c = if let Some(ty) = &ac.ty {
            self.type_ref_to_c(ty)?
        } else {
            self.infer_expr_c_type(&ac.value)
        };
        match self.emit_const_expr_typed(&ac.value, Some(&ty_c)) {
            // #1412: the module-`const` rule -- a braced value for a POINTER type is a heap record,
            // not a `static const` initializer; it goes the lazy way like `emit_const_decl`'s.
            Ok(val) if Self::const_value_is_static(&ty_c, &val) => {
                self.line(&format!(
                    "{}const {} {} = {};",
                    self.top_level_storage(), ty_c, symbol, val
                ));
                self.var_types.insert(symbol, ty_c);
                Ok(())
            }
            other => self.emit_lazy_const(&symbol, &symbol, &ty_c, &ac.value).map_err(|e2| {
                format!(
                    "assoc const `{}.{}` codegen failed: {} (lazy fallback also failed: {})",
                    type_name, ac.name, other.err().unwrap_or_default(), e2
                )
            }),
        }
    }

    /// `const` value `val` of C type `ty_c` can be a `static const` initializer -- not a braced
    /// (record) value for a pointer type, which is a heap record and is built by lazy init.
    pub(crate) fn const_value_is_static(ty_c: &str, val: &str) -> bool {
        !(ty_c.ends_with('*') && val.starts_with('{'))
    }
}
