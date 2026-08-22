//! Plan 172.14 F.2 atom A4 -- sum PLACEMENT: which sums are emitted as an
//! inline `NovaValue_<X>` tag struct instead of a heap `Nova_<X>*`, and the
//! emission that follows from that choice.
//!
//! Split out of `emit_c.rs` per the arch-ratchet rule (`emit_detach.rs` /
//! `variant_ctor_channel.rs` / `variant_ctor_disarm.rs` precedent): a child
//! module of `emit_c` sees `CEmitter`'s private fields, and the ratchet does
//! not measure it, so the mechanism lives here and `emit_c.rs` keeps only the
//! call sites.
//!
//! SCOPE, and why it is this narrow. A4 takes payload-less, non-generic,
//! unfenced sums only. Two consequences of that choice REMOVE work rather than
//! add it, and both are worth stating because they are easy to mistake for
//! oversights:
//!
//!   * payload-less means ZERO inline edges, so such a sum is non-recursive by
//!     construction -- the A2 detector is not consulted here at all. It starts
//!     earning its keep at A7, where payload-carrying sums arrive.
//!   * payload-less means there is no `->payload.<V>` anywhere, so the four
//!     hard-coded payload accessors in the match emitter are unreachable for
//!     this class. Only the TAG accessor has to learn the value form.
//!
//! THE FENCE is the other half of the scope. `Option[X]` on a heap sum is today
//! a bare nullable `Nova_X*` under NPO, carrying no tag of its own (NPO keys off
//! a trailing `*`). Turning X into a value would silently move `Option[X]` to
//! the tagged form -- a size and ABI change to a SECOND type. Same story for
//! `Vec[X]` element storage and for erased `void*` positions. So a sum that
//! appears as a generic argument, a slice element, or behind a pointer stays on
//! the old path; those are A5's and A6's subjects. The result is that this atom
//! changes the representation of one type at a time and never a second type's
//! layout as a side effect.
//!
//! `NOVA_KILL_A4=1` restores the previous emission byte for byte.

use std::collections::{HashMap, HashSet};

use super::{CEmitter, RUNTIME_DEFINED_TYPES};
use crate::ast::{Item, Module, SumVariant, TypeDeclKind, TypeRef};

/// A4: sums whose C name appears in HAND-WRITTEN runtime C (`nova_rt/*.h`,
/// `*.c`), so their representation is pinned by code the compiler does not
/// emit. `Decision` is returned by the supervisor callback
/// `_h->on_child_fail(...)` as `Nova_Decision*`; `ScopeOutcome` is built by the
/// scope epilogue; the atomics/once/wait trio is read by the sync primitives.
/// Turning any of them into a value would change a signature the runtime
/// already fixed, and the C compiler would report it far from any sum code.
///
/// Derived, not guessed -- regenerate with:
///
/// ```text
/// grep -rhoE "Nova_[A-Z][A-Za-z0-9_]*" compiler-codegen/nova_rt/*.h compiler-codegen/nova_rt/*.c ///   | sed 's/^Nova_//' | sort -u > /tmp/rt.txt
/// # intersect with the sum names declared in std/src, examples, spec_tests
/// ```
///
/// `Result` and `RuntimeError` are already covered by `RUNTIME_DEFINED_TYPES`;
/// they are listed here too so the set reads as one rule rather than two.
const A4_RUNTIME_ABI_SUMS: &[&str] = &[
    "Decision", "MemOrdering", "OnceState", "Result", "RuntimeError",
    "ScopeOutcome", "WaitResult",
];

impl CEmitter {
    /// Plan 172.14 F.2 atom A4: `NOVA_KILL_A4=1` restores the pre-atom emission
    /// (every sum a heap pointer). The acceptance baseline is taken with this
    /// switch on the SAME binary -- same rule and shape as `NOVA_KILL_744` and
    /// `NOVA_KILL_D55NT`.
    fn a4_value_sums_disabled() -> bool {
        std::env::var("NOVA_KILL_A4").map(|v| v == "1").unwrap_or(false)
    }

    /// A4: collect every type name that appears in a position where turning a sum
    /// into an inline value would change something OTHER than the sum itself --
    /// a generic ARGUMENT (`Option[X]`, `Vec[X]`, `Result[X, _]`, any user
    /// template), a slice element, or behind a pointer.
    ///
    /// `Option[X]` is the one that matters most and the reason this fence exists
    /// at all: `Option[<heap sum>]` is today a bare nullable `Nova_X*` under NPO,
    /// carrying no tag of its own (`emit_c.rs:56632` keys NPO off a trailing `*`).
    /// Making X a value would silently move Option to the tagged form -- a size
    /// and ABI change to a second type. That belongs to A6, not here.
    fn a4_collect_poisoned_from_typeref(ty: &TypeRef, out: &mut HashSet<String>) {
        match ty {
            TypeRef::Named { generics, .. } => {
                for g in generics {
                    Self::a4_collect_named_leaves(g, out);
                    Self::a4_collect_poisoned_from_typeref(g, out);
                }
            }
            // `[]X` lowers to a `Vec` mono whose element storage is the C type of
            // X -- A5's subject, not A4's.
            TypeRef::Array(inner, _) => {
                Self::a4_collect_named_leaves(inner, out);
                Self::a4_collect_poisoned_from_typeref(inner, out);
            }
            // `[N]X` stores X inline too, and `*X` fixes a pointer ABI.
            TypeRef::FixedArray(_, inner, _) | TypeRef::Pointer(inner, _) => {
                Self::a4_collect_named_leaves(inner, out);
                Self::a4_collect_poisoned_from_typeref(inner, out);
            }
            TypeRef::Tuple(items, _) => {
                for it in items {
                    Self::a4_collect_named_leaves(it, out);
                    Self::a4_collect_poisoned_from_typeref(it, out);
                }
            }
            TypeRef::Func { params, return_type, .. } => {
                for p in params {
                    Self::a4_collect_named_leaves(p, out);
                    Self::a4_collect_poisoned_from_typeref(p, out);
                }
                if let Some(r) = return_type {
                    Self::a4_collect_named_leaves(r, out);
                    Self::a4_collect_poisoned_from_typeref(r, out);
                }
            }
            TypeRef::Readonly(inner, _)
            | TypeRef::Mut(inner, _)
            | TypeRef::Uninit(inner, _)
            | TypeRef::Ref(inner, _) => Self::a4_collect_poisoned_from_typeref(inner, out),
            TypeRef::Protocol { .. } | TypeRef::Unit(_) => {}
        }
    }

    /// A4: every `Named` name reachable inside `ty`, at any depth.
    fn a4_collect_named_leaves(ty: &TypeRef, out: &mut HashSet<String>) {
        match ty {
            TypeRef::Named { path, generics, .. } => {
                if let Some(n) = path.last() {
                    out.insert(n.clone());
                }
                for g in generics {
                    Self::a4_collect_named_leaves(g, out);
                }
            }
            TypeRef::Array(inner, _)
            | TypeRef::FixedArray(_, inner, _)
            | TypeRef::Pointer(inner, _)
            | TypeRef::Readonly(inner, _)
            | TypeRef::Mut(inner, _)
            | TypeRef::Uninit(inner, _)
            | TypeRef::Ref(inner, _) => Self::a4_collect_named_leaves(inner, out),
            TypeRef::Tuple(items, _) => {
                for it in items {
                    Self::a4_collect_named_leaves(it, out);
                }
            }
            TypeRef::Func { params, return_type, .. } => {
                for p in params {
                    Self::a4_collect_named_leaves(p, out);
                }
                if let Some(r) = return_type {
                    Self::a4_collect_named_leaves(r, out);
                }
            }
            TypeRef::Protocol { .. } | TypeRef::Unit(_) => {}
        }
    }

    /// A4: the same poison rule over a checker-resolved type. Declarations are
    /// walked as syntax above, but EXPRESSIONS are not walked at all -- the
    /// checker already visited every one of them and left the answer in
    /// `resolved_types`, which is both cheaper and broader than re-walking the
    /// AST (a turbofish `Vec[X].new()` or an inferred `Option[X]` local never has
    /// to be found by hand).
    fn a4_collect_poisoned_from_resolved(
        rt: &crate::types::ResolvedType,
        out: &mut HashSet<String>,
    ) {
        use crate::types::ResolvedType as R;
        match rt {
            R::Named { args, .. } => {
                for a in args {
                    Self::a4_collect_resolved_leaves(a, out);
                    Self::a4_collect_poisoned_from_resolved(a, out);
                }
            }
            // `[]X` / `[N]X` store X as an element; a typed pointer pins an ABI.
            R::Array(inner) | R::FixedArray(_, inner) | R::TypedPtr(_, inner) => {
                Self::a4_collect_resolved_leaves(inner, out);
                Self::a4_collect_poisoned_from_resolved(inner, out);
            }
            R::Tuple(items) => {
                for it in items {
                    Self::a4_collect_resolved_leaves(it, out);
                    Self::a4_collect_poisoned_from_resolved(it, out);
                }
            }
            _ => {}
        }
    }

    /// A4: every `Named` name reachable inside a resolved type, at any depth.
    fn a4_collect_resolved_leaves(rt: &crate::types::ResolvedType, out: &mut HashSet<String>) {
        use crate::types::ResolvedType as R;
        if let R::Named { name, args, .. } = rt {
            out.insert(name.clone());
            for a in args {
                Self::a4_collect_resolved_leaves(a, out);
            }
        } else {
            match rt {
                R::Array(inner) | R::FixedArray(_, inner) | R::TypedPtr(_, inner) => {
                    Self::a4_collect_resolved_leaves(inner, out)
                }
                R::Tuple(items) => {
                    for it in items {
                        Self::a4_collect_resolved_leaves(it, out);
                    }
                }
                _ => {}
            }
        }
    }

    /// Plan 172.14 F.2 atom A4 -- which sums are emitted as an inline
    /// `NovaValue_<X>` tag struct instead of a `nova_alloc`'d `Nova_<X>*`?
    ///
    /// A4 deliberately takes the narrowest class that still exercises the whole
    /// mechanism: **payload-less, non-generic, unfenced** sums. Two consequences
    /// of that choice are worth stating, because they remove work rather than add
    /// it:
    ///
    ///  - payload-less means ZERO inline edges, so such a sum is non-recursive by
    ///    construction. The A2 detector is not consulted here; it starts earning
    ///    its keep at A7, where payload-carrying sums arrive.
    ///  - payload-less means there is no `->payload.<V>` access anywhere, so the
    ///    four hard-coded payload accessors in the match emitter are unreachable
    ///    for this class. Only the TAG accessor has to learn the value form.
    ///
    /// The fence (see `a4_collect_poisoned_*`) keeps `Option[X]`, `Vec[X]`, slice
    /// elements, pointers and erased generic arguments on the old path, so this
    /// atom changes the representation of ONE type at a time and never a second
    /// type's layout as a side effect.
    pub(super) fn build_value_sum_set(&mut self, module: &Module) {
        if Self::a4_value_sums_disabled() {
            return;
        }
        let mut poisoned: HashSet<String> = HashSet::new();

        // (a) declarations: fields, variant payloads, newtype/alias targets.
        for item in &module.items {
            match item {
                Item::Type(t) => match &t.kind {
                    TypeDeclKind::Record(fields) => {
                        for f in fields {
                            Self::a4_collect_poisoned_from_typeref(&f.ty, &mut poisoned);
                        }
                    }
                    TypeDeclKind::NamedTuple(fields) => {
                        for f in fields {
                            Self::a4_collect_poisoned_from_typeref(&f.ty, &mut poisoned);
                        }
                    }
                    TypeDeclKind::Sum(variants) => {
                        for v in variants {
                            match &v.kind {
                                crate::ast::SumVariantKind::Unit => {}
                                crate::ast::SumVariantKind::Tuple(tys) => {
                                    for ty in tys {
                                        Self::a4_collect_poisoned_from_typeref(ty, &mut poisoned);
                                    }
                                }
                                crate::ast::SumVariantKind::Record(fields) => {
                                    for f in fields {
                                        Self::a4_collect_poisoned_from_typeref(
                                            &f.ty, &mut poisoned);
                                    }
                                }
                            }
                        }
                    }
                    TypeDeclKind::Newtype(inner) | TypeDeclKind::Alias(inner) => {
                        Self::a4_collect_poisoned_from_typeref(inner, &mut poisoned);
                    }
                    _ => {}
                },
                Item::Fn(f) => {
                    // An `external` signature pins a C ABI we do not own.
                    let is_external = matches!(f.body, crate::ast::FnBody::External);
                    for p in &f.params {
                        Self::a4_collect_poisoned_from_typeref(&p.ty, &mut poisoned);
                        if is_external {
                            Self::a4_collect_named_leaves(&p.ty, &mut poisoned);
                        }
                    }
                    if let Some(r) = &f.return_type {
                        Self::a4_collect_poisoned_from_typeref(r, &mut poisoned);
                        if is_external {
                            Self::a4_collect_named_leaves(r, &mut poisoned);
                        }
                    }
                }
                _ => {}
            }
        }

        // (b) expressions, via the checker's own annotations.
        for rt in self.resolved_types.values() {
            Self::a4_collect_poisoned_from_resolved(rt, &mut poisoned);
        }

        // (b2) variant-name collisions. When two sums declare the same variant
        // name, the emitter's variant->sum resolution is first-wins and can name
        // the WRONG sum for an expression's C type (`find_variant_compat`, and the
        // `crossmod_samename_method_lastwins` fixture is exactly that shape). While
        // both sums were heap pointers the emitter papered over it with an explicit
        // `(Nova_X*)` cast; an inline value cannot be cast that way, so the latent
        // mis-resolution turns into "assigning to NovaValue_A from NovaValue_B".
        // Fixing the resolution is not this atom's job, so an ambiguous sum stays
        // on the heap.
        {
            let mut variant_owner: HashMap<String, String> = HashMap::new();
            for item in &module.items {
                let Item::Type(t) = item else { continue };
                let TypeDeclKind::Sum(variants) = &t.kind else { continue };
                for v in variants {
                    match variant_owner.get(&v.name) {
                        Some(prev) if prev != &t.name => {
                            poisoned.insert(prev.clone());
                            poisoned.insert(t.name.clone());
                        }
                        Some(_) => {}
                        None => {
                            variant_owner.insert(v.name.clone(), t.name.clone());
                        }
                    }
                }
            }
        }

        // (c) candidates.
        for item in &module.items {
            let Item::Type(t) = item else { continue };
            let TypeDeclKind::Sum(variants) = &t.kind else { continue };
            if variants.is_empty() {
                continue;
            }
            if !variants
                .iter()
                .all(|v| matches!(v.kind, crate::ast::SumVariantKind::Unit))
            {
                continue;
            }
            if !t.generics.is_empty() {
                continue;
            }
            if RUNTIME_DEFINED_TYPES.contains(&t.name.as_str()) {
                continue;
            }
            if A4_RUNTIME_ABI_SUMS.contains(&t.name.as_str()) {
                continue;
            }
            // A colliding simple name is emitted under a module-qualified base;
            // that second naming axis is not worth crossing with a second ABI in
            // the first behaviour-changing atom.
            if self.colliding_type_names.contains(&t.name) {
                continue;
            }
            if poisoned.contains(&t.name) {
                continue;
            }
            self.value_sum_names.insert(t.name.clone());
        }

        if std::env::var("NOVA_DETECT_A4").map(|v| v == "1").unwrap_or(false) {
            let mut names: Vec<&String> = self.value_sum_names.iter().collect();
            names.sort();
            eprintln!("[a4-valuesum] eligible={} names={}", names.len(),
                      names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","));
        }
    }

    /// A4: forward-decl + `type_aliases` registration for an eligible sum.
    /// Returns `false` when the sum is not eligible, so the caller falls through
    /// to the ordinary `Nova_X` forward decl.
    ///
    /// Registering the alias HERE is what makes every later pass agree:
    /// `resolved_named_to_c` consults `type_aliases` first, so the sum lowers to
    /// `NovaValue_X` everywhere, and because that name carries the `NovaValue_`
    /// prefix, `is_value_type` recognises it and the `.`-versus-`->` oracle flips
    /// for free at its seven consumers.
    pub(super) fn a4_fwd_decl_value_sum(&mut self, t: &crate::ast::TypeDecl, fb: &str) -> bool {
        if !self.value_sum_names.contains(&t.name) {
            return false;
        }
        let TypeDeclKind::Sum(variants) = &t.kind else { return false };
        // The COMPLETE definition goes into the forward-decl buffer, not just a
        // `typedef struct`. A field of value type needs a complete struct, and
        // record bodies are emitted before `emit_sum_type` runs -- a forward
        // declaration alone gives "field has incomplete type", which is exactly
        // what conformance reported for `NovaValue_PathStyle` embedded in a
        // record. Emitting the whole thing this early is sound only because a
        // payload-less sum has ZERO dependencies on other user types: its body
        // is one tag word. A payload-carrying sum (A7) will have to join the
        // unified value-type topo-sort instead.
        let mut decl = String::new();
        decl.push_str("typedef enum {
");
        for v in variants {
            match v.discriminant {
                Some(d) => decl.push_str(&format!("    NOVA_TAG_{}_{} = {},
", fb, v.name, d)),
                None => decl.push_str(&format!("    NOVA_TAG_{}_{},
", fb, v.name)),
            }
        }
        decl.push_str(&format!("}} Nova_{}_Tag;
", fb));
        decl.push_str(&format!("typedef struct NovaValue_{0} NovaValue_{0};
", fb));
        decl.push_str(&format!(
            "struct NovaValue_{} {{ Nova_{}_Tag tag; }};
", fb, fb));
        self.user_type_fwd_decls.push_str(&decl);
        self.type_aliases
            .insert(t.name.clone(), format!("NovaValue_{}", fb));
        true
    }

    /// A4: a bare unit-variant used where the expected type is erased. A HEAP
    /// sum's constructor returns a pointer, which this site boxes into `nova_int`;
    /// a VALUE sum's returns a struct, and casting a struct to an integer is not
    /// C. Emit the call unboxed in that case.
    pub(super) fn a4_unit_variant_ctor(
        &self,
        sum_key: &str,
        ctor_prefix: &str,
        variant: &str,
    ) -> String {
        if self.value_sum_names.contains(sum_key) {
            return format!("nova_make_{}_{}()", ctor_prefix, variant);
        }
        format!("(nova_int)(intptr_t)nova_make_{}_{}()", ctor_prefix, variant)
    }

    /// A4: the C type of a sum VALUE, for the C-type inference paths that used
    /// to hardcode `Nova_<X>*`. An eligible sum is an inline struct with no star;
    /// everything else keeps the heap-pointer spelling byte for byte.
    ///
    /// These paths matter more than they look. A `let` with no annotation takes
    /// its declared C type from the inferred type of its right-hand side, and
    /// `callnorm` rewrites every call into exactly such lets
    /// (`__nova_arg_src<k>`), so a stale `Nova_X*` here surfaces as an
    /// "unknown type name" on an argument temp far from any sum code.
    pub(super) fn a4_sum_c_type(&self, sum_name: &str) -> String {
        if self.value_sum_names.contains(sum_name) {
            return format!("NovaValue_{}", sum_name);
        }
        format!("Nova_{}*", sum_name)
    }

    /// A4: `sum as int` yields the DISCRIMINANT (D52, spec 02-types.md:331).
    /// The pointer form re-casts and reads `->tag`; an inline value sum reads
    /// `.tag` off the value with no cast at all. Without this the value form
    /// falls through to a plain C cast of a struct to an integer, which is not
    /// C -- caught by `d52_sumint`'s `D52Red as int == 0`.
    pub(super) fn a4_sum_as_int_cast(
        &self,
        inner_c_ty: &str,
        target_c: &str,
        v: &str,
    ) -> Option<String> {
        let bare = inner_c_ty.strip_prefix("NovaValue_")?;
        if inner_c_ty.ends_with('*') || !self.value_sum_names.contains(bare) {
            return None;
        }
        if !matches!(target_c,
            "nova_int" | "int64_t" | "int32_t" | "int16_t" | "int8_t"
            | "nova_uint" | "uint64_t" | "uint32_t" | "uint16_t" | "nova_byte" | "uint8_t")
        {
            return None;
        }
        Some(format!("(({})(({}).tag))", target_c, v))
    }

    /// A4: an inline payload-less sum IS its tag, so equality is a tag compare on
    /// values. Consulted before the generic `NovaValue_` record arm of
    /// `emit_field_eq`, which would look for a record schema this type does not
    /// have and fall through to a struct `==` that C rejects.
    pub(super) fn a4_value_sum_eq(&self, cty: &str, l: &str, r: &str) -> Option<String> {
        let bare = cty.strip_prefix("NovaValue_")?;
        if cty.ends_with('*') || !self.value_sum_names.contains(bare) {
            return None;
        }
        Some(format!("(({}).tag == ({}).tag)", l, r))
    }

    /// A4: is this match scrutinee an inline value sum, so its tag is read with
    /// `.`? Both the Nova-level name and the already-lowered C type are consulted
    /// because the scrutinee arrives by either route depending on the arm.
    pub(super) fn a4_is_value_sum_scrutinee(&self, type_name: &str, scr_ty: &str) -> bool {
        self.value_sum_names.contains(type_name)
            || (scr_ty.starts_with("NovaValue_") && !scr_ty.ends_with('*'))
    }

    /// A4: emit the inline tag struct and by-value constructors. Returns `false`
    /// when the sum is not eligible, leaving `emit_sum_type` to take its usual
    /// heap path.
    pub(super) fn emit_value_sum_type(&mut self, name: &str, variants: &[SumVariant]) -> bool {
        // Plan 172.14 F.2 atom A4: payload-less, non-generic, unfenced sum ->
        // inline `NovaValue_X { tag; }`, with constructors returning it by value.
        // The tag ENUM keeps its `Nova_X_Tag` / `NOVA_TAG_X_V` spelling, so every
        // existing reference to a tag constant is untouched and only the carrier
        // changes. There is no union here at all -- payload-less is precisely the
        // class with nothing to put in one, which is why it is the first atom.
        if !self.value_sum_names.contains(name) {
            return false;
        }
        {
            // Enum + struct were already emitted into the forward-decl buffer by
            // `a4_fwd_decl_value_sum`; only the constructors belong here, where
            // they still precede every function body that calls them.
            for v in variants {
                self.line(&format!(
                    "{storage}NovaValue_{name} nova_make_{name}_{var}(void) {{",
                    storage = self.top_level_storage(), name = name, var = v.name));
                self.indent += 1;
                self.line(&format!("NovaValue_{} _r;", name));
                self.line(&format!("_r.tag = NOVA_TAG_{}_{};", name, v.name));
                self.line("return _r;");
                self.indent -= 1;
                self.line("}");
                self.line("");
            }
            let empty_schema: HashMap<String, Vec<String>> =
                variants.iter().map(|v| (v.name.clone(), Vec::new())).collect();
            let variant_order: Vec<String> =
                variants.iter().map(|v| v.name.clone()).collect();
            let c_name = format!("NovaValue_{}", name);
            self.sum_schema_registry.register_user_sum(
                name,
                &empty_schema,
                &c_name,
                crate::codegen::sum_schema_registry::SumAbi::ValueTagPayload,
                &variant_order,
            );
            self.sum_schemas.insert(name.to_string(), empty_schema);
        }
        true
    }
}
