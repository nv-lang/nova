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
#include "numeric.h"  /* Plan 74: f64/f32 ↔ uN IEEE 754 bit-cast */
/* conv.h подключается в array.h (после nova_alloc и определения nova_str). */

/* ---- Primitive types ---- */
typedef int64_t  nova_int;
/* Plan 70.3: distinct typedef для char — same underlying int64_t storage,
 * но distinct C type для generic mangling. Без этого `Option[char]` и
 * `Option[int]` оба mangle'ятся в `NovaOpt_nova_int` → silent type collapse
 * в mono'd generic instances (см. docs/plans/70.3-char-int-mono-distinction.md).
 * Zero ABI cost — typedef alias, не отдельный type. Compiler-side mangling
 * generates distinct `Nova*_nova_char` vs `Nova*_nova_int` C-names. */
typedef int64_t  nova_char;
typedef double   nova_f64;
typedef float    nova_f32;
typedef bool     nova_bool;
/* Plan 115 D214: distinct typedef для `ptr` — same underlying `void*` storage,
 * но distinct C type. Mirrors Plan 70.3 nova_char rationale: `Option[ptr]` и
 * other generics не должны silent-stomp'ить в erased `void*` mono mangling.
 * Также explicit `ptr` value distinguishable от erased generic-T placeholder
 * на codegen уровне (TupleLit + infer_expr_c_type решают mono'd vs legacy
 * fallback). Zero ABI cost — typedef alias `void*`. */
typedef void*    nova_ptr;

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

/* ---- String ---- */
typedef struct {
    const char* ptr;
    size_t      len;
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
    return (nova_str){ s, strlen(s) };
}

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

/* nova_str_to_upper: allocates via nova_alloc, returns new nova_str */
static inline nova_str nova_str_to_upper(nova_str s) {
    char* buf = (char*)nova_alloc(s.len + 1);
    for (size_t i = 0; i < s.len; i++) {
        unsigned char c = (unsigned char)s.ptr[i];
        buf[i] = (c >= 'a' && c <= 'z') ? (char)(c - 32) : (char)c;
    }
    buf[s.len] = '\0';
    return (nova_str){ buf, s.len };
}

static inline nova_str nova_str_to_lower(nova_str s) {
    char* buf = (char*)nova_alloc(s.len + 1);
    for (size_t i = 0; i < s.len; i++) {
        unsigned char c = (unsigned char)s.ptr[i];
        buf[i] = (c >= 'A' && c <= 'Z') ? (char)(c + 32) : (char)c;
    }
    buf[s.len] = '\0';
    return (nova_str){ buf, s.len };
}

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

/* nova_str_concat: concatenate two strings, allocates via nova_alloc */
static inline nova_str nova_str_concat(nova_str a, nova_str b) {
    size_t total = a.len + b.len;
    char* buf = (char*)nova_alloc(total + 1);
    memcpy(buf, a.ptr, a.len);
    memcpy(buf + a.len, b.ptr, b.len);
    buf[total] = '\0';
    return (nova_str){ buf, total };
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

/* ---- println ---- */
/* Variadic nova_println is generated per call-site. Each arg is printed
 * with its own helper depending on type. */

static inline void nova_print_int(nova_int v)  { printf("%lld", (long long)v); }
static inline void nova_print_f64(nova_f64 v)  { printf("%g", v); }
static inline void nova_print_f32(nova_f32 v)  { printf("%g", (double)v); }
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
#include "string_builder.h"
#include "write_buffer.h"
#include "read_buffer.h"

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
#  include "net.h"
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

/* Plan 115 D214 Ф.2: tuple-return FFI test shim. Header-only inline
 * helpers used by `nova_tests/plan115/t2_external_fn_tuple_ok.nv`.
 * Plan 115 v1 ships minimum FFI scaffolding here; full user-side shim
 * pipeline (`nova build --c-shim path/to/file.c`) — followup
 * `[M-115-ffi-build-pipeline]`. */
#include "plan115_ffi_test.h"

/* Plan 115 D214 Ф.3 / A7: embedded mini-sqlite-equivalent для end-to-end
 * FFI sample. In-memory key-value store с sqlite-like API.
 * Plan 115 V1 — без external libsqlite3 dependency (followup
 * `[M-115-examples-ffi-real-build]` добавит real libsqlite3 link через
 * vcpkg integration). Используется в `nova_tests/plan115/t4_sqlite_*`
 * фикстурах + `examples/ffi/sqlite_mini.nv`. */
#include "sqlite_mini_ffi.h"

#endif /* NOVA_RT_H */
