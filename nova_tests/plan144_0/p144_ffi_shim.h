/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * p144_ffi_shim.h — Plan 144.0 may-GC FFI negative-test shim.
 *
 * A single header-only `static inline` C function so the FFI negative fixture
 * (`negative_ffi_call.nv`) has a real, linkable extern target. The Nova fn
 * that calls this external function must classify MayGC: a C callee is an
 * unknown target whose may-GC effect cannot be proven NoGC, so the analysis
 * flags the caller `makes_ffi` → MayGC (soundness — H4).
 *
 * Naming/type convention (Plan 115 Ф.3 FFI surface): the compiler emits a call
 * to `nova_fn_<name>` with Nova C ABI types, so the shim must define
 * `nova_fn_p144_c_add` using `nova_int` (mirrors plan143_2/p143_ffi_shim.h).
 */
#ifndef P144_FFI_SHIM_H
#define P144_FFI_SHIM_H

/* nova_rt for the nova_int typedef. Path is relative to cg_include (passed via
 * clang -I), mirroring plan143_2/p143_ffi_shim.h. */
#include "nova_rt/nova_rt.h"

static inline nova_int nova_fn_p144_c_add(nova_int a, nova_int b) {
    return a + b;
}

#endif /* P144_FFI_SHIM_H */
