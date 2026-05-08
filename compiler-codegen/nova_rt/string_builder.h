#ifndef NOVA_RT_STRING_BUILDER_H
#define NOVA_RT_STRING_BUILDER_H

/* ---- Nova StringBuilder — UTF-8 string accumulator (Plan 04) ----
 *
 * Append-only текстовый builder. Принимает только str / char — UTF-8
 * invariant поддерживается типом, поэтому @into() -> str infallible.
 *
 * Capacity-grow: 2x при переполнении. Initial capacity — 16 байт.
 *
 * После consume (any @into / @clone'ом не считаем) флаг `consumed = 1`.
 * Любой @append на consumed buffer → nova_assert "string builder consumed".
 *
 * См. spec/decisions/08-runtime.md → D26 (prelude), D82 (external fn),
 * spec/open-questions.md → Q-string-builder.
 */

#include "alloc.h"
#include "nova_rt.h"
#include <stdint.h>
#include <string.h>

#define NOVA_STRING_BUILDER_INIT_CAP 16

typedef struct Nova_StringBuilder {
    nova_byte* data;
    int64_t    len;
    int64_t    cap;
    nova_bool  consumed;
} Nova_StringBuilder;

static inline Nova_StringBuilder* Nova_StringBuilder_static_new(void) {
    Nova_StringBuilder* b = (Nova_StringBuilder*)nova_alloc(sizeof(Nova_StringBuilder));
    b->data = (nova_byte*)nova_alloc(NOVA_STRING_BUILDER_INIT_CAP);
    b->len = 0;
    b->cap = NOVA_STRING_BUILDER_INIT_CAP;
    b->consumed = 0;
    return b;
}

static inline Nova_StringBuilder* Nova_StringBuilder_static_with_capacity(nova_int n) {
    Nova_StringBuilder* b = (Nova_StringBuilder*)nova_alloc(sizeof(Nova_StringBuilder));
    int64_t cap = n > 0 ? (int64_t)n : NOVA_STRING_BUILDER_INIT_CAP;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    b->len = 0;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

static inline void _nova_string_builder_reserve(Nova_StringBuilder* b, int64_t extra) {
    int64_t need = b->len + extra;
    if (need <= b->cap) return;
    int64_t new_cap = b->cap;
    while (new_cap < need) new_cap *= 2;
    nova_byte* new_data = (nova_byte*)nova_alloc((size_t)new_cap);
    memcpy(new_data, b->data, (size_t)b->len);
    b->data = new_data;
    b->cap = new_cap;
}

static inline void _nova_string_builder_check_live(Nova_StringBuilder* b) {
    nova_assert(!b->consumed, "string builder consumed: cannot mutate after @into");
}

/* StringBuilder.from(s str). */
static inline Nova_StringBuilder* Nova_StringBuilder_static_from_str(nova_str s) {
    Nova_StringBuilder* b = (Nova_StringBuilder*)nova_alloc(sizeof(Nova_StringBuilder));
    int64_t cap = s.len > 0 ? (int64_t)s.len : NOVA_STRING_BUILDER_INIT_CAP;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    if (s.len > 0) memcpy(b->data, s.ptr, s.len);
    b->len = (int64_t)s.len;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

/* Encode codepoint as UTF-8 в b->data + b->len. Возвращает количество записанных байт. */
static inline int _nova_utf8_encode(nova_byte* dst, nova_int cp) {
    if (cp < 0) return 0;
    if (cp < 0x80) {
        dst[0] = (nova_byte)cp;
        return 1;
    }
    if (cp < 0x800) {
        dst[0] = (nova_byte)(0xC0 | (cp >> 6));
        dst[1] = (nova_byte)(0x80 | (cp & 0x3F));
        return 2;
    }
    if (cp < 0x10000) {
        dst[0] = (nova_byte)(0xE0 | (cp >> 12));
        dst[1] = (nova_byte)(0x80 | ((cp >> 6) & 0x3F));
        dst[2] = (nova_byte)(0x80 | (cp & 0x3F));
        return 3;
    }
    if (cp < 0x110000) {
        dst[0] = (nova_byte)(0xF0 | (cp >> 18));
        dst[1] = (nova_byte)(0x80 | ((cp >> 12) & 0x3F));
        dst[2] = (nova_byte)(0x80 | ((cp >> 6) & 0x3F));
        dst[3] = (nova_byte)(0x80 | (cp & 0x3F));
        return 4;
    }
    return 0; /* invalid codepoint */
}

/* StringBuilder.from(c char) — UTF-8 encode 1-4 байта. */
static inline Nova_StringBuilder* Nova_StringBuilder_static_from_char(nova_int cp) {
    Nova_StringBuilder* b = (Nova_StringBuilder*)nova_alloc(sizeof(Nova_StringBuilder));
    b->data = (nova_byte*)nova_alloc(NOVA_STRING_BUILDER_INIT_CAP);
    b->cap = NOVA_STRING_BUILDER_INIT_CAP;
    b->consumed = 0;
    b->len = _nova_utf8_encode(b->data, cp);
    return b;
}

/* @append(s str) -> Self — copy UTF-8 bytes; returns self for chaining. */
static inline Nova_StringBuilder* Nova_StringBuilder_method_append_str(Nova_StringBuilder* b, nova_str s) {
    _nova_string_builder_check_live(b);
    if (s.len == 0) return b;
    _nova_string_builder_reserve(b, (int64_t)s.len);
    memcpy(b->data + b->len, s.ptr, s.len);
    b->len += (int64_t)s.len;
    return b;
}

/* @append(c char) -> Self — UTF-8 encode 1-4 bytes; returns self for chaining. */
static inline Nova_StringBuilder* Nova_StringBuilder_method_append_char(Nova_StringBuilder* b, nova_int cp) {
    _nova_string_builder_check_live(b);
    _nova_string_builder_reserve(b, 4);
    int n = _nova_utf8_encode(b->data + b->len, cp);
    b->len += n;
    return b;
}

/* @len() -> int. */
static inline nova_int Nova_StringBuilder_method_len(Nova_StringBuilder* b) {
    return (nova_int)b->len;
}

/* @capacity() -> int. */
static inline nova_int Nova_StringBuilder_method_capacity(Nova_StringBuilder* b) {
    return (nova_int)b->cap;
}

/* @clone() -> StringBuilder — deep copy. */
static inline Nova_StringBuilder* Nova_StringBuilder_method_clone(Nova_StringBuilder* src) {
    Nova_StringBuilder* b = (Nova_StringBuilder*)nova_alloc(sizeof(Nova_StringBuilder));
    int64_t cap = src->cap;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    if (src->len > 0) memcpy(b->data, src->data, (size_t)src->len);
    b->len = src->len;
    b->cap = cap;
    b->consumed = 0;
    return b;
}

/* @into() -> str — INFALLIBLE (UTF-8 invariant поддерживается типом).
 * Transfers ownership: reuse b->data as the new str's backing. */
static inline nova_str Nova_StringBuilder_method_into(Nova_StringBuilder* b) {
    _nova_string_builder_check_live(b);
    nova_str s = (nova_str){
        .ptr = (const char*)b->data,
        .len = (size_t)b->len,
    };
    b->consumed = 1;
    return s;
}

/* Validate UTF-8 bytes. Returns 1 if valid, 0 otherwise.
 * Используется в `str.try_from([]byte)` (D77). */
static inline nova_bool _nova_validate_utf8(const nova_byte* data, int64_t len) {
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

/* Helper: box nova_str в heap-allocated nova_str* для упаковки в
 * Result.Ok (boxed payload, читается через `let s = r.unwrap_or(...)`
 * в Nova-коде где Result-payload = nova_int slot). */
static inline void* nova_box_str(nova_str s) {
    nova_str* p = (nova_str*)nova_alloc(sizeof(nova_str));
    *p = s;
    return (void*)p;
}

/* str.try_from([]byte) -> Result[str, ParseStrError].
 * Validates UTF-8; на success возвращает Ok(str), иначе Err(msg).
 * Используется для финализации mixed text+binary в WriteBuffer →
 * str (через D77 try_from). */
static inline Nova_Result* Nova_str_static_try_from_bytes(NovaArray_nova_byte* arr) {
    if (!_nova_validate_utf8(arr->data, arr->len)) {
        return nova_make_Result_Err((nova_str){
            .ptr = "invalid UTF-8 byte sequence",
            .len = 26,
        });
    }
    /* Copy bytes into a fresh nul-terminated buffer (str-API expects
     * stable storage; arr->data может быть переиспользован). */
    char* buf = (char*)nova_alloc((size_t)arr->len + 1);
    if (arr->len > 0) memcpy(buf, arr->data, (size_t)arr->len);
    buf[arr->len] = '\0';
    nova_str s = (nova_str){.ptr = buf, .len = (size_t)arr->len};
    return nova_make_Result_Ok((nova_int)(intptr_t)nova_box_str(s));
}

/* str.from(c char) — UTF-8 encode 1-4 bytes из codepoint в новый nova_str.
 * Размещён здесь т.к. использует _nova_utf8_encode. */
static inline nova_str Nova_str_static_from_char(nova_int cp) {
    nova_byte tmp[4];
    int n = _nova_utf8_encode(tmp, cp);
    if (n == 0) {
        return (nova_str){.ptr = "", .len = 0};
    }
    /* Allocate len+1 (nul-terminated for C-interop, см. D26 nul-termination note). */
    char* buf = (char*)nova_alloc((size_t)n + 1);
    memcpy(buf, tmp, (size_t)n);
    buf[n] = '\0';
    return (nova_str){.ptr = buf, .len = (size_t)n};
}

#endif /* NOVA_RT_STRING_BUILDER_H */
