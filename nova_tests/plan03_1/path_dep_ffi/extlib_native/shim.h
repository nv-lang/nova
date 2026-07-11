/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * shim.h — Plan 03.1 (ext-dep native/FFI propagation) fixture.
 *
 * Minimal native C shim belonging to the `extlib_native` path-dependency
 * package. Proves that a dependency's own `[ffi]` c_shim is force-included
 * into the CONSUMER's compilation unit (not just the consumer's own
 * package's [ffi]) — the FFI-propagation half of Plan 03.1's path-resolver.
 */
#ifndef EXTLIB_NATIVE_SHIM_H
#define EXTLIB_NATIVE_SHIM_H

static inline int extlib_native_add(int a, int b) {
    return a + b;
}

#endif
