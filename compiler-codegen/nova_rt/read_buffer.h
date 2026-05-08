#ifndef NOVA_RT_READ_BUFFER_H
#define NOVA_RT_READ_BUFFER_H

/* ---- Nova ReadBuffer — cursor-style binary reader (Plan 04) ----
 *
 * View над []byte с position-cursor. @read_* / @try_read_* —
 * pair (Fail-form / Result-form), генерируется через одну C-функцию.
 *
 * Endianness explicit (как в WriteBuffer): @read_uN_le / @read_uN_be.
 *
 * См. spec/decisions/08-runtime.md → D26, D77, D82,
 * spec/open-questions.md → Q-read-buffer.
 */

#include "alloc.h"
#include "nova_rt.h"
#include <stdint.h>
#include <string.h>

typedef struct Nova_ReadBuffer {
    const nova_byte* data;
    int64_t          len;
    int64_t          pos;
} Nova_ReadBuffer;

/* ReadBuffer.from(b []byte) — view, no copy. */
static inline Nova_ReadBuffer* Nova_ReadBuffer_static_from_bytes(NovaArray_nova_byte* arr) {
    Nova_ReadBuffer* b = (Nova_ReadBuffer*)nova_alloc(sizeof(Nova_ReadBuffer));
    b->data = arr->data;
    b->len = arr->len;
    b->pos = 0;
    return b;
}

/* @position() -> int. */
static inline nova_int Nova_ReadBuffer_method_position(Nova_ReadBuffer* b) {
    return (nova_int)b->pos;
}

/* @remaining() -> int. */
static inline nova_int Nova_ReadBuffer_method_remaining(Nova_ReadBuffer* b) {
    return (nova_int)(b->len - b->pos);
}

/* @has_remaining(n int) -> bool. */
static inline nova_bool Nova_ReadBuffer_method_has_remaining(Nova_ReadBuffer* b, nova_int n) {
    if (n < 0) return 0;
    return (b->len - b->pos) >= n;
}

/* @remaining_bytes() -> []byte (copy of remaining). */
static inline NovaArray_nova_byte* Nova_ReadBuffer_method_remaining_bytes(Nova_ReadBuffer* b) {
    int64_t rem = b->len - b->pos;
    NovaArray_nova_byte* arr = (NovaArray_nova_byte*)nova_alloc(sizeof(NovaArray_nova_byte));
    arr->cap = rem > 0 ? rem : 8;
    arr->len = rem;
    arr->data = (nova_byte*)nova_alloc((size_t)arr->cap * sizeof(nova_byte));
    if (rem > 0) memcpy(arr->data, b->data + b->pos, (size_t)rem);
    return arr;
}

/* ────── Helper: throw UnexpectedEnd Fail with wanted/available ──────
 *
 * Bootstrap-ограничение: ReadBufferError структурированный sum-тип, но
 * Nova_Fail_fail сейчас принимает только nova_str payload. Поэтому
 * Fail-form @read_* throw'ит сообщение "ReadBuffer.UnexpectedEnd:
 * wanted N, available M" — формат совместимый с tests'ами.
 *
 * Когда fail-frame mechanism будет расширен на void* payload (см.
 * D26 Bootstrap-ограничение про RuntimeError), wrapper'ы обновятся
 * чтобы пакетировать ReadBufferError struct напрямую.
 */
static inline void _nova_read_buffer_throw_unexpected_end(int64_t wanted, int64_t available) {
    /* Сообщение: "ReadBuffer.UnexpectedEnd: wanted N, available M". */
    char msg[96];
    int n = snprintf(msg, sizeof(msg),
        "ReadBuffer.UnexpectedEnd: wanted %lld, available %lld",
        (long long)wanted, (long long)available);
    if (n < 0) n = 0;
    if ((size_t)n >= sizeof(msg)) n = (int)sizeof(msg) - 1;
    /* Copy в heap чтобы fail-frame пережил stack-unwind. */
    char* heap_msg = (char*)nova_alloc((size_t)n + 1);
    memcpy(heap_msg, msg, (size_t)n);
    heap_msg[n] = '\0';
    Nova_Fail_fail((nova_str){.ptr = heap_msg, .len = (size_t)n});
}

/* ────── Read primitives (одна функция на N-bit / endianness) ──────
 *
 * Возвращает nova_bool: 1 если success (out_ptr заполнен), 0 если short.
 */
static inline nova_bool _nova_rb_read_u8_raw(Nova_ReadBuffer* b, uint8_t* out) {
    if (b->len - b->pos < 1) return 0;
    *out = (uint8_t)b->data[b->pos];
    b->pos += 1;
    return 1;
}
static inline nova_bool _nova_rb_read_u16_le_raw(Nova_ReadBuffer* b, uint16_t* out) {
    if (b->len - b->pos < 2) return 0;
    *out = (uint16_t)b->data[b->pos] | ((uint16_t)b->data[b->pos + 1] << 8);
    b->pos += 2;
    return 1;
}
static inline nova_bool _nova_rb_read_u16_be_raw(Nova_ReadBuffer* b, uint16_t* out) {
    if (b->len - b->pos < 2) return 0;
    *out = ((uint16_t)b->data[b->pos] << 8) | (uint16_t)b->data[b->pos + 1];
    b->pos += 2;
    return 1;
}
static inline nova_bool _nova_rb_read_u32_le_raw(Nova_ReadBuffer* b, uint32_t* out) {
    if (b->len - b->pos < 4) return 0;
    *out =  (uint32_t)b->data[b->pos]
         | ((uint32_t)b->data[b->pos + 1] << 8)
         | ((uint32_t)b->data[b->pos + 2] << 16)
         | ((uint32_t)b->data[b->pos + 3] << 24);
    b->pos += 4;
    return 1;
}
static inline nova_bool _nova_rb_read_u32_be_raw(Nova_ReadBuffer* b, uint32_t* out) {
    if (b->len - b->pos < 4) return 0;
    *out =  ((uint32_t)b->data[b->pos] << 24)
         | ((uint32_t)b->data[b->pos + 1] << 16)
         | ((uint32_t)b->data[b->pos + 2] << 8)
         |  (uint32_t)b->data[b->pos + 3];
    b->pos += 4;
    return 1;
}
static inline nova_bool _nova_rb_read_u64_le_raw(Nova_ReadBuffer* b, uint64_t* out) {
    if (b->len - b->pos < 8) return 0;
    uint64_t v = 0;
    for (int i = 0; i < 8; ++i) v |= ((uint64_t)b->data[b->pos + i]) << (i * 8);
    *out = v;
    b->pos += 8;
    return 1;
}
static inline nova_bool _nova_rb_read_u64_be_raw(Nova_ReadBuffer* b, uint64_t* out) {
    if (b->len - b->pos < 8) return 0;
    uint64_t v = 0;
    for (int i = 0; i < 8; ++i) v |= ((uint64_t)b->data[b->pos + i]) << ((7 - i) * 8);
    *out = v;
    b->pos += 8;
    return 1;
}

/* ────── Fail-form @read_* ────── */

static inline nova_byte Nova_ReadBuffer_method_read_byte(Nova_ReadBuffer* b) {
    uint8_t out;
    if (!_nova_rb_read_u8_raw(b, &out)) {
        _nova_read_buffer_throw_unexpected_end(1, b->len - b->pos);
        return 0;
    }
    return (nova_byte)out;
}
static inline NovaArray_nova_byte* Nova_ReadBuffer_method_read_bytes(Nova_ReadBuffer* b, nova_int n) {
    if (n < 0 || (b->len - b->pos) < n) {
        _nova_read_buffer_throw_unexpected_end(n, b->len - b->pos);
        return NULL;
    }
    NovaArray_nova_byte* arr = (NovaArray_nova_byte*)nova_alloc(sizeof(NovaArray_nova_byte));
    arr->cap = n > 0 ? n : 8;
    arr->len = n;
    arr->data = (nova_byte*)nova_alloc((size_t)arr->cap);
    if (n > 0) memcpy(arr->data, b->data + b->pos, (size_t)n);
    b->pos += n;
    return arr;
}
static inline nova_byte Nova_ReadBuffer_method_read_u8(Nova_ReadBuffer* b) {
    uint8_t out;
    if (!_nova_rb_read_u8_raw(b, &out)) {
        _nova_read_buffer_throw_unexpected_end(1, b->len - b->pos);
        return 0;
    }
    return (nova_byte)out;
}
static inline nova_int Nova_ReadBuffer_method_read_i8(Nova_ReadBuffer* b) {
    uint8_t out;
    if (!_nova_rb_read_u8_raw(b, &out)) {
        _nova_read_buffer_throw_unexpected_end(1, b->len - b->pos);
        return 0;
    }
    return (nova_int)(int8_t)out;
}

/* Macros для генерации 16/32/64-bit Fail-форм. */
#define NOVA_RB_DEFINE_READ(WIDTH, ENDIAN, SIGNED, RET_TYPE, RET_CAST) \
    static inline RET_TYPE Nova_ReadBuffer_method_read_##SIGNED##WIDTH##_##ENDIAN(Nova_ReadBuffer* b) { \
        uint##WIDTH##_t out; \
        if (!_nova_rb_read_u##WIDTH##_##ENDIAN##_raw(b, &out)) { \
            _nova_read_buffer_throw_unexpected_end(WIDTH/8, b->len - b->pos); \
            return 0; \
        } \
        return RET_CAST out; \
    }

/* u16/u32/u64 — return nova_int (zero-extended). */
NOVA_RB_DEFINE_READ(16, le, u, nova_int, (nova_int))
NOVA_RB_DEFINE_READ(16, be, u, nova_int, (nova_int))
NOVA_RB_DEFINE_READ(32, le, u, nova_int, (nova_int))
NOVA_RB_DEFINE_READ(32, be, u, nova_int, (nova_int))
NOVA_RB_DEFINE_READ(64, le, u, nova_int, (nova_int))
NOVA_RB_DEFINE_READ(64, be, u, nova_int, (nova_int))

/* i16/i32/i64 — return nova_int (sign-extended via cast through signed). */
NOVA_RB_DEFINE_READ(16, le, i, nova_int, (nova_int)(int16_t))
NOVA_RB_DEFINE_READ(16, be, i, nova_int, (nova_int)(int16_t))
NOVA_RB_DEFINE_READ(32, le, i, nova_int, (nova_int)(int32_t))
NOVA_RB_DEFINE_READ(32, be, i, nova_int, (nova_int)(int32_t))
NOVA_RB_DEFINE_READ(64, le, i, nova_int, (nova_int)(int64_t))
NOVA_RB_DEFINE_READ(64, be, i, nova_int, (nova_int)(int64_t))

/* f32/f64 — IEEE 754 bit-cast. */
static inline nova_f64 Nova_ReadBuffer_method_read_f32_le(Nova_ReadBuffer* b) {
    uint32_t out;
    if (!_nova_rb_read_u32_le_raw(b, &out)) {
        _nova_read_buffer_throw_unexpected_end(4, b->len - b->pos);
        return 0.0;
    }
    float f;
    memcpy(&f, &out, 4);
    return (nova_f64)f;
}
static inline nova_f64 Nova_ReadBuffer_method_read_f32_be(Nova_ReadBuffer* b) {
    uint32_t out;
    if (!_nova_rb_read_u32_be_raw(b, &out)) {
        _nova_read_buffer_throw_unexpected_end(4, b->len - b->pos);
        return 0.0;
    }
    float f;
    memcpy(&f, &out, 4);
    return (nova_f64)f;
}
static inline nova_f64 Nova_ReadBuffer_method_read_f64_le(Nova_ReadBuffer* b) {
    uint64_t out;
    if (!_nova_rb_read_u64_le_raw(b, &out)) {
        _nova_read_buffer_throw_unexpected_end(8, b->len - b->pos);
        return 0.0;
    }
    nova_f64 d;
    memcpy(&d, &out, 8);
    return d;
}
static inline nova_f64 Nova_ReadBuffer_method_read_f64_be(Nova_ReadBuffer* b) {
    uint64_t out;
    if (!_nova_rb_read_u64_be_raw(b, &out)) {
        _nova_read_buffer_throw_unexpected_end(8, b->len - b->pos);
        return 0.0;
    }
    nova_f64 d;
    memcpy(&d, &out, 8);
    return d;
}

/* ────── Try-form @try_read_* — Result[T, ReadBufferError] ──────
 *
 * Bootstrap-ограничение: Result[T, E] зашит на (nova_int Ok, nova_str Err).
 * Поэтому Ok-payload — нужно box'ить non-int значения (NovaArray_nova_byte*
 * → boxed pointer как nova_int, nova_byte → zero-extend, nova_f64 →
 * union punning через double-as-int64). Это согласовано с подходом для
 * других Result-возвратов в bootstrap-codegen.
 *
 * Err-вариант хранится как nova_str с тем же сообщением "ReadBuffer.
 * UnexpectedEnd: wanted N, available M".
 *
 * См. spec/decisions/08-runtime.md → D26 «Result зашит на (nova_int, nova_str)»
 * для bootstrap.
 */

static inline Nova_Result* _nova_rb_make_err(int64_t wanted, int64_t available) {
    char msg[96];
    int n = snprintf(msg, sizeof(msg),
        "ReadBuffer.UnexpectedEnd: wanted %lld, available %lld",
        (long long)wanted, (long long)available);
    if (n < 0) n = 0;
    if ((size_t)n >= sizeof(msg)) n = (int)sizeof(msg) - 1;
    char* heap_msg = (char*)nova_alloc((size_t)n + 1);
    memcpy(heap_msg, msg, (size_t)n);
    heap_msg[n] = '\0';
    return nova_make_Result_Err((nova_str){.ptr = heap_msg, .len = (size_t)n});
}

static inline Nova_Result* Nova_ReadBuffer_method_try_read_byte(Nova_ReadBuffer* b) {
    uint8_t out;
    if (!_nova_rb_read_u8_raw(b, &out)) return _nova_rb_make_err(1, b->len - b->pos);
    return nova_make_Result_Ok((nova_int)(nova_byte)out);
}
static inline Nova_Result* Nova_ReadBuffer_method_try_read_bytes(Nova_ReadBuffer* b, nova_int n) {
    if (n < 0 || (b->len - b->pos) < n) return _nova_rb_make_err(n, b->len - b->pos);
    NovaArray_nova_byte* arr = (NovaArray_nova_byte*)nova_alloc(sizeof(NovaArray_nova_byte));
    arr->cap = n > 0 ? n : 8;
    arr->len = n;
    arr->data = (nova_byte*)nova_alloc((size_t)arr->cap);
    if (n > 0) memcpy(arr->data, b->data + b->pos, (size_t)n);
    b->pos += n;
    return nova_make_Result_Ok((nova_int)(intptr_t)arr);
}
static inline Nova_Result* Nova_ReadBuffer_method_try_read_u8(Nova_ReadBuffer* b) {
    uint8_t out;
    if (!_nova_rb_read_u8_raw(b, &out)) return _nova_rb_make_err(1, b->len - b->pos);
    return nova_make_Result_Ok((nova_int)(nova_byte)out);
}
static inline Nova_Result* Nova_ReadBuffer_method_try_read_i8(Nova_ReadBuffer* b) {
    uint8_t out;
    if (!_nova_rb_read_u8_raw(b, &out)) return _nova_rb_make_err(1, b->len - b->pos);
    return nova_make_Result_Ok((nova_int)(int8_t)out);
}

#define NOVA_RB_DEFINE_TRY_READ(WIDTH, ENDIAN, SIGNED, RET_CAST) \
    static inline Nova_Result* Nova_ReadBuffer_method_try_read_##SIGNED##WIDTH##_##ENDIAN(Nova_ReadBuffer* b) { \
        uint##WIDTH##_t out; \
        if (!_nova_rb_read_u##WIDTH##_##ENDIAN##_raw(b, &out)) return _nova_rb_make_err(WIDTH/8, b->len - b->pos); \
        return nova_make_Result_Ok(RET_CAST out); \
    }

NOVA_RB_DEFINE_TRY_READ(16, le, u, (nova_int))
NOVA_RB_DEFINE_TRY_READ(16, be, u, (nova_int))
NOVA_RB_DEFINE_TRY_READ(32, le, u, (nova_int))
NOVA_RB_DEFINE_TRY_READ(32, be, u, (nova_int))
NOVA_RB_DEFINE_TRY_READ(64, le, u, (nova_int))
NOVA_RB_DEFINE_TRY_READ(64, be, u, (nova_int))

NOVA_RB_DEFINE_TRY_READ(16, le, i, (nova_int)(int16_t))
NOVA_RB_DEFINE_TRY_READ(16, be, i, (nova_int)(int16_t))
NOVA_RB_DEFINE_TRY_READ(32, le, i, (nova_int)(int32_t))
NOVA_RB_DEFINE_TRY_READ(32, be, i, (nova_int)(int32_t))
NOVA_RB_DEFINE_TRY_READ(64, le, i, (nova_int)(int64_t))
NOVA_RB_DEFINE_TRY_READ(64, be, i, (nova_int)(int64_t))

/* f32/f64 try-form: bit-cast double → int64 для упаковки в Result.Ok(nova_int). */
static inline nova_int _nova_f64_to_bits(nova_f64 d) {
    uint64_t u;
    memcpy(&u, &d, 8);
    return (nova_int)u;
}

static inline Nova_Result* Nova_ReadBuffer_method_try_read_f32_le(Nova_ReadBuffer* b) {
    uint32_t out;
    if (!_nova_rb_read_u32_le_raw(b, &out)) return _nova_rb_make_err(4, b->len - b->pos);
    float f;
    memcpy(&f, &out, 4);
    return nova_make_Result_Ok(_nova_f64_to_bits((nova_f64)f));
}
static inline Nova_Result* Nova_ReadBuffer_method_try_read_f32_be(Nova_ReadBuffer* b) {
    uint32_t out;
    if (!_nova_rb_read_u32_be_raw(b, &out)) return _nova_rb_make_err(4, b->len - b->pos);
    float f;
    memcpy(&f, &out, 4);
    return nova_make_Result_Ok(_nova_f64_to_bits((nova_f64)f));
}
static inline Nova_Result* Nova_ReadBuffer_method_try_read_f64_le(Nova_ReadBuffer* b) {
    uint64_t out;
    if (!_nova_rb_read_u64_le_raw(b, &out)) return _nova_rb_make_err(8, b->len - b->pos);
    nova_f64 d;
    memcpy(&d, &out, 8);
    return nova_make_Result_Ok(_nova_f64_to_bits(d));
}
static inline Nova_Result* Nova_ReadBuffer_method_try_read_f64_be(Nova_ReadBuffer* b) {
    uint64_t out;
    if (!_nova_rb_read_u64_be_raw(b, &out)) return _nova_rb_make_err(8, b->len - b->pos);
    nova_f64 d;
    memcpy(&d, &out, 8);
    return nova_make_Result_Ok(_nova_f64_to_bits(d));
}

#endif /* NOVA_RT_READ_BUFFER_H */
