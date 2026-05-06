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

/* nova_fiber_yield — suspend the current fiber, yielding to the scheduler.
 * Can be called from within a spawn body. Outside any fiber — no-op.
 */
static inline void nova_fiber_yield(void) {
    mco_coro* co = mco_running();
    if (co) mco_yield(co);
}

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
    mco_coro* fibers[NOVA_SCOPE_CAP];
    int       count;
} NovaFiberQueue;

static inline void nova_scope_init(NovaFiberQueue* q) {
    q->count = 0;
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
    q->fibers[q->count++] = co;
}

/* Single round-robin pass: resume each live fiber in the queue ONCE.
 * Returns the number of still-live fibers after the pass.
 * Used for `Time.sleep(0)` in supervised body (main-flow yields into queue).
 */
static inline int nova_supervised_step(NovaFiberQueue* q) {
    int alive = 0;
    for (int i = 0; i < q->count; i++) {
        mco_coro* co = q->fibers[i];
        if (co == NULL) continue;
        if (mco_status(co) == MCO_DEAD) {
            mco_destroy(co);
            q->fibers[i] = NULL;
            continue;
        }
        mco_result r = mco_resume(co);
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
 * On every full pass without progress (no live fibers), exit.
 */
static inline void nova_supervised_run(NovaFiberQueue* q) {
    int alive;
    do { alive = nova_supervised_step(q); } while (alive > 0);
    q->count = 0;
}

#endif /* NOVA_RT_FIBERS_H */
