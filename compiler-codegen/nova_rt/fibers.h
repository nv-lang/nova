#ifndef NOVA_RT_FIBERS_H
#define NOVA_RT_FIBERS_H

/* ---- Nova fiber runtime — wraps minicoro ----
 *
 * Design:
 *   spawn { body }  compiles to:
 *
 *     nova_fiber_result _r = nova_fiber_run(_nova_spawn_N, &_ctx_N);
 *
 *   where  _nova_spawn_N  is a file-scope function:
 *
 *     static void _nova_spawn_N(mco_coro* _co) {
 *         NovaSpawnCtx_N* _c = (NovaSpawnCtx_N*)mco_get_user_data(_co);
 *         nova_int _result = <body>;
 *         _c->result = _result;
 *     }
 *
 * `nova_fiber_run` creates the coroutine, resumes it to completion, then
 * returns the result stored in the ctx struct.  Because we call mco_resume
 * to completion (no yield in body), this is eager-synchronous — correct
 * semantics for Phase 5.  Cooperative yield can be added later.
 *
 * Result type: nova_int for now (most spawn bodies return int/unit).
 * The codegen stores the result as nova_int in the ctx.
 */

/* Pull in minicoro — define implementation in exactly one .c file. */
#ifndef MINICORO_INCLUDED_IMPL
#include "minicoro.h"
#endif

#include "nova_rt.h"
/* effects.h is included by nova_rt.h before fibers.h, so NovaFailFrame
 * and _nova_fail_top are visible here. */

/* Plan 22 Ф.4: libuv для uv_timer_t sleep + uv_run idle.
 * eventloop.h обычно подключается из nova_rt.h ПОСЛЕ fibers.h, но
 * нам надо здесь — supervised_run использует nova_evloop(). Подключаем
 * напрямую — header-guard защитит от re-entry. */
#ifdef NOVA_USE_LIBUV
#include <uv.h>
#include "eventloop.h"
#endif



/* Run a fiber to completion and return its result.
 * entry      : the generated spawn wrapper function
 * user       : pointer to a NovaSpawnCtx_N stack struct (captures)
 * out_result : pointer to a nova_int that receives the result
 */
static inline void nova_fiber_run(void (*entry)(mco_coro*), void* user) {
    mco_desc desc = mco_desc_init(entry, 0);
    desc.user_data = user;
    mco_coro* co = NULL;
    mco_result r = mco_create(&co, &desc);
    if (r != MCO_SUCCESS || co == NULL) {
        fprintf(stderr, "nova: fiber create failed (%d)\n", (int)r);
        abort();
    }
    r = mco_resume(co);
    if (r != MCO_SUCCESS) {
        fprintf(stderr, "nova: fiber resume failed (%d)\n", (int)r);
        abort();
    }
    mco_destroy(co);
    /* result is already stored in user->result by the entry function */
}

/* nova_fiber_yield is defined later (after NovaFiberQueue / _nova_active_scope). */
static inline void nova_fiber_yield(void);

/* ---- Supervised scope: round-robin scheduler over a local fiber queue ----
 *
 * Inside a `supervised { ... }` scope, each `spawn` adds a coroutine to a
 * local NovaFiberQueue without resuming it. When the scope closes, we run
 * round-robin: keep resuming live coroutines until all are MCO_DEAD.
 * This gives real interleaving when fibers yield via nova_fiber_yield()
 * (e.g. through Time.sleep handler).
 *
 * Capacity is fixed at 64 — enough for tests; production would grow.
 */
#define NOVA_SCOPE_CAP 1024

typedef struct {
    mco_coro*       fibers[NOVA_SCOPE_CAP];
    /* Per-fiber saved fail-frame top. Switched in/out around mco_resume so that
     * each fiber has its own throw-protection chain — longjmp from inside the
     * fiber lands in a frame on the SAME fiber-stack, never crosses fibers. */
    NovaFailFrame*  fiber_fail_top[NOVA_SCOPE_CAP];
    /* Per-fiber saved interrupt-frame top. Same rationale as fiber_fail_top:
     * with-blocks inside spawn-body push their own interrupt-frames on
     * fiber-stack; outer with-blocks (on main-stack) must NOT be visible
     * from inside the fiber. */
    NovaInterruptFrame* fiber_interrupt_top[NOVA_SCOPE_CAP];
    /* Per-fiber snapshot of effect handler-pointers (D-handler-scope).
     * Saved when fiber yields, restored when fiber resumes — изолирует
     * `with X = handler { ... }` между fiber'ами. Без этого все fiber'ы
     * на одном OS-thread делят одни __declspec(thread) globals, и
     * handler одного fiber'а перезаписывался бы handler'ом другого
     * (cross-fiber UB).
     *
     * Heap-allocated (через nova_alloc) lazily в spawn_into — иначе
     * NOVA_SCOPE_CAP*sizeof(NovaEffectSnapshot) занимает много стека
     * (256+ KB), и nested supervised выходит за границы. */
    NovaEffectSnapshot* fiber_effect_snapshot[NOVA_SCOPE_CAP];
    /* Per-fiber error captured from a fiber-local fail-frame. NULL means OK.
     * The owner ctx (or scope-runner) reads this after fiber dies to know
     * whether the fiber threw. */
    const char*     fiber_error[NOVA_SCOPE_CAP];
    /* Slot pointer to a fiber's "did_throw" flag inside the fiber's ctx.
     * The spawn-entry stores its address here so scope-runner can also
     * mark via context (used by codegen when needed). NULL = unused slot. */
    nova_bool*      fiber_did_throw[NOVA_SCOPE_CAP];
    int             count;
    /* Scope error: first error captured from any fiber. Reset on init. */
    const char*     first_error;
    /* Cancellation: set to true after the first fiber throws.
     * Other fibers see this on their next yield-point and throw "cancelled"
     * (cooperative cancellation — D50). */
    nova_bool       cancel_requested;
    /* Pending interrupt: when a fiber's handler-method calls `interrupt v`
     * but the matching with-frame lives on main-stack (not in fiber), we
     * cannot longjmp across the mco boundary. Instead we record the
     * interrupt value here and abort the fiber via fail-frame. After
     * supervised_run drains all fibers, on main-flow it re-issues
     * `nova_interrupt(pending_interrupt_value)` so the with-frame catches
     * it correctly. interrupt_pending=true → value is set. */
    nova_bool       interrupt_pending;
    nova_int        interrupt_value;
} NovaFiberQueue;

/* Plan 22 Ф.3 (D93): NovaSchedState typedef + extern globals.
 * Полный API — в sched.h (header-only inline). Здесь только typedef и
 * extern declarations чтобы supervised_step мог использовать
 * nova_sched_find_state. */
typedef void (*NovaSchedStopCb)(void* handle);
typedef struct {
    NovaFiberQueue* scope;
    nova_bool       parked[NOVA_SCOPE_CAP];
    void*           pending_handle[NOVA_SCOPE_CAP];
    NovaSchedStopCb pending_stop_cb[NOVA_SCOPE_CAP];
} NovaSchedState;

#define NOVA_SCHED_STATE_CAP 16
extern NovaSchedState _nova_sched_states[NOVA_SCHED_STATE_CAP];
extern int            _nova_sched_state_count;

static inline NovaSchedState* nova_sched_find_state(NovaFiberQueue* scope) {
    if (!scope) return NULL;
    for (int i = 0; i < _nova_sched_state_count; i++) {
        if (_nova_sched_states[i].scope == scope) return &_nova_sched_states[i];
    }
    return NULL;
}

/* Forward declarations: full implementations в sched.h (header-only).
 * Декларируем здесь чтобы supervised_run/_step и _nova_sleep_via_libuv
 * могли вызвать sched-функции (sched.h подключается ПОСЛЕ fibers.h
 * в nova_rt.h). NovaSchedStopCb уже определён выше с NovaSchedState. */
static inline NovaSchedState* nova_sched_get_state(NovaFiberQueue* scope);
static inline void nova_sched_drop_state(NovaFiberQueue* scope);
static inline void nova_sched_cancel_all_pending(NovaFiberQueue* scope);
static inline int  nova_sched_count_alive(NovaFiberQueue* scope);
static inline int  nova_sched_count_parked(NovaFiberQueue* scope);
static inline int  nova_sched_count_ready(NovaFiberQueue* scope);
static inline void nova_sched_park(NovaFiberQueue* scope, int slot);
static inline void nova_sched_wake(NovaFiberQueue* scope, int slot);
static inline nova_bool nova_sched_is_parked(NovaFiberQueue* scope, int slot);
static inline void nova_sched_register_pending(NovaFiberQueue* scope, int slot,
                                                void* handle,
                                                NovaSchedStopCb stop_cb);
static inline void nova_sched_unregister_pending(NovaFiberQueue* scope, int slot);

static inline void nova_scope_init(NovaFiberQueue* q) {
    q->count = 0;
    q->first_error = NULL;
    q->cancel_requested = false;
    q->interrupt_pending = false;
    q->interrupt_value = 0;
    for (int i = 0; i < NOVA_SCOPE_CAP; i++) {
        q->fiber_fail_top[i] = NULL;
        q->fiber_interrupt_top[i] = NULL;
        q->fiber_error[i] = NULL;
        q->fiber_did_throw[i] = NULL;
        q->fiber_effect_snapshot[i] = NULL;
    }
}

/* ---- D75: CancelToken — first-class cancellation handle ----
 *
 * Token wraps a NovaFiberQueue* (its "own" scope). cancel() sets the
 * scope's cancel_requested flag — same mechanism D71 uses for cooperative
 * cancellation. linked[] holds tokens that should also be cancelled when
 * this one is (parent kill-switch composition via bind()). */
#define NOVA_CANCEL_LINKED_CAP 8

typedef struct NovaCancelToken {
    NovaFiberQueue*           scope;          /* own scope (owner) */
    struct NovaCancelToken*   linked[NOVA_CANCEL_LINKED_CAP];
    int                       linked_count;
} NovaCancelToken;

static inline void nova_cancel_token_init(NovaCancelToken* t, NovaFiberQueue* q) {
    t->scope = q;
    t->linked_count = 0;
    for (int i = 0; i < NOVA_CANCEL_LINKED_CAP; i++) t->linked[i] = NULL;
}

static inline void nova_cancel_token_cancel(NovaCancelToken* t) {
    if (!t || !t->scope) return;
    if (t->scope->cancel_requested) return;   /* idempotent */
    t->scope->cancel_requested = true;
    /* Plan 22 Ф.4 (D93): wake all parked fiber'ов через registered
     * stop_cb's. Это immediate (не дожидаемся следующего yield-point
     * — fiber вообще park'ом без yield'ов). */
    nova_sched_cancel_all_pending(t->scope);
    /* Walk linked tokens and cancel them too — kill-switch composition. */
    for (int i = 0; i < t->linked_count; i++) {
        NovaCancelToken* other = t->linked[i];
        if (other) nova_cancel_token_cancel(other);
    }
}

static inline nova_bool nova_cancel_token_is_cancelled(NovaCancelToken* t) {
    if (!t || !t->scope) return false;
    return t->scope->cancel_requested;
}

/* bind(self, parent): when parent.cancel() fires, self gets cancelled too.
 * Implementation: append self into parent.linked[]. */
static inline void nova_cancel_token_bind(NovaCancelToken* self,
                                          NovaCancelToken* parent) {
    if (!self || !parent) return;
    if (parent->linked_count >= NOVA_CANCEL_LINKED_CAP) {
        fprintf(stderr, "nova: cancel-token linked cap (%d) exceeded\n",
            NOVA_CANCEL_LINKED_CAP);
        abort();
    }
    parent->linked[parent->linked_count++] = self;
    /* If parent is already cancelled, propagate immediately. */
    if (parent->scope && parent->scope->cancel_requested) {
        nova_cancel_token_cancel(self);
    }
}

/* Create a fiber and push it into the scope queue without resuming it. */
static inline void nova_fiber_spawn_into(NovaFiberQueue* q,
                                         void (*entry)(mco_coro*),
                                         void* user) {
    if (q->count >= NOVA_SCOPE_CAP) {
        fprintf(stderr, "nova: supervised scope exceeded NOVA_SCOPE_CAP=%d\n",
            (int)NOVA_SCOPE_CAP);
        abort();
    }
    mco_desc desc = mco_desc_init(entry, 0);
    desc.user_data = user;
    mco_coro* co = NULL;
    mco_result r = mco_create(&co, &desc);
    if (r != MCO_SUCCESS || co == NULL) {
        fprintf(stderr, "nova: fiber create failed (%d)\n", (int)r);
        abort();
    }
    q->fibers[q->count] = co;
    q->fiber_fail_top[q->count] = NULL;       /* fresh fiber: empty fail-stack */
    q->fiber_interrupt_top[q->count] = NULL;  /* and empty interrupt-stack */
    q->fiber_error[q->count] = NULL;
    q->fiber_did_throw[q->count] = NULL;
    /* Inherit current handler-state: новый fiber видит handlers из enclosing
     * scope. Heap-allocate snapshot — на стеке держать массив 1024
     * snapshot'ов недопустимо (nested supervised → stack overflow). */
    q->fiber_effect_snapshot[q->count] =
        (NovaEffectSnapshot*)nova_alloc(sizeof(NovaEffectSnapshot));
    nova_effect_snapshot_save(q->fiber_effect_snapshot[q->count]);
    q->count++;
}

/* Active scope queue + current fiber slot index — used by spawn-entry to
 * report errors back to the scope, and by main-flow Time.sleep dispatch.
 * Set by:
 *  - nova_supervised_step around each mco_resume (fiber-active context)
 *  - emit_supervised entry/exit (main-flow scope context)
 * Externally linked so codegen can write to it from emitted C. */
#ifdef _MSC_VER
__declspec(thread) extern NovaFiberQueue* _nova_active_scope;
__declspec(thread) extern int             _nova_active_slot;
#else
extern __thread NovaFiberQueue* _nova_active_scope;
extern __thread int             _nova_active_slot;
#endif

/* Called from spawn-entry's catch block when the body threw.
 * Records the error message into the scope queue's slot.
 * Also signals cancellation to remaining live fibers (cooperative). */
static inline void nova_fiber_report_error(const char* msg) {
    if (_nova_active_scope && _nova_active_slot >= 0) {
        _nova_active_scope->fiber_error[_nova_active_slot] = msg;
        if (_nova_active_scope->first_error == NULL) {
            _nova_active_scope->first_error = msg;
        }
        _nova_active_scope->cancel_requested = true;
    }
}

/* Single round-robin pass: resume each live fiber in the queue ONCE.
 * Returns the number of still-live fibers after the pass.
 *
 * Per-fiber fail-frame switching: before resuming fiber i, save the current
 * (main or outer) `_nova_fail_top` and install fiber i's saved top. After
 * resume returns (yield or completion), save fiber i's current top back into
 * the queue and restore the outer top. This ensures throw protection chains
 * never cross fiber boundaries.
 */
static inline int nova_supervised_step(NovaFiberQueue* q) {
    int alive = 0;
    NovaFiberQueue* outer_scope = _nova_active_scope;
    int             outer_slot  = _nova_active_slot;
    NovaFailFrame*  outer_fail_top = _nova_fail_top;
    NovaInterruptFrame* outer_interrupt_top = _nova_interrupt_top;
    /* Save outer effect-handler-snapshot before scheduling fibers — после
     * resume каждого fiber'а handlers будут восстановлены к состоянию
     * outer flow. Фибры могут устанавливать собственные `with X = h`
     * внутри своего тела — те состояния хранятся per-fiber, не утекают
     * наружу. */
    NovaEffectSnapshot outer_effects;
    nova_effect_snapshot_save(&outer_effects);
    /* Plan 22 Ф.3/Ф.4: lookup sched-state (если есть parked fiber'ы).
     * NULL значит никто не park'ился — старая логика unchanged. */
    NovaSchedState* sched_st = nova_sched_find_state(q);
    for (int i = 0; i < q->count; i++) {
        mco_coro* co = q->fibers[i];
        if (co == NULL) continue;
        if (mco_status(co) == MCO_DEAD) {
            mco_destroy(co);
            q->fibers[i] = NULL;
            continue;
        }
        /* Plan 22 Ф.3/Ф.4 (D93): skip parked fiber'ы. Они resume'ятся
         * когда wake'нутся (callback timer'а либо cancel). Count alive++,
         * чтобы supervised_run не выходил оставив parked permanently. */
        if (sched_st && sched_st->parked[i]) {
            alive++;
            continue;
        }
        /* Switch fail-top + interrupt-top to fiber's saved chains.
         * Outer with-frames live on main-stack — must NOT be visible to
         * code running on fiber-stack (longjmp across mco-boundary = UB). */
        _nova_fail_top      = q->fiber_fail_top[i];
        _nova_interrupt_top = q->fiber_interrupt_top[i];
        _nova_active_scope  = q;
        _nova_active_slot   = i;
        /* Per-fiber handler scoping: install fiber's saved handler-snapshot
         * before resume. Каждый fiber видит свои `with X = h` биндинги,
         * не handlers других fibers. */
        if (q->fiber_effect_snapshot[i]) {
            nova_effect_snapshot_restore(q->fiber_effect_snapshot[i]);
        }
        mco_result r = mco_resume(co);
        /* Save fiber's current handler state back (с учётом изменений
         * сделанных fiber'ом во время выполнения — `with`-блоков push/pop). */
        if (q->fiber_effect_snapshot[i]) {
            nova_effect_snapshot_save(q->fiber_effect_snapshot[i]);
        }
        /* Save fiber's current state back; restore outer state. */
        q->fiber_fail_top[i]      = _nova_fail_top;
        q->fiber_interrupt_top[i] = _nova_interrupt_top;
        _nova_fail_top      = outer_fail_top;
        _nova_interrupt_top = outer_interrupt_top;
        _nova_active_scope  = outer_scope;
        _nova_active_slot   = outer_slot;
        /* Restore outer handlers (clean state для следующего fiber'а
         * или main-flow после step). */
        nova_effect_snapshot_restore(&outer_effects);
        if (r != MCO_SUCCESS) {
            fprintf(stderr, "nova: fiber resume failed (%d)\n", (int)r);
            abort();
        }
        if (mco_status(co) == MCO_DEAD) {
            mco_destroy(co);
            q->fibers[i] = NULL;
        } else {
            alive++;
        }
    }
    return alive;
}

/* Plan 22 Ф.5 (D92): drain implicit main-scope to quiescence without
 * re-throwing fiber errors. Detach-fiber'ы в top-level main могут
 * throw'нуть после main-body — но re-throw на main-flow (который
 * уже завершён) приведёт к abort. Семантика D50 «detach = fire-and-
 * forget» означает что такие throws logged but не abort'ят процесс.
 *
 * Если fiber-error appears — printf to stderr (диагностика), но
 * нормальный exit. */
static inline void nova_supervised_drain_main_scope(NovaFiberQueue* q) {
    for (;;) {
        int alive = nova_supervised_step(q);
        if (alive == 0) break;
#ifdef NOVA_USE_LIBUV
        int parked = nova_sched_count_parked(q);
        if (parked > 0 && parked == alive) {
            uv_run(nova_evloop(), UV_RUN_ONCE);
        }
#endif
    }
    nova_sched_drop_state(q);
    if (q->first_error) {
        fprintf(stderr, "nova: detach-fiber error after main: %s\n",
                q->first_error);
    }
    q->count = 0;
}

/* Round-robin run: resume each live fiber until all are dead.
 * After all fibers complete, if any threw — re-throw on main-flow.
 *
 * Plan 22 Ф.4: когда все живые fiber'ы parked (никто не ready), idle —
 * uv_run UV_RUN_ONCE. Это блокирует main-thread в kernel-wait'е до
 * ближайшего libuv-события (наш timer's callback пробудит fiber). Так
 * scheduler не жжёт CPU busy-loop'ом.
 */
static inline void nova_supervised_run(NovaFiberQueue* q) {
    for (;;) {
        int alive = nova_supervised_step(q);
        if (alive == 0) break;
        /* alive > 0: либо есть ready fiber'ы (step resume'ил кого-то и
         * counter увеличен), либо ВСЕ alive = parked (никого не
         * resume'или, только counted++). Различим: если ready=0 и
         * parked>0 → idle в uv_run UV_RUN_ONCE. */
#ifdef NOVA_USE_LIBUV
        int parked = nova_sched_count_parked(q);
        if (parked > 0 && parked == alive) {
            /* Все alive parked. Spin до libuv-события (наш sleep timer
             * либо stop_cb из cancel). */
            uv_run(nova_evloop(), UV_RUN_ONCE);
        }
#endif
    }
    /* Cleanup sched-state for этого scope'а (если был alloc'ом). */
    nova_sched_drop_state(q);
    const char* err = q->first_error;
    nova_bool pending = q->interrupt_pending;
    nova_int  ivalue  = q->interrupt_value;
    q->count = 0;
    /* Pending interrupt from a fiber's handler-method takes priority over
     * fiber-throw error: handler ran successfully, decided to interrupt
     * the with-block. Re-issue on main-flow where the with-frame is reachable. */
    if (pending) {
        nova_interrupt(ivalue);
        /* unreachable */
    }
    if (err) {
        /* Re-throw on main-flow (back in caller's stack — safe to longjmp). */
        nova_throw(nova_str_from_cstr(err));
    }
}

/* nova_fiber_yield — suspend the current fiber, yielding to the scheduler.
 * Outside any fiber — no-op.
 *
 * Checks scope cancellation: if another fiber in the same scope threw,
 * `cancel_requested` is set on the scope, and this fiber throws
 * "scope cancelled" instead of yielding. The throw is caught by the
 * fiber's local fail-frame (set up by spawn-entry) — fiber dies cleanly.
 */
static inline void nova_fiber_yield(void) {
    mco_coro* co = mco_running();
    if (!co) return;
    /* Cooperative cancellation check. _nova_active_scope set by step. */
    if (_nova_active_scope && _nova_active_scope->cancel_requested) {
        nova_throw(nova_str_from_cstr("scope cancelled"));
    }
    mco_yield(co);
}

/* ---- Built-in `Time` effect operations ----
 *
 * Defined here because the default handler needs nova_fiber_yield +
 * nova_supervised_step + _nova_active_scope, all of which require
 * NovaFiberQueue to be complete. Declarations are in effects.h.
 */

/* Monotonic wall clock in milliseconds. Used by Nova_Time_now and as
 * the timing source for Nova_Time_sleep's yield-loop. Resolution is
 * platform-dependent but at least 1ms. Implemented inline so each TU
 * gets its own copy — fine since the call is cheap and the function
 * is small. */
#ifdef _WIN32
#  ifndef WIN32_LEAN_AND_MEAN
#    define WIN32_LEAN_AND_MEAN
#  endif
#  include <windows.h>
static inline int64_t _nova_monotonic_ms(void) {
    /* GetTickCount64 returns milliseconds since system boot, monotonic,
     * 64-bit so no rollover concern. */
    return (int64_t)GetTickCount64();
}
static inline void _nova_native_sleep_ms(int64_t ms) {
    if (ms <= 0) return;
    Sleep((DWORD)ms);
}
#else
#  include <time.h>
#  ifdef __unix__
#    include <unistd.h>
#  endif
static inline int64_t _nova_monotonic_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000 + (int64_t)(ts.tv_nsec / 1000000);
}
static inline void _nova_native_sleep_ms(int64_t ms) {
    if (ms <= 0) return;
    struct timespec req;
    req.tv_sec = (time_t)(ms / 1000);
    req.tv_nsec = (long)((ms % 1000) * 1000000L);
    nanosleep(&req, NULL);
}
#endif

/* ─── Plan 22 Ф.4: libuv-based fiber-sleep (NOVA_USE_LIBUV) ─── */
#ifdef NOVA_USE_LIBUV
/* uv.h + eventloop.h уже подключены выше в этом файле. */

/* State для one-shot uv_timer_t sleep на fiber'е. Живёт на стеке
 * fiber-coroutine'ы (caller frame). */
typedef struct {
    NovaFiberQueue* scope;
    int             slot;
    uv_timer_t      timer;
    nova_bool       handle_closed;   /* set by _nova_sleep_close_cb */
} NovaSleepState;

/* Timer fired: wake parked fiber. Handle remains open until close_cb. */
static void _nova_sleep_timer_cb(uv_timer_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    nova_sched_wake(st->scope, st->slot);
}

/* Handle closed — caller cleanup wait может exit'нуть. */
static void _nova_sleep_close_cb(uv_handle_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    st->handle_closed = true;
}

/* stop_cb для cancel-integration. Idempotent — handle может уже быть
 * closing'ом из timer_cb path. */
static void _nova_sleep_stop_cb(void* handle) {
    uv_timer_t* timer = (uv_timer_t*)handle;
    if (!uv_is_closing((uv_handle_t*)timer)) {
        uv_timer_stop(timer);
        uv_close((uv_handle_t*)timer, _nova_sleep_close_cb);
    }
}

/* Fiber-context sleep через uv_timer_t + park/wake. Production-grade —
 * нулевой CPU overhead на sleep period, immediate cancel response. */
static inline void _nova_sleep_via_libuv(NovaFiberQueue* scope, int slot,
                                          nova_int ms) {
    NovaSleepState st = {
        .scope = scope,
        .slot  = slot,
        .handle_closed = false,
    };
    int rc = uv_timer_init(nova_evloop(), &st.timer);
    if (rc != 0) {
        fprintf(stderr, "nova: uv_timer_init failed: %s\n", uv_strerror(rc));
        return;  /* fallback на busy-yield ниже не нужен — это runtime bug */
    }
    st.timer.data = &st;
    rc = uv_timer_start(&st.timer, _nova_sleep_timer_cb, (uint64_t)ms, 0);
    if (rc != 0) {
        fprintf(stderr, "nova: uv_timer_start failed: %s\n", uv_strerror(rc));
        uv_close((uv_handle_t*)&st.timer, NULL);
        return;
    }
    /* Register для cancel-wake (D93). */
    nova_sched_register_pending(scope, slot, &st.timer, _nova_sleep_stop_cb);
    /* Park: scheduler skip'нет нас пока wake не вернёт parked=false. */
    nova_sched_park(scope, slot);
    /* Возврат сюда после wake: либо timer_cb fired, либо cancel сделал
     * stop_cb (закрыл timer) + cancel_all_pending unpark'нул нас. */
    nova_sched_unregister_pending(scope, slot);
    /* Cleanup handle. Если timer-cb отработал normal — handle still open,
     * нужно close. Если cancel stop_cb уже close'нул — uv_is_closing
     * вернёт true. */
    if (!uv_is_closing((uv_handle_t*)&st.timer)) {
        uv_close((uv_handle_t*)&st.timer, _nova_sleep_close_cb);
    }
    /* Wait для close_cb fire. Под bootstrap нет multi-thread — close_cb
     * fire'ает в ближайшем uv_run NOWAIT pass'е. Обычно 1-2 итерации. */
    while (!st.handle_closed) {
        uv_run(nova_evloop(), UV_RUN_NOWAIT);
    }
}
#endif /* NOVA_USE_LIBUV */

/* Default impl: context-sensitive sleep (D71).
 *  - In fiber + NOVA_USE_LIBUV: park-on-uv_timer (Plan 22 Ф.4, D93)
 *  - In fiber (no libuv): busy-yield-loop (legacy fallback)
 *  - On main inside supervised body → drain queue once per yield, then check.
 *  - Else (top-level, no scope) → native OS sleep (no scheduler to yield to).
 *
 * `ms <= 0` → single yield (compatibility with `Time.sleep(0)` idiom).
 *
 * Main + top-level ветки **пока** на busy-yield/native-sleep — D92
 * (Plan 22 Ф.5) выровняет их через implicit main-scope. */
static inline nova_unit _nova_time_default_sleep(nova_int ms) {
    if (ms <= 0) {
        if (mco_running()) {
            nova_fiber_yield();
        } else if (_nova_active_scope) {
            nova_supervised_step(_nova_active_scope);
        }
        return NOVA_UNIT;
    }
    if (mco_running()) {
#ifdef NOVA_USE_LIBUV
        /* Plan 22 Ф.4 (D93): production path через park-on-uv_timer. */
        if (_nova_active_scope && _nova_active_slot >= 0) {
            _nova_sleep_via_libuv(_nova_active_scope, _nova_active_slot, ms);
            return NOVA_UNIT;
        }
        /* Edge case: fiber без scope (shouldn't happen в bootstrap'е, но
         * defensive). Fall through to legacy busy-yield. */
#endif
        /* Legacy: busy-yield until deadline. Сохраняется для не-libuv
         * сборок (NOVA_USE_LIBUV=0) и для edge cases где fiber без scope. */
        int64_t deadline = _nova_monotonic_ms() + (int64_t)ms;
        while (_nova_monotonic_ms() < deadline) {
            nova_fiber_yield();
        }
    } else if (_nova_active_scope) {
        /* Main flow inside a scope: drain queue per pass until deadline.
         * Plan 22 Ф.5 заменит на uv_run + main-scope main-step. */
        int64_t deadline = _nova_monotonic_ms() + (int64_t)ms;
        while (_nova_monotonic_ms() < deadline) {
            nova_supervised_step(_nova_active_scope);
        }
    } else {
        /* Top-level, no scheduler — fall back to native sleep.
         * Plan 22 Ф.5: D92 implicit main-scope обернёт main, эта ветка
         * станет unreachable в normal flow. */
        _nova_native_sleep_ms((int64_t)ms);
    }
    return NOVA_UNIT;
}

/* Default impl: monotonic milliseconds since some unspecified epoch. */
static inline nova_int _nova_time_default_now(void) {
    return (nova_int)_nova_monotonic_ms();
}

/* Inline dispatch: with user handler → handler method; else → default. */
static inline nova_unit Nova_Time_sleep(nova_int ms) {
    if (_nova_handler_Time) {
        return _nova_handler_Time->sleep(_nova_handler_Time->ctx, ms);
    }
    return _nova_time_default_sleep(ms);
}

static inline nova_int Nova_Time_now(void) {
    if (_nova_handler_Time) {
        return _nova_handler_Time->now(_nova_handler_Time->ctx);
    }
    return _nova_time_default_now();
}

#endif /* NOVA_RT_FIBERS_H */
