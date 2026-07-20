/* conv.h — D73/D77 prelude конверсии: str→numeric, char↔str,
 * []byte↔str, bool↔str.
 *
 * Plan 08 Ф.1: bootstrap-table в codegen эмитит вызовы этих helper'ов
 * при `T.try_from(v)` / `T.from(v)`.
 *
 * Все helper'ы возвращают `nova_*_result` структуры — чтобы вызывающий
 * мог построить `Result[T, E]` без cross-FFI.
 *
 * Не охвачено: int↔char range-check, byte↔char, sub-int range-check.
 * Эти случаи делаются inline в codegen (Plan 08 Ф.2).
 *
 * Plan 208 Ф.4R Ш4 (owner signal after Д3, docs/plans/208-unified-
 * formatter.md §10R): the Display/Debug primitive-formatting family that
 * used to live in this file (`nova_bool_to_str`/`nova_f64_to_str`/
 * `nova_f32_to_str`/`nova_char_to_str`, their `_to_debug_str` twins,
 * `nova_str_to_debug_str`/`nova_char_to_debug_str`, and the whole
 * `nova_fmt_int_body`/`nova_fmt_int_radix_body`/`nova_fmt_int_prefix`/
 * `nova_fmt_radix_prefix`/`nova_fmt_f64_body`/`nova_fmt_f64_prefix`/
 * `nova_fmt_str_precision` format-spec chain) has been RETIRED — every
 * caller (the interp fast path AND the rich format-spec lowering,
 * `compiler-codegen/src/codegen/emit_c.rs` `emit_interpolated_str`/
 * `emit_format_spec_value`) now renders through the `.nv` `*_display_spec`
 * family (`std/src/runtime/string_builder.nv`, Ф.4R Ш1) instead — the
 * single surviving carrier for primitive int/f64/f32/char/bool/str
 * Display+Debug rendering. Two fmt-family members remain here, each with a
 * REAL non-fmt caller (see their own comments below for the exact call
 * site): `nova_fmt_pad` (the composite/user-type rich-spec tail's external
 * width/align post-step — composites have no `*_display_spec` sibling of
 * their own yet) and its two helpers `nova_fmt_encode_fill`/
 * `nova_fmt_char_count`; `nova_ptr_to_debug_str` (pointer `${p:?}` hex-
 * address rendering — no `.nv` port exists, out of Ф.4R's primitive-family
 * scope). `nova_fmt_bytes_for_chars` was ONLY used by the now-removed
 * `nova_fmt_str_precision` and has no other caller — removed alongside it.
 */

#ifndef NOVA_CONV_H
#define NOVA_CONV_H

#include <stdint.h>
#include <stdbool.h>
#include <string.h>
#include <stdlib.h>
#include <ctype.h>

/* === Result-структуры для парсеров === */

typedef struct { nova_int  value; nova_bool ok; } nova_parse_int_result;
typedef struct { uint64_t  value; nova_bool ok; } nova_parse_u64_result;
typedef struct { double    value; nova_bool ok; } nova_parse_f64_result;
typedef struct { nova_bool value; nova_bool ok; } nova_parse_bool_result;
typedef struct { nova_int  value; nova_bool ok; } nova_char_decode_result;

/* === str → int (signed 64-bit) === */
/* Trim'ует ведущие пробелы. Принимает '+'/'-' префиксы.
 * Только десятичный (для hex/bin использовать отдельные парсеры). */
static inline nova_parse_int_result nova_str_to_i64(nova_str s) {
    nova_parse_int_result r = { 0, 0 };
    if (s.len == 0) return r;
    /* skip leading whitespace */
    size_t i = 0;
    while (i < s.len && (s.ptr[i] == ' ' || s.ptr[i] == '\t')) i++;
    if (i >= s.len) return r;
    int negative = 0;
    if (s.ptr[i] == '+' || s.ptr[i] == '-') {
        negative = (s.ptr[i] == '-');
        i++;
    }
    if (i >= s.len) return r;
    int64_t acc = 0;
    int any = 0;
    while (i < s.len) {
        char c = s.ptr[i];
        if (c < '0' || c > '9') return r;  /* invalid char — fail */
        /* overflow check: acc*10 + (c-'0') */
        int64_t digit = c - '0';
        if (acc > (INT64_MAX - digit) / 10) return r;  /* overflow */
        acc = acc * 10 + digit;
        any = 1;
        i++;
    }
    if (!any) return r;
    r.value = negative ? -acc : acc;
    r.ok = 1;
    return r;
}

/* === str → u64 === */
static inline nova_parse_u64_result nova_str_to_u64(nova_str s) {
    nova_parse_u64_result r = { 0, 0 };
    if (s.len == 0) return r;
    size_t i = 0;
    while (i < s.len && (s.ptr[i] == ' ' || s.ptr[i] == '\t')) i++;
    if (i >= s.len) return r;
    if (s.ptr[i] == '+') i++;
    if (i >= s.len) return r;
    uint64_t acc = 0;
    int any = 0;
    while (i < s.len) {
        char c = s.ptr[i];
        if (c < '0' || c > '9') return r;
        uint64_t digit = (uint64_t)(c - '0');
        if (acc > (UINT64_MAX - digit) / 10) return r;
        acc = acc * 10 + digit;
        any = 1;
        i++;
    }
    if (!any) return r;
    r.value = acc;
    r.ok = 1;
    return r;
}

/* === str → f64 === */
/* Делегирует strtod. NaN/Inf-литералы поддержаны strtod'ом
 * стандартно ("nan", "inf"). */
static inline nova_parse_f64_result nova_str_to_f64(nova_str s) {
    nova_parse_f64_result r = { 0.0, 0 };
    if (s.len == 0) return r;
    /* strtod ожидает null-terminated; копируем в стековый буфер
     * (для длинных строк — heap-fallback). */
    char stack_buf[64];
    char* buf = stack_buf;
    int allocated = 0;
    if (s.len + 1 > sizeof(stack_buf)) {
        buf = (char*)nova_alloc(s.len + 1);
        allocated = 1;
    }
    memcpy(buf, s.ptr, s.len);
    buf[s.len] = '\0';
    char* endptr = NULL;
    double v = strtod(buf, &endptr);
    /* Должен распарсить ВСЁ — endptr указывает на null-term, иначе fail. */
    nova_bool full = (endptr != NULL && (size_t)(endptr - buf) == s.len);
    (void)allocated;  /* heap не освобождаем — GC */
    if (!full) return r;
    r.value = v;
    r.ok = 1;
    return r;
}

/* [M-f64-try-parse-to-parse-f64] (2026-07-07): thin out-param shim over
 * `nova_str_to_f64` (D407-style net2/os FFI convention — bool return +
 * `*out` pointer — so the Nova-side `f64.parse` can be a PLAIN `extern "C"
 * fn` declaration, no compiler-side name knowledge needed). Writes the
 * parsed value to `*out` and returns whether the parse succeeded. */
static inline nova_bool str_parse_f64(nova_str s, double* out) {
    nova_parse_f64_result r = nova_str_to_f64(s);
    *out = r.value;
    return r.ok;
}

/* === str → bool === */
/* Принимает "true"/"false" (case-sensitive). */
static inline nova_parse_bool_result nova_str_to_bool(nova_str s) {
    nova_parse_bool_result r = { 0, 0 };
    if (s.len == 4 && memcmp(s.ptr, "true", 4) == 0) {
        r.value = 1; r.ok = 1; return r;
    }
    if (s.len == 5 && memcmp(s.ptr, "false", 5) == 0) {
        r.value = 0; r.ok = 1; return r;
    }
    return r;
}

/* === Plan 91.14 (D229) pointer Debug — the ONE Display/Debug-family
 * survivor besides `nova_fmt_pad` below (see file-header note) ===
 *
 * ptr → debug str: hex address (Plan 91.14 D229 §«Pointer integration»,
 * Ф.5). Output examples: "0x7f8a4b3c..." (16 hex chars on 64-bit) или
 * "0x0" for null pointer. Caller wraps в "<Type @ 0x...>" form via
 * caller-side concat для full pointer-debug shape.
 *
 * Note: addr disclosure is the security concern motivating
 * E_PTR_NO_DISPLAY_USE_DEBUG_STR for bare ${ptr}. Explicit ${ptr:?}
 * acknowledges the opt-in. Live call site: `emit_c.rs`
 * `emit_interpolated_str`'s pointer-AddrOf-Debug branch — no `.nv` port
 * exists (out of Ф.4R's primitive-family scope, Plan 208 Ф.4R Ш4). */
static inline nova_str nova_ptr_to_debug_str(const void* p) {
    if (p == 0) {
        return (nova_str){ "0x0 (null)", 10 };
    }
    char* buf = (char*)nova_alloc(20);
    int n = snprintf(buf, 20, "0x%p", p);
    /* %p может вывести с "0x" prefix уже — нормализуем. */
    if (n < 0) n = 0;
    /* On некоторых platforms snprintf(%p) уже выводит "0xADDR" prefix.
     * Если так — выкинем дублирующий "0x" prefix. */
    if (n >= 4 && buf[0] == '0' && buf[1] == 'x' && buf[2] == '0' && buf[3] == 'x') {
        memmove(buf, buf + 2, n - 2);
        n -= 2;
        buf[n] = '\0';
    }
    return (nova_str){ buf, (size_t)n };
}

/* === str → char (single codepoint) === */
/* err_kind: 0 ok, 1 empty, 2 multi-char, 3 invalid UTF-8. */
static inline nova_char_decode_result nova_str_to_char(nova_str s) {
    nova_char_decode_result r = { 0, 0 };
    if (s.len == 0) return r;
    unsigned char b = (unsigned char)s.ptr[0];
    nova_int cp = 0;
    size_t step = 1;
    if (b < 0x80) {
        cp = b; step = 1;
    } else if ((b & 0xE0) == 0xC0 && s.len >= 2) {
        cp = ((nova_int)(b & 0x1F) << 6)
           | ((nova_int)((unsigned char)s.ptr[1] & 0x3F));
        step = 2;
    } else if ((b & 0xF0) == 0xE0 && s.len >= 3) {
        cp = ((nova_int)(b & 0x0F) << 12)
           | ((nova_int)((unsigned char)s.ptr[1] & 0x3F) << 6)
           | ((nova_int)((unsigned char)s.ptr[2] & 0x3F));
        step = 3;
    } else if ((b & 0xF8) == 0xF0 && s.len >= 4) {
        cp = ((nova_int)(b & 0x07) << 18)
           | ((nova_int)((unsigned char)s.ptr[1] & 0x3F) << 12)
           | ((nova_int)((unsigned char)s.ptr[2] & 0x3F) << 6)
           | ((nova_int)((unsigned char)s.ptr[3] & 0x3F));
        step = 4;
    } else {
        return r;  /* invalid UTF-8 lead byte */
    }
    /* Должно быть ровно 1 codepoint — иначе multi-char fail. */
    if (step != s.len) return r;
    r.value = cp;
    r.ok = 1;
    return r;
}

/* === int (codepoint) → char === */
/* Range check: 0..0x10FFFF, не в surrogate. */
static inline nova_char_decode_result nova_int_to_char(nova_int n) {
    nova_char_decode_result r = { 0, 0 };
    if (n < 0 || n > 0x10FFFF) return r;
    if (n >= 0xD800 && n <= 0xDFFF) return r;  /* surrogate */
    r.value = n;
    r.ok = 1;
    return r;
}

/* ============================================================================
 * Plan 152.7-B (D258) — format-spec mini-language runtime helpers.
 *
 * Plan 208 Ф.4R Ш4: only `nova_fmt_pad` (+ its two helpers below) survives
 * from this section — the ONLY remaining external width/align post-step,
 * used by `emit_c.rs` `emit_format_spec_value`'s composite/user-type rich-
 * spec tail (composites dispatch `@display(f)`/`@debug(f)` into a fresh
 * builder, then this function pads the result — no `*_display_spec`
 * sibling exists for arbitrary user types, unlike every primitive, which
 * now renders+pads through its own `.nv` `*_display_spec` entry point).
 * All formatting here is locale-INDEPENDENT (no setlocale; fixed ASCII
 * digit/letter tables, '.' decimal point regardless of host locale).
 * ============================================================================ */

/* Encode one Unicode scalar (the fill char) into UTF-8 at `dst`, returning the
 * number of bytes written (1..4). Invalid scalars are coerced to U+FFFD. */
static inline size_t nova_fmt_encode_fill(int32_t cp, char* dst) {
    if (cp < 0 || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) cp = 0xFFFD;
    if (cp < 0x80)        { dst[0] = (char)cp; return 1; }
    if (cp < 0x800)       { dst[0]=(char)(0xC0|(cp>>6)); dst[1]=(char)(0x80|(cp&0x3F)); return 2; }
    if (cp < 0x10000)     { dst[0]=(char)(0xE0|(cp>>12)); dst[1]=(char)(0x80|((cp>>6)&0x3F)); dst[2]=(char)(0x80|(cp&0x3F)); return 3; }
    dst[0]=(char)(0xF0|(cp>>18)); dst[1]=(char)(0x80|((cp>>12)&0x3F)); dst[2]=(char)(0x80|((cp>>6)&0x3F)); dst[3]=(char)(0x80|(cp&0x3F));
    return 4;
}

/* Count Unicode scalar values (codepoints) in a UTF-8 byte run — used as the
 * "display width" of the content (Rust counts chars for width/precision). */
static inline size_t nova_fmt_char_count(const char* p, size_t len) {
    size_t n = 0;
    for (size_t i = 0; i < len; i++) {
        if (((unsigned char)p[i] & 0xC0) != 0x80) n++;
    }
    return n;
}

/* align: 0 = left (pad right), 1 = right (pad left), 2 = center.
 * width is in CODEPOINTS. `prefix` (sign / `0x` etc.) is always emitted first,
 * unpadded by alignment-fill but counted toward width; `zero_pad` (when align
 * is the implied numeric right-align) inserts `'0'` between prefix and body.
 *
 * Returns a freshly GC-allocated nova_str. */
static inline nova_str nova_fmt_pad(
    nova_str prefix, nova_str body,
    int32_t fill_cp, int align, int64_t width, int zero_pad)
{
    size_t content_chars = nova_fmt_char_count(prefix.ptr, prefix.len)
                         + nova_fmt_char_count(body.ptr, body.len);
    int64_t pad_total = (width > (int64_t)content_chars)
                      ? (width - (int64_t)content_chars) : 0;

    /* Zero-padding: fill is '0', placed between prefix and body, never split. */
    if (zero_pad && pad_total > 0) {
        /* Plan 199 Ф.3 (D418): buffer is EXACTLY `need` bytes — no trailing NUL.
         * pad_total > 0 in this branch, so need > 0 — never nova_alloc(0). */
        size_t need = prefix.len + (size_t)pad_total + body.len;
        char* buf = (char*)nova_alloc(need);
        size_t j = 0;
        memcpy(buf + j, prefix.ptr, prefix.len); j += prefix.len;
        for (int64_t k = 0; k < pad_total; k++) buf[j++] = '0';
        memcpy(buf + j, body.ptr, body.len); j += body.len;
        return (nova_str){ (const uint8_t*)buf, j };
    }

    int64_t left_pad = 0, right_pad = 0;
    switch (align) {
        case 0: right_pad = pad_total; break;             /* left-justify */
        case 2: left_pad = pad_total / 2;                 /* center */
                right_pad = pad_total - left_pad; break;
        default: left_pad = pad_total; break;             /* right-justify */
    }

    char fbuf[4];
    size_t fbytes = nova_fmt_encode_fill(fill_cp, fbuf);
    size_t need = prefix.len + body.len
                + (size_t)(left_pad + right_pad) * fbytes;
    /* Plan 199 Ф.3 (D418): buffer is EXACTLY `need` bytes — no trailing NUL.
     * Guard the all-empty/no-pad case so we never nova_alloc(0). */
    if (need == 0) return (nova_str){ (const uint8_t*)"", 0 };
    char* buf = (char*)nova_alloc(need);
    size_t j = 0;
    for (int64_t k = 0; k < left_pad; k++) { memcpy(buf + j, fbuf, fbytes); j += fbytes; }
    memcpy(buf + j, prefix.ptr, prefix.len); j += prefix.len;
    memcpy(buf + j, body.ptr, body.len); j += body.len;
    for (int64_t k = 0; k < right_pad; k++) { memcpy(buf + j, fbuf, fbytes); j += fbytes; }
    return (nova_str){ (const uint8_t*)buf, j };
}

#endif /* NOVA_CONV_H */
