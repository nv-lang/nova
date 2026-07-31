//! Plan opunify (owner decision 2026-08-01, BRIEF_opunify.md): ONE table +
//! ONE dispatch path for every binary/unary operator-overload, replacing the
//! per-operator copy-paste that plans 65/85.4/175/234/int128 grew over time
//! (three same-class regressions in two days: `Set[int] | Set[int]` with no
//! mono body, `<<`/`>>` not dispatching at all, `@neg`/`@not` DCE'd away).
//! Was `bitwise_ops.rs` (Plan 234, git mv — history preserved); this module
//! now owns the D46 selector table (`03-syntax.md`) for ALL binops, not just
//! the bitwise family, and the shared resolver both the bitwise fast-path
//! AND the arithmetic fast-path call through. `emit_c.rs` keeps only thin
//! call-sites (arch-ratchet, `scripts/guards/arch-ratchet.sh`), same pattern
//! as `codegen/mono_method_registry.rs` (№129) and `codegen/assoc_ro.rs`
//! (№157).

use crate::ast::{BinOp, UnOp, FnDecl, TypeRef};
use super::emit_c::MethodSig;

/// Shape of the second operand for a binary operator-overload dispatch (D46,
/// `03-syntax.md`). Drives which flat (non-generic-receiver) resolution
/// strategy `resolve_binop_dispatch` uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum OperandShape {
    /// RHS type == `Self` (the receiver) — the checker already guarantees
    /// this holds, so the flat/non-generic-receiver path does not need a
    /// runtime overload lookup: it can emit `<Recv>_method_<name>(l, r)`
    /// directly. `+ * / % & | ^` (Plan 65/234's original "flat fast-path").
    Homogeneous,
    /// RHS type may legitimately differ from `Self` (`Timestamp - Duration`,
    /// `record << int`) — the flat path MUST look up `method_overloads` for
    /// an overload whose sole parameter matches `rty` EXACTLY (D124: this is
    /// what prevents e.g. silent `Timestamp - Monotonic` cross-clock
    /// arithmetic), erroring with `NoMatchingOverload` otherwise. `- << >>`.
    Heterogeneous,
}

/// One row of the D46 operator-dispatch table: operator -> {selector name,
/// operand shape}. SINGLE source of truth, consumed by:
/// - `resolve_binop_dispatch` (this file) — the shared dispatch algorithm.
/// - `emit_c.rs` call-sites — thin, table-driven instead of per-op copies.
/// - `lints.rs` `collect_used_names` — DCE seeds iterate this table instead
///   of a hand-maintained selector list (closes the "forgot to seed" class
///   of bug: the reachability closure never sees these as syntactic calls).
pub(crate) struct BinOpEntry {
    pub op: BinOp,
    /// D46 selector name (`03-syntax.md`) — e.g. `Add` -> `"plus"`. Names
    /// are NOT invented here; each one already existed per-operator before
    /// this unification (see the file history / git-blame on the pre-mv
    /// `bitwise_ops.rs` and the arithmetic arms this table replaces in
    /// `emit_c.rs`).
    pub method_name: &'static str,
    pub shape: OperandShape,
}

pub(crate) const BINOP_TABLE: &[BinOpEntry] = &[
    BinOpEntry { op: BinOp::Add,    method_name: "plus",   shape: OperandShape::Homogeneous },
    BinOpEntry { op: BinOp::Mul,    method_name: "times",  shape: OperandShape::Homogeneous },
    BinOpEntry { op: BinOp::Div,    method_name: "div",    shape: OperandShape::Homogeneous },
    BinOpEntry { op: BinOp::Mod,    method_name: "rem",    shape: OperandShape::Homogeneous },
    BinOpEntry { op: BinOp::BitAnd, method_name: "bitand", shape: OperandShape::Homogeneous },
    BinOpEntry { op: BinOp::BitOr,  method_name: "bitor",  shape: OperandShape::Homogeneous },
    BinOpEntry { op: BinOp::BitXor, method_name: "bitxor", shape: OperandShape::Homogeneous },
    BinOpEntry { op: BinOp::Sub,    method_name: "minus",  shape: OperandShape::Heterogeneous },
    BinOpEntry { op: BinOp::Shl,    method_name: "shl",    shape: OperandShape::Heterogeneous },
    BinOpEntry { op: BinOp::Shr,    method_name: "shr",    shape: OperandShape::Heterogeneous },
];

/// One row of the D46 UNARY operator-dispatch table (`- ! ~`).
pub(crate) struct UnOpEntry {
    pub op: UnOp,
    pub method_name: &'static str,
}

pub(crate) const UNOP_TABLE: &[UnOpEntry] = &[
    UnOpEntry { op: UnOp::Neg,    method_name: "neg" },
    UnOpEntry { op: UnOp::Not,    method_name: "not" },
    UnOpEntry { op: UnOp::BitNot, method_name: "bitnot" },
];

/// D46 selector name for a binary operator-overload, `None` for operators
/// with no user-overload form (`==`/comparisons — `emit_c.rs` still handles
/// those with its own `@equal`/`@compare` protocol-dispatch chain — and
/// non-overloadable sugar like `==>`).
pub(crate) fn binop_method_name(op: BinOp) -> Option<&'static str> {
    BINOP_TABLE.iter().find(|e| e.op == op).map(|e| e.method_name)
}

/// D46 selector name for a unary operator-overload.
pub(crate) fn unop_method_name(op: UnOp) -> Option<&'static str> {
    UNOP_TABLE.iter().find(|e| e.op == op).map(|e| e.method_name)
}

/// Outcome of resolving a table-driven binary operator dispatch for a heap
/// `Nova_T*` (record/sum VALUE, single pointer) receiver.
pub(crate) enum BinOpResolution {
    /// Concrete non-generic overload found — its already-mangled `c_name`.
    Concrete(String),
    /// Generic-mono receiver (`Set[T]`-class, `____` in the receiver
    /// spelling): caller must `register_mono_method_instance(&fn_decl,
    /// type_subst, &mono_name, &recv_short)` before emitting a call to
    /// `mono_name`. Shared by ALL table entries — this is what closes
    /// [M-arith-binop-generic-receiver-no-mono-register]: `+ * / %` did not
    /// have this arm at all before unification (only `- & | ^ << >>` did).
    GenericMono {
        fn_decl: FnDecl,
        type_subst: Vec<(String, String)>,
        mono_name: String,
    },
    /// A registered overload set exists on this receiver but none takes
    /// `rty` (Heterogeneous shape only) — hard compile error (D124).
    NoMatchingOverload(String),
    /// Nothing registered for this receiver/method — leave to fall-through
    /// untouched (would become invalid C, caught later).
    NotFound,
}

/// ONE resolver for every `BINOP_TABLE` entry on a `Nova_T*` (record/sum
/// value, single pointer) receiver — replaces the separate per-operator
/// flat+generic-mono arms `emit_c.rs` used to carry for Bit*/Shl-Shr/@minus,
/// and ADDS the previously-missing generic-mono arm for `+ * / %` (closes
/// [M-arith-binop-generic-receiver-no-mono-register]).
///
/// Caller contract (mirrors the pre-unification `resolve_shift_dispatch`):
/// - `recv_full`: `lty` (or the matched sum-type side) minus the trailing
///   `*`, `Nova_` prefix INTACT (e.g. `Nova_Set____nova_int`, `Nova_Timestamp`).
/// - `recv_short`: `recv_full` minus the `Nova_` prefix (`Set____nova_int`,
///   `Timestamp`) — the key `method_overloads`/`self_method_decls` use.
/// - `overloads`: `self.method_overloads.get(&(recv_short, op_method))`
///   (only consulted for `Heterogeneous` shape — see `OperandShape`).
/// - `mono_fn_decl`: `self.self_method_decls.get(&(base_type, op_method))`
///   when `recv_short` carries a `____` mono marker, `None` otherwise.
pub(crate) fn resolve_binop_dispatch(
    op: BinOp,
    rty: &str,
    recv_full: &str,
    recv_short: &str,
    overloads: Option<&[MethodSig]>,
    mono_fn_decl: Option<FnDecl>,
) -> BinOpResolution {
    let Some(entry) = BINOP_TABLE.iter().find(|e| e.op == op) else {
        return BinOpResolution::NotFound;
    };
    let op_method = entry.method_name;
    if entry.shape == OperandShape::Heterogeneous {
        if let Some(sigs) = overloads {
            let matching = sigs.iter().find(|s| {
                s.is_instance && s.param_c_types.len() == 1 && s.param_c_types[0] == rty
            });
            if let Some(sig) = matching {
                return BinOpResolution::Concrete(sig.c_name.clone());
            }
            return BinOpResolution::NoMatchingOverload(format!(
                "binop `{:?}`: no @{} overload on {} taking {} \
                 (D46 03-syntax.md / D124: exact-overload cross-type rule). \
                 Available overloads: {}",
                op,
                op_method,
                recv_short,
                rty,
                sigs.iter()
                    .filter(|s| s.is_instance && s.param_c_types.len() == 1)
                    .map(|s| s.param_c_types[0].clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        }
        // No registered overload SET at all — either a generic-mono
        // receiver or truly nothing; falls through to the shared
        // generic-mono arm below, same as `Homogeneous`.
    }
    // Shared generic-mono branch (D46: `Set[T]`-class receivers where
    // `method_overloads` has no concrete mono'd entry — the erased key is
    // the base type, not the mono'd one).
    if let Some(idx) = recv_short.find("____") {
        let Some(fn_decl) = mono_fn_decl else { return BinOpResolution::NotFound; };
        let mono_args = &recv_short[idx + 4..];
        let mono_parts: Vec<String> = mono_args.split("__").map(|s| s.to_string()).collect();
        let recv_generics: Vec<String> = fn_decl.receiver.as_ref()
            .map(|r| r.generics.iter().filter_map(|tr| {
                if let TypeRef::Named { path, .. } = tr { path.first().cloned() } else { None }
            }).collect::<Vec<_>>())
            .unwrap_or_default();
        if recv_generics.len() == mono_parts.len() {
            let type_subst: Vec<(String, String)> = recv_generics.into_iter().zip(mono_parts).collect();
            let mono_name = format!("{}_method_{}", recv_full, op_method);
            return BinOpResolution::GenericMono { fn_decl, type_subst, mono_name };
        }
        return BinOpResolution::NotFound;
    }
    // No `____` marker — non-generic (flat) receiver.
    match entry.shape {
        // Homogeneous flat receiver: the checker already guarantees the
        // overload exists (it type-checked `Self op Self`), so no runtime
        // `method_overloads` lookup is needed — emit the direct call. This
        // is the SAME blind emission `+ * / % & | ^` always did pre-
        // unification (byte-identical for every currently-passing flat
        // case); now routed through this ONE resolver instead of being
        // duplicated per operator at the call-site.
        OperandShape::Homogeneous =>
            BinOpResolution::Concrete(format!("{}_method_{}", recv_full, op_method)),
        // Heterogeneous flat receiver with NO registered overload set at
        // all (the `overloads` param was `None`) — nothing to dispatch to;
        // leave to fall-through (would become invalid C, caught later).
        // Matches the pre-unification `@minus`/shift behavior exactly.
        OperandShape::Heterogeneous => BinOpResolution::NotFound,
    }
}

/// План 234 Ф.2 — integer-promotion таблица эмиссии унарного `~` на
/// ПРИМИТИВНОМ C-типе (D46-амендмент, решение владельца 2026-07-27: эмитить
/// надёжный низкий уровень, как обход и делал). C продвигает узкий операнд
/// в `int` перед `~`; БЕЗЗНАКОВЫЕ узкие (`u8`->`nova_byte`, `u16`->
/// `uint16_t`) получают НУЛЕВОЕ продвижение, так что голый `~` оставил бы
/// лишние единичные биты выше исходной ширины в expression-контексте —
/// эмитим `x ^ MASK` нужной ширины вместо (маскирует продвинутые лишние
/// биты, восстанавливая корректное узкое значение). ЗНАКОВЫЕ узкие
/// (`i8`/`i16`) получают ЗНАКОВОЕ продвижение — голый `~x` УЖЕ корректен в
/// диапазоне типа (XOR-маска была бы НЕВЕРНА без каста). Широкие типы
/// (ранг >= `int`: `i32`/`i64`/`u32`/`u64`/`int`/`uint`) продвижения не
/// имеют вовсе — голый `~` корректен. `operand_c` — уже эмитированный C-
/// текст операнда; `operand_c_ty` — его C-тип, как даёт `infer_expr_c_type`.
pub(crate) fn bitnot_primitive_emit(operand_c: &str, operand_c_ty: &str) -> String {
    let mask = match operand_c_ty {
        "nova_byte" => Some("0xFFU"),
        "uint16_t" => Some("0xFFFFU"),
        _ => None,
    };
    match mask {
        Some(m) => format!("({} ^ {})", operand_c, m),
        None => format!("(~{})", operand_c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binop_table_covers_arithmetic_bitwise_and_shift() {
        assert_eq!(binop_method_name(BinOp::Add), Some("plus"));
        assert_eq!(binop_method_name(BinOp::Sub), Some("minus"));
        assert_eq!(binop_method_name(BinOp::Mul), Some("times"));
        assert_eq!(binop_method_name(BinOp::Div), Some("div"));
        assert_eq!(binop_method_name(BinOp::Mod), Some("rem"));
        assert_eq!(binop_method_name(BinOp::BitAnd), Some("bitand"));
        assert_eq!(binop_method_name(BinOp::BitOr), Some("bitor"));
        assert_eq!(binop_method_name(BinOp::BitXor), Some("bitxor"));
        assert_eq!(binop_method_name(BinOp::Shl), Some("shl"));
        assert_eq!(binop_method_name(BinOp::Shr), Some("shr"));
        assert_eq!(binop_method_name(BinOp::Eq), None);
    }

    #[test]
    fn resolve_homogeneous_flat_receiver_emits_direct_call() {
        // No `____` marker, no overloads passed in (checker already
        // guaranteed the flat receiver has the overload) — Homogeneous ops
        // (`+ * / % & | ^`) resolve to a direct call, mirroring the
        // pre-unification blind emission exactly.
        match resolve_binop_dispatch(BinOp::Add, "Nova_Duration", "Nova_Duration", "Duration", None, None) {
            BinOpResolution::Concrete(c) => assert_eq!(c, "Nova_Duration_method_plus"),
            _ => panic!("expected Concrete"),
        }
        match resolve_binop_dispatch(BinOp::BitOr, "Nova_SetX", "Nova_SetX", "SetX", None, None) {
            BinOpResolution::Concrete(c) => assert_eq!(c, "Nova_SetX_method_bitor"),
            _ => panic!("expected Concrete"),
        }
    }

    #[test]
    fn resolve_heterogeneous_flat_receiver_no_overloads_is_not_found() {
        // Mirrors the pre-unification `@minus`/shift behavior: no
        // registered overload SET at all (not even a mismatched one) and no
        // `____` marker => NotFound, left for the caller's fall-through.
        match resolve_binop_dispatch(BinOp::Sub, "nova_int", "Nova_Timestamp", "Timestamp", None, None) {
            BinOpResolution::NotFound => {}
            _ => panic!("expected NotFound"),
        }
    }

    #[test]
    fn unop_table_covers_neg_not_bitnot() {
        assert_eq!(unop_method_name(UnOp::Neg), Some("neg"));
        assert_eq!(unop_method_name(UnOp::Not), Some("not"));
        assert_eq!(unop_method_name(UnOp::BitNot), Some("bitnot"));
    }

    #[test]
    fn bitnot_narrow_unsigned_uses_mask() {
        assert_eq!(bitnot_primitive_emit("x", "nova_byte"), "(x ^ 0xFFU)");
        assert_eq!(bitnot_primitive_emit("x", "uint16_t"), "(x ^ 0xFFFFU)");
    }

    #[test]
    fn bitnot_signed_narrow_and_wide_use_bare_tilde() {
        assert_eq!(bitnot_primitive_emit("x", "int8_t"), "(~x)");
        assert_eq!(bitnot_primitive_emit("x", "int16_t"), "(~x)");
        assert_eq!(bitnot_primitive_emit("x", "uint32_t"), "(~x)");
        assert_eq!(bitnot_primitive_emit("x", "nova_int"), "(~x)");
        assert_eq!(bitnot_primitive_emit("x", "nova_uint"), "(~x)");
    }
}
