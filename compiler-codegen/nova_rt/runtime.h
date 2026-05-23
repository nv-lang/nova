// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_RUNTIME_H
#define NOVA_RT_RUNTIME_H

/* Plan 44 (M:N Этап 0, 2026-05-13) — multi-thread runtime API.
 *
 * Status: proof of concept. NOT default — bootstrap stays на single-thread
 * cooperative (libuv + minicoro). M:N opt-in через nova_runtime_init().
 *
 * Architecture:
 *   - N worker OS threads (default = uv_available_parallelism()).
 *   - Каждый worker имеет: libuv loop, fiber scope, mutex-protected push queue.
 *   - Spawn round-robin между workers (Chase-Lev deque — Этап 1).
 *   - Cross-worker wake через uv_async_send.
 *   - TLS: _nova_active_scope/_active_slot уже __thread (correct).
 *   - Fiber arena per-thread (Plan 44.2 Linux/macOS) — лениво init per worker.
 *   - Boehm GC: REQUIRES GC_THREADS build (Linux Docker автоматом).
 *
 * NOT included в Этап 0:
 *   - Work-stealing (deque) — Plan 45.
 *   - TLS migration для effect handlers — Plan 45.
 *   - Blocking pool — Plan 46.
 *   - std.sync Atomic[T] etc — Plan 18 + 46.
 */

#include <stddef.h>
#include <stdbool.h>
#include "minicoro.h"   /* для mco_coro в API signatures */
#include "deque.h"      /* Plan 44.5: Chase-Lev work-stealing deque */

/* Forward — full definition в runtime.c (opaque to API users). */
typedef struct NovaWorker NovaWorker;

/* ── Public API ─────────────────────────────────────────────────── */

/* Initialize M:N runtime с n_workers workers. Idempotent — повторный
 * вызов no-op. Если n=0 → автодетект через uv_available_parallelism. */
void nova_runtime_init(int n_workers);

/* Plan 83.2 Ф.1: auto-arm рантайма без указания n. Эквивалент
 * `nova_runtime_init(0)`, но семантически разделён — вызывается
 * codegen-эмитом ON START любой compiled-программы (в main()) для
 * default-on M:N. Идемпотентен; пул не материализуется до spawn. */
void nova_runtime_auto_arm(void);

/* Spawn fiber на следующий worker (round-robin). Используется для
 * top-level work distribution. Within-fiber spawn пока остаётся через
 * existing nova_fiber_spawn_into — на текущий scope.
 *
 * Идея: codegen может генерировать nova_runtime_spawn_global для
 * top-level supervised, и nova_fiber_spawn_into для nested.
 *
 * `entry` — обычная mco_coro callback с user_data.
 * `user` — pointer на NovaSpawnCtx_N (heap-managed, GC-tracked). */
void nova_runtime_spawn_global(void (*entry)(mco_coro*), void* user);

/* Plan 44.5 Layer 5: structured M:N spawn — push fiber в worker deque
 * + increment scope.pending_remote. Под `runtime.is_initialized()`
 * codegen routes spawn'ы через эту API вместо nova_fiber_spawn_into.
 *
 * Caller (codegen-emit'ом) обязан перед вызовом set ctx->_nova_parent_scope
 * = scope (через SpawnCtx field) — entry-функция читает его для:
 *   - error reporting в parent (TLS swap _nova_active_scope = parent),
 *   - decrement pending_remote + signal main thread после complete.
 *
 * Increment счётчика — release, чтобы main thread видел инкремент
 * before push'нутый fiber начнёт выполняться. */
struct NovaFiberQueue;  /* forward */
void nova_runtime_spawn_into(struct NovaFiberQueue* scope,
                              void (*entry)(mco_coro*),
                              void* user);

/* Plan 44.5 Layer 5: signal main thread из worker (cross-thread wake).
 * Worker fiber после complete / on error вызывает это, чтобы main
 * thread проснулся из uv_run(UV_RUN_ONCE) в supervised_run wait-loop.
 *
 * Internally — uv_async_send на singleton handle инициализированный
 * в nova_runtime_init на nova_evloop() (main thread's loop). No-op
 * если runtime не initialized (main wake handle отсутствует). */
void nova_runtime_signal_main(void);

/* Plan 83.4.5.2 Ф.1 (2026-05-23): orphan fiber spawn — fire-and-forget
 * без bound supervised-scope. Production-grade default для D50
 * `detach { body }` (паритет с Go `go fn()` / tokio::spawn / Kotlin
 * GlobalScope.launch).
 *
 * Semantics:
 *   - Под armed runtime → push в worker deque (как nova_runtime_spawn_global,
 *     но не tracking pending_remote какого-либо scope'а — fiber truly
 *     orphan).
 *   - Под bootstrap (`_armed == false`) → cooperative fallback в global
 *     orphan-scope; drain через atexit (nova_runtime_drain_orphans).
 *   - LogAndDrop fail semantic (Plan 50): orphan fiber's unhandled
 *     errors → fprintf stderr + не abort (caller уже вернулся).
 *
 * Caller (codegen): emit аналогично emit_spawn — alloc heap-managed ctx
 * с capture'ами, передаёт fn pointer на generated entry-функцию.
 * Возвращается мгновенно — body's side-effects не гарантированы видимы
 * сразу после вызова caller'у (контракт fire-and-forget).
 *
 * Cross-runtime parity:
 *   - Go `go fn()` → runtime.newproc → goroutine scheduler runq.
 *   - tokio `tokio::spawn(future)` → multi-thread executor task queue.
 *   - Kotlin `GlobalScope.launch { … }` → truly detached coroutine.
 *   - Node `setImmediate(cb)` → event-loop queue (single-thread analog).
 */
void nova_runtime_spawn_orphan(void (*entry)(mco_coro*), void* user);

/* Plan 83.4.5.2 Ф.1 (2026-05-23): drain orphan-scope synchronously.
 * Используется в test-suite для explicit-sync (как Go `sync.WaitGroup`)
 * + atexit вызывает автоматически. Idempotent: empty orphan scope —
 * no-op. После drain orphan scope reset к empty (можно snadily spawn'ить
 * new orphans, drain снова). */
void nova_runtime_drain_orphans(void);

/* Graceful shutdown — signal all workers, join, free resources.
 * Called by codegen в exit path (либо явно через runtime.shutdown()). */
void nova_runtime_shutdown(void);

/* Plan 83.1 Ф.1+Ф.2: резолвер числа worker-потоков.
 * Порядок: explicit runtime.init(n>0) > ENV NOVA_MAXPROCS >
 * uv_available_parallelism(). Результат всегда в [1, 1024]. `explicit_n`
 * <= 0 означает «не задано явно» (auto-detect). nova_runtime_init
 * прогоняет свой аргумент через этот резолвер. */
int  nova_runtime_resolve_maxprocs(int explicit_n);

/* Diagnostic — exposed через std.runtime.runtime. */
int  nova_runtime_worker_count(void);
/* Plan 83.1 Ф.3: целевое число worker'ов (резолвится и до init).
 * worker_count() = фактически поднятые; maxprocs() = цель. */
int  nova_runtime_maxprocs(void);
int  nova_runtime_current_worker_id(void);  /* -1 если main thread */
bool nova_runtime_is_initialized(void);

#endif /* NOVA_RT_RUNTIME_H */
