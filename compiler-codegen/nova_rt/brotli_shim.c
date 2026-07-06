/* SPDX-License-Identifier: MIT OR Apache-2.0 */
/* Plan 179 Ф.2 (D337): C shim over libbrotlidec — DEFINITIONS.
 *
 * Compiled ONLY when the compilation unit references a nova_brotli_* symbol
 * (test_runner.rs conditional-link gate). Two modes:
 *
 *   NOVA_USE_BROTLI defined   → real decoder over the vendored libbrotlidec
 *                               (headers nova_rt/brotli/include + libbrotlidec.lib).
 *   NOVA_USE_BROTLI undefined  → feature-gate STUBS (Q11): the vendored lib is not
 *                               available for this host, so dec_new() returns 0 and
 *                               the Nova wrapper degrades to UnsupportedMethod at
 *                               runtime — NEVER a link error, NEVER a panic.
 *
 * Streaming contract (D335/D337): feed appends compressed bytes; pull runs
 * BrotliDecoderDecompressStream, emitting <= out_cap bytes per call so the Nova
 * bomb-cap (D334) is enforced INCREMENTALLY over the FFI — output never grows past
 * `max_output + one chunk` before Nova aborts with CompressError{Bomb}. brotli's
 * own window is format-bounded (lgwin<=24 => <=16 MiB), independent of out_cap.
 */
#include "brotli_shim.h"

#include <stdlib.h>
#include <string.h>

#ifdef NOVA_USE_BROTLI

#include "brotli/decode.h"

typedef struct NovaBrotliDec {
    BrotliDecoderState* st;
    uint8_t* in;      /* malloc'd input accumulation buffer */
    size_t   in_len;  /* valid bytes appended                 */
    size_t   in_pos;  /* bytes already consumed by the decoder */
    size_t   in_cap;  /* capacity of `in`                     */
    int      done;    /* stream reached BROTLI_DECODER_RESULT_SUCCESS */
    int      err;     /* BrotliDecoderErrorCode (<0), 0 = none */
    int      needs_input; /* last pull blocked on NEEDS_MORE_INPUT */
} NovaBrotliDec;

intptr_t nova_brotli_dec_new(void) {
    NovaBrotliDec* d = (NovaBrotliDec*)calloc(1, sizeof(NovaBrotliDec));
    if (!d) return 0;
    d->st = BrotliDecoderCreateInstance(NULL, NULL, NULL);
    if (!d->st) { free(d); return 0; }
    return (intptr_t)d;
}

intptr_t nova_brotli_dec_feed(intptr_t h, const uint8_t* p, intptr_t len) {
    NovaBrotliDec* d = (NovaBrotliDec*)(intptr_t)h;
    if (!d || len < 0) return -1;
    if (len == 0) return 0;
    /* Compact the already-consumed prefix so streaming memory stays bounded. */
    if (d->in_pos > 0) {
        size_t rem = d->in_len - d->in_pos;
        if (rem > 0) memmove(d->in, d->in + d->in_pos, rem);
        d->in_len = rem;
        d->in_pos = 0;
    }
    {
        size_t need = d->in_len + (size_t)len;
        if (need > d->in_cap) {
            size_t nc = d->in_cap ? d->in_cap : 4096;
            while (nc < need) nc *= 2;
            uint8_t* nb = (uint8_t*)realloc(d->in, nc);
            if (!nb) return -1;
            d->in = nb;
            d->in_cap = nc;
        }
        memcpy(d->in + d->in_len, p, (size_t)len);
        d->in_len += (size_t)len;
    }
    return 0;
}

intptr_t nova_brotli_dec_pull(intptr_t h, uint8_t* out, intptr_t out_cap) {
    NovaBrotliDec* d = (NovaBrotliDec*)(intptr_t)h;
    if (!d || out_cap < 0) return -1;
    if (d->err != 0) return -1;
    if (d->done) return 0;
    {
        const uint8_t* next_in = d->in + d->in_pos;
        size_t avail_in = d->in_len - d->in_pos;
        uint8_t* next_out = out;
        size_t avail_out = (size_t)out_cap;
        size_t total = 0;
        BrotliDecoderResult r = BrotliDecoderDecompressStream(
            d->st, &avail_in, &next_in, &avail_out, &next_out, &total);
        d->in_pos = d->in_len - avail_in; /* advance consumed cursor */
        d->needs_input = 0;
        if (r == BROTLI_DECODER_RESULT_ERROR) {
            d->err = (int)BrotliDecoderGetErrorCode(d->st);
            if (d->err == 0) d->err = -1;
            return -1;
        }
        if (r == BROTLI_DECODER_RESULT_SUCCESS) {
            d->done = 1;
        } else if (r == BROTLI_DECODER_RESULT_NEEDS_MORE_INPUT) {
            d->needs_input = 1;
        }
        return (intptr_t)((size_t)out_cap - avail_out);
    }
}

intptr_t nova_brotli_dec_done(intptr_t h) {
    NovaBrotliDec* d = (NovaBrotliDec*)(intptr_t)h;
    return (d && d->done) ? 1 : 0;
}

intptr_t nova_brotli_dec_needs_input(intptr_t h) {
    NovaBrotliDec* d = (NovaBrotliDec*)(intptr_t)h;
    return (d && d->needs_input) ? 1 : 0;
}

intptr_t nova_brotli_dec_error(intptr_t h) {
    NovaBrotliDec* d = (NovaBrotliDec*)(intptr_t)h;
    return d ? (intptr_t)d->err : -1;
}

void nova_brotli_dec_free(intptr_t h) {
    NovaBrotliDec* d = (NovaBrotliDec*)(intptr_t)h;
    if (!d) return;
    if (d->st) BrotliDecoderDestroyInstance(d->st);
    if (d->in) free(d->in);
    free(d);
}

#else /* !NOVA_USE_BROTLI — feature-gate stubs (Q11) */

intptr_t nova_brotli_dec_new(void) { return 0; }
intptr_t nova_brotli_dec_feed(intptr_t h, const uint8_t* p, intptr_t len) {
    (void)h; (void)p; (void)len; return -1;
}
intptr_t nova_brotli_dec_pull(intptr_t h, uint8_t* out, intptr_t out_cap) {
    (void)h; (void)out; (void)out_cap; return -1;
}
intptr_t nova_brotli_dec_done(intptr_t h) { (void)h; return 0; }
intptr_t nova_brotli_dec_needs_input(intptr_t h) { (void)h; return 0; }
intptr_t nova_brotli_dec_error(intptr_t h) { (void)h; return -1; }
void nova_brotli_dec_free(intptr_t h) { (void)h; }

#endif /* NOVA_USE_BROTLI */
