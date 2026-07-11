#ifndef NOVA_RT_STRING_BUILDER_H
#define NOVA_RT_STRING_BUILDER_H

/* ---- Nova StringBuilder — Plan 109 (D179) ----
 *
 * StringBuilder is now a Nova-defined consume type:
 *   type StringBuilder consume { mut buf []u8 }
 *
 * The Nova_StringBuilder struct is emitted by the Nova codegen (not here).
 * This header retains only utility helpers used by str methods and
 * str.from_bytes_* that do NOT depend on the old StringBuilder layout.
 *
 * Old C-based implementation (Nova_StringBuilder_static_new, _method_append_str,
 * etc.) removed — all StringBuilder methods now implemented in Nova
 * (std/runtime/string_builder.nv).
 */

#include "alloc.h"
#include "nova_rt.h"
#include <stdint.h>
#include <string.h>

/* Validate UTF-8 bytes. Returns 1 if valid, 0 otherwise.
 * Shared well-formedness helper (the retired C from_bytes_* used it; the
 * Nova-body `str.from_bytes` decode reuses the same rules — 172.12 A6). */
static inline nova_bool _nova_validate_utf8(const nova_byte* data, int64_t len) {
    int64_t i = 0;
    while (i < len) {
        nova_byte c = data[i];
        if (c < 0x80) {
            i++;
        } else if ((c & 0xE0) == 0xC0) {
            if (i + 1 >= len) return 0;
            if ((data[i + 1] & 0xC0) != 0x80) return 0;
            if (c < 0xC2) return 0;
            i += 2;
        } else if ((c & 0xF0) == 0xE0) {
            if (i + 2 >= len) return 0;
            if ((data[i + 1] & 0xC0) != 0x80) return 0;
            if ((data[i + 2] & 0xC0) != 0x80) return 0;
            if (c == 0xE0 && data[i + 1] < 0xA0) return 0;
            if (c == 0xED && data[i + 1] >= 0xA0) return 0;
            i += 3;
        } else if ((c & 0xF8) == 0xF0) {
            if (i + 3 >= len) return 0;
            if ((data[i + 1] & 0xC0) != 0x80) return 0;
            if ((data[i + 2] & 0xC0) != 0x80) return 0;
            if ((data[i + 3] & 0xC0) != 0x80) return 0;
            if (c == 0xF0 && data[i + 1] < 0x90) return 0;
            if (c == 0xF4 && data[i + 1] >= 0x90) return 0;
            if (c > 0xF4) return 0;
            i += 4;
        } else {
            return 0;
        }
    }
    return 1;
}

/* Encode codepoint as UTF-8 bytes into dst. Returns byte count (1-4) or 0 if invalid. */
static inline int _nova_utf8_encode(nova_byte* dst, nova_int cp) {
    if (cp < 0) return 0;
    if (cp < 0x80) {
        dst[0] = (nova_byte)cp;
        return 1;
    }
    if (cp < 0x800) {
        dst[0] = (nova_byte)(0xC0 | (cp >> 6));
        dst[1] = (nova_byte)(0x80 | (cp & 0x3F));
        return 2;
    }
    if (cp < 0x10000) {
        dst[0] = (nova_byte)(0xE0 | (cp >> 12));
        dst[1] = (nova_byte)(0x80 | ((cp >> 6) & 0x3F));
        dst[2] = (nova_byte)(0x80 | (cp & 0x3F));
        return 3;
    }
    if (cp < 0x110000) {
        dst[0] = (nova_byte)(0xF0 | (cp >> 18));
        dst[1] = (nova_byte)(0x80 | ((cp >> 12) & 0x3F));
        dst[2] = (nova_byte)(0x80 | ((cp >> 6) & 0x3F));
        dst[3] = (nova_byte)(0x80 | (cp & 0x3F));
        return 4;
    }
    return 0;
}

/* str.from(c char) — UTF-8 encode 1-4 bytes from codepoint. */
static inline nova_str Nova_str_static_from_char(nova_int cp) {
    nova_byte tmp[4];
    int n = _nova_utf8_encode(tmp, cp);
    if (n == 0) {
        return (nova_str){.ptr = (const uint8_t*)"", .len = 0};
    }
    /* Plan 199 Ф.3 (D418): EXACTLY n bytes — no trailing NUL. n is 1..4 here. */
    char* buf = (char*)nova_alloc((size_t)n);
    memcpy(buf, tmp, (size_t)n);
    return (nova_str){.ptr = (const uint8_t*)buf, .len = (size_t)n};
}

/* Plan 91.13: str.from_codepoint(int) — alias for str.from(char).
 * Bypasses `int as char` D54 ban для known-valid codepoints
 * (JSON \uXXXX escapes, protocol decoders). Same UTF-8 encode impl. */
static inline nova_str Nova_str_static_from_codepoint(nova_int cp) {
    return Nova_str_static_from_char(cp);
}

/* str.from_bytes_* []u8 consumers (from_bytes_unchecked / steal_bytes /
 * from_bytes_lossy) — REMOVED in Plan 172.12 A6 (Vec-canon substrate).
 *
 * These `NovaArray_nova_byte*`-accepting helpers were retired from codegen
 * routing by Plan 139.2 — `str.from_bytes_*` are now Nova-body statics
 * (`Nova_str_static_from_bytes_*`) that take a real `Vec[u8]` and read its
 * `@ptr`/`@len`. Verified dead (2026-07-07): zero call sites in generated C,
 * zero FFI/extern decls (every reference is a doc comment). Owner decision
 * (2026-07-08): NovaArray dies wholesale → removed here (byte-identical:
 * `static inline` + unused, headers `#include`d not spliced). The shared
 * `_nova_validate_utf8` well-formedness helper is untouched (used elsewhere). */

/* Plan 176 Ф.0.5: `Nova_str_static_try_from_bytes` (the C backing of the retired
 * `str.try_from([]u8)` intrinsic) + its `nova_box_str` helper were removed.
 * Fallible byte→str decode is now the Nova-body `str.from_bytes(bytes) ->
 * Result[str, Utf8Error]` (std/runtime/string/core.nv), which reuses the same
 * `_nova_validate_utf8` well-formedness rules on the Nova side. */

/* nova_str_replace — pure C helper for str.replace bootstrap.
 * Used only when Nova-body dispatch is unavailable (fallback). */
static inline nova_str nova_str_replace(nova_str s, nova_str from, nova_str to) {
    if (from.len == 0 || s.len == 0) return s;
    size_t count = 0, i = 0;
    while (i + from.len <= s.len) {
        if (memcmp(s.ptr + i, from.ptr, from.len) == 0) { count++; i += from.len; }
        else i++;
    }
    if (count == 0) return s;
    size_t out_len = s.len - count * from.len + count * to.len;
    /* Plan 199 Ф.3 (D418): EXACTLY out_len bytes — no trailing NUL. Guard the
     * all-replaced-with-empty case (out_len == 0) so we never nova_alloc(0). */
    if (out_len == 0) return (nova_str){.ptr = (const uint8_t*)"", .len = 0};
    char* out = (char*)nova_alloc(out_len);
    size_t w = 0, src = 0;
    while (src + from.len <= s.len) {
        if (memcmp(s.ptr + src, from.ptr, from.len) == 0) {
            memcpy(out + w, to.ptr, to.len); w += to.len; src += from.len;
        } else {
            out[w++] = s.ptr[src++];
        }
    }
    while (src < s.len) out[w++] = s.ptr[src++];
    return (nova_str){.ptr = (const uint8_t*)out, .len = out_len};
}

#endif /* NOVA_RT_STRING_BUILDER_H */
