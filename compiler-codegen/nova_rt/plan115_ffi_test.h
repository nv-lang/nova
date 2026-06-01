/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * plan115_ffi_test.h — Plan 115 Ф.2 minimal tuple-return FFI shim.
 *
 * Зачем. Plan 115 D214 documents tuple-by-value returns в external fn ABI:
 *
 *     external fn nova_p115_make_pair(seed int) -> (ptr, int)
 *
 * Mono'd Nova tuple type emit'тся как:
 *
 *     typedef struct _NovaTuple_2_8_nova_ptr_8_nova_int {
 *         nova_ptr f0;
 *         nova_int f1;
 *     } _NovaTuple_2_8_nova_ptr_8_nova_int;
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
 * `nova_ptr` / `nova_int` typedefs приходят из nova_rt.h (transitive include
 * этого header'а). */

/* (ptr, int) — 2-word tuple, register-return на Sys V AMD64. */
#ifndef NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_ptr_8_nova_int
#define NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_ptr_8_nova_int
typedef struct _NovaTuple_2_8_nova_ptr_8_nova_int {
    nova_ptr f0;
    nova_int f1;
} _NovaTuple_2_8_nova_ptr_8_nova_int;
#endif

/* (ptr, int, int) — 3-word tuple, hidden-out на Win x64, register на Sys V
 * AMD64 (24 bytes > 16 → spilled to memory but compiler may optimize). */
#ifndef NOVA_TUPLE_TYPEDEF__NovaTuple_3_8_nova_ptr_8_nova_int_8_nova_int
#define NOVA_TUPLE_TYPEDEF__NovaTuple_3_8_nova_ptr_8_nova_int_8_nova_int
typedef struct _NovaTuple_3_8_nova_ptr_8_nova_int_8_nova_int {
    nova_ptr f0;
    nova_int f1;
    nova_int f2;
} _NovaTuple_3_8_nova_ptr_8_nova_int_8_nova_int;
#endif

/* ─── Plan 115 Ф.2 — Test shim functions ─── */
/* C name convention для free external fn — ExternalRegistry приставляет
 * `nova_fn_` префикс (см. external_registry.rs decl_from_fn). Nova-side
 * declaration `external fn nova_p115_make_pair(...)` → C call site
 * `nova_fn_nova_p115_make_pair(...)`. Чтобы избежать double-prefix
 * confusion, я даю Nova-side identifier'у короткое имя `p115_pair`, и
 * C shim — `nova_fn_p115_pair`. См. fixture для actual Nova declaration. */

/* T2.2: tuple (ptr, int) by-value return.
 * Returns synthetic pair: ptr derived from seed + 0x100 (cast to opaque),
 * code = seed * 2. */
static inline _NovaTuple_2_8_nova_ptr_8_nova_int
nova_fn_p115_make_pair(nova_int seed) {
    _NovaTuple_2_8_nova_ptr_8_nova_int r;
    r.f0 = (nova_ptr)(uintptr_t)(seed + 0x100);
    r.f1 = seed * 2;
    return r;
}

/* T2.2: 3-element tuple — exercises hidden-out-pointer ABI на Win x64. */
static inline _NovaTuple_3_8_nova_ptr_8_nova_int_8_nova_int
nova_fn_p115_make_triple(nova_int seed) {
    _NovaTuple_3_8_nova_ptr_8_nova_int_8_nova_int r;
    r.f0 = (nova_ptr)(uintptr_t)(seed + 0x200);
    r.f1 = seed * 3;
    r.f2 = seed + 1000;
    return r;
}

/* Roundtrip helper: extracts the ptr value back to integer. */
static inline nova_int nova_fn_p115_ptr_to_int(nova_ptr p) {
    return (nova_int)(uintptr_t)p;
}

/* Constructor from raw int — для testing typed handle pattern from C side. */
static inline nova_ptr nova_fn_p115_int_to_ptr(nova_int value) {
    return (nova_ptr)(uintptr_t)value;
}

#endif /* NOVA_P115_FFI_TEST_H */
