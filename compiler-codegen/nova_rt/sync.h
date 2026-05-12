// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_SYNC_H
#define NOVA_RT_SYNC_H

/* Plan 40 Ф.1 (2026-05-12): thread-safety primitives для channel runtime
 * (и потенциально других shared structures, когда они появятся).
 *
 * Backend: C11 `<threads.h>` + `<stdatomic.h>`. Cross-platform:
 *   - clang/gcc: давняя поддержка.
 *   - MSVC: VS2019 16.8+ для <threads.h>, <stdatomic.h> с _Atomic.
 *
 * Под single-thread runtime'ом (Plan 23 не активен) mutex acquire/release
 * = два разыменования + branch (uncontended), ~5-10 ns на operation.
 * Атомики = sequenced (no fence cost) на x86_64. Полная стоимость
 * thread-safety ≈ 2-3% на send/recv path. Acceptable.
 *
 * Под M:N (Plan 23) — реальная mutex contention + atomic CAS-loop'ы.
 * Performance под нагрузкой — отдельный benchmark target в Plan 23.
 */

#include <threads.h>
#include <stdatomic.h>

/* ── Mutex ─────────────────────────────────────────────────────── */

typedef mtx_t nova_mutex_t;

static inline void nova_mutex_init(nova_mutex_t* m) {
    /* mtx_plain — без recursion и timeout (нам не нужно). */
    mtx_init(m, mtx_plain);
}

static inline void nova_mutex_lock(nova_mutex_t* m)   { mtx_lock(m); }
static inline void nova_mutex_unlock(nova_mutex_t* m) { mtx_unlock(m); }
static inline void nova_mutex_destroy(nova_mutex_t* m) { mtx_destroy(m); }

/* ── Atomic primitives ──────────────────────────────────────────
 *
 * Wrappers с explicit memory_order для совместимости с future Boehm
 * GC scanner alignment requirements.  Acquire/release semantics —
 * безопасный default для shared state.
 */

typedef atomic_int       nova_atomic_int;
typedef atomic_bool      nova_atomic_bool;
typedef atomic_intptr_t  nova_atomic_intptr;

/* int operations */
#define nova_aint_init(p, v)   atomic_init((p), (v))
#define nova_aint_load(p)      atomic_load_explicit((p), memory_order_acquire)
#define nova_aint_store(p, v)  atomic_store_explicit((p), (v), memory_order_release)
#define nova_aint_inc(p)       atomic_fetch_add_explicit((p), 1, memory_order_acq_rel)
#define nova_aint_dec(p)       atomic_fetch_sub_explicit((p), 1, memory_order_acq_rel)
/* CAS: returns 1 on success (desired stored), 0 on failure (expected updated to current). */
#define nova_aint_cas(p, expected, desired) \
    atomic_compare_exchange_strong_explicit((p), (expected), (desired), \
                                             memory_order_acq_rel, memory_order_acquire)

/* bool operations */
#define nova_abool_init(p, v)  atomic_init((p), (v))
#define nova_abool_load(p)     atomic_load_explicit((p), memory_order_acquire)
#define nova_abool_store(p, v) atomic_store_explicit((p), (v), memory_order_release)

#endif /* NOVA_RT_SYNC_H */
