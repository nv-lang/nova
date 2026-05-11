#ifndef NOVA_RT_SCHED_H
#define NOVA_RT_SCHED_H

/* Plan 22 Ф.3 (D93): нормативный park/wake API для блокирующих операций.
 *
 * Любая блокирующая операция в runtime'е (Time.sleep, Channel.recv,
 * socket-read, file-read) обязана использовать этот API:
 *
 *   1. register_pending(scope, slot, handle, stop_cb)
 *   2. park(scope, slot)
 *   3. (control returns here when callback либо cancel сделал wake)
 *   4. unregister_pending(scope, slot)
 *   5. check cancel_requested → throw if cancelled
 *
 * Под bootstrap (single-thread N:1) park/wake state живёт в **side-table**
 * indexed by scope-pointer — не embedded в NovaFiberQueue, потому что
 * NovaFiberQueue allocated на C-стеке (вокруг supervised), и 3× по
 * NOVA_SCOPE_CAP массивов раздули бы стек.
 *
 * Side-table — static array на 16 nested scopes (обычно ≤ 3 в практике).
 * Lazy init при первом nova_sched_get_state. Drop при scope-exit.
 *
 * Под M:N (Plan 23) — переход на per-worker scope-map, atomic operations.
 */

/* Подключается из nova_rt.h ПОСЛЕ fibers.h. Прямые deps:
 *   - NovaFiberQueue, NOVA_SCOPE_CAP — из fibers.h
 *   - mco_running, mco_yield — из minicoro.h (подключён через fibers.h)
 *   - nova_bool, fprintf, abort — из stdio.h/stdlib.h (через nova_rt.h)
 */

#ifdef __cplusplus
extern "C" {
#endif

/* ─── Park/wake state (side-table) ─────────────────────────────── */

typedef void (*NovaSchedStopCb)(void* handle);

typedef struct {
    NovaFiberQueue* scope;        /* owner — для lookup; NULL = empty slot */
    nova_bool       parked[NOVA_SCOPE_CAP];
    void*           pending_handle[NOVA_SCOPE_CAP];
    NovaSchedStopCb pending_stop_cb[NOVA_SCOPE_CAP];
} NovaSchedState;

#define NOVA_SCHED_STATE_CAP 16

/* Globals: storage + count. Один extern, одно definition (см. fibers.c). */
extern NovaSchedState _nova_sched_states[NOVA_SCHED_STATE_CAP];
extern int            _nova_sched_state_count;

/* Lookup state by scope. Returns NULL если не существует. */
static inline NovaSchedState* nova_sched_find_state(NovaFiberQueue* scope) {
    if (!scope) return NULL;
    for (int i = 0; i < _nova_sched_state_count; i++) {
        if (_nova_sched_states[i].scope == scope) return &_nova_sched_states[i];
    }
    return NULL;
}

/* Lookup-or-create state for given scope. */
static inline NovaSchedState* nova_sched_get_state(NovaFiberQueue* scope) {
    NovaSchedState* st = nova_sched_find_state(scope);
    if (st) return st;
    if (_nova_sched_state_count >= NOVA_SCHED_STATE_CAP) {
        fprintf(stderr, "nova: NOVA_SCHED_STATE_CAP (%d) exceeded\n",
                NOVA_SCHED_STATE_CAP);
        abort();
    }
    st = &_nova_sched_states[_nova_sched_state_count++];
    st->scope = scope;
    for (int i = 0; i < NOVA_SCOPE_CAP; i++) {
        st->parked[i] = false;
        st->pending_handle[i] = NULL;
        st->pending_stop_cb[i] = NULL;
    }
    return st;
}

/* Drop state for scope. Called from supervised_run end. */
static inline void nova_sched_drop_state(NovaFiberQueue* scope) {
    for (int i = 0; i < _nova_sched_state_count; i++) {
        if (_nova_sched_states[i].scope == scope) {
            /* Compact: shift remaining states down. */
            for (int j = i + 1; j < _nova_sched_state_count; j++) {
                _nova_sched_states[j-1] = _nova_sched_states[j];
            }
            _nova_sched_state_count--;
            return;
        }
    }
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

/* Wake parked fiber. Idempotent. */
static inline void nova_sched_wake(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) return;
    NovaSchedState* st = nova_sched_find_state(scope);
    if (st) st->parked[slot] = false;
}

/* True если fiber в slot сейчас parked. */
static inline nova_bool nova_sched_is_parked(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) return false;
    NovaSchedState* st = nova_sched_find_state(scope);
    return st && st->parked[slot];
}

/* ─── Cancel-integration ──────────────────────────────────────── */

static inline void nova_sched_register_pending(NovaFiberQueue* scope, int slot,
                                                void* handle,
                                                NovaSchedStopCb stop_cb) {
    if (!scope || slot < 0 || slot >= NOVA_SCOPE_CAP) return;
    NovaSchedState* st = nova_sched_get_state(scope);
    st->pending_handle[slot] = handle;
    st->pending_stop_cb[slot] = stop_cb;
}

static inline void nova_sched_unregister_pending(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= NOVA_SCOPE_CAP) return;
    NovaSchedState* st = nova_sched_find_state(scope);
    if (st) {
        st->pending_handle[slot] = NULL;
        st->pending_stop_cb[slot] = NULL;
    }
}

/* ─── Cancel-flow integration: вызывается из nova_cancel_token_cancel ── */

/* Trigger all pending stop_cb's for scope. После этого все parked
 * fiber'ы должны be wake'ed (либо stop_cb сделал wake, либо мы здесь). */
static inline void nova_sched_cancel_all_pending(NovaFiberQueue* scope) {
    NovaSchedState* st = nova_sched_find_state(scope);
    if (!st) return;
    for (int i = 0; i < scope->count; i++) {
        if (st->pending_stop_cb[i] && st->pending_handle[i]) {
            st->pending_stop_cb[i](st->pending_handle[i]);
        }
        st->parked[i] = false;  /* unpark */
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
    for (int i = 0; i < scope->count; i++) {
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
