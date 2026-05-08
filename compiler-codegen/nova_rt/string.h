/* string.h — Plan 13 umbrella header для str runtime API.
 *
 * Plan 13 фиксирует: компилятор знает str-функции через
 * `runtime_registry.rs` — single source of truth. Этот header
 * существует как stable include-point: при future рефакторингах
 * str-функции могут переехать сюда из nova_rt.h, но эту дверь
 * std/runtime/string.nv (auto-generated) закрывает за собой.
 *
 * Сейчас все str-функции живут в `nova_rt.h` (для backward compat
 * существующего кода). Этот header просто `#include`'ит nova_rt.h.
 *
 * См. docs/plans/13-runtime-stdlib-and-autogen.md.
 */

#ifndef NOVA_RT_STRING_H
#define NOVA_RT_STRING_H

#include "nova_rt.h"

/* Все str runtime-функции уже декларированы в nova_rt.h:
 *   nova_str_starts_with, nova_str_ends_with, nova_str_contains,
 *   nova_str_to_upper, nova_str_to_lower, nova_str_trim,
 *   nova_str_slice, nova_str_concat, nova_str_eq,
 *   nova_str_find, nova_str_rfind, nova_str_char_len,
 *   nova_str_byte_len, nova_str_bytes, nova_str_chars,
 *   nova_str_char_at, nova_str_split, nova_str_is_empty.
 *
 * std/runtime/string.nv (auto-generated) — Nova-side декларации.
 */

#endif /* NOVA_RT_STRING_H */
