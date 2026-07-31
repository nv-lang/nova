//! Plan 234 (D46-амендмент 2026-07-27, spec/decisions/03-syntax.md D46):
//! чистые helper'ы для побитового операторного семейства (`@bitand`/
//! `@bitor`/`@bitxor`/`@bitnot`) — держим НОВУЮ логику ВНЕ `emit_c.rs`
//! (arch-ratchet, `scripts/guards/arch-ratchet.sh`), тот же паттерн, что
//! `codegen/mono_method_registry.rs` (№129) и `codegen/assoc_ro.rs` (№157):
//! логика/обоснования — здесь, `emit_c.rs` получает только тонкие
//! call-сайты.

use crate::ast::{BinOp, FnDecl, TypeRef};
use super::emit_c::MethodSig;

/// D46-амендмент: `BinOp` -> имя operator-dispatch метода для ТРЁХ
/// побитовых бинопов (`&`/`|`/`^`). `None` для всего остального.
/// Используется на ОБОИХ dispatch-сайтах в `emit_c.rs` (generic
/// `self_method_decls`-путь И плоский `is_single_nova_ptr` fast-path,
/// зеркало `@plus`/`@times`) — единая точка, чтобы два сайта не разошлись
/// в маппинге имён.
pub(crate) fn bitop_method_name(op: BinOp) -> Option<&'static str> {
    match op {
        BinOp::BitAnd => Some("bitand"),
        BinOp::BitOr => Some("bitor"),
        BinOp::BitXor => Some("bitxor"),
        _ => None,
    }
}

/// [M-shl-shr-user-type-no-dispatch] (int128-связка, план 234 «фикс тем же
/// паттерном»): `BinOp` -> имя operator-dispatch метода для сдвигов
/// (`<<`/`>>`). D46 (`03-syntax.md:2872`): `a << n` → `@shl`, `a >> n` →
/// `@shr` — эти два НЕ переименовывались планом 234 (не входили в
/// `and`/`or`/`xor`-путаницу), поэтому имя метода = буквальное имя оператора,
/// в отличие от `bitop_method_name` выше. Отдельная функция (не слияние с
/// `bitop_method_name`) — держит D46-таблицу «бинoп → имя метода» явной по
/// каждой семье, зеркало структуры, не единая мешанина.
pub(crate) fn shift_method_name(op: BinOp) -> Option<&'static str> {
    match op {
        BinOp::Shl => Some("shl"),
        BinOp::Shr => Some("shr"),
        _ => None,
    }
}

/// [M-shl-shr-user-type-no-dispatch]: outcome of resolving `<<`/`>>` operator
/// dispatch for a heap `Nova_T*` receiver. `@shl`/`@shr` take a HETEROGENEOUS
/// second param (`n int`, not `Self`, D46 `03-syntax.md:2872`) — mirrors
/// `@minus`'s dispatch shape (`Timestamp - Duration`, emit_c.rs), NOT the
/// homogeneous Bit* fast-path (`Set[T] & Set[T]`, both operands `Self`).
/// Kept out of `emit_c.rs` (arch-ratchet) — `emit_c.rs` does only the
/// `self`-owned `HashMap` reads (`method_overloads`/`self_method_decls`) and
/// hands the results here for the actual matching/formatting decision.
pub(crate) enum ShiftResolution {
    /// Concrete non-generic overload found — its already-mangled `c_name`.
    Concrete(String),
    /// Generic-mono receiver (`Set[T]`-class, `____` in the receiver
    /// spelling): caller must `register_mono_method_instance(&fn_decl,
    /// type_subst, &mono_name, &recv_short)` before emitting a call to
    /// `mono_name`.
    GenericMono {
        fn_decl: FnDecl,
        type_subst: Vec<(String, String)>,
        mono_name: String,
    },
    /// A registered overload set exists on this receiver but none takes
    /// `rty` — hard compile error (mirrors `@minus`'s D124 rejection point).
    NoMatchingOverload(String),
    /// Nothing registered for this receiver/method — leave to fall-through
    /// untouched (would become invalid C, caught later).
    NotFound,
}

/// Resolve `<<`/`>>` dispatch given the caller's own `method_overloads`/
/// `self_method_decls` lookups (plain data — no `&self`, see `ShiftResolution`
/// doc). `overloads` = `self.method_overloads.get(&(recv_short, op_method))`;
/// `mono_fn_decl` = `self.self_method_decls.get(&(base_type, op_method))`
/// when `recv_short` carries a `____` mono-name, `None` otherwise.
pub(crate) fn resolve_shift_dispatch(
    op: BinOp,
    rty: &str,
    recv_full: &str,
    recv_short: &str,
    overloads: Option<&[MethodSig]>,
    mono_fn_decl: Option<FnDecl>,
) -> ShiftResolution {
    let Some(op_method) = shift_method_name(op) else { return ShiftResolution::NotFound; };
    if let Some(sigs) = overloads {
        let matching = sigs.iter().find(|s| {
            s.is_instance && s.param_c_types.len() == 1 && s.param_c_types[0] == rty
        });
        if let Some(sig) = matching {
            return ShiftResolution::Concrete(sig.c_name.clone());
        }
        return ShiftResolution::NoMatchingOverload(format!(
            "binop `{}`: no @{} overload on {} taking {} \
             (D46 03-syntax.md:2872). Available overloads: {}",
            if matches!(op, BinOp::Shl) { "<<" } else { ">>" },
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
    // D46: @shl/@shr for generic types (Set[T]-class) when method_overloads has
    // no concrete mono'd entry (erased key is base type, not mono'd) — mirrors
    // the @minus generic-mono arm (emit_c.rs).
    if let Some(idx) = recv_short.find("____") {
        if let Some(fn_decl) = mono_fn_decl {
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
                return ShiftResolution::GenericMono { fn_decl, type_subst, mono_name };
            }
        }
    }
    ShiftResolution::NotFound
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
    fn bitop_names_match_d46() {
        assert_eq!(bitop_method_name(BinOp::BitAnd), Some("bitand"));
        assert_eq!(bitop_method_name(BinOp::BitOr), Some("bitor"));
        assert_eq!(bitop_method_name(BinOp::BitXor), Some("bitxor"));
        assert_eq!(bitop_method_name(BinOp::Add), None);
    }

    #[test]
    fn shift_names_match_d46() {
        assert_eq!(shift_method_name(BinOp::Shl), Some("shl"));
        assert_eq!(shift_method_name(BinOp::Shr), Some("shr"));
        assert_eq!(shift_method_name(BinOp::BitAnd), None);
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
