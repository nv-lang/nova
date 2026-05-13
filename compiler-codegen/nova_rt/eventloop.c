/* Plan 22 Ф.2: глобальный uv_loop_t lifecycle.
 *
 * Простая реализация:
 *   - один static uv_loop_t* (использует uv_default_loop())
 *   - флаг initialized: bool
 *   - init и close — идемпотентные
 *
 * Close-логика: при uv_loop_close возможен UV_EBUSY если есть active
 * handles (forgot to close timer / socket / fs req etc.). В таком случае
 * walk-and-close с retry-loop, max 100 iterations против infinite hang.
 *
 * Под bootstrap (single-thread) — без локов. Под M:N (Plan 23) loop
 * станет per-worker, эта реализация сменится.
 */

/* Plan 22 F2: NOVA_USE_LIBUV mandatory — `#error` если не defined. */
#ifndef NOVA_USE_LIBUV
#  error "Plan 22 F2: NOVA_USE_LIBUV is mandatory."
#endif

#include "eventloop.h"
#include <stdio.h>
#include <stdlib.h>

static uv_loop_t* _evloop = NULL;
static int        _evloop_state = 0;  /* 0 = uninit, 1 = active, 2 = closed */

/* Plan 44.6 Layer 3: TLS storage. Initialised lazily — main thread в
 * nova_evloop_init(), worker thread в _worker_main (runtime.c).
 * Fallback path в nova_current_loop() — если NULL, set к global default. */
#ifdef _MSC_VER
__declspec(thread) uv_loop_t* _nova_current_loop = NULL;
#else
__thread uv_loop_t* _nova_current_loop = NULL;
#endif

void nova_evloop_init(void) {
    if (_evloop_state != 0) return;  /* idempotent */
    _evloop = uv_default_loop();
    if (!_evloop) {
        fprintf(stderr, "nova: uv_default_loop() returned NULL\n");
        abort();
    }
    _evloop_state = 1;
    /* Plan 44.6 Layer 3: main thread current loop = global default.
     * Worker threads set'ят own loop в _worker_main. */
    _nova_current_loop = _evloop;
}

uv_loop_t* nova_evloop(void) {
    if (_evloop_state == 0) {
        /* Lazy auto-init — обычно main-prelude вызывает init явно,
         * но защита на случай если что-то лезет в evloop раньше. */
        nova_evloop_init();
    }
    if (_evloop_state == 2) {
        fprintf(stderr,
            "nova: nova_evloop() called after close — use-after-close bug\n");
        return NULL;
    }
    return _evloop;
}

uv_loop_t* nova_current_loop(void) {
    if (_nova_current_loop) return _nova_current_loop;
    /* Fallback: TLS не set'нут (main thread где evloop_init ещё не
     * fired либо thread без runtime.init). Lazily берём global default. */
    _nova_current_loop = nova_evloop();
    return _nova_current_loop;
}

bool nova_evloop_is_initialized(void) {
    return _evloop_state == 1;
}

/* Walk callback: close any handle that's not already closing. */
static void _evloop_close_walk_cb(uv_handle_t* handle, void* arg) {
    (void)arg;
    if (!uv_is_closing(handle)) {
        uv_close(handle, NULL);
    }
}

void nova_evloop_close(void) {
    if (_evloop_state != 1) return;  /* not active, no-op */

    /* Попытка close — если active handles остались, walk-and-close,
     * затем drain pending callbacks через uv_run, повторить. Max 100
     * iterations против infinite loop. */
    int attempts = 0;
    while (attempts < 100) {
        int rc = uv_loop_close(_evloop);
        if (rc == 0) {
            /* Чистый close. */
            _evloop_state = 2;
            _evloop = NULL;
            return;
        }
        if (rc != UV_EBUSY) {
            fprintf(stderr,
                "nova: uv_loop_close returned %d (%s), giving up\n",
                rc, uv_strerror(rc));
            _evloop_state = 2;
            _evloop = NULL;
            return;
        }
        /* UV_EBUSY — есть active handles. Закрыть все, drain. */
        uv_walk(_evloop, _evloop_close_walk_cb, NULL);
        uv_run(_evloop, UV_RUN_DEFAULT);
        attempts++;
    }

    fprintf(stderr,
        "nova: nova_evloop_close: failed to close loop after 100 attempts "
        "(handles stuck?). Forcing exit.\n");
    _evloop_state = 2;
    _evloop = NULL;
}

static void _evloop_count_walk_cb(uv_handle_t* handle, void* arg) {
    int* counter = (int*)arg;
    if (!uv_is_closing(handle)) {
        (*counter)++;
    }
}

int nova_evloop_active_handles(void) {
    if (_evloop_state != 1 || !_evloop) return 0;
    int count = 0;
    uv_walk(_evloop, _evloop_count_walk_cb, &count);
    return count;
}

/* Plan 22 Ф.10: SIGINT handler — graceful Ctrl+C → cancel main-scope.
 *
 * uv_signal_t handle регистрируется на SIGINT. При получении сигнала
 * callback ставит `cancel_requested = true` на main-scope. Все fiber'ы
 * на next yield-point бросают "scope cancelled" → unwind → defer'ы →
 * graceful exit.
 *
 * NB: Это работает **только** для fiber'ов внутри scope (D92 implicit
 * либо explicit supervised). Top-level main-flow exit'ит через scope-
 * drain после main-body, scope.cancel_requested срабатывает на pending
 * detach'ах. */
#include "nova_rt.h"  /* для NovaFiberQueue (тут нужен complete type) */

static uv_signal_t _sigint_handle;
static int         _sigint_installed = 0;
static NovaFiberQueue* _sigint_target = NULL;

static void _sigint_cb(uv_signal_t* handle, int signum) {
    (void)handle;
    (void)signum;
    if (_sigint_target) {
        _sigint_target->cancel_requested = true;
        /* Plan 22 Ф.10: cancel-wake parked fiber'ов immediate через
         * generic stop_cb mechanism (D93). */
        nova_sched_cancel_all_pending(_sigint_target);
        fprintf(stderr, "\nnova: SIGINT received — initiating graceful shutdown\n");
    }
}

void nova_evloop_install_sigint(struct NovaFiberQueue* main_scope) {
    if (_sigint_installed) return;
    if (!main_scope) return;
    _sigint_target = (NovaFiberQueue*)main_scope;
    if (uv_signal_init(nova_evloop(), &_sigint_handle) != 0) {
        fprintf(stderr, "nova: uv_signal_init failed — SIGINT graceful shutdown disabled\n");
        return;
    }
    /* SIGINT = 2 (POSIX standard, valid on Windows тоже через uv_signal). */
    if (uv_signal_start(&_sigint_handle, _sigint_cb, 2) != 0) {
        fprintf(stderr, "nova: uv_signal_start failed — SIGINT graceful shutdown disabled\n");
        uv_close((uv_handle_t*)&_sigint_handle, NULL);
        return;
    }
    /* unref'аем handle — он не должен держать loop alive. Loop exit'нет
     * когда все обычные handles закрыты, signal handler — passive. */
    uv_unref((uv_handle_t*)&_sigint_handle);
    _sigint_installed = 1;
}
