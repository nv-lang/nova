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

/* No <gc.h> — workers don't touch Boehm в Этапе 0 (см. _worker_main note). */

/* ── Worker struct ─────────────────────────────────────────────── */

struct NovaWorker {
    int               id;
    uv_thread_t       thread;
    uv_loop_t         loop;
    uv_async_t        wake_handle;
    NovaFiberQueue    scope;
    nova_mutex_t      queue_mu;
    nova_atomic_bool  stop;
    nova_atomic_int   pending_count;  /* fibers waiting в queue */
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

    /* Plan 44.1 (Plan 44 Этап 0): НЕ регистрируем thread с Boehm GC — это требует
     * GC_THREADS build (Linux Docker default, Windows vcpkg может без).
     * Worker'ы в Этапе 0 не делают nova_alloc — только infrastructure
     * idle. GC register — задача Plan 45+ когда workers будут actually
     * выполнять fiber workloads. */

    /* Per-worker TLS: _nova_active_scope указывает на own scope.
     * Объявлены в fibers.h cross-platform; здесь только set. */
    _nova_active_scope = &w->scope;
    _nova_active_slot  = -1;

    while (!nova_abool_load(&w->stop)) {
        /* (1) Drain ready fibers без global event-loop dependency.
         * Plan 44.1 (Plan 44 Этап 0): simple drain — supports CPU-bound fibers только.
         * Yielding fibers (Time.sleep, Channel.recv) требуют param'ed
         * nova_supervised_run — Plan 45. */
        bool did_work = false;
        nova_mutex_lock(&w->queue_mu);
        int n = w->scope.count;
        nova_mutex_unlock(&w->queue_mu);
        if (n > 0) {
            for (int i = 0; i < n; i++) {
                mco_coro* co;
                nova_mutex_lock(&w->queue_mu);
                co = (i < w->scope.count) ? w->scope.fibers[i] : NULL;
                nova_mutex_unlock(&w->queue_mu);
                if (!co) continue;
                if (mco_status(co) == MCO_SUSPENDED) {
                    _nova_active_slot = i;
                    mco_resume(co);
                    did_work = true;
                }
                if (mco_status(co) == MCO_DEAD) {
                    mco_destroy(co);
                    nova_mutex_lock(&w->queue_mu);
                    /* Mark slot empty by setting fiber=NULL. Compaction
                     * — TODO; для PoC просто NULL'им. */
                    if (i < w->scope.count) {
                        w->scope.fibers[i] = NULL;
                    }
                    nova_mutex_unlock(&w->queue_mu);
                }
            }
            _nova_active_slot = -1;
            /* Compact scope: remove NULL entries. */
            nova_mutex_lock(&w->queue_mu);
            int wi = 0;
            for (int i = 0; i < w->scope.count; i++) {
                if (w->scope.fibers[i]) {
                    if (wi != i) {
                        w->scope.fibers[wi] = w->scope.fibers[i];
                        w->scope.fiber_ctx[wi] = w->scope.fiber_ctx[i];
                    }
                    wi++;
                }
            }
            w->scope.count = wi;
            nova_mutex_unlock(&w->queue_mu);
        }

        /* (2) Idle — block в libuv до wake_handle (cross-worker push). */
        if (!did_work) {
            uv_run(&w->loop, UV_RUN_ONCE);
        }
    }

    /* Cleanup — drain remaining. */
    _nova_active_slot = -1;

    /* No GC unregister — мы не регистрировали (см. note выше). */
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
        nova_mutex_init(&w->queue_mu);
        nova_abool_init(&w->stop, false);
        nova_aint_init(&w->pending_count, 0);
        nova_scope_init(&w->scope);

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
        nova_mutex_destroy(&w->queue_mu);
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

    nova_mutex_lock(&target->queue_mu);
    nova_fiber_spawn_into(&target->scope, entry, user);
    nova_mutex_unlock(&target->queue_mu);

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
