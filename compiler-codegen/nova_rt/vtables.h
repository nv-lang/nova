#ifndef NOVA_RT_VTABLES_H
#define NOVA_RT_VTABLES_H

/*
 * Plan 56 — Vtable dispatch для bound-K methods в erased generics.
 *
 * Когда compiler не может monomorphize (e.g. erased emit для generic
 * stdlib types или recursive generic calls с unresolved K/V), bound
 * methods (Hashable.hash, Hashable.eq, Comparable.lt, Display.to_str)
 * dispatch'аются через vtable.
 *
 * ABI design:
 * - `self` передаётся как `const void*`. Для value types — указатель
 *   на stack/heap slot. Для pointer types (Nova_X*) — pointer хранит
 *   сам Nova_X* (т.е. `const void*` value — это `Nova_X*` value).
 *   Thunks know how to cast based на K type.
 * - Vtable structs — `static const`, immutable, thread-safe by default.
 * - Layout stability: layout — ABI surface, изменения через major
 *   version bump (когда package ecosystem заработает — Plan 03).
 *
 * Bootstrap-set of vtables:
 *   1. NovaVtable_Hashable — hash + eq (HashMap / HashSet bound).
 *   2. NovaVtable_Comparable — lt + le + gt + ge + ne (default) + eq.
 *      Используется sorted collections (future BTreeMap).
 *   3. NovaVtable_Display — to_str (debugging, error formatting).
 *
 * Multi-bound: caller передаёт несколько vtables как hidden ABI params.
 * Например, `fn f[T Hashable + Display](x T)` принимает скрытые
 * `_vt_T_h: const NovaVtable_Hashable*` и `_vt_T_d: const NovaVtable_Display*`.
 */

#include <stdint.h>

/* Forward declarations of primitive types (defined in nova_rt.h). */
#ifndef NOVA_RT_H
typedef int64_t  nova_int;
typedef double   nova_f64;
typedef bool     nova_bool;
typedef uint8_t  nova_byte;
typedef struct { const char* ptr; size_t len; } nova_str;
#endif

/* ============================================================
 * NovaVtable_Hashable — для HashMap / HashSet bound K.
 *
 * Required: hash, eq.
 * `self` and `other` — same runtime type (Self).
 * ============================================================ */
typedef struct NovaVtable_Hashable {
    uint64_t (*hash)(const void* self);
    nova_bool (*eq)(const void* self, const void* other);
} NovaVtable_Hashable;

/* ============================================================
 * NovaVtable_Comparable — для sorted collections / sort algorithm.
 *
 * Required: lt, le, gt, ge.
 * Default (если concrete K не override): ne (= !eq).
 * Eq — inherited semantics (compare K через memcmp когда possible,
 * иначе требует separate Hashable bound).
 * ============================================================ */
typedef struct NovaVtable_Comparable {
    nova_bool (*lt)(const void* self, const void* other);
    nova_bool (*le)(const void* self, const void* other);
    nova_bool (*gt)(const void* self, const void* other);
    nova_bool (*ge)(const void* self, const void* other);
    nova_bool (*eq)(const void* self, const void* other);
    nova_bool (*ne)(const void* self, const void* other);  /* default = !eq */
} NovaVtable_Comparable;

/* ============================================================
 * NovaVtable_Display — для error formatting / debug.
 *
 * Required: to_str.
 * ============================================================ */
typedef struct NovaVtable_Display {
    nova_str (*to_str)(const void* self);
} NovaVtable_Display;

/* ============================================================
 * Built-in primitive thunks + vtables.
 *
 * Each primitive K gets:
 *   - thunks `_vt_<K>_<method>` — cast self/other в правильный type
 *     and call native function.
 *   - vtable instance `_vt_<Protocol>_<K>` — static const.
 *
 * `static inline` на thunks — C compiler inline'ит в vtable call site
 * после dead-code elimination (если call site known concrete K).
 * ============================================================ */

/* ---- nova_int ---- */
static inline uint64_t _vt_nova_int_hash(const void* self) {
    /* Same as std/runtime/hash.nv: identity hash for int. */
    return (uint64_t)(*(const nova_int*)self);
}
static inline nova_bool _vt_nova_int_eq(const void* self, const void* other) {
    return (*(const nova_int*)self) == (*(const nova_int*)other);
}
static inline nova_bool _vt_nova_int_lt(const void* self, const void* other) {
    return (*(const nova_int*)self) < (*(const nova_int*)other);
}
static inline nova_bool _vt_nova_int_le(const void* self, const void* other) {
    return (*(const nova_int*)self) <= (*(const nova_int*)other);
}
static inline nova_bool _vt_nova_int_gt(const void* self, const void* other) {
    return (*(const nova_int*)self) > (*(const nova_int*)other);
}
static inline nova_bool _vt_nova_int_ge(const void* self, const void* other) {
    return (*(const nova_int*)self) >= (*(const nova_int*)other);
}
static inline nova_bool _vt_nova_int_ne(const void* self, const void* other) {
    return !_vt_nova_int_eq(self, other);
}

static const NovaVtable_Hashable _vt_Hashable_nova_int = {
    .hash = _vt_nova_int_hash,
    .eq   = _vt_nova_int_eq,
};
static const NovaVtable_Comparable _vt_Comparable_nova_int = {
    .lt = _vt_nova_int_lt,
    .le = _vt_nova_int_le,
    .gt = _vt_nova_int_gt,
    .ge = _vt_nova_int_ge,
    .eq = _vt_nova_int_eq,
    .ne = _vt_nova_int_ne,
};

/* ---- nova_bool ---- */
static inline uint64_t _vt_nova_bool_hash(const void* self) {
    return (uint64_t)(*(const nova_bool*)self ? 1 : 0);
}
static inline nova_bool _vt_nova_bool_eq(const void* self, const void* other) {
    return (*(const nova_bool*)self) == (*(const nova_bool*)other);
}
static const NovaVtable_Hashable _vt_Hashable_nova_bool = {
    .hash = _vt_nova_bool_hash,
    .eq   = _vt_nova_bool_eq,
};

/* ---- nova_byte ---- */
static inline uint64_t _vt_nova_byte_hash(const void* self) {
    return (uint64_t)(*(const nova_byte*)self);
}
static inline nova_bool _vt_nova_byte_eq(const void* self, const void* other) {
    return (*(const nova_byte*)self) == (*(const nova_byte*)other);
}
static inline nova_bool _vt_nova_byte_lt(const void* self, const void* other) {
    return (*(const nova_byte*)self) < (*(const nova_byte*)other);
}
static inline nova_bool _vt_nova_byte_le(const void* self, const void* other) {
    return (*(const nova_byte*)self) <= (*(const nova_byte*)other);
}
static inline nova_bool _vt_nova_byte_gt(const void* self, const void* other) {
    return (*(const nova_byte*)self) > (*(const nova_byte*)other);
}
static inline nova_bool _vt_nova_byte_ge(const void* self, const void* other) {
    return (*(const nova_byte*)self) >= (*(const nova_byte*)other);
}
static inline nova_bool _vt_nova_byte_ne(const void* self, const void* other) {
    return !_vt_nova_byte_eq(self, other);
}
static const NovaVtable_Hashable _vt_Hashable_nova_byte = {
    .hash = _vt_nova_byte_hash,
    .eq   = _vt_nova_byte_eq,
};
static const NovaVtable_Comparable _vt_Comparable_nova_byte = {
    .lt = _vt_nova_byte_lt,
    .le = _vt_nova_byte_le,
    .gt = _vt_nova_byte_gt,
    .ge = _vt_nova_byte_ge,
    .eq = _vt_nova_byte_eq,
    .ne = _vt_nova_byte_ne,
};

/* ---- nova_f64 ---- */
/* IEEE 754: NaN != NaN, so eq возвращает false для NaN. */
static inline uint64_t _vt_nova_f64_hash(const void* self) {
    /* Bit-pattern hash (ignores NaN payload variation). */
    uint64_t bits;
    nova_f64 v = *(const nova_f64*)self;
    memcpy(&bits, &v, sizeof(bits));
    return bits;
}
static inline nova_bool _vt_nova_f64_eq(const void* self, const void* other) {
    return (*(const nova_f64*)self) == (*(const nova_f64*)other);
}
static inline nova_bool _vt_nova_f64_lt(const void* self, const void* other) {
    return (*(const nova_f64*)self) < (*(const nova_f64*)other);
}
static inline nova_bool _vt_nova_f64_le(const void* self, const void* other) {
    return (*(const nova_f64*)self) <= (*(const nova_f64*)other);
}
static inline nova_bool _vt_nova_f64_gt(const void* self, const void* other) {
    return (*(const nova_f64*)self) > (*(const nova_f64*)other);
}
static inline nova_bool _vt_nova_f64_ge(const void* self, const void* other) {
    return (*(const nova_f64*)self) >= (*(const nova_f64*)other);
}
static inline nova_bool _vt_nova_f64_ne(const void* self, const void* other) {
    return !_vt_nova_f64_eq(self, other);
}
static const NovaVtable_Hashable _vt_Hashable_nova_f64 = {
    .hash = _vt_nova_f64_hash,
    .eq   = _vt_nova_f64_eq,
};
static const NovaVtable_Comparable _vt_Comparable_nova_f64 = {
    .lt = _vt_nova_f64_lt,
    .le = _vt_nova_f64_le,
    .gt = _vt_nova_f64_gt,
    .ge = _vt_nova_f64_ge,
    .eq = _vt_nova_f64_eq,
    .ne = _vt_nova_f64_ne,
};

/* ---- nova_str ----
 *
 * nova_str — struct { ptr, len }. eq / hash / lt / le / gt / ge defined
 * в nova_rt.h (static inline). vtables.h включается ПОСЛЕ их definitions
 * через nova_rt.h #include order — forward decl не нужен.
 */

static inline uint64_t _vt_nova_str_hash(const void* self) {
    return (uint64_t)nova_str_hash(*(const nova_str*)self);
}
static inline nova_bool _vt_nova_str_eq(const void* self, const void* other) {
    return nova_str_eq(*(const nova_str*)self, *(const nova_str*)other);
}
static inline nova_bool _vt_nova_str_lt(const void* self, const void* other) {
    return nova_str_lt(*(const nova_str*)self, *(const nova_str*)other);
}
static inline nova_bool _vt_nova_str_le(const void* self, const void* other) {
    return nova_str_le(*(const nova_str*)self, *(const nova_str*)other);
}
static inline nova_bool _vt_nova_str_gt(const void* self, const void* other) {
    return nova_str_gt(*(const nova_str*)self, *(const nova_str*)other);
}
static inline nova_bool _vt_nova_str_ge(const void* self, const void* other) {
    return nova_str_ge(*(const nova_str*)self, *(const nova_str*)other);
}
static inline nova_bool _vt_nova_str_ne(const void* self, const void* other) {
    return !_vt_nova_str_eq(self, other);
}

static const NovaVtable_Hashable _vt_Hashable_nova_str = {
    .hash = _vt_nova_str_hash,
    .eq   = _vt_nova_str_eq,
};
static const NovaVtable_Comparable _vt_Comparable_nova_str = {
    .lt = _vt_nova_str_lt,
    .le = _vt_nova_str_le,
    .gt = _vt_nova_str_gt,
    .ge = _vt_nova_str_ge,
    .eq = _vt_nova_str_eq,
    .ne = _vt_nova_str_ne,
};

/* ============================================================
 * Generic helpers — call через vtable safe casts.
 *
 * Эти macros использует codegen для call sites:
 *   NOVA_VT_HASH(vt, self)         → vt->hash(self)
 *   NOVA_VT_EQ(vt, self, other)    → vt->eq(self, other)
 *   NOVA_VT_LT(vt, self, other)    → vt->lt(self, other)
 *   ... etc.
 *
 * Compiler может devirtualize если vt — `static const` known
 * на call site (LICM hoists vtable lookup out of loops).
 * ============================================================ */
#define NOVA_VT_HASH(vt, self)         ((vt)->hash((const void*)(self)))
#define NOVA_VT_EQ(vt, self, other)    ((vt)->eq((const void*)(self), (const void*)(other)))
#define NOVA_VT_LT(vt, self, other)    ((vt)->lt((const void*)(self), (const void*)(other)))
#define NOVA_VT_LE(vt, self, other)    ((vt)->le((const void*)(self), (const void*)(other)))
#define NOVA_VT_GT(vt, self, other)    ((vt)->gt((const void*)(self), (const void*)(other)))
#define NOVA_VT_GE(vt, self, other)    ((vt)->ge((const void*)(self), (const void*)(other)))
#define NOVA_VT_NE(vt, self, other)    ((vt)->ne((const void*)(self), (const void*)(other)))

#endif /* NOVA_RT_VTABLES_H */
