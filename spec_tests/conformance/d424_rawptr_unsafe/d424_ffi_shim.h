/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * d424_ffi_shim.h — [M-174.6-rawptr-extern-unsafe-infer] (D424 rule 1/2,
 * Plan 174.6 M4) positive-fixture shim.
 *
 * Two header-only `static inline` C functions so `d424_rawptr_unsafe_pos.nv`
 * has real, linkable extern targets for both halves of D424's inference
 * rule:
 *   - `d424_ptr_first_byte` — a raw pointer (`const uint8_t*`) parameter, the
 *     motivating FFI shape (mirrors `net_addr_ip`/`net_tcp_read`-style
 *     `extern "C" fn` signatures in std/net/ffi.nv — literal C name, no
 *     `nova_fn_` prefix, since this is `extern "C" fn` not the legacy
 *     `nova`-ABI `external fn`).
 *   - `d424_scalar_add` — scalar-only, no pointer anywhere in the signature.
 *
 * C-type mapping mirrors compiler-codegen/nova_rt/net.h (Nova `int` →
 * `nova_int` = `intptr_t`; Nova `*u8` → `const uint8_t*`).
 */
#ifndef D424_FFI_SHIM_H
#define D424_FFI_SHIM_H

#include "nova_rt/nova_rt.h"

/* Raw-ptr param — returns the first byte of `p` (or -1 if len <= 0). Real,
 * observable behavior so the positive test's `assert` is genuine, not a
 * rubber-stamp. */
static inline nova_int d424_ptr_first_byte(const uint8_t *p, nova_int len) {
    if (len <= 0) {
        return -1;
    }
    return (nova_int)p[0];
}

/* Scalar-only — no pointer anywhere in the signature. */
static inline nova_int d424_scalar_add(nova_int a, nova_int b) {
    return a + b;
}

#endif /* D424_FFI_SHIM_H */
