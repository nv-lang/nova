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
    nova_atomic_int* new_pwake = (nova_atomic_int*)nova_alloc(sizeof(nova_atomic_int) * cap);
    /* Copy existing + init new. */
    for (int i = 0; i < cap; i++) {
        if (i < st->capacity && st->parked) {
            new_parked[i] = st->parked[i];
            new_handle[i] = st->pending_handle[i];
            new_stop_cb[i] = st->pending_stop_cb[i];
            new_pwake[i] = st->pending_wake ? st->pending_wake[i] : 0;
        } else {
            new_parked[i] = false;
            new_handle[i] = NULL;
            new_stop_cb[i] = NULL;
            new_pwake[i] = 0;
        }
    }
    st->parked = new_parked;
    st->pending_handle = new_handle;
    st->pending_stop_cb = new_stop_cb;
    st->pending_wake = new_pwake;
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
    st->pending_wake = NULL;
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
    mco_coro* co = mco_running();
    if (!co) {
        fprintf(stderr, "nova: nova_sched_park: not in fiber context\n");
        abort();
    }

    /* Plan 83.11 Ф.3.A v3 (Option A): wake-before-park race fix через
     * pending_wake counter. Two CAS checkpoints around SEQ_CST parked store:
     *
     *   t1: pre-park CAS pending_wake 1→0 — wake event already pending → consume + return
     *   t2: SEQ_CST parked=true — commit park (full memory fence)
     *   t3: post-barrier CAS pending_wake 1→0 — wake came в barrier window → undo parked + return
     *   t4: mco_yield — wait for wake's dispatch
     *
     * Wake side (nova_sched_wake): CAS pending_wake 0→1 (deliver) THEN CAS
     * parked true→false (try dispatch). If wake fires before park's t2: CAS
     * parked fails (parked still false), but pending_wake=1; worker's t3
     * catches it. If wake fires after t2: CAS parked succeeds → dispatch.
     * No stuck-fiber scenario.
     *
     * pending_wake[] array allocated в NovaSchedState (lazy grow alongside parked[]). */
    if (st->pending_wake && slot < st->capacity) {
        int32_t expected = 1;
        if (__atomic_compare_exchange_n(
                &st->pending_wake[slot], &expected, 0,
                false, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
            return;
        }
    }

    /* SEQ_CST store: on x86 compiles to XCHG (full fence). */
    __atomic_store_n((volatile bool*)&st->parked[slot], true, __ATOMIC_SEQ_CST);

    /* Post-barrier recheck: wake came в barrier window? */
    if (st->pending_wake && slot < st->capacity) {
        int32_t expected = 1;
        if (__atomic_compare_exchange_n(
                &st->pending_wake[slot], &expected, 0,
                false, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
            /* Wake came в barrier window — undo parked, return. */
            __atomic_store_n((volatile bool*)&st->parked[slot], false, __ATOMIC_SEQ_CST);
            return;
        }
    }

    /* Plan 83.4.5.7 (2026-05-23): RUNNING → PARKED state transition. */
    nova_fiber_state_store(co, NOVA_FIBER_STATE_PARKED);

    /* Plan 83.11 t4-race-fix: nova_sched_wake can win parked CAS between t2
     * and here, setting state=IDLE and dispatching us to a worker deque.
     * Our PARKED store above overrides wake's IDLE → dispatch CAS(IDLE→RUNNING)
     * fails → fiber stuck in mco_yield forever.
     *
     * Fix: reload parked[slot] with SEQ_CST (flushes our PARKED store).
     * If false, wake already won → it set IDLE → restore IDLE so CAS succeeds.
     * The runtime.c post-resume check (nova_fiber_state_load == IDLE) then
     * skips the yielded-FIFO push, preventing double-queue. */
    if (!__atomic_load_n((volatile bool*)&st->parked[slot], __ATOMIC_SEQ_CST)) {
        nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
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
    /* SEQ_CST store: emits XCHG on x86 (full fence — drains store buffer).
     * Identical rationale to nova_sched_park above: RELEASE is a plain MOV
     * on x86 and may leave the store in the CPU store buffer, causing a
     * concurrent waker's CAS to see stale false → no dispatch → deadlock.
     * SEQ_CST guarantees the store is globally visible before this returns. */
    __atomic_store_n((volatile bool*)&st->parked[slot], true, __ATOMIC_SEQ_CST);
    mco_coro* co = mco_running();
    if (!co) {
        fprintf(stderr, "nova: nova_sched_park_with_unlock: not in fiber context\n");
        abort();
    }
    /* Plan 44.5 Layer 5 deferred-unlock (M:N correctness):
     *
     * Store unlock fn/arg in thread-local instead of calling immediately.
     * The scheduler (worker loop in runtime.c or nova_supervised_step) will:
     *   1. Call mco_resume(co) — fiber sets parked[slot]=true, stores fn, yields.
     *   2. Check nova_sched_is_parked WHILE fn is not yet called (mutex still held).
     *   3. Call fn(arg) — releases mutex — only now can a cross-thread sender wake us.
     *   4. Since parked check happened at step 2, the worker correctly sees parked=true
     *      and does NOT re-push. Only dispatch_ready (from nova_sched_wake) re-queues.
     *
     * Without this, a race exists: sender could call nova_sched_wake (clearing
     * parked[slot]) BEFORE mco_yield completes, causing the worker to re-push the
     * fiber to its deque AND wake_pending → double-push → double-resume → crash. */
    _nova_park_unlock_fn  = unlock_fn;
    _nova_park_unlock_arg = unlock_arg;
    /* Plan 83.4.5.7 (2026-05-23): RUNNING → PARKED. Set BEFORE yield so что
     * wake-сайт (CAS PARKED→IDLE) видит правильное state. Order:
     *   1. Store PARKED (RELEASE).
     *   2. mco_yield — control returns to scheduler.
     *   3. Scheduler calls unlock_fn — only NOW cross-thread waker can run.
     *   4. Waker CAS PARKED→IDLE: видит state'у published в шаге 1. */
    nova_fiber_state_store(co, NOVA_FIBER_STATE_PARKED);
    mco_yield(co);
    /* Fiber resumed: clear deferred state (scheduler already called fn). */
    _nova_park_unlock_fn  = NULL;
    _nova_park_unlock_arg = NULL;
}

/* Wake parked fiber. Idempotent. Безопасно вызывать из libuv-callback'а.
 *
 * Plan 44.5 Layer 5 park/wake: если scope->dispatch_ready != NULL (M:N
 * worker scope), вызывает его чтобы re-queue fiber в worker deque.
 * Single-thread: dispatch_ready == NULL — main loop resume'ит fiber сам.
 *
 * Plan 83.4.5.7 (2026-05-23): atomic CAS guard PARKED→IDLE. Только
 * winner вызывает dispatch_ready. Защищает от double-wake → double-push →
 * double-resume race. Без guard'а:
 *   T1: close_cb fires → nova_sched_wake → parked[slot]=false → push.
 *   T2: cancel_wake_all reads stale parked[slot]=true → wake → push AGAIN.
 *   Worker pops twice → concurrent mco_resume → fiber arena slot
 *   corruption (Windows TIB swap conflict / POSIX context double-swap).
 *
 * Bootstrap (single-thread): без atomic тоже работает (один thread). CAS
 * стоит ~10ns — приемлемо для wake hot path. */
static inline void nova_sched_wake(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) return;
    NovaSchedState* st = nova_sched_find_state(scope);

    /* Plan 83.11 Ф.3.A v3 (Option A): deliver pending_wake event FIRST.
     * Park side (nova_sched_park) checks pending_wake before yielding —
     * closes wake-before-park race. CAS 0→1 is idempotent (multiple wakes
     * coalesce into one delivered event).
     *
     * If CAS fails (pending_wake already 1), it means previous wake event
     * не consumed by park yet — park will consume both на next iteration
     * (counter saturates at 1 for now; could be N для multi-event later). */
    if (st && st->pending_wake && slot < st->capacity) {
        int32_t expected = 0;
        __atomic_compare_exchange_n(
            &st->pending_wake[slot], &expected, 1,
            false, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE);
        /* Don't care if it failed — wake already pending, idempotent. */
    }

    /* Plan 83.4.5.7 (2026-05-23): atomic-bool exchange parked flag.
     * Only winner CAS true→false делает dispatch. */
    bool was_parked = false;
    if (st && slot < st->capacity) {
        bool expected = true;
        was_parked = __atomic_compare_exchange_n(
            (volatile bool*)&st->parked[slot], &expected, false,
            false, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE);
    }

    /* If we won parked CAS — consume the pending_wake we just delivered
     * (we're about to dispatch the fiber; park will not check pending_wake
     * again because fiber resumes via dispatch, not retry-park). */
    if (was_parked && st && st->pending_wake && slot < st->capacity) {
        __atomic_store_n(&st->pending_wake[slot], 0, __ATOMIC_RELEASE);
    }
    /* M:N dispatch: push woken fiber back to worker deque.
     * Под bootstrap dispatch_ready==NULL — pure parked-flag clear. */
    if (was_parked && scope->dispatch_ready && slot < scope->count) {
        mco_coro* co = scope->fibers[slot];
        if (co && mco_status(co) != MCO_DEAD) {
            nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
            scope->dispatch_ready(scope->dispatch_ctx, co);
        }
    } else if (!was_parked) {
        /* parked was already false — может быть double-wake. Skip dispatch. */
    } else {
        /* Bootstrap path: was_parked=true but dispatch_ready=NULL.
         * supervised_step видит cleared parked[i] и resume'ит fiber.
         * Sync state для consistency. */
        if (slot < scope->count) {
            mco_coro* co = scope->fibers[slot];
            if (co && mco_status(co) != MCO_DEAD) {
                nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
            }
        }
    }
}

/* True если fiber в slot сейчас parked. */
static inline nova_bool nova_sched_is_parked(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) return false;
    NovaSchedState* st = nova_sched_find_state(scope);
    return st && slot < st->capacity && st->parked[slot];
}

/* ─── Plan 83.4.1 (2026-05-23): park-with-predicate ──────────────
 *
 * Industry-standard pattern для spurious-wake-resilient park:
 * POSIX `pthread_cond_wait(cv, mu)` + caller-loop,
 * C++ `std::condition_variable::wait(lock, pred)`,
 * Go runtime `gopark(unlockf, ...)`,
 * tokio `Notify` + `Notified::poll`.
 *
 * Контракт: park возвращается **только** когда `pred(ctx)` вернёт `true`.
 * Spurious wake (включая M:N drain-quiescence-wake до завершения
 * close_cb для D93-async-handle'ов) автоматически re-park'ится в loop'е.
 *
 * Memory ordering: предикат-функция ОБЯЗАНА читать опубликованное
 * состояние с ACQUIRE-ordering (через `nova_aint_load(...,ACQUIRE)` и
 * аналоги), а wake-сайт ОБЯЗАН опубликовать «predicate-affecting»
 * состояние с RELEASE-ordering ДО `nova_sched_wake`. Park-instance
 * (`nova_sched_park`) делает `mco_yield`, который flush'ит регистры
 * и работает как compiler-barrier.
 *
 * Fast-path: если `pred` уже вернул `true` при входе, park не делается.
 *
 * Если `pred == NULL` — legacy single-shot park (равно `nova_sched_park`).
 * Это для backward-compat сайтов, где caller сам делает predicate-recheck. */

typedef nova_bool (*NovaParkPredicate)(void* ctx);

static inline void nova_sched_park_until(NovaFiberQueue* scope, int slot,
                                          NovaParkPredicate pred, void* ctx) {
    if (pred && pred(ctx)) return;       /* fast-path: condition уже выполнено */
    for (;;) {
        nova_sched_park(scope, slot);
        if (!pred) return;               /* legacy single-shot */
        if (pred(ctx)) return;           /* predicate satisfied */
        /* spurious wake → re-park */
    }
}

/* ─── Cancel-integration ──────────────────────────────────────── */

/* Регистрация handle + stop_cb для cancel-wake. ОБЯЗАТЕЛЬНО перед park'ом
 * для cancel-correctness (D93 contract). Lazy-allocates sched_state.
 *
 * Memory ordering (Plan 83.10.2 fix, iteration 2 — SEQ_CST):
 *   SEQ_CST store on pending_stop_cb emits mfence/xchg on x86, flushing
 *   Thread A's store buffer. This makes BOTH pending_handle (plain store
 *   before the atomic) AND pending_stop_cb globally visible before
 *   register_pending returns. The ACQUIRE-load in cancel_worker_fibers
 *   then sees both fields correctly.
 *
 *   RELEASE was insufficient on x86: __ATOMIC_RELEASE compiles to a
 *   plain store + compiler barrier (no mfence). Stores may remain in
 *   Thread A's store buffer and not be visible to Thread B's ACQUIRE
 *   load. This is NOT a happens-before violation in the C memory model —
 *   if Thread B reads before Thread A's store is committed, the C model
 *   says the behaviour is well-defined (Thread B simply sees the old
 *   value). But that old value (NULL) causes the cancel to miss the
 *   stop_cb.
 *
 *   Background: pending_stop_cb is written AFTER nova_scope_alloc_slot's
 *   RELEASE store of count. The ACQUIRE on count in cancel_worker_fibers
 *   only synchronises writes that precede the count RELEASE — not these
 *   later writes. Without SEQ_CST on pending_stop_cb the read in
 *   cancel_worker_fibers can return NULL when stores are still in the
 *   store buffer (classic "heisenbug" cured by fprintf's implicit
 *   full-fence via stdio mutex → mfence). */
static inline void nova_sched_register_pending(NovaFiberQueue* scope, int slot,
                                                void* handle,
                                                NovaSchedStopCb stop_cb) {
    if (!scope || slot < 0) return;
    NovaSchedState* st = nova_sched_get_state(scope);
    if (slot >= st->capacity) {
        nova_sched_grow_state(scope, slot + 1);
    }
    /* Write handle BEFORE stop_cb with SEQ_CST so the mfence/xchg on x86
     * flushes Thread A's store buffer, making pending_handle AND pending_stop_cb
     * globally visible before register_pending returns. RELEASE was insufficient
     * on x86 (compiler-only barrier, no mfence — store stays in store buffer).
     * The ACQUIRE-load in cancel_worker_fibers then correctly sees both fields.
     * Plan 83.10.2 fix iteration 2. */
    st->pending_handle[slot] = handle;
    __atomic_store(&st->pending_stop_cb[slot], &stop_cb, __ATOMIC_SEQ_CST);
}

/* Снять регистрацию. Должно вызываться ПОСЛЕ wake (любой — normal либо
 * cancel), перед cancel-check. Idempotent.
 *
 * Memory ordering: RELEASE-clear of stop_cb ensures a concurrent
 * cancel_worker_fibers reading ACQUIRE sees NULL and skips this slot,
 * preventing a stale call after the fiber has already resumed. */
static inline void nova_sched_unregister_pending(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0) return;
    NovaSchedState* st = nova_sched_find_state(scope);
    if (st && slot < st->capacity) {
        NovaSchedStopCb _null_cb = NULL;
        __atomic_store(&st->pending_stop_cb[slot], &_null_cb, __ATOMIC_RELEASE);
        st->pending_handle[slot] = NULL;
    }
}

/* ─── Cancel wake-all (Plan 83.4.5.1, 2026-05-23) ─────────────────────
 *
 * Walks scope's slots; для каждого parked fiber'а вызывает
 * `nova_sched_wake` который clears parked-flag + (если scope под M:N)
 * вызывает `dispatch_ready` callback для re-queue в worker deque.
 *
 * Применяется ПОСЛЕ `nova_sched_cancel_all_pending` в cancel-flow для
 * обработки слотов, где stop_cb был SYNC либо bare-park (parked-флаг
 * cleared cancel_all_pending'ом, но dispatch_ready НЕ был вызван →
 * worker под M:N не знает что fiber готов к re-pop).
 *
 * Также покрывает fiber'ы parked через `nova_sched_park_until` predicate
 * loop'е без registered stop_cb — `_is_done_or_cancelled` predicate на
 * след. iter увидит `cancel_requested` и вернёт true → park exit'ит.
 *
 * Idempotent: parked[slot]==false на момент входа → no-op для этого slot'а.
 *
 * Memory ordering: caller (nova_cancel_token_cancel_reason) публикует
 * `cancel_requested = true` через atomic-store (Plan 83.4.3 B5)
 * ДО этого вызова. Каждый fiber on park-loop check'ает
 * `cancel_requested` ACQUIRE-load'ом → видит флаг.
 *
 * Cross-runtime parity:
 *   - Go `context/context.go::cancelCtx.cancel` — closes `done` channel
 *     (broadcast to all waiters).
 *   - tokio `CancellationTokenState::cancel` → `notify_waiters()` —
 *     unparks all wakers зарегистрированных через `cancelled().await`.
 *   - Kotlin `JobSupport.cancelInternal` — iterates child list, cancels each.
 */
static inline void nova_scope_cancel_wake_all(NovaFiberQueue* scope) {
    if (!scope) return;
    NovaSchedState* st = nova_sched_find_state(scope);
    if (!st) return;
    int n = scope->count < st->capacity ? scope->count : st->capacity;
    for (int i = 0; i < n; i++) {
        /* Idempotent: parked[slot]==false → wake — no-op (clears flag
         * commit, dispatch_ready возможно re-push'ит running fiber'а
         * в deque, но Chase-Lev push owner-thread'ом — safe). nova_sched_wake
         * проверяет mco_status != MCO_DEAD ДО dispatch — terminated fibers
         * skip'нутся.
         *
         * Под bootstrap (scope->dispatch_ready == NULL) — pure flag-clear.
         * Под M:N (dispatch_ready != NULL) — push в worker deque. */
        if (st->parked[i]) {
            nova_sched_wake(scope, i);
        }
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
        NovaSchedStopCb _cb;
        __atomic_load(&st->pending_stop_cb[i], &_cb, __ATOMIC_ACQUIRE);
        void* _hdl = st->pending_handle[i];  /* visible after ACQUIRE on _cb */
        if (_cb && _hdl) {
            NovaStopMode mode = _cb(_hdl);
            if (mode == NOVA_STOP_SYNC) {
                /* SYNC: unpark immediate + dispatch_ready re-queue в worker
                 * deque (если scope под M:N). Plan 83.4.5.1 fix: до этого
                 * только parked[i]=false — worker не знал, fiber остался
                 * не-re-popped → drain hang. nova_sched_wake чистит флаг +
                 * dispatch'ит. */
                nova_sched_wake(scope, i);
            }
            /* ASYNC: НЕ unpark'аем — backend сделает wake через close_cb
             * (для sleep) либо waitlist-removal callback (для channels). */
        } else if (st->parked[i]) {
            /* Park без registered stop_cb (bare park) — unpark
             * unconditional + dispatch_ready. */
            nova_sched_wake(scope, i);
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
