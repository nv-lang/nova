//! Registry 221.1 #1405 -- the `nova_opt_eq_<T>` of a COMPOSITE by-value
//! payload (a value record, a named or anonymous tuple, `nova_str`, a nested
//! `NovaOpt_`) gets its prototype EARLY and its body LATE.
//!
//! The typedef `NovaOpt_<T>` has to stand in the early zone
//! (`novaopt_typedefs_buf`): heap structs and other early helpers name it. Its
//! eq body used to stand there too, built by `emit_field_eq` under a flag that
//! told it "no struct body and no method prototype is visible yet". That
//! context cannot compare a field of a heap aggregate correctly, and it failed
//! in the two ways the context allows:
//!
//!   * when the field type's `@equal` was already registered, the body CALLED
//!     it -- before its prototype: C declared it implicitly and then met the
//!     `static` definition (Carina's `Option[TypeDef]` with `kind TypeKind`
//!     under `#impl(Equal)` -- the self-build stopped here);
//!   * when it was not yet registered, the body compared the field by POINTER
//!     IDENTITY: `Some(Def{kind: K.A}) == Some(Def{kind: K.A})` answered false
//!     for two separately built values.
//!
//! Both are one fact -- the comparison was built where it cannot be built. The
//! sibling branches of `register_novaopt_decl` (value-generic payloads,
//! heap-aggregate payloads) already split the helper; this brings the last
//! composite case to the same shape: a prototype next to the typedef, the body
//! in `novaopt_eq_fns_buf` (spliced after struct bodies AND method prototypes),
//! built by the one comparison dispatcher with nothing hidden from it.

use super::CEmitter;

impl CEmitter {
    /// Emit `nova_opt_eq_<sani>` for a TAGGED Option over a composite payload:
    /// prototype into the early typedef buffer, body into the late eq buffer.
    /// `none_tag` is the C spelling of the None tag at the caller's site.
    pub(super) fn emit_opt_eq_split(&self, sanitized: &str, c_ty: &str, none_tag: &str) {
        let storage = self.top_level_storage_inline();
        self.novaopt_typedefs_buf.borrow_mut().push_str(&format!(
            "{storage}nova_bool nova_opt_eq_{sani}(NovaOpt_{sani} a, NovaOpt_{sani} b);\n",
            storage = storage, sani = sanitized));
        let body = self.emit_field_eq(c_ty, "a.value", "b.value", 0);
        self.novaopt_eq_fns_buf.borrow_mut().push_str(&format!(
            "{storage}nova_bool nova_opt_eq_{sani}(NovaOpt_{sani} a, NovaOpt_{sani} b) {{\n\
             \x20   if (a.tag != b.tag) return 0;\n\
             \x20   if (a.tag == {none}) return 1;\n\
             \x20   return {body};\n\
             }}\n",
            storage = storage, sani = sanitized, none = none_tag, body = body));
    }
}
