/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * nova_msvc_compat.h — Plan 82 followup (2026-05-23):
 * GCC/Clang builtin compat-слой для MSVC cl.exe.
 *
 * Runtime (sync.h / sync_primitives.h / deque.h / runtime.c) использует
 * `__atomic_*` GCC/Clang builtins; sync.h §Tier-1 называет Windows clang
 * как поддерживаемый toolchain, но MSVC cl.exe этих builtin'ов не имеет
 * → C2065 «необъявленный идентификатор». effects.h использует
 * `__builtin_*_overflow` для checked-arith, fibers.h — `__builtin_expect`,
 * channels.h — `__builtin_readcyclecounter`.
 *
 * Этот header **force-инклюдится** test_runner'ом (`/FI`) в каждый TU
 * под MSVC → нулевые правки call-sites; clang-toolchain не затрагивается.
 *
 * Что предоставлено:
 *   - `__atomic_load_n / store_n / fetch_add / fetch_sub /
 *      compare_exchange_n / exchange_n / thread_fence`
 *   - `__ATOMIC_RELAXED / ACQUIRE / RELEASE / ACQ_REL / SEQ_CST`
 *   - `__builtin_expect / ctzll / readcyclecounter / *_overflow`
 *
 * Дизайн дисппатча: `sizeof(*(p))` (не C11 `_Generic`). _Generic требует
 * `/std:c11`, но та же опция отключает MS-extension struct-cast'ов,
 * которые эмитит codegen (`(nova_str)(t.f1)` → C2440 под /std:c11).
 * sizeof-диспатч работает в permissive MS-режиме без /std:; цена —
 * helper'ы возвращают канонический целочисленный тип, вызывающая сторона
 * получает implicit-conversion warnings (silenced под /W0). Это
 * стандартный паттерн compat-shim'ов (libuv, mimalloc, etc.).
 *
 * Целевая платформа: x86_64 Windows. x64 — TSO модель памяти:
 *   - aligned ≤8B load/store атомарны;
 *   - HW даёт acquire на loads / release на stores «бесплатно»;
 *   - единственный реордеринг HW — store→load (StoreLoad);
 *     блокируется `mfence`/`xchg`.
 * Отображение GCC `__atomic_*` → MSVC:
 *   - load/store любого порядка КРОМЕ seq_cst → volatile-доступ +
 *     `_ReadWriteBarrier()` (compiler barrier, HW даёт остальное);
 *   - seq_cst store → `_InterlockedExchange*` (xchg, full HW barrier);
 *   - все RMW → `_Interlocked*` (full HW barrier — корректно для любого
 *     запрошенного порядка, потенциально over-strong, но не слабее).
 *
 * Корректность: над-сильный порядок никогда не нарушает контракт
 * lock-free-структуры (Chase-Lev deque в deque.h); ту же математику
 * применяют портирования `__atomic_*` в libuv, libsodium, mimalloc.
 *
 *  Активен ТОЛЬКО под `_MSC_VER && !__clang__` (clang-cl определяет
 *  `__clang__` и берёт штатные builtin'ы).
 */
#ifndef NOVA_MSVC_COMPAT_H
#define NOVA_MSVC_COMPAT_H

#if defined(_MSC_VER) && !defined(__clang__)

#include <intrin.h>
#include <stdint.h>
#include <stdbool.h>

/* ── C11 keyword fallback (MS permissive mode) ──────────────────
 * _Alignas — C11 keyword; в MSVC доступен только под /std:c11+.
 * Runtime (channels.h §B7) использует `_Alignas(N) char _pad[1];`
 * для cache-line padding. В permissive MS-режиме `_Alignas` —
 * undefined identifier → C2061. Maps to MSVC __declspec(align(N))
 * (тот же эффект — alignment-атрибут на следующем объявлении). */
#ifndef _Alignas
#define _Alignas(n) __declspec(align(n))
#endif

/* ── Memory-ordering константы (имена GCC builtin'ов) ───────────── */
#ifndef __ATOMIC_RELAXED
#define __ATOMIC_RELAXED  0
#define __ATOMIC_CONSUME  1   /* alias на ACQUIRE — runtime не использует */
#define __ATOMIC_ACQUIRE  2
#define __ATOMIC_RELEASE  3
#define __ATOMIC_ACQ_REL  4
#define __ATOMIC_SEQ_CST  5
#endif

/* ── Барьеры ────────────────────────────────────────────────────── */

static __forceinline void _nova_compat_compiler_barrier(void) {
    _ReadWriteBarrier();
}
static __forceinline void _nova_compat_hw_mfence(void) {
    _mm_mfence();
}

static __forceinline void __atomic_thread_fence(int order) {
    if (order == __ATOMIC_SEQ_CST) _nova_compat_hw_mfence();
    else                            _nova_compat_compiler_barrier();
}

/* ── Per-width load helpers (returning canonical __int64) ────────
 * Aligned ≤8B read атомарен на x64; HW даёт acquire-load.
 * Возвращаем __int64 единообразно — callsite получает implicit
 * conversion (C4244/C4312/C4047 silenced под /W0). */

static __forceinline __int64 _nova_compat_load_sz(const volatile void* p, size_t sz) {
    __int64 v;
    if (sz == 1)      v = (__int64)*(const volatile char*)p;
    else if (sz == 4) v = (__int64)*(const volatile long*)p;
    else /* 8 */      v =          *(const volatile __int64*)p;
    _ReadWriteBarrier();
    return v;
}

/* Store. SEQ_CST → xchg (full HW barrier, обеспечивает StoreLoad).
 * Иначе — volatile-write + compiler barrier (HW release на x64). */
static __forceinline void _nova_compat_store_sz(volatile void* p, __int64 v, size_t sz, int order) {
    if (order == __ATOMIC_SEQ_CST) {
        if (sz == 1)      (void)_InterlockedExchange8 ((volatile char*)p,    (char)v);
        else if (sz == 4) (void)_InterlockedExchange  ((volatile long*)p,    (long)v);
        else              (void)_InterlockedExchange64((volatile __int64*)p, v);
    } else {
        _ReadWriteBarrier();
        if (sz == 1)      *(volatile char*)p    = (char)v;
        else if (sz == 4) *(volatile long*)p    = (long)v;
        else              *(volatile __int64*)p = v;
    }
}

/* RMW — _Interlocked*: lock-prefix, full HW barrier. Корректно для
 * любого порядка (over-strong, но не слабее). */

static __forceinline __int64 _nova_compat_fadd_sz(volatile void* p, __int64 v, size_t sz) {
    if (sz == 4) return (__int64)_InterlockedExchangeAdd((volatile long*)p, (long)v);
    else         return          _InterlockedExchangeAdd64((volatile __int64*)p, v);
}

static __forceinline __int64 _nova_compat_xchg_sz(volatile void* p, __int64 v, size_t sz) {
    if (sz == 1)      return (__int64)_InterlockedExchange8 ((volatile char*)p,    (char)v);
    else if (sz == 4) return (__int64)_InterlockedExchange  ((volatile long*)p,    (long)v);
    else              return          _InterlockedExchange64((volatile __int64*)p, v);
}

/* CAS — GCC семантика: возвращает bool; на неуспехе *expected ← текущее.
 * Все sizeof-ветки делают одну и ту же логику на своей ширине. */
static __forceinline bool _nova_compat_cas_sz(volatile void* p, void* expected,
                                              __int64 desired, size_t sz) {
    if (sz == 1) {
        char e = *(char*)expected;
        char prev = _InterlockedCompareExchange8((volatile char*)p, (char)desired, e);
        if (prev == e) return true;
        *(char*)expected = prev; return false;
    } else if (sz == 4) {
        long e = *(long*)expected;
        long prev = _InterlockedCompareExchange((volatile long*)p, (long)desired, e);
        if (prev == e) return true;
        *(long*)expected = prev; return false;
    } else {
        __int64 e = *(__int64*)expected;
        __int64 prev = _InterlockedCompareExchange64((volatile __int64*)p, desired, e);
        if (prev == e) return true;
        *(__int64*)expected = prev; return false;
    }
}

/* ── Builtin-имена через макросы. side-effects в (p) НЕ допускаются
 * (макрос раскрывает p многократно — в sizeof и в helper-вызове).
 * В runtime все call-site'ы используют &var. ─────────────────────── */

#define __atomic_load_n(p, order) \
    _nova_compat_load_sz((const volatile void*)(p), sizeof(*(p)))

#define __atomic_store_n(p, val, order) \
    _nova_compat_store_sz((volatile void*)(p), (__int64)(val), sizeof(*(p)), (order))

#define __atomic_fetch_add(p, val, order) \
    _nova_compat_fadd_sz((volatile void*)(p), (__int64)(val), sizeof(*(p)))

#define __atomic_fetch_sub(p, val, order) \
    _nova_compat_fadd_sz((volatile void*)(p), -(__int64)(val), sizeof(*(p)))

#define __atomic_exchange_n(p, val, order) \
    _nova_compat_xchg_sz((volatile void*)(p), (__int64)(val), sizeof(*(p)))

/* GCC: __atomic_compare_exchange_n(p, expected_ptr, desired, weak, succ_o, fail_o).
 * `weak` на x64 не имеет значения (CAS всегда strong). */
#define __atomic_compare_exchange_n(p, expected, desired, weak, succ, fail) \
    _nova_compat_cas_sz((volatile void*)(p), (void*)(expected), \
                        (__int64)(desired), sizeof(*(p)))

/* ── __builtin_* shim ───────────────────────────────────────────── */

/* Branch hint — MSVC C: no portable analog; identity. */
#define __builtin_expect(x, c) (x)

/* Count trailing zeros (64-bit). _BitScanForward64 → index в [0..63].
 * UB при x == 0 (как и GCC __builtin_ctzll). */
static __forceinline int __builtin_ctzll(unsigned long long x) {
    unsigned long idx;
    _BitScanForward64(&idx, x);
    return (int)idx;
}

/* TSC — паритет с GCC __builtin_readcyclecounter. */
static __forceinline unsigned long long __builtin_readcyclecounter(void) {
    return __rdtsc();
}

/* Signed overflow checks. GCC семантика: *r пишется всегда (low-trunc
 * при overflow), функция возвращает bool overflow-flag.
 * Runtime (effects.h) использует на int64_t — NOVA_INT_OVF_PANIC. */
static __forceinline bool _nova_compat_add_ov_ll(long long a, long long b, long long* r) {
    *r = (long long)((unsigned long long)a + (unsigned long long)b);
    return ((a ^ *r) & (b ^ *r)) < 0;
}
static __forceinline bool _nova_compat_sub_ov_ll(long long a, long long b, long long* r) {
    *r = (long long)((unsigned long long)a - (unsigned long long)b);
    return ((a ^ b) & (a ^ *r)) < 0;
}
static __forceinline bool _nova_compat_mul_ov_ll(long long a, long long b, long long* r) {
    /* 128-битное умножение → overflow ↔ hi != sign-extension(lo). */
    long long hi;
    long long lo = _mul128(a, b, &hi);
    *r = lo;
    return hi != (lo >> 63);
}

/* Type-generic overflow macros. В runtime аргументы — int64_t (signed
 * long long на Win64); только 8-байтную ветку и обслуживаем. На прочей
 * ширине (теоретическая защита) возвращаем overflow=1 — безопасно. */
#define __builtin_add_overflow(a, b, r) \
    (sizeof(*(r)) == 8 \
        ? _nova_compat_add_ov_ll((long long)(a), (long long)(b), (long long*)(r)) \
        : (*(r) = 0, true))
#define __builtin_sub_overflow(a, b, r) \
    (sizeof(*(r)) == 8 \
        ? _nova_compat_sub_ov_ll((long long)(a), (long long)(b), (long long*)(r)) \
        : (*(r) = 0, true))
#define __builtin_mul_overflow(a, b, r) \
    (sizeof(*(r)) == 8 \
        ? _nova_compat_mul_ov_ll((long long)(a), (long long)(b), (long long*)(r)) \
        : (*(r) = 0, true))

#endif /* _MSC_VER && !__clang__ */
#endif /* NOVA_MSVC_COMPAT_H */
