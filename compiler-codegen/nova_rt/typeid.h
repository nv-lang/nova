#ifndef NOVA_RT_TYPEID_H
#define NOVA_RT_TYPEID_H

/*
 * Plan 61 Ф.1: TypeId runtime infrastructure.
 *
 * Каждый Nova-тип (user-defined sum, record, primitive) получает unique
 * compile-time константу NovaTypeId. Используется:
 *
 *   1. Plan 61 Ф.2 — erased Fail[any] path: throw упаковывает
 *      `(void* err, NovaTypeId tid)`. Handler arm для `Fail` (без [E])
 *      получает `e: any` + runtime tag.
 *
 *   2. Nova-side `any.is[T]() -> bool` / `any.as[T]() -> Option[T]`
 *      (D54 anonymous-protocol `any`) — runtime сравнение tag'а.
 *
 *   3. Diagnostic / debug: `nova_typeid_to_name(tid)` для panic messages,
 *      bug reports, gdb pretty-print.
 *
 * Allocation strategy:
 *   - NOVA_TID_NONE = 0 — sentinel, "not a real type" (используется
 *     в default-init vtables).
 *   - Per-type IDs >= 1 эмитятся compile-time как `#define NOVA_TID_<mangled> N`
 *     в auto-gen'd секции preamble (emit_c.rs:emit_typeid_defines).
 *   - Monotonic counter (1, 2, 3, ...) — порядок не стабилен между
 *     compile-сессиями, но это OK: TID используется только within
 *     compilation unit (single C source per Nova module/program).
 *
 * Primitive типы (nova_int, nova_str, nova_bool, и т.д.) получают
 * reserved IDs 1..16 для cheap pattern-match в any.is_int() / etc.
 */

#include <stdint.h>

typedef uint32_t NovaTypeId;

#define NOVA_TID_NONE         ((NovaTypeId)0)

/* Reserved primitive IDs (1..16). Plan 61: эти константы стабильны
 * между compile-сессиями и используются в hard-coded runtime helpers
 * (nova_any_is_*, nova_print_any). User-defined types получают IDs >=17. */
#define NOVA_TID_nova_int     ((NovaTypeId)1)
#define NOVA_TID_nova_str     ((NovaTypeId)2)
#define NOVA_TID_nova_bool    ((NovaTypeId)3)
#define NOVA_TID_nova_f64     ((NovaTypeId)4)
#define NOVA_TID_nova_f32     ((NovaTypeId)5)
#define NOVA_TID_nova_byte    ((NovaTypeId)6)
#define NOVA_TID_nova_unit    ((NovaTypeId)7)
/* 8..16 reserved для future primitives. User IDs start at 17. */
#define NOVA_TID_USER_BASE    ((NovaTypeId)17)

/* Auto-gen'd defines будут splice'нуты ниже codegen'ом в emit_preamble
 * (после emit_c.rs::emit_typeid_defines). Здесь — runtime helpers. */

/* Сравнение типов. Тривиально inline. */
static inline int nova_typeid_eq(NovaTypeId a, NovaTypeId b) {
    return a == b;
}

/* Diagnostic name lookup. Implementation — в auto-gen'd C-file
 * (compiler-codegen генерирует switch-case на основе всех registered
 * types). Здесь — forward decl. Если codegen не emit'ит implementation
 * (например, в minimal test), linker найдёт weak fallback в typeid.c. */
const char* nova_typeid_to_name(NovaTypeId tid);

/* Plan 61 Ф.2 (forward): `any.is[T]()` / `any.as[T]()` builtin support.
 * Используется codegen'ом для `e is ParseError` runtime check внутри
 * `with Fail = |e: any| ...` handler-arm. tid — actual type tag из
 * boxed any-value; expected — compile-time константа NOVA_TID_<T>. */
static inline int nova_any_is_typeid(NovaTypeId actual, NovaTypeId expected) {
    return actual == expected;
}

#endif /* NOVA_RT_TYPEID_H */
