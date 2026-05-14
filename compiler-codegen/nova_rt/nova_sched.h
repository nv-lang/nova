#ifndef NOVA_RT_SCHED_H
#define NOVA_RT_SCHED_H

/* Plan 22 Ф.3 (D93): нормативный park/wake API для блокирующих операций.
 *
 * Любая блокирующая операция в runtime'е (Time.sleep, Channel.recv,
 * socket-read, file-read) обязана использовать этот API:
 *
 *   1. register_pending(scope, slot, handle, stop_cb)
 *   2. park(scope, slot)
 *   3. (control returns when callback либо cancel сделал wake)
 *   4. unregister_pending(scope, slot)
 *   5. check cancel_requested → throw if cancelled
 *
 * Production: state — **lazy-allocated pointer-в-NovaFiberQueue**
 * (Вариант B). O(1) lookup через pointer-deref, нет cap'а на nested
 * scopes, память выделяется только когда реально park'аем. GC автоматически
 * освобождает state при сборке scope'а.
 *
 * Под M:N (Plan 23) — переход на per-worker scope-state, atomic operations
 * через MO_RELEASE/MO_ACQUIRE на parked[].
 */

/* Подключается из nova_rt.h ПОСЛЕ fibers.h. Прямые deps:
 *   - NovaFiberQueue, NOVA_SCOPE_CAP — из fibers.h
 *   - NovaSchedState, NovaSchedStopCb — из fibers.h (typedef'нуты там)
 *   - nova_sched_find_state — из fibers.h (inline)
 *   - mco_running, mco_yield — из minicoro.h (через fibers.h)
 *   - nova_alloc — из alloc.h (через nova_rt.h)
 *   - nova_bool, fprintf, abort — из stdio.h/stdlib.h (через nova_rt.h)
 */

#ifdef __cplusplus
extern "C" {
#endif

/* ─── Park/wake state allocation ──────────────────────────────── */

/* Plan 22 Ф.7: grow sched_state arrays до new_cap (синхронизируется
 * с scope.capacity). Internal API. */
static inline void nova_sched_grow_state(NovaFiberQueue* scope, int new_cap) {
    if (!scope || !scope->sched_state) return;
    NovaSchedState* st = scope->sched_state;
    if (new_cap <= st->capacity) return;
    int cap = st->capacity > 0 ? st->capacity : NOVA_SCOPE_INITIAL_CAP;
    while (cap < new_cap) cap *= 2;
    /* Allocate new arrays. */
    nova_bool*       new_parked = (nova_bool*)nova_alloc(sizeof(nova_bool) * cap);
    void**           new_handle = (void**)nova_alloc(sizeof(void*) * cap);
    NovaSchedStopCb* new_stop_cb = (NovaSchedStopCb*)nova_alloc(sizeof(NovaSchedStopCb) * cap);
    /* Copy existing + init new. */
    for (int i = 0; i < cap; i++) {
        if (i < st->capacity && st->parked) {
            new_parked[i] = st->parked[i];
            new_handle[i] = st->pending_handle[i];
            new_stop_cb[i] = st->pending_stop_cb[i];
        } else {
            new_parked[i] = false;
            new_handle[i] = NULL;
            new_stop_cb[i] = NULL;
        }
    }
    st->parked = new_parked;
    st->pending_handle = new_handle;
    st->pending_stop_cb = new_stop_cb;
    st->capacity = cap;
}

/* Lookup-or-create state for given scope. Production-grade Ф.7: lazy
 * heap-alloc + arrays sized под scope.capacity (которая растёт через
 * nova_scope_grow + nova_sched_grow_state в spawn_into). */
static inline NovaSchedState* nova_sched_get_state(NovaFiberQueue* scope) {
    if (!scope) return NULL;
    if (scope->sched_state) return scope->sched_state;
    NovaSchedState* st = (NovaSchedState*)nova_alloc(sizeof(NovaSchedState));
    if (!st) {
        fprintf(stderr, "nova: nova_sched_get_state: nova_alloc failed\n");
        abort();
    }
    st->parked = NULL;
    st->pending_handle = NULL;
    st->pending_stop_cb = NULL;
    st->capacity = 0;
    scope->sched_state = st;
    /* Grow до текущего scope.capacity (обычно ≥ NOVA_SCOPE_INITIAL_CAP). */
    int target = scope->capacity > 0 ? scope->capacity : NOVA_SCOPE_INITIAL_CAP;
    nova_sched_grow_state(scope, target);
    return st;
}

/* Drop state for scope. Production: no-op — GC автоматически соберёт
 * state когда NovaFiberQueue станет unreachable (после supervised_run
 * exit'а). Оставлена как API surface для будущей M:N миграции, где
 * eager-drop может быть полезен. */
static inline void nova_sched_drop_state(NovaFiberQueue* scope) {
    /* GC handles это автоматически. Если scope живёт в stack-allocated
     * NovaFiberQueue (как в emit_c для supervised), sched_state живёт
     * на managed heap и GC соберёт когда scope-stack-frame uniwind'ит. */
    if (scope) scope->sched_state = NULL;
}

/* ─── Park / wake ─────────────────────────────────────────────── */

/* Park current fiber: remove from ready-queue, отдать control scheduler'у.
 * Возвращается только когда nova_sched_wake() будет вызван для (scope, slot).
 * Не вызывать из не-fiber кода. */
static inline void nova_sched_park(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) {
        fprintf(stderr, "nova: nova_sched_park: invalid scope/slot\n");
        abort();
    }
    NovaSchedState* st = nova_sched_get_state(scope);
    st->parked[slot] = true;
    mco_coro* co = mco_running();
    if (!co) {
        fprintf(stderr, "nova: nova_sched_park: not in fiber context\n");
        abort();
    }
    mco_yield(co);
}

/* Plan 44.1 R2 C6: park_with_unlock — atomically transition to parked state
 * and release a lock, so wake from another thread cannot race with park.
 *
 * Pattern (lost-wakeup-free):
 *   nova_mutex_lock(&st->mu);
 *   // register waiter under lock
 *   nova_sched_park_with_unlock(scope, slot, _unlock_mu, &st->mu);
 *   // mutex is already released by the time we return
 *   nova_mutex_lock(&st->mu);
 *   // re-check state, unlink waiter
 *
 * Bootstrap implementation (single-thread): unlock BEFORE park is safe
 * because no other thread exists. Under Plan 23 M:N this MUST become
 * atomic — scheduler must transition fiber to parked state, then call
 * unlock_fn, then deschedule. Until then this API contract is the
 * single source of truth for callers; callers MUST use this instead of
 * lock+unlock+park because they will break under M:N otherwise.
 *
 * IMPORTANT: callers MUST re-check application state after park returns
 * (spurious wakes are allowed). For Plan 44.1 channels/select, the state
 * is `BaseWaiter.fired` atomic — if 0 after park, retry try_immediate
 * or park again. This re-check is correctness, not optimization. */
static inline void nova_sched_park_with_unlock(NovaFiberQueue* scope, int slot,
                                                 void (*unlock_fn)(void*),
                                                 void* unlock_arg) {
    if (!scope || slot < 0 || slot >= scope->count) {
        fprintf(stderr, "nova: nova_sched_park_with_unlock: invalid scope/slot\n");
        abort();
    }
    NovaSchedState* st = nova_sched_get_state(scope);
    st->parked[slot] = true;
    mco_coro* co = mco_running();
    if (!co) {
        fprintf(stderr, "nova: nova_sched_park_with_unlock: not in fiber context\n");
        abort();
    }
    /* Bootstrap single-thread: unlock before park is safe.
     * M:N (Plan 23): unlock must happen AFTER parked-state visible to
     * other threads — scheduler implementation responsibility. */
    if (unlock_fn) unlock_fn(unlock_arg);
    mco_yield(co);
}

/* Wake parked fiber. Idempotent. Безопасно вызывать из libuv-callback'а.
 *
 * Plan 44.5 Layer 5: если scope->dispatch_ready != NULL (M:N worker
 * scope), re-schedule fiber через dispatch hook (same-thread deque push
 * или cross-thread pending list + uv_async_send). Single-thread scopes
 * (dispatch_ready == NULL) — поведение прежнее: supervisor loop resume'ит. */
static inline void nova_sched_wake(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) return;
    NovaSchedState* st = nova_sched_find_state(scope);
    if (st && slot < st->capacity) st->parked[slot] = false;
    /* M:N: push woken fiber back to worker deque. */
    if (scope->dispatch_ready && slot < scope->count) {
        mco_coro* co = scope->fibers[slot];
        if (co && mco_status(co) != MCO_DEAD) {
            scope->dispatch_ready(scope->dispatch_ctx, co);
        }
    }
}

/* True если fiber в slot сейчас parked. */
static inline nova_bool nova_sched_is_parked(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) return false;
    NovaSchedState* st = nova_sched_find_state(scope);
    return st && slot < st->capacity && st->parked[slot];
}

/* ─── Cancel-integration ──────────────────────────────────────── */

/* Регистрация handle + stop_cb для cancel-wake. ОБЯЗАТЕЛЬНО перед park'ом
 * для cancel-correctness (D93 contract). Lazy-allocates sched_state. */
static inline void nova_sched_register_pending(NovaFiberQueue* scope, int slot,
                                                void* handle,
                                                NovaSchedStopCb stop_cb) {
    if (!scope || slot < 0) return;
    NovaSchedState* st = nova_sched_get_state(scope);
    if (slot >= st->capacity) {
        nova_sched_grow_state(scope, slot + 1);
    }
    st->pending_handle[slot] = handle;
    st->pending_stop_cb[slot] = stop_cb;
}

/* Снять регистрацию. Должно вызываться ПОСЛЕ wake (любой — normal либо
 * cancel), перед cancel-check. Idempotent. */
static inline void nova_sched_unregister_pending(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0) return;
    NovaSchedState* st = nova_sched_find_state(scope);
    if (st && slot < st->capacity) {
        st->pending_handle[slot] = NULL;
        st->pending_stop_cb[slot] = NULL;
    }
}

/* ─── Cancel-flow integration: вызывается из nova_cancel_token_cancel ── */

/* Trigger all pending stop_cb's for scope. Cancel-during-park flow по D93:
 *
 * Ф.8 contract: stop_cb возвращает NovaStopMode:
 *  - SYNC:  handle полностью cleaned после stop_cb return → unpark
 *           immediate, fiber resume'ится сразу, видит cancel_requested,
 *           throw'ает "scope cancelled".
 *  - ASYNC: stop_cb лишь инициировал close → fiber остаётся parked,
 *           backend (uv close_cb / waitlist removal) сделает wake
 *           когда handle полностью released. После backend wake fiber
 *           resume'ится, видит cancel_requested, throw'ает.
 *
 * Slots без registered handle (stop_cb == NULL): unpark unconditional —
 * fiber park'нулся через bare nova_sched_park без блокирующей операции
 * (нештатный flow), нет smart-cleanup. */
static inline void nova_sched_cancel_all_pending(NovaFiberQueue* scope) {
    NovaSchedState* st = nova_sched_find_state(scope);
    if (!st) return;
    /* Iterate min(scope->count, st->capacity). Если spawn_into добавил
     * slots но sched-state ещё не grow'нулся — нечего отменять (никто
     * не park'ался). */
    int n = scope->count < st->capacity ? scope->count : st->capacity;
    for (int i = 0; i < n; i++) {
        if (st->pending_stop_cb[i] && st->pending_handle[i]) {
            NovaStopMode mode = st->pending_stop_cb[i](st->pending_handle[i]);
            if (mode == NOVA_STOP_SYNC) {
                st->parked[i] = false;  /* SYNC: unpark immediate */
            }
            /* ASYNC: НЕ unpark'аем — backend сделает wake через close_cb
             * (для sleep) либо waitlist-removal callback (для channels). */
        } else if (st->parked[i]) {
            /* Park без registered stop_cb (bare park) — unpark
             * unconditional, нет handle для cleanup'а. */
            st->parked[i] = false;
        }
    }
}

/* ─── Introspection ──────────────────────────────────────────── */

static inline int nova_sched_count_alive(NovaFiberQueue* scope) {
    if (!scope) return 0;
    int count = 0;
    for (int i = 0; i < scope->count; i++) {
        if (scope->fibers[i] != NULL && mco_status(scope->fibers[i]) != MCO_DEAD) {
            count++;
        }
    }
    return count;
}

static inline int nova_sched_count_parked(NovaFiberQueue* scope) {
    if (!scope) return 0;
    NovaSchedState* st = nova_sched_find_state(scope);
    if (!st) return 0;
    int count = 0;
    int n = scope->count < st->capacity ? scope->count : st->capacity;
    for (int i = 0; i < n; i++) {
        if (scope->fibers[i] != NULL
            && mco_status(scope->fibers[i]) != MCO_DEAD
            && st->parked[i]) {
            count++;
        }
    }
    return count;
}

static inline int nova_sched_count_ready(NovaFiberQueue* scope) {
    return nova_sched_count_alive(scope) - nova_sched_count_parked(scope);
}

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_SCHED_H */
