// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_SYNC_PRIMITIVES_H
#define NOVA_RT_SYNC_PRIMITIVES_H

#include <stdio.h>
#include <stdlib.h>

/* Plan 18 std.sync: fiber-aware AtomicInt / AtomicBool / Mutex / WaitGroup / Once.
 *
 * Included from nova_rt.h AFTER nova_sched.h (needs nova_sched_park_with_unlock,
 * nova_sched_wake) and fibers.h (_nova_active_scope, _nova_active_slot TLS).
 *
 * Design:
 *  - AtomicInt / AtomicBool: thin wrappers around __atomic_* builtins. No park/wake.
 *  - Mutex: nova_mutex_t guards `locked` + waiter list. Fiber waiters park;
 *    non-fiber callers spin with CPU yield hint.
 *  - WaitGroup: nova_mutex_t guards `count` + waiter list. wait() parks when
 *    count > 0; done() wakes all waiters when count reaches 0.
 *  - Once: state machine NEW→RUNNING→DONE. First caller transitions NEW→RUNNING
 *    and returns true (should do the work). Concurrent callers park until DONE.
 *    All callers other than the first return false only after DONE is set.
 *
 * Waiter structs are stack-allocated in the parking fiber's call frame —
 * identical to ChannelWaiter pattern in channels.h. Safe because the fiber
 * stack is fixed (8 MB) and persists until the fiber resumes.
 *
 * Non-fiber path (_nova_active_slot < 0): spin with _nova_cpu_yield() hint.
 * This covers init/teardown and test scenarios that call sync primitives
 * outside a supervised scope.
 *
 * INVARIANTS (checked via NOVA_SYNC_ASSERT in debug builds):
 *  - Mutex.unlock() must be called only when the mutex is locked.
 *  - WaitGroup.done() must not decrement below zero.
 *  - Once.done() must be called exactly once, by the fiber whose run() returned true.
 *
 * NOT SUPPORTED (by design, same as Go/parking_lot):
 *  - Mutex is NOT reentrant. Calling lock() twice from the same fiber deadlocks.
 *  - WaitGroup.add() after wait() has started is undefined (same as Go).
 */

/* ── Debug assertions ──────────────────────────────────────────── */

#ifdef NOVA_DEBUG
#  define NOVA_SYNC_ASSERT(cond, msg)                                   \
     do {                                                                \
         if (!(cond)) {                                                  \
             fprintf(stderr, "[nova sync] FATAL: " msg "\n");           \
             abort();                                                    \
         }                                                               \
     } while (0)
#else
#  define NOVA_SYNC_ASSERT(cond, msg) ((void)0)
#endif

/* ── CPU yield hint ────────────────────────────────────────────── */

/* Used in OS-thread spin loops. Reduces bus traffic and gives the OS
 * scheduler a hint that this thread is busy-waiting.
 * x86: PAUSE reduces pipeline pressure (1 instruction vs tight CAS loop).
 * ARM: YIELD is the equivalent hint.
 * Windows: YieldProcessor() wraps the PAUSE/YIELD intrinsic. */
static inline void _nova_cpu_yield(void) {
#if defined(_WIN32)
    YieldProcessor();
#elif defined(__aarch64__) || defined(__arm64__)
    __asm__ volatile("yield" ::: "memory");
#elif defined(__x86_64__) || defined(__i386__)
    __asm__ volatile("pause" ::: "memory");
#endif
    /* On other POSIX platforms: fall through — the nova_mutex_lock/unlock
     * pair in the spin loop already implies OS scheduler interaction. */
}

/* ── AtomicInt ─────────────────────────────────────────────────── */

/* AtomicInt wraps nova_atomic_int (int32_t). All accesses go through
 * __atomic_* builtins in sync.h — the 'volatile' qualifier is NOT needed
 * (and would be misleading: volatile ≠ atomic in C11). */
typedef struct {
    nova_atomic_int value;
} Nova_AtomicInt;

static inline Nova_AtomicInt* Nova_AtomicInt_static_new(nova_int v) {
    Nova_AtomicInt* a = (Nova_AtomicInt*)nova_alloc(sizeof(Nova_AtomicInt));
    nova_aint_init(&a->value, (int32_t)v);
    return a;
}

static inline nova_int Nova_AtomicInt_method_load(const Nova_AtomicInt* a) {
    return (nova_int)nova_aint_load(&a->value);
}

static inline nova_unit Nova_AtomicInt_method_store(Nova_AtomicInt* a, nova_int v) {
    nova_aint_store(&a->value, (int32_t)v);
    return NOVA_UNIT;
}

static inline nova_int Nova_AtomicInt_method_fetch_add(Nova_AtomicInt* a, nova_int delta) {
    return (nova_int)__atomic_fetch_add(&a->value, (int32_t)delta, __ATOMIC_ACQ_REL);
}

static inline nova_int Nova_AtomicInt_method_fetch_sub(Nova_AtomicInt* a, nova_int delta) {
    return (nova_int)__atomic_fetch_sub(&a->value, (int32_t)delta, __ATOMIC_ACQ_REL);
}

static inline nova_bool Nova_AtomicInt_method_compare_exchange(
        Nova_AtomicInt* a, nova_int expected_val, nova_int desired) {
    int32_t exp = (int32_t)expected_val;
    return nova_aint_cas(&a->value, &exp, (int32_t)desired);
}

/* ── MemOrdering (Plan 103.1, relocated forward for Plan 103.2) ───────
 *
 * Pre-declared here so nova_mo_c() and all sized-atomic ordering-aware
 * methods can reference Nova_MemOrdering* — they appear in the file
 * before the Once/fence section where this was originally defined.
 * Codegen skips re-emitting MemOrdering (RUNTIME_DEFINED_TYPES in emit_c.rs).
 * Tag values = D167: Relaxed=0 Acquire=1 Release=2 AcqRel=3 SeqCst=4.
 */
typedef enum {
    NOVA_TAG_MemOrdering_Relaxed = 0,
    NOVA_TAG_MemOrdering_Acquire = 1,
    NOVA_TAG_MemOrdering_Release = 2,
    NOVA_TAG_MemOrdering_AcqRel  = 3,
    NOVA_TAG_MemOrdering_SeqCst  = 4,
} Nova_MemOrdering_Tag;

typedef struct Nova_MemOrdering Nova_MemOrdering;
struct Nova_MemOrdering {
    Nova_MemOrdering_Tag tag;
    union { char _dummy; } payload;   /* unit-only variants — MSVC requires >=1 member */
};

/* Constructors — normally emitted by emit_sum_type; here because MemOrdering
 * is in RUNTIME_DEFINED_TYPES (emit_sum_type is skipped). */
static inline Nova_MemOrdering* nova_make_MemOrdering_Relaxed(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_Relaxed;
    return _r;
}
static inline Nova_MemOrdering* nova_make_MemOrdering_Acquire(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_Acquire;
    return _r;
}
static inline Nova_MemOrdering* nova_make_MemOrdering_Release(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_Release;
    return _r;
}
static inline Nova_MemOrdering* nova_make_MemOrdering_AcqRel(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_AcqRel;
    return _r;
}
static inline Nova_MemOrdering* nova_make_MemOrdering_SeqCst(void) {
    Nova_MemOrdering* _r = (Nova_MemOrdering*)nova_alloc(sizeof(Nova_MemOrdering));
    _r->tag = NOVA_TAG_MemOrdering_SeqCst;
    return _r;
}
/* ── Plan 103.2: MemOrdering → __ATOMIC_* helper ───────────────── */

/* Convert Nova_MemOrdering* tag to the corresponding __ATOMIC_* constant.
 * Used by all ordering-aware overloads below. SeqCst is the default.
 * Tag values coordinated with NOVA_TAG_MemOrdering_* above. */
static inline int nova_mo_c(const Nova_MemOrdering* ord) {
    switch (ord->tag) {
        case NOVA_TAG_MemOrdering_Relaxed: return __ATOMIC_RELAXED;
        case NOVA_TAG_MemOrdering_Acquire: return __ATOMIC_ACQUIRE;
        case NOVA_TAG_MemOrdering_Release: return __ATOMIC_RELEASE;
        case NOVA_TAG_MemOrdering_AcqRel:  return __ATOMIC_ACQ_REL;
        case NOVA_TAG_MemOrdering_SeqCst:
        default:                           return __ATOMIC_SEQ_CST;
    }
}

/* ── Plan 103.2: AtomicI64 ─────────────────────────────────────── */

typedef struct { int64_t value; } Nova_AtomicI64;

static inline Nova_AtomicI64* Nova_AtomicI64_static_new(nova_int v) {
    Nova_AtomicI64* a = (Nova_AtomicI64*)nova_alloc(sizeof(Nova_AtomicI64));
    __atomic_store_n(&a->value, (int64_t)v, __ATOMIC_SEQ_CST);
    return a;
}
/* load */
static inline nova_int Nova_AtomicI64_method_load_MemOrdering(const Nova_AtomicI64* a, const Nova_MemOrdering* ord) {
    return (nova_int)__atomic_load_n(&a->value, nova_mo_c(ord));
}
static inline nova_int Nova_AtomicI64_method_load(const Nova_AtomicI64* a) {
    return (nova_int)__atomic_load_n(&a->value, __ATOMIC_SEQ_CST);
}
/* store */
static inline nova_unit Nova_AtomicI64_method_store_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) {
    __atomic_store_n(&a->value, (int64_t)v, nova_mo_c(ord)); return NOVA_UNIT;
}
static inline nova_unit Nova_AtomicI64_method_store_i64(Nova_AtomicI64* a, nova_int v) {
    __atomic_store_n(&a->value, (int64_t)v, __ATOMIC_SEQ_CST); return NOVA_UNIT;
}
/* swap */
static inline nova_int Nova_AtomicI64_method_swap_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) {
    return (nova_int)__atomic_exchange_n(&a->value, (int64_t)v, nova_mo_c(ord));
}
static inline nova_int Nova_AtomicI64_method_swap_i64(Nova_AtomicI64* a, nova_int v) {
    return (nova_int)__atomic_exchange_n(&a->value, (int64_t)v, __ATOMIC_SEQ_CST);
}
/* compare_exchange strong */
static inline nova_bool Nova_AtomicI64_method_compare_exchange_MemOrdering(
        Nova_AtomicI64* a, nova_int expected_val, nova_int desired,
        const Nova_MemOrdering* success_ord, const Nova_MemOrdering* failure_ord) {
    int64_t exp = (int64_t)expected_val;
    return (nova_bool)__atomic_compare_exchange_n(&a->value, &exp, (int64_t)desired,
        false, nova_mo_c(success_ord), nova_mo_c(failure_ord));
}
static inline nova_bool Nova_AtomicI64_method_compare_exchange_i64(
        Nova_AtomicI64* a, nova_int expected_val, nova_int desired) {
    int64_t exp = (int64_t)expected_val;
    return (nova_bool)__atomic_compare_exchange_n(&a->value, &exp, (int64_t)desired,
        false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
/* compare_exchange weak */
static inline nova_bool Nova_AtomicI64_method_compare_exchange_weak_MemOrdering(
        Nova_AtomicI64* a, nova_int expected_val, nova_int desired,
        const Nova_MemOrdering* success_ord, const Nova_MemOrdering* failure_ord) {
    int64_t exp = (int64_t)expected_val;
    return (nova_bool)__atomic_compare_exchange_n(&a->value, &exp, (int64_t)desired,
        true, nova_mo_c(success_ord), nova_mo_c(failure_ord));
}
static inline nova_bool Nova_AtomicI64_method_compare_exchange_weak_i64(
        Nova_AtomicI64* a, nova_int expected_val, nova_int desired) {
    int64_t exp = (int64_t)expected_val;
    return (nova_bool)__atomic_compare_exchange_n(&a->value, &exp, (int64_t)desired,
        true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
/* fetch_add */
static inline nova_int Nova_AtomicI64_method_fetch_add_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) { return (nova_int)__atomic_fetch_add(&a->value, (int64_t)v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicI64_method_fetch_add_i64(Nova_AtomicI64* a, nova_int v) { return (nova_int)__atomic_fetch_add(&a->value, (int64_t)v, __ATOMIC_SEQ_CST); }
/* fetch_sub */
static inline nova_int Nova_AtomicI64_method_fetch_sub_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) { return (nova_int)__atomic_fetch_sub(&a->value, (int64_t)v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicI64_method_fetch_sub_i64(Nova_AtomicI64* a, nova_int v) { return (nova_int)__atomic_fetch_sub(&a->value, (int64_t)v, __ATOMIC_SEQ_CST); }
/* fetch_or */
static inline nova_int Nova_AtomicI64_method_fetch_or_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) { return (nova_int)__atomic_fetch_or(&a->value, (int64_t)v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicI64_method_fetch_or_i64(Nova_AtomicI64* a, nova_int v) { return (nova_int)__atomic_fetch_or(&a->value, (int64_t)v, __ATOMIC_SEQ_CST); }
/* fetch_and */
static inline nova_int Nova_AtomicI64_method_fetch_and_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) { return (nova_int)__atomic_fetch_and(&a->value, (int64_t)v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicI64_method_fetch_and_i64(Nova_AtomicI64* a, nova_int v) { return (nova_int)__atomic_fetch_and(&a->value, (int64_t)v, __ATOMIC_SEQ_CST); }
/* fetch_xor */
static inline nova_int Nova_AtomicI64_method_fetch_xor_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) { return (nova_int)__atomic_fetch_xor(&a->value, (int64_t)v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicI64_method_fetch_xor_i64(Nova_AtomicI64* a, nova_int v) { return (nova_int)__atomic_fetch_xor(&a->value, (int64_t)v, __ATOMIC_SEQ_CST); }
/* fetch_max (CAS loop — no __atomic_fetch_max builtin) */
static inline nova_int Nova_AtomicI64_method_fetch_max_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); int64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < (int64_t)v) { if (__atomic_compare_exchange_n(&a->value, &cur, (int64_t)v, true, mo, __ATOMIC_RELAXED)) break; }
    return (nova_int)cur;
}
static inline nova_int Nova_AtomicI64_method_fetch_max_i64(Nova_AtomicI64* a, nova_int v) {
    int64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < (int64_t)v) { if (__atomic_compare_exchange_n(&a->value, &cur, (int64_t)v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; }
    return (nova_int)cur;
}
/* fetch_min */
static inline nova_int Nova_AtomicI64_method_fetch_min_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); int64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > (int64_t)v) { if (__atomic_compare_exchange_n(&a->value, &cur, (int64_t)v, true, mo, __ATOMIC_RELAXED)) break; }
    return (nova_int)cur;
}
static inline nova_int Nova_AtomicI64_method_fetch_min_i64(Nova_AtomicI64* a, nova_int v) {
    int64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > (int64_t)v) { if (__atomic_compare_exchange_n(&a->value, &cur, (int64_t)v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; }
    return (nova_int)cur;
}
/* fetch_nand */
static inline nova_int Nova_AtomicI64_method_fetch_nand_MemOrdering(Nova_AtomicI64* a, nova_int v, const Nova_MemOrdering* ord) { return (nova_int)__atomic_fetch_nand(&a->value, (int64_t)v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicI64_method_fetch_nand_i64(Nova_AtomicI64* a, nova_int v) { return (nova_int)__atomic_fetch_nand(&a->value, (int64_t)v, __ATOMIC_SEQ_CST); }

/* ── Plan 103.2: AtomicI32 ─────────────────────────────────────── */

typedef struct { int32_t value; } Nova_AtomicI32;

static inline Nova_AtomicI32* Nova_AtomicI32_static_new(int32_t v) {
    Nova_AtomicI32* a = (Nova_AtomicI32*)nova_alloc(sizeof(Nova_AtomicI32));
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return a;
}
static inline int32_t Nova_AtomicI32_method_load_MemOrdering(const Nova_AtomicI32* a, const Nova_MemOrdering* ord) { return __atomic_load_n(&a->value, nova_mo_c(ord)); }
static inline int32_t Nova_AtomicI32_method_load(const Nova_AtomicI32* a) { return __atomic_load_n(&a->value, __ATOMIC_SEQ_CST); }
static inline nova_unit Nova_AtomicI32_method_store_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) { __atomic_store_n(&a->value, v, nova_mo_c(ord)); return NOVA_UNIT; }
static inline nova_unit Nova_AtomicI32_method_store_i32(Nova_AtomicI32* a, int32_t v) { __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return NOVA_UNIT; }
static inline int32_t Nova_AtomicI32_method_swap_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) { return __atomic_exchange_n(&a->value, v, nova_mo_c(ord)); }
static inline int32_t Nova_AtomicI32_method_swap_i32(Nova_AtomicI32* a, int32_t v) { return __atomic_exchange_n(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicI32_method_compare_exchange_MemOrdering(Nova_AtomicI32* a, int32_t e, int32_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicI32_method_compare_exchange_i32(Nova_AtomicI32* a, int32_t e, int32_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicI32_method_compare_exchange_weak_MemOrdering(Nova_AtomicI32* a, int32_t e, int32_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicI32_method_compare_exchange_weak_i32(Nova_AtomicI32* a, int32_t e, int32_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline int32_t Nova_AtomicI32_method_fetch_add_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_add(&a->value, v, nova_mo_c(ord)); }
static inline int32_t Nova_AtomicI32_method_fetch_add_i32(Nova_AtomicI32* a, int32_t v) { return __atomic_fetch_add(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int32_t Nova_AtomicI32_method_fetch_sub_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_sub(&a->value, v, nova_mo_c(ord)); }
static inline int32_t Nova_AtomicI32_method_fetch_sub_i32(Nova_AtomicI32* a, int32_t v) { return __atomic_fetch_sub(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int32_t Nova_AtomicI32_method_fetch_or_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_or(&a->value, v, nova_mo_c(ord)); }
static inline int32_t Nova_AtomicI32_method_fetch_or_i32(Nova_AtomicI32* a, int32_t v) { return __atomic_fetch_or(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int32_t Nova_AtomicI32_method_fetch_and_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_and(&a->value, v, nova_mo_c(ord)); }
static inline int32_t Nova_AtomicI32_method_fetch_and_i32(Nova_AtomicI32* a, int32_t v) { return __atomic_fetch_and(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int32_t Nova_AtomicI32_method_fetch_xor_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_xor(&a->value, v, nova_mo_c(ord)); }
static inline int32_t Nova_AtomicI32_method_fetch_xor_i32(Nova_AtomicI32* a, int32_t v) { return __atomic_fetch_xor(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int32_t Nova_AtomicI32_method_fetch_max_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); int32_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int32_t Nova_AtomicI32_method_fetch_max_i32(Nova_AtomicI32* a, int32_t v) {
    int32_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int32_t Nova_AtomicI32_method_fetch_min_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); int32_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int32_t Nova_AtomicI32_method_fetch_min_i32(Nova_AtomicI32* a, int32_t v) {
    int32_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int32_t Nova_AtomicI32_method_fetch_nand_MemOrdering(Nova_AtomicI32* a, int32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_nand(&a->value, v, nova_mo_c(ord)); }
static inline int32_t Nova_AtomicI32_method_fetch_nand_i32(Nova_AtomicI32* a, int32_t v) { return __atomic_fetch_nand(&a->value, v, __ATOMIC_SEQ_CST); }

/* ── Plan 103.2: AtomicI16 ─────────────────────────────────────── */

typedef struct { int16_t value; } Nova_AtomicI16;

static inline Nova_AtomicI16* Nova_AtomicI16_static_new(int16_t v) {
    Nova_AtomicI16* a = (Nova_AtomicI16*)nova_alloc(sizeof(Nova_AtomicI16));
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return a;
}
static inline int16_t Nova_AtomicI16_method_load_MemOrdering(const Nova_AtomicI16* a, const Nova_MemOrdering* ord) { return __atomic_load_n(&a->value, nova_mo_c(ord)); }
static inline int16_t Nova_AtomicI16_method_load(const Nova_AtomicI16* a) { return __atomic_load_n(&a->value, __ATOMIC_SEQ_CST); }
static inline nova_unit Nova_AtomicI16_method_store_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) { __atomic_store_n(&a->value, v, nova_mo_c(ord)); return NOVA_UNIT; }
static inline nova_unit Nova_AtomicI16_method_store_i16(Nova_AtomicI16* a, int16_t v) { __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return NOVA_UNIT; }
static inline int16_t Nova_AtomicI16_method_swap_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) { return __atomic_exchange_n(&a->value, v, nova_mo_c(ord)); }
static inline int16_t Nova_AtomicI16_method_swap_i16(Nova_AtomicI16* a, int16_t v) { return __atomic_exchange_n(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicI16_method_compare_exchange_MemOrdering(Nova_AtomicI16* a, int16_t e, int16_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicI16_method_compare_exchange_i16(Nova_AtomicI16* a, int16_t e, int16_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicI16_method_compare_exchange_weak_MemOrdering(Nova_AtomicI16* a, int16_t e, int16_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicI16_method_compare_exchange_weak_i16(Nova_AtomicI16* a, int16_t e, int16_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline int16_t Nova_AtomicI16_method_fetch_add_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_add(&a->value, v, nova_mo_c(ord)); }
static inline int16_t Nova_AtomicI16_method_fetch_add_i16(Nova_AtomicI16* a, int16_t v) { return __atomic_fetch_add(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int16_t Nova_AtomicI16_method_fetch_sub_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_sub(&a->value, v, nova_mo_c(ord)); }
static inline int16_t Nova_AtomicI16_method_fetch_sub_i16(Nova_AtomicI16* a, int16_t v) { return __atomic_fetch_sub(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int16_t Nova_AtomicI16_method_fetch_or_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_or(&a->value, v, nova_mo_c(ord)); }
static inline int16_t Nova_AtomicI16_method_fetch_or_i16(Nova_AtomicI16* a, int16_t v) { return __atomic_fetch_or(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int16_t Nova_AtomicI16_method_fetch_and_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_and(&a->value, v, nova_mo_c(ord)); }
static inline int16_t Nova_AtomicI16_method_fetch_and_i16(Nova_AtomicI16* a, int16_t v) { return __atomic_fetch_and(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int16_t Nova_AtomicI16_method_fetch_xor_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_xor(&a->value, v, nova_mo_c(ord)); }
static inline int16_t Nova_AtomicI16_method_fetch_xor_i16(Nova_AtomicI16* a, int16_t v) { return __atomic_fetch_xor(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int16_t Nova_AtomicI16_method_fetch_max_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); int16_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int16_t Nova_AtomicI16_method_fetch_max_i16(Nova_AtomicI16* a, int16_t v) {
    int16_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int16_t Nova_AtomicI16_method_fetch_min_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); int16_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int16_t Nova_AtomicI16_method_fetch_min_i16(Nova_AtomicI16* a, int16_t v) {
    int16_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int16_t Nova_AtomicI16_method_fetch_nand_MemOrdering(Nova_AtomicI16* a, int16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_nand(&a->value, v, nova_mo_c(ord)); }
static inline int16_t Nova_AtomicI16_method_fetch_nand_i16(Nova_AtomicI16* a, int16_t v) { return __atomic_fetch_nand(&a->value, v, __ATOMIC_SEQ_CST); }

/* ── Plan 103.2: AtomicI8 ──────────────────────────────────────── */

typedef struct { int8_t value; } Nova_AtomicI8;

static inline Nova_AtomicI8* Nova_AtomicI8_static_new(int8_t v) {
    Nova_AtomicI8* a = (Nova_AtomicI8*)nova_alloc(sizeof(Nova_AtomicI8));
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return a;
}
static inline int8_t Nova_AtomicI8_method_load_MemOrdering(const Nova_AtomicI8* a, const Nova_MemOrdering* ord) { return __atomic_load_n(&a->value, nova_mo_c(ord)); }
static inline int8_t Nova_AtomicI8_method_load(const Nova_AtomicI8* a) { return __atomic_load_n(&a->value, __ATOMIC_SEQ_CST); }
static inline nova_unit Nova_AtomicI8_method_store_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) { __atomic_store_n(&a->value, v, nova_mo_c(ord)); return NOVA_UNIT; }
static inline nova_unit Nova_AtomicI8_method_store_i8(Nova_AtomicI8* a, int8_t v) { __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return NOVA_UNIT; }
static inline int8_t Nova_AtomicI8_method_swap_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) { return __atomic_exchange_n(&a->value, v, nova_mo_c(ord)); }
static inline int8_t Nova_AtomicI8_method_swap_i8(Nova_AtomicI8* a, int8_t v) { return __atomic_exchange_n(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicI8_method_compare_exchange_MemOrdering(Nova_AtomicI8* a, int8_t e, int8_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicI8_method_compare_exchange_i8(Nova_AtomicI8* a, int8_t e, int8_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicI8_method_compare_exchange_weak_MemOrdering(Nova_AtomicI8* a, int8_t e, int8_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicI8_method_compare_exchange_weak_i8(Nova_AtomicI8* a, int8_t e, int8_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline int8_t Nova_AtomicI8_method_fetch_add_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_add(&a->value, v, nova_mo_c(ord)); }
static inline int8_t Nova_AtomicI8_method_fetch_add_i8(Nova_AtomicI8* a, int8_t v) { return __atomic_fetch_add(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int8_t Nova_AtomicI8_method_fetch_sub_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_sub(&a->value, v, nova_mo_c(ord)); }
static inline int8_t Nova_AtomicI8_method_fetch_sub_i8(Nova_AtomicI8* a, int8_t v) { return __atomic_fetch_sub(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int8_t Nova_AtomicI8_method_fetch_or_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_or(&a->value, v, nova_mo_c(ord)); }
static inline int8_t Nova_AtomicI8_method_fetch_or_i8(Nova_AtomicI8* a, int8_t v) { return __atomic_fetch_or(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int8_t Nova_AtomicI8_method_fetch_and_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_and(&a->value, v, nova_mo_c(ord)); }
static inline int8_t Nova_AtomicI8_method_fetch_and_i8(Nova_AtomicI8* a, int8_t v) { return __atomic_fetch_and(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int8_t Nova_AtomicI8_method_fetch_xor_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_xor(&a->value, v, nova_mo_c(ord)); }
static inline int8_t Nova_AtomicI8_method_fetch_xor_i8(Nova_AtomicI8* a, int8_t v) { return __atomic_fetch_xor(&a->value, v, __ATOMIC_SEQ_CST); }
static inline int8_t Nova_AtomicI8_method_fetch_max_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); int8_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int8_t Nova_AtomicI8_method_fetch_max_i8(Nova_AtomicI8* a, int8_t v) {
    int8_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int8_t Nova_AtomicI8_method_fetch_min_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); int8_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int8_t Nova_AtomicI8_method_fetch_min_i8(Nova_AtomicI8* a, int8_t v) {
    int8_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline int8_t Nova_AtomicI8_method_fetch_nand_MemOrdering(Nova_AtomicI8* a, int8_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_nand(&a->value, v, nova_mo_c(ord)); }
static inline int8_t Nova_AtomicI8_method_fetch_nand_i8(Nova_AtomicI8* a, int8_t v) { return __atomic_fetch_nand(&a->value, v, __ATOMIC_SEQ_CST); }

/* ── Plan 103.2: AtomicU64 ─────────────────────────────────────── */

typedef struct { uint64_t value; } Nova_AtomicU64;

static inline Nova_AtomicU64* Nova_AtomicU64_static_new(uint64_t v) {
    Nova_AtomicU64* a = (Nova_AtomicU64*)nova_alloc(sizeof(Nova_AtomicU64));
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return a;
}
static inline uint64_t Nova_AtomicU64_method_load_MemOrdering(const Nova_AtomicU64* a, const Nova_MemOrdering* ord) { return __atomic_load_n(&a->value, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicU64_method_load(const Nova_AtomicU64* a) { return __atomic_load_n(&a->value, __ATOMIC_SEQ_CST); }
static inline nova_unit Nova_AtomicU64_method_store_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) { __atomic_store_n(&a->value, v, nova_mo_c(ord)); return NOVA_UNIT; }
static inline nova_unit Nova_AtomicU64_method_store_u64(Nova_AtomicU64* a, uint64_t v) { __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return NOVA_UNIT; }
static inline uint64_t Nova_AtomicU64_method_swap_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_exchange_n(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicU64_method_swap_u64(Nova_AtomicU64* a, uint64_t v) { return __atomic_exchange_n(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicU64_method_compare_exchange_MemOrdering(Nova_AtomicU64* a, uint64_t e, uint64_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicU64_method_compare_exchange_u64(Nova_AtomicU64* a, uint64_t e, uint64_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicU64_method_compare_exchange_weak_MemOrdering(Nova_AtomicU64* a, uint64_t e, uint64_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicU64_method_compare_exchange_weak_u64(Nova_AtomicU64* a, uint64_t e, uint64_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicU64_method_fetch_add_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_add(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicU64_method_fetch_add_u64(Nova_AtomicU64* a, uint64_t v) { return __atomic_fetch_add(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicU64_method_fetch_sub_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_sub(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicU64_method_fetch_sub_u64(Nova_AtomicU64* a, uint64_t v) { return __atomic_fetch_sub(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicU64_method_fetch_or_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_or(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicU64_method_fetch_or_u64(Nova_AtomicU64* a, uint64_t v) { return __atomic_fetch_or(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicU64_method_fetch_and_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_and(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicU64_method_fetch_and_u64(Nova_AtomicU64* a, uint64_t v) { return __atomic_fetch_and(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicU64_method_fetch_xor_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_xor(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicU64_method_fetch_xor_u64(Nova_AtomicU64* a, uint64_t v) { return __atomic_fetch_xor(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicU64_method_fetch_max_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint64_t Nova_AtomicU64_method_fetch_max_u64(Nova_AtomicU64* a, uint64_t v) {
    uint64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint64_t Nova_AtomicU64_method_fetch_min_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint64_t Nova_AtomicU64_method_fetch_min_u64(Nova_AtomicU64* a, uint64_t v) {
    uint64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint64_t Nova_AtomicU64_method_fetch_nand_MemOrdering(Nova_AtomicU64* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_nand(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicU64_method_fetch_nand_u64(Nova_AtomicU64* a, uint64_t v) { return __atomic_fetch_nand(&a->value, v, __ATOMIC_SEQ_CST); }

/* ── Plan 103.2: AtomicU32 ─────────────────────────────────────── */

typedef struct { uint32_t value; } Nova_AtomicU32;

static inline Nova_AtomicU32* Nova_AtomicU32_static_new(uint32_t v) {
    Nova_AtomicU32* a = (Nova_AtomicU32*)nova_alloc(sizeof(Nova_AtomicU32));
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return a;
}
static inline uint32_t Nova_AtomicU32_method_load_MemOrdering(const Nova_AtomicU32* a, const Nova_MemOrdering* ord) { return __atomic_load_n(&a->value, nova_mo_c(ord)); }
static inline uint32_t Nova_AtomicU32_method_load(const Nova_AtomicU32* a) { return __atomic_load_n(&a->value, __ATOMIC_SEQ_CST); }
static inline nova_unit Nova_AtomicU32_method_store_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) { __atomic_store_n(&a->value, v, nova_mo_c(ord)); return NOVA_UNIT; }
static inline nova_unit Nova_AtomicU32_method_store_u32(Nova_AtomicU32* a, uint32_t v) { __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return NOVA_UNIT; }
static inline uint32_t Nova_AtomicU32_method_swap_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) { return __atomic_exchange_n(&a->value, v, nova_mo_c(ord)); }
static inline uint32_t Nova_AtomicU32_method_swap_u32(Nova_AtomicU32* a, uint32_t v) { return __atomic_exchange_n(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicU32_method_compare_exchange_MemOrdering(Nova_AtomicU32* a, uint32_t e, uint32_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicU32_method_compare_exchange_u32(Nova_AtomicU32* a, uint32_t e, uint32_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicU32_method_compare_exchange_weak_MemOrdering(Nova_AtomicU32* a, uint32_t e, uint32_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicU32_method_compare_exchange_weak_u32(Nova_AtomicU32* a, uint32_t e, uint32_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline uint32_t Nova_AtomicU32_method_fetch_add_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_add(&a->value, v, nova_mo_c(ord)); }
static inline uint32_t Nova_AtomicU32_method_fetch_add_u32(Nova_AtomicU32* a, uint32_t v) { return __atomic_fetch_add(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint32_t Nova_AtomicU32_method_fetch_sub_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_sub(&a->value, v, nova_mo_c(ord)); }
static inline uint32_t Nova_AtomicU32_method_fetch_sub_u32(Nova_AtomicU32* a, uint32_t v) { return __atomic_fetch_sub(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint32_t Nova_AtomicU32_method_fetch_or_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_or(&a->value, v, nova_mo_c(ord)); }
static inline uint32_t Nova_AtomicU32_method_fetch_or_u32(Nova_AtomicU32* a, uint32_t v) { return __atomic_fetch_or(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint32_t Nova_AtomicU32_method_fetch_and_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_and(&a->value, v, nova_mo_c(ord)); }
static inline uint32_t Nova_AtomicU32_method_fetch_and_u32(Nova_AtomicU32* a, uint32_t v) { return __atomic_fetch_and(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint32_t Nova_AtomicU32_method_fetch_xor_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_xor(&a->value, v, nova_mo_c(ord)); }
static inline uint32_t Nova_AtomicU32_method_fetch_xor_u32(Nova_AtomicU32* a, uint32_t v) { return __atomic_fetch_xor(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint32_t Nova_AtomicU32_method_fetch_max_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint32_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint32_t Nova_AtomicU32_method_fetch_max_u32(Nova_AtomicU32* a, uint32_t v) {
    uint32_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint32_t Nova_AtomicU32_method_fetch_min_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint32_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint32_t Nova_AtomicU32_method_fetch_min_u32(Nova_AtomicU32* a, uint32_t v) {
    uint32_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint32_t Nova_AtomicU32_method_fetch_nand_MemOrdering(Nova_AtomicU32* a, uint32_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_nand(&a->value, v, nova_mo_c(ord)); }
static inline uint32_t Nova_AtomicU32_method_fetch_nand_u32(Nova_AtomicU32* a, uint32_t v) { return __atomic_fetch_nand(&a->value, v, __ATOMIC_SEQ_CST); }

/* ── Plan 103.2: AtomicU16 ─────────────────────────────────────── */

typedef struct { uint16_t value; } Nova_AtomicU16;

static inline Nova_AtomicU16* Nova_AtomicU16_static_new(uint16_t v) {
    Nova_AtomicU16* a = (Nova_AtomicU16*)nova_alloc(sizeof(Nova_AtomicU16));
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return a;
}
static inline uint16_t Nova_AtomicU16_method_load_MemOrdering(const Nova_AtomicU16* a, const Nova_MemOrdering* ord) { return __atomic_load_n(&a->value, nova_mo_c(ord)); }
static inline uint16_t Nova_AtomicU16_method_load(const Nova_AtomicU16* a) { return __atomic_load_n(&a->value, __ATOMIC_SEQ_CST); }
static inline nova_unit Nova_AtomicU16_method_store_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) { __atomic_store_n(&a->value, v, nova_mo_c(ord)); return NOVA_UNIT; }
static inline nova_unit Nova_AtomicU16_method_store_u16(Nova_AtomicU16* a, uint16_t v) { __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return NOVA_UNIT; }
static inline uint16_t Nova_AtomicU16_method_swap_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) { return __atomic_exchange_n(&a->value, v, nova_mo_c(ord)); }
static inline uint16_t Nova_AtomicU16_method_swap_u16(Nova_AtomicU16* a, uint16_t v) { return __atomic_exchange_n(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicU16_method_compare_exchange_MemOrdering(Nova_AtomicU16* a, uint16_t e, uint16_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicU16_method_compare_exchange_u16(Nova_AtomicU16* a, uint16_t e, uint16_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicU16_method_compare_exchange_weak_MemOrdering(Nova_AtomicU16* a, uint16_t e, uint16_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicU16_method_compare_exchange_weak_u16(Nova_AtomicU16* a, uint16_t e, uint16_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline uint16_t Nova_AtomicU16_method_fetch_add_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_add(&a->value, v, nova_mo_c(ord)); }
static inline uint16_t Nova_AtomicU16_method_fetch_add_u16(Nova_AtomicU16* a, uint16_t v) { return __atomic_fetch_add(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint16_t Nova_AtomicU16_method_fetch_sub_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_sub(&a->value, v, nova_mo_c(ord)); }
static inline uint16_t Nova_AtomicU16_method_fetch_sub_u16(Nova_AtomicU16* a, uint16_t v) { return __atomic_fetch_sub(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint16_t Nova_AtomicU16_method_fetch_or_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_or(&a->value, v, nova_mo_c(ord)); }
static inline uint16_t Nova_AtomicU16_method_fetch_or_u16(Nova_AtomicU16* a, uint16_t v) { return __atomic_fetch_or(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint16_t Nova_AtomicU16_method_fetch_and_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_and(&a->value, v, nova_mo_c(ord)); }
static inline uint16_t Nova_AtomicU16_method_fetch_and_u16(Nova_AtomicU16* a, uint16_t v) { return __atomic_fetch_and(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint16_t Nova_AtomicU16_method_fetch_xor_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_xor(&a->value, v, nova_mo_c(ord)); }
static inline uint16_t Nova_AtomicU16_method_fetch_xor_u16(Nova_AtomicU16* a, uint16_t v) { return __atomic_fetch_xor(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint16_t Nova_AtomicU16_method_fetch_max_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint16_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint16_t Nova_AtomicU16_method_fetch_max_u16(Nova_AtomicU16* a, uint16_t v) {
    uint16_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint16_t Nova_AtomicU16_method_fetch_min_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint16_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint16_t Nova_AtomicU16_method_fetch_min_u16(Nova_AtomicU16* a, uint16_t v) {
    uint16_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint16_t Nova_AtomicU16_method_fetch_nand_MemOrdering(Nova_AtomicU16* a, uint16_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_nand(&a->value, v, nova_mo_c(ord)); }
static inline uint16_t Nova_AtomicU16_method_fetch_nand_u16(Nova_AtomicU16* a, uint16_t v) { return __atomic_fetch_nand(&a->value, v, __ATOMIC_SEQ_CST); }

/* ── Plan 103.2: AtomicU8 ──────────────────────────────────────── */

typedef struct { uint8_t value; } Nova_AtomicU8;

static inline Nova_AtomicU8* Nova_AtomicU8_static_new(nova_byte v) {
    Nova_AtomicU8* a = (Nova_AtomicU8*)nova_alloc(sizeof(Nova_AtomicU8));
    __atomic_store_n(&a->value, (uint8_t)v, __ATOMIC_SEQ_CST); return a;
}
static inline nova_byte Nova_AtomicU8_method_load_MemOrdering(const Nova_AtomicU8* a, const Nova_MemOrdering* ord) { return (nova_byte)__atomic_load_n(&a->value, nova_mo_c(ord)); }
static inline nova_byte Nova_AtomicU8_method_load(const Nova_AtomicU8* a) { return (nova_byte)__atomic_load_n(&a->value, __ATOMIC_SEQ_CST); }
static inline nova_unit Nova_AtomicU8_method_store_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) { __atomic_store_n(&a->value, (uint8_t)v, nova_mo_c(ord)); return NOVA_UNIT; }
static inline nova_unit Nova_AtomicU8_method_store_u8(Nova_AtomicU8* a, nova_byte v) { __atomic_store_n(&a->value, (uint8_t)v, __ATOMIC_SEQ_CST); return NOVA_UNIT; }
static inline nova_byte Nova_AtomicU8_method_swap_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) { return (nova_byte)__atomic_exchange_n(&a->value, (uint8_t)v, nova_mo_c(ord)); }
static inline nova_byte Nova_AtomicU8_method_swap_u8(Nova_AtomicU8* a, nova_byte v) { return (nova_byte)__atomic_exchange_n(&a->value, (uint8_t)v, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicU8_method_compare_exchange_MemOrdering(Nova_AtomicU8* a, nova_byte ev, nova_byte dv, const Nova_MemOrdering* s, const Nova_MemOrdering* f) {
    uint8_t e = (uint8_t)ev; return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, (uint8_t)dv, false, nova_mo_c(s), nova_mo_c(f));
}
static inline nova_bool Nova_AtomicU8_method_compare_exchange_u8(Nova_AtomicU8* a, nova_byte ev, nova_byte dv) {
    uint8_t e = (uint8_t)ev; return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, (uint8_t)dv, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicU8_method_compare_exchange_weak_MemOrdering(Nova_AtomicU8* a, nova_byte ev, nova_byte dv, const Nova_MemOrdering* s, const Nova_MemOrdering* f) {
    uint8_t e = (uint8_t)ev; return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, (uint8_t)dv, true, nova_mo_c(s), nova_mo_c(f));
}
static inline nova_bool Nova_AtomicU8_method_compare_exchange_weak_u8(Nova_AtomicU8* a, nova_byte ev, nova_byte dv) {
    uint8_t e = (uint8_t)ev; return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, (uint8_t)dv, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
static inline nova_byte Nova_AtomicU8_method_fetch_add_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) { return (nova_byte)__atomic_fetch_add(&a->value, (uint8_t)v, nova_mo_c(ord)); }
static inline nova_byte Nova_AtomicU8_method_fetch_add_u8(Nova_AtomicU8* a, nova_byte v) { return (nova_byte)__atomic_fetch_add(&a->value, (uint8_t)v, __ATOMIC_SEQ_CST); }
static inline nova_byte Nova_AtomicU8_method_fetch_sub_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) { return (nova_byte)__atomic_fetch_sub(&a->value, (uint8_t)v, nova_mo_c(ord)); }
static inline nova_byte Nova_AtomicU8_method_fetch_sub_u8(Nova_AtomicU8* a, nova_byte v) { return (nova_byte)__atomic_fetch_sub(&a->value, (uint8_t)v, __ATOMIC_SEQ_CST); }
static inline nova_byte Nova_AtomicU8_method_fetch_or_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) { return (nova_byte)__atomic_fetch_or(&a->value, (uint8_t)v, nova_mo_c(ord)); }
static inline nova_byte Nova_AtomicU8_method_fetch_or_u8(Nova_AtomicU8* a, nova_byte v) { return (nova_byte)__atomic_fetch_or(&a->value, (uint8_t)v, __ATOMIC_SEQ_CST); }
static inline nova_byte Nova_AtomicU8_method_fetch_and_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) { return (nova_byte)__atomic_fetch_and(&a->value, (uint8_t)v, nova_mo_c(ord)); }
static inline nova_byte Nova_AtomicU8_method_fetch_and_u8(Nova_AtomicU8* a, nova_byte v) { return (nova_byte)__atomic_fetch_and(&a->value, (uint8_t)v, __ATOMIC_SEQ_CST); }
static inline nova_byte Nova_AtomicU8_method_fetch_xor_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) { return (nova_byte)__atomic_fetch_xor(&a->value, (uint8_t)v, nova_mo_c(ord)); }
static inline nova_byte Nova_AtomicU8_method_fetch_xor_u8(Nova_AtomicU8* a, nova_byte v) { return (nova_byte)__atomic_fetch_xor(&a->value, (uint8_t)v, __ATOMIC_SEQ_CST); }
static inline nova_byte Nova_AtomicU8_method_fetch_max_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint8_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED), vv = (uint8_t)v;
    while (cur < vv) { if (__atomic_compare_exchange_n(&a->value, &cur, vv, true, mo, __ATOMIC_RELAXED)) break; } return (nova_byte)cur;
}
static inline nova_byte Nova_AtomicU8_method_fetch_max_u8(Nova_AtomicU8* a, nova_byte v) {
    uint8_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED), vv = (uint8_t)v;
    while (cur < vv) { if (__atomic_compare_exchange_n(&a->value, &cur, vv, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return (nova_byte)cur;
}
static inline nova_byte Nova_AtomicU8_method_fetch_min_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint8_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED), vv = (uint8_t)v;
    while (cur > vv) { if (__atomic_compare_exchange_n(&a->value, &cur, vv, true, mo, __ATOMIC_RELAXED)) break; } return (nova_byte)cur;
}
static inline nova_byte Nova_AtomicU8_method_fetch_min_u8(Nova_AtomicU8* a, nova_byte v) {
    uint8_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED), vv = (uint8_t)v;
    while (cur > vv) { if (__atomic_compare_exchange_n(&a->value, &cur, vv, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return (nova_byte)cur;
}
static inline nova_byte Nova_AtomicU8_method_fetch_nand_MemOrdering(Nova_AtomicU8* a, nova_byte v, const Nova_MemOrdering* ord) { return (nova_byte)__atomic_fetch_nand(&a->value, (uint8_t)v, nova_mo_c(ord)); }
static inline nova_byte Nova_AtomicU8_method_fetch_nand_u8(Nova_AtomicU8* a, nova_byte v) { return (nova_byte)__atomic_fetch_nand(&a->value, (uint8_t)v, __ATOMIC_SEQ_CST); }

/* ── Plan 103.2: AtomicIsize (int = nova_int = int64_t) ─────────── */

typedef struct { nova_int value; } Nova_AtomicIsize;

static inline Nova_AtomicIsize* Nova_AtomicIsize_static_new(nova_int v) {
    Nova_AtomicIsize* a = (Nova_AtomicIsize*)nova_alloc(sizeof(Nova_AtomicIsize));
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return a;
}
static inline nova_int Nova_AtomicIsize_method_load_MemOrdering(const Nova_AtomicIsize* a, const Nova_MemOrdering* ord) { return __atomic_load_n(&a->value, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicIsize_method_load(const Nova_AtomicIsize* a) { return __atomic_load_n(&a->value, __ATOMIC_SEQ_CST); }
static inline nova_unit Nova_AtomicIsize_method_store_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) { __atomic_store_n(&a->value, v, nova_mo_c(ord)); return NOVA_UNIT; }
static inline nova_unit Nova_AtomicIsize_method_store_int(Nova_AtomicIsize* a, nova_int v) { __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return NOVA_UNIT; }
static inline nova_int Nova_AtomicIsize_method_swap_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) { return __atomic_exchange_n(&a->value, v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicIsize_method_swap_int(Nova_AtomicIsize* a, nova_int v) { return __atomic_exchange_n(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicIsize_method_compare_exchange_MemOrdering(Nova_AtomicIsize* a, nova_int e, nova_int d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicIsize_method_compare_exchange_int(Nova_AtomicIsize* a, nova_int e, nova_int d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicIsize_method_compare_exchange_weak_MemOrdering(Nova_AtomicIsize* a, nova_int e, nova_int d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicIsize_method_compare_exchange_weak_int(Nova_AtomicIsize* a, nova_int e, nova_int d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline nova_int Nova_AtomicIsize_method_fetch_add_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) { return __atomic_fetch_add(&a->value, v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicIsize_method_fetch_add_int(Nova_AtomicIsize* a, nova_int v) { return __atomic_fetch_add(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_int Nova_AtomicIsize_method_fetch_sub_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) { return __atomic_fetch_sub(&a->value, v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicIsize_method_fetch_sub_int(Nova_AtomicIsize* a, nova_int v) { return __atomic_fetch_sub(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_int Nova_AtomicIsize_method_fetch_or_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) { return __atomic_fetch_or(&a->value, v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicIsize_method_fetch_or_int(Nova_AtomicIsize* a, nova_int v) { return __atomic_fetch_or(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_int Nova_AtomicIsize_method_fetch_and_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) { return __atomic_fetch_and(&a->value, v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicIsize_method_fetch_and_int(Nova_AtomicIsize* a, nova_int v) { return __atomic_fetch_and(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_int Nova_AtomicIsize_method_fetch_xor_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) { return __atomic_fetch_xor(&a->value, v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicIsize_method_fetch_xor_int(Nova_AtomicIsize* a, nova_int v) { return __atomic_fetch_xor(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_int Nova_AtomicIsize_method_fetch_max_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); nova_int cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline nova_int Nova_AtomicIsize_method_fetch_max_int(Nova_AtomicIsize* a, nova_int v) {
    nova_int cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline nova_int Nova_AtomicIsize_method_fetch_min_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); nova_int cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline nova_int Nova_AtomicIsize_method_fetch_min_int(Nova_AtomicIsize* a, nova_int v) {
    nova_int cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline nova_int Nova_AtomicIsize_method_fetch_nand_MemOrdering(Nova_AtomicIsize* a, nova_int v, const Nova_MemOrdering* ord) { return __atomic_fetch_nand(&a->value, v, nova_mo_c(ord)); }
static inline nova_int Nova_AtomicIsize_method_fetch_nand_int(Nova_AtomicIsize* a, nova_int v) { return __atomic_fetch_nand(&a->value, v, __ATOMIC_SEQ_CST); }

/* ── Plan 103.2: AtomicUsize (uint = uint64_t) ──────────────────── */

typedef struct { uint64_t value; } Nova_AtomicUsize;

static inline Nova_AtomicUsize* Nova_AtomicUsize_static_new(uint64_t v) {
    Nova_AtomicUsize* a = (Nova_AtomicUsize*)nova_alloc(sizeof(Nova_AtomicUsize));
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return a;
}
static inline uint64_t Nova_AtomicUsize_method_load_MemOrdering(const Nova_AtomicUsize* a, const Nova_MemOrdering* ord) { return __atomic_load_n(&a->value, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicUsize_method_load(const Nova_AtomicUsize* a) { return __atomic_load_n(&a->value, __ATOMIC_SEQ_CST); }
static inline nova_unit Nova_AtomicUsize_method_store_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) { __atomic_store_n(&a->value, v, nova_mo_c(ord)); return NOVA_UNIT; }
static inline nova_unit Nova_AtomicUsize_method_store_uint(Nova_AtomicUsize* a, uint64_t v) { __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST); return NOVA_UNIT; }
static inline uint64_t Nova_AtomicUsize_method_swap_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_exchange_n(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicUsize_method_swap_uint(Nova_AtomicUsize* a, uint64_t v) { return __atomic_exchange_n(&a->value, v, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicUsize_method_compare_exchange_MemOrdering(Nova_AtomicUsize* a, uint64_t e, uint64_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicUsize_method_compare_exchange_uint(Nova_AtomicUsize* a, uint64_t e, uint64_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline nova_bool Nova_AtomicUsize_method_compare_exchange_weak_MemOrdering(Nova_AtomicUsize* a, uint64_t e, uint64_t d, const Nova_MemOrdering* s, const Nova_MemOrdering* f) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, nova_mo_c(s), nova_mo_c(f)); }
static inline nova_bool Nova_AtomicUsize_method_compare_exchange_weak_uint(Nova_AtomicUsize* a, uint64_t e, uint64_t d) { return (nova_bool)__atomic_compare_exchange_n(&a->value, &e, d, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicUsize_method_fetch_add_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_add(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicUsize_method_fetch_add_uint(Nova_AtomicUsize* a, uint64_t v) { return __atomic_fetch_add(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicUsize_method_fetch_sub_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_sub(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicUsize_method_fetch_sub_uint(Nova_AtomicUsize* a, uint64_t v) { return __atomic_fetch_sub(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicUsize_method_fetch_or_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_or(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicUsize_method_fetch_or_uint(Nova_AtomicUsize* a, uint64_t v) { return __atomic_fetch_or(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicUsize_method_fetch_and_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_and(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicUsize_method_fetch_and_uint(Nova_AtomicUsize* a, uint64_t v) { return __atomic_fetch_and(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicUsize_method_fetch_xor_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_xor(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicUsize_method_fetch_xor_uint(Nova_AtomicUsize* a, uint64_t v) { return __atomic_fetch_xor(&a->value, v, __ATOMIC_SEQ_CST); }
static inline uint64_t Nova_AtomicUsize_method_fetch_max_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint64_t Nova_AtomicUsize_method_fetch_max_uint(Nova_AtomicUsize* a, uint64_t v) {
    uint64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur < v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint64_t Nova_AtomicUsize_method_fetch_min_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) {
    int mo = nova_mo_c(ord); uint64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, mo, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint64_t Nova_AtomicUsize_method_fetch_min_uint(Nova_AtomicUsize* a, uint64_t v) {
    uint64_t cur = __atomic_load_n(&a->value, __ATOMIC_RELAXED);
    while (cur > v) { if (__atomic_compare_exchange_n(&a->value, &cur, v, true, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) break; } return cur;
}
static inline uint64_t Nova_AtomicUsize_method_fetch_nand_MemOrdering(Nova_AtomicUsize* a, uint64_t v, const Nova_MemOrdering* ord) { return __atomic_fetch_nand(&a->value, v, nova_mo_c(ord)); }
static inline uint64_t Nova_AtomicUsize_method_fetch_nand_uint(Nova_AtomicUsize* a, uint64_t v) { return __atomic_fetch_nand(&a->value, v, __ATOMIC_SEQ_CST); }

/* ── AtomicBool ────────────────────────────────────────────────── */

/* AtomicBool wraps nova_atomic_bool (bool). Useful for flags that are set
 * once (e.g., cancel sentinels) or toggled atomically.
 *
 * Plan 103.2: all methods now have both a default (SeqCst) and an
 * explicit-ordering variant. Suffix rule (last-param): methods with a bool
 * param get _bool suffix, methods with MemOrdering get _MemOrdering suffix.
 * load() has 0 params → no suffix (two overloads: load vs load_MemOrdering). */
typedef struct {
    nova_atomic_bool value;
} Nova_AtomicBool;

static inline Nova_AtomicBool* Nova_AtomicBool_static_new(nova_bool v) {
    Nova_AtomicBool* a = (Nova_AtomicBool*)nova_alloc(sizeof(Nova_AtomicBool));
    __atomic_store_n(&a->value, (bool)v, __ATOMIC_SEQ_CST);
    return a;
}

/* load(): 0 params → no suffix; load_MemOrdering: explicit. */
static inline nova_bool Nova_AtomicBool_method_load(const Nova_AtomicBool* a) {
    return (nova_bool)__atomic_load_n(&a->value, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicBool_method_load_MemOrdering(const Nova_AtomicBool* a, const Nova_MemOrdering* ord) {
    return (nova_bool)__atomic_load_n(&a->value, nova_mo_c(ord));
}

/* store_bool / store_MemOrdering. */
static inline nova_unit Nova_AtomicBool_method_store_bool(Nova_AtomicBool* a, nova_bool v) {
    __atomic_store_n(&a->value, (bool)v, __ATOMIC_SEQ_CST);
    return NOVA_UNIT;
}
static inline nova_unit Nova_AtomicBool_method_store_MemOrdering(Nova_AtomicBool* a, nova_bool v, const Nova_MemOrdering* ord) {
    __atomic_store_n(&a->value, (bool)v, nova_mo_c(ord));
    return NOVA_UNIT;
}

/* swap_bool / swap_MemOrdering. */
static inline nova_bool Nova_AtomicBool_method_swap_bool(Nova_AtomicBool* a, nova_bool v) {
    return (nova_bool)__atomic_exchange_n(&a->value, (bool)v, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicBool_method_swap_MemOrdering(Nova_AtomicBool* a, nova_bool v, const Nova_MemOrdering* ord) {
    return (nova_bool)__atomic_exchange_n(&a->value, (bool)v, nova_mo_c(ord));
}

/* compare_exchange_bool (strong, SeqCst) / compare_exchange_MemOrdering. */
static inline nova_bool Nova_AtomicBool_method_compare_exchange_bool(
        Nova_AtomicBool* a, nova_bool expected_val, nova_bool desired) {
    bool exp = (bool)expected_val;
    return (nova_bool)__atomic_compare_exchange_n(
        &a->value, &exp, (bool)desired,
        false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicBool_method_compare_exchange_MemOrdering(
        Nova_AtomicBool* a, nova_bool expected_val, nova_bool desired,
        const Nova_MemOrdering* success, const Nova_MemOrdering* failure) {
    bool exp = (bool)expected_val;
    return (nova_bool)__atomic_compare_exchange_n(
        &a->value, &exp, (bool)desired,
        false, nova_mo_c(success), nova_mo_c(failure));
}

/* compare_exchange_weak_bool / compare_exchange_weak_MemOrdering. */
static inline nova_bool Nova_AtomicBool_method_compare_exchange_weak_bool(
        Nova_AtomicBool* a, nova_bool expected_val, nova_bool desired) {
    bool exp = (bool)expected_val;
    return (nova_bool)__atomic_compare_exchange_n(
        &a->value, &exp, (bool)desired,
        true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicBool_method_compare_exchange_weak_MemOrdering(
        Nova_AtomicBool* a, nova_bool expected_val, nova_bool desired,
        const Nova_MemOrdering* success, const Nova_MemOrdering* failure) {
    bool exp = (bool)expected_val;
    return (nova_bool)__atomic_compare_exchange_n(
        &a->value, &exp, (bool)desired,
        true, nova_mo_c(success), nova_mo_c(failure));
}

/* fetch_or_bool / fetch_or_MemOrdering. */
static inline nova_bool Nova_AtomicBool_method_fetch_or_bool(Nova_AtomicBool* a, nova_bool v) {
    return (nova_bool)__atomic_fetch_or(&a->value, (bool)v, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicBool_method_fetch_or_MemOrdering(Nova_AtomicBool* a, nova_bool v, const Nova_MemOrdering* ord) {
    return (nova_bool)__atomic_fetch_or(&a->value, (bool)v, nova_mo_c(ord));
}

/* fetch_and_bool / fetch_and_MemOrdering. */
static inline nova_bool Nova_AtomicBool_method_fetch_and_bool(Nova_AtomicBool* a, nova_bool v) {
    return (nova_bool)__atomic_fetch_and(&a->value, (bool)v, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicBool_method_fetch_and_MemOrdering(Nova_AtomicBool* a, nova_bool v, const Nova_MemOrdering* ord) {
    return (nova_bool)__atomic_fetch_and(&a->value, (bool)v, nova_mo_c(ord));
}

/* fetch_xor_bool / fetch_xor_MemOrdering. */
static inline nova_bool Nova_AtomicBool_method_fetch_xor_bool(Nova_AtomicBool* a, nova_bool v) {
    return (nova_bool)__atomic_fetch_xor(&a->value, (bool)v, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicBool_method_fetch_xor_MemOrdering(Nova_AtomicBool* a, nova_bool v, const Nova_MemOrdering* ord) {
    return (nova_bool)__atomic_fetch_xor(&a->value, (bool)v, nova_mo_c(ord));
}

/* ── AtomicPtr ─────────────────────────────────────────────────── */

/* AtomicPtr stores a pointer-sized integer (GC-object address proxy).
 * Generic AtomicPtr[T] deferred to Plan 103.7; for now uses nova_int as
 * the underlying type, which covers 64-bit pointer addresses on all targets.
 *
 * Naming: store(v int) → _int suffix; store(v int, ord) → _MemOrdering suffix.
 * compare_exchange(exp, des) → _int; compare_exchange(exp, des, s, f) → _MemOrdering. */
typedef struct {
    nova_int value;
} Nova_AtomicPtr;

static inline Nova_AtomicPtr* Nova_AtomicPtr_static_null(void) {
    Nova_AtomicPtr* a = (Nova_AtomicPtr*)nova_alloc(sizeof(Nova_AtomicPtr));
    __atomic_store_n(&a->value, (nova_int)0, __ATOMIC_SEQ_CST);
    return a;
}

static inline Nova_AtomicPtr* Nova_AtomicPtr_static_new(nova_int v) {
    Nova_AtomicPtr* a = (Nova_AtomicPtr*)nova_alloc(sizeof(Nova_AtomicPtr));
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST);
    return a;
}

static inline nova_int Nova_AtomicPtr_method_load(const Nova_AtomicPtr* a) {
    return __atomic_load_n(&a->value, __ATOMIC_SEQ_CST);
}
static inline nova_int Nova_AtomicPtr_method_load_MemOrdering(const Nova_AtomicPtr* a, const Nova_MemOrdering* ord) {
    return __atomic_load_n(&a->value, nova_mo_c(ord));
}

static inline nova_unit Nova_AtomicPtr_method_store_int(Nova_AtomicPtr* a, nova_int v) {
    __atomic_store_n(&a->value, v, __ATOMIC_SEQ_CST);
    return NOVA_UNIT;
}
static inline nova_unit Nova_AtomicPtr_method_store_MemOrdering(Nova_AtomicPtr* a, nova_int v, const Nova_MemOrdering* ord) {
    __atomic_store_n(&a->value, v, nova_mo_c(ord));
    return NOVA_UNIT;
}

static inline nova_int Nova_AtomicPtr_method_swap_int(Nova_AtomicPtr* a, nova_int v) {
    return __atomic_exchange_n(&a->value, v, __ATOMIC_SEQ_CST);
}
static inline nova_int Nova_AtomicPtr_method_swap_MemOrdering(Nova_AtomicPtr* a, nova_int v, const Nova_MemOrdering* ord) {
    return __atomic_exchange_n(&a->value, v, nova_mo_c(ord));
}

static inline nova_bool Nova_AtomicPtr_method_compare_exchange_int(
        Nova_AtomicPtr* a, nova_int expected, nova_int desired) {
    return (nova_bool)__atomic_compare_exchange_n(
        &a->value, &expected, desired, false, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicPtr_method_compare_exchange_MemOrdering(
        Nova_AtomicPtr* a, nova_int expected, nova_int desired,
        const Nova_MemOrdering* success, const Nova_MemOrdering* failure) {
    return (nova_bool)__atomic_compare_exchange_n(
        &a->value, &expected, desired, false, nova_mo_c(success), nova_mo_c(failure));
}

static inline nova_bool Nova_AtomicPtr_method_compare_exchange_weak_int(
        Nova_AtomicPtr* a, nova_int expected, nova_int desired) {
    return (nova_bool)__atomic_compare_exchange_n(
        &a->value, &expected, desired, true, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
static inline nova_bool Nova_AtomicPtr_method_compare_exchange_weak_MemOrdering(
        Nova_AtomicPtr* a, nova_int expected, nova_int desired,
        const Nova_MemOrdering* success, const Nova_MemOrdering* failure) {
    return (nova_bool)__atomic_compare_exchange_n(
        &a->value, &expected, desired, true, nova_mo_c(success), nova_mo_c(failure));
}

/* ── Plan 103.3: TLF timer state (try_lock_for / try_read_for / try_write_for)
 *
 * NovaMutexTLFHandle is raw-malloc'd (NOT GC-managed). Lifecycle:
 *   allocated in try_lock_for() before park.
 *   timer_cb or close_cb frees it (via _nova_mutex_tlf_close_cb).
 *
 * Protocol (all under lock's internal mu for serialization):
 *   - try_lock_for(): alloc handle, enqueue timed waiter, start timer, park.
 *   - On acquire (unlock transfers lock to waiter): set handle->waiter=NULL,
 *     wake fiber. Timer will eventually fire, see waiter==NULL, call uv_close.
 *   - On timeout (timer fires first): remove waiter from queue, set
 *     waiter->timed_out=true, set handle->waiter=NULL, wake fiber, uv_close.
 *   - close_cb: frees handle (raw malloc). */

typedef struct NovaMutexTLFHandle {
    uv_timer_t  timer;    /* embedded; must be first (timer.data=handle) */
    void*       mutex;    /* Nova_Mutex* — forward-compatible (struct defined below) */
    void*       waiter;   /* NovaMutexWaiter* or NULL */
} NovaMutexTLFHandle;

static void _nova_mutex_tlf_close_cb(uv_handle_t* h) {
    free(h->data);   /* free NovaMutexTLFHandle (raw malloc) */
}

/* ── Mutex waiter ──────────────────────────────────────────────── */

typedef struct NovaMutexWaiter {
    NovaFiberQueue*         scope;
    int                     slot;
    struct NovaMutexWaiter* next;
    struct NovaMutexWaiter* prev;
    /* Plan 103.3: extended fields for try_lock_for(). Zero-init for lock(). */
    bool                    timed_out;   /* set by timer_cb before wake */
    NovaMutexTLFHandle*     tlf_handle;  /* NULL for plain lock() waiters */
} NovaMutexWaiter;

/* ── Mutex ─────────────────────────────────────────────────────── */

/* Fair FIFO Mutex (default) / unfair LIFO opt-in (new_unfair()).
 *
 * Fair mode: waiters queued in arrival order; unlock() pops from head.
 * Unfair mode: unlock() pops from tail (LIFO). Higher throughput under
 *   short critical sections; starvation possible.
 *
 * NOT reentrant: lock() from the same fiber that holds the lock deadlocks.
 * unlock() without a matching lock() → unconditional runtime panic (Plan 103.3:
 * same pattern as Nova_Once_method_done — fires in both Dev AND Release builds). */
typedef struct {
    nova_mutex_t      mu;       /* guards locked + waiter list */
    bool              locked;
    bool              unfair;   /* Plan 103.3: LIFO pop if true */
    NovaMutexWaiter*  head;
    NovaMutexWaiter*  tail;
} Nova_Mutex;

/* ── Mutex timer callback (fires when try_lock_for timeout expires) ─
 * Defined after Nova_Mutex so we can use it by name (forward-compat: we used
 * void* in NovaMutexTLFHandle.mutex, so no circular-struct issue). */

static void _nova_mutex_tlf_timer_cb(uv_timer_t* h) {
    NovaMutexTLFHandle* handle = (NovaMutexTLFHandle*)h->data;
    Nova_Mutex* m = (Nova_Mutex*)handle->mutex;
    nova_mutex_lock(&m->mu);
    NovaMutexWaiter* w = (NovaMutexWaiter*)handle->waiter;
    if (w != NULL) {
        /* Timer won the race — remove waiter from queue. */
        if (w->prev) w->prev->next = w->next;
        else         m->head = w->next;
        if (w->next) w->next->prev = w->prev;
        else         m->tail = w->prev;
        w->timed_out   = true;
        handle->waiter = NULL;
        NovaFiberQueue* scope = w->scope;
        int slot = w->slot;
        nova_mutex_unlock(&m->mu);
        nova_sched_wake(scope, slot);
    } else {
        /* unlock() already transferred lock: this timer fires as no-op. */
        nova_mutex_unlock(&m->mu);
    }
    uv_close((uv_handle_t*)h, _nova_mutex_tlf_close_cb);
}

static inline Nova_Mutex* Nova_Mutex_static_new(void) {
    /* Plan 103.3: uncollectable to prevent GC race under M:N on Windows
     * (Boehm may miss the mutex pointer on main thread's stack during
     * worker materialization, causing premature collection). */
    Nova_Mutex* m = (Nova_Mutex*)nova_alloc_uncollectable(sizeof(Nova_Mutex));
    nova_mutex_init(&m->mu);
    m->locked = false;
    m->unfair  = false;
    m->head   = NULL;
    m->tail   = NULL;
    return m;
}

/* Plan 103.3: unfair opt-in constructor. Same layout; LIFO pop in unlock(). */
static inline Nova_Mutex* Nova_Mutex_static_new_unfair(void) {
    Nova_Mutex* m = Nova_Mutex_static_new();
    m->unfair = true;
    return m;
}

static inline nova_bool Nova_Mutex_method_try_lock(Nova_Mutex* m) {
    nova_mutex_lock(&m->mu);
    if (!m->locked) {
        m->locked = true;
        nova_mutex_unlock(&m->mu);
        return true;
    }
    nova_mutex_unlock(&m->mu);
    return false;
}

static inline nova_unit Nova_Mutex_method_lock(Nova_Mutex* m) {
    nova_mutex_lock(&m->mu);
    if (!m->locked) {
        m->locked = true;
        nova_mutex_unlock(&m->mu);
        return NOVA_UNIT;
    }
    if (_nova_active_slot < 0) {
        /* Non-fiber: spin with CPU yield to avoid burning the bus. */
        nova_mutex_unlock(&m->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&m->mu);
            if (!m->locked) {
                m->locked = true;
                nova_mutex_unlock(&m->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&m->mu);
        }
    }
    /* Fiber path: register as waiter and park atomically with unlock. */
    NovaMutexWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.next       = NULL;
    w.prev       = m->tail;
    w.timed_out  = false;
    w.tlf_handle = NULL;
    if (m->tail) m->tail->next = &w;
    else         m->head = &w;
    m->tail = &w;
    /* park_with_unlock: parks fiber first, then releases mu. Prevents
     * lost-wakeup race (unlock cannot fire before park is registered). */
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &m->mu);
    /* Resumed: lock ownership transferred from unlock() — no re-check needed. */
    return NOVA_UNIT;
}

/* Plan 103.3: try_lock_for(Duration) — attempt to acquire within timeout.
 * Returns true if acquired, false if timeout expired.
 * Timeout <= 0: behaves as try_lock() (non-blocking).
 * Fiber path: arms a libuv timer; parks until lock acquired or timer fires.
 * Non-fiber path: spin-poll until deadline. */
static inline nova_bool Nova_Mutex_method_try_lock_for(Nova_Mutex* m,
                                                        void* timeout) {
    /* timeout is Nova_Duration* — void* avoids include-order dep;
     * first field is int64_t nanos. */
    int64_t nanos = *(int64_t*)timeout;
    if (nanos <= 0) return Nova_Mutex_method_try_lock(m);

    /* Fast path: check immediately before any timer work. */
    nova_mutex_lock(&m->mu);
    if (!m->locked) {
        m->locked = true;
        nova_mutex_unlock(&m->mu);
        return true;
    }

    if (_nova_active_slot < 0) {
        /* Non-fiber: spin-poll with deadline. */
        nova_mutex_unlock(&m->mu);
        int64_t deadline = _nova_monotonic_ns() + nanos;
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&m->mu);
            if (!m->locked) {
                m->locked = true;
                nova_mutex_unlock(&m->mu);
                return true;
            }
            nova_mutex_unlock(&m->mu);
            if (_nova_monotonic_ns() >= deadline) return false;
        }
    }

    /* Fiber path: set up timer + register as timed waiter. */
    uint64_t delay_ms = (uint64_t)((nanos + 999999LL) / 1000000LL);
    if (delay_ms == 0) delay_ms = 1;

    /* Allocate timer state on heap (libuv owns until close_cb). */
    NovaMutexTLFHandle* handle = (NovaMutexTLFHandle*)malloc(sizeof(NovaMutexTLFHandle));
    if (!handle) {
        nova_mutex_unlock(&m->mu);
        fprintf(stderr, "nova: Mutex.try_lock_for: malloc failed\n");
        abort();
    }
    handle->mutex      = (void*)m;
    handle->timer.data = handle;

    /* Stack waiter (valid until fiber returns from this function). */
    NovaMutexWaiter w;
    w.scope      = _nova_active_scope;
    w.slot       = _nova_active_slot;
    w.timed_out  = false;
    w.tlf_handle = handle;
    handle->waiter = &w;

    /* Enqueue waiter (under mu held since fast-path check above). */
    w.next = NULL;
    w.prev = m->tail;
    if (m->tail) m->tail->next = &w;
    else         m->head = &w;
    m->tail = &w;

    /* Start timer (safe to call under mu — doesn't block). */
    int rc = uv_timer_init(nova_current_loop(), &handle->timer);
    if (rc != 0) {
        /* Remove waiter from queue and bail. */
        if (w.prev) w.prev->next = NULL; else m->head = NULL;
        m->tail = w.prev;
        nova_mutex_unlock(&m->mu);
        free(handle);
        return false;
    }
    rc = uv_timer_start(&handle->timer, _nova_mutex_tlf_timer_cb, delay_ms, 0);
    if (rc != 0) {
        if (w.prev) w.prev->next = NULL; else m->head = NULL;
        m->tail = w.prev;
        nova_mutex_unlock(&m->mu);
        uv_close((uv_handle_t*)&handle->timer, _nova_mutex_tlf_close_cb);
        return false;
    }

    /* Park atomically with mu release. */
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &m->mu);
    /* Resumed: either lock transferred (timed_out=false) or timer fired (timed_out=true). */
    return !w.timed_out;
}

/* Plan 103.3: is_locked() — best-effort observability. NOT for CAS. */
static inline nova_bool Nova_Mutex_method_is_locked(const Nova_Mutex* m) {
    /* Relaxed load suffices for best-effort check (plan D169 §3). */
    return (nova_bool)__atomic_load_n((const bool*)&m->locked, __ATOMIC_RELAXED);
}

static inline nova_unit Nova_Mutex_method_unlock(Nova_Mutex* m) {
    nova_mutex_lock(&m->mu);
    /* Plan 103.3: unconditional check (fires in Dev AND Release — same pattern
     * as Nova_Once_method_done). NOVA_SYNC_ASSERT was a no-op in Dev/Release. */
    if (!m->locked) {
        nova_mutex_unlock(&m->mu);
        Nova_Fail_fail(nova_str_from_cstr("Mutex.unlock() called on an unlocked mutex"));
        nova_throw(nova_str_from_cstr("Mutex.unlock() called on an unlocked mutex"));
    }
    if (m->head) {
        /* Plan 103.3: unfair = LIFO (pop from tail); fair = FIFO (pop from head). */
        NovaMutexWaiter* w;
        if (m->unfair) {
            w = m->tail;
            m->tail = w->prev;
            if (m->tail) m->tail->next = NULL;
            else         m->head = NULL;
        } else {
            w = m->head;
            m->head = w->next;
            if (m->head) m->head->prev = NULL;
            else         m->tail = NULL;
        }
        /* Plan 103.3: nullify handle->waiter before wake so timer_cb becomes no-op. */
        if (w->tlf_handle) w->tlf_handle->waiter = NULL;
        /* Transfer lock ownership: locked stays true. */
        NovaFiberQueue* scope = w->scope;
        int slot = w->slot;
        nova_mutex_unlock(&m->mu);
        nova_sched_wake(scope, slot);
    } else {
        m->locked = false;
        nova_mutex_unlock(&m->mu);
    }
    return NOVA_UNIT;
}

/* ── Plan 103.3: RwLock TLF handle ────────────────────────────────── */

typedef struct NovaRwLockTLFHandle {
    uv_timer_t  timer;
    void*       rwlock;     /* Nova_RwLock* — forward-compatible */
    void*       waiter;     /* NovaRwLockWaiter* or NULL */
    bool        is_writer;
} NovaRwLockTLFHandle;

static void _nova_rwlock_tlf_close_cb(uv_handle_t* h) { free(h->data); }

/* ── Plan 103.3: RwLock waiter ─────────────────────────────────────── */

typedef struct NovaRwLockWaiter {
    NovaFiberQueue*             scope;
    int                         slot;
    struct NovaRwLockWaiter*    next;
    struct NovaRwLockWaiter*    prev;
    bool                        timed_out;
    NovaRwLockTLFHandle*        tlf_handle;
} NovaRwLockWaiter;

/* ── Plan 103.3: RwLock ────────────────────────────────────────────── */

/* Fiber-aware reader-writer lock.
 *
 * Default: writer-priority (prevents writer starvation in read-heavy workloads).
 *   read(): if write_locked OR (write_waiting AND !reader_priority) → park.
 *   write(): set write_waiting=true, park until !write_locked AND reader_count==0.
 *
 * reader_priority opt-in (new_reader_priority()): new readers bypass the
 *   write_waiting gate — writers may starve if readers arrive continuously.
 *
 * read_unlock/write_unlock: unconditional invariant check (Plan 103.3 pattern). */
typedef struct {
    nova_mutex_t        mu;
    int                 reader_count;   /* # active readers */
    bool                write_locked;   /* true while a writer holds the lock */
    bool                write_waiting;  /* >=1 writer queued (writer-priority gate) */
    bool                reader_priority;/* bypass write_waiting gate on read() */
    NovaRwLockWaiter*   reader_head;
    NovaRwLockWaiter*   reader_tail;
    NovaRwLockWaiter*   writer_head;
    NovaRwLockWaiter*   writer_tail;
} Nova_RwLock;

/* Forward-declare timer callback (defined after Nova_RwLock). */
static void _nova_rwlock_tlf_timer_cb(uv_timer_t* h);

static inline Nova_RwLock* Nova_RwLock_static_new(void) {
    /* Plan 103.3: uncollectable — same GC-race fix as Nova_Mutex/Nova_ReentrantMutex. */
    Nova_RwLock* rw = (Nova_RwLock*)nova_alloc_uncollectable(sizeof(Nova_RwLock));
    nova_mutex_init(&rw->mu);
    rw->reader_count   = 0;
    rw->write_locked   = false;
    rw->write_waiting  = false;
    rw->reader_priority = false;
    rw->reader_head = rw->reader_tail = NULL;
    rw->writer_head = rw->writer_tail = NULL;
    return rw;
}

static inline Nova_RwLock* Nova_RwLock_static_new_reader_priority(void) {
    Nova_RwLock* rw = Nova_RwLock_static_new();
    rw->reader_priority = true;
    return rw;
}

/* Internal: wake all parked readers. Called under mu. */
static inline void _nova_rwlock_wake_readers(Nova_RwLock* rw) {
    NovaRwLockWaiter* cur = rw->reader_head;
    rw->reader_head = rw->reader_tail = NULL;
    while (cur) {
        /* Skip timed-out readers (handle->waiter already NULL). */
        if (cur->tlf_handle && cur->tlf_handle->waiter == NULL) {
            cur = cur->next;
            continue;
        }
        rw->reader_count++;
        if (cur->tlf_handle) cur->tlf_handle->waiter = NULL;
        NovaFiberQueue* s = cur->scope;
        int slot = cur->slot;
        cur = cur->next;
        nova_sched_wake(s, slot);
    }
}

/* Internal: wake one writer (direct ownership transfer). Called under mu. */
static inline void _nova_rwlock_wake_one_writer(Nova_RwLock* rw) {
    while (rw->writer_head) {
        NovaRwLockWaiter* w = rw->writer_head;
        rw->writer_head = w->next;
        if (rw->writer_head) rw->writer_head->prev = NULL;
        else                 rw->writer_tail = NULL;
        /* Skip timed-out writers. */
        if (w->tlf_handle && w->tlf_handle->waiter == NULL) continue;
        /* Transfer write ownership. */
        rw->write_locked  = true;
        rw->write_waiting = (rw->writer_head != NULL);
        if (w->tlf_handle) w->tlf_handle->waiter = NULL;
        nova_sched_wake(w->scope, w->slot);
        return;
    }
    /* No active writer waiters. */
    rw->write_waiting = false;
}

/* Helper: enqueue a timed waiter and start its timer. Called under rw->mu. */
static inline bool _nova_rwlock_start_timed_waiter(
        Nova_RwLock* rw, NovaRwLockWaiter* w,
        NovaRwLockWaiter** head, NovaRwLockWaiter** tail,
        uint64_t delay_ms, bool is_writer) {
    NovaRwLockTLFHandle* handle = (NovaRwLockTLFHandle*)malloc(sizeof(NovaRwLockTLFHandle));
    if (!handle) return false;
    handle->rwlock     = (void*)rw;
    handle->waiter     = w;
    handle->is_writer  = is_writer;
    handle->timer.data = handle;
    w->tlf_handle = handle;

    /* Enqueue. */
    w->next = NULL;
    w->prev = *tail;
    if (*tail) (*tail)->next = w;
    else       *head = w;
    *tail = w;

    int rc = uv_timer_init(nova_current_loop(), &handle->timer);
    if (rc != 0) { /* cleanup */
        if (w->prev) w->prev->next = NULL; else *head = NULL;
        *tail = w->prev;
        free(handle);
        w->tlf_handle = NULL;
        return false;
    }
    rc = uv_timer_start(&handle->timer, _nova_rwlock_tlf_timer_cb, delay_ms, 0);
    if (rc != 0) {
        if (w->prev) w->prev->next = NULL; else *head = NULL;
        *tail = w->prev;
        uv_close((uv_handle_t*)&handle->timer, _nova_rwlock_tlf_close_cb);
        w->tlf_handle = NULL;
        return false;
    }
    return true;
}

static inline nova_unit Nova_RwLock_method_read(Nova_RwLock* rw) {
    nova_mutex_lock(&rw->mu);
    bool block = rw->write_locked || (!rw->reader_priority && rw->write_waiting);
    if (!block) {
        rw->reader_count++;
        nova_mutex_unlock(&rw->mu);
        return NOVA_UNIT;
    }
    if (_nova_active_slot < 0) {
        nova_mutex_unlock(&rw->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&rw->mu);
            if (!rw->write_locked && (rw->reader_priority || !rw->write_waiting)) {
                rw->reader_count++;
                nova_mutex_unlock(&rw->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&rw->mu);
        }
    }
    NovaRwLockWaiter w = { .scope=_nova_active_scope, .slot=_nova_active_slot,
                            .next=NULL, .prev=rw->reader_tail,
                            .timed_out=false, .tlf_handle=NULL };
    if (rw->reader_tail) rw->reader_tail->next = &w;
    else                 rw->reader_head = &w;
    rw->reader_tail = &w;
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &rw->mu);
    return NOVA_UNIT;
}

static inline nova_bool Nova_RwLock_method_try_read(Nova_RwLock* rw) {
    nova_mutex_lock(&rw->mu);
    if (!rw->write_locked && (rw->reader_priority || !rw->write_waiting)) {
        rw->reader_count++;
        nova_mutex_unlock(&rw->mu);
        return true;
    }
    nova_mutex_unlock(&rw->mu);
    return false;
}

static inline nova_bool Nova_RwLock_method_try_read_for(Nova_RwLock* rw, void* timeout) {
    /* timeout is Nova_Duration* — void* avoids include-order dep. */
    int64_t tnanos = *(int64_t*)timeout;
    if (tnanos <= 0) return Nova_RwLock_method_try_read(rw);
    nova_mutex_lock(&rw->mu);
    if (!rw->write_locked && (rw->reader_priority || !rw->write_waiting)) {
        rw->reader_count++;
        nova_mutex_unlock(&rw->mu);
        return true;
    }
    if (_nova_active_slot < 0) {
        nova_mutex_unlock(&rw->mu);
        int64_t deadline = _nova_monotonic_ns() + tnanos;
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&rw->mu);
            if (!rw->write_locked && (rw->reader_priority || !rw->write_waiting)) {
                rw->reader_count++;
                nova_mutex_unlock(&rw->mu);
                return true;
            }
            nova_mutex_unlock(&rw->mu);
            if (_nova_monotonic_ns() >= deadline) return false;
        }
    }
    uint64_t delay_ms = (uint64_t)((tnanos + 999999LL) / 1000000LL);
    if (delay_ms == 0) delay_ms = 1;
    NovaRwLockWaiter w = { .scope=_nova_active_scope, .slot=_nova_active_slot,
                            .next=NULL, .prev=NULL, .timed_out=false, .tlf_handle=NULL };
    if (!_nova_rwlock_start_timed_waiter(rw, &w, &rw->reader_head, &rw->reader_tail,
                                          delay_ms, false)) {
        nova_mutex_unlock(&rw->mu);
        return false;
    }
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &rw->mu);
    return !w.timed_out;
}

static inline nova_unit Nova_RwLock_method_read_unlock(Nova_RwLock* rw) {
    nova_mutex_lock(&rw->mu);
    if (rw->reader_count <= 0 || rw->write_locked) {
        nova_mutex_unlock(&rw->mu);
        Nova_Fail_fail(nova_str_from_cstr("RwLock.read_unlock() called without a matching read()"));
        nova_throw(nova_str_from_cstr("RwLock.read_unlock() called without a matching read()"));
    }
    rw->reader_count--;
    if (rw->reader_count == 0 && rw->write_waiting) {
        _nova_rwlock_wake_one_writer(rw);
        nova_mutex_unlock(&rw->mu);
    } else {
        nova_mutex_unlock(&rw->mu);
    }
    return NOVA_UNIT;
}

static inline nova_unit Nova_RwLock_method_write(Nova_RwLock* rw) {
    nova_mutex_lock(&rw->mu);
    if (!rw->write_locked && rw->reader_count == 0) {
        rw->write_locked  = true;
        rw->write_waiting = (rw->writer_head != NULL);
        nova_mutex_unlock(&rw->mu);
        return NOVA_UNIT;
    }
    if (_nova_active_slot < 0) {
        rw->write_waiting = true;
        nova_mutex_unlock(&rw->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&rw->mu);
            if (!rw->write_locked && rw->reader_count == 0) {
                rw->write_locked  = true;
                rw->write_waiting = (rw->writer_head != NULL);
                nova_mutex_unlock(&rw->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&rw->mu);
        }
    }
    rw->write_waiting = true;
    NovaRwLockWaiter w = { .scope=_nova_active_scope, .slot=_nova_active_slot,
                            .next=NULL, .prev=rw->writer_tail,
                            .timed_out=false, .tlf_handle=NULL };
    if (rw->writer_tail) rw->writer_tail->next = &w;
    else                 rw->writer_head = &w;
    rw->writer_tail = &w;
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &rw->mu);
    return NOVA_UNIT;
}

static inline nova_bool Nova_RwLock_method_try_write(Nova_RwLock* rw) {
    nova_mutex_lock(&rw->mu);
    if (!rw->write_locked && rw->reader_count == 0) {
        rw->write_locked  = true;
        rw->write_waiting = (rw->writer_head != NULL);
        nova_mutex_unlock(&rw->mu);
        return true;
    }
    nova_mutex_unlock(&rw->mu);
    return false;
}

static inline nova_bool Nova_RwLock_method_try_write_for(Nova_RwLock* rw, void* timeout) {
    /* timeout is Nova_Duration* — void* avoids include-order dep. */
    int64_t tnanos = *(int64_t*)timeout;
    if (tnanos <= 0) return Nova_RwLock_method_try_write(rw);
    nova_mutex_lock(&rw->mu);
    if (!rw->write_locked && rw->reader_count == 0) {
        rw->write_locked  = true;
        rw->write_waiting = (rw->writer_head != NULL);
        nova_mutex_unlock(&rw->mu);
        return true;
    }
    if (_nova_active_slot < 0) {
        nova_mutex_unlock(&rw->mu);
        int64_t deadline = _nova_monotonic_ns() + tnanos;
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&rw->mu);
            if (!rw->write_locked && rw->reader_count == 0) {
                rw->write_locked  = true;
                rw->write_waiting = (rw->writer_head != NULL);
                nova_mutex_unlock(&rw->mu);
                return true;
            }
            nova_mutex_unlock(&rw->mu);
            if (_nova_monotonic_ns() >= deadline) return false;
        }
    }
    uint64_t delay_ms = (uint64_t)((tnanos + 999999LL) / 1000000LL);
    if (delay_ms == 0) delay_ms = 1;
    rw->write_waiting = true;
    NovaRwLockWaiter w = { .scope=_nova_active_scope, .slot=_nova_active_slot,
                            .next=NULL, .prev=NULL, .timed_out=false, .tlf_handle=NULL };
    if (!_nova_rwlock_start_timed_waiter(rw, &w, &rw->writer_head, &rw->writer_tail,
                                          delay_ms, true)) {
        nova_mutex_unlock(&rw->mu);
        return false;
    }
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &rw->mu);
    if (w.timed_out) {
        /* Timer won: update write_waiting flag (may have become false). */
        nova_mutex_lock(&rw->mu);
        if (!rw->writer_head) rw->write_waiting = false;
        nova_mutex_unlock(&rw->mu);
        return false;
    }
    return true;
}

static inline nova_unit Nova_RwLock_method_write_unlock(Nova_RwLock* rw) {
    nova_mutex_lock(&rw->mu);
    if (!rw->write_locked) {
        nova_mutex_unlock(&rw->mu);
        Nova_Fail_fail(nova_str_from_cstr("RwLock.write_unlock() called without a matching write()"));
        nova_throw(nova_str_from_cstr("RwLock.write_unlock() called without a matching write()"));
    }
    rw->write_locked = false;
    if (rw->writer_head) {
        /* Prefer next writer (writer-priority maintained). */
        _nova_rwlock_wake_one_writer(rw);
        nova_mutex_unlock(&rw->mu);
    } else {
        rw->write_waiting = false;
        /* Wake all parked readers. */
        _nova_rwlock_wake_readers(rw);
        nova_mutex_unlock(&rw->mu);
    }
    return NOVA_UNIT;
}

/* Plan 103.3: reader_count() / is_write_locked() — best-effort observability. */
static inline nova_int Nova_RwLock_method_reader_count(const Nova_RwLock* rw) {
    return (nova_int)__atomic_load_n((const int*)&rw->reader_count, __ATOMIC_RELAXED);
}

static inline nova_bool Nova_RwLock_method_is_write_locked(const Nova_RwLock* rw) {
    return (nova_bool)__atomic_load_n((const bool*)&rw->write_locked, __ATOMIC_RELAXED);
}

/* RwLock TLF timer callback (defined after Nova_RwLock is complete). */
static void _nova_rwlock_tlf_timer_cb(uv_timer_t* h) {
    NovaRwLockTLFHandle* handle = (NovaRwLockTLFHandle*)h->data;
    Nova_RwLock* rw = (Nova_RwLock*)handle->rwlock;
    nova_mutex_lock(&rw->mu);
    NovaRwLockWaiter* w = (NovaRwLockWaiter*)handle->waiter;
    if (w != NULL) {
        /* Remove from appropriate queue. */
        if (handle->is_writer) {
            if (w->prev) w->prev->next = w->next; else rw->writer_head = w->next;
            if (w->next) w->next->prev = w->prev; else rw->writer_tail = w->prev;
            if (!rw->writer_head) rw->write_waiting = false;
        } else {
            if (w->prev) w->prev->next = w->next; else rw->reader_head = w->next;
            if (w->next) w->next->prev = w->prev; else rw->reader_tail = w->prev;
        }
        w->timed_out   = true;
        handle->waiter = NULL;
        NovaFiberQueue* s = w->scope;
        int slot = w->slot;
        nova_mutex_unlock(&rw->mu);
        nova_sched_wake(s, slot);
    } else {
        nova_mutex_unlock(&rw->mu);
    }
    uv_close((uv_handle_t*)h, _nova_rwlock_tlf_close_cb);
}

/* ── Plan 103.3: ReentrantMutex TLF handle ─────────────────────────── */

typedef struct NovaReMutexTLFHandle {
    uv_timer_t  timer;
    void*       relock;   /* Nova_ReentrantMutex* — forward-compatible */
    void*       waiter;
} NovaReMutexTLFHandle;

static void _nova_remutex_tlf_close_cb(uv_handle_t* h) { free(h->data); }

/* ── Plan 103.3: ReentrantMutex waiter ─────────────────────────────── */

typedef struct NovaReMutexWaiter {
    NovaFiberQueue*              scope;
    int                          slot;
    mco_coro*                    coro;       /* owner identity — for transfer on wake */
    struct NovaReMutexWaiter*    next;
    struct NovaReMutexWaiter*    prev;
    bool                         timed_out;
    NovaReMutexTLFHandle*        tlf_handle;
} NovaReMutexWaiter;

/* ── Plan 103.3: ReentrantMutex ─────────────────────────────────────── */

/* Reentrant mutex: same fiber can lock() multiple times without deadlock.
 * Owner identified by mco_running() — the current fiber's coroutine pointer,
 * or NULL for the main (non-fiber) thread.  This is stable across supervised{}
 * boundaries (unlike _nova_active_scope which changes per supervised block).
 *
 * Interaction with Condvar (Plan 103.4): wait() releases ENTIRE lock
 * (lock_count → 0), wake re-acquires with count=1.
 * AI-diagnostic W_REENTRANT_CONDVAR_RECOMMEND if mix detected.
 *
 * unlock() invariants checked unconditionally (Plan 103.3 pattern). */
typedef struct {
    nova_mutex_t          mu;
    bool                  locked;
    mco_coro*             owner_coro;  /* mco_running() of owner; NULL = main thread or unlocked */
    int32_t               lock_count;
    NovaReMutexWaiter*    head;
    NovaReMutexWaiter*    tail;
} Nova_ReentrantMutex;

/* Forward-declare timer cb. */
static void _nova_remutex_tlf_timer_cb(uv_timer_t* h);

static inline Nova_ReentrantMutex* Nova_ReentrantMutex_static_new(void) {
    Nova_ReentrantMutex* rm = (Nova_ReentrantMutex*)nova_alloc_uncollectable(sizeof(Nova_ReentrantMutex));
    nova_mutex_init(&rm->mu);
    rm->locked     = false;
    rm->owner_coro = NULL;
    rm->lock_count = 0;
    rm->head = rm->tail = NULL;
    return rm;
}

static inline nova_unit Nova_ReentrantMutex_method_lock(Nova_ReentrantMutex* rm) {
    nova_mutex_lock(&rm->mu);
    /* Reentrant: same fiber re-acquires without blocking. */
    if (rm->locked && rm->owner_coro == mco_running()) {
        rm->lock_count++;
        nova_mutex_unlock(&rm->mu);
        return NOVA_UNIT;
    }
    if (!rm->locked) {
        rm->locked     = true;
        rm->owner_coro = mco_running();
        rm->lock_count = 1;
        nova_mutex_unlock(&rm->mu);
        return NOVA_UNIT;
    }
    if (_nova_active_slot < 0) {
        /* Non-fiber (main thread) spin path. */
        nova_mutex_unlock(&rm->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&rm->mu);
            if (!rm->locked) {
                rm->locked     = true;
                rm->owner_coro = mco_running();
                rm->lock_count = 1;
                nova_mutex_unlock(&rm->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&rm->mu);
        }
    }
    NovaReMutexWaiter w = { .scope=_nova_active_scope, .slot=_nova_active_slot,
                             .coro=mco_running(),
                             .next=NULL, .prev=rm->tail,
                             .timed_out=false, .tlf_handle=NULL };
    if (rm->tail) rm->tail->next = &w;
    else          rm->head = &w;
    rm->tail = &w;
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &rm->mu);
    return NOVA_UNIT;
}

static inline nova_bool Nova_ReentrantMutex_method_try_lock(Nova_ReentrantMutex* rm) {
    nova_mutex_lock(&rm->mu);
    if (rm->locked && rm->owner_coro == mco_running()) {
        rm->lock_count++;
        nova_mutex_unlock(&rm->mu);
        return true;
    }
    if (!rm->locked) {
        rm->locked     = true;
        rm->owner_coro = mco_running();
        rm->lock_count = 1;
        nova_mutex_unlock(&rm->mu);
        return true;
    }
    nova_mutex_unlock(&rm->mu);
    return false;
}

static inline nova_bool Nova_ReentrantMutex_method_try_lock_for(
        Nova_ReentrantMutex* rm, void* timeout) {
    /* timeout is Nova_Duration* — void* avoids include-order dep. */
    int64_t tnanos = *(int64_t*)timeout;
    if (tnanos <= 0) return Nova_ReentrantMutex_method_try_lock(rm);
    nova_mutex_lock(&rm->mu);
    /* Reentrant fast-path. */
    if (rm->locked && rm->owner_coro == mco_running()) {
        rm->lock_count++;
        nova_mutex_unlock(&rm->mu);
        return true;
    }
    if (!rm->locked) {
        rm->locked     = true;
        rm->owner_coro = mco_running();
        rm->lock_count = 1;
        nova_mutex_unlock(&rm->mu);
        return true;
    }
    if (_nova_active_slot < 0) {
        /* Non-fiber (main thread) spin path with deadline. */
        nova_mutex_unlock(&rm->mu);
        int64_t deadline = _nova_monotonic_ns() + tnanos;
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&rm->mu);
            if (!rm->locked) {
                rm->locked     = true;
                rm->owner_coro = mco_running();
                rm->lock_count = 1;
                nova_mutex_unlock(&rm->mu);
                return true;
            }
            nova_mutex_unlock(&rm->mu);
            if (_nova_monotonic_ns() >= deadline) return false;
        }
    }
    uint64_t delay_ms = (uint64_t)((tnanos + 999999LL) / 1000000LL);
    if (delay_ms == 0) delay_ms = 1;
    NovaReMutexTLFHandle* handle = (NovaReMutexTLFHandle*)malloc(sizeof(NovaReMutexTLFHandle));
    if (!handle) { nova_mutex_unlock(&rm->mu); return false; }
    handle->relock     = (void*)rm;
    handle->timer.data = handle;
    NovaReMutexWaiter w = { .scope=_nova_active_scope, .slot=_nova_active_slot,
                             .coro=mco_running(),
                             .next=NULL, .prev=rm->tail,
                             .timed_out=false, .tlf_handle=handle };
    handle->waiter = &w;
    if (rm->tail) rm->tail->next = &w;
    else          rm->head = &w;
    rm->tail = &w;
    int rc = uv_timer_init(nova_current_loop(), &handle->timer);
    if (rc != 0) {
        if (w.prev) w.prev->next = NULL; else rm->head = NULL;
        rm->tail = w.prev;
        nova_mutex_unlock(&rm->mu);
        free(handle);
        return false;
    }
    rc = uv_timer_start(&handle->timer, _nova_remutex_tlf_timer_cb, delay_ms, 0);
    if (rc != 0) {
        if (w.prev) w.prev->next = NULL; else rm->head = NULL;
        rm->tail = w.prev;
        nova_mutex_unlock(&rm->mu);
        uv_close((uv_handle_t*)&handle->timer, _nova_remutex_tlf_close_cb);
        return false;
    }
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &rm->mu);
    return !w.timed_out;
}

static inline nova_unit Nova_ReentrantMutex_method_unlock(Nova_ReentrantMutex* rm) {
    nova_mutex_lock(&rm->mu);
    /* Unconditional invariant checks (Plan 103.3 pattern). */
    if (!rm->locked || rm->owner_coro != mco_running()) {
        nova_mutex_unlock(&rm->mu);
        Nova_Fail_fail(nova_str_from_cstr("ReentrantMutex.unlock() called by non-owner fiber or mutex not locked"));
        nova_throw(nova_str_from_cstr("ReentrantMutex.unlock() called by non-owner fiber or mutex not locked"));
    }
    rm->lock_count--;
    if (rm->lock_count > 0) {
        nova_mutex_unlock(&rm->mu);
        return NOVA_UNIT;
    }
    /* lock_count == 0: release ownership. */
    rm->locked     = false;
    rm->owner_coro = NULL;
    if (rm->head) {
        NovaReMutexWaiter* w = rm->head;
        rm->head = w->next;
        if (rm->head) rm->head->prev = NULL;
        else          rm->tail = NULL;
        /* Skip timed-out waiters. */
        while (w && w->tlf_handle && w->tlf_handle->waiter == NULL) {
            w = rm->head;
            if (w) { rm->head = w->next; if (rm->head) rm->head->prev = NULL; else rm->tail = NULL; }
        }
        if (w) {
            rm->locked     = true;
            rm->owner_coro = w->coro;   /* transfer ownership to waiter's fiber */
            rm->lock_count = 1;
            if (w->tlf_handle) w->tlf_handle->waiter = NULL;
            NovaFiberQueue* s = w->scope;
            int slot = w->slot;
            nova_mutex_unlock(&rm->mu);
            nova_sched_wake(s, slot);
            return NOVA_UNIT;
        }
    }
    nova_mutex_unlock(&rm->mu);
    return NOVA_UNIT;
}

/* Plan 103.3: lock_count() — depth for current fiber, 0 if not owner. */
static inline nova_int Nova_ReentrantMutex_method_lock_count(const Nova_ReentrantMutex* rm) {
    /* Read under mu for consistency (caller may use for debugging). */
    Nova_ReentrantMutex* mrm = (Nova_ReentrantMutex*)rm; /* cast away const for mu */
    nova_mutex_lock(&mrm->mu);
    nova_int count = 0;
    if (rm->locked && rm->owner_coro == mco_running()) {
        count = (nova_int)rm->lock_count;
    }
    nova_mutex_unlock(&mrm->mu);
    return count;
}

/* ReentrantMutex TLF timer callback. */
static void _nova_remutex_tlf_timer_cb(uv_timer_t* h) {
    NovaReMutexTLFHandle* handle = (NovaReMutexTLFHandle*)h->data;
    Nova_ReentrantMutex* rm = (Nova_ReentrantMutex*)handle->relock;
    nova_mutex_lock(&rm->mu);
    NovaReMutexWaiter* w = (NovaReMutexWaiter*)handle->waiter;
    if (w != NULL) {
        if (w->prev) w->prev->next = w->next; else rm->head = w->next;
        if (w->next) w->next->prev = w->prev; else rm->tail = w->prev;
        w->timed_out   = true;
        handle->waiter = NULL;
        NovaFiberQueue* s = w->scope;
        int slot = w->slot;
        nova_mutex_unlock(&rm->mu);
        nova_sched_wake(s, slot);
    } else {
        nova_mutex_unlock(&rm->mu);
    }
    uv_close((uv_handle_t*)h, _nova_remutex_tlf_close_cb);
}

/* ── WaitGroup waiter ──────────────────────────────────────────── */

typedef struct NovaWGWaiter {
    NovaFiberQueue*      scope;
    int                  slot;
    struct NovaWGWaiter* next;
    struct NovaWGWaiter* prev;
} NovaWGWaiter;

/* ── WaitGroup ─────────────────────────────────────────────────── */

/* Counter-based barrier. add(n) before spawning n workers; each worker
 * calls done() when finished; wait() parks until count reaches zero.
 *
 * Multiple callers may wait() concurrently — all are woken when done()
 * drives count to zero (WakeAll semantics).
 *
 * add() after wait() has started is undefined (same behavior as Go's
 * sync.WaitGroup — add must complete-happens-before any wait). */
typedef struct {
    nova_mutex_t    mu;    /* guards count + waiter list */
    int             count;
    NovaWGWaiter*   head;
    NovaWGWaiter*   tail;
} Nova_WaitGroup;

static inline Nova_WaitGroup* Nova_WaitGroup_static_new(void) {
    Nova_WaitGroup* wg = (Nova_WaitGroup*)nova_alloc(sizeof(Nova_WaitGroup));
    nova_mutex_init(&wg->mu);
    wg->count = 0;
    wg->head  = NULL;
    wg->tail  = NULL;
    return wg;
}

static inline nova_unit Nova_WaitGroup_method_add(Nova_WaitGroup* wg, nova_int delta) {
    nova_mutex_lock(&wg->mu);
    wg->count += (int)delta;
    nova_mutex_unlock(&wg->mu);
    return NOVA_UNIT;
}

static inline nova_unit Nova_WaitGroup_method_done(Nova_WaitGroup* wg) {
    nova_mutex_lock(&wg->mu);
    NOVA_SYNC_ASSERT(wg->count > 0,
                     "WaitGroup.done() called more times than add() — counter underflow");
    wg->count -= 1;
    if (wg->count == 0) {
        /* Detach the whole waiter list under lock, then wake outside.
         * Waking under the lock would cause the woken fiber to immediately
         * contend for the lock again — releasing first is more efficient. */
        NovaWGWaiter* w = wg->head;
        wg->head = NULL;
        wg->tail = NULL;
        nova_mutex_unlock(&wg->mu);
        while (w) {
            NovaWGWaiter* next = w->next;
            nova_sched_wake(w->scope, w->slot);
            w = next;
        }
    } else {
        nova_mutex_unlock(&wg->mu);
    }
    return NOVA_UNIT;
}

static inline nova_unit Nova_WaitGroup_method_wait(Nova_WaitGroup* wg) {
    nova_mutex_lock(&wg->mu);
    if (wg->count <= 0) {
        nova_mutex_unlock(&wg->mu);
        return NOVA_UNIT;
    }
    if (_nova_active_slot < 0) {
        /* Non-fiber: spin with CPU yield. */
        nova_mutex_unlock(&wg->mu);
        for (;;) {
            _nova_cpu_yield();
            nova_mutex_lock(&wg->mu);
            if (wg->count <= 0) {
                nova_mutex_unlock(&wg->mu);
                return NOVA_UNIT;
            }
            nova_mutex_unlock(&wg->mu);
        }
    }
    /* Fiber path: register as waiter and park atomically with unlock. */
    NovaWGWaiter w;
    w.scope = _nova_active_scope;
    w.slot  = _nova_active_slot;
    w.next  = NULL;
    w.prev  = wg->tail;
    if (wg->tail) wg->tail->next = &w;
    else          wg->head = &w;
    wg->tail = &w;
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &wg->mu);
    return NOVA_UNIT;
}

/* ── Once waiter ───────────────────────────────────────────────── */

typedef struct NovaOnceWaiter {
    NovaFiberQueue*        scope;
    int                    slot;
    struct NovaOnceWaiter* next;  /* singly-linked: LIFO order, but once is
                                   * always just one burst of wakeups */
} NovaOnceWaiter;

/* ── Once ──────────────────────────────────────────────────────── */

#define NOVA_ONCE_NEW      0   /* not yet started */
#define NOVA_ONCE_RUNNING  1   /* one fiber is executing the once-body */
#define NOVA_ONCE_DONE     2   /* body complete, state is permanent */
#define NOVA_ONCE_POISONED 3   /* call_once body panicked — subsequent calls re-panic */

/* Once guarantees that a body executes exactly once even under concurrency.
 *
 * Usage pattern:
 *
 *   if once.run() {
 *       // executed by exactly one fiber
 *       expensive_init()
 *       once.done()   // MUST call — releases waiting fibers
 *   }
 *   // all fibers reach here after init is complete
 *
 * run() returns true for the first caller (which must call done()).
 * Concurrent callers that arrive while state=RUNNING park until done() fires.
 * All callers that arrive after state=DONE return false immediately.
 *
 * CONTRACT: if run() returns true, the caller MUST call done() exactly once.
 * Failing to call done() leaves all waiting fibers permanently parked. */
typedef struct {
    nova_mutex_t     mu;
    int              state;    /* NOVA_ONCE_* constants */
    NovaOnceWaiter*  waiters;
} Nova_Once;

static inline Nova_Once* Nova_Once_static_new(void) {
    Nova_Once* o = (Nova_Once*)nova_alloc(sizeof(Nova_Once));
    nova_mutex_init(&o->mu);
    o->state   = NOVA_ONCE_NEW;
    o->waiters = NULL;
    return o;
}

/* run(): transitions state NEW→RUNNING for the first caller (returns true).
 * Subsequent callers park (fiber) or spin (OS thread) until DONE, then
 * return false. Callers arriving after DONE return false immediately. */
static inline nova_bool Nova_Once_method_run(Nova_Once* o) {
    /* Fast path: acquire-load without mutex. Safe because DONE is terminal
     * and the release-store in done() synchronizes with this acquire-load. */
    if (__atomic_load_n(&o->state, __ATOMIC_ACQUIRE) == NOVA_ONCE_DONE)
        return false;

    nova_mutex_lock(&o->mu);

    if (o->state == NOVA_ONCE_DONE) {
        nova_mutex_unlock(&o->mu);
        return false;
    }
    if (o->state == NOVA_ONCE_NEW) {
        o->state = NOVA_ONCE_RUNNING;
        nova_mutex_unlock(&o->mu);
        return true;   /* this fiber is the runner */
    }

    /* state == RUNNING: another fiber is executing the once-body. */
    if (_nova_active_slot < 0) {
        /* Non-fiber: spin with CPU yield until DONE. */
        nova_mutex_unlock(&o->mu);
        for (;;) {
            _nova_cpu_yield();
            if (__atomic_load_n(&o->state, __ATOMIC_ACQUIRE) == NOVA_ONCE_DONE)
                return false;
        }
    }

    /* Fiber: park until done() sets state=DONE and wakes us. */
    NovaOnceWaiter w;
    w.scope    = _nova_active_scope;
    w.slot     = _nova_active_slot;
    w.next     = o->waiters;
    o->waiters = &w;
    nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                 (void(*)(void*))nova_mutex_unlock, &o->mu);
    /* Woken by done() — state is NOVA_ONCE_DONE. */
    return false;
}

/* done(): marks the once-body as complete. Wakes all parked waiters.
 * Must be called exactly once, by the fiber that received true from run(). */
static inline nova_unit Nova_Once_method_done(Nova_Once* o) {
    nova_mutex_lock(&o->mu);
    /* Plan 103.5: always check (not just in NOVA_DEBUG) — calling done() on a
     * Fresh/Done/Poisoned Once is always a programming error that must surface
     * as a Nova runtime panic (nova_throw path), not a silent no-op.
     * NOVA_SYNC_ASSERT would be a no-op in Dev/Release builds.
     * Note: cannot use NOVA_ONCE_REPANIC here — that macro is defined later in
     * this file (before call_once). nova_throw / Nova_Fail_fail come from
     * effects.h which is included before sync_primitives.h in nova_rt.h. */
    if (o->state != NOVA_ONCE_RUNNING) {
        nova_mutex_unlock(&o->mu);
        Nova_Fail_fail(nova_str_from_cstr("Once.done() called without a matching run() returning true"));
        nova_throw(nova_str_from_cstr("Once.done() called without a matching run() returning true"));
    }
    NOVA_SYNC_ASSERT(o->state == NOVA_ONCE_RUNNING,
                     "Once.done() called without a matching run() returning true");
    /* Release-store: makes the body's side-effects visible to all callers
     * that observe DONE via the acquire-load fast path in run(). */
    __atomic_store_n(&o->state, NOVA_ONCE_DONE, __ATOMIC_RELEASE);
    NovaOnceWaiter* w = o->waiters;
    o->waiters = NULL;
    nova_mutex_unlock(&o->mu);
    while (w) {
        NovaOnceWaiter* next = w->next;
        nova_sched_wake(w->scope, w->slot);
        w = next;
    }
    return NOVA_UNIT;
}

/* ── OnceState (Plan 103.5) ────────────────────────────────────────────
 *
 * Pre-declared here so Nova_Once_method_state can reference Nova_OnceState*
 * before the generated code defines it. Tag values must match the variant
 * ORDER declared in std/runtime/sync.nv (OnceState type):
 *   Fresh=0  Running=1  Done=2  Poisoned=3
 * This is coordinated with emit_c.rs (RUNTIME_DEFINED_TYPES "OnceState")
 * and documented in D171.
 */
typedef enum {
    NOVA_TAG_OnceState_Fresh    = 0,
    NOVA_TAG_OnceState_Running  = 1,
    NOVA_TAG_OnceState_Done     = 2,
    NOVA_TAG_OnceState_Poisoned = 3,
} Nova_OnceState_Tag;

typedef struct Nova_OnceState Nova_OnceState;
struct Nova_OnceState {
    Nova_OnceState_Tag tag;
    union { char _dummy; } payload;   /* unit-only variants — MSVC requires ≥1 member */
};

/* Constructors — normally emitted by emit_sum_type; here because OnceState
 * is in RUNTIME_DEFINED_TYPES (emit_sum_type is skipped). */
static inline Nova_OnceState* nova_make_OnceState_Fresh(void) {
    Nova_OnceState* _r = (Nova_OnceState*)nova_alloc(sizeof(Nova_OnceState));
    _r->tag = NOVA_TAG_OnceState_Fresh; return _r;
}
static inline Nova_OnceState* nova_make_OnceState_Running(void) {
    Nova_OnceState* _r = (Nova_OnceState*)nova_alloc(sizeof(Nova_OnceState));
    _r->tag = NOVA_TAG_OnceState_Running; return _r;
}
static inline Nova_OnceState* nova_make_OnceState_Done(void) {
    Nova_OnceState* _r = (Nova_OnceState*)nova_alloc(sizeof(Nova_OnceState));
    _r->tag = NOVA_TAG_OnceState_Done; return _r;
}
static inline Nova_OnceState* nova_make_OnceState_Poisoned(void) {
    Nova_OnceState* _r = (Nova_OnceState*)nova_alloc(sizeof(Nova_OnceState));
    _r->tag = NOVA_TAG_OnceState_Poisoned; return _r;
}

/* call_once(): panic-safe primary API (Plan 103.5, D171).
 *
 * Runs `body` exactly once. `body` is a no-arg closure: fn() -> ()
 * whose C layout is { void* fn; void* env } (NovaClosBase).
 *
 * Panic-safety contract:
 *   - If body panics: state → POISONED (permanent).
 *   - All waiting fibers are woken; they also re-panic.
 *   - Subsequent call_once() on a poisoned Once always re-panics.
 *
 * Concurrent callers while RUNNING: park (fiber) / spin (non-fiber)
 * until the runner finishes, then return normally (DONE) or re-panic (POISONED).
 */
/* Plan 103.5 helper: throw a poison re-panic through the effect system
 * (Nova_Fail_fail), then fall through to nova_throw as raw fallback.
 * Used wherever Once/OnceCell/Lazy re-panics on poisoned state. */
#define NOVA_ONCE_REPANIC(msg) \
    do { \
        Nova_Fail_fail(nova_str_from_cstr(msg)); \
        nova_throw(nova_str_from_cstr(msg));  /* unreachable if Nova_Fail_fail throws */ \
    } while(0)

static inline nova_unit Nova_Once_method_call_once(Nova_Once* o, NovaClosBase* body) {
    /* Fast path A: already done — no-op. */
    int _st = __atomic_load_n(&o->state, __ATOMIC_ACQUIRE);
    if (_st == NOVA_ONCE_DONE) return NOVA_UNIT;
    /* Fast path B: poisoned — re-panic through effect system. */
    if (_st == NOVA_ONCE_POISONED)
        NOVA_ONCE_REPANIC("Once: poisoned by a previous call_once panic");

    nova_mutex_lock(&o->mu);

    if (o->state == NOVA_ONCE_DONE) {
        nova_mutex_unlock(&o->mu);
        return NOVA_UNIT;
    }
    if (o->state == NOVA_ONCE_POISONED) {
        nova_mutex_unlock(&o->mu);
        NOVA_ONCE_REPANIC("Once: poisoned by a previous call_once panic");
    }
    if (o->state == NOVA_ONCE_RUNNING) {
        /* Another fiber is executing the body — wait. */
        if (_nova_active_slot < 0) {
            /* Non-fiber: spin until DONE or POISONED. */
            nova_mutex_unlock(&o->mu);
            for (;;) {
                _nova_cpu_yield();
                _st = __atomic_load_n(&o->state, __ATOMIC_ACQUIRE);
                if (_st == NOVA_ONCE_DONE) return NOVA_UNIT;
                if (_st == NOVA_ONCE_POISONED)
                    NOVA_ONCE_REPANIC("Once: poisoned by a previous call_once panic");
            }
        }
        /* Fiber: park until done() / call_once sets terminal state. */
        NovaOnceWaiter _oc_w;
        _oc_w.scope    = _nova_active_scope;
        _oc_w.slot     = _nova_active_slot;
        _oc_w.next     = o->waiters;
        o->waiters     = &_oc_w;
        nova_sched_park_with_unlock(_nova_active_scope, _nova_active_slot,
                                     (void(*)(void*))nova_mutex_unlock, &o->mu);
        /* Woken by runner — check resulting state. */
        _st = __atomic_load_n(&o->state, __ATOMIC_ACQUIRE);
        if (_st == NOVA_ONCE_DONE) return NOVA_UNIT;
        NOVA_ONCE_REPANIC("Once: poisoned by a previous call_once panic");
    }

    /* state == NEW: we become the runner. */
    o->state = NOVA_ONCE_RUNNING;
    nova_mutex_unlock(&o->mu);

    /* Run body with panic capture.
     * Plan 103.5: temporarily clear _nova_handler_Fail so that `throw` inside
     * the body goes through nova_throw → NOVA_TRY, not through a user-installed
     * `with Fail { interrupt () }` handler that would bypass NOVA_TRY and leave
     * Once stuck in RUNNING state. We re-throw via Nova_Fail_fail after state
     * is finalized so user handlers see the panic. */
    NovaVtable_Fail* _oc_saved_fail = _nova_handler_Fail;
    _nova_handler_Fail = NULL;

    NovaFailFrame _oc_frame;
    nova_bool     _oc_panicked = false;
    nova_str      _oc_msg;
    if (NOVA_TRY(_oc_frame)) {
        ((nova_unit(*)(void*))body->fn)(body->env);
        nova_fail_pop(); /* success: pop our TRY frame */
    } else {
        _oc_panicked = true;
        _oc_msg = NOVA_CATCH(_oc_frame); /* catch: pops frame + returns msg */
    }

    /* Restore user handler before finalizing + re-throw. */
    _nova_handler_Fail = _oc_saved_fail;

    /* Finalize state and wake all waiters. */
    nova_mutex_lock(&o->mu);
    __atomic_store_n(&o->state,
                     _oc_panicked ? NOVA_ONCE_POISONED : NOVA_ONCE_DONE,
                     __ATOMIC_RELEASE);
    NovaOnceWaiter* _oc_waiters = o->waiters;
    o->waiters = NULL;
    nova_mutex_unlock(&o->mu);
    while (_oc_waiters) {
        NovaOnceWaiter* _oc_next = _oc_waiters->next;
        nova_sched_wake(_oc_waiters->scope, _oc_waiters->slot);
        _oc_waiters = _oc_next;
    }

    /* Re-throw through user handler (Nova_Fail_fail), then nova_throw fallback. */
    if (_oc_panicked) { Nova_Fail_fail(_oc_msg); nova_throw(_oc_msg); }
    return NOVA_UNIT;
}

/* is_completed(): returns true iff state == DONE (body ran successfully).
 * Returns false for Fresh, Running, and Poisoned states. */
static inline nova_bool Nova_Once_method_is_completed(Nova_Once* o) {
    return __atomic_load_n(&o->state, __ATOMIC_ACQUIRE) == NOVA_ONCE_DONE;
}

/* state(): returns heap-allocated OnceState reflecting current state.
 * Mapping: Fresh=0, Running=1, Done=2, Poisoned=3. */
static inline Nova_OnceState* Nova_Once_method_state(Nova_Once* o) {
    int _st = __atomic_load_n(&o->state, __ATOMIC_ACQUIRE);
    Nova_OnceState* _r = (Nova_OnceState*)nova_alloc(sizeof(Nova_OnceState));
    _r->tag = (Nova_OnceState_Tag)_st;
    return _r;
}

/* nova_fn_fence — implements `export external fn fence(ord MemOrdering)`.
 *
 * C name derived by ExternalRegistry: free function → nova_fn_fence.
 * Parameter type MemOrdering maps to Nova_MemOrdering* (heap-pointer ABI).
 *
 * Semantics (D167):
 *  Relaxed — no-op (fence is valid syntactically; has no ordering effect)
 *  Acquire — all subsequent reads/writes happen-after all prior Release stores
 *  Release — all prior reads/writes happen-before all subsequent Acquire loads
 *  AcqRel  — combination Acquire + Release
 *  SeqCst  — total order participation; sequenced relative to all SeqCst ops
 */
static inline nova_unit nova_fn_fence(Nova_MemOrdering* ord) {
    switch (ord->tag) {
        case NOVA_TAG_MemOrdering_Relaxed: /* no-op: valid syntactically */ break;
        case NOVA_TAG_MemOrdering_Acquire: nova_thread_fence_acquire(); break;
        case NOVA_TAG_MemOrdering_Release: nova_thread_fence_release(); break;
        case NOVA_TAG_MemOrdering_AcqRel:  nova_thread_fence_acq_rel(); break;
        case NOVA_TAG_MemOrdering_SeqCst:  nova_thread_fence_seq_cst(); break;
    }
    return NOVA_UNIT;
}

/* === PLAN-103.4 PARALLEL INCLUDES (alphabetical, uncomment in own branch) === */
/* AGENT-B */  /* #include "sync_barrier.h"          */
/* AGENT-D */  /* #include "sync_condvar.h"          */
/* AGENT-C */  /* #include "sync_countdown_latch.h"  */
/* AGENT-A */  /* #include "sync_semaphore.h"        */
/* === END PLAN-103.4 PARALLEL INCLUDES === */

#endif /* NOVA_RT_SYNC_PRIMITIVES_H */
