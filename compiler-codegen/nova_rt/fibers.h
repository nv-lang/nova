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

/* Plan 22 Ф.4 + F2 (2026-05-11): libuv MANDATORY. NOVA_USE_LIBUV должен
 * быть определён в build flags (-DNOVA_USE_LIBUV=1). No-libuv build
 * больше не поддерживается — busy-yield fallback нарушал R7 «no busy-loops»
 * и был conscious shortcut. Решение Plan 22: libuv — обязательная зависимость.
 *
 * Сборка без libuv остановится тут — `#error` указывает где fix:
 * test_runner.rs должен всегда detect_or_build_libuv и pass через build_command. */
#ifndef NOVA_USE_LIBUV
#  error "Plan 22 F2: NOVA_USE_LIBUV is mandatory. " \
          "Build chain must -DNOVA_USE_LIBUV=1 + link libuv.lib. " \
          "See test_runner.rs detect_or_build_libuv()."
#endif
#include <uv.h>
#include "eventloop.h"

/* Plan 27 R4 → Plan 44.2 Этап 1/2: Boehm GC + minicoro fiber stacks.
 *
 * Suspended fiber stacks are off the OS stack — Boehm's conservative scanner
 * would miss pointers stored in them. GC_add_roots per-fiber hits Boehm's
 * internal root-set limit (128 entries) with many fibers.
 *
 * Solution (Plan 44.2 Этап 1):
 *  - Linux/macOS: fiber stacks allocated из per-thread mmap arena
 *    (nova_fiber_alloc). Arena registered ONE GC root для всего active
 *    range → нет MAX_ROOT_SETS issue.
 *  - Windows: пока остаётся на calloc-пути. Single-thread cooperative
 *    means GC физически не запускается между yield/resume — calloc'нутые
 *    stacks остаются «логически live» для одной collect window. Не
 *    идеально, но безопасно для bootstrap (см. Plan 42+).
 *
 * Plan 44.2 Этап 2: GC_disable/GC_enable workaround удалён — arena делает
 * его ненужным на Linux/macOS, а Windows polled только в blocking sync
 * points где fiber stacks не активны.
 *
 * Extension points (для Plan 23 concurrent GC): per-fiber root hooks
 * остаются noop'ами; concurrent collector будет полагаться на
 * arena-range root + write barriers, не на per-fiber registration. */
#ifdef NOVA_GC_BOEHM
#  include <gc.h>
#endif

static inline void _nova_gc_add_fiber_roots(mco_coro* co)    { (void)co; }
static inline void _nova_gc_remove_fiber_roots(mco_coro* co) { (void)co; }

/* Plan 44.2 Etap 1 — fiber stack arena (Linux/macOS).
 *
 * Wire minicoro's alloc_cb/dealloc_cb to nova_fiber_alloc/dealloc, которые
 * берут стек из per-thread mmap'нутой арены вместо calloc. На Windows
 * остаёмся на дефолтном calloc-пути (Plan 44.3 — открытый).
 *
 * Stack size: slot_usable (= slot_size − guard) минус минимальный
 * mco_desc header overhead. Реальный header < 1KB на amd64; 8KB
 * закладывается с запасом. */
#define _NOVA_MCO_HEADER_OVERHEAD 8192
#if (defined(__linux__) || defined(__APPLE__))
  #include "fiber_arena.h"
  #if NOVA_FIBER_ARENA_ENABLED
    static inline mco_desc _nova_mco_desc_init_arena(void (*entry)(mco_coro*)) {
        size_t slot_usable = NOVA_FIBER_STACK_SIZE - NOVA_FIBER_GUARD_SIZE;
        size_t stack_size  = slot_usable - _NOVA_MCO_HEADER_OVERHEAD;
        mco_desc d = mco_desc_init(entry, stack_size);
        d.alloc_cb       = nova_fiber_alloc;
        d.dealloc_cb     = nova_fiber_dealloc;
        d.allocator_data = NULL;
        return d;
    }
    #define _NOVA_MCO_DESC_INIT(entry) (_nova_mco_desc_init_arena(entry))
  #else
    #define _NOVA_MCO_DESC_INIT(entry) (mco_desc_init((entry), 0))
  #endif
#else
  #define _NOVA_MCO_DESC_INIT(entry) (mco_desc_init((entry), 0))
#endif

/* Run a fiber to completion and return its result.
 * entry      : the generated spawn wrapper function
 * user       : pointer to a NovaSpawnCtx_N stack struct (captures)
 * out_result : pointer to a nova_int that receives the result
 */
static inline void nova_fiber_run(void (*entry)(mco_coro*), void* user) {
    mco_desc desc = _NOVA_MCO_DESC_INIT(entry);
    desc.user_data = user;
    mco_coro* co = NULL;
    mco_result r = mco_create(&co, &desc);
    if (r != MCO_SUCCESS || co == NULL) {
        fprintf(stderr, "nova: fiber create failed (%d)\n", (int)r);
        abort();
    }
    _nova_gc_add_fiber_roots(co);
    r = mco_resume(co);
    if (r != MCO_SUCCESS) {
        fprintf(stderr, "nova: fiber resume failed (%d)\n", (int)r);
        abort();
    }
    _nova_gc_remove_fiber_roots(co);
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
 * Plan 22 Ф.7 production: NovaFiberQueue arrays — **heap-allocated**
 * через nova_alloc + capacity-doubling. Hard cap НЕТ — управляется
 * только доступной памятью managed heap.
 *
 * Memory cost:
 *  - Idle scope (count=0): ~100 bytes (struct fields, pointers NULL).
 *  - Initial alloc (на первом spawn_into): capacity=16, ~700 bytes.
 *  - Growth doubling до текущего count. На 10000 fiber'ов ~450 KB
 *    в managed heap (GC соберёт при scope-exit либо unreachable).
 *
 * NovaFiberQueue stack-footprint: ~100 bytes. Nested supervised на 50
 * уровней (нереалистично) — 5 KB stack. Старый embedded arrays был
 * ~50 KB/scope — nested overflow'ил stack на 5+ уровнях. */

#define NOVA_SCOPE_INITIAL_CAP 16

typedef struct {
    /* Plan 22 Ф.7: dynamic arrays через managed heap.
     * NULL до первого spawn_into. capacity показывает alloc'нутую
     * длину массивов (все 7 синхронизированы — растут вместе). */
    mco_coro**      fibers;              /* dynamic [count] */
    void**          fiber_ctx;           /* dynamic [count] — GC root для SpawnCtx */
    NovaFailFrame** fiber_fail_top;      /* dynamic [count] */
    NovaInterruptFrame** fiber_interrupt_top; /* dynamic [count] */
    NovaEffectSnapshot** fiber_effect_snapshot; /* dynamic [count] */
    const char**    fiber_error;         /* dynamic [count] */
    nova_bool**     fiber_did_throw;     /* dynamic [count] */
    int             capacity;            /* alloc'нутая длина массивов */
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
     * it correctly. interrupt_pending=true → value is set.
     *
     * Plan 39 Issue A: добавлено `interrupt_value_ptr` для pointer/struct
     * interrupt values (parallel slot к interrupt_value). Использует ту
     * же логику pending → re-issue на main-flow. Codegen выбирает слот
     * по типу. interrupt_via_ptr=true → re-issue через nova_interrupt_ptr. */
    nova_bool       interrupt_pending;
    nova_bool       interrupt_via_ptr;     /* true: use value_ptr, иначе value */
    nova_int        interrupt_value;
    void*           interrupt_value_ptr;
    /* Plan 22 Ф.3 (D93) production: lazy-allocated park/wake state.
     *
     * Pointer-в-struct вместо global side-table (предыдущая итерация
     * Ф.3). Преимущества:
     *  - O(1) lookup (pointer-deref), не O(N) linear search.
     *  - Нет hard cap на nested scopes — managed heap unlimited.
     *  - Память выделяется только когда реально park'аем (обычно NULL).
     *  - GC автоматически освобождает state когда scope unreachable.
     *
     * NULL = ни один fiber в этом scope не park'ился (типичный случай
     * для большинства supervised блоков без Time.sleep/Channel.recv).
     * Lazy-alloc через nova_alloc при первом nova_sched_park либо
     * nova_sched_register_pending. */
    struct NovaSchedState* sched_state;
    /* Plan 44.5 Layer 5: counter fiber'ов running на workers (M:N).
     *
     * Под `runtime.is_initialized()` codegen эмитит
     * `nova_runtime_spawn_into(&scope, ...)` вместо `nova_fiber_spawn_into`.
     * spawn_into push'ит fiber в worker's deque, increments
     * `pending_remote`. После завершения worker fiber decrement'ит
     * counter + `uv_async_send` main thread wake'ом.
     *
     * `nova_supervised_run` / `drain_main_scope` ждут пока
     * `pending_remote == 0 && local fibers == 0`.
     *
     * Atomic operations:
     *   - increment: nova_aint_inc (release ordering)
     *   - decrement: nova_aint_dec (acq_rel)
     *   - load: nova_aint_load (acquire)
     *
     * Initial value 0 — для single-thread (без runtime.init) остаётся 0
     * navсегда, behaviour identical. */
    nova_atomic_int pending_remote;
    /* Plan 44.5 Layer 5: atomic first_error для cross-worker error
     * propagation. Worker fiber на throw делает CAS (NULL → err_msg);
     * первый wins. После CAS — sets cancel_requested = true для
     * cooperative cancel других fiber'ов в scope.
     *
     * NULL = no error. Read через nova_aptr_load(acquire) в main thread
     * после `pending_remote == 0` — корректный happens-before. */
    nova_atomic_ptr first_error_atomic;
    /* Plan 44.5 Layer 5 park/wake: hook вызывается из nova_sched_wake
     * чтобы re-queue fiber обратно в owner-worker deque после park.
     * NULL в single-thread mode — wake не делает ничего лишнего. */
    void (*dispatch_ready)(void* ctx, mco_coro* co);
    void*  dispatch_ctx;
} NovaFiberQueue;

/* Plan 22 Ф.3 (D93) + Ф.7 + Ф.8: NovaSchedState typedef.
 * Полный API — в sched.h (header-only inline). Здесь только определение
 * struct (используется в NovaFiberQueue.sched_state) + forward-deref
 * helper.
 *
 * Ф.7: arrays — dynamic, синхронно растут со scope.capacity.
 *
 * Ф.8: stop_cb возвращает NovaStopMode — sync vs async wake contract.
 * SYNC: handle полностью cleaned после stop_cb return; cancel_all_pending
 *       делает immediate unpark, fiber resume'ится сразу.
 * ASYNC: stop_cb лишь инициировал close; wake придёт от backend
 *        (uv close_cb для sleep/socket/file). cancel_all_pending
 *        НЕ делает unpark — fiber остаётся parked до backend wake.
 *
 * Use-cases (по типам пробуждающихся handle'ов):
 *  - sleep (Plan 22 Ф.4+Ф.8): ASYNC — stop_cb инициирует uv_close,
 *    wake из close_cb.
 *  - channel waitlist (Plan 21): SYNC — stop_cb отвязывает node
 *    inline, handle (waitlist node) убран immediately.
 *  - socket read (Plan 23+): ASYNC — uv_read_stop + uv_close,
 *    wake из close_cb.
 *  - file read (Plan 23+): ASYNC — uv_cancel на uv_fs_t, wake из
 *    request callback. */
typedef enum {
    NOVA_STOP_SYNC  = 0,   /* handle freed после stop_cb return; unpark immediate */
    NOVA_STOP_ASYNC = 1,   /* close initiated; wake придёт от backend, парк сохраняется */
} NovaStopMode;

typedef NovaStopMode (*NovaSchedStopCb)(void* handle);
typedef struct NovaSchedState {
    nova_bool*       parked;              /* dynamic [capacity] */
    void**           pending_handle;      /* dynamic [capacity] */
    NovaSchedStopCb* pending_stop_cb;     /* dynamic [capacity] */
    int              capacity;            /* alloc'нутая длина */
} NovaSchedState;

/* O(1) lookup: pointer-deref. NULL = state ещё не allocated
 * (никто не park'ился в этом scope). */
static inline NovaSchedState* nova_sched_find_state(NovaFiberQueue* scope) {
    return scope ? scope->sched_state : NULL;
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

/* Plan 22 Ф.7: grow scope arrays до new_cap. capacity-doubling.
 * Caller responsibility: вызывать ПЕРЕД увеличением count past capacity. */
static inline void nova_scope_grow(NovaFiberQueue* q, int new_cap) {
    if (new_cap <= q->capacity) return;
    /* Round up to power-of-2 либо doubling. */
    int cap = q->capacity > 0 ? q->capacity : NOVA_SCOPE_INITIAL_CAP;
    while (cap < new_cap) cap *= 2;
    /* Allocate new arrays. */
    mco_coro**           new_fibers = (mco_coro**)nova_alloc(sizeof(mco_coro*) * cap);
    void**               new_ctx    = (void**)nova_alloc(sizeof(void*) * cap);
    NovaFailFrame**      new_fail_top = (NovaFailFrame**)nova_alloc(sizeof(NovaFailFrame*) * cap);
    NovaInterruptFrame** new_interrupt_top = (NovaInterruptFrame**)nova_alloc(sizeof(NovaInterruptFrame*) * cap);
    NovaEffectSnapshot** new_effect_snapshot = (NovaEffectSnapshot**)nova_alloc(sizeof(NovaEffectSnapshot*) * cap);
    const char**         new_error = (const char**)nova_alloc(sizeof(const char*) * cap);
    nova_bool**          new_did_throw = (nova_bool**)nova_alloc(sizeof(nova_bool*) * cap);
    /* Copy existing data. */
    if (q->fibers) {
        for (int i = 0; i < q->count; i++) {
            new_fibers[i]          = q->fibers[i];
            new_ctx[i]             = q->fiber_ctx[i];
            new_fail_top[i]        = q->fiber_fail_top[i];
            new_interrupt_top[i]   = q->fiber_interrupt_top[i];
            new_effect_snapshot[i] = q->fiber_effect_snapshot[i];
            new_error[i]           = q->fiber_error[i];
            new_did_throw[i]       = q->fiber_did_throw[i];
        }
    }
    /* Init new slots to NULL/safe defaults. */
    for (int i = q->count; i < cap; i++) {
        new_fibers[i]          = NULL;
        new_ctx[i]             = NULL;
        new_fail_top[i]        = NULL;
        new_interrupt_top[i]   = NULL;
        new_effect_snapshot[i] = NULL;
        new_error[i]           = NULL;
        new_did_throw[i]       = NULL;
    }
    /* Swap. Old arrays — GC соберёт когда они станут unreachable. */
    q->fibers              = new_fibers;
    q->fiber_ctx           = new_ctx;
    q->fiber_fail_top      = new_fail_top;
    q->fiber_interrupt_top = new_interrupt_top;
    q->fiber_effect_snapshot = new_effect_snapshot;
    q->fiber_error         = new_error;
    q->fiber_did_throw     = new_did_throw;
    q->capacity            = cap;
}

static inline void nova_scope_init(NovaFiberQueue* q) {
    q->count = 0;
    q->capacity = 0;
    q->fibers = NULL;
    q->fiber_ctx = NULL;
    q->fiber_fail_top = NULL;
    q->fiber_interrupt_top = NULL;
    q->fiber_effect_snapshot = NULL;
    q->fiber_error = NULL;
    q->fiber_did_throw = NULL;
    q->first_error = NULL;
    q->cancel_requested = false;
    q->interrupt_pending = false;
    q->interrupt_via_ptr = false;
    q->interrupt_value = 0;
    q->interrupt_value_ptr = NULL;
    /* Plan 22 Ф.3 production: lazy sched_state alloc — NULL пока никто
     * не park'ился. Большинство supervised блоков не используют sleep/
     * recv => sched_state остаётся NULL, нулевой overhead. */
    q->sched_state = NULL;
    /* Plan 44.5 Layer 5: atomic counters для M:N integration.
     * Single-thread baseline (без runtime.init) — оба остаются нулевыми
     * forever, behaviour identical. */
    nova_aint_init(&q->pending_remote, 0);
    nova_aptr_init(&q->first_error_atomic, NULL);
    q->dispatch_ready = NULL;
    q->dispatch_ctx   = NULL;
    /* Plan 22 Ф.7: arrays — lazy alloc'нутся в nova_fiber_spawn_into.
     * Idle scope (count=0) = ~100 bytes на стеке. */
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

/* Plan 44.5 Layer 5 park/wake: alloc a tracking slot in worker scope for
 * a fiber that will block (Time.sleep, Channel.recv). Caller is within the
 * fiber itself (worker thread). Sets _nova_active_slot so park/wake API works.
 *
 * Returns slot index >= 0. Caller must call nova_scope_free_slot on exit.
 * Thread-safety: called only from own-worker thread (no contention). */
static inline int nova_scope_alloc_slot(NovaFiberQueue* scope, mco_coro* co) {
    void* user = mco_get_user_data(co);
    /* Fast path: reuse a NULL slot from a previously freed fiber. */
    for (int i = 0; i < scope->count; i++) {
        if (scope->fibers[i] == NULL) {
            scope->fibers[i]               = co;
            scope->fiber_ctx[i]            = user;
            scope->fiber_fail_top[i]       = NULL;
            scope->fiber_interrupt_top[i]  = NULL;
            scope->fiber_effect_snapshot[i]= NULL;
            scope->fiber_error[i]          = NULL;
            scope->fiber_did_throw[i]      = NULL;
            return i;
        }
    }
    /* Slow path: extend scope. Pre-alloc in runtime_init prevents realloc
     * on worker thread for the common case (< 64 concurrent fibers). */
    if (scope->count >= scope->capacity) {
        nova_scope_grow(scope, scope->count + 1);
    }
    int slot = scope->count++;
    scope->fibers[slot]               = co;
    scope->fiber_ctx[slot]            = user;
    scope->fiber_fail_top[slot]       = NULL;
    scope->fiber_interrupt_top[slot]  = NULL;
    scope->fiber_effect_snapshot[slot]= NULL;
    scope->fiber_error[slot]          = NULL;
    scope->fiber_did_throw[slot]      = NULL;
    return slot;
}

/* Release a slot previously allocated by nova_scope_alloc_slot. */
static inline void nova_scope_free_slot(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) return;
    scope->fibers[slot]    = NULL;
    scope->fiber_ctx[slot] = NULL;
}

/* Forward-decl для использования из spawn_into. */
static inline void nova_sched_grow_state(NovaFiberQueue* scope, int new_cap);

/* Create a fiber and push it into the scope queue without resuming it.
 * Plan 22 Ф.7: grow arrays через nova_scope_grow если count >= capacity.
 * Hard cap НЕТ — управляется только managed-heap размером. */
static inline void nova_fiber_spawn_into(NovaFiberQueue* q,
                                         void (*entry)(mco_coro*),
                                         void* user) {
    if (q->count >= q->capacity) {
        nova_scope_grow(q, q->count + 1);
        /* Если sched_state allocated — он тоже grow'нется через
         * nova_sched_grow_state (capacity sync). */
        if (q->sched_state) {
            nova_sched_grow_state(q, q->capacity);
        }
    }
    mco_desc desc = _NOVA_MCO_DESC_INIT(entry);
    desc.user_data = user;
    mco_coro* co = NULL;
    mco_result r = mco_create(&co, &desc);
    if (r != MCO_SUCCESS || co == NULL) {
        fprintf(stderr, "nova: fiber create failed (%d)\n", (int)r);
        abort();
    }
    _nova_gc_add_fiber_roots(co);
    q->fibers[q->count]    = co;
    q->fiber_ctx[q->count] = user;            /* GC root: SpawnCtx reachable via managed array */
    q->fiber_fail_top[q->count] = NULL;       /* fresh fiber: empty fail-stack */
    q->fiber_interrupt_top[q->count] = NULL;  /* and empty interrupt-stack */
    q->fiber_error[q->count] = NULL;
    q->fiber_did_throw[q->count] = NULL;
    /* Inherit current handler-state: новый fiber видит handlers из enclosing
     * scope. Heap-allocate snapshot. */
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
 * Also signals cancellation to remaining live fibers (cooperative).
 *
 * Plan 44.5 Layer 5 note: для remote fiber'ов (running на worker под M:N
 * distribution) error propagation идёт через explicit inline code в
 * generated entry function (см. codegen emit_spawn) — не через эту
 * функцию. Worker'е _nova_active_scope = &w->scope (worker's own scope,
 * не parent) — вызов report_error пошёл бы в wrong scope. Codegen
 * routes на _c->_nova_parent_scope.first_error_atomic CAS вместо. */
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
            _nova_gc_remove_fiber_roots(co);
            mco_destroy(co);
            q->fibers[i]    = NULL;
            q->fiber_ctx[i] = NULL;  /* release SpawnCtx GC root */
            continue;
        }
        /* Plan 22 Ф.3/Ф.4 (D93): skip parked fiber'ы. Они resume'ятся
         * когда wake'нутся (callback timer'а либо cancel). Count alive++,
         * чтобы supervised_run не выходил оставив parked permanently.
         * Ф.7: bounds check на sched_st->capacity (может быть меньше
         * scope.count если sched_state alloc'нулся раньше grow'а). */
        if (sched_st && i < sched_st->capacity && sched_st->parked[i]) {
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
            _nova_gc_remove_fiber_roots(co);
            mco_destroy(co);
            q->fibers[i]    = NULL;
            q->fiber_ctx[i] = NULL;  /* release SpawnCtx GC root */
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
        if (alive == 0) {
            /* Plan 44.5 Layer 5: local done — но могут быть remote
             * fiber'ы running на workers. Wait для них. */
            int remote = (int)nova_aint_load(&q->pending_remote);
            if (remote == 0) break;
            uv_run(nova_current_loop(), UV_RUN_ONCE);
            continue;
        }
        int parked = nova_sched_count_parked(q);
        if (parked > 0 && parked == alive) {
            uv_run(nova_current_loop(), UV_RUN_ONCE);
        }
    }
    nova_sched_drop_state(q);
    /* Plan 44.5 Layer 5: cross-worker first_error_atomic check. */
    const char* atomic_err = (const char*)nova_aptr_load(&q->first_error_atomic);
    const char* err = q->first_error ? q->first_error : atomic_err;
    if (err) {
        fprintf(stderr, "nova: detach-fiber error after main: %s\n", err);
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
        if (alive == 0) {
            /* Plan 44.5 Layer 5: local done — но могут быть remote
             * fiber'ы running на workers. Wait для них. */
            int remote = (int)nova_aint_load(&q->pending_remote);
            if (remote == 0) break;
            uv_run(nova_current_loop(), UV_RUN_ONCE);
            continue;
        }
        /* alive > 0: либо есть ready fiber'ы, либо ВСЕ alive = parked.
         * Если ready=0 и parked>0 → idle в uv_run UV_RUN_ONCE. */
        int parked = nova_sched_count_parked(q);
        if (parked > 0 && parked == alive) {
            uv_run(nova_current_loop(), UV_RUN_ONCE);
        }
    }
    /* Cleanup sched-state for этого scope'а (если был alloc'ом). */
    nova_sched_drop_state(q);
    /* Plan 44.5 Layer 5: prefer cross-worker first_error_atomic (set
     * через CAS из worker fiber'а) над single-thread first_error.
     * После pending_remote == 0 cause-effect через atomic release/acquire
     * — main видит final значение atomic. */
    const char* atomic_err = (const char*)nova_aptr_load(&q->first_error_atomic);
    const char* err = q->first_error ? q->first_error : atomic_err;
    nova_bool pending = q->interrupt_pending;
    nova_bool via_ptr = q->interrupt_via_ptr;
    nova_int  ivalue  = q->interrupt_value;
    void*     iptr    = q->interrupt_value_ptr;
    q->count = 0;
    /* Pending interrupt from a fiber's handler-method takes priority over
     * fiber-throw error: handler ran successfully, decided to interrupt
     * the with-block. Re-issue on main-flow where the with-frame is reachable.
     * Plan 39 Issue A: dispatch на ptr-variant если interrupt был pointer. */
    if (pending) {
        if (via_ptr) {
            nova_interrupt_ptr(iptr);
        } else {
            nova_interrupt(ivalue);
        }
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

/* Plan 22 Ф.6 + F2: monotonic clock в миллисекундах.
 *
 * libuv mandatory (см. `#error` в начале fibers.h). uv_hrtime() —
 * наносекунды через QueryPerformanceCounter на Windows,
 * clock_gettime(CLOCK_MONOTONIC) на POSIX. Sub-ms precision,
 * monotonic guarantee, не подвержен NTP/wall-clock jumps.
 *
 * Возвращает миллисекунды (nova_int = int64_t). Epoch — реализация-
 * зависимый. Только дельты значимы. */
static inline int64_t _nova_monotonic_ms(void) {
    return (int64_t)(uv_hrtime() / 1000000ULL);
}

/* ─── Plan 22 Ф.4: libuv-based fiber-sleep ─── */
/* uv.h + eventloop.h уже подключены выше в этом файле. */

/* Plan 22 Ф.8: state-machine для sleep'а. Убирает busy-loop
 * `while !handle_closed uv_run NOWAIT` через async-close contract
 * D93 (stop_cb возвращает ASYNC, wake придёт из close_cb).
 *
 * Lifecycle:
 *   normal path:
 *     START → uv_timer_init/start → stage=PENDING → register_pending → park
 *     (timer fires)
 *       → _nova_sleep_timer_cb: stage=CLOSING, uv_close(close_cb)
 *         (НЕ wake — fiber всё ещё parked)
 *       (close_cb fires асинхронно в ближайшем uv_run pass'е)
 *       → _nova_sleep_close_cb: stage=CLOSED, wake parked fiber
 *       → fiber resumes, sanity-check stage == CLOSED, unregister + return
 *
 *   cancel path:
 *     cancel_all_pending → _nova_sleep_stop_cb: stage=CLOSING,
 *         uv_timer_stop + uv_close(close_cb), return ASYNC
 *       (cancel_all_pending видит ASYNC → НЕ unpark'ает)
 *       (close_cb fires асинхронно)
 *       → _nova_sleep_close_cb: stage=CLOSED, wake parked fiber
 *       → fiber resumes, scope->cancel_requested == true → throw
 *
 * Ключевая идея: один park, никто не wake'ает fiber пока handle полностью
 * не closed. R7 «no busy-loops anywhere» полностью enforced. */

typedef enum {
    NOVA_SLEEP_PENDING = 0,   /* timer armed, fiber parked */
    NOVA_SLEEP_CLOSING = 1,   /* uv_close issued, awaiting close_cb */
    NOVA_SLEEP_CLOSED  = 2,   /* close_cb fired — safe to wake fiber */
} NovaSleepStage;

typedef struct {
    NovaFiberQueue* scope;
    int             slot;
    uv_timer_t      timer;
    NovaSleepStage  stage;
} NovaSleepState;

/* Forward-decl close_cb для использования в timer_cb / stop_cb. */
static void _nova_sleep_close_cb(uv_handle_t* h);

/* Timer fired: инициировать close. НЕ wake'аем fiber — wake придёт из
 * close_cb когда handle полностью released. */
static void _nova_sleep_timer_cb(uv_timer_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    if (st->stage != NOVA_SLEEP_PENDING) {
        return;  /* race с cancel stop_cb — handle уже closing */
    }
    st->stage = NOVA_SLEEP_CLOSING;
    uv_close((uv_handle_t*)h, _nova_sleep_close_cb);
}

/* Close completed — handle fully released. Wake parked fiber. */
static void _nova_sleep_close_cb(uv_handle_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    st->stage = NOVA_SLEEP_CLOSED;
    nova_sched_wake(st->scope, st->slot);
}

/* stop_cb для cancel-integration (D93 Ф.8 ASYNC contract).
 * Идемпотентен — handle может уже быть closing'ом из timer_cb path.
 * Возвращает NOVA_STOP_ASYNC — cancel_all_pending НЕ unpark'нет нас,
 * wake придёт из close_cb. */
static NovaStopMode _nova_sleep_stop_cb(void* handle) {
    uv_timer_t* timer = (uv_timer_t*)handle;
    NovaSleepState* st = (NovaSleepState*)timer->data;
    if (st->stage == NOVA_SLEEP_PENDING) {
        st->stage = NOVA_SLEEP_CLOSING;
        uv_timer_stop(timer);
        uv_close((uv_handle_t*)timer, _nova_sleep_close_cb);
    }
    /* else: timer_cb уже инициировал close — wake придёт из close_cb. */
    return NOVA_STOP_ASYNC;
}

/* No-op timer callback для main-flow uv_run waits (Plan 22 Ф.6).
 * F1 reverted: state-machine refactor вызывал hang в parallel runs. */
static void _nova_main_wait_timer_cb(uv_timer_t* h) { (void)h; }

/* Fiber-context sleep через uv_timer_t + park/wake — Ф.8 state-machine.
 * Production-grade: нулевой CPU overhead, immediate cancel, никаких
 * busy-loop'ов. R7 fully enforced. */
static inline void _nova_sleep_via_libuv(NovaFiberQueue* scope, int slot,
                                          nova_int ms) {
    NovaSleepState st = {
        .scope = scope,
        .slot  = slot,
        .stage = NOVA_SLEEP_PENDING,
    };
    int rc = uv_timer_init(nova_current_loop(), &st.timer);
    if (rc != 0) {
        fprintf(stderr, "nova: FATAL uv_timer_init failed: %s\n", uv_strerror(rc));
        abort();  /* Plan 22 Ф.6: timer_init fails только при OOM либо
                   * loop corruption — это runtime bug, не recoverable. */
    }
    st.timer.data = &st;
    rc = uv_timer_start(&st.timer, _nova_sleep_timer_cb, (uint64_t)ms, 0);
    if (rc != 0) {
        fprintf(stderr, "nova: FATAL uv_timer_start failed: %s\n", uv_strerror(rc));
        uv_close((uv_handle_t*)&st.timer, NULL);
        abort();
    }
    /* Register для cancel-wake (D93). stop_cb тоже initiates close — wake
     * придёт из close_cb. */
    nova_sched_register_pending(scope, slot, &st.timer, _nova_sleep_stop_cb);
    /* Ф.8: один park на весь lifecycle. Wake придёт ТОЛЬКО когда close_cb
     * выполнен (stage == CLOSED). До этого момента fiber parked даже если
     * timer_cb fired (timer_cb лишь инициирует close, не делает wake). */
    nova_sched_park(scope, slot);
    /* Возврат: stage == CLOSED, handle полностью released, busy-loop wait
     * не нужен. */
    nova_sched_unregister_pending(scope, slot);
    /* Sanity: stage должен быть CLOSED. Если что-то wake'нуло раньше —
     * runtime bug либо protocol violation D93. */
    if (st.stage != NOVA_SLEEP_CLOSED) {
        fprintf(stderr,
            "nova: FATAL sleep wake before close_cb (stage=%d) — D93 protocol bug\n",
            (int)st.stage);
        abort();
    }
}

/* Default impl: context-sensitive sleep (D71 + Plan 22 F2 libuv mandatory).
 *  - In fiber: park-on-uv_timer (Plan 22 Ф.4, D93)
 *  - On main inside supervised body → drain queue + bounded uv_run.
 *  - Else (top-level, no scope) → FATAL abort (D92 implicit main-scope
 *    invariant violated).
 *
 * `ms <= 0` → single yield (compatibility with `Time.sleep(0)` idiom). */
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
        /* Plan 22 Ф.4 (D93): production path через park-on-uv_timer.
         * После D92 (Plan 22 Ф.5) _nova_active_scope всегда non-NULL
         * в user-code; fiber без scope — это runtime bug. */
        if (!_nova_active_scope || _nova_active_slot < 0) {
            fprintf(stderr,
                "nova: FATAL Time.sleep called in fiber without active scope "
                "(D92 invariant violated)\n");
            abort();
        }
        _nova_sleep_via_libuv(_nova_active_scope, _nova_active_slot, ms);
        return NOVA_UNIT;
    } else if (_nova_active_scope) {
        /* Main flow inside a scope (D92 implicit либо explicit supervised):
         * drain queue + bounded uv_run пока deadline не пройдёт.
         * Plan 22 Ф.6: вместо busy-loop'а — drain ready, потом uv_run
         * с bounded timeout до deadline. CPU idle когда нет ready fiber'ов.
         *
         * F1 reverted (2026-05-11): попытка proper close_cb state-machine
         * вызвала hang в parallel test runs (race с другими event-loop
         * activities). Откат к simple uv_close(NULL) + NOWAIT pass —
         * не R7 violation (NOWAIT не блокирует), это known acceptable
         * cleanup pattern. F1 откладывается до архитектурного refactor'а
         * main-flow через D93 idle hook (Plan 23+). */
        int64_t deadline = _nova_monotonic_ms() + (int64_t)ms;
        while (_nova_monotonic_ms() < deadline) {
            int alive = nova_supervised_step(_nova_active_scope);
            if (alive == 0) {
                /* Никого нет — просто ждём оставшееся время через
                 * uv_run UV_RUN_ONCE с pending timer на остаток. */
                int64_t remaining = deadline - _nova_monotonic_ms();
                if (remaining > 0) {
                    uv_timer_t main_wait;
                    uv_timer_init(nova_current_loop(), &main_wait);
                    uv_timer_start(&main_wait, _nova_main_wait_timer_cb,
                                    (uint64_t)remaining, 0);
                    uv_run(nova_current_loop(), UV_RUN_ONCE);
                    uv_timer_stop(&main_wait);
                    uv_close((uv_handle_t*)&main_wait, NULL);
                    /* close handle через NOWAIT pass. */
                    uv_run(nova_current_loop(), UV_RUN_NOWAIT);
                }
            } else {
                /* Есть alive fiber'ы — может быть parked. */
                int parked = nova_sched_count_parked(_nova_active_scope);
                if (parked > 0 && parked == alive) {
                    /* Все parked — ждать libuv event. */
                    int64_t remaining = deadline - _nova_monotonic_ms();
                    if (remaining > 0) {
                        uv_run(nova_current_loop(), UV_RUN_ONCE);
                    }
                }
            }
        }
    } else {
        /* Plan 22 Ф.6: top-level вне any scope. После D92 emit_main
         * всегда устанавливает implicit main-scope, эта ветка
         * unreachable в normal flow. Если попали сюда — runtime bug
         * (например Time.sleep в C-static initializer до main). */
        fprintf(stderr,
            "nova: FATAL Time.sleep called outside any scope — D92 "
            "invariant violated. _nova_active_scope == NULL in user-code.\n");
        abort();
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
