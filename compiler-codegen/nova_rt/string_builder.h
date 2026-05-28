#ifndef NOVA_RT_STRING_BUILDER_H
#define NOVA_RT_STRING_BUILDER_H

/* ---- Nova StringBuilder — Plan 109 (D179) ----
 *
 * StringBuilder is now a Nova-defined consume type:
 *   type StringBuilder consume { mut buf []u8 }
 *
 * The Nova_StringBuilder struct is emitted by the Nova codegen (not here).
 * This header retains only utility helpers used by str methods and
 * str.from_bytes_* that do NOT depend on the old StringBuilder layout.
 *
 * Old C-based implementation (Nova_StringBuilder_static_new, _method_append_str,
 * etc.) removed — all StringBuilder methods now implemented in Nova
 * (std/runtime/string_builder.nv).
 */

#include "alloc.h"
#include "nova_rt.h"
#include <stdint.h>
#include <string.h>

/* Validate UTF-8 bytes. Returns 1 if valid, 0 otherwise.
 * Used by str.from_bytes_lossy and Nova_str_static_try_from_bytes. */
static inline nova_bool _nova_validate_utf8(const nova_byte* data, int64_t len) {
    int64_t i = 0;
    while (i < len) {
        nova_byte c = data[i];
        if (c < 0x80) {
            i++;
        } else if ((c & 0xE0) == 0xC0) {
            if (i + 1 >= len) return 0;
            if ((data[i + 1] & 0xC0) != 0x80) return 0;
            if (c < 0xC2) return 0;
            i += 2;
        } else if ((c & 0xF0) == 0xE0) {
            if (i + 2 >= len) return 0;
            if ((data[i + 1] & 0xC0) != 0x80) return 0;
            if ((data[i + 2] & 0xC0) != 0x80) return 0;
            if (c == 0xE0 && data[i + 1] < 0xA0) return 0;
            if (c == 0xED && data[i + 1] >= 0xA0) return 0;
            i += 3;
        } else if ((c & 0xF8) == 0xF0) {
            if (i + 3 >= len) return 0;
            if ((data[i + 1] & 0xC0) != 0x80) return 0;
            if ((data[i + 2] & 0xC0) != 0x80) return 0;
            if ((data[i + 3] & 0xC0) != 0x80) return 0;
            if (c == 0xF0 && data[i + 1] < 0x90) return 0;
            if (c == 0xF4 && data[i + 1] >= 0x90) return 0;
            if (c > 0xF4) return 0;
            i += 4;
        } else {
            return 0;
        }
    }
    return 1;
}

/* Encode codepoint as UTF-8 bytes into dst. Returns byte count (1-4) or 0 if invalid. */
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
    return 0;
}

/* str.from(c char) — UTF-8 encode 1-4 bytes from codepoint. */
static inline nova_str Nova_str_static_from_char(nova_int cp) {
    nova_byte tmp[4];
    int n = _nova_utf8_encode(tmp, cp);
    if (n == 0) {
        return (nova_str){.ptr = "", .len = 0};
    }
    char* buf = (char*)nova_alloc((size_t)n + 1);
    memcpy(buf, tmp, (size_t)n);
    buf[n] = '\0';
    return (nova_str){.ptr = buf, .len = (size_t)n};
}

/* str.from_bytes_unchecked(bytes readonly []u8) -> str.
 * O(n) copy. Caller guarantees valid UTF-8. */
static inline nova_str nova_str_from_bytes_unchecked(NovaArray_nova_byte* arr) {
    char* buf = (char*)nova_alloc((size_t)arr->len + 1);
    if (arr->len > 0) memcpy(buf, arr->data, (size_t)arr->len);
    buf[arr->len] = '\0';
    return (nova_str){.ptr = buf, .len = (size_t)arr->len};
}

/* str.from_bytes_lossy(bytes readonly []u8) -> str.
 * Replaces invalid UTF-8 sequences with U+FFFD. */
static inline nova_str nova_str_from_bytes_lossy(NovaArray_nova_byte* arr) {
    static const nova_byte FFFD[3] = {0xEF, 0xBF, 0xBD};
    if (_nova_validate_utf8(arr->data, arr->len)) {
        return nova_str_from_bytes_unchecked(arr);
    }
    int64_t cap = arr->len * 3 + 1;
    char* out = (char*)nova_alloc((size_t)cap);
    int64_t w = 0, i = 0;
    while (i < arr->len) {
        nova_byte c = arr->data[i];
        int seq = 0;
        if (c < 0x80) { seq = 1; }
        else if ((c & 0xE0) == 0xC0 && c >= 0xC2) { seq = 2; }
        else if ((c & 0xF0) == 0xE0) { seq = 3; }
        else if ((c & 0xF8) == 0xF0 && c <= 0xF4) { seq = 4; }
        int valid = (seq > 0);
        if (valid) {
            for (int k = 1; k < seq && valid; k++) {
                if (i + k >= arr->len || (arr->data[i + k] & 0xC0) != 0x80) valid = 0;
            }
        }
        if (valid && seq == 2 && c < 0xC2) valid = 0;
        if (valid && seq == 3) {
            if (c == 0xE0 && arr->data[i+1] < 0xA0) valid = 0;
            else if (c == 0xED && arr->data[i+1] >= 0xA0) valid = 0;
        }
        if (valid && seq == 4) {
            if (c == 0xF0 && arr->data[i+1] < 0x90) valid = 0;
            else if (c == 0xF4 && arr->data[i+1] >= 0x90) valid = 0;
        }
        if (valid) {
            for (int k = 0; k < seq; k++) out[w++] = (char)arr->data[i + k];
            i += seq;
        } else {
            out[w++] = (char)FFFD[0];
            out[w++] = (char)FFFD[1];
            out[w++] = (char)FFFD[2];
            i++;
        }
    }
    out[w] = '\0';
    return (nova_str){.ptr = out, .len = (size_t)w};
}

/* Helper: box nova_str into heap-allocated nova_str* for Result.Ok payload. */
static inline void* nova_box_str(nova_str s) {
    nova_str* p = (nova_str*)nova_alloc(sizeof(nova_str));
    *p = s;
    return (void*)p;
}

/* str.try_from([]byte) -> Result[str, ParseStrError]. */
static inline Nova_Result* Nova_str_static_try_from_bytes(NovaArray_nova_byte* arr) {
    if (!_nova_validate_utf8(arr->data, arr->len)) {
        return nova_make_Result_Err((nova_str){
            .ptr = "invalid UTF-8 byte sequence",
            .len = 26,
        });
    }
    char* buf = (char*)nova_alloc((size_t)arr->len + 1);
    if (arr->len > 0) memcpy(buf, arr->data, (size_t)arr->len);
    buf[arr->len] = '\0';
    nova_str s = (nova_str){.ptr = buf, .len = (size_t)arr->len};
    return nova_make_Result_Ok((nova_int)(intptr_t)nova_box_str(s));
}

/* nova_str_replace — pure C helper for str.replace bootstrap.
 * Used only when Nova-body dispatch is unavailable (fallback). */
static inline nova_str nova_str_replace(nova_str s, nova_str from, nova_str to) {
    if (from.len == 0 || s.len == 0) return s;
    size_t count = 0, i = 0;
    while (i + from.len <= s.len) {
        if (memcmp(s.ptr + i, from.ptr, from.len) == 0) { count++; i += from.len; }
        else i++;
    }
    if (count == 0) return s;
    size_t out_len = s.len - count * from.len + count * to.len;
    char* out = (char*)nova_alloc(out_len + 1);
    size_t w = 0, src = 0;
    while (src + from.len <= s.len) {
        if (memcmp(s.ptr + src, from.ptr, from.len) == 0) {
            memcpy(out + w, to.ptr, to.len); w += to.len; src += from.len;
        } else {
            out[w++] = s.ptr[src++];
        }
    }
    while (src < s.len) out[w++] = s.ptr[src++];
    out[w] = '\0';
    return (nova_str){.ptr = out, .len = out_len};
}

#endif /* NOVA_RT_STRING_BUILDER_H */
