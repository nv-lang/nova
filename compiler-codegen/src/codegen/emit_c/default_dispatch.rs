//! Registry 221.1 #1414 -- a protocol DEFAULT method called on a VALUE receiver.
//!
//! The call site synthesizes the default body (`try_synthesize_default_method`)
//! for the receiver's type, named from the receiver's C type. It stripped only
//! the heap prefix `Nova_`, so a value record / payload-less sum
//! (`NovaValue_X`) or a named tuple (`NovaTuple_X`) produced no type name the
//! registry knows; synthesis missed and the call fell through to a FIELD
//! access (`a.near` -- "no member named 'near' in 'struct NovaValue_X'").
//! A heap receiver was fine.
//!
//! The receiver ABI of a value type's method is its pointer carrier (D226), so
//! the synthesized body takes `NovaValue_X* nova_self` (its `Self` positions
//! are the value since #1395) and the call passes the receiver by address,
//! through the same `prepare_method_recv` every other value-receiver method
//! call uses (an rvalue receiver is hoisted to a temporary there).

use crate::ast::TypeRef;

use super::CEmitter;

impl CEmitter {
    /// The C return type of `type_name.method(..)` when it is a protocol
    /// DEFAULT method the type opted into (`#impl(P)`, the same gate the call
    /// site's synthesis applies) and that has not been synthesized yet.
    ///
    /// The checker writes no channel for such a call, and the synthesis
    /// registers `(type, method)` only when the CALL is emitted -- after the
    /// result's temporary was typed. The return inference (`infer_call_ret_c`,
    /// branch B03) answered from the FIRST protocol anywhere with a default
    /// method of that name, and lowered its `Self` through whatever `Self`
    /// the emitter had bound last -- another type's (`P1414Lv.Mid.pick(..)`
    /// declared `NovaTuple_P1414Tv`, #1413's class). Now: only protocols the
    /// type opted into, and `Self` is the receiver's type (#1395). A return
    /// the lowering cannot answer stays `None`.
    pub(super) fn default_method_ret_c(&self, type_name: &str, method: &str) -> Option<String> {
        let opted = self.type_impl_protocols.get(type_name)?;
        let ret = self
            .protocol_method_registry
            .iter()
            .filter(|(p, _)| opted.contains(p.as_str()))
            .flat_map(|(_, (_, ms))| ms.iter())
            .find(|m| m.name == method && m.default_body.is_some())
            .map(|m| m.return_type.clone())?;
        match ret {
            None => Some("nova_unit".to_string()),
            Some(TypeRef::Named { path, generics, span }) if path.len() == 1 && path[0] == "Self" && generics.is_empty() => self
                .type_ref_to_c(&TypeRef::Named { path: vec![type_name.to_string()], generics, span })
                .ok(),
            Some(t) => self.type_ref_to_c(&t).ok(),
        }
    }

    /// `(type name, receiver C type for the synthesized body, is value)` for
    /// a receiver whose materialized C type is `obj_ty`.
    pub(super) fn default_method_recv(&self, obj_ty: &str) -> (String, String, bool) {
        if Self::is_value_struct_val(obj_ty) {
            let name = obj_ty
                .strip_prefix("NovaTuple_")
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| Self::debt_strip_value_prefix_or_nova_trim_start(obj_ty));
            (name, format!("{}*", obj_ty), true)
        } else {
            (self.debt_strip_nova_trim_start_no_ws(obj_ty), obj_ty.to_string(), false)
        }
    }
}
