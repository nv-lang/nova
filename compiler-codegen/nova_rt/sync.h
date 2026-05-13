// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_SYNC_H
#define NOVA_RT_SYNC_H

/* Plan 44.1 Ф.1 (2026-05-12): thread-safety primitives для channel runtime.
 *
 * Tier 1 supported toolchains (Plan 44.1 R3-2):
 *   - Linux x86_64/aarch64 + clang LLVM 15+ (glibc 2.35+)
 *   - Windows x86_64 + clang LLVM 15+
 *   - macOS arm64 + Apple Clang
 *
 * Backend selector:
 *   - Mutex: SRWLOCK (Windows) / os_unfair_lock (macOS) / pthread + adaptive
 *     (Linux). C11 <threads.h> избегаем потому что: VS2019 не имеет
 *     <stdatomic.h>, mingw-w64 не имеет <threads.h>, Apple Clang не имеет
 *     <threads.h> в libc.
 *   - Atomics: __atomic_* GCC/Clang builtins. Identical API на clang/gcc/
 *     Apple Clang. Portable.
 *
 * Под single-thread runtime'ом (Plan 23 не активен): mutex uncontended
 * ≈ atomic CAS, ~5-10 ns. Атомики на x86 = sequenced (no fence cost).
 *
 * Под M:N (Plan 23) — реальная contention + CAS-loop'ы.
 *
 * Memory ordering правила (Plan 44.1 R1 A1):
 *   - `closed`: Release-store / Acquire-load (paired flag+payload).
 *   - `writer_count`: Release-dec + Acquire-fence-on-zero (refcount idiom
 *     как Arc::drop / shared_ptr).
 *   - `fired` CAS: acq_rel/acquire — mutex acquire даёт оставшийся ordering.
 *   - Weak CAS (loop, failure ничего не carry): nova_aint_cas_weak с
 *     release/relaxed-on-failure (Plan 44.1 R2 C7) — на ARM saves a barrier
 *     per failed CAS, на x86 same code.
 */

#include <stdint.h>
#include <stdbool.h>

/* ── Backend selector ──────────────────────────────────────────── */

#if defined(_WIN32)
  #define NOVA_SYNC_BACKEND_WINDOWS 1
  #define WIN32_LEAN_AND_MEAN
  #include <windows.h>
  #include <synchapi.h>
#elif defined(__APPLE__)
  #define NOVA_SYNC_BACKEND_DARWIN 1
  #include <os/lock.h>
#elif defined(__linux__)
  #define NOVA_SYNC_BACKEND_PTHREAD 1
  #include <pthread.h>
#else
  #error "Plan 44.1 Tier 1 unsupported platform"
#endif

/* ── Mutex ─────────────────────────────────────────────────────── */

#if defined(NOVA_SYNC_BACKEND_WINDOWS)

  typedef SRWLOCK nova_mutex_t;

  static inline void nova_mutex_init(nova_mutex_t* m) {
      InitializeSRWLock(m);
  }
  static inline void nova_mutex_lock(nova_mutex_t* m)   { AcquireSRWLockExclusive(m); }
  static inline void nova_mutex_unlock(nova_mutex_t* m) { ReleaseSRWLockExclusive(m); }
  static inline void nova_mutex_destroy(nova_mutex_t* m) { (void)m; /* SRWLOCK не требует destroy */ }

#elif defined(NOVA_SYNC_BACKEND_DARWIN)

  /* macOS: os_unfair_lock — ~50 ns vs ~20 µs для pthread_mutex_t (R3-5).
   * Plan 44.1 acceptance <50 ns round-trip достижимо только с unfair_lock. */
  typedef os_unfair_lock nova_mutex_t;

  static inline void nova_mutex_init(nova_mutex_t* m) {
      *m = OS_UNFAIR_LOCK_INIT;
  }
  static inline void nova_mutex_lock(nova_mutex_t* m)   { os_unfair_lock_lock(m); }
  static inline void nova_mutex_unlock(nova_mutex_t* m) { os_unfair_lock_unlock(m); }
  static inline void nova_mutex_destroy(nova_mutex_t* m) { (void)m; }

#elif defined(NOVA_SYNC_BACKEND_PTHREAD)

  typedef pthread_mutex_t nova_mutex_t;

  static inline void nova_mutex_init(nova_mutex_t* m) {
      /* Plan 44.1 R3-4: PTHREAD_MUTEX_ADAPTIVE_NP для short critical sections
       * (glibc benchmark: ~55% throughput gain vs NORMAL). */
      #if defined(__GLIBC__) && defined(PTHREAD_MUTEX_ADAPTIVE_NP)
          pthread_mutexattr_t attr;
          pthread_mutexattr_init(&attr);
          pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_ADAPTIVE_NP);
          pthread_mutex_init(m, &attr);
          pthread_mutexattr_destroy(&attr);
      #else
          pthread_mutex_init(m, NULL);  /* PTHREAD_MUTEX_NORMAL */
      #endif
  }
  static inline void nova_mutex_lock(nova_mutex_t* m)    { pthread_mutex_lock(m); }
  static inline void nova_mutex_unlock(nova_mutex_t* m)  { pthread_mutex_unlock(m); }
  static inline void nova_mutex_destroy(nova_mutex_t* m) { pthread_mutex_destroy(m); }

#endif

/* ── Atomic primitives via __atomic_* builtins ──────────────────
 *
 * Portable across clang/gcc/Apple Clang. Identical API.
 * Memory orders mapped to __ATOMIC_* constants:
 *   acquire  → __ATOMIC_ACQUIRE
 *   release  → __ATOMIC_RELEASE
 *   acq_rel  → __ATOMIC_ACQ_REL
 *   relaxed  → __ATOMIC_RELAXED
 *   seq_cst  → __ATOMIC_SEQ_CST
 */

typedef int32_t  nova_atomic_int;
typedef bool     nova_atomic_bool;
typedef intptr_t nova_atomic_intptr;

/* int operations */

static inline void nova_aint_init(volatile nova_atomic_int* p, int32_t v) {
    __atomic_store_n(p, v, __ATOMIC_RELAXED);
}

static inline int32_t nova_aint_load(const volatile nova_atomic_int* p) {
    return __atomic_load_n(p, __ATOMIC_ACQUIRE);
}

static inline void nova_aint_store(volatile nova_atomic_int* p, int32_t v) {
    __atomic_store_n(p, v, __ATOMIC_RELEASE);
}

static inline int32_t nova_aint_inc(volatile nova_atomic_int* p) {
    return __atomic_fetch_add(p, 1, __ATOMIC_ACQ_REL);
}

/* R1 A1: Release-decrement (NOT acq_rel) — refcount idiom.
 * Only the thread that drove count to zero MUST issue
 * atomic_thread_fence(Acquire) before reading owned data. */
static inline int32_t nova_aint_fetch_sub_release(volatile nova_atomic_int* p) {
    return __atomic_fetch_sub(p, 1, __ATOMIC_RELEASE);
}

/* Convenience: acquire fence для refcount-zero path. */
static inline void nova_thread_fence_acquire(void) {
    __atomic_thread_fence(__ATOMIC_ACQUIRE);
}

/* Strong CAS: returns true on success (desired stored), false on failure
 * (expected обновлён до current). Use для one-shot CAS — например
 * SelectWaiter.fired transition 0→1 в wake helper iteration. */
static inline bool nova_aint_cas(volatile nova_atomic_int* p,
                                  int32_t* expected, int32_t desired) {
    return __atomic_compare_exchange_n(p, expected, desired,
                                         false,  /* strong */
                                         __ATOMIC_ACQ_REL,
                                         __ATOMIC_ACQUIRE);
}

/* Weak CAS с release/relaxed-on-failure (Plan 44.1 R2 C7).
 * Используется в loop где failure не carry data — на ARM saves DMB per
 * failed iteration. На x86 emit identical code. */
static inline bool nova_aint_cas_weak_release(volatile nova_atomic_int* p,
                                                int32_t* expected, int32_t desired) {
    return __atomic_compare_exchange_n(p, expected, desired,
                                         true,  /* weak */
                                         __ATOMIC_RELEASE,
                                         __ATOMIC_RELAXED);
}

/* bool operations */

static inline void nova_abool_init(volatile nova_atomic_bool* p, bool v) {
    __atomic_store_n(p, v, __ATOMIC_RELAXED);
}

static inline bool nova_abool_load(const volatile nova_atomic_bool* p) {
    return __atomic_load_n(p, __ATOMIC_ACQUIRE);
}

static inline void nova_abool_store(volatile nova_atomic_bool* p, bool v) {
    __atomic_store_n(p, v, __ATOMIC_RELEASE);
}

/* pointer operations (Plan 44.5 Layer 5) — для cross-worker first_error
 * propagation через атомарный first-writer-wins CAS. */

typedef const void* nova_atomic_ptr;

static inline void nova_aptr_init(volatile nova_atomic_ptr* p, const void* v) {
    __atomic_store_n((const void**)p, v, __ATOMIC_RELAXED);
}

static inline const void* nova_aptr_load(const volatile nova_atomic_ptr* p) {
    return __atomic_load_n((const void**)p, __ATOMIC_ACQUIRE);
}

/* Strong CAS на pointer: returns true on success, false on failure
 * (expected обновляется текущим значением). Acq_rel на успехе, acquire
 * на failure — стандартный pattern для one-shot first-writer-wins. */
static inline bool nova_aptr_cas(volatile nova_atomic_ptr* p,
                                  const void** expected,
                                  const void* desired) {
    return __atomic_compare_exchange_n((const void**)p, expected, desired,
                                         false,  /* strong */
                                         __ATOMIC_ACQ_REL,
                                         __ATOMIC_ACQUIRE);
}

/* ── Cache-line size (Plan 44.1 R2 C5) ───────────────────────────── */

/* x86_64: 64 bytes. ARM big cores: 128 bytes. Default to 64 для bootstrap;
 * Plan 23 может tune через runtime detection. */
#if defined(__aarch64__) || defined(__arm64__)
  #define NOVA_CACHELINE_SIZE 128
#else
  #define NOVA_CACHELINE_SIZE 64
#endif

#endif /* NOVA_RT_SYNC_H */
