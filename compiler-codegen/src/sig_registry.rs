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
    /// Plan 172.1 U.4.3 (c2.2): declaration `Span` of the source `FnDecl` (when this
    /// view was built from one). The codegen dispatch site matches this against the
    /// checker's `resolved_callees` choice so codegen LOWERS the callee the checker
    /// CHOSE instead of re-resolving the overload by exact C-type (§0). `None` for
    /// synthesized views with no single source decl (D39 embed proxy, protocol-default,
    /// external-registry entries) — those never carry a checker-recorded instance choice.
    pub fn_span: Option<crate::diag::Span>,
}

/// One signature entry: the raw `&FnDecl` (checker side) + the [`CodegenView`]
/// (codegen side). Lifetime `'a` ties it to the `Module`/arena the `FnDecl`
/// lives in (mirrors `types::FnDeclArena`'s `&'arena FnDecl` pattern; the
/// multi-module arena merge is U.2.2).
pub struct SigEntry<'a> {
    pub fn_decl: &'a FnDecl,
    pub codegen: CodegenView,
}

/// Unified signature registry. ONE build pass; multiple indexes over the SAME
/// `&'a FnDecl`s — indexes are *views* of one source, not copies of data (§0).
///
/// - `by_key` — flat `(receiver, name) → overloads` + [`CodegenView`] (codegen side, U.2.4).
/// - `method_table` / `fn_decls` — the nested + free-fn shapes the CHECKER reads
///   verbatim (U.2.3 F1): a flat key cannot serve `method_table.get(type)` borrows,
///   full iteration, `contains_key`, or `keys().any(..)`, so the nested base index is
///   STORED, not reconstructed. These mirror the legacy `TypeCheckCtx`/`BoundCtx`/
///   `CapabilityCtx` field types byte-for-byte, in declaration order.
#[derive(Default)]
pub struct SigRegistry<'a> {
    pub by_key: HashMap<(Option<String>, String), Vec<SigEntry<'a>>>,
    /// Nested checker index: `type → method → overloads`. BASE ONLY — synthesized
    /// auto-derive methods are a `TypeCheckCtx`-private overlay (U.2.3 F2), never here,
    /// so this one registry stays byte-identical for `BoundCtx`/`CapabilityCtx`.
    pub method_table: HashMap<String, HashMap<String, Vec<&'a FnDecl>>>,
    /// Free-fn checker index: `name → overloads`.
    pub fn_decls: HashMap<String, Vec<&'a FnDecl>>,
}

impl<'a> SigRegistry<'a> {
    pub fn new() -> Self {
        Self::default()
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

    // --- Checker-facing accessors (U.2.3 F1) -------------------------------
    // Return the legacy `fn_decls`/`method_table` shapes verbatim so read-sites
    // swap `self.method_table` → `self.sig.methods_of(..)` etc. with ZERO behavior
    // change. BASE ONLY (no synthesized methods — see [`build_base`]); the synth
    // overlay lives in `TypeCheckCtx` (U.2.3.3, F2).

    /// Free-fn overload set by name (legacy `fn_decls.get(name)`).
    pub fn free_fns(&self, name: &str) -> Option<&Vec<&'a FnDecl>> {
        self.fn_decls.get(name)
    }

    /// All methods of a receiver type (legacy `method_table.get(type)`), for sites
    /// that borrow the inner map (iterate keys, `.get(method)`).
    pub fn methods_of(&self, type_name: &str) -> Option<&HashMap<String, Vec<&'a FnDecl>>> {
        self.method_table.get(type_name)
    }

    /// Overload set for `(type, method)` (legacy
    /// `method_table.get(type).and_then(|m| m.get(method))`).
    pub fn method_overloads(&self, type_name: &str, method: &str) -> Option<&Vec<&'a FnDecl>> {
        self.method_table.get(type_name).and_then(|m| m.get(method))
    }

    /// Whether any method is declared on `type_name` (legacy
    /// `method_table.contains_key(type)`).
    pub fn has_type(&self, type_name: &str) -> bool {
        self.method_table.contains_key(type_name)
    }

    /// Iterate `(type, methods)` (legacy `for (t, m) in &method_table`).
    pub fn iter_types(
        &self,
    ) -> impl Iterator<Item = (&String, &HashMap<String, Vec<&'a FnDecl>>)> {
        self.method_table.iter()
    }

    /// Build the cheap CHECKER-facing base index in ONE pass: the flat `by_key`
    /// (with EMPTY [`CodegenView`]s) plus the nested `method_table` + `fn_decls`
    /// the checker reads (U.2.3 F1). No `type_ref_to_c`/mangling here — the checker
    /// never reads [`CodegenView`], so the cost is deferred to
    /// [`populate_codegen_views`] for the codegen side (U.2.3 F3 perf §2).
    ///
    /// Iteration is `module.items` in declaration order, so every overload `Vec`
    /// matches the legacy `TypeCheckCtx`/`BoundCtx`/`CapabilityCtx` build loops
    /// byte-for-byte (this is the SINGLE loop those three duplicated, U.2.3).
    ///
    /// BASE = user `fn`s only. Synthesized auto-derive methods are deliberately NOT
    /// added here so this ONE registry is byte-identical for `BoundCtx`/`CapabilityCtx`
    /// (which never saw synth); `TypeCheckCtx` layers synth as a private overlay (F2).
    pub fn build_base(module: &'a Module) -> Self {
        let mut reg = Self::new();
        for item in &module.items {
            if let Item::Fn(f) = item {
                let receiver = f.receiver.as_ref().map(|r| r.type_name.clone());
                // Nested checker indexes (legacy shapes, declaration order).
                match &receiver {
                    Some(recv) => {
                        reg.method_table
                            .entry(recv.clone())
                            .or_default()
                            .entry(f.name.clone())
                            .or_default()
                            .push(f);
                    }
                    None => {
                        reg.fn_decls.entry(f.name.clone()).or_default().push(f);
                    }
                }
                // Flat codegen index (CodegenView populated lazily, see below).
                reg.insert(
                    receiver,
                    f.name.clone(),
                    SigEntry { fn_decl: f, codegen: CodegenView::default() },
                );
            }
        }
        reg
    }

    /// Plan 172.1.1 (U.1): MERGE method signatures from an additional `'static` builtin module
    /// (registry-only concrete types — StringBuilder/WriteBuffer/ReadBuffer) into the checker
    /// indexes, so the checker RESOLVES their method callees (`resolved_callees` → Call-channel).
    /// SKIP a type already known from `module.items`/prelude (snapshot before merge — no duplicate
    /// overloads). METHODS ONLY. `&'static FnDecl` coerce into `Vec<&'a FnDecl>` (`'static: 'a`).
    pub fn merge_module_fns(&mut self, module: &'a Module) {
        let known_types: std::collections::HashSet<String> =
            self.method_table.keys().cloned().collect();
        for item in &module.items {
            if let Item::Fn(f) = item {
                let Some(recv) = f.receiver.as_ref().map(|r| r.type_name.clone()) else {
                    continue; // methods only
                };
                if known_types.contains(&recv) {
                    continue; // already from module.items / prelude — don't duplicate overloads
                }
                self.method_table
                    .entry(recv.clone())
                    .or_default()
                    .entry(f.name.clone())
                    .or_default()
                    .push(f);
                self.insert(
                    Some(recv),
                    f.name.clone(),
                    SigEntry { fn_decl: f, codegen: CodegenView::default() },
                );
            }
        }
    }

    /// Populate every [`SigEntry::codegen`] in `by_key` (C-types + Plan 11 `c_name`)
    /// via the SHARED `ExternalRegistry` helpers (§0 «один type→C mapping» / Plan 11
    /// mangling — same source as `ExternalRegistry::decl_from_fn`, not a copy).
    /// `Self`-return resolves to the receiver type. Codegen-only (U.2.4); the checker
    /// (U.2.3) never reads these fields.
    ///
    /// Per-key overload count = `entries.len()` (Plan 11 — `c_name` suffix only when
    /// ≥2), so no separate counting pass is needed.
    ///
    /// STILL TODO ([M-172-sig-registry]): `variadic_last`, `param_defaults`,
    /// `is_delegated`; merging the import-resolved std + intrinsic .nv source (U.2.4);
    /// the `MethodSig` fold + dead `resolve_overload` removal (U.2.5).
    ///
    /// [M-172-sig-registry] §1 «никаких авто-выводимых неверных типов»: the
    /// `unwrap_or_default()` below (empty C-type on a `type_ref_to_c` error, e.g.
    /// removed `usize`) is INERT until codegen consumes this — when wired (U.2.4)
    /// an unresolved type MUST become a checker `[E_*]`, NOT a silent empty/`nova_int`.
    pub fn populate_codegen_views(&mut self) {
        use crate::codegen::external_registry::ExternalRegistry as ER;
        for ((receiver, _name), entries) in self.by_key.iter_mut() {
            let total = entries.len();
            let recv_c = receiver.as_deref();
            for entry in entries.iter_mut() {
                let f = entry.fn_decl;
                let param_c_types: Vec<String> = f
                    .params
                    .iter()
                    .map(|p| ER::type_ref_to_c(&p.ty, recv_c).unwrap_or_default())
                    .collect();
                let return_c_type = match &f.return_type {
                    Some(t) => ER::type_ref_to_c(t, recv_c).unwrap_or_default(),
                    None => "nova_unit".to_string(),
                };
                let is_instance = f
                    .receiver
                    .as_ref()
                    .map(|r| matches!(r.kind, ReceiverKind::Instance))
                    .unwrap_or(false);
                let is_consume = f.receiver.as_ref().map(|r| r.consume).unwrap_or(false);
                let c_name = ER::mangle_method_c_name(
                    recv_c,
                    is_instance,
                    is_consume,
                    &f.name,
                    total,
                    &ER::last_param_suffix(&f.params),
                );
                entry.codegen = CodegenView {
                    param_c_types,
                    return_c_type,
                    is_instance,
                    is_external: f.is_external,
                    recv_mutable: f.receiver.as_ref().map(|r| r.mutable).unwrap_or(false),
                    c_name,
                    // variadic_last / param_defaults / is_delegated: U.2.4
                    // [M-172-sig-registry].
                    ..Default::default()
                };
            }
        }
    }

    /// Full build (base index + codegen views). Used where BOTH sides are needed
    /// (and by the U.2.1/U.2.2 unit tests). The checker path calls [`build_base`]
    /// directly to skip the codegen-view cost (U.2.3 F3).
    pub fn build_from_module(module: &'a Module) -> Self {
        let mut reg = Self::build_base(module);
        reg.populate_codegen_views();
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

    #[test]
    fn c_name_mangling_free_static_instance() {
        let m = parse(
            "module t\nfn foo(x int) -> int => x\nfn Bar.make() -> int => 0\nfn Bar @get() -> int => 0\n",
        );
        let reg = SigRegistry::build_from_module(&m);
        assert_eq!(reg.lookup(None, "foo").unwrap()[0].codegen.c_name, "nova_fn_foo");
        assert_eq!(reg.lookup(Some("Bar"), "make").unwrap()[0].codegen.c_name, "Nova_Bar_static_make");
        assert_eq!(reg.lookup(Some("Bar"), "get").unwrap()[0].codegen.c_name, "Nova_Bar_method_get");
    }

    #[test]
    fn c_name_overload_gets_last_param_suffix() {
        // ≥2 overloads → c_name carries the last-param Nova-type suffix.
        let m = parse("module t\nfn g(x int) -> int => x\nfn g(x str) -> int => 0\n");
        let reg = SigRegistry::build_from_module(&m);
        let mut names: Vec<String> =
            reg.lookup(None, "g").unwrap().iter().map(|e| e.codegen.c_name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["nova_fn_g_int".to_string(), "nova_fn_g_str".to_string()]);
    }

    #[test]
    fn parity_with_external_registry_for_extern_fn() {
        // §0: SigRegistry's c_name MUST match ExternalRegistry for the same fn
        // (both use the shared mangle_method_c_name).
        use crate::codegen::external_registry::ExternalRegistry;
        let src = "module t\nexport extern \"nova\" fn AtomicI64.new(v i64) -> Self\n";
        let m = parse(src);
        let sig = SigRegistry::build_from_module(&m);
        let ext = ExternalRegistry::from_module(&m).expect("ext registry");
        let sig_cname = &sig.lookup(Some("AtomicI64"), "new").unwrap()[0].codegen.c_name;
        let ext_cname = &ext.lookup("AtomicI64", "new").unwrap()[0].c_name;
        assert_eq!(sig_cname, ext_cname, "SigRegistry c_name must match ExternalRegistry");
    }

    // --- U.2.3.1: base index (nested method_table + fn_decls) + accessors ---

    #[test]
    fn build_base_indexes_free_fns_and_methods_nested() {
        let m = parse(
            "module t\nfn foo(x int) -> int => x\nfn Bar @get() -> int => 0\nfn Bar.make() -> int => 0\n",
        );
        let reg = SigRegistry::build_base(&m);
        // free fn → fn_decls (NOT method_table)
        assert_eq!(reg.free_fns("foo").map(|v| v.len()), Some(1));
        assert!(reg.free_fns("get").is_none());
        // receiver fns → nested method_table
        assert!(reg.has_type("Bar"));
        assert!(!reg.has_type("foo"));
        assert_eq!(reg.method_overloads("Bar", "get").map(|v| v.len()), Some(1));
        assert_eq!(reg.method_overloads("Bar", "make").map(|v| v.len()), Some(1));
        assert!(reg.method_overloads("Bar", "nope").is_none());
        // methods_of borrows the whole inner map (F1 — borrow-able shape)
        assert_eq!(reg.methods_of("Bar").map(|m| m.len()), Some(2));
        assert!(reg.methods_of("foo").is_none());
    }

    #[test]
    fn build_base_leaves_codegen_view_empty() {
        // F3: base build is cheap — CodegenView stays Default until populate.
        let m = parse("module t\nfn h(a int) -> bool => true\n");
        let reg = SigRegistry::build_base(&m);
        let cv = &reg.lookup(None, "h").expect("h")[0].codegen;
        assert!(cv.c_name.is_empty());
        assert!(cv.param_c_types.is_empty());
        assert_eq!(cv.return_c_type, "");
    }

    #[test]
    fn build_from_module_still_populates_codegen_view() {
        // build_from_module = build_base + populate_codegen_views (U.2.2 parity).
        let m = parse("module t\nfn h(a int, b str) -> bool => true\n");
        let reg = SigRegistry::build_from_module(&m);
        let cv = &reg.lookup(None, "h").expect("h")[0].codegen;
        assert_eq!(cv.param_c_types, vec!["nova_int".to_string(), "nova_str".to_string()]);
        assert_eq!(cv.return_c_type, "nova_bool");
        assert_eq!(cv.c_name, "nova_fn_h");
    }

    #[test]
    fn overload_vec_in_declaration_order() {
        // byte-identical resolution depends on overload order = declaration order.
        let m = parse("module t\nfn g(x int) -> int => x\nfn g(x str) -> int => 0\n");
        let reg = SigRegistry::build_from_module(&m);
        // free_fns (nested index) and by_key share one build pass → same order.
        assert_eq!(reg.free_fns("g").map(|v| v.len()), Some(2));
        let g = reg.lookup(None, "g").expect("g");
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].codegen.c_name, "nova_fn_g_int");
        assert_eq!(g[1].codegen.c_name, "nova_fn_g_str");
    }

    #[test]
    fn iter_types_visits_all_receiver_types() {
        let m = parse("module t\nfn A @m() -> int => 0\nfn B @m() -> int => 0\n");
        let reg = SigRegistry::build_base(&m);
        let mut types: Vec<String> = reg.iter_types().map(|(t, _)| t.clone()).collect();
        types.sort();
        assert_eq!(types, vec!["A".to_string(), "B".to_string()]);
    }
}
