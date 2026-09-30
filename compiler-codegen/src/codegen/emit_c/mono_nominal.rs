//! Registry 221.1 #895: a generic instantiation over a NEWTYPE keeps the
//! newtype's identity in the mono layer.
//!
//! The defect. The mono subst the call site hands to an instance is a list of
//! C-strings (`I → "nova_int"`), lifted verbatim into `ResolvedType::Raw` (the
//! A1′ `lift_c_name` debt). A newtype lowers to its representation
//! (`type FnRow int` → `nova_int`, `type Wrap Cell` → `Nova_Cell*`), so by the
//! time the body is emitted the name `FnRow` is gone: `I.from_ordinal(n)`
//! dispatched to `int.from_ordinal` (E_UNKNOWN_STATIC_METHOD) or to
//! `Nova_Cell_static_from_ordinal` (undefined at link), and the instance name
//! was keyed by the C-string too, so `mk[FnRow]` and `mk[int]` were ONE
//! instance.
//!
//! The fix reads the checker channel (`node_substs[call_id]`, via the A1″ RT
//! slots) instead of re-deriving from the C-string: the channel already says
//! `I = Named{FnRow}`. Owner's direction (2026-09-18): «FnRow must reach the
//! NAME, and the parameter stays int» — the instance NAME carries the Nova
//! type, the C ABI stays the representation (no wrapper, no call cost).
//!
//! Scope: only slots whose nominal identity the C lowering ERASES (the Nova
//! name read back from the C-string differs from the channel's). Every other
//! slot keeps the `Raw` debt and the old name — byte-identical output. A slot
//! is adopted only when its RT lowers to the exact C-string the legacy path
//! produced (the `subst_map_adopt_rt` guard), so the ABI cannot move.
//!
//! Not covered here (named in the registry row): the instance of a generic
//! TYPE (`IdxVec[FnRow, T]`) — its args are keyed by C-string in
//! `generic_type_instance_info`, a different producer.

use super::CEmitter;
use crate::types::ResolvedType;

impl CEmitter {
    /// #895: the slots of a mono subst whose nominal type the C lowering erases,
    /// as `(name, Named RT)`. `pairs` is the legacy C-string subst, `slots` the
    /// A1″ channel slots for the same call. Empty for every non-newtype call.
    pub(super) fn mono_nominal_slots(
        &self,
        pairs: &[(String, String)],
        slots: &[(String, Option<ResolvedType>)],
    ) -> Vec<(String, ResolvedType)> {
        pairs
            .iter()
            .filter_map(|(k, c)| {
                let mut rt = slots.iter().find(|(n, _)| n == k)?.1.clone()?;
                // A nested call forwarding the ENCLOSING body's own param (`inner[T](x)`
                // inside `outer[T]`) names that param; read it through the enclosing
                // subst, so an adopted newtype travels one level down.
                if let ResolvedType::TypeParam(n) | ResolvedType::Named { name: n, .. } = &rt {
                    if let Some(outer) = self.current_type_subst.get(n.as_str()) {
                        rt = outer.clone();
                    }
                }
                let ResolvedType::Named { name, module, args } = &rt else { return None };
                if !args.is_empty() || !module.is_empty() {
                    return None;
                }
                // Never adopt a slot that names a param of this very subst (`T → Named{T}`
                // recurses forever in the lowering; see `subst_map_adopt_rt`).
                if pairs.iter().any(|(p, _)| p == name) || self.current_type_subst.contains_key(name) {
                    return None;
                }
                if Self::debt_nova_type_name_from_c(c) == *name {
                    return None;
                }
                if self.resolved_type_to_c(&rt).ok().as_deref() != Some(c.as_str()) {
                    return None;
                }
                Some((k.clone(), rt))
            })
            .collect()
    }

    /// #895: the instance name — `compute_mono_name`, except that an adopted
    /// nominal slot contributes `Nova_<Newtype>` instead of its representation,
    /// so `mk[FnRow]` (`mk____Nova_FnRow`) and `mk[int]` (`mk____nova_int`) are
    /// distinct instances. Identical to `compute_mono_name` when `nominal` is empty.
    pub(super) fn compute_mono_name_nominal(
        base_c_name: &str,
        pairs: &[(String, String)],
        nominal: &[(String, ResolvedType)],
    ) -> String {
        let keyed: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, c)| match nominal.iter().find(|(n, _)| n == k) {
                Some((_, ResolvedType::Named { name, .. })) => (k.clone(), format!("Nova_{}", name)),
                _ => (k.clone(), c.clone()),
            })
            .collect();
        Self::compute_mono_name(base_c_name, &keyed)
    }

    /// #895: after a `register_mono_*` call, put the nominal RT in place of the
    /// `Raw` C-string in the queued instance's subst, so its body sees
    /// `I = Named{FnRow}`. A no-op when `nominal` is empty or the instance was
    /// already queued earlier (it was then queued with the same subst).
    pub(super) fn mono_worklist_adopt_nominal(
        &mut self,
        mono_name: &str,
        nominal: &[(String, ResolvedType)],
    ) {
        if nominal.is_empty() {
            return;
        }
        if let Some(entry) = self.mono_worklist.iter_mut().find(|e| e.2 == mono_name) {
            for (k, v) in entry.1.iter_mut() {
                if let Some((_, rt)) = nominal.iter().find(|(n, _)| n == k) {
                    *v = rt.clone();
                }
            }
        }
    }

    /// #895: the Nova type name a type parameter stands for in the current mono
    /// body, for static dispatch `T.method(..)`: the nominal name when the subst
    /// carries a real `Named` (adopted above), else the name read back from the
    /// C-string (the pre-#895 path, unchanged). Only a name the lowering erases
    /// (a newtype/alias, registered in `type_aliases`) takes the nominal path, so
    /// any other `Named` the A1″ seeders adopted dispatches exactly as before.
    pub(super) fn subst_static_recv_name(&self, key: &str) -> Option<String> {
        match self.current_type_subst.get(key)? {
            ResolvedType::Named { name, module, args }
                if args.is_empty() && module.is_empty() && self.type_aliases.contains_key(name.as_str()) =>
            {
                Some(name.clone())
            }
            rt => Some(Self::debt_nova_type_name_from_c(&self.subst_val_c(rt))),
        }
    }
}
