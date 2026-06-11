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

/* Plan 22 Ф.7 / Plan 83-go-cmn Ф.1b: grow sched_state до new_cap.
 *
 * Ф.1b: NEVER realloc. Growth = ADD MISSING CHUNKS only. Existing chunks and
 * their elements are NEVER touched, so a concurrent reader holding
 * &parked[slot] is never invalidated (this is what closes the torn-pointer
 * grow-vs-wake race — the old realloc + pointer-swap is deleted).
 *
 * BLOCKER 3 (concurrency): grow is NOT single-writer. nova_scope_alloc_slot
 * (fibers.h) holds slot_lock, but nova_fiber_spawn_into, nova_sched_get_state
 * and nova_sched_register_pending all grow WITHOUT slot_lock — two threads
 * can attempt to publish the SAME chunk index concurrently. We therefore
 * publish each chunk with a CAS (directory[c]: NULL → new_chunk). The CAS
 * winner's chunk becomes canonical; a loser frees its own freshly-allocated
 * chunk and uses the winner's. This makes chunk publication idempotent and
 * safe for concurrent growers without any lock. capacity is RELEASE-stored
 * AFTER all chunk pointers are published, so a reader observing slot<capacity
 * (with ACQUIRE in the accessor) is guaranteed to see a published, fully
 * zero-inited chunk. */
static inline void nova_sched_grow_state(NovaFiberQueue* scope, int new_cap) {
    if (!scope || !scope->sched_state) return;
    NovaSchedState* st = scope->sched_state;
    if (new_cap <= st->capacity) return;
    /* Round target up to a whole number of chunks (keeps the old capacity-
     * doubling spirit: capacity is always a multiple of CHUNK). */
    int cap = st->capacity > 0 ? st->capacity : NOVA_SCOPE_INITIAL_CAP;
    while (cap < new_cap) cap *= 2;
    int needed_chunks = ((cap - 1) >> NOVA_SCHED_CHUNK_SHIFT) + 1;
    if (needed_chunks > NOVA_SCHED_MAX_CHUNKS) {
        fprintf(stderr,
            "nova: nova_sched_grow_state: scope slot count exceeds ceiling "
            "(%d chunks > %d max = %d slots)\n",
            needed_chunks, NOVA_SCHED_MAX_CHUNKS,
            NOVA_SCHED_MAX_CHUNKS * NOVA_SCHED_CHUNK);
        abort();
    }
    /* Allocate + zero-init + CAS-publish any chunk index that is not yet
     * published. We scan the whole [0, needed_chunks) range (not just from the
     * current chunk count) so concurrent growers converge regardless of who
     * published which index first. */
    for (int c = 0; c < needed_chunks; c++) {
        if (__atomic_load_n(&st->parked_chunks[c], __ATOMIC_ACQUIRE) != NULL) {
            continue;  /* already published by us or a peer */
        }
        nova_bool*       nc_parked  = (nova_bool*)nova_alloc(sizeof(nova_bool) * NOVA_SCHED_CHUNK);
        void**           nc_handle  = (void**)nova_alloc(sizeof(void*) * NOVA_SCHED_CHUNK);
        NovaSchedStopCb* nc_stop_cb = (NovaSchedStopCb*)nova_alloc(sizeof(NovaSchedStopCb) * NOVA_SCHED_CHUNK);
        /* Plan 83-go-cmn Ф.2: parked_co directory replaces the deleted pending_wake. */
        mco_coro**       nc_pco     = (mco_coro**)nova_alloc(sizeof(mco_coro*) * NOVA_SCHED_CHUNK);
        if (!nc_parked || !nc_handle || !nc_stop_cb || !nc_pco) {
            fprintf(stderr, "nova: nova_sched_grow_state: nova_alloc failed\n");
            abort();
        }
        for (int o = 0; o < NOVA_SCHED_CHUNK; o++) {
            nc_parked[o]  = false;
            nc_handle[o]  = NULL;
            nc_stop_cb[o] = NULL;
            nc_pco[o]     = NULL;
        }
        /* CAS-publish parked_chunks[c] first; that single index is the
         * "ownership token" for this chunk index. Whoever wins it publishes
         * its sibling chunks (handle/stop_cb/parked_co) too. RELEASE on success
         * so the zero-inited elements are visible to an ACQUIRE reader. */
        nova_bool* expected_null = NULL;
        if (__atomic_compare_exchange_n(
                &st->parked_chunks[c], &expected_null, nc_parked,
                false, __ATOMIC_RELEASE, __ATOMIC_ACQUIRE)) {
            /* Won this chunk index: publish the 3 siblings (RELEASE). */
            __atomic_store_n(&st->pending_handle_chunks[c], nc_handle, __ATOMIC_RELEASE);
            __atomic_store_n(&st->pending_stop_cb_chunks[c], nc_stop_cb, __ATOMIC_RELEASE);
            __atomic_store_n(&st->parked_co_chunks[c], nc_pco, __ATOMIC_RELEASE);
        }
        /* Lost the CAS (peer published first) OR sibling slots may already be
         * populated by the winner: in either case our 4 freshly-allocated
         * chunks for this index are now redundant. They are plain nova_alloc
         * (collectable) blocks with no remaining references → GC reclaims them;
         * no explicit free needed (and nova_alloc has no matching free in this
         * GC'd runtime). We simply drop them and let the published chunk win. */
    }
    /* Publish capacity LAST (RELEASE) — pairs with the accessor's ACQUIRE +
     * the caller's slot<capacity guard. capacity = published chunk count<<SHIFT
     * can never exceed NOVA_SCHED_MAX_CHUNKS<<SHIFT by construction. */
    __atomic_store_n(&st->capacity, needed_chunks << NOVA_SCHED_CHUNK_SHIFT, __ATOMIC_RELEASE);
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
    /* Ф.1b: NovaSchedState is nova_alloc'd (zeroed per alloc.h contract), so
     * all 4 chunk directories are already all-NULL and capacity==0. No
     * explicit field init needed beyond documenting the invariant. */
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

/* ─── Plan 83-go-cmn Ф.2: gopark / goready (Go-style by-co handshake) ──
 *
 * The single-winner election and the ready-before-park latch both live on
 * the per-fiber `_nova_park_state` (NIL/WAIT/READY/DISPATCHED), addressed BY
 * co-pointer. This REPLACES the entire Plan 83.11 pending_wake t1/t3 dance +
 * the parked[] CAS dispatch gate. parked[] is DEMOTED to a cancel-reachability
 * filter; parked_co[] carries the genuinely-parked co for cancel-by-co.
 *
 * See fence_plan in tmp_f2_design.json for the full lost-wakeup-free proof.
 * Ordering summary:
 *   gopark:  G0 fiber_state PARKED (RELEASE) → G1 park_state WAIT (SEQ_CST,
 *            publishes PARKED too) → G2 stash unlockf TLS → G3 commit-recheck
 *            CAS READY->DISPATCHED (ACQ_REL/ACQUIRE): success ⇒ ready-before-park
 *            (run unlockf INLINE + clear unlockf TLS + restore IDLE + RETURN,
 *            no yield); failure ⇒ ALWAYS mco_yield.
 *   goready: R1 base=user_data → R2 CAS WAIT->DISPATCHED (winner: fiber_state
 *            PARKED->IDLE, clear parked[slot], dispatch_ready(co)) | R3 CAS
 *            NIL->READY (ready-before-park latch) | R4 else no-op (idempotent).
 *
 * NOTE: the unlockf is run by the SCHEDULER (runtime.c / supervised_step) AFTER
 * the fiber yields — gopark only STASHES it (G2). G1=WAIT (SEQ_CST) is the
 * publish that orders against the unlock; gopark itself does NOT unlock on the
 * yield path (only on the G3 ready-before-park fast-path, where it self-drains
 * the single non-stack TLS slot to avoid a stale double-unlock). */

/* nova_goready(co): the by-co single-winner wake. Idempotent. Safe to call
 * from any thread / libuv callback. `co` MUST be the genuinely-parked fiber
 * (resolved via parked_co[] or held by the primitive's waiter), NEVER
 * re-derived from scope->fibers[slot] on the cancel path. */
static inline void nova_goready(mco_coro* co) {
    if (!co) return;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return;  /* legacy fiber без base — no latch; cannot have gopark'd */

    /* R2: sole-dispatcher election. ACQ_REL on success, ACQUIRE on failure. */
    int32_t expected = NOVA_PARK_WAIT;
    if (__atomic_compare_exchange_n(&base->_nova_park_state, &expected,
                                    NOVA_PARK_DISPATCHED, false,
                                    __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        /* WON WAIT->DISPATCHED. Consistency order (review correction #4):
         *   (2) fiber_state PARKED->IDLE
         *   (3) clear parked[slot] (cancel-reachability bit) + parked_co[slot]
         *   (4) dispatch_ready(co)
         * Liveness gates that read parked[] must treat park_state==DISPATCHED as
         * 'alive, requeue-in-flight', NOT 'gone' (correction #4 / supervised_step). */
        NovaFiberQueue* scope = base->_nova_fiber_scope;
        int slot = base->_nova_worker_slot;
        if (mco_status(co) != MCO_DEAD) {
            nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
        }
        if (scope && slot >= 0) {
            NovaSchedState* st = nova_sched_find_state(scope);
            if (st && slot < nova_sched_cap_acq(st)) {
                /* Clear the cancel-reachability bit AFTER winning the election but
                 * BEFORE dispatch (correction #4). SEQ_CST keeps it ordered w.r.t.
                 * a concurrent alloc_slot skip-stale read + a racing cancel walk. */
                bool exp_t = true;
                __atomic_compare_exchange_n(
                    (volatile bool*)nova_sched_parked_at(st, slot), &exp_t, false,
                    false, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE);
                mco_coro** pco = nova_sched_parked_co_at(st, slot);
                if (pco) __atomic_store_n(pco, (mco_coro*)NULL, __ATOMIC_RELEASE);
            }
        }
        if (scope && scope->dispatch_ready && mco_status(co) != MCO_DEAD) {
            scope->dispatch_ready(scope->dispatch_ctx, co);
        }
        /* Bootstrap (dispatch_ready==NULL): supervised_step sees parked[slot]
         * cleared + fiber_state IDLE and resumes the fiber itself. */
        return;
    }

    /* R3: ready-before-park latch. NIL->READY (ACQ_REL). gopark's G3 recheck
     * will consume it and return without yielding. */
    expected = NOVA_PARK_NIL;
    if (__atomic_compare_exchange_n(&base->_nova_park_state, &expected,
                                    NOVA_PARK_READY, false,
                                    __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        return;
    }

    /* R4: observed READY or DISPATCHED — idempotent no-op (double-goready
     * coalesces; the prior dispatcher already re-queued or latched). */
}

/* nova_gopark(unlock_fn, unlock_arg): commit the current fiber to a wait.
 * Returns when a peer/cancel goready dispatches us (or immediately, on the
 * ready-before-park fast-path). MUST be called from fiber context.
 *
 * unlock_fn/unlock_arg (may be NULL) is the lock to release AFTER the fiber
 * has parked — stashed in TLS and drained by the scheduler on the yield path,
 * or run INLINE here on the ready-before-park fast-path. */
static inline void nova_gopark(void (*unlock_fn)(void*), void* unlock_arg) {
    mco_coro* co = mco_running();
    if (!co) {
        fprintf(stderr, "nova: nova_gopark: not in fiber context\n");
        abort();
    }
    /* G0: fiber_state RUNNING -> PARKED (RELEASE). Ordered BEFORE G1 so the
     * SEQ_CST G1 store publishes PARKED too (single coherent observation point
     * for goready's R2 PARKED->IDLE — review fence_hazard #1). */
    nova_fiber_state_store(co, NOVA_FIBER_STATE_PARKED);
    /* G1: park_state -> WAIT (SEQ_CST = XCHG on x86, full fence, drains store
     * buffer). The load-bearing publish: globally visible before any waker that
     * later acquires the resource lock (released by unlock_fn post-yield). */
    nova_park_state_store(co, NOVA_PARK_WAIT, __ATOMIC_SEQ_CST);
    /* G2: stash deferred-unlock in TLS — the scheduler drains it AFTER the fiber
     * yields (runtime.c / supervised_step), so the unlock (and thus a peer's
     * waker reachability) happens strictly after G1's WAIT is visible. */
    _nova_park_unlock_fn  = unlock_fn;
    _nova_park_unlock_arg = unlock_arg;
    /* G3: commit-recheck. A goready that latched NIL->READY before G1 (or raced
     * between G1 and now) is caught here: CAS READY->DISPATCHED. */
    if (nova_park_state_cas(co, NOVA_PARK_READY, NOVA_PARK_DISPATCHED,
                            __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        /* Ready-before-park: do NOT yield. The scheduler never drains the TLS on
         * this path (no yield), so we MUST run the unlock inline AND clear the
         * single non-stack TLS slot to avoid a later stale double-unlock
         * (review correction #2 / lost_wake_hazard #2). */
        _nova_park_unlock_fn  = NULL;
        _nova_park_unlock_arg = NULL;
        nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
        if (unlock_fn) unlock_fn(unlock_arg);
        return;
    }
    /* G3 failed ⇒ state is WAIT (normal) or DISPATCHED (a peer goready already
     * won WAIT->DISPATCHED and re-queued us). In BOTH cases ALWAYS yield exactly
     * once: the dispatcher (self via READY above, or a peer via DISPATCHED) has
     * guaranteed a re-queue, so the next resume picks us up. Skipping the yield
     * on DISPATCHED would fall through while ALSO on the runq → double-run
     * (review fence_hazard #2). */
    mco_yield(co);
    /* Resumed. Clear the TLS the scheduler already drained (yield path). */
    _nova_park_unlock_fn  = NULL;
    _nova_park_unlock_arg = NULL;
    /* Reset the descriptor latch to NIL at the canonical end-of-wait (review
     * correction #3): a late/duplicate cross-thread waker that latched NIL->READY
     * after we resumed would otherwise make our NEXT gopark skip yielding. */
    nova_park_state_store(co, NOVA_PARK_NIL, __ATOMIC_RELEASE);
}

/* ─── (scope,slot)-addressed park/wake shims over gopark/goready ──────
 *
 * These keep the existing (scope,slot) call surface (channels, sync, sleep,
 * select, net.c, driver, cancel) unchanged — only the WAKE LVALUE is
 * substituted (nova_sched_wake → nova_goready). They (a) publish parked[slot]
 * (cancel-reachability) + parked_co[slot] (cancel-by-co carrier) before parking,
 * and (b) route the wake by-co. The single-winner election + ready-before-park
 * latch are entirely on _nova_park_state inside gopark/goready. */

/* Set the per-slot cancel-reachability bit + the parked_co carrier for `co`.
 * Must run BEFORE the gopark commit so a racing cancel walk can resolve co.
 * parked[] SEQ_CST mirrors the old commit fence; parked_co[] is RELEASE
 * (read with ACQUIRE on the cancel/wake path; the WAIT election still guards
 * races). */
static inline void _nova_park_mark_slot(NovaFiberQueue* scope, int slot, mco_coro* co) {
    NovaSchedState* st = nova_sched_get_state(scope);
    if (slot < nova_sched_cap_acq(st)) {
        mco_coro** pco = nova_sched_parked_co_at(st, slot);
        if (pco) __atomic_store_n(pco, co, __ATOMIC_RELEASE);
    }
    __atomic_store_n((volatile bool*)nova_sched_parked_at(st, slot), true, __ATOMIC_SEQ_CST);
}

/* Clear the per-slot cancel-reachability bit on the normal (self) return path;
 * the goready winner already cleared it on the wake path (idempotent here). */
static inline void _nova_park_clear_slot(NovaFiberQueue* scope, int slot) {
    NovaSchedState* st = nova_sched_find_state(scope);
    if (st && slot < nova_sched_cap_acq(st)) {
        bool exp_t = true;
        __atomic_compare_exchange_n(
            (volatile bool*)nova_sched_parked_at(st, slot), &exp_t, false,
            false, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE);
    }
}

/* Park current fiber on (scope, slot). Returns when nova_sched_wake(scope,slot)
 * (or a cancel goready) dispatches us. Не вызывать из не-fiber кода. */
static inline void nova_sched_park(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) {
        fprintf(stderr, "nova: nova_sched_park: invalid scope/slot\n");
        abort();
    }
    mco_coro* co = mco_running();
    if (!co) {
        fprintf(stderr, "nova: nova_sched_park: not in fiber context\n");
        abort();
    }
    _nova_park_mark_slot(scope, slot, co);
    nova_gopark(NULL, NULL);
    _nova_park_clear_slot(scope, slot);
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
    mco_coro* co = mco_running();
    if (!co) {
        fprintf(stderr, "nova: nova_sched_park_with_unlock: not in fiber context\n");
        abort();
    }
    /* Mark the slot (parked[] + parked_co[]) WHILE the resource lock is still
     * held by the caller (gopark releases it only post-yield). A racing waker
     * thus observes parked_co before it can reach us via the unlocked resource. */
    _nova_park_mark_slot(scope, slot, co);
    nova_gopark(unlock_fn, unlock_arg);
    _nova_park_clear_slot(scope, slot);
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
    if (!st || slot >= nova_sched_cap_acq(st)) return;
    /* Resolve the GENUINELY-parked co via parked_co[slot] (set at gopark, by
     * mco_running) — NOT scope->fibers[slot], which may be NULL'd-but-alive
     * (alloc_slot skip-stale) or reused (review correction #1). Funnel through
     * nova_goready, the by-co single-winner. */
    mco_coro** pco = nova_sched_parked_co_at(st, slot);
    mco_coro* co = pco ? __atomic_load_n(pco, __ATOMIC_ACQUIRE) : NULL;
    if (co) nova_goready(co);
}

/* True если fiber в slot сейчас parked. */
static inline nova_bool nova_sched_is_parked(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) return false;
    NovaSchedState* st = nova_sched_find_state(scope);
    return st && slot < nova_sched_cap_acq(st) && *nova_sched_parked_at(st, slot);
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
        /* Plan 83-go-cmn Ф.2: each nova_sched_park is a fresh gopark transaction.
         * gopark resets _nova_park_state to NIL on its normal (yield) return, so
         * a ready-before-park goready latched during THIS iteration is consumed
         * by gopark's G3 recheck (returns immediately) and the predicate re-check
         * below sees the published state — no lost spurious-wake, no stale READY
         * carried into the next iteration (review correction #3). */
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
    /* Ф.1b: pending_handle and pending_stop_cb now live in SEPARATE chunk
     * directories (different base arrays / cache lines). The SEQ_CST store on
     * pending_stop_cb still ORDERS THEM GLOBALLY: on x86 it emits mfence/xchg
     * which drains the WHOLE store buffer (not just one array), so the
     * preceding plain pending_handle store is globally visible before
     * register_pending returns. Do NOT colocate them or weaken the SEQ_CST. */
    *nova_sched_pending_handle_at(st, slot) = handle;
    __atomic_store(nova_sched_pending_stop_cb_at(st, slot), &stop_cb, __ATOMIC_SEQ_CST);
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
    if (st && slot < nova_sched_cap_acq(st)) {
        NovaSchedStopCb _null_cb = NULL;
        /* Correction #7: PRESERVE the SEQ_CST register-side store on pending_stop_cb;
         * the RELEASE-clear here is the mirror that lets a concurrent cancel ACQUIRE-
         * read NULL and skip this slot. Do NOT weaken either (Plan 83.10.2 heisenbug). */
        __atomic_store(nova_sched_pending_stop_cb_at(st, slot), &_null_cb, __ATOMIC_RELEASE);
        *nova_sched_pending_handle_at(st, slot) = NULL;
        /* Plan 83-go-cmn Ф.2 (replaces the deleted pending_wake reset — correction
         * #5/#3): clear the cancel-reachability bit + the parked_co carrier at the
         * canonical end-of-wait choke point. A cross-thread waker (driver Ф.3/Ф.4)
         * can fire AFTER the worker satisfied the done-predicate and returned via
         * the park_until fast-path (which never enters gopark, so its NIL-reset is
         * skipped). Stale parked[]/parked_co[] would make the NEXT wait on this slot
         * (same fiber's next chained blocking/sleep, or another fiber after slot
         * reuse) cancel-reachable for a wait that already completed. The fiber is
         * already running past its park, so dropping a concurrent wake is correct.
         * The descriptor latch (_nova_park_state) is reset to NIL inside gopark's
         * normal return; nothing slot-indexed remains for it to strand. */
        {
            bool exp_t = true;
            __atomic_compare_exchange_n(
                (volatile bool*)nova_sched_parked_at(st, slot), &exp_t, false,
                false, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE);
            mco_coro** pco = nova_sched_parked_co_at(st, slot);
            if (pco) __atomic_store_n(pco, (mco_coro*)NULL, __ATOMIC_RELEASE);
        }
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
    int _cap = nova_sched_cap_acq(st);
    int n = scope->count < _cap ? scope->count : _cap;
    for (int i = 0; i < n; i++) {
        /* Idempotent: parked[slot]==false → wake — no-op (clears flag
         * commit, dispatch_ready возможно re-push'ит running fiber'а
         * в deque, но Chase-Lev push owner-thread'ом — safe). nova_sched_wake
         * проверяет mco_status != MCO_DEAD ДО dispatch — terminated fibers
         * skip'нутся.
         *
         * Под bootstrap (scope->dispatch_ready == NULL) — pure flag-clear.
         * Под M:N (dispatch_ready != NULL) — push в worker deque. */
        if (*nova_sched_parked_at(st, i)) {
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
    int _cap = nova_sched_cap_acq(st);
    int n = scope->count < _cap ? scope->count : _cap;
    for (int i = 0; i < n; i++) {
        NovaSchedStopCb _cb;
        __atomic_load(nova_sched_pending_stop_cb_at(st, i), &_cb, __ATOMIC_ACQUIRE);
        void* _hdl = *nova_sched_pending_handle_at(st, i);  /* visible after ACQUIRE on _cb */
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
        } else if (*nova_sched_parked_at(st, i)) {
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
    int _cap = nova_sched_cap_acq(st);
    int n = scope->count < _cap ? scope->count : _cap;
    for (int i = 0; i < n; i++) {
        /* Plan 83-go-cmn Ф.2 review hardening: conjoin park_state==WAIT (as
         * supervised_step's liveness gates do). The transient window
         * parked[i]==true && park_state==DISPATCHED (goready won, re-queue in
         * flight) is "alive, not parked" — counting it as parked over-reports.
         * Introspection-only; does not affect park/wake correctness. */
        if (scope->fibers[i] != NULL
            && mco_status(scope->fibers[i]) != MCO_DEAD
            && *nova_sched_parked_at(st, i)
            && nova_park_state_load(scope->fibers[i]) == NOVA_PARK_WAIT) {
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
