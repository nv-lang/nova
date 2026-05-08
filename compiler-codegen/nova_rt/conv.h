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

#endif /* NOVA_CONV_H */
