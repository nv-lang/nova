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

#ifdef NOVA_USE_LIBUV

#include "eventloop.h"
#include <stdio.h>
#include <stdlib.h>

static uv_loop_t* _evloop = NULL;
static int        _evloop_state = 0;  /* 0 = uninit, 1 = active, 2 = closed */

void nova_evloop_init(void) {
    if (_evloop_state != 0) return;  /* idempotent */
    _evloop = uv_default_loop();
    if (!_evloop) {
        fprintf(stderr, "nova: uv_default_loop() returned NULL\n");
        abort();
    }
    _evloop_state = 1;
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

#endif /* NOVA_USE_LIBUV */
