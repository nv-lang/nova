#ifndef NOVA_RT_STRING_BUILDER_H
#define NOVA_RT_STRING_BUILDER_H

/* ---- Nova StringBuilder — UTF-8 string accumulator (Plan 04) ----
 *
 * Append-only текстовый builder. Принимает только str / char — UTF-8
 * invariant поддерживается типом, поэтому @into() -> str infallible.
 *
 * Capacity-grow: 2x при переполнении. Initial capacity — 16 байт.
 *
 * Plan 73 (D131): `@into()` помечен `consume` — use-after-@into ловится
 * компилятором (compile error). Runtime-флаг `consumed` удалён; вместо
 * него `@into()` зануляет `data`/`len`/`cap`, а `_nova_string_builder_
 * check_live` — defense-in-depth assert (`data != NULL`): если статическая
 * проверка обойдена, доступ fail-fast'ит с понятным сообщением, а не
 * молча портит данные.
 *
 * См. spec/decisions/08-runtime.md → D26 (prelude), D82 (external fn),
 * spec/decisions/05-memory.md → D131 (consume), Q-string-builder.
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
} Nova_StringBuilder;

static inline Nova_StringBuilder* Nova_StringBuilder_static_new(void) {
    Nova_StringBuilder* b = (Nova_StringBuilder*)nova_alloc(sizeof(Nova_StringBuilder));
    b->data = (nova_byte*)nova_alloc(NOVA_STRING_BUILDER_INIT_CAP);
    b->len = 0;
    b->cap = NOVA_STRING_BUILDER_INIT_CAP;
    return b;
}

static inline Nova_StringBuilder* Nova_StringBuilder_static_with_capacity(nova_int n) {
    Nova_StringBuilder* b = (Nova_StringBuilder*)nova_alloc(sizeof(Nova_StringBuilder));
    int64_t cap = n > 0 ? (int64_t)n : NOVA_STRING_BUILDER_INIT_CAP;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    b->len = 0;
    b->cap = cap;
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

/* Plan 73 (D131): defense-in-depth liveness guard. Use-after-@into() —
 * compile error (D131 consume-check); этот assert ловит случай, когда
 * статическая проверка обойдена (`@into()` зануляет `data`). Fail-fast
 * с понятным сообщением вместо тихой порчи / null-deref. */
static inline void _nova_string_builder_check_live(Nova_StringBuilder* b) {
    nova_assert(b->data != NULL,
        "StringBuilder use-after-@into(): значение потреблено (D131 consume)");
}

/* StringBuilder.from(s str). */
static inline Nova_StringBuilder* Nova_StringBuilder_static_from_str(nova_str s) {
    Nova_StringBuilder* b = (Nova_StringBuilder*)nova_alloc(sizeof(Nova_StringBuilder));
    int64_t cap = s.len > 0 ? (int64_t)s.len : NOVA_STRING_BUILDER_INIT_CAP;
    b->data = (nova_byte*)nova_alloc((size_t)cap);
    if (s.len > 0) memcpy(b->data, s.ptr, s.len);
    b->len = (int64_t)s.len;
    b->cap = cap;
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

/* @byte_len() -> int — текущий размер в UTF-8 байтах. O(1).
 * Поле `len` в struct хранит именно байты (как `s.byte_len()` в str). */
static inline nova_int Nova_StringBuilder_method_byte_len(Nova_StringBuilder* b) {
    return (nova_int)b->len;
}

/* @len() -> int — codepoint count (UTF-8 walk). O(n).
 * D26 школа B: единая семантика @len для всех текстовых типов.
 * Для O(1) byte size — @byte_len(). */
static inline nova_int Nova_StringBuilder_method_len(Nova_StringBuilder* b) {
    nova_int count = 0;
    int64_t i = 0;
    while (i < b->len) {
        nova_byte c = b->data[i];
        int step;
        if (c < 0x80)              step = 1;
        else if ((c & 0xE0) == 0xC0) step = 2;
        else if ((c & 0xF0) == 0xE0) step = 3;
        else if ((c & 0xF8) == 0xF0) step = 4;
        else                          step = 1; /* invalid lead — count as one to make progress */
        i += step;
        count++;
    }
    return count;
}

/* @capacity() -> int — allocated capacity в байтах (>= byte_len). */
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
    return b;
}

/* @into() -> str — INFALLIBLE (UTF-8 invariant поддерживается типом).
 * Transfers ownership: reuse b->data as the new str's backing.
 * Plan 73 (D131): `consume @into` — после вызова StringBuilder
 * недоступен. Зануляем data/len/cap: повторный доступ → fail-fast
 * через `_nova_string_builder_check_live` (defense-in-depth, если
 * compile-time consume-check обойдён). */
static inline nova_str Nova_StringBuilder_method_into(Nova_StringBuilder* b) {
    _nova_string_builder_check_live(b);
    nova_str s = (nova_str){
        .ptr = (const char*)b->data,
        .len = (size_t)b->len,
    };
    b->data = NULL;
    b->len  = 0;
    b->cap  = 0;
    return s;
}

/* Plan 100.6 D164 / Plan 103.9 D174: consume-receiver ABI alias.
 * Emit_c.rs generates Nova_StringBuilder_consume_into for `consume @into()`
 * calls coming through method dispatch (D164 consume-bit naming). The C
 * implementation is identical to method_into — just an extra symbol. */
static inline nova_str Nova_StringBuilder_consume_into(Nova_StringBuilder* b) {
    return Nova_StringBuilder_method_into(b);
}

/* @starts_with(prefix str) -> bool — non-consuming читает буфер.
 * O(min(|prefix|, |buf|)) memcmp. */
static inline nova_bool Nova_StringBuilder_method_starts_with(Nova_StringBuilder* b, nova_str prefix) {
    _nova_string_builder_check_live(b);
    if ((int64_t)prefix.len > b->len) return 0;
    if (prefix.len == 0) return 1;
    return memcmp(b->data, prefix.ptr, prefix.len) == 0 ? 1 : 0;
}

/* @ends_with(suffix str) -> bool — non-consuming читает буфер.
 * O(min(|suffix|, |buf|)) memcmp. */
static inline nova_bool Nova_StringBuilder_method_ends_with(Nova_StringBuilder* b, nova_str suffix) {
    _nova_string_builder_check_live(b);
    if ((int64_t)suffix.len > b->len) return 0;
    if (suffix.len == 0) return 1;
    int64_t offset = b->len - (int64_t)suffix.len;
    return memcmp(b->data + offset, suffix.ptr, suffix.len) == 0 ? 1 : 0;
}

/* @is_empty() -> bool — non-consuming, O(1). */
static inline nova_bool Nova_StringBuilder_method_is_empty(Nova_StringBuilder* b) {
    _nova_string_builder_check_live(b);
    return b->len == 0 ? 1 : 0;
}

/* @peek() -> str — non-consuming snapshot буфера как str.
 * ВАЖНО: pointer указывает на тот же buffer что и StringBuilder;
 * subsequent append'ы могут invalidate (realloc). Использовать только
 * для immediate read'а (sb.peek().ends_with(...)). */
static inline nova_str Nova_StringBuilder_method_peek(Nova_StringBuilder* b) {
    _nova_string_builder_check_live(b);
    return (nova_str){
        .ptr = (const char*)b->data,
        .len = (size_t)b->len,
    };
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

/* Plan 91 Ф.2: @append_repeat(s str, n int) -> Self
 * Append строку s ровно n раз. n <= 0 → no-op. O(n * |s|) одним
 * reserve + n*memcpy — эффективнее чем n вызовов append_str.
 * Используется в Nova-реализации str @repeat и str @pad_left/pad_right. */
static inline Nova_StringBuilder* Nova_StringBuilder_method_append_repeat(
        Nova_StringBuilder* b, nova_str s, nova_int n) {
    _nova_string_builder_check_live(b);
    if (n <= 0 || s.len == 0) return b;
    _nova_string_builder_reserve(b, (int64_t)n * (int64_t)s.len);
    for (nova_int i = 0; i < n; i++) {
        memcpy(b->data + b->len, s.ptr, s.len);
        b->len += (int64_t)s.len;
    }
    return b;
}

/* Plan 91 Ф.2: @truncate(len int) -> Self
 * Обрезать буфер до `len` байт. Если len >= byte_len — no-op.
 * len < 0 → трактуется как 0 (пустой буфер). Не realloc'ает.
 * ВАЖНО: truncate по байтам, не codepoints — caller отвечает за
 * то что граница не рвёт UTF-8 sequence. */
static inline Nova_StringBuilder* Nova_StringBuilder_method_truncate(
        Nova_StringBuilder* b, nova_int len) {
    _nova_string_builder_check_live(b);
    if (len < 0) len = 0;
    if ((int64_t)len < b->len) b->len = (int64_t)len;
    return b;
}

/* Plan 91 Ф.2: @append_bytes(arr []u8) -> Self
 * Append raw bytes из []u8. Caller отвечает за UTF-8 validity —
 * no validation performed (zero-copy path для performance-critical ops). */
static inline Nova_StringBuilder* Nova_StringBuilder_method_append_bytes(
        Nova_StringBuilder* b, NovaArray_nova_byte* arr) {
    _nova_string_builder_check_live(b);
    if (!arr || arr->len == 0) return b;
    _nova_string_builder_reserve(b, arr->len);
    memcpy(b->data + b->len, arr->data, (size_t)arr->len);
    b->len += arr->len;
    return b;
}

/* Plan 91 Ф.2: str @repeat / @replace / @pad_left / @pad_right.
 * Nova-first: nova_body в runtime_registry + Nova body в string.nv — авторитетная spec.
 * C-реализации здесь зеркалят Nova-семантику (bootstrap; C codegen использует c_name).
 * parse_int_radix — C-bootstrap в array.h (зеркало Nova body в string.nv). */

static inline nova_str nova_str_repeat(nova_str s, nova_int n) {
    if (n <= 0 || s.len == 0) return (nova_str){ "", 0 };
    Nova_StringBuilder* sb = Nova_StringBuilder_static_with_capacity((nova_int)((int64_t)n * (int64_t)s.len));
    Nova_StringBuilder_method_append_repeat(sb, s, n);
    return Nova_StringBuilder_method_into(sb);
}

static inline nova_str nova_str_replace(nova_str s, nova_str from, nova_str to) {
    if (from.len == 0 || s.len == 0) return s;
    size_t count = 0, i = 0;
    while (i + from.len <= s.len) {
        if (memcmp(s.ptr + i, from.ptr, from.len) == 0) { count++; i += from.len; }
        else i++;
    }
    if (count == 0) return s;
    size_t out_len = s.len - count * from.len + count * to.len;
    Nova_StringBuilder* sb = Nova_StringBuilder_static_with_capacity((nova_int)(out_len + 1));
    size_t src = 0;
    while (src + from.len <= s.len) {
        if (memcmp(s.ptr + src, from.ptr, from.len) == 0) {
            Nova_StringBuilder_method_append_str(sb, to);
            src += from.len;
        } else {
            Nova_StringBuilder_method_append_str(sb, (nova_str){ s.ptr + src, 1 });
            src++;
        }
    }
    if (src < s.len)
        Nova_StringBuilder_method_append_str(sb, (nova_str){ s.ptr + src, s.len - src });
    return Nova_StringBuilder_method_into(sb);
}

static inline nova_str nova_str_pad_left(nova_str s, nova_int width, nova_int fill) {
    nova_int pad = width - nova_str_char_len(s);
    if (pad <= 0) return s;
    /* UTF-8 encode fill codepoint inline (1-4 байта). */
    nova_byte fill_buf[4];
    int fill_len = _nova_utf8_encode(fill_buf, fill);
    nova_str fill_str = { (const char*)fill_buf, (size_t)fill_len };
    /* capacity = s.byte_len + pad * fill_byte_len (точный, no realloc). */
    Nova_StringBuilder* sb = Nova_StringBuilder_static_with_capacity(
        (nova_int)s.len + pad * (nova_int)fill_len);
    Nova_StringBuilder_method_append_repeat(sb, fill_str, pad);
    Nova_StringBuilder_method_append_str(sb, s);
    return Nova_StringBuilder_method_into(sb);
}

static inline nova_str nova_str_pad_right(nova_str s, nova_int width, nova_int fill) {
    nova_int pad = width - nova_str_char_len(s);
    if (pad <= 0) return s;
    nova_byte fill_buf[4];
    int fill_len = _nova_utf8_encode(fill_buf, fill);
    nova_str fill_str = { (const char*)fill_buf, (size_t)fill_len };
    Nova_StringBuilder* sb = Nova_StringBuilder_static_with_capacity(
        (nova_int)s.len + pad * (nova_int)fill_len);
    Nova_StringBuilder_method_append_str(sb, s);
    Nova_StringBuilder_method_append_repeat(sb, fill_str, pad);
    return Nova_StringBuilder_method_into(sb);
}

/* str.from_bytes_unchecked(bytes readonly []u8) -> str.
 * Copies bytes without UTF-8 validation. Caller guarantees valid UTF-8.
 * Zero overhead: O(n) memcpy only. */
static inline nova_str nova_str_from_bytes_unchecked(NovaArray_nova_byte* arr) {
    char* buf = (char*)nova_alloc((size_t)arr->len + 1);
    if (arr->len > 0) memcpy(buf, arr->data, (size_t)arr->len);
    buf[arr->len] = '\0';
    return (nova_str){.ptr = buf, .len = (size_t)arr->len};
}

/* str.from_bytes_lossy(bytes readonly []u8) -> str.
 * Converts bytes to str, replacing invalid UTF-8 sequences with U+FFFD
 * (UTF-8: 0xEF 0xBF 0xBD). Output may be larger than input. */
static inline nova_str nova_str_from_bytes_lossy(NovaArray_nova_byte* arr) {
    static const nova_byte FFFD[3] = {0xEF, 0xBF, 0xBD};
    /* Pre-scan: if all valid, skip extra allocation. */
    if (_nova_validate_utf8(arr->data, arr->len)) {
        return nova_str_from_bytes_unchecked(arr);
    }
    /* Worst case: every byte replaced by 3-byte FFFD. */
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
        /* Validate continuation bytes. */
        int valid = (seq > 0);
        if (valid) {
            for (int k = 1; k < seq && valid; k++) {
                if (i + k >= arr->len || (arr->data[i + k] & 0xC0) != 0x80) valid = 0;
            }
        }
        /* Extra overlong/surrogate checks. */
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
