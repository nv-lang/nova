// Plan 144.1 — per-type GC pointer-offset bitmap pass  (Plan 144 §4 / §7 / §8 Ф.1)
//
// Source-level, COMPILE-TIME analytical pass computing, for every named user
// type (record / sum / named-tuple / newtype) and the relevant built-in value
// types (`str`), the set of byte-offsets within the object's emitted C layout
// that hold a GC-managed pointer — the per-type pointer-offset "bitmap" (Go
// `gcdata` analogue).  Sum types get PER-VARIANT bitmaps keyed by the variant
// tag, so an inactive variant's scalar payload is never scanned as a pointer.
//
// This pass EMITS NOTHING into the generated C.  It is consulted ONLY by the
// `gc-layout-analyze` introspection CLI and the unit tests below.  No layout-id
// is written into object headers; allocation paths are untouched.  Runtime
// consumption (layout-id in header + precise tracer) is Plan 144.5 — out of
// scope here.  `emit_c.rs` MUST NOT call into this module during emission.
//
// STRUCTURE — modelled on the emit-nothing precedent `may_gc.rs` (Plan 144.0):
// a standalone pass over the parsed AST, exposing a public result type
// (`GcLayoutMap`) + a `compute_gc_layout` driver + accessors, wired to a CLI
// command and exercised by `#[cfg(test)]` unit tests only.
//
// LAYOUT ACCURACY — the crux.  Offsets/sizes/alignment MUST match what codegen
// actually emits for the C struct, otherwise a future tracer reads garbage.
// We REUSE the canonical layout computer
// `const_fn_eval::type_size_or_align_resolved` (the same field-walk emit_c's
// `type_decl_size_or_align` agrees with) for the recursion MATH, and fold
// per-field offsets ALONGSIDE the identical pad-then-place loop.  Two F0
// discrepancies between that math source and what emit_c actually lowers are
// reconciled here by sizing each field from its EMITTED C representation:
//   1. `[N]T` (FixedArray): the size fn computes N*size(T) inline, but emit_c
//      lowers a `[N]T` FIELD to ONE `NovaArray_T*`/Vec HEAP POINTER — so the
//      field is a single 8-byte GC pointer, not N inline elements.
//   2. `[]T` (Vec): the size fn treats it as a 16-byte slice, but emit_c lowers
//      the FIELD to one 8-byte `Nova_Vec____*` pointer — a single GC pointer.
//   3. `char`: the size fn returns 4, but emit_c maps `char` → `nova_char`
//      which is `typedef int64_t` (8 bytes) — a SCALAR, but its emitted width
//      is 8, which shifts following field offsets.
// `classify_field` below returns each field's (gc-ness, emitted-size,
// emitted-align), and the field-walk uses THOSE — so the accumulated offsets
// are byte-accurate vs the emitted C, and the total agrees with the layout
// math for the (common) types where the two never diverge.
//
// SOUNDNESS — CONSERVATIVE DIRECTION mirrors may-GC's default-to-MayGC:
// NEVER miss a real GC pointer (missing → freed-while-reachable → UAF).  When a
// field's GC-ness or layout is unknown / unhandled (unresolved generic slot,
// erased `nova_int`-boxed element, an opaque/protocol type with no layout, any
// C type the classifier cannot PROVE scalar), MARK IT AS A POINTER
// (over-approximate).  False retention is the lesser evil; missing a pointer is
// fatal.  A whole type whose layout cannot be resolved is reported as
// `Unresolved` (caller must treat every word conservatively), NEVER as
// all-scalar.  We emit a NON-pointer classification only when the field is
// PROVABLY a scalar / raw-FFI pointer with non-GC pointee / value-embedded
// scalar.

use crate::ast::{
    RecordField, SumVariant, SumVariantKind, TypeDecl, TypeDeclKind, TypeRef,
};
use std::collections::{HashMap, HashSet};

/// Width (bytes) of any heap pointer on the x64 ABI emit_c targets.  A
/// GC-pointer FIELD occupies exactly one of these regardless of pointee.
pub const PTR_SIZE: usize = 8;
/// Alignment of any heap pointer.
pub const PTR_ALIGN: usize = 8;

/// Per-type GC layout.  For a record / named-tuple / value-type: `pointer_offsets`
/// lists the byte-offsets of GC-managed pointer slots in declaration-order
/// layout; `variants` is empty.  For a sum type: `pointer_offsets` is empty (the
/// tag at offset 0 is scalar) and `variants` holds one bitmap PER variant keyed
/// by variant name (and tag index), each listing the GC-pointer offsets active
/// when that variant's tag is live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutInfo {
    /// Total emitted C size in bytes (tail-padded).  `None` when the layout
    /// could not be resolved (see `unresolved`).
    pub size: Option<usize>,
    /// Emitted C alignment in bytes.  `None` when unresolved.
    pub align: Option<usize>,
    /// GC-pointer byte-offsets for a non-sum type (record / named-tuple /
    /// newtype / value-record / `str`).  Empty for a pure-scalar type and for
    /// sum types (whose offsets live per-variant in `variants`).
    pub pointer_offsets: Vec<usize>,
    /// Per-variant GC-pointer bitmaps for a sum type, in declaration order
    /// (tag index = position in this Vec).  Empty for non-sum types.
    pub variants: Vec<VariantLayout>,
    /// True when the layout (or some transitively-needed field layout) could
    /// not be resolved.  A consumer MUST then treat the object conservatively
    /// (scan every word) — NEVER assume the partial bitmap is complete.
    pub unresolved: bool,
}

/// One sum variant's GC-pointer bitmap, keyed by name + tag index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantLayout {
    /// Variant name (`Some`, `Cons`, `Red`, …).
    pub name: String,
    /// Tag discriminant index (declaration order, 0-based).
    pub tag: usize,
    /// GC-pointer byte-offsets WITHIN the whole sum object that are live when
    /// this variant's tag is selected (payload-base + per-field offsets).
    pub pointer_offsets: Vec<usize>,
}

impl LayoutInfo {
    fn unresolved() -> Self {
        LayoutInfo {
            size: None,
            align: None,
            pointer_offsets: Vec::new(),
            variants: Vec::new(),
            unresolved: true,
        }
    }
}

/// The whole-program result: type-name → its GC layout bitmap.  Built once over
/// a populated type universe; `populated` is false for an empty/default map (a
/// consumer must then treat every type conservatively).
#[derive(Debug, Default, Clone)]
pub struct GcLayoutMap {
    by_name: HashMap<String, LayoutInfo>,
    populated: bool,
}

impl GcLayoutMap {
    /// The GC layout for a named type, if it was computed.
    pub fn get(&self, type_name: &str) -> Option<&LayoutInfo> {
        self.by_name.get(type_name)
    }

    /// All computed (name, layout) pairs — for the CLI report.  Order is
    /// unspecified; callers sort for determinism.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &LayoutInfo)> {
        self.by_name.iter()
    }

    /// Number of types in the map.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// True once the pass ran over a non-empty type universe.  When false,
    /// every queried type is absent and a consumer MUST treat all objects
    /// conservatively (scan every word).
    pub fn populated(&self) -> bool {
        self.populated
    }
}

// =============================================================================
//                          field classification
// =============================================================================

/// Classification of a single field's emitted C slot.
struct FieldClass {
    /// Emitted C size in bytes (8 for any pointer-lowered field; `nova_char`
    /// counts as 8, not the 4 the size-math reports).
    size: usize,
    /// Emitted C alignment in bytes.
    align: usize,
    /// GC-pointer offsets WITHIN this field's own storage (relative to the
    /// field's base offset), to be shifted by the field offset and spliced into
    /// the containing type.  A single-pointer field yields `[0]`; an inline
    /// value-aggregate (str / value-record / tuple) yields its recursed
    /// offsets; a pure scalar yields `[]`.
    rel_pointer_offsets: Vec<usize>,
    /// True when this field's layout could not be resolved (over-approximated
    /// conservatively as one pointer).  Propagates `unresolved` upward.
    unresolved: bool,
}

/// Strip the transparent type-level modifier wrappers (`ro` / `mut` / `unsafe`)
/// — identical treatment to `type_size_or_align_resolved` and `type_ref_to_c`,
/// which recurse through them.  Returns the inner non-wrapper `TypeRef`.
fn strip_modifiers(t: &TypeRef) -> &TypeRef {
    match t {
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Unsafe(inner, _) => {
            strip_modifiers(inner)
        }
        other => other,
    }
}

/// Round `n` up to a multiple of `align` (align ≥ 1).
fn align_up(n: usize, align: usize) -> usize {
    if align <= 1 {
        return n;
    }
    let rem = n % align;
    if rem == 0 {
        n
    } else {
        n + (align - rem)
    }
}

/// Names of built-in primitive SCALAR types (non-GC, fixed C layout).  `char`
/// is sized 4 (emitted `nova_char` = `uint32_t`, D128 AMEND Plan 152.8), see `prim_emit`.
fn prim_emit(name: &str) -> Option<(usize, usize)> {
    // (emitted size, align) — matches emit_c's C typedefs.
    match name {
        "int" | "i64" | "u64" | "f64" | "uint" => Some((8, 8)),
        // `char` → nova_char = typedef uint32_t → 4 bytes (D128 AMEND, Plan 152.8,
        // nova_rt.h:25).  Scalar, 4 bytes like Rust char / Go rune.
        "char" => Some((4, 4)),
        "i32" | "u32" | "f32" => Some((4, 4)),
        "i16" | "u16" => Some((2, 2)),
        "i8" | "u8" | "bool" => Some((1, 1)),
        // `never` → nova_int placeholder slot, never read — scalar 8.
        "never" => Some((8, 8)),
        _ => None,
    }
}

/// Whether a named type is a built-in GC-managed heap CONTAINER whose FIELD
/// representation is a single heap pointer (Vec/array/Map families).  These are
/// `Nova_Vec____*` / `NovaArray_*` / `Nova_HashMap____*` at the field level.
fn is_builtin_heap_container(name: &str) -> bool {
    matches!(
        name,
        "Vec" | "Array" | "Map" | "HashMap" | "HashSet" | "Set" | "StringBuilder"
    )
}

/// Whether a type's EMITTED C representation is a single pointer — i.e.
/// `type_ref_to_c` returns a `*`-suffixed C type.  This is exactly the NPO
/// (Null Pointer Optimization) predicate emit_c's `register_novaopt_decl` uses
/// to decide `Option[T]`'s lowering: when the inner C type ends in `*`, the
/// option is `typedef struct NovaOpt_X { T* value; }` — a single 8-byte heap
/// pointer @0; otherwise it is the tagged inline struct `{ int tag; T value; }`.
///
/// Mirrors `type_ref_to_c` (emit_c.rs:5249+) for the constructs that can appear
/// as an Option inner; it must NEVER under-report (a false→tagged path is the
/// conservative, sound direction — it recurses into the payload and over-marks
/// the scalar tag rather than missing a pointer).  A type the predicate cannot
/// PROVE pointer-lowered returns `false` and is therefore modelled as the
/// tagged form, which is always sound.
fn inner_lowers_to_single_pointer(
    ty: &TypeRef,
    type_decls: &HashMap<String, TypeDecl>,
) -> bool {
    let ty = strip_modifiers(ty);
    match ty {
        // `*T` family (`T*`, `void*`, `*ro u8`, `*()`) → `*`-suffixed.
        TypeRef::Pointer(_, _) => true,
        // Closures / function types → `void*`.
        TypeRef::Func { .. } => true,
        // `[]T` / `[N]T` → `Nova_Vec____*` / `NovaArray_*`.
        TypeRef::Array(_, _) | TypeRef::FixedArray(_, _, _) => true,
        // Anonymous protocol object → `void*` (emit_c.rs:5658).
        TypeRef::Protocol { .. } => true,
        // Value-aggregates lower to an inline struct (NOT a pointer).
        TypeRef::Tuple(_, _) | TypeRef::Unit(_) => false,
        TypeRef::Named { path, generics, .. } => {
            let name = path.last().map(|s| s.as_str()).unwrap_or("");
            // `str` → `nova_str` value struct (not a pointer).
            if name == "str" && generics.is_empty() {
                return false;
            }
            // Primitive scalars → not a pointer.
            if generics.is_empty() && prim_emit(name).is_some() {
                return false;
            }
            // Built-in heap containers → `Nova_Vec____*` etc.
            if is_builtin_heap_container(name) {
                return true;
            }
            // Result → `NovaRes_<ok>_<err>*` (always a heap pointer ABI).
            if name == "Result" {
                return true;
            }
            // `Option[T]` inner of an Option: the inner option lowers to a
            // `NovaOpt_X` *value struct* (NPO or tagged) — NOT a pointer — so a
            // nested `Option[Option[..]]` falls into the tagged branch (matches
            // emit_c, which has the outer NovaOpt hold a struct field).
            if name == "Option" {
                return false;
            }
            // User-declared named type: heap record / sum → `Nova_X*`;
            // value-record / named-tuple → inline struct; newtype/alias → recurse.
            if generics.is_empty() {
                if let Some(td) = type_decls.get(name) {
                    return named_decl_lowers_to_single_pointer(td, type_decls);
                }
            }
            // Unknown / unresolved generic slot: emit_c erases it to a pointer
            // representation in practice (`void*` / `Nova_X*`).  Treating it as
            // a pointer here keeps the NPO option at 8 bytes; but since the
            // CALLER (the Option arm) only consults this to choose the NPO fast
            // path, and the tagged fallback is strictly more conservative, we
            // return `false` so an unknown inner is modelled as the tagged form
            // (which over-marks rather than risks a wrong NPO size).
            false
        }
        _ => false,
    }
}

/// NPO eligibility for a user-declared named type (delegate of
/// `inner_lowers_to_single_pointer`).
fn named_decl_lowers_to_single_pointer(
    td: &TypeDecl,
    type_decls: &HashMap<String, TypeDecl>,
) -> bool {
    match &td.kind {
        // Heap record / sum → `Nova_X*` pointer.
        TypeDeclKind::Record(_) if td.allocation.is_heap() => true,
        TypeDeclKind::Sum(_) => true,
        // Value-record / named-tuple → inline value struct.
        TypeDeclKind::Record(_) | TypeDeclKind::NamedTuple(_) => false,
        // Newtype / alias — transparent: recurse on the inner type.
        TypeDeclKind::Newtype(inner) | TypeDeclKind::Alias(inner) => {
            inner_lowers_to_single_pointer(inner, type_decls)
        }
        // Opaque / effect / protocol — emit_c erases to a pointer (`void*` /
        // `NovaBox_*`); treat as a pointer so the NPO option stays 8 bytes.
        TypeDeclKind::Opaque
        | TypeDeclKind::Effect(_)
        | TypeDeclKind::Protocol { .. } => true,
    }
}

/// Classify one field's emitted C slot: size/align + GC-pointer offsets within
/// it.  `type_decls` is the resolved TypeDecl registry; `visited` guards
/// value-type recursion (a value-record cannot transitively contain itself
/// by-value, but we keep the set defensively).
fn classify_field(
    ty: &TypeRef,
    type_decls: &HashMap<String, TypeDecl>,
    visited: &mut HashSet<String>,
) -> FieldClass {
    let ty = strip_modifiers(ty);
    match ty {
        // ---- raw / typed pointers: `*T`, `*ro u8`, `*()` ----
        // Emitted as `T*` / `void*` — a RAW pointer that points OUTSIDE the GC
        // (nova_alloc) heap (FFI buffers, static const data, interior into a
        // stack frame).  Classified NON-GC for precision: the str.ptr case (a
        // pointer that CAN point into a GC string buffer) is handled inline by
        // the `str` arm which marks offset 0 explicitly, not via this arm.  See
        // residual note [classify-raw-ptr]: under non-moving object-start
        // lookup, marking a non-GC address is harmless, so this is the precise
        // (not unsound) default; it never causes a MISS because raw `*T` does
        // not own a GC allocation in today's Nova.
        TypeRef::Pointer(_, _) => FieldClass {
            size: PTR_SIZE,
            align: PTR_ALIGN,
            rel_pointer_offsets: Vec::new(),
            unresolved: false,
        },

        // ---- function types: `fn(...) -> R` ----
        // A Func-typed field is one pointer to a closure object (NovaClosBase).
        // The field slot itself is a GC pointer (the closure env is heap).
        TypeRef::Func { .. } => FieldClass {
            size: PTR_SIZE,
            align: PTR_ALIGN,
            rel_pointer_offsets: vec![0],
            unresolved: false,
        },

        // ---- anonymous protocol / dyn object ----
        // emit_c lowers an ANONYMOUS `protocol { .. }` value-position type to
        // `void*` — a SINGLE 8-byte pointer (emit_c.rs:5658), NOT the 16-byte
        // `NovaBox_X{ void* data; const VT* vtable }` fat pointer used for
        // NAMED protocol types (which route through `classify_named_decl`'s
        // Protocol arm).  The bare `void*` points into the GC heap (a boxed dyn
        // object), so it is ONE GC pointer @0.  Sizing it 16 over-sizes the
        // field and shifts every following field's offset — review finding #3
        // (a wrong offset MISSES the real next pointer → future-tracer UAF).
        // Flag `unresolved` for max safety: the anonymous dyn object model is
        // opaque (consistent with the named-protocol path flagging it).
        TypeRef::Protocol { .. } => FieldClass {
            size: PTR_SIZE,
            align: PTR_ALIGN,
            rel_pointer_offsets: vec![0],
            unresolved: true,
        },

        // ---- []T (Vec) and [N]T (FixedArray) ----
        // F0 discrepancy: the size-math treats `[]T` as a 16-byte slice and
        // `[N]T` as N*size(T) inline, but emit_c lowers BOTH FIELDS to ONE
        // 8-byte `Nova_Vec____*` / `NovaArray_*` HEAP POINTER.  Trust emit_c:
        // the field is a single GC pointer slot.
        TypeRef::Array(_, _) | TypeRef::FixedArray(_, _, _) => FieldClass {
            size: PTR_SIZE,
            align: PTR_ALIGN,
            rel_pointer_offsets: vec![0],
            unresolved: false,
        },

        // ---- unit ----
        TypeRef::Unit(_) => FieldClass {
            size: 0,
            align: 1,
            rel_pointer_offsets: Vec::new(),
            unresolved: false,
        },

        // ---- tuple `(A, B, ...)` (value-embedded inline struct) ----
        // Mono'd tuples are inline value structs (`_NovaTuple_*`); recurse with
        // the identical pad-then-place layout, splicing each element's GC
        // offsets shifted by the element offset.
        TypeRef::Tuple(elems, _) => {
            let mut offset = 0usize;
            let mut max_align = 1usize;
            let mut offsets: Vec<usize> = Vec::new();
            let mut unresolved = false;
            for el in elems {
                let fc = classify_field(el, type_decls, visited);
                if fc.unresolved {
                    unresolved = true;
                }
                offset = align_up(offset, fc.align);
                if fc.align > max_align {
                    max_align = fc.align;
                }
                for o in &fc.rel_pointer_offsets {
                    offsets.push(offset + o);
                }
                offset += fc.size;
            }
            let size = align_up(offset, max_align);
            FieldClass {
                size,
                align: max_align,
                rel_pointer_offsets: offsets,
                unresolved,
            }
        }

        // ---- named types: primitives, str, containers, user types ----
        TypeRef::Named { path, generics, .. } => {
            // A qualified/multi-segment path or a generic instance whose
            // arguments we cannot resolve to a concrete layout: treat the head
            // name when it is a known built-in container, else conservative.
            let name = path.last().map(|s| s.as_str()).unwrap_or("");

            // str — value-embedded `{const uint8_t* ptr; int64_t len}`,
            // sizeof 16, align 8.  `ptr` @0 IS a GC-pointer slot (object-start
            // lookup tolerates a literal/FFI buffer at mark — §7.6 H1); `len`
            // @8 is scalar.
            if name == "str" && generics.is_empty() {
                return FieldClass {
                    size: 16,
                    align: 8,
                    rel_pointer_offsets: vec![0],
                    unresolved: false,
                };
            }

            // Primitive scalar (sized per the EMITTED width, char = 8).
            if generics.is_empty() {
                if let Some((size, align)) = prim_emit(name) {
                    return FieldClass {
                        size,
                        align,
                        rel_pointer_offsets: Vec::new(),
                        unresolved: false,
                    };
                }
            }

            // Built-in GC container (Vec / Map / HashMap / …): one heap pointer.
            if is_builtin_heap_container(name) {
                return FieldClass {
                    size: PTR_SIZE,
                    align: PTR_ALIGN,
                    rel_pointer_offsets: vec![0],
                    unresolved: false,
                };
            }

            // Result[T,E] — ALWAYS lowers to `NovaRes_<ok>_<err>*`, a single
            // heap pointer (emit_c.rs:5336 / result_repr_c_type).  One GC
            // pointer slot @0; the (T,E) payload's own GC slots are reached
            // transitively through the box.
            if name == "Result" {
                return FieldClass {
                    size: PTR_SIZE,
                    align: PTR_ALIGN,
                    rel_pointer_offsets: vec![0],
                    unresolved: false,
                };
            }

            // Option[T] — TWO distinct emit_c lowerings (register_novaopt_decl,
            // emit_c.rs:5289 / 32169):
            //   (a) NPO form  — inner C type ends in `*` (boxed record/sum,
            //       `*T`, container, closure, protocol, Result, newtype-over-ptr):
            //       `typedef struct NovaOpt_X { T* value; }` — a single 8-byte
            //       heap pointer @0, NULL = None.  → `{size:8, [0]}`.
            //   (b) Tagged form — inner is a scalar / `str` / value-record /
            //       by-value tuple / by-value sum:
            //       `typedef struct NovaOpt_X { int tag; <payload> value; }`.
            //       `int` = `nova_int` = 8 bytes on x64 (Plan 133), so the tag
            //       occupies offset 0..8 (SCALAR — never a GC pointer) and the
            //       payload starts at `align_up(8, align(payload))`.  Any GC
            //       pointer inside the payload lives at `payload_base + sub`, and
            //       the field size is `align_up(payload_base + size(payload),
            //       max(8, align(payload)))` — NOT 8.  Modelling this uniformly
            //       as one 8-byte pointer @0 MISSES the real payload pointers and
            //       under-sizes the field, shifting every following field
            //       (Plan 144.1 review findings #1/#2 — a future-tracer UAF).
            if name == "Option" {
                let inner = generics.first();
                match inner {
                    Some(inner_ty) if inner_lowers_to_single_pointer(inner_ty, type_decls) => {
                        // (a) NPO: single heap pointer @0.
                        return FieldClass {
                            size: PTR_SIZE,
                            align: PTR_ALIGN,
                            rel_pointer_offsets: vec![0],
                            unresolved: false,
                        };
                    }
                    Some(inner_ty) => {
                        // (b) Tagged inline struct `{ int tag; <payload> }`.
                        let inner_fc = classify_field(inner_ty, type_decls, visited);
                        // Tag: nova_int = 8 bytes / align 8, SCALAR (offset 0).
                        const TAG_SIZE: usize = 8;
                        const TAG_ALIGN: usize = 8;
                        let payload_align = inner_fc.align.max(1);
                        let payload_base = align_up(TAG_SIZE, payload_align);
                        let rel: Vec<usize> = inner_fc
                            .rel_pointer_offsets
                            .iter()
                            .map(|o| payload_base + o)
                            .collect();
                        let max_align = TAG_ALIGN.max(payload_align);
                        let size = align_up(payload_base + inner_fc.size, max_align);
                        return FieldClass {
                            size,
                            align: max_align,
                            rel_pointer_offsets: rel,
                            unresolved: inner_fc.unresolved,
                        };
                    }
                    None => {
                        // Erased `Option` with no inner — emit_c falls back to
                        // `NovaOpt_nova_int` (tagged, `{int tag; nova_int}`),
                        // 16 bytes, NO GC pointer.  Conservative direction: we
                        // cannot prove the payload scalar, so flag unresolved.
                        return FieldClass {
                            size: 16,
                            align: 8,
                            rel_pointer_offsets: Vec::new(),
                            unresolved: true,
                        };
                    }
                }
            }

            // User-defined named type — consult the TypeDecl registry.
            if generics.is_empty() {
                if let Some(td) = type_decls.get(name) {
                    return classify_named_decl(name, td, type_decls, visited);
                }
            }

            // Unknown / unresolved generic type-param slot / cross-module type
            // with no decl in scope.  CONSERVATIVE: a single pointer slot, and
            // flag unresolved so the containing type is over-approximated.
            FieldClass {
                size: PTR_SIZE,
                align: PTR_ALIGN,
                rel_pointer_offsets: vec![0],
                unresolved: true,
            }
        }

        // ---- residual wrappers already stripped; anything else conservative ----
        _ => FieldClass {
            size: PTR_SIZE,
            align: PTR_ALIGN,
            rel_pointer_offsets: vec![0],
            unresolved: true,
        },
    }
}

/// Classify a field whose type is a user-declared named type.  Heap-allocated
/// records / sums / named-tuples are a SINGLE pointer slot at the field level
/// (`Nova_X*`); value-records (D226 `type X value`) and newtypes/aliases are
/// recursed INLINE at the field's offset.
fn classify_named_decl(
    name: &str,
    td: &TypeDecl,
    type_decls: &HashMap<String, TypeDecl>,
    visited: &mut HashSet<String>,
) -> FieldClass {
    match &td.kind {
        // Records: Heap (default) → single `Nova_X*` pointer slot; Value
        // (D226) → inline `NovaValue_X`, recurse fields at the field offset.
        TypeDeclKind::Record(fields) => {
            if td.allocation.is_heap() {
                FieldClass {
                    size: PTR_SIZE,
                    align: PTR_ALIGN,
                    rel_pointer_offsets: vec![0],
                    unresolved: false,
                }
            } else {
                // value-record — recurse inline.
                if !visited.insert(name.to_string()) {
                    // Defensive cycle guard (a value-record cannot legally
                    // contain itself by value; over-approximate if it ever did).
                    return FieldClass {
                        size: PTR_SIZE,
                        align: PTR_ALIGN,
                        rel_pointer_offsets: vec![0],
                        unresolved: true,
                    };
                }
                let (size, align, offsets, unresolved) =
                    walk_record_fields(fields, type_decls, visited);
                visited.remove(name);
                FieldClass { size, align, rel_pointer_offsets: offsets, unresolved }
            }
        }
        // Named-tuples (`type T(x, y)`, Plan 120) are VALUE structs
        // (`NovaTuple_T`) — recurse inline.
        TypeDeclKind::NamedTuple(fields) => {
            if !visited.insert(name.to_string()) {
                return FieldClass {
                    size: PTR_SIZE,
                    align: PTR_ALIGN,
                    rel_pointer_offsets: vec![0],
                    unresolved: true,
                };
            }
            // NamedTupleField → reuse RecordField-shaped walk via the ty list.
            let tys: Vec<&TypeRef> = fields.iter().map(|f| &f.ty).collect();
            let (size, align, offsets, unresolved) =
                walk_field_types(&tys, type_decls, visited);
            visited.remove(name);
            FieldClass { size, align, rel_pointer_offsets: offsets, unresolved }
        }
        // Sum types are HEAP-allocated (`Nova_X*`) — a single pointer slot at
        // the field level.  (The per-variant bitmap of the sum type itself is
        // computed when THAT type is the top-level subject, not when it is an
        // embedded field.)
        TypeDeclKind::Sum(_) => FieldClass {
            size: PTR_SIZE,
            align: PTR_ALIGN,
            rel_pointer_offsets: vec![0],
            unresolved: false,
        },
        // Newtype / alias — transparent: recurse on the inner type.
        TypeDeclKind::Newtype(inner) | TypeDeclKind::Alias(inner) => {
            classify_field(inner, type_decls, visited)
        }
        // Opaque / effect / protocol — no concrete layout.  CONSERVATIVE: one
        // pointer slot, flag unresolved.
        TypeDeclKind::Opaque
        | TypeDeclKind::Effect(_)
        | TypeDeclKind::Protocol { .. } => FieldClass {
            size: PTR_SIZE,
            align: PTR_ALIGN,
            rel_pointer_offsets: vec![0],
            unresolved: true,
        },
    }
}

/// Walk a list of record fields with the pad-then-place layout, returning
/// (size, align, gc-pointer-offsets, unresolved).  Identical pad/place rule to
/// `type_decl_size_or_align`'s Record arm, but each field is sized by its
/// EMITTED C slot (via `classify_field`) so offsets are byte-accurate.
fn walk_record_fields(
    fields: &[RecordField],
    type_decls: &HashMap<String, TypeDecl>,
    visited: &mut HashSet<String>,
) -> (usize, usize, Vec<usize>, bool) {
    let tys: Vec<&TypeRef> = fields.iter().map(|f| &f.ty).collect();
    walk_field_types(&tys, type_decls, visited)
}

/// Core field-walk over a list of field types.
fn walk_field_types(
    tys: &[&TypeRef],
    type_decls: &HashMap<String, TypeDecl>,
    visited: &mut HashSet<String>,
) -> (usize, usize, Vec<usize>, bool) {
    let mut offset = 0usize;
    let mut max_align = 1usize;
    let mut offsets: Vec<usize> = Vec::new();
    let mut unresolved = false;
    for ty in tys {
        let fc = classify_field(ty, type_decls, visited);
        if fc.unresolved {
            unresolved = true;
        }
        offset = align_up(offset, fc.align);
        if fc.align > max_align {
            max_align = fc.align;
        }
        for o in &fc.rel_pointer_offsets {
            offsets.push(offset + o);
        }
        offset += fc.size;
    }
    let size = align_up(offset, max_align);
    offsets.sort_unstable();
    (size, max_align, offsets, unresolved)
}

// =============================================================================
//                      per-type layout computation
// =============================================================================

/// Compute the GC layout bitmap for one named TypeDecl (top-level subject).
fn layout_of_decl(
    name: &str,
    td: &TypeDecl,
    type_decls: &HashMap<String, TypeDecl>,
) -> LayoutInfo {
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(name.to_string());
    match &td.kind {
        TypeDeclKind::Record(fields) => {
            let (size, align, offsets, unresolved) =
                walk_record_fields(fields, type_decls, &mut visited);
            LayoutInfo {
                size: Some(size),
                align: Some(align),
                pointer_offsets: offsets,
                variants: Vec::new(),
                unresolved,
            }
        }
        TypeDeclKind::NamedTuple(fields) => {
            let tys: Vec<&TypeRef> = fields.iter().map(|f| &f.ty).collect();
            let (size, align, offsets, unresolved) =
                walk_field_types(&tys, type_decls, &mut visited);
            LayoutInfo {
                size: Some(size),
                align: Some(align),
                pointer_offsets: offsets,
                variants: Vec::new(),
                unresolved,
            }
        }
        TypeDeclKind::Sum(variants) => {
            layout_of_sum(td, variants, type_decls, &mut visited)
        }
        // Newtype / alias — transparent: the layout is the inner type's, but
        // a newtype's field-level GC offsets are the inner classification's
        // relative offsets (the newtype has no extra header).
        TypeDeclKind::Newtype(inner) | TypeDeclKind::Alias(inner) => {
            let fc = classify_field(inner, type_decls, &mut visited);
            LayoutInfo {
                size: Some(fc.size),
                align: Some(fc.align),
                pointer_offsets: {
                    let mut v = fc.rel_pointer_offsets;
                    v.sort_unstable();
                    v
                },
                variants: Vec::new(),
                unresolved: fc.unresolved,
            }
        }
        // No concrete layout — report unresolved (consumer scans every word).
        TypeDeclKind::Opaque
        | TypeDeclKind::Effect(_)
        | TypeDeclKind::Protocol { .. } => LayoutInfo::unresolved(),
    }
}

/// Compute the per-variant GC bitmaps for a sum type.
///
/// Emitted C layout (emit_c::emit_sum_type): `struct { Nova_T_Tag tag; union {
/// <per-variant sub-struct> } payload; }`.  The tag is field 0 (C enum, 4
/// bytes / align 4 — matches `type_decl_size_or_align`'s hardcoded
/// tag_size=4/tag_align=4).  The payload union starts at
/// `align_up(4, max_payload_align)`.  Each variant's fields are laid out
/// INDEPENDENTLY from the payload base, so per-variant offsets are
/// `payload_base + (per-variant field offset)`.  The tag at offset 0 is SCALAR
/// and never appears in any variant bitmap.
fn layout_of_sum(
    td: &TypeDecl,
    variants: &[SumVariant],
    type_decls: &HashMap<String, TypeDecl>,
    _visited: &mut HashSet<String>,
) -> LayoutInfo {
    const TAG_SIZE: usize = 4;
    const TAG_ALIGN: usize = 4;

    // First pass: per-variant (relative payload offsets, payload size/align).
    struct VarTmp {
        name: String,
        rel_offsets: Vec<usize>,
        payload_size: usize,
        payload_align: usize,
        unresolved: bool,
    }
    let mut tmps: Vec<VarTmp> = Vec::with_capacity(variants.len());
    let mut max_payload_align = 1usize;
    let mut any_unresolved = false;

    for v in variants {
        let mut visited: HashSet<String> = HashSet::new();
        let (psize, palign, offsets, unresolved) = match &v.kind {
            SumVariantKind::Unit => (0usize, 1usize, Vec::new(), false),
            SumVariantKind::Tuple(types) => {
                let tys: Vec<&TypeRef> = types.iter().collect();
                walk_field_types(&tys, type_decls, &mut visited)
            }
            SumVariantKind::Record(fields) => {
                walk_record_fields(fields, type_decls, &mut visited)
            }
        };
        if palign > max_payload_align {
            max_payload_align = palign;
        }
        if unresolved {
            any_unresolved = true;
        }
        tmps.push(VarTmp {
            name: v.name.clone(),
            rel_offsets: offsets,
            payload_size: psize,
            payload_align: palign,
            unresolved,
        });
    }

    let payload_base = align_up(TAG_SIZE, max_payload_align);
    let max_align = TAG_ALIGN.max(max_payload_align);

    // Total size = tag + pad + max payload, tail-padded to max align.
    let mut max_payload_size = 0usize;
    for t in &tmps {
        if t.payload_size > max_payload_size {
            max_payload_size = t.payload_size;
        }
    }
    let size = align_up(payload_base + max_payload_size, max_align);

    // Second pass: shift each variant's relative offsets by payload_base.
    let mut variant_layouts: Vec<VariantLayout> = Vec::with_capacity(tmps.len());
    for (tag, t) in tmps.iter().enumerate() {
        let mut abs: Vec<usize> = t.rel_offsets.iter().map(|o| payload_base + o).collect();
        abs.sort_unstable();
        variant_layouts.push(VariantLayout {
            name: t.name.clone(),
            tag,
            pointer_offsets: abs,
        });
        if t.unresolved {
            any_unresolved = true;
        }
    }

    // NOTE: `size` above is emit-accurate — it comes from the boxing-aware
    // per-variant field walk, which treats heap-boxed record/sum/Vec fields as
    // 8-byte pointers (so a recursive variant like `Node(Tree, Tree)` resolves
    // its `Tree` fields to pointer-leaves and never recurses). A former
    // cross-check against const_fn_eval's `type_size_or_align_resolved` was
    // REMOVED: that size-math is boxing-UNAWARE — it inlines a field of the
    // sum's own type instead of treating it as a boxed pointer — and therefore
    // infinite-recurses → STACK OVERFLOW on ANY recursive sum type (e.g.
    // `Tree = Leaf | Node(Tree, Tree)`), including valid ones that `nova check`
    // accepts. The per-variant walk is the authoritative, recursion-safe source.

    LayoutInfo {
        size: Some(size),
        align: Some(max_align),
        pointer_offsets: Vec::new(),
        variants: variant_layouts,
        unresolved: any_unresolved,
    }
}


// =============================================================================
//                                 driver
// =============================================================================

/// Compute the per-type GC pointer-offset bitmap map over a type universe.
///
/// `type_decls` is the resolved TypeDecl registry (built via
/// `const_fn_eval::build_type_decl_registry` over the module set, mirroring how
/// the const-eval / size_of pass populates it).  Every record / named-tuple /
/// sum / newtype in the registry gets an entry.  The built-in value type `str`
/// is injected so consumers can query it directly.
///
/// EMIT-NOTHING: this is consulted ONLY by the CLI + tests; no caller in
/// `emit_c.rs` invokes it and no layout-id is written to object headers.
pub fn compute_gc_layout(type_decls: &HashMap<String, TypeDecl>) -> GcLayoutMap {
    let mut by_name: HashMap<String, LayoutInfo> = HashMap::new();

    for (name, td) in type_decls {
        let info = layout_of_decl(name, td, type_decls);
        by_name.insert(name.clone(), info);
    }

    // Inject the built-in `str` value type so it is queryable directly.
    by_name
        .entry("str".to_string())
        .or_insert_with(|| LayoutInfo {
            size: Some(16),
            align: Some(8),
            // ptr @0 is the single GC-pointer slot; len @8 is scalar.
            pointer_offsets: vec![0],
            variants: Vec::new(),
            unresolved: false,
        });

    let populated = !by_name.is_empty();
    GcLayoutMap { by_name, populated }
}

// =============================================================================
//                                   TESTS
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    // Only the tests still reference the const_fn_eval size-math (to assert the
    // boxing-aware walk agrees on NON-recursive types); production code no longer
    // calls it (see the removed sum cross-check — it infinite-recursed on
    // recursive sums).
    use crate::const_fn_eval::type_size_or_align_resolved;
    use crate::ast::*;
    use crate::diag::Span;

    fn sp() -> Span {
        Span::default()
    }

    fn ty(name: &str) -> TypeRef {
        TypeRef::Named { path: vec![name.to_string()], generics: vec![], span: sp() }
    }

    fn ty_array(inner: TypeRef) -> TypeRef {
        TypeRef::Array(Box::new(inner), sp())
    }

    /// `Option[inner]`.
    fn ty_option(inner: TypeRef) -> TypeRef {
        TypeRef::Named { path: vec!["Option".to_string()], generics: vec![inner], span: sp() }
    }

    /// `Result[ok, err]`.
    fn ty_result(ok: TypeRef, err: TypeRef) -> TypeRef {
        TypeRef::Named { path: vec!["Result".to_string()], generics: vec![ok, err], span: sp() }
    }

    /// An anonymous `protocol { }` type (lowers to `void*`).
    fn ty_protocol() -> TypeRef {
        TypeRef::Protocol { methods: Vec::new(), span: sp() }
    }

    fn rfield(name: &str, t: TypeRef) -> RecordField {
        RecordField { name: name.to_string(), ty: t, span: sp(), ..Default::default() }
    }

    /// A record TypeDecl with the given allocation kind.
    fn record_decl(name: &str, alloc: AllocKind, fields: Vec<RecordField>) -> TypeDecl {
        TypeDecl {
            name: name.to_string(),
            kind: TypeDeclKind::Record(fields),
            allocation: alloc,
            span: sp(),
            ..Default::default()
        }
    }

    fn sum_decl(name: &str, variants: Vec<SumVariant>) -> TypeDecl {
        TypeDecl {
            name: name.to_string(),
            kind: TypeDeclKind::Sum(variants),
            span: sp(),
            ..Default::default()
        }
    }

    fn unit_variant(name: &str) -> SumVariant {
        SumVariant { name: name.to_string(), kind: SumVariantKind::Unit, discriminant: None, span: sp() }
    }

    fn tuple_variant(name: &str, types: Vec<TypeRef>) -> SumVariant {
        SumVariant {
            name: name.to_string(),
            kind: SumVariantKind::Tuple(types),
            discriminant: None,
            span: sp(),
        }
    }

    fn registry(decls: Vec<TypeDecl>) -> HashMap<String, TypeDecl> {
        decls.into_iter().map(|d| (d.name.clone(), d)).collect()
    }

    // ---- the tests ----

    /// Pure-scalar record → EMPTY bitmap; size/align match the size-math.
    #[test]
    fn scalar_only_record_empty_bitmap() {
        // type Point { x int, y int }
        let pt = record_decl(
            "Point",
            AllocKind::Heap,
            vec![rfield("x", ty("int")), rfield("y", ty("int"))],
        );
        let reg = registry(vec![pt]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Point").unwrap();
        assert_eq!(info.pointer_offsets, Vec::<usize>::new(), "scalar-only record has no GC slots");
        assert_eq!(info.size, Some(16));
        assert_eq!(info.align, Some(8));
        assert!(!info.unresolved);
        // Cross-check against the canonical size-math.
        // [M-checker-recursive-type-overflow]: `type_size_or_align_resolved`
        // is now BOXING-aware — a reference to a HEAP record (`Point`, no
        // `value` kw) is a `Nova_Point*` (8 bytes, reference semantics), which
        // is what `size_of[Point]()` and every field/var occurrence resolves
        // to. `compute_gc_layout` reports the INLINE OBJECT size (16) because
        // `Point` is the TOP-LEVEL subject of the bitmap (the bytes inside the
        // heap allocation). These two quantities legitimately differ for a heap
        // type (object size vs reference size); they coincide only for value
        // types. So the cross-check now asserts the size-math yields the
        // pointer size, NOT the inline object size.
        let math = type_size_or_align_resolved(&ty("Point"), false, &reg).unwrap();
        assert_eq!(math, 8, "heap record reference is a Nova_Point* (8 bytes)");
        let align_math = type_size_or_align_resolved(&ty("Point"), true, &reg).unwrap();
        assert_eq!(align_math, 8, "heap record pointer align is 8");
    }

    /// Record with mixed scalar + boxed-record field → bitmap = the boxed
    /// field's offset only.
    #[test]
    fn record_mixed_ptr_and_scalar() {
        // type Inner { a int }                (heap → Nova_Inner*)
        // type Outer { tag int, child Inner, n int }
        // layout: tag@0 (8), child@8 (ptr 8), n@16 (8) → ptr offset {8}
        let inner = record_decl("Inner", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let outer = record_decl(
            "Outer",
            AllocKind::Heap,
            vec![rfield("tag", ty("int")), rfield("child", ty("Inner")), rfield("n", ty("int"))],
        );
        let reg = registry(vec![inner, outer]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Outer").unwrap();
        assert_eq!(info.pointer_offsets, vec![8], "only the boxed-record field offset is a GC slot");
        assert_eq!(info.size, Some(24));
        assert_eq!(info.align, Some(8));
    }

    /// Record with a heap container field (Vec via `[]T`) → that offset marked.
    #[test]
    fn record_with_vec_field_marked() {
        // type Buf { len int, items []int }
        // len@0 (8), items@8 (Vec ptr 8) → {8}
        let buf = record_decl(
            "Buf",
            AllocKind::Heap,
            vec![rfield("len", ty("int")), rfield("items", ty_array(ty("int")))],
        );
        let reg = registry(vec![buf]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Buf").unwrap();
        assert_eq!(info.pointer_offsets, vec![8], "Vec field is a single GC pointer slot");
        assert_eq!(info.size, Some(16));
    }

    /// Record with a `str` field → the str.ptr offset (field_offset + 0)
    /// marked; the embedded `len` (field_offset + 8) is NOT.
    #[test]
    fn record_with_str_field_recursed() {
        // type Named { id int, name str }
        // id@0 (8), name@8 (str: ptr@8 GC, len@16 scalar) → {8}
        let named = record_decl(
            "Named",
            AllocKind::Heap,
            vec![rfield("id", ty("int")), rfield("name", ty("str"))],
        );
        let reg = registry(vec![named]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Named").unwrap();
        assert_eq!(info.pointer_offsets, vec![8], "str.ptr at field offset is the only GC slot; len is scalar");
        // id(8) + str(16) = 24
        assert_eq!(info.size, Some(24));
    }

    /// Nested VALUE-record → recursed offsets (inline, not one opaque pointer).
    #[test]
    fn nested_value_record_recursed() {
        // type Vptr value { p Box }            (Box heap → Nova_Box*)
        // type Holder { x int, v Vptr, y int }
        // Vptr is a VALUE record (inline): contains one pointer at its offset 0.
        // Holder: x@0 (8), v@8 (Vptr inline, ptr@8), y@16 (8) → {8}
        let boxd = record_decl("Box", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let vptr = record_decl("Vptr", AllocKind::Value, vec![rfield("p", ty("Box"))]);
        let holder = record_decl(
            "Holder",
            AllocKind::Heap,
            vec![rfield("x", ty("int")), rfield("v", ty("Vptr")), rfield("y", ty("int"))],
        );
        let reg = registry(vec![boxd, vptr, holder]);
        let map = compute_gc_layout(&reg);

        // The value-record itself, as a top-level subject, has its ptr at @0.
        let vinfo = map.get("Vptr").unwrap();
        assert_eq!(vinfo.pointer_offsets, vec![0]);
        assert_eq!(vinfo.size, Some(8));

        // Embedded inline in Holder, the nested ptr lands at field offset 8.
        let hinfo = map.get("Holder").unwrap();
        assert_eq!(hinfo.pointer_offsets, vec![8], "nested value-record ptr recursed to its embedded offset");
        assert_eq!(hinfo.size, Some(24));
    }

    /// Nested value-record embedding str → recursed str.ptr offset.
    #[test]
    fn nested_value_record_with_str() {
        // type Pair value { k str, n int }      (value: str inline + int)
        // type Wrap { lead int, pair Pair }
        // Pair inline: k=str(ptr@0,len@8), n@16 → ptr {0}
        // Wrap: lead@0(8), pair@8 (Pair inline → ptr@8) → {8}
        let pair = record_decl(
            "Pair",
            AllocKind::Value,
            vec![rfield("k", ty("str")), rfield("n", ty("int"))],
        );
        let wrap = record_decl(
            "Wrap",
            AllocKind::Heap,
            vec![rfield("lead", ty("int")), rfield("pair", ty("Pair"))],
        );
        let reg = registry(vec![pair, wrap]);
        let map = compute_gc_layout(&reg);

        let pinfo = map.get("Pair").unwrap();
        assert_eq!(pinfo.pointer_offsets, vec![0], "value Pair: str.ptr@0; len@8 + n@16 scalar");
        assert_eq!(pinfo.size, Some(24));

        let winfo = map.get("Wrap").unwrap();
        assert_eq!(winfo.pointer_offsets, vec![8], "nested value Pair's str.ptr recursed to offset 8");
        assert_eq!(winfo.size, Some(32)); // lead(8) + Pair(24)
    }

    /// Sum type → PER-VARIANT bitmaps keyed by tag; tag@0 is scalar; a variant
    /// with a non-GC scalar payload proves per-variant precision (its slot is
    /// NOT marked).
    #[test]
    fn sum_per_variant_bitmaps() {
        // type Node {
        //   Leaf,                   (unit → empty bitmap)
        //   Count(int),             (scalar payload → empty bitmap)
        //   Cons(Box),              (heap record → one ptr)
        // }
        // tag@0 (4), payload starts at align_up(4, 8) = 8.
        // Leaf:  []
        // Count: [] (the int payload at @8 is SCALAR — per-variant precision!)
        // Cons:  [8] (Box ptr at payload base 8)
        let boxd = record_decl("Box", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let node = sum_decl(
            "Node",
            vec![
                unit_variant("Leaf"),
                tuple_variant("Count", vec![ty("int")]),
                tuple_variant("Cons", vec![ty("Box")]),
            ],
        );
        let reg = registry(vec![boxd, node]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Node").unwrap();

        // Top-level pointer_offsets empty (offsets live per-variant).
        assert_eq!(info.pointer_offsets, Vec::<usize>::new());
        assert_eq!(info.variants.len(), 3);

        let leaf = &info.variants[0];
        assert_eq!(leaf.name, "Leaf");
        assert_eq!(leaf.tag, 0);
        assert_eq!(leaf.pointer_offsets, Vec::<usize>::new(), "unit variant: no GC slots");

        let count = &info.variants[1];
        assert_eq!(count.name, "Count");
        assert_eq!(count.tag, 1);
        assert_eq!(
            count.pointer_offsets,
            Vec::<usize>::new(),
            "scalar-payload variant: int@8 is NOT scanned as a pointer (per-variant precision)"
        );

        let cons = &info.variants[2];
        assert_eq!(cons.name, "Cons");
        assert_eq!(cons.tag, 2);
        assert_eq!(cons.pointer_offsets, vec![8], "Cons(Box): boxed ptr at payload base 8");

        // Size cross-check vs the canonical size-math.
        // [M-checker-recursive-type-overflow]: sum types are ALWAYS heap-boxed
        // (`Nova_Node*`), so `type_size_or_align_resolved` is now boxing-aware
        // and returns the pointer size (8) for a `Node` reference — what
        // `size_of[Node]()` and every field/var occurrence resolves to. The gc
        // bitmap `info.size` is the INLINE union object size (16) for the
        // top-level subject. Those differ for a heap type by design (object vs
        // reference size); pre-fix the size-math inlined the recursive `Box`
        // field and infinite-recursed (the very bug this guards). Assert the
        // size-math now yields pointer size.
        let math = type_size_or_align_resolved(&ty("Node"), false, &reg).unwrap();
        assert_eq!(math, 8, "heap sum reference is a Nova_Node* (8 bytes)");
        let align_math = type_size_or_align_resolved(&ty("Node"), true, &reg).unwrap();
        assert_eq!(align_math, 8, "heap sum pointer align is 8");
    }

    /// Sum variant with a str payload → recursed str.ptr at payload base.
    #[test]
    fn sum_variant_with_str_payload() {
        // type Msg { Empty, Text(str) }
        // payload base = align_up(4, 8) = 8; Text: str.ptr @8.
        let msg = sum_decl(
            "Msg",
            vec![unit_variant("Empty"), tuple_variant("Text", vec![ty("str")])],
        );
        let reg = registry(vec![msg]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Msg").unwrap();
        assert_eq!(info.variants[0].pointer_offsets, Vec::<usize>::new());
        assert_eq!(info.variants[1].pointer_offsets, vec![8], "Text(str): str.ptr @ payload base 8");
    }

    /// Record variant in a sum → per-field offsets within the variant struct.
    #[test]
    fn sum_record_variant_offsets() {
        // type Shape { Dot, Seg { from Box, n int, to Box } }
        // payload base = 8. Seg struct: from@0(ptr), n@8(scalar), to@16(ptr)
        //   → absolute {8, 24}
        let boxd = record_decl("Box", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let shape = sum_decl(
            "Shape",
            vec![
                unit_variant("Dot"),
                SumVariant {
                    name: "Seg".to_string(),
                    kind: SumVariantKind::Record(vec![
                        rfield("from", ty("Box")),
                        rfield("n", ty("int")),
                        rfield("to", ty("Box")),
                    ]),
                    discriminant: None,
                    span: sp(),
                },
            ],
        );
        let reg = registry(vec![boxd, shape]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Shape").unwrap();
        assert_eq!(info.variants[1].pointer_offsets, vec![8, 24], "Seg: from@8 + to@24, n@16 scalar");
    }

    #[test]
    fn recursive_sum_does_not_overflow() {
        // type Tree { Leaf, Node(Tree, Tree) } — a VALID recursive sum (boxed).
        // REGRESSION: computing its layout used to STACK-OVERFLOW via the (now
        // removed) const_fn_eval size cross-check, which inlined the recursive
        // `Tree` field instead of treating it as a boxed 8-byte pointer. The
        // boxing-aware per-variant walk treats each `Tree` field as a pointer
        // leaf, so this resolves without recursion. `nova check` accepts such a
        // type, so gc-layout-analyze must not crash on it.
        let tree = sum_decl(
            "Tree",
            vec![
                unit_variant("Leaf"),
                tuple_variant("Node", vec![ty("Tree"), ty("Tree")]),
            ],
        );
        let reg = registry(vec![tree]);
        let map = compute_gc_layout(&reg); // must NOT overflow
        let info = map.get("Tree").expect("Tree must resolve");
        assert_eq!(info.variants.len(), 2, "Leaf + Node");
        assert!(
            info.variants[0].pointer_offsets.is_empty(),
            "Leaf has no payload → no pointers"
        );
        assert_eq!(
            info.variants[1].pointer_offsets.len(),
            2,
            "Node(Tree, Tree): both recursive fields are boxed GC pointers"
        );
    }

    /// `str` is queryable directly → ptr@0 marked, len@8 not.
    #[test]
    fn str_builtin_ptr_offset() {
        let reg: HashMap<String, TypeDecl> = HashMap::new();
        // Empty registry still injects `str`.
        let map = compute_gc_layout(&reg);
        let info = map.get("str").unwrap();
        assert_eq!(info.pointer_offsets, vec![0], "str.ptr@0 is the GC slot");
        assert_eq!(info.size, Some(16));
        assert_eq!(info.align, Some(8));
        assert!(!info.unresolved);
    }

    /// Raw FFI pointer field (`*ro u8`) is NON-GC; a Vec field beside it IS GC —
    /// proves raw pointers are not over-marked while heap pointers are.
    #[test]
    fn raw_ptr_field_not_marked() {
        // type FfiView { raw *ro u8, owned []int }
        // raw@0 (ptr 8, NON-GC), owned@8 (Vec ptr 8, GC) → {8}
        let raw_ty = TypeRef::Pointer(
            Box::new(TypeRef::Readonly(Box::new(ty("u8")), sp())),
            sp(),
        );
        let view = record_decl(
            "FfiView",
            AllocKind::Heap,
            vec![rfield("raw", raw_ty), rfield("owned", ty_array(ty("int")))],
        );
        let reg = registry(vec![view]);
        let map = compute_gc_layout(&reg);
        let info = map.get("FfiView").unwrap();
        assert_eq!(info.pointer_offsets, vec![8], "raw FFI ptr non-GC; only the Vec field is a GC slot");
        assert_eq!(info.size, Some(16));
    }

    /// Char field is a SCALAR sized 4 (nova_char = uint32_t, D128 AMEND Plan 152.8).
    /// Verify the following GC field's offset is 8 (4-byte char + 4-byte padding → align 8).
    #[test]
    fn char_is_scalar_four_bytes() {
        // type WithChar { c char, b Box }
        // c@0 (4, scalar), padding@4 (4), b@8 (ptr) → {8}
        let boxd = record_decl("Box", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let wc = record_decl(
            "WithChar",
            AllocKind::Heap,
            vec![rfield("c", ty("char")), rfield("b", ty("Box"))],
        );
        let reg = registry(vec![boxd, wc]);
        let map = compute_gc_layout(&reg);
        let info = map.get("WithChar").unwrap();
        assert_eq!(info.pointer_offsets, vec![8], "char is 4 bytes (nova_char=uint32_t) → padding to align 8 → Box at offset 8");
        assert_eq!(info.size, Some(16));
    }

    /// Unknown / unresolved field type → conservatively marked as a pointer AND
    /// the type flagged unresolved (never silently treated as scalar).
    #[test]
    fn unknown_field_conservative_pointer() {
        // type Mystery { g SomeUnknownGeneric }   (no decl in registry)
        let myst = record_decl(
            "Mystery",
            AllocKind::Heap,
            vec![rfield("g", ty("SomeUnknownGeneric"))],
        );
        let reg = registry(vec![myst]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Mystery").unwrap();
        assert_eq!(info.pointer_offsets, vec![0], "unknown field over-approximated as a GC pointer");
        assert!(info.unresolved, "unknown field flags the type unresolved (consumer scans conservatively)");
    }

    // ---- Option / Result field layout (review findings #1 / #2) ----

    /// NPO `Option[T]` where T is a HEAP record → inner C type `Nova_Inner*`
    /// ends in `*`, so emit_c lowers it to `{ Nova_Inner* value; }` — ONE 8-byte
    /// GC pointer @0.  Verify the NPO fast path is preserved.
    #[test]
    fn option_npo_pointer_inner() {
        // type Inner { a int }   (heap → Nova_Inner*)
        // type H { opt Option[Inner] }   → opt@0 (NovaOpt NPO, ptr@0) → {0}
        let inner = record_decl("Inner", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let h = record_decl(
            "H",
            AllocKind::Heap,
            vec![rfield("opt", ty_option(ty("Inner")))],
        );
        let reg = registry(vec![inner, h]);
        let map = compute_gc_layout(&reg);
        let info = map.get("H").unwrap();
        assert_eq!(info.pointer_offsets, vec![0], "NPO Option[heap-record] is a single GC pointer @0");
        assert_eq!(info.size, Some(8), "NPO Option lowers to one 8-byte pointer");
        assert!(!info.unresolved);
    }

    /// Tagged `Option[str]` followed by a heap field — the crux of finding #1.
    /// emit_c lowers `Option[str]` to `{ int tag; nova_str value; }`: tag@0 (8,
    /// scalar), str.ptr@(base+0), str.len@(base+8).  payload base = align_up(8,
    /// align(str)=8) = 8, so str.ptr is at field+8 (GC), str.len at field+16
    /// (scalar); the Option field spans 8..32 → size 24.  The trailing heap
    /// `after` field then lands at offset (8 + 24)=32 and MUST be marked.
    #[test]
    fn option_str_field_tagged_recursed() {
        // type Inner { a int }   (heap → Nova_Inner*)
        // type OptStrHolder { lead int, opt Option[str], after Inner }
        // lead@0(8) | opt@8 (tag@8 scalar, str.ptr@16 GC, str.len@24 scalar; 24B)
        //          | after@32 (Nova_Inner* GC)
        // → size 40, ptr_offsets {16, 32}
        let inner = record_decl("Inner", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let holder = record_decl(
            "OptStrHolder",
            AllocKind::Heap,
            vec![
                rfield("lead", ty("int")),
                rfield("opt", ty_option(ty("str"))),
                rfield("after", ty("Inner")),
            ],
        );
        let reg = registry(vec![inner, holder]);
        let map = compute_gc_layout(&reg);
        let info = map.get("OptStrHolder").unwrap();
        assert_eq!(
            info.pointer_offsets,
            vec![16, 32],
            "tagged Option[str]: str.ptr@16 GC; trailing heap field @32 NOT shifted off"
        );
        assert_eq!(info.size, Some(40), "lead(8)+Option[str](24)+Inner ptr(8)=40");
        assert!(!info.unresolved);
    }

    /// Tagged `Option[int]` carries NO GC pointer (its payload is a scalar), and
    /// must NOT shift the trailing heap field off the bitmap — finding #1 case 2.
    #[test]
    fn option_int_field_tagged_no_inner_ptr() {
        // type Inner { a int }   (heap)
        // type OptInt { lead int, opt Option[int], after Inner }
        // lead@0(8) | opt@8 = {int tag@8; nova_int value@16} = 16B, NO GC ptr
        //          | after@24 (Nova_Inner* GC)
        // → size 32, ptr_offsets {24}
        let inner = record_decl("Inner", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let holder = record_decl(
            "OptInt",
            AllocKind::Heap,
            vec![
                rfield("lead", ty("int")),
                rfield("opt", ty_option(ty("int"))),
                rfield("after", ty("Inner")),
            ],
        );
        let reg = registry(vec![inner, holder]);
        let map = compute_gc_layout(&reg);
        let info = map.get("OptInt").unwrap();
        assert_eq!(
            info.pointer_offsets,
            vec![24],
            "Option[int] has no inner GC ptr; only the trailing heap field @24 is marked"
        );
        assert_eq!(info.size, Some(32), "lead(8)+Option[int](16)+Inner ptr(8)=32");
        assert!(!info.unresolved);
    }

    /// Record with TWO `Option[str]` fields then a heap field — finding #2's
    /// exact reproduction (`type Rec { a Option[str], b Option[str], c Box }`).
    /// Each Option[str] is 24 bytes with str.ptr at its base+8.
    #[test]
    fn record_two_option_str_then_heap() {
        // a@0 = Option[str] (tag@0, str.ptr@8 GC, str.len@16) 24B
        // b@24 = Option[str] (tag@24, str.ptr@32 GC, str.len@40) 24B
        // c@48 = Nova_Box* GC
        // → size 56, ptr_offsets {8, 32, 48}
        let boxd = record_decl("Box", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let rec = record_decl(
            "Rec",
            AllocKind::Heap,
            vec![
                rfield("a", ty_option(ty("str"))),
                rfield("b", ty_option(ty("str"))),
                rfield("c", ty("Box")),
            ],
        );
        let reg = registry(vec![boxd, rec]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Rec").unwrap();
        assert_eq!(
            info.pointer_offsets,
            vec![8, 32, 48],
            "two Option[str] str.ptrs @8/@32 + heap field @48; none missed"
        );
        assert_eq!(info.size, Some(56));
        assert!(!info.unresolved);
    }

    /// `Result[T,E]` field is a single `NovaRes_*` heap pointer @0; a heap field
    /// after it is NOT shifted (Result keeps the single-pointer model).
    #[test]
    fn result_field_single_pointer() {
        // type Box { a int }   (heap)
        // type R { lead int, res Result[int, str], after Box }
        // lead@0(8) | res@8 (NovaRes_* ptr@8) | after@16 (Nova_Box* @16)
        // → size 24, ptr_offsets {8, 16}
        let boxd = record_decl("Box", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let r = record_decl(
            "R",
            AllocKind::Heap,
            vec![
                rfield("lead", ty("int")),
                rfield("res", ty_result(ty("int"), ty("str"))),
                rfield("after", ty("Box")),
            ],
        );
        let reg = registry(vec![boxd, r]);
        let map = compute_gc_layout(&reg);
        let info = map.get("R").unwrap();
        assert_eq!(
            info.pointer_offsets,
            vec![8, 16],
            "Result is one NovaRes_* ptr@8; trailing heap field @16 marked"
        );
        assert_eq!(info.size, Some(24));
    }

    /// `Option[Box]` (NPO, pointer inner) followed by a heap field — NPO path
    /// keeps the field 8 bytes so the next pointer offset is accurate.
    #[test]
    fn option_npo_then_heap_field_offsets_accurate() {
        // type Box { a int }   (heap)
        // type N { lead int, opt Option[Box], after Box }
        // lead@0(8) | opt@8 (NovaOpt NPO ptr@8) | after@16 (Nova_Box* @16)
        // → size 24, ptr_offsets {8, 16}
        let boxd = record_decl("Box", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let n = record_decl(
            "N",
            AllocKind::Heap,
            vec![
                rfield("lead", ty("int")),
                rfield("opt", ty_option(ty("Box"))),
                rfield("after", ty("Box")),
            ],
        );
        let reg = registry(vec![boxd, n]);
        let map = compute_gc_layout(&reg);
        let info = map.get("N").unwrap();
        assert_eq!(info.pointer_offsets, vec![8, 16], "NPO Option@8 (8B) + heap @16");
        assert_eq!(info.size, Some(24));
    }

    // ---- anonymous protocol field layout (review finding #3) ----

    /// Anonymous `protocol { }` field is `void*` (8 bytes), ONE GC pointer @0;
    /// a heap field after it must land at the correct (un-shifted) offset.
    #[test]
    fn anon_protocol_field_is_void_ptr_eight_bytes() {
        // type Box { a int }   (heap)
        // type Holder { lead int, p protocol{}, b Box }
        // lead@0(8) | p@8 (void* GC) | b@16 (Nova_Box* GC)
        // → size 24, ptr_offsets {8, 16}
        let boxd = record_decl("Box", AllocKind::Heap, vec![rfield("a", ty("int"))]);
        let holder = record_decl(
            "Holder",
            AllocKind::Heap,
            vec![
                rfield("lead", ty("int")),
                rfield("p", ty_protocol()),
                rfield("b", ty("Box")),
            ],
        );
        let reg = registry(vec![boxd, holder]);
        let map = compute_gc_layout(&reg);
        let info = map.get("Holder").unwrap();
        assert_eq!(
            info.pointer_offsets,
            vec![8, 16],
            "anon protocol void* @8 (8B); trailing heap field @16 NOT @24"
        );
        assert_eq!(info.size, Some(24), "lead(8)+void*(8)+Nova_Box*(8)=24");
        assert!(info.unresolved, "anon-protocol object model is opaque → over-approximate");
    }

    /// Empty/default map is not populated.
    #[test]
    fn default_map_not_populated() {
        let map = GcLayoutMap::default();
        assert!(!map.populated());
        assert!(map.get("Anything").is_none());
    }
}
