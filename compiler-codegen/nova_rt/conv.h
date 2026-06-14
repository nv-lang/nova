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

/* === bool → str === */
static inline nova_str nova_bool_to_str(nova_bool b) {
    if (b) return (nova_str){ "true", 4 };
    else   return (nova_str){ "false", 5 };
}

/* === f64/i64 → str === */
/* nova_int_to_str уже определён в nova_rt.h; здесь — f64. */
static inline nova_str nova_f64_to_str(double v) {
    char* buf = (char*)nova_alloc(32);
    int n = snprintf(buf, 32, "%g", v);
    if (n < 0) n = 0;
    return (nova_str){ buf, (size_t)n };
}

/* Plan 154.1 [M-154.1-f32-display-debug]: f32 → str via widening to double +
 * f64 formatter. `%g` default 6-sig-fig precision hides the f32→f64 mantissa
 * tail for typical values (0.1f → "0.1"). */
static inline nova_str nova_f32_to_str(nova_f32 v) {
    return nova_f64_to_str((double)v);
}

/* === char (codepoint) → str (UTF-8 encode) === */
/* Infallible: codepoint предполагается валидным (0..0x10FFFF, не surrogate).
 * Если не валидный — эмитим replacement char U+FFFD. */
static inline nova_str nova_char_to_str(nova_int cp) {
    if (cp < 0 || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) {
        cp = 0xFFFD;  /* replacement */
    }
    char* buf = (char*)nova_alloc(5);
    size_t len;
    if (cp < 0x80) {
        buf[0] = (char)cp;
        len = 1;
    } else if (cp < 0x800) {
        buf[0] = (char)(0xC0 | (cp >> 6));
        buf[1] = (char)(0x80 | (cp & 0x3F));
        len = 2;
    } else if (cp < 0x10000) {
        buf[0] = (char)(0xE0 | (cp >> 12));
        buf[1] = (char)(0x80 | ((cp >> 6) & 0x3F));
        buf[2] = (char)(0x80 | (cp & 0x3F));
        len = 3;
    } else {
        buf[0] = (char)(0xF0 | (cp >> 18));
        buf[1] = (char)(0x80 | ((cp >> 12) & 0x3F));
        buf[2] = (char)(0x80 | ((cp >> 6) & 0x3F));
        buf[3] = (char)(0x80 | (cp & 0x3F));
        len = 4;
    }
    buf[len] = '\0';
    return (nova_str){ buf, len };
}

/* === Plan 91.14 (D229) DebugPrintable primitives ===
 *
 * Per D229 §«Default body synthesis»: debug-form per primitive type.
 * Numeric primitives — same output как display. str — quoted + escaped.
 * char — single-quoted + escaped. bool — same as display.
 *
 * Note: nova_int_to_str defined в nova_rt.h (forward visible через include
 * order — nova_rt.h included BEFORE conv.h в generated C). nova_f64_to_str,
 * nova_bool_to_str, nova_char_to_str defined выше в этом файле.
 */

/* bool → debug str: same as display ("true"/"false"). */
static inline nova_str nova_bool_to_debug_str(nova_bool b) {
    return nova_bool_to_str(b);
}

/* int → debug str: same as display (numbers don't need escaping). */
static inline nova_str nova_int_to_debug_str(nova_int v) {
    return nova_int_to_str(v);
}

/* f64 → debug str: same as display. */
static inline nova_str nova_f64_to_debug_str(double v) {
    return nova_f64_to_str(v);
}

/* f32 → debug str: same as display (Plan 154.1). */
static inline nova_str nova_f32_to_debug_str(nova_f32 v) {
    return nova_f32_to_str(v);
}

/* str → debug str: quoted + escaped form (Rust-style).
 *
 * Escape rules:
 *   "  → \"        \  → \\
 *   \n → \n        \t → \t        \r → \r        \0 → \0
 *   ASCII control bytes (< 0x20, non-printable) → \x{HH}
 *   Multi-byte UTF-8 — passthrough (valid из source). */
static inline nova_str nova_str_to_debug_str(nova_str s) {
    /* Pass 1: count output bytes (incl. 2 surrounding quotes). */
    size_t out_len = 2;
    for (size_t i = 0; i < s.len; i++) {
        unsigned char c = (unsigned char)s.ptr[i];
        if (c == '"' || c == '\\') out_len += 2;
        else if (c == '\n' || c == '\t' || c == '\r' || c == '\0') out_len += 2;
        else if (c < 0x20) out_len += 4;
        else out_len += 1;
    }
    char* buf = (char*)nova_alloc(out_len + 1);
    size_t j = 0;
    buf[j++] = '"';
    for (size_t i = 0; i < s.len; i++) {
        unsigned char c = (unsigned char)s.ptr[i];
        switch (c) {
            case '"':  buf[j++] = '\\'; buf[j++] = '"';  break;
            case '\\': buf[j++] = '\\'; buf[j++] = '\\'; break;
            case '\n': buf[j++] = '\\'; buf[j++] = 'n';  break;
            case '\t': buf[j++] = '\\'; buf[j++] = 't';  break;
            case '\r': buf[j++] = '\\'; buf[j++] = 'r';  break;
            case '\0': buf[j++] = '\\'; buf[j++] = '0';  break;
            default:
                if (c < 0x20) {
                    static const char hex[] = "0123456789abcdef";
                    buf[j++] = '\\';
                    buf[j++] = 'x';
                    buf[j++] = hex[(c >> 4) & 0xF];
                    buf[j++] = hex[c & 0xF];
                } else {
                    buf[j++] = (char)c;
                }
                break;
        }
    }
    buf[j++] = '"';
    buf[j] = '\0';
    return (nova_str){ buf, j };
}

/* ptr → debug str: hex address (Plan 91.14 D229 §«Pointer integration»,
 * Ф.5). Output examples: "0x7f8a4b3c..." (16 hex chars on 64-bit) или
 * "0x0" for null pointer. Caller wraps в "<Type @ 0x...>" form via
 * caller-side concat для full pointer-debug shape.
 *
 * Note: addr disclosure is the security concern motivating
 * E_PTR_NO_DISPLAY_USE_DEBUG_STR for bare ${ptr}. Explicit ${ptr:?}
 * acknowledges the opt-in. */
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

/* char (codepoint) → debug str: single-quoted + escaped if needed.
 * Output examples: 'A' '\n' '\\' '\'' (escaped apostrophe). */
static inline nova_str nova_char_to_debug_str(nova_int cp) {
    if (cp < 0 || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) {
        cp = 0xFFFD;
    }
    char* buf = (char*)nova_alloc(8);
    size_t j = 0;
    buf[j++] = '\'';
    if (cp == '\n')      { buf[j++] = '\\'; buf[j++] = 'n'; }
    else if (cp == '\t') { buf[j++] = '\\'; buf[j++] = 't'; }
    else if (cp == '\r') { buf[j++] = '\\'; buf[j++] = 'r'; }
    else if (cp == '\0') { buf[j++] = '\\'; buf[j++] = '0'; }
    else if (cp == '\'') { buf[j++] = '\\'; buf[j++] = '\''; }
    else if (cp == '\\') { buf[j++] = '\\'; buf[j++] = '\\'; }
    else if (cp < 0x20 || cp == 0x7F) {
        static const char hex[] = "0123456789abcdef";
        buf[j++] = '\\';
        buf[j++] = 'x';
        buf[j++] = hex[(cp >> 4) & 0xF];
        buf[j++] = hex[cp & 0xF];
    } else if (cp < 0x80) {
        buf[j++] = (char)cp;
    } else if (cp < 0x800) {
        buf[j++] = (char)(0xC0 | (cp >> 6));
        buf[j++] = (char)(0x80 | (cp & 0x3F));
    } else if (cp < 0x10000) {
        buf[j++] = (char)(0xE0 | (cp >> 12));
        buf[j++] = (char)(0x80 | ((cp >> 6) & 0x3F));
        buf[j++] = (char)(0x80 | (cp & 0x3F));
    } else {
        buf[j++] = (char)(0xF0 | (cp >> 18));
        buf[j++] = (char)(0x80 | ((cp >> 12) & 0x3F));
        buf[j++] = (char)(0x80 | ((cp >> 6) & 0x3F));
        buf[j++] = (char)(0x80 | (cp & 0x3F));
    }
    buf[j++] = '\'';
    buf[j] = '\0';
    return (nova_str){ buf, j };
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
 * Rust-style `${expr:[[fill]align][sign][#][0][width][.precision][type]]}`.
 * All formatting is locale-INDEPENDENT (no setlocale; fixed ASCII digit/letter
 * tables, '.' decimal point regardless of host locale). Codegen parses the
 * spec at compile time (ast/format_spec.rs) and emits calls into these helpers.
 *
 * The split between "prefix" (sign + alt radix marker) and "body" (the
 * magnitude digits) lets `0`-padding insert zeros BETWEEN the sign/prefix and
 * the digits (e.g. `-007`, `0x00ff`) — matching Rust/printf semantics.
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

/* Byte length of the first `nchars` codepoints of a UTF-8 run (for truncation
 * to a precision in codepoints without splitting a multibyte char). */
static inline size_t nova_fmt_bytes_for_chars(const char* p, size_t len, size_t nchars) {
    size_t i = 0, seen = 0;
    while (i < len && seen < nchars) {
        size_t step = 1;
        unsigned char b = (unsigned char)p[i];
        if      (b >= 0xF0) step = 4;
        else if (b >= 0xE0) step = 3;
        else if (b >= 0xC0) step = 2;
        if (i + step > len) step = len - i;
        i += step;
        seen++;
    }
    return i;
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
        size_t need = prefix.len + (size_t)pad_total + body.len;
        char* buf = (char*)nova_alloc(need + 1);
        size_t j = 0;
        memcpy(buf + j, prefix.ptr, prefix.len); j += prefix.len;
        for (int64_t k = 0; k < pad_total; k++) buf[j++] = '0';
        memcpy(buf + j, body.ptr, body.len); j += body.len;
        buf[j] = '\0';
        return (nova_str){ buf, j };
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
    char* buf = (char*)nova_alloc(need + 1);
    size_t j = 0;
    for (int64_t k = 0; k < left_pad; k++) { memcpy(buf + j, fbuf, fbytes); j += fbytes; }
    memcpy(buf + j, prefix.ptr, prefix.len); j += prefix.len;
    memcpy(buf + j, body.ptr, body.len); j += body.len;
    for (int64_t k = 0; k < right_pad; k++) { memcpy(buf + j, fbuf, fbytes); j += fbytes; }
    buf[j] = '\0';
    return (nova_str){ buf, j };
}

/* int → decimal magnitude digit string. Produces UNSIGNED magnitude only;
 * sign/prefix handled by the caller via nova_fmt_int_prefix + nova_fmt_pad.
 * Handles INT64_MIN correctly via the unsigned domain. */
static inline nova_str nova_fmt_int_body(nova_int v, int base, int upper) {
    uint64_t mag = (v < 0) ? (uint64_t)(-(v + 1)) + 1u : (uint64_t)v;
    const char* digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    char tmp[66];
    size_t n = 0;
    if (mag == 0) { tmp[n++] = '0'; }
    while (mag > 0) { tmp[n++] = digits[mag % (uint64_t)base]; mag /= (uint64_t)base; }
    char* buf = (char*)nova_alloc(n + 1);
    for (size_t k = 0; k < n; k++) buf[k] = tmp[n - 1 - k];
    buf[n] = '\0';
    return (nova_str){ buf, n };
}

/* Radix body (`x`/`X`/`b`/`o`): reinterpret the value as an UNSIGNED two's-
 * complement bit pattern (Rust semantics — `{:x}` of -1i64 == "ff..ff").
 * No sign char; the `#` prefix is added separately. */
static inline nova_str nova_fmt_int_radix_body(nova_int v, int base, int upper) {
    uint64_t bits = (uint64_t)v;
    const char* digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    char tmp[66];
    size_t n = 0;
    if (bits == 0) { tmp[n++] = '0'; }
    while (bits > 0) { tmp[n++] = digits[bits % (uint64_t)base]; bits /= (uint64_t)base; }
    char* buf = (char*)nova_alloc(n + 1);
    for (size_t k = 0; k < n; k++) buf[k] = tmp[n - 1 - k];
    buf[n] = '\0';
    return (nova_str){ buf, n };
}

/* Build the sign + alt-radix prefix string for a DECIMAL integer.
 *   sign_plus : force a leading '+' for non-negatives. */
static inline nova_str nova_fmt_int_prefix(nova_int v, int sign_plus) {
    char buf[2];
    size_t n = 0;
    if (v < 0) buf[n++] = '-';
    else if (sign_plus) buf[n++] = '+';
    if (n == 0) return (nova_str){ "", 0 };
    char* out = (char*)nova_alloc(n + 1);
    memcpy(out, buf, n);
    out[n] = '\0';
    return (nova_str){ out, n };
}

/* Build the alt-radix prefix (`0x`/`0X`/`0o`/`0b`) for a radix integer.
 * Radix formatting is unsigned (two's complement), so there is never a sign. */
static inline nova_str nova_fmt_radix_prefix(int alt, int base, int upper) {
    if (!alt) return (nova_str){ "", 0 };
    char buf[2];
    size_t n = 0;
    if (base == 16)      { buf[n++] = '0'; buf[n++] = upper ? 'X' : 'x'; }
    else if (base == 8)  { buf[n++] = '0'; buf[n++] = 'o'; }
    else if (base == 2)  { buf[n++] = '0'; buf[n++] = 'b'; }
    if (n == 0) return (nova_str){ "", 0 };
    char* out = (char*)nova_alloc(n + 1);
    memcpy(out, buf, n);
    out[n] = '\0';
    return (nova_str){ out, n };
}

/* f64 → fixed-precision magnitude string (no sign), `prec` decimal places. */
static inline nova_str nova_fmt_f64_body(double v, int prec) {
    double mag = (v < 0.0) ? -v : v;
    /* worst case: 309 integer digits + '.' + prec + NUL. Cap precision. */
    if (prec < 0) prec = 0;
    if (prec > 64) prec = 64;
    int cap = 340 + prec + 2;
    char* buf = (char*)nova_alloc((size_t)cap);
    int n = snprintf(buf, (size_t)cap, "%.*f", prec, mag);
    if (n < 0) n = 0;
    return (nova_str){ buf, (size_t)n };
}

/* Sign prefix for a float value (NaN/Inf carry their own sign from snprintf for
 * the body path, but the fixed-precision body above takes magnitude, so the
 * sign is computed here from the raw value). */
static inline nova_str nova_fmt_f64_prefix(double v, int sign_plus) {
    /* signbit handles -0.0 too (Rust prints "-0.00" for f64 -0.0). */
    int neg = (v < 0.0) || (v == 0.0 && (1.0 / v) < 0.0);
    char buf[2]; size_t n = 0;
    if (neg) buf[n++] = '-';
    else if (sign_plus) buf[n++] = '+';
    if (n == 0) return (nova_str){ "", 0 };
    char* out = (char*)nova_alloc(n + 1);
    memcpy(out, buf, n); out[n] = '\0';
    return (nova_str){ out, n };
}

/* Truncate a string to `prec` codepoints (string precision). Returns a view
 * into the same backing bytes (zero-copy) — safe because nova_str is immutable
 * and the source outlives the interpolation expression. */
static inline nova_str nova_fmt_str_precision(nova_str s, int64_t prec) {
    if (prec < 0) return s;
    size_t nbytes = nova_fmt_bytes_for_chars(s.ptr, s.len, (size_t)prec);
    return (nova_str){ s.ptr, nbytes };
}

#endif /* NOVA_CONV_H */
