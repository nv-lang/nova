//! Plan 172.1 U.2.1 — unified signature registry (single source of truth for
//! method/fn signatures, §0 conventions).
//!
//! GOAL (U.2): collapse the FOUR places method/signature data lives today —
//! checker `TypeCheckCtx.method_table` / `fn_decls` (types/mod.rs), the
//! near-identical rebuilds in `BoundCtx` / `CapabilityCtx`, and codegen's
//! `method_overloads: BTreeMap<(String,String),Vec<MethodSig>>` (emit_c.rs) —
//! into ONE registry both layers read. This module defines that registry.
//!
//! U.2.1 SCOPE (this task): ADD the type + a builder skeleton + unit tests.
//! It does NOT yet replace `MethodSig` or the three checker tables — that
//! happens in U.2.3 (checker consumes) / U.2.4 (codegen consumes) / U.2.5
//! (fold `MethodSig` into [`CodegenView`], delete dead `resolve_overload`).
//! Marker **[M-172-sig-registry]**.
//!
//! Keyed by `(receiver: Option<String>, name)` → `Vec<SigEntry>` (Vec for
//! Plan 11 overloads). `receiver = None` ⇒ free function. Each `SigEntry`
//! carries BOTH the raw `&FnDecl` (for the checker's TypeRef-level work —
//! params/generics/receiver/return as TypeRef, spans) AND a [`CodegenView`]
//! (the C-type / mangling facts codegen needs), so neither layer re-derives.

use crate::ast::{FnDecl, Item, Module, ReceiverKind};
use std::collections::HashMap;

/// Codegen-facing view of one signature. Mirrors `emit_c.rs::MethodSig`; in
/// U.2.5 `MethodSig` is FOLDED into this ([M-172-sig-registry]).
///
/// The C-type / mangling fields (`param_c_types`, `return_c_type`, `c_name`,
/// `param_defaults`) require `type_ref_to_c` + Plan 11 mangling and are
/// populated by the codegen-aware builder in **U.2.2**. The U.2.1 skeleton
/// builder ([`SigRegistry::build_from_module`]) fills only the TypeRef-
/// independent flags and leaves the C-type fields at their `Default` (empty).
#[derive(Debug, Clone, Default)]
pub struct CodegenView {
    /// C-types of params (without receiver). Populated in U.2.2.
    pub param_c_types: Vec<String>,
    /// Return C-type (`Self` resolved to receiver). Populated in U.2.2.
    pub return_c_type: String,
    /// `@method` (instance) vs `Type.method` (static).
    pub is_instance: bool,
    /// `extern "nova"`/legacy `external fn`.
    pub is_external: bool,
    /// D39 anonymous-embed auto-proxy (Own beats Delegated). Set in U.2.2.
    pub is_delegated: bool,
    /// Mangled C name (`Nova_T_method_m` / `Nova_T_static_m` [+ overload suffix]).
    /// Populated in U.2.2.
    pub c_name: String,
    /// D69: last param is variadic (`...items []T`). Set in U.2.2.
    pub variadic_last: bool,
    /// D178: C-string exprs for param defaults (parallel to `param_c_types`).
    /// Populated in U.2.2.
    pub param_defaults: Vec<Option<String>>,
    /// `fn Type mut @method` ⇒ receiver is mutable.
    pub recv_mutable: bool,
}

/// One signature entry: the raw `&FnDecl` (checker side) + the [`CodegenView`]
/// (codegen side). Lifetime `'a` ties it to the `Module`/arena the `FnDecl`
/// lives in (mirrors `types::FnDeclArena`'s `&'arena FnDecl` pattern; the
/// multi-module arena merge is U.2.2).
pub struct SigEntry<'a> {
    pub fn_decl: &'a FnDecl,
    pub codegen: CodegenView,
}

/// Unified signature registry. `by_key[(receiver, name)]` = overload set.
#[derive(Default)]
pub struct SigRegistry<'a> {
    pub by_key: HashMap<(Option<String>, String), Vec<SigEntry<'a>>>,
}

impl<'a> SigRegistry<'a> {
    pub fn new() -> Self {
        Self { by_key: HashMap::new() }
    }

    /// Add an overload under `(receiver, name)`.
    pub fn insert(&mut self, receiver: Option<String>, name: String, entry: SigEntry<'a>) {
        self.by_key.entry((receiver, name)).or_default().push(entry);
    }

    /// Look up the overload set for `(receiver, name)`. `receiver = None` ⇒ free fn.
    pub fn lookup(&self, receiver: Option<&str>, name: &str) -> Option<&[SigEntry<'a>]> {
        self.by_key
            .get(&(receiver.map(str::to_string), name.to_string()))
            .map(Vec::as_slice)
    }

    /// Builder: index every `fn`/method of `module` by `(receiver, name)` with
    /// the raw `&FnDecl` + a [`CodegenView`].
    ///
    /// U.2.1: TypeRef-independent flags (is_instance/is_external/recv_mutable).
    /// **U.2.2 (this step):** also populate `param_c_types` + `return_c_type` by
    /// REUSING the shared `ExternalRegistry::type_ref_to_c` (§0 «один type→C
    /// mapping» — not a copy). `Self`-return resolves to the receiver type.
    ///
    /// STILL TODO ([M-172-sig-registry]): `c_name` (Plan 11 mangling needs the
    /// per-key overload count + consume/static/instance base + last-param
    /// suffix), `variadic_last`, `param_defaults`, `is_delegated`; merging the
    /// import-resolved std + intrinsic .nv source; checker/codegen consumption
    /// (U.2.3/U.2.4) and the `MethodSig` fold + dead `resolve_overload` removal
    /// (U.2.5). A `type_ref_to_c` error (e.g. removed `usize`) leaves the C-type
    /// field empty (best-effort; the checker emits the real diagnostic).
    pub fn build_from_module(module: &'a Module) -> Self {
        use crate::codegen::external_registry::ExternalRegistry as ER;
        let mut reg = Self::new();
        for item in &module.items {
            if let Item::Fn(f) = item {
                let receiver = f.receiver.as_ref().map(|r| r.type_name.clone());
                let recv_c = receiver.as_deref();
                let param_c_types: Vec<String> = f
                    .params
                    .iter()
                    .map(|p| ER::type_ref_to_c(&p.ty, recv_c).unwrap_or_default())
                    .collect();
                let return_c_type = match &f.return_type {
                    Some(t) => ER::type_ref_to_c(t, recv_c).unwrap_or_default(),
                    None => "nova_unit".to_string(),
                };
                let cv = CodegenView {
                    param_c_types,
                    return_c_type,
                    is_instance: f
                        .receiver
                        .as_ref()
                        .map(|r| matches!(r.kind, ReceiverKind::Instance))
                        .unwrap_or(false),
                    is_external: f.is_external,
                    recv_mutable: f.receiver.as_ref().map(|r| r.mutable).unwrap_or(false),
                    // c_name / variadic_last / param_defaults / is_delegated:
                    // U.2.2-next [M-172-sig-registry].
                    ..Default::default()
                };
                reg.insert(receiver, f.name.clone(), SigEntry { fn_decl: f, codegen: cv });
            }
        }
        reg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Module {
        crate::parser::parse(src).expect("test snippet should parse")
    }

    #[test]
    fn free_fn_keyed_by_none_receiver() {
        let m = parse("module t\nfn foo(x int) -> int => x\n");
        let reg = SigRegistry::build_from_module(&m);
        let foo = reg.lookup(None, "foo").expect("foo present");
        assert_eq!(foo.len(), 1);
        assert!(!foo[0].codegen.is_instance);
        assert_eq!(foo[0].fn_decl.name, "foo");
    }

    #[test]
    fn static_vs_instance_method_keyed_by_receiver() {
        // `Type.method` = static (`.`); `Type @method` = instance (`@`).
        let m = parse("module t\nfn Bar.make() -> int => 0\nfn Bar @get() -> int => 0\n");
        let reg = SigRegistry::build_from_module(&m);

        let make = reg.lookup(Some("Bar"), "make").expect("make present");
        assert_eq!(make.len(), 1);
        assert!(!make[0].codegen.is_instance, "Bar.make is static");

        let get = reg.lookup(Some("Bar"), "get").expect("get present");
        assert!(get[0].codegen.is_instance, "Bar @get is instance");

        // free-fn lookup must NOT find a receiver-qualified method.
        assert!(reg.lookup(None, "make").is_none());
    }

    #[test]
    fn overloads_accumulate_under_one_key() {
        let m = parse("module t\nfn g(x int) -> int => x\nfn g(x str) -> int => 0\n");
        let reg = SigRegistry::build_from_module(&m);
        assert_eq!(reg.lookup(None, "g").expect("g present").len(), 2);
    }

    #[test]
    fn codegen_view_c_types_via_shared_mapping() {
        // U.2.2: param/return C-types populated via ExternalRegistry::type_ref_to_c.
        let m = parse("module t\nfn h(a int, b str) -> bool => true\n");
        let reg = SigRegistry::build_from_module(&m);
        let cv = &reg.lookup(None, "h").expect("h")[0].codegen;
        assert_eq!(cv.param_c_types, vec!["nova_int".to_string(), "nova_str".to_string()]);
        assert_eq!(cv.return_c_type, "nova_bool");
    }

    #[test]
    fn unit_return_maps_to_nova_unit() {
        let m = parse("module t\nfn noret(x int) => ()\n");
        let reg = SigRegistry::build_from_module(&m);
        let cv = &reg.lookup(None, "noret").expect("noret")[0].codegen;
        assert_eq!(cv.return_c_type, "nova_unit");
    }

    #[test]
    fn self_return_resolves_to_receiver() {
        // `Self` in a method return → `Nova_<Recv>*` (shared mapping).
        let m = parse("module t\nfn Bar.make() -> Self => Bar.make()\n");
        let reg = SigRegistry::build_from_module(&m);
        let cv = &reg.lookup(Some("Bar"), "make").expect("make")[0].codegen;
        assert_eq!(cv.return_c_type, "Nova_Bar*");
    }
}
