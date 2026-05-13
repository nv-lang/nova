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

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_EVENTLOOP_H */
