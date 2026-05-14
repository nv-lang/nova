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

/* Plan 44.5 Layer 4+5: Boehm GC_THREADS register per worker.
 * vcpkg bdwgc build.ninja shows -DGC_THREADS in DEFINES — library IS thread-safe.
 * Client must define GC_THREADS too (via test_runner -DGC_THREADS) to expose
 * GC_register_my_thread / GC_allow_register_threads prototypes.
 * Works on all platforms when GC_THREADS defined at compile time. */
#if defined(NOVA_GC_BOEHM)
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
    /* Plan 44.5 Layer 5 park/wake: cross-thread wake queue.
     * Fibers parked on this worker (via dispatch_ready from another worker or
     * timer callbacks) accumulate here under wake_mu; drained at each worker
     * loop iteration before deque pop. */
    nova_mutex_t      wake_mu;
    mco_coro**        wake_pending;
    int               wake_pending_count;
    int               wake_pending_cap;
};

/* ── Runtime state ─────────────────────────────────────────────── */

static NovaWorker*     _workers = NULL;
static int             _n_workers = 0;
static nova_atomic_int _round_robin = 0;
static bool            _initialized = false;
static nova_mutex_t    _init_mu;
static bool            _init_mu_inited = false;

/* Plan 44.5 Layer 5: main wake handle для cross-thread signal'а из
 * worker'а в main thread'а supervised_run wait-loop. Init'ится в
 * nova_runtime_init на nova_evloop (main thread's default loop). */
static uv_async_t      _main_wake;
static bool            _main_wake_inited = false;

static void _main_wake_cb(uv_async_t* h) {
    (void)h;
    /* No-op — signal itself wakes uv_run(UV_RUN_ONCE) в main thread'е.
     * Main thread сам проверяет scope.pending_remote после wake'а. */
}

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

/* Plan 44.5 Layer 5 park/wake: dispatch hook called by nova_sched_wake.
 * Same-thread (owner wake via timer on own loop): direct deque push.
 * Cross-thread (wake from different worker or main thread): mutex-protected
 * wake_pending list + uv_async_send to wake the target worker's uv_run. */
static void _worker_dispatch_ready(void* ctx, mco_coro* co) {
    NovaWorker* w = (NovaWorker*)ctx;
    if (_current_worker_id == w->id) {
        /* Owner push: lock-free, same thread as deque owner. */
        nova_deque_push(&w->deque, co);
    } else {
        /* Cross-thread: queue under mutex, wake worker's uv loop. */
        nova_mutex_lock(&w->wake_mu);
        if (w->wake_pending_count >= w->wake_pending_cap) {
            int new_cap = w->wake_pending_cap > 0 ? w->wake_pending_cap * 2 : 8;
            w->wake_pending = (mco_coro**)realloc(w->wake_pending,
                                                   (size_t)new_cap * sizeof(mco_coro*));
            if (!w->wake_pending) abort();
            w->wake_pending_cap = new_cap;
        }
        w->wake_pending[w->wake_pending_count++] = co;
        nova_mutex_unlock(&w->wake_mu);
        uv_async_send(&w->wake_handle);
    }
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

    /* Plan 44.5 Layer 4+5: register thread с Boehm GC.
     * Required для workers — без register Boehm STW walker skips thread stack,
     * GC objects referenced only from worker stack → premature collect → SIGSEGV.
     * All platforms: vcpkg bdwgc built with -DGC_THREADS; client passes same flag. */
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
        /* (0) Drain cross-thread wake queue (fibers re-queued after park). */
        nova_mutex_lock(&w->wake_mu);
        for (int i = 0; i < w->wake_pending_count; i++) {
            nova_deque_push(&w->deque, w->wake_pending[i]);
        }
        w->wake_pending_count = 0;
        nova_mutex_unlock(&w->wake_mu);

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

        /* (3) Still nothing — block в libuv (own loop) до cross-worker wake.
         * UV_RUN_ONCE: wait for at least one event (timer fire, async send),
         * then return — loop checks wake_pending at next iteration start. */
        if (!co) {
            uv_run(&w->loop, UV_RUN_ONCE);
            continue;
        }

        /* (4) Run fiber.
         *
         * Plan 44.5 Layer 5 fix: save/restore _nova_fail_top, _nova_interrupt_top,
         * and _nova_active_slot per fiber — mirrors nova_supervised_step behavior.
         *
         * Bug without this: fiber F1 parks (fail-top = &_ff_F1). Fiber F2 runs
         * and parks (fail-top = &_ff_F2 → &_ff_F1). F1 resumes and throws →
         * longjmp(&_ff_F2->jmp) → cross-stack jump into F2's suspended coroutine
         * → SIGSEGV / STATUS_ACCESS_VIOLATION.
         *
         * Also fixes stale _nova_active_slot: without restore, _nova_active_slot
         * = previous fiber's slot (or -1) when fiber resumes, causing wrong slot
         * in channel ops on second+ park. */
        NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);

        /* Restore fiber's TLS snapshot (fail-top chain + active scope/slot).
         *
         * Plan 44.5 deadlock fix (work-stealing): a fiber's home scope
         * (_nova_fiber_scope) is fixed to the worker that ran its preamble.
         * If stolen by another worker, we MUST restore _nova_active_scope to
         * the home scope so channel ops capture the correct scope/slot.
         * Without this, the channel waiter records the stealer's scope, and
         * nova_sched_wake finds scope->fibers[slot]=NULL → dispatch_ready not
         * called → fiber never re-queued → permanent hang (deadlock). */
        NovaFiberQueue*     outer_scope     = _nova_active_scope;
        NovaFailFrame*      outer_fail      = _nova_fail_top;
        NovaInterruptFrame* outer_interrupt = _nova_interrupt_top;
        if (base && base->_nova_worker_slot >= 0 && base->_nova_fiber_scope) {
            /* Preamble already ran: restore home scope + saved TLS. */
            _nova_active_scope  = base->_nova_fiber_scope;
            _nova_active_slot   = base->_nova_worker_slot;
            _nova_fail_top      = base->_nova_saved_fail_top;
            _nova_interrupt_top = base->_nova_saved_interrupt_top;
        } else if (base) {
            /* Before preamble (first run): restore saved fail/interrupt but
             * leave _nova_active_scope as this worker's scope (preamble will
             * allocate the home slot + set _nova_fiber_scope on first resume). */
            _nova_fail_top      = base->_nova_saved_fail_top;
            _nova_interrupt_top = base->_nova_saved_interrupt_top;
        }

        if (mco_status(co) == MCO_SUSPENDED) {
            mco_resume(co);
        }

        /* Save fiber's current TLS state back; restore outer worker state. */
        if (base) {
            base->_nova_saved_fail_top      = _nova_fail_top;
            base->_nova_saved_interrupt_top = _nova_interrupt_top;
        }
        _nova_active_scope  = outer_scope;
        _nova_fail_top      = outer_fail;
        _nova_interrupt_top = outer_interrupt;

        /* Plan 44.5 Layer 5 deferred-unlock: check parked state BEFORE releasing
         * the channel/sleep mutex. This captures parked[slot]=true while no
         * cross-thread waker can clear it (they are blocked on the mutex).
         * Only after this check do we release the mutex via the deferred fn.
         *
         * Use _nova_fiber_scope (home scope) for is_parked check — must match
         * the scope used by the fiber in nova_sched_park_with_unlock, which
         * captures _nova_active_scope (restored to _nova_fiber_scope above). */
        bool fiber_is_parked = false;
        if (mco_status(co) == MCO_SUSPENDED) {
            NovaFiberQueue* check_scope = (base && base->_nova_fiber_scope)
                                          ? base->_nova_fiber_scope : &w->scope;
            int act_slot = base ? base->_nova_worker_slot : _nova_active_slot;
            if (act_slot >= 0) {
                fiber_is_parked = (bool)nova_sched_is_parked(check_scope, act_slot);
            }
        }
        if (_nova_park_unlock_fn) {
            void (*fn)(void*) = _nova_park_unlock_fn;
            void* arg = _nova_park_unlock_arg;
            _nova_park_unlock_fn  = NULL;
            _nova_park_unlock_arg = NULL;
            fn(arg);
        }

        if (mco_status(co) == MCO_DEAD) {
            mco_destroy(co);
        } else if (mco_status(co) == MCO_SUSPENDED) {
            /* Yielded: if parked (timer/channel wait) → dispatch_ready re-queues.
             * If not parked (cooperative yield) → re-push immediately. */
            if (fiber_is_parked) {
                /* Parked: let dispatch_ready handle requeueing when wake fires. */
            } else {
                nova_deque_push(&w->deque, co);
            }
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

    /* Plan 44.5 Layer 5: init main wake handle на nova_evloop()
     * (main thread's default loop — мы сейчас на main thread). Workers
     * сделают uv_async_send(&_main_wake) после fiber complete; main
     * thread в uv_run(UV_RUN_ONCE) проснётся и проверит pending_remote. */
    if (!_main_wake_inited) {
        int rc = uv_async_init(nova_evloop(), &_main_wake, _main_wake_cb);
        if (rc != 0) {
            fprintf(stderr, "nova: uv_async_init main_wake failed: %s\n",
                    uv_strerror(rc));
            abort();
        }
        /* Unref — handle не должен сам keep'ить loop alive. Loop active
         * пока есть active timer/handles из user code (sleep, channels). */
        uv_unref((uv_handle_t*)&_main_wake);
        _main_wake_inited = true;
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
        nova_abool_init(&w->stop, false);
        nova_aint_init(&w->pending_count, 0);
        nova_scope_init(&w->scope);
        /* Plan 44.5 Layer 2: per-worker Chase-Lev deque. */
        if (!nova_deque_init(&w->deque, 64)) {
            fprintf(stderr, "nova: deque_init failed\n");
            abort();
        }
        /* Plan 44.5 Layer 5 park/wake: pre-alloc scope arrays on main thread
         * (GC-safe) so worker fibers don't call nova_alloc during slot alloc.
         * Also pre-alloc sched_state so park arrays exist before first park. */
        nova_scope_grow(&w->scope, 64);
        (void)nova_sched_get_state(&w->scope);
        /* dispatch_ready hook wires nova_sched_wake → worker deque push. */
        w->scope.dispatch_ready = _worker_dispatch_ready;
        w->scope.dispatch_ctx   = w;
        /* wake_pending: cross-thread fiber re-queue under mutex. */
        nova_mutex_init(&w->wake_mu);
        w->wake_pending       = NULL;
        w->wake_pending_count = 0;
        w->wake_pending_cap   = 0;

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
        free(w->wake_pending);
        w->wake_pending = NULL;
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

/* Plan 44.5 Layer 5: structured M:N spawn — distribute fiber на worker
 * + tracking в parent scope. Caller (codegen) обязан set
 * ctx->_nova_parent_scope = scope **перед** этим вызовом — entry-функция
 * читает поле для post-completion decrement + signal_main.
 *
 * Release ordering на increment — main thread в supervised_run wait-loop
 * увидит инкремент до того как worker fiber sees decremented count
 * (через cause-effect через memory). */
void nova_runtime_spawn_into(struct NovaFiberQueue* scope,
                              void (*entry)(mco_coro*),
                              void* user) {
    if (!scope) {
        fprintf(stderr, "nova: runtime_spawn_into: NULL scope\n");
        abort();
    }
    if (!_initialized || _n_workers == 0) {
        /* Fallback — fall through к normal spawn в active scope.
         * Это safety net; codegen эмитит conditional check, но если
         * runtime caller вызовет напрямую без init — degraded behavior. */
        nova_fiber_spawn_into((NovaFiberQueue*)scope, entry, user);
        return;
    }
    /* Increment ДО push'а — main thread в drain-loop должен видеть
     * pending_remote > 0 даже если worker сразу подхватит fiber и завершит
     * его до того как main опросит counter. */
    nova_aint_inc(&((NovaFiberQueue*)scope)->pending_remote);
    /* Реальный push идёт через spawn_global. */
    nova_runtime_spawn_global(entry, user);
}

/* Plan 44.5 Layer 5: signal main thread из worker context'а.
 * No-op до runtime.init либо после shutdown — main thread в этих режимах
 * либо вообще нет (test'у без init), либо exit'ит (shutdown). */
void nova_runtime_signal_main(void) {
    if (_main_wake_inited) {
        uv_async_send(&_main_wake);
    }
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
