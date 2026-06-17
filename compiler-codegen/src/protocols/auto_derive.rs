// Plan 126 (D230) Ф.2: auto-derive synthesis infrastructure + cycle detection.
// Plan 137 (D237): protocol rename — Equal/Hash/Clone/Compare/Display/Debug.
//
// Этот модуль предоставляет foundation для synthesis memberwise рекурсивного
// AST FnDecl для built-in protocol methods. Per-protocol synthesizer bodies —
// в Ф.3 (next commit).
//
// **Supported protocols** (built-in, Ф.3 implements):
// - Equal   → `@equal(other Self) -> bool`
// - Hash    → `@hash() -> u64`
// - Clone   → `@clone() -> Self` (D230 NEW)
// - Compare → `@compare(other Self) -> int`
// - Display → `@display(sb StringBuilder) -> ()`
// - Debug   → `@debug(sb StringBuilder) -> ()`
//
// **Field iteration (Ф.2):**
// - `TypeDeclKind::Record(fields)` — for-each по `RecordField.name`.
// - `TypeDeclKind::NamedTuple(fields)` — for-each по `NamedTupleField.name`.
// - `TypeDeclKind::Sum(variants)` — собирает variants через `iter_sum_variants`.
// - Другие kinds — не поддерживаются (`UnsupportedTypeKind`).
//
// **Cycle detection (Ф.2):**
// Visited set по парам `(type_name, protocol_name)`. Synthesizer вызывает
// `mark_visiting` перед рекурсией; duplicate → `DeriveError::Cycle`.
//
// **Field eligibility (Ф.2):**
// Каждое поле type'а должно либо:
// - быть primitive (`int`/`f64`/`bool`/`char`/`byte`/`str`/`u*`/`i*`/`f*`),
// - иметь `#impl(P)` annotation на своём type-decl, OR
// - предоставлять explicit method (`fn FieldType @method(...)`).

use std::collections::HashSet;

use crate::ast::{
    BinOp, Block, CallArg, Expr, ExprKind, FnBody, FnDecl, NamedTupleField, Param, RecordField,
    RecordLitField, Receiver, ReceiverKind, Stmt, SumVariant, TypeDecl, TypeDeclKind, TypeRef,
};
use crate::diag::Span;

/// Имена built-in protocols, поддерживаемых auto-derive (Plan 126; D237 rename).
pub const EQUAL:   &str = "Equal";
pub const HASH:    &str = "Hash";
pub const CLONE:   &str = "Clone";
pub const COMPARE: &str = "Compare";
pub const DISPLAY: &str = "Display";
pub const DEBUG:   &str = "Debug";

/// True если `proto_name` — один из known built-in protocols.
pub fn is_builtin_protocol(proto_name: &str) -> bool {
    matches!(
        proto_name,
        EQUAL | HASH | CLONE | COMPARE | DISPLAY | DEBUG
    )
}

/// Получить имя метода built-in protocol'а (single-method assumption).
/// Returns None for unknown protocol.
pub fn builtin_protocol_method(proto_name: &str) -> Option<&'static str> {
    match proto_name {
        EQUAL   => Some("equal"),
        HASH    => Some("hash"),
        CLONE   => Some("clone"),
        COMPARE => Some("compare"),
        DISPLAY => Some("display"),
        DEBUG   => Some("debug"),
        _ => None,
    }
}

/// Имена примитивных типов Nova bootstrap. Используются для
/// field-eligibility check'а — primitive поля всегда eligible.
// Plan 133: usize/isize removed; int=intptr_t, uint=uintptr_t.
pub const NOVA_PRIMITIVES: &[&str] = &[
    "int", "uint", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64",
    "f32", "f64", "bool", "char", "byte", "str", "u128", "i128",
];

/// True если type-name — primitive.
pub fn is_primitive_type(name: &str) -> bool {
    NOVA_PRIMITIVES.contains(&name)
}

/// Errors, возникающие при auto-derive synthesis.
#[derive(Debug, Clone, PartialEq)]
pub enum DeriveError {
    /// Cycle detected — type A references B, B references A (через embed/
    /// field), и оба пытаются auto-derive один и тот же protocol.
    /// Error code: `E_AUTO_DERIVE_CYCLE`.
    Cycle {
        type_name: String,
        protocol: String,
    },
    /// Field type doesn't implement required protocol.
    /// Error code: `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`.
    FieldLacksProtocol {
        type_name: String,
        field_name: String,
        field_type: String,
        protocol: String,
    },
    /// Protocol name unknown (not in built-in list).
    UnknownProtocol(String),
    /// Type kind не поддерживает auto-derive (Newtype/Alias/Effect/Protocol/Opaque).
    UnsupportedTypeKind {
        type_name: String,
        kind: String,
        protocol: String,
    },
}

impl DeriveError {
    /// Render to a diagnostic message with the proper error code prefix.
    pub fn diagnostic_message(&self) -> String {
        match self {
            DeriveError::Cycle { type_name, protocol } => format!(
                "[E_AUTO_DERIVE_CYCLE] type `{}` cannot auto-derive `{}` — \
                 cyclic recursion through fields would not terminate. \
                 Provide an explicit `fn {} @{}(...) -> ...` implementation.",
                type_name, protocol, type_name,
                builtin_protocol_method(protocol).unwrap_or("method"),
            ),
            DeriveError::FieldLacksProtocol {
                type_name,
                field_name,
                field_type,
                protocol,
            } => format!(
                "[E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL] type `{}` claims \
                 `#impl({})` but field `{}` (type `{}`) does not implement \
                 `{}`. Either add `#impl({})` to `{}`, or provide an explicit \
                 `fn {} @{}(...)` implementation на `{}`.",
                type_name, protocol, field_name, field_type, protocol,
                protocol, field_type, type_name,
                builtin_protocol_method(protocol).unwrap_or("method"),
                type_name,
            ),
            DeriveError::UnknownProtocol(p) => format!(
                "[E_AUTO_DERIVE_UNKNOWN_PROTOCOL] `{}` is not a built-in \
                 protocol — auto-derive supports only \
                 Equatable / Hashable / Cloneable / Comparable / Printable.",
                p,
            ),
            DeriveError::UnsupportedTypeKind {
                type_name,
                kind,
                protocol,
            } => format!(
                "[E_AUTO_DERIVE_UNSUPPORTED_KIND] type `{}` ({}) does not \
                 support auto-derive for `{}`. Provide explicit method \
                 implementation.",
                type_name, kind, protocol,
            ),
        }
    }
}

/// Trait providing query methods нужные synthesizer'у — позволяет
/// auto_derive быть unit-testable без полного TypeCheckCtx.
///
/// В production реализуется на TypeCheckCtx (через newtype wrapper).
///
/// Returns references tied к self's lifetime — позволяет mock'у владеть
/// данными напрямую, а production реализации делегировать к long-lived
/// type registry.
pub trait DeriveQuery {
    /// Lookup type declaration by name. None если type unknown.
    fn lookup_type(&self, name: &str) -> Option<&TypeDecl>;

    /// True если type `t` provides explicit method `@method_name`.
    fn type_provides_method(&self, t: &str, method_name: &str) -> bool;
}

/// Synthesis context — несёт visited set для cycle detection + ссылку
/// на query backend.
pub struct AutoDeriveCtx<'a, Q: DeriveQuery> {
    /// Backend query interface (TypeCheckCtx wrapper в production).
    pub query: &'a Q,
    /// Visited pairs (type, protocol) — для cycle detection.
    /// Synthesizer вызывает `mark_visiting` перед рекурсией; duplicate
    /// возвращает false → cycle.
    visited: HashSet<(String, String)>,
}

impl<'a, Q: DeriveQuery> AutoDeriveCtx<'a, Q> {
    pub fn new(query: &'a Q) -> Self {
        Self {
            query,
            visited: HashSet::new(),
        }
    }

    /// Push (type, protocol) в visited set. Returns false if already visited
    /// (cycle detected).
    pub fn mark_visiting(&mut self, type_name: &str, protocol: &str) -> bool {
        self.visited.insert((type_name.to_string(), protocol.to_string()))
    }

    pub fn unmark_visiting(&mut self, type_name: &str, protocol: &str) {
        self.visited.remove(&(type_name.to_string(), protocol.to_string()));
    }

    /// True если type+protocol уже в visited.
    pub fn is_visiting(&self, type_name: &str, protocol: &str) -> bool {
        self.visited.contains(&(type_name.to_string(), protocol.to_string()))
    }
}

/// Поля типа в нормализованной форме — name + type. Sum types обрабатываются
/// отдельно через `iter_sum_variants`.
#[derive(Debug, Clone)]
pub struct DerivedField {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

/// Извлечь нормализованный список fields из type-decl. Returns None если
/// type не имеет fields (Sum, Alias, Effect, etc.).
pub fn iter_fields(td: &TypeDecl) -> Option<Vec<DerivedField>> {
    match &td.kind {
        TypeDeclKind::Record(fields) => Some(
            fields.iter().map(|f: &RecordField| DerivedField {
                name: f.name.clone(),
                ty: f.ty.clone(),
                span: f.span,
            }).collect()
        ),
        TypeDeclKind::NamedTuple(fields) => Some(
            fields.iter().map(|f: &NamedTupleField| DerivedField {
                name: f.name.clone(),
                ty: f.ty.clone(),
                span: f.span,
            }).collect()
        ),
        _ => None,
    }
}

/// Извлечь variants для Sum-type. Returns None если type не Sum.
pub fn iter_sum_variants(td: &TypeDecl) -> Option<&[SumVariant]> {
    match &td.kind {
        TypeDeclKind::Sum(variants) => Some(variants.as_slice()),
        _ => None,
    }
}

/// Получить имя type'а из TypeRef::Named. Returns None если не Named.
pub fn type_ref_name(t: &TypeRef) -> Option<&str> {
    match t.strip_modifiers() {
        TypeRef::Named { path, .. } => path.last().map(|s| s.as_str()),
        _ => None,
    }
}

/// Render TypeRef как user-readable string (для diagnostics).
pub fn type_ref_render(t: &TypeRef) -> String {
    match t.strip_modifiers() {
        TypeRef::Named { path, .. } => path.join("."),
        TypeRef::Array(inner, _) => format!("[]{}", type_ref_render(inner)),
        TypeRef::FixedArray(n, inner, _) => format!("[{}]{}", n, type_ref_render(inner)),
        TypeRef::Tuple(elems, _) => {
            let parts: Vec<String> = elems.iter().map(type_ref_render).collect();
            format!("({})", parts.join(", "))
        }
        TypeRef::Unit(_) => "()".to_string(),
        _ => "<complex type>".to_string(),
    }
}

/// Type-kind name для diagnostics.
pub fn type_decl_kind_name(td: &TypeDecl) -> &'static str {
    match &td.kind {
        TypeDeclKind::Record(_) => "record",
        TypeDeclKind::Sum(_) => "sum",
        TypeDeclKind::Effect(_) => "effect",
        TypeDeclKind::Protocol { .. } => "protocol",
        TypeDeclKind::Newtype(_) => "newtype",
        TypeDeclKind::Alias(_) => "alias",
        TypeDeclKind::NamedTuple(_) => "named tuple",
        TypeDeclKind::Opaque => "opaque",
    }
}

/// Field eligibility check — поле должно быть либо primitive, либо
/// иметь explicit method, либо иметь `#impl(P)` annotation.
///
/// Для array `[]T` рекурсивно проверяем eligibility T'а.
/// Для tuple `(A, B)` рекурсивно по element types.
pub fn check_field_eligibility<Q: DeriveQuery>(
    query: &Q,
    field_type: &TypeRef,
    protocol: &str,
    method_name: &str,
) -> bool {
    match field_type.strip_modifiers() {
        TypeRef::Named { path, .. } => {
            let name = match path.last() {
                Some(n) => n.as_str(),
                None => return false,
            };
            if is_primitive_type(name) {
                return true;
            }
            // Explicit method check.
            if query.type_provides_method(name, method_name) {
                return true;
            }
            // #impl(protocol) annotation check.
            if let Some(td) = query.lookup_type(name) {
                if td.impl_protocols.iter().any(|p| p == protocol) {
                    return true;
                }
            }
            false
        }
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
            check_field_eligibility(query, inner, protocol, method_name)
        }
        TypeRef::Tuple(elems, _) => elems
            .iter()
            .all(|t| check_field_eligibility(query, t, protocol, method_name)),
        TypeRef::Unit(_) => true,
        // Func / Protocol / Pointer / Unsafe — not eligible for auto-derive.
        _ => false,
    }
}

/// Top-level synthesizer entry point — выбирает per-protocol synthesizer.
///
/// **Pre-conditions:**
/// - `protocol` is built-in (verify via `is_builtin_protocol`).
/// - `type_decl` имеет `protocol` в `impl_protocols` list.
/// - User does NOT provide explicit `fn T @<method>(...)` (verified by caller).
///
/// **Returns:**
/// - Ok(FnDecl) — synthesized method declaration ready для регистрации.
/// - Err(DeriveError) — cycle / field-eligibility / unsupported-kind / unknown.
///
/// **Ф.2 stub:** возвращает UnsupportedTypeKind для всех protocols —
/// per-protocol synthesizers landing в Ф.3 (next commit).
pub fn synthesize_method<Q: DeriveQuery>(
    ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
    protocol: &str,
) -> Result<FnDecl, DeriveError> {
    if !is_builtin_protocol(protocol) {
        return Err(DeriveError::UnknownProtocol(protocol.to_string()));
    }

    // Cycle detection — попытка пометить visit'инг.
    if !ctx.mark_visiting(&type_decl.name, protocol) {
        return Err(DeriveError::Cycle {
            type_name: type_decl.name.clone(),
            protocol: protocol.to_string(),
        });
    }

    let method_name = builtin_protocol_method(protocol)
        .expect("is_builtin_protocol guarded above");

    // Field eligibility check up-front.
    let result = synthesize_method_inner(ctx, type_decl, protocol, method_name);

    ctx.unmark_visiting(&type_decl.name, protocol);
    result
}

fn synthesize_method_inner<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
    protocol: &str,
    method_name: &str,
) -> Result<FnDecl, DeriveError> {
    // Validate field eligibility (kind-dependent).
    if let Some(fields) = iter_fields(type_decl) {
        for f in &fields {
            if !check_field_eligibility(_ctx.query, &f.ty, protocol, method_name) {
                return Err(DeriveError::FieldLacksProtocol {
                    type_name: type_decl.name.clone(),
                    field_name: f.name.clone(),
                    field_type: type_ref_render(&f.ty),
                    protocol: protocol.to_string(),
                });
            }
        }
    } else if iter_sum_variants(type_decl).is_none() {
        let kind_name = type_decl_kind_name(type_decl);
        return Err(DeriveError::UnsupportedTypeKind {
            type_name: type_decl.name.clone(),
            kind: kind_name.to_string(),
            protocol: protocol.to_string(),
        });
    }

    // Ф.3: dispatch к per-protocol synthesizer body builders.
    match protocol {
        EQUAL   => synthesize_equal(_ctx, type_decl),
        HASH    => synthesize_hash(_ctx, type_decl),
        CLONE   => synthesize_clone(_ctx, type_decl),
        COMPARE => synthesize_compare(_ctx, type_decl),
        DISPLAY => synthesize_display(_ctx, type_decl),
        DEBUG   => synthesize_debug(_ctx, type_decl),
        _ => unreachable!("is_builtin_protocol guarded earlier"),
    }
}

// ────────────────────────────────────────────────────────────────────────
// AST builder helpers — Ф.3 синтез построен поверх этих helper'ов.
// ────────────────────────────────────────────────────────────────────────

fn span_dummy() -> Span {
    Span::dummy()
}

fn ex(kind: ExprKind) -> Expr {
    Expr::new(kind, span_dummy())
}

fn ident(name: &str) -> Expr {
    ex(ExprKind::Ident(name.to_string()))
}

fn self_field(field_name: &str) -> Expr {
    ex(ExprKind::Member {
        obj: Box::new(ex(ExprKind::SelfAccess)),
        name: field_name.to_string(),
    })
}

fn ident_field(obj_name: &str, field_name: &str) -> Expr {
    ex(ExprKind::Member {
        obj: Box::new(ident(obj_name)),
        name: field_name.to_string(),
    })
}

fn call(target: Expr, args: Vec<Expr>) -> Expr {
    ex(ExprKind::Call {
        func: Box::new(target),
        args: args.into_iter().map(CallArg::Item).collect(),
        trailing: None,
    })
}

fn member_call(obj: Expr, method: &str, args: Vec<Expr>) -> Expr {
    let func = ex(ExprKind::Member {
        obj: Box::new(obj),
        name: method.to_string(),
    });
    call(func, args)
}

fn binop(op: BinOp, l: Expr, r: Expr) -> Expr {
    ex(ExprKind::Binary {
        op,
        left: Box::new(l),
        right: Box::new(r),
    })
}

fn type_ref_named(name: &str) -> TypeRef {
    TypeRef::Named {
        path: vec![name.to_string()],
        generics: vec![],
        span: span_dummy(),
    }
}

fn type_ref_self() -> TypeRef {
    type_ref_named("Self")
}

fn block_with_trailing(stmts: Vec<Stmt>, trailing: Expr) -> Block {
    Block {
        stmts,
        trailing: Some(Box::new(trailing)),
        span: span_dummy(),
        is_unsafe: false,
    }
}

/// Создать FnDecl shell для synthesized method.
fn make_synth_method(
    type_name: &str,
    method_name: &str,
    params: Vec<Param>,
    return_type: Option<TypeRef>,
    body: FnBody,
) -> FnDecl {
    FnDecl {
        name: method_name.to_string(),
        receiver: Some(Receiver {
            type_name: type_name.to_string(),
            generics: vec![],
            carrier_bounds: vec![],
            receiver_ty: None,
            kind: ReceiverKind::Instance,
            mutable: false,
            consume: false,
            span: span_dummy(),
        }),
        params,
        effects: vec![],
        return_type,
        return_is_const: false,
        returns_receiver: false,
        body,
        span: span_dummy(),
        is_export: false,
        is_external: false,
        // Plan 126.2 Ф.1: mark synthesized auto-derive method so downstream
        // passes (method_table registration + Plan 127 lint-suppression) can
        // distinguish compiler-generated bodies from user source.
        compiler_generated: true,
        ..FnDecl::default()
    }
}

fn make_param(name: &str, ty: TypeRef) -> Param {
    Param {
        name: name.to_string(),
        ty,
        span: span_dummy(),
        is_variadic: false,
        default: None,
        consume: false,
        is_mut: false,
        is_const: false,
    }
}

fn is_primitive_field(t: &TypeRef) -> bool {
    matches!(t.strip_modifiers(), TypeRef::Named { path, .. }
        if path.len() == 1 && is_primitive_type(&path[0]))
}

// ────────────────────────────────────────────────────────────────────────
// Per-protocol synthesizers (Ф.3)
// ────────────────────────────────────────────────────────────────────────

/// Synthesize `@equal(other Self) -> bool` — memberwise && combine.
///
/// Empty record/named-tuple → returns `true` (trivially equal).
/// Sum-type → V1: identity-eq placeholder (rich match-arms — followup).
pub fn synthesize_equal<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    let body_expr = if let Some(fields) = iter_fields(type_decl) {
        synth_equal_record_body(&fields)
    } else if iter_sum_variants(type_decl).is_some() {
        // Sum-type equal: V1 — defer к identity (compiler resolves через
        // existing eq mechanism для sum). Rich match-arms — followup
        // [M-126-sum-equal-rich].
        binop(BinOp::Eq, ex(ExprKind::SelfAccess), ident("other"))
    } else {
        return Err(DeriveError::UnsupportedTypeKind {
            type_name: type_decl.name.clone(),
            kind: type_decl_kind_name(type_decl).to_string(),
            protocol: EQUAL.to_string(),
        });
    };

    Ok(make_synth_method(
        &type_decl.name,
        "equal",
        vec![make_param("other", type_ref_self())],
        Some(type_ref_named("bool")),
        FnBody::Expr(body_expr),
    ))
}

fn synth_equal_record_body(fields: &[DerivedField]) -> Expr {
    if fields.is_empty() {
        return ex(ExprKind::BoolLit(true));
    }
    // f1 == other.f1 && f2 == other.f2 && ...
    let mut iter = fields.iter();
    let first = iter.next().unwrap();
    let mut acc = binop(BinOp::Eq, self_field(&first.name), ident_field("other", &first.name));
    for f in iter {
        let cmp = binop(BinOp::Eq, self_field(&f.name), ident_field("other", &f.name));
        acc = binop(BinOp::And, acc, cmp);
    }
    acc
}

/// Synthesize `@hash() -> u64` — XOR + rotate combine FxHash-style.
///
/// Empty type-body → returns `0u64`.
/// Combine formula: `acc ^= field_i.hash() rotate_left(13*i)`.
pub fn synthesize_hash<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    let body_expr = if let Some(fields) = iter_fields(type_decl) {
        synth_hash_record_body(&fields)
    } else if iter_sum_variants(type_decl).is_some() {
        // Sum-type hash: discriminant + payload-hash combine — V1 placeholder.
        // Followup [M-126-sum-hash-rich].
        ex(ExprKind::IntLit(0))
    } else {
        return Err(DeriveError::UnsupportedTypeKind {
            type_name: type_decl.name.clone(),
            kind: type_decl_kind_name(type_decl).to_string(),
            protocol: HASH.to_string(),
        });
    };

    Ok(make_synth_method(
        &type_decl.name,
        "hash",
        vec![],
        Some(type_ref_named("u64")),
        FnBody::Expr(body_expr),
    ))
}

fn synth_hash_record_body(fields: &[DerivedField]) -> Expr {
    if fields.is_empty() {
        return ex(ExprKind::IntLit(0));
    }
    // acc = f0.hash()
    // acc = acc xor rotl(f1.hash(), 13)
    // acc = acc xor rotl(f2.hash(), 26)
    // ...
    // Rotate-and-XOR combine. `rotate_left` has no scalar codegen builtin, so
    // it is emulated purely with bit-ops on the u64 hash — `(h << s) | (h >>
    // (64 - s))` — which never trip the checked-arithmetic overflow guard that
    // multiplication does. Distinct shifts per field decorrelate field order so
    // swapped fields hash differently.
    let rotl = |h: Expr, s: i64| -> Expr {
        let left = binop(BinOp::Shl, h.clone(), ex(ExprKind::IntLit(s)));
        let right = binop(BinOp::Shr, h, ex(ExprKind::IntLit(64 - s)));
        binop(BinOp::BitOr, left, right)
    };
    let mut iter = fields.iter().enumerate();
    let (_, first) = iter.next().unwrap();
    let mut acc = member_call(self_field(&first.name), "hash", vec![]);
    for (i, f) in iter {
        let h = member_call(self_field(&f.name), "hash", vec![]);
        let s = (((13 * i) % 63) + 1) as i64; // 1..=63 — avoid 0 and 64 shifts
        acc = binop(BinOp::BitXor, acc, rotl(h, s));
    }
    acc
}

/// Synthesize `@clone() -> Self` — recursive deep clone.
///
/// Record / NamedTuple → record literal с `field: @field.clone()` per поле.
/// Primitive поля копируются через `@field` без `.clone()` (compiler
/// resolves к built-in copy semantics).
/// Sum-type → V1: returns @ itself (shallow copy для unit variants);
/// rich clone — followup.
pub fn synthesize_clone<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    let body_expr = if let Some(fields) = iter_fields(type_decl) {
        synth_clone_record_body(&type_decl.name, &fields)
    } else if iter_sum_variants(type_decl).is_some() {
        // Sum-type clone — V1 placeholder. Followup [M-126-sum-clone-rich].
        ex(ExprKind::SelfAccess)
    } else {
        return Err(DeriveError::UnsupportedTypeKind {
            type_name: type_decl.name.clone(),
            kind: type_decl_kind_name(type_decl).to_string(),
            protocol: CLONE.to_string(),
        });
    };

    Ok(make_synth_method(
        &type_decl.name,
        "clone",
        vec![],
        Some(type_ref_self()),
        FnBody::Expr(body_expr),
    ))
}

fn synth_clone_record_body(type_name: &str, fields: &[DerivedField]) -> Expr {
    let lit_fields: Vec<RecordLitField> = fields
        .iter()
        .map(|f| {
            let cloned = if is_primitive_field(&f.ty) {
                // Primitive: shallow copy via @field — no recursion.
                self_field(&f.name)
            } else {
                member_call(self_field(&f.name), "clone", vec![])
            };
            RecordLitField {
                name: f.name.clone(),
                value: Some(cloned),
                is_spread: false,
                at_shorthand: false,
                span: span_dummy(),
            }
        })
        .collect();

    ex(ExprKind::RecordLit {
        type_name: Some(vec![type_name.to_string()]),
        fields: lit_fields,
        inferred_map_v: None,
    })
}

/// Synthesize `@compare(other Self) -> int` — lexicographic if-chain.
///
/// Empty type-body → returns `0` (always equal).
/// Sum-type → V1 placeholder (returns 0).
pub fn synthesize_compare<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    let body = if let Some(fields) = iter_fields(type_decl) {
        synth_compare_record_body(&fields)
    } else if iter_sum_variants(type_decl).is_some() {
        // Sum-type compare — V1 placeholder. Followup [M-126-sum-compare-rich].
        FnBody::Expr(ex(ExprKind::IntLit(0)))
    } else {
        return Err(DeriveError::UnsupportedTypeKind {
            type_name: type_decl.name.clone(),
            kind: type_decl_kind_name(type_decl).to_string(),
            protocol: COMPARE.to_string(),
        });
    };

    Ok(make_synth_method(
        &type_decl.name,
        "compare",
        vec![make_param("other", type_ref_self())],
        Some(type_ref_named("int")),
        body,
    ))
}

fn synth_compare_record_body(fields: &[DerivedField]) -> FnBody {
    if fields.is_empty() {
        return FnBody::Expr(ex(ExprKind::IntLit(0)));
    }
    // Build block:
    //   let c_0 = @f0.compare(other.f0); if c_0 != 0 { return c_0 }
    //   let c_1 = ...
    //   0
    let mut stmts: Vec<Stmt> = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        let cmp_call = member_call(
            self_field(&f.name),
            "compare",
            vec![ident_field("other", &f.name)],
        );
        let var_name = format!("__nv_cmp_{}", i);
        let let_decl = crate::ast::LetDecl {
            mutable: false,
            pattern: crate::ast::Pattern::Ident {
                name: var_name.clone(),
                span: span_dummy(),
                is_mut: false,
            },
            ty: Some(type_ref_named("int")),
            value: cmp_call,
            span: span_dummy(),
            is_ghost: false,
            consume: false,
        };
        stmts.push(Stmt::Let(let_decl));
        // if c != 0 { return c }
        let cond = binop(BinOp::Neq, ident(&var_name), ex(ExprKind::IntLit(0)));
        let then_block = Block {
            stmts: vec![Stmt::Return {
                value: Some(ident(&var_name)),
                span: span_dummy(),
            }],
            trailing: None,
            span: span_dummy(),
            is_unsafe: false,
        };
        stmts.push(Stmt::Expr(ex(ExprKind::If {
            cond: Box::new(cond),
            then: then_block,
            else_: None,
        })));
    }
    FnBody::Block(block_with_trailing(stmts, ex(ExprKind::IntLit(0))))
}

/// Synthesize `@display(w Write) -> ()` — memberwise format.
/// D237: renamed from synthesize_fmt (Printable → Display, @fmt → @display).
/// Plan 152.7.1 (D258 AMEND): param changed from `sb StringBuilder` to `w Write`.
///
/// Output form: `TypeName { f1: <display_f1>, f2: <display_f2> }`.
/// Empty type-body → `w.write_str("TypeName")`.
/// Sum-type → V1 placeholder (writes type name).
pub fn synthesize_display<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    let body = if let Some(fields) = iter_fields(type_decl) {
        synth_display_record_body(&type_decl.name, &fields)
    } else if iter_sum_variants(type_decl).is_some() {
        // Sum-type display — V1 placeholder. Followup [M-126-sum-fmt-rich].
        FnBody::Block(simple_display_block(&type_decl.name))
    } else {
        return Err(DeriveError::UnsupportedTypeKind {
            type_name: type_decl.name.clone(),
            kind: type_decl_kind_name(type_decl).to_string(),
            protocol: DISPLAY.to_string(),
        });
    };

    Ok(make_synth_method(
        &type_decl.name,
        "display",
        vec![make_param("w", type_ref_named("Write"))],
        Some(TypeRef::Unit(span_dummy())),
        body,
    ))
}

/// Synthesize `@debug(w Write) -> ()` — memberwise debug format.
/// D237: renamed from synthesize_debug_fmt (DebugPrintable → Debug, @debug_fmt → @debug).
/// Plan 152.7.1 (D258 AMEND): param changed from `sb StringBuilder` to `w Write`.
///
/// Output form: `TypeName { f1: <debug_f1>, f2: <debug_f2> }`.
/// Empty type-body → `w.write_str("TypeName")`.
/// Sum-type → V1 placeholder (writes type name).
pub fn synthesize_debug<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    let body = if let Some(fields) = iter_fields(type_decl) {
        synth_debug_record_body(&type_decl.name, &fields)
    } else if iter_sum_variants(type_decl).is_some() {
        // Sum-type debug — V1 placeholder.
        FnBody::Block(simple_display_block(&type_decl.name))
    } else {
        return Err(DeriveError::UnsupportedTypeKind {
            type_name: type_decl.name.clone(),
            kind: type_decl_kind_name(type_decl).to_string(),
            protocol: DEBUG.to_string(),
        });
    };

    Ok(make_synth_method(
        &type_decl.name,
        "debug",
        vec![make_param("w", type_ref_named("Write"))],
        Some(TypeRef::Unit(span_dummy())),
        body,
    ))
}

fn simple_display_block(type_name: &str) -> Block {
    Block {
        stmts: vec![Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![ex(ExprKind::StrLit(type_name.to_string()))],
        ))],
        trailing: None,
        span: span_dummy(),
        is_unsafe: false,
    }
}

fn synth_display_record_body(type_name: &str, fields: &[DerivedField]) -> FnBody {
    let mut stmts: Vec<Stmt> = Vec::new();
    if fields.is_empty() {
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![ex(ExprKind::StrLit(type_name.to_string()))],
        )));
    } else {
        // w.write_str("TypeName { ")
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![ex(ExprKind::StrLit(format!("{} {{ ", type_name)))],
        )));
        for (i, f) in fields.iter().enumerate() {
            let prefix = if i == 0 {
                format!("{}: ", f.name)
            } else {
                format!(", {}: ", f.name)
            };
            stmts.push(Stmt::Expr(member_call(
                ident("w"),
                "write_str",
                vec![ex(ExprKind::StrLit(prefix))],
            )));
            if is_primitive_field(&f.ty) {
                // Primitive field: no `.display()` method on scalars — route via
                // `w.write_str(str.from(@field))` (Display path).
                stmts.push(Stmt::Expr(member_call(
                    ident("w"),
                    "write_str",
                    vec![member_call(ident("str"), "from", vec![self_field(&f.name)])],
                )));
            } else {
                // Record / nested field: recurse into its synthesized @display.
                stmts.push(Stmt::Expr(member_call(
                    self_field(&f.name),
                    "display",
                    vec![ident("w")],
                )));
            }
        }
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![ex(ExprKind::StrLit(" }".to_string()))],
        )));
    }
    FnBody::Block(Block {
        stmts,
        trailing: None,
        span: span_dummy(),
        is_unsafe: false,
    })
}

fn synth_debug_record_body(type_name: &str, fields: &[DerivedField]) -> FnBody {
    let mut stmts: Vec<Stmt> = Vec::new();
    if fields.is_empty() {
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![ex(ExprKind::StrLit(type_name.to_string()))],
        )));
    } else {
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![ex(ExprKind::StrLit(format!("{} {{ ", type_name)))],
        )));
        for (i, f) in fields.iter().enumerate() {
            let prefix = if i == 0 {
                format!("{}: ", f.name)
            } else {
                format!(", {}: ", f.name)
            };
            stmts.push(Stmt::Expr(member_call(
                ident("w"),
                "write_str",
                vec![ex(ExprKind::StrLit(prefix))],
            )));
            // All fields (primitive or record) implement Debug — call @debug(w) uniformly.
            stmts.push(Stmt::Expr(member_call(
                self_field(&f.name),
                "debug",
                vec![ident("w")],
            )));
        }
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![ex(ExprKind::StrLit(" }".to_string()))],
        )));
    }
    FnBody::Block(Block {
        stmts,
        trailing: None,
        span: span_dummy(),
        is_unsafe: false,
    })
}

// ────────────────────────────────────────────────────────────────────────
// Plan 126.2 Ф.2 — codegen-bound AST injection pass.
//
// Ф.1 registered synthesized `FnDecl`s в TypeCheckCtx.method_table — но это
// type-check-local структура, она НЕ доживает до codegen (`check_module`
// берёт `&Module`, не мутирует его; `emit_module(&Module)` запускается
// отдельно). Codegen строит свой method_overloads / all_methods из
// `module.items` + `peer_files[].items_here`, поэтому synthesized методы
// должны физически попасть в AST как `Item::Fn`.
//
// `inject_synthesized_methods` — AST→AST pass, запускается ПОСЛЕ
// `check_module` (типы validated, impl_protocols проверены) и ДО `desugar`/
// codegen. Для каждого type-decl с `#impl(P)` (built-in P) и без explicit
// метода — синтезирует FnDecl и append'ит как `Item::Fn` в `module.items`.
//
// Operator dispatch (`a == b` → `Nova_T_method_equals`, `<`/`compare` etc.)
// УЖЕ существует в emit_c.rs (D183 amendment, Plan 91.8a.2) — он резолвит
// через method_overloads / all_methods, которые теперь содержат synthesized
// методы. Никаких изменений в operator dispatch не требуется: synthesized
// методам достаточно просто БЫТЬ в module.items как обычные user-методы.
// ────────────────────────────────────────────────────────────────────────

use crate::ast::{Item, Module};

/// Query backend над `Module` — собирает типы + explicit-method coverage
/// прямо из AST items (включая peer_files). Используется injection pass'ом.
struct ModuleDeriveQuery {
    types: std::collections::HashMap<String, TypeDecl>,
    /// (type_name, method_name) пары для explicit instance методов.
    methods: HashSet<(String, String)>,
}

impl ModuleDeriveQuery {
    fn build(module: &Module) -> Self {
        let mut types = std::collections::HashMap::new();
        let mut methods = HashSet::new();
        let mut collect = |items: &[Item]| {
            for item in items {
                match item {
                    Item::Type(td) => {
                        types.insert(td.name.clone(), td.clone());
                    }
                    Item::Fn(fd) => {
                        if let Some(recv) = &fd.receiver {
                            // Instance-метод: ключ (receiver type, method name).
                            // Включая compiler_generated — так повторный запуск
                            // pass'а (defensive idempotency) видит уже-injected
                            // метод как "provided" и НЕ дублирует его. User-vs-
                            // synthesized приоритет уже обеспечен порядком: user
                            // методы в исходном AST, synthesized append'ятся
                            // ПОСЛЕ, и для single-run user-метод присутствует
                            // ДО synthesis-проверки.
                            methods.insert((recv.type_name.clone(), fd.name.clone()));
                        }
                    }
                    _ => {}
                }
            }
        };
        collect(&module.items);
        for pf in &module.peer_files {
            collect(&pf.items_here);
        }
        Self { types, methods }
    }
}

impl DeriveQuery for ModuleDeriveQuery {
    fn lookup_type(&self, name: &str) -> Option<&TypeDecl> {
        self.types.get(name)
    }
    fn type_provides_method(&self, t: &str, method_name: &str) -> bool {
        self.methods.contains(&(t.to_string(), method_name.to_string()))
    }
}

/// Plan 126.2 Ф.2: synthesize built-in protocol methods for `#impl(P)` types
/// and inject them as `Item::Fn` into `module.items`, so codegen emits C
/// bodies and operator dispatch resolves through method_overloads.
///
/// Idempotent w.r.t. explicit user methods (user always wins — skipped via
/// `type_provides_method`) and w.r.t. previously-injected synthesized methods
/// (guarded by `compiler_generated` already present in `methods` exclusion +
/// per-run dedup set). Returns count of injected methods (for diagnostics/tests).
pub fn inject_synthesized_methods(module: &mut Module) -> usize {
    let query = ModuleDeriveQuery::build(module);

    // Collect target (type_decl, protocol) pairs first — borrow of module
    // ends before we mutate module.items.
    let mut synthesized: Vec<FnDecl> = Vec::new();
    // Dedup guard: avoid re-injecting if this pass somehow runs twice, or two
    // protocols map to the same method name (they don't today, but be safe).
    let mut already_injected: HashSet<(String, String)> = HashSet::new();

    // Iterate over a snapshot of type decls (query owns clones).
    let mut type_decls: Vec<TypeDecl> = query.types.values().cloned().collect();
    // Deterministic order — stable codegen output.
    type_decls.sort_by(|a, b| a.name.cmp(&b.name));

    for td in &type_decls {
        if td.impl_protocols.is_empty() {
            continue;
        }
        for proto_name in &td.impl_protocols {
            if !is_builtin_protocol(proto_name) {
                continue;
            }
            let Some(method_name) = builtin_protocol_method(proto_name) else {
                continue;
            };
            // User-explicit method wins — never synthesize over it.
            if query.type_provides_method(&td.name, method_name) {
                continue;
            }
            let key = (td.name.clone(), method_name.to_string());
            if already_injected.contains(&key) {
                continue;
            }
            let mut ctx = AutoDeriveCtx::new(&query);
            match synthesize_method(&mut ctx, td, proto_name) {
                Ok(fd) => {
                    already_injected.insert(key);
                    synthesized.push(fd);
                }
                // Synthesis failures already surfaced as diagnostics during
                // type-check (verify_impl_protocols). Skip silently here —
                // injecting an error'd body would produce invalid C.
                Err(_) => {}
            }
        }
    }

    let count = synthesized.len();
    for fd in synthesized {
        module.items.push(Item::Fn(fd));
    }
    count
}

// ────────────────────────────────────────────────────────────────────────
// Plan 126 Ф.2 unit tests — infrastructure coverage.
// Per-protocol synthesizer tests — в Ф.3 (next commit).
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Mock query backend for unit tests.
    struct MockQuery {
        types: HashMap<String, TypeDecl>,
        provides: HashMap<(String, String), bool>,
    }

    impl MockQuery {
        fn new() -> Self {
            Self {
                types: HashMap::new(),
                provides: HashMap::new(),
            }
        }

        fn add_type(&mut self, td: TypeDecl) {
            self.types.insert(td.name.clone(), td);
        }

        fn add_method(&mut self, type_name: &str, method: &str) {
            self.provides.insert((type_name.to_string(), method.to_string()), true);
        }
    }

    impl DeriveQuery for MockQuery {
        fn lookup_type(&self, name: &str) -> Option<&TypeDecl> {
            self.types.get(name)
        }

        fn type_provides_method(&self, t: &str, method_name: &str) -> bool {
            self.provides
                .get(&(t.to_string(), method_name.to_string()))
                .copied()
                .unwrap_or(false)
        }
    }

    fn type_ref_named(name: &str) -> TypeRef {
        TypeRef::Named {
            path: vec![name.to_string()],
            generics: vec![],
            span: Span::dummy(),
        }
    }

    fn make_record_type(name: &str, field_specs: &[(&str, &str)]) -> TypeDecl {
        let fields: Vec<RecordField> = field_specs
            .iter()
            .map(|(fname, ftype)| RecordField {
                name: fname.to_string(),
                ty: type_ref_named(ftype),
                span: Span::dummy(),
                ..RecordField::default()
            })
            .collect();
        TypeDecl {
            name: name.to_string(),
            kind: TypeDeclKind::Record(fields),
            span: Span::dummy(),
            ..TypeDecl::default()
        }
    }

    fn make_record_with_impl(name: &str, field_specs: &[(&str, &str)], proto: &str) -> TypeDecl {
        let mut td = make_record_type(name, field_specs);
        td.impl_protocols.push(proto.to_string());
        td
    }

    // ─── T01: built-in protocol detection ─────────────────────────────
    #[test]
    fn t01_builtin_protocol_detection() {
        assert!(is_builtin_protocol("Equal"));
        assert!(is_builtin_protocol("Hash"));
        assert!(is_builtin_protocol("Clone"));
        assert!(is_builtin_protocol("Compare"));
        assert!(is_builtin_protocol("Display"));
        assert!(is_builtin_protocol("Debug"));
        assert!(!is_builtin_protocol("From"));
        assert!(!is_builtin_protocol("MyProtocol"));
        // Old names no longer recognized:
        assert!(!is_builtin_protocol("Equatable"));
        assert!(!is_builtin_protocol("Hashable"));
        assert!(!is_builtin_protocol("Cloneable"));
        assert!(!is_builtin_protocol("Comparable"));
        assert!(!is_builtin_protocol("Printable"));
    }

    // ─── T02: protocol → method name lookup ──────────────────────────
    #[test]
    fn t02_protocol_method_name_lookup() {
        assert_eq!(builtin_protocol_method("Equal"), Some("equal"));
        assert_eq!(builtin_protocol_method("Hash"), Some("hash"));
        assert_eq!(builtin_protocol_method("Clone"), Some("clone"));
        assert_eq!(builtin_protocol_method("Compare"), Some("compare"));
        assert_eq!(builtin_protocol_method("Display"), Some("display"));
        assert_eq!(builtin_protocol_method("Debug"), Some("debug"));
        assert_eq!(builtin_protocol_method("Unknown"), None);
    }

    // ─── T03: primitive type detection ────────────────────────────────
    #[test]
    fn t03_primitive_type_detection() {
        assert!(is_primitive_type("int"));
        assert!(is_primitive_type("f64"));
        assert!(is_primitive_type("bool"));
        assert!(is_primitive_type("str"));
        assert!(is_primitive_type("u64"));
        assert!(!is_primitive_type("Vec3"));
        assert!(!is_primitive_type("StringBuilder"));
    }

    // ─── T04: cycle detection — mark/unmark ──────────────────────────
    #[test]
    fn t04_cycle_detection_marks_visited() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        assert!(ctx.mark_visiting("A", "Clone"));
        assert!(!ctx.mark_visiting("A", "Clone")); // duplicate
        assert!(ctx.is_visiting("A", "Clone"));
        ctx.unmark_visiting("A", "Clone");
        assert!(!ctx.is_visiting("A", "Clone"));
    }

    // ─── T05: cycle detection — cross-protocol independence ──────────
    #[test]
    fn t05_cycle_detection_protocols_independent() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        assert!(ctx.mark_visiting("A", "Clone"));
        // Different protocol — should NOT collide.
        assert!(ctx.mark_visiting("A", "Equal"));
        assert!(ctx.is_visiting("A", "Clone"));
        assert!(ctx.is_visiting("A", "Equal"));
    }

    // ─── T06: field eligibility — primitive passes ───────────────────
    #[test]
    fn t06_field_eligibility_primitive_passes() {
        let q = MockQuery::new();
        let f = type_ref_named("int");
        assert!(check_field_eligibility(&q, &f, "Clone", "clone"));
        let s = type_ref_named("str");
        assert!(check_field_eligibility(&q, &s, "Clone", "clone"));
    }

    // ─── T07: field eligibility — missing protocol fails ─────────────
    #[test]
    fn t07_field_eligibility_missing_protocol_fails() {
        let mut q = MockQuery::new();
        q.add_type(make_record_type("Inner", &[("a", "int")]));
        let f = type_ref_named("Inner");
        assert!(!check_field_eligibility(&q, &f, "Clone", "clone"));
    }

    // ─── T08: field eligibility — with #impl passes ──────────────────
    #[test]
    fn t08_field_eligibility_with_impl_passes() {
        let mut q = MockQuery::new();
        q.add_type(make_record_with_impl("Inner", &[("a", "int")], "Clone"));
        let f = type_ref_named("Inner");
        assert!(check_field_eligibility(&q, &f, "Clone", "clone"));
    }

    // ─── T09: field eligibility — explicit method passes ─────────────
    #[test]
    fn t09_field_eligibility_with_explicit_method_passes() {
        let mut q = MockQuery::new();
        q.add_type(make_record_type("Inner", &[("a", "int")]));
        q.add_method("Inner", "clone");
        let f = type_ref_named("Inner");
        assert!(check_field_eligibility(&q, &f, "Clone", "clone"));
    }

    // ─── T10: field eligibility — array recurses ─────────────────────
    #[test]
    fn t10_field_eligibility_array_recurses() {
        let mut q = MockQuery::new();
        q.add_type(make_record_with_impl("Inner", &[("a", "int")], "Clone"));
        let f = TypeRef::Array(Box::new(type_ref_named("Inner")), Span::dummy());
        assert!(check_field_eligibility(&q, &f, "Clone", "clone"));
    }

    // ─── T11: field eligibility — tuple recurses ─────────────────────
    #[test]
    fn t11_field_eligibility_tuple_recurses() {
        let q = MockQuery::new();
        let f = TypeRef::Tuple(
            vec![type_ref_named("int"), type_ref_named("f64")],
            Span::dummy(),
        );
        assert!(check_field_eligibility(&q, &f, "Clone", "clone"));
    }

    // ─── T12: field eligibility — tuple with bad elem fails ──────────
    #[test]
    fn t12_field_eligibility_tuple_with_bad_elem_fails() {
        let mut q = MockQuery::new();
        q.add_type(make_record_type("Inner", &[("a", "int")]));
        let f = TypeRef::Tuple(
            vec![type_ref_named("int"), type_ref_named("Inner")],
            Span::dummy(),
        );
        assert!(!check_field_eligibility(&q, &f, "Clone", "clone"));
    }

    // ─── T13: unknown protocol rejected ──────────────────────────────
    #[test]
    fn t13_synthesize_unknown_protocol_rejected() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("X", &[]);
        let err = synthesize_method(&mut ctx, &td, "Unknown").unwrap_err();
        match err {
            DeriveError::UnknownProtocol(p) => assert_eq!(p, "Unknown"),
            other => panic!("expected UnknownProtocol, got {:?}", other),
        }
    }

    // ─── T14: iter_fields — Record ───────────────────────────────────
    #[test]
    fn t14_iter_fields_record() {
        let td = make_record_type("Point", &[("x", "int"), ("y", "int")]);
        let fields = iter_fields(&td).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[1].name, "y");
    }

    // ─── T15: iter_fields — NamedTuple ───────────────────────────────
    #[test]
    fn t15_iter_fields_named_tuple() {
        let td = TypeDecl {
            name: "Pair".to_string(),
            kind: TypeDeclKind::NamedTuple(vec![
                NamedTupleField {
                    name: "first".to_string(),
                    ty: type_ref_named("int"),
                    span: Span::dummy(),
                    priv_field: false,
                    priv_module_field: false,
                    visible_to: vec![],
                },
                NamedTupleField {
                    name: "second".to_string(),
                    ty: type_ref_named("int"),
                    span: Span::dummy(),
                    priv_field: false,
                    priv_module_field: false,
                    visible_to: vec![],
                },
            ]),
            span: Span::dummy(),
            ..TypeDecl::default()
        };
        let fields = iter_fields(&td).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "first");
        assert_eq!(fields[1].name, "second");
    }

    // ─── T16: iter_fields — Sum returns None ─────────────────────────
    #[test]
    fn t16_iter_fields_sum_returns_none() {
        let td = TypeDecl {
            name: "Option".to_string(),
            kind: TypeDeclKind::Sum(vec![]),
            span: Span::dummy(),
            ..TypeDecl::default()
        };
        assert!(iter_fields(&td).is_none());
        assert!(iter_sum_variants(&td).is_some());
    }

    // ─── T17: diagnostic messages — error code prefixes ─────────────
    #[test]
    fn t17_diagnostic_messages_have_proper_prefix() {
        let cycle = DeriveError::Cycle {
            type_name: "A".to_string(),
            protocol: "Cloneable".to_string(),
        };
        assert!(cycle.diagnostic_message().contains("[E_AUTO_DERIVE_CYCLE]"));

        let field = DeriveError::FieldLacksProtocol {
            type_name: "Outer".to_string(),
            field_name: "inner".to_string(),
            field_type: "Inner".to_string(),
            protocol: "Cloneable".to_string(),
        };
        assert!(field
            .diagnostic_message()
            .contains("[E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL]"));

        let unknown = DeriveError::UnknownProtocol("Foo".to_string());
        assert!(unknown.diagnostic_message().contains("[E_AUTO_DERIVE_UNKNOWN_PROTOCOL]"));

        let unsupported = DeriveError::UnsupportedTypeKind {
            type_name: "X".to_string(),
            kind: "effect".to_string(),
            protocol: "Cloneable".to_string(),
        };
        assert!(unsupported.diagnostic_message().contains("[E_AUTO_DERIVE_UNSUPPORTED_KIND]"));
    }

    // ─── T18: type_ref_name extraction ───────────────────────────────
    #[test]
    fn t18_type_ref_name_extraction() {
        assert_eq!(type_ref_name(&type_ref_named("Vec3")), Some("Vec3"));
        assert_eq!(
            type_ref_name(&TypeRef::Array(Box::new(type_ref_named("int")), Span::dummy())),
            None
        );
    }

    // ─── T19: type_ref_render ────────────────────────────────────────
    #[test]
    fn t19_type_ref_render() {
        assert_eq!(type_ref_render(&type_ref_named("Vec3")), "Vec3");
        let arr = TypeRef::Array(Box::new(type_ref_named("int")), Span::dummy());
        assert_eq!(type_ref_render(&arr), "[]int");
        let tup = TypeRef::Tuple(
            vec![type_ref_named("int"), type_ref_named("str")],
            Span::dummy(),
        );
        assert_eq!(type_ref_render(&tup), "(int, str)");
    }

    // ─── T20: Ф.3 — synthesize_equal — record with primitives ────────
    #[test]
    fn t20_synthesize_equal_record_primitives() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Vec3", &[("x", "f64"), ("y", "f64"), ("z", "f64")]);
        let fd = synthesize_method(&mut ctx, &td, EQUAL).unwrap();
        assert_eq!(fd.name, "equal");
        assert_eq!(fd.params.len(), 1);
        assert_eq!(fd.params[0].name, "other");
        match &fd.body {
            FnBody::Expr(e) => match &e.kind {
                ExprKind::Binary { op: BinOp::And, .. } => {}
                _ => panic!("expected And-chain root for 3-field equal"),
            },
            _ => panic!("expected FnBody::Expr"),
        }
    }

    // ─── T21: Ф.3 — synthesize_equal — empty record ──────────────────
    #[test]
    fn t21_synthesize_equal_empty_record() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Empty", &[]);
        let fd = synthesize_method(&mut ctx, &td, EQUAL).unwrap();
        match &fd.body {
            FnBody::Expr(e) => match &e.kind {
                ExprKind::BoolLit(true) => {}
                _ => panic!("expected BoolLit(true)"),
            },
            _ => panic!("expected FnBody::Expr"),
        }
    }

    // ─── T22: Ф.3 — synthesize_equal — single-field record ───────────
    #[test]
    fn t22_synthesize_equal_single_field() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Wrapper", &[("v", "int")]);
        let fd = synthesize_method(&mut ctx, &td, EQUAL).unwrap();
        match &fd.body {
            FnBody::Expr(e) => match &e.kind {
                ExprKind::Binary { op: BinOp::Eq, .. } => {}
                _ => panic!("expected single Eq for 1-field equal"),
            },
            _ => panic!("expected FnBody::Expr"),
        }
    }

    // ─── T23: Ф.3 — synthesize_hash ──────────────────────────────────
    #[test]
    fn t23_synthesize_hash_returns_u64() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Point", &[("x", "int"), ("y", "int")]);
        let fd = synthesize_method(&mut ctx, &td, HASH).unwrap();
        assert_eq!(fd.name, "hash");
        assert_eq!(fd.params.len(), 0);
        match &fd.return_type {
            Some(TypeRef::Named { path, .. }) => assert_eq!(path.last().unwrap(), "u64"),
            _ => panic!("expected u64 return type"),
        }
    }

    // ─── T24: Ф.3 — synthesize_clone ─────────────────────────────────
    #[test]
    fn t24_synthesize_clone_returns_self() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Vec3", &[("x", "f64"), ("y", "f64"), ("z", "f64")]);
        let fd = synthesize_method(&mut ctx, &td, CLONE).unwrap();
        assert_eq!(fd.name, "clone");
        match &fd.return_type {
            Some(TypeRef::Named { path, .. }) => assert_eq!(path.last().unwrap(), "Self"),
            _ => panic!("expected Self return type"),
        }
        match &fd.body {
            FnBody::Expr(e) => match &e.kind {
                ExprKind::RecordLit { type_name, fields, .. } => {
                    assert_eq!(type_name.as_ref().unwrap()[0], "Vec3");
                    assert_eq!(fields.len(), 3);
                }
                _ => panic!("expected RecordLit body for clone"),
            },
            _ => panic!("expected FnBody::Expr"),
        }
    }

    // ─── T25: Ф.3 — synthesize_compare ───────────────────────────────
    #[test]
    fn t25_synthesize_compare_returns_int_block() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Money", &[("cents", "int")]);
        let fd = synthesize_method(&mut ctx, &td, COMPARE).unwrap();
        assert_eq!(fd.name, "compare");
        assert_eq!(fd.params.len(), 1);
        match &fd.return_type {
            Some(TypeRef::Named { path, .. }) => assert_eq!(path.last().unwrap(), "int"),
            _ => panic!("expected int return type"),
        }
        match &fd.body {
            FnBody::Block(_) => {}
            _ => panic!("expected FnBody::Block for compare body"),
        }
    }

    // ─── T26: Ф.3 — synthesize_compare empty record ──────────────────
    #[test]
    fn t26_synthesize_compare_empty_returns_zero() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Empty", &[]);
        let fd = synthesize_method(&mut ctx, &td, COMPARE).unwrap();
        match &fd.body {
            FnBody::Expr(e) => match &e.kind {
                ExprKind::IntLit(0) => {}
                _ => panic!("expected 0 lit for empty compare"),
            },
            _ => panic!("expected FnBody::Expr"),
        }
    }

    // ─── T27: Ф.3 — synthesize_display ───────────────────────────────
    #[test]
    fn t27_synthesize_display_takes_stringbuilder() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Point", &[("x", "int"), ("y", "int")]);
        let fd = synthesize_method(&mut ctx, &td, DISPLAY).unwrap();
        assert_eq!(fd.name, "display");
        assert_eq!(fd.params.len(), 1);
        assert_eq!(fd.params[0].name, "sb");
        match &fd.return_type {
            Some(TypeRef::Unit(_)) => {}
            _ => panic!("expected unit return type for display"),
        }
    }

    // ─── T28: Ф.3 — synthesize fails when field not eligible ─────────
    #[test]
    fn t28_synthesize_fails_when_field_not_eligible() {
        let mut q = MockQuery::new();
        q.add_type(make_record_type("Inner", &[("a", "int")]));
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Outer", &[("inner", "Inner")]);
        let err = synthesize_method(&mut ctx, &td, CLONE).unwrap_err();
        match err {
            DeriveError::FieldLacksProtocol { type_name, field_name, .. } => {
                assert_eq!(type_name, "Outer");
                assert_eq!(field_name, "inner");
            }
            other => panic!("expected FieldLacksProtocol, got {:?}", other),
        }
    }

    // ─── T29: Ф.3 — NamedTuple synthesis ─────────────────────────────
    #[test]
    fn t29_synthesize_named_tuple() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = TypeDecl {
            name: "Pair".to_string(),
            kind: TypeDeclKind::NamedTuple(vec![
                NamedTupleField {
                    name: "first".to_string(),
                    ty: type_ref_named("int"),
                    span: Span::dummy(),
                    priv_field: false,
                    priv_module_field: false,
                    visible_to: vec![],
                },
                NamedTupleField {
                    name: "second".to_string(),
                    ty: type_ref_named("int"),
                    span: Span::dummy(),
                    priv_field: false,
                    priv_module_field: false,
                    visible_to: vec![],
                },
            ]),
            span: Span::dummy(),
            ..TypeDecl::default()
        };
        let fd = synthesize_method(&mut ctx, &td, EQUAL).unwrap();
        assert_eq!(fd.name, "equal");
    }

    // ─── T30: Ф.3 — clone body uses .clone() for non-primitive ──────
    #[test]
    fn t30_synthesize_clone_calls_clone_on_non_primitive() {
        let mut q = MockQuery::new();
        q.add_type(make_record_with_impl("Inner", &[("a", "int")], "Clone"));
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_with_impl("Outer", &[("inner", "Inner")], "Clone");
        let fd = synthesize_method(&mut ctx, &td, CLONE).unwrap();
        match &fd.body {
            FnBody::Expr(e) => match &e.kind {
                ExprKind::RecordLit { fields, .. } => {
                    assert_eq!(fields.len(), 1);
                    // Non-primitive Inner field must use .clone() call.
                    match &fields[0].value.as_ref().unwrap().kind {
                        ExprKind::Call { func, .. } => match &func.kind {
                            ExprKind::Member { name, .. } => assert_eq!(name, "clone"),
                            _ => panic!("expected Member-call for non-primitive clone"),
                        },
                        _ => panic!("expected Call for non-primitive clone"),
                    }
                }
                _ => panic!("expected RecordLit"),
            },
            _ => panic!("expected FnBody::Expr"),
        }
    }

    // ────────────────────────────────────────────────────────────────────
    // Plan 126.2 Ф.2 — injection pass tests (codegen-bound AST rewrite).
    // ────────────────────────────────────────────────────────────────────

    use crate::ast::{Item, Module};

    fn module_with(items: Vec<Item>) -> Module {
        Module {
            name: vec![],
            imports: vec![],
            items,
            attrs: vec![],
            contract_opt_out: Default::default(),
            doc_attrs: vec![],
            span: Span::dummy(),
            peer_files: vec![],
            doc: None,
        }
    }

    /// Helper: collect names of injected (compiler_generated) instance
    /// methods present in module.items, keyed by (receiver type, method).
    fn injected_methods(m: &Module) -> Vec<(String, String)> {
        m.items
            .iter()
            .filter_map(|it| match it {
                Item::Fn(fd) if fd.compiler_generated => fd
                    .receiver
                    .as_ref()
                    .map(|r| (r.type_name.clone(), fd.name.clone())),
                _ => None,
            })
            .collect()
    }

    /// Build an explicit (user) instance method FnDecl for `type @method`.
    fn user_method(type_name: &str, method: &str) -> FnDecl {
        FnDecl {
            name: method.to_string(),
            receiver: Some(Receiver {
                type_name: type_name.to_string(),
                generics: vec![],
                carrier_bounds: vec![],
                receiver_ty: None,
                kind: ReceiverKind::Instance,
                mutable: false,
                consume: false,
                span: Span::dummy(),
            }),
            params: vec![],
            body: FnBody::Expr(ex(ExprKind::BoolLit(true))),
            compiler_generated: false,
            ..FnDecl::default()
        }
    }

    // ─── T31: inject emits Nova_T_method_equal for #impl(Equal) ──
    #[test]
    fn t31_inject_equal_record() {
        let td = make_record_with_impl("Vec3", &[("x", "f64"), ("y", "f64")], EQUAL);
        let mut m = module_with(vec![Item::Type(td)]);
        let n = inject_synthesized_methods(&mut m);
        assert_eq!(n, 1, "exactly one method synthesized for Equal");
        let injected = injected_methods(&m);
        assert!(injected.contains(&("Vec3".to_string(), "equal".to_string())),
            "expected synthesized Vec3.equal, got {:?}", injected);
    }

    // ─── T32: inject all six built-in protocols ─────────────────────
    #[test]
    fn t32_inject_all_protocols() {
        let mut td = make_record_type("Point", &[("x", "int"), ("y", "int")]);
        for p in [EQUAL, HASH, CLONE, COMPARE, DISPLAY, DEBUG] {
            td.impl_protocols.push(p.to_string());
        }
        let mut m = module_with(vec![Item::Type(td)]);
        let n = inject_synthesized_methods(&mut m);
        assert_eq!(n, 6, "six built-in protocols → six methods");
        let injected = injected_methods(&m);
        for meth in ["equal", "hash", "clone", "compare", "display", "debug"] {
            assert!(injected.contains(&("Point".to_string(), meth.to_string())),
                "missing synthesized Point.{}, got {:?}", meth, injected);
        }
    }

    // ─── T33: user-explicit method wins — no synthesis ───────────────
    #[test]
    fn t33_inject_user_method_wins() {
        let td = make_record_with_impl("Money", &[("cents", "int")], EQUAL);
        let mut m = module_with(vec![
            Item::Type(td),
            Item::Fn(user_method("Money", "equal")),
        ]);
        let n = inject_synthesized_methods(&mut m);
        assert_eq!(n, 0, "user-provided equal suppresses synthesis");
        assert!(injected_methods(&m).is_empty(),
            "no compiler_generated method should be injected");
    }

    // ─── T34: non-builtin protocol ignored ───────────────────────────
    #[test]
    fn t34_inject_ignores_non_builtin() {
        let td = make_record_with_impl("Widget", &[("id", "int")], "Drawable");
        let mut m = module_with(vec![Item::Type(td)]);
        let n = inject_synthesized_methods(&mut m);
        assert_eq!(n, 0, "non-builtin protocol → no synthesis");
    }

    // ─── T35: field lacks protocol → synthesis skipped (diag elsewhere) ─
    #[test]
    fn t35_inject_skips_when_field_ineligible() {
        // Outer #impl(Clone) with Inner field that lacks Clone.
        let inner = make_record_type("Inner", &[("a", "int")]);
        let outer = make_record_with_impl("Outer", &[("inner", "Inner")], CLONE);
        let mut m = module_with(vec![Item::Type(inner), Item::Type(outer)]);
        let n = inject_synthesized_methods(&mut m);
        assert_eq!(n, 0, "ineligible field → synthesis skipped (no invalid C)");
    }

    // ─── T36: nested eligible field → both synthesize ────────────────
    #[test]
    fn t36_inject_nested_eligible() {
        let inner = make_record_with_impl("Inner", &[("a", "int")], CLONE);
        let outer = make_record_with_impl("Outer", &[("inner", "Inner")], CLONE);
        let mut m = module_with(vec![Item::Type(inner), Item::Type(outer)]);
        let n = inject_synthesized_methods(&mut m);
        assert_eq!(n, 2, "both Inner and Outer synthesize clone");
        let injected = injected_methods(&m);
        assert!(injected.contains(&("Inner".to_string(), "clone".to_string())));
        assert!(injected.contains(&("Outer".to_string(), "clone".to_string())));
    }

    // ─── T37: idempotent — second run does not double-inject ─────────
    #[test]
    fn t37_inject_idempotent_via_compiler_generated_guard() {
        let td = make_record_with_impl("Vec3", &[("x", "f64")], EQUAL);
        let mut m = module_with(vec![Item::Type(td)]);
        let n1 = inject_synthesized_methods(&mut m);
        assert_eq!(n1, 1);
        // Defensive idempotency: the already-injected compiler_generated method
        // is now seen as "provided" by ModuleDeriveQuery, so a second run does
        // not re-synthesize it.
        let n2 = inject_synthesized_methods(&mut m);
        assert_eq!(n2, 0, "second run must be a no-op (idempotent)");
        assert_eq!(injected_methods(&m).len(), 1, "no duplicate injected");
    }
}
