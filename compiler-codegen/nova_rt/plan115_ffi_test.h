/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * plan115_ffi_test.h — Plan 115 Ф.2 minimal tuple-return FFI shim.
 *
 * Plan 134: nova_ptr typedef REMOVED — all occurrences replaced with void*.
 * Tuple mangled names updated: nova_ptr → void_p.
 *
 * Зачем. Plan 115 D214 documents tuple-by-value returns в external fn ABI:
 *
 *     external fn nova_p115_make_pair(seed int) -> (*(), int)
 *
 * Mono'd Nova tuple type emit'тся как:
 *
 *     typedef struct _NovaTuple_2_6_void_p_8_nova_int {
 *         void* f0;
 *         nova_int f1;
 *     } _NovaTuple_2_6_void_p_8_nova_int;
 *
 * Это header-only shim предоставляет C-side implementation тестируемых
 * external fn — minimal pair-returning helpers used by Nova fixtures.
 *
 * Cooperation с Nova codegen. Nova emit'тит mono'd tuple typedefs ПОСЛЕ
 * `#include "nova_rt.h"`. Чтобы избежать redefinition error при
 * forward-declaration в этом header'е, Plan 115 D214 ввёл `#ifndef
 * NOVA_TUPLE_TYPEDEF_<mangled>` guard вокруг каждого typedef'а. Shim header
 * декларирует guard ДО Nova's emit → Nova skip'ает redeclaration.
 *
 * Layout соответствие. Tagged struct form (`struct NAME { ... } NAME;`)
 * обеспечивает type identity между shim header'ом и Nova-generated .c.
 */

#ifndef NOVA_P115_FFI_TEST_H
#define NOVA_P115_FFI_TEST_H

#include <stdint.h>

/* ─── Plan 115 Ф.2 — Forward-declarations matching Nova mono'd tuple typedefs
 *
 * Guard symbols mirror Nova's `#ifndef NOVA_TUPLE_TYPEDEF_<mangled>` emit
 * pattern (см. emit_c.rs «Plan 115 D214: tagged struct form»).
 *
 * Plan 134: nova_ptr removed; void* used directly.
 * `nova_int` typedef приходит из nova_rt.h (transitive include). */

/* (*(), int) — 2-word tuple, register-return на Sys V AMD64. */
/* Plan 134: mangled name uses void_p (sanitize_c_for_ident("void*") = "void_p"). */
#ifndef NOVA_TUPLE_TYPEDEF__NovaTuple_2_6_void_p_8_nova_int
#define NOVA_TUPLE_TYPEDEF__NovaTuple_2_6_void_p_8_nova_int
typedef struct _NovaTuple_2_6_void_p_8_nova_int {
    void* f0;
    nova_int f1;
} _NovaTuple_2_6_void_p_8_nova_int;
#endif

/* (*(), int, int) — 3-word tuple. */
/* Plan 134: mangled name uses void_p. */
#ifndef NOVA_TUPLE_TYPEDEF__NovaTuple_3_6_void_p_8_nova_int_8_nova_int
#define NOVA_TUPLE_TYPEDEF__NovaTuple_3_6_void_p_8_nova_int_8_nova_int
typedef struct _NovaTuple_3_6_void_p_8_nova_int_8_nova_int {
    void* f0;
    nova_int f1;
    nova_int f2;
} _NovaTuple_3_6_void_p_8_nova_int_8_nova_int;
#endif

/* ─── Plan 115 Ф.2 — Test shim functions ─── */
/* C name convention для free external fn — ExternalRegistry приставляет
 * `nova_fn_` префикс (см. external_registry.rs decl_from_fn). Nova-side
 * declaration `external fn nova_p115_make_pair(...)` → C call site
 * `nova_fn_nova_p115_make_pair(...)`. Чтобы избежать double-prefix
 * confusion, я даю Nova-side identifier'у короткое имя `p115_pair`, и
 * C shim — `nova_fn_p115_pair`. См. fixture для actual Nova declaration. */

/* T2.2: tuple (*(), int) by-value return.
 * Returns synthetic pair: ptr derived from seed + 0x100 (cast to opaque),
 * code = seed * 2. */
static inline _NovaTuple_2_6_void_p_8_nova_int
nova_fn_p115_make_pair(nova_int seed) {
    _NovaTuple_2_6_void_p_8_nova_int r;
    r.f0 = (void*)(uintptr_t)(seed + 0x100);
    r.f1 = seed * 2;
    return r;
}

/* T2.2: 3-element tuple — exercises hidden-out-pointer ABI на Win x64. */
static inline _NovaTuple_3_6_void_p_8_nova_int_8_nova_int
nova_fn_p115_make_triple(nova_int seed) {
    _NovaTuple_3_6_void_p_8_nova_int_8_nova_int r;
    r.f0 = (void*)(uintptr_t)(seed + 0x200);
    r.f1 = seed * 3;
    r.f2 = seed + 1000;
    return r;
}

/* Roundtrip helper: extracts the void* value back to integer. */
static inline nova_int nova_fn_p115_ptr_to_int(void* p) {
    return (nova_int)(uintptr_t)p;
}

/* Constructor from raw int — для testing typed handle pattern from C side. */
static inline void* nova_fn_p115_int_to_ptr(nova_int value) {
    return (void*)(uintptr_t)value;
}

/* ─── Plan 139 Ф.4 — str ↔ C-string FFI interop shim ───
 *
 * Exercises the `str` value-record ABI (`{const uint8_t* ptr; int64_t len;}`)
 * at the C FFI boundary, proving Plan 139's risk-limiter (the nova_str typedef
 * alias) holds end-to-end:
 *
 *   - p139_str_byte_sum(nova_str): reads .ptr/.len of a str passed BY VALUE,
 *     sums the bytes. Proves str → C (a C fn taking the str value-record reads
 *     the immutable UTF-8 buffer through `const uint8_t* ptr`, no copy).
 *
 *   - p139_cstr_strlen(const char*): receives a CStr (Nova `CStr(*u8)` newtype,
 *     which marshals to `const char*` / `const uint8_t*` at the ABI) and runs
 *     C strlen. Proves str → CStr → C: the NUL-terminator invariant (D26) means
 *     a CStr from `s.as_cstr()` is a valid `const char*` and strlen terminates
 *     correctly. The Nova-side `as_cstr()` scans for embedded NULs first.
 *
 *   - p139_make_str(): returns a `nova_str` constructed C-side via
 *     nova_str_from_cstr from a static rodata C-string. Proves C → str
 *     (from_cstr path): the returned 16-byte value-record carries a pointer
 *     into rodata (never collected) and the correct byte length.
 *
 * All three take/return the `nova_str` typedef or its `const char*` field type
 * unchanged — no Plan 139-specific C surface beyond the typedef redefinition. */

static inline nova_int nova_fn_p139_str_byte_sum(nova_str s) {
    nova_int acc = 0;
    for (int64_t i = 0; i < s.len; i++) {
        acc += (nova_int)(unsigned char)s.ptr[i];
    }
    return acc;
}

/* CStr (Nova `CStr(*u8)`) marshals to a raw byte pointer at the ABI. We take
 * `const char*` (≡ `const uint8_t*`, same 8-byte ptr) and run libc strlen —
 * valid because as_cstr() guarantees the NUL terminator (D26 invariant + the
 * embedded-NUL scan). */
static inline nova_int nova_fn_p139_cstr_strlen(const char* c) {
    return (nova_int)strlen(c);
}

/* C → str via from_cstr: returns the canonical 11-byte greeting from rodata. */
static inline nova_str nova_fn_p139_make_str(void) {
    return nova_str_from_cstr("hello world");
}

#endif /* NOVA_P115_FFI_TEST_H */
