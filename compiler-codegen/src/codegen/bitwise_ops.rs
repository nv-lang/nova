//! Plan 234 (D46-амендмент 2026-07-27, spec/decisions/03-syntax.md D46):
//! чистые helper'ы для побитового операторного семейства (`@bitand`/
//! `@bitor`/`@bitxor`/`@bitnot`) — держим НОВУЮ логику ВНЕ `emit_c.rs`
//! (arch-ratchet, `scripts/guards/arch-ratchet.sh`), тот же паттерн, что
//! `codegen/mono_method_registry.rs` (№129) и `codegen/assoc_ro.rs` (№157):
//! логика/обоснования — здесь, `emit_c.rs` получает только тонкие
//! call-сайты.

use crate::ast::BinOp;

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
