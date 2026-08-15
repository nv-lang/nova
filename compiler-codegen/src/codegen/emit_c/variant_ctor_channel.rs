//! №658 (реестр 221.1): emit-side integration of the `resolved_variant_ctors`
//! channel — the setter and the channel-first lookup helper, split out of
//! `emit_c.rs` per the arch-ratchet's rule (`scripts/guards/arch-ratchet.sh`
//! measures `emit_c.rs` itself; new emit-side code goes to new files, the
//! `emit_detach.rs` precedent). Child module of `codegen::emit_c` — sees
//! `CEmitter`'s private fields/methods per Rust's ancestor-module privacy
//! rule. What stays IN `emit_c.rs`: the struct field + init and the two
//! one-line consult sites (`emit_call`, inference Channel 6) — the minimal
//! integration points the ratcheted baseline records.

use super::CEmitter;

impl CEmitter {
    /// №658: feed the bare-variant-ctor channel (call-site `ExprId` → (sum
    /// simple name, variant decl index)) the checker populated. Mirrors
    /// `set_pattern_variant_types`. Read via `channel_variant_ctx`.
    pub fn set_resolved_variant_ctors(
        &mut self,
        m: &std::collections::HashMap<crate::ast::ExprId, (String, usize)>,
    ) {
        self.resolved_variant_ctors = m.clone();
    }

    /// №658 (реестр 221.1): channel-first resolution of a BARE variant-ctor
    /// call — consult the checker's `resolved_variant_ctors` channel (the
    /// call-site EXPECTED-type truth recorded by `assignable_direct`) BEFORE
    /// the name-based `debt_find_variant_ctx` heuristics in `emit_c.rs`.
    /// Returns the collision-aware sum base (the key the schema actually
    /// resolved under — the ctor emit must use it) + the variant's
    /// field-C-types on a validated hit; ANY miss (no entry, unset id,
    /// generic/mono base, no schema, variant/arity mismatch) returns `None`
    /// so the caller falls back to the untouched legacy path (strangler-fig
    /// — never a panic, never exclusive).
    pub(super) fn channel_variant_ctx(
        &self,
        call_id: crate::ast::ExprId,
        variant: &str,
        argc: usize,
    ) -> Option<(String, Vec<String>)> {
        if !call_id.is_set() {
            return None;
        }
        let (sum_name, idx) = self.resolved_variant_ctors.get(&call_id).cloned()?;
        // Collision-aware sum base — mirrors the qualified-receiver ctor path
        // (`try_emit_explicit_variant_ctor`'s `ref_type_base`).
        let base = self.ref_type_base(&sum_name, &[]);
        // Generic sums own their mono ctor path (arg boxing + instance
        // queuing) — do not intercept (mirrors `debt_find_variant_ctx`'s
        // plain-only filter and `try_emit_explicit_variant_ctor`'s guard).
        if base.contains("____")
            || self.generic_types.contains(&base)
            || self.generic_types.contains(&sum_name)
        {
            return None;
        }
        // Double lookup — mirrors `try_emit_explicit_variant_ctor`: the schema
        // may be registered under the collision-aware base OR the plain name;
        // remember WHICH key actually resolved.
        let (key, entry) = match self.sum_schema_registry.lookup_sum_schema(&base) {
            Some(e) => (base, e),
            None => (
                sum_name.clone(),
                self.sum_schema_registry.lookup_sum_schema(&sum_name)?,
            ),
        };
        // Validate the channel value against codegen's OWN schema view — a
        // mismatch (drifted index, wrong variant, wrong arity) is a miss, not
        // an error.
        let v = entry.variants.get(idx)?;
        if v.variant_name == variant && v.field_c_types.len() == argc {
            Some((key, v.field_c_types.clone()))
        } else {
            None
        }
    }
}
