#ifndef NOVA_RT_BUFFER_H
#define NOVA_RT_BUFFER_H

/* ---- Nova Buffer — mutable byte accumulator (Q-buffer) ----
 *
 * Один тип для двух use-case'ов:
 *   - StringBuilder pattern: аккумулировать str в hot loop, финализировать
 *     в str через @try_into() (UTF-8 валидация) или @into_str_unchecked().
 *   - Bytes-buffer: аккумулировать []byte для бинарных протоколов,
 *     финализировать через @into() -> []byte.
 *
 * Capacity-grow: 2x при переполнении. Initial capacity — 16 байт.
 *
 * После consume (any @into / @try_into / @into_str_unchecked) флаг
 * `consumed = 1`. Любой @add_* на consumed buffer → nova_assert
 * "buffer consumed". См. Q-buffer в spec/open-questions.md.
 */

#include "alloc.h"
#include "nova_rt.h"
#include <stdint.h>
#include <string.h>

#define NOVA_BUFFER_INIT_CAP 16

typedef struct Nova_Buffer {
    nova_byte* data;
    int64_t    len;
    int64_t    cap;
    nova_bool  consumed;
} Nova_Buffer;

static inline Nova_Buffer* Nova_Buffer_static_new(void) {
    Nova_Buffer* b = (Nova_Buffer*)nova_alloc(sizeof(Nova_Buffer));
    b->data = (nova_byte*)nova_alloc(NOVA_BUFFER_INIT_CAP);
    b->len = 0;
    b->cap = NOVA_BUFFER_INIT_CAP;
    b->consumed = 0;
    return b;
}

static inline Nova_Buffer* Nova_Buffer_static_with_capacity(nova_int n) {
    Nova_Buffer* b = (Nova_Buffer*)nova_alloc(sizeof(Nova_Buffer));
    int64_t cap = n > 0 ? (int64_t)n : NOVA_BUFFER_INIT_CAP;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    b->len = 0;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

/* Grow capacity to fit at least len + extra bytes. 2x grow strategy. */
static inline void _nova_buffer_reserve(Nova_Buffer* b, int64_t extra) {
    int64_t need = b->len + extra;
    if (need <= b->cap) return;
    int64_t new_cap = b->cap;
    while (new_cap < need) new_cap *= 2;
    nova_byte* new_data = (nova_byte*)nova_alloc((size_t)new_cap);
    memcpy(new_data, b->data, (size_t)b->len);
    b->data = new_data;
    b->cap = new_cap;
}

static inline void _nova_buffer_check_live(Nova_Buffer* b) {
    nova_assert(!b->consumed, "buffer consumed: cannot mutate after @into/@try_into");
}

/* Buffer.from(s str) — copy of UTF-8 bytes. */
static inline Nova_Buffer* Nova_Buffer_static_from_str(nova_str s) {
    Nova_Buffer* b = (Nova_Buffer*)nova_alloc(sizeof(Nova_Buffer));
    int64_t cap = s.len > 0 ? (int64_t)s.len : NOVA_BUFFER_INIT_CAP;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    if (s.len > 0) memcpy(b->data, s.ptr, s.len);
    b->len = (int64_t)s.len;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

/* Buffer.from(b []byte) — copy of bytes. */
static inline Nova_Buffer* Nova_Buffer_static_from_bytes(NovaArray_nova_int* arr) {
    Nova_Buffer* b = (Nova_Buffer*)nova_alloc(sizeof(Nova_Buffer));
    int64_t cap = arr->len > 0 ? arr->len : NOVA_BUFFER_INIT_CAP;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    /* arr->data has nova_int (int64_t) elements storing byte values; copy
     * the low byte of each into the buffer. Kept for Nova-side []byte
     * compatibility — Nova represents []byte as []int currently (D27 not
     * yet differentiating byte arrays). */
    for (int64_t i = 0; i < arr->len; i++) {
        b->data[i] = (nova_byte)(arr->data[i] & 0xFF);
    }
    b->len = arr->len;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

/* @add_str(s str) — append UTF-8 bytes. */
static inline nova_unit Nova_Buffer_method_add_str(Nova_Buffer* b, nova_str s) {
    _nova_buffer_check_live(b);
    if (s.len == 0) return NOVA_UNIT;
    _nova_buffer_reserve(b, (int64_t)s.len);
    memcpy(b->data + b->len, s.ptr, s.len);
    b->len += (int64_t)s.len;
    return NOVA_UNIT;
}

/* @add_bytes(b []byte) — append bytes from a Nova []byte (stored as []int). */
static inline nova_unit Nova_Buffer_method_add_bytes(Nova_Buffer* buf, NovaArray_nova_int* arr) {
    _nova_buffer_check_live(buf);
    if (arr->len == 0) return NOVA_UNIT;
    _nova_buffer_reserve(buf, arr->len);
    for (int64_t i = 0; i < arr->len; i++) {
        buf->data[buf->len + i] = (nova_byte)(arr->data[i] & 0xFF);
    }
    buf->len += arr->len;
    return NOVA_UNIT;
}

/* @add_byte(b byte) — append single byte. */
static inline nova_unit Nova_Buffer_method_add_byte(Nova_Buffer* buf, nova_int byte_val) {
    _nova_buffer_check_live(buf);
    _nova_buffer_reserve(buf, 1);
    buf->data[buf->len++] = (nova_byte)(byte_val & 0xFF);
    return NOVA_UNIT;
}

/* @add_char(c char) — UTF-8 encode 1-4 bytes.
 * char in Nova bootstrap is represented as nova_int (Unicode codepoint). */
static inline nova_unit Nova_Buffer_method_add_char(Nova_Buffer* buf, nova_int cp) {
    _nova_buffer_check_live(buf);
    if (cp < 0) return NOVA_UNIT;
    if (cp < 0x80) {
        _nova_buffer_reserve(buf, 1);
        buf->data[buf->len++] = (nova_byte)cp;
    } else if (cp < 0x800) {
        _nova_buffer_reserve(buf, 2);
        buf->data[buf->len++] = (nova_byte)(0xC0 | (cp >> 6));
        buf->data[buf->len++] = (nova_byte)(0x80 | (cp & 0x3F));
    } else if (cp < 0x10000) {
        _nova_buffer_reserve(buf, 3);
        buf->data[buf->len++] = (nova_byte)(0xE0 | (cp >> 12));
        buf->data[buf->len++] = (nova_byte)(0x80 | ((cp >> 6) & 0x3F));
        buf->data[buf->len++] = (nova_byte)(0x80 | (cp & 0x3F));
    } else if (cp < 0x110000) {
        _nova_buffer_reserve(buf, 4);
        buf->data[buf->len++] = (nova_byte)(0xF0 | (cp >> 18));
        buf->data[buf->len++] = (nova_byte)(0x80 | ((cp >> 12) & 0x3F));
        buf->data[buf->len++] = (nova_byte)(0x80 | ((cp >> 6) & 0x3F));
        buf->data[buf->len++] = (nova_byte)(0x80 | (cp & 0x3F));
    }
    /* cp >= 0x110000 — invalid Unicode codepoint, silently dropped.
     * (Could nova_assert; choosing lenient behaviour for now.) */
    return NOVA_UNIT;
}

/* @len() -> int. */
static inline nova_int Nova_Buffer_method_len(Nova_Buffer* b) {
    return (nova_int)b->len;
}

/* @capacity() -> int. */
static inline nova_int Nova_Buffer_method_capacity(Nova_Buffer* b) {
    return (nova_int)b->cap;
}

/* @clone() -> Buffer — deep copy of byte buffer. New buffer is not consumed
 * even if the source was; @clone() is a snapshot operation. */
static inline Nova_Buffer* Nova_Buffer_method_clone(Nova_Buffer* src) {
    Nova_Buffer* b = (Nova_Buffer*)nova_alloc(sizeof(Nova_Buffer));
    int64_t cap = src->cap;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    if (src->len > 0) memcpy(b->data, src->data, (size_t)src->len);
    b->len = src->len;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

/* @into() -> []byte — consume and return contents as []byte.
 * Implemented as conversion to NovaArray_nova_int (Nova's []byte
 * representation). */
static inline NovaArray_nova_int* Nova_Buffer_method_into(Nova_Buffer* b) {
    _nova_buffer_check_live(b);
    NovaArray_nova_int* arr = (NovaArray_nova_int*)nova_alloc(sizeof(NovaArray_nova_int));
    arr->cap = b->len > 0 ? b->len : 8;
    arr->len = b->len;
    arr->data = (nova_int*)nova_alloc((size_t)arr->cap * sizeof(nova_int));
    for (int64_t i = 0; i < b->len; i++) {
        arr->data[i] = (nova_int)b->data[i];
    }
    b->consumed = 1;
    return arr;
}

/* Validate that buffer contents are valid UTF-8.
 * Returns 1 if valid, 0 otherwise. */
static inline nova_bool _nova_buffer_validate_utf8(const nova_byte* data, int64_t len) {
    int64_t i = 0;
    while (i < len) {
        nova_byte c = data[i];
        if (c < 0x80) {
            i++;
        } else if ((c & 0xE0) == 0xC0) {
            if (i + 1 >= len) return 0;
            if ((data[i + 1] & 0xC0) != 0x80) return 0;
            if (c < 0xC2) return 0;             /* overlong */
            i += 2;
        } else if ((c & 0xF0) == 0xE0) {
            if (i + 2 >= len) return 0;
            if ((data[i + 1] & 0xC0) != 0x80) return 0;
            if ((data[i + 2] & 0xC0) != 0x80) return 0;
            if (c == 0xE0 && data[i + 1] < 0xA0) return 0;  /* overlong */
            if (c == 0xED && data[i + 1] >= 0xA0) return 0; /* surrogate */
            i += 3;
        } else if ((c & 0xF8) == 0xF0) {
            if (i + 3 >= len) return 0;
            if ((data[i + 1] & 0xC0) != 0x80) return 0;
            if ((data[i + 2] & 0xC0) != 0x80) return 0;
            if ((data[i + 3] & 0xC0) != 0x80) return 0;
            if (c == 0xF0 && data[i + 1] < 0x90) return 0;  /* overlong */
            if (c == 0xF4 && data[i + 1] >= 0x90) return 0; /* > U+10FFFF */
            if (c > 0xF4) return 0;
            i += 4;
        } else {
            return 0;
        }
    }
    return 1;
}

/* @try_into() -> Result[str, Utf8Error]. Validates UTF-8, on success
 * transfers ownership of internal byte buffer to a freshly built nova_str.
 *
 * Bootstrap-shortcut: instead of returning a Nova `Result` value (which
 * would require codegen support for Result construction here), this
 * helper signals failure via Fail effect — Nova_Fail_fail with
 * "invalid UTF-8" message. On success returns the str directly.
 *
 * Programmer-side: `let s = buf.try_into()` either succeeds with str or
 * the surrounding `with Fail = handler` catches the error. This is
 * the same pattern as existing Nova_Fail dispatch (see effects.h). */
static inline nova_str Nova_Buffer_method_try_into(Nova_Buffer* b) {
    _nova_buffer_check_live(b);
    if (!_nova_buffer_validate_utf8(b->data, b->len)) {
        Nova_Fail_fail((nova_str){.ptr = "invalid UTF-8", .len = 13});
        /* unreachable */
        return (nova_str){.ptr = NULL, .len = 0};
    }
    /* Transfer ownership: reuse b->data as the new str's backing. */
    nova_str s = (nova_str){
        .ptr = (const char*)b->data,
        .len = (size_t)b->len,
    };
    b->consumed = 1;
    return s;
}

/* @into_str_unchecked() -> str — escape hatch. Skips UTF-8 validation.
 * Programmer guarantees content is valid (e.g. only @add_str / @add_char
 * were used). */
static inline nova_str Nova_Buffer_method_into_str_unchecked(Nova_Buffer* b) {
    _nova_buffer_check_live(b);
    nova_str s = (nova_str){
        .ptr = (const char*)b->data,
        .len = (size_t)b->len,
    };
    b->consumed = 1;
    return s;
}

#endif /* NOVA_RT_BUFFER_H */
