// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 44 (M:N Этап 0, 2026-05-13) — multi-thread runtime impl.
 *
 * Minimal proof of concept:
 *   - N worker OS threads (uv_thread_create).
 *   - Each worker: own libuv loop, own scope, mutex-protected push queue.
 *   - Spawn round-robin (Chase-Lev deque — Этап 1).
 *   - Cross-worker wake via uv_async_send.
 *
 * Не использовать без явного nova_runtime_init() вызова — bootstrap
 * default остаётся single-thread.
 */

/* Include umbrella для правильного ordering (fibers.h → nova_sched.h → ...). */
#include "nova_rt.h"
#include "runtime.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef NOVA_USE_LIBUV
#  error "Plan 44 requires NOVA_USE_LIBUV — libuv mandatory for M:N"
#endif

#include <uv.h>

/* Plan 44.5 Layer 4: Boehm GC_THREADS register per worker.
 * Linux/macOS: libgc built with GC_THREADS (Ubuntu apt default).
 * Windows: vcpkg bdwgc[multithreaded] feature flag — может отсутствовать.
 * Conditional через NOVA_GC_THREADS_REGISTER (set by build на supported
 * platforms). Default — off, safe fallback. */
#if defined(NOVA_GC_BOEHM) && (defined(__linux__) || defined(__APPLE__))
#  define NOVA_GC_THREADS_REGISTER 1
#  include <gc.h>
#endif

/* ── Worker struct ─────────────────────────────────────────────── */

struct NovaWorker {
    int               id;
    uv_thread_t       thread;
    uv_loop_t         loop;
    uv_async_t        wake_handle;
    /* Plan 44.5 Layer 2: Chase-Lev deque вместо mutex+scope push.
     * Lock-free owner ops, lock-free CAS steals. */
    NovaDeque         deque;
    /* scope остаётся для cancellation propagation и fiber bookkeeping —
     * но fiber dispatch идёт через deque. */
    NovaFiberQueue    scope;
    nova_atomic_bool  stop;
    nova_atomic_int   pending_count;
};

/* ── Runtime state ─────────────────────────────────────────────── */

static NovaWorker*     _workers = NULL;
static int             _n_workers = 0;
static nova_atomic_int _round_robin = 0;
static bool            _initialized = false;
static nova_mutex_t    _init_mu;
static bool            _init_mu_inited = false;

/* TLS: current worker id (для diagnostic). -1 = main thread. */
#ifdef _MSC_VER
static __declspec(thread) int _current_worker_id = -1;
#else
static __thread int _current_worker_id = -1;
#endif

/* ── Worker main ──────────────────────────────────────────────── */

/* uv_async callback — fires when cross-worker spawn pushes fiber.
 * Просто wakes uv_run; actual drain делается в worker loop. */
static void _worker_async_cb(uv_async_t* h) {
    (void)h;
    /* No-op — wake-up itself is the signal. */
}

static void _worker_main(void* arg) {
    NovaWorker* w = (NovaWorker*)arg;
    _current_worker_id = w->id;

    /* Plan 44.6 Layer 3: per-worker libuv loop visible через TLS.
     * Все timer/handle registrations в этом thread'е (Time.sleep,
     * channels Time.after) пойдут на &w->loop, не на main thread's
     * nova_evloop(). Без этого fiber park'ается на main loop'е, но
     * worker крутит свой uv_run — callback никогда не fire'нет на
     * worker'е, fiber hangs permanently. */
    _nova_current_loop = &w->loop;

    /* Plan 44.5 Layer 4: register thread с Boehm GC.
     * Required для workers что вызывают nova_alloc (channels, NovaSpawnCtx).
     * Linux/macOS: libgc built с GC_THREADS — register/unregister работают.
     * Windows: conditional skip (build flag NOVA_GC_THREADS_REGISTER).
     * Без register workers могут crash при nova_alloc (Boehm tries to
     * walk не-зарегистрированный thread stack). */
#ifdef NOVA_GC_THREADS_REGISTER
    struct GC_stack_base sb;
    if (GC_get_stack_base(&sb) == GC_SUCCESS) {
        GC_register_my_thread(&sb);
    }
#endif

    /* Per-worker TLS: _nova_active_scope указывает на own scope.
     * Объявлены в fibers.h cross-platform; здесь только set. */
    _nova_active_scope = &w->scope;
    _nova_active_slot  = -1;

    while (!nova_abool_load(&w->stop)) {
        mco_coro* co = NULL;

        /* (1) Local deque — owner LIFO pop. Wait-free hot path. */
        co = (mco_coro*)nova_deque_pop(&w->deque);

        /* (2) Idle — try steal у соседей (FIFO from their deque top). */
        if (!co) {
            for (int i = 0; i < _n_workers; i++) {
                if (i == w->id) continue;
                co = (mco_coro*)nova_deque_steal(&_workers[i].deque);
                if (co) break;
            }
        }

        /* (3) Still nothing — block в libuv (own loop) до cross-worker wake. */
        if (!co) {
            uv_run(&w->loop, UV_RUN_ONCE);
            continue;
        }

        /* (4) Run fiber. */
        if (mco_status(co) == MCO_SUSPENDED) {
            mco_resume(co);
        }
        if (mco_status(co) == MCO_DEAD) {
            mco_destroy(co);
        } else if (mco_status(co) == MCO_SUSPENDED) {
            /* Yielded — re-push в own deque. */
            nova_deque_push(&w->deque, co);
        }
    }

    /* Cleanup — drain remaining items в deque. */
    while (true) {
        mco_coro* co = (mco_coro*)nova_deque_pop(&w->deque);
        if (!co) break;
        if (mco_status(co) == MCO_SUSPENDED) {
            mco_resume(co);
        }
        if (mco_status(co) == MCO_DEAD) {
            mco_destroy(co);
        }
    }
    _nova_active_slot = -1;

#ifdef NOVA_GC_THREADS_REGISTER
    GC_unregister_my_thread();
#endif
}

/* ── Init / shutdown ──────────────────────────────────────────── */

void nova_runtime_init(int n_workers) {
    /* Idempotent guard. */
    if (!_init_mu_inited) {
        nova_mutex_init(&_init_mu);
        _init_mu_inited = true;
    }
    nova_mutex_lock(&_init_mu);
    if (_initialized) {
        nova_mutex_unlock(&_init_mu);
        return;
    }

    if (n_workers <= 0) {
        n_workers = (int)uv_available_parallelism();
        if (n_workers <= 0) n_workers = 1;
    }

#ifdef NOVA_GC_THREADS_REGISTER
    /* Boehm требует разрешения explicit thread registration ПЕРЕД
     * первым GC_register_my_thread. Idempotent — safe вызывать
     * многократно. Without this — register fails с "Threads explicit
     * registering is not previously enabled" error. */
    GC_allow_register_threads();
#endif

    _workers = (NovaWorker*)calloc((size_t)n_workers, sizeof(NovaWorker));
    if (!_workers) {
        fprintf(stderr, "nova: runtime_init OOM (%d workers)\n", n_workers);
        abort();
    }
    _n_workers = n_workers;
    nova_aint_init(&_round_robin, 0);

    for (int i = 0; i < n_workers; i++) {
        NovaWorker* w = &_workers[i];
        w->id = i;
        nova_abool_init(&w->stop, false);
        nova_aint_init(&w->pending_count, 0);
        nova_scope_init(&w->scope);
        /* Plan 44.5 Layer 2: per-worker Chase-Lev deque. */
        if (!nova_deque_init(&w->deque, 64)) {
            fprintf(stderr, "nova: deque_init failed\n");
            abort();
        }

        int rc = uv_loop_init(&w->loop);
        if (rc != 0) {
            fprintf(stderr, "nova: uv_loop_init failed: %s\n", uv_strerror(rc));
            abort();
        }
        rc = uv_async_init(&w->loop, &w->wake_handle, _worker_async_cb);
        if (rc != 0) {
            fprintf(stderr, "nova: uv_async_init failed: %s\n", uv_strerror(rc));
            abort();
        }
        w->wake_handle.data = w;

        rc = uv_thread_create(&w->thread, _worker_main, w);
        if (rc != 0) {
            fprintf(stderr, "nova: uv_thread_create failed: %s\n", uv_strerror(rc));
            abort();
        }
    }

    _initialized = true;
    nova_mutex_unlock(&_init_mu);
}

void nova_runtime_shutdown(void) {
    if (!_init_mu_inited) return;
    nova_mutex_lock(&_init_mu);
    if (!_initialized) {
        nova_mutex_unlock(&_init_mu);
        return;
    }

    /* Signal stop + wake workers. */
    for (int i = 0; i < _n_workers; i++) {
        NovaWorker* w = &_workers[i];
        nova_abool_store(&w->stop, true);
        uv_async_send(&w->wake_handle);
    }

    /* Join. */
    for (int i = 0; i < _n_workers; i++) {
        uv_thread_join(&_workers[i].thread);
    }

    /* Cleanup. */
    for (int i = 0; i < _n_workers; i++) {
        NovaWorker* w = &_workers[i];
        uv_close((uv_handle_t*)&w->wake_handle, NULL);
        /* Run one more tick to process close. */
        uv_run(&w->loop, UV_RUN_NOWAIT);
        uv_loop_close(&w->loop);
        nova_deque_destroy(&w->deque);
    }

    free(_workers);
    _workers = NULL;
    _n_workers = 0;
    _initialized = false;

    nova_mutex_unlock(&_init_mu);
}

/* ── Spawn ────────────────────────────────────────────────────── */

void nova_runtime_spawn_global(void (*entry)(mco_coro*), void* user) {
    if (!_initialized || _n_workers == 0) {
        /* Fallback: single-thread spawn в current scope. */
        if (_nova_active_scope) {
            nova_fiber_spawn_into(_nova_active_scope, entry, user);
        } else {
            fprintf(stderr, "nova: runtime_spawn_global: not initialized + no active scope\n");
            abort();
        }
        return;
    }

    int idx = (int)((uint32_t)nova_aint_inc(&_round_robin) % (uint32_t)_n_workers);
    NovaWorker* target = &_workers[idx];

    /* Plan 44.5 Layer 2: create mco_coro + push в target's deque.
     * nova_fiber_spawn_into push'ит в scope arrays, но мы хотим в deque.
     * Использую low-level mco_create + manual deque push. */
    mco_desc desc = _NOVA_MCO_DESC_INIT(entry);
    desc.user_data = user;
    mco_coro* co = NULL;
    mco_result r = mco_create(&co, &desc);
    if (r != MCO_SUCCESS || co == NULL) {
        fprintf(stderr, "nova: runtime_spawn_global: mco_create failed (%d)\n", (int)r);
        abort();
    }
    if (!nova_deque_push(&target->deque, co)) {
        fprintf(stderr, "nova: runtime_spawn_global: deque_push failed\n");
        mco_destroy(co);
        abort();
    }

    nova_aint_inc(&target->pending_count);
    uv_async_send(&target->wake_handle);
}

/* ── Diagnostic ───────────────────────────────────────────────── */

int nova_runtime_worker_count(void) {
    return _n_workers;
}

int nova_runtime_current_worker_id(void) {
    return _current_worker_id;
}

bool nova_runtime_is_initialized(void) {
    return _initialized;
}
