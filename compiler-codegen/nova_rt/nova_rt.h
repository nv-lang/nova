#ifndef NOVA_RT_H
#define NOVA_RT_H

#include "alloc.h"
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

/* ---- Primitive types ---- */
typedef int64_t  nova_int;
typedef double   nova_f64;
typedef float    nova_f32;
typedef bool     nova_bool;
typedef uint8_t  nova_byte;

/* ---- String ---- */
typedef struct {
    const char* ptr;
    size_t      len;
} nova_str;

static inline nova_str nova_str_from_cstr(const char* s) {
    return (nova_str){ s, strlen(s) };
}

/* ---- String methods ---- */

static inline nova_bool nova_str_starts_with(nova_str s, nova_str prefix) {
    return s.len >= prefix.len && memcmp(s.ptr, prefix.ptr, prefix.len) == 0;
}

static inline nova_bool nova_str_ends_with(nova_str s, nova_str suffix) {
    return s.len >= suffix.len &&
           memcmp(s.ptr + s.len - suffix.len, suffix.ptr, suffix.len) == 0;
}

static inline nova_bool nova_str_contains(nova_str s, nova_str needle) {
    if (needle.len == 0) return true;
    if (needle.len > s.len) return false;
    for (size_t i = 0; i <= s.len - needle.len; i++) {
        if (memcmp(s.ptr + i, needle.ptr, needle.len) == 0) return true;
    }
    return false;
}

/* nova_str_to_upper: allocates via nova_alloc, returns new nova_str */
static inline nova_str nova_str_to_upper(nova_str s) {
    char* buf = (char*)nova_alloc(s.len + 1);
    for (size_t i = 0; i < s.len; i++) {
        unsigned char c = (unsigned char)s.ptr[i];
        buf[i] = (c >= 'a' && c <= 'z') ? (char)(c - 32) : (char)c;
    }
    buf[s.len] = '\0';
    return (nova_str){ buf, s.len };
}

static inline nova_str nova_str_to_lower(nova_str s) {
    char* buf = (char*)nova_alloc(s.len + 1);
    for (size_t i = 0; i < s.len; i++) {
        unsigned char c = (unsigned char)s.ptr[i];
        buf[i] = (c >= 'A' && c <= 'Z') ? (char)(c + 32) : (char)c;
    }
    buf[s.len] = '\0';
    return (nova_str){ buf, s.len };
}

static inline nova_str nova_str_trim(nova_str s) {
    size_t start = 0, end = s.len;
    while (start < end && (unsigned char)s.ptr[start] <= ' ') start++;
    while (end > start && (unsigned char)s.ptr[end-1] <= ' ') end--;
    return (nova_str){ s.ptr + start, end - start };
}

static inline nova_str nova_str_slice(nova_str s, nova_int from, nova_int to) {
    if (from < 0) from = 0;
    if (to > (nova_int)s.len) to = (nova_int)s.len;
    if (from >= to) return (nova_str){ s.ptr, 0 };
    return (nova_str){ s.ptr + from, (size_t)(to - from) };
}

/* nova_str_concat: concatenate two strings, allocates via nova_alloc */
static inline nova_str nova_str_concat(nova_str a, nova_str b) {
    size_t total = a.len + b.len;
    char* buf = (char*)nova_alloc(total + 1);
    memcpy(buf, a.ptr, a.len);
    memcpy(buf + a.len, b.ptr, b.len);
    buf[total] = '\0';
    return (nova_str){ buf, total };
}

static inline nova_bool nova_str_eq(nova_str a, nova_str b) {
    return a.len == b.len && memcmp(a.ptr, b.ptr, a.len) == 0;
}

/* nova_int_to_str: convert integer to string */
static inline nova_str nova_int_to_str(nova_int v) {
    char* buf = (char*)nova_alloc(24);
    int n = snprintf(buf, 24, "%lld", (long long)v);
    return (nova_str){ buf, (size_t)(n < 0 ? 0 : n) };
}

/* ---- println ---- */
/* Variadic nova_println is generated per call-site. Each arg is printed
 * with its own helper depending on type. */

static inline void nova_print_int(nova_int v)  { printf("%lld", (long long)v); }
static inline void nova_print_f64(nova_f64 v)  { printf("%g", v); }
static inline void nova_print_f32(nova_f32 v)  { printf("%g", (double)v); }
static inline void nova_print_bool(nova_bool v) { printf("%s", v ? "true" : "false"); }
static inline void nova_print_str(nova_str v)   { fwrite(v.ptr, 1, v.len, stdout); }
static inline void nova_print_newline(void)     { putchar('\n'); }

/* ---- Unit ---- */
typedef struct { char _dummy; } nova_unit;
#define NOVA_UNIT ((nova_unit){0})

/* ---- Effects (Phase 4) — also defines NovaTestFrame + nova_assert ---- */
#include "effects.h"

/* ---- Fibers / spawn (Phase 5) ---- */
#include "fibers.h"

#endif /* NOVA_RT_H */
