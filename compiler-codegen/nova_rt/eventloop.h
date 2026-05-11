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

#ifdef NOVA_USE_LIBUV

#include <uv.h>
#include <stdbool.h>   /* для bool без circular include nova_rt.h */

#ifdef __cplusplus
extern "C" {
#endif

/* Get the default loop. Lazy-initializes on first call.
 * Не возвращает NULL — на ошибке программа abort'ает. */
uv_loop_t* nova_evloop(void);

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

#else  /* !NOVA_USE_LIBUV */

/* Stub когда libuv не включён (Ф.1 default — libuv есть в build chain,
 * но runtime может его не использовать). Все функции no-op.
 * Используем bool (stdbool.h) вместо nova_bool — избегаем circular
 * include nova_rt.h → fibers.h → eventloop.h → nova_rt.h. */

#include <stdbool.h>

static inline void nova_evloop_init(void) { }
static inline void nova_evloop_close(void) { }
static inline bool nova_evloop_is_initialized(void) { return false; }
static inline int  nova_evloop_active_handles(void) { return 0; }
/* SIGINT handler stub (no-libuv): no-op. */
struct NovaFiberQueue;  /* forward */
static inline void nova_evloop_install_sigint(struct NovaFiberQueue* main_scope) {
    (void)main_scope;
}

#endif /* NOVA_USE_LIBUV */

#endif /* NOVA_RT_EVENTLOOP_H */
