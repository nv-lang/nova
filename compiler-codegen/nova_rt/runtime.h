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
 *   - Fiber arena per-thread (Plan 41 Linux/macOS) — лениво init per worker.
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

/* Forward — full definition в runtime.c (opaque to API users). */
typedef struct NovaWorker NovaWorker;

/* ── Public API ─────────────────────────────────────────────────── */

/* Initialize M:N runtime с n_workers workers. Idempotent — повторный
 * вызов no-op. Если n=0 → автодетект через uv_available_parallelism. */
void nova_runtime_init(int n_workers);

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

/* Graceful shutdown — signal all workers, join, free resources.
 * Called by codegen в exit path (либо явно через runtime.shutdown()). */
void nova_runtime_shutdown(void);

/* Diagnostic — exposed через std.runtime.runtime. */
int  nova_runtime_worker_count(void);
int  nova_runtime_current_worker_id(void);  /* -1 если main thread */
bool nova_runtime_is_initialized(void);

#endif /* NOVA_RT_RUNTIME_H */
