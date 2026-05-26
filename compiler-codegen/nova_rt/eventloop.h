#ifndef NOVA_RT_EVENTLOOP_H
#define NOVA_RT_EVENTLOOP_H

/* Plan 22 Ф.2: глобальный uv_loop_t для всего runtime'а.
 *
 * Идея: одним event loop'ом крутится sleep (Plan 22 Ф.4), socket IO
 * (Plan 23+ std.net), file IO (Plan 23+ std.fs), таймеры, etc. Все
 * блокирующие операции паркуют fiber через park/wake API (Plan 22 Ф.3,
 * sched.h) — scheduler в idle уходит в uv_run.
 *
 * Lifecycle:
 *   1. Программа стартует — main() вызывает nova_evloop_init() (idempotent).
 *   2. Runtime операции получают loop через nova_evloop().
 *   3. На exit — nova_evloop_close() через atexit (либо явно в эпилоге).
 *
 * Threading model: single loop on main thread. Под M:N (Plan 23)
 * будет per-worker loop. До тех пор — один глобальный.
 */

/* Plan 22 F2: NOVA_USE_LIBUV mandatory. Stub branch удалён. */
#ifndef NOVA_USE_LIBUV
#  error "Plan 22 F2: NOVA_USE_LIBUV is mandatory (см. fibers.h)."
#endif

#include <uv.h>
#include <stdbool.h>   /* для bool без circular include nova_rt.h */
#include <stdlib.h>    /* malloc/realloc/free */
#include "sync.h"      /* nova_mutex_t — Plan 83.10.2 */

#ifdef __cplusplus
extern "C" {
#endif

/* Get the default loop. Lazy-initializes on first call.
 * Не возвращает NULL — на ошибке программа abort'ает. */
uv_loop_t* nova_evloop(void);

/* Plan 44.6 Layer 3: per-thread current event loop.
 *
 * libuv `uv_loop_t` — thread-bound resource. Под M:N runtime каждый
 * worker thread имеет own loop (NovaWorker.loop); main thread использует
 * глобальный nova_evloop(). Все timer/handle registrations в runtime
 * (Time.sleep, channels, Time.after) обязаны идти на own loop текущего
 * thread'а, иначе callback'и fire'ятся в чужом loop'е и park'нутый
 * fiber никогда не resume'ится.
 *
 * Set'ится:
 *   - main thread: в nova_evloop_init() = _evloop (global).
 *   - worker thread: в _worker_main (runtime.c) = &worker->loop.
 *
 * Fallback: NULL → nova_current_loop() ленится на nova_evloop()
 * (для C-static init paths и threads без runtime.init). */
#ifdef _MSC_VER
extern __declspec(thread) uv_loop_t* _nova_current_loop;
#else
extern __thread uv_loop_t* _nova_current_loop;
#endif

uv_loop_t* nova_current_loop(void);

/* Init the event loop. Идемпотентна — повторные вызовы no-op.
 * Должна быть вызвана из main-prelude. */
void nova_evloop_init(void);

/* Graceful shutdown: drain pending handles, close loop, free resources.
 * Регистрируется в main через atexit либо вызывается явно. После close —
 * nova_evloop() возвращает NULL и пишет warning. */
void nova_evloop_close(void);

/* True если nova_evloop_init был вызван и close ещё не произошёл.
 * Returns bool (== nova_bool). */
bool nova_evloop_is_initialized(void);

/* Introspection: количество активных libuv-handle'ов (для тестов). */
int nova_evloop_active_handles(void);

/* Plan 22 Ф.10: install SIGINT handler. Передаваем pointer на main-scope
 * cancel-flag (NovaFiberQueue.cancel_requested) — handler ставит его в
 * true, fiber'ы на yield-point бросают "scope cancelled", defer'ы
 * отрабатывают, graceful shutdown.
 *
 * Вызывать ОДИН раз из emit_main prelude после установки _nova_main_scope.
 * Idempotent (второй вызов — no-op). */
struct NovaFiberQueue;  /* forward */
void nova_evloop_install_sigint(struct NovaFiberQueue* main_scope);

/* ── Plan 83.10.2 (2026-05-26): cross-thread uv_close dispatch ──────
 *
 * libuv requires handle ops only on the loop's thread. Cancel can run
 * on any thread; the timer/handle may belong to a worker's loop.
 * Schedule the close via this queue + uv_async signal.
 *
 * Producer: any thread (e.g. cancel) — push job + uv_async_send.
 * Consumer: loop's thread — nova_loop_drain_closes drains the queue
 * and invokes uv_close for each job.
 *
 * Lifetime: embedded in NovaWorker (workers) or static global (main).
 * Mutex-protected for cross-thread push. */
typedef struct {
    uv_handle_t* handle;
    uv_close_cb  close_cb;
} NovaDeferredCloseJob;

typedef struct {
    NovaDeferredCloseJob* jobs;
    int                   count;
    int                   cap;
    nova_mutex_t          mu;
} NovaDeferredCloseQueue;

/* Initialize a deferred-close queue. Called once per worker preamble
 * + once for main during pool materialization. */
void nova_close_queue_init(NovaDeferredCloseQueue* q);

/* Destroy a deferred-close queue (frees job array, not the queue struct
 * itself — caller owns the struct). */
void nova_close_queue_destroy(NovaDeferredCloseQueue* q);

/* Drain pending close jobs on the current loop. Must be called from
 * the loop's thread (typically from the async wake callback). */
void nova_loop_drain_closes(NovaDeferredCloseQueue* q);

/* Schedule `handle` to be closed on its loop's thread. Thread-safe —
 * may be called from any thread. Triggers uv_async_send on the loop's
 * wake handle so the loop thread wakes and drains the queue.
 *
 * Preconditions:
 *   - handle must have been initialised on a known loop (worker or main).
 *   - close_cb must be non-NULL.
 *   - Caller ensures idempotency (e.g. via CAS on a stage field).
 *
 * Returns 0 on success, -1 if loop is not recognised or OOM.
 * Declaration only — implementation lives in runtime.c (needs _workers). */
int nova_loop_defer_close(uv_loop_t* loop,
                          uv_handle_t* handle,
                          uv_close_cb close_cb);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_EVENTLOOP_H */
