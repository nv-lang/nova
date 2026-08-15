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
    ArrayElem, BinOp, Block, CallArg, Expr, ExprKind, FnBody, FnDecl, GenericParam, MatchArm,
    MatchArmBody, NamedTupleField, Param, Pattern, RecordField, RecordLitField,
    RecordPatternField, Receiver, ReceiverKind, RenameConvention, SerdeArg, SerdeTagging, Stmt,
    SumVariant, SumVariantKind, TypeDecl, TypeDeclKind, TypeRef, VariantPatternKind,
};
use crate::diag::Span;

/// Имена built-in protocols, поддерживаемых auto-derive (Plan 126; D237 rename).
pub const EQUAL:   &str = "Equal";
pub const HASH:    &str = "Hash";
pub const CLONE:   &str = "Clone";
pub const COMPARE: &str = "Compare";
pub const DISPLAY: &str = "Display";
pub const DEBUG:   &str = "Debug";
/// Plan 180: serde protocols — 7th/8th members of the auto-derive family.
/// `Serialize` synth = uniform memberwise push (`@field.serialize(s)`, like
/// `Debug`); `Deserialize` synth = type-directed pull (scalar → `deser_X`,
/// record/container → static `.deserialize`, `Option` → inline null-check).
pub const SERIALIZE:   &str = "Serialize";
pub const DESERIALIZE: &str = "Deserialize";
/// Plan 222.8 Ф.1 (D438): `Reflect` — 9th auto-derive protocol. Synthesizes
/// `.reflect() -> TypeShape`, a STATIC (no receiver value) description of a
/// type's structural shape — format/domain-independent (std/src/reflect.nv).
/// Same field-walk infra as Serialize (`resolve_fields`/`wire_name_for`/
/// `serde_tagging_mode`), but builds a single literal VALUE expression
/// instead of a push-protocol call sequence — see `synthesize_reflect`'s doc
/// section below for why (type-graph cycles need compile-time `Ref`
/// substitution, unlike per-instance protocols whose recursion terminates on
/// finite runtime data).
pub const REFLECT: &str = "Reflect";

/// True если `proto_name` — один из known built-in protocols.
pub fn is_builtin_protocol(proto_name: &str) -> bool {
    matches!(
        proto_name,
        EQUAL | HASH | CLONE | COMPARE | DISPLAY | DEBUG | SERIALIZE | DESERIALIZE | REFLECT
    )
}

/// Plan 104.10 Ф.5 ([M-104.10-hardcode-lists]): the canonical list of built-in
/// auto-derivable protocol names, in a stable order, for tooling (nova-lsp
/// `#derive(...)` quick-fixes). Single source of truth — keeps the LSP from
/// drifting with a stale, renamed list (the old LSP table still named the
/// pre-D237 `Printable`/`Hashable`/`Equatable`/`Ordered`/`Cloneable` protocols).
pub fn builtin_protocol_names() -> &'static [&'static str] {
    &[EQUAL, HASH, CLONE, COMPARE, DISPLAY, DEBUG, SERIALIZE, DESERIALIZE, REFLECT]
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
        SERIALIZE   => Some("serialize"),
        DESERIALIZE => Some("deserialize"),
        REFLECT     => Some("reflect"),
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
    /// Plan 180 Ф.6 (D382): `#serde(...)` tagging-mode misconfiguration on a
    /// sum/record type. `message` already carries the specific `[E_SERDE_*]`
    /// code (E_SERDE_TAGGING_CONFLICT / E_SERDE_CONTENT_WITHOUT_TAG /
    /// E_SERDE_TAGGING_ON_NON_SUM / E_SERDE_INTERNAL_TAG_NON_STRUCT). Plan
    /// 180.1 Ф.1/Ф.7/Ф.10 reuses this same carrier for the newer field-attr /
    /// wire-contract diagnostics (E_SERDE_ATTRIBUTE_MISPLACED /
    /// E_SERDE_WIRE_NAME_COLLISION / E_SERDE_FLATTEN_* /
    /// E_SERDE_SKIP_RENAME_CONFLICT / E_SERDE_UNKNOWN_FIELD_POLICY_CONFLICT /
    /// E_SERDE_SKIP_FIELD_NO_DEFAULT) — one message-carrying variant, many
    /// codes (mirrors the existing precedent, no new enum sprawl).
    SerdeTagging {
        type_name: String,
        message: String,
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
            DeriveError::SerdeTagging { message, .. } => message.clone(),
        }
    }
}

/// Plan 180 Ф.6 (D382): compute the serde tagging mode for a type from its
/// declaration-level `#serde(...)` attributes. For a sum type this validates
/// the `tag`/`content`/`untagged` combination and (for internal tagging) that
/// every variant is struct-shaped (unit or record). For a non-sum type any
/// tagging attribute is rejected (E_SERDE_TAGGING_ON_NON_SUM).
///
/// Called by the serde synthesizers; returns `SerdeTagging::External` when the
/// type carries no serde attributes (default, behaviour unchanged from Ф.2-sum).
pub fn serde_tagging_mode(td: &TypeDecl) -> Result<SerdeTagging, DeriveError> {
    let err = |msg: String| DeriveError::SerdeTagging {
        type_name: td.name.clone(),
        message: msg,
    };
    // Plan 180.1 Ф.1/Ф.7: field-only keys never belong on a TYPE decl,
    // regardless of sum/record — reject up front (E_SERDE_ATTRIBUTE_MISPLACED).
    for a in &td.serde_attrs {
        if matches!(
            a,
            SerdeArg::Rename(_) | SerdeArg::Skip | SerdeArg::SkipSerializingIf(_)
                | SerdeArg::Default(_) | SerdeArg::Alias(_) | SerdeArg::Flatten
        ) {
            return Err(err(format!(
                "[E_SERDE_ATTRIBUTE_MISPLACED] type `{}`: `{}` is a field-level \
                 serde attribute and cannot appear on a `#serde(...)` at type level. \
                 Move it to the individual field's own `#serde(...)`.",
                td.name, serde_arg_key_name(a),
            )));
        }
    }
    let is_sum = iter_sum_variants(td).is_some();
    let has_tagging_attr = td.serde_attrs.iter().any(|a| {
        matches!(a, SerdeArg::Tag(_) | SerdeArg::Content(_) | SerdeArg::Untagged)
    });
    if !is_sum {
        if has_tagging_attr {
            return Err(err(format!(
                "[E_SERDE_TAGGING_ON_NON_SUM] type `{}` is not a sum type — \
                 `#serde(tag/content/untagged)` tagging attributes apply only to \
                 sum (`type X enum A | B`, D406) declarations.",
                td.name,
            )));
        }
        return Ok(SerdeTagging::External);
    }
    // Plan 180.1 Ф.1 scope: rename_all / allow_unknown / deny_unknown_fields
    // consumption targets RECORD types only (v1) — reject explicitly on sum
    // (no-magic: silently ignoring would be a worse footgun than a clear gate).
    for a in &td.serde_attrs {
        if matches!(
            a,
            SerdeArg::RenameAll(_) | SerdeArg::AllowUnknown | SerdeArg::DenyUnknownFields
        ) {
            return Err(err(format!(
                "[E_SERDE_ATTRIBUTE_ON_SUM_UNSUPPORTED] sum type `{}`: `{}` is only \
                 consumed on record types (180.1 Ф.1 scope) — sum-type rich \
                 attribute support is a separate followup ([M-126-sum-*-rich]).",
                td.name, serde_arg_key_name(a),
            )));
        }
    }
    let mut tag: Option<String> = None;
    let mut content: Option<String> = None;
    let mut untagged = false;
    for a in &td.serde_attrs {
        match a {
            SerdeArg::Tag(s) => tag = Some(s.clone()),
            SerdeArg::Content(s) => content = Some(s.clone()),
            SerdeArg::Untagged => untagged = true,
            _ => {}
        }
    }
    if untagged && (tag.is_some() || content.is_some()) {
        return Err(err(format!(
            "[E_SERDE_TAGGING_CONFLICT] sum type `{}`: `#serde(untagged)` cannot \
             be combined with `tag`/`content`.",
            td.name,
        )));
    }
    if untagged {
        // Plan 180 Ф.6: untagged deserialize (try-each-variant, D345/D382) is
        // synthesized correctly, BUT compiling an untagged-derive body triggers a
        // latent codegen mono-collection-ordering corruption of `std/encoding/json`
        // (`Json.parse` mis-tags a number as a bool in the SAME CU) — a
        // compiler-hardening prerequisite, NOT a serde-logic defect. Gated, not
        // shipped broken. Internally- and adjacently-tagged land unaffected.
        return Err(err(format!(
            "[E_SERDE_UNTAGGED_GATED] sum type `{}`: `#serde(untagged)` is gated \
             pending a codegen mono-ordering fix ([M-180-untagged-codegen-mono]) — \
             deriving an untagged sum currently perturbs `std/encoding/json` \
             codegen in the same compilation unit. Use externally-tagged (default), \
             internally-tagged (`#serde(tag=\"k\")`), or adjacently-tagged \
             (`#serde(tag=\"t\", content=\"c\")`) instead.",
            td.name,
        )));
    }
    match (tag, content) {
        (Some(tag), Some(content)) => {
            if tag == content {
                return Err(err(format!(
                    "[E_SERDE_TAGGING_CONFLICT] sum type `{}`: adjacently-tagged \
                     `tag` and `content` field names must differ (both `\"{}\"`).",
                    td.name, tag,
                )));
            }
            Ok(SerdeTagging::Adjacent { tag, content })
        }
        (Some(tag), None) => {
            // Internal tagging inlines the discriminator INTO the payload object,
            // so every variant must be struct-shaped (unit or record); a tuple
            // payload has no object to inline the tag into (serde rule).
            if let Some(variants) = iter_sum_variants(td) {
                for v in variants {
                    if matches!(v.kind, SumVariantKind::Tuple(_)) {
                        return Err(err(format!(
                            "[E_SERDE_INTERNAL_TAG_NON_STRUCT] sum type `{}`: \
                             internally-tagged `#serde(tag=\"{}\")` requires every \
                             variant be unit or record-shaped, but variant `{}` \
                             has a positional (tuple) payload. Use adjacent \
                             (`tag`+`content`) or untagged tagging instead.",
                            td.name, tag, v.name,
                        )));
                    }
                }
            }
            Ok(SerdeTagging::Internal { tag })
        }
        (None, Some(_)) => Err(err(format!(
            "[E_SERDE_CONTENT_WITHOUT_TAG] sum type `{}`: `#serde(content=…)` \
             requires a `tag=…` (adjacently-tagged form `#serde(tag=\"t\", \
             content=\"c\")`).",
            td.name,
        ))),
        (None, None) => Ok(SerdeTagging::External),
    }
}

// ────────────────────────────────────────────────────────────────────────
// Plan 180.1 Ф.1 (field-attributes) / Ф.7 (strict-by-default unknown-field
// policy) / Ф.10 (compile-time wire-contract validation).
//
// `rename`/`rename_all`/`skip`/`skip_serializing_if`/`default`/`alias` are
// resolved ONCE per record type (`resolve_fields`) into `ResolvedField`s
// shared by `synthesize_serialize`/`synthesize_deserialize`; `validate_wire_
// contract` runs the Ф.10 collision checks over the resolved set.
// `flatten` is parsed but synthesis is honestly GATED — see
// `validate_wire_contract`'s dedicated diagnostics ([M-180-serde-flatten]).
// ────────────────────────────────────────────────────────────────────────

/// Human-readable key name for a `SerdeArg` — used in misplaced-attribute /
/// duplicate-attribute diagnostics.
fn serde_arg_key_name(a: &SerdeArg) -> &'static str {
    match a {
        SerdeArg::Tag(_) => "tag",
        SerdeArg::Content(_) => "content",
        SerdeArg::Untagged => "untagged",
        SerdeArg::Rename(_) => "rename",
        SerdeArg::RenameAll(_) => "rename_all",
        SerdeArg::Skip => "skip",
        SerdeArg::SkipSerializingIf(_) => "skip_serializing_if",
        SerdeArg::Default(_) => "default",
        SerdeArg::Alias(_) => "alias",
        SerdeArg::Flatten => "flatten",
        SerdeArg::DenyUnknownFields => "deny_unknown_fields",
        SerdeArg::AllowUnknown => "allow_unknown",
    }
}

/// Plan 180.1 Ф.7: type-level serde options — `rename_all` convention
/// (Ф.1.2) + unknown-field policy. **Reversal (owner-decided 2026-07-22):**
/// absent `allow_unknown` ⇒ STRICT (unknown wire field ⇒ typed
/// `DeError{UnknownField}`) — this is now the DEFAULT (was: ignore-by-
/// default, serde parity). `allow_unknown` opts back OUT to the old
/// ignore-unknown behaviour (forward-compatible API).
#[derive(Debug, Clone, Default)]
pub struct TypeSerdeOptions {
    pub rename_all: Option<RenameConvention>,
    pub allow_unknown: bool,
}

/// Resolve + validate type-level serde options (record types only — callers
/// must have already rejected sum-type placement via `serde_tagging_mode`).
pub fn type_serde_options(td: &TypeDecl) -> Result<TypeSerdeOptions, DeriveError> {
    let err = |msg: String| DeriveError::SerdeTagging { type_name: td.name.clone(), message: msg };
    let mut rename_all: Option<RenameConvention> = None;
    let mut allow_unknown = false;
    let mut deny_unknown = false;
    for a in &td.serde_attrs {
        match a {
            SerdeArg::RenameAll(conv) => {
                if rename_all.is_some() {
                    return Err(err(format!(
                        "[E_SERDE_DUPLICATE_ATTRIBUTE] type `{}`: `rename_all` given more \
                         than once.", td.name)));
                }
                rename_all = Some(*conv);
            }
            SerdeArg::AllowUnknown => allow_unknown = true,
            SerdeArg::DenyUnknownFields => deny_unknown = true,
            _ => {}
        }
    }
    if allow_unknown && deny_unknown {
        return Err(err(format!(
            "[E_SERDE_UNKNOWN_FIELD_POLICY_CONFLICT] type `{}`: `#serde(allow_unknown)` \
             (opt-out) and `#serde(deny_unknown_fields)` (strict — now the DEFAULT, \
             180.1 Ф.7) cannot both be given on the same type.", td.name)));
    }
    Ok(TypeSerdeOptions { rename_all, allow_unknown })
}

/// Plan 180.1 Ф.1: resolved per-field serde customization.
#[derive(Debug, Clone, Default)]
pub struct FieldSerdeOptions {
    pub rename: Option<String>,
    pub skip: bool,
    pub skip_serializing_if: Option<String>,
    /// `None` = no `default` attribute (field is `Required` unless `skip`).
    /// `Some(None)` = bare `default` (zero-value). `Some(Some(fn))` =
    /// `default = "fn"` (zero-arg function call).
    pub default: Option<Option<String>>,
    pub aliases: Vec<String>,
    pub flatten: bool,
}

/// Resolve + validate field-level serde options.
pub fn field_serde_options(
    type_name: &str, field_name: &str, attrs: &[SerdeArg],
) -> Result<FieldSerdeOptions, DeriveError> {
    let err = |msg: String| DeriveError::SerdeTagging { type_name: type_name.to_string(), message: msg };
    let mut opts = FieldSerdeOptions::default();
    for a in attrs {
        match a {
            SerdeArg::Tag(_) | SerdeArg::Content(_) | SerdeArg::Untagged
            | SerdeArg::RenameAll(_) | SerdeArg::AllowUnknown | SerdeArg::DenyUnknownFields => {
                return Err(err(format!(
                    "[E_SERDE_ATTRIBUTE_MISPLACED] type `{}`, field `{}`: `{}` is a \
                     type-level serde attribute and cannot appear on a field. Move it \
                     to the type's own `#serde(...)`.",
                    type_name, field_name, serde_arg_key_name(a))));
            }
            SerdeArg::Rename(s) => {
                if opts.rename.is_some() {
                    return Err(err(format!(
                        "[E_SERDE_DUPLICATE_ATTRIBUTE] type `{}`, field `{}`: `rename` \
                         given more than once.", type_name, field_name)));
                }
                opts.rename = Some(s.clone());
            }
            SerdeArg::Skip => opts.skip = true,
            SerdeArg::SkipSerializingIf(pred) => {
                if opts.skip_serializing_if.is_some() {
                    return Err(err(format!(
                        "[E_SERDE_DUPLICATE_ATTRIBUTE] type `{}`, field `{}`: \
                         `skip_serializing_if` given more than once.", type_name, field_name)));
                }
                opts.skip_serializing_if = Some(pred.clone());
            }
            SerdeArg::Default(fn_name) => {
                if opts.default.is_some() {
                    return Err(err(format!(
                        "[E_SERDE_DUPLICATE_ATTRIBUTE] type `{}`, field `{}`: `default` \
                         given more than once.", type_name, field_name)));
                }
                opts.default = Some(fn_name.clone());
            }
            SerdeArg::Alias(s) => opts.aliases.push(s.clone()),
            SerdeArg::Flatten => opts.flatten = true,
        }
    }
    if opts.skip && opts.rename.is_some() {
        return Err(err(format!(
            "[E_SERDE_SKIP_RENAME_CONFLICT] type `{}`, field `{}`: `skip` + `rename` \
             together is meaningless — a skipped field is never on the wire, so \
             renaming it has no effect. Remove one.", type_name, field_name)));
    }
    Ok(opts)
}

/// Plan 180.1 Ф.1.1/Ф.1.2: effective wire name for a field — field-level
/// `rename` OVERRIDES type-level `rename_all`; absent both, the canonical
/// Nova field name is used as-is.
fn wire_name_for(field_name: &str, opts: &FieldSerdeOptions, rename_all: Option<RenameConvention>) -> String {
    if let Some(r) = &opts.rename {
        r.clone()
    } else if let Some(conv) = rename_all {
        conv.apply(field_name)
    } else {
        field_name.to_string()
    }
}

/// A record field with resolved serde customization + effective wire name —
/// computed once (`resolve_fields`), shared by the serialize/deserialize
/// synthesizers and the Ф.10 wire-contract validation.
struct ResolvedField {
    field: DerivedField,
    opts: FieldSerdeOptions,
    wire: String,
}

/// Plan 180.1 Ф.1/Ф.10: resolve type + per-field serde options for a record
/// type and validate the wire contract. Single entry point shared by
/// `synthesize_serialize`/`synthesize_deserialize`.
fn resolve_fields(td: &TypeDecl, fields: &[DerivedField]) -> Result<(TypeSerdeOptions, Vec<ResolvedField>), DeriveError> {
    let type_opts = type_serde_options(td)?;
    let mut resolved = Vec::with_capacity(fields.len());
    for f in fields {
        let opts = field_serde_options(&td.name, &f.name, &f.serde_attrs)?;
        let wire = wire_name_for(&f.name, &opts, type_opts.rename_all);
        resolved.push(ResolvedField { field: f.clone(), opts, wire });
    }
    validate_wire_contract(td, &resolved, &type_opts)?;
    Ok((type_opts, resolved))
}

/// Plan 180.1 Ф.10: compile-time wire-contract validation, run once per
/// record type after `rename`/`rename_all`/`alias` resolution:
/// - two fields' effective wire names collide → `E_SERDE_WIRE_NAME_COLLISION`
///   (`skip` fields excluded — never on the wire);
/// - an `alias` collides with another field's wire name or another field's
///   alias → the same code;
/// - `flatten` (any field) — currently ALWAYS an error: with the strict-by-
///   default unknown-field policy (Ф.7) this is a real incompatibility
///   (`E_SERDE_FLATTEN_DENY_CONFLICT`) unless the type opts out via
///   `allow_unknown`, in which case actual flatten SYNTHESIS is still
///   unimplemented (`E_SERDE_FLATTEN_UNSUPPORTED`, honest scope-out,
///   `[M-180-serde-flatten]` — 180.1 Ф.1.8, the hardest item).
fn validate_wire_contract(td: &TypeDecl, fields: &[ResolvedField], type_opts: &TypeSerdeOptions) -> Result<(), DeriveError> {
    let err = |msg: String| DeriveError::SerdeTagging { type_name: td.name.clone(), message: msg };
    let mut owners: Vec<(String, String)> = Vec::new(); // (wire_name, field_name), non-skip only
    for rf in fields {
        if rf.opts.skip { continue; }
        if let Some((existing_wire, existing_field)) = owners.iter().find(|(w, _)| *w == rf.wire) {
            return Err(err(format!(
                "[E_SERDE_WIRE_NAME_COLLISION] type `{}`: fields `{}` and `{}` both \
                 resolve to wire name `\"{}\"` (after rename/rename_all).",
                td.name, existing_field, rf.field.name, existing_wire,
            )));
        }
        owners.push((rf.wire.clone(), rf.field.name.clone()));
    }
    for rf in fields {
        for alias in &rf.opts.aliases {
            if let Some((_, owner_field)) = owners.iter().find(|(w, _)| w == alias) {
                if owner_field == &rf.field.name && rf.wire == *alias {
                    continue; // alias identical to own primary wire name — redundant, harmless
                }
                return Err(err(format!(
                    "[E_SERDE_WIRE_NAME_COLLISION] type `{}`: field `{}`'s alias \
                     `\"{}\"` collides with field `{}`'s wire name.",
                    td.name, rf.field.name, alias, owner_field,
                )));
            }
        }
    }
    let mut alias_owners: Vec<(String, String)> = Vec::new();
    for rf in fields {
        for alias in &rf.opts.aliases {
            if let Some((_, owner_field)) = alias_owners.iter().find(|(a, _)| a == alias) {
                if owner_field != &rf.field.name {
                    return Err(err(format!(
                        "[E_SERDE_WIRE_NAME_COLLISION] type `{}`: alias `\"{}\"` is \
                         declared on both field `{}` and field `{}`.",
                        td.name, alias, owner_field, rf.field.name,
                    )));
                }
            }
            alias_owners.push((alias.clone(), rf.field.name.clone()));
        }
    }
    if fields.iter().any(|rf| rf.opts.flatten) {
        if !type_opts.allow_unknown {
            return Err(err(format!(
                "[E_SERDE_FLATTEN_DENY_CONFLICT] type `{}`: `#serde(flatten)` is \
                 incompatible with the (now-default, 180.1 Ф.7) strict unknown-field \
                 policy — a flattened field's inner keys arrive mixed into the parent \
                 wire object and cannot be attributed by the parent's unknown-field \
                 check. Add `#serde(allow_unknown)` to the type — flatten SYNTHESIS \
                 itself remains gated regardless, see [M-180-serde-flatten].",
                td.name,
            )));
        }
        return Err(err(format!(
            "[E_SERDE_FLATTEN_UNSUPPORTED] type `{}`: `#serde(flatten)` synthesis is \
             not yet implemented ([M-180-serde-flatten], 180.1 Ф.1.8 — the hardest \
             attribute, honestly scoped out) — it needs a companion \"fields-only\" \
             synth variant (no begin_struct/end_struct wrapper) the auto-derive \
             machine does not yet emit. Use a nested (non-flattened) sub-object \
             field instead.",
            td.name,
        )));
    }
    Ok(())
}

/// Plan 180.1 Ф.1.3/Ф.1.5: a zero-value AST expression for `ty` (`Option` →
/// typed `None`; numeric → `0 as T`; `bool` → `false`; `str` → `""`; `[]T`/
/// `Vec[T]` → `[]`; `HashMap`/`Map` → `.new()`). `None` ⇒ no computable zero
/// value — caller must require an explicit `#serde(default = "fn")` instead.
fn zero_value_expr(ty: &TypeRef) -> Option<Expr> {
    if option_inner(ty).is_some() {
        return Some(ex(ExprKind::As(Box::new(ident("None")), ty.clone())));
    }
    match ty.strip_modifiers() {
        TypeRef::Named { path, generics, .. } => {
            let name = path.last()?.as_str();
            if generics.is_empty() {
                match name {
                    "int" | "i8" | "i16" | "i32" | "i64"
                    | "uint" | "u8" | "u16" | "u32" | "u64" =>
                        Some(ex(ExprKind::As(Box::new(int_lit(0)), type_ref_named(name)))),
                    "f32" | "f64" =>
                        Some(ex(ExprKind::As(Box::new(ex(ExprKind::FloatLit(0.0))), type_ref_named(name)))),
                    "bool" => Some(ex(ExprKind::BoolLit(false))),
                    "str" => Some(str_lit("")),
                    _ => None,
                }
            } else if name == "Vec" {
                Some(ex(ExprKind::ArrayLit(vec![])))
            } else if name == "HashMap" || name == "Map" {
                Some(member_call(type_static_expr(ty), "new", vec![]))
            } else {
                None
            }
        }
        TypeRef::Array(_, _) | TypeRef::FixedArray(_, _, _) => Some(ex(ExprKind::ArrayLit(vec![]))),
        _ => None,
    }
}

/// Plan 180.1 Ф.1.3/Ф.1.5: resolve the fallback value for a field that is
/// either `skip` (always) or `default`-attributed (wire-absent fallback).
/// Callers must NOT invoke this for a plain `Required` field (no `skip`, no
/// `default`) — that case keeps the natural `MissingField` error.
fn resolve_missing_value(
    type_name: &str, field_name: &str, ty: &TypeRef, default: &Option<Option<String>>,
    file_id: crate::diag::FileId,
) -> Result<Expr, DeriveError> {
    if let Some(Some(fn_name)) = default {
        return Ok(call(ident_at(fn_name, file_id), vec![]));
    }
    match zero_value_expr(ty) {
        Some(e) => Ok(e),
        None => Err(DeriveError::SerdeTagging {
            type_name: type_name.to_string(),
            message: format!(
                "[E_SERDE_SKIP_FIELD_NO_DEFAULT] type `{}`, field `{}` (type `{}`): \
                 `#serde(skip)`/bare `#serde(default)` needs a computable zero value, \
                 but `{}` has none synthesized. Provide `#serde(default = \"fn_name\")` \
                 naming a zero-arg function returning `{}`.",
                type_name, field_name, type_ref_render(ty), type_ref_render(ty), type_ref_render(ty),
            ),
        }),
    }
}

/// Build the Block that reads a field's value assuming cursor `cursor` is
/// ALREADY entered (present) — trailing expression = decoded value. Mirrors
/// the plain per-field decode logic (narrow-scalar inline vs
/// `deser_field_expr`) so it is reusable by both the plain path and the
/// has_field-guarded default/alias fallback chain.
fn build_field_value_block(f_name: &str, ty: &TypeRef, cursor: &str, file_id: crate::diag::FileId) -> Block {
    if let Some(plan) = narrow_scalar_deser_plan(ty) {
        let mut stmts = Vec::new();
        emit_narrow_scalar_deser(&mut stmts, f_name, cursor, &plan);
        Block { stmts, trailing: Some(Box::new(ident(f_name))), span: span_dummy(), is_unsafe: false }
    } else {
        block_trailing(deser_field_expr(ty, cursor, file_id))
    }
}

/// Plan 180.1 Ф.1.5/Ф.1.6: build the deserialize value-expression for a field
/// with `default` and/or `alias` customization — try each candidate wire name
/// (primary first, then aliases, in declaration order) via `has_field`; the
/// FIRST present wins (`enter_field` + decode); if NONE are present, fall back
/// to `missing_block` (pre-resolved by the caller: a default/zero value, a
/// typed `None`, or — for a plain `Required` field with only `alias`
/// customization — a final `enter_field(primary)?` re-attempt so the natural
/// `MissingField(primary)` error still fires, naming the primary wire name).
fn build_field_with_fallback(f_name: &str, ty: &TypeRef, names: &[String], missing_block: Block, file_id: crate::diag::FileId) -> Expr {
    let mut acc = missing_block;
    for (i, name) in names.iter().enumerate().rev() {
        let cursor = format!("__nv_hf_{}_{}", f_name, i);
        let mut then_stmts = vec![let_stmt(&cursor, true, None,
            try_(member_call(ident("d"), "enter_field", vec![str_lit(name)])))];
        let value_block = build_field_value_block(f_name, ty, &cursor, file_id);
        then_stmts.extend(value_block.stmts);
        let then_block = Block {
            stmts: then_stmts, trailing: value_block.trailing, span: span_dummy(), is_unsafe: false,
        };
        let if_expr = ex(ExprKind::If {
            cond: Box::new(try_(member_call(ident("d"), "has_field", vec![str_lit(name)]))),
            then: then_block,
            else_: Some(crate::ast::ElseBranch::Block(acc)),
        });
        acc = block_trailing(if_expr);
    }
    *acc.trailing.expect("build_field_with_fallback: non-empty fold always sets trailing")
}

/// Plan 180.1 Ф.7: the set of wire names a strict-by-default record accepts
/// (primary wire name + all aliases of every NON-skip field) — used to build
/// the unknown-field scan. `skip` fields are excluded (never on the wire).
fn known_wire_names(fields: &[ResolvedField]) -> Vec<String> {
    let mut names = Vec::new();
    for rf in fields {
        if rf.opts.skip { continue; }
        names.push(rf.wire.clone());
        for a in &rf.opts.aliases {
            names.push(a.clone());
        }
    }
    names
}

/// Plan 180.1 Ф.7: build the strict-by-default unknown-field check —
/// `ro __nv_wire_keys = d.map_keys()?; for __nv_uk in __nv_wire_keys { if
/// <not in known> { return Err(DeError.new(UnknownField(__nv_uk))) } }`.
/// Emitted at the START of a record's `.deserialize` body unless the type
/// carries `#serde(allow_unknown)`.
fn build_unknown_field_check(known: &[String]) -> Vec<Stmt> {
    let cond = if known.is_empty() {
        ex(ExprKind::BoolLit(true))
    } else {
        let mut it = known.iter();
        let first = it.next().expect("non-empty checked above");
        let mut acc = binop(BinOp::Neq, ident("__nv_uk"), str_lit(first));
        for n in it {
            acc = binop(BinOp::And, acc, binop(BinOp::Neq, ident("__nv_uk"), str_lit(n)));
        }
        acc
    };
    let raise = Stmt::Return {
        value: Some(call(ident("Err"), vec![
            call(ex(ExprKind::Path(vec!["DeError".to_string(), "new".to_string()])),
                vec![call(ident("UnknownField"), vec![ident("__nv_uk")])]),
        ])),
        span: span_dummy(),
    };
    let inner_if = ex(ExprKind::If {
        cond: Box::new(cond),
        then: Block { stmts: vec![raise], trailing: None, span: span_dummy(), is_unsafe: false },
        else_: None,
    });
    let for_body = Block { stmts: vec![Stmt::Expr(inner_if)], trailing: None, span: span_dummy(), is_unsafe: false };
    let for_loop = ex(ExprKind::For {
        pattern: Pattern::Ident { name: "__nv_uk".to_string(), span: span_dummy(), is_mut: false, is_consume: false },
        iter: Box::new(ident("__nv_wire_keys")),
        body: for_body,
        elem_type: None,
        invariants: vec![],
        decreases: None,
        iter_consume: false,
    });
    vec![
        let_stmt("__nv_wire_keys", false, None, try_(member_call(ident("d"), "map_keys", vec![]))),
        Stmt::Expr(for_loop),
    ]
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
    /// Plan 180.1 Ф.1: field-level `#serde(...)` attributes (empty for
    /// `NamedTupleField` — positional tuple fields don't carry serde attrs,
    /// out of Ф.1 scope; record fields carry theirs verbatim).
    pub serde_attrs: Vec<SerdeArg>,
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
                serde_attrs: f.serde_attrs.clone(),
            }).collect()
        ),
        TypeDeclKind::NamedTuple(fields) => Some(
            fields.iter().map(|f: &NamedTupleField| DerivedField {
                name: f.name.clone(),
                ty: f.ty.clone(),
                span: f.span,
                serde_attrs: Vec::new(),
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
        TypeDeclKind::TypeSet(_) => "type set", // Plan 172.3 (D310)
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
    let is_serde = protocol == SERIALIZE || protocol == DESERIALIZE;
    // Plan 222.8 Ф.1 (D438): Reflect needs its OWN container-aware
    // eligibility (bespoke `Option`/`Vec` recursion, like serde) — a plain
    // field/payload type must either provide an explicit `.reflect()` or
    // declare `#impl(Reflect)`. Kept SEPARATE from `is_serde` (not folded
    // into one flag) because the two have different scalar/container rules
    // (no byte-seq/HashMap/narrow-scalar wire concerns for Reflect).
    let is_reflect = protocol == REFLECT;
    // Validate field eligibility (kind-dependent).
    if let Some(fields) = iter_fields(type_decl) {
        for f in &fields {
            let eligible = if is_serde {
                check_field_eligibility_serde(_ctx.query, &f.ty, protocol, method_name)
            } else if is_reflect {
                check_field_eligibility_reflect(_ctx.query, &f.ty, protocol, method_name)
            } else {
                check_field_eligibility(_ctx.query, &f.ty, protocol, method_name)
            };
            if !eligible {
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
    } else if is_serde || is_reflect {
        // Plan 180 Ф.2-sum (D345) / Plan 222.8 Ф.1 (D438): externally-tagged
        // sum serde / Reflect. Validate that every variant's payload element
        // is eligible (mirror of the record-field check) so an unshapeable
        // payload surfaces a typed `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`
        // (named by variant), not a bad synth. On success, fall through to
        // the sum dispatch below.
        if let Some(variants) = iter_sum_variants(type_decl) {
            for v in variants {
                let payload: Vec<&TypeRef> = match &v.kind {
                    SumVariantKind::Unit => vec![],
                    SumVariantKind::Tuple(tys) => tys.iter().collect(),
                    SumVariantKind::Record(fields) => fields.iter().map(|f| &f.ty).collect(),
                };
                for ty in payload {
                    let eligible = if is_reflect {
                        check_field_eligibility_reflect(_ctx.query, ty, protocol, method_name)
                    } else {
                        check_field_eligibility_serde(_ctx.query, ty, protocol, method_name)
                    };
                    if !eligible {
                        return Err(DeriveError::FieldLacksProtocol {
                            type_name: type_decl.name.clone(),
                            field_name: v.name.clone(),
                            field_type: type_ref_render(ty),
                            protocol: protocol.to_string(),
                        });
                    }
                }
            }
        }
    }

    // Ф.3: dispatch к per-protocol synthesizer body builders.
    match protocol {
        EQUAL       => synthesize_equal(_ctx, type_decl),
        HASH        => synthesize_hash(_ctx, type_decl),
        CLONE       => synthesize_clone(_ctx, type_decl),
        COMPARE     => synthesize_compare(_ctx, type_decl),
        DISPLAY     => synthesize_display(_ctx, type_decl),
        DEBUG       => synthesize_debug(_ctx, type_decl),
        SERIALIZE   => synthesize_serialize(_ctx, type_decl),
        DESERIALIZE => synthesize_deserialize(_ctx, type_decl),
        REFLECT     => synthesize_reflect(_ctx, type_decl),
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

/// Plan 180.1 Ф.1.5: a bare `Ident` reference tagged with a REAL `file_id`
/// (not `span_dummy()`'s `MAIN_FILE_ID`). Needed for `#serde(default =
/// "fn_name")`: the synthesized call to a user free function is the FIRST
/// auto-derive body to reference an arbitrary lowercase user symbol by bare
/// identifier (every other synthesized reference is either a builtin/
/// Capitalized name — bootstrap-exempt in `is_known` — or a `.method()` call,
/// which resolves via the method table, not identifier-scope). The identifier-
/// resolution checker looks up visibility via `group_decls[file_id]` (Plan
/// 42.15 Rule C: peers of ONE folder-module share a declaration namespace
/// keyed by any member file_id of that group); `span_dummy()`'s `MAIN_FILE_ID`
/// resolves to the COMPILATION UNIT's entry file, which is WRONG whenever the
/// type carrying `#impl(Deserialize)` is declared in a module imported from
/// elsewhere (the normal case — DTOs are typically decoded from a different
/// file than where they're declared) — `default_role` would then be looked up
/// in the IMPORTER's own module-group, not the type's, and spuriously fail as
/// `undefined identifier`. Tagging with the type-decl's OWN `file_id` (any
/// member of its peer-group) fixes the lookup regardless of which file ends
/// up as the compilation entry.
fn ident_at(name: &str, file_id: crate::diag::FileId) -> Expr {
    Expr::new(ExprKind::Ident(name.to_string()), Span::with_file(0, 0, file_id))
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

/// Plan 221.1 №33 (`[M-autoderive-extension-import-seed-combined-cu]`): an
/// `Expr` tagged with a REAL `file_id`, mirroring `ident_at` (Plan 180.1
/// Ф.1.5) but for arbitrary expression kinds — needed for `Call`/`Member`
/// nodes, not just bare `Ident`s.
fn ex_at(kind: ExprKind, file_id: crate::diag::FileId) -> Expr {
    Expr::new(kind, Span::with_file(0, 0, file_id))
}

/// `file_id`-tagged `call` — see `ex_at`.
fn call_at(target: Expr, args: Vec<Expr>, file_id: crate::diag::FileId) -> Expr {
    ex_at(ExprKind::Call {
        func: Box::new(target),
        args: args.into_iter().map(CallArg::Item).collect(),
        trailing: None,
    }, file_id)
}

/// Plan 221.1 №33: `file_id`-tagged `member_call` — used for synthesized
/// calls that dispatch to a container/record's OWN `@serialize`/
/// `.deserialize` (an EXTENSION method from the caller's point of view,
/// e.g. `HashMap[str,V].serialize`, declared in `std.encoding.serde`, not in
/// `HashMap`'s own defining module `collections.hashmap`). Plain
/// `member_call`'s `span_dummy()` always carries `file_id = MAIN_FILE_ID`
/// (the COMPILATION UNIT's entry file — see `Span::dummy()`), which
/// `check_extension_method_policy` (types/mod.rs) reads to decide WHICH
/// file's import list to check. For a call written by the user, that's
/// sound (a call always lives in its own file). For a compiler-synthesized
/// call it is NOT: the synthesized body conceptually belongs to the type
/// declaration's OWN file (whichever module carries `#impl(Serialize)`),
/// not to whichever file the compiler happens to have been invoked on as
/// its entry point. Tagging with the type-decl's own `file_id` (mirroring
/// `make_serde_method`'s `fn_span` fix, 180.1 Ф.1.5/6af52e8d9) makes the
/// policy check the DECLARING file's own imports — the file that actually
/// wrote `#impl(Serialize)` and is expected to have imported the container's
/// serde extension — regardless of which file ends up as the entry of
/// whatever larger compilation unit pulls this type in (the combined-CU
/// case that regressed: http.server + http.servernet in one CU, entry =
/// neither file that declares/imports the type).
fn member_call_at(obj: Expr, method: &str, args: Vec<Expr>, file_id: crate::diag::FileId) -> Expr {
    let func = ex_at(ExprKind::Member {
        obj: Box::new(obj),
        name: method.to_string(),
    }, file_id);
    call_at(func, args, file_id)
}

fn member_call(obj: Expr, method: &str, args: Vec<Expr>) -> Expr {
    let func = ex(ExprKind::Member {
        obj: Box::new(obj),
        name: method.to_string(),
    });
    call(func, args)
}

/// Plan 208 Ф.2 (D422 §1, D374 AMEND ×2): `Write.@write` now takes `[]u8`,
/// not `str` — every synthesized `w.write(<str-expr>)` below needs an
/// explicit `.bytes()` (D176) rather than relying on the D55 literal→`[]u8`
/// coercion (which only covers a BARE string-literal argument expression,
/// not `str.from(x)`'s call-result — and this helper is used for both
/// shapes uniformly, so it always emits the explicit conversion rather than
/// depending on which of the two shapes the coercion mechanism does or does
/// not reach).
fn to_bytes(e: Expr) -> Expr {
    member_call(e, "bytes", vec![])
}

fn binop(op: BinOp, l: Expr, r: Expr) -> Expr {
    ex(ExprKind::Binary {
        op,
        left: Box::new(l),
        right: Box::new(r),
    })
}

/// Plan 180.1 Ф.1.4: `!<e>` — used by `skip_serializing_if`'s inverted guard.
fn not_expr(e: Expr) -> Expr {
    ex(ExprKind::Unary { op: crate::ast::UnOp::Not, operand: Box::new(e) })
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
///
/// #659/#660 (2026-08-14): the shell's spans carry the DECLARING TYPE's
/// `file_id`, not `span_dummy()`'s `MAIN_FILE_ID`. The emitter resolves the
/// receiver's C base name through the span's file (`emit_fn_forward_decl`
/// sets `current_emit_file_id` from `f.span.file_id`; `ref_type_base` then
/// consults `file_type_module[(file, name)]`). With `MAIN_FILE_ID` that
/// lookup runs against the CU's ENTRY file, so for a name-colliding type
/// (two `Node`s in one merged CU — D381 qualification) the receiver either
/// resolved to a BARE base nobody emits (`Nova_Node`, unknown type — #659)
/// or to the OTHER module's same-named type (wrong union members — #660),
/// depending on what the entry file happened to import. Exact mirror of the
/// `make_serde_method` fix below (Plan 180.1 F.1.5): only `file_id` is
/// load-bearing, a dummy start/end never renders in a diagnostic.
fn make_synth_method(
    type_name: &str,
    method_name: &str,
    params: Vec<Param>,
    return_type: Option<TypeRef>,
    body: FnBody,
    file_id: crate::diag::FileId,
) -> FnDecl {
    let fn_span = Span::with_file(0, 0, file_id);
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
            span: fn_span,
        }),
        params,
        effects: vec![],
        return_type,
        return_is_const: false,
        returns_receiver: false,
        body,
        span: fn_span,
        is_export: false,
        is_external: false,
        // Plan 126.2 Ф.1: mark synthesized auto-derive method so downstream
        // passes (method_table registration + Plan 127 lint-suppression) can
        // distinguish compiler-generated bodies from user source.
        compiler_generated: true,
        ..FnDecl::default()
    }
}

fn make_param_mut(name: &str, ty: TypeRef) -> Param {
    Param { is_mut: true, ..make_param(name, ty) }
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
        fiber_safe_attr: false,
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
    } else if let Some(variants) = iter_sum_variants(type_decl) {
        // Sum-type equal: same-variant + payload-wise `==` (D345 / Plan 180 Ф.1).
        // [M-126-sum-equal-rich] CLOSED.
        synth_equal_sum_body(variants)
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
        type_decl.span.file_id,
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
    } else if let Some(variants) = iter_sum_variants(type_decl) {
        // Sum-type hash: variant-index seed + payload-hash combine (Plan 180 Ф.1).
        // [M-126-sum-hash-rich] CLOSED.
        synth_hash_sum_body(variants)
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
        type_decl.span.file_id,
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
    } else if let Some(variants) = iter_sum_variants(type_decl) {
        // Sum-type clone: match-arm-per-variant reconstruction (Plan 180 Ф.1).
        // [M-126-sum-clone-rich] CLOSED.
        synth_clone_sum_body(variants)
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
        type_decl.span.file_id,
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
        inferred_target_type: None,
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
    } else if let Some(variants) = iter_sum_variants(type_decl) {
        // Sum-type compare: variant-index order, then payload lexicographic
        // (Plan 180 Ф.1). [M-126-sum-compare-rich] CLOSED.
        synth_compare_sum_body(variants)
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
        type_decl.span.file_id,
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
                is_consume: false,
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

/// Synthesize `@display(w Fmt) -> ()` — memberwise format.
/// D237: renamed from synthesize_fmt (Printable → Display, @fmt → @display).
/// Plan 152.7.1 (D374 AMEND): param changed from `sb StringBuilder` to `w Write`.
/// Plan 208 Ф.2 (D422 §3): param changed AGAIN, `w Write` → `w Fmt` (D374
/// AMEND ×2) — `Display`/`Debug` are now REQUIRED (no to_str-calling default,
/// D422 §3 invariant), so auto-derive is the ONLY source of a `@display`/
/// `@debug` body for a structural type that never hand-writes one — this
/// synthesizer's output signature must match the (now Fmt-typed) protocol
/// exactly or the synthesized method fails to satisfy `Display`/`Debug`.
///
/// Output form (Plan 208 Ф.3, D422 §4): **compact/positional**
/// `TypeName(f1_value, f2_value)` — NO field names, distinct from Debug's
/// named `TypeName { f1: v1, f2: v2 }` (see `synth_debug_record_body`). This
/// is the divergence D422 §4 calls for; Ф.2 left both forms identical
/// (named) as a signature-migration-only interim step — see
/// `docs/plans/208-impl-progress.md`.
/// Empty type-body → `w.write("TypeName".bytes())` (nothing to diverge with
/// zero fields — matches Rust's unit-struct Debug output too).
/// Field values are emitted via a UNIFORM `field.display(w)` call — every
/// type (primitive or composite) now implements `Display` (Plan 208 Ф.2
/// primitives, `std/prelude/protocols.nv`), so there is no special-case for
/// primitive fields as before. (The old primitive branch called the now-
/// retracted `str.from(x)` static method — Plan 174.2 — which had been dead
/// code since it was never covered by any field-eligible fixture; this
/// synth's Debug counterpart already used the equivalent uniform
/// `field.debug(w)` call, so Display now mirrors it.)
/// Sum-type → variant-aware (`synth_fmt_sum_body`), same compact-vs-named
/// divergence for `Record`-kind variants.
pub fn synthesize_display<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    let body = if let Some(fields) = iter_fields(type_decl) {
        synth_display_record_body(&type_decl.name, &fields)
    } else if let Some(variants) = iter_sum_variants(type_decl) {
        // Sum-type display: variant-aware output (Plan 180 Ф.1).
        // [M-126-sum-fmt-rich] CLOSED.
        synth_fmt_sum_body(variants, false)
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
        vec![make_param_mut("w", type_ref_named("Fmt"))],
        Some(TypeRef::Unit(span_dummy())),
        body,
        type_decl.span.file_id,
    ))
}

/// Synthesize `@debug(w Fmt) -> ()` — memberwise debug format.
/// D237: renamed from synthesize_debug_fmt (DebugPrintable → Debug, @debug_fmt → @debug).
/// Plan 152.7.1 (D374 AMEND): param changed from `sb StringBuilder` to `w Write`.
/// Plan 208 Ф.2 (D422 §3): param changed AGAIN, `w Write` → `w Fmt` — see
/// `synthesize_display` doc comment above (same rationale, required-no-default).
///
/// Output form: **named** `TypeName { f1: <debug_f1>, f2: <debug_f2> }` —
/// UNCHANGED since Ф.2 (this is the "diagnostic, with field names" half of
/// D422 §4's divergence; `synthesize_display` above got the Ф.3 positional
/// rewrite, this one was already the target shape).
/// Empty type-body → `w.write("TypeName".bytes())`.
/// Sum-type → variant-aware (`synth_fmt_sum_body`).
pub fn synthesize_debug<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    let body = if let Some(fields) = iter_fields(type_decl) {
        synth_debug_record_body(&type_decl.name, &fields)
    } else if let Some(variants) = iter_sum_variants(type_decl) {
        // Sum-type debug: variant-aware output (Plan 180 Ф.1).
        // [M-126-sum-fmt-rich] CLOSED.
        synth_fmt_sum_body(variants, true)
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
        vec![make_param_mut("w", type_ref_named("Fmt"))],
        Some(TypeRef::Unit(span_dummy())),
        body,
        type_decl.span.file_id,
    ))
}

fn simple_display_block(type_name: &str) -> Block {
    Block {
        stmts: vec![Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![to_bytes(ex(ExprKind::StrLit(type_name.to_string())))],
        ))],
        trailing: None,
        span: span_dummy(),
        is_unsafe: false,
    }
}

/// Plan 208 Ф.3 (D422 §4): compact/positional form — `TypeName(v1, v2)`, no
/// field names, unlike `synth_debug_record_body`'s named `{ f: v }` form.
/// Every field (primitive or composite) dispatches uniformly via
/// `field.display(w)` — primitives are `Display` too now (Plan 208 Ф.2), so
/// there is nothing left to special-case (see `synthesize_display` doc
/// comment for why the old primitive branch, which called the retracted
/// `str.from(x)`, is gone).
fn synth_display_record_body(type_name: &str, fields: &[DerivedField]) -> FnBody {
    let mut stmts: Vec<Stmt> = Vec::new();
    if fields.is_empty() {
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![to_bytes(ex(ExprKind::StrLit(type_name.to_string())))],
        )));
    } else {
        // w.write("TypeName(".bytes())
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![to_bytes(ex(ExprKind::StrLit(format!("{}(", type_name))))],
        )));
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                // [race-198 / 196.6] `write` (NOT `write_str`): the `w: Fmt`
                // param (Plan 208 Ф.2 — was `w: Write`) lowers to a CONCRETE
                // sink (type_ref_to_c special case), and StringBuilder has
                // `mut @write(bytes []u8)` but NO `write_str` — a `write_str`
                // call here fell through to the single-key `method_receivers`
                // name-only fallback (documented last-wins) and dispatched to
                // whichever OTHER type in the CU happened to register
                // `write_str` last: WriteBuffer in a small CU (accidentally
                // layout-compatible `{buf Vec[u8]}` → silently "worked"),
                // TcpStream in a merged CU with std.net (foreign struct
                // offset read → 0xC0000005, the Plan 198 floating-AV blocker;
                // see docs/plans/196.6-race-state-dump-notes.md).
                stmts.push(Stmt::Expr(member_call(
                    ident("w"),
                    "write",
                    vec![to_bytes(ex(ExprKind::StrLit(", ".to_string())))],
                )));
            }
            // Uniform dispatch — every type implements Display (Plan 208 Ф.2
            // gave int/f64/f32/bool/char/str their own `@display`), so a
            // plain `field.display(w)` works whether `f.ty` is primitive or
            // composite. No more `str.from(x)` (retracted, Plan 174.2).
            stmts.push(Stmt::Expr(member_call(
                self_field(&f.name),
                "display",
                vec![ident("w")],
            )));
        }
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![to_bytes(ex(ExprKind::StrLit(")".to_string())))],
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
            vec![to_bytes(ex(ExprKind::StrLit(type_name.to_string())))],
        )));
    } else {
        stmts.push(Stmt::Expr(member_call(
            ident("w"),
            "write",
            vec![to_bytes(ex(ExprKind::StrLit(format!("{} {{ ", type_name))))],
        )));
        for (i, f) in fields.iter().enumerate() {
            let prefix = if i == 0 {
                format!("{}: ", f.name)
            } else {
                format!(", {}: ", f.name)
            };
            // [race-198 / 196.6] `write` (NOT `write_str`) — см. комментарий в
            // synth_display_record_body выше (StringBuilder has no write_str;
            // name-only fallback dispatched to a foreign type → merged-CU AV).
            stmts.push(Stmt::Expr(member_call(
                ident("w"),
                "write",
                vec![to_bytes(ex(ExprKind::StrLit(prefix)))],
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
            vec![to_bytes(ex(ExprKind::StrLit(" }".to_string())))],
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
// Plan 180 Ф.1 — sum-type rich auto-derive synthesizers.
// Closes [M-126-sum-equal-rich]/-clone-rich/-hash-rich (+ -compare-rich/-fmt-rich).
//
// The record-path synthesizers above walk fields; sum-types instead emit a
// `match @ { … }` with one arm per variant. Each variant is one of three shapes
// (`SumVariantKind`): `Unit` (no payload), `Tuple(tys)` (positional payload),
// `Record(fields)` (named payload). Payload elements are bound in the arm
// pattern and recursed into exactly like record fields (primitives shallow /
// via `str.from`, composites via the protocol method).
//
// IMPORTANT (ordering): these methods inject AFTER type-check (unlike serde),
// so the emitted `match` / variant-patterns / variant-construction must be
// lowerable by codegen's annotation-free `infer_expr_c_type`. The scrutinee is
// `@` (receiver type known) and `other` is a `Self` param (its type known), so
// codegen resolves variant tags + payload C-types the same way it does for a
// hand-written `.nv` `@debug` on `Option`/`Result` (protocols.nv).
// ────────────────────────────────────────────────────────────────────────

/// `match scrutinee { arms }` expression.
fn ex_match(scrutinee: Expr, arms: Vec<MatchArm>) -> Expr {
    ex(ExprKind::Match { scrutinee: Box::new(scrutinee), arms })
}

fn match_arm_expr(pattern: Pattern, body: Expr) -> MatchArm {
    MatchArm { pattern, guard: None, body: MatchArmBody::Expr(body), span: span_dummy() }
}

fn match_arm_block(pattern: Pattern, body: Block) -> MatchArm {
    MatchArm { pattern, guard: None, body: MatchArmBody::Block(body), span: span_dummy() }
}

fn ident_pat(name: &str) -> Pattern {
    Pattern::Ident { name: name.to_string(), span: span_dummy(), is_mut: false, is_consume: false }
}

fn wildcard_pat() -> Pattern {
    Pattern::Wildcard(span_dummy())
}

/// Pattern that matches variant `v` and binds each payload element to a local
/// named `{prefix}{i}` (tuple) / `{prefix}{fieldname}` (record). Returns the
/// pattern plus the list of `(bind_name, element_type)` for the payload (in
/// declaration order); empty for a `Unit` variant.
fn variant_bind_pattern(v: &SumVariant, prefix: &str) -> (Pattern, Vec<(String, TypeRef)>) {
    match &v.kind {
        SumVariantKind::Unit => (
            Pattern::Variant {
                path: vec![v.name.clone()],
                kind: VariantPatternKind::Unit,
                span: span_dummy(),
            },
            vec![],
        ),
        SumVariantKind::Tuple(tys) => {
            let binds: Vec<(String, TypeRef)> = tys
                .iter()
                .enumerate()
                .map(|(i, t)| (format!("{}{}", prefix, i), t.clone()))
                .collect();
            let pat = Pattern::Variant {
                path: vec![v.name.clone()],
                kind: VariantPatternKind::Tuple {
                    patterns: binds.iter().map(|(n, _)| ident_pat(n)).collect(),
                    rest: false,
                },
                span: span_dummy(),
            };
            (pat, binds)
        }
        SumVariantKind::Record(fields) => {
            let binds: Vec<(String, TypeRef)> = fields
                .iter()
                .map(|f| (format!("{}{}", prefix, f.name), f.ty.clone()))
                .collect();
            let pat = Pattern::Record {
                type_path: Some(vec![v.name.clone()]),
                fields: fields
                    .iter()
                    .zip(&binds)
                    .map(|(f, (bind, _))| RecordPatternField {
                        name: f.name.clone(),
                        pattern: Some(ident_pat(bind)),
                        span: span_dummy(),
                    })
                    .collect(),
                rest: false,
                span: span_dummy(),
            };
            (pat, binds)
        }
    }
}

/// Pattern that matches variant `v` while IGNORING its payload — used for tag
/// discrimination (`| V(..)` / `| V{ .. }` / `| V`).
fn variant_ignore_pattern(v: &SumVariant) -> Pattern {
    match &v.kind {
        SumVariantKind::Unit => Pattern::Variant {
            path: vec![v.name.clone()],
            kind: VariantPatternKind::Unit,
            span: span_dummy(),
        },
        SumVariantKind::Tuple(_) => Pattern::Variant {
            path: vec![v.name.clone()],
            kind: VariantPatternKind::Tuple { patterns: vec![], rest: true },
            span: span_dummy(),
        },
        SumVariantKind::Record(_) => Pattern::Record {
            type_path: Some(vec![v.name.clone()]),
            fields: vec![],
            rest: true,
            span: span_dummy(),
        },
    }
}

/// Reconstruct variant `v` from payload value-expressions `values` (in
/// declaration order; empty for `Unit`).
fn variant_construct(v: &SumVariant, values: Vec<Expr>) -> Expr {
    match &v.kind {
        SumVariantKind::Unit => ident(&v.name),
        SumVariantKind::Tuple(_) => call(ident(&v.name), values),
        SumVariantKind::Record(fields) => ex(ExprKind::RecordLit {
            type_name: Some(vec![v.name.clone()]),
            fields: fields
                .iter()
                .zip(values)
                .map(|(f, val)| RecordLitField {
                    name: f.name.clone(),
                    value: Some(val),
                    is_spread: false,
                    at_shorthand: false,
                    span: span_dummy(),
                })
                .collect(),
            inferred_map_v: None,
            inferred_target_type: None,
        }),
    }
}

/// Sum `@equal`: same-variant + payload-wise `==`. Different variants → false.
/// Emitted form (per variant):
///   `V(a0,a1) => match other { V(b0,b1) => a0 == b0 && a1 == b1, _ => false }`
fn synth_equal_sum_body(variants: &[SumVariant]) -> Expr {
    if variants.is_empty() {
        return ex(ExprKind::BoolLit(true));
    }
    let arms = variants
        .iter()
        .map(|v| {
            let (self_pat, self_binds) = variant_bind_pattern(v, "__nv_a_");
            let (other_pat, other_binds) = variant_bind_pattern(v, "__nv_b_");
            // Payload-wise `a_i == b_i` chained with `&&`; unit/empty → true.
            let eq_expr = if self_binds.is_empty() {
                ex(ExprKind::BoolLit(true))
            } else {
                let mut acc: Option<Expr> = None;
                for ((a, _), (b, _)) in self_binds.iter().zip(&other_binds) {
                    let cmp = binop(BinOp::Eq, ident(a), ident(b));
                    acc = Some(match acc {
                        Some(prev) => binop(BinOp::And, prev, cmp),
                        None => cmp,
                    });
                }
                acc.unwrap()
            };
            let inner = ex_match(
                ident("other"),
                vec![
                    match_arm_expr(other_pat, eq_expr),
                    match_arm_expr(wildcard_pat(), ex(ExprKind::BoolLit(false))),
                ],
            );
            match_arm_expr(self_pat, inner)
        })
        .collect();
    ex_match(ex(ExprKind::SelfAccess), arms)
}

/// Sum `@hash`: variant-index seed combined with payload hashes (same
/// rotate-and-XOR combine as the record path). Unit variant → just the seed.
fn synth_hash_sum_body(variants: &[SumVariant]) -> Expr {
    if variants.is_empty() {
        return ex(ExprKind::IntLit(0));
    }
    // Reuse the record-path rotate: `(h << s) | (h >> (64 - s))`.
    let rotl = |h: Expr, s: i64| -> Expr {
        let left = binop(BinOp::Shl, h.clone(), ex(ExprKind::IntLit(s)));
        let right = binop(BinOp::Shr, h, ex(ExprKind::IntLit(64 - s)));
        binop(BinOp::BitOr, left, right)
    };
    let arms = variants
        .iter()
        .enumerate()
        .map(|(idx, v)| {
            let (pat, binds) = variant_bind_pattern(v, "__nv_h_");
            // Seed = variant discriminant so distinct unit variants hash apart.
            let mut acc = ex(ExprKind::IntLit(idx as i64 + 1));
            for (i, (bind, _)) in binds.iter().enumerate() {
                let h = member_call(ident(bind), "hash", vec![]);
                let s = (((13 * (i + 1)) % 63) + 1) as i64; // 1..=63
                acc = binop(BinOp::BitXor, acc, rotl(h, s));
            }
            match_arm_expr(pat, acc)
        })
        .collect();
    ex_match(ex(ExprKind::SelfAccess), arms)
}

/// Sum `@clone`: match-arm-per-variant reconstruction, payloads cloned
/// (primitives shallow-copied, composites via `.clone()`).
fn synth_clone_sum_body(variants: &[SumVariant]) -> Expr {
    if variants.is_empty() {
        return ex(ExprKind::SelfAccess);
    }
    let arms = variants
        .iter()
        .map(|v| {
            let (pat, binds) = variant_bind_pattern(v, "__nv_c_");
            let values: Vec<Expr> = binds
                .iter()
                .map(|(bind, ty)| {
                    if is_primitive_field(ty) {
                        ident(bind)
                    } else {
                        member_call(ident(bind), "clone", vec![])
                    }
                })
                .collect();
            match_arm_expr(pat, variant_construct(v, values))
        })
        .collect();
    ex_match(ex(ExprKind::SelfAccess), arms)
}

/// Sum `@compare`: order by variant index first, then lexicographically by
/// payload within the same variant. Emitted as a block:
///   `ro __a = match @ {..}; ro __b = match other {..};`
///   `if __a != __b { return __a.compare(__b) }`
///   `match @ { V(a..) => match other { V(b..) => <chain, 0>, _ => 0 }, .. }`
fn synth_compare_sum_body(variants: &[SumVariant]) -> FnBody {
    if variants.is_empty() {
        return FnBody::Expr(ex(ExprKind::IntLit(0)));
    }
    // Tag-extraction match: `match <scrut> { V0.. => 0, V1.. => 1, ... }`.
    let tag_match = |scrut: Expr| -> Expr {
        let arms = variants
            .iter()
            .enumerate()
            .map(|(idx, v)| match_arm_expr(variant_ignore_pattern(v), ex(ExprKind::IntLit(idx as i64))))
            .collect();
        ex_match(scrut, arms)
    };
    let mut stmts: Vec<Stmt> = Vec::new();
    stmts.push(let_stmt("__nv_ta", false, Some(type_ref_named("int")), tag_match(ex(ExprKind::SelfAccess))));
    stmts.push(let_stmt("__nv_tb", false, Some(type_ref_named("int")), tag_match(ident("other"))));
    // if __nv_ta != __nv_tb { return __nv_ta.compare(__nv_tb) }
    let tag_cmp = member_call(ident("__nv_ta"), "compare", vec![ident("__nv_tb")]);
    stmts.push(Stmt::Expr(ex(ExprKind::If {
        cond: Box::new(binop(BinOp::Neq, ident("__nv_ta"), ident("__nv_tb"))),
        then: Block {
            stmts: vec![Stmt::Return { value: Some(tag_cmp), span: span_dummy() }],
            trailing: None,
            span: span_dummy(),
            is_unsafe: false,
        },
        else_: None,
    })));
    // Same-variant payload compare.
    let arms = variants
        .iter()
        .map(|v| {
            let (self_pat, self_binds) = variant_bind_pattern(v, "__nv_a_");
            let (other_pat, other_binds) = variant_bind_pattern(v, "__nv_b_");
            // Inner arm body: lexicographic compare chain (or `0` for unit).
            let inner_body: MatchArmBody = if self_binds.is_empty() {
                MatchArmBody::Expr(ex(ExprKind::IntLit(0)))
            } else {
                let mut cstmts: Vec<Stmt> = Vec::new();
                for (i, ((a, _), (b, _))) in self_binds.iter().zip(&other_binds).enumerate() {
                    let var = format!("__nv_cc_{}", i);
                    cstmts.push(let_stmt(
                        &var, false, Some(type_ref_named("int")),
                        member_call(ident(a), "compare", vec![ident(b)]),
                    ));
                    cstmts.push(Stmt::Expr(ex(ExprKind::If {
                        cond: Box::new(binop(BinOp::Neq, ident(&var), ex(ExprKind::IntLit(0)))),
                        then: Block {
                            stmts: vec![Stmt::Return { value: Some(ident(&var)), span: span_dummy() }],
                            trailing: None,
                            span: span_dummy(),
                            is_unsafe: false,
                        },
                        else_: None,
                    })));
                }
                MatchArmBody::Block(block_with_trailing(cstmts, ex(ExprKind::IntLit(0))))
            };
            let inner = ex_match(
                ident("other"),
                vec![
                    MatchArm { pattern: other_pat, guard: None, body: inner_body, span: span_dummy() },
                    match_arm_expr(wildcard_pat(), ex(ExprKind::IntLit(0))),
                ],
            );
            match_arm_expr(self_pat, inner)
        })
        .collect();
    let tail = ex_match(ex(ExprKind::SelfAccess), arms);
    FnBody::Block(block_with_trailing(stmts, tail))
}

/// Sum `@display`/`@debug`: variant-aware output. `Unit` → `"V"` (both).
/// `Tuple` → `"V(x, y)"` (both — a tuple payload has no field names to begin
/// with, so there's nothing for Display/Debug to diverge over). `Record` →
/// Plan 208 Ф.3 (D422 §4) divergence: Debug keeps the named
/// `"V { f: x, g: y }"`; Display drops the field names, `"V(x, y)"` (mirrors
/// `synth_display_record_body`'s top-level-record positional form). Payload
/// values dispatch UNIFORMLY via `x.display(w)`/`x.debug(w)` regardless of
/// primitive-vs-composite (Plan 208 Ф.2 gave every primitive its own
/// `@display`/`@debug`) — the old Display-primitive branch called the
/// retracted `str.from(x)` (Plan 174.2), which was dead/broken code (no
/// existing fixture exercised a sum-type Display auto-derive with a
/// primitive payload); this fixes it in the same pass that adds the
/// Record-variant divergence, per zero-tolerance-bugs (found in code this
/// same change touches).
fn synth_fmt_sum_body(variants: &[SumVariant], is_debug: bool) -> FnBody {
    if variants.is_empty() {
        // No variants — emit nothing writable; write empty type marker is odd,
        // fall back to a no-op block (unreachable at runtime).
        return FnBody::Block(Block { stmts: vec![], trailing: None, span: span_dummy(), is_unsafe: false });
    }
    let write_lit = |s: String| -> Stmt {
        Stmt::Expr(member_call(ident("w"), "write", vec![to_bytes(ex(ExprKind::StrLit(s)))]))
    };
    // Emit one payload value into `w` — uniform dispatch, no primitive
    // special-case (every type implements both Display and Debug now).
    let emit_value = |bind: &str| -> Stmt {
        let method = if is_debug { "debug" } else { "display" };
        Stmt::Expr(member_call(ident(bind), method, vec![ident("w")]))
    };
    let arms = variants
        .iter()
        .map(|v| {
            let (pat, binds) = variant_bind_pattern(v, "__nv_f_");
            let mut stmts: Vec<Stmt> = Vec::new();
            match &v.kind {
                SumVariantKind::Unit => {
                    stmts.push(write_lit(v.name.clone()));
                }
                SumVariantKind::Tuple(_) => {
                    stmts.push(write_lit(format!("{}(", v.name)));
                    for (i, (bind, _ty)) in binds.iter().enumerate() {
                        if i > 0 {
                            stmts.push(write_lit(", ".to_string()));
                        }
                        stmts.push(emit_value(bind));
                    }
                    stmts.push(write_lit(")".to_string()));
                }
                SumVariantKind::Record(fields) => {
                    if is_debug {
                        // Named form — unchanged from pre-Ф.3.
                        stmts.push(write_lit(format!("{} {{ ", v.name)));
                        for (i, ((bind, _ty), f)) in binds.iter().zip(fields).enumerate() {
                            let prefix = if i == 0 {
                                format!("{}: ", f.name)
                            } else {
                                format!(", {}: ", f.name)
                            };
                            // [race-198 / 196.6] `write` (NOT `write_str`) —
                            // см. synth_display_record_body.
                            stmts.push(Stmt::Expr(member_call(
                                ident("w"), "write", vec![to_bytes(ex(ExprKind::StrLit(prefix)))])));
                            stmts.push(emit_value(bind));
                        }
                        stmts.push(write_lit(" }".to_string()));
                    } else {
                        // Plan 208 Ф.3 (D422 §4): positional form — field
                        // names dropped, same shape as a Tuple-kind variant.
                        stmts.push(write_lit(format!("{}(", v.name)));
                        for (i, (bind, _ty)) in binds.iter().enumerate() {
                            if i > 0 {
                                stmts.push(write_lit(", ".to_string()));
                            }
                            stmts.push(emit_value(bind));
                        }
                        stmts.push(write_lit(")".to_string()));
                    }
                }
            }
            match_arm_block(pat, Block { stmts, trailing: None, span: span_dummy(), is_unsafe: false })
        })
        .collect();
    let body = ex_match(ex(ExprKind::SelfAccess), arms);
    FnBody::Block(Block {
        stmts: vec![Stmt::Expr(body)],
        trailing: None,
        span: span_dummy(),
        is_unsafe: false,
    })
}

// ────────────────────────────────────────────────────────────────────────
// Plan 180 — serde auto-derive synthesizers (Serialize / Deserialize).
// Record-path only (SUM gated: [M-126-sum-*-rich] / 180.2). Emitted shapes are
// exactly those validated by the manual-impl round-trip (nova_tests/serde/):
//   @serialize:  s.begin_struct(name, N)?; per field s.struct_field("k")?;
//                @field.serialize(s)?; s.end_struct()  — UNIFORM (like @debug).
//   .deserialize: per field  mut sub = d.enter_field[_or_null]("k")?;
//                then TYPE-DIRECTED read (scalar → sub.deser_X()?; record/
//                container → <T>.deserialize(sub)?; Option → inline null-check);
//                Ok(Type{ f1, f2, … }).
// ────────────────────────────────────────────────────────────────────────

fn str_lit(s: &str) -> Expr { ex(ExprKind::StrLit(s.to_string())) }
fn int_lit(n: i64) -> Expr { ex(ExprKind::IntLit(n)) }
fn try_(e: Expr) -> Expr { ex(ExprKind::Try(Box::new(e))) }

fn let_stmt(name: &str, mutable: bool, ty: Option<TypeRef>, value: Expr) -> Stmt {
    Stmt::Let(crate::ast::LetDecl {
        mutable,
        pattern: crate::ast::Pattern::Ident {
            name: name.to_string(), span: span_dummy(), is_mut: mutable, is_consume: false,
        },
        ty,
        value,
        span: span_dummy(),
        is_ghost: false,
        consume: false,
    })
}

fn block_trailing(e: Expr) -> Block {
    Block { stmts: vec![], trailing: Some(Box::new(e)), span: span_dummy(), is_unsafe: false }
}

/// `Result[ok, err]` type-ref.
fn result_ty(ok: TypeRef, err: &str) -> TypeRef {
    TypeRef::Named {
        path: vec!["Result".to_string()],
        generics: vec![ok, type_ref_named(err)],
        span: span_dummy(),
    }
}

/// `Some(inner)`.
fn some_call(inner: Expr) -> Expr { call(ident("Some"), vec![inner]) }

/// `if <null_cond>? { None as opt_ty } else { Some(<inner>) }` — the inline
/// Option null-check used for both top-level and nested `Option` fields. The
/// `None` carries an explicit `as Option[T]` ascription: a bare `None` in the
/// then-branch of a NESTED check mis-infers to the INNER option type
/// (`Some(inner)` is `Option[Option[T]]` but the then/else would disagree), so
/// the cast pins it to the full field type.
fn try_wrap_none(null_cond: Expr, inner: Expr, opt_ty: &TypeRef) -> Expr {
    let typed_none = ex(ExprKind::As(Box::new(ident("None")), opt_ty.clone()));
    ex(ExprKind::If {
        cond: Box::new(try_(null_cond)),
        then: block_trailing(typed_none),
        else_: Some(crate::ast::ElseBranch::Block(block_trailing(some_call(inner)))),
    })
}

/// Map a "same-width" scalar field type → the `Deserializer` instance method
/// that reads it directly (the method's return type matches the field type, so
/// no narrowing/range-check is needed). `None` ⇒ not a same-width scalar
/// (narrow scalar → `narrow_deser_fn`; record/container → static `.deserialize`).
fn scalar_deser_method(ty: &TypeRef) -> Option<&'static str> {
    match type_ref_name(ty)? {
        "int"  => Some("deser_int"),
        "str"  => Some("deser_str"),
        "f64"  => Some("deser_float"),
        "bool" => Some("deser_bool"),
        "u64"  => Some("deser_uint"),
        _ => None,
    }
}

/// Narrow-scalar deserialize plan: which `Deserializer` read method feeds the
/// value, the narrow cast target, and the inclusive `[min, max]` bounds the
/// (already exact-integer, D346) wire value must satisfy. A primitive
/// `T.deserialize` static call does NOT dispatch cleanly through codegen (the
/// `?` mis-lowers), so the synthesizer INLINES: read via the protocol method
/// (which works), range-guard, then cast. `None` bound ⇒ no check on that side
/// (`i64`/`uint` are full-width; unsigned needs no lower bound — `deser_uint`
/// already rejects negatives; `f32` is a lossy narrowing with no integer bound).
struct NarrowScalarPlan {
    read: &'static str,
    cast: &'static str,
    min: Option<i64>,
    max: Option<i64>,
}

fn narrow_scalar_deser_plan(ty: &TypeRef) -> Option<NarrowScalarPlan> {
    let (read, cast, min, max) = match type_ref_name(ty)? {
        "i8"   => ("deser_int",   "i8",   Some(-128),        Some(127)),
        "i16"  => ("deser_int",   "i16",  Some(-32768),      Some(32767)),
        "i32"  => ("deser_int",   "i32",  Some(-2147483648), Some(2147483647)),
        "i64"  => ("deser_int",   "i64",  None,              None),
        "u8"   => ("deser_uint",  "u8",   None,              Some(255)),
        "u16"  => ("deser_uint",  "u16",  None,              Some(65535)),
        "u32"  => ("deser_uint",  "u32",  None,              Some(4294967295)),
        "uint" => ("deser_uint",  "uint", None,              None),
        "f32"  => ("deser_float", "f32",  None,              None),
        _ => return None,
    };
    Some(NarrowScalarPlan { read, cast, min, max })
}

/// `Err(DeError.new(OutOfRange(str.from(<raw>))))` — the typed range-violation
/// error raised inline by a narrow-scalar deserialize guard.
fn deerror_out_of_range(raw: &str) -> Expr {
    let str_from = member_call(ident("str"), "from", vec![ident(raw)]);
    let variant = call(ident("OutOfRange"), vec![str_from]);
    let de = call(ex(ExprKind::Path(vec!["DeError".to_string(), "new".to_string()])), vec![variant]);
    call(ident("Err"), vec![de])
}

/// Emit the inline statements for a narrow-scalar field into `stmts`, binding
/// the narrowed value to local `field` read from sub-deserializer `sub`:
///   `ro __raw = sub.deser_X()?`
///   `if __raw < min || __raw > max { return Err(DeError.new(OutOfRange(...))) }`
///   `ro field = __raw as T`
fn emit_narrow_scalar_deser(stmts: &mut Vec<Stmt>, field: &str, sub: &str, plan: &NarrowScalarPlan) {
    let raw = format!("__nv_raw_{}", field);
    stmts.push(let_stmt(&raw, false, None, try_(member_call(ident(sub), plan.read, vec![]))));
    let mut cond: Option<Expr> = None;
    if let Some(min) = plan.min {
        cond = Some(binop(BinOp::Lt, ident(&raw), int_lit(min)));
    }
    if let Some(max) = plan.max {
        let hi = binop(BinOp::Gt, ident(&raw), int_lit(max));
        cond = Some(match cond {
            Some(prev) => binop(BinOp::Or, prev, hi),
            None => hi,
        });
    }
    if let Some(c) = cond {
        let ret = Stmt::Return { value: Some(deerror_out_of_range(&raw)), span: span_dummy() };
        stmts.push(Stmt::Expr(ex(ExprKind::If {
            cond: Box::new(c),
            then: Block { stmts: vec![ret], trailing: None, span: span_dummy(), is_unsafe: false },
            else_: None,
        })));
    }
    stmts.push(let_stmt(
        field, false, None,
        ex(ExprKind::As(Box::new(ident(&raw)), type_ref_named(plan.cast))),
    ));
}

/// Scalar field type → `(Serializer method, optional widening cast target)` for
/// the DIRECT `s.serialize_X(@f [as WIDE])` emission. A primitive receiver does
/// not dispatch a user `@serialize`, so the record synthesizer pushes the wire
/// call itself. Signed ints widen to `int`, unsigned to `u64`, `f32`→`f64`.
fn scalar_ser_wire(ty: &TypeRef) -> Option<(&'static str, Option<&'static str>)> {
    match type_ref_name(ty)? {
        "int"                  => Some(("serialize_int", None)),
        "i8" | "i16" | "i32" | "i64" => Some(("serialize_int", Some("int"))),
        "u64"                  => Some(("serialize_uint", None)),
        "uint" | "u8" | "u16" | "u32" => Some(("serialize_uint", Some("u64"))),
        "f64"                  => Some(("serialize_float", None)),
        "f32"                  => Some(("serialize_float", Some("f64"))),
        "bool"                 => Some(("serialize_bool", None)),
        "str"                  => Some(("serialize_str", None)),
        _ => None,
    }
}

fn is_option_ty(ty: &TypeRef) -> bool { type_ref_name(ty) == Some("Option") }

/// Scalar primitives serde supports as a DIRECT record field: the record
/// synthesizer emits their wire ser/deser INLINE (`s.serialize_int(@f as int)`
/// + inline range-guard), so no primitive-method dispatch is needed. Everything
/// routes through the JSON numeric / string / bool wire with exact-integer +
/// range checks (D342/D346). NOT supported (→ typed
/// `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`, never ICE):
///   - `char`  — no faithful JSON scalar (followup [M-180-char-serde]);
///   - `byte`  — retired alias of `u8` (D367); use `u8`;
///   - `i128`/`u128` — exceed the numeric wire's int/u64 carrier and are not
///     even lowered to a C type.
pub fn serde_supported_scalar(name: &str) -> bool {
    matches!(
        name,
        "int" | "uint" | "i8" | "i16" | "i32" | "i64"
            | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool" | "str"
    )
}

/// Scalar primitives serde supports NESTED inside a container (`Option[T]`,
/// `Vec[T]`, `HashMap[str,T]`, tuple): only those with real `fn T @serialize` +
/// `fn T.deserialize` conformance in serde.nv, because a container's generic
/// body dispatches `v.serialize(s)` / `T.deserialize(sub)` on the element.
/// Narrow scalars (i8..i64/u8..u32/uint/f32) do NOT dispatch as primitive
/// methods inside a generic mono, so they are top-level-only — nesting one
/// yields a typed `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL` (followup
/// [M-180-container-narrow-scalar]) rather than a CC-FAIL/ICE.
fn serde_container_scalar(name: &str) -> bool {
    matches!(name, "int" | "u64" | "f64" | "bool" | "str")
}

/// `[]u8` / `Vec[u8]` / `[N]u8` — the byte-sequence field shape that maps to a
/// base64 string on the wire (`Serializer::serialize_bytes`, Q9 / D-canon),
/// NOT to a JSON array of numbers.
fn is_byte_seq_ty(ty: &TypeRef) -> bool {
    match ty.strip_modifiers() {
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
            type_ref_name(inner) == Some("u8")
        }
        TypeRef::Named { path, generics, .. }
            if path.last().map(|s| s.as_str()) == Some("Vec") && generics.len() == 1 =>
        {
            type_ref_name(&generics[0]) == Some("u8")
        }
        _ => false,
    }
}

/// Inner type of `Option[T]`.
fn option_inner(ty: &TypeRef) -> Option<TypeRef> {
    match ty.strip_modifiers() {
        TypeRef::Named { path, generics, .. }
            if path.last().map(|s| s.as_str()) == Some("Option") && generics.len() == 1 =>
            Some(generics[0].clone()),
        _ => None,
    }
}

/// Build the type-expression used as the static receiver of `<T>.deserialize`.
/// `Named{path, generics}` → `path` (+ turbofish); `[]T` → `Vec[T]`.
fn type_static_expr(ty: &TypeRef) -> Expr {
    match ty.strip_modifiers() {
        TypeRef::Named { path, generics, .. } => {
            let base = if path.len() == 1 {
                ident(&path[0])
            } else {
                ex(ExprKind::Path(path.clone()))
            };
            if generics.is_empty() {
                base
            } else {
                ex(ExprKind::TurboFish { base: Box::new(base), type_args: generics.clone() })
            }
        }
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
            ex(ExprKind::TurboFish {
                base: Box::new(ident("Vec")),
                type_args: vec![(**inner).clone()],
            })
        }
        _ => ident("__nv_unsupported"),
    }
}

/// Deserialize expression for a field of type `ty` from sub-deserializer `sub`
/// (already `?`-wrapped): `Option[T]` → inline null-check on the SAME cursor
/// (`if sub.is_null()? { None } else { Some(<T deser>) }`); `[]u8`/`Vec[u8]` →
/// `sub.deser_bytes()?` (base64 wire, Q9); scalar → `sub.deser_X()?`; else
/// static `<T>.deserialize(sub)?`.
///
/// The `Option` arm is RECURSIVE so nested `Option[Option[T]]` works: the
/// built-in `Option` does not dispatch a user static `.deserialize`
/// (`Option[int].deserialize` → bad `Option->deserialize` C), so the inline
/// null-check is the only sound form. `Some(None)` collapses to `null` on the
/// wire (D342), making this the faithful inverse of `@serialize`.
fn deser_field_expr(ty: &TypeRef, sub: &str, file_id: crate::diag::FileId) -> Expr {
    if let Some(inner) = option_inner(ty) {
        let inner_de = deser_field_expr(&inner, sub, file_id);
        try_wrap_none(member_call(ident(sub), "is_null", vec![]), inner_de, ty)
    } else if is_byte_seq_ty(ty) {
        try_(member_call(ident(sub), "deser_bytes", vec![]))
    } else if let Some(m) = scalar_deser_method(ty) {
        try_(member_call(ident(sub), m, vec![]))
    } else {
        // Static `<Type>.deserialize(sub)`. For a SIMPLE named type the parser
        // emits `Path([Type, "deserialize"])` (a static call); the `Member{
        // Ident(Type), …}` form we'd otherwise build dispatches as an INSTANCE
        // method (`Nova_T_method_…` — wrong). Generic/array receivers keep the
        // `Member{TurboFish{…}, …}` shape (matching the parser for `Vec[str]…`).
        // Plan 221.1 №33: tagged with the HOST type-decl's `file_id` (not
        // `span_dummy()`'s `MAIN_FILE_ID`) — see `member_call_at` doc; a
        // container's static `.deserialize` is exactly as extension-policy-
        // sensitive as its instance `.serialize` (`ser_value_expr`).
        let func = match ty.strip_modifiers() {
            TypeRef::Named { path, generics, .. } if generics.is_empty() => {
                let mut p = path.clone();
                p.push("deserialize".to_string());
                ex_at(ExprKind::Path(p), file_id)
            }
            _ => ex_at(ExprKind::Member {
                obj: Box::new(type_static_expr(ty)),
                name: "deserialize".to_string(),
            }, file_id),
        };
        try_(call_at(func, vec![ident(sub)], file_id))
    }
}

/// Serde-aware field eligibility: primitives, `Option`/`Vec`/`HashMap[str,_]`
/// (recurse into element types), `[]T`/tuple (recurse), types that provide the
/// method or declare `#impl(P)`. Q16: `HashMap` key must be `str`.
///
/// The `top_level` distinction is load-bearing (§6, no latent ICE): a NARROW
/// scalar (i8..i64/u8..u32/uint/f32) is emitted INLINE by the synthesizer as a
/// direct field, but a container's generic body dispatches `v.serialize(s)` /
/// `T.deserialize(sub)` on the element and a narrow primitive does NOT dispatch
/// as a method inside a mono → nesting one must be a TYPED diagnostic, not a
/// CC-FAIL. `[]u8`/`Vec[u8]` are the byte-seq exception (base64, top-level).
pub fn check_field_eligibility_serde<Q: DeriveQuery>(
    query: &Q,
    field_type: &TypeRef,
    protocol: &str,
    method_name: &str,
) -> bool {
    check_field_eligibility_serde_at(query, field_type, protocol, method_name, true)
}

fn check_field_eligibility_serde_at<Q: DeriveQuery>(
    query: &Q,
    field_type: &TypeRef,
    protocol: &str,
    method_name: &str,
    top_level: bool,
) -> bool {
    // `[]u8` / `Vec[u8]` → base64 bytes wire (Q9) — top-level only: a nested
    // byte-seq (e.g. `Option[[]u8]`) has no per-element wire in a container mono.
    if top_level && is_byte_seq_ty(field_type) {
        return true;
    }
    match field_type.strip_modifiers() {
        TypeRef::Named { path, generics, .. } => {
            let name = match path.last() { Some(n) => n.as_str(), None => return false };
            if is_primitive_type(name) {
                // char / byte / i128 / u128 have no faithful JSON wire → typed
                // E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL (never an ICE, §6). Narrow
                // scalars are direct-field only (see fn doc / serde_container_scalar).
                return if top_level {
                    serde_supported_scalar(name)
                } else {
                    serde_container_scalar(name)
                };
            }
            match name {
                "Option" | "Vec" | "Set" | "HashSet" => generics.iter().all(|g| {
                    check_field_eligibility_serde_at(query, g, protocol, method_name, false)
                }),
                "HashMap" | "Map" => {
                    // Q16: only `str` keys are serializable (E_SERDE_NONSTRING_MAP_KEY).
                    if generics.len() == 2
                        && type_ref_name(&generics[0]) == Some("str")
                    {
                        check_field_eligibility_serde_at(query, &generics[1], protocol, method_name, false)
                    } else {
                        false
                    }
                }
                _ => {
                    if query.type_provides_method(name, method_name) {
                        return true;
                    }
                    if let Some(td) = query.lookup_type(name) {
                        if td.impl_protocols.iter().any(|p| p == protocol) {
                            return true;
                        }
                    }
                    false
                }
            }
        }
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) =>
            check_field_eligibility_serde_at(query, inner, protocol, method_name, false),
        TypeRef::Tuple(elems, _) => elems.iter()
            .all(|t| check_field_eligibility_serde_at(query, t, protocol, method_name, false)),
        TypeRef::Unit(_) => true,
        _ => false,
    }
}

/// Build a serde FnDecl shell (generic `[G Bound]` method, `mut param G`,
/// static or instance receiver, `Result[..]` return, `compiler_generated`).
fn make_serde_method(
    type_name: &str,
    method_name: &str,
    is_static: bool,
    generic_name: &str,
    bound_name: &str,
    param_name: &str,
    return_type: TypeRef,
    body: FnBody,
    file_id: crate::diag::FileId,
) -> FnDecl {
    // Plan 180.1 Ф.1.5: the FnDecl's OWN span (not each inner expression's) is
    // what the identifier-resolution checker uses to pick `file_id` for
    // walking the WHOLE body (`fd.span.file_id`, one value per function — a
    // sound assumption for ordinary code, where a function's body always
    // lives in the same file as its declaration). A synthesized method's
    // `span_dummy()` (`MAIN_FILE_ID`) breaks that assumption whenever the type
    // being synthesized for was pulled into the CU from a DIFFERENT module
    // than the entry (the common case: DTOs are declared once, decoded
    // elsewhere) — free-function references inside the body (`#serde(default
    // = "fn")`) would resolve against the ENTRY's own scope instead of the
    // type's declaring module's. Tagging the FnDecl's span with the type's
    // OWN `file_id` fixes the lookup unconditionally (harmless — a dummy
    // start/end never renders in a diagnostic; only `file_id` is load-bearing
    // here, ever consulted by name-resolution, not by the module's own path
    // lookup for the diagnostic's OWN pretty-printer — that resolves the
    // path from `file_id` via a separate table populated at parse time, which
    // already has an entry for `file_id`).
    let fn_span = Span::with_file(0, 0, file_id);
    FnDecl {
        name: method_name.to_string(),
        receiver: Some(Receiver {
            type_name: type_name.to_string(),
            generics: vec![],
            carrier_bounds: vec![],
            receiver_ty: None,
            kind: if is_static { ReceiverKind::Static } else { ReceiverKind::Instance },
            mutable: false,
            consume: false,
            span: fn_span,
        }),
        generics: vec![GenericParam {
            name: generic_name.to_string(),
            bounds: vec![type_ref_named(bound_name)],
            default: None,
            span: fn_span,
            consume_bound: false,
        }],
        params: vec![Param {
            name: param_name.to_string(),
            ty: type_ref_named(generic_name),
            span: fn_span,
            is_variadic: false,
            default: None,
            consume: false,
            is_mut: true,
            is_const: false,
            fiber_safe_attr: false,
        }],
        effects: vec![],
        return_type: Some(return_type),
        return_is_const: false,
        returns_receiver: false,
        body,
        span: fn_span,
        is_export: false,
        is_external: false,
        compiler_generated: true,
        ..FnDecl::default()
    }
}

/// Serialize a value-expression `val` of type `ty` into serializer `s` — the
/// wire-push call (NOT yet `?`-wrapped). Same decision tree as the record-field
/// path: `[]u8`→bytes, direct scalar wire (primitive receiver doesn't dispatch
/// `@serialize`), else `val.serialize(s)`.
fn ser_value_expr(val: Expr, ty: &TypeRef, file_id: crate::diag::FileId) -> Expr {
    if is_byte_seq_ty(ty) {
        member_call(ident("s"), "serialize_bytes", vec![val])
    } else if let Some((method, widen)) = scalar_ser_wire(ty) {
        let arg = match widen {
            Some(w) => ex(ExprKind::As(Box::new(val), type_ref_named(w))),
            None => val,
        };
        member_call(ident("s"), method, vec![arg])
    } else {
        // Plan 221.1 №33 (`[M-autoderive-extension-import-seed-combined-cu]`):
        // `val.serialize(s)` dispatches to the VALUE's own type — a
        // container (`Vec`/`HashMap`/`Option`) or a nested record — whose
        // `@serialize` may live in a DIFFERENT module than either `val`'s
        // type or the HOST type being synthesized for (e.g. `HashMap`'s is
        // in `std.encoding.serde`, not `collections.hashmap`). `member_call`'s
        // `span_dummy()` always carries `MAIN_FILE_ID`, mis-attributing this
        // call to whatever file the compiler was invoked on as entry instead
        // of the host type's OWN declaring file — see `member_call_at` doc.
        member_call_at(val, "serialize", vec![ident("s")], file_id)
    }
}

/// Serialize a `struct_field(key)? + <ser value>?` pair into `stmts`.
fn ser_struct_field_stmt(stmts: &mut Vec<Stmt>, key: &str, bind: &str, ty: &TypeRef, file_id: crate::diag::FileId) {
    stmts.push(Stmt::Expr(try_(member_call(ident("s"), "struct_field", vec![str_lit(key)]))));
    stmts.push(Stmt::Expr(try_(ser_value_expr(ident(bind), ty, file_id))));
}

/// Emit the statements that serialize a variant's PAYLOAD (without any tag
/// wrapper), in the current container position of `s`:
///   single `V(x)`   → `<ser x>?`
///   tuple  `V(a,b)` → `begin_seq(N)?; <ser a>?; <ser b>?; end_seq()?`
///   record `V{f,g}` → `begin_struct(V,N)?; struct_field("f")?; <ser f>?; …; end_struct()?`
/// Unit variants have no payload → empty.
fn ser_variant_payload_stmts(v: &SumVariant, binds: &[(String, TypeRef)], file_id: crate::diag::FileId) -> Vec<Stmt> {
    let mut stmts = Vec::new();
    match &v.kind {
        SumVariantKind::Unit => {}
        SumVariantKind::Tuple(tys) if tys.len() == 1 => {
            let (bind, ty) = &binds[0];
            stmts.push(Stmt::Expr(try_(ser_value_expr(ident(bind), ty, file_id))));
        }
        SumVariantKind::Tuple(_) => {
            stmts.push(Stmt::Expr(try_(member_call(ident("s"), "begin_seq",
                vec![int_lit(binds.len() as i64)]))));
            for (bind, ty) in binds {
                stmts.push(Stmt::Expr(try_(ser_value_expr(ident(bind), ty, file_id))));
            }
            stmts.push(Stmt::Expr(try_(member_call(ident("s"), "end_seq", vec![]))));
        }
        SumVariantKind::Record(fields) => {
            stmts.push(Stmt::Expr(try_(member_call(ident("s"), "begin_struct",
                vec![str_lit(&v.name), int_lit(fields.len() as i64)]))));
            for ((bind, ty), f) in binds.iter().zip(fields) {
                ser_struct_field_stmt(&mut stmts, &f.name, bind, ty, file_id);
            }
            stmts.push(Stmt::Expr(try_(member_call(ident("s"), "end_struct", vec![]))));
        }
    }
    stmts
}

/// Plan 180 Ф.2-sum (D345) / Ф.6 (D382): sum `@serialize` per tagging mode.
///   External (default): unit→`"V"`; single→`{"V":x}`; tuple→`{"V":[..]}`;
///                       record→`{"V":{f}}`.
///   Internal `tag=k`:   unit→`{"k":"V"}`; record→`{"k":"V","f":x,…}` (fields
///                       inlined; tuple rejected at validation).
///   Adjacent `tag=t,content=c`: unit→`{"t":"V"}`; else→`{"t":"V","c":payload}`.
///   Untagged:           unit→`null`; single→`x`; tuple→`[..]`; record→`{f}`.
/// Emitted as `match @ { <arm per variant> }`, each arm `Result[(), SerError]`.
fn synth_serialize_sum_body(type_name: &str, variants: &[SumVariant], mode: &SerdeTagging, file_id: crate::diag::FileId) -> FnBody {
    let arms: Vec<MatchArm> = variants
        .iter()
        .map(|v| {
            let (pat, binds) = variant_bind_pattern(v, "__nv_s_");
            let is_unit = matches!(v.kind, SumVariantKind::Unit);
            match mode {
                SerdeTagging::External => {
                    if is_unit {
                        return match_arm_expr(pat, member_call(
                            ident("s"), "serialize_str", vec![str_lit(&v.name)]));
                    }
                    let mut stmts = vec![
                        Stmt::Expr(try_(member_call(ident("s"), "begin_struct",
                            vec![str_lit(type_name), int_lit(1)]))),
                        Stmt::Expr(try_(member_call(ident("s"), "struct_field",
                            vec![str_lit(&v.name)]))),
                    ];
                    stmts.extend(ser_variant_payload_stmts(v, &binds, file_id));
                    match_arm_block(pat, block_with_trailing(
                        stmts, member_call(ident("s"), "end_struct", vec![])))
                }
                SerdeTagging::Internal { tag } => {
                    // Tag field + (for record variants) inlined fields in ONE object.
                    let n = 1 + match &v.kind {
                        SumVariantKind::Record(fs) => fs.len() as i64,
                        _ => 0,
                    };
                    let mut stmts = vec![
                        Stmt::Expr(try_(member_call(ident("s"), "begin_struct",
                            vec![str_lit(type_name), int_lit(n)]))),
                        Stmt::Expr(try_(member_call(ident("s"), "struct_field", vec![str_lit(tag)]))),
                        Stmt::Expr(try_(member_call(ident("s"), "serialize_str", vec![str_lit(&v.name)]))),
                    ];
                    if let SumVariantKind::Record(fields) = &v.kind {
                        for ((bind, ty), f) in binds.iter().zip(fields) {
                            ser_struct_field_stmt(&mut stmts, &f.name, bind, ty, file_id);
                        }
                    }
                    match_arm_block(pat, block_with_trailing(
                        stmts, member_call(ident("s"), "end_struct", vec![])))
                }
                SerdeTagging::Adjacent { tag, content } => {
                    let n = if is_unit { 1 } else { 2 };
                    let mut stmts = vec![
                        Stmt::Expr(try_(member_call(ident("s"), "begin_struct",
                            vec![str_lit(type_name), int_lit(n)]))),
                        Stmt::Expr(try_(member_call(ident("s"), "struct_field", vec![str_lit(tag)]))),
                        Stmt::Expr(try_(member_call(ident("s"), "serialize_str", vec![str_lit(&v.name)]))),
                    ];
                    if !is_unit {
                        stmts.push(Stmt::Expr(try_(member_call(ident("s"), "struct_field",
                            vec![str_lit(content)]))));
                        stmts.extend(ser_variant_payload_stmts(v, &binds, file_id));
                    }
                    match_arm_block(pat, block_with_trailing(
                        stmts, member_call(ident("s"), "end_struct", vec![])))
                }
                SerdeTagging::Untagged => {
                    // Payload emitted directly; unit → null.
                    if is_unit {
                        return match_arm_expr(pat, member_call(
                            ident("s"), "serialize_unit", vec![]));
                    }
                    if let SumVariantKind::Tuple(tys) = &v.kind {
                        if tys.len() == 1 {
                            let (bind, ty) = &binds[0];
                            return match_arm_expr(pat, ser_value_expr(ident(bind), ty, file_id));
                        }
                    }
                    // tuple(multi) → seq; record → struct. Both end via trailing.
                    let (stmts, closer): (Vec<Stmt>, &str) = match &v.kind {
                        SumVariantKind::Record(fields) => {
                            let mut s = vec![Stmt::Expr(try_(member_call(ident("s"),
                                "begin_struct", vec![str_lit(&v.name), int_lit(fields.len() as i64)])))];
                            for ((bind, ty), f) in binds.iter().zip(fields) {
                                ser_struct_field_stmt(&mut s, &f.name, bind, ty, file_id);
                            }
                            (s, "end_struct")
                        }
                        _ => {
                            let mut s = vec![Stmt::Expr(try_(member_call(ident("s"),
                                "begin_seq", vec![int_lit(binds.len() as i64)])))];
                            for (bind, ty) in &binds {
                                s.push(Stmt::Expr(try_(ser_value_expr(ident(bind), ty, file_id))));
                            }
                            (s, "end_seq")
                        }
                    };
                    match_arm_block(pat, block_with_trailing(
                        stmts, member_call(ident("s"), closer, vec![])))
                }
            }
        })
        .collect();
    FnBody::Block(block_with_trailing(vec![], ex_match(ex(ExprKind::SelfAccess), arms)))
}

/// Read a value of type `ty` from a sub-deserializer local `cursor` (already
/// positioned AT the value) into local `bind`. Mirrors the record-field read
/// decision tree (narrow-scalar inline / Option null-check / scalar / static).
fn emit_payload_read(stmts: &mut Vec<Stmt>, bind: &str, ty: &TypeRef, cursor: &str, file_id: crate::diag::FileId) {
    if let Some(plan) = narrow_scalar_deser_plan(ty) {
        emit_narrow_scalar_deser(stmts, bind, cursor, &plan);
    } else {
        let ann = if is_option_ty(ty) { Some(ty.clone()) } else { None };
        stmts.push(let_stmt(bind, false, ann, deser_field_expr(ty, cursor, file_id)));
    }
}

/// Read record-variant field `f_name: f_ty` from object-cursor `cursor` into a
/// local named `f_name` (RecordLit shorthand). Mirrors `synthesize_deserialize`
/// per-field emission but rooted at `cursor` instead of `d`.
fn emit_record_variant_field(stmts: &mut Vec<Stmt>, cursor: &str, f_name: &str, f_ty: &TypeRef, file_id: crate::diag::FileId) {
    let sub = format!("__nv_rf_{}", f_name);
    if let Some(plan) = narrow_scalar_deser_plan(f_ty) {
        stmts.push(let_stmt(&sub, true, None, try_(member_call(
            ident(cursor), "enter_field", vec![str_lit(f_name)]))));
        emit_narrow_scalar_deser(stmts, f_name, &sub, &plan);
    } else {
        let is_opt = is_option_ty(f_ty);
        let enter = if is_opt { "enter_field_or_null" } else { "enter_field" };
        stmts.push(let_stmt(&sub, true, None, try_(member_call(
            ident(cursor), enter, vec![str_lit(f_name)]))));
        let ann = if is_opt { Some(f_ty.clone()) } else { None };
        stmts.push(let_stmt(f_name, false, ann, deser_field_expr(f_ty, &sub, file_id)));
    }
}

/// `Err(DeError.at(UnknownVariant { name: <tag>, expected: [<names>] }, "$"))`.
fn deerror_unknown_variant(tag_local: &str, variant_names: &[String]) -> Expr {
    let expected = ex(ExprKind::ArrayLit(
        variant_names.iter().map(|n| crate::ast::ArrayElem::Item(str_lit(n))).collect(),
    ));
    let uv = ex(ExprKind::RecordLit {
        type_name: Some(vec!["UnknownVariant".to_string()]),
        fields: vec![
            RecordLitField { name: "name".to_string(), value: Some(ident(tag_local)),
                is_spread: false, at_shorthand: false, span: span_dummy() },
            RecordLitField { name: "expected".to_string(), value: Some(expected),
                is_spread: false, at_shorthand: false, span: span_dummy() },
        ],
        inferred_map_v: None,
        inferred_target_type: None,
    });
    let de = ex(ExprKind::Call {
        func: Box::new(ex(ExprKind::Path(vec!["DeError".to_string(), "new".to_string()]))),
        args: vec![CallArg::Item(uv),
                   CallArg::Named { name: "path".to_string(), value: str_lit("$") }],
        trailing: None,
    });
    call(ident("Err"), vec![de])
}

/// Synthesize `@serialize[S Serializer](mut s S) -> Result[(), SerError]`.
/// Record → UNIFORM memberwise push (like `@debug`); sum → externally-tagged.
pub fn synthesize_serialize<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    // Plan 180 Ф.6: validate/compute serde tagging mode (also rejects tagging
    // attrs on a non-sum type: E_SERDE_TAGGING_ON_NON_SUM).
    let mode = serde_tagging_mode(type_decl)?;
    let body = if let Some(fields) = iter_fields(type_decl) {
        // Plan 180.1 Ф.1/Ф.10: resolve rename/rename_all/skip/skip_serializing_if
        // + validate the wire contract (collisions, flatten-gate) once.
        let (_type_opts, resolved) = resolve_fields(type_decl, &fields)?;
        let active_len = resolved.iter().filter(|rf| !rf.opts.skip).count();
        let mut stmts: Vec<Stmt> = Vec::new();
        // s.begin_struct("Type", N)? — N excludes `skip` fields (decorative
        // count only; `skip_serializing_if`'s runtime-conditional omission is
        // NOT reflected — no backend currently uses `len` for correctness).
        stmts.push(Stmt::Expr(try_(member_call(
            ident("s"), "begin_struct",
            vec![str_lit(&type_decl.name), int_lit(active_len as i64)],
        ))));
        for rf in &resolved {
            if rf.opts.skip { continue; }
            let field_stmts = vec![
                Stmt::Expr(try_(member_call(ident("s"), "struct_field", vec![str_lit(&rf.wire)]))),
                Stmt::Expr(try_(ser_value_expr(self_field(&rf.field.name), &rf.field.ty, type_decl.span.file_id))),
            ];
            match &rf.opts.skip_serializing_if {
                Some(pred) => {
                    // Plan 180.1 Ф.1.4: general predicate form — `if
                    // !(@field.<predicate>()) { struct_field(...)?; <ser>?; }`.
                    let cond = not_expr(member_call(self_field(&rf.field.name), pred, vec![]));
                    stmts.push(Stmt::Expr(ex(ExprKind::If {
                        cond: Box::new(cond),
                        then: Block { stmts: field_stmts, trailing: None, span: span_dummy(), is_unsafe: false },
                        else_: None,
                    })));
                }
                None => stmts.extend(field_stmts),
            }
        }
        FnBody::Block(block_with_trailing(
            stmts, member_call(ident("s"), "end_struct", vec![])))
    } else if let Some(variants) = iter_sum_variants(type_decl) {
        synth_serialize_sum_body(&type_decl.name, variants, &mode, type_decl.span.file_id)
    } else {
        return Err(DeriveError::UnsupportedTypeKind {
            type_name: type_decl.name.clone(),
            kind: type_decl_kind_name(type_decl).to_string(),
            protocol: SERIALIZE.to_string(),
        });
    };
    Ok(make_serde_method(
        &type_decl.name, "serialize", false, "S", "Serializer", "s",
        result_ty(TypeRef::Unit(span_dummy()), "SerError"), body, type_decl.span.file_id))
}

/// `Variant` / `Variant(p0, p1)` / `Variant { f, g }` reconstruction expression
/// (payload locals named `__nv_p{i}` for tuples, field-name locals for records).
/// NOT wrapped in `Ok(...)`.
fn variant_ctor_expr(v: &SumVariant) -> Expr {
    match &v.kind {
        SumVariantKind::Unit => ident(&v.name),
        SumVariantKind::Tuple(tys) => call(
            ident(&v.name),
            (0..tys.len()).map(|i| ident(&format!("__nv_p{}", i))).collect(),
        ),
        SumVariantKind::Record(fields) => ex(ExprKind::RecordLit {
            type_name: Some(vec![v.name.clone()]),
            fields: fields.iter().map(|f| RecordLitField {
                name: f.name.clone(), value: None, is_spread: false,
                at_shorthand: false, span: span_dummy(),
            }).collect(),
            inferred_map_v: None,
            inferred_target_type: None,
        }),
    }
}

/// Read a variant payload from the cursor `sub` yielded by `sub_source` (a
/// `?`-wrapped `Result[Deserializer, DeError]` expression). Used by the
/// externally-tagged and adjacently-tagged paths (the payload lives under one
/// key/index). Trailing = `Ok(<ctor>)`. For a Unit variant `sub_source` is
/// unused and the arm is just `Ok(V)`.
fn build_payload_arm(v: &SumVariant, sub_source: Expr, file_id: crate::diag::FileId) -> Block {
    let mut astmts: Vec<Stmt> = Vec::new();
    match &v.kind {
        SumVariantKind::Unit => {}
        SumVariantKind::Tuple(tys) if tys.len() == 1 => {
            astmts.push(let_stmt("__nv_sub", true, None, sub_source));
            emit_payload_read(&mut astmts, "__nv_p0", &tys[0], "__nv_sub", file_id);
        }
        SumVariantKind::Tuple(tys) => {
            astmts.push(let_stmt("__nv_sub", true, None, sub_source));
            for (i, ty) in tys.iter().enumerate() {
                let e = format!("__nv_e{}", i);
                astmts.push(let_stmt(&e, true, None, try_(member_call(
                    ident("__nv_sub"), "enter_index", vec![int_lit(i as i64)]))));
                emit_payload_read(&mut astmts, &format!("__nv_p{}", i), ty, &e, file_id);
            }
        }
        SumVariantKind::Record(fields) => {
            astmts.push(let_stmt("__nv_sub", true, None, sub_source));
            for f in fields {
                emit_record_variant_field(&mut astmts, "__nv_sub", &f.name, &f.ty, file_id);
            }
        }
    }
    Block {
        stmts: astmts,
        trailing: Some(Box::new(call(ident("Ok"), vec![variant_ctor_expr(v)]))),
        span: span_dummy(),
        is_unsafe: false,
    }
}

/// Internally-tagged arm: the variant's record fields are inlined into the SAME
/// object as the tag, so they are read from `d` directly (not a sub-cursor).
/// Only Unit and Record variants reach here (Tuple rejected at validation).
fn build_internal_arm(v: &SumVariant, file_id: crate::diag::FileId) -> Block {
    let mut astmts: Vec<Stmt> = Vec::new();
    if let SumVariantKind::Record(fields) = &v.kind {
        for f in fields {
            emit_record_variant_field(&mut astmts, "d", &f.name, &f.ty, file_id);
        }
    }
    Block {
        stmts: astmts,
        trailing: Some(Box::new(call(ident("Ok"), vec![variant_ctor_expr(v)]))),
        span: span_dummy(),
        is_unsafe: false,
    }
}

/// Fold variants into an `if __nv_tag == "V0" { … } else if … { … } else {
/// Err(UnknownVariant) }` chain over the tag string `tag_local`.
fn fold_tag_dispatch(variants: &[SumVariant], tag_local: &str, build_arm: &dyn Fn(&SumVariant) -> Block) -> Expr {
    let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    let mut chain: crate::ast::ElseBranch = crate::ast::ElseBranch::Block(Block {
        stmts: vec![],
        trailing: Some(Box::new(deerror_unknown_variant(tag_local, &variant_names))),
        span: span_dummy(),
        is_unsafe: false,
    });
    for v in variants.iter().rev() {
        let cond = binop(BinOp::Eq, ident(tag_local), str_lit(&v.name));
        let if_expr = ex(ExprKind::If {
            cond: Box::new(cond),
            then: build_arm(v),
            else_: Some(chain),
        });
        chain = crate::ast::ElseBranch::If(Box::new(if_expr));
    }
    match chain {
        crate::ast::ElseBranch::If(e) => *e,
        // Sum with zero variants — unreachable in practice; emit UnknownVariant.
        crate::ast::ElseBranch::Block(b) => ex(ExprKind::If {
            cond: Box::new(ex(ExprKind::BoolLit(false))),
            then: Block { stmts: vec![], trailing: None, span: span_dummy(), is_unsafe: false },
            else_: Some(crate::ast::ElseBranch::Block(b)),
        }),
    }
}

/// Statements that read the discriminator string of an internally/adjacently
/// tagged object into local `__nv_tag`:
///   `mut __nv_tsub = d.enter_field(<tag_field>)?`
///   `ro  __nv_tag  = __nv_tsub.deser_str()?`
fn read_tag_field_stmts(tag_field: &str) -> Vec<Stmt> {
    vec![
        let_stmt("__nv_tsub", true, None,
            try_(member_call(ident("d"), "enter_field", vec![str_lit(tag_field)]))),
        let_stmt("__nv_tag", false, Some(type_ref_named("str")),
            try_(member_call(ident("__nv_tsub"), "deser_str", vec![]))),
    ]
}

/// `Ok(<inner>)` / `Err(<inner>)` variant-tuple pattern for Result-threading.
fn result_pat(ctor: &str, bind: &str, mutable: bool) -> Pattern {
    Pattern::Variant {
        path: vec![ctor.to_string()],
        kind: VariantPatternKind::Tuple {
            patterns: vec![Pattern::Ident { name: bind.to_string(), span: span_dummy(), is_mut: mutable, is_consume: false }],
            rest: false,
        },
        span: span_dummy(),
    }
}

/// Thread a `Result`-valued step without `?`: `match <result_expr> { Ok(<bind>)
/// => <cont>, Err(__nv_thr) => Err(__nv_thr) }`. Used by the untagged try-each
/// path where an error must NOT propagate out of the whole deserialize (it
/// means "this variant did not match — try the next").
fn thread_result(bind: &str, mutable: bool, result_expr: Expr, cont: Expr) -> Expr {
    ex_match(result_expr, vec![
        match_arm_expr(result_pat("Ok", bind, mutable), cont),
        match_arm_expr(result_pat("Err", "__nv_thr", false),
            call(ident("Err"), vec![ident("__nv_thr")])),
    ])
}

/// `Err(DeError.new(Other(<msg>)))` — an internal "attempt failed" error for the
/// untagged try-each path (always discarded; the outer fold replaces it with
/// `NoVariantMatched`).
fn de_attempt_fail(msg: &str) -> Expr {
    let de = call(ex(ExprKind::Path(vec!["DeError".to_string(), "new".to_string()])),
        vec![call(ident("Other"), vec![str_lit(msg)])]);
    call(ident("Err"), vec![de])
}

/// Read a value of type `ty` from cursor `cur` as a RAW `Result[ty, DeError]`
/// expression (no `?` — for the untagged try-each path). Mirrors
/// `deser_field_expr`/`emit_narrow_scalar_deser` but value-threaded.
fn raw_read(ty: &TypeRef, cur: &str, file_id: crate::diag::FileId) -> Expr {
    if let Some(inner) = option_inner(ty) {
        // if cur.is_null()? { Ok(None as Option[T]) } else { <thread inner→Some> }
        let some_branch = thread_result("__nv_ox", false, raw_read(&inner, cur, file_id),
            call(ident("Ok"), vec![some_call(ident("__nv_ox"))]));
        let typed_none = ex(ExprKind::As(Box::new(ident("None")), ty.clone()));
        ex(ExprKind::If {
            cond: Box::new(try_(member_call(ident(cur), "is_null", vec![]))),
            then: block_trailing(call(ident("Ok"), vec![typed_none])),
            else_: Some(crate::ast::ElseBranch::Block(block_trailing(some_branch))),
        })
    } else if is_byte_seq_ty(ty) {
        member_call(ident(cur), "deser_bytes", vec![])
    } else if let Some(plan) = narrow_scalar_deser_plan(ty) {
        // match cur.deser_X() { Ok(__raw) => if <oob> { Err(OutOfRange) } else {
        //   Ok(__raw as T) }, Err(__e) => Err(__e) }
        let raw = "__nv_nraw";
        let mut cond: Option<Expr> = None;
        if let Some(min) = plan.min {
            cond = Some(binop(BinOp::Lt, ident(raw), int_lit(min)));
        }
        if let Some(max) = plan.max {
            let hi = binop(BinOp::Gt, ident(raw), int_lit(max));
            cond = Some(match cond { Some(p) => binop(BinOp::Or, p, hi), None => hi });
        }
        let ok_cast = call(ident("Ok"),
            vec![ex(ExprKind::As(Box::new(ident(raw)), type_ref_named(plan.cast)))]);
        let ok_body = match cond {
            Some(c) => ex(ExprKind::If {
                cond: Box::new(c),
                then: block_trailing(deerror_out_of_range(raw)),
                else_: Some(crate::ast::ElseBranch::Block(block_trailing(ok_cast))),
            }),
            None => ok_cast,
        };
        thread_result(raw, false, member_call(ident(cur), plan.read, vec![]), ok_body)
    } else if let Some(m) = scalar_deser_method(ty) {
        member_call(ident(cur), m, vec![])
    } else {
        // Static `<Type>.deserialize(cur)` (already Result). Same receiver form
        // as deser_field_expr's static arm. Plan 221.1 №33: same file_id
        // tagging as deser_field_expr's static arm — see member_call_at doc.
        let func = match ty.strip_modifiers() {
            TypeRef::Named { path, generics, .. } if generics.is_empty() => {
                let mut p = path.clone();
                p.push("deserialize".to_string());
                ex_at(ExprKind::Path(p), file_id)
            }
            _ => ex_at(ExprKind::Member {
                obj: Box::new(type_static_expr(ty)),
                name: "deserialize".to_string(),
            }, file_id),
        };
        call_at(func, vec![ident(cur)], file_id)
    }
}

/// Untagged (Q17) single-variant attempt → `Result[Self, DeError]` expression
/// built by value-threading (no `?`), so a mismatch falls through to the next
/// variant. Payload = whole current value: unit→`null`; single→value; tuple→
/// array; record→object.
fn untagged_attempt(v: &SumVariant, file_id: crate::diag::FileId) -> Expr {
    match &v.kind {
        SumVariantKind::Unit => {
            // if d.is_null()? { Ok(V) } else { Err(...) }
            ex(ExprKind::If {
                cond: Box::new(try_(member_call(ident("d"), "is_null", vec![]))),
                then: block_trailing(call(ident("Ok"), vec![ident(&v.name)])),
                else_: Some(crate::ast::ElseBranch::Block(block_trailing(
                    de_attempt_fail("untagged: expected null for unit variant")))),
            })
        }
        SumVariantKind::Tuple(tys) if tys.len() == 1 => {
            thread_result("__nv_p0", false, raw_read(&tys[0], "d", file_id),
                call(ident("Ok"), vec![variant_ctor_expr(v)]))
        }
        SumVariantKind::Tuple(tys) => {
            // enter_index i, read element i, threaded.
            let mut cont = call(ident("Ok"), vec![variant_ctor_expr(v)]);
            for (i, ty) in tys.iter().enumerate().rev() {
                let e = format!("__nv_e{}", i);
                cont = thread_result(&format!("__nv_p{}", i), false, raw_read(ty, &e, file_id), cont);
                cont = thread_result(&e, true,
                    member_call(ident("d"), "enter_index", vec![int_lit(i as i64)]), cont);
            }
            cont
        }
        SumVariantKind::Record(fields) => {
            let mut cont = call(ident("Ok"), vec![variant_ctor_expr(v)]);
            for f in fields.iter().rev() {
                let sub = format!("__nv_rf_{}", f.name);
                let is_opt = is_option_ty(&f.ty);
                let enter = if is_opt { "enter_field_or_null" } else { "enter_field" };
                // read field value from sub → bind field name
                cont = thread_result(&f.name, false, raw_read(&f.ty, &sub, file_id), cont);
                // enter the field (mut sub)
                cont = thread_result(&sub, true,
                    member_call(ident("d"), enter, vec![str_lit(&f.name)]), cont);
            }
            cont
        }
    }
}

/// Plan 180 Ф.2-sum (D345) / Ф.6 (D382): sum `.deserialize` per tagging mode.
fn synth_deserialize_sum_body(_type_name: &str, variants: &[SumVariant], mode: &SerdeTagging, file_id: crate::diag::FileId) -> FnBody {
    match mode {
        SerdeTagging::External => {
            let mut stmts: Vec<Stmt> = Vec::new();
            // ro __nv_tag = if d.is_str()? { d.deser_str()? } else { <single key> }
            let then_branch = block_trailing(try_(member_call(ident("d"), "deser_str", vec![])));
            let mut else_stmts: Vec<Stmt> = Vec::new();
            else_stmts.push(let_stmt("__nv_keys", false, None,
                try_(member_call(ident("d"), "map_keys", vec![]))));
            let bad = call(ex(ExprKind::Path(vec!["DeError".to_string(), "new".to_string()])),
                vec![call(ident("Syntax"), vec![str_lit(
                    "externally-tagged enum expects a single-key object")])]);
            else_stmts.push(Stmt::Expr(ex(ExprKind::If {
                cond: Box::new(binop(BinOp::Neq,
                    member_call(ident("__nv_keys"), "len", vec![]), int_lit(1))),
                then: Block {
                    stmts: vec![Stmt::Return { value: Some(call(ident("Err"), vec![bad])), span: span_dummy() }],
                    trailing: None, span: span_dummy(), is_unsafe: false,
                },
                else_: None,
            })));
            let key_index = ex(ExprKind::Index {
                obj: Box::new(ident("__nv_keys")),
                index: Box::new(int_lit(0)),
            });
            let else_branch = Block {
                stmts: else_stmts,
                trailing: Some(Box::new(key_index)),
                span: span_dummy(),
                is_unsafe: false,
            };
            let tag_if = ex(ExprKind::If {
                cond: Box::new(try_(member_call(ident("d"), "is_str", vec![]))),
                then: then_branch,
                else_: Some(crate::ast::ElseBranch::Block(else_branch)),
            });
            stmts.push(let_stmt("__nv_tag", false, Some(type_ref_named("str")), tag_if));
            let dispatch = fold_tag_dispatch(variants, "__nv_tag", &|v| {
                build_payload_arm(v, try_(member_call(ident("d"), "enter_key", vec![str_lit(&v.name)])), file_id)
            });
            FnBody::Block(Block { stmts, trailing: Some(Box::new(dispatch)), span: span_dummy(), is_unsafe: false })
        }
        SerdeTagging::Internal { tag } => {
            let stmts = read_tag_field_stmts(tag);
            let dispatch = fold_tag_dispatch(variants, "__nv_tag", &|v| build_internal_arm(v, file_id));
            FnBody::Block(Block { stmts, trailing: Some(Box::new(dispatch)), span: span_dummy(), is_unsafe: false })
        }
        SerdeTagging::Adjacent { tag, content } => {
            let stmts = read_tag_field_stmts(tag);
            let content = content.clone();
            let dispatch = fold_tag_dispatch(variants, "__nv_tag", &move |v| {
                build_payload_arm(v, try_(member_call(ident("d"), "enter_field", vec![str_lit(&content)])), file_id)
            });
            FnBody::Block(Block { stmts, trailing: Some(Box::new(dispatch)), span: span_dummy(), is_unsafe: false })
        }
        SerdeTagging::Untagged => {
            // Try each variant in order; first Ok wins; else NoVariantMatched.
            let mut chain = call(ident("Err"),
                vec![ex(ExprKind::Call {
                    func: Box::new(ex(ExprKind::Path(vec!["DeError".to_string(), "new".to_string()]))),
                    args: vec![CallArg::Item(ident("NoVariantMatched")),
                               CallArg::Named { name: "path".to_string(), value: str_lit("$") }],
                    trailing: None,
                })]);
            for v in variants.iter().rev() {
                let err_wild = Pattern::Variant {
                    path: vec!["Err".to_string()],
                    kind: VariantPatternKind::Tuple { patterns: vec![wildcard_pat()], rest: false },
                    span: span_dummy(),
                };
                chain = ex_match(untagged_attempt(v, file_id), vec![
                    match_arm_expr(result_pat("Ok", "__nv_uv", false),
                        call(ident("Ok"), vec![ident("__nv_uv")])),
                    match_arm_expr(err_wild, chain),
                ]);
            }
            FnBody::Block(Block { stmts: vec![], trailing: Some(Box::new(chain)), span: span_dummy(), is_unsafe: false })
        }
    }
}

/// Synthesize `.deserialize[D Deserializer](mut d D) -> Result[Self, DeError]`.
/// Record → type-directed pull; sum → externally-tagged dispatch (Ф.2-sum).
pub fn synthesize_deserialize<Q: DeriveQuery>(
    _ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    // Plan 180 Ф.6: validate/compute serde tagging mode (rejects tagging attrs
    // on a non-sum type: E_SERDE_TAGGING_ON_NON_SUM).
    let mode = serde_tagging_mode(type_decl)?;
    let body = if let Some(fields) = iter_fields(type_decl) {
    // Plan 180.1 Ф.1/Ф.7/Ф.10: resolve rename/rename_all/skip/default/alias +
    // validate the wire contract once (collisions, flatten-gate).
    let (type_opts, resolved) = resolve_fields(type_decl, &fields)?;
    let mut stmts: Vec<Stmt> = Vec::new();
    // Ф.7 (owner-decided reversal): strict-by-default unknown-field policy —
    // scan the wire object's keys against the known set BEFORE reading any
    // field, unless the type opts out via `#serde(allow_unknown)`.
    if !type_opts.allow_unknown {
        stmts.extend(build_unknown_field_check(&known_wire_names(&resolved)));
    }
    let mut lit_fields: Vec<RecordLitField> = Vec::new();
    for rf in &resolved {
        let f = &rf.field;
        if rf.opts.skip {
            // Plan 180.1 Ф.1.3: `skip` — never read from the wire; the field's
            // value is always the resolved default/zero value (the `.or` picks
            // the explicit `default = "fn"` if given, else the bare zero-value
            // path — `resolve_missing_value`'s `None` case IS "no default at
            // all", which for a `skip` field must still mean "zero-value", not
            // "required").
            let value = resolve_missing_value(
                &type_decl.name, &f.name, &f.ty, &rf.opts.default.clone().or(Some(None)),
                type_decl.span.file_id,
            )?;
            stmts.push(let_stmt(&f.name, false, None, value));
        } else if rf.opts.default.is_some() || !rf.opts.aliases.is_empty() {
            // Plan 180.1 Ф.1.5/Ф.1.6: default and/or alias customization — try
            // primary + aliases via `has_field`, else fall back.
            let mut names = vec![rf.wire.clone()];
            names.extend(rf.opts.aliases.iter().cloned());
            let is_opt = is_option_ty(&f.ty);
            let missing_block: Block = if rf.opts.default.is_some() {
                let v = resolve_missing_value(&type_decl.name, &f.name, &f.ty, &rf.opts.default, type_decl.span.file_id)?;
                block_trailing(v)
            } else if is_opt {
                // No explicit default on an Option field — absence (of ALL
                // candidate names) still means `None` (Q7 semantics extended
                // to cover aliases).
                block_trailing(ex(ExprKind::As(Box::new(ident("None")), f.ty.clone())))
            } else {
                // Required field, alias-only (no default): re-attempt the
                // PRIMARY wire name so the natural `MissingField(primary)`
                // error still fires (all candidates already confirmed absent).
                let final_cursor = format!("__nv_hf_final_{}", f.name);
                let mut fstmts = vec![let_stmt(&final_cursor, true, None,
                    try_(member_call(ident("d"), "enter_field", vec![str_lit(&rf.wire)])))];
                let vb = build_field_value_block(&f.name, &f.ty, &final_cursor, type_decl.span.file_id);
                fstmts.extend(vb.stmts);
                Block { stmts: fstmts, trailing: vb.trailing, span: span_dummy(), is_unsafe: false }
            };
            let ty_ann = if is_opt { Some(f.ty.clone()) } else { None };
            let value_expr = build_field_with_fallback(&f.name, &f.ty, &names, missing_block, type_decl.span.file_id);
            stmts.push(let_stmt(&f.name, false, ty_ann, value_expr));
        } else {
            // Plain path (unchanged shape from Ф.2-record) — only the WIRE
            // name (`rf.wire`, may differ under `rename`/`rename_all`) changes;
            // the local/Nova-side field name (`f.name`) never does.
            let sub = format!("__nv_de_{}", f.name);
            if let Some(plan) = narrow_scalar_deser_plan(&f.ty) {
                stmts.push(let_stmt(&sub, true, None, try_(member_call(
                    ident("d"), "enter_field", vec![str_lit(&rf.wire)]))));
                emit_narrow_scalar_deser(&mut stmts, &f.name, &sub, &plan);
            } else {
                let is_opt = is_option_ty(&f.ty);
                let enter = if is_opt { "enter_field_or_null" } else { "enter_field" };
                stmts.push(let_stmt(&sub, true, None, try_(member_call(
                    ident("d"), enter, vec![str_lit(&rf.wire)]))));
                let ty_ann = if is_opt { Some(f.ty.clone()) } else { None };
                stmts.push(let_stmt(&f.name, false, ty_ann, deser_field_expr(&f.ty, &sub, type_decl.span.file_id)));
            }
        }
        lit_fields.push(RecordLitField {
            name: f.name.clone(),
            // Shorthand `{ f }` (D52 §2): the field is bound by the local of the
            // same name; the explicit `f: f` form is rejected by the type checker.
            value: None,
            is_spread: false,
            at_shorthand: false,
            span: span_dummy(),
        });
    }
    // Ok(Type{ f1, f2, … })
    let record_lit = ex(ExprKind::RecordLit {
        type_name: Some(vec![type_decl.name.clone()]),
        fields: lit_fields,
        inferred_map_v: None,
        inferred_target_type: None,
    });
    let ok = call(ident("Ok"), vec![record_lit]);
        FnBody::Block(block_with_trailing(stmts, ok))
    } else if let Some(variants) = iter_sum_variants(type_decl) {
        synth_deserialize_sum_body(&type_decl.name, variants, &mode, type_decl.span.file_id)
    } else {
        return Err(DeriveError::UnsupportedTypeKind {
            type_name: type_decl.name.clone(),
            kind: type_decl_kind_name(type_decl).to_string(),
            protocol: DESERIALIZE.to_string(),
        });
    };
    // Return the CONCRETE receiver type (`Result[Type, DeError]`), not
    // `Result[Self, DeError]`: `Self` inside a `Result` generic resolves to a
    // POINTER ABI (`NovaValue_Type*`) which mismatches the by-value record
    // literal returned in the body. The concrete form yields the value ABI; the
    // protocol's `Result[Self, DeError]` still matches via the (now recursive)
    // `type_refs_equiv_modulo_self` Self↔receiver check.
    Ok(make_serde_method(
        &type_decl.name, "deserialize", true, "D", "Deserializer", "d",
        result_ty(type_ref_named(&type_decl.name), "DeError"), body, type_decl.span.file_id))
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
    inject_synthesized_methods_filtered(module, |_| true)
}

/// Plan 180: filtered injection. Serde (`Serialize`/`Deserialize`) is injected
/// BEFORE type-check (its bodies call other methods whose return types codegen's
/// annotation-free infer cannot resolve, so they must be type-checked +
/// annotated). The other auto-derive protocols (`Equal`/`Clone`/…/`Display`/
/// `Debug`) are injected AFTER type-check as before — some of their emitted
/// bodies (e.g. `@display` uses `w.write_str`, not a `Write` protocol method)
/// are intentionally NOT type-checkable and would be rejected if checked. The
/// `accept` predicate selects which protocols this pass injects.
pub fn inject_synthesized_methods_filtered<F: Fn(&str) -> bool>(
    module: &mut Module,
    accept: F,
) -> usize {
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
            if !accept(proto_name) {
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
// Plan 222.8 Ф.1 (D438) — Reflect auto-derive: synthesize `.reflect() ->
// TypeShape` for record/sum types carrying `#impl(Reflect)`. Same field-walk
// as Serialize (`resolve_fields`/`wire_name_for` for record wire-names,
// `serde_tagging_mode` for sum repr — "ничего нового не решается", brief
// 222.8 §1.2) but produces a single literal VALUE expression (not a
// push-protocol call sequence): a type's shape is a STATIC property of the
// type GRAPH, which can be genuinely cyclic (self-referential / mutually
// recursive types) — unlike Equal/Hash/Serialize, whose per-instance
// recursion always terminates on finite RUNTIME data (a linked list's
// `@equal` bottoms out because the VALUE is finite; `TypeShape` of a
// self-referential type would not, if built by unconditional dispatch).
//
// `in_progress` tracks the chain of type names currently being INLINED; a
// field naming one of those types emits `TypeShape.Ref(name)` INSTEAD of
// expanding further — the ONLY place a cycle is broken, so the produced
// literal AST is always finite by construction (not by a depth limit), and
// covers indirect/mutual cycles too (A→B→A: by the time B's own expansion
// reaches the field back to A, A is still on the SAME shared stack).
//
// A type providing an EXPLICIT (hand-written) `.reflect()` — e.g. a manual
// `Opaque(name)` wrapper for a raw/unshapeable type (owner-decided addition
// to `TypeShape`, see std/src/reflect.nv) — is DISPATCHED (a plain static
// call), never inlined: the compiler does not know what a hand-written body
// does, so it trusts it rather than trying to expand it. The compiler NEVER
// synthesizes `Opaque` itself — no "which types are opaque" policy lives
// here; that is entirely the hand-written impl's call.
// ────────────────────────────────────────────────────────────────────────

/// Qualified `TypeShape.Variant` reference — `Path(["TypeShape", name])`, NOT
/// a bare `Ident(name)`. Plan 222.8 Ф.1 found ([M-reflect-bare-variant-cu-
/// collision]): the checker's expected-type-driven resolution of a BARE
/// capitalized identifier/call — the same mechanism `deerror_unknown_variant`
/// et al. rely on elsewhere in this file — does NOT reliably disambiguate
/// when the CU also happens to declare an UNRELATED symbol of the identical
/// name (e.g. `spec_tests/conformance/v3_generic_newtype_non_ptr_inner_ok.nv`
/// declares an unrelated generic newtype `Tagged[T, U](int)`; in the merged
/// conformance mega-CU, a bare `Tagged("kind")` silently miscompiled into a
/// cast against THAT type's constructor instead of `SumRepr.Tagged`).
/// `TypeShape`/`SumRepr`'s variant names (`Record`/`Sum`/`Int`/`Str`/…) are
/// short and common enough that bare construction is a real collision risk
/// in any large compile unit, not a contrived edge case — so EVERY
/// `TypeShape`/`SumRepr` value this synthesizer builds is qualified,
/// unconditionally, rather than only patching the one collision found.
/// (Bare unqualified construction verified independently to work in
/// isolation via a scratch probe — `SumRepr.Tagged(..)`/`TypeShape.Record(..
/// )` qualified-call syntax compiles and resolves correctly, so this is a
/// zero-risk hardening, not a new syntax being introduced.)
fn typeshape_variant(name: &str) -> Expr {
    ex(ExprKind::Path(vec!["TypeShape".to_string(), name.to_string()]))
}

/// Qualified `SumRepr.Variant` reference — see `typeshape_variant` doc.
fn sumrepr_variant(name: &str) -> Expr {
    ex(ExprKind::Path(vec!["SumRepr".to_string(), name.to_string()]))
}

/// Scalar primitive → `TypeShape` leaf. Narrower than serde's scalar sets
/// (`serde_supported_scalar`/`serde_container_scalar`) on purpose: Reflect
/// has no wire-precision concern (no widen/narrow direction to pick — it
/// describes a TYPE, not a wire encoding), so every integer width maps
/// uniformly to `Int`. `char`/`byte`/`i128`/`u128` still have no `TypeShape`
/// variant of their own (no faithful shape) and are NOT included here,
/// mirroring serde's exclusion of the same primitives for the same reason.
fn reflect_scalar_shape(name: &str) -> Option<Expr> {
    match name {
        "int" | "i8" | "i16" | "i32" | "i64" | "uint" | "u8" | "u16" | "u32" | "u64" =>
            Some(typeshape_variant("Int")),
        "f32" | "f64" => Some(typeshape_variant("Float")),
        "bool" => Some(typeshape_variant("Bool")),
        "str" => Some(typeshape_variant("Str")),
        _ => None,
    }
}

/// Reflect-aware field eligibility — mirrors `check_field_eligibility_serde`'s
/// shape (bespoke `Option`/`Vec` recursion; a plain named type must either
/// provide an explicit `.reflect()` or declare `#impl(Reflect)`), minus the
/// byte-seq/HashMap/narrow-scalar wire concerns that don't apply here.
/// `Tuple` field types are NOT supported directly (v1 scope) — tuples only
/// appear internally, for multi-element sum-variant tuple-payload synthesis
/// (`build_variant_shape_expr`), not as a first-class field type.
pub fn check_field_eligibility_reflect<Q: DeriveQuery>(
    query: &Q,
    field_type: &TypeRef,
    protocol: &str,
    method_name: &str,
) -> bool {
    match field_type.strip_modifiers() {
        TypeRef::Named { path, generics, .. } => {
            let name = match path.last() { Some(n) => n.as_str(), None => return false };
            if reflect_scalar_shape(name).is_some() { return true; }
            match name {
                "Option" | "Vec" if generics.len() == 1 =>
                    check_field_eligibility_reflect(query, &generics[0], protocol, method_name),
                _ => {
                    if query.type_provides_method(name, method_name) { return true; }
                    if let Some(td) = query.lookup_type(name) {
                        if td.impl_protocols.iter().any(|p| p == protocol) { return true; }
                    }
                    false
                }
            }
        }
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) =>
            check_field_eligibility_reflect(query, inner, protocol, method_name),
        TypeRef::Unit(_) => true,
        _ => false,
    }
}

/// `(a, b)` tuple-literal expression — used for `TypeShape.Record`/`.Sum`'s
/// `[](str, TypeShape)` payload entries.
fn tuple2(a: Expr, b: Expr) -> Expr {
    ex(ExprKind::TupleLit(vec![a, b]))
}

/// Sum tagging mode (already parsed/validated for serde, `serde_tagging_mode`
/// — D382/D435) → `TypeShape.SumRepr` constructor expression. Reused
/// WHOLESALE (not re-parsed) per the 222.8 brief ("ничего нового не
/// решается, форма разметки уже вычислена для serde") — this DOES mean
/// Reflect inherits the same `#serde(untagged)` synthesis gate as Serialize/
/// Deserialize (`E_SERDE_UNTAGGED_GATED`, `[M-180-untagged-codegen-mono]`)
/// even though Reflect itself never touches `json.nv`'s mono-ordering —
/// a deliberate, documented simplification (D438), not a hidden coupling:
/// revisit if/when the untagged codegen gate lifts.
fn sum_repr_expr(mode: &SerdeTagging) -> Expr {
    match mode {
        SerdeTagging::External => sumrepr_variant("External"),
        SerdeTagging::Internal { tag } => call(sumrepr_variant("Tagged"), vec![str_lit(tag)]),
        SerdeTagging::Adjacent { tag, content } =>
            call(sumrepr_variant("TaggedContent"), vec![str_lit(tag), str_lit(content)]),
        // Unreachable in practice — `serde_tagging_mode` errors (gated)
        // before ever returning this variant; kept for match-exhaustiveness.
        SerdeTagging::Untagged => sumrepr_variant("Untagged"),
    }
}

/// FnDecl shell for a STATIC synthesized method with no params/generics
/// (`.reflect() -> TypeShape`) — `make_synth_method` hardcodes an INSTANCE
/// receiver (`@method`), `make_serde_method` forces a `[G Bound]` generic +
/// one param (the Serializer/Deserializer shape); Reflect's `.reflect()`
/// needs neither. `file_id`-tagged span mirrors `make_serde_method`'s
/// `fn_span` rationale (identifier resolution walks the WHOLE body using the
/// FnDecl's own span's file_id — must be the type's declaring file, not
/// whatever file happens to be the compilation entry).
fn make_reflect_method(type_name: &str, body_expr: Expr, file_id: crate::diag::FileId) -> FnDecl {
    let fn_span = Span::with_file(0, 0, file_id);
    FnDecl {
        name: "reflect".to_string(),
        receiver: Some(Receiver {
            type_name: type_name.to_string(),
            generics: vec![],
            carrier_bounds: vec![],
            receiver_ty: None,
            kind: ReceiverKind::Static,
            mutable: false,
            consume: false,
            span: fn_span,
        }),
        params: vec![],
        effects: vec![],
        return_type: Some(type_ref_named("TypeShape")),
        return_is_const: false,
        returns_receiver: false,
        body: FnBody::Block(block_trailing(body_expr)),
        span: fn_span,
        is_export: false,
        is_external: false,
        compiler_generated: true,
        ..FnDecl::default()
    }
}

/// Named non-scalar, non-container type reference `name` encountered while
/// building a `TypeShape` value: a cycle back-edge (`name` already on
/// `in_progress`) → `Ref(name)`; an EXPLICIT (hand-written) `.reflect()` →
/// dispatched via a static call (trusts the user's own body — this is the
/// `Opaque` escape hatch); an auto-derivable (`#impl(Reflect)`) type →
/// INLINED (pushed onto `in_progress`, its own shape built recursively,
/// popped) — see the section doc above for why inlining (not dispatch) is
/// required for correctness on a genuinely cyclic type graph.
///
/// `generics` — реестр 221.1 №146 (`[M-reflect-fieldwalk-generic-field-not-
/// monomorphized]`) fix: type-args поля-ресивера, ЕСЛИ `name` сам
/// generic-инстанциация (`PathParam[TodoIdParam]`, `Query[T]`, `Json[T]` —
/// канон-бандл 222.8). До фикса эта функция получала только голое `name` —
/// `build_type_shape_expr`'s Named-арм ОБРЕЗАЛ `generics` до вызова сюда,
/// и explicit-`.reflect()`-ветка ниже эмитила НЕКВАЛИФИЦИРОВАННЫЙ
/// `Path([name, "reflect"])` — статический вызов БЕЗ receiver-типа. Codegen
/// (emit_c.rs `ExprKind::Path` static-call арм) резолвит такой bare-Path
/// вызов по ИМЕНИ (`Nova_<name>_static_reflect`, БЕЗ mono-суффикса T) — для
/// generic-ресивера это НЕМОНОМОРФИЗИРОВАННЫЙ общий символ (тело — `return
/// NULL`, т.к. настоящее тело существует только per-T через
/// `Nova_<name>____<T>_static_reflect`). Symptom на живом сервере: `nova:
/// fiber stack overflow in slot 0` (пустой NULL-shape ломает downstream
/// OpenAPI-генерацию). Фикс: когда `generics` НЕ пуст, эмитим
/// TurboFish-квалифицированный `Member{obj: TurboFish{Ident(name),
/// generics}, name: "reflect"}` — ТА ЖЕ форма (`Type[T].method()`), что
/// codegen уже правильно монет для обычных генерик-типов через
/// `emit_call`'s "1b" static-turbofish dispatch (реестр 221.1 №137 фикс,
/// тот же коммит) — резолв T приходит СТРУКТУРНО из AST-узла (turbofish
/// type_args), не восстанавливается по имени.
fn build_named_type_shape<Q: DeriveQuery>(
    query: &Q,
    name: &str,
    generics: &[TypeRef],
    in_progress: &mut Vec<String>,
    file_id: crate::diag::FileId,
    owner_type: &str,
    field_name: &str,
) -> Result<Expr, DeriveError> {
    if in_progress.iter().any(|n| n == name) {
        return Ok(call(typeshape_variant("Ref"), vec![str_lit(name)]));
    }
    if query.type_provides_method(name, "reflect") {
        // Plan 221.1 №33 rationale (`member_call_at`/`deser_field_expr` doc):
        // a call synthesized for a HOST type's own body that dispatches to a
        // DIFFERENT type's static method is extension-policy-sensitive —
        // tag with the CURRENT synthesis's file_id, not span_dummy's
        // MAIN_FILE_ID. Simple named receiver → `Path([Type, "reflect"])`
        // (the static-call shape the parser emits; a `Member{Ident(Type)}`
        // form would wrongly dispatch as an INSTANCE method).
        //
        // №146: a GENERIC named receiver (`generics` non-empty) MUST instead
        // use the TurboFish-qualified `Type[Args].reflect()` shape — see the
        // fn-doc above. Without turbofish type-args the receiver's `T` is
        // unrecoverable downstream (codegen has no other channel to it).
        if generics.is_empty() {
            return Ok(call_at(
                ex_at(ExprKind::Path(vec![name.to_string(), "reflect".to_string()]), file_id),
                vec![],
                file_id,
            ));
        }
        let turbofish = ex_at(
            ExprKind::TurboFish {
                base: Box::new(ident_at(name, file_id)),
                type_args: generics.to_vec(),
            },
            file_id,
        );
        return Ok(member_call_at(turbofish, "reflect", vec![], file_id));
    }
    match query.lookup_type(name) {
        Some(td) if td.impl_protocols.iter().any(|p| p == REFLECT) => {
            in_progress.push(name.to_string());
            let result = build_type_decl_shape(query, td, in_progress);
            in_progress.pop();
            result
        }
        _ => Err(DeriveError::FieldLacksProtocol {
            type_name: owner_type.to_string(),
            field_name: field_name.to_string(),
            field_type: name.to_string(),
            protocol: REFLECT.to_string(),
        }),
    }
}

/// Build the `TypeShape` value-expression for a field/payload type `ty`.
/// `Option[T]`/`Vec[T]`/`[]T` recurse into the element and wrap
/// (`Opt(..)`/`Arr(..)`) — built HERE directly (not by dispatching to the
/// std blanket `Option[T Reflect].reflect()`/`[]T.reflect()`), so a
/// self-referential element (`type Node { children []Node }`) still goes
/// through the SAME `in_progress` cycle check as a direct field.
fn build_type_shape_expr<Q: DeriveQuery>(
    query: &Q,
    ty: &TypeRef,
    in_progress: &mut Vec<String>,
    file_id: crate::diag::FileId,
    owner_type: &str,
    field_name: &str,
) -> Result<Expr, DeriveError> {
    if let Some(inner) = option_inner(ty) {
        let shape = build_type_shape_expr(query, &inner, in_progress, file_id, owner_type, field_name)?;
        return Ok(call(typeshape_variant("Opt"), vec![shape]));
    }
    match ty.strip_modifiers() {
        TypeRef::Named { path, generics, .. } => {
            let name = path.last().map(|s| s.as_str()).unwrap_or("");
            if let Some(scalar) = reflect_scalar_shape(name) {
                return Ok(scalar);
            }
            if name == "Vec" && generics.len() == 1 {
                let shape = build_type_shape_expr(query, &generics[0], in_progress, file_id, owner_type, field_name)?;
                return Ok(call(typeshape_variant("Arr"), vec![shape]));
            }
            build_named_type_shape(query, name, generics, in_progress, file_id, owner_type, field_name)
        }
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
            let shape = build_type_shape_expr(query, inner, in_progress, file_id, owner_type, field_name)?;
            Ok(call(typeshape_variant("Arr"), vec![shape]))
        }
        TypeRef::Unit(_) => Ok(typeshape_variant("Unit")),
        _ => Err(DeriveError::FieldLacksProtocol {
            type_name: owner_type.to_string(),
            field_name: field_name.to_string(),
            field_type: type_ref_render(ty),
            protocol: REFLECT.to_string(),
        }),
    }
}

/// Build the `TypeShape` value-expression for one sum variant's payload,
/// keyed by variant name at the call site (`build_type_decl_shape`'s
/// `Sum(name, repr, variants)` list). Unit → `Unit`; single-element tuple →
/// TRANSPARENT (the element's own shape directly — mirrors serde's own
/// "single payload → bare content" treatment, e.g. adjacent-tagging's
/// `Val(int)` → `{"t":"Val","c":9}`, not `{"t":"Val","c":[9]}`); multi-
/// element tuple → a synthetic `Record(variant_name, [("0",..),("1",..)])`
/// (positional fields named by index — a documented Ф.1 representation
/// choice, D438); record payload → `Record(variant_name, [(field,..)])`
/// using RAW field names (no wire-rename: D435's own scope note says
/// `SumVariantKind::Record` payload fields don't consume field-attrs yet,
/// `[M-126-sum-*-rich]` — mirrors `synth_serialize_sum_body`'s identical
/// choice, `ser_struct_field_stmt(&mut stmts, &f.name, ..)`).
fn build_variant_shape_expr<Q: DeriveQuery>(
    query: &Q,
    v: &SumVariant,
    in_progress: &mut Vec<String>,
    file_id: crate::diag::FileId,
    owner_type: &str,
) -> Result<Expr, DeriveError> {
    match &v.kind {
        SumVariantKind::Unit => Ok(typeshape_variant("Unit")),
        SumVariantKind::Tuple(tys) if tys.len() == 1 =>
            build_type_shape_expr(query, &tys[0], in_progress, file_id, owner_type, &v.name),
        SumVariantKind::Tuple(tys) => {
            let mut items: Vec<ArrayElem> = Vec::new();
            for (i, ty) in tys.iter().enumerate() {
                let shape = build_type_shape_expr(query, ty, in_progress, file_id, owner_type, &v.name)?;
                items.push(ArrayElem::Item(tuple2(str_lit(&i.to_string()), shape)));
            }
            Ok(call(typeshape_variant("Record"), vec![str_lit(&v.name), ex(ExprKind::ArrayLit(items))]))
        }
        SumVariantKind::Record(fields) => {
            let mut items: Vec<ArrayElem> = Vec::new();
            for f in fields {
                let shape = build_type_shape_expr(query, &f.ty, in_progress, file_id, owner_type, &f.name)?;
                items.push(ArrayElem::Item(tuple2(str_lit(&f.name), shape)));
            }
            Ok(call(typeshape_variant("Record"), vec![str_lit(&v.name), ex(ExprKind::ArrayLit(items))]))
        }
    }
}

/// Build the full `TypeShape` value-expression for a record/sum `TypeDecl`
/// (the entry point AND the recursive-inline step, shared — see
/// `build_named_type_shape`). Record → `Record(name, [(wire, shape), ..])`
/// (`resolve_fields`/`wire_name_for` — D435 rename/rename_all/alias, `skip`
/// fields excluded, exactly like `synthesize_serialize`'s record body). Sum
/// → `Sum(name, repr, [(variant, shape), ..])` (`serde_tagging_mode` for
/// `repr` — D382/D435).
fn build_type_decl_shape<Q: DeriveQuery>(
    query: &Q,
    td: &TypeDecl,
    in_progress: &mut Vec<String>,
) -> Result<Expr, DeriveError> {
    let file_id = td.span.file_id;
    if let Some(fields) = iter_fields(td) {
        let (_type_opts, resolved) = resolve_fields(td, &fields)?;
        let mut items: Vec<ArrayElem> = Vec::new();
        for rf in &resolved {
            if rf.opts.skip { continue; }
            let shape = build_type_shape_expr(
                query, &rf.field.ty, in_progress, file_id, &td.name, &rf.field.name,
            )?;
            items.push(ArrayElem::Item(tuple2(str_lit(&rf.wire), shape)));
        }
        Ok(call(typeshape_variant("Record"), vec![str_lit(&td.name), ex(ExprKind::ArrayLit(items))]))
    } else if let Some(variants) = iter_sum_variants(td) {
        let mode = serde_tagging_mode(td)?;
        let repr_expr = sum_repr_expr(&mode);
        let mut items: Vec<ArrayElem> = Vec::new();
        for v in variants {
            let shape = build_variant_shape_expr(query, v, in_progress, file_id, &td.name)?;
            items.push(ArrayElem::Item(tuple2(str_lit(&v.name), shape)));
        }
        Ok(call(typeshape_variant("Sum"), vec![str_lit(&td.name), repr_expr, ex(ExprKind::ArrayLit(items))]))
    } else {
        Err(DeriveError::UnsupportedTypeKind {
            type_name: td.name.clone(),
            kind: type_decl_kind_name(td).to_string(),
            protocol: REFLECT.to_string(),
        })
    }
}

/// Synthesize `.reflect() -> TypeShape` (Plan 222.8 Ф.1, D438). Record/sum
/// type-DECL-kinds only (`UnsupportedTypeKind` for Newtype/Alias/Effect/
/// Protocol/Opaque — the type-decl-kind sense, NOT to be confused with the
/// unrelated `TypeShape.Opaque` VALUE variant, which this synthesizer never
/// produces — see the section doc above).
pub fn synthesize_reflect<Q: DeriveQuery>(
    ctx: &mut AutoDeriveCtx<'_, Q>,
    type_decl: &TypeDecl,
) -> Result<FnDecl, DeriveError> {
    let mut in_progress = vec![type_decl.name.clone()];
    let body_expr = build_type_decl_shape(ctx.query, type_decl, &mut in_progress)?;
    Ok(make_reflect_method(&type_decl.name, body_expr, type_decl.span.file_id))
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
                    default: None,
                },
                NamedTupleField {
                    name: "second".to_string(),
                    ty: type_ref_named("int"),
                    span: Span::dummy(),
                    priv_field: false,
                    priv_module_field: false,
                    visible_to: vec![],
                    default: None,
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
    // Plan 152.7.1 (D374 AMEND): display param renamed `sb StringBuilder` → `w Write`.
    #[test]
    fn t27_synthesize_display_takes_write() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_record_type("Point", &[("x", "int"), ("y", "int")]);
        let fd = synthesize_method(&mut ctx, &td, DISPLAY).unwrap();
        assert_eq!(fd.name, "display");
        assert_eq!(fd.params.len(), 1);
        assert_eq!(fd.params[0].name, "w");
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
                    default: None,
                },
                NamedTupleField {
                    name: "second".to_string(),
                    ty: type_ref_named("int"),
                    span: Span::dummy(),
                    priv_field: false,
                    priv_module_field: false,
                    visible_to: vec![],
                    default: None,
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

    // ─── T31–T36: Plan 180 Ф.1 — SUM rich synthesis (not placeholder) ──
    fn make_sum_type(name: &str, proto: &str) -> TypeDecl {
        // `type Shape | Nought | Dot(int) | Ring { r int }`
        let variants = vec![
            SumVariant {
                name: "Nought".to_string(),
                kind: SumVariantKind::Unit,
                discriminant: None,
                span: Span::dummy(),
                serde_attrs: Vec::new(),
                doc: None,
            },
            SumVariant {
                name: "Dot".to_string(),
                kind: SumVariantKind::Tuple(vec![type_ref_named("int")]),
                discriminant: None,
                span: Span::dummy(),
                serde_attrs: Vec::new(),
                doc: None,
            },
            SumVariant {
                name: "Ring".to_string(),
                kind: SumVariantKind::Record(vec![RecordField {
                    name: "r".to_string(),
                    ty: type_ref_named("int"),
                    span: Span::dummy(),
                    ..RecordField::default()
                }]),
                discriminant: None,
                span: Span::dummy(),
                serde_attrs: Vec::new(),
                doc: None,
            },
        ];
        let mut td = TypeDecl {
            name: name.to_string(),
            kind: TypeDeclKind::Sum(variants),
            span: Span::dummy(),
            ..TypeDecl::default()
        };
        td.impl_protocols.push(proto.to_string());
        td
    }

    #[test]
    fn t31_sum_equal_is_match_not_identity() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_sum_type("Shape", "Equal");
        let fd = synthesize_method(&mut ctx, &td, EQUAL).unwrap();
        // Rich synth = `match @ { … }`, NOT the old `@ == other` identity.
        match &fd.body {
            FnBody::Expr(e) => match &e.kind {
                ExprKind::Match { arms, .. } => assert_eq!(arms.len(), 3),
                other => panic!("expected Match body, got {:?}", other),
            },
            other => panic!("expected FnBody::Expr(Match), got {:?}", other),
        }
    }

    #[test]
    fn t32_sum_hash_is_match_not_zero() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_sum_type("Shape", "Hash");
        let fd = synthesize_method(&mut ctx, &td, HASH).unwrap();
        match &fd.body {
            FnBody::Expr(e) => assert!(
                matches!(&e.kind, ExprKind::Match { .. }),
                "expected Match body, got {:?}", e.kind
            ),
            other => panic!("expected FnBody::Expr, got {:?}", other),
        }
    }

    #[test]
    fn t33_sum_clone_is_match_not_self() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_sum_type("Shape", "Clone");
        let fd = synthesize_method(&mut ctx, &td, CLONE).unwrap();
        match &fd.body {
            FnBody::Expr(e) => match &e.kind {
                ExprKind::Match { arms, .. } => {
                    assert_eq!(arms.len(), 3);
                    // Unit-variant arm reconstructs via bare ident (not SelfAccess).
                    assert!(matches!(
                        &arms[0].pattern,
                        Pattern::Variant { kind: VariantPatternKind::Unit, .. }
                    ));
                }
                other => panic!("expected Match body, got {:?}", other),
            },
            other => panic!("expected FnBody::Expr, got {:?}", other),
        }
    }

    #[test]
    fn t34_sum_compare_is_block_not_zero() {
        let q = MockQuery::new();
        let mut ctx = AutoDeriveCtx::new(&q);
        let td = make_sum_type("Shape", "Compare");
        let fd = synthesize_method(&mut ctx, &td, COMPARE).unwrap();
        // Rich compare = tag-extract block, not the old `FnBody::Expr(0)`.
        assert!(matches!(&fd.body, FnBody::Block(_)), "expected block body");
    }

    #[test]
    fn t35_sum_display_debug_are_blocks() {
        let q = MockQuery::new();
        for proto in [DISPLAY, DEBUG] {
            let mut ctx = AutoDeriveCtx::new(&q);
            let td = make_sum_type("Shape", proto);
            let fd = synthesize_method(&mut ctx, &td, proto).unwrap();
            assert!(matches!(&fd.body, FnBody::Block(_)), "expected block for {proto}");
        }
    }

    #[test]
    fn t36_sum_serde_externally_tagged() {
        // Plan 180 Ф.2-sum: sum + Serialize/Deserialize now synthesize
        // externally-tagged bodies (primitive payloads are eligible).
        let q = MockQuery::new();
        let td_s = make_sum_type("Shape", "Serialize");
        let mut ctx = AutoDeriveCtx::new(&q);
        let fd_s = synthesize_method(&mut ctx, &td_s, SERIALIZE).unwrap();
        assert_eq!(fd_s.name, "serialize");
        assert!(matches!(&fd_s.body, FnBody::Block(_)));

        let td_d = make_sum_type("Shape", "Deserialize");
        let mut ctx2 = AutoDeriveCtx::new(&q);
        let fd_d = synthesize_method(&mut ctx2, &td_d, DESERIALIZE).unwrap();
        assert_eq!(fd_d.name, "deserialize");
        assert!(fd_d.receiver.as_ref().unwrap().kind == ReceiverKind::Static);
    }

    #[test]
    fn t37_sum_serde_ineligible_payload_typed_error() {
        // A variant payload lacking serde conformance → typed FieldLacksProtocol
        // (named by variant), never a bad synth.
        let mut q = MockQuery::new();
        q.add_type(make_record_type("Widget", &[("n", "int")])); // no #impl(Serialize)
        let td = TypeDecl {
            name: "Holder".to_string(),
            kind: TypeDeclKind::Sum(vec![SumVariant {
                name: "Has".to_string(),
                kind: SumVariantKind::Tuple(vec![type_ref_named("Widget")]),
                discriminant: None,
                span: Span::dummy(),
                serde_attrs: Vec::new(),
                doc: None,
            }]),
            span: Span::dummy(),
            ..TypeDecl::default()
        };
        let mut ctx = AutoDeriveCtx::new(&q);
        let err = synthesize_method(&mut ctx, &td, SERIALIZE).unwrap_err();
        match err {
            DeriveError::FieldLacksProtocol { field_name, .. } => assert_eq!(field_name, "Has"),
            other => panic!("expected FieldLacksProtocol, got {:?}", other),
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
            doc_attrs: vec![],
            span: Span::dummy(),
            peer_files: vec![],
            doc: None,
            rebind_shadows: std::collections::HashMap::new(),
            consume_reuse_spans: std::collections::HashSet::new(),
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
