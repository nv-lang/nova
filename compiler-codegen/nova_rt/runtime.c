// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 44 (M:N Этап 0, 2026-05-13) — multi-thread runtime impl.
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
    /* Plan 44.5 Layer 2: Chase-Lev deque вместо mutex+scope push.
     * Lock-free owner ops, lock-free CAS steals. */
    NovaDeque         deque;
    /* scope остаётся для cancellation propagation и fiber bookkeeping —
     * но fiber dispatch идёт через deque. */
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
     * `preempt_flag` — plain `volatile int`, НЕ снапшот: codegen safepoint
     * (nova_preempt_check) читает его ВЖИВУЮ через TLS-указатель
     * `_nova_preempt_ptr`, выставленный в _worker_main на &w->preempt_flag.
     * Снапшот не годится — worker thread застревает внутри mco_resume на
     * весь CPU-loop и не может перечитать флаг; sysmon выставляет его уже
     * после старта fiber'ы. Single producer (sysmon) + single consumer
     * (бегущая на этом worker'е fiber) для 0/1 флага — volatile достаточно
     * (Go так же делает non-atomic write в stackguard0). `current_fiber_start`
     * — torn-read safe через __atomic_* (sysmon читает, worker пишет). */
    uint64_t          current_fiber_start;  /* __atomic_* accessed */
    volatile int      preempt_flag;
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
 * Hello-world без spawn — `_armed=true`, но `_materialized=false`
 * (пул не поднят) → 0 worker-потоков (Plan 83.2 §4 acceptance).
 * Идемпотентно, thread-safe через _init_mu. */
static bool _nova_autoarm_env_disabled(void);  /* fwd-decl, def ниже */

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
 * env-name; replaces legacy `NOVA_NO_AUTOARM=1`). */
void nova_runtime_auto_arm(void) {
    if (_nova_autoarm_env_disabled()) return;
    _auto_arm_if_needed();
}

/* Plan 44.5 Layer 5: main wake handle для cross-thread signal'а из
 * worker'а в main thread'а supervised_run wait-loop. Init'ится в
 * nova_runtime_init на nova_evloop (main thread's default loop). */
static uv_async_t      _main_wake;
static bool            _main_wake_inited = false;

static void _main_wake_cb(uv_async_t* h) {
    (void)h;
    /* No-op — signal itself wakes uv_run(UV_RUN_ONCE) в main thread'е.
     * Main thread сам проверяет scope.pending_remote после wake'а. */
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
                w->preempt_flag = 1;  /* живой флаг — fiber перечитает */
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
    if (size == 0) {
        /* Allocation went через oversize/legacy path — direct Boehm free. */
        nova_free_uncollectable(ctx);
        return;
    }
    int cls = _nova_spawn_pool_class(size);
    if (cls < 0) {
        nova_free_uncollectable(ctx);
        return;
    }

    int wid = _current_worker_id;
    if (wid < 0) {
        /* Main thread free path — pool not available. Direct Boehm. */
        nova_free_uncollectable(ctx);
        return;
    }

    NovaWorker* w = &_workers[wid];
    if (w->spawn_pool_count[cls] >= NOVA_SPAWN_POOL_MAX_PER_CLASS) {
        /* Pool capped — excess Boehm free. */
        nova_free_uncollectable(ctx);
        return;
    }

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
    for (int cls = 0; cls < NOVA_SPAWN_POOL_SIZE_CLASSES; cls++) {
        void* head = w->spawn_pool_free[cls];
        while (head) {
            void* next = *(void**)head;
            nova_free_uncollectable(head);
            head = next;
        }
        w->spawn_pool_free[cls] = NULL;
        w->spawn_pool_count[cls] = 0;
    }
}

/* ── Worker main ──────────────────────────────────────────────── */

/* uv_async callback — fires when cross-worker spawn pushes fiber.
 * Просто wakes uv_run; actual drain делается в worker loop. */
static void _worker_async_cb(uv_async_t* h) {
    (void)h;
    /* No-op — wake-up itself is the signal. */
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
            nova_deque_push(&w->deque, prev);
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

    /* Per-worker TLS: _nova_active_scope указывает на own scope.
     * Объявлены в fibers.h cross-platform; здесь только set. */
    _nova_active_scope = &w->scope;
    _nova_active_slot  = -1;

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
            nova_deque_push(&w->deque, w->wake_pending[i]);
        }
        w->wake_pending_count = 0;
        nova_mutex_unlock(&w->wake_mu);

        mco_coro* co = NULL;

        /* Plan 83.7 (2026-05-25): (1.9) runnext priority slot — woken
         * fiber from same-thread dispatch_ready (channel recv → handler
         * spawn re-wake same-worker chain). Cache-warm vs going through
         * deque tail. Same-thread access — plain pointer. */
        if (w->runnext) {
            co = w->runnext;
            w->runnext = NULL;
        }

        /* (2) Local deque — owner LIFO pop. Wait-free hot path. Свежие
         * spawn'ы + разбуженные fiber'ы (приоритет — они progress'ят). */
        if (!co) {
            co = (mco_coro*)nova_deque_pop(&w->deque);
        }

        /* (2.5) Plan 44.7: yielded-FIFO — кооперативно вытесненные fiber'ы.
         * После deque, до steal: своя preempted-работа продвигается, но
         * уступает свежим/разбуженным. FIFO → честный round-robin между
         * несколькими CPU-bound fiber'ами. */
        if (!co) {
            co = _worker_yielded_pop(w);
        }

        /* (3) Idle — try steal у соседей (FIFO from their deque top). */
        if (!co) {
            for (int i = 0; i < _n_workers; i++) {
                if (i == w->id) continue;
                co = (mco_coro*)nova_deque_steal(&_workers[i].deque);
                if (co) break;
            }
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
         * Plan 44.5 Layer 5 fix: save/restore _nova_fail_top, _nova_interrupt_top,
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

        /* Restore fiber's TLS snapshot (fail-top chain + active scope/slot).
         *
         * Plan 44.5 deadlock fix (work-stealing): a fiber's home scope
         * (_nova_fiber_scope) is fixed to the worker that ran its preamble.
         * If stolen by another worker, we MUST restore _nova_active_scope to
         * the home scope so channel ops capture the correct scope/slot.
         * Without this, the channel waiter records the stealer's scope, and
         * nova_sched_wake finds scope->fibers[slot]=NULL → dispatch_ready not
         * called → fiber never re-queued → permanent hang (deadlock). */
        NovaFiberQueue*     outer_scope     = _nova_active_scope;
        NovaFailFrame*      outer_fail      = _nova_fail_top;
        NovaInterruptFrame* outer_interrupt = _nova_interrupt_top;
        /* Plan 83.4.2 Ф.2 (2026-05-23): per-fiber handler-snapshot
         * save/restore на worker (A3+B2 fix). Раньше worker НЕ менял
         * TLS handler-state перед mco_resume — fiber видел handler'ы
         * предыдущего fiber'а / worker'а. Аналог tokio TaskLocal
         * restore on poll / Node AsyncLocalStorage context-switch. */
        NovaEffectSnapshot outer_effects;
        nova_effect_snapshot_save(&outer_effects);
        if (base && base->_nova_worker_slot >= 0 && base->_nova_fiber_scope) {
            /* Preamble already ran: restore home scope + saved TLS. */
            _nova_active_scope  = base->_nova_fiber_scope;
            _nova_active_slot   = base->_nova_worker_slot;
            _nova_fail_top      = base->_nova_saved_fail_top;
            _nova_interrupt_top = base->_nova_saved_interrupt_top;
            /* Plan 83.4.2 Ф.2: restore fiber's handler-snapshot из home
             * scope (parallel array). Codegen эмитит snapshot init на
             * spawn (nova_alloc'нутый NovaEffectSnapshot). */
            NovaFiberQueue* fscope = base->_nova_fiber_scope;
            int fslot = base->_nova_worker_slot;
            if (fslot < fscope->count && fscope->fiber_effect_snapshot[fslot]) {
                nova_effect_snapshot_restore(fscope->fiber_effect_snapshot[fslot]);
            }
        } else if (base) {
            /* Before preamble (first run): restore saved fail/interrupt but
             * leave _nova_active_scope as this worker's scope (preamble will
             * allocate the home slot + set _nova_fiber_scope on first resume). */
            _nova_fail_top      = base->_nova_saved_fail_top;
            _nova_interrupt_top = base->_nova_saved_interrupt_top;
        }

        /* Plan 44.7: preemption hand-off. Clear the preempt flag so each
         * fiber starts its slice clean, and stamp `current_fiber_start` so
         * sysmon can detect an overrun. The running fiber reads the LIVE
         * flag via `_nova_preempt_ptr` (set once in _worker_main) at every
         * codegen safepoint — no stale snapshot. */
        w->preempt_flag = 0;
        __atomic_store_n(&w->current_fiber_start, uv_hrtime(), __ATOMIC_RELAXED);

        /* Plan 83.4.5.7 (2026-05-23): atomic state guard для double-resume race.
         * CAS IDLE→RUNNING — winner runs mco_resume, loser skips. Loser case:
         * cross-worker steal race (one worker stole, owner также пытается popнуть
         * через wake_pending duplicate-push race) или concurrent wake-during-running
         * (см. NovaSpawnCtxBase doc выше). Без guard'а — TIB swap conflict
         * (Windows) / context corruption (POSIX) → access violation в fiber arena.
         *
         * Loser обязан НЕ trogать co (другой thread держит контекст). Restore
         * outer TLS, continue loop.
         *
         * NB: для FIRST RUN — fiber's state IDLE (from nova_alloc zero-init),
         * CAS трivially succeeds. Race materialization только для re-pop'ов
         * (wake + wake = double-push, или steal race на edge-case). */
        bool _nova_state_owned = true;  /* default — non-MCO_SUSPENDED branch */
        if (mco_status(co) == MCO_SUSPENDED) {
            _nova_state_owned = (bool)nova_fiber_state_cas(
                co, NOVA_FIBER_STATE_IDLE, NOVA_FIBER_STATE_RUNNING);
            if (_nova_state_owned) {
                mco_resume(co);
            }
            /* else: другой thread держит RUNNING. Skip mco_resume — но всё
             * равно нужно restore outer TLS ниже + дать другому owner'у
             * dispose'нуть fiber. Don't touch co. */
        }

        /* Fiber returned to the loop — clear the overrun timestamp so an
         * idle worker is never marked for preemption. */
        __atomic_store_n(&w->current_fiber_start, 0, __ATOMIC_RELAXED);

        /* Save fiber's current TLS state back; restore outer worker state. */
        if (base) {
            base->_nova_saved_fail_top      = _nova_fail_top;
            base->_nova_saved_interrupt_top = _nova_interrupt_top;
            /* Plan 83.4.2 Ф.2: save fiber's current handler-state (с учётом
             * with-блоков push/pop сделанных fiber'ом во время выполнения)
             * обратно в home scope's snapshot. */
            if (base->_nova_fiber_scope && base->_nova_worker_slot >= 0) {
                NovaFiberQueue* fscope = base->_nova_fiber_scope;
                int fslot = base->_nova_worker_slot;
                if (fslot < fscope->count && fscope->fiber_effect_snapshot[fslot]) {
                    nova_effect_snapshot_save(fscope->fiber_effect_snapshot[fslot]);
                }
            }
        }
        _nova_active_scope  = outer_scope;
        _nova_fail_top      = outer_fail;
        _nova_interrupt_top = outer_interrupt;
        /* Plan 83.4.2 Ф.2: restore outer worker's effect state (для следующего
         * fiber'а или idle worker loop'а). */
        nova_effect_snapshot_restore(&outer_effects);

        /* Plan 44.5 Layer 5 deferred-unlock: check parked state BEFORE releasing
         * the channel/sleep mutex. This captures parked[slot]=true while no
         * cross-thread waker can clear it (they are blocked on the mutex).
         * Only after this check do we release the mutex via the deferred fn.
         *
         * Use _nova_fiber_scope (home scope) for is_parked check — must match
         * the scope used by the fiber in nova_sched_park_with_unlock, which
         * captures _nova_active_scope (restored to _nova_fiber_scope above). */
        bool fiber_is_parked = false;
        if (_nova_state_owned && mco_status(co) == MCO_SUSPENDED) {
            NovaFiberQueue* check_scope = (base && base->_nova_fiber_scope)
                                          ? base->_nova_fiber_scope : &w->scope;
            int act_slot = base ? base->_nova_worker_slot : _nova_active_slot;
            if (act_slot >= 0) {
                fiber_is_parked = (bool)nova_sched_is_parked(check_scope, act_slot);
            }
        }
        if (_nova_park_unlock_fn) {
            void (*fn)(void*) = _nova_park_unlock_fn;
            void* arg = _nova_park_unlock_arg;
            _nova_park_unlock_fn  = NULL;
            _nova_park_unlock_arg = NULL;
            fn(arg);
        }

        /* Plan 83.4.5.7 (2026-05-23): state transitions для current owner.
         * Если мы НЕ owned (CAS lost) — другой thread сейчас держит RUNNING,
         * он сам сделает dispose. Просто continue.
         *
         * EXCEPTION: co already DEAD upon worker pop (rare — re-popped after
         * mco_destroy by some path). Still need to skip — base может быть
         * освобождён GC'ем уже. */
        if (!_nova_state_owned) {
            if (mco_status(co) == MCO_DEAD) {
                /* Co was DEAD before we even tried CAS. mco_resume не вызван,
                 * другой thread уже destroyed. Skip. */
            }
            continue;
        }

        if (mco_status(co) == MCO_DEAD) {
            /* Plan 83.4.5.8 (2026-05-24): grab ctx pointer ДО mco_destroy
             * (destroy frees co, не ctx — separate allocations). All ctx
             * allocated через nova_alloc_uncollectable под armed M:N
             * (codegen emit_spawn / emit_detach choice based on
             * nova_runtime_is_initialized()). Free здесь — гарантирует
             * lifecycle ends точно когда fiber finishes. */
            NovaSpawnCtxBase* dead_ctx = (NovaSpawnCtxBase*)mco_get_user_data(co);
            /* Also grab init_snapshot pointer (may be NULL — already consumed
             * by preamble OR cooperative path). Snapshot moved в
             * scope->fiber_effect_snapshot[slot] которое GC-managed (под
             * armed scope's fiber_effect_snapshot array — nova_alloc'd),
             * но snapshot itself был nova_alloc_uncollectable. После
             * fiber DEAD никто не держит ссылку, можно free.
             *
             * Snapshot adoption: codegen preamble делает
             *   scope->fiber_effect_snapshot[slot] = _c->_nova_init_snapshot;
             *   _c->_nova_init_snapshot = NULL;
             * Так что _c->_nova_init_snapshot читается NULL здесь
             * (already transferred). Snapshot живёт в scope's array
             * пока scope alive → eventually freed когда scope's
             * uncollectable count reaches zero. К сожалению snapshot
             * никто не free'ит explicitly — it would leak under armed.
             * V1 tradeoff: snapshots leak per fiber. V2 followup —
             * хранить параллельный массив "uncollectable" в scope. */
            nova_fiber_state_store(co, NOVA_FIBER_STATE_DEAD);
            mco_destroy(co);
            if (dead_ctx) {
                /* Plan 83.6: route через pool release. base->_nova_pool_size
                 * decides: pool route (size > 0, push back) либо direct
                 * Boehm free (size == 0). */
                nova_spawn_pool_release(dead_ctx, dead_ctx->_nova_pool_size);
            }
        } else if (mco_status(co) == MCO_SUSPENDED) {
            /* Yielded: if parked (timer/channel wait) → dispatch_ready re-queues.
             * If not parked (cooperative yield via preemption or runtime.yield)
             * → yielded-FIFO, NOT the deque. Re-pushing to the LIFO deque would
             * make the worker immediately re-pop the same fiber, starving every
             * peer below it (Plan 44.7). */
            if (fiber_is_parked) {
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
        if (!co) co = (mco_coro*)nova_deque_pop(&w->deque);
        if (!co) co = _worker_yielded_pop(w);
        if (!co) break;
        if (mco_status(co) == MCO_SUSPENDED) {
            if (nova_fiber_state_cas(co, NOVA_FIBER_STATE_IDLE,
                                          NOVA_FIBER_STATE_RUNNING)) {
                mco_resume(co);
                if (mco_status(co) == MCO_DEAD) {
                    nova_fiber_state_store(co, NOVA_FIBER_STATE_DEAD);
                } else {
                    nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
                }
            }
            /* else: другой owner. Skip. */
        }
        if (mco_status(co) == MCO_DEAD) {
            NovaSpawnCtxBase* dead_ctx = (NovaSpawnCtxBase*)mco_get_user_data(co);
            mco_destroy(co);
            if (dead_ctx) {
                /* Plan 83.6: pool release. */
                nova_spawn_pool_release(dead_ctx, dead_ctx->_nova_pool_size);
            }
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
    }

    _workers = (NovaWorker*)calloc((size_t)n_workers, sizeof(NovaWorker));
    if (!_workers) {
        fprintf(stderr, "nova: runtime_init OOM (%d workers)\n", n_workers);
        abort();
    }
    _n_workers = n_workers;
    nova_aint_init(&_round_robin, 0);

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

    for (int i = 0; i < n_workers; i++) {
        NovaWorker* w = &_workers[i];
        w->id = i;
        nova_abool_init(&w->stop, false);
        nova_aint_init(&w->pending_count, 0);
        /* Plan 44.7: preemption state — calloc'нуто в 0, инициализируем явно. */
        w->preempt_flag = 0;
        w->current_fiber_start = 0;
        nova_scope_init(&w->scope);
        /* Plan 44.5 Layer 2: per-worker Chase-Lev deque. */
        if (!nova_deque_init(&w->deque, 64)) {
            fprintf(stderr, "nova: deque_init failed\n");
            abort();
        }
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

        rc = uv_thread_create(&w->thread, _worker_main, w);
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

    /* Plan 44.7: stop sysmon ПЕРВЫМ — до free(_workers), чтобы sysmon
     * не читал освобождённую память. join гарантирует тред вышел. */
    if (_sysmon_started) {
        nova_abool_store(&_sysmon_running, false);
        uv_thread_join(&_sysmon_thread);
        _sysmon_started = false;
    }

    /* Signal stop + wake workers. */
    for (int i = 0; i < _n_workers; i++) {
        NovaWorker* w = &_workers[i];
        nova_abool_store(&w->stop, true);
        uv_async_send(&w->wake_handle);
    }

    /* Join. */
    for (int i = 0; i < _n_workers; i++) {
        uv_thread_join(&_workers[i].thread);
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
        uv_close((uv_handle_t*)&w->wake_handle, NULL);
        /* Run one more tick to process close. */
        uv_run(&w->loop, UV_RUN_NOWAIT);
        uv_loop_close(&w->loop);
        nova_deque_destroy(&w->deque);
        free(w->wake_pending);
        w->wake_pending = NULL;
        free(w->yielded);          /* Plan 44.7 yielded-FIFO */
        w->yielded = NULL;
        /* Plan 83.6: drain SpawnCtx pool — free retained ctx buffers. */
        _nova_spawn_pool_drain(w);
    }

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
    /* Plan 83.4.5.7 (2026-05-23): SEQ_CST fence перед cross-thread deque push.
     * spawn_global вызывается main thread'ом, но deque принадлежит worker'у.
     * Chase-Lev push designed для single-owner: main → target's deque
     * нарушает контракт. Fence гарантирует видимость main's writes на ctx
     * fields (`_nova_parent_scope` etc.) до того, как worker's deque pop
     * (single-owner pop, RELAXED loads) их прочитает. */
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
    if (!nova_deque_push(&target->deque, co)) {
        fprintf(stderr, "nova: runtime_spawn_global: deque_push failed\n");
        mco_destroy(co);
        abort();
    }
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
    /* Increment ДО push'а — main thread в drain-loop должен видеть
     * pending_remote > 0 даже если worker сразу подхватит fiber и завершит
     * его до того как main опросит counter. */
    nova_aint_inc(&((NovaFiberQueue*)scope)->pending_remote);
    /* Реальный push идёт через spawn_global. */
    nova_runtime_spawn_global(entry, user);
}

/* Plan 44.5 Layer 5: signal main thread из worker context'а.
 * No-op до runtime.init либо после shutdown — main thread в этих режимах
 * либо вообще нет (test'у без init), либо exit'ит (shutdown). */
void nova_runtime_signal_main(void) {
    if (_main_wake_inited) {
        uv_async_send(&_main_wake);
    }
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
        /* Plan 22 Ф.7 nova_scope_init: heap-init lazy arrays. */
        nova_scope_init(&_nova_orphan_scope);
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
