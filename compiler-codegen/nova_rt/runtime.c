// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 44 (M:N Этап 0, 2026-05-13) — multi-thread runtime impl.
 *
 * Vela — M:N-движок конкурентности Nova; этот файл — планировщик (worker-
 * потоки, per-worker libuv-loop). Бренд-имя рантайма — docs/dev/naming-conventions.md
 * §1.2, план 224 (идентификаторы/ABI не переименованы).
 *
 * Minimal proof of concept:
 *   - N worker OS threads (uv_thread_create).
 *   - Each worker: own libuv loop, own scope, mutex-protected push queue.
 *   - Spawn round-robin (Chase-Lev deque — Этап 1).
 *   - Cross-worker wake via uv_async_send.
 *
 * Не использовать без явного nova_runtime_init() вызова — bootstrap
 * default остаётся single-thread.
 */

/* Include umbrella для правильного ordering (fibers.h → nova_sched.h → ...). */
#include "nova_rt.h"
#include "runtime.h"
#include "driver.h"  /* Plan 83.11 Ф.2: centralized I/O driver */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <limits.h>

#ifndef NOVA_USE_LIBUV
#  error "Plan 44 requires NOVA_USE_LIBUV — libuv mandatory for M:N"
#endif

#include <uv.h>

/* Plan 44.5 Layer 4+5: Boehm GC_THREADS register per worker.
 * vcpkg bdwgc build.ninja shows -DGC_THREADS in DEFINES — library IS thread-safe.
 * Client must define GC_THREADS too (via test_runner -DGC_THREADS) to expose
 * GC_register_my_thread / GC_allow_register_threads prototypes.
 * Works on all platforms when GC_THREADS defined at compile time. */
#if defined(NOVA_GC_BOEHM)
#  define NOVA_GC_THREADS_REGISTER 1
#  include <gc.h>
#endif

/* ── Worker struct ─────────────────────────────────────────────── */

struct NovaWorker {
    int               id;
    uv_thread_t       thread;
    uv_loop_t         loop;
    uv_async_t        wake_handle;
    /* Plan 83-go-cmn Ф.1: fixed-size inline ring (go1.4 P.runq port) replaces
     * the Chase-Lev deque. Base address stable for the worker's whole life →
     * no realloc/grow race. Owner FIFO get + store-release tail; thieves CAS
     * head (steal-half); overflow spills to _nova_global_runq via schedlink.
     * ~32 KiB inline (sizeof(ptr)*NOVA_RUNQ_CAP). */
    NovaRunq          runq;
    /* scope остаётся для cancellation propagation и fiber bookkeeping —
     * но fiber dispatch идёт через runq. */
    NovaFiberQueue    scope;
    nova_atomic_bool  stop;
    nova_atomic_int   pending_count;
    /* Plan 44.5 Layer 5 park/wake: cross-thread wake queue.
     * Fibers parked on this worker (via dispatch_ready from another worker or
     * timer callbacks) accumulate here under wake_mu; drained at each worker
     * loop iteration before deque pop. */
    nova_mutex_t      wake_mu;
    mco_coro**        wake_pending;
    int               wake_pending_count;
    int               wake_pending_cap;
    /* Plan 44.7: preemption. `current_fiber_start` — uv_hrtime() snapshot,
     * записанный worker loop'ом перед mco_resume, обнуляемый после. sysmon
     * thread читает его, и если worker крутит одну fiber'у дольше
     * NOVA_PREEMPT_SLICE_NS — выставляет `preempt_flag = 1`.
     *
     * `preempt_flag` — НЕ снапшот: codegen safepoint (nova_preempt_check)
     * читает его ВЖИВУЮ через TLS-указатель `_nova_preempt_ptr`,
     * выставленный в _worker_main на &w->preempt_flag. Снапшот не годится —
     * worker thread застревает внутри mco_resume на весь CPU-loop и не может
     * перечитать флаг; sysmon выставляет его уже после старта fiber'ы.
     *
     * [M-211-preempt-flag-plain-race] (2026-07-17, TSan-confirmed via
     * Plan 211 mn_smoke — runtime.c:615 write vs runtime.c:1082 write,
     * "as if synchronized via sleep" i.e. NO real happens-before, only
     * incidental ordering from sysmon's 10ms poll): the prior comment here
     * claimed "single producer (sysmon) + single consumer (текущий
     * worker'а fiber), volatile достаточно (Go делает non-atomic write в
     * stackguard0)" — TSan disagrees: sysmon (producer) and the worker's
     * OWN thread (consumer, clearing to 0 in _worker_main + the
     * nova_preempt_check safepoint) are DIFFERENT OS threads with no fence
     * between them, so it is a genuine data race under the C11 memory
     * model even though the field is `volatile` and single-word (volatile
     * prevents compiler reordering/elision, not cross-thread visibility
     * ordering — TSan correctly does not special-case it). Risk in
     * practice is low (worst case: one missed/extra preemption tick,
     * self-corrects on sysmon's next 10ms pass — no correctness impact on
     * fiber scheduling), but it is UB and pollutes TSan output, potentially
     * masking the real [M-211-runq-init-steal-visibility-gap] race in the
     * same run. Fixed by using explicit RELAXED atomics on every read/write
     * site (runtime.c:615/1082/1568/1945, fibers.h nova_preempt_check) —
     * zero cost (RELAXED load/store compiles to the same plain mov as the
     * old volatile access on x86/ARM; ordering between producer and
     * consumer was never required, only that the access itself not be a
     * formal race). `current_fiber_start` — torn-read safe через __atomic_*
     * (sysmon читает, worker пишет), same discipline, already correct. */
    uint64_t          current_fiber_start;  /* __atomic_* accessed */
    volatile int      preempt_flag;         /* __atomic_*(RELAXED) accessed */
    /* Plan 83-go-cmn Ф.4 (safe subset): per-worker scheduler scratch, owner-only
     * (read/written ONLY by this worker's own thread in the find-work loop) →
     * plain, no atomics. steal_rng = xorshift32 state for randomized steal-victim
     * start (avoids all idle workers hammering victim 0 = thundering herd).
     * sched_tick = find-work iteration counter for the 61-tick global-poll
     * fairness (anti-starve global-overflow work behind a busy local ring). */
    uint32_t          steal_rng;
    uint32_t          sched_tick;
    /* Plan 44.7: FIFO-очередь кооперативно-yield'нутых fiber'ов. Вытесненный
     * (или вызвавший runtime.yield()) fiber кладётся СЮДА, не обратно в deque.
     * Причина: deque — LIFO для owner'а, re-push вытесненного CPU-fiber'а →
     * он сразу же re-popнут → peer'ы (включая ещё не стартовавшие, на дне
     * deque) голодают. Worker loop берёт из deque (свежие spawn'ы + разбуженные
     * fiber'ы — приоритет), и лишь когда deque пуст — из этой FIFO. Доступ
     * только из worker thread'а (fiber yield'ится НА нём, loop обрабатывает
     * ТАМ ЖЕ) → без mutex'а. Front-advancing массив с компактизацией. */
    mco_coro**        yielded;
    int               yielded_count;
    int               yielded_cap;
    int               yielded_head;
    /* Plan 83.7 (2026-05-25): runnext LIFO priority slot. Single-slot
     * priority queue для cache-warm handler chains (Go runtime
     * runnext + tokio LIFO slot parity).
     *
     * Same-worker wake (timer fire, channel send из owner-thread fiber)
     * stores fiber here вместо deque tail. Worker loop pops runnext
     * первым → woken fiber resumes immediately, instruction cache
     * + data cache warm от previous fiber.
     *
     * Option B (Tokio-style): NOT stealable — only owner thread reads
     * runnext. Max cache-warmth. Imbalanced workloads helped through
     * existing deque steal (Plan 44.5).
     *
     * Access: owner-thread-only (dispatch_ready owner-branch guarded
     * by _current_worker_id == w->id). Plain pointer — no atomic.
     * NULL = empty. */
    mco_coro*         runnext;
    /* Plan 83.6 (2026-05-24): per-worker SpawnCtx pool (Go P-mcache аналог).
     * 4 size classes (64/128/256/512 bytes — покрывают ~90% spawn-sites).
     * Larger contexts → Boehm fallback (rare).
     *
     * Lock-free: single owner (this worker thread). Other threads НЕ должны
     * push/pop. Cross-worker fiber move keeps base->_nova_pool_size — free
     * goes к worker'у который сейчас держит fiber'у (его TLS = this worker).
     *
     * INTRUSIVE list: free buffer первые sizeof(void*) bytes — next pointer
     * (overlaying NovaSpawnCtxBase._nova_parent_scope field). На acquire
     * pop, memset zeros весь buffer ДО возврата caller'у. Это критично —
     * избегает дополнительных GC_malloc_uncollectable calls per pool op
     * (которые defeats purpose pool'а).
     *
     * spawn_pool_free[cls] — head of intrusive singly-linked free list.
     * spawn_pool_count[cls] — current length (capped NOVA_SPAWN_POOL_MAX_PER_CLASS).
     *
     * Memory: max 256 entries × 4 classes × 512 bytes = 512KB per worker.
     * 16 workers × 512KB = 8MB total. Acceptable cap. */
    void*             spawn_pool_free[4];   /* intrusive: head ptr к freed buffer */
    int               spawn_pool_count[4];
    /* Plan 83.10.2 (2026-05-26): deferred uv_close queue for cross-thread
     * cancel dispatch. Timer handles are created on this worker's loop; cancel
     * may arrive from main or another worker. Close must happen on owner's
     * thread — we enqueue here + signal wake_handle → drain in _worker_async_cb. */
    NovaDeferredCloseQueue close_queue;
    /* [M-183-net2-loop-affinity-cross-thread-op] fix: deferred generic-call
     * queue for cross-thread uv-op issue (net.c read/write/accept/udp
     * send/recv when the issuing fiber was work-stolen off this handle's
     * owning worker since the handle was created). Same shape/drain path as
     * close_queue. */
    NovaDeferredCallQueue call_queue;
};

/* 4 size classes covering 64/128/256/512 byte contexts. Empirical: most
 * spawn-sites have ≤3 captures (≤ ~80 bytes). 256+ class catches closures
 * с many captures. > 512 falls back to direct Boehm path.
 *
 * Index: 0=64, 1=128, 2=256, 3=512. */
#define NOVA_SPAWN_POOL_SIZE_CLASSES 4
static const size_t _nova_spawn_pool_class_size[NOVA_SPAWN_POOL_SIZE_CLASSES] = {
    64, 128, 256, 512
};

/* Pool capacity per size class per worker. 256 × 4 × 16 workers × 512B max
 * = 8 MB total — bounded. Excess returns go к direct Boehm free (slow
 * path; rare under steady-state pool hit). */
#define NOVA_SPAWN_POOL_MAX_PER_CLASS 256

/* Pick size class index или -1 если size > 512. */
static int _nova_spawn_pool_class(size_t size) {
    for (int i = 0; i < NOVA_SPAWN_POOL_SIZE_CLASSES; i++) {
        if (size <= _nova_spawn_pool_class_size[i]) return i;
    }
    return -1;
}

/* Plan 44.7: timeslice до preemption. Go использует 10ms. */
#define NOVA_PREEMPT_SLICE_NS 10000000ULL

/* Plan 44.7: yielded-FIFO helpers. Single-threaded (worker owns it) — no
 * locking. push_back добавляет в хвост (с компактизацией/ростом), pop_front
 * снимает с головы. */
static void _worker_yielded_push(NovaWorker* w, mco_coro* co) {
    if (w->yielded_head + w->yielded_count >= w->yielded_cap) {
        if (w->yielded_head > 0) {
            /* Компактизация: сдвигаем живой хвост к началу. */
            for (int i = 0; i < w->yielded_count; i++) {
                w->yielded[i] = w->yielded[w->yielded_head + i];
            }
            w->yielded_head = 0;
        }
        if (w->yielded_count >= w->yielded_cap) {
            int new_cap = w->yielded_cap > 0 ? w->yielded_cap * 2 : 8;
            w->yielded = (mco_coro**)realloc(w->yielded,
                                             (size_t)new_cap * sizeof(mco_coro*));
            if (!w->yielded) abort();
            w->yielded_cap = new_cap;
        }
    }
    w->yielded[w->yielded_head + w->yielded_count] = co;
    w->yielded_count++;
}

static mco_coro* _worker_yielded_pop(NovaWorker* w) {
    if (w->yielded_count == 0) return NULL;
    mco_coro* co = w->yielded[w->yielded_head];
    w->yielded_head++;
    w->yielded_count--;
    if (w->yielded_count == 0) w->yielded_head = 0;
    return co;
}

/* Plan 83.6: pool acquire/release implementations defined later в этом
 * TU (после _workers + _current_worker_id TLS declarations). Public API
 * declared в runtime.h (nova_spawn_pool_acquire/release). */

/* ── Runtime state ─────────────────────────────────────────────── */

static NovaWorker*     _workers = NULL;
static int             _n_workers = 0;          /* materialized worker count */
static nova_atomic_int _round_robin = 0;

/* Plan 83-go-cmn Ф.1: ONE global overflow run queue per runtime. Workers
 * spill HALF their ring here (nova_runq_put_slow) when it fills, and pull
 * from it (nova_globrunq_get_one) in the find-work loop. Initialized once in
 * the pool-materialize path. */
static NovaGlobalRunq  _nova_global_runq;

/* runq.h declares `extern NovaRunqDiag nova_runq_diag;` — defined once here
 * (runtime.c is the sole TU linked with the runtime; the standalone
 * test_runq.c compiles its OWN copy, never linked together → no ODR clash). */
NovaRunqDiag nova_runq_diag = {0};

/* runq.h declares `mco_coro** nova_co_schedlink(mco_coro*)` — defined once
 * here. Returns an lvalue-pointer to the intrusive overflow link on the
 * fiber's SpawnCtxBase (mco user-data). The schedlink field is mirrored in
 * the codegen SpawnCtx_N layouts (emit_c.rs) at the same offset. */
mco_coro** nova_co_schedlink(mco_coro* co) {
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    return base ? &base->schedlink : NULL;
}


/* Plan 83.11 Phase A diagnostics (Variant B): in-process runtime state dump.
 *
 * Lock-free, best-effort snapshot. Caller must accept inconsistency между
 * fields if other threads are mutating concurrently. Purpose — diagnostic
 * snapshot для post-hoc analysis, не correctness guarantee.
 *
 * Output format (one line per logical entity, prefix [tag]):
 *   === NOVA_RUNTIME_DUMP === reason=<str>
 *   [globals] n_workers=N driver_started=B armed=B materialized=B
 *   [diag-drv] <counters>
 *   [worker i] deque=N wake_pending=M runnext=<ptr>
 *     [w.i.scope] count=K
 *     [w.i.parked  s0..sK] 01010...
 *     [w.i.pwake   s0..sK] 00100...
 *     [w.i.fiber.slot=s] mco=<status> active_scope=<ptr>
 *   [driver] armed_list_size=N (per-scope)
 *   === END DUMP ===
 *
 * Thread-safety: no locks. Atomic loads where available. Pointer reads as
 * plain because we accept stale snapshot. Race-free against shutdown — caller
 * must ensure called BEFORE nova_runtime_shutdown. */
extern NovaDriver _nova_driver;  /* defined in driver.c */
/* Forward decls for state below — `_armed` / `_materialized` defined later in this file. */
static bool _armed;
static bool _materialized;
/* Plan 83.11 Phase A: track the currently-waiting supervised scope (set by
 * supervised_run_impl on main thread). dump can include its state. */
static struct NovaFiberQueue* _watchdog_active_scope = NULL;
void nova_runtime_set_watchdog_scope(struct NovaFiberQueue* q) {
    _watchdog_active_scope = q;
}
void nova_runtime_dump_state(const char* reason) {
    fprintf(stderr, "=== NOVA_RUNTIME_DUMP === reason=%s\n",
            reason ? reason : "unspecified");
    fprintf(stderr, "[globals] n_workers=%d driver_started=%d armed=%d materialized=%d\n",
            _n_workers,
            (int)nova_abool_load(&_nova_driver.started),
            (int)_armed, (int)_materialized);
    if (!_workers || _n_workers <= 0) {
        fprintf(stderr, "[workers] none materialized\n");
        fprintf(stderr, "=== END DUMP ===\n");
        return;
    }
    for (int wi = 0; wi < _n_workers; wi++) {
        NovaWorker* w = &_workers[wi];
        fprintf(stderr,
            "[worker %d] runnext=%p wake_pending=%d preempt_flag=%d stop=%d\n",
            wi, (void*)w->runnext, w->wake_pending_count,
            (int)__atomic_load_n(&w->preempt_flag, __ATOMIC_RELAXED),
            (int)nova_abool_load(&w->stop));
        NovaFiberQueue* s = &w->scope;
        int count = (int)__atomic_load_n(&s->count, __ATOMIC_ACQUIRE);
        fprintf(stderr, "[w.%d.scope] count=%d cancel_req=%d pending_remote=%d\n",
            wi, count,
            (int)nova_abool_load(&s->cancel_requested),
            (int)nova_aint_load(&s->pending_remote));
        NovaSchedState* st = s->sched_state;
        if (st && st->capacity > 0) {
            int cap = st->capacity;
            int show = cap < 128 ? cap : 128;
            fprintf(stderr, "[w.%d.parked  cap=%d] ", wi, cap);
            for (int i = 0; i < show; i++) {
                nova_bool* _pk = nova_sched_parked_at(st, i);
                fputc(_pk && *_pk ? '1' : '0', stderr);
            }
            fputc('\n', stderr);
            {
                /* Plan 83-go-cmn Ф.2: pending_wake deleted; dump park_state of
                 * each parked_co instead (0=NIL 1=WAIT 2=READY 3=DISPATCHED). */
                fprintf(stderr, "[w.%d.pstate  cap=%d] ", wi, cap);
                for (int i = 0; i < show; i++) {
                    mco_coro** _pco = nova_sched_parked_co_at(st, i);
                    mco_coro* _pc = _pco ? *_pco : NULL;
                    int v = _pc ? (int)nova_park_state_load(_pc) : 0;
                    fputc('0' + (v & 7), stderr);
                }
                fputc('\n', stderr);
            }
            /* Per-slot fiber detail. Show ALL slots с co!=NULL (any status), OR
             * с parked/pwake set. Skip purely empty slots (no fiber, no flags).
             * Plan 83.11 Phase A: critical для finding stuck-but-not-parked fibers. */
            int detail_max = count < cap ? count : cap;
            int alive_non_parked = 0;
            for (int i = 0; i < detail_max; i++) {
                nova_bool* _pk_p = nova_sched_parked_at(st, i);
                bool pk = _pk_p && *_pk_p;
                /* Plan 83-go-cmn Ф.2: park_state of the parked_co replaces pwake. */
                mco_coro** _pco_p = nova_sched_parked_co_at(st, i);
                mco_coro* _pco = _pco_p ? *_pco_p : NULL;
                int ps = _pco ? (int)nova_park_state_load(_pco) : 0;
                mco_coro* co = (i < count) ? s->fibers[i] : NULL;
                if (!pk && !_pco && !co) continue;
                int mco_st = co ? (int)mco_status(co) : -1;
                NovaSpawnCtxBase* base = co ? (NovaSpawnCtxBase*)mco_get_user_data(co) : NULL;
                /* Detect "stuck": fiber alive (SUSPENDED) but not parked */
                bool stuck_alive = co && mco_st == MCO_SUSPENDED && !pk;
                if (stuck_alive) alive_non_parked++;
                fprintf(stderr,
                    "[w.%d.fiber.s%d] co=%p mco_status=%d parent_scope=%p parked=%d pstate=%d hdl=%p stop_cb=%p%s\n",
                    wi, i, (void*)co, mco_st,
                    base ? (void*)base->_nova_parent_scope : NULL,
                    (int)pk, ps,
                    nova_sched_pending_handle_at(st, i) ? *nova_sched_pending_handle_at(st, i) : NULL,
                    (void*)(uintptr_t)(nova_sched_pending_stop_cb_at(st, i) ? (void*)*nova_sched_pending_stop_cb_at(st, i) : NULL),
                    stuck_alive ? " ⚠ STUCK_ALIVE_NOT_PARKED" : "");
            }
            if (alive_non_parked > 0) {
                fprintf(stderr,
                    "[w.%d] ⚠ %d alive-but-not-parked fibers (potential lost-wake or stuck-completion)\n",
                    wi, alive_non_parked);
            }
            /* Deque + runnext detail — fibers WAITING TO RUN but worker not draining */
            int dq_size = (int)nova_runq_len(&w->runq);
            if (dq_size > 0 || w->runnext) {
                fprintf(stderr, "[w.%d.deque] size=%d runnext=%p\n",
                        wi, dq_size, (void*)w->runnext);
            }
        } else {
            fprintf(stderr, "[w.%d.sched_state] NULL or empty\n", wi);
        }
    }
    /* Plan 83.11 Phase A: dump active supervised scope если установлен. */
    NovaFiberQueue* sup = (NovaFiberQueue*)_watchdog_active_scope;
    if (sup) {
        int sup_count = (int)__atomic_load_n(&sup->count, __ATOMIC_ACQUIRE);
        int sup_remote = (int)nova_aint_load(&sup->pending_remote);
        fprintf(stderr,
            "[supervised] scope=%p count=%d pending_remote=%d cancel_req=%d armed_sleeps_head=%p first_error=%s\n",
            (void*)sup, sup_count, sup_remote,
            (int)nova_abool_load(&sup->cancel_requested),
            (void*)sup->armed_sleeps_head,
            sup->first_error ? sup->first_error : "(null)");
        /* Show every slot — supervised's scope.fibers[] tracks completion */
        NovaSchedState* sst = sup->sched_state;
        int sup_cap = sst ? sst->capacity : 0;
        int sup_limit = sup_count < sup_cap ? sup_count : sup_cap;
        if (sup_limit > 256) sup_limit = 256;
        int alive_count = 0, dead_count = 0, null_count = 0;
        for (int i = 0; i < sup_count; i++) {
            mco_coro* co = sup->fibers ? sup->fibers[i] : NULL;
            if (!co) { null_count++; continue; }
            int mc = (int)mco_status(co);
            if (mc == MCO_DEAD) { dead_count++; continue; }
            alive_count++;
            NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
            bool pk = (sst && i < sup_cap && nova_sched_parked_at(sst, i))
                ? *nova_sched_parked_at(sst, i) : 0;
            /* Plan 83-go-cmn Ф.2: park_state replaces pwake. */
            int pw = (int)nova_park_state_load(co);
            fprintf(stderr,
                "[sup.fiber.s%d] co=%p mco=%d parent=%p parked=%d pstate=%d\n",
                i, (void*)co, mc,
                base ? (void*)base->_nova_parent_scope : NULL,
                (int)pk, pw);
        }
        fprintf(stderr,
            "[supervised.summary] slots=%d alive=%d dead=%d null=%d\n",
            sup_count, alive_count, dead_count, null_count);
        /* Walk armed_sleeps_head list if any */
        if (sup->armed_sleeps_head) {
            int n = 0;
            struct NovaSleepState* st = sup->armed_sleeps_head;
            while (st && n < 32) {
                fprintf(stderr,
                    "[supervised.armed.%d] st=%p scope=%p slot=%d\n",
                    n, (void*)st, (void*)st->scope, st->slot);
                st = st->next_in_scope;
                n++;
            }
        }
    }
    fprintf(stderr, "=== END DUMP ===\n");
    fflush(stderr);
}

/* Plan 187 [M-187-watchdog-idle-server-kill]: see runtime.h doc-comment.
 * Deliberately independent of nova_runtime_dump_state's own per-worker loop
 * above (small, self-contained scan) rather than factored into a shared
 * helper — keeps the diagnostic dump's existing, already-battle-tested print
 * path untouched while this only answers "alarm or not". Same lock-free
 * stale-snapshot caveat as the full dump: a false "false" here (missed
 * stuck fiber) just delays the eventual real diagnosis by one more
 * threshold window (the caller re-arms and rechecks), it never permanently
 * suppresses it. */
bool nova_runtime_has_stuck_fibers(void) {
    if (!_workers || _n_workers <= 0) return false;
    for (int wi = 0; wi < _n_workers; wi++) {
        NovaWorker* w = &_workers[wi];
        NovaFiberQueue* s = &w->scope;
        int count = (int)__atomic_load_n(&s->count, __ATOMIC_ACQUIRE);
        NovaSchedState* st = s->sched_state;
        if (!st || st->capacity <= 0) continue;
        int cap = st->capacity;
        int detail_max = count < cap ? count : cap;
        for (int i = 0; i < detail_max; i++) {
            mco_coro* co = s->fibers ? s->fibers[i] : NULL;
            if (!co) continue;
            nova_bool* pk_p = nova_sched_parked_at(st, i);
            bool pk = pk_p && *pk_p;
            /* Same "stuck_alive" test as nova_runtime_dump_state's per-slot
             * detail loop: alive (SUSPENDED) but not cooperatively parked —
             * a legitimately-idle fiber (e.g. an accept-loop parked in
             * uv_accept) always has parked==true; lost-wake/orphaned slots
             * are SUSPENDED with parked==false. */
            if ((int)mco_status(co) == MCO_SUSPENDED && !pk) return true;
        }
    }
    return false;
}

/* Plan 83.1 Ф.4: lazy worker-пул. `_armed` — runtime.init() вызван
 * (M:N запрошен); `_materialized` — пул-потоки реально подняты (лениво,
 * на первом worker-bound spawn). `_target_workers` — резолвнутое число
 * worker'ов, зафиксированное на init/re-tune. До первого spawn пул не
 * существует: hello-world без spawn идёт на одном главном потоке,
 * 0 worker-потоков, 0 sysmon. */
static bool            _armed = false;
static bool            _materialized = false;
static int             _target_workers = 0;
static nova_mutex_t    _init_mu;
static bool            _init_mu_inited = false;
/* Plan 83.1 Ф.4: auto-shutdown — nova_runtime_shutdown регистрируется
 * через atexit() один раз при первом runtime.init. Покрывает graceful
 * cleanup на нормальном return из main и на exit(). */
static bool            _atexit_registered = false;

/* Plan 83.2 Ф.1 (2026-05-23): default-on M:N. До 83.2 пул armиtся
 * только явным `nova_runtime_init()`; без него spawn-пути падали на
 * single-thread cooperative-fallback. С 83.2 — auto-arm на старте
 * программы (через nova_runtime_auto_arm() из codegen-emit main())
 * + защитный auto-arm на каждом spawn-входе. Эквивалент
 * `nova_runtime_init(0)`: резолв maxprocs (NOVA_MAXPROCS env →
 * uv_available_parallelism), _armed=true, atexit-регистрация.
 *
 * [ИЗМЕНЕНО Plan 259 Слой 2, 2026-08-09, D451]: раньше здесь стояло
 * «Hello-world без spawn — _armed=true, но _materialized=false (пул не
 * поднят) → 0 worker-потоков» — верно ДО этого амендмента (D137/D138
 * lazy pool). Причина смены — №457: ленивая материализация «на первом
 * spawn» списывала цену старта пула (потоки + sysmon + per-worker
 * libuv loop) на того, кто СЛУЧАЙНО оказался первым, кто спавнит — что
 * могло быть внутри пользовательского `supervised(timeout:)`-бюджета
 * (тот же класс запрета, что №470/№474: не компенсировать дедлайн
 * длительностью инициализации). Теперь `nova_runtime_auto_arm()` сам
 * материализует пул сразу после arm (см. ниже) — ЛЮБАЯ armed-программа
 * (default-on M:N, без `NOVA_AUTOARM=0`) поднимает `maxprocs()`
 * worker-потоков на старте, даже hello-world без единого `spawn`.
 * Полностью ленивая материализация (0 threads без spawn) остаётся
 * достижимой ТОЛЬКО через `NOVA_AUTOARM=0` (без M:N вовсе). Подробности
 * и обоснование — D451 (spec/decisions/06-concurrency.md).
 * Идемпотентно, thread-safe через _init_mu. */
static bool _nova_autoarm_env_disabled(void);  /* fwd-decl, def ниже */
/* Plan 259 Слой 2 (D451): fwd-decl — nova_runtime_auto_arm() (ниже) теперь
 * материализует пул сразу после arm; полное определение _ensure_materialized
 * ближе к _materialize_pool (после nova_runtime_init), где ему самое место. */
static void _ensure_materialized(void);

static void _auto_arm_if_needed(void) {
    if (_armed) return;
    /* Plan 83.4.5.9 (2026-05-24): escape hatch — `NOVA_AUTOARM=0`
     * полностью отключает auto-arm даже на spawn-fallback. User-кодовая
     * `runtime.init(n)` явная не задействует _auto_arm_if_needed (она
     * сама себе arm'ит) — так что explicit user не блокируется этим
     * env-флагом. Convention: positive env-name (`AUTOARM`) с inverted
     * semantics; `=0`/`=false`/`=no` disables. Replaces legacy
     * `NOVA_NO_AUTOARM=1` (Plan 83.4.5.5; renamed Plan 83.4.5.9 для
     * избавления от двойного отрицания в env-name). */
    if (_nova_autoarm_env_disabled()) return;
    if (!_init_mu_inited) {
        nova_mutex_init(&_init_mu);
        _init_mu_inited = true;
    }
    nova_mutex_lock(&_init_mu);
    if (!_armed) {
        nova_hash_seed_ensure_init();
        _target_workers = nova_runtime_resolve_maxprocs(0);
        _armed = true;
        if (!_atexit_registered) {
            atexit(nova_runtime_shutdown);
            _atexit_registered = true;
        }
    }
    nova_mutex_unlock(&_init_mu);
}

/* Plan 83.4.5.9 Ф.1 (2026-05-24): escape hatch для cooperative-зависимых
 * тестов. Convention: positive env-name (`NOVA_AUTOARM`) с inverted
 * semantics — `=0`/`=false`/`=no` disables auto-arm. Replaces legacy
 * `NOVA_NO_AUTOARM=1` (Plan 83.4.5.5; renamed чтобы избавиться от
 * двойного отрицания в env-name; "не использовать инвертированных
 * имен в env" — project convention 2026-05-24).
 *
 * Когда `NOVA_AUTOARM=0` задан в env, `nova_runtime_auto_arm()`
 * становится no-op — runtime НЕ армится автоматически. spawn-codegen
 * под `is_initialized() == false` route fiber'ы в main scope queue
 * (cooperative drain), а не в worker deque (work-stealing). Это
 * восстанавливает bootstrap-семантику для тестов, специально проверяющих
 * round-robin ordering через `main_yield + Time.sleep(0)` патерн
 * (концептуально аналог Node `setImmediate` semantics).
 *
 * Tests с `// ENV NOVA_AUTOARM=0` будут работать одинаково на
 * armed-default builds и bootstrap. Production user-code остаётся armed
 * (default — unset либо `NOVA_AUTOARM=1`). Phenotype escape hatch —
 * same idea как `NOVA_MAXPROCS=1` directive для single-worker fallback,
 * но более radical (полный bootstrap mode).
 *
 * Cross-runtime parity: Go runtime НЕТ analog (нет cooperative-only mode);
 * tokio `current_thread` runtime — closest equivalent (single-thread async);
 * Node — всегда cooperative single-thread.
 *
 * Returns true если env заполнен AND равно "0"/"false"/"no" (либо
 * варианты "f"/"F"/"n"/"N" — case-insensitive первая буква).
 * Иначе (unset / "1" / "true" / garbage) returns false — auto-arm
 * enabled (default per D138). */
static bool _nova_autoarm_env_disabled(void) {
    const char* env = getenv("NOVA_AUTOARM");
    if (!env || env[0] == '\0') return false;  /* unset → enabled (default) */
    /* "0", "false", "no", "n", "f" (case-insensitive) → disable. */
    return (env[0] == '0' || env[0] == 'f' || env[0] == 'F'
            || env[0] == 'n' || env[0] == 'N');
}

/* Public entry — Plan 83.2 Ф.1 codegen-emit'нутый вызов в main().
 * Plan 83.4.5.9: respect `NOVA_AUTOARM=0` escape hatch (positive
 * env-name; replaces legacy `NOVA_NO_AUTOARM=1`).
 *
 * Plan 259 Слой 2 (2026-08-09, №457, D451): материализует пул СРАЗУ
 * после arm — было отложено до первого worker-bound spawn (D137 lazy
 * pool), что рисковало списать цену старта (потоки + sysmon +
 * per-worker libuv loop + per-worker fiber-арена) на произвольный
 * spawn, включая spawn ВНУТРИ пользовательского
 * `supervised(timeout:)`-бюджета. Этот вызов — первая содержательная
 * строка эмитируемого `main()` (emit_c.rs::emit_main_wrapper, до тела
 * программы), поэтому вся цена материализации оплачивается ДО того,
 * как какой-либо пользовательский таймер мог начать отсчёт. Слой 1
 * этого же плана (fiber_arena.c: ленивые guard-страницы) — предпосылка:
 * без него материализация пула здесь была бы такой же дорогой, как
 * была, просто раньше по времени. `NOVA_AUTOARM=0` (`_nova_autoarm_
 * env_disabled` возвращает true и функция не арм'ит вовсе) остаётся
 * единственным способом получить 0 worker-потоков — материализация
 * достижима только через arm. */
void nova_runtime_auto_arm(void) {
    if (_nova_autoarm_env_disabled()) return;
    _auto_arm_if_needed();
    _ensure_materialized();
}

/* Plan 44.5 Layer 5: main wake handle для cross-thread signal'а из
 * worker'а в main thread'а supervised_run wait-loop. Init'ится в
 * nova_runtime_init на nova_evloop (main thread's default loop). */
static uv_async_t      _main_wake;
static bool            _main_wake_inited = false;

/* Plan 83.10.2 (2026-05-26): deferred close queue for main thread's loop.
 * Init'd alongside _main_wake; drained in _main_wake_cb. */
static NovaDeferredCloseQueue _main_close_queue;
static bool                   _main_close_queue_inited = false;

/* [M-183-net2-loop-affinity-cross-thread-op] fix: deferred generic-call
 * queue for main thread's loop — mirrors _main_close_queue. */
static NovaDeferredCallQueue _main_call_queue;
static bool                  _main_call_queue_inited = false;

static void _main_wake_cb(uv_async_t* h) {
    (void)h;
    /* Plan 83.10.2: drain any deferred uv_close jobs scheduled for main loop. */
    if (_main_close_queue_inited) {
        nova_loop_drain_closes(&_main_close_queue);
    }
    /* [M-183-net2-loop-affinity-cross-thread-op] fix: drain any deferred
     * uv-op issue jobs scheduled for main loop. */
    if (_main_call_queue_inited) {
        nova_loop_drain_calls(&_main_call_queue);
    }
    /* No-op signal otherwise — wakes uv_run(UV_RUN_ONCE) in main thread.
     * Main thread checks scope.pending_remote after wake. */
}

/* ── Plan 44.7: sysmon (system monitor) thread ─────────────────────
 *
 * Аналог Go's sysmon goroutine. Отдельный OS-thread, не привязан к
 * worker'ам. Каждые ~10ms проходит по всем workers и если worker
 * крутит одну fiber'у дольше timeslice'а — выставляет preempt_flag.
 * Worker loop копирует флаг в TLS `_nova_should_yield`, который
 * проверяется codegen'ом в function prologue + loop backedge → fiber
 * кооперативно yield'ится. Это даёт честный CPU-sharing даже для
 * CPU-bound fibers без явного runtime.yield().
 *
 * Почему не signal-based (Go's SIGURG): minicoro mco_yield НЕ
 * async-signal-safe. TLS-флаг + codegen safepoints — 80% benefit за
 * 20% сложности (см. docs/plans/44.7-preemption.md, Вариант B). */
static uv_thread_t       _sysmon_thread;
static nova_atomic_bool  _sysmon_running;
static bool              _sysmon_started = false;

static void _sysmon_main(void* arg) {
    (void)arg;
    while (nova_abool_load(&_sysmon_running)) {
        uv_sleep(10);  /* ~10ms (Windows timer gran → ~15ms — приемлемо). */
        if (!nova_abool_load(&_sysmon_running)) break;
        uint64_t now = uv_hrtime();
        for (int i = 0; i < _n_workers; i++) {
            NovaWorker* w = &_workers[i];
            uint64_t started = __atomic_load_n(&w->current_fiber_start,
                                               __ATOMIC_RELAXED);
            /* started == 0 → worker idle / между fiber'ами — не trip. */
            if (started != 0 && (now - started) > NOVA_PREEMPT_SLICE_NS) {
                /* [M-211-preempt-flag-plain-race] relaxed atomic — see field
                 * comment above (NovaWorker.preempt_flag). */
                __atomic_store_n(&w->preempt_flag, 1, __ATOMIC_RELAXED);
            }
        }
    }
}

/* TLS: current worker id (для diagnostic). -1 = main thread. */
#ifdef _MSC_VER
static __declspec(thread) int _current_worker_id = -1;
#else
static __thread int _current_worker_id = -1;
#endif

/* ── Plan 83.6: per-worker SpawnCtx pool implementation ─────────── */

/* ── [M-mn-spawnctx-corruption-cancel-wake] R1-трипваер (2026-07-19) ──
 *
 * Плейбук docs/dev/debugging-races.md + 173.0 §2 «Риск R1 (HIGHEST) — pool-recycle
 * aliasing SpawnCtx»: poison + магик-канарейка + карантин GC-free-пути.
 * Opt-in через env NOVA_SPAWN_POOL_DIAG=1 — по умолчанию ВЫКЛ, ноль оверхеда
 * (один кешированный int-бранч). Диагностика, не Heisen-тест: все проверки —
 * в холодных точках (release/acquire/goready-entry), никакого контрол-флоу
 * не меняют, кроме abort() на пойманной порче.
 *
 * Схема свободного буфера (диаг-режим):
 *   [0,8)   — intrusive next-линк (pool freelist ЛИБО карантин-стек)
 *   [8,16)  — магик 0xC7A9DEADC7A9DEAD (перекрывает _nova_parent_slot/
 *             _nova_worker_slot — живой ctx там держит маленькие числа,
 *             коллизия исключена)
 *   [16,24) — сохранённый размер буфера (для верификатора)
 *   [24,N)  — 0xDD-poison
 *
 * Ловит (abort ДО фатала, с hex-дампом):
 *   1. DOUBLE-RELEASE — release уже освобождённого ctx (магик на входе);
 *   2. WRITE-AFTER-FREE — чужая запись в свободный буфер (poison-скан
 *      выборки pool-freelist + карантина на каждом release/acquire);
 *   3. USE-AFTER-RELEASE — goready/worker-resume/sweep читают ctx с
 *      магиком/мусорным pool_size (nova_spawn_ctx_diag_check_live).
 *
 * Карантин: в диаг-режиме GC-free-путь (main-thread free / pool-cap /
 * oversize / drain) НЕ зовёт nova_free_uncollectable — буфер уходит в
 * глобальный lock-free стек навсегда (утечка осознанная, только под env).
 * Это (а) делает write-after-GC_free детектируемым (память наша, poison
 * верифицируем), (б) дискриминатор: если краш исчезает под карантином без
 * единого срабатывания — источник порчи НЕ released-SpawnCtx память. */

#define NOVA_POOL_DIAG_MAGIC  0xC7A9DEADC7A9DEADULL
#define NOVA_POOL_DIAG_POISON 0xDD

static int _nova_pool_diag_state = -1;   /* -1 = не читали env */
int nova_spawn_pool_diag(void) {
    int s = __atomic_load_n(&_nova_pool_diag_state, __ATOMIC_RELAXED);
    if (s < 0) {
        const char* e = getenv("NOVA_SPAWN_POOL_DIAG");
        s = (e && e[0] == '1') ? 1 : 0;
        __atomic_store_n(&_nova_pool_diag_state, s, __ATOMIC_RELAXED);
    }
    return s;
}

/* Карантин-стек: push-only Treiber; узлы никогда не уходят — обход без
 * снятия безопасен при конкурентных push (link пишется до CAS-publish). */
static void* _nova_pool_quar_head = NULL;
static int   _nova_pool_quar_count = 0;

static void _nova_pool_diag_dump(const char* why, const char* where,
                                 const void* buf, size_t size) {
    const unsigned char* p = (const unsigned char*)buf;
    size_t n = size && size <= 512 ? size : 128;
    fprintf(stderr,
            "nova: [R1-TRIPWIRE] %s at %s: ctx=%p size=%zu worker=%d\n",
            why, where, buf, size, _current_worker_id);
    for (size_t i = 0; i < n; i += 16) {
        fprintf(stderr, "  +%03zu:", i);
        for (size_t j = i; j < i + 16 && j < n; j++) fprintf(stderr, " %02x", p[j]);
        fprintf(stderr, "\n");
    }
    fflush(stderr);
}

/* Пометить свободный буфер (магик+размер+poison). Линк [0,8) НЕ трогаем —
 * его пишет push-сайт ПОСЛЕ этой пометки. */
static void _nova_pool_diag_mark_free(void* buf, size_t size) {
    if (size < 32) return;
    *(uint64_t*)((char*)buf + 8)  = NOVA_POOL_DIAG_MAGIC;
    *(uint64_t*)((char*)buf + 16) = (uint64_t)size;
    memset((char*)buf + 24, NOVA_POOL_DIAG_POISON, size - 24);
}

/* Проверить свободный буфер: магик цел, размер согласован, poison нетронут. */
static void _nova_pool_diag_verify_free(const void* buf, const char* where) {
    const char* p = (const char*)buf;
    uint64_t magic = *(const uint64_t*)(p + 8);
    if (magic != NOVA_POOL_DIAG_MAGIC) {
        _nova_pool_diag_dump("WRITE-AFTER-FREE (magic clobbered)", where, buf, 128);
        abort();
    }
    uint64_t size = *(const uint64_t*)(p + 16);
    if (size < 32 || size > 4096) {
        _nova_pool_diag_dump("WRITE-AFTER-FREE (size clobbered)", where, buf, 128);
        abort();
    }
    for (uint64_t i = 24; i < size; i++) {
        if ((unsigned char)p[i] != NOVA_POOL_DIAG_POISON) {
            fprintf(stderr, "nova: [R1-TRIPWIRE] first poison diff at +%llu\n",
                    (unsigned long long)i);
            _nova_pool_diag_dump("WRITE-AFTER-FREE (poison diff)", where, buf, (size_t)size);
            abort();
        }
    }
}

/* Выборочная верификация: первые 16 узлов карантина + первые 16 узлов
 * pool-freelist'ов ТЕКУЩЕГО воркера. Только диаг-режим (холодный путь). */
static void _nova_pool_diag_verify_sample(const char* where);

static void _nova_pool_diag_quarantine(void* buf, size_t size) {
    _nova_pool_diag_mark_free(buf, size < 32 ? 32 : size);
    void* head;
    do {
        head = __atomic_load_n(&_nova_pool_quar_head, __ATOMIC_ACQUIRE);
        *(void**)buf = head;
    } while (!__atomic_compare_exchange_n(&_nova_pool_quar_head, &head, buf,
                                          false, __ATOMIC_RELEASE, __ATOMIC_ACQUIRE));
    __atomic_fetch_add(&_nova_pool_quar_count, 1, __ATOMIC_RELAXED);
}

/* Вход release в диаг-режиме: double-release-детект + выборочная проверка. */
static void _nova_pool_diag_release_entry(void* ctx, size_t size) {
    if (*(uint64_t*)((char*)ctx + 8) == NOVA_POOL_DIAG_MAGIC) {
        _nova_pool_diag_dump("DOUBLE-RELEASE (freed magic already present)",
                             "pool_release entry", ctx, size ? size : 128);
        abort();
    }
    _nova_pool_diag_verify_sample("pool_release");
}

/* Живой ctx: магика быть НЕ должно, pool_size ∈ {0, 64..512}, слоты — малые
 * числа. Ловит goready/resume/sweep по уже-освобождённому либо мусорному
 * SpawnCtx (сигнатура-2 гонки: мусорный _nova_fiber_scope к моменту wake). */
void nova_spawn_ctx_diag_check_live(const void* vbase, const char* where) {
    const NovaSpawnCtxBase* b = (const NovaSpawnCtxBase*)vbase;
    const char* why = NULL;
    if (*(const uint64_t*)((const char*)vbase + 8) == NOVA_POOL_DIAG_MAGIC) {
        why = "USE-AFTER-RELEASE (freed magic present on live path)";
    } else if (b->_nova_pool_size != 0
               && (b->_nova_pool_size < 64 || b->_nova_pool_size > 512)) {
        why = "CTX GARBAGE (pool_size out of range)";
    } else if (b->_nova_worker_slot < -2) {
        why = "CTX GARBAGE (worker_slot below -2)";
    }
    if (why) {
        _nova_pool_diag_dump(why, where, vbase, 128);
        abort();
    }
}

static void _nova_pool_diag_verify_sample(const char* where) {
    void* q = __atomic_load_n(&_nova_pool_quar_head, __ATOMIC_ACQUIRE);
    for (int i = 0; q && i < 16; i++) {
        _nova_pool_diag_verify_free(q, where);
        q = *(void**)q;
    }
    int wid = _current_worker_id;
    if (wid >= 0 && _workers) {
        NovaWorker* w = &_workers[wid];
        for (int cls = 0; cls < NOVA_SPAWN_POOL_SIZE_CLASSES; cls++) {
            void* p = w->spawn_pool_free[cls];
            for (int i = 0; p && i < 16; i++) {
                _nova_pool_diag_verify_free(p, where);
                p = *(void**)p;
            }
        }
    }
}

/* Acquire SpawnCtx из P-local pool либо Boehm fallback.
 *
 * Returns zero-initialized buffer of size `_nova_spawn_pool_class_size[cls]`
 * для slot size class (>= requested size), либо exactly `size` если
 * out of bounds (size > 512 → direct Boehm uncollectable).
 *
 * Fast path: lock-free pop из per-worker free list (single owner = this thread).
 * Slow path: GC_malloc_uncollectable (rare — pool empty under contention или
 * first spawn в worker lifecycle).
 *
 * Caller (codegen) НЕ требует доступа к size class: returned buffer
 * automatically has `base->_nova_pool_size` set к class size (либо 0 если
 * oversize fallback path). Release later использует это поле. */
void* nova_spawn_pool_acquire(size_t size) {
    int cls = _nova_spawn_pool_class(size);
    if (cls < 0) {
        /* Oversize — direct Boehm. _nova_pool_size = 0 marker (no pool route). */
        void* p = nova_alloc_uncollectable(size);
        if (p) {
            NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)p;
            base->_nova_pool_size = 0;  /* mark "not from pool" */
        }
        return p;
    }

    int wid = _current_worker_id;
    if (wid < 0) {
        /* Main thread или unregistered context — fallback Boehm.
         * Important: main thread под bootstrap calls spawn_into → codegen
         * routes через regular nova_alloc, не сюда. _armed M:N path:
         * caller всегда worker thread → wid >= 0. */
        void* p = nova_alloc_uncollectable(_nova_spawn_pool_class_size[cls]);
        if (p) {
            NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)p;
            base->_nova_pool_size = _nova_spawn_pool_class_size[cls];
        }
        return p;
    }

    NovaWorker* w = &_workers[wid];
    void* head = w->spawn_pool_free[cls];
    if (head) {
        /* [R1-трипваер]: перед реюзом проверяем, что свободный буфер никто
         * не трогал (poison цел), + выборку остального freelist/карантина. */
        if (nova_spawn_pool_diag()) {
            _nova_pool_diag_verify_free(head, "pool_acquire pop");
            _nova_pool_diag_verify_sample("pool_acquire");
        }
        /* Fast path: pop intrusive head. Lock-free — single owner.
         * Free buffer holds next pointer в первых sizeof(void*) bytes. */
        void* next = *(void**)head;
        w->spawn_pool_free[cls] = next;
        w->spawn_pool_count[cls]--;
        /* Zero-init reused buffer. memset is cheap (~30ns for 256B на modern CPU). */
        memset(head, 0, _nova_spawn_pool_class_size[cls]);
        NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)head;
        base->_nova_pool_size = _nova_spawn_pool_class_size[cls];
        return head;
    }

    /* Slow path: Boehm uncollectable. */
    void* p = nova_alloc_uncollectable(_nova_spawn_pool_class_size[cls]);
    if (p) {
        NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)p;
        base->_nova_pool_size = _nova_spawn_pool_class_size[cls];
    }
    return p;
}

/* Release SpawnCtx back to P-local pool либо Boehm free.
 *
 * Fast path: pool not full → push back. Lock-free single owner.
 * Slow path: pool capped OR oversize OR no worker thread → Boehm free.
 *
 * Caller passes `size` = `base->_nova_pool_size` (0 if "not from pool"
 * → direct Boehm free). */
void nova_spawn_pool_release(void* ctx, size_t size) {
    if (!ctx) return;
    int diag = nova_spawn_pool_diag();
    if (diag) {
        /* [R1-трипваер] double-release-детект + выборочная poison-проверка. */
        _nova_pool_diag_release_entry(ctx, size);
    }
    if (size == 0) {
        /* Allocation went через oversize/legacy path — direct Boehm free. */
        if (diag) { _nova_pool_diag_quarantine(ctx, 128); return; }
        nova_free_uncollectable(ctx);
        return;
    }
    int cls = _nova_spawn_pool_class(size);
    if (cls < 0) {
        if (diag) { _nova_pool_diag_quarantine(ctx, size); return; }
        nova_free_uncollectable(ctx);
        return;
    }

    int wid = _current_worker_id;
    if (wid < 0) {
        /* Main thread free path — pool not available. Direct Boehm. */
        if (diag) { _nova_pool_diag_quarantine(ctx, _nova_spawn_pool_class_size[cls]); return; }
        nova_free_uncollectable(ctx);
        return;
    }

    NovaWorker* w = &_workers[wid];
    if (w->spawn_pool_count[cls] >= NOVA_SPAWN_POOL_MAX_PER_CLASS) {
        /* Pool capped — excess Boehm free. */
        if (diag) { _nova_pool_diag_quarantine(ctx, _nova_spawn_pool_class_size[cls]); return; }
        nova_free_uncollectable(ctx);
        return;
    }

    /* [R1-трипваер] пометить буфер (магик+poison) ДО записи линка. */
    if (diag) _nova_pool_diag_mark_free(ctx, _nova_spawn_pool_class_size[cls]);

    /* Intrusive push: store next pointer в первых bytes ctx'а.
     * No Boehm alloc — single-instruction overhead. */
    *(void**)ctx = w->spawn_pool_free[cls];
    w->spawn_pool_free[cls] = ctx;
    w->spawn_pool_count[cls]++;
}

/* Plan 83.6: drain pool entries на worker shutdown. Called from
 * nova_runtime_shutdown после worker join. Frees all retained ctx
 * buffers через Boehm (no separate entry structs — intrusive list). */
static void _nova_spawn_pool_drain(NovaWorker* w) {
    int diag = nova_spawn_pool_diag();
    for (int cls = 0; cls < NOVA_SPAWN_POOL_SIZE_CLASSES; cls++) {
        void* head = w->spawn_pool_free[cls];
        while (head) {
            void* next = *(void**)head;
            if (diag) {
                /* [R1-трипваер] финальная проверка poison на shutdown;
                 * буфер не освобождаем (карантин-дисциплина, утечка под env). */
                _nova_pool_diag_verify_free(head, "pool_drain");
            } else {
                nova_free_uncollectable(head);
            }
            head = next;
        }
        w->spawn_pool_free[cls] = NULL;
        w->spawn_pool_count[cls] = 0;
    }
}

/* ── Worker main ──────────────────────────────────────────────── */

/* uv_async callback — fires when cross-worker spawn pushes fiber, or
 * when a deferred uv_close is enqueued for this worker's loop (Plan 83.10.2). */
static void _worker_async_cb(uv_async_t* h) {
    NovaWorker* w = (NovaWorker*)h->data;
    if (w) {
        /* Plan 83.10.2: drain cross-thread uv_close jobs on this loop's thread. */
        nova_loop_drain_closes(&w->close_queue);
        /* [M-183-net2-loop-affinity-cross-thread-op] fix: drain cross-thread
         * uv-op issue jobs (net.c read/write/accept/udp send/recv marshaled
         * here after a work-steal moved the issuing fiber off this worker). */
        nova_loop_drain_calls(&w->call_queue);
    }
    /* Wake-up itself signals uv_run; actual fiber drain in worker loop. */
}

/* Plan 44.5 Layer 5 park/wake: dispatch hook called by nova_sched_wake.
 * Same-thread (owner wake via timer on own loop): direct deque push.
 * Cross-thread (wake from different worker or main thread): mutex-protected
 * wake_pending list + uv_async_send to wake the target worker's uv_run. */
static void _worker_dispatch_ready(void* ctx, mco_coro* co) {
    NovaWorker* w = (NovaWorker*)ctx;
    if (_current_worker_id == w->id) {
        /* Plan 83.7 (2026-05-25): owner-thread wake → runnext priority
         * slot. Cache-warm handler chains (Go runnext + tokio LIFO slot).
         * Previous runnext (if any) flushes к deque tail — no loss.
         *
         * Same-thread access guaranteed by enclosing _current_worker_id
         * check → plain pointer, no atomic. */
        mco_coro* prev = w->runnext;
        w->runnext = co;
        if (prev) {
            nova_runq_put(&w->runq, &_nova_global_runq, prev);
        }
    } else {
        /* Cross-thread: queue under mutex, wake worker's uv loop. */
        nova_mutex_lock(&w->wake_mu);
        if (w->wake_pending_count >= w->wake_pending_cap) {
            int new_cap = w->wake_pending_cap > 0 ? w->wake_pending_cap * 2 : 8;
            w->wake_pending = (mco_coro**)realloc(w->wake_pending,
                                                   (size_t)new_cap * sizeof(mco_coro*));
            if (!w->wake_pending) abort();
            w->wake_pending_cap = new_cap;
        }
        w->wake_pending[w->wake_pending_count++] = co;
        nova_mutex_unlock(&w->wake_mu);
        uv_async_send(&w->wake_handle);
    }
}

/* ─── presume-cas-gate window (221.1 №446/№447): shared TLS restore/save
 * hooks for `nova_resume_fiber` (fibers.h) — the ctx-based (`NovaSpawnCtxBase*`)
 * flavor, shared by all THREE ctx-based resume sites: the main loop below,
 * the cleanup-drain tail of the same function, and `_worker_run_one_fiber`.
 * `nova_supervised_step` (fibers.h) uses its OWN array-based hooks instead —
 * its fibers are indexed by (queue, slot), not by a NovaSpawnCtxBase*.
 *
 * This is the exact 3-branch restore that `_worker_main`'s main loop always
 * had (Plan 44.5/83.11/83.10.4) — including the "displaced fiber" branch
 * (`_nova_worker_slot <= -2`, Plan 83.11 STALE-slot fix) that
 * `_worker_run_one_fiber` was previously MISSING (a second, narrower
 * instance of the "convention on N sites, not structure" pattern found
 * while unifying — that call site now gets the full, correct restore too). */
static void _nova_resume_restore_ctx_tls(void* vctx) {
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)vctx;
    if (base && base->_nova_worker_slot >= 0 && base->_nova_fiber_scope) {
        /* Preamble already ran: restore home scope + saved TLS. */
        _nova_active_scope  = base->_nova_fiber_scope;
        _nova_active_slot   = base->_nova_worker_slot;
        _nova_fail_top      = base->_nova_saved_fail_top;
        _nova_interrupt_top = base->_nova_saved_interrupt_top;
        NovaFiberQueue* fscope = base->_nova_fiber_scope;
        int fslot = base->_nova_worker_slot;
        if (fslot < fscope->count && fscope->fiber_effect_snapshot[fslot]) {
            nova_effect_snapshot_restore(fscope->fiber_effect_snapshot[fslot]);
        }
        /* Plan 201 trace-per-fiber: point at this fiber's OWN persistent
         * error-diag bucket regardless of which OS thread resumes it. */
        if (fslot < fscope->count && fscope->fiber_error_state[fslot]) {
            _nova_error_state_p = fscope->fiber_error_state[fslot];
        }
    } else if (base && base->_nova_worker_slot <= -2 && base->_nova_fiber_scope) {
        /* Plan 83.11 fix: displaced fiber (slot=-2 sentinel set by close_cb
         * Fix B). Preamble ran, but slot was invalidated (STALE race).
         * Restore home scope + TLS so the fiber sees the correct scope when
         * it finishes sleeping and exits. */
        _nova_active_scope  = base->_nova_fiber_scope;
        _nova_active_slot   = -1;  /* slot is invalidated — no valid slot */
        _nova_fail_top      = base->_nova_saved_fail_top;
        _nova_interrupt_top = base->_nova_saved_interrupt_top;
        /* Effect snapshot: slot is -1, skip snapshot restore (fiber is exiting soon). */
    } else if (base) {
        /* Before preamble (first run): restore saved fail/interrupt but
         * leave _nova_active_scope as this worker's scope (preamble will
         * allocate the home slot + set _nova_fiber_scope on first resume). */
        _nova_fail_top      = base->_nova_saved_fail_top;
        _nova_interrupt_top = base->_nova_saved_interrupt_top;
        /* Plan 83.10.4 Ф.3: restore spawn-time handler snapshot so fiber
         * sees parent's effect handlers on its FIRST run (before preamble). */
        if (base->_nova_init_snapshot) {
            nova_effect_snapshot_restore(base->_nova_init_snapshot);
        }
    }
}

static void _nova_resume_save_ctx_tls(void* vctx) {
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)vctx;
    if (!base) return;
    base->_nova_saved_fail_top      = _nova_fail_top;
    base->_nova_saved_interrupt_top = _nova_interrupt_top;
    /* Plan 83.4.2 Ф.2: save fiber's current handler-state (с учётом
     * with-блоков push/pop сделанных fiber'ом во время выполнения) обратно
     * в home scope's snapshot. */
    if (base->_nova_fiber_scope && base->_nova_worker_slot >= 0) {
        NovaFiberQueue* fscope = base->_nova_fiber_scope;
        int fslot = base->_nova_worker_slot;
        if (fslot < fscope->count && fscope->fiber_effect_snapshot[fslot]) {
            nova_effect_snapshot_save(fscope->fiber_effect_snapshot[fslot]);
        }
    }
}

static void _worker_main(void* arg) {
    NovaWorker* w = (NovaWorker*)arg;
    _current_worker_id = w->id;

    /* Plan 44.6 Layer 3: per-worker libuv loop visible через TLS.
     * Все timer/handle registrations в этом thread'е (Time.sleep,
     * channels Time.after) пойдут на &w->loop, не на main thread's
     * nova_evloop(). Без этого fiber park'ается на main loop'е, но
     * worker крутит свой uv_run — callback никогда не fire'нет на
     * worker'е, fiber hangs permanently. */
    _nova_current_loop = &w->loop;

    /* Plan 44.5 Layer 4+5: register thread с Boehm GC.
     * Required для workers — без register Boehm STW walker skips thread stack,
     * GC objects referenced only from worker stack → premature collect → SIGSEGV.
     * All platforms: vcpkg bdwgc built with -DGC_THREADS; client passes same flag. */
#ifdef NOVA_GC_THREADS_REGISTER
    struct GC_stack_base sb;
    if (GC_get_stack_base(&sb) == GC_SUCCESS) {
        GC_register_my_thread(&sb);
    }
#endif
#if NOVA_FIBER_ARENA_ENABLED
    /* [M-mn-spawnctx-corruption-cancel-wake]: native-стек воркера в реестр
     * GC push_other_roots-колбэка (POSIX; Windows/non-Boehm — no-op). */
    nova_fiber_arena_register_native_stack();
#endif

    /* Per-worker TLS: _nova_active_scope указывает на own scope.
     * Объявлены в fibers.h cross-platform; здесь только set. */
    _nova_active_scope = &w->scope;
    _nova_active_slot  = -1;

    /* Plan 83.10.4 Ф.3 [M-83.10.1-per-fiber-handler-tls-race]:
     * Initialize per-thread effect registry for this worker.
     *
     * _nova_effect_registry is now __declspec(thread) / __thread so each
     * worker gets its own zero-initialized registry. Here we populate it
     * with THIS thread's TLS addresses (which differ from main thread's
     * addresses — Windows TLS: each thread has distinct copy of __thread
     * vars at TEB+offset, only OFFSET is fixed, not the absolute address).
     *
     * Without this registration, nova_effect_snapshot_restore() on the
     * worker would write to the main thread's _nova_handler_* copies
     * (those addresses were registered by nova_fn_main), leaving the
     * worker's TLS handlers at NULL. Fiber would then see no handler.
     *
     * Registration order must match nova_fn_main to keep snapshot indices
     * consistent (index 0 = Fail, ...everything else exactly as
     * `_nova_register_effects_fn` — generated `_nova_register_all_effects_`
     * — orders it). save/restore iterate by index, so ORDER must be the
     * same everywhere.
     *
     * Plan 175 Ф.2-v3 [regression fix]: `_nova_handler_Time` explicit
     * hardcoded registration REMOVED from this worker-thread path — Time is
     * no longer a builtin special-cased ahead of user effects (it flows
     * through the same generic `effect_schemas`-driven loop as everything
     * else now, see emit_c.rs `emit_user_effect_registrations`). Leaving it
     * hardcoded HERE while the main-thread path (`emit_main_wrapper`)
     * dropped its own matching explicit line put Time at index 1 on worker
     * threads but at its alphabetical generic-loop position on the main
     * thread — a snapshot-index MISMATCH between threads (silently swaps
     * which slot Application/Time/other-effects' inherited handler value
     * lands in for a fiber stolen onto a worker) — caught by
     * spec_tests/conformance/app_effect_basic_t8_1 (child fiber inherited
     * the WRONG Application handler). Registering Time ONLY via the one
     * generic function below, on BOTH main and worker threads, keeps the
     * index assignment single-sourced and consistent everywhere. */
    nova_register_effect_storage((void**)&_nova_handler_Fail);
    /* User-defined effects (and now Time) registered via function pointer
     * set by generated code in nova_fn_main. If NULL (bootstrap / missing
     * generated fn) — only Fail is registered (sufficient for current test
     * suite — no CU exists without SOME effect_schemas entry beyond Fail,
     * since Time/prelude is always present, so this should always be set
     * in practice; NULL-guard kept for defensive bootstrap safety). */
    if (_nova_register_effects_fn) {
        _nova_register_effects_fn();
    }

    /* Plan 44.7: point this worker thread's preemption TLS at its own
     * preempt_flag. Codegen safepoints (nova_preempt_check) dereference
     * `_nova_preempt_ptr` to read the LIVE flag set by sysmon. A fiber
     * always runs on exactly one worker thread, so the ptr always refers
     * to "the worker I'm currently on" — survives work-stealing migration. */
    _nova_preempt_ptr = &w->preempt_flag;

    /* Plan 82 Ф.3: создать fiber-арену этого worker'а заранее. Это
     * регистрирует её (и native-стек worker'а) в глобальном списке арен
     * → GC-колбэк (fiber_arena_win.c) сканирует fiber-стеки И
     * «подвешенный» scheduler-стек КАЖДОГО worker'а, не только тех, что
     * успели сделать spawn. */
#if NOVA_FIBER_ARENA_ENABLED
    nova_fiber_arena_init();
#endif

    while (!nova_abool_load(&w->stop)) {
        /* (0) Service the worker's libuv loop non-blockingly EVERY iteration.
         *
         * Plan 44.7: this is mandatory once preemption exists. A CPU-bound
         * fiber that gets preempted is re-pushed to the deque and (LIFO)
         * immediately re-popped — so the deque is never empty and the old
         * "uv_run only when idle" path would never run. Timer/async callbacks
         * (Time.sleep wakeups, channel Time.after, cross-worker async) would
         * then never fire → parked fibers never resume → deadlock.
         * UV_RUN_NOWAIT processes whatever is ready and returns at once;
         * with nothing ready it is a cheap poll(0). */
        uv_run(&w->loop, UV_RUN_NOWAIT);

        /* (1) Drain cross-thread wake queue (fibers re-queued after park).
         * Done after uv_run so same-thread timer dispatches (which push
         * straight to the deque) and cross-thread ones are both visible. */
        nova_mutex_lock(&w->wake_mu);
        for (int i = 0; i < w->wake_pending_count; i++) {
            nova_runq_put(&w->runq, &_nova_global_runq, w->wake_pending[i]);
        }
        w->wake_pending_count = 0;
        nova_mutex_unlock(&w->wake_mu);

        mco_coro* co = NULL;

        /* Plan 83-go-cmn Ф.4 (safe subset): 61-tick global-poll fairness. Every
         * 61st find-work pass, drain ONE global-overflow fiber BEFORE the local
         * slots, so global work is not starved behind a perpetually-busy local
         * ring. 61 = Go's schedtick prime (coprime to common batch sizes).
         * Owner-only counter; no routing change (home-affinity preserved). */
        if (++w->sched_tick % 61 == 0) {
            co = nova_globrunq_get_one(&_nova_global_runq);
        }

        /* Plan 83.7 (2026-05-25): (1.9) runnext priority slot — woken
         * fiber from same-thread dispatch_ready (channel recv → handler
         * spawn re-wake same-worker chain). Cache-warm vs going through
         * the ring tail. Same-thread access — plain pointer. */
        if (!co && w->runnext) {
            co = w->runnext;
            w->runnext = NULL;
        }

        /* (2) Plan 83-go-cmn Ф.1: local fixed ring — owner FIFO get (was
         * Chase-Lev LIFO pop; FIFO is correctness-preserving, timing only —
         * runnext remains the LIFO fast slot). Свежие spawn'ы + разбуженные. */
        if (!co) {
            co = nova_runq_get(&w->runq);
        }

        /* (2.5) Plan 44.7: yielded-FIFO — кооперативно вытесненные fiber'ы.
         * После ring, до steal: своя preempted-работа продвигается, но
         * уступает свежим/разбуженным. FIFO → честный round-robin между
         * несколькими CPU-bound fiber'ами. */
        if (!co) {
            co = _worker_yielded_pop(w);
        }

        /* (2.7) Plan 83-go-cmn Ф.1: drain the global overflow queue — fibers
         * spilled by nova_runq_put_slow when some worker's ring filled (>CAP).
         * MUST run as a consumer here, else overflow fibers strand forever →
         * pending_remote never reaches 0 → deterministic supervised hang. */
        if (!co) {
            co = nova_globrunq_get_one(&_nova_global_runq);
        }

        /* (3) Idle — try steal у соседей (steal-half from their ring head).
         * Plan 83-go-cmn Ф.4: randomized victim START (xorshift32) so all idle
         * workers don't scan from worker 0 first (thundering herd on one victim);
         * wrap-around still covers every peer exactly once. Owner-only RNG. */
        if (!co) {
            if (w->steal_rng == 0)
                w->steal_rng = (uint32_t)(w->id + 1) * 2654435761u;
            uint32_t r = w->steal_rng;
            r ^= r << 13; r ^= r >> 17; r ^= r << 5;   /* xorshift32 */
            w->steal_rng = r;
            int start = (_n_workers > 0) ? (int)(r % (uint32_t)_n_workers) : 0;
            for (int k = 0; k < _n_workers; k++) {
                int i = (start + k) % _n_workers;
                if (i == w->id) continue;
                co = nova_runq_steal(&w->runq, &_workers[i].runq);
                if (co) break;
            }
        }

        /* (3.5) Plan 83-go-cmn Ф.4: ONE post-steal global re-poll — a fiber may
         * have spilled into _nova_global_runq DURING the steal scan (which walks
         * every peer). Cheap single check before parking; no spin. */
        if (!co) {
            co = nova_globrunq_get_one(&_nova_global_runq);
        }

        /* (4) Still nothing — block в libuv (own loop) до cross-worker wake.
         * UV_RUN_ONCE: wait for at least one event (timer fire, async send),
         * then return — loop checks wake_pending at next iteration start. */
        if (!co) {
            uv_run(&w->loop, UV_RUN_ONCE);
            continue;
        }

        /* (5) Run fiber.
         *
         * Plan 83.11 Phase B: track first-run fiber lifecycle.
         * A first-run fiber has _nova_worker_slot < 0 (preamble not yet run).
         * If first_pop < inc at watchdog time, a fiber was never popped.
         * If first_cas_lost > 0, CAS failed on first run — impossible for fresh fiber. */
        /* Plan 44.5 Layer 5 fix: save/restore _nova_fail_top, _nova_interrupt_top,
         * and _nova_active_slot per fiber — mirrors nova_supervised_step behavior.
         *
         * Bug without this: fiber F1 parks (fail-top = &_ff_F1). Fiber F2 runs
         * and parks (fail-top = &_ff_F2 → &_ff_F1). F1 resumes and throws →
         * longjmp(&_ff_F2->jmp) → cross-stack jump into F2's suspended coroutine
         * → SIGSEGV / STATUS_ACCESS_VIOLATION.
         *
         * Also fixes stale _nova_active_slot: without restore, _nova_active_slot
         * = previous fiber's slot (or -1) when fiber resumes, causing wrong slot
         * in channel ops on second+ park. */
        NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
        /* [R1-трипваер] ctx обязан выглядеть живым перед resume. */
        if (base && nova_spawn_pool_diag()) {
            nova_spawn_ctx_diag_check_live(base, "worker-main resume");
        }

        /* Plan 44.7: preemption hand-off. Clear the preempt flag so each
         * fiber starts its slice clean, and stamp `current_fiber_start` so
         * sysmon can detect an overrun. The running fiber reads the LIVE
         * flag via `_nova_preempt_ptr` (set once in _worker_main) at every
         * codegen safepoint — no stale snapshot. */
        __atomic_store_n(&w->preempt_flag, 0, __ATOMIC_RELAXED);  /* [M-211-preempt-flag-plain-race] */
        __atomic_store_n(&w->current_fiber_start, uv_hrtime(), __ATOMIC_RELAXED);

        /* presume-cas-gate (221.1 №446/№447): THE single resume call in the
         * runtime — restores this fiber's own TLS (fail-top chain, active
         * scope/slot incl. Plan 44.5 work-stealing home-scope fix, Plan
         * 201 error-diag bucket, Plan 83.4.2 handler-snapshot), CAS-gates
         * IDLE→RUNNING (Plan 83.4.5.7 double-resume guard), calls
         * mco_resume exactly once, restores the outer (worker-loop) TLS,
         * and classifies dead/parked — all inside fibers.h::nova_resume_fiber.
         * `ro.owned == false` covers BOTH the CAS-loser case (another
         * thread holds RUNNING right now) AND the №446 case (co was not
         * even MCO_SUSPENDED at entry — a duplicate pop of an already-dead
         * co, source: wake_pending duplicate-push / cross-worker steal
         * races documented at `_worker_dispatch_ready` above): either way
         * the caller MUST NOT touch `co` again — no destroy, no sweep, no
         * state-store, no further mco_status read. */
        NovaResumeOutcome ro = nova_resume_fiber(co, base,
            _nova_resume_restore_ctx_tls, _nova_resume_save_ctx_tls);

        /* Fiber returned to the loop — clear the overrun timestamp so an
         * idle worker is never marked for preemption. */
        __atomic_store_n(&w->current_fiber_start, 0, __ATOMIC_RELAXED);

        if (!ro.owned) {
            /* Not ours to dispose (CAS lost, or co wasn't even SUSPENDED —
             * see doc-comment above). Don't touch co further. */
            continue;
        }

        if (ro.dead) {
            /* Plan 83.4.5.8 (2026-05-24): grab ctx pointer ДО mco_destroy
             * (destroy frees co, не ctx — separate allocations). All ctx
             * allocated через nova_alloc_uncollectable под armed M:N
             * (codegen emit_spawn / emit_detach choice based on
             * nova_runtime_is_initialized()). Free здесь — гарантирует
             * lifecycle ends точно когда fiber finishes.
             *
             * Snapshot adoption note (init_snapshot): moved to
             * scope->fiber_effect_snapshot[slot] by the preamble; nothing
             * to free here (see nova_scope_sweep_dead_child doc). */
            NovaSpawnCtxBase* dead_ctx = (NovaSpawnCtxBase*)mco_get_user_data(co);
            nova_fiber_state_store(co, NOVA_FIBER_STATE_DEAD);
            mco_destroy(co);
            if (dead_ctx) {
                /* Plan 83.6: route через pool release. base->_nova_pool_size
                 * decides: pool route (size > 0, push back) либо direct
                 * Boehm free (size == 0).
                 * Plan 173.0 Ф.3 (A3.3, R1-guard): UNLESS this child failed
                 * under a supervised parent — then retain the ctx instead
                 * of releasing/recycling it (nova_scope_retain_or_release_
                 * child, fibers.h); nova_supervised_run_impl's decision-loop
                 * frees it later, after the failure has been observed. */
                /* [196.6 / D228 §6 class]: unified sweep — retain/release +
                 * pending_sweeps release-decrement (scope-lifetime fence). */
                nova_scope_sweep_dead_child(dead_ctx);
            }
        } else {
            /* Yielded (mco_status is MCO_SUSPENDED — the only other
             * possibility for a fiber we just resumed ourselves): if
             * parked (timer/channel wait) → dispatch_ready re-queues. If
             * not parked (cooperative yield via preemption or
             * runtime.yield) → yielded-FIFO, NOT the deque. Re-pushing to
             * the LIFO deque would make the worker immediately re-pop the
             * same fiber, starving every peer below it (Plan 44.7). */
            if (ro.parked) {
                /* Parked: nova_sched_park уже store'ил PARKED state. dispatch_ready
                 * (через wake CAS PARKED→IDLE) handle'ит requeue + state-transition. */
            } else {
                /* Voluntary yield: RUNNING → IDLE; push в yielded-FIFO. */
                nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
                _worker_yielded_push(w, co);
            }
        }
    }

    /* Cleanup — drain remaining items в deque + yielded-FIFO + runnext
     * (Plan 44.7, Plan 83.7).
     * Plan 83.4.5.7 (2026-05-23): CAS-guard для double-resume race.
     * Plan 83.4.5.8 (2026-05-24): free uncollectable ctx после mco_destroy.
     * Plan 83.7 (2026-05-25): drain runnext priority slot. */
    while (true) {
        mco_coro* co = NULL;
        if (w->runnext) {
            co = w->runnext;
            w->runnext = NULL;
        }
        if (!co) co = nova_runq_get(&w->runq);
        if (!co) co = _worker_yielded_pop(w);
        if (!co) co = nova_globrunq_get_one(&_nova_global_runq); /* Ф.1: drain overflow */
        if (!co) break;
        /* presume-cas-gate (221.1 №446/№447): same unified resume as the
         * main loop above — this closes TWO bugs this drain tail used to
         * have on its own:
         *  №446 — the old code's dead-check was gated on `mco_status(co)`
         *    alone, not on "did we actually win the CAS this iteration",
         *    so a duplicate pop of an already-dead co (same source as the
         *    main-loop bug) fell through to a SECOND mco_destroy + sweep.
         *  №447 — the old code restored NO TLS at all before mco_resume
         *    (fail_top/interrupt_top/active_scope+slot/effect-snapshot —
         *    §10 mn-coding-conventions.md) and unconditionally overwrote
         *    PARKED with IDLE post-resume; `nova_resume_fiber` restores
         *    the fiber's own TLS via the SAME ctx-based hooks the main
         *    loop uses, and `ro.parked` here gates the state-store exactly
         *    like the main loop and `_worker_run_one_fiber` already did. */
        NovaSpawnCtxBase* drain_base = (NovaSpawnCtxBase*)mco_get_user_data(co);
        NovaResumeOutcome ro = nova_resume_fiber(co, drain_base,
            _nova_resume_restore_ctx_tls, _nova_resume_save_ctx_tls);
        if (!ro.owned) {
            /* CAS lost, or co wasn't even SUSPENDED (duplicate pop) — not
             * ours to dispose. Don't touch co further. */
            continue;
        }
        if (ro.dead) {
            NovaSpawnCtxBase* dead_ctx = (NovaSpawnCtxBase*)mco_get_user_data(co);
            nova_fiber_state_store(co, NOVA_FIBER_STATE_DEAD);
            mco_destroy(co);
            if (dead_ctx) {
                /* Plan 83.6: pool release.
                 * Plan 173.0 Ф.3 (A3.3, R1-guard): retain instead of
                 * releasing if this child failed under a supervised parent
                 * (see nova_scope_retain_or_release_child, fibers.h). */
                /* [196.6 / D228 §6 class]: unified sweep — retain/release +
                 * pending_sweeps release-decrement (scope-lifetime fence). */
                nova_scope_sweep_dead_child(dead_ctx);
            }
        } else if (!ro.parked) {
            /* Cooperative yield (not a genuine park) — §447: only store
             * IDLE when NOT parked, mirroring the main loop / gopark
             * protocol (runtime.c §1462-1469-equivalent, fibers.h). A
             * genuinely parked fiber's PARKED state must survive untouched
             * — the waker's CAS PARKED→IDLE is the only legal transition
             * out of it. */
            nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
        }
    }
    _nova_active_slot = -1;
    /* Plan 44.7: worker thread exiting — its preempt_flag (in NovaWorker,
     * freed by shutdown) must not be dereferenced again. */
    _nova_preempt_ptr = NULL;

    /* Plan 82 Ф.3: отвязать TLS-указатель арены ДО GC_unregister — пока
     * поток ещё GC-зарегистрирован, STW его suspend'ит, исключая гонку с
     * GC-колбэком, обходящим список арен. Память арены освободит
     * nova_runtime_shutdown::nova_fiber_arena_release_retired после join. */
#if NOVA_FIBER_ARENA_ENABLED
    /* [M-mn-spawnctx-corruption-cancel-wake]: снять native-стек из реестра
     * ДО GC_unregister (симметрия с регистрацией на входе). */
    nova_fiber_arena_unregister_native_stack();
    nova_fiber_arena_thread_exit();
#endif

#ifdef NOVA_GC_THREADS_REGISTER
    GC_unregister_my_thread();
#endif
}

/* Plan 52 Ф.22: per-process random seed для SipHash.
 * Lazy-init на первом hash-вызове через atomic flag (idempotent,
 * thread-safe). Cryptographically secure: BCryptGenRandom на Windows,
 * getrandom() на Linux/macOS. Если RNG fails — abort (без random seed
 * мы не лучше чем без SipHash; падать лучше чем silent vulnerability).
 *
 * Decl extern в nova_rt.h, definition здесь — ровно одна копия. */
uint64_t nova_hash_seed_k0 = 0;
uint64_t nova_hash_seed_k1 = 0;
/* Atomic flag: 0 = not initialized, 1 = init in progress, 2 = done.
 * Используем простой mutex + flag — init выполняется один раз, race window
 * минимален, дополнительный atomic не оправдан. */
static nova_mutex_t _hash_seed_mu;
static bool _hash_seed_mu_inited = false;
static bool _hash_seed_inited = false;

#if defined(_WIN32)
#  define WIN32_LEAN_AND_MEAN
#  include <windows.h>
#  include <bcrypt.h>
#  pragma comment(lib, "bcrypt.lib")
static void _nova_hash_seed_init(void) {
    uint64_t buf[2];
    NTSTATUS rc = BCryptGenRandom(NULL, (PUCHAR)buf, sizeof(buf),
                                  BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    if (rc != 0) {
        fprintf(stderr, "nova: BCryptGenRandom failed для hash-seed init: 0x%lx\n",
                (unsigned long)rc);
        abort();
    }
    nova_hash_seed_k0 = buf[0];
    nova_hash_seed_k1 = buf[1];
}
#elif defined(__linux__) || defined(__APPLE__)
#  include <sys/random.h>
#  include <errno.h>
static void _nova_hash_seed_init(void) {
    uint64_t buf[2];
    ssize_t n = getrandom(buf, sizeof(buf), 0);
    if (n != (ssize_t)sizeof(buf)) {
        fprintf(stderr, "nova: getrandom failed для hash-seed init: %s\n",
                strerror(errno));
        abort();
    }
    nova_hash_seed_k0 = buf[0];
    nova_hash_seed_k1 = buf[1];
}
#else
/* Fallback на time-based seed. Слабее (predictable если attacker знает
 * start time программы), но лучше чем zero seed. */
#  include <time.h>
static void _nova_hash_seed_init(void) {
    nova_hash_seed_k0 = (uint64_t)time(NULL) ^ 0x9E3779B97F4A7C15ULL;
    nova_hash_seed_k1 = (uint64_t)clock() ^ 0xBB67AE8584CAA73BULL;
}
#endif

/* Public lazy-init entry. Thread-safe через mutex; idempotent.
 * Hot path после init: один cmp/branch (predict-true) + early return. */
void nova_hash_seed_ensure_init(void) {
    if (_hash_seed_inited) return;
    if (!_hash_seed_mu_inited) {
        /* Init mutex inline на первом вызове. Race на самом mutex init
         * сужен до самого первого hash-вызова в программе; в single-thread
         * программе никогда не race; в multi-thread — runtime_init
         * обычно вызывается до spawn, и мы хорошо защищены. На крайний
         * случай — atomic CAS на bool. Для bootstrap: ok. */
        nova_mutex_init(&_hash_seed_mu);
        _hash_seed_mu_inited = true;
    }
    nova_mutex_lock(&_hash_seed_mu);
    if (!_hash_seed_inited) {
        _nova_hash_seed_init();
        _hash_seed_inited = true;
    }
    nova_mutex_unlock(&_hash_seed_mu);
}

/* ── Plan 83.1 Ф.1+Ф.2: worker-count resolution ────────────────────
 *
 * Резолвер числа worker-потоков. Порядок разрешения (Plan 83 §3 П6):
 *
 *   explicit runtime.init(n>0)  >  ENV NOVA_MAXPROCS  >  uv_available_parallelism()
 *
 * `uv_available_parallelism()` (libuv 1.52) уже cgroup+affinity-aware —
 * НЕ переизобретаем через sysconf/GetSystemInfo (это была бы регрессия
 * по cgroup-корректности в контейнерах).
 *
 * Клэмп [NOVA_MAXPROCS_MIN, NOVA_MAXPROCS_MAX]. Запрос выше потолка
 * (любой источник) → клэмп до потолка + диагностический warning на
 * stderr. Динамический re-read cgroup-квоты во время работы (Go 1.25) —
 * followup; зафиксировано как известная дельта vs Go в 06-concurrency.md. */

#define NOVA_MAXPROCS_MIN 1
#define NOVA_MAXPROCS_MAX 1024

/* Клэмпит `n` в [MIN, MAX]. При срабатывании верхнего потолка печатает
 * диагностику — `source` называет того, кто запросил завышенное число. */
static int _nova_clamp_maxprocs(int n, const char* source) {
    if (n > NOVA_MAXPROCS_MAX) {
        fprintf(stderr,
                "nova: %s requested %d workers, clamped to NOVA_MAXPROCS limit %d\n",
                source, n, NOVA_MAXPROCS_MAX);
        return NOVA_MAXPROCS_MAX;
    }
    if (n < NOVA_MAXPROCS_MIN) return NOVA_MAXPROCS_MIN;
    return n;
}

/* Парсит env-переменную NOVA_MAXPROCS. Возврат:
 *   > 0  — валидное значение (до клэмпа);
 *   0    — переменная не задана;
 *   -1   — задана, но невалидна (диагностика уже напечатана).
 * Невалидное значение НЕ abort'ит процесс — резолвер делает fallback на
 * auto-detect (Plan 83.1 Ф.2: «понятная диагностика + fallback»). */
static int _nova_parse_maxprocs_env(void) {
    const char* env = getenv("NOVA_MAXPROCS");
    if (!env || env[0] == '\0') return 0;
    errno = 0;
    char* end = NULL;
    long v = strtol(env, &end, 10);
    /* Разрешаем хвостовой whitespace, но не прочий мусор. */
    while (*end == ' ' || *end == '\t' || *end == '\r' || *end == '\n') end++;
    if (end == env || *end != '\0' || errno != 0 || v < 1 || v > INT_MAX) {
        fprintf(stderr,
                "nova: invalid NOVA_MAXPROCS=\"%s\" (expected integer >= 1); "
                "falling back to auto-detect\n", env);
        return -1;
    }
    return (int)v;
}

/* Резолвит итоговое число worker'ов из трёх источников по приоритету.
 * `explicit_n` — аргумент runtime.init (<= 0 означает «не задано явно»,
 * т.е. auto-detect). Всегда возвращает значение в [MIN, MAX]. */
int nova_runtime_resolve_maxprocs(int explicit_n) {
    /* (1) Явный аргумент runtime.init(n>0) — высший приоритет. */
    if (explicit_n > 0) {
        return _nova_clamp_maxprocs(explicit_n, "runtime.init");
    }
    /* (2) ENV NOVA_MAXPROCS. */
    int env_n = _nova_parse_maxprocs_env();
    if (env_n > 0) {
        return _nova_clamp_maxprocs(env_n, "NOVA_MAXPROCS");
    }
    /* env_n == 0 (не задано) либо -1 (невалидно — диагностика напечатана):
     * (3) авто-детект, cgroup+affinity-aware. */
    int auto_n = (int)uv_available_parallelism();
    if (auto_n < 1) auto_n = 1;
    return _nova_clamp_maxprocs(auto_n, "uv_available_parallelism");
}

/* ── Init / shutdown ──────────────────────────────────────────── */

void nova_runtime_init(int n_workers) {
    /* Idempotent guard. */
    if (!_init_mu_inited) {
        nova_mutex_init(&_init_mu);
        _init_mu_inited = true;
    }
    nova_mutex_lock(&_init_mu);
    if (_materialized) {
        /* Plan 83.1 Ф.3/Ф.4: runtime.init — одноразовый тюнер, валиден
         * только ДО первого spawn (до материализации пула). Пул уже
         * поднят → init опоздал; диагностируем громко (не молчаливый
         * no-op, маскирующий баг конфигурации), но не abort'им —
         * существующий пул корректен. */
        fprintf(stderr,
                "nova: runtime.init() ignored — M:N pool already materialized "
                "(%d workers); runtime.init is a one-shot tuner, call it "
                "before the first spawn\n", _n_workers);
        nova_mutex_unlock(&_init_mu);
        return;
    }

    /* Plan 52 Ф.22: SipHash seed init upfront — готовность hash до пула. */
    nova_hash_seed_ensure_init();

    /* Plan 83.1 Ф.1+Ф.2: резолв числа worker'ов (explicit > NOVA_MAXPROCS
     * > auto-detect; клэмп [1, 1024]). Ф.4: лишь ЗАПОМИНАЕМ цель — потоки
     * поднимутся лениво на первом spawn. Повторный init до материализации
     * — валидный re-tune (последний выигрывает). */
    _target_workers = nova_runtime_resolve_maxprocs(n_workers);
    _armed = true;

    /* Plan 83.1 Ф.4: auto-shutdown. Регистрируем graceful shutdown на
     * выходе процесса — atexit покрывает нормальный return из main и
     * exit() (для _exit/abort ОС и так освобождает потоки). Один раз;
     * nova_runtime_shutdown идемпотентен (повторный вызов / явный
     * runtime.shutdown() до atexit — безопасны). */
    if (!_atexit_registered) {
        atexit(nova_runtime_shutdown);
        _atexit_registered = true;
    }
    nova_mutex_unlock(&_init_mu);
}

/* Plan 83.1 Ф.4: материализация worker-пула — собственно создание
 * worker-потоков + sysmon. Вызывается ЛЕНИВО при первом worker-bound
 * spawn (через _ensure_materialized). PRECONDITION: _init_mu удержан,
 * _armed == true, _materialized == false. Вызывается только с главного
 * потока — до материализации программа однопоточна. */
static void _materialize_pool(void) {
    int n_workers = _target_workers;
    if (n_workers < 1) n_workers = 1;  /* defensive — резолвер уже клэмпит */

    /* Plan 151 (2026-06-13): зафиксировать native-стек главного потока для
     * GC push_other_roots ДО создания worker'ов. Мы гарантированно на main
     * (см. doc выше: «до материализации программа однопоточна») и main НЕ
     * крутит fiber здесь → NT_TIB.StackBase описывает настоящий native-стек.
     * Без этого: при ≥4 worker'ах GC может сработать во время materialize'а,
     * пока main блокирован в supervised-setup и держит ЕДИНСТВЕННЫЙ корень на
     * heap-замыкание spawn-body; собственная fiber-арена main'а ещё не создана
     * (lazy на первом mco_create) → его стек выпадает из обхода → premature
     * collect замыкания → closure->fn зануляется → worker зовёт NULL (RIP=0),
     * рапортуется VEH как «fiber stack overflow in slot 0».
     * [M-cancellation-test-mono-recursion-overflow] — НЕ моно-рекурсия. */
    nova_fiber_arena_set_main_stack();

#ifdef NOVA_GC_THREADS_REGISTER
    /* Boehm требует разрешения explicit thread registration ПЕРЕД
     * первым GC_register_my_thread. Idempotent — safe вызывать
     * многократно. Without this — register fails с "Threads explicit
     * registering is not previously enabled" error. */
    GC_allow_register_threads();
#endif

    /* Plan 44.5 Layer 5: init main wake handle на nova_evloop()
     * (main thread's default loop — мы сейчас на main thread). Workers
     * сделают uv_async_send(&_main_wake) после fiber complete; main
     * thread в uv_run(UV_RUN_ONCE) проснётся и проверит pending_remote. */
    if (!_main_wake_inited) {
        int rc = uv_async_init(nova_evloop(), &_main_wake, _main_wake_cb);
        if (rc != 0) {
            fprintf(stderr, "nova: uv_async_init main_wake failed: %s\n",
                    uv_strerror(rc));
            abort();
        }
        /* Unref — handle не должен сам keep'ить loop alive. Loop active
         * пока есть active timer/handles из user code (sleep, channels). */
        uv_unref((uv_handle_t*)&_main_wake);
        _main_wake_inited = true;
        /* Plan 83.10.2: init main-loop deferred-close queue. */
        nova_close_queue_init(&_main_close_queue);
        _main_close_queue_inited = true;
        /* [M-183-net2-loop-affinity-cross-thread-op] fix: init main-loop
         * deferred-call queue. */
        nova_call_queue_init(&_main_call_queue);
        _main_call_queue_inited = true;
    }

    _workers = (NovaWorker*)calloc((size_t)n_workers, sizeof(NovaWorker));
    if (!_workers) {
        fprintf(stderr, "nova: runtime_init OOM (%d workers)\n", n_workers);
        abort();
    }
    _n_workers = n_workers;
    nova_aint_init(&_round_robin, 0);
    nova_globrunq_init(&_nova_global_runq);  /* Plan 83-go-cmn Ф.1: overflow queue */

#ifdef NOVA_GC_BOEHM
    /* Plan 82 Ф.3 (§П3): NovaWorker-массив calloc'нут (C-heap, не GC).
     * Каждый w->scope (NovaFiberQueue) держит указатели на nova_alloc'-
     * нутые GC-массивы (fibers / fiber_ctx / fiber_effect_snapshot / …).
     * Без явного root они достижимы лишь из не-сканируемой C-heap →
     * premature collect → UAF. Один GC_add_roots на весь worker-массив
     * (НЕ per-fiber — лимит MAX_ROOT_SETS не задет). Снимается в
     * nova_runtime_shutdown перед free(_workers). */
    GC_add_roots(_workers,
                 (char*)_workers + (size_t)n_workers * sizeof(NovaWorker));
#endif

    /* Plan 211 §7.3 [M-runq-init-steal-race]: расщеплено на 2 фазы (было один
     * цикл init+spawn на итерацию). Воркер i, запущенный uv_thread_create
     * в старой единой итерации, немедленно входит в _worker_main и на первой
     * же попытке steal (nova_runq_steal по всем _n_workers) мог обратиться
     * к _workers[k] для k>i — которые main-поток ещё НЕ дошёл инициализировать
     * (или инициализирует ПРЯМО СЕЙЧАС). TSan подтвердил: write nova_runq_init
     * (main) vs atomic-read nova_runq_grab (уже запущенный воркер), БЕЗ
     * синхронизирующего mutex между ними (runq.h:131↔273). Было безобидно
     * только потому, что calloc уже занулил память ДО цикла (head=0,tail=0,
     * slots=NULL совпадают с nova_runq_init'ными значениями) — везение по
     * совпадению начальных значений, не спроектированный инвариант.
     *
     * Фикс (тот же приём, что Go procresize() строит allp[] под STW ДО
     * того, как любой G встанет на новый P; Tokio строит весь Vec<Worker>
     * до thread::spawn любого из них): Фаза 1 инициализирует КАЖДЫЙ
     * _workers[i] — ни один OS-поток ещё не существует ни для одного
     * воркера, поэтому ничто не может гоняться с этими записями. Фаза 2
     * стартует OS-потоки — к этому моменту весь _workers[] полностью
     * инициализирован, поэтому pthread_create-гарантия («запись создателя
     * ДО create() видна новому потоку») покрывает ВЕСЬ массив для КАЖДОГО
     * воркера, т.к. все записи произошли строго до ПЕРВОГО создания потока.
     * Нулевая цена — тот же объём работы, просто пересортирован. */

    /* Фаза 1: инициализировать КАЖДЫЙ _workers[i]. */
    for (int i = 0; i < n_workers; i++) {
        NovaWorker* w = &_workers[i];
        w->id = i;
        nova_abool_init(&w->stop, false);
        nova_aint_init(&w->pending_count, 0);
        /* Plan 44.7: preemption state — calloc'нуто в 0, инициализируем явно.
         * Single-threaded here (before ANY worker's uv_thread_create) — no
         * race yet, but atomic for consistency with the other 3 touch-sites
         * ([M-211-preempt-flag-plain-race]). */
        __atomic_store_n(&w->preempt_flag, 0, __ATOMIC_RELAXED);
        w->current_fiber_start = 0;
        /* [221.1 #38] container-init: НЕ наследовать ambient deadline /
         * saved_active_scope арм-сайта в вечный worker-scope (см.
         * nova_scope_init_container, fibers.h). */
        nova_scope_init_container(&w->scope);
        /* Plan 83-go-cmn Ф.1: per-worker fixed ring (inline, cannot fail). */
        nova_runq_init(&w->runq);
        /* Plan 44.5 Layer 5 park/wake: pre-alloc scope arrays on main thread
         * (GC-safe) so worker fibers don't call nova_alloc during slot alloc.
         * Also pre-alloc sched_state so park arrays exist before first park. */
        nova_scope_grow(&w->scope, 64);
        (void)nova_sched_get_state(&w->scope);
        /* dispatch_ready hook wires nova_sched_wake → worker deque push. */
        w->scope.dispatch_ready = _worker_dispatch_ready;
        w->scope.dispatch_ctx   = w;
        /* wake_pending: cross-thread fiber re-queue under mutex. */
        nova_mutex_init(&w->wake_mu);
        w->wake_pending       = NULL;
        w->wake_pending_count = 0;
        w->wake_pending_cap   = 0;
        /* Plan 83.7: runnext priority slot — initially empty. */
        w->runnext            = NULL;

        int rc = uv_loop_init(&w->loop);
        if (rc != 0) {
            fprintf(stderr, "nova: uv_loop_init failed: %s\n", uv_strerror(rc));
            abort();
        }
        rc = uv_async_init(&w->loop, &w->wake_handle, _worker_async_cb);
        if (rc != 0) {
            fprintf(stderr, "nova: uv_async_init failed: %s\n", uv_strerror(rc));
            abort();
        }
        w->wake_handle.data = w;
        /* Plan 83.10.2: per-worker deferred-close queue. */
        nova_close_queue_init(&w->close_queue);
        /* [M-183-net2-loop-affinity-cross-thread-op] fix: per-worker
         * deferred-call queue. */
        nova_call_queue_init(&w->call_queue);
    }

    /* Фаза 2: только теперь стартуют OS-потоки — каждый _workers[i] уже
     * полностью инициализирован (см. комментарий выше). */
    for (int i = 0; i < n_workers; i++) {
        NovaWorker* w = &_workers[i];
        int rc = uv_thread_create(&w->thread, _worker_main, w);
        if (rc != 0) {
            fprintf(stderr, "nova: uv_thread_create failed: %s\n", uv_strerror(rc));
            abort();
        }
    }

    /* Plan 44.7: launch sysmon thread — preemption ticker. Started ПОСЛЕ
     * workers (sysmon читает _workers/_n_workers), остановлен ПЕРВЫМ в
     * shutdown (до free(_workers)). */
    nova_abool_init(&_sysmon_running, true);
    if (uv_thread_create(&_sysmon_thread, _sysmon_main, NULL) == 0) {
        _sysmon_started = true;
    } else {
        /* sysmon — best-effort: без него runtime работает, просто без
         * автоматической preemption (остаётся кооперативный yield). */
        _sysmon_started = false;
        nova_abool_store(&_sysmon_running, false);
    }

    _materialized = true;

    /* Plan 83.11 Ф.2: start driver thread AFTER worker pool materialization.
     * Workers must exist before driver routes wake events to them
     * (home_worker_id references _workers[]). */
    nova_driver_init();
}

/* Plan 83.1 Ф.4: гарантирует, что пул материализован. Fast-path без
 * lock'а — после материализации `_materialized` навсегда true (до
 * shutdown). Вызывается из spawn-путей; первый spawn поднимает пул. */
static void _ensure_materialized(void) {
    if (_materialized) return;
    nova_mutex_lock(&_init_mu);
    if (!_materialized && _armed) {
        _materialize_pool();
    }
    nova_mutex_unlock(&_init_mu);
}

void nova_runtime_shutdown(void) {
    if (!_init_mu_inited) return;
    nova_mutex_lock(&_init_mu);
    if (!_armed) {
        nova_mutex_unlock(&_init_mu);
        return;
    }
    if (!_materialized) {
        /* Plan 83.1 Ф.4: armed, но пул так и не материализован (программа
         * вызвала runtime.init, но ни разу не сделала spawn). Потоков нет
         * — join'ить нечего, просто disarm. */
        _armed = false;
        _target_workers = 0;
        nova_mutex_unlock(&_init_mu);
        return;
    }

    /* Plan 83.11 Ф.2: stop driver thread BEFORE workers join — driver
     * routes wake-events to workers; if workers gone first, driver writes
     * to dead worker handles → UAF. */
    int _nv456_diag = getenv("NOVA_DIAG_M456") != NULL;
    if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: before driver_shutdown n_workers=%d\n", _n_workers); fflush(stderr); }
    nova_driver_shutdown();
    if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: after driver_shutdown\n"); fflush(stderr); }

    /* Plan 44.7: stop sysmon ПЕРВЫМ — до free(_workers), чтобы sysmon
     * не читал освобождённую память. join гарантирует тред вышел. */
    if (_sysmon_started) {
        nova_abool_store(&_sysmon_running, false);
        uv_thread_join(&_sysmon_thread);
        _sysmon_started = false;
    }
    if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: after sysmon join\n"); fflush(stderr); }

    /* Signal stop + wake workers. */
    for (int i = 0; i < _n_workers; i++) {
        NovaWorker* w = &_workers[i];
        nova_abool_store(&w->stop, true);
        uv_async_send(&w->wake_handle);
    }
    if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: signaled stop to %d workers\n", _n_workers); fflush(stderr); }

    /* Join. */
    for (int i = 0; i < _n_workers; i++) {
        if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: joining worker %d...\n", i); fflush(stderr); }
        uv_thread_join(&_workers[i].thread);
        if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: joined worker %d\n", i); fflush(stderr); }
    }

    /* Plan 82 Ф.3: worker-потоки join'нуты (мертвы) — освободить их
     * fiber-арены. Эксклюзивный момент: исполняется только main, обход
     * списка арен GC-колбэком/find_arena не конкурирует. */
#if NOVA_FIBER_ARENA_ENABLED
    nova_fiber_arena_release_retired();
#endif

    /* Cleanup. */
    for (int i = 0; i < _n_workers; i++) {
        NovaWorker* w = &_workers[i];
        if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: cleanup worker %d begin loop_alive=%d\n", i, uv_loop_alive(&w->loop)); fflush(stderr); }
        uv_close((uv_handle_t*)&w->wake_handle, NULL);
        /* Run one more tick to process close. */
        uv_run(&w->loop, UV_RUN_NOWAIT);
        /* [M-net-close-teardown-hang] fix (2026-07-11): drain the deferred
         * close/call queues and PUMP the loop BEFORE attempting
         * uv_loop_close, not after. Root cause: the worker's own
         * while(!stop) loop normally services close_queue/call_queue via
         * _worker_async_cb on its next uv_run(NOWAIT) tick — net.c
         * TcpListener/TcpStream .close() (nova_loop_defer_close) and
         * cross-thread net-op issues (nova_loop_defer_call, Plan
         * 183/[M-183-net2-loop-affinity-cross-thread-op]) both go through
         * these queues, even for same-thread callers. If `w->stop` flips
         * (loop above) in the narrow window between a fiber enqueuing such
         * a job and the worker's next iteration, the job is left queued
         * when the worker thread exits and is joined here — nobody left to
         * service that loop. The PREVIOUS order (uv_loop_close THEN drain)
         * was too late: draining after uv_loop_close still calls
         * uv_close()/invokes the deferred fn, but nothing ever calls
         * uv_run() again for this loop afterward, so the close_cb never
         * fires — a live uv_tcp_t/uv_udp_t handle (and its OS socket fd) is
         * silently leaked, never actually closed (uv__finish_close never
         * runs). Drain + pump here instead, while the loop is still open
         * and this cleanup path is the only thread touching it (worker
         * already joined — single-threaded now, no race). Bounded ticks
         * (not an unbounded uv_loop_alive spin) — a genuinely-stuck handle
         * must not turn a leak into a shutdown hang. */
        nova_loop_drain_calls(&w->call_queue);   /* calls first — may enqueue closes */
        nova_loop_drain_closes(&w->close_queue);
        for (int tick = 0; tick < 64 && uv_loop_alive(&w->loop); tick++) {
            uv_run(&w->loop, UV_RUN_NOWAIT);
            /* A drained call can itself enqueue a follow-on close (e.g. a
             * deferred accept-issue that errors and self-closes) — service
             * those too before giving up. */
            nova_loop_drain_calls(&w->call_queue);
            nova_loop_drain_closes(&w->close_queue);
        }
        int _nv456_lc = uv_loop_close(&w->loop);
        if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: cleanup worker %d uv_loop_close=%d\n", i, _nv456_lc); fflush(stderr); }
        /* Plan 83-go-cmn Ф.1: runq is inline (no heap) → nothing to destroy. */
        free(w->wake_pending);
        w->wake_pending = NULL;
        free(w->yielded);          /* Plan 44.7 yielded-FIFO */
        w->yielded = NULL;
        /* Plan 83.6: drain SpawnCtx pool — free retained ctx buffers. */
        _nova_spawn_pool_drain(w);
        /* Plan 83.10.2 / [M-183-net2-loop-affinity-cross-thread-op]: queues
         * already drained above (pre-close, so their close_cb's/fn's
         * actually get pumped) — just tear down the now-empty structures. */
        nova_close_queue_destroy(&w->close_queue);
        nova_call_queue_destroy(&w->call_queue);
        if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: cleanup worker %d done\n", i); fflush(stderr); }
    }
    if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: all worker cleanup done, freeing _workers\n"); fflush(stderr); }

#ifdef NOVA_GC_BOEHM
    /* Plan 82 Ф.3: снять GC-root worker-массива до его free. */
    GC_remove_roots(_workers,
                    (char*)_workers + (size_t)_n_workers * sizeof(NovaWorker));
#endif
    free(_workers);
    _workers = NULL;
    _n_workers = 0;
    _materialized = false;
    _armed = false;
    _target_workers = 0;

    nova_mutex_unlock(&_init_mu);
    if (_nv456_diag) { fprintf(stderr, "[m456] shutdown: nova_runtime_shutdown RETURNING\n"); fflush(stderr); }
}

/* ── Spawn ────────────────────────────────────────────────────── */

void nova_runtime_spawn_global(void (*entry)(mco_coro*), void* user) {
    /* Plan 83.2 Ф.1: auto-arm на первом spawn (default-on M:N). */
    _auto_arm_if_needed();
    if (!_armed) {
        /* Plan 83.2 Ф.1 примечание: ветка теоретически достижима только
         * если _auto_arm_if_needed разпал (resolve_maxprocs OOM), что не
         * происходит на текущем коде — clamp [1,1024] всегда возвращает
         * валидное число. Оставлено как safety net на случай поломки
         * резолвера. */
        if (_nova_active_scope) {
            nova_fiber_spawn_into(_nova_active_scope, entry, user);
        } else {
            fprintf(stderr, "nova: runtime_spawn_global: not armed + no active scope\n");
            abort();
        }
        return;
    }
    /* Plan 83.1 Ф.4: первый worker-bound spawn материализует пул. */
    _ensure_materialized();

    int idx = (int)((uint32_t)nova_aint_inc(&_round_robin) % (uint32_t)_n_workers);
    NovaWorker* target = &_workers[idx];

    /* Plan 44.5 Layer 2: create mco_coro + push в target's deque.
     * nova_fiber_spawn_into push'ит в scope arrays, но мы хотим в deque.
     * Использую low-level mco_create + manual deque push. */
    mco_desc desc = _NOVA_MCO_DESC_INIT(entry);
    desc.user_data = user;
    mco_coro* co = NULL;
    mco_result r = mco_create(&co, &desc);
    if (r != MCO_SUCCESS || co == NULL) {
        fprintf(stderr, "nova: runtime_spawn_global: mco_create failed (%d)\n", (int)r);
        abort();
    }
    nova_fiber_post_create(co);  /* Plan 82 Ф.1: patch ctx.stack_limit (Windows) */
    /* Plan 83.11 Phase B (2026-05-28): route via wake_pending (mutex) instead
     * of direct deque push.
     *
     * Root cause of "1 fiber never starts" bug: nova_deque_push wrote `bottom`
     * (RELAXED) concurrently with the owner worker's nova_deque_pop which also
     * writes `bottom` (RELAXED). On x86 TSO the race is rare but real: if the
     * worker's `bottom = b-1` store lands AFTER main's `bottom = b+1` store, the
     * pushed item strands above `bottom` and is never popped or stolen.
     *
     * Fix: use the existing wake_pending path (mutex-protected MPSC queue),
     * the same path used by _worker_dispatch_ready cross-thread. The worker
     * drains wake_pending → deque at every iteration start, so latency is
     * unchanged (≤ 1 worker loop iteration). The mutex guarantees correct
     * visibility of ctx fields without the SEQ_CST fence hack. */
    __atomic_thread_fence(__ATOMIC_SEQ_CST);  /* visibility of ctx fields */
    nova_mutex_lock(&target->wake_mu);
    if (target->wake_pending_count >= target->wake_pending_cap) {
        int new_cap = target->wake_pending_cap > 0 ? target->wake_pending_cap * 2 : 8;
        target->wake_pending = (mco_coro**)realloc(target->wake_pending,
                                                    (size_t)new_cap * sizeof(mco_coro*));
        if (!target->wake_pending) abort();
        target->wake_pending_cap = new_cap;
    }
    target->wake_pending[target->wake_pending_count++] = co;
    nova_mutex_unlock(&target->wake_mu);
    nova_aint_inc(&target->pending_count);
    uv_async_send(&target->wake_handle);
}

/* Plan 44.5 Layer 5: structured M:N spawn — distribute fiber на worker
 * + tracking в parent scope. Caller (codegen) обязан set
 * ctx->_nova_parent_scope = scope **перед** этим вызовом — entry-функция
 * читает поле для post-completion decrement + signal_main.
 *
 * Release ordering на increment — main thread в supervised_run wait-loop
 * увидит инкремент до того как worker fiber sees decremented count
 * (через cause-effect через memory). */
void nova_runtime_spawn_into(struct NovaFiberQueue* scope,
                              void (*entry)(mco_coro*),
                              void* user) {
    if (!scope) {
        fprintf(stderr, "nova: runtime_spawn_into: NULL scope\n");
        abort();
    }
    /* Plan 83.4.5.7 (2026-05-23): pin SpawnCtx в parent scope's ctx_pins
     * для GC reachability. Без pin'а — ctx достижим только через worker
     * deque slot (malloc'd, не GC-scanned) → Boehm может collect/zero
     * fields ДО worker resume. Симптом: worker reads ctx->_nova_parent_scope
     * == NULL → spawn entry skip'ает preamble + epilogue → main hang в
     * supervised_run_impl wait-loop'е (pending_remote stays > 0). */
    nova_scope_pin_ctx((NovaFiberQueue*)scope, user);
    /* Plan 83.2 Ф.1: auto-arm на первом spawn (default-on M:N).
     * supervised{} использует этот путь через codegen — каждый spawn
     * внутри supervised теперь идёт через worker pool. */
    _auto_arm_if_needed();
    if (!_armed) {
        /* Safety net (см. spawn_global): теоретически недостижимо после
         * _auto_arm_if_needed. */
        nova_fiber_spawn_into((NovaFiberQueue*)scope, entry, user);
        return;
    }
    /* Plan 173.0 Ф.2 (A2.2): assign this remote child its own retention
     * slot in the parent scope's child_error[]/child_ctx[] arrays — on
     * THIS (spawning) thread, before the push, so the slot exists no later
     * than the increment below makes the child visible to the drain loop.
     * -1 sentinel (codegen init) would otherwise persist and disable Ф.2/Ф.3
     * retention for this child (nova_fiber_report_child_kinded /
     * nova_scope_retain_or_release_child both no-op on slot < 0). */
    ((NovaSpawnCtxBase*)user)->_nova_parent_slot =
        nova_scope_alloc_child_slot((NovaFiberQueue*)scope);
    /* Increment ДО push'а — main thread в drain-loop должен видеть
     * pending_remote > 0 даже если worker сразу подхватит fiber и завершит
     * его до того как main опросит counter. */
    nova_aint_inc(&((NovaFiberQueue*)scope)->pending_remote);
    /* Реальный push идёт через spawn_global. */
    nova_runtime_spawn_global(entry, user);
}

/* Plan 44.5 Layer 5: signal main thread из worker context'а.
 * No-op до runtime.init либо после shutdown — main thread в этих режимах
 * либо вообще нет (test'у без init), либо exit'ит (shutdown).
 *
 * Plan 83.10.3 (2026-05-26): nested supervised fix does NOT require broadcasting
 * here. Workers inside pump_scope poll with UV_RUN_NOWAIT + uv_sleep(1) rather
 * than blocking on UV_RUN_ONCE, so they discover pending_remote==0 within 1ms
 * without needing an explicit wakeup signal. Broadcast reverted: it caused
 * uv_async_send storms (16 workers x many completions) that TIMEOUTed
 * plan83_6 spawn-pool tests. */
void nova_runtime_signal_main(void) {
    if (_main_wake_inited) {
        uv_async_send(&_main_wake);
    }
}

/* Plan 83.10.3 (2026-05-26): run one fiber on worker w — extracted logic from
 * _worker_main fiber-run block. Handles context save/restore, CAS guard,
 * parked/dead/yielded post-resume transitions.
 *
 * Caller guarantees: co is MCO_SUSPENDED (or MCO_DEAD for safety check),
 * and we are on worker thread w (_current_worker_id == w->id). */
static void _worker_run_one_fiber(NovaWorker* w, mco_coro* co) {
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    /* [R1-трипваер] ctx обязан выглядеть живым перед resume. */
    if (base && nova_spawn_pool_diag()) {
        nova_spawn_ctx_diag_check_live(base, "run-one-fiber resume");
    }

    __atomic_store_n(&w->preempt_flag, 0, __ATOMIC_RELAXED);  /* [M-211-preempt-flag-plain-race] */
    __atomic_store_n(&w->current_fiber_start, uv_hrtime(), __ATOMIC_RELAXED);

    /* presume-cas-gate (221.1 №446/№447): THE single resume call — see
     * _worker_main for the full rationale. This call site used to be
     * MISSING the "displaced fiber" restore branch (`_nova_worker_slot <=
     * -2`) that the main loop had — a second, narrower instance of the
     * "convention on N sites, not structure" pattern found while
     * unifying; `_nova_resume_restore_ctx_tls` (the SAME hook the main
     * loop and cleanup-drain use) now gives this call site the complete,
     * correct 3-branch restore too. `ro.owned == false` covers CAS-loser
     * AND №446 (co not SUSPENDED — duplicate pop of an already-dead co):
     * caller must not touch co further either way. */
    NovaResumeOutcome ro = nova_resume_fiber(co, base,
        _nova_resume_restore_ctx_tls, _nova_resume_save_ctx_tls);

    __atomic_store_n(&w->current_fiber_start, 0, __ATOMIC_RELAXED);

    if (!ro.owned) return;  /* другой owner, или co не был SUSPENDED: skip dispose */

    if (ro.dead) {
        NovaSpawnCtxBase* dead_ctx = (NovaSpawnCtxBase*)mco_get_user_data(co);
        nova_fiber_state_store(co, NOVA_FIBER_STATE_DEAD);
        mco_destroy(co);
        if (dead_ctx) {
            /* Plan 173.0 Ф.3 (A3.3, R1-guard): retain instead of releasing
             * if this child failed under a supervised parent (see
             * nova_scope_retain_or_release_child, fibers.h). */
            /* [196.6 / D228 §6 class]: unified sweep — see nova_scope_sweep_dead_child. */
            nova_scope_sweep_dead_child(dead_ctx);
        }
    } else {
        /* Yielded (the only other possibility post-resume): */
        if (ro.parked) {
            /* Parked: dispatch_ready (via nova_sched_wake timer/channel cb)
             * will re-queue when ready. No action here. */
        } else {
            /* Voluntary yield: push to yielded-FIFO (not deque — avoids
             * LIFO starvation, matches _worker_main behavior). */
            nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
            _worker_yielded_push(w, co);
        }
    }
}

/* [PERMANENT REGRESSION PROBE — 221.1 №446] trivial fiber body — returns
 * immediately so the coro reaches MCO_DEAD after exactly one resume. */
static void _nova_p446_probe_entry(mco_coro* co) {
    (void)co;
}

/* [PERMANENT REGRESSION PROBE — 221.1 №446] presume-cas-gate window:
 * deterministic, timing-INDEPENDENT proof of `nova_resume_fiber`'s core
 * contract — "a duplicate resume attempt on a co that is NOT MCO_SUSPENDED
 * (already DEAD, in this probe) must return owned=false" — the exact
 * structural invariant that closes №446 (a duplicate pop of an
 * already-dead co used to default to owned=true and fall through to a
 * SECOND mco_destroy + sweep).
 *
 * KEPT PERMANENTLY (not removed post-window): does NOT depend on winning
 * the organic wake_pending-duplicate-push race (which this window's stress
 * fixture — docs/plans/repro/presume_446_stress.nv — did NOT reproduce in
 * ~180 attempts, ~2M+ fiber lifecycles — see that file's header and the
 * window's commit log for the honest result). A probability-based stress
 * test can silently stop catching a future regression if the race window
 * narrows; this probe calls the single resume function twice on the same
 * co directly — exactly the "duplicate pop" shape — and is deterministic
 * pass/fail on every single run. Doubled as the "подсунь негодное"
 * fault-injection proof: with the CAS gate deliberately reverted to the
 * pre-fix default (`out.owned = true` outside the MCO_SUSPENDED branch),
 * this probe's assertion goes red immediately (`ro2.owned=1` instead of
 * `0`) — see window report for the exact captured output.
 *
 * Returns 1 iff the CORRECT (fixed) behavior held for all three checks;
 * 0 if ANY check failed (i.e. the №446 defect is present). */
nova_int nova_p446_sabotage_probe(void) {
    static NovaSpawnCtxBase probe_ctx;
    memset(&probe_ctx, 0, sizeof(probe_ctx));

    mco_desc desc = _NOVA_MCO_DESC_INIT(_nova_p446_probe_entry);
    desc.user_data = &probe_ctx;
    mco_coro* co = NULL;
    mco_result r = mco_create(&co, &desc);
    if (r != MCO_SUCCESS || co == NULL) {
        fprintf(stderr, "nova: p446 probe fiber create failed (%d)\n", (int)r);
        abort();
    }

    /* First "pop": fresh, MCO_SUSPENDED, state IDLE (zero-init) — must WIN
     * the CAS and run the trivial entry to completion (MCO_DEAD). */
    NovaResumeOutcome ro1 = nova_resume_fiber(co, NULL, NULL, NULL);

    /* Second "pop" of the SAME co pointer — the duplicate-push shape from
     * registry №446. co is now MCO_DEAD, not MCO_SUSPENDED. The correct
     * (fixed) contract: owned=false, untouched. The pre-fix contract
     * (sabotage target): `_nova_state_owned` defaulted true here, and the
     * caller would go on to mco_destroy(co) A SECOND TIME. */
    NovaResumeOutcome ro2 = nova_resume_fiber(co, NULL, NULL, NULL);

    int ok = ro1.owned && ro1.dead && !ro2.owned;
    fprintf(stderr,
        "[p446-probe] ro1.owned=%d ro1.dead=%d ro2.owned=%d ro2.dead=%d "
        "→ %s\n",
        (int)ro1.owned, (int)ro1.dead, (int)ro2.owned, (int)ro2.dead,
        ok ? "CORRECT (№446 closed)" : "DEFECT REPRODUCED (№446 live)");
    fflush(stderr);

    mco_destroy(co);  /* the ONE legitimate destroy, done by the probe itself */
    return ok ? 1 : 0;
}

/* ── Plan 83.10.2 (2026-05-26): nova_runtime_cancel_worker_fibers ───
 *
 * Under armed M:N, fiber preamble sets _nova_active_scope = &w->scope
 * (worker scope). Timer/channel stop_cbs are registered in &w->scope's
 * sched_state. The supervised cancel-token is bound to the supervised
 * scope. When tok.cancel() fires, nova_sched_cancel_all_pending(supervised_
 * scope) finds nothing because stop_cbs live in worker scopes.
 *
 * This function walks all worker scopes and cancels slots where the
 * fiber's _nova_parent_scope == target_scope. Stop_cb uses nova_loop_
 * defer_close (Plan 83.10.2 Ф.3) so the dispatch is always on the
 * owning loop's thread — safe for cross-thread cancel.
 *
 * Memory ordering (Plan 83.10.2 fix):
 *   pending_stop_cb is written via RELEASE store in nova_sched_register_pending.
 *   We must read it via ACQUIRE load so that if we observe a non-NULL value,
 *   we are guaranteed to also see the pending_handle write that preceded the
 *   RELEASE store. Without this, the compiler + CPU (on ARM in particular,
 *   but also theoretically on x86 with compiler reordering) could observe
 *   pending_stop_cb=NULL even after the fiber completed register_pending,
 *   causing cancel_worker_fibers to miss the fiber → permanent park → TIMEOUT.
 *   This is the root cause of the heisenbug where fprintf "fixed" the hang
 *   (fprintf's stdio lock is a full memory fence). */
void nova_runtime_cancel_worker_fibers(struct NovaFiberQueue* target_scope) {
    /* [M-187-sse-live-tls-server-hang] fix (2026-07-15): this function used to
     * return UNCONDITIONALLY once the driver started (Plan 83.11 Ф.3), on the
     * theory that "Driver's CANCEL_SCOPE job handles all sleep-related cancel
     * through single-mutator armed_sleeps_head walk. Running legacy too
     * creates double-wake race" — true for Time.sleep, but the blanket
     * early-return ALSO silently dropped cancellation for every OTHER
     * pending_stop_cb-registered park (TCP connect/read/write, TLS handshake/
     * read/write, UDP, DNS — every `nova_sched_register_pending` call site in
     * net.c) once the driver is up, which in this M:N runtime is almost
     * immediately after the FIRST spawn (`_materialize_pool` calls
     * `nova_driver_init()` right after worker-pool materialization). A
     * `supervised(deadline:)` scope whose child is genuinely mid a real
     * network op (aggregator flagship's live-TLS lanes: DNS resolve → TCP
     * connect → TLS handshake → read, `examples/flagship/aggregator/src/
     * app/live.nv`'s `live_fetch_weather`) at deadline-elapse would call
     * `nova_scope_deliver_cancel` → this fn → no-op → the fiber's async
     * handle is never closed → `pending_remote` never decrements →
     * `nova_supervised_run_impl`'s wait loop spins forever (confirmed:
     * repro'd with an artificially-tightened `LIVE_BUDGET_MS`, 100% CPU on
     * the worker thread, no watchdog dump — reached `alive==0`/`remote>0`
     * every iteration but the child fiber was never actually stuck-alive,
     * just genuinely still parked on a real, uncancelled network op).
     *
     * Root cause was the BLANKET skip, not the sleep-vs-network distinction
     * itself — `_nova_sleep_via_driver` (fibers.h) never calls
     * `nova_sched_register_pending` at all (it parks via `armed_sleeps_head`
     * + a bare `parked[slot]` flag, no stop_cb), so a registered stop_cb
     * (`cb && hdl` below) can ONLY belong to a net.c-style async op — never
     * a driver-routed sleep. Running the `cb(hdl)` branch unconditionally is
     * therefore safe in driver mode: it can never race the driver's own
     * armed_sleeps_head walk (disjoint fiber sets). The double-wake race the
     * original comment warned about is specifically the BARE-park fallback
     * branch (`else if (parked_at)`, no stop_cb) — THAT one still matches a
     * driver-routed sleeping fiber (bare-parked, no stop_cb, same as a
     * network op that hasn't registered its pending handle yet) and must
     * stay driver-exclusive to avoid double-dispatch. Fix: gate only the
     * bare-park fallback on driver-mode; let the stop_cb branch run always. */
    bool driver_mode = nova_driver_is_started();
    /* Cast from `struct NovaFiberQueue*` (forward-declared in runtime.h) to
     * `NovaFiberQueue*` (anonymous typedef from fibers.h) for sched helpers. */
    NovaFiberQueue* tscope = (NovaFiberQueue*)target_scope;
    if (!tscope || !_materialized) return;
    for (int i = 0; i < _n_workers; i++) {
        NovaWorker* w = &_workers[i];
        NovaSchedState* st = nova_sched_find_state(&w->scope);
        /* ACQUIRE-load on count pairs with the RELEASE-store in
         * nova_scope_alloc_slot, ensuring we see fibers[slot]=co when
         * we observe count=slot+1. */
        int wcount = (int)__atomic_load_n(&w->scope.count, __ATOMIC_ACQUIRE);
        if (!st) continue;
        int _cap = nova_sched_cap_acq(st);
        int n = wcount < _cap ? wcount : _cap;
        for (int j = 0; j < n; j++) {
            mco_coro* co = (j < wcount) ? w->scope.fibers[j] : NULL;
            NovaSpawnCtxBase* base = co ? (NovaSpawnCtxBase*)mco_get_user_data(co) : NULL;
            if (!co || mco_status(co) == MCO_DEAD) continue;
            if (!base || base->_nova_parent_scope != tscope) continue;
            /* ACQUIRE-load: pairs with RELEASE store in register_pending.
             * Guarantees visibility of pending_handle when stop_cb != NULL. */
            NovaSchedStopCb cb;
            __atomic_load(nova_sched_pending_stop_cb_at(st, j), &cb, __ATOMIC_ACQUIRE);
            void* hdl = *nova_sched_pending_handle_at(st, j);  /* visible after ACQUIRE on cb */
            if (cb && hdl) {
                /* ASYNC stop_cb: initiates cross-thread safe uv_close via
                 * nova_loop_defer_close; close_cb wakes fiber afterward. Never
                 * a driver-routed sleep (see fn header) — safe unconditionally,
                 * including driver mode. This is the actual fix: previously
                 * unreachable whenever driver_mode was true. */
                cb(hdl);
            } else if (!driver_mode && j < nova_sched_cap_acq(st) && *nova_sched_parked_at(st, j)) {
                /* Bare park (no registered stop_cb): direct dispatch_ready.
                 * Driver-exclusive when driver_mode — a driver-routed sleep
                 * is ALSO bare-parked with no stop_cb; touching it here too
                 * would double-wake against the driver's own CANCEL_SCOPE/
                 * armed_sleeps_head close_cb CAS (original heisenbug this fn
                 * documents). Unchanged from pre-fix behavior when driver is
                 * not started (bootstrap/single-thread mode). */
                nova_sched_wake(&w->scope, j);
            }
            /* else: fiber not yet parked; FIX 2b in _nova_sleep_via_libuv
             * will self-initiate close after register_pending completes. */
        }
    }
}

/* Plan 83.10.3 (2026-05-26): pump current worker's runnext+deque for a
 * fiber belonging to scope q. Called from nova_supervised_run_impl when
 * alive==0 && pending_remote>0 on a worker thread (nested supervised case).
 *
 * Strategy:
 *   1. Service our UV loop (timers, async callbacks) non-blockingly.
 *   2. Drain wake_pending queue (cross-thread fiber wakeups).
 *   3. Pop from runnext then deque.
 *   4a. If fiber belongs to scope q → resume inline via _worker_run_one_fiber.
 *   4b. If fiber belongs to different scope → push back + UV_RUN_NOWAIT + sleep(1);
 *       return so outer loop re-checks pending_remote.
 *   5. If deque empty → UV_RUN_NOWAIT + uv_sleep(1) poll (1ms latency max
 *      to detect pending_remote decrement by completing remote fiber). */
void nova_runtime_worker_pump_scope(struct NovaFiberQueue* scope) {
    int wid = _current_worker_id;
    if (wid < 0 || wid >= _n_workers || !_workers) return;
    NovaWorker* w = &_workers[wid];

    /* (1) Service UV loop non-blockingly (timers, deferred dispatches). */
    uv_run(&w->loop, UV_RUN_NOWAIT);

    /* (2) Drain cross-thread wake_pending list into the ring. */
    nova_mutex_lock(&w->wake_mu);
    for (int i = 0; i < w->wake_pending_count; i++) {
        nova_runq_put(&w->runq, &_nova_global_runq, w->wake_pending[i]);
    }
    w->wake_pending_count = 0;
    nova_mutex_unlock(&w->wake_mu);

    /* (3) Pop candidate: runnext priority, then ring, then global overflow. */
    mco_coro* co = NULL;
    if (w->runnext) {
        co = w->runnext;
        w->runnext = NULL;
    }
    if (!co) co = nova_runq_get(&w->runq);
    /* Plan 83.4.5.12 (2026-07-15): also drain THIS worker's yielded-FIFO —
     * cooperatively-preempted fibers (Plan 44.7 sysmon preemption / runtime.
     * yield) land here via _worker_run_one_fiber (line ~1980), which pump_scope
     * itself calls at step (4a). While the worker is blocked in this nested-
     * supervised pump it never returns to _worker_main's own drain
     * (_worker_yielded_pop, line ~890), so a yielded child of the very scope
     * being pumped would be black-holed → its pending_remote never reaches 0 →
     * permanent supervised hang ([M-187-supervised-nested-fiber-slot-race]:
     * dump shows the fiber SUSPENDED-not-parked with a non-empty yielded-FIFO).
     * Same class + same fix shape as the global-overflow consumer just below
     * (the "MUST run as a consumer here" note in _worker_main). Ordering
     * mirrors _worker_main exactly: runnext → runq → yielded → global. */
    if (!co) co = _worker_yielded_pop(w);
    /* Ф.1: also consult the global overflow queue — a nested-supervised pump
     * must not strand spilled fibers (compounds the overflow black-hole). */
    if (!co) co = nova_globrunq_get_one(&_nova_global_runq);

    if (!co) {
        /* (5) Nothing ready — poll UV loop non-blockingly, then sleep 1ms.
         * Pending_remote is decremented by the completing fiber on another worker;
         * the outer supervised_run_impl loop re-checks pending_remote after we
         * return, so 1ms polling gives sub-millisecond detection without needing
         * an explicit broadcast wake. */
        uv_run(&w->loop, UV_RUN_NOWAIT);
        uv_sleep(1);
        return;
    }

    /* (4) [M-187-high-concurrency-connection-wedge] fix (2026-07-19): run ANY
     * popped fiber inline, WORK-CONSERVING — do NOT special-case "belongs to our
     * scope" by pushing a foreign fiber back.
     *
     * The old (4b) push-back-and-spin was a cross-worker deadlock: under a
     * connection storm (MAXPROCS≥2, MAX_INFLIGHT>2) every worker blocks inside a
     * nested supervised_run_impl pump, each holding a SIBLING scope's ready child
     * in its own deque. Because each pump only ran fibers of ITS OWN scope and
     * pushed every foreign child straight back to the same ring, no worker ever
     * ran another worker's child → no child completed → no parent's
     * pending_remote ever decremented → permanent wedge (watchdog:
     * `[supervised] count=0 pending_remote=1` + `STUCK_ALIVE_NOT_PARKED` fibers
     * with a registered net handle sitting un-run in a size>0 deque). Proven by
     * the MAXPROCS discriminator: 1 worker survives (no sibling to strand on),
     * ≥2 wedges; and this is a lost-wake/stuck-completion, NOT the SpawnCtx/GC
     * corruption closed separately by the child_error[] uncollectable fix.
     *
     * Running any ready fiber is always safe here (_worker_run_one_fiber saves +
     * restores the outer active-scope/slot TLS, identical to the own-scope path)
     * and always makes global progress: a foreign fiber's eventual park or
     * completion decrements ITS OWN parent's pending_remote, freeing that parent
     * to return from its pump — the cycle unwinds. The outer supervised_run_impl
     * re-checks OUR pending_remote after we return, exactly as before. This is
     * the same work-conserving discipline _worker_main already uses (it runs
     * whatever it pops, never scope-filtered). */
    _worker_run_one_fiber(w, co);
}

/* ── Plan 83.10.2 (2026-05-26): nova_loop_defer_close ───────────────
 *
 * Schedule a uv_close for `handle` on the thread that owns `loop`.
 * Thread-safe — may be called from any thread (e.g. cancel on main,
 * timer on worker). Enqueues the job on the matching NovaDeferredCloseQueue
 * then uv_async_send's the loop's wake handle so it drains promptly.
 *
 * Loop resolution:
 *   main loop (nova_evloop()) → _main_close_queue + _main_wake
 *   worker loop (&w->loop)   → w->close_queue + w->wake_handle
 *
 * Returns 0 on success, -1 on unrecognised loop or OOM. */
int nova_loop_defer_close(uv_loop_t* loop,
                          uv_handle_t* handle,
                          uv_close_cb close_cb) {
    if (!loop || !handle || !close_cb) return -1;

    /* Find the queue + async wake handle for this loop. */
    NovaDeferredCloseQueue* q    = NULL;
    uv_async_t*             wake = NULL;

    if (_main_wake_inited && loop == nova_evloop()) {
        q    = &_main_close_queue;
        wake = &_main_wake;
    } else {
        for (int i = 0; i < _n_workers; i++) {
            if (&_workers[i].loop == loop) {
                q    = &_workers[i].close_queue;
                wake = &_workers[i].wake_handle;
                break;
            }
        }
    }

    if (!q || !wake) {
        /* Cooperative single-thread path: _main_wake not inited (no workers
         * spawned), but the loop IS the main event loop — uv_close is safe
         * because we are on the main thread and no cross-thread races exist. */
        if (!_main_wake_inited && loop == nova_evloop()) {
            uv_close(handle, close_cb);
            return 0;
        }
        /* Unknown loop — caller bug. Fall back to direct close as last resort
         * (best-effort; may be UB if cross-thread, but better than assert). */
        fprintf(stderr,
            "nova: nova_loop_defer_close: unknown loop %p (caller bug) "
            "— falling back to direct uv_close (may be cross-thread UB)\n",
            (void*)loop);
        uv_close(handle, close_cb);
        return -1;
    }

    /* Enqueue the job under lock. */
    nova_mutex_lock(&q->mu);
    if (q->count >= q->cap) {
        int new_cap = q->cap > 0 ? q->cap * 2 : 8;
        NovaDeferredCloseJob* new_jobs = (NovaDeferredCloseJob*)realloc(
            q->jobs, (size_t)new_cap * sizeof(*new_jobs));
        if (!new_jobs) {
            nova_mutex_unlock(&q->mu);
            /* OOM — fall back to direct close (UB if cross-thread, but rare). */
            uv_close(handle, close_cb);
            return -1;
        }
        q->jobs = new_jobs;
        q->cap  = new_cap;
    }
    q->jobs[q->count].handle   = handle;
    q->jobs[q->count].close_cb = close_cb;
    q->count++;
    nova_mutex_unlock(&q->mu);

    /* Wake the loop thread so it drains the queue promptly. */
    uv_async_send(wake);
    return 0;
}

/* [M-183-net2-loop-affinity-cross-thread-op] fix: nova_loop_defer_call —
 * generic version of nova_loop_defer_close above (same loop-resolution +
 * mutex-queue + uv_async_send shape), for marshalling an arbitrary uv-op
 * ISSUE call (uv_read_start/uv_write/uv_tcp_init+uv_accept/uv_udp_send/
 * uv_udp_recv_start) onto the thread that owns the target handle's loop.
 *
 * Callers (net.c) only invoke this on a genuine cross-thread mismatch
 * (`nova_current_loop() != handle->loop`); the fast/common same-thread path
 * calls `fn(arg)` directly without going through this queue at all. */
int nova_loop_defer_call(uv_loop_t* loop, NovaDeferredCallFn fn, void* arg) {
    if (!loop || !fn) return -1;

    NovaDeferredCallQueue* q    = NULL;
    uv_async_t*            wake = NULL;

    if (_main_wake_inited && loop == nova_evloop()) {
        q    = &_main_call_queue;
        wake = &_main_wake;
    } else {
        for (int i = 0; i < _n_workers; i++) {
            if (&_workers[i].loop == loop) {
                q    = &_workers[i].call_queue;
                wake = &_workers[i].wake_handle;
                break;
            }
        }
    }

    if (!q || !wake) {
        /* Cooperative single-thread path: no workers materialized, loop IS
         * the main loop — direct call is safe (single thread, no race). */
        if (!_main_wake_inited && loop == nova_evloop()) {
            fn(arg);
            return 0;
        }
        /* Unknown loop — caller bug. Fall back to a direct (possibly
         * cross-thread) call as last resort — logged, not silently UB. */
        fprintf(stderr,
            "nova: nova_loop_defer_call: unknown loop %p (caller bug) "
            "— falling back to direct call (may be cross-thread UB)\n",
            (void*)loop);
        fn(arg);
        return -1;
    }

    nova_mutex_lock(&q->mu);
    if (q->count >= q->cap) {
        int new_cap = q->cap > 0 ? q->cap * 2 : 8;
        NovaDeferredCallJob* new_jobs = (NovaDeferredCallJob*)realloc(
            q->jobs, (size_t)new_cap * sizeof(*new_jobs));
        if (!new_jobs) {
            nova_mutex_unlock(&q->mu);
            /* OOM — fall back to direct call (UB if cross-thread, but rare). */
            fn(arg);
            return -1;
        }
        q->jobs = new_jobs;
        q->cap  = new_cap;
    }
    q->jobs[q->count].fn  = fn;
    q->jobs[q->count].arg = arg;
    q->count++;
    nova_mutex_unlock(&q->mu);

    uv_async_send(wake);
    return 0;
}

/* ── Plan 83.4.5.2 Ф.1: orphan fiber pool ─────────────────────────
 *
 * Global cooperative-fallback scope для `detach { body }` под bootstrap.
 * Под armed runtime orphan fibers идут directly через
 * nova_runtime_spawn_global (worker round-robin); orphan scope тогда
 * используется только для diagnostics (если worker fiber'у нужен home
 * scope reference).
 *
 * Semantics (паритет Go runtime.newproc orphan goroutines / tokio
 * tokio::spawn без JoinHandle):
 *   - Spawn возвращается мгновенно (fire-and-forget).
 *   - Body's errors → LogAndDrop (fprintf stderr, никаких re-throw'ов).
 *   - Drain on atexit обеспечивает что bootstrap-cooperative orphans
 *     отработают перед process exit.
 *   - Каллер может explicit `runtime.drain_orphans()` для test-suite
 *     sync (Go `sync.WaitGroup.Wait` analog).
 *
 * Реализация — НЕ под мьютексом в hot-path: cooperative bootstrap
 * single-thread; armed runtime обходит orphan_scope (spawn_global
 * round-robin). Только init/destroy под mutex (rare events). */
static NovaFiberQueue _nova_orphan_scope;
static bool           _nova_orphan_scope_inited = false;
static bool           _nova_orphan_atexit_registered = false;
static nova_mutex_t   _nova_orphan_mu;
static bool           _nova_orphan_mu_inited = false;

/* Lazy-init orphan scope state + register atexit drain. Idempotent.
 * Mutex-protected — может вызываться cross-thread (если armed runtime
 * вызывает spawn_orphan из worker context'а — теоретически не должен,
 * но защитимся). */
static void _orphan_scope_ensure_init(void) {
    if (!_nova_orphan_mu_inited) {
        nova_mutex_init(&_nova_orphan_mu);
        _nova_orphan_mu_inited = true;
    }
    nova_mutex_lock(&_nova_orphan_mu);
    if (!_nova_orphan_scope_inited) {
        /* Plan 22 Ф.7 nova_scope_init: heap-init lazy arrays.
         * [221.1 #38] container-init: orphan scope арм-ится лениво на первом
         * detach — тот же класс порчи, что worker-scope (стейл ambient
         * deadline навсегда в static-структуре); плюс detach семантически
         * СЕВЕРИТ deadline родителя (D349/D50). */
        nova_scope_init_container(&_nova_orphan_scope);
        _nova_orphan_scope_inited = true;
    }
    if (!_nova_orphan_atexit_registered) {
        /* Drain перед exit'ом гарантирует bootstrap orphans завершат
         * body. atexit вызывается ДО уничтожения static state'а. */
        atexit(nova_runtime_drain_orphans);
        _nova_orphan_atexit_registered = true;
    }
    nova_mutex_unlock(&_nova_orphan_mu);
}

void nova_runtime_spawn_orphan(void (*entry)(mco_coro*), void* user) {
    _orphan_scope_ensure_init();
    /* Plan 83.4.5.2: armed branch → push в worker deque напрямую
     * (worker pool обрабатывает; no scope binding — fiber orphan).
     * NovaSpawnCtxBase._nova_parent_scope = NULL → entry-функция знает
     * что нет scope для pending_remote / error reporting → LogAndDrop
     * path активируется при throw'ах. */
    if (_armed) {
        /* Под armed orphan goes directly в worker pool. Caller (codegen)
         * уже set ctx->_nova_parent_scope = NULL (см. emit_detach). */
        nova_runtime_spawn_global(entry, user);
        return;
    }
    /* Bootstrap fallback: cooperative spawn в orphan scope queue. */
    nova_fiber_spawn_into(&_nova_orphan_scope, entry, user);
}

/* Plan 83.4.5.8 (2026-05-24): explicit init для orphan scope.
 * Lazy-init guard повторно использует _orphan_scope_ensure_init. */
void nova_runtime_orphan_scope_init(void) {
    _orphan_scope_ensure_init();
}

/* Plan 83.4.5.10 Ф.3 (2026-05-24): cached inline-threshold для parallel-for.
 * Race-tolerant lazy init — multiple threads converge к одному значению;
 * intermediate -1 → один extra getenv (harmless). После warm-up — lock-free
 * read одной memory location. */
long nova_runtime_parallel_inline_threshold(void) {
    static long _cached_threshold = -1;
    long v = _cached_threshold;
    if (v >= 0) return v;
    const char* env = getenv("NOVA_PARALLEL_INLINE_THRESHOLD");
    if (env && env[0] != '\0') {
        char* end = NULL;
        long parsed = strtol(env, &end, 10);
        v = (end != env && parsed >= 0) ? parsed : 32;
    } else {
        v = 32;  /* default: ~16-32 worker overhead × default 16-32 short iters */
    }
    _cached_threshold = v;
    return v;
}

/* Plan 83.4.5.8 (2026-05-24): public pointer на orphan scope.
 * Returns NULL если scope ещё не initialized. Используется codegen
 * emit_detach под armed: set ctx->_nova_parent_scope =
 * nova_runtime_orphan_scope() чтобы fiber tracking шёл через
 * pending_remote counter (как supervised children). */
struct NovaFiberQueue* nova_runtime_orphan_scope(void) {
    if (!_nova_orphan_scope_inited) return NULL;
    return (struct NovaFiberQueue*)&_nova_orphan_scope;
}

void nova_runtime_drain_orphans(void) {
    /* Если scope ни разу не initialized — нечего drain'ить (programs
     * без detach'ей). */
    if (!_nova_orphan_scope_inited) return;
    /* Plan 83.4.5.2 bugfix (2026-05-23): mutex НЕ держим во время drain.
     * Inner-detach из тела outer-orphan вызовет spawn_orphan →
     * _orphan_scope_ensure_init, который пытается взять тот же mutex →
     * deadlock (non-recursive POSIX mutex). Под bootstrap drain
     * single-threaded — race не существует. Под armed runtime drain
     * вызывается с main thread; workers НЕ зовут drain. Init mutex
     * нужен только для lazy-init под потенциальным cross-thread spawn,
     * не для drain. */
    nova_supervised_drain_main_scope(&_nova_orphan_scope);
    /* После drain orphan scope's q->count = 0 — готов к re-use. */
}

/* ── Diagnostic ───────────────────────────────────────────────── */

/* Фактически поднятые worker-потоки. Plan 83.1 Ф.4: 0 до первого spawn
 * (пул ленивый), даже если runtime.init() уже вызван — для целевого
 * числа см. nova_runtime_maxprocs(). */
int nova_runtime_worker_count(void) {
    return _n_workers;
}

/* Plan 83.1 Ф.3: целевое число worker'ов (аналог Go runtime.GOMAXPROCS(-1)).
 * Отличается от worker_count(): maxprocs() — ЦЕЛЬ (резолвится и до
 * runtime.init, и после shutdown), worker_count() — фактически поднятые
 * потоки (с lazy-spawn Ф.4 это 0 пока не было первого spawn).
 *
 * Если пул поднят — возвращает реальное число. Иначе резолвит цель
 * (NOVA_MAXPROCS / auto-detect) и кэширует: target детерминирован, а
 * кэш не даёт повторно печатать clamp/invalid-диагностику на каждом
 * вызове getter'а. Race на первой инициализации кэша безвреден —
 * резолвер детерминирован, оба потока запишут одно значение. */
static int _maxprocs_cache = 0;  /* 0 = ещё не резолвилось */

int nova_runtime_maxprocs(void) {
    /* Plan 83.1 Ф.4: armed → возвращаем зафиксированную цель (потоки
     * могут быть ещё не подняты). Иначе резолвим default + кэшируем. */
    if (_armed) return _target_workers;
    if (_maxprocs_cache == 0) {
        _maxprocs_cache = nova_runtime_resolve_maxprocs(0);
    }
    return _maxprocs_cache;
}

int nova_runtime_current_worker_id(void) {
    return _current_worker_id;
}

/* Plan 83.1 Ф.4: «M:N запрошен» — runtime.init() вызван (пул может быть
 * ещё не материализован — это lazy). worker_count() == 0 до первого
 * spawn даже при is_initialized() == true. */
bool nova_runtime_is_initialized(void) {
    return _armed;
}
