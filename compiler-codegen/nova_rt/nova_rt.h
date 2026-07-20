#ifndef NOVA_RT_H
#define NOVA_RT_H

#include "alloc.h"
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <math.h>     /* D74: f64.sqrt()/sin()/cos()/etc. → libm */
#include <float.h>    /* Plan 38: f64.MAX (DBL_MAX) / f64.EPSILON / etc. */
#include "cast.h"     /* План 07: float→int saturation helpers */
/* numeric.h снят [M-ptr-raw-access-contract-and-unaligned] item 3
 * (2026-07-08): f64/f32 to_bits/from_bits теперь ЧИСТЫЙ .nv
 * (std/runtime/numeric.nv, unsafe read_unaligned) — C-обёртки больше не
 * нужны, удалены вместе с этим include. */
/* conv.h подключается в array.h (после nova_alloc и определения nova_str). */

/* ---- Primitive types ---- */
typedef intptr_t  nova_int;   /* int  — signed address-sized (Go C-era intgo, Plan 133) */
typedef uintptr_t nova_uint;  /* uint — unsigned address-sized (Plan 133) */
/* Plan 70.3: distinct typedef для char — uint32_t (Plan 152.8 D128 AMEND).
 * Codepoints fit in 21 bits (U+0000..U+10FFFF); uint32_t is the natural
 * unsigned type matching Rust's `char` ABI. Distinct from nova_int so that
 * `Option[char]` and `Option[int]` mangle to different C names:
 * `NovaOpt_nova_char` vs `NovaOpt_nova_int` (no silent type collapse).
 * Zero ABI cost — typedef alias, не отдельный type. */
typedef uint32_t nova_char;
typedef double   nova_f64;
typedef float    nova_f32;
typedef bool     nova_bool;
/* Plan 134: nova_ptr typedef REMOVED — use *() (pointer-to-unit = void*).
 * `ptr` builtin type replaced with typed pointer syntax `*()` at Nova level;
 * C codegen emits `void*` directly. */

/* ---- Closure representation ---- */
/* Closures are stored as void* pointing to a struct { fn_ptr; void* env }. */
/* fn_ptr takes (void* env, args...) and returns the result type. */
/* NovaClosBase — generic closure layout, для arbitrary-sig calls (Plan 11 Ф.4). */
typedef struct { void* fn; void* env; } NovaClosBase;
typedef nova_int(*nova_fn_vi)(void*);
typedef struct { nova_fn_vi fn; void* env; } NovaClos_vi;
typedef nova_int(*nova_fn_ii)(void*, nova_int);
typedef struct { nova_fn_ii fn; void* env; } NovaClos_ii;
typedef nova_bool(*nova_fn_ib)(void*, nova_int);
typedef struct { nova_fn_ib fn; void* env; } NovaClos_ib;
typedef nova_int(*nova_fn_iii)(void*, nova_int, nova_int);
typedef struct { nova_fn_iii fn; void* env; } NovaClos_iii;
typedef nova_int(*nova_fn_vii)(void*, void*, nova_int);
typedef struct { nova_fn_vii fn; void* env; } NovaClos_vii;
#define NOVA_CLOS_CALL_vi(f)        (((NovaClos_vi*)(f))->fn(((NovaClos_vi*)(f))->env))
#define NOVA_CLOS_CALL_ii(f, a)     (((NovaClos_ii*)(f))->fn(((NovaClos_ii*)(f))->env, (a)))
#define NOVA_CLOS_CALL_ib(f, a)     (((NovaClos_ib*)(f))->fn(((NovaClos_ib*)(f))->env, (a)))
#define NOVA_CLOS_CALL_iii(f,a,b)   (((NovaClos_iii*)(f))->fn(((NovaClos_iii*)(f))->env, (a), (b)))
#define NOVA_CLOS_CALL_vii(f,a,b)   (((NovaClos_vii*)(f))->fn(((NovaClos_vii*)(f))->env, (a), (b)))
typedef uint8_t  nova_byte;

/* ---- String ----
 * Plan 139 Ф.0: `str` is now a Nova value-record `{ ptr *ro u8, len int }`.
 * This C typedef is the ABI image of that value-record. It is layout/ABI-
 * identical to the previous `{const char* ptr; size_t len;}` on x64
 * (`const char*` ≡ `const uint8_t*` — same 8-byte pointer; `size_t` ≡
 * `int64_t` — same 8-byte width), so all ~354 runtime-C sites that consume
 * the `nova_str` typedef keep working source-compatibly. sizeof == 16. */
typedef struct {
    const uint8_t* ptr;   /* *ro u8 — immutable UTF-8 byte buffer */
    int64_t        len;   /* length in BYTES (D26: str.len = bytes) */
} nova_str;

/* Plan 90: forward-декларация nv_panic (определён `static inline` в
 * effects.h, который включается в nova_rt.h ПОСЛЕ array.h). Нужна для
 * bounds-check в nova_str_byte_at и bulk slice-операциях array.h. */
static void nv_panic(nova_str);

/* Plan 96 Ф.4 — forward-декларация nv_panic_slice_oob (определён
 * static inline в array.h, который включается в nova_rt.h ПОСЛЕ
 * этой точки). Нужна для bounds-check в nova_str_slice_panic. */
static void nv_panic_slice_oob(nova_int from, nova_int to, nova_int len);

/* Plan 90.1 — forward-декларации для новых panic-помощников.
 * Функции определены static inline в array.h ПОСЛЕ macro-instantiations,
 * но вызываются внутри NOVA_ARRAY_IMPL, которое развёртывается при include.
 * Forward-декларации здесь, до #include "array.h" — решают conflicting-types. */
static void nv_panic_insert_oob(nova_int i, nova_int len);
static void nv_panic_negative_reserve(nova_int extra);

static inline nova_str nova_str_from_cstr(const char* s) {
    /* Plan 139 Ф.0: ptr field is now `const uint8_t*` (str value-record ABI). */
    return (nova_str){ (const uint8_t*)s, (int64_t)strlen(s) };
}

/* Plan 199 Ф.3 (D418, retracts D26 §Nul-termination): `nova_fn_nova_str_terminated_ptr`
 * REMOVED. `str` is now a pure `ptr[len]` buffer with NO trailing-NUL guarantee, so
 * the one-past-end peek (`s.ptr[s.len]`) this primitive relied on is out-of-bounds and
 * has no meaning. C-FFI goes through the explicit copy-based `str.to_cstr()`
 * (std/ffi/cstr.nv, Plan 199 Ф.2/Ф.3), which allocates its own len+1 NUL-terminated
 * copy — it no longer calls this. Verified zero call sites in generated C / std. */

/* Plan 90: O(1) доступ к байту строки. bounds-checked → panic.
 * Неустранимый примитив для str-алгоритмов на Nova (lexer/find/trim). */
static inline nova_byte nova_str_byte_at(nova_str s, int64_t i) {
    if (i < 0 || (size_t)i >= s.len) {
        nv_panic((nova_str){ .ptr = "str.byte_at: index out of bounds",
                             .len = sizeof("str.byte_at: index out of bounds") - 1 });
    }
    return (nova_byte)(unsigned char)s.ptr[i];
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

/* find/rfind defined in array.h after NovaOpt_nova_int is available. */

/* Plan 199 Ф.3: `nova_str_to_upper` / `nova_str_to_lower` REMOVED — DEAD C
 * primitives (zero call sites in generated C / std; case-folding is the Nova-body
 * `str @to_ascii_upper()`/`@to_ascii_lower()` in std/runtime/string/transform.nv,
 * which route through `str.alloc_copy`). Removed here rather than left as unused
 * `static inline` dead weight while the surrounding string allocators were being
 * de-NUL-reserved. */

static inline nova_str nova_str_trim(nova_str s) {
    size_t start = 0, end = s.len;
    while (start < end && (unsigned char)s.ptr[start] <= ' ') start++;
    while (end > start && (unsigned char)s.ptr[end-1] <= ' ') end--;
    return (nova_str){ s.ptr + start, end - start };
}

/* Plan 96 Ф.4 — codepoint-indexed slice с panic-семантикой для
 * bracket-form `s[a..b]`. Отличается от `s.slice(a, b)` метода тем,
 * что OOB вызывает nv_panic (consistent с arr[a..b]); метод
 * `s.slice` оставлен с clamp-семантикой (D27 §1632 backwards-compat;
 * align→panic откладывается в Plan 94, см. [P-str-slice-clamp-vs-panic]). */
static inline nova_str nova_str_slice_panic(nova_str s, nova_int from, nova_int to) {
    /* Count total codepoints для validation. */
    nova_int total_cp = 0;
    for (size_t i = 0; i < s.len; ) {
        unsigned char b = (unsigned char)s.ptr[i];
        if      (b < 0x80) i += 1;
        else if ((b & 0xE0) == 0xC0) i += 2;
        else if ((b & 0xF0) == 0xE0) i += 3;
        else if ((b & 0xF8) == 0xF0) i += 4;
        else                          i += 1;
        total_cp++;
    }
    if (from < 0 || to < from || to > total_cp) {
        nv_panic_slice_oob(from, to, total_cp);
    }
    /* Walk UTF-8 to find byte offsets для codepoint-indices. */
    size_t byte_from = 0, byte_to = s.len;
    nova_int cp = 0;
    nova_bool found_from = (from == 0);
    for (size_t i = 0; i < s.len; ) {
        if (cp == from && !found_from) { byte_from = i; found_from = 1; }
        if (cp == to) { byte_to = i; break; }
        unsigned char b = (unsigned char)s.ptr[i];
        if      (b < 0x80) i += 1;
        else if ((b & 0xE0) == 0xC0) i += 2;
        else if ((b & 0xF0) == 0xE0) i += 3;
        else if ((b & 0xF8) == 0xF0) i += 4;
        else                          i += 1;
        cp++;
    }
    if (cp < to) byte_to = s.len;
    if (byte_from > byte_to) byte_from = byte_to;
    return (nova_str){ s.ptr + byte_from, byte_to - byte_from };
}

/* Plan 96.1: `nova_str_slice` (clamp-семантика, D26) удалён.
 * Используйте `nova_str_slice_panic` (выше) — bracket-form `s[a..b]`
 * codepoint-indexed view с **panic** при OOB (consistent с `arr[a..b]`).
 * Convergence с Rust/Go/Swift/Python (bracket-only). D9 «один очевидный
 * путь». Closes [P-str-slice-clamp-vs-panic]. */

/* nova_str_concat: concatenate two strings, allocates via nova_alloc.
 * Plan 199 Ф.3 (D418): buffer is EXACTLY a.len+b.len bytes — no trailing NUL. */
static inline nova_str nova_str_concat(nova_str a, nova_str b) {
    size_t total = a.len + b.len;
    if (total == 0) return (nova_str){ (const uint8_t*)"", 0 };
    char* buf = (char*)nova_alloc(total);
    memcpy(buf, a.ptr, a.len);
    memcpy(buf + a.len, b.ptr, b.len);
    return (nova_str){ (const uint8_t*)buf, total };
}

/* Plan 91 Ф.2: repeat / replace / pad_left / pad_right реализованы
 * в string_builder.h (после определения Nova_StringBuilder). */

static inline nova_bool nova_str_eq(nova_str a, nova_str b) {
    return a.len == b.len && memcmp(a.ptr, b.ptr, a.len) == 0;
}

/* Lexicographic byte-wise comparison.
 *
 * Returns negative if a < b, 0 if equal, positive if a > b.
 * Bootstrap MVP: byte-wise (works correctly для ASCII; UTF-8 is partial
 * — byte order совпадает с codepoint order для valid UTF-8 кроме edge
 * cases). Полное Unicode-aware сравнение (locale collation) — production
 * milestone.
 *
 * Используется std.runtime.string `@lt`/`@gt`/`@le`/`@ge` и Binary
 * BinOp::Lt/Gt/Le/Ge operator overload codegen для nova_str. */
static inline nova_int nova_str_cmp(nova_str a, nova_str b) {
    size_t min_len = a.len < b.len ? a.len : b.len;
    int r = memcmp(a.ptr, b.ptr, min_len);
    if (r != 0) return (nova_int)r;
    if (a.len < b.len) return -1;
    if (a.len > b.len) return 1;
    return 0;
}
static inline nova_bool nova_str_lt(nova_str a, nova_str b) { return nova_str_cmp(a, b) <  0; }
static inline nova_bool nova_str_le(nova_str a, nova_str b) { return nova_str_cmp(a, b) <= 0; }
static inline nova_bool nova_str_gt(nova_str a, nova_str b) { return nova_str_cmp(a, b) >  0; }
static inline nova_bool nova_str_ge(nova_str a, nova_str b) { return nova_str_cmp(a, b) >= 0; }

/* Plan 52 Ф.22: DoS-resistant hash (SipHash-1-3 + per-process random seed).
 *
 * SipHash by Jean-Philippe Aumasson & Daniel J. Bernstein (public domain).
 * Используется как default hash в Rust HashMap, Python dict, Ruby Hash, Perl —
 * защищает от hash-flooding атак (attacker control'нл keys → O(n²) deg).
 *
 * Раньше Nova использовал FNV-1a без seed — vulnerable: с фиксированным
 * hash function attacker может precompute collision'ы для known target.
 * SipHash + per-process random seed делает collision-precompute невозможным
 * (seed unknown во время атаки).
 *
 * Variant: SipHash-1-3 (1 compression round, 3 finalization rounds) — Rust
 * default. Trade-off: ~2× быстрее SipHash-2-4 при сравнимой security для
 * default-hashmap usage. Для cryptographic уровня — SipHash-2-4 (через
 * #[secure_hash], future). */

/* Per-process random seed. Инициализируется lazy при первом
 * hash-вызове (или явно в nova_runtime_init для предсказуемости) —
 * через getrandom() / BCryptGenRandom (cryptographically secure).
 *
 * `nova_hash_seed_ensure_init` — idempotent thread-safe init. Вызывается
 * на entry в каждый hash-helper. На hot path после init — single
 * atomic load `_hash_seed_inited` (predicted-true).
 *
 * Стоимость per-hash check: один atomic load + branch (~1ns на x86_64,
 * predict-true). Negligible vs SipHash compute (~10ns/8bytes). */
extern uint64_t nova_hash_seed_k0;
extern uint64_t nova_hash_seed_k1;
extern void nova_hash_seed_ensure_init(void);

#define NOVA_SIP_ROTL(x, b) (uint64_t)(((x) << (b)) | ((x) >> (64 - (b))))
#define NOVA_SIP_ROUND(v0, v1, v2, v3) do { \
    v0 += v1; v1 = NOVA_SIP_ROTL(v1, 13); v1 ^= v0; v0 = NOVA_SIP_ROTL(v0, 32); \
    v2 += v3; v3 = NOVA_SIP_ROTL(v3, 16); v3 ^= v2; \
    v0 += v3; v3 = NOVA_SIP_ROTL(v3, 21); v3 ^= v0; \
    v2 += v1; v1 = NOVA_SIP_ROTL(v1, 17); v1 ^= v2; v2 = NOVA_SIP_ROTL(v2, 32); \
} while (0)

/* SipHash-1-3 core: c=1 compression, d=3 finalization. */
static inline uint64_t nova_siphash13(const uint8_t* data, size_t len,
                                      uint64_t k0, uint64_t k1) {
    uint64_t v0 = 0x736f6d6570736575ULL ^ k0;
    uint64_t v1 = 0x646f72616e646f6dULL ^ k1;
    uint64_t v2 = 0x6c7967656e657261ULL ^ k0;
    uint64_t v3 = 0x7465646279746573ULL ^ k1;
    const uint8_t* end = data + (len - (len % 8));
    for (; data != end; data += 8) {
        uint64_t m;
        memcpy(&m, data, 8);
        v3 ^= m;
        NOVA_SIP_ROUND(v0, v1, v2, v3);
        v0 ^= m;
    }
    uint64_t b = ((uint64_t)len) << 56;
    switch (len & 7) {
        case 7: b |= ((uint64_t)data[6]) << 48; /* fallthrough */
        case 6: b |= ((uint64_t)data[5]) << 40; /* fallthrough */
        case 5: b |= ((uint64_t)data[4]) << 32; /* fallthrough */
        case 4: b |= ((uint64_t)data[3]) << 24; /* fallthrough */
        case 3: b |= ((uint64_t)data[2]) << 16; /* fallthrough */
        case 2: b |= ((uint64_t)data[1]) << 8;  /* fallthrough */
        case 1: b |= ((uint64_t)data[0]);
        case 0: break;
    }
    v3 ^= b;
    NOVA_SIP_ROUND(v0, v1, v2, v3);
    v0 ^= b;
    v2 ^= 0xff;
    NOVA_SIP_ROUND(v0, v1, v2, v3);
    NOVA_SIP_ROUND(v0, v1, v2, v3);
    NOVA_SIP_ROUND(v0, v1, v2, v3);
    return v0 ^ v1 ^ v2 ^ v3;
}

static inline nova_int nova_str_hash(nova_str s) {
    nova_hash_seed_ensure_init();
    return (nova_int)nova_siphash13((const uint8_t*)s.ptr, s.len,
                                    nova_hash_seed_k0, nova_hash_seed_k1);
}
static inline nova_int nova_int_hash(nova_int v) {
    nova_hash_seed_ensure_init();
    uint64_t bits = (uint64_t)v;
    return (nova_int)nova_siphash13((const uint8_t*)&bits, sizeof(bits),
                                    nova_hash_seed_k0, nova_hash_seed_k1);
}
static inline nova_int nova_bool_hash(nova_bool v) {
    /* Bool: 2 значения, DoS не релевантен (не может быть collision storm
     * на 2-value space). Простой identity. */
    return (nova_int)(uint64_t)(v != 0);
}
static inline nova_int nova_f64_hash(nova_f64 v) {
    nova_hash_seed_ensure_init();
    uint64_t bits = 0;
    memcpy(&bits, &v, sizeof(bits));
    return (nova_int)nova_siphash13((const uint8_t*)&bits, sizeof(bits),
                                    nova_hash_seed_k0, nova_hash_seed_k1);
}

/* nova_str_char_len: count UTF-8 code points (not bytes).
 * Leading bytes of multi-byte sequences start with 11xxxxxx; continuation
 * bytes start with 10xxxxxx and are skipped. ASCII bytes (0xxxxxxx) count 1. */
static inline nova_int nova_str_char_len(nova_str s) {
    nova_int count = 0;
    for (size_t i = 0; i < s.len; i++) {
        unsigned char c = (unsigned char)s.ptr[i];
        if ((c & 0xC0) != 0x80) count++;
    }
    return count;
}

/* nova_str_char_at объявлен в array.h (после NovaOpt_nova_int instantiation). */

/* nova_int_to_str: convert integer to string */
static inline nova_str nova_int_to_str(nova_int v) {
    char* buf = (char*)nova_alloc(24);
    int n = snprintf(buf, 24, "%lld", (long long)v);
    return (nova_str){ buf, (size_t)(n < 0 ? 0 : n) };
}

/* ---- shortest round-trip float → decimal (Plan 180 [M-180-f64-shortest-roundtrip]) ----
 *
 * SINGLE SOURCE OF TRUTH for every float→str path in the language: `str.from`,
 * `@display`/`@debug`, `${x}` interpolation, `StringBuilder.append`, AND direct
 * `println(float)` all funnel here (conv.h `nova_f64_to_str`/`nova_f32_to_str`
 * are thin GC-allocating wrappers over these; the print helpers below print the
 * bytes directly).
 *
 * Prior formatting was `snprintf("%g")` = 6 significant figures — LOSSY for
 * arbitrary f64 (`3.141592653589793` → "3.14159", `1234567.89` → "1.23457e+06"),
 * silently breaking `decode(encode(v)) == v`. These emit the MINIMAL decimal
 * string whose `strtod`/`strtof` re-parses bit-for-bit to `v`.
 *
 * Method (style-preserving, zero-churn on legacy output): try default `%g`
 * (6 sig-figs) first — every value already faithful in <=6 sig-figs keeps its
 * historical rendering (incl. "100000", "0.1"); otherwise escalate precision
 * (7..17 for f64, 7..9 for f32) and take the FIRST exact round-trip. `%.17g`
 * (f64) / `%.9g` (f32) is an exact round-trip for every finite value, so the
 * loop always terminates. Non-finite (inf/-inf/nan): `%g` renders them and the
 * decimal probe is skipped (NaN != NaN, inf has no decimal form).
 *
 * Writes into caller `buf` (>= 32 bytes; worst case `%.17g` == 24 chars + NUL),
 * returns the byte length. */
static inline int nova_f64_shortest(nova_f64 v, char* buf) {
    int n;
    if (isnan(v) || isinf(v)) { n = snprintf(buf, 32, "%g", v); return n < 0 ? 0 : n; }
    n = snprintf(buf, 32, "%g", v);
    if (n >= 0 && strtod(buf, NULL) == v) return n;
    for (int prec = 7; prec <= 17; prec++) {
        n = snprintf(buf, 32, "%.*g", prec, v);
        if (n < 0) return 0;
        if (strtod(buf, NULL) == v) return n;
    }
    return n < 0 ? 0 : n;
}
static inline int nova_f32_shortest(nova_f32 v, char* buf) {
    double dv = (double)v;
    int n;
    if (isnan(dv) || isinf(dv)) { n = snprintf(buf, 32, "%g", dv); return n < 0 ? 0 : n; }
    n = snprintf(buf, 32, "%g", dv);
    if (n >= 0 && strtof(buf, NULL) == v) return n;
    for (int prec = 7; prec <= 9; prec++) {
        n = snprintf(buf, 32, "%.*g", prec, dv);
        if (n < 0) return 0;
        if (strtof(buf, NULL) == v) return n;
    }
    return n < 0 ? 0 : n;
}

/* Plan 208 Ф.1 (D422 §5) — buffer-form float formatter, ADDITIVE alongside the
 * existing str-returning `nova_f64_to_str`/`nova_fmt_f64_body` (conv.h) — those
 * keep backing the CURRENT `.nv` prelude / interpolation path unchanged. This
 * is the SOLE C-extern surface the Unified Formatter design keeps (D422 §5:
 * "float — ЕДИНСТВЕННЫЙ C-extern, dtoa непортируем"); everything else
 * (int/bool/char/радикс/pad) moves to `.nv` (`std/src/runtime/fmt_buf.nv`).
 *
 * Literal C symbol name — [D282](../../spec/decisions/
 * 08-runtime.md#d282) `extern "C" fn` contract: the `.nv`-side declaration
 * (`extern "C" fn nova_f64_fmt(...)`) calls this exact name, resolved purely
 * via header visibility (nova_rt.h is `#include`d into every generated C
 * translation unit — no separate forward-declaration emitted by codegen for
 * `extern "C" fn`, Plan 91.12 Ф.-1). Kept `static inline` (like every other
 * nova_rt.h primitive) so multiple .c translation units that include this
 * header do NOT collide with duplicate external-linkage definitions.
 *
 * Plan 208 Ф.4R §10R-Д3 (owner 2026-07-21, `docs/plans/208-unified-formatter.md`
 * §10R-Д, source of truth): renamed from `f64_fmt_into` — the `_into` suffix
 * is retired repo-wide (it meant two different things: this C-extern AND the
 * now-retired `.nv` bridge functions, while every member of the family writes
 * into `(buf, cap)` regardless of suffix). `nova_`-prefixed header-style name
 * (matches `nova_f64_shortest`/`nova_print_f64` siblings in this same file) —
 * still a D282 literal name (whatever the `.nv` extern declares IS the C
 * symbol, no compiler mangling), just a different literal than before.
 *
 * kind: 0=Shortest (delegates to `nova_f64_shortest`, the existing
 *       round-trip-minimal engine — reused verbatim, not reimplemented),
 *       1=Fixed    (`%.*f` fixed-point, `prec` decimal places),
 *       2=Sci      (`%.*e` scientific, `prec` decimal places).
 * `FloatKind` is a `.nv`-side enum (`std/src/runtime/fmt_buf/core.nv`); the
 * `.nv`-wrapper `f64_fmt` converts it to this `int` at the ABI boundary
 * (enums do not cross `extern "C"` directly, D422 §5).
 *
 * Writes at most `cap` bytes into `buf` (TRUNCATING defensively if `cap` is
 * smaller than the rendered length — no overflow, matches the buffer-safety
 * discipline of `int_fmt`/`bool_fmt`/`char_fmt` in fmt_buf.nv). Returns the
 * number of bytes actually written. `tmp[400]` covers the worst case: a
 * `%.*f` render of `DBL_MAX` (~309 integer digits) at the widest clamped
 * precision (40) plus sign/decimal-point — comfortably under 400. */
static inline nova_int nova_f64_fmt(double v, uint8_t* buf, nova_int cap, nova_int kind, nova_int prec) {
    char tmp[400];
    int n;
    if (kind == 1) {
        int p = (prec < 0) ? 6 : ((prec > 340) ? 340 : (int)prec);
        n = snprintf(tmp, sizeof(tmp), "%.*f", p, v);
    } else if (kind == 2) {
        int p = (prec < 0) ? 6 : ((prec > 40) ? 40 : (int)prec);
        n = snprintf(tmp, sizeof(tmp), "%.*e", p, v);
    } else {
        n = nova_f64_shortest(v, tmp);
    }
    if (n < 0) n = 0;
    if (cap < 0) cap = 0;
    nova_int result = (nova_int)n;
    if (result > cap) result = cap;
    if (result > 0) memcpy(buf, tmp, (size_t)result);
    return result;
}

/* f32-собрат nova_f64_fmt (2026-07-20, владелец: SB @append(f32) без
 * str-аллокации): shortest-only — оси Fixed/Sci для f32 не нужны (D422:
 * пользовательский spec-путь идёт через f64-ось). Тот же
 * defensive-truncate контракт. Разрешается D282 literal-name extern'ом.
 * Переименован из `f32_fmt_into` (Ф.4R §10R-Д3, тот же мотив, что у
 * `nova_f64_fmt` выше — суффикс `_into` упразднён репо-wide). */
static inline nova_int nova_f32_fmt(nova_f32 v, uint8_t* buf, nova_int cap) {
    char tmp[64];
    int n = nova_f32_shortest(v, tmp);
    if (n < 0) n = 0;
    nova_int m = (nova_int)n < cap ? (nova_int)n : cap;
    memcpy(buf, tmp, (size_t)m);
    return m;
}

/* ---- println ---- */
/* Variadic nova_println is generated per call-site. Each arg is printed
 * with its own helper depending on type. */

static inline void nova_print_int(nova_int v)  { printf("%lld", (long long)v); }
/* Plan 180: direct println(float) uses the same shortest-round-trip formatter
 * as str.from / interpolation — no more `%g` 6-sig divergence between
 * `println(x)` and `println("${x}")`. */
static inline void nova_print_f64(nova_f64 v)  { char buf[32]; int n = nova_f64_shortest(v, buf); fwrite(buf, 1, (size_t)n, stdout); }
static inline void nova_print_f32(nova_f32 v)  { char buf[32]; int n = nova_f32_shortest(v, buf); fwrite(buf, 1, (size_t)n, stdout); }
static inline void nova_print_bool(nova_bool v) { printf("%s", v ? "true" : "false"); }
static inline void nova_print_str(nova_str v)   { fwrite(v.ptr, 1, v.len, stdout); }
static inline void nova_print_char(nova_int cp) {
    if (cp < 0 || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) cp = 0xFFFD;
    char buf[4]; size_t n;
    if (cp < 0x80)        { buf[0]=(char)cp; n=1; }
    else if (cp < 0x800)  { buf[0]=(char)(0xC0|(cp>>6)); buf[1]=(char)(0x80|(cp&0x3F)); n=2; }
    else if (cp < 0x10000){ buf[0]=(char)(0xE0|(cp>>12)); buf[1]=(char)(0x80|((cp>>6)&0x3F)); buf[2]=(char)(0x80|(cp&0x3F)); n=3; }
    else                  { buf[0]=(char)(0xF0|(cp>>18)); buf[1]=(char)(0x80|((cp>>12)&0x3F)); buf[2]=(char)(0x80|((cp>>6)&0x3F)); buf[3]=(char)(0x80|(cp&0x3F)); n=4; }
    fwrite(buf, 1, n, stdout);
}
static inline void nova_print_newline(void)     { putchar('\n'); }

/* ---- Unit ---- */
typedef struct { char _dummy; } nova_unit;
#define NOVA_UNIT ((nova_unit){0})

/* ---- Plan 61 Ф.1: TypeId runtime infrastructure ---- *
 * Должен идти до effects.h — позже Plan 61 Ф.2 effects.h будет
 * использовать NovaTypeId в Fail[any] erased path. */
#include "typeid.h"

/* ---- Arrays (Phase 6) ---- */
#include "array.h"

/* ---- Effects (Phase 4) — also defines NovaTestFrame + nova_assert ---- */
#include "effects.h"

/* ---- Plan 44.2 Etap 1: per-thread fiber stack arena (Linux/macOS only) ---- */
#include "fiber_arena.h"

/* ---- Plan 44.1 Ф.1: thread-safety primitives — moved up для Plan 44.5 L5
 * NovaFiberQueue.pending_remote / first_error_atomic в fibers.h. ---- */
#include "sync.h"

/* ---- Fibers / spawn (Phase 5) ---- */
#include "fibers.h"

/* ---- Plan 04 Этап 6: Buffer удалён, заменён split'ом ---- */
/* string_builder.h — Plan 109 (D179): только UTF-8 utility helpers
 * (_nova_validate_utf8 для str.from_bytes_*). StringBuilder type сам теперь
 * Nova-defined в std/runtime/string_builder.nv.
 * write_buffer.h / read_buffer.h — Plan 91.12 (D126 retract, 2026-06-01)
 * удалены целиком: WriteBuffer/ReadBuffer теперь Nova-defined records
 * в std/runtime/{write,read}_buffer.nv (no C-side helpers needed). */
#include "string_builder.h"

/* ---- Plan 13: umbrella headers для runtime stdlib API ----
 *
 * `string.h` / `math.h` — stable include-points для str и f64/f32
 * runtime-функций. Сейчас просто re-export'ят nova_rt.h (str) и
 * `<math.h>` (math). Future migration переносит фактические
 * декларации сюда.
 *
 * Включаются в конце чтобы не было forward-decl issues.
 */
/* Note: эти headers re-include nova_rt.h, поэтому помещаем в самый
 * низ — header-guard в nova_rt.h защищает от re-entry. Для
 * generated кода они не критичны (codegen использует nova_rt.h),
 * но нужны как stable include-points для future C-кода. */
/* Не включаем здесь — circular из-за того что они #include
 * "nova_rt.h". Вместо этого они доступны как отдельные include'ы
 * в C-output codegen'а. См. docs/plans/13-runtime-stdlib-and-autogen.md.
 */

/* ---- Plan 22 Ф.2: глобальный uv_loop_t lifecycle ---- */
#include "eventloop.h"

/* ---- Plan 22 Ф.3 (D93): нормативный park/wake API ---- */
/* После fibers.h (NovaFiberQueue полный тип). */
#include "nova_sched.h"  /* renamed from sched.h to avoid Linux <sched.h> collision */

/* sync.h уже включён выше (перед fibers.h, для NovaFiberQueue
 * atomic-полей Plan 44.5 L5). Header guard защитит от re-entry. */

/* Plan 44.5 Layer 5: declarations для nova_runtime_is_initialized,
 * nova_runtime_spawn_into, nova_runtime_signal_main — codegen эмитит
 * эти вызовы в каждой spawn-call-site и entry-function. Без явного
 * include'а компилятор использует implicit-int declaration → ABI
 * mismatch (bool vs int return) → reads garbage. */
#include "runtime.h"

/* ---- Plan 21 (D91): capability-split Channels ---- */
/* После sched.h — channels.h использует nova_sched_park/wake/register. */
#include "channels.h"

/* ---- Plan 18 std.sync: fiber-aware AtomicInt / Mutex / WaitGroup ----
 * После nova_sched.h (park/wake API) + fibers.h (TLS scope/slot). */
#include "sync_primitives.h"

/* ---- Plan 83.12: std/net — async TCP/UDP via libuv ----
 * После sync_primitives.h (nova_alloc_uncollectable) + nova_sched.h
 * (park/wake) + eventloop.h (nova_loop_defer_close). Only when libuv
 * is available. */
#ifdef NOVA_USE_LIBUV
#  include "net.h" /* Plan 183 Ф.1 / Plan 182 Ф.1: reworked std/net substrate (D407); file renamed from net2.h */
#  include "fs.h"   /* Plan 176 Ф.2: std/fs — async uv_fs_* via libuv */
#endif

/* ---- Plan 33.1 Ф.4 (D24): contracts runtime helper ----
 * После effects.h + fibers.h — nova_contract_violation использует
 * NovaFailFrame routing + NovaTestFrame. */
#include "contracts.h"

/* Plan 22 Ф.4: Windows headers подтянутые libuv (rpcndr.h, etc.)
 * захламляют namespace макросами типа `small`, `interface` и т.д.
 * Это collides с Nova-generated кодом (e.g. `int32_t small = ...`).
 * Undef'им известные коллизии чтобы generated .c компилировался. */
#ifdef NOVA_USE_LIBUV
#  ifdef small
#    undef small
#  endif
#  ifdef interface
#    undef interface
#  endif
#  ifdef ERROR
#    undef ERROR
#  endif
#endif

/* Plan 56 Ф.1: vtable dispatch для bound-K methods в erased generics.
 * Must be included AFTER nova_str / nova_int / array.h т.к. зависит
 * от nova_str_eq, nova_str_hash, etc. */
#include "vtables.h"

/* Plan 57: bench DSL runtime (header-only). Подключается после alloc.h
 * (uses nova_gc_alloc_count) и eventloop.h (optional uv_hrtime). */
#include "bench.h"

/* Plan 176 Ф.1 (D322 §3c): std/io console byte hooks (io_read_fd / io_write_fd)
 * for the `Io` effect real handler. Header-only (C stdio FILE*). */
#include "io_console.h"

/* Plan 176 Ф.3 (D324): std/os native hooks (env / args / cwd / dirs / process)
 * for the `Os` effect real handler. Header-only (getenv/getcwd/...); argv is
 * captured by main() via os_set_args. Included after nova_str / nova_alloc
 * are defined (used by its nova_str wrappers). */
#include "os_env.h"

/* Plan 115 D214 Ф.2: tuple-return FFI test shim. Header-only inline
 * helpers used by `nova_tests/plan115/t2_external_fn_tuple_ok.nv`.
 * Plan 115 v1 ships minimum FFI scaffolding here; full user-side shim
 * pipeline (`nova build --c-shim path/to/file.c`) — followup
 * `[M-115-ffi-build-pipeline]`. */
#include "plan115_ffi_test.h"

/* Plan 115 D214 Ф.3 / A7: sqlite_mini_ffi.h moved → `examples/ffi/`
 * (user-side location). Now wired through `[M-115-ffi-build-pipeline]` —
 * `nova_tests/nova.toml [ffi] c_shims` force-includes header per package.
 * См. examples/ffi/sqlite_mini_ffi.h + nova_tests/nova.toml. */

/* Plan 118.1 Ф.1 (D-block — see plan-doc): RawMem byte-level memory
 * intrinsics. Underlying C primitives для FFI / driver / embedded work.
 *
 * Naming convention: `Nova_RawMem_static_*` matches Nova's static-method
 * external-fn binding `RawMem.copy` → C `Nova_RawMem_static_copy`
 * (codegen mangles static methods as `Nova_<Type>_static_<method>`). Plain
 * wrappers around libc memmove/memcpy/memset/memcmp.
 *
 * Pointer types: const void* / void* — opaque (caller casts *T to *u8
 * or *T as appropriate; codegen handles ABI mapping). usize for byte
 * count (D226 §3 amend — usize = u64 alias, ABI-compatible с C size_t).
 */
static inline void Nova_RawMem_static_copy(const void* src, void* dst, uint64_t n) {
    memmove(dst, src, (size_t)n);
}
static inline void Nova_RawMem_static_copy_nonoverlapping(const void* src, void* dst, uint64_t n) {
    memcpy(dst, src, (size_t)n);
}
static inline void Nova_RawMem_static_fill(void* dst, nova_byte val, uint64_t n) {
    memset(dst, (int)val, (size_t)n);
}
static inline nova_int Nova_RawMem_static_compare(const void* a, const void* b, uint64_t n) {
    int r = memcmp(a, b, (size_t)n);
    /* Normalize libc memcmp's implementation-defined non-zero к -1/+1. */
    return (nova_int)(r > 0 ? 1 : (r < 0 ? -1 : 0));
}

/* Plan 131 Ф.1: GC-allocation intrinsics exposed to Nova.
 *
 * Nova naming: RawMem.alloc / RawMem.alloc_uncollectable /
 *              RawMem.free_uncollectable.
 * Codegen mangles static-method external fn as
 *   Nova_<Type>_static_<method> (ExternalRegistry convention).
 *
 * Return type `nova_byte*` matches Nova `*mut u8` → C `nova_byte*`
 * mapping in external_registry.rs (TypeRef::Mut(TypeRef::Pointer(u8))).
 *
 * CONTRACT: both alloc functions return zeroed memory (alloc.h guarantee).
 * nova_free_uncollectable signature: `void nova_free_uncollectable(void*)`.
 */
static inline nova_byte* Nova_RawMem_static_alloc(uint64_t n) {
    return (nova_byte*)nova_alloc((size_t)n);
}
static inline nova_byte* Nova_RawMem_static_alloc_uncollectable(uint64_t n) {
    return (nova_byte*)nova_alloc_uncollectable((size_t)n);
}
static inline void Nova_RawMem_static_free_uncollectable(nova_byte* ptr) {
    nova_free_uncollectable((void*)ptr);
}

#endif /* NOVA_RT_H */
