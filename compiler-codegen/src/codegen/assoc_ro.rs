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

use crate::ast::{Item, Module};
use super::emit_c::CEmitter;

impl CEmitter {
    /// Emit a lazy-static global (via `emit_lazy_const`) for every
    /// `is_lazy_ro` entry across all types' `assoc_consts` in `module`.
    ///
    /// Called from `emit_module` right after the bare module-level `ro`
    /// loop — the SAME pipeline position (after `/*__GENERIC_TYPE_DEFS__*/`
    /// has been spliced in, so a mono'd generic C type — e.g. `[]u32` limbs
    /// — already has its typedef; method-receiver/generic routing tables are
    /// also fully populated by then, unlike inside `emit_type_decl`, which
    /// runs too early for that).
    ///
    /// The mangled assoc symbol (`Type_NAME`, same convention as the
    /// constexpr `const Type.NAME` path in `emit_type_decl`) is passed to
    /// `emit_lazy_const` as BOTH the Nova-level registry key AND the C-name
    /// qualifier — it cannot collide with a real Nova source identifier
    /// (Nova identifiers never contain this type-qualifier underscore) and
    /// is the exact key `emit_expr`'s Path-2-segment read arm checks against
    /// `lazy_consts` (`[M-157-assoc-ro-lazy-read]`, emit_c.rs).
    pub(super) fn emit_assoc_ro_lazy_globals(&mut self, module: &Module) -> Result<(), String> {
        for item in &module.items {
            if let Item::Type(t) = item {
                for ac in &t.assoc_consts {
                    if !ac.is_lazy_ro {
                        continue;
                    }
                    let symbol = format!("{}_{}", t.name, ac.name);
                    let ty_c = if let Some(ty) = &ac.ty {
                        self.type_ref_to_c(ty)?
                    } else {
                        self.infer_expr_c_type(&ac.value)
                    };
                    self.emit_lazy_const(&symbol, &symbol, &ty_c, &ac.value)
                        .map_err(|e| format!(
                            "assoc ro `{}.{}` codegen failed: {}",
                            t.name, ac.name, e
                        ))?;
                }
            }
        }
        Ok(())
    }
}
