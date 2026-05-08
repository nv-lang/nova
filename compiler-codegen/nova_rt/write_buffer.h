#ifndef NOVA_RT_WRITE_BUFFER_H
#define NOVA_RT_WRITE_BUFFER_H

/* ---- Nova WriteBuffer — binary serialization buffer (Plan 04) ----
 *
 * Append-only buffer для бинарных протоколов. Endianness explicit:
 * @write_uN_le / @write_uN_be — программист выбирает явно.
 *
 * Capacity-grow: 2x при переполнении. Initial capacity — 16 байт.
 *
 * После consume (@into) флаг `consumed = 1`. Любой @write на consumed
 * buffer → nova_assert.
 *
 * См. spec/decisions/08-runtime.md → D26, D82,
 * spec/open-questions.md → Q-write-buffer.
 */

#include "alloc.h"
#include "nova_rt.h"
#include <stdint.h>
#include <string.h>

#define NOVA_WRITE_BUFFER_INIT_CAP 16

typedef struct Nova_WriteBuffer {
    nova_byte* data;
    int64_t    len;
    int64_t    cap;
    nova_bool  consumed;
} Nova_WriteBuffer;

static inline Nova_WriteBuffer* Nova_WriteBuffer_static_new(void) {
    Nova_WriteBuffer* b = (Nova_WriteBuffer*)nova_alloc(sizeof(Nova_WriteBuffer));
    b->data = (nova_byte*)nova_alloc(NOVA_WRITE_BUFFER_INIT_CAP);
    b->len = 0;
    b->cap = NOVA_WRITE_BUFFER_INIT_CAP;
    b->consumed = 0;
    return b;
}

static inline Nova_WriteBuffer* Nova_WriteBuffer_static_with_capacity(nova_int n) {
    Nova_WriteBuffer* b = (Nova_WriteBuffer*)nova_alloc(sizeof(Nova_WriteBuffer));
    int64_t cap = n > 0 ? (int64_t)n : NOVA_WRITE_BUFFER_INIT_CAP;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    b->len = 0;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

static inline Nova_WriteBuffer* Nova_WriteBuffer_static_from(NovaArray_nova_byte* arr) {
    Nova_WriteBuffer* b = (Nova_WriteBuffer*)nova_alloc(sizeof(Nova_WriteBuffer));
    int64_t cap = arr->len > 0 ? arr->len : NOVA_WRITE_BUFFER_INIT_CAP;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    if (arr->len > 0) memcpy(b->data, arr->data, (size_t)arr->len);
    b->len = arr->len;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

static inline void _nova_write_buffer_reserve(Nova_WriteBuffer* b, int64_t extra) {
    int64_t need = b->len + extra;
    if (need <= b->cap) return;
    int64_t new_cap = b->cap;
    while (new_cap < need) new_cap *= 2;
    nova_byte* new_data = (nova_byte*)nova_alloc((size_t)new_cap);
    memcpy(new_data, b->data, (size_t)b->len);
    b->data = new_data;
    b->cap = new_cap;
}

static inline void _nova_write_buffer_check_live(Nova_WriteBuffer* b) {
    nova_assert(!b->consumed, "write buffer consumed: cannot mutate after @into");
}

/* @write_byte(v byte) — append single byte. v passed as nova_int (sign-ext); LSB used. */
static inline nova_unit Nova_WriteBuffer_method_write_byte(Nova_WriteBuffer* b, nova_int v) {
    _nova_write_buffer_check_live(b);
    _nova_write_buffer_reserve(b, 1);
    b->data[b->len++] = (nova_byte)(v & 0xFF);
    return NOVA_UNIT;
}

/* @write_bytes(src []byte) — copy of input bytes. */
static inline nova_unit Nova_WriteBuffer_method_write_bytes(Nova_WriteBuffer* b, NovaArray_nova_byte* src) {
    _nova_write_buffer_check_live(b);
    if (src->len == 0) return NOVA_UNIT;
    _nova_write_buffer_reserve(b, src->len);
    memcpy(b->data + b->len, src->data, (size_t)src->len);
    b->len += src->len;
    return NOVA_UNIT;
}

/* Plan 12 acceptance: @write_zero(n int) — append n null bytes.
 * Тест что добавление новой external fn в builtins.nv + runtime impl
 * работает без правки Rust-codegen'а (registry-driven dispatch). */
static inline nova_unit Nova_WriteBuffer_method_write_zero(Nova_WriteBuffer* b, nova_int n) {
    _nova_write_buffer_check_live(b);
    if (n <= 0) return NOVA_UNIT;
    _nova_write_buffer_reserve(b, n);
    memset(b->data + b->len, 0, (size_t)n);
    b->len += n;
    return NOVA_UNIT;
}

/* Plan 04 Этап 6: text → UTF-8 bytes append.
 * @write_char(c char): UTF-8 encode codepoint в 1-4 bytes.
 * @write_str(s str):   копирует UTF-8 bytes из nova_str.
 *
 * Используется для смешанных text+binary use-case'ов (URL
 * percent-decoding и т.п.) — replace для Buffer.add_char/add_str. */
static inline nova_unit Nova_WriteBuffer_method_write_char(Nova_WriteBuffer* b, nova_int cp) {
    _nova_write_buffer_check_live(b);
    /* UTF-8 encode codepoint cp в 1-4 байта в b->data. */
    _nova_write_buffer_reserve(b, 4);
    int n = _nova_utf8_encode(b->data + b->len, cp);
    b->len += n;
    return NOVA_UNIT;
}

static inline nova_unit Nova_WriteBuffer_method_write_str(Nova_WriteBuffer* b, nova_str s) {
    _nova_write_buffer_check_live(b);
    if (s.len == 0) return NOVA_UNIT;
    _nova_write_buffer_reserve(b, (int64_t)s.len);
    memcpy(b->data + b->len, s.ptr, s.len);
    b->len += (int64_t)s.len;
    return NOVA_UNIT;
}

/* ────── 18 numeric × LE/BE × write helpers ─────────────────────────────
 *
 * Все принимают nova_int (для u/i 8-32, signedness handled by codegen)
 * или nova_f64 (для f32/f64). Это согласовано с bootstrap-конвенцией где
 * uN/iN до 64 бит передаются как nova_int (== int64_t), а f32/f64 как
 * nova_f64 (== double).
 */

/* u8 / i8 — без endianness (1 байт). */
static inline nova_unit Nova_WriteBuffer_method_write_u8(Nova_WriteBuffer* b, nova_int v) {
    _nova_write_buffer_check_live(b);
    _nova_write_buffer_reserve(b, 1);
    b->data[b->len++] = (nova_byte)(v & 0xFF);
    return NOVA_UNIT;
}
static inline nova_unit Nova_WriteBuffer_method_write_i8(Nova_WriteBuffer* b, nova_int v) {
    _nova_write_buffer_check_live(b);
    _nova_write_buffer_reserve(b, 1);
    b->data[b->len++] = (nova_byte)(v & 0xFF);
    return NOVA_UNIT;
}

/* Helper macros для 16/32/64 bit писем. */
#define NOVA_WB_WRITE_LE_16(b, v) do { \
    _nova_write_buffer_check_live(b); \
    _nova_write_buffer_reserve(b, 2); \
    uint16_t _nova_u = (uint16_t)(v); \
    (b)->data[(b)->len + 0] = (nova_byte)(_nova_u & 0xFF); \
    (b)->data[(b)->len + 1] = (nova_byte)((_nova_u >> 8) & 0xFF); \
    (b)->len += 2; \
} while (0)

#define NOVA_WB_WRITE_BE_16(b, v) do { \
    _nova_write_buffer_check_live(b); \
    _nova_write_buffer_reserve(b, 2); \
    uint16_t _nova_u = (uint16_t)(v); \
    (b)->data[(b)->len + 0] = (nova_byte)((_nova_u >> 8) & 0xFF); \
    (b)->data[(b)->len + 1] = (nova_byte)(_nova_u & 0xFF); \
    (b)->len += 2; \
} while (0)

#define NOVA_WB_WRITE_LE_32(b, v) do { \
    _nova_write_buffer_check_live(b); \
    _nova_write_buffer_reserve(b, 4); \
    uint32_t _nova_u = (uint32_t)(v); \
    (b)->data[(b)->len + 0] = (nova_byte)(_nova_u & 0xFF); \
    (b)->data[(b)->len + 1] = (nova_byte)((_nova_u >> 8) & 0xFF); \
    (b)->data[(b)->len + 2] = (nova_byte)((_nova_u >> 16) & 0xFF); \
    (b)->data[(b)->len + 3] = (nova_byte)((_nova_u >> 24) & 0xFF); \
    (b)->len += 4; \
} while (0)

#define NOVA_WB_WRITE_BE_32(b, v) do { \
    _nova_write_buffer_check_live(b); \
    _nova_write_buffer_reserve(b, 4); \
    uint32_t _nova_u = (uint32_t)(v); \
    (b)->data[(b)->len + 0] = (nova_byte)((_nova_u >> 24) & 0xFF); \
    (b)->data[(b)->len + 1] = (nova_byte)((_nova_u >> 16) & 0xFF); \
    (b)->data[(b)->len + 2] = (nova_byte)((_nova_u >> 8) & 0xFF); \
    (b)->data[(b)->len + 3] = (nova_byte)(_nova_u & 0xFF); \
    (b)->len += 4; \
} while (0)

#define NOVA_WB_WRITE_LE_64(b, v) do { \
    _nova_write_buffer_check_live(b); \
    _nova_write_buffer_reserve(b, 8); \
    uint64_t _nova_u = (uint64_t)(v); \
    for (int _i = 0; _i < 8; ++_i) \
        (b)->data[(b)->len + _i] = (nova_byte)((_nova_u >> (_i * 8)) & 0xFF); \
    (b)->len += 8; \
} while (0)

#define NOVA_WB_WRITE_BE_64(b, v) do { \
    _nova_write_buffer_check_live(b); \
    _nova_write_buffer_reserve(b, 8); \
    uint64_t _nova_u = (uint64_t)(v); \
    for (int _i = 0; _i < 8; ++_i) \
        (b)->data[(b)->len + _i] = (nova_byte)((_nova_u >> ((7 - _i) * 8)) & 0xFF); \
    (b)->len += 8; \
} while (0)

/* u16 / i16 / u32 / i32 / u64 / i64 — все варианты. */
static inline nova_unit Nova_WriteBuffer_method_write_u16_le(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_LE_16(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_u16_be(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_BE_16(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_i16_le(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_LE_16(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_i16_be(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_BE_16(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_u32_le(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_LE_32(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_u32_be(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_BE_32(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_i32_le(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_LE_32(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_i32_be(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_BE_32(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_u64_le(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_LE_64(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_u64_be(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_BE_64(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_i64_le(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_LE_64(b, v); return NOVA_UNIT; }
static inline nova_unit Nova_WriteBuffer_method_write_i64_be(Nova_WriteBuffer* b, nova_int v) { NOVA_WB_WRITE_BE_64(b, v); return NOVA_UNIT; }

/* f32 / f64 — IEEE 754 bit-cast. */
static inline nova_unit Nova_WriteBuffer_method_write_f32_le(Nova_WriteBuffer* b, nova_f64 v) {
    float f = (float)v;
    uint32_t u;
    memcpy(&u, &f, 4);
    NOVA_WB_WRITE_LE_32(b, u);
    return NOVA_UNIT;
}
static inline nova_unit Nova_WriteBuffer_method_write_f32_be(Nova_WriteBuffer* b, nova_f64 v) {
    float f = (float)v;
    uint32_t u;
    memcpy(&u, &f, 4);
    NOVA_WB_WRITE_BE_32(b, u);
    return NOVA_UNIT;
}
static inline nova_unit Nova_WriteBuffer_method_write_f64_le(Nova_WriteBuffer* b, nova_f64 v) {
    uint64_t u;
    memcpy(&u, &v, 8);
    NOVA_WB_WRITE_LE_64(b, u);
    return NOVA_UNIT;
}
static inline nova_unit Nova_WriteBuffer_method_write_f64_be(Nova_WriteBuffer* b, nova_f64 v) {
    uint64_t u;
    memcpy(&u, &v, 8);
    NOVA_WB_WRITE_BE_64(b, u);
    return NOVA_UNIT;
}

/* @len() / @capacity(). */
static inline nova_int Nova_WriteBuffer_method_len(Nova_WriteBuffer* b) { return (nova_int)b->len; }
static inline nova_int Nova_WriteBuffer_method_capacity(Nova_WriteBuffer* b) { return (nova_int)b->cap; }

/* @clone() -> WriteBuffer — deep copy. */
static inline Nova_WriteBuffer* Nova_WriteBuffer_method_clone(Nova_WriteBuffer* src) {
    Nova_WriteBuffer* b = (Nova_WriteBuffer*)nova_alloc(sizeof(Nova_WriteBuffer));
    int64_t cap = src->cap;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    if (src->len > 0) memcpy(b->data, src->data, (size_t)src->len);
    b->len = src->len;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

/* @into() -> []byte — consume, ownership transfer. */
static inline NovaArray_nova_byte* Nova_WriteBuffer_method_into(Nova_WriteBuffer* b) {
    _nova_write_buffer_check_live(b);
    NovaArray_nova_byte* arr = (NovaArray_nova_byte*)nova_alloc(sizeof(NovaArray_nova_byte));
    arr->cap = b->len > 0 ? b->len : 8;
    arr->len = b->len;
    arr->data = (nova_byte*)nova_alloc((size_t)arr->cap * sizeof(nova_byte));
    if (b->len > 0) memcpy(arr->data, b->data, (size_t)b->len);
    b->consumed = 1;
    return arr;
}

#endif /* NOVA_RT_WRITE_BUFFER_H */
