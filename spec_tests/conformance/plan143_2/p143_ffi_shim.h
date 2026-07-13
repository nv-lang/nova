/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * p143_ffi_shim.h — Plan 143.2 FFI negative-test shim.
 *
 * A single header-only `static inline` C function so the FFI negative fixture
 * (`negative_ffi_call.nv`) has a real, linkable extern target. The Nova fn
 * that calls this external function must KEEP its function-prologue
 * `nova_preempt_check();` (a C callee may re-enter Nova on an unknown path, so
 * the analysis flags the caller `makes_ffi`).
 *
 * Naming/type convention (Plan 115 Ф.3 FFI surface): the compiler emits a call
 * to `nova_fn_<name>` with Nova C ABI types, so the shim must define
 * `nova_fn_p143_c_add` using `nova_int` (mirrors examples/ffi/sqlite_mini_ffi.h).
 */
#ifndef P143_FFI_SHIM_H
#define P143_FFI_SHIM_H

/* nova_rt for the nova_int typedef. Path is relative to cg_include (passed via
 * clang -I), mirroring examples/ffi/sqlite_mini_ffi.h. */
#include "nova_rt/nova_rt.h"

static inline nova_int nova_fn_p143_c_add(nova_int a, nova_int b) {
    return a + b;
}

#endif /* P143_FFI_SHIM_H */
