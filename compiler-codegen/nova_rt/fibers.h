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
} NovaFiberQueue;

static inline void nova_scope_init(NovaFiberQueue* q) {
    q->count = 0;
    q->first_error = NULL;
    q->cancel_requested = false;
    for (int i = 0; i < NOVA_SCOPE_CAP; i++) {
        q->fiber_fail_top[i] = NULL;
        q->fiber_error[i] = NULL;
        q->fiber_did_throw[i] = NULL;
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
    q->fiber_fail_top[q->count] = NULL;     /* fresh fiber: empty fail-stack */
    q->fiber_error[q->count] = NULL;
    q->fiber_did_throw[q->count] = NULL;
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
    for (int i = 0; i < q->count; i++) {
        mco_coro* co = q->fibers[i];
        if (co == NULL) continue;
        if (mco_status(co) == MCO_DEAD) {
            mco_destroy(co);
            q->fibers[i] = NULL;
            continue;
        }
        /* Switch fail-top to fiber's saved chain before resuming. */
        _nova_fail_top    = q->fiber_fail_top[i];
        _nova_active_scope = q;
        _nova_active_slot  = i;
        mco_result r = mco_resume(co);
        /* Save fiber's current fail-top back; restore outer fail-top. */
        q->fiber_fail_top[i] = _nova_fail_top;
        _nova_fail_top    = outer_fail_top;
        _nova_active_scope = outer_scope;
        _nova_active_slot  = outer_slot;
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

/* Round-robin run: resume each live fiber until all are dead.
 * After all fibers complete, if any threw — re-throw on main-flow.
 */
static inline void nova_supervised_run(NovaFiberQueue* q) {
    int alive;
    do { alive = nova_supervised_step(q); } while (alive > 0);
    const char* err = q->first_error;
    q->count = 0;
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

/* Default impl: context-sensitive yield (D71).
 *  - In fiber → nova_fiber_yield (cooperative suspend, scope cancel-check).
 *  - On main inside supervised body → nova_supervised_step (drain queue once).
 *  - Else → no-op (no scheduler to yield to).
 * `ms` is ignored — no timer-wheel in bootstrap. */
static inline nova_unit _nova_time_default_sleep(nova_int ms) {
    (void)ms;
    if (mco_running()) {
        nova_fiber_yield();
    } else if (_nova_active_scope) {
        nova_supervised_step(_nova_active_scope);
    }
    return NOVA_UNIT;
}

/* Default impl: returns 0 (real clock not wired up in bootstrap). */
static inline nova_int _nova_time_default_now(void) {
    return 0;
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
