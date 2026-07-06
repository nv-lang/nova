/* SPDX-License-Identifier: MIT OR Apache-2.0 */
/* Plan 179 Ф.2 (D337): thin C shim over libbrotlidec for std/encoding/compress.
 *
 * These are PURE PROTOTYPES with no brotli dependency, so this header is safe to
 * #include unconditionally from nova_rt.h. The DEFINITIONS live in brotli_shim.c,
 * which is compiled — and libbrotlidec.lib linked — ONLY when the compilation unit
 * actually calls a nova_brotli_* symbol. The build layer (test_runner.rs) detects
 * that by scanning the generated .c for the `nova_brotli_` marker: extern "C" fns
 * emit CALL SITES ONLY (no forward decls, D82), so the marker appears iff a brotli
 * codec is actually reached. A program that never touches brotli links no brotli
 * code and no brotli lib (owner requirement: conditional, not always-on).
 *
 * Model mirrors std/fs's fd + (buf, len) FFI shape: the caller owns Nova []u8
 * buffers and passes .as_ptr()+len; the shim never retains a Nova pointer across
 * calls (fed input is copied into a malloc'd accumulation buffer). Handles are an
 * opaque nova_int carrying a (NovaBrotliDec*). All state is malloc'd (not GC), and
 * released by nova_brotli_dec_free — the Nova wrapper guarantees the free on every
 * exit path (BrotliReader is `consume`, D133).
 */
#ifndef NOVA_BROTLI_SHIM_H
#define NOVA_BROTLI_SHIM_H

#include <stdint.h>
/* Handles/counts cross as `intptr_t` — identical to nova_rt's `nova_int` (the
 * address-sized int, Plan 133). Using intptr_t keeps this header self-contained
 * so brotli_shim.c compiles as a standalone TU without pulling nova_rt.h. */

/* Create a streaming decoder. Returns an opaque handle (>0), or 0 on OOM. */
intptr_t nova_brotli_dec_new(void);

/* Append `len` compressed bytes (copied internally). 0 = OK, -1 = OOM/bad-arg. */
intptr_t nova_brotli_dec_feed(intptr_t h, const uint8_t* p, intptr_t len);

/* Decode into `out` (up to `out_cap` bytes). Returns bytes written (>=0), or -1
 * on a decode error (query nova_brotli_dec_error). After the call, inspect
 * nova_brotli_dec_done / nova_brotli_dec_needs_input for stream state. */
intptr_t nova_brotli_dec_pull(intptr_t h, uint8_t* out, intptr_t out_cap);

/* 1 once the stream reached clean end (BROTLI_DECODER_RESULT_SUCCESS), else 0. */
intptr_t nova_brotli_dec_done(intptr_t h);

/* 1 if the last pull blocked needing more input (truncated when no more feeds). */
intptr_t nova_brotli_dec_needs_input(intptr_t h);

/* Detailed BrotliDecoderErrorCode (<0) after a -1 pull, else 0. */
intptr_t nova_brotli_dec_error(intptr_t h);

/* Destroy the decoder + free all internal buffers. Idempotent-safe on 0. */
void nova_brotli_dec_free(intptr_t h);

#endif /* NOVA_BROTLI_SHIM_H */
