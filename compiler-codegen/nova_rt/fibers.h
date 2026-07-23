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

/* Plan 173.0 Ф.2: explicit (not relying on transitive minicoro.h include) —
 * used by the R2 tripwire in nova_scope_grow_children. No-op under NDEBUG
 * (release builds), matching the rest of the codebase's debug-tripwire
 * convention (canary/poison checks are debug-only). */
#include <assert.h>

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
#include "driver.h"  /* Plan 83.11 Ф.3: NovaDriverJob, nova_driver_submit_job */

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
 * Plan 82 Ф.1 — Windows присоединён к arena-пути.
 *
 * Wire minicoro's alloc_cb/dealloc_cb to nova_fiber_alloc/dealloc, которые
 * берут стек из per-thread арены вместо calloc. POSIX — fiber_arena.c
 * (mmap MAP_NORESERVE); Windows — fiber_arena_win.c (VirtualAlloc
 * lazy-commit). Раньше Windows шёл на minicoro default calloc (fixed
 * 56 KB, без guard, без GC-видимости fiber-стеков).
 *
 * Stack size: slot_usable (= slot_size − guard) минус минимальный
 * mco_desc header overhead. Реальный header < 1KB на amd64; 8KB
 * закладывается с запасом. */
#define _NOVA_MCO_HEADER_OVERHEAD 8192
#if (defined(__linux__) || defined(__APPLE__) || defined(_WIN32))
  #include "fiber_arena.h"
  #if NOVA_FIBER_ARENA_ENABLED
    static inline mco_desc _nova_mco_desc_init_arena(void (*entry)(mco_coro*)) {
        /* Plan 149 (review must_fix #1/#2): derive minicoro stack_size from
         * the RUNTIME arena slot_size (env ∨ -D ∨ builtin, post round+clamp),
         * NOT the compile-time NOVA_FIBER_STACK_SIZE macro. nova_fiber_arena_slot_size
         * lazily inits the arena (idempotent) and returns the resolved size, so
         * minicoro's coro_size (== the `size` later requested from
         * nova_fiber_alloc) scales with the slot. This makes NOVA_FIBER_STACK
         * env actually change the usable stack (AC2) and keeps the 256KB floor
         * usable (coro_size ≤ slot_usable ⇒ nova_fiber_alloc's `size > usable`
         * guard passes). */
        size_t slot_usable = nova_fiber_arena_slot_size() - NOVA_FIBER_GUARD_SIZE;
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

/* Plan 82 Ф.1: post-create hook. Вызывается после КАЖДОГО mco_create.
 * На Windows патчит ctx.stack_limit корутины на committed-low слота
 * arena — обязательно для lazy-commit (иначе __chkstk-код с кадром
 * >1 страницы крашит на MSVC; Ф.0 test a, decision-point). No-op на
 * POSIX и при отключённой arena. Определена в fibers.c — нужен доступ
 * к minicoro-внутреннему типу _mco_context (виден только в TU с
 * MINICORO_IMPL). */
void nova_fiber_post_create(mco_coro* co);

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
    nova_fiber_post_create(co);  /* Plan 82 Ф.1: patch ctx.stack_limit (Windows) */
    _nova_gc_add_fiber_roots(co);
    /* Plan 83.4.5.7: no CAS guard здесь — nova_fiber_run is one-shot,
     * single thread, no concurrent resume race. State machine helpers
     * defined ниже после NovaSpawnCtxBase (forward-order). */
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

/* ─── Plan 173.0 Ф.2: per-child error retention (runtime substrate) ───
 *
 * Replaces the one-slot `first_error_atomic` CAS (which collapses N
 * simultaneous remote-child failures into a single winner — decisions 1/2
 * of docs/plans/173.0-concurrency-runtime-substrate.md) with a genuine
 * per-child slot: each M:N remote child is assigned its own index
 * (`_nova_parent_slot`, NovaSpawnCtxBase) at spawn time; on throw, that
 * child writes ONLY its own slot — no CAS, no collapsing, N failures stay
 * N distinct retained records.
 *
 * Deliberately a SEPARATE index space from the local-scheduling arrays
 * (`fibers[]`/`fiber_error[]`/`count`, Ф.1 — frozen, not touched here):
 * those stay dedicated to the bootstrap/single-thread path. Under the
 * shipped auto-arm design (codegen's `emit_main_function` calls
 * `_auto_arm_if_needed()` as main()'s first statement) the M:N remote path
 * is live before any user `spawn` executes, so a single scope object never
 * sees BOTH local- and remote-scheduled children in practice — the two
 * index spaces never collide within one NovaFiberQueue instance.
 *
 * R2 tripwire (§EXEC risk R2): capacity is frozen once the drain loop
 * starts (`_drain_started`) — every remote child is spawned into the scope
 * on the calling thread BEFORE `nova_supervised_run_impl`'s loop begins
 * (codegen emits all `spawn` statements before the scope-exit drain call,
 * and a spawned child's body has no reference to the parent's stack-local
 * NovaFiberQueue, so it cannot itself spawn further children into it) —
 * so grow-during-drain is structurally unreachable; the assert in
 * `nova_scope_grow_children` proves it stays that way. */
typedef struct {
    const char*   msg;      /* NULL = empty slot (no error recorded yet) */
    NovaThrowKind kind;      /* USER / CANCEL / USER_TYPED */
    void*         reason;    /* boxed T for CANCEL (GC-managed) */
    void*         payload;   /* boxed T for USER_TYPED (GC-managed) */
    NovaTypeId    tid;       /* payload type id for USER_TYPED */
    /* Plan 173.2 (supervision-as-effect): mid-drain publication flag.
     * The writer (the failing child, worker thread) fills the fields above
     * and THEN release-stores `published=true`; the reader (the scope's
     * drive thread, nova_supervised_process_decisions) acquire-loads it
     * before touching the plain fields — a per-slot happens-before that
     * makes DURING-drain reads sound (the 173.0 end-of-drain read relied on
     * the pending_remote==0 acquire gate instead; that gate still holds for
     * the final catch-up pass). Slots have disjoint owners (one child each,
     * nova_scope_alloc_child_slot) so there is no writer-writer race. */
    nova_atomic_bool published;
    /* Plan 173.2: drive-thread-only bookkeeping — this failure has been fed
     * to the Supervisor decision exactly once. Never touched by workers. */
    nova_bool     decided;
    /* Plan 173 хвост (D414 §1, MultiError-агрегация, 2026-07-13):
     * drive-thread-only — решение по этому падению было ESCALATE (ошибка
     * участвует в primary-выборе и, если не выиграла, уходит в suppressed-
     * карман при scope re-throw). Stop-решённые остаются retained, но в
     * suppressed НЕ попадают (хендлер осознанно их выкинул — D416).
     * Для default-scope (нет супервизора) флаг не используется: там ВСЕ
     * retained не-CANCEL падения escalate-класса по определению. */
    nova_bool     escalated;
} NovaChildError;

/* ─── Plan 175 (owner TODO closure, 2026-07-10): virtual-clock auto-idle-
 * advance for mock `Time` handlers (`std/testing/handlers.nv` mut_clock) ───
 *
 * Problem: under `with Time = mut_clock(...)`, `sleep(ms)` used to just
 * synchronously add `ms` to the mock's `current_ms` and return — no fiber
 * suspension at all. Fine for a single sequential flow, but breaks deadline
 * ORDERING across concurrently `spawn`ed fibers: since nothing ever yields
 * inside a mock sleep, whichever fiber the scheduler happens to resume
 * first runs its ENTIRE body (including the "sleep") to completion before
 * any sibling gets a turn — side effects land in spawn order, not virtual-
 * deadline order (tokio/Kotlin `TestCoroutineScheduler` parity requires
 * deadline order: a fiber sleeping 10ms should observably go before one
 * sleeping 100ms, even though neither actually waits in real time).
 *
 * Fix: `mut_clock`'s `sleep` op now computes the absolute deadline itself
 * (`current_ms + delta`) and calls the new `vclock.park_until(deadline)`
 * runtime hook BEFORE bumping `current_ms`. `park_until` parks the calling
 * fiber (if any — see `nova_vclock_park_until` below) in this per-scope
 * registry; once EVERY alive fiber of the scope is registered here (idle —
 * nobody has real work left to do), the entry with the smallest deadline
 * is fired (woken); the woken fiber resumes, bumps its OWN `current_ms` to
 * its deadline (monotonic — Nova-side, `std/testing/handlers.nv`) and
 * returns.
 *
 * Deliberately NOT reusing `nova_sched_park`/`nova_sched_wake` (the real-
 * timer/driver-facing primitive, with its own atomics and hard-won race
 * fixes documented all over this file) — virtual-clock coordination is
 * single-threaded BY CONTRACT (docs/time.md "M:N-контракт": mut_clock
 * already needs `NOVA_MAXPROCS=1`, which is `nova test`'s default —
 * runq.h:67 "ALL of them land on a single [worker thread]"). A separate,
 * deliberately-simple, non-atomic registry avoids any risk to the real
 * scheduler's carefully-tuned park/wake invariants. Plain `nova_fiber_
 * yield()` (not `nova_sched_park`) is used to cooperatively hand control
 * back — `nova_supervised_step` already re-resumes any slot that isn't
 * `nova_sched_park`ed on every pass, so a virtual-sleeping fiber is simply
 * resumed, spins its loop once (checking whether it's been fired yet),
 * and yields again — no busy CPU loop across passes, bounded spins.
 *
 * Real-clock path (no `with Time = mut_clock(...)` in scope) is entirely
 * untouched — mut_clock's `sleep` op is the ONLY caller of `park_until`. */
typedef struct {
    int64_t   deadline_ms;  /* absolute virtual-clock deadline (mut_clock's
                             * own `current_ms` domain — see std/testing/
                             * handlers.nv, computed by the CALLER). */
    mco_coro* co;           /* informational only (debugging); not used for
                             * dispatch — `park_until` re-checks its OWN
                             * `idx`, not `co`. */
    nova_bool fired;        /* set by whichever fiber calls fire_earliest()
                             * and finds this the (a) minimum deadline. */
    nova_bool used;         /* false = free/consumed slot. */
} NovaVClockEntry;

/* gcc 14+ (incl. 15.2, -Wincompatible-pointer-types promoted to error-by-default
 * for C) treats a tagged forward declaration (`struct NovaFiberQueue;`, as used
 * by driver.h) and an anonymous-struct typedef (`typedef struct {...} X;`) as
 * two DIFFERENT C types, even though the typedef name matches — clang accepts
 * this pattern silently. Tagging the struct here unifies both spellings into
 * one type; purely a source-level fix, no ABI/layout change. */
typedef struct NovaFiberQueue {
    /* Plan 22 Ф.7: dynamic arrays через managed heap.
     * NULL до первого spawn_into. capacity показывает alloc'нутую
     * длину массивов (все 7 синхронизированы — растут вместе). */
    mco_coro**      fibers;              /* dynamic [count] */
    void**          fiber_ctx;           /* dynamic [count] — GC root для SpawnCtx */
    NovaFailFrame** fiber_fail_top;      /* dynamic [count] */
    NovaInterruptFrame** fiber_interrupt_top; /* dynamic [count] */
    NovaEffectSnapshot** fiber_effect_snapshot; /* dynamic [count] */
    /* Plan 201 trace-per-fiber (2026-07-13): per-fiber owned bucket for
     * `_nova_last_error`/`_nova_throw_site`/`_nova_throw_trace` (effects.h
     * NovaFiberErrorState). Allocated once when the slot is created
     * (nova_scope_alloc_slot / nova_fiber_spawn_into) — mirrors
     * fiber_effect_snapshot's lifecycle exactly, but swapped by POINTER
     * (not copied) around mco_resume: see effects.h NovaFiberErrorState
     * doc-comment for why a copy is unnecessary here. */
    NovaFiberErrorState** fiber_error_state;    /* dynamic [count] */
    const char**    fiber_error;         /* dynamic [count] */
    nova_bool**     fiber_did_throw;     /* dynamic [count] */
    int             capacity;            /* alloc'нутая длина массивов */
    int             count;
    /* Scope error: first error captured from any fiber. Reset on init.
     * Plan 49 Ф.2: kind + reason добавлены — supervised_run (Ф.3) различает
     * USER (re-throw) от CANCEL (silent return). USER-precedence: реальная
     * ошибка может overwrite предыдущую CANCEL (см. nova_fiber_report_error). */
    const char*     first_error;
    NovaThrowKind   first_error_kind;     /* USER (default) или CANCEL */
    void*           first_error_reason;   /* box'нутый T для CANCEL, NULL для USER */
    /* Cancellation: set to true after the first fiber throws.
     * Other fibers see this on their next yield-point and throw "cancelled"
     * (cooperative cancellation — D50).
     * Plan 49 Ф.2: cancel_reason_ptr — причина из bound token'а, копируется
     * при cancel(). Используется nova_fiber_yield для throw'а CANCEL+reason.
     *
     * Plan 83.4.3/B5 (2026-05-23): nova_atomic_bool — под M:N main thread
     * (token.cancel()) пишет, worker fiber'ы читают на каждом yield. На x86
     * byte-load атомарен; на ARM нужны acquire/release fences для visibility.
     * ACQUIRE-load в nova_abool_load + RELEASE-store в nova_abool_store
     * гарантирует happens-before между cancel() и yield-check на любой
     * memory-модели. Аналог tokio CancellationToken atomic-flag. */
    nova_atomic_bool cancel_requested;
    void*           cancel_reason_ptr;    /* box'нутый T (TLV-owned), NULL если без причины */
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
    /* [196.6 / D228 §6 class, 2026-07-13] pending_sweeps — count of remote
     * children whose fiber body (epilogue) has finished but whose WORKER-side
     * post-mortem sweep (_worker_main: mco_destroy → nova_scope_retain_or_
     * release_child → nova_spawn_pool_release) has not completed yet.
     *
     * Race this closes (VEH-localized, docs/plans/196.6-race-state-dump-notes.md):
     * the child epilogue decrements `pending_remote` INSIDE the fiber; the
     * worker's sweep runs strictly AFTER the fiber returns, and
     * nova_scope_retain_or_release_child dereferences
     * `dead_ctx->_nova_parent_scope` — but by then the scope owner may have
     * observed pending_remote==0, returned from nova_supervised_run_impl, and
     * the STACK-allocated NovaFiberQueue is gone: the sweep reads (and on the
     * error-retention path WRITES child_ctx[slot] into) reused stack memory →
     * the Plan 198 floating corruption / 0xC0000005 at
     * `parent->child_capacity` (offset 0x74 off a NULL/garbage reload).
     * Same class as §12.31 `pending_driver_jobs` (stack scope must outlive
     * all async references — D228 §6); same counter-based-wait fix:
     *   - increment: child epilogue (codegen), program-order BEFORE its
     *     pending_remote release-decrement — the acquire that sees
     *     pending_remote==0 therefore also sees pending_sweeps>0 until the
     *     sweep finishes (relaxed inc suffices).
     *   - decrement: worker sweep, fetch_sub RELEASE, AFTER retain/release —
     *     via a parent pointer SNAPSHOT taken before the ctx can be pooled
     *     (pool push overlays _nova_parent_scope with the freelist next-ptr).
     *   - wait: supervised_run_impl tail (next to the pending_driver_jobs
     *     wait) + drain_main_scope, acquire loads.
     * Single-thread baseline: stays 0 forever, behaviour identical. */
    nova_atomic_int pending_sweeps;
    /* Plan 44.5 Layer 5: atomic first_error для cross-worker error
     * propagation. Worker fiber на throw делает CAS (NULL → err_msg);
     * первый wins. После CAS — sets cancel_requested = true для
     * cooperative cancel других fiber'ов в scope.
     *
     * NULL = no error. Read через nova_aptr_load(acquire) в main thread
     * после `pending_remote == 0` — корректный happens-before.
     *
     * Plan 49 Ф.5: kind + reason пишутся ПОСЛЕ успешного CAS на msg
     * (обычный store, не atomic — happens-before гарантирован release/acquire
     * на msg pointer). Reader (main supervised_run) читает kind/reason
     * увидев non-NULL msg. USER-precedence: см. nova_fiber_report_atomic_kinded
     * — compare-kind CAS-loop overwrite CANCEL→USER. */
    nova_atomic_ptr first_error_atomic;
    NovaThrowKind   first_error_atomic_kind;     /* USER (default) или CANCEL */
    void*           first_error_atomic_reason;   /* box'нутый T для CANCEL */
    /* Plan 83.10 (2026-05-25): fix [M-83.10-armed-user-throw-routing].
     * Typed throw (NOVA_THROW_USER_TYPED) needs payload + tid для proper
     * handler dispatch на main re-throw. Без этих полей `throw 42` под
     * armed M:N теряет int payload — main's nova_throw(str) bypasses
     * typed handler dispatch chain.
     *
     * Worker fiber catch (emit_spawn): writes payload + tid alongside
     * msg + kind. Main supervised_run_impl reads when re-throwing —
     * if kind == USER_TYPED → call nova_throw_typed(msg, payload, tid)
     * instead of nova_throw(str), preserving typed handler dispatch. */
    void*           first_error_atomic_payload;  /* Plan 83.10: typed payload */
    NovaTypeId      first_error_atomic_tid;      /* Plan 83.10: payload type ID */
    /* Plan 44.5 Layer 5 park/wake: M:N fiber re-dispatch hook.
     * Set by runtime.c on worker scopes (в nova_runtime_init).
     * NULL = single-thread scope (main thread, test scopes) — no M:N.
     *
     * Protocol: nova_sched_wake calls this after clearing parked[slot].
     *   same-thread (timer close_cb): owner deque push — wait-free.
     *   cross-thread (channel send from another worker): mutex-protected
     *   pending list + uv_async_send → worker drains on next iteration.
     *
     * ctx: opaque NovaWorker* set alongside this pointer. */
    void (*dispatch_ready)(void* ctx, mco_coro* co);
    void*  dispatch_ctx;
    /* Plan 44.5 L5: GC pin для remote SpawnCtx (M:N spawn path).
     * До worker resume SpawnCtx unrooted (deque malloc + coro calloc).
     * Pin в parent supervised scope's ctx_pins (на main stack →
     * reachable via thread root scan). Lazy-alloc + capacity-doubling. */
    void**  ctx_pins;
    int     ctx_pins_count;
    int     ctx_pins_cap;
    /* Plan 65 Ф.10: reverse-pointer back to the CancelToken currently bound
     * to this scope. Set in nova_cancel_token_bind, cleared in unbind.
     * Used by runtime to discover the cancel-token from inside arbitrary
     * blocking-resource constructors (e.g. ChanReader.close_after timers)
     * without threading the token through every call site.
     *
     * NULL = scope has no bound cancel-token (top-level main, or
     * supervised { ... } without `cancel:` arg). Resource constructors
     * gracefully skip cancel-registration in that case.
     *
     * Forward-declared as void* — actual type is `NovaCancelToken*` (declared
     * after this struct). Set/cleared via nova_cancel_token_bind/unbind. */
    void*   bound_token;

    /* Plan 83.11 Ф.3: linked-list head of armed sleeps in this scope.
     * Driver-thread only mutator (insert при ARM_SLEEP, unlink при close_cb).
     * Cancel walks list (driver-side, single thread). NULL = no armed sleeps. */
    struct NovaSleepState* armed_sleeps_head;

    /* Plan 83.11 Ф.3.B: spinlock protecting nova_scope_alloc_slot /
     * nova_scope_free_slot from concurrent access.
     *
     * nova_scope_alloc_slot is called from EACH fiber's preamble, which runs
     * on a worker thread. Under M:N (16 workers), 16 fibers can call alloc_slot
     * simultaneously on the same scope. The original scan+grow+assign was
     * completely non-atomic: two workers could both see fibers[i]==NULL, both
     * claim slot i, and one overwrites the other's entry. The overwritten fiber
     * gets a WRONG-FIBER close_cb → wake skipped → hangs forever.
     *
     * 0 = unlocked, 1 = locked. CAS 0→1 to acquire, store 0 to release. */
    nova_atomic_int slot_lock;

    /* Plan 83.11 §12.31: outstanding CANCEL_SCOPE jobs that reference this
     * scope, held by the driver thread. NovaFiberQueue is stack-allocated by
     * codegen (one per `supervised { ... }` block). If main returns from
     * nova_supervised_run_impl while the driver still holds a CANCEL_SCOPE
     * job that points here, the scope memory is reused (next stack frame)
     * and the driver's deref reads garbage → SEGV (see §12.30 cdb / §12.31
     * VEH localization: crash in `_nova_driver_handle_cancel_scope` at the
     * CAS `&st->stage` where `st = scope->armed_sleeps_head` is now wild).
     *
     * Lifetime contract: incremented (ACQ_REL) before nova_driver_submit_job
     * in `_nova_cancel_via_driver`; decremented (RELEASE) at the end of
     * `_nova_driver_handle_cancel_scope`. nova_supervised_run_impl spins
     * on this counter == 0 before returning, so the stack frame stays alive
     * until the driver has finished dereferencing scope fields. */
    nova_atomic_int pending_driver_jobs;

    /* Plan 174 (D349): scope deadline — absolute monotonic-clock nanoseconds
     * (uv_hrtime() epoch). 0 = no deadline. Set by codegen for
     * `supervised(deadline:/timeout:)`; inherited (min-combined) from the
     * enclosing scope by nova_scope_init so a deadline propagates into nested
     * scopes and an inner scope can only TIGHTEN, never extend, the outer
     * deadline. nova_supervised_run_impl arms a bounded uv_run wait against
     * it, delivers scope-cancel on expiry, and throws a typed `TimeoutError`.
     * Not atomic: single-owner (the scope's own driving thread reads it;
     * the deadline callback runs inline on that same loop). */
    int64_t deadline_ns;

    /* Plan 174 (D349): the enclosing `_nova_active_scope` captured at
     * nova_scope_init (before this scope makes itself active). Restored by
     * nova_supervised_run_impl on EVERY exit path — crucially the re-throw /
     * interrupt / timeout longjmp paths, where codegen's own
     * `_nova_active_scope = prev` line is skipped. Without this a caught throw
     * (e.g. TimeoutError) would leave _nova_active_scope dangling at this
     * scope's freed stack frame, and the next scope_init would inherit a
     * garbage deadline_ns from it. NULL for the top-level scope. */
    struct NovaFiberQueue* saved_active_scope;

    /* ─── Plan 173.0 Ф.2/Ф.3: per-child retention (see NovaChildError above) ───
     * Separate index space from fibers[]/fiber_error[]/count (Ф.1, frozen).
     * Populated ONLY by the M:N remote-spawn path (nova_runtime_spawn_into /
     * nova_scope_alloc_child_slot); bootstrap/local spawn is unaffected. */
    NovaChildError* child_error;     /* dynamic [child_capacity], NULL until 1st remote spawn */
    void**          child_ctx;       /* dynamic [child_capacity] — retained SpawnCtx (Ф.3 R1-guard);
                                       * NULL entry = child still alive or completed without error
                                       * (ctx already recycled to the pool normally) */
    int             child_count;     /* next free index == number of remote children ever spawned */
    int             child_capacity;  /* allocated length of child_error[]/child_ctx[] */
    /* R2 tripwire (§EXEC risk R2): set true at the top of
     * nova_supervised_run_impl's drain loop; nova_scope_grow_children asserts
     * this is still false — proves grow-during-drain never happens (see
     * comment above NovaChildError). Debug-only cost (no-op in NDEBUG). */
    nova_bool       _drain_started;
    /* ─── Plan 173.2: supervision-as-effect ───
     * Stamped by codegen at scope entry (right after nova_scope_init, on the
     * scope's own thread, strictly BEFORE any spawn):
     *   q.has_supervisor = (_nova_handler_Supervisor != NULL);
     * true  → deferred-decision mode: a failing child's report goes ONLY to
     *         its per-slot child_error[] entry (no first_error CAS, no
     *         cancel_requested broadcast); the drive thread consults the
     *         ambient `Supervisor.on_child_fail` handler serially, one
     *         retained failure at a time (nova_supervised_process_decisions),
     *         and EXECUTES the returned Decision (Escalate/Stop).
     * false → default path, byte-parity with pre-173.2 behaviour (the
     *         supervision branches are never entered).
     * Cross-thread visibility: written before the first spawn_into on the
     * same thread; children observe it through the same publication chain
     * that already carries every other scope field (deque push/steal). */
    nova_bool       has_supervisor;
    /* Plan 173.2: re-entrancy latch for nova_supervised_process_decisions —
     * drive-thread only. Guards against a handler body that (indirectly)
     * re-enters the drive machinery. */
    nova_bool       _deciding;
    /* Plan 175 (owner TODO closure, 2026-07-10): auto-idle-advance registry
     * for `std/testing/handlers.nv` `mut_clock` virtual sleeps — see the
     * big comment block above `NovaVClockEntry` (near `nova_scope_init`)
     * for the full design. Lazy-alloc'd (NULL until the first virtual
     * sleep in this scope); deliberately NOT atomic/thread-safe — virtual-
     * clock coordination is single-threaded BY CONTRACT (docs/time.md
     * "M:N-контракт": mut_clock already requires NOVA_MAXPROCS=1, which is
     * `nova test`'s default, runq.h:67). A scope that never uses a mock
     * `Time` handler never touches these fields (zero overhead). */
    NovaVClockEntry* vclock_entries;
    int              vclock_count;
    int              vclock_cap;
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

/* ─── Plan 83-go-cmn Ф.1b: chunked, STABLE-ADDRESS park/wake storage ──
 *
 * Closes [M-83.11-grow-vs-wake-race]. The old NovaSchedState held 4 raw
 * pointers (parked/pending_handle/pending_stop_cb/pending_wake) that
 * nova_sched_grow_state REALLOCATED with plain non-atomic pointer-swaps.
 * A peer/driver thread in nova_sched_wake reading the old base between the
 * swap and the capacity update dereferenced a torn/orphaned base → CAS into
 * a dead array → lost wake → hang.
 *
 * Fix (Option C — chunked block-list): each of the 4 arrays is now backed by
 * a DIRECTORY of fixed-size CHUNKS that are allocated EXACTLY ONCE and never
 * moved/realloc'd/freed during the scope's life. Therefore the element
 * address &parked_chunks[c][o] is constant for the whole life of slot's data
 * → the torn base pointer is structurally impossible (same guarantee as the
 * Ф.1a run-queue fixed ring). Indexing stays (scope,slot): every atomic
 * access keeps its exact __ATOMIC_* ordering — only the lvalue changes
 * (st->X[slot] → *accessor(st,slot), pure address math, no fence/lock).
 *
 * Geometry: CHUNK = 64 elements (power of two) ⇒ c = slot>>6, o = slot&63
 * (shift+mask, no division). MAX_CHUNKS = 1024 ⇒ hard ceiling 65536 slots
 * per scope (>> the ~2k observed; grow aborts with a clear message if
 * exceeded, mirroring the existing nova_alloc-fail abort). The 4 directories
 * are INLINE in NovaSchedState so the struct itself is one stable heap
 * object reached via scope->sched_state. */
#define NOVA_SCHED_CHUNK        64
#define NOVA_SCHED_CHUNK_SHIFT  6
#define NOVA_SCHED_CHUNK_MASK   63
#define NOVA_SCHED_MAX_CHUNKS   1024   /* 1024*64 = 65536 slots/scope ceiling */

typedef struct NovaSchedState {
    /* Each directory holds chunk pointers; a chunk is allocated once and its
     * pointer is __ATOMIC_RELEASE-published into the directory slot, then
     * read with __ATOMIC_ACQUIRE in the accessors. Chunks are never freed/
     * moved/realloc'd ⇒ &X_chunks[c][o] is immutable for slot's lifetime. */
    nova_bool*       parked_chunks[NOVA_SCHED_MAX_CHUNKS];         /* [c][o] */
    void**           pending_handle_chunks[NOVA_SCHED_MAX_CHUNKS]; /* [c][o] */
    NovaSchedStopCb* pending_stop_cb_chunks[NOVA_SCHED_MAX_CHUNKS];/* [c][o] */
    /* Plan 83-go-cmn Ф.2: per-slot parked co-pointer directory. REPLACES the
     * deleted pending_wake counter directory (same chunk geometry / never-realloc
     * stable-address). Set at gopark (BY co = mco_running()), cleared at goready/
     * cancel. This is the cancel-by-co carrier (review correction #1): the cancel
     * walk resolves the GENUINELY-parked fiber via parked_co[slot] — NOT via
     * scope->fibers[slot], which may be NULL'd-but-alive (alloc_slot skip-stale)
     * or reused by a different fiber. Plain pointer store/load (RELEASE/ACQUIRE);
     * the single-winner election is on _nova_park_state, not here. */
    mco_coro**       parked_co_chunks[NOVA_SCHED_MAX_CHUNKS];      /* [c][o] */
    int              capacity;            /* published slots = chunks_pub<<SHIFT */
} NovaSchedState;

/* ─── Hot-path element accessors (Ф.1b) ───────────────────────────────
 *
 * Each returns the ELEMENT ADDRESS only — pure address computation. The
 * ONLY synchronization is an ACQUIRE-load of the chunk pointer, which pairs
 * with grow's RELEASE-publish (so a reader that observed slot<capacity is
 * guaranteed to see the published chunk + its zero-inited elements). NO
 * fence and NO lock is applied to the element itself — the caller's atomic
 * op (SEQ_CST store, ACQ_REL CAS, ACQUIRE load, RELEASE store) is applied
 * to the returned lvalue BYTE-IDENTICALLY to the pre-Ф.1b code.
 *
 * Returns NULL if the chunk has not been published yet; callers keep their
 * existing slot<capacity guard, which (paired with grow's publish-before-
 * capacity RELEASE) makes a non-NULL return guaranteed when slot<capacity. */
static inline nova_bool* nova_sched_parked_at(NovaSchedState* st, int slot) {
    int c = slot >> NOVA_SCHED_CHUNK_SHIFT, o = slot & NOVA_SCHED_CHUNK_MASK;
    nova_bool* ch = __atomic_load_n(&st->parked_chunks[c], __ATOMIC_ACQUIRE);
    return ch ? &ch[o] : NULL;
}
static inline void** nova_sched_pending_handle_at(NovaSchedState* st, int slot) {
    int c = slot >> NOVA_SCHED_CHUNK_SHIFT, o = slot & NOVA_SCHED_CHUNK_MASK;
    void** ch = __atomic_load_n(&st->pending_handle_chunks[c], __ATOMIC_ACQUIRE);
    return ch ? &ch[o] : NULL;
}
static inline NovaSchedStopCb* nova_sched_pending_stop_cb_at(NovaSchedState* st, int slot) {
    int c = slot >> NOVA_SCHED_CHUNK_SHIFT, o = slot & NOVA_SCHED_CHUNK_MASK;
    NovaSchedStopCb* ch = __atomic_load_n(&st->pending_stop_cb_chunks[c], __ATOMIC_ACQUIRE);
    return ch ? &ch[o] : NULL;
}
/* Plan 83-go-cmn Ф.2: element address of the per-slot parked co-pointer. */
static inline mco_coro** nova_sched_parked_co_at(NovaSchedState* st, int slot) {
    int c = slot >> NOVA_SCHED_CHUNK_SHIFT, o = slot & NOVA_SCHED_CHUNK_MASK;
    mco_coro** ch = __atomic_load_n(&st->parked_co_chunks[c], __ATOMIC_ACQUIRE);
    return ch ? &ch[o] : NULL;
}

/* Plan 83-go-cmn Ф.1b followup [M-83.11-f1b-acquire-capacity]: ACQUIRE-read of
 * capacity for the `slot < capacity` accessor-guards. capacity is RELEASE-stored
 * LAST in nova_sched_grow_state (after every chunk publish), so an ACQUIRE-load
 * here establishes happens-before: a reader observing slot<capacity is
 * guaranteed the chunk for that slot is published → the accessor returns
 * non-NULL. On x86 TSO a plain read sufficed; on weak memory (ARM) the ACQUIRE
 * stops the accessor's chunk-ptr load being speculated ahead of the guard →
 * closes the theoretical NULL-deref window (clean crash, never torn-pointer). */
static inline int nova_sched_cap_acq(NovaSchedState* st) {
    return __atomic_load_n(&st->capacity, __ATOMIC_ACQUIRE);
}

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
/* Plan 83.4.5.1 (2026-05-23): forward decl, definition in nova_sched.h. */
static inline void nova_scope_cancel_wake_all(NovaFiberQueue* scope);
static inline int  nova_sched_count_alive(NovaFiberQueue* scope);
static inline int  nova_sched_count_parked(NovaFiberQueue* scope);
static inline int  nova_sched_count_ready(NovaFiberQueue* scope);
static inline void nova_sched_park(NovaFiberQueue* scope, int slot);
static inline void nova_sched_wake(NovaFiberQueue* scope, int slot);
static inline nova_bool nova_sched_is_parked(NovaFiberQueue* scope, int slot);
/* Plan 83.4.1: park-with-predicate forward decl — definition in nova_sched.h. */
typedef nova_bool (*NovaParkPredicate)(void* ctx);
static inline void nova_sched_park_until(NovaFiberQueue* scope, int slot,
                                          NovaParkPredicate pred, void* ctx);
static inline void nova_sched_register_pending(NovaFiberQueue* scope, int slot,
                                                void* handle,
                                                NovaSchedStopCb stop_cb);
static inline void nova_sched_unregister_pending(NovaFiberQueue* scope, int slot);

/* Plan 83.11 Ф.3: forward decls (definitions further down in this file). */
static inline void _nova_sleep_via_driver(NovaFiberQueue* scope, int slot, nova_int ms);
static inline void _nova_cancel_via_driver(NovaFiberQueue* scope);

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
    NovaFiberErrorState** new_error_state = (NovaFiberErrorState**)nova_alloc(sizeof(NovaFiberErrorState*) * cap);
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
            new_error_state[i]     = q->fiber_error_state[i];
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
        new_error_state[i]     = NULL;
        new_error[i]           = NULL;
        new_did_throw[i]       = NULL;
    }
    /* Swap. Old arrays — GC соберёт когда они станут unreachable. */
    q->fibers              = new_fibers;
    q->fiber_ctx           = new_ctx;
    q->fiber_fail_top      = new_fail_top;
    q->fiber_interrupt_top = new_interrupt_top;
    q->fiber_effect_snapshot = new_effect_snapshot;
    q->fiber_error_state   = new_error_state;
    q->fiber_error         = new_error;
    q->fiber_did_throw     = new_did_throw;
    q->capacity            = cap;
}

/* Plan 174 (D349): forward-decl of the active-scope TLS so nova_scope_init can
 * inherit the enclosing scope's deadline. Full platform-conditional definition
 * is below (~line 1660); a matching earlier extern decl is legal C. */
#ifdef _MSC_VER
__declspec(thread) extern NovaFiberQueue* _nova_active_scope;
#else
extern __thread NovaFiberQueue* _nova_active_scope;
#endif

static inline void nova_scope_init(NovaFiberQueue* q) {
    q->count = 0;
    q->capacity = 0;
    q->fibers = NULL;
    q->fiber_ctx = NULL;
    q->fiber_fail_top = NULL;
    q->fiber_interrupt_top = NULL;
    q->fiber_effect_snapshot = NULL;
    q->fiber_error_state = NULL;  /* Plan 201 trace-per-fiber */
    q->fiber_error = NULL;
    q->fiber_did_throw = NULL;
    q->first_error = NULL;
    nova_abool_init(&q->cancel_requested, false);  /* Plan 83.4.3/B5 */
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
    nova_aint_init(&q->pending_sweeps, 0);   /* [196.6 / D228 §6 class] */
    nova_aptr_init(&q->first_error_atomic, NULL);
    q->first_error_atomic_kind = NOVA_THROW_USER;
    q->first_error_atomic_reason = NULL;
    q->first_error_atomic_payload = NULL;     /* Plan 83.10 */
    q->first_error_atomic_tid = 0;            /* Plan 83.10 */
    q->dispatch_ready = NULL;
    q->dispatch_ctx   = NULL;
    q->ctx_pins        = NULL;
    q->ctx_pins_count  = 0;
    q->ctx_pins_cap    = 0;
    q->armed_sleeps_head = NULL;  /* Plan 83.11 Ф.3 */
    nova_aint_init(&q->slot_lock, 0);  /* Plan 83.11 Ф.3.B: slot alloc spinlock */
    nova_aint_init(&q->pending_driver_jobs, 0);  /* Plan 83.11 §12.31 */
    /* Plan 174 (D349): inherit the enclosing scope's deadline. At scope_init
     * time _nova_active_scope still points at the PARENT scope (the child sets
     * it to itself only after init), so a nested scope automatically picks up
     * an ambient deadline — including plain `supervised {}` blocks (no codegen
     * change). Scopes with their own `deadline:`/`timeout:` tighten this via
     * nova_deadline_combine right after init. Top-level scope: parent NULL → 0. */
    q->deadline_ns = _nova_active_scope ? _nova_active_scope->deadline_ns : 0;
    /* Plan 174 (D349): capture the enclosing active scope for longjmp-safe
     * restore in nova_supervised_run_impl (parent at init time; codegen sets
     * _nova_active_scope=&q only AFTER init). */
    q->saved_active_scope = _nova_active_scope;
    /* Plan 173.0 Ф.2/Ф.3: per-child retention — lazy-alloc'нутся в
     * nova_scope_alloc_child_slot (первый remote spawn). Idle scope
     * (никогда не spawn'ил remote-ребёнка) = нулевой overhead. */
    q->child_error    = NULL;
    q->child_ctx      = NULL;
    q->child_count    = 0;
    q->child_capacity = 0;
    q->_drain_started = false;
    /* Plan 173.2: default = no supervisor (byte-parity path). Codegen stamps
     * has_supervisor right after this call when a Supervisor handler is
     * ambient at scope entry. */
    q->has_supervisor = false;
    q->_deciding      = false;
    /* Plan 175 (owner TODO closure, 2026-07-10): virtual-clock auto-idle-
     * advance registry — lazy-alloc'd (NULL until first virtual sleep),
     * see NovaVClockEntry comment above. */
    q->vclock_entries = NULL;
    q->vclock_count   = 0;
    q->vclock_cap     = 0;
    /* Plan 22 Ф.7: arrays — lazy alloc'нутся в nova_fiber_spawn_into.
     * Idle scope (count=0) = ~100 bytes на стеке. */
}

/* [221.1 #38 / M-sequential-serve-instances-stale-state] (2026-07-23):
 * hermetic init for RUNTIME-CONTAINER scopes (per-worker `w->scope`, the
 * orphan scope) — long-lived bookkeeping structs that are NOT semantic
 * children of whatever user scope happens to be `_nova_active_scope` at
 * their (lazy) creation. Plain nova_scope_init inherits the ambient D349
 * deadline (fibers.h:~875) + captures `saved_active_scope` — both are
 * snapshots of the ARMING call-site. The worker pool arms lazily on the
 * FIRST `spawn` of the process; if that happens inside a
 * `supervised(timeout:)` block, every worker's process-lifetime scope is
 * born with that block's absolute deadline baked in. Any LATER nested
 * `supervised{}`/`supervised(deadline:)` scope_init'd on a worker fiber
 * (ambient = `w->scope`) then inherits the long-EXPIRED deadline →
 * nova_deadline_combine keeps the earlier (stale) point → instant bogus
 * TimeoutError in an unrelated later test/request. Proven by live trace on
 * scratch38/repro38g2 (sequential serve instances, 221.1 #38).
 * Deadline/cancel enforcement for real children goes through their
 * `_nova_parent_scope` (deliver_cancel walk / pending_remote), never
 * through the worker scope's own deadline — clearing these fields loses
 * nothing. `saved_active_scope` is likewise cleared: it would otherwise
 * dangle at the armer's C stack frame for the rest of the process. */
static inline void nova_scope_init_container(NovaFiberQueue* q) {
    nova_scope_init(q);
    q->deadline_ns = 0;
    q->saved_active_scope = NULL;
}

/* Plan 175 (owner TODO closure, 2026-07-10): virtual-clock registry helpers.
 * See the NovaVClockEntry comment block above for the full design. These
 * are deliberately simple/non-atomic (single-thread contract, see there). */

/* Registers `deadline_ms` for the calling fiber; returns the index to pass
 * to `nova_vclock_check_and_consume`/later bookkeeping. Always appends (no
 * free-slot reuse) — test-scale usage, bounded by total sleep() calls in
 * one scope's lifetime; the scope itself is freed at block-exit. */
static inline int nova_vclock_register(NovaFiberQueue* scope, mco_coro* co, int64_t deadline_ms) {
    if (scope->vclock_count >= scope->vclock_cap) {
        int new_cap = scope->vclock_cap > 0 ? scope->vclock_cap * 2 : 8;
        NovaVClockEntry* grown = (NovaVClockEntry*)nova_alloc(sizeof(NovaVClockEntry) * (size_t)new_cap);
        for (int i = 0; i < scope->vclock_count; i++) grown[i] = scope->vclock_entries[i];
        scope->vclock_entries = grown;
        scope->vclock_cap = new_cap;
    }
    int idx = scope->vclock_count++;
    scope->vclock_entries[idx].deadline_ms = deadline_ms;
    scope->vclock_entries[idx].co = co;
    scope->vclock_entries[idx].fired = false;
    scope->vclock_entries[idx].used = true;
    return idx;
}

/* Count of entries still `used` (registered, not yet fired+consumed) —
 * i.e. how many alive fibers of this scope are CURRENTLY blocked in
 * `nova_vclock_park_until`. Compared against `nova_vclock_alive_count`
 * below to detect "everyone is virtually parked" idle. */
static inline int nova_vclock_pending_count(NovaFiberQueue* scope) {
    int n = 0;
    for (int i = 0; i < scope->vclock_count; i++) {
        if (scope->vclock_entries[i].used) n++;
    }
    return n;
}

/* Total alive-fiber count for THIS scope, spanning BOTH bookkeeping
 * schemes fibers can be spawned under:
 *  - Local/bootstrap path (`nova_fiber_spawn_into`) — tracked in
 *    `scope->fibers[]`/`count`, queried via `nova_sched_count_alive`
 *    (forward-declared above, defined in nova_sched.h).
 *  - Default ARMED M:N path (`nova_runtime_spawn_into`, auto-armed on
 *    first `spawn` — Plan 83.2/173.0) — these children are NEVER added
 *    to `scope->fibers[]`/`count` at all (separate `child_error[]`/
 *    `child_ctx[]` index space, runtime.c `nova_runtime_spawn_into`);
 *    `nova_sched_count_alive` is BLIND to them. The scope's own
 *    `pending_remote` atomic counter (incremented before the push,
 *    decremented on completion — same counter the drain/join loop
 *    already waits on for `== 0`) is the correct "still alive" signal
 *    for this path.
 *
 * A scope with `spawn`ed fibers under the (typical, auto-armed) default
 * therefore has `nova_sched_count_alive(scope) == 0` always — using ONLY
 * that (as an earlier iteration of this function did) makes ANY single
 * registered vclock entry look like "the whole scope is idle" the moment
 * the FIRST fiber parks, regardless of how many siblings are still about
 * to register — firing prematurely, in spawn order instead of deadline
 * order. Summing both counters fixes this: `pending_remote` was already
 * incremented for ALL siblings before the drain/join point is reached
 * (codegen emits every `spawn` statement before the scope's join —
 * same invariant the R2 tripwire in `nova_scope_grow_children` relies
 * on), so by the time the FIRST fiber's body starts running, the total
 * is already final. */
static inline int nova_vclock_alive_count(NovaFiberQueue* scope) {
    return nova_sched_count_alive(scope) + (int)nova_aint_load(&scope->pending_remote);
}

/* Fires the entry (or entries, on an exact deadline tie) with the smallest
 * deadline among still-pending entries. No-op if there are none. Called
 * whenever a virtually-parked fiber observes the scope is fully idle
 * (see `nova_vclock_park_until`) — may fire a DIFFERENT fiber's entry
 * than the caller's own. */
static inline void nova_vclock_fire_earliest(NovaFiberQueue* scope) {
    int64_t min_deadline = 0;
    nova_bool found = false;
    for (int i = 0; i < scope->vclock_count; i++) {
        if (!scope->vclock_entries[i].used) continue;
        if (!found || scope->vclock_entries[i].deadline_ms < min_deadline) {
            min_deadline = scope->vclock_entries[i].deadline_ms;
            found = true;
        }
    }
    if (!found) return;
    for (int i = 0; i < scope->vclock_count; i++) {
        if (scope->vclock_entries[i].used && scope->vclock_entries[i].deadline_ms == min_deadline) {
            scope->vclock_entries[i].fired = true;
        }
    }
}

/* Called by the parked fiber itself (via its own `idx`), on every
 * re-resume, until it observes `fired`. Consumes (frees) its own slot the
 * moment it observes the fire — never touches another fiber's entry. */
static inline nova_bool nova_vclock_check_and_consume(NovaFiberQueue* scope, int idx) {
    if (!scope->vclock_entries[idx].fired) return false;
    scope->vclock_entries[idx].used = false;
    return true;
}

/* Plan 44.5 Layer 5 park/wake: alloc/free slots in a worker scope.
 *
 * Worker-spawned fibers need a slot in the worker's NovaFiberQueue so
 * that nova_sched_park/wake can track their parked state (used by
 * Time.sleep and Channel.recv). These functions are called from the
 * fiber's entry function (codegen-emitted preamble/epilogue).
 *
 * Reuses freed slots (fibers[i] == NULL) to avoid unbounded growth
 * when fibers complete and new ones spawn. */
/* Forward-decl: nova_sched_grow_state / nova_sched_get_state defined in
 * sched.h (included AFTER fibers.h). Used by alloc_slot when growing scope
 * arrays, and by _nova_sleep_via_driver to pre-initialize sched_state before
 * driver job submission (Plan 83.11 Phase A fix). */
static inline void nova_sched_grow_state(NovaFiberQueue* scope, int new_cap);
static inline NovaSchedState* nova_sched_get_state(NovaFiberQueue* scope);
static inline int nova_scope_alloc_slot(NovaFiberQueue* scope, mco_coro* co) {
    /* Plan 83.11 Ф.3.B: spinlock — alloc_slot is called concurrently from
     * fiber preambles (one per worker thread). Without serialization, two workers
     * can both see fibers[i]==NULL and both claim slot i; the loser's fiber gets
     * a WRONG-FIBER close_cb → wake skipped → permanent hang. */
    int _sl_exp = 0;
    while (!__atomic_compare_exchange_n(
                &scope->slot_lock, &_sl_exp, 1,
                false, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        _sl_exp = 0;
    }

    void* user = mco_get_user_data(co);  /* SpawnCtx — must be GC-rooted */
    for (int i = 0; i < scope->count; i++) {
        if (scope->fibers[i] == NULL) {
            /* Plan 83.11 fix: check for stale parked/pending_wake before reuse.
             * If parked[i]=true while fibers[i]=NULL, the original fiber is still
             * alive in mco_yield and its close_cb hasn't fired yet. Reusing this
             * slot would cause close_cb to see WRONG-FIBER and skip the wake,
             * leaving the original fiber permanently stuck in mco_yield.
             *
             * Fix: SKIP stale slots entirely. close_cb (Fix B in driver.c) will
             * clear parked[i] and directly dispatch the original fiber when the
             * timer fires. After parked[i]=false, the slot becomes eligible for
             * reuse by the next alloc_slot call. */
            {
                NovaSchedState* _das = nova_sched_find_state(scope);
                if (_das && i < _das->capacity) {
                    /* Plan 83-go-cmn Ф.2 (correction #5): the new skip-stale
                     * predicate is parked[i] ALONE (the pending_wake signal is
                     * deleted). parked[i] is set at gopark and cleared only by the
                     * goready winner / cancel / driver close_cb, so while it is
                     * true the original fiber is still parked (alive in mco_yield)
                     * and its parked_co[i] carrier still points to it. Reusing the
                     * slot now would let a wake/cancel resolve the WRONG identity.
                     * Skipping keeps the slot pinned until the genuine wake clears
                     * parked[i] — at which point parked_co[i] is also NULL'd, so the
                     * NULL'd-but-alive window is handled by the by-co wake (driver
                     * Fix-B dispatches expected_co directly; cancel/primitive wake
                     * uses parked_co[i], never fibers[i]). */
                    bool _das_pk = __atomic_load_n((volatile bool*)nova_sched_parked_at(_das, i), __ATOMIC_SEQ_CST);
                    if (_das_pk) {
                        /* Skip: do NOT reset parked, do NOT assign co here. The
                         * genuine wake (close_cb Fix-B / goready) clears parked[i]
                         * and dispatches the original fiber; after that the slot
                         * becomes eligible for reuse. */
                        continue;
                    }
                }
            }
            scope->fibers[i]               = co;
            scope->fiber_ctx[i]            = user;  /* GC root: SpawnCtx pinned */
            scope->fiber_fail_top[i]       = NULL;
            scope->fiber_interrupt_top[i]  = NULL;
            scope->fiber_effect_snapshot[i]= NULL;
            /* Plan 201 trace-per-fiber: fresh per-fiber error-diag bucket
             * (fresh fiber = no in-flight error yet) + point the active
             * TLS pointer at it NOW — this call runs INSIDE the fiber's
             * own preamble (first resume, before any user body statement
             * that could throw), so by the time user code starts running
             * `_nova_error_state_p` already targets this slot's OWN
             * bucket instead of the calling worker's ambient one. */
            scope->fiber_error_state[i]    =
                (NovaFiberErrorState*)nova_alloc(sizeof(NovaFiberErrorState));
            _nova_error_state_p = scope->fiber_error_state[i];
            scope->fiber_error[i]          = NULL;
            scope->fiber_did_throw[i]      = NULL;
            __atomic_store_n(&scope->slot_lock, 0, __ATOMIC_RELEASE);
            return i;
        }
    }
    /* No free slot — grow arrays and take the next index. */
    if (scope->count >= scope->capacity) {
        nova_scope_grow(scope, scope->count + 1);
        if (scope->sched_state) nova_sched_grow_state(scope, scope->capacity);
    }
    /* FIX 83.10.2: Write fibers[slot]=co (and all other slot fields) BEFORE
     * incrementing count. nova_runtime_cancel_worker_fibers reads count then
     * reads fibers[j] — if count++ came first there is a window where a
     * concurrent scanner sees count=N+1 but fibers[N]=NULL and skips the slot.
     * Release store on count ensures all preceding stores are visible to any
     * thread that subsequently observes count=slot+1 (acquire-read). */
    int slot = scope->count;          /* read index; do NOT increment yet */
    scope->fibers[slot]               = co;
    scope->fiber_ctx[slot]            = user;       /* GC root: SpawnCtx pinned */
    scope->fiber_fail_top[slot]       = NULL;
    scope->fiber_interrupt_top[slot]  = NULL;
    scope->fiber_effect_snapshot[slot]= NULL;
    /* Plan 201 trace-per-fiber: see reuse-path comment above. */
    scope->fiber_error_state[slot]    =
        (NovaFiberErrorState*)nova_alloc(sizeof(NovaFiberErrorState));
    _nova_error_state_p = scope->fiber_error_state[slot];
    scope->fiber_error[slot]          = NULL;
    scope->fiber_did_throw[slot]      = NULL;
    /* Release store: makes slot visible to other threads only after all
     * field writes above are complete (prevents compiler and CPU reordering
     * on non-TSO architectures; on x86 TSO the hardware guarantees it but
     * a compiler fence is still required). */
    __atomic_store_n(&scope->count, slot + 1, __ATOMIC_RELEASE);
    __atomic_store_n(&scope->slot_lock, 0, __ATOMIC_RELEASE);
    return slot;
}

static inline void nova_scope_free_slot(NovaFiberQueue* scope, int slot) {
    if (!scope || slot < 0 || slot >= scope->count) return;
    /* Plan 83.11 Ф.3.B: lock so alloc_slot's scan cannot observe this slot
     * as NULL while we are in the middle of other epilogue work (GC root clear).
     * Without the lock, a concurrent alloc_slot could claim this slot before
     * fiber_ctx is cleared, then overwrite it. */
    int _sl_exp = 0;
    while (!__atomic_compare_exchange_n(
                &scope->slot_lock, &_sl_exp, 1,
                false, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        _sl_exp = 0;
    }
    scope->fibers[slot]    = NULL;
    scope->fiber_ctx[slot] = NULL;  /* release SpawnCtx GC root */
    __atomic_store_n(&scope->slot_lock, 0, __ATOMIC_RELEASE);
    /* sched_state parked[slot] is already false (wake cleared it). */
}

/* Plan 44.5 L5: pin SpawnCtx в parent supervised scope ctx_pins для
 * GC root protection в окне между nova_runtime_spawn_into и worker
 * resume'ом fiber'а. */
static inline void nova_scope_pin_ctx(NovaFiberQueue* scope, void* ctx) {
    if (!scope || !ctx) return;
    if (scope->ctx_pins_count >= scope->ctx_pins_cap) {
        int new_cap = scope->ctx_pins_cap > 0 ? scope->ctx_pins_cap * 2 : 16;
        /* Plan 83.11 §11.6 V2 fix (2026-06-08) [M-83.11-cancel-token-bound-race-2k]:
         * uncollectable allocation для ctx_pins array. Под high fiber count
         * (N≥2k spawns) array growth (16→32→64→...→1024+) triggers many GC
         * cycles. Pre-fix nova_alloc'd array sometimes lost root coverage
         * под heavy allocation pressure — Boehm conservative scanner could
         * miss the pointer-chain `stack-scope → ctx_pins → tokens` during
         * Mark phase, reclaiming tokens. Result: token memory reused for
         * SpawnCtx, struct overlap at offset 8 (bound_scope vs worker_slot)
         * → panic "token already bound to a live scope".
         *
         * Fix: allocate ctx_pins[] array via nova_alloc_uncollectable. Array
         * never reclaimed → tokens stored within always reachable. Array
         * still pointer-scanned (GC_malloc_uncollectable returns scanned
         * memory, just not swept). Old array becomes garbage on growth but
         * также uncollectable — small per-scope leak (~16-1024 ptrs *
         * 8 bytes = 128B-8KB tail). Acceptable: each supervised scope
         * is finite + scopes are typically not in tight loops. Тоkens
         * themselves now safely use nova_alloc (collectable) — ctx_pins
         * holds them alive. */
        void** new_pins = (void**)nova_alloc_uncollectable(sizeof(void*) * (size_t)new_cap);
        if (scope->ctx_pins) {
            for (int i = 0; i < scope->ctx_pins_count; i++) {
                new_pins[i] = scope->ctx_pins[i];
            }
            /* Free OLD uncollectable array to avoid per-scope geometric leak.
             * Old array's contents already copied — каждый ctx still alive via
             * new_pins entry (+ optional stack/other GC roots). */
            nova_free_uncollectable(scope->ctx_pins);
        }
        for (int i = scope->ctx_pins_count; i < new_cap; i++) {
            new_pins[i] = NULL;
        }
        scope->ctx_pins     = new_pins;
        scope->ctx_pins_cap = new_cap;
    }
    scope->ctx_pins[scope->ctx_pins_count++] = ctx;
}

/* Plan 173.0 Ф.2: grow child_error[]/child_ctx[] to at least new_cap slots.
 *
 * [M-mn-spawnctx-corruption-cancel-wake] fix (Plan 211 family): child_ctx[]
 * used to be nova_alloc_uncollectable'd (see git history for the original
 * "same discipline as ctx_pins" reasoning) — WRONG: that reasoning conflated
 * "the SpawnCtx entries pointed-to must survive GC pressure" (true, and
 * already guaranteed independently — they are themselves allocated via
 * nova_spawn_pool_acquire -> nova_alloc_uncollectable, a SEPARATE concern)
 * with "the child_ctx ARRAY ITSELF must be uncollectable" (false — the array
 * is reachable for exactly as long as `scope` is, via the same conservative-
 * stack-scan/GC-root path every other nova_alloc'd NovaFiberQueue field
 * already relies on; nova_alloc's normal GC-scanned memory finds the boxed
 * SpawnCtx pointers inside it just fine). Making the array itself
 * uncollectable had a hazardous side effect: at NOVA_SCOPE_INITIAL_CAP=16
 * (fibers.h above), the FIRST grow allocates exactly 16*sizeof(void*)=128
 * bytes — the SAME Boehm uncollectable size-class SpawnCtx itself pools
 * through (nova_spawn_pool_class_size[1]=128, runtime.c). Every subsequent
 * grow (32/64/...) freed the previous uncollectable buffer via
 * nova_free_uncollectable (= GC_free), handing that exact 128-byte block
 * back to Boehm's internal free list for the SAME size-class SpawnCtx draws
 * fresh allocations from during a spawn storm (2000 concurrent spawns,
 * pos_max_fibers_concurrent.nv) — gdb confirmed SIGSEGV inside
 * GC_generic_malloc_uncollectable dereferencing a corrupted free-list link
 * on a FRESH (never-before-Nova-recycled) 128-byte allocation, and a
 * separately-observed garbage `_nova_fiber_scope` surfacing later at
 * cancel-wake time is consistent with an EARLIER-corrupted SpawnCtx handed
 * out from that same reused block, only detonating once its fields are
 * actually read. child_ctx[] switched to nova_alloc (regular GC-collectable,
 * scanned) — removes the size-class collision entirely; nothing else about
 * the array's semantics (copy-on-grow, NULL-init tail) changes.
 *
 * R2 tripwire (§EXEC risk R2): asserts !scope->_drain_started. Every remote
 * child is spawned into the scope on the calling thread BEFORE the drain
 * loop starts (nova_supervised_run_impl sets _drain_started=true at loop
 * entry) — a spawned child's body has no reference to the parent's
 * stack-local NovaFiberQueue, so it structurally cannot call back into this
 * grow path during drain. The assert proves that invariant holds instead of
 * silently trusting it — if it ever fires, the fix is chunked stable-address
 * storage (Option A fallback, §EXEC risk R2), NOT loosening this assert. */
static inline void nova_scope_grow_children(NovaFiberQueue* scope, int new_cap) {
    if (new_cap <= scope->child_capacity) return;
    assert(!scope->_drain_started &&
           "[M-173.0-R2] child_error[] grow-during-drain — torn-base risk, "
           "see §EXEC risk R2 in docs/plans/173.0-concurrency-runtime-substrate.md");
    int cap = scope->child_capacity > 0 ? scope->child_capacity : NOVA_SCOPE_INITIAL_CAP;
    while (cap < new_cap) cap *= 2;
    NovaChildError* new_err = (NovaChildError*)nova_alloc(sizeof(NovaChildError) * (size_t)cap);
    void**          new_ctx = (void**)nova_alloc(sizeof(void*) * (size_t)cap);
    if (scope->child_error) {
        for (int i = 0; i < scope->child_count; i++) {
            new_err[i] = scope->child_error[i];
            new_ctx[i] = scope->child_ctx[i];
        }
        /* child_error's and child_ctx's old arrays are both regular
         * GC-collectable now — no manual free (GC reclaims once
         * unreachable, same as every other nova_scope_grow-style array). */
    }
    for (int i = scope->child_count; i < cap; i++) {
        new_err[i].msg = NULL;
        new_err[i].kind = NOVA_THROW_USER;
        new_err[i].reason = NULL;
        new_err[i].payload = NULL;
        new_err[i].tid = 0;
        nova_abool_init(&new_err[i].published, false);  /* Plan 173.2 */
        new_err[i].decided = false;                      /* Plan 173.2 */
        new_ctx[i] = NULL;
    }
    scope->child_error    = new_err;
    scope->child_ctx      = new_ctx;
    scope->child_capacity = cap;
}

/* Plan 173.0 Ф.2/A2.2: allocate a fresh per-child retention slot for a
 * REMOTE (M:N) child, at spawn time, on the thread executing the `spawn`
 * statement. Not lock-protected — matches the established invariant of
 * nova_scope_pin_ctx just above (also non-atomic): spawn-time calls into a
 * given parent scope happen sequentially on one thread (D50 structured
 * concurrency — a spawned body has no handle to the parent's local scope
 * variable, so it cannot itself call spawn into it), so concurrent callers
 * of THIS function against the SAME scope do not occur in practice. */
static inline int nova_scope_alloc_child_slot(NovaFiberQueue* scope) {
    if (scope->child_count >= scope->child_capacity) {
        nova_scope_grow_children(scope, scope->child_count + 1);
    }
    int idx = scope->child_count++;
    scope->child_error[idx].msg = NULL;
    nova_abool_init(&scope->child_error[idx].published, false);  /* Plan 173.2 */
    scope->child_error[idx].decided = false;                      /* Plan 173.2 */
    scope->child_error[idx].escalated = false;                    /* D414 §1 агрегация */
    scope->child_ctx[idx] = NULL;
    return idx;
}

/* Plan 173.0 Ф.2/A2.4: read-API — collect all retained (non-empty) child
 * errors for this scope into caller-supplied `out` (capacity `cap`).
 * Returns the number written. Safe to call any time (empty slots simply
 * skipped) but the intended contract (§EXEC A2.4) is: call only after
 * `pending_remote == 0` has been observed (the existing acquire-load gate
 * already present at every nova_supervised_run_impl/drain_main_scope exit
 * path) — that established happens-before is what makes the plain (non-
 * atomic) `child_error[]` writes below visible here. */
static inline int nova_scope_collect_child_errors(NovaFiberQueue* scope,
                                                    NovaChildError* out,
                                                    int cap) {
    if (!scope || !scope->child_error) return 0;
    int n = 0;
    for (int i = 0; i < scope->child_count && n < cap; i++) {
        if (scope->child_error[i].msg != NULL) {
            out[n++] = scope->child_error[i];
        }
    }
    return n;
}

/* ---- D75 (revised, Plan 47): CancelToken — caller-owned cancellation handle ----
 *
 * Модель: токен — caller-owned значение, создаётся `CancelToken.new()`,
 * живёт сколько нужно вызывающему коду. `supervised(cancel: tok)` при входе
 * ПРИВЯЗЫВАЕТ токен к scope'у (`bind`), при выходе — ОТВЯЗЫВАЕТ (`unbind`).
 * Токен переживает scope: `cancel()` на отвязанном / завершённом scope'е —
 * безвредный no-op (только записывает intent в сам токен).
 *
 * Поля:
 *  - cancel_requested — intent-флаг: был ли вызван cancel() на этом токене.
 *    Сохраняется навсегда (kill-switch остаётся flipped). `is_cancelled()`
 *    читает именно его — токен это first-class handle, ответ не зависит от
 *    того, привязан ли он сейчас.
 *  - bound_scope — живой scope, к которому токен сейчас привязан, или NULL.
 *    Bind-check: повторный bind при non-NULL → runtime panic.
 *  - linked[] — динамический список токенов-каскадов: при cancel() этого
 *    токена каскадно отменяются они. Растёт геометрически; GC-managed
 *    (nova_alloc), чтобы хранимые указатели не давали GC собрать цели. */
/* Plan 65 Ф.10: resource cleanup callback registered against a CancelToken.
 * Invoked from nova_cancel_token_cancel_reason. Callback receives the
 * resource handle (e.g. NovaAfterState* for a close_after timer) and is
 * responsible for stopping/closing the underlying OS resource.
 *
 * Idempotent: caller MUST tolerate being called twice (one cancel may race
 * with the resource's own natural completion path). */
typedef void (*NovaCancelResourceCb)(void* handle);

typedef struct NovaCancelToken {
    /* Plan 83.4.3/B5: atomic intent-flag — cancel() пишет (любой поток),
     * is_cancelled() читает. ACQUIRE-load + RELEASE-store. */
    nova_atomic_bool          cancel_requested;
    NovaFiberQueue*           bound_scope;       /* live scope, либо NULL */
    struct NovaCancelToken**  linked;            /* cascade children (GC array) */
    int                       linked_count;
    int                       linked_cap;
    /* Plan 49 Ф.1: typed reason — box'нутый T (caller-owned, переживает
     * scope). Для CancelToken[str] указывает на nova_str с сообщением
     * (default "cancelled" если cancel() без arg). NULL когда cancel()
     * ещё не вызван. */
    void*                     reason_ptr;
    nova_bool                 has_reason;        /* true ↔ cancel() уже сработал */
    /* Plan 49 Ф.6 cross-type cascade: per-link converter B→A (NULL =
     * same-type pass-through). Parallel array к linked[], same length.
     * Lazy-allocated (NULL пока ни одного cross-type cascade'а).
     * Converter signature: `void* (B-reason) → void* (A-reason boxed)` —
     * codegen эмитит wrapper который unbox'ит B, вызывает A.from(b),
     * box'ит A. */
    void*                  (**linked_converters)(void*);
    /* Plan 65 Ф.10: cancel-aware resource list (timers, file handles, etc).
     * При cancel() — каждый callback вызывается с соответствующим handle.
     * Используется ChanReader.close_after timers для cleanup без firing.
     *
     * Параллельные arrays — растут вместе. NULL handle/cb skip'аются (lazy
     * de-registration mark). GC-managed (nova_alloc). */
    void**                    cleanup_handles;
    NovaCancelResourceCb*     cleanup_cbs;
    int                       cleanup_count;
    int                       cleanup_cap;
} NovaCancelToken;

/* Аллокация GC-managed токена. nova_alloc zero-инициализирует — все поля
 * 0/NULL/false, токен сразу валиден (unbound, не-cancelled, без каскадов). */
/* ✅ SUPERSEDED V1 hypothesis (Plan 83.11 [M-83.11-cancel-token-bound-race-2k], 2026-06-05):
 * V1 пытался делать uncollectable allocation самого NovaCancelToken. Под
 * high fiber count (≥2k spawns в supervised(cancel:) scope) ctx_pins[]-based
 * GC root protection (Plan 83.11 §11.6) казалось insufficient — Boehm GC
 * reclaims token while it's still в use, then memory is reallocated for new
 * SpawnCtx struct whose write at offset 8 (_nova_worker_slot) overlaps
 * с token->bound_scope offset → panic "token already bound to a live
 * scope" on bind.
 *
 * РЕЗОЛЮЦИЯ V2/V3 (2026-06-08): корень был не в токене, а в ctx_pins[]
 * array, который сам аллоцировался через nova_alloc и терял root coverage
 * под GC pressure. Fix перенесён туда (nova_scope_pin_ctx →
 * nova_alloc_uncollectable для ctx_pins[]); токен снова collectable
 * (nova_alloc). 30/30 @10k. Followup [M-83.11-cancel-token-explicit-cleanup]
 * для explicit dispose API остаётся отдельным P2-вопросом.
 *
 * Root cause hypothesis (исторический V1): Plan 83.11 §11.6 ctx_pins[]-pin
 * works in the pin-time window but Boehm conservative scanner может потерять
 * root под heavy GC pressure (frequent collections triggered by 2k+ spawn
 * allocations + ctx GC churn). Same defensive pattern as Plan 83.4.5.8
 * SpawnCtx uncollectable. */
static inline NovaCancelToken* nova_cancel_token_new(void) {
    /* ✅ RESOLVED V2/V3 (Plan 83.11 §11.6, 2026-06-08) [M-83.11-cancel-token-bound-race-2k]:
     * race устранена переносом GC-root protection на ctx_pins[] array
     * (nova_alloc_uncollectable, см. nova_scope_pin_ctx выше) — 30/30 @10k.
     * Токен сам остаётся collectable (nova_alloc): ctx_pins[] держит его
     * живым, отдельный uncollectable per-token больше не нужен. */
    return (NovaCancelToken*)nova_alloc(sizeof(NovaCancelToken));
}

/* Привязать токен к scope'у (вызывается emit_supervised при входе).
 * Bind-check: токен уже привязан к живому scope'у → runtime panic.
 * Если cancel() уже был вызван до bind'а — отмена немедленно
 * пробрасывается в свежепривязанный scope. */
static inline void nova_cancel_token_bind(NovaCancelToken* t, NovaFiberQueue* q) {
    if (!t || !q) return;
    if (t->bound_scope != NULL) {
        fprintf(stderr, "nova: panic: token already bound to a live scope\n");
        abort();
    }
    t->bound_scope = q;
    /* Plan 65 Ф.10: reverse-pointer for resource cancel-registration lookup. */
    q->bound_token = (void*)t;
    /* cancel-before-bind: pending intent пробрасывается в новый scope.
     * Plan 49 Ф.2: reason тоже копируется чтобы nova_fiber_yield увидел
     * её при throw'е CANCEL.
     *
     * Plan 83.11 [M-83.11-nested-supervised-cascade-drain-hang] fix
     * (2026-06-05): полная пропагация cancel'а. Pre-fix только
     * nova_sched_cancel_all_pending вызывался — но для armed M:N workers
     * fibers parked в worker scopes (не supervised), и armed sleeps
     * через driver — нужны те же пути что и в nova_cancel_token_cancel_reason:
     * nova_scope_cancel_wake_all (ASYNC slots) + nova_runtime_cancel_worker_fibers
     * (worker-parked fibers с parent_scope==q) + _nova_cancel_via_driver
     * (armed timers через driver). Без них cascade-cancel (outer fires
     * cancel BEFORE outer.bind, cascade to inner_tok hits its scope, но
     * outer.bind LATER только частично пропагирует) leaves outer's worker
     * fibers parked → supervised_run hangs до watchdog timeout. */
    if (nova_abool_load(&t->cancel_requested)) {
        nova_abool_store(&q->cancel_requested, true);
        q->cancel_reason_ptr = t->reason_ptr;
        nova_sched_cancel_all_pending(q);
        nova_scope_cancel_wake_all(q);
        {
            extern void nova_runtime_cancel_worker_fibers(
                struct NovaFiberQueue* scope);
            nova_runtime_cancel_worker_fibers(q);
        }
        _nova_cancel_via_driver(q);
    }
}

/* Отвязать токен от scope'а (вызывается emit_supervised на выходе, включая
 * throw-путь). Intent-флаг (cancel_requested) НЕ сбрасывается — токен
 * помнит, что был отменён. */
static inline void nova_cancel_token_unbind(NovaCancelToken* t) {
    if (!t) return;
    /* Plan 65 Ф.10: clear reverse-pointer too. */
    if (t->bound_scope) {
        t->bound_scope->bound_token = NULL;
    }
    t->bound_scope = NULL;
}

/* Plan 65 Ф.10: register cancel-aware resource. Returns slot index for
 * later unregister (>= 0), or -1 on failure. Idempotent only at the
 * caller's discretion (re-register with same handle creates a 2nd slot).
 *
 * Если token уже cancelled — cb вызывается immediately и регистрация
 * skip'ается (handle бесполезно держать в списке для уже-cancelled token'а).
 * Slot index в этом случае возвращается == -1.
 *
 * Growth strategy: геометрический (×2), GC-managed массивы. */
static inline int nova_cancel_token_register_resource(NovaCancelToken* t,
                                                      void* handle,
                                                      NovaCancelResourceCb cb) {
    if (!t || !cb || !handle) return -1;
    if (nova_abool_load(&t->cancel_requested)) {
        /* Late registration: token уже cancelled — выполняем cleanup
         * сразу, не записываем в список. */
        cb(handle);
        return -1;
    }
    if (t->cleanup_count >= t->cleanup_cap) {
        int new_cap = t->cleanup_cap > 0 ? t->cleanup_cap * 2 : 4;
        void** new_handles = (void**)nova_alloc(sizeof(void*) * new_cap);
        NovaCancelResourceCb* new_cbs = (NovaCancelResourceCb*)nova_alloc(sizeof(NovaCancelResourceCb) * new_cap);
        for (int i = 0; i < t->cleanup_count; i++) {
            new_handles[i] = t->cleanup_handles[i];
            new_cbs[i]     = t->cleanup_cbs[i];
        }
        for (int i = t->cleanup_count; i < new_cap; i++) {
            new_handles[i] = NULL;
            new_cbs[i]     = NULL;
        }
        t->cleanup_handles = new_handles;
        t->cleanup_cbs     = new_cbs;
        t->cleanup_cap     = new_cap;
    }
    int slot = t->cleanup_count++;
    t->cleanup_handles[slot] = handle;
    t->cleanup_cbs[slot]     = cb;
    return slot;
}

/* Plan 65 Ф.10: unregister cancel resource (timer fired naturally, etc).
 * Idempotent — slot может быть уже -1 или соответствовать уже-cleared entry. */
static inline void nova_cancel_token_unregister_resource(NovaCancelToken* t, int slot) {
    if (!t || slot < 0 || slot >= t->cleanup_count) return;
    t->cleanup_handles[slot] = NULL;
    t->cleanup_cbs[slot]     = NULL;
}

/* Запросить отмену с типизированной причиной (Plan 49 Ф.1). `reason_ptr` —
 * box'нутый T (caller-owned). NULL допустим (отмена без структурированной
 * причины). Idempotent: повторный cancel сохраняет ПЕРВУЮ причину
 * (first-cancel-wins) — как в Go context.Cause. */
static inline void nova_cancel_token_cancel_reason(NovaCancelToken* t, void* reason_ptr) {
    if (!t) return;
    if (nova_abool_load(&t->cancel_requested)) return;  /* idempotent — first-cancel-wins */
    nova_abool_store(&t->cancel_requested, true);
    t->reason_ptr = reason_ptr;
    t->has_reason = true;
    /* Plan 65 Ф.10: invoke registered cancel-resource cleanup callbacks
     * (timers, FDs, etc.) BEFORE waking parked fibers — так resource
     * shutdown viewable как atomic с cancel propagation. */
    for (int i = 0; i < t->cleanup_count; i++) {
        if (t->cleanup_cbs[i] && t->cleanup_handles[i]) {
            NovaCancelResourceCb cb = t->cleanup_cbs[i];
            void* h = t->cleanup_handles[i];
            /* Clear slot BEFORE invoking, so a cb that calls unregister
             * (idempotent path) sees a no-op. */
            t->cleanup_handles[i] = NULL;
            t->cleanup_cbs[i]     = NULL;
            cb(h);
        }
    }
    if (t->bound_scope) {
        nova_abool_store(&t->bound_scope->cancel_requested, true);
        /* Plan 49 Ф.2: пропагируем reason в scope queue чтобы nova_fiber_yield
         * увидел причину при throw'е CANCEL. */
        t->bound_scope->cancel_reason_ptr = reason_ptr;
        /* Plan 22 Ф.4 (D93): wake all parked fiber'ов через registered
         * stop_cb's — immediate, не дожидаясь следующего yield-point'а.
         *
         * Plan 83.4.5.1 (2026-05-23): cancel_all_pending теперь зовёт
         * nova_sched_wake (вместо просто parked=false) → SYNC slots тоже
         * получают dispatch_ready re-queue. */
        nova_sched_cancel_all_pending(t->bound_scope);
        /* Plan 83.4.5.1 Ф.1: defense-in-depth wake_all — покрывает any
         * parked slot ASYNC handle которого ещё не закрылся (close_cb
         * запланирован, но fiber-side cancel-check может среагировать
         * раньше через predicate park_until → cancel_requested =true
         * заставит predicate exit'нуться). Идемпотентно: parked-флаги уже
         * cleared cancel_all_pending'ом для SYNC+bare; ASYNC slot'ы
         * остаются parked, на них wake_all сделает dispatch_ready —
         * predicate re-check вернёт true → exit. */
        nova_scope_cancel_wake_all(t->bound_scope);
        /* Plan 83.10.2 (2026-05-26): under armed M:N, spawned fibers park in
         * worker scopes (not the supervised scope). nova_sched_cancel_all_pending
         * above found nothing. Route cancel to worker-parked fibers whose
         * _nova_parent_scope == bound_scope. External non-inline — declared in
         * runtime.h (included after fibers.h in nova_rt.h); forward-decl here
         * to break include-order circular dependency. */
        {
            extern void nova_runtime_cancel_worker_fibers(
                struct NovaFiberQueue* scope);
            nova_runtime_cancel_worker_fibers(t->bound_scope);
        }
        /* Plan 83.11 Ф.3: also submit CANCEL_SCOPE job to driver. Driver walks
         * scope.armed_sleeps_head list (single mutator) — closes any timers armed
         * via _nova_sleep_via_driver. Idempotent с legacy cancel_worker_fibers
         * (которое touches scoped fibers parked via _nova_sleep_via_libuv path):
         * no-op для slots that аre driver-armed (legacy cb=NULL), no-op для
         * slots that аre legacy-armed (driver list doesn't contain them). */
        _nova_cancel_via_driver(t->bound_scope);
    }
    /* Каскад: отменяем все linked-токены (kill-switch composition).
     * Plan 49 Ф.6 cross-type: если для link есть converter — применяем
     * `converter(reason_ptr)` чтобы child получил correctly-typed reason
     * (B → A через `A.from(B)` wrapper'ом). NULL converter = same-type
     * pass-through (existing behavior).
     * Реализация безопасна даже когда linked_converters == NULL —
     * проверка на каждой итерации (cross-type не активирован → array NULL). */
    for (int i = 0; i < t->linked_count; i++) {
        NovaCancelToken* other = t->linked[i];
        if (!other) continue;
        void* converted_reason = reason_ptr;
        if (t->linked_converters && t->linked_converters[i] && reason_ptr) {
            converted_reason = t->linked_converters[i](reason_ptr);
        }
        nova_cancel_token_cancel_reason(other, converted_reason);
    }
}

/* Backward-compatible wrapper: cancel без явной причины. Plan 49 Ф.1:
 * default reason — NULL (caller-сайт codegen передаёт `"cancelled"` для
 * CancelToken[str] чтобы reason() возвращал Some, а не None). */
static inline void nova_cancel_token_cancel(NovaCancelToken* t) {
    nova_cancel_token_cancel_reason(t, NULL);
}

/* Чтение intent-флага без yield. Не throws. Отражает «был ли вызван
 * cancel() на этом токене» — независимо от bind-состояния. */
static inline nova_bool nova_cancel_token_is_cancelled(NovaCancelToken* t) {
    if (!t) return false;
    return nova_abool_load(&t->cancel_requested);
}

/* Plan 49 Ф.1: возвращает box'нутую причину отмены или NULL если отмена
 * ещё не вызвана / была без reason. Caller'у вернётся `Option[T]` на
 * Nova-уровне (NULL → None, иначе Some). */
static inline void* nova_cancel_token_reason(NovaCancelToken* t) {
    if (!t) return NULL;
    if (!t->has_reason) return NULL;
    return t->reason_ptr;
}

/* Plan 49 Ф.1: проверка наличия reason — нужна codegen'у чтобы решить
 * между None и Some(deref(reason_ptr)). Отделена от is_cancelled потому
 * что cancel может быть вызван с NULL reason (отмена без причины). */
static inline nova_bool nova_cancel_token_has_reason(NovaCancelToken* t) {
    if (!t) return false;
    return t->has_reason && t->reason_ptr != NULL;
}

/* Plan 49 Ф.1: typed-getter для CancelToken[str] — возвращает Option[str].
 * `reason_ptr` хранит box'нутый nova_str (caller-side boxed на cancel-site).
 * Codegen дергает эту функцию для `tok.reason()` когда T=str (default). */
static inline NovaOpt_nova_str nova_cancel_token_reason_str(NovaCancelToken* t) {
    NovaOpt_nova_str r;
    if (!t || !t->has_reason || t->reason_ptr == NULL) {
        r.tag = NOVA_TAG_Option_None;
        r.value = (nova_str){0, 0};
        return r;
    }
    r.tag = NOVA_TAG_Option_Some;
    r.value = *(nova_str*)t->reason_ptr;
    return r;
}

/* Plan 49 Ф.6 P0 fix: raw void* reason getter для per-T un-box.
 * Codegen для `tok.reason()` где T≠str эмитит ternary:
 *   nova_cancel_token_has_reason(tok)
 *     ? (NovaOpt_T){.tag=Some, .value=*(T*)nova_cancel_token_reason_raw(tok)}
 *     : (NovaOpt_T){.tag=None}
 * Возвращает NULL когда отмены не было или reason_ptr NULL —
 * caller использует has_reason() как guard. */
static inline void* nova_cancel_token_reason_raw(NovaCancelToken* t) {
    if (!t || !t->has_reason) return NULL;
    return t->reason_ptr;
}

/* Plan 49 Ф.1: helper — alloc copy of nova_str on GC heap so reason
 * outlives the caller's stack frame. Used by codegen для `tok.cancel(reason)`
 * когда T=str (default CancelToken). */
static inline void* nova_cancel_box_str(nova_str s) {
    nova_str* boxed = (nova_str*)nova_alloc(sizeof(nova_str));
    *boxed = s;
    return (void*)boxed;
}

/* Plan 49 Ф.6: generic box для CancelToken[T] где T ≠ str — memcpy
 * произвольного size'а в GC-heap, возврат void*. Codegen эмитит
 * через compound literal: `nova_cancel_box_copy_raw(&((T){val}), sizeof(T))`.
 * Per-T un-box на стороне reason()-getter'а (см. emit_c.rs). */
static inline void* nova_cancel_box_copy_raw(const void* src, int64_t size) {
    void* boxed = nova_alloc((size_t)size);
    if (src && size > 0) {
        memcpy(boxed, src, (size_t)size);
    }
    return boxed;
}

/* Направленный каскад: Nova-уровень `child.cancelled_by(parent)` — когда
 * `parent.cancel()` сработает, `child` тоже будет отменён (но НЕ наоборот:
 * отмена течёт только вниз, parent → child). Реализация: `child`
 * добавляется в `parent->linked[]`. Динамический рост массива (GC-managed
 * copy). Если `parent` уже отменён — `child` отменяется немедленно.
 * Параметры названы tok/other по historical reasons — семантически
 * tok = child, other = parent. */
static inline void nova_cancel_token_bind_cascade(NovaCancelToken* tok,
                                                  NovaCancelToken* other) {
    if (!tok || !other) return;
    if (other->linked_count >= other->linked_cap) {
        int new_cap = other->linked_cap > 0 ? other->linked_cap * 2 : 4;
        NovaCancelToken** grown = (NovaCancelToken**)nova_alloc(
            (size_t)new_cap * sizeof(NovaCancelToken*));
        for (int i = 0; i < other->linked_count; i++) {
            grown[i] = other->linked[i];
        }
        other->linked = grown;
        other->linked_cap = new_cap;
        /* Также вырастить linked_converters parallel array (lazy alloc). */
        if (other->linked_converters) {
            void* (**grown_conv)(void*) = (void* (**)(void*))nova_alloc(
                (size_t)new_cap * sizeof(void* (*)(void*)));
            for (int i = 0; i < other->linked_count; i++) {
                grown_conv[i] = other->linked_converters[i];
            }
            other->linked_converters = grown_conv;
        }
    }
    other->linked[other->linked_count] = tok;
    /* same-type cascade: converter NULL. Parallel array NULL'ит entry
     * автоматически если linked_converters NULL — иначе явный NULL. */
    if (other->linked_converters) {
        other->linked_converters[other->linked_count] = NULL;
    }
    other->linked_count++;
    /* Если other уже отменён — пробрасываем немедленно (same-type). */
    if (nova_abool_load(&other->cancel_requested)) {
        nova_cancel_token_cancel_reason(tok, other->reason_ptr);
    }
}

/* Plan 49 Ф.6 cross-type cascade: `child.cancelled_by(parent)` где типы
 * причин разные. `converter` — codegen-generated wrapper:
 *   void* my_from_B_to_A(void* b_reason_ptr) {
 *       B b = *(B*)b_reason_ptr;
 *       A a = nova_fn_A_from_B(b);
 *       A* boxed = (A*)nova_alloc(sizeof(A));
 *       *boxed = a;
 *       return (void*)boxed;
 *   }
 * При cancel parent — для этого link runtime применяет converter перед
 * cancel(child). Безопасно даже если ни одного cross-type нет —
 * linked_converters остаётся NULL (lazy). */
static inline void nova_cancel_token_bind_cascade_typed(
    NovaCancelToken* tok,
    NovaCancelToken* other,
    void* (*converter)(void*))
{
    if (!tok || !other) return;
    /* Grow linked[] + linked_converters[] параллельно. */
    if (other->linked_count >= other->linked_cap) {
        int new_cap = other->linked_cap > 0 ? other->linked_cap * 2 : 4;
        NovaCancelToken** grown = (NovaCancelToken**)nova_alloc(
            (size_t)new_cap * sizeof(NovaCancelToken*));
        for (int i = 0; i < other->linked_count; i++) {
            grown[i] = other->linked[i];
        }
        other->linked = grown;
        void* (**grown_conv)(void*) = (void* (**)(void*))nova_alloc(
            (size_t)new_cap * sizeof(void* (*)(void*)));
        for (int i = 0; i < other->linked_count; i++) {
            grown_conv[i] = other->linked_converters
                ? other->linked_converters[i] : NULL;
        }
        other->linked_converters = grown_conv;
        other->linked_cap = new_cap;
    } else if (!other->linked_converters) {
        /* First cross-type link — lazy-alloc converter array, NULL-fill
         * existing entries (those были same-type). */
        void* (**conv)(void*) = (void* (**)(void*))nova_alloc(
            (size_t)other->linked_cap * sizeof(void* (*)(void*)));
        for (int i = 0; i < other->linked_count; i++) conv[i] = NULL;
        other->linked_converters = conv;
    }
    other->linked[other->linked_count] = tok;
    other->linked_converters[other->linked_count] = converter;
    other->linked_count++;
    /* Если other уже отменён — applied конвертер немедленно. */
    if (nova_abool_load(&other->cancel_requested)) {
        void* converted = other->reason_ptr;
        if (converter && other->reason_ptr) {
            converted = converter(other->reason_ptr);
        }
        nova_cancel_token_cancel_reason(tok, converted);
    }
}

/* Plan 49 P3: `tok = tok1.merge(tok2)` — композиция токенов. Возвращает
 * новый CancelToken который cancelled когда ЛЮБОЙ из источников cancelled.
 * Реализация: создать new token, bind его cascade'м с tok1 И tok2.
 * Любой из них при cancel() пробросит cancel на merged.
 *
 * Семантика first-cancel-wins для reason'а — тот источник кто отменился
 * первым, его reason оказывается в merged.reason() (cancel_reason
 * idempotent → second-cancel no-op).
 *
 * Превосходит индустрию:
 *   - Go: context.WithCancel(parent) cascade parent → child, но НЕТ
 *     general merge нескольких источников; нужно вручную select-loop.
 *   - TS: AbortSignal.any([...]) — TC39 stage 3, но reason: any.
 *   - Rust: tokio_util::sync::CancellationToken.child_token() — child
 *     cancelled когда parent cancelled, но опять no general merge of N.
 *
 * Same-type only в V1 (merged: CancelToken[T] где T = T1 = T2).
 * Cross-type merge — V2 (требует converter pair). */
static inline NovaCancelToken* nova_cancel_token_merge2(
    NovaCancelToken* a, NovaCancelToken* b)
{
    NovaCancelToken* merged = nova_cancel_token_new();
    if (a) nova_cancel_token_bind_cascade(merged, a);
    if (b) nova_cancel_token_bind_cascade(merged, b);
    return merged;
}

/* Plan 44.5 Layer 5 fix: common base prefix for all generated SpawnCtx structs.
 * Worker loop (runtime.c _worker_main) accesses these via NovaSpawnCtxBase* cast
 * from mco_get_user_data(co). Codegen guarantees these are the FIRST five fields
 * in every SpawnCtx (before user captures). nova_alloc zero-inits all fields:
 *   _nova_parent_scope = NULL    → preamble sets per path (M:N vs single-thread)
 *   _nova_worker_slot  = 0       → preamble overwrites with real slot on first run
 *   _nova_saved_fail_top = NULL  → fiber starts with clean fail-stack (correct)
 *   _nova_saved_interrupt_top = NULL → same
 *   _nova_fiber_scope = NULL     → preamble sets to home worker scope (set once)
 * Worker saves/restores these around each mco_resume, isolating each fiber's
 * fail-frame chain so cross-fiber longjmp (crash) cannot happen.
 *
 * Work-stealing correctness (Plan 44.5 Layer 5 deadlock fix):
 * A fiber's slot lives in its HOME worker scope (_nova_fiber_scope), set once
 * in preamble. If the fiber is stolen by another worker, the stealing worker
 * restores _nova_active_scope = _nova_fiber_scope so channel ops capture the
 * correct (home) scope/slot. Without this, the channel waiter records the
 * wrong scope, nova_sched_wake finds scope->fibers[slot]=NULL, dispatch_ready
 * is never called, and the fiber hangs permanently (deadlock). */
/* Plan 83.4.5.7 (2026-05-23): atomic fiber state machine для защиты от
 * concurrent mco_resume race (Windows TIB swap conflict / POSIX context
 * corruption) на armed multi-worker runtime.
 *
 * Race scenario до этого fix'а:
 *   1. Fiber F parked на channel.recv (parked[slot]=true).
 *   2. Sender A: nova_sched_wake(scope, slot) → parked[slot]=false →
 *      dispatch_ready(co) → push F в worker deque.
 *   3. Concurrent cancel: nova_scope_cancel_wake_all → reads parked[slot]
 *      stale-true → nova_sched_wake → dispatch_ready(co) → push F AGAIN.
 *   4. Worker pops F twice → mco_resume(F) on two iterations → TIB swap
 *      conflict / fiber arena slot 0 corruption → access violation.
 *
 * Fix: per-fiber atomic state. Wake — CAS PARKED→IDLE; только winner
 * вызывает dispatch_ready (push в deque). Worker — CAS IDLE→RUNNING
 * перед mco_resume; CAS-loser SKIP'ает resume.
 *
 * Cross-runtime reference:
 *   - Go runtime/proc.go::casgstatus — CAS на g.atomicstatus.
 *   - tokio task/state.rs::transition_to_running — bit-packed atomic
 *     с RUNNING/NOTIFIED/JOIN_INTEREST/COMPLETE bits.
 *   - Kotlin JobSupport.state_ — atomic CAS на job states. */
#define NOVA_FIBER_STATE_IDLE    0  /* suspended, NOT in any deque/wake-queue */
#define NOVA_FIBER_STATE_RUNNING 1  /* mco_resume in-progress on some thread */
#define NOVA_FIBER_STATE_PARKED  2  /* park called; waiting for wake */
#define NOVA_FIBER_STATE_DEAD    3  /* mco_status==DEAD; never resume */

/* ─── Plan 83-go-cmn Ф.2: gopark/goready 4-state park-latch ───────────
 *
 * Go runtime gopark/goready handshake. Lives in NovaSpawnCtxBase as
 * `_nova_park_state` (by-pointer via mco_get_user_data, zero-init = NIL).
 * Orthogonal to _nova_fiber_state (resume-ownership): this is the
 * wait/ready handshake — who gets to re-queue + the ready-before-park latch.
 *
 *   NIL=0        — not in a gopark transaction (resting; zero-init value).
 *   WAIT=1       — gopark committed; fiber about to / has yielded; awaiting goready.
 *   READY=2      — goready fired BEFORE gopark committed WAIT (the latch sentinel);
 *                  gopark's commit-recheck consumes it (READY->DISPATCHED) and
 *                  returns WITHOUT yielding (ready-before-park, no hang).
 *   DISPATCHED=3 — goready won WAIT->DISPATCHED: it (and only it) dispatch_ready's
 *                  the fiber. Also: the transient "alive, requeue-in-flight" state
 *                  that parked[]-reading liveness gates must NOT treat as 'gone'.
 *
 * Single-winner moves from parked[slot] CAS to _nova_park_state WAIT->DISPATCHED. */
#define NOVA_PARK_NIL        0
#define NOVA_PARK_WAIT       1
#define NOVA_PARK_READY      2
#define NOVA_PARK_DISPATCHED 3

typedef struct {
    NovaFiberQueue*      _nova_parent_scope;
    /* Plan 173.0 Ф.2 (A2.2): index into _nova_parent_scope's child_error[]/
     * child_ctx[] retention arrays (nova_scope_alloc_child_slot), set by
     * nova_runtime_spawn_into BEFORE the remote push. -1 = not assigned
     * (single-thread/bootstrap spawn via nova_fiber_spawn_into, or the
     * orphan detach path — neither participates in Ф.2 retention).
     * MUST be mirrored in BOTH codegen SpawnCtx_N layouts (emit_c.rs
     * emit_spawn + emit_detach), immediately after _nova_parent_scope —
     * same FATAL-if-forgotten discipline as every other NovaSpawnCtxBase
     * field (see schedlink/_nova_park_state notes below). */
    int                  _nova_parent_slot;
    int                  _nova_worker_slot;
    NovaFailFrame*       _nova_saved_fail_top;
    NovaInterruptFrame*  _nova_saved_interrupt_top;
    NovaFiberQueue*      _nova_fiber_scope;
    /* Plan 83.4.5.4 (2026-05-23): spawn-time TLS handler-snapshot capture
     * для inheritance в worker'е. Saved BEFORE nova_runtime_spawn_into на
     * parent-thread'е (TLS handlers видимы). Worker preamble adopts it в
     * fiber_effect_snapshot[slot]. NULL для single-thread spawn path
     * (nova_fiber_spawn_into сам save'ит). */
    NovaEffectSnapshot*  _nova_init_snapshot;
    /* Plan 83.4.5.7 (2026-05-23): atomic state machine. nova_alloc
     * zero-init → starts as NOVA_FIBER_STATE_IDLE (= 0). State machine
     * documented above. */
    nova_atomic_int      _nova_fiber_state;
    /* Plan 83.6 (2026-05-24): allocation size — used by free path для
     * routing ctx обратно в P-local SpawnCtx pool. 0 means не из pool
     * (legacy nova_alloc fallback path). Codegen sets to sizeof(SpawnCtx_N)
     * на acquire. nova_spawn_pool_release derives size class из value. */
    size_t               _nova_pool_size;
    /* Plan 110.2.1.a (D188 R3): cancel-shield mask depth counter.
     * Incremented by nv_consume_enter_shield (ConsumeScope entry),
     * decremented by nv_consume_leave_shield (scope exit). When > 0,
     * cooperative cancel-check points (nova_fiber_yield, suspend entry)
     * defer the cancel-throw until count returns to 0. Atomic int because
     * decrement may happen on a different worker thread than increment
     * after migration (work-stealing M:N). zero-init via nova_alloc. */
    nova_atomic_int      _nova_cancel_mask_count;
    /* Plan 173 Ф.5 п.2 (D192-ретракт; ранее Plan 110.2.2.a): watchdog
     * deadline_ns. Армится nv_cleanup_watchdog_arm ТОЛЬКО на время
     * cleanup-вызова (now_ns + threshold_ms*1e6). Suspend entries compare
     * uv_hrtime() vs this value; if exceeded while mask > 0 —
     * ONE-SHOT stderr-варн «fiber stuck in cleanup» (НЕ прерывание;
     * cleanup добегает). 0 = not armed. */
    int64_t              _nova_cancel_deadline_ns;
    /* Plan 83-go-cmn Ф.2: gopark/goready 4-state park-latch (NIL/WAIT/READY/
     * DISPATCHED). By-pointer via mco_get_user_data; zero-init = NOVA_PARK_NIL.
     * Inserted BEFORE schedlink so schedlink stays the LAST base field (Ф.1
     * invariant). MUST be mirrored in BOTH codegen SpawnCtx_N layouts
     * (emit_c.rs emit_spawn + emit_detach), before schedlink — same FATAL as
     * Ф.1a if forgotten. Accessed via nova_park_state_* helpers below. */
    nova_atomic_int      _nova_park_state;
    /* Plan 83-go-cmn Ф.1: intrusive overflow link. Used ONLY while this fiber
     * lives on the global overflow queue (NovaGlobalRunq) after a
     * nova_runq_put_slow spill; NULL otherwise. Accessed via nova_co_schedlink.
     * MUST be mirrored as the LAST base field in the codegen SpawnCtx_N layouts
     * (emit_c.rs emit_spawn + emit_detach) — else the overflow write lands on a
     * user-capture field. Zero-init by nova_alloc / pool memset. */
    mco_coro*            schedlink;
} NovaSpawnCtxBase;

/* Plan 83.4.5.7: helper — CAS fiber state. Returns true if CAS succeeded.
 * Safe для co без NovaSpawnCtxBase (legacy nova_fiber_run): base==NULL
 * → returns true (no atomic guard available, but single-shot fibers
 * doesn't have race window). */
static inline nova_bool nova_fiber_state_cas(mco_coro* co, int32_t from, int32_t to) {
    if (!co) return false;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return true;  /* legacy fiber без base — no guard */
    int32_t expected = from;
    return nova_aint_cas(&base->_nova_fiber_state, &expected, to);
}

static inline void nova_fiber_state_store(mco_coro* co, int32_t state) {
    if (!co) return;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return;
    nova_aint_store(&base->_nova_fiber_state, state);
}

static inline int32_t nova_fiber_state_load(mco_coro* co) {
    if (!co) return NOVA_FIBER_STATE_IDLE;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return NOVA_FIBER_STATE_IDLE;
    return nova_aint_load(&base->_nova_fiber_state);
}

/* ─── Plan 83-go-cmn Ф.2: by-co park_state accessors ─────────────────
 *
 * The gopark/goready latch is addressed BY co-pointer (mco_get_user_data),
 * never by (scope,slot) — so the slot-reuse lost-wake that rejected Option A
 * in Ф.1b structurally cannot occur on this path (no slot→ctx resolution).
 * NULL-safe for legacy fibers without a base (treated as a no-op latch:
 * load→NIL, store→noop, CAS→false). Such fibers never park via gopark. */
static inline nova_bool nova_park_state_cas(mco_coro* co, int32_t from, int32_t to,
                                            int success_mo, int failure_mo) {
    if (!co) return false;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return false;
    int32_t expected = from;
    return __atomic_compare_exchange_n(&base->_nova_park_state, &expected, to,
                                       false, success_mo, failure_mo);
}

static inline void nova_park_state_store(mco_coro* co, int32_t state, int mo) {
    if (!co) return;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return;
    __atomic_store_n(&base->_nova_park_state, state, mo);
}

static inline int32_t nova_park_state_load(mco_coro* co) {
    if (!co) return NOVA_PARK_NIL;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return NOVA_PARK_NIL;
    return __atomic_load_n(&base->_nova_park_state, __ATOMIC_ACQUIRE);
}

/* ===== Plan 110.2.1.a (D188 R3): cancel-shield primitives =====
 *
 * The mask depth counter lives in NovaSpawnCtxBase (per-fiber state,
 * survives mco_yield/resume + worker migration). Inc/dec on enter/leave
 * of `consume X = expr { body }` scope; cancel-receive sites consult
 * load() and defer the throw while > 0.
 *
 * NULL-safe для legacy fibers без base (treated as no-shield, mask==0). */

static inline void nova_cancel_mask_inc(mco_coro* co) {
    if (!co) return;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return;
    (void)nova_aint_inc(&base->_nova_cancel_mask_count);
}

static inline void nova_cancel_mask_dec(mco_coro* co) {
    if (!co) return;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return;
    (void)nova_aint_fetch_sub_release(&base->_nova_cancel_mask_count);
}

static inline int32_t nova_cancel_mask_load(mco_coro* co) {
    if (!co) return 0;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return 0;
    return nova_aint_load(&base->_nova_cancel_mask_count);
}

/* Convenience: query mask for currently-running fiber. */
static inline int32_t nova_cancel_mask_active(void) {
    return nova_cancel_mask_load(mco_running());
}

/* Plan 110.2.2.a: deadline accessors. _nova_cancel_deadline_ns не атомарен
 * (per-fiber single-writer: enter→leave; suspend сайт читает только когда
 * mask>0 значит этот же fiber выполняет тело consume{}). int64_t reads на
 * 64-битных платформах atomic by alignment. */
static inline void nova_cancel_deadline_set(mco_coro* co, int64_t ns) {
    if (!co) return;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return;
    base->_nova_cancel_deadline_ns = ns;
}

static inline int64_t nova_cancel_deadline_get(mco_coro* co) {
    if (!co) return 0;
    NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(co);
    if (!base) return 0;
    return base->_nova_cancel_deadline_ns;
}

/* Plan 110.2.1.a (D188 R3): ConsumeScope shield entry/exit.
 *
 * Plan 173 Ф.5 п.2 (D192-РЕТРАКТ, §3a 2026-06-26): `nv_consume_enter_shield`
 * теперь НЕ армит deadline на scope — только инкрементирует cancel-mask
 * (кооперативная отмена отложена, cleanup ВСЕГДА добегает; форс-прерывания
 * cleanup'а НЕ СУЩЕСТВУЕТ). Параметр `threshold_ms` сохранён в сигнатуре
 * как источник ПОРОГА watchdog-варна (3-level D192-resolution живёт в
 * codegen consume-prologue), но деадлайн армится ТОЛЬКО вокруг самого
 * cleanup-вызова — парой `nv_cleanup_watchdog_arm/disarm` ниже. Возврат
 * prev_deadline сохранён для symmetry restore на leave (D197 re-entrance:
 * consume внутри cleanup-тела наследует корректный outer-арм). */
static inline int64_t nv_consume_enter_shield(int threshold_ms) {
    (void)threshold_ms;  /* порог применяется в nv_cleanup_watchdog_arm */
    mco_coro* co = mco_running();
    if (!co) return 0;  /* main thread / non-fiber — shield is no-op */
    int64_t prev_deadline = nova_cancel_deadline_get(co);
    nova_cancel_mask_inc(co);
    return prev_deadline;
}

static inline void nv_consume_leave_shield(int64_t prev_deadline) {
    mco_coro* co = mco_running();
    if (!co) return;
    nova_cancel_mask_dec(co);
    /* Plan 110.x fix: restore outer's deadline (or 0 если outermost). */
    nova_cancel_deadline_set(co, prev_deadline);
}

/* Plan 173 Ф.5 п.2 (D192-ретракт): watchdog-порог АРМИТСЯ только на время
 * cleanup-вызова («fiber застрял в CLEANUP» — не в body). Превышение порога
 * на suspend-точке внутри cleanup'а → stderr-ВАРН (nv_shield_check_deadline),
 * НЕ прерывание: cleanup продолжает бежать до конца. threshold_ms == 0
 * (#realtime bypass D198) → не армим. Пара arm/disarm стекуется через
 * prev-значение (D197 re-entrance-safe). */
static inline int64_t nv_cleanup_watchdog_arm(int threshold_ms) {
    mco_coro* co = mco_running();
    if (!co) return 0;
    int64_t prev = nova_cancel_deadline_get(co);
    if (threshold_ms > 0) {
        nova_cancel_deadline_set(co,
            (int64_t)uv_hrtime() + (int64_t)threshold_ms * 1000000LL);
    } else {
        nova_cancel_deadline_set(co, 0);
    }
    return prev;
}

static inline void nv_cleanup_watchdog_disarm(int64_t prev) {
    mco_coro* co = mco_running();
    if (!co) return;
    nova_cancel_deadline_set(co, prev);
}

/* Plan 173 Ф.5 п.2 (D192-РЕТРАКТ, §3a): watchdog-check на suspend-точках
 * (Time.sleep, nova_fiber_yield, future Net I/O). Если watchdog армлен
 * (nv_cleanup_watchdog_arm — только на время cleanup-вызова) и порог
 * превышен — печатается stderr-ВАРН «fiber застрял в cleanup» и деадлайн
 * ЗАНУЛЯЕТСЯ (one-shot: варн не спамится на каждой последующей
 * suspend-точке). НИКАКОГО прерывания: cleanup продолжает бежать до конца
 * («defer всегда добегает», ни один из 7 языков-планки не форс-прерывает
 * cleanup). Прежний механизм — throw CleanupTimeoutError
 * (`_nova_throw_cleanup_timeout_fn` + string-fallback) — УДАЛЁН вместе с
 * самим типом (D192-ретракт); превышение порога наблюдаемо структурно в
 * ResourceTrace exit-событии (duration_ms/overrun — D185 amend).
 *
 * Idempotent: returns immediately on no-shield / no-deadline / not
 * exceeded. Safe to call at every suspend site без performance cost
 * на the hot non-shielded path. */
static inline void nv_shield_check_deadline(void) {
    mco_coro* co = mco_running();
    if (!co) return;
    if (nova_cancel_mask_load(co) == 0) return;  /* no shield active */
    int64_t deadline = nova_cancel_deadline_get(co);
    if (deadline == 0) return;  /* not armed / #realtime bypass (D198) */
    int64_t now = (int64_t)uv_hrtime();
    if (now <= deadline) return;  /* within budget */
    int over_ms = (int)((now - deadline) / 1000000LL);
    if (over_ms < 0) over_ms = 0;
    /* One-shot: disarm so the warn fires once per cleanup overrun. The
     * cancel-mask stays intact (cancel remains deferred until cleanup
     * completes); nv_cleanup_watchdog_disarm restores the outer state. */
    nova_cancel_deadline_set(co, 0);
    fflush(stdout);
    fprintf(stderr,
        "nova: warning: fiber stuck in cleanup: %d ms over watchdog threshold "
        "(cleanup keeps running; D192 retracted — no force-interrupt)\n",
        over_ms);
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
    nova_fiber_post_create(co);  /* Plan 82 Ф.1: patch ctx.stack_limit (Windows) */
    _nova_gc_add_fiber_roots(co);
    q->fibers[q->count]    = co;
    q->fiber_ctx[q->count] = user;            /* GC root: SpawnCtx reachable via managed array */
    q->fiber_fail_top[q->count] = NULL;       /* fresh fiber: empty fail-stack */
    q->fiber_interrupt_top[q->count] = NULL;  /* and empty interrupt-stack */
    /* Plan 201 trace-per-fiber: fresh per-fiber error-diag bucket (single-
     * thread/bootstrap path). Parent thread allocates it here; the fiber's
     * OWN first resume (nova_supervised_step, below) points the active TLS
     * pointer at it before mco_resume — this function itself never runs
     * the fiber, so pointing the TLS pointer here would be pointless (and
     * wrong: this runs on the SPAWNING thread/fiber, not the new one). */
    q->fiber_error_state[q->count] =
        (NovaFiberErrorState*)nova_alloc(sizeof(NovaFiberErrorState));
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
/* Plan 44.5 Layer 5 deferred-unlock: set by park_with_unlock before mco_yield;
 * called by scheduler (worker loop / supervised_step) after mco_resume returns. */
__declspec(thread) extern void (*_nova_park_unlock_fn)(void*);
__declspec(thread) extern void*           _nova_park_unlock_arg;
/* Plan 44.7: preemption pointer. Each worker thread sets this (in
 * _worker_main) to point at its own NovaWorker.preempt_flag. The sysmon
 * thread raises that flag on a timeslice overrun; codegen safepoints
 * (nova_preempt_check) dereference this ptr to read the LIVE flag and
 * cooperatively yield. NULL on the main thread / single-thread mode → the
 * safepoint is a pure no-op. A snapshot wouldn't work — the worker thread
 * is stuck inside mco_resume for the whole CPU-loop and can't re-copy. */
__declspec(thread) extern volatile int*   _nova_preempt_ptr;
#else
extern __thread NovaFiberQueue* _nova_active_scope;
extern __thread int             _nova_active_slot;
extern __thread void (*_nova_park_unlock_fn)(void*);
extern __thread void*           _nova_park_unlock_arg;
extern __thread volatile int*   _nova_preempt_ptr;
#endif

/* Plan 44.7: branch-hint macro — codegen emits NOVA_UNLIKELY(_nova_should_yield)
 * at every safepoint, so the not-taken path must stay cheap. */
#if defined(__GNUC__) || defined(__clang__)
#  define NOVA_UNLIKELY(x) __builtin_expect(!!(x), 0)
#else
#  define NOVA_UNLIKELY(x) (x)
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
/* Plan 173 Ф.3 п.1: primary-selection precedence rank среди retained ошибок
 * разных kind. Строгий порядок **PANIC > USER/USER_TYPED > CANCEL**:
 *   - PANIC (3)          — fiber-катастрофа; D13-инвариант: НЕ деградирует до
 *                          ловимого USER — panic ВСЕГДА становится primary,
 *                          даже если реальная throw-ошибка случилась раньше.
 *   - USER / USER_TYPED (2) — управляемая ошибка (реальная), приоритетнее
 *                          отмены (Go errgroup теряет её после cancel — мы нет).
 *   - CANCEL (1)         — кооперативная отмена siblings; самый низкий приоритет
 *                          (это следствие чужого падения, не корневая причина).
 * Правило overwrite: incoming становится primary ⇔ rank(incoming) > rank(current)
 * (строго больше → ties keep-first: first-PANIC-wins, first-USER-wins,
 * first-CANCEL-wins). Не-primary ошибки уходят в suppressed-карман (Ф.4). */
static inline int nova_throw_kind_precedence(NovaThrowKind kind) {
    switch (kind) {
        case NOVA_THROW_PANIC:      return 3;
        case NOVA_THROW_USER:       return 2;
        case NOVA_THROW_USER_TYPED: return 2;
        case NOVA_THROW_CANCEL:     return 1;
        default:                    return 0;
    }
}

/* Plan 49 Ф.2 → Plan 173 Ф.3 п.1: kinded report — precedence-таблица (rank выше):
 *   current=(none)  → write (любой kind)
 *   incoming rank > current rank → overwrite (PANIC бьёт USER/CANCEL; USER бьёт CANCEL)
 *   incoming rank ≤ current rank → keep (first-wins в пределах ранга)
 * Это даёт: (1) реальная ошибка surface'ится наружу даже если отмена случилась
 * раньше (Go errgroup первый-wins ТЕРЯЕТ её — у нас нет); (2) **panic ребёнка
 * ВСЕГДА становится primary** (D13 — не глотается уже-записанным USER'ом). */
static inline void nova_fiber_report_error_kinded(const char* msg,
                                                  NovaThrowKind kind,
                                                  void* reason_ptr) {
    if (!_nova_active_scope || _nova_active_slot < 0) return;
    _nova_active_scope->fiber_error[_nova_active_slot] = msg;
    NovaFiberQueue* q = _nova_active_scope;
    if (q->first_error == NULL) {
        q->first_error = msg;
        q->first_error_kind = kind;
        q->first_error_reason = reason_ptr;
    } else if (nova_throw_kind_precedence(kind) >
               nova_throw_kind_precedence(q->first_error_kind)) {
        /* incoming rank выше — overwrite primary. PANIC бьёт USER/CANCEL
         * (D13: panic не деградирует до ловимого USER); USER бьёт CANCEL
         * (реальная ошибка приоритетнее отмены). Ties → keep-first. */
        q->first_error = msg;
        q->first_error_kind = kind;
        q->first_error_reason = reason_ptr;
    }
    /* USER errors також сигналят cancel_requested (peer fibers пробудятся
     * и выйдут через CANCEL); CANCEL errors не сбрасывают чужие USER'ы. */
    nova_abool_store(&q->cancel_requested, true);
}

/* Backward-compatible wrapper для existing codegen — старый report_error
 * без kind/reason считает throw USER (текущая семантика). Когда codegen
 * перейдёт на kinded-emit, эту обёртку можно будет удалить. */
static inline void nova_fiber_report_error(const char* msg) {
    nova_fiber_report_error_kinded(msg, NOVA_THROW_USER, NULL);
}

/* Plan 49 Ф.5: M:N cross-worker kinded error report.
 * Worker fiber'е (parent_scope != NULL): CAS msg pointer + USER-precedence
 * для kind. Используется emit_spawn в remote-error-path (vs local
 * report_error_kinded). Reader main supervised_run видит kind/reason
 * через usual release/acquire на msg pointer.
 *
 * Algorithm:
 *   loop {
 *     exp = aptr_load(first_error_atomic);
 *     if (exp == NULL):
 *       CAS NULL → msg; success → store kind/reason → set cancel_requested → break
 *     else: // already set
 *       cur_kind = first_error_atomic_kind;
 *       if (cur_kind == CANCEL && incoming == USER):
 *         CAS prev_msg → msg; success → overwrite kind/reason → break
 *       else: keep (CANCEL keep на CANCEL incoming; USER keep на любое)
 *   }
 * NB: race: между load kind и CAS msg кто-то ещё может overwrite. Acceptable
 * (precedence — best-effort hint, не strict ordering): main reader получит
 * либо USER либо raison; CANCEL никогда не "тащит за собой" USER. */
/* Plan 83.10 (2026-05-25): extended signature — payload + tid для
 * typed throw routing. NULL payload + 0 tid OK для legacy USER/CANCEL
 * (string throws). Worker catch passes _ff.error_user_payload +
 * _ff.error_user_type_id. */
static inline void nova_fiber_report_atomic_kinded(NovaFiberQueue* parent,
                                                   const char* msg,
                                                   NovaThrowKind kind,
                                                   void* reason_ptr,
                                                   void* payload,
                                                   NovaTypeId tid) {
    if (!parent || !msg) return;
    for (;;) {
        const void* expected = nova_aptr_load(&parent->first_error_atomic);
        if (expected == NULL) {
            const void* exp_for_cas = NULL;
            if (nova_aptr_cas(&parent->first_error_atomic, &exp_for_cas,
                              (const void*)msg)) {
                parent->first_error_atomic_kind = kind;
                parent->first_error_atomic_reason = reason_ptr;
                parent->first_error_atomic_payload = payload;     /* Plan 83.10 */
                parent->first_error_atomic_tid = tid;             /* Plan 83.10 */
                nova_abool_store(&parent->cancel_requested, true);
                return;
            }
            /* CAS failed → loop: someone else wrote first, re-evaluate. */
            continue;
        }
        /* Already non-NULL: precedence check (Plan 173 Ф.3 п.1).
         * Overwrite ⇔ rank(incoming) > rank(current) — строгий порядок
         * PANIC > USER/USER_TYPED > CANCEL. Panic ребёнка бьёт уже-записанный
         * USER (D13 — не деградирует до ловимого); USER бьёт CANCEL. */
        NovaThrowKind cur_kind = parent->first_error_atomic_kind;
        if (nova_throw_kind_precedence(kind) >
            nova_throw_kind_precedence(cur_kind)) {
            const void* exp_for_cas = expected;
            if (nova_aptr_cas(&parent->first_error_atomic, &exp_for_cas,
                              (const void*)msg)) {
                parent->first_error_atomic_kind = kind;
                parent->first_error_atomic_reason = reason_ptr;
                parent->first_error_atomic_payload = payload;     /* Plan 83.10 */
                parent->first_error_atomic_tid = tid;             /* Plan 83.10 */
                /* cancel_requested already true; no change needed. */
                return;
            }
            continue;  /* expected changed под нами — retry. */
        }
        /* Keep existing (equal-or-lower rank → first-wins в пределах ранга). */
        return;
    }
}

/* Plan 173.0 Ф.2 (A2.3): per-child kinded error report for the M:N remote
 * path. Wraps nova_fiber_report_atomic_kinded (UNCHANGED — still the cheap
 * single-slot cancel-signal / first-error fast path the existing re-throw
 * tail reads) with a per-slot write into this child's OWN
 * parent->child_error[base->_nova_parent_slot] entry — no CAS, no
 * collapsing: each child owns a disjoint index (assigned once at spawn
 * time by nova_scope_alloc_child_slot, never shared), so concurrent
 * siblings writing their own slots never race each other.
 *
 * Ordering (R2 happens-before, §EXEC risk R2): called from the spawn
 * entry-function's catch block STRICTLY BEFORE the epilogue's
 * `nova_aint_fetch_sub_release(&pending_remote)` a few lines later in the
 * SAME thread's program order (emit_c.rs emit_spawn) — the identical
 * ordering discipline this file already documents for first_error_atomic
 * ("read через nova_aptr_load(acquire) в main thread после
 * pending_remote == 0 — корректный happens-before", NovaFiberQueue comment
 * above); child_error[] rides the same established guarantee. */
static inline void nova_fiber_report_child_kinded(NovaSpawnCtxBase* base,
                                                   const char* msg,
                                                   NovaThrowKind kind,
                                                   void* reason_ptr,
                                                   void* payload,
                                                   NovaTypeId tid) {
    if (!base || !base->_nova_parent_scope || !msg) return;
    NovaFiberQueue* parent = base->_nova_parent_scope;
    int slot = base->_nova_parent_slot;
    /* Plan 173.2 (supervision-as-effect): deferred-decision mode. When the
     * scope entered with an ambient Supervisor handler (has_supervisor
     * stamped by codegen at scope entry, before any spawn), a child failure
     * must NOT unilaterally elect itself primary nor broadcast cancellation
     * — that is the Supervisor's decision, taken serially on the scope's
     * drive thread (nova_supervised_process_decisions). Report goes ONLY to
     * this child's own retention slot; `published` release-store pairs with
     * the drive thread's acquire-load (per-slot happens-before for the
     * DURING-drain read).
     *
     * Fallback to the default path below when no slot is available (slot<0
     * can only mean the bootstrap/local path, which never calls this fn) —
     * an error must never be dropped silently. */
    if (parent->has_supervisor
        && slot >= 0 && parent->child_error && slot < parent->child_capacity) {
        parent->child_error[slot].msg     = msg;
        parent->child_error[slot].kind    = kind;
        parent->child_error[slot].reason  = reason_ptr;
        parent->child_error[slot].payload = payload;
        parent->child_error[slot].tid     = tid;
        nova_abool_store(&parent->child_error[slot].published, true);
        return;
    }
    /* Default path — byte-parity with pre-173.2 behaviour. */
    nova_fiber_report_atomic_kinded(parent, msg, kind, reason_ptr, payload, tid);
    if (slot >= 0 && parent->child_error && slot < parent->child_capacity) {
        parent->child_error[slot].msg     = msg;
        parent->child_error[slot].kind    = kind;
        parent->child_error[slot].reason  = reason_ptr;
        parent->child_error[slot].payload = payload;
        parent->child_error[slot].tid     = tid;
        nova_abool_store(&parent->child_error[slot].published, true);
    }
}

/* Plan 173.0 Ф.3 (A3.2/A3.3 — R1-guard): called by the worker loop right
 * after a remote child dies (MCO_DEAD), BEFORE routing its SpawnCtx buffer
 * back to the pool (runtime.c nova_spawn_pool_release call sites). If this
 * child recorded an error (child_error[slot].msg != NULL — written by
 * nova_fiber_report_child_kinded strictly before this point in the SAME
 * child fiber's execution: same thread, program order, no synchronization
 * needed), retain the ctx pointer in child_ctx[slot] instead of releasing
 * it — the Ф.3 decision-loop (nova_supervised_run_impl) needs it alive.
 *
 * R1 risk (§EXEC): if we released to the pool here, the buffer could be
 * handed to the NEXT nova_spawn_pool_acquire before the decision-loop reads
 * the retained ctx — the retained pointer would then alias a live,
 * differently-captured fiber's storage → silent corruption. This is the
 * ONLY point where that decision can be made (the ctx is about to be freed
 * or pooled either way, right here, right now).
 *
 * Returns true if retained (caller MUST skip nova_spawn_pool_release for
 * this ctx), false if the caller should release exactly as before (clean
 * completion, no parent scope, or no slot assigned — e.g. bootstrap/local
 * spawn, or the orphan detach path, neither of which sets a slot ≥ 0). */
static inline bool nova_scope_retain_or_release_child(NovaSpawnCtxBase* dead_ctx) {
    if (!dead_ctx || !dead_ctx->_nova_parent_scope || dead_ctx->_nova_parent_slot < 0) {
        return false;
    }
    NovaFiberQueue* parent = dead_ctx->_nova_parent_scope;
    int slot = dead_ctx->_nova_parent_slot;
    if (slot >= parent->child_capacity || !parent->child_error
        || parent->child_error[slot].msg == NULL) {
        return false;  /* clean completion (or slot never grown) — normal release. */
    }
    parent->child_ctx[slot] = (void*)dead_ctx;
    return true;
}

/* Plan 173.0 Ф.3 (A3.4): forward-decl — defined in runtime.c, declared in
 * runtime.h which (like the 83.10.3 decls just below) is included AFTER
 * fibers.h in nova_rt.h. Needed by nova_supervised_run_impl's decision-loop
 * tail to release retained child ctx buffers back to their pool. */
void nova_spawn_pool_release(void* ctx, size_t size);

/* [196.6 / D228 §6 class, 2026-07-13]: the ONE post-mortem sweep for a dead
 * remote child — retain-or-release the ctx, then RELEASE-decrement the parent
 * scope's pending_sweeps (see the field doc at NovaFiberQueue.pending_sweeps).
 * All worker-side dead-fiber sites MUST route through this helper (three in
 * runtime.c: _worker_main loop, worker cleanup drain, pump_scope) so the
 * scope owner's sweep-wait can pair with every sweep.
 *
 * Ordering contract:
 *  1. `parent` is SNAPSHOT before retain/release — a pool push overlays
 *     `_nova_parent_scope` with the freelist next pointer, so the field must
 *     not be re-read afterwards.
 *  2. The snapshot is safe to dereference until OUR decrement: the child's
 *     epilogue incremented pending_sweeps program-order-before its
 *     pending_remote release-decrement, so the owner cannot observe
 *     "all done" until this function's fetch_sub lands.
 *  3. fetch_sub is RELEASE — a retained `child_ctx[slot]` store must be
 *     visible to the owner's decision-loop (acquire wait) before it reads. */
static inline void nova_scope_sweep_dead_child(NovaSpawnCtxBase* dead_ctx) {
    if (!dead_ctx) return;
    /* [M-mn-spawnctx-corruption-cancel-wake] R1-трипваер: sweep по уже
     * освобождённому ctx = double-sweep (двойной pool-release + чтение
     * freelist-линка как _nova_parent_scope). Диаг-режим ловит до порчи. */
    {
        extern int  nova_spawn_pool_diag(void);
        extern void nova_spawn_ctx_diag_check_live(const void* vbase, const char* where);
        if (nova_spawn_pool_diag()) nova_spawn_ctx_diag_check_live(dead_ctx, "sweep-dead-child");
    }
    NovaFiberQueue* parent_snapshot = dead_ctx->_nova_parent_scope;
    if (!nova_scope_retain_or_release_child(dead_ctx)) {
        nova_spawn_pool_release(dead_ctx, dead_ctx->_nova_pool_size);
    }
    if (parent_snapshot) {
        (void)nova_aint_fetch_sub_release(&parent_snapshot->pending_sweeps);
    }
}

/* Plan 83.10.3 (2026-05-26): forward-decls — runtime.h included AFTER
 * fibers.h in nova_rt.h. Forward-declare to allow use in fibers.h functions.
 * Returns -1 on main thread, worker id (>=0) on worker thread. */
int nova_runtime_current_worker_id(void);

/* Plan 83.10.3 (2026-05-26): pump current worker's deque/runnext for a fiber
 * belonging to scope q. If found and IDLE, resumes it inline (handles nested
 * supervised case where worker can't return to its main pickup loop while in
 * supervised_run_impl). If nothing found, blocks on UV_RUN_ONCE (woken by
 * nova_runtime_signal_main broadcast or timer). Defined in runtime.c.
 * forward-declared here because runtime.h comes AFTER fibers.h. */
void nova_runtime_worker_pump_scope(struct NovaFiberQueue* scope);

/* Plan 83.10.3 (2026-05-26): helper — true when running on a worker thread.
 * Used in nova_supervised_run_impl to detect nested supervised case. */
static inline bool _nova_on_worker_thread(void) {
    return nova_runtime_current_worker_id() >= 0;
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
    /* Plan 201 trace-per-fiber: save outer active error-state pointer
     * (getter self-heals to the native per-thread bucket if never touched
     * on this thread). Restored after each fiber's resume below — no
     * "save fiber's current value back" step needed (the fiber's bucket
     * is fixed for its whole lifetime; mutations already land in it
     * in-place through the pointer, see effects.h NovaFiberErrorState). */
    NovaFiberErrorState* outer_error_state = nova_error_state_active();
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
        /* Plan 83.4.2 (2026-05-23) — A2 fix: under M:N, fiber spawned
         * через runtime_spawn_into попал в worker deque; codegen ставит
         * _nova_parent_scope = &queue (vs NULL для single-thread spawn).
         * Worker запустит mco_resume сам — main НЕ должен делать вторую
         * resume (двойной TIB-swap минiкоро corrupt'ит arena stack → access
         * violation в slot 0). Main скипает worker-owned fiber'ы; drain
         * exit-условие — pending_remote == 0 (worker decrement'ит). */
        if (q->fiber_ctx[i]) {
            NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)q->fiber_ctx[i];
            if (base->_nova_parent_scope) {
                alive++;  /* worker owns; count alive чтобы drain не exit'ил */
                continue;
            }
        }
        /* Plan 22 Ф.3/Ф.4 (D93): skip parked fiber'ы. Они resume'ятся
         * когда wake'нутся (callback timer'а либо cancel). Count alive++,
         * чтобы supervised_run не выходил оставив parked permanently.
         * Ф.7: bounds check на sched_st->capacity (может быть меньше
         * scope.count если sched_state alloc'нулся раньше grow'а).
         *
         * Plan 83-go-cmn Ф.2 (correction #4): parked[] is the cheap filter, but
         * the authority for "still waiting" is park_state==WAIT. A goready winner
         * clears parked[i] + sets fiber_state IDLE before dispatch; if we observe
         * the transient where parked[i] is still true but park_state==DISPATCHED,
         * the fiber is 'alive, requeue-in-flight' — count it alive but DO resume
         * it (fall through), since in bootstrap supervised_step IS the dispatcher.
         * Only park_state==WAIT means genuinely-parked → skip+count. */
        if (sched_st && i < sched_st->capacity && *nova_sched_parked_at(sched_st, i)
            && nova_park_state_load(co) == NOVA_PARK_WAIT) {
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
        /* Plan 201 trace-per-fiber: point the active error-state pointer at
         * THIS fiber's own bucket (allocated once at slot-creation —
         * nova_fiber_spawn_into). Falls back to outer if somehow NULL
         * (defensive; should not happen post slot-creation). */
        if (q->fiber_error_state[i]) {
            _nova_error_state_p = q->fiber_error_state[i];
        }
        /* Per-fiber handler scoping: install fiber's saved handler-snapshot
         * before resume. Каждый fiber видит свои `with X = h` биндинги,
         * не handlers других fibers. */
        if (q->fiber_effect_snapshot[i]) {
            nova_effect_snapshot_restore(q->fiber_effect_snapshot[i]);
        }
        /* Plan 83.4.5.7 (2026-05-23): supervised_step под bootstrap — single
         * thread, no concurrent mco_resume race. CAS guard НЕ нужен здесь.
         * Под armed M:N main thread СКИПАЕТ worker-owned fibers (A2 fix
         * выше: parent_scope != NULL → continue), так что mco_resume here
         * РЕДКАЯ ветка (только для non-worker-owned fibers — главным
         * образом single-thread fallback фaйберы).
         *
         * Still need state-store post-resume для PARKED transition viability
         * (wake's CAS PARKED→IDLE требует видимое PARKED state'а). */
        mco_result r = mco_resume(co);
        /* Plan 83.4.5.7: state restore. DEAD если mco terminated, иначе
         * IDLE (готов к next resume). RELEASE-store видим через ACQUIRE-load
         * на следующий wake/resume. */
        if (mco_status(co) == MCO_DEAD) {
            nova_fiber_state_store(co, NOVA_FIBER_STATE_DEAD);
        } else if (sched_st && i < sched_st->capacity && *nova_sched_parked_at(sched_st, i)
                   && nova_park_state_load(co) == NOVA_PARK_WAIT) {
            /* Fiber запарковался во время resume'а (park_state==WAIT). gopark уже
             * store'ил PARKED — НЕ overwrite'ить здесь (correction #4: only WAIT
             * means genuinely-parked; DISPATCHED/NIL = ready → fall to IDLE). */
        } else {
            nova_fiber_state_store(co, NOVA_FIBER_STATE_IDLE);
        }
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
        _nova_error_state_p = outer_error_state;  /* Plan 201 trace-per-fiber */
        /* Restore outer handlers (clean state для следующего fiber'а
         * или main-flow после step). */
        nova_effect_snapshot_restore(&outer_effects);
        /* Plan 44.5 Layer 5 deferred-unlock: call if fiber used park_with_unlock.
         * Single-thread: no race (no concurrent wakers), just maintain protocol. */
        if (_nova_park_unlock_fn) {
            void (*_pufn)(void*) = _nova_park_unlock_fn;
            void* _puarg = _nova_park_unlock_arg;
            _nova_park_unlock_fn  = NULL;
            _nova_park_unlock_arg = NULL;
            _pufn(_puarg);
        }
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
    /* [196.6 / D228 §6 class]: wait for worker-side sweeps of this scope's
     * remote children (see pending_sweeps field doc / supervised_run_impl
     * tail). The orphan scope is static, but drain is also the pre-exit
     * fence — keep the sweep/ctx-pool accounting symmetric. */
    while (nova_aint_load(&q->pending_sweeps) > 0) {
        uv_run(nova_current_loop(), UV_RUN_NOWAIT);
        if (nova_aint_load(&q->pending_sweeps) > 0) {
            uv_sleep(1);
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

/* ─── Plan 174 (D349): scope-deadline helpers ─── */

/* Forward-decl: monotonic-ns clock (defined below, after the scheduler). */
static inline int64_t time_monotonic_ns(void);

/* Combine two absolute-ns deadlines treating 0 as "no deadline". Result =
 * the earliest (tightest) non-zero point. An inner scope can only TIGHTEN an
 * inherited deadline, never extend it (план 173 §3a). */
static inline int64_t nova_deadline_combine(int64_t a, int64_t b) {
    if (a == 0) return b;
    if (b == 0) return a;
    return a < b ? a : b;
}

/* Deliver cooperative cancellation to a scope directly (no cancel-token). Same
 * wake-fan-out as the bound-scope branch of nova_cancel_token_cancel_reason:
 * mark the scope cancelled, propagate the reason, and wake every parked fiber
 * (SYNC/ASYNC slots, worker-parked fibers, driver-armed timers) so a blocked
 * `Time.sleep` / network park unblocks EARLY instead of running to full term.
 * Idempotent: first-cancel-wins is enforced by the caller checking
 * cancel_requested before calling. */
static inline void nova_scope_deliver_cancel(NovaFiberQueue* q, void* reason_ptr) {
    if (!q) return;
    nova_abool_store(&q->cancel_requested, true);
    q->cancel_reason_ptr = reason_ptr;
    nova_sched_cancel_all_pending(q);
    nova_scope_cancel_wake_all(q);
    {
        extern void nova_runtime_cancel_worker_fibers(struct NovaFiberQueue* scope);
        nova_runtime_cancel_worker_fibers(q);
    }
    _nova_cancel_via_driver(q);
}

/* Plan 174 (D349): typed `TimeoutError` throw hook. Assigned by codegen in
 * main() when the user program references `TimeoutError`. When NULL —
 * string-fallback throw is used (production-safe; outer fail-frame still
 * catches). Carries the exceeded deadline point (absolute monotonic ns).
 * NB: НЕ путать с ретрактнутым CleanupTimeoutError (D192-ретракт, Plan 173
 * Ф.5 п.2) — тот был про cleanup-бюджет ресурса и удалён; этот — про
 * scope-дедлайн supervised(deadline:/timeout:) и живёт. */
extern void (*_nova_throw_scope_timeout_fn)(int64_t deadline_ns);

static inline void nova_throw_scope_timeout(int64_t deadline_ns) {
    if (_nova_throw_scope_timeout_fn) {
        _nova_throw_scope_timeout_fn(deadline_ns);
        /* unreachable — fn does not return on the throw path */
    }
    /* Fallback: plain string throw (USER kind). Prefix is the recognized
     * marker; `with Fail` still catches and propagates. */
    nova_throw(nova_str_from_cstr("supervised-timeout: scope deadline exceeded"));
    /* unreachable */
}

/* Plan 174 (D349): no-op timer cb for the scope-deadline bounded wait. */
static void _nova_scope_deadline_wait_cb(uv_timer_t* h) { (void)h; }

/* UV_RUN_ONCE bounded by the scope deadline so the drain loop wakes at the
 * deadline even when every fiber is parked on a far-future timer (e.g.
 * `spawn { Time.sleep(10_000) }` under a 1s deadline). Mirrors the proven
 * armed-stack-timer + UV_RUN_ONCE pattern of the main-flow Time.sleep loop.
 * deadline_ns==0 → plain UV_RUN_ONCE (byte-identical to legacy behaviour). */
static inline void _nova_scope_deadline_run_once(int64_t deadline_ns) {
    uv_loop_t* loop = nova_current_loop();
    if (deadline_ns == 0) { uv_run(loop, UV_RUN_ONCE); return; }
    int64_t remaining_ns = deadline_ns - time_monotonic_ns();
    if (remaining_ns <= 0) {
        /* Deadline already passed — don't block; pump ready events so
         * cancellation close_cbs can complete and fibers drain. */
        uv_run(loop, UV_RUN_NOWAIT);
        return;
    }
    int64_t remaining_ms = remaining_ns / 1000000LL + 1;  /* round up, min 1 */
    uv_timer_t w;
    uv_timer_init(loop, &w);
    uv_timer_start(&w, _nova_scope_deadline_wait_cb, (uint64_t)remaining_ms, 0);
    uv_run(loop, UV_RUN_ONCE);
    uv_timer_stop(&w);
    uv_close((uv_handle_t*)&w, NULL);
    uv_run(loop, UV_RUN_NOWAIT);  /* release handle via NOWAIT pass */
}

/* Plan 173.0 Ф.3 (A3.5): internal decision hook — called once per retained
 * child failure by the serialized decision-loop in nova_supervised_run_impl
 * (below), BEFORE that child's SpawnCtx is freed. `idx`/`err`/`ctx` are all
 * still valid at the call (ctx alive precisely because Ф.3's R1-guard
 * withheld it from the SpawnCtx pool at death — see
 * nova_scope_retain_or_release_child above).
 *
 * THIS IS THE HOOK POINT FOR 173.2 (supervision-as-effect) — `on_child_fail`
 * dispatch replaces this body there. Plan 173.0 itself does not implement
 * any supervision policy (per §3/§EXEC: "hook-точка для 173.2 ... сам
 * эффект НЕ реализуешь"): the DEFAULT policy for 173.0 is a pure observer
 * that changes nothing — the actual escalate-all-or-throw decision (first
 * USER error re-thrown, CANCEL-only silently swallowed) remains driven
 * EXACTLY as before by q->first_error / q->first_error_atomic in this
 * function's own tail (unchanged control flow, unchanged byte-for-byte
 * externally observable behaviour — G-NEG parity gate). This function
 * existing + being called once per failure with a live ctx is the
 * infrastructure guarantee 173.0 promises; it does nothing else yet. */
static inline void nova_supervised_decide(NovaFiberQueue* scope, int idx,
                                           const NovaChildError* err, void* ctx) {
    (void)scope; (void)idx; (void)err; (void)ctx;
    /* Default policy (Plan 173.0): no-op observer. See comment above.
     * Plan 173.2: this observer remains the DEFAULT-mode hook (scope without
     * a Supervisor handler) — byte-parity guaranteed. Supervisor-mode scopes
     * take the nova_supervised_process_decisions path below instead. */
}

/* ─── Plan 173.2: supervision-as-effect — decision execution ───
 *
 * `Supervisor` is a real Nova effect (std/prelude/effects.nv):
 *     type Supervisor effect { on_child_fail(idx int, err any) -> Decision }
 *     type Decision enum Escalate | Stop
 * Strategies are handler VALUES (`with Supervisor = policy { ... }`), like
 * Time/Fail. The runtime cannot reference codegen-emitted symbols
 * (NovaVtable_Supervisor / Nova_Decision), so dispatch crosses through a
 * function pointer assigned by generated main() — the same pattern as
 * _nova_throw_scope_timeout_fn. The bridge (emit_c.rs,
 * _nova_supervisor_decide_impl):
 *   - reads the ambient `_nova_handler_Supervisor` TLS vtable (NULL →
 *     Escalate, the default);
 *   - boxes the NovaChildError into a Nova `any` (typed payload via
 *     nova_any_from_boxed, string throws/panics as `str`);
 *   - invokes the Nova handler under a local fail-frame — a handler that
 *     itself throws is treated as Escalate-with-handler-error (Q-block:
 *     `Fail` allowed, guarded against handler-fails-self recursion);
 *   - maps the returned Decision tag to the codes below. */
#define NOVA_SUPERVISE_ESCALATE       0   /* Decision.Escalate (and default) */
#define NOVA_SUPERVISE_STOP           1   /* Decision.Stop */
/* (code 2 retired: Restart family retracted from Decision — D416 amend
 * 2026-07-10; the dictionary is complete with Escalate|Stop.) */

/* Assigned by generated main() when the CU knows the Supervisor effect
 * (prelude present). NULL → every decision defaults to Escalate. Signature
 * erased to void* so runtime TUs (effects.c) can define the storage without
 * seeing NovaFiberQueue/NovaChildError. */
extern nova_int (*_nova_supervisor_decide_fn)(void* scope, nova_int idx,
                                              const void* err);

/* Serialized decision pass for supervisor-mode scopes (has_supervisor).
 * Runs ONLY on the scope's drive thread — from nova_supervised_run_impl's
 * drain loop (one call per iteration: failures are decided while siblings
 * are still running, so an Escalate can cancel them EARLY, and a Stop lets
 * them finish undisturbed) and once more after the drain completes (final
 * catch-up under the pending_remote==0 acquire gate).
 *
 * Per-slot protocol: acquire-load of `published` (pairs with the child's
 * release-store in nova_fiber_report_child_kinded) → plain fields readable;
 * `decided` is drive-thread-only. Each failure is fed to the handler EXACTLY
 * once, in slot order (deterministic within one pass).
 *
 * Decision execution:
 *   ESCALATE — feed the child's error into the scope's primary machinery
 *     (nova_fiber_report_atomic_kinded: precedence PANIC > USER > CANCEL +
 *     cancel_requested broadcast — exactly what the child itself does on
 *     the default path, so the re-throw tail needs NO changes).
 *   STOP — drop: no primary election, no cancellation; the slot stays
 *     retained (not lost silently — child_error[] keeps it).
 *   (Restart family retracted from Decision — D416 amend 2026-07-10;
 *    the bridge maps any non-Stop tag to ESCALATE defensively.)
 *
 * Induced sibling cancellations (kind CANCEL) are consumed silently: they
 * are the CONSEQUENCE of an escalation, not a root failure — the handler
 * sees only genuine child failures (USER/USER_TYPED/PANIC). */
static inline void nova_supervised_process_decisions(NovaFiberQueue* q) {
    if (!q->has_supervisor || !q->child_error || q->_deciding) return;
    q->_deciding = true;
    for (int i = 0; i < q->child_count; i++) {
        NovaChildError* ce = &q->child_error[i];
        if (ce->decided) continue;
        if (!nova_abool_load(&ce->published)) continue;
        ce->decided = true;
        if (ce->kind == NOVA_THROW_CANCEL) continue;
        nova_int code = _nova_supervisor_decide_fn
            ? _nova_supervisor_decide_fn((void*)q, (nova_int)i, (const void*)ce)
            : (nova_int)NOVA_SUPERVISE_ESCALATE;
        switch ((int)code) {
            case NOVA_SUPERVISE_STOP:
                break;
            case NOVA_SUPERVISE_ESCALATE:
            default:
                ce->escalated = true;  /* D414 §1: участвует в primary/suppressed */
                nova_fiber_report_atomic_kinded(q, ce->msg, ce->kind,
                                                ce->reason, ce->payload, ce->tid);
                break;
        }
    }
    q->_deciding = false;
}

/* Round-robin run: resume each live fiber until all are dead.
 * After all fibers complete, if any threw — re-throw on main-flow.
 *
 * Plan 22 Ф.4: когда все живые fiber'ы parked (никто не ready), idle —
 * uv_run UV_RUN_ONCE. Это блокирует main-thread в kernel-wait'е до
 * ближайшего libuv-события (наш timer's callback пробудит fiber). Так
 * scheduler не жжёт CPU busy-loop'ом.
 *
 * Plan 47: `tok` (nullable) — cancel-токен `supervised(cancel: tok)`.
 * Отвязывается ПЕРЕД любым re-throw/interrupt — scope (`q`) живёт на
 * стеке сгенерированного C-frame'а и становится невалидным после
 * longjmp'а, поэтому `bound_scope` нельзя оставлять висеть.
 */

/* Plan 201 (владелец: «механика, не инвариант-в-комментарии»): единая
 * scope re-throw точка, ЯВНО принимающая suppressed-цепочку параметром —
 * заменяет удалённый ambient TLS-слот `_nova_pending_suppressed`. Раньше
 * механизм полагался на недоказанный руками инвариант «между постановкой
 * слота и потреблением ближайшим throw нет точки планирования»; теперь
 * цепочка физически идёт по стеку вызова (аргумент), планирование между
 * "составить цепочку" и "бросить" структурно не может её потерять/подменить
 * — компилятору/ревьюеру нечего инспектировать вручную.
 *
 * Диспетчеризует на explicit-suppressed вариант throw-пути по `kind`
 * (единственный вызывающий — хвост `nova_supervised_run_impl` ниже, где
 * kind уже классифицирован: USER_TYPED cross-worker / PANIC / USER-иначе). */
static inline void nova_rethrow_scope(const char* err_cstr, NovaThrowKind kind,
                                       void* payload, NovaTypeId tid,
                                       NovaErrorChain* suppressed) {
    nova_str msg = nova_str_from_cstr(err_cstr);
    if (kind == NOVA_THROW_USER_TYPED) {
        nova_throw_typed_ex(msg, payload, tid, suppressed);
        return;  /* unreachable */
    }
    if (kind == NOVA_THROW_PANIC) {
        nv_panic_ex(msg, suppressed);
        return;  /* unreachable */
    }
    nova_throw_ex(msg, suppressed);
}

static inline void nova_supervised_run_impl(NovaFiberQueue* q,
                                            NovaCancelToken* tok) {
    /* Plan 83.11 Phase A diagnostics (Variant B, 2026-05-27): watchdog timer.
     * If supervised wait exceeds NOVA_WATCHDOG_DUMP_SECS (default 5s on main
     * thread, disabled on worker threads to avoid re-entrance noise), dump
     * runtime state once. Env-controlled: NOVA_WATCHDOG_DUMP_SECS=N (0=off). */
    uint64_t _watchdog_start = uv_hrtime();
    bool     _watchdog_fired = false;
    int      _watchdog_threshold_secs = 5;
    {
        const char* env = getenv("NOVA_WATCHDOG_DUMP_SECS");
        if (env && env[0]) {
            int v = atoi(env);
            if (v >= 0) _watchdog_threshold_secs = v;
        }
    }
    /* [M-187-sse-live-tls-server-hang] diagnostic-only, opt-in (2026-07-15):
     * the worker-thread nested-supervised path (this same fn, invoked from
     * `nova_runtime_worker_pump_scope`'s caller when a `supervised{}` block
     * runs INSIDE an already-spawned fiber) has NO dump path at all today —
     * `_nova_on_worker_thread()` unconditionally disables the watchdog there.
     * That leaves this class of hang (nested supervised deep inside a spawned
     * connection-handler fiber, aggregator flagship's own shape) completely
     * undiagnosable via the existing mechanism. Opt-in via a SEPARATE env var
     * (default off — zero behavior change unless explicitly requested) lets
     * worker-thread scopes dump too; `_watchdog_active_scope` stays a single
     * global (cosmetic race across concurrent workers, dump's per-worker/
     * per-slot fiber detail below is unaffected — that part is what actually
     * localizes the stuck fiber). Not wired into production defaults. */
    bool _watchdog_worker_opt_in = false;
    {
        const char* wenv = getenv("NOVA_WATCHDOG_WORKER");
        if (wenv && wenv[0] == '1') _watchdog_worker_opt_in = true;
    }
    bool _watchdog_enabled = (!_nova_on_worker_thread() || _watchdog_worker_opt_in)
                              && _watchdog_threshold_secs > 0;
    if (_watchdog_enabled) {
        extern void nova_runtime_set_watchdog_scope(struct NovaFiberQueue* q);
        nova_runtime_set_watchdog_scope((struct NovaFiberQueue*)q);
    }
    /* Plan 174 (D349): scope deadline — absolute monotonic ns (0 = none),
     * inherited+tightened by nova_scope_init/codegen. On expiry we deliver
     * cooperative cancel to this scope (waking parked fibers early) and latch
     * `_dl_fired` so the tail throws a typed TimeoutError. */
    int64_t _dl_ns    = q->deadline_ns;
    bool    _dl_fired = false;
    /* Plan 173.0 Ф.2 R2 tripwire: latch BEFORE the drain loop starts — every
     * remote child was spawned into `q` on this thread earlier in program
     * order (see NovaChildError comment). nova_scope_grow_children asserts
     * this stays false for its whole call, proving no grow-during-drain. */
    q->_drain_started = true;
    for (;;) {
        int alive = nova_supervised_step(q);
        /* Plan 173.2: supervisor-mode scopes decide retained failures WHILE
         * siblings still run — serialized, on this (drive) thread. A failing
         * child's epilogue calls nova_runtime_signal_main(), so the uv_run
         * waits below wake promptly and the decision latency is one loop
         * iteration. No-op (single flag test) for default scopes. */
        if (q->has_supervisor) nova_supervised_process_decisions(q);
        /* Plan 174 (D349): deadline gate. Fire once, and only if the scope
         * wasn't already cancelled by a bound token (earliest-of-the-two wins
         * — token cancel takes precedence, no bogus TimeoutError). */
        if (_dl_ns != 0 && !_dl_fired
            && !nova_abool_load(&q->cancel_requested)
            && time_monotonic_ns() >= _dl_ns) {
            _dl_fired = true;
            nova_scope_deliver_cancel(q, NULL);
        }
        if (alive == 0) {
            /* Plan 44.5 Layer 5: local done — но могут быть remote
             * fiber'ы running на workers. Wait для них. */
            int remote = (int)nova_aint_load(&q->pending_remote);
            if (remote == 0) break;
            /* Plan 83.11 Phase A + Plan 187 [M-187-watchdog-idle-server-kill]:
             * watchdog check. A supervised scope waiting on `pending_remote`
             * with local `count == 0` (this branch) is NOT by itself a hang
             * signature — it's exactly the shape of a healthy long-lived
             * server's accept-loop fiber, legitimately parked in `uv_accept`
             * (or any other suspending I/O op) on a worker thread, forever
             * (until a connection arrives). Only fire the (alarming, full
             * runtime-state) dump when `nova_runtime_has_stuck_fibers()`
             * confirms an actual lost-wake/orphaned slot (SUSPENDED but NOT
             * parked) — the same signature the dump's own per-slot detail
             * already flags as "STUCK_ALIVE_NOT_PARKED". If everything is
             * legitimately parked, re-arm (reset the elapsed-time clock)
             * instead of firing once-and-silent-forever — a scope that's
             * healthy now but turns genuinely stuck later still gets
             * diagnosed, just delayed by at most one more threshold window. */
            if (_watchdog_enabled && !_watchdog_fired) {
                uint64_t now = uv_hrtime();
                uint64_t elapsed_ns = now - _watchdog_start;
                if (elapsed_ns / 1000000000ULL >= (uint64_t)_watchdog_threshold_secs) {
                    extern bool nova_runtime_has_stuck_fibers(void);
                    if (nova_runtime_has_stuck_fibers()) {
                        _watchdog_fired = true;
                        extern void nova_runtime_dump_state(const char* reason);
                        char buf[64];
                        snprintf(buf, sizeof(buf),
                                 "supervised-watchdog-%ds-remote-%d",
                                 _watchdog_threshold_secs, remote);
                        nova_runtime_dump_state(buf);
                    } else {
                        _watchdog_start = now;  /* healthy idle-on-IO — re-arm */
                    }
                }
            }
            /* Plan 83.10.3 (2026-05-26): nested supervised on worker thread.
             * When supervised_run_impl runs on a worker (fiber body calls
             * supervised{spawn{...}}), the worker is blocked here and cannot
             * return to its _worker_main pickup loop to drain its own
             * runnext/deque. Fibers pushed into this worker's deque for scope
             * q will never run → hang (pending_remote stays > 0 forever).
             * Fix: cooperatively drain the worker's own deque/runnext for
             * fibers belonging to q. For fibers on other workers,
             * pump_scope polls with UV_RUN_NOWAIT + uv_sleep(1) (1ms); no
             * broadcast needed — outer loop re-checks pending_remote. */
            if (_nova_on_worker_thread()) {
                nova_runtime_worker_pump_scope((struct NovaFiberQueue*)q);
            } else {
                _nova_scope_deadline_run_once(_dl_ns);  /* Plan 174 (D349) */
            }
            continue;
        }
        /* alive > 0: либо есть ready fiber'ы, либо ВСЕ alive = parked.
         * Если ready=0 и parked>0 → idle в uv_run UV_RUN_ONCE. */
        int parked = nova_sched_count_parked(q);
        if (parked > 0 && parked == alive) {
            _nova_scope_deadline_run_once(_dl_ns);  /* Plan 174 (D349) */
        }
    }
    /* Plan 83.11 Phase A: clear watchdog scope before cleanup. */
    if (_watchdog_enabled) {
        extern void nova_runtime_set_watchdog_scope(struct NovaFiberQueue* qq);
        nova_runtime_set_watchdog_scope(NULL);
    }
    /* Plan 83.11 §12.31: wait for driver to finish processing any in-flight
     * CANCEL_SCOPE jobs that hold a pointer to this scope. NovaFiberQueue is
     * stack-allocated by codegen; if we return now while the driver still has
     * a job referencing q, the next stack frame reuses the memory and the
     * driver's deref reads garbage → SEGV in `_nova_driver_handle_cancel_scope`.
     * See §12.31 for VEH-localized crash analysis. ACQUIRE load synchronizes
     * with the driver's RELEASE decrement at end of handle_cancel_scope. */
    while (nova_aint_load(&q->pending_driver_jobs) > 0) {
        uv_run(nova_current_loop(), UV_RUN_NOWAIT);
        if (nova_aint_load(&q->pending_driver_jobs) > 0) {
            uv_sleep(1);  /* yield ~1ms; driver thread is independent of our loop */
        }
    }
    /* [196.6 / D228 §6 class]: same guarantee for the WORKER-side post-mortem
     * sweep of remote children (mco_destroy → retain_or_release_child →
     * pool_release). A child's epilogue decrements pending_remote INSIDE the
     * fiber; the sweep runs after the fiber returns and dereferences THIS
     * stack-allocated scope (child_capacity/child_error/child_ctx). Returning
     * before every sweep finished lets the next stack frame reuse the memory
     * → the sweep reads garbage / writes child_ctx into a live frame (Plan
     * 198 floating AV). Wait here — strictly BEFORE the decision-loop below
     * reads child_ctx[] (a retained store must be visible: release-dec in the
     * sweep pairs with this acquire load). Typical wait: zero iterations —
     * the sweep is the very next thing the worker does after the fiber
     * returns. See pending_sweeps field doc. */
    while (nova_aint_load(&q->pending_sweeps) > 0) {
        uv_run(nova_current_loop(), UV_RUN_NOWAIT);
        if (nova_aint_load(&q->pending_sweeps) > 0) {
            uv_sleep(1);
        }
    }
    /* Cleanup sched-state for этого scope'а (если был alloc'ом). */
    nova_sched_drop_state(q);
    /* Plan 173.0 Ф.3 (A3.4): serialized failure-decision loop. Runs exactly
     * here — pending_remote==0 already observed (loop above only exits with
     * remote==0), so every remote child that will ever report is done
     * reporting; nova_sched_drop_state just ran, so this is STRICTLY BEFORE
     * ctx_pins is freed below (§EXEC ordering: retention(step-death) →
     * loop/hook(here) → free ctx(ctx_pins block) → re-throw(tail)).
     * Iterates every retained failure ONCE, ctx alive at the call (Ф.3
     * R1-guard withheld it from the pool at death) — then releases each
     * retained ctx back to its pool now that the decision hook has run. */
    if (q->child_error) {
        if (q->has_supervisor) {
            /* Plan 173.2: final catch-up decision pass — pending_remote==0
             * was observed above (the loop only exits with remote==0), so
             * every report is visible; any failure whose publish landed
             * after the last in-loop pass is decided here, still strictly
             * before the retained ctx is released below. */
            nova_supervised_process_decisions(q);
        } else {
            for (int _nv_di = 0; _nv_di < q->child_count; _nv_di++) {
                if (q->child_error[_nv_di].msg != NULL) {
                    void* _nv_dctx = q->child_ctx ? q->child_ctx[_nv_di] : NULL;
                    nova_supervised_decide(q, _nv_di, &q->child_error[_nv_di], _nv_dctx);
                }
            }
        }
        for (int _nv_di = 0; _nv_di < q->child_count; _nv_di++) {
            if (q->child_ctx && q->child_ctx[_nv_di]) {
                NovaSpawnCtxBase* _nv_rctx = (NovaSpawnCtxBase*)q->child_ctx[_nv_di];
                nova_spawn_pool_release(_nv_rctx, _nv_rctx->_nova_pool_size);
                q->child_ctx[_nv_di] = NULL;
            }
        }
    }
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
    /* Plan 47: unbind токен ПЕРЕД любым longjmp'ом (re-throw / interrupt).
     * После unbind'а `tok->bound_scope == NULL` → последующий `tok.cancel()`
     * / повторный bind безопасны; `tok` (caller-owned, GC) переживает
     * unwind. На normal-пути (нет err/pending) unbind тоже здесь. */
    if (tok) nova_cancel_token_unbind(tok);
    /* Plan 83.11 §11.6 V2 [M-83.11-ctx-pins-scope-cleanup] (2026-06-08):
     * free uncollectable ctx_pins[] array on scope exit. Без этого array
     * (~128B-8KB tail) leaks per supervised scope until process exit.
     * Token остаётся reachable через caller's stack ref (Boehm scans stack
     * roots) даже после free — array нужен был только для cross-worker
     * pointer-chain reachability (Plan 83.11 §11.6), который заканчивается
     * на scope exit. SpawnCtx entries also already cleaned up via their
     * own nova_spawn_pool_release lifecycle. Runs before ALL exit paths
     * (re-throw at line ~1958, interrupt at ~1911, CANCEL return at ~1937,
     * normal fall-through). */
    if (q->ctx_pins) {
        nova_free_uncollectable(q->ctx_pins);
        q->ctx_pins       = NULL;
        q->ctx_pins_count = 0;
        q->ctx_pins_cap   = 0;
    }
    /* Plan 174 (D349): restore the enclosing active scope on EVERY exit path
     * (normal + interrupt + CANCEL-return + re-throw + TimeoutError longjmp).
     * Codegen's post-run `_nova_active_scope = prev` covers only the normal
     * fall-through; the longjmp paths below skip it, which would leave the TLS
     * dangling at this (freed) stack scope and corrupt the next scope's
     * inherited deadline. Idempotent with the codegen restore on the normal
     * path (same value). */
    _nova_active_scope = q->saved_active_scope;
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
    /* Plan 49 Ф.3 + Ф.5: kind-aware re-throw.
     * CANCEL → scope отменён штатно, наружу НИЧЕГО не летит (отмена сделала
     *          работу). Это паритет с Go: `ctx` отменён → функция просто
     *          возвращается.
     * USER  → реальная ошибка fiber'а. Re-throw на main flow; внешний
     *          `with Fail` handler пользователя поймает её.
     * USER-precedence (Ф.2) гарантирует что если БЫЛИ и CANCEL и USER —
     * naружу всплывёт USER (реальная ошибка не теряется).
     *
     * Plan 49 Ф.5: kind для cross-worker (M:N) ошибок читается из
     * first_error_atomic_kind. Приоритет: local first_error побеждает над
     * atomic (если оба есть — local зафиксировался первым в этом thread'е).
     * Если только atomic — берём atomic_kind. */
    /* Plan 174 (D349): the scope deadline fired. Surface it as a typed
     * `TimeoutError` — UNLESS a real USER error propagated, in which case
     * USER-precedence applies (the genuine error wins; the deadline merely
     * cancelled the siblings). CANCEL-only or error-free scope → TimeoutError.
     * Runs after tok-unbind + ctx_pins free above, so the longjmp leaves no
     * dangling scope state (same discipline as the USER re-throw below). */
    if (_dl_fired) {
        NovaThrowKind _dk = q->first_error ? q->first_error_kind
                                           : q->first_error_atomic_kind;
        if (!err || _dk == NOVA_THROW_CANCEL) {
            nova_throw_scope_timeout(_dl_ns);
            /* unreachable */
        }
    }
    if (err) {
        NovaThrowKind kind = q->first_error ? q->first_error_kind
                                            : q->first_error_atomic_kind;
        if (kind == NOVA_THROW_CANCEL) {
            /* Отмена не убегает наружу. Caller продолжает выполнение. */
            return;
        }
        /* ── Plan 173 хвост (D414 §1 ← Ф.4), рефактор Plan 201 ──
         * Спека обещает: «Не-primary ошибки уходят в suppressed-карман».
         * Здесь (единственная точка, где primary покидает scope) собираем
         * ВСЕ прочие retained детские падения в локальную цепочку
         * `_nv_suppressed` и передаём её ЯВНЫМ параметром в
         * `nova_rethrow_scope` ниже — никакого ambient TLS-relay (был
         * `_nova_pending_suppressed`, удалён вместе с held-by-comment
         * инвариантом «нет точки планирования между постановкой и
         * потреблением»; цепочка теперь физически в аргументе вызова,
         * планированию тут нечего портить).
         * Исключаются: CANCEL-производные (следствие эскалации, не корень);
         * Stop-решённые супервизором (хендлер осознанно выкинул — D416;
         * retained в child_error[] для observability, наружу не текут);
         * сам primary. Идентификация primary: msg-указатель НЕДОСТАТОЧЕН —
         * typed-броски делят один литерал msg_repr («<nova_int>» и т.п.),
         * поэтому для atomic-primary дополнительно сверяем payload/tid/kind
         * (боксы per-throw — уникальны). Для str-бросков payload=NULL у
         * всех — совпадение всех полей = неотличимый дубликат, его всё
         * равно схлопнул бы identity-check nv_compose_suppressed (D193).
         * Порядок: prepend-compose (LIFO) + back-to-front чтение accessor'а
         * (`suppressed()` материализует цепочку с хвоста) → обход слотов по
         * ВОЗРАСТАНИЮ даёт видимый порядок = порядок слотов (spawn-порядок,
         * детерминированно). */
        NovaErrorChain* _nv_suppressed = NULL;
        {
            NovaFailFrame _nv_aggf;
            _nv_aggf.error_suppressed = NULL;
            if (q->child_error) {
                nova_bool _nv_prim_local = (q->first_error != NULL);
                for (int _nv_ai = 0; _nv_ai < q->child_count; _nv_ai++) {
                    NovaChildError* _nv_ce = &q->child_error[_nv_ai];
                    if (_nv_ce->msg == NULL) continue;
                    if (_nv_ce->kind == NOVA_THROW_CANCEL) continue;
                    if (q->has_supervisor && !_nv_ce->escalated) continue;
                    if (_nv_ce->msg == err
                        && (_nv_prim_local
                            || (_nv_ce->payload == q->first_error_atomic_payload
                                && _nv_ce->tid  == q->first_error_atomic_tid
                                && _nv_ce->kind == q->first_error_atomic_kind))) {
                        continue;  /* primary сам */
                    }
                    nv_compose_suppressed(&_nv_aggf,
                                          nova_str_from_cstr(_nv_ce->msg),
                                          _nv_ce->kind,
                                          _nv_ce->payload,
                                          _nv_ce->tid);
                }
            }
            _nv_suppressed = _nv_aggf.error_suppressed;
        }
        /* Plan 83.10 (2026-05-25): fix [M-83.10-armed-user-throw-routing].
         * USER_TYPED re-throw must preserve payload + tid для typed handler
         * dispatch. Без этого `with Fail[int]` handler не fires на main thread
         * — main's nova_throw(str) bypasses dispatch chain.
         *
         * Local path: payload/tid stored в q->first_error_user_payload (TBD V2)
         * либо нужны fields на local NovaFiberQueue. V1: typed throw на main
         * thread go через _ff.error_user_payload TLS; atomic path для worker
         * fiber typed throw routes here.
         *
         * Atomic path: read payload + tid от atomic fields. */
        if (kind == NOVA_THROW_USER_TYPED && !q->first_error) {
            /* Cross-worker typed throw. */
            void* payload = q->first_error_atomic_payload;
            NovaTypeId tid = q->first_error_atomic_tid;
            nova_rethrow_scope(err, NOVA_THROW_USER_TYPED, payload, tid, _nv_suppressed);
            /* unreachable */
        }
        /* Plan 173 Ф.6 (§4а, вскрыто panics-миграцией; D13/D414): PANIC
         * ребёнка НЕ деградирует до ловимого USER при re-throw наружу из
         * supervised — транспортируем nv_panic'ом (kind=PANIC сохранён:
         * with-Fail не ловит, panics-клаузула/D13-класс различают). Тот же
         * класс дефекта, что Ф.1 #1 (with-Fail глотал panic) — прежний
         * plain nova_throw терял kind на этом сайте. */
        {
            NovaThrowKind _rk = q->first_error ? q->first_error_kind
                                               : q->first_error_atomic_kind;
            if (_rk == NOVA_THROW_PANIC) {
                nova_rethrow_scope(err, NOVA_THROW_PANIC, NULL, NOVA_TID_NONE, _nv_suppressed);
                /* unreachable */
            }
        }
        /* USER либо USER_TYPED (local — see note above): plain throw. */
        nova_rethrow_scope(err, NOVA_THROW_USER, NULL, NOVA_TID_NONE, _nv_suppressed);
    }
}

/* `supervised { body }` — без cancel-токена. */
static inline void nova_supervised_run(NovaFiberQueue* q) {
    nova_supervised_run_impl(q, NULL);
}

/* `supervised(cancel: tok) { body }` — с cancel-токеном (Plan 47).
 * Токен отвязывается внутри _impl перед нормальным возвратом И перед
 * любым re-throw. */
static inline void nova_supervised_run_cancel(NovaFiberQueue* q,
                                              NovaCancelToken* tok) {
    nova_supervised_run_impl(q, tok);
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
    if (!co) {
        /* Plan 83.4.3 (2026-05-23) — B4 fix: yield на main thread.
         * Раньше — silent no-op. Теперь — один turn libuv loop'а
         * (UV_RUN_NOWAIT) даёт прогресс pending I/O / async-events /
         * scheduler-wake'ам. Это паритет с Node `setImmediate(cb)` /
         * Go `runtime.Gosched()` semantics on the main goroutine.
         * Безопасно: uv_run libuv поддерживает re-entrancy (drain-цикл
         * supervised_run сам может вызвать yield → не блокируется). */
        uv_loop_t* loop = nova_evloop();
        if (loop) uv_run(loop, UV_RUN_NOWAIT);
        return;
    }
    /* Cooperative cancellation check. _nova_active_scope set by step.
     * Plan 49 Ф.2: бросаем kind=CANCEL (вместо USER) и тащим причину
     * из bound token'а scope'а (если есть). Это позволяет supervised_run
     * (Ф.3) различать отмену от реальной ошибки и не пробрасывать наружу.
     *
     * [M-cancel-loop-accept-swallowed-residual] fix (221.1 Ф.2 #15,
     * 2026-07-23): under armed M:N, the WORKER's own resume preamble
     * repoints the TLS `_nova_active_scope` to the worker's bookkeeping
     * scope (`&w->scope`, see `nova_runtime_cancel_worker_fibers`'s doc
     * comment) for the ENTIRE life of a worker-run fiber — that scope is
     * never the one a user-level `supervised(timeout:/deadline:)` block
     * cancels (`nova_scope_deliver_cancel` sets `cancel_requested` on the
     * SUPERVISED scope, a genuinely different `NovaFiberQueue`). A fiber
     * that never parks — e.g. a `while` retry-loop whose every op returns
     * an immediate `Err` post-cancel, never actually blocking again (net.c
     * `accept`/`read`/`write` all short-circuit once `stage>=CLOSING`) —
     * has no registered stop_cb for `nova_runtime_cancel_worker_fibers` to
     * dispatch either, so it never observed its OWN logical scope's
     * cancellation here: confirmed by instrumentation — `nova_preempt_check`
     * WAS firing `nova_fiber_yield` every ~10ms slice (sysmon works fine),
     * but this check here never saw `cancel_requested` true because it was
     * reading the wrong scope, so the loop spun until the outer test
     * timeout, not the 300ms `supervised(timeout:)` deadline. Only
     * genuinely-parked ops (net.c reads/writes/accepts, `Time.sleep`) were
     * ever cancelled correctly — through the SEPARATE stop_cb fan-out in
     * `nova_scope_deliver_cancel`/`nova_runtime_cancel_worker_fibers`, which
     * never touches `_nova_active_scope` at all.
     *
     * Fix: fall back to the fiber's OWN ctx (`NovaSpawnCtxBase.
     * _nova_parent_scope`, set exactly ONCE at spawn time to the REAL
     * user-level scope the fiber was spawned into — see `emit_spawn`/
     * `emit_detach` codegen — and never repointed afterward, unlike
     * `_nova_active_scope`) when `_nova_active_scope` itself isn't flagged.
     * `_nova_active_scope` is checked FIRST and unchanged for every case it
     * already covered correctly (sequential/main-thread execution, where it
     * directly IS the logical scope; a fiber's OWN nested `supervised{}`
     * block, which codegen retargets `_nova_active_scope` to for its
     * duration) — this only ADDS coverage for the armed-M:N gap above.
     *
     * Plan 110.2.1.a (D188 R3): if a ConsumeScope shield is active
     * (cancel_mask_count > 0), defer the cancel-throw — the fiber is
     * currently running cleanup code that must complete (subject to
     * the exit_timeout enforced separately by suspend-entry checks).
     * Yield cooperatively без throw — cancel remains latched on scope. */
    NovaFiberQueue* _nv_cancel_scope = _nova_active_scope;
    bool _nv_cancel_hit = _nv_cancel_scope && nova_abool_load(&_nv_cancel_scope->cancel_requested);
    if (!_nv_cancel_hit) {
        NovaSpawnCtxBase* _nv_yield_base = (NovaSpawnCtxBase*)mco_get_user_data(co);
        if (_nv_yield_base && _nv_yield_base->_nova_parent_scope
            && _nv_yield_base->_nova_parent_scope != _nv_cancel_scope
            && nova_abool_load(&_nv_yield_base->_nova_parent_scope->cancel_requested)) {
            _nv_cancel_scope = _nv_yield_base->_nova_parent_scope;
            _nv_cancel_hit = true;
        }
    }
    if (_nv_cancel_hit) {
        if (nova_cancel_mask_load(co) == 0) {
            void* reason = _nv_cancel_scope->cancel_reason_ptr;
            nova_throw_cancel_reason(
                nova_str_from_cstr("scope cancelled"),
                reason);
        }
        /* shielded: cancel deferred; fall through to mco_yield. */
    }
    /* Plan 110.2.2.a (D188 R3 + D192): deadline check at cooperative
     * suspend entry. When shield active and deadline exceeded, throws
     * cleanup-timeout marker — outer ConsumeScope fail-frame catches. */
    nv_shield_check_deadline();
    mco_yield(co);
}

/* Plan 175 (owner TODO closure, 2026-07-10): `vclock.park_until(deadline_ms)`
 * — extern "nova" hook backing `std/testing/handlers.nv` `mut_clock`'s
 * auto-idle-advance (see the big NovaVClockEntry comment block near
 * `nova_scope_init` above for the full design/rationale).
 *
 * Sequential (no fiber, e.g. a plain top-level test body) — no concurrent
 * siblings possible to coordinate with, resolves IMMEDIATELY: byte-for-byte
 * the pre-existing synchronous mock behaviour for the overwhelmingly common
 * single-flow case (D92: `_nova_active_scope` is always non-NULL in user
 * code, but `mco_running()` is NULL outside a fiber — that's the gate).
 *
 * Inside a fiber: registers this fiber's absolute virtual deadline in the
 * active scope's registry, then cooperatively yields (plain
 * `nova_fiber_yield()`, NOT `nova_sched_park` — see design comment) until
 * either (a) it observes its own entry fired, or (b) — on every resume —
 * it notices every currently-alive fiber of the scope is ALSO registered
 * here (idle) and fires the globally-earliest entry itself (which may be
 * a sibling's, not its own). Returns once its OWN entry fires. */
static inline nova_unit nova_vclock_park_until(nova_int deadline_ms) {
    if (!mco_running() || !_nova_active_scope) {
        return NOVA_UNIT;
    }
    NovaFiberQueue* scope = _nova_active_scope;
    mco_coro* self = mco_running();
    int idx = nova_vclock_register(scope, self, (int64_t)deadline_ms);
    for (;;) {
        if (nova_vclock_check_and_consume(scope, idx)) {
            return NOVA_UNIT;
        }
        /* Idle detection: every currently-alive fiber of this scope is
         * registered here (nobody has real work left) → advance to the
         * earliest pending deadline (may wake a sibling, not us).
         * `nova_vclock_alive_count` (NOT bare `nova_sched_count_alive`) —
         * see its comment for why: default ARMED M:N spawns are invisible
         * to `nova_sched_count_alive` alone. */
        if (nova_vclock_pending_count(scope) >= nova_vclock_alive_count(scope)) {
            nova_vclock_fire_earliest(scope);
        }
        nova_fiber_yield();
    }
}

/* Plan 44.7: preemption safepoint. Codegen emits a call to this at every
 * function prologue and every loop backedge. Cost on the hot (not-preempt)
 * path: one TLS load + a predicted-not-taken branch, and — only if the ptr
 * is non-NULL — one more load (~1-2 cycles total). When the sysmon thread
 * has flagged this worker as overrunning its timeslice, *_nova_preempt_ptr
 * is 1 → clear it and cooperatively yield so peer fibers get CPU.
 *
 * Safe outside a fiber (main thread, single-thread mode): _nova_preempt_ptr
 * is NULL there → pure no-op. `nova_fiber_yield()` itself also no-ops if
 * `mco_running()` is NULL — double safety.
 *
 * [M-211-preempt-flag-plain-race] (2026-07-17): the pointee (sysmon's
 * producer thread vs this consumer thread) is TSan-confirmed racy as a
 * plain access — see NovaWorker.preempt_flag field comment in runtime.c.
 * `__atomic_*(RELAXED)` here compiles to the exact same load/store
 * instruction as the old plain deref on x86/ARM (relaxed needs no fence) —
 * this is a correctness/TSan-cleanliness fix only, NOT a hot-path cost
 * change. */
static inline void nova_preempt_check(void) {
    if (NOVA_UNLIKELY(_nova_preempt_ptr != NULL) &&
        __atomic_load_n(_nova_preempt_ptr, __ATOMIC_RELAXED)) {
        __atomic_store_n(_nova_preempt_ptr, 0, __ATOMIC_RELAXED);
        nova_fiber_yield();
    }
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

/* Plan 65 Ф.12.2 / D124: monotonic clock в наносекундах для типа Monotonic.
 *
 * Same underlying source as _nova_monotonic_ms (uv_hrtime).
 *
 * Windows: QueryPerformanceCounter normalised к ns через
 *   QueryPerformanceFrequency (libuv handles 32→64-bit overflow
 *   guard internally — см. uv__hrtime_win32 в libuv/src/win/util.c).
 * macOS: mach_absolute_time + mach_timebase_info.
 * Linux: clock_gettime(CLOCK_MONOTONIC).
 *
 * Returns int64_t (Nova-side Monotonic.nanos field is i64). Overflow при
 * процесс-uptime > ~292 years — пренебрежимо. */
static inline int64_t time_monotonic_ns(void) {
    return (int64_t)uv_hrtime();
}

/* [M-time-default-handler-not-wallclock] / D316 amend (2026-07-06):
 * настоящий wall-clock unix epoch в миллисекундах для default (боевого,
 * без `with Time = handler {...}`) обработчика Time.now_unix_ms().
 *
 * До фикса default-путь возвращал _nova_monotonic_ms() (uptime процесса,
 * epoch реализация-зависим — НЕ unix epoch), что ломало любой боевой код,
 * читающий Timestamp.now() как настоящее календарное время (логи, TTL,
 * сравнение с внешними timestamp'ами).
 *
 * uv_gettimeofday(uv_timeval64_t*) — libuv wall-clock (gettimeofday на
 * POSIX, аналог на Windows), tv_sec — секунды с unix epoch (int64_t),
 * tv_usec — микросекунды (int32_t). Возвращает 0 при успехе; libuv-реализация
 * этого вызова не имеет документированных failure-путей на
 * поддерживаемых платформах — при (теоретическом) сбое возвращаем 0
 * вместо undefined tv, а не abort (Time.now_unix_ms() ambient — не должен
 * валить процесс). */
static inline int64_t time_wall_unix_ms(void) {
    uv_timeval64_t tv;
    if (uv_gettimeofday(&tv) != 0) {
        return 0;
    }
    return (int64_t)tv.tv_sec * 1000 + (int64_t)tv.tv_usec / 1000;
}

/* Plan 175.1 (D316 amend + D321, 2026-07-10): system-local UTC offset in
 * seconds — closes [M-175.1-local-offset-effect-op]. Owner decision:
 * the machine's configured timezone MUST be reachable from Nova.
 *
 * This is the offset a fresh `Timestamp.now()` would observe RIGHT NOW
 * (DST already folded in where applicable) — not a fixed standard-time
 * offset. It is ONLY a numeric offset: no implicit `TimeZone`/`Fixed`
 * substitution is introduced anywhere — civil-time (`std/time/civil`)
 * still requires an EXPLICIT zone everywhere (D319 R1 unchanged);
 * `Offset.local()` (std/time/civil/offset.nv) is the explicit Nova-side
 * query wrapping this hook (java.time `ZoneId.systemDefault()` /
 * Temporal `Now.timeZoneId()` class of operation — explicit, not an
 * ambient default). */
#if defined(_WIN32)
#  ifndef WIN32_LEAN_AND_MEAN
#    define WIN32_LEAN_AND_MEAN
#  endif
#  include <windows.h>
static inline int64_t time_local_offset_sec(void) {
    TIME_ZONE_INFORMATION tzi;
    DWORD rc = GetTimeZoneInformation(&tzi);
    /* `Bias`/`*Bias` are MINUTES to ADD to local time to get UTC
     * (UTC == local + Bias) => offset-from-UTC == -(Bias [+ DST bias]). */
    LONG bias = tzi.Bias;
    if (rc == TIME_ZONE_ID_DAYLIGHT) {
        bias += tzi.DaylightBias;
    } else if (rc == TIME_ZONE_ID_STANDARD) {
        bias += tzi.StandardBias;
    }
    /* TIME_ZONE_ID_UNKNOWN (no DST rule for this zone) — raw Bias only. */
    return (int64_t)(-bias) * 60;
}
#else
#  include <time.h>
static inline int64_t time_local_offset_sec(void) {
    time_t now = time(NULL);
    struct tm local_tm;
    localtime_r(&now, &local_tm);
    /* `tm_gmtoff` (BSD/glibc/macOS-libc extension, present on every
     * Nova-supported POSIX target) — seconds EAST of UTC, DST already
     * folded in by localtime_r. */
    return (int64_t)local_tm.tm_gmtoff;
}
#endif

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
    /* Legacy stages (bootstrap path _nova_sleep_via_libuv) */
    NOVA_SLEEP_PENDING = 0,   /* timer armed, fiber parked */
    NOVA_SLEEP_CLOSING = 1,   /* uv_close issued, awaiting close_cb */
    NOVA_SLEEP_CLOSED  = 2,   /* close_cb fired — safe to wake fiber */

    /* Plan 83.11 Ф.3 driver-path stages — single-mutator (driver thread).
     * Port Tokio TimerEntry pattern (tokio/src/runtime/time/entry.rs). */
    NOVA_SLEEP_DRV_NEW         = 10, /* allocated, not yet on driver loop */
    NOVA_SLEEP_DRV_ARMED       = 11, /* uv_timer started, in scope.armed_list */
    NOVA_SLEEP_DRV_FIRING      = 12, /* timer_cb won CAS, uv_close in flight */
    NOVA_SLEEP_DRV_CANCEL_REQ  = 13, /* cancel-job won CAS, uv_close in flight */
    NOVA_SLEEP_DRV_CLOSED      = 14, /* close_cb fired — wake fiber */
} NovaSleepStage;

/* Plan 83.11 Ф.3.A v3 (Option A): wait_state moved into nova_sched_state.
 * pending_wake[] counter integrated в generic park/wake API. Sleep no longer
 * needs sleep-specific state machine — generic park_until handles race. */

typedef struct NovaSleepState {
    NovaFiberQueue*  scope;
    int              slot;
    uv_timer_t       timer;
    /* Plan 83.4.1 (2026-05-23): atomic stage — read с ACQUIRE из
     * park-predicate, write с RELEASE из timer_cb/close_cb. Защищает
     * от инверсии visibility между worker, owning loop'а и worker'ом,
     * resumeющим fiber после wake. На x86 — no extra cost; на ARM —
     * корректные fence-ы.
     *
     * Plan 83.11 Ф.3: same atomic also used for driver-path stages
     * (NOVA_SLEEP_DRV_*). Single-mutator (driver thread) — CAS only
     * for race ARMED→FIRING vs ARMED→CANCEL_REQ. Worker ACQUIRE-loads. */
    nova_atomic_int  stage;   /* NovaSleepStage values */

    /* Plan 83.11 Ф.3: driver-path specific fields. */
    int                       home_worker_id;  /* worker that armed; wake target */
    NovaFiberQueue*           cancel_scope;    /* supervised scope (для armed_list) */
    struct NovaSleepState*    next_in_scope;   /* singly-linked, driver-only */
    struct NovaSleepState**   pprev_in_scope;  /* O(1) unlink — pointer to predecessor's next */
    /* Plan 83.11 Phase B2 diagnostic: fiber pointer captured at ARM_SLEEP time.
     * close_cb compares scope->fibers[slot] with this to detect slot reuse
     * or scope/slot mismatch (wrong fiber woken). */
    mco_coro*                 expected_co;
} NovaSleepState;

/* Forward-decl close_cb для использования в timer_cb / stop_cb. */
static void _nova_sleep_close_cb(uv_handle_t* h);

/* Timer fired: инициировать close. НЕ wake'аем fiber — wake придёт из
 * close_cb когда handle полностью released. */
static void _nova_sleep_timer_cb(uv_timer_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    /* Plan 83.4.1: CAS PENDING → CLOSING; защита от race со stop_cb. */
    int32_t expected = NOVA_SLEEP_PENDING;
    if (!nova_aint_cas(&st->stage, &expected, NOVA_SLEEP_CLOSING)) {
        return;  /* stop_cb уже инициировал close */
    }
    uv_close((uv_handle_t*)h, _nova_sleep_close_cb);
}

/* Close completed — handle fully released. Wake parked fiber. */
static void _nova_sleep_close_cb(uv_handle_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    /* Plan 83.4.1: RELEASE-store предиката — park-predicate в
     * _sleep_stage_is_closed читает с ACQUIRE и видит этот write. */
    nova_aint_store(&st->stage, NOVA_SLEEP_CLOSED);
    nova_sched_wake(st->scope, st->slot);
}

/* Plan 83.4.1 park-predicate: park-until возвращается ТОЛЬКО когда
 * close_cb отработал и stage == NOVA_SLEEP_CLOSED. ACQUIRE-load
 * парный с RELEASE-store в close_cb. */
static nova_bool _nova_sleep_stage_is_closed(void* ctx) {
    NovaSleepState* st = (NovaSleepState*)ctx;
    return nova_aint_load(&st->stage) == NOVA_SLEEP_CLOSED;
}

/* stop_cb для cancel-integration (D93 Ф.8 ASYNC contract).
 * Идемпотентен — handle может уже быть closing'ом из timer_cb path.
 * Возвращает NOVA_STOP_ASYNC — cancel_all_pending НЕ unpark'нет нас,
 * wake придёт из close_cb. */
static NovaStopMode _nova_sleep_stop_cb(void* handle) {
    uv_timer_t* timer = (uv_timer_t*)handle;
    NovaSleepState* st = (NovaSleepState*)timer->data;
    /* Plan 83.4.1: CAS PENDING → CLOSING; защита от race с timer_cb. */
    int32_t expected = NOVA_SLEEP_PENDING;
    if (nova_aint_cas(&st->stage, &expected, NOVA_SLEEP_CLOSING)) {
        /* Plan 83.10.2 (2026-05-26): cross-thread safe dispatch.
         * timer->loop may not be the current thread's loop under armed M:N
         * (timer was created on the worker's loop, but cancel fires on main).
         * uv_timer_stop + uv_close from a foreign thread are libuv UB — they
         * silently corrupt the handle list or miss the close_cb entirely,
         * leaving the fiber parked forever → TIMEOUT.
         *
         * Fix: defer close to timer->loop's thread via async dispatch.
         * uv_close implies uv_timer_stop — explicit stop is not needed. */
        nova_loop_defer_close(timer->loop, (uv_handle_t*)timer, _nova_sleep_close_cb);
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
    /* FIX 83.10.2 (Race 2a/2b): Get the parent (supervised) scope whose
     * cancel_requested is set by tok.cancel(). `scope` is the WORKER scope
     * in M:N mode; cancel_requested on the worker scope is NEVER set.
     * The supervised scope is accessible via the fiber's SpawnCtxBase. */
    NovaFiberQueue* cancel_scope = scope;  /* fallback: single-thread mode */
    {
        mco_coro* _rc = mco_running();
        if (_rc) {
            NovaSpawnCtxBase* _base = (NovaSpawnCtxBase*)mco_get_user_data(_rc);
            if (_base && _base->_nova_parent_scope) {
                cancel_scope = (NovaFiberQueue*)_base->_nova_parent_scope;
            }
        }
    }
    /* FIX 83.10.2 (Race 2a): Early-exit — parent scope already cancelled
     * BEFORE we start the timer. [M-178-server-graceful-deadline] amend
     * (Plan 173 Ф.3, 2026-07-12): the original fix just returned here
     * ("fiber will complete normally") — that is precisely the leak this
     * amendment closes: returning success lets the fiber run its remaining
     * body instead of unwinding via cancel-throw, same class of bug as the
     * post-park gap fixed below. Throw here too (shield-aware, matching
     * nova_fiber_yield), so a fiber that calls `Time.sleep` AFTER its scope
     * was already cancelled unwinds immediately instead of skipping the
     * sleep silently. */
    if (nova_abool_load(&cancel_scope->cancel_requested)) {
        if (nova_cancel_mask_load(mco_running()) == 0) {
            nova_throw_cancel_reason(
                nova_str_from_cstr("scope cancelled"),
                cancel_scope->cancel_reason_ptr);
            /* unreachable */
        }
        return;
    }
    NovaSleepState st = { .scope = scope, .slot = slot };
    nova_aint_init(&st.stage, NOVA_SLEEP_PENDING);
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
    /* FIX 83.10.2 (Race 2b): Post-register cancel check.
     *
     * Window: cancel fired BETWEEN early-exit (2a) and register_pending.
     * cancel_worker_fibers saw stop_cb=NULL → did nothing. Without this
     * fix, the fiber would park and sleep the full ms uncancelled.
     *
     * Fix: re-check cancel_requested on the PARENT scope AFTER registering.
     * If true, self-initiate close via CAS (PENDING→CLOSING). We are on the
     * worker's loop thread (fiber runs inside uv_run worker step), so direct
     * uv_close is safe here. The CAS is idempotent: if cancel_worker_fibers's
     * stop_cb already fired first, we lose the CAS and do nothing — stop_cb
     * already initiated close; close_cb will wake us anyway. */
    if (nova_abool_load(&cancel_scope->cancel_requested)) {
        int32_t expected = NOVA_SLEEP_PENDING;
        if (nova_aint_cas(&st.stage, &expected, NOVA_SLEEP_CLOSING)) {
            /* We won the CAS — stop the timer now; close_cb wakes fiber. */
            uv_close((uv_handle_t*)&st.timer, _nova_sleep_close_cb);
        }
        /* else: stop_cb already won CAS; close_cb will wake us. */
    }
    /* Plan 83.4.1: park-until — возвращается только когда stage==CLOSED.
     * Под M:N drain-quiescence-wake мог разбудить park до завершения
     * close_cb; park_until re-park'нется и подождёт реального close_cb.
     * Никакого FATAL-check'а больше не нужно — by construction. */
    nova_sched_park_until(scope, slot, _nova_sleep_stage_is_closed, &st);
    nova_sched_unregister_pending(scope, slot);

    /* [M-178-server-graceful-deadline] fix (Plan 173 Ф.3, 2026-07-12): see the
     * identical comment in _nova_sleep_via_driver above — CLOSED wakes on
     * BOTH natural timer expiry and a cooperative-cancel early-close, and
     * this legacy (non-driver) path silently treated both the same,
     * dropping the cancel-throw a spawned child needs to actually unwind
     * instead of running its remaining body to completion. */
    if (nova_abool_load(&cancel_scope->cancel_requested)) {
        if (nova_cancel_mask_load(mco_running()) == 0) {
            nova_throw_cancel_reason(
                nova_str_from_cstr("scope cancelled"),
                cancel_scope->cancel_reason_ptr);
            /* unreachable */
        }
    }
}

/* ─── Plan 83.3 Ф.1: `Blocking`-эффект → libuv threadpool offload ───
 *
 * Genuinely-blocking работа (FFI в блокирующие C-библиотеки, syscall'ы
 * вне uv_fs) выполненная инлайн на worker'е пинит весь worker — теряется
 * один `P` (Plan 83 §3 П1/П2). Решение: увести работу в libuv threadpool
 * (uv_queue_work), запарковать fiber, освободить worker.
 *
 * Переиспользует park/wake D93 (тот же путь, что Time.sleep). Отличие от
 * sleep: uv_work_t — это REQUEST, не handle → не нужен uv_close-dance.
 * После after_work_cb libuv с request'ом закончил.
 *
 * Lifecycle:
 *   START → uv_queue_work → register_pending → park
 *     (work_cb на threadpool-потоке делает блокирующую работу)
 *     → after_work_cb на loop'е worker'а-владельца: done=true, wake
 *     → fiber резюмится, sanity-check done, unregister + return
 *   cancel:
 *     cancel_all_pending → stop_cb: uv_cancel (отменяет ТОЛЬКО
 *       не-стартовавшую работу), return ASYNC
 *     → after_work_cb всё равно отработает (status=UV_ECANCELED либо 0)
 *       → wake; fiber видит cancel_requested → throw
 *
 * V1-контракт (D50, Plan 83.3 Ф.2): `fn` — LEAF: FFI/syscall без
 * GC-аллокации и без вызовов обратно в Nova-рантайм (work_cb идёт на
 * потоке, не зарегистрированном в Boehm и не являющемся fiber'ом). */

/* Tagged for the same reason as `struct NovaFiberQueue` above (driver.h
 * forward-declares `struct NovaBlockingState;`; gcc 14+ treats that and an
 * anonymous-struct typedef as distinct types). */
typedef struct NovaBlockingState {
    NovaFiberQueue*  scope;
    int              slot;
    uv_work_t        work;
    void           (*fn)(void*);  /* leaf-работа (V1) */
    void*            arg;
    /* Plan 83.4.1: atomic done — RELEASE-store в after_work_cb (workpool
     * thread/owner loop), ACQUIRE-load в park-predicate (worker resume'я
     * fiber'а). На x86 — no extra cost; на ARM — корректные fences. */
    nova_atomic_bool done;
} NovaBlockingState;

/* Выполняется на потоке libuv threadpool. НЕ Boehm-registered, НЕ fiber.
 * V1: `fn` обязан быть leaf (см. контракт выше). */
static void _nova_blocking_work_cb(uv_work_t* req) {
    NovaBlockingState* st = (NovaBlockingState*)req->data;
    st->fn(st->arg);
}

/* Выполняется обратно на loop'е submitting worker'а (libuv threadpool
 * процесс-глобален, after_work_cb приходит на тот loop, что submit'ил).
 * Будит запаркованный fiber. `status` == UV_ECANCELED если работа была
 * отменена до старта — fiber всё равно будится (сам проверит cancel). */
static void _nova_blocking_after_cb(uv_work_t* req, int status) {
    (void)status;
    NovaBlockingState* st = (NovaBlockingState*)req->data;
    nova_abool_store(&st->done, true);  /* Plan 83.4.1: RELEASE */
    nova_sched_wake(st->scope, st->slot);
}

/* Plan 83.4.1 park-predicate для park-until — возвращается ТОЛЬКО когда
 * after_work_cb отработал и опубликовал done=true. ACQUIRE-load парный
 * с RELEASE-store в after_work_cb. */
static nova_bool _nova_blocking_is_done(void* ctx) {
    NovaBlockingState* st = (NovaBlockingState*)ctx;
    return nova_abool_load(&st->done);
}

/* stop_cb для cancel-integration (D93 ASYNC contract). uv_cancel
 * отменяет работу ТОЛЬКО если она ещё не подхвачена threadpool-потоком;
 * in-flight C-вызов непрозрачен и доводится до конца — industry-standard
 * (Go не прерывает блокирующий cgo, tokio не отменяет running
 * spawn_blocking). В обоих случаях after_work_cb отработает → wake. */
static NovaStopMode _nova_blocking_stop_cb(void* handle) {
    uv_work_t* req = (uv_work_t*)handle;
    uv_cancel((uv_req_t*)req);  /* best-effort; result игнорируем */
    return NOVA_STOP_ASYNC;
}

/* Fiber-context blocking offload. Уводит leaf-блокирующую `fn` на libuv
 * threadpool, паркует fiber, освобождает worker до завершения работы.
 * PRECONDITION: вызывается из fiber-контекста (scope/slot валидны).
 *
 * Plan 83.11 Ф.4: routes via driver UV loop when driver is started.
 * Driver receives ARM_BLOCKING job, calls uv_queue_work on its own loop.
 * after_work_cb fires on driver thread → done=true + nova_sched_wake.
 *
 * Wake-before-park race covered by park_until fast-path predicate check:
 * if done=true before park_until is reached, returns immediately. */
static inline void nova_blocking_offload(NovaFiberQueue* scope, int slot,
                                          void (*fn)(void*), void* arg) {
    NovaBlockingState st = { .scope = scope, .slot = slot, .fn = fn, .arg = arg };
    nova_abool_init(&st.done, false);
    st.work.data = &st;

    /* Plan 83.11 Ф.4 fix: pre-init SchedState BEFORE submitting the driver job.
     * Under Ф.4 the after_work_cb (and thus nova_sched_wake) runs on the DRIVER
     * thread, so it can fire between job-submission and register_pending — while
     * scope->sched_state is still NULL. nova_sched_wake would then call
     * nova_sched_find_state → NULL → silently drop BOTH the pending_wake delivery
     * AND the parked CAS, risking a lost wakeup. Allocating the state here
     * guarantees a cross-thread wake always lands. Same rationale and contract as
     * _nova_sleep_via_driver, which pre-inits via nova_sched_get_state for the
     * identical Ф.3 race. Harmless for the legacy worker-loop branch. */
    nova_sched_get_state(scope);

    if (nova_driver_is_started()) {
        /* Plan 83.11 Ф.4: route via centralized driver UV loop. */
        NovaDriverJob* job = (NovaDriverJob*)malloc(sizeof(NovaDriverJob));
        if (!job) {
            fprintf(stderr, "nova: FATAL nova_blocking_offload: malloc job failed\n");
            abort();
        }
        job->kind = NOVA_DRV_JOB_ARM_BLOCKING;
        job->u.arm_blocking.st   = &st;
        job->u.arm_blocking.work = fn;
        job->u.arm_blocking.arg  = arg;
        if (nova_driver_submit_job(job) != 0) {
            free(job);
            fprintf(stderr, "nova: FATAL nova_blocking_offload: submit_job failed\n");
            abort();
        }
    } else {
        /* Legacy path: worker's UV loop (bootstrap / pre-driver). */
        int rc = uv_queue_work(nova_current_loop(), &st.work,
                               _nova_blocking_work_cb, _nova_blocking_after_cb);
        if (rc != 0) {
            fprintf(stderr, "nova: FATAL uv_queue_work failed: %s\n",
                    uv_strerror(rc));
            abort();
        }
    }

    /* Register для cancel-wake (D93). uv_cancel is thread-safe for work
     * requests; works for both driver-loop and worker-loop paths. */
    nova_sched_register_pending(scope, slot, &st.work, _nova_blocking_stop_cb);
    /* Plan 83.4.1: park-until — возвращается только когда after_work_cb
     * установил done=true. Fast-path predicate check handles wake-before-park
     * race (if done=true already, returns immediately without yielding). */
    nova_sched_park_until(scope, slot, _nova_blocking_is_done, &st);
    nova_sched_unregister_pending(scope, slot);
}

/* Plan 83.11 Ф.3: predicate для park_until — Acquire-load packed_state.
 * Returns true when driver's close_cb wrote NOVA_SLEEP_DRV_CLOSED. */
static nova_bool _nova_sleep_drv_state_is_closed(void* ctx) {
    NovaSleepState* st = (NovaSleepState*)ctx;
    return nova_aint_load(&st->stage) == NOVA_SLEEP_DRV_CLOSED;
}

/* Plan 83.11 Ф.3: driver-path sleep — closes [M-83.10.4-iso-cancel-startup-race].
 *
 * Architecture: worker submits ARM_SLEEP job to driver thread. Driver creates
 * uv_timer_t on its own loop, links into scope.armed_sleeps_head list, transitions
 * state NEW→ARMED. On timer_cb (natural fire) or cancel_scope job (tok.cancel()),
 * driver CAS ARMED→{FIRING,CANCEL_REQ}, uv_close. close_cb stores CLOSED + wakes
 * worker fiber via existing dispatch_ready cross-thread path.
 *
 * Single-mutator (driver thread) для ALL state transitions eliminates the three-
 * load race that Plan 83.10.5 Tactical couldn't fix. Cross-thread visibility
 * trivial: worker ACQUIRE-loads packed_state at park_until predicate.
 *
 * Port Tokio TimerEntry pattern (tokio/src/runtime/time/entry.rs). */
static inline void _nova_sleep_via_driver(NovaFiberQueue* scope, int slot,
                                          nova_int ms) {
    /* Derive cancel_scope (parent supervised) — same logic as _nova_sleep_via_libuv. */
    NovaFiberQueue* cancel_scope = scope;
    {
        mco_coro* _rc = mco_running();
        if (_rc) {
            NovaSpawnCtxBase* _base = (NovaSpawnCtxBase*)mco_get_user_data(_rc);
            if (_base && _base->_nova_parent_scope) {
                cancel_scope = (NovaFiberQueue*)_base->_nova_parent_scope;
            }
        }
    }

    /* Race 2a early-exit — still useful as cheap fast-path. Если cancel уже
     * fired, even submitting ARM_SLEEP job is wasted work.
     * [M-178-server-graceful-deadline] amend (Plan 173 Ф.3, 2026-07-12):
     * throw here too, shield-aware — same reasoning as the identical
     * amendment in _nova_sleep_via_libuv above. */
    if (nova_abool_load(&cancel_scope->cancel_requested)) {
        if (nova_cancel_mask_load(mco_running()) == 0) {
            nova_throw_cancel_reason(
                nova_str_from_cstr("scope cancelled"),
                cancel_scope->cancel_reason_ptr);
            /* unreachable */
        }
        return;
    }

    /* NovaSleepState на coroutine stack — fiber parked while driver dereferences,
     * stack stays alive до park_until exits with CLOSED state. */
    NovaSleepState st;
    memset(&st, 0, sizeof(st));
    st.scope = scope;
    st.slot = slot;
    st.cancel_scope = cancel_scope;
    /* home_worker_id captured here — wake target. Cross-thread atomic read OK.
     * Через публичный аксессор, а не raw `extern __declspec(thread)`: тот был
     * (а) непортируем — `__declspec` не включён на Linux clang без -fdeclspec,
     * ронял компиляцию всего рантайма (fibers.h:2356); (б) extern к `static`
     * TLS из runtime.c — линковочный хазард. nova_runtime_current_worker_id()
     * читает ту же TLS внутри своего TU. */
    st.home_worker_id = nova_runtime_current_worker_id();
    nova_aint_init(&st.stage, NOVA_SLEEP_DRV_NEW);

    /* Plan 83.11 Phase A fix: pre-initialize SchedState BEFORE ARM_SLEEP submission.
     *
     * nova_sched_wake (called from close_cb on the driver thread) uses
     * nova_sched_find_state — a pure pointer-deref that returns NULL if the
     * state has not yet been created. If the timer fires (or is cancelled via
     * CANCEL_SCOPE) BEFORE this fiber first calls nova_sched_park (which lazily
     * creates the state via nova_sched_get_state), nova_sched_wake silently
     * drops both the pending_wake delivery AND the parked CAS. The fiber then
     * parks with no pending_wake recorded → nobody wakes it → permanent hang.
     *
     * Fix: ensure the state (including pending_wake[]) is allocated now, while
     * we are still on the fiber thread and before the ARM_SLEEP job is queued.
     * After this call nova_sched_find_state will always return non-NULL for this
     * scope, and close_cb → nova_sched_wake can safely set pending_wake[slot]. */
    nova_sched_get_state(scope);

    /* Plan 83.11 Phase B2: capture fiber pointer for close_cb mismatch detection. */
    st.expected_co = mco_running();

    /* Submit ARM_SLEEP job to driver. malloc + driver frees после processing. */
    NovaDriverJob* job = (NovaDriverJob*)malloc(sizeof(NovaDriverJob));
    if (!job) {
        fprintf(stderr, "nova: _nova_sleep_via_driver: malloc job failed\n");
        return;  /* sleep skipped — fiber wakes immediately */
    }
    job->kind = NOVA_DRV_JOB_ARM_SLEEP;
    job->u.arm_sleep.st = &st;
    job->u.arm_sleep.ms = (uint64_t)ms;
    if (nova_driver_submit_job(job) != 0) {
        /* Driver not started or shutting down — degrade gracefully. */
        free(job);
        return;
    }

    /* Plan 83.11 Phase A fix: CANCEL_SCOPE vs ARM_SLEEP ordering race.
     *
     * Race: fiber checks cancel_requested=false (fast-path above), then
     * tok.cancel() fires — sets cancel_requested=true, submits CANCEL_SCOPE
     * to driver. If CANCEL_SCOPE is dequeued and processed by the driver
     * BEFORE ARM_SLEEP (because ARM_SLEEP was submitted after CANCEL_SCOPE
     * was already queued), CANCEL_SCOPE walks the armed list and finds
     * nothing for this fiber → no close_cb → timer fires after 10s → hang.
     *
     * Fix: re-check cancel_requested AFTER ARM_SLEEP is in the driver queue.
     * If set, submit CANCEL_TIMER for this specific sleep entry. Since
     * ARM_SLEEP was submitted first, the driver's FIFO queue guarantees
     * ARM_SLEEP is processed before CANCEL_TIMER → timer exists → CAS
     * ARMED→CANCEL_REQ succeeds → close_cb fires → fiber wakes. Idempotent
     * with CANCEL_SCOPE (CAS is guarded; only one winner). */
    if (nova_abool_load(&cancel_scope->cancel_requested)) {
        NovaDriverJob* cjob = (NovaDriverJob*)malloc(sizeof(NovaDriverJob));
        if (cjob) {
            cjob->kind = NOVA_DRV_JOB_CANCEL_TIMER;
            cjob->u.cancel_timer.st = &st;
            if (nova_driver_submit_job(cjob) != 0) {
                free(cjob);
                /* driver gone; stage = NEW, park predicate loops until
                 * timer fires naturally (worst-case: full sleep duration) */
            }
        }
    }

    /* Plan 83.11 Ф.3: futex-style park (post-park predicate recheck).
     *
     * Standard nova_sched_park_until has wake-before-park race for our driver
     * path: driver может close_cb fire ДО того как worker reach nova_sched_park
     * (SEQ_CST parked=true). Wake CAS parked true→false fails (still false),
     * no dispatch, fiber yields, no more wake → stuck.
     *
     * Fix (port Linux futex / Tokio AtomicWaker pattern): set parked=true
     * SEQ_CST FIRST, then re-check predicate. If pred true now (close_cb fired
     * в window), we either won CAS-clear (no wake came → return clean) or
     * lost (wake fired → must yield to consume dispatch, then exit on next
     * iteration). */
    /* Plan 83.11 Ф.3 futex park — closes wake-before-park race
     * (cas_won=93 vs 100 diag confirmed loss point).
     *
     * Pattern (port Linux futex / Tokio AtomicWaker recheck):
     *   1. fast-path pred check (close_cb fired between arm and park)
     *   2. SEQ_CST store parked=true — full fence, propagates state globally
     *   3. recheck pred AFTER barrier: если CLOSED — close_cb fired в window
     *      between step 1 and step 2
     *      - CAS clear parked true→false:
     *        - WIN: no wake raced us → return clean (no yield)
     *        - LOSE: wake fired between SEQ_CST and recheck, fiber in deque
     *          → yield to consume dispatch (avoid double-resume)
     *   4. else yield wait for wake */
    /* Plan 83.11 Ф.3.A v3 (Option A): use generic nova_sched_park_until.
     * Race-free now thanks к pending_wake[] integration в nova_sched.h. */
    nova_sched_park_until(scope, slot, _nova_sleep_drv_state_is_closed, &st);

    /* After CLOSED: st unlinked from armed_list by close_cb. Fiber safe to
     * return; coroutine stack может deallocate when fiber dies.
     *
     * [M-178-server-graceful-deadline] fix (Plan 173 Ф.3, 2026-07-12): the
     * wake above is ambiguous — CLOSED fires both on natural timer expiry
     * AND on a cooperative-cancel-driven early close (nova_scope_deliver_cancel
     * → nova_scope_cancel_wake_all → this fiber's stop_cb → close_cb).
     * Every OTHER park-based suspend site (nova_fiber_yield, channels.h
     * recv/send, net.c accept/read/write) re-checks cancel_requested after
     * waking and throws — this one silently returned success either way,
     * so a spawned child's `Time.sleep` treated a cancel-wake exactly like
     * a completed sleep and ran its remaining body to completion instead of
     * unwinding. That is the root cause of the deadline/timeout + spawned-
     * child leak: `supervised(deadline:)`/`supervised(timeout:)` fires the
     * TimeoutError on time (the scope's own deadline gate in
     * nova_supervised_run_impl is independent), but the child fiber that was
     * sleeping kept running in the background instead of being unwound.
     * Same shield-aware check as nova_fiber_yield (D188 R3): a cleanup body
     * running under an active cancel-mask defers the throw (cancel stays
     * latched on the scope; sleep returns normally so `defer`/cleanup code
     * completes-by-default). */
    if (nova_abool_load(&cancel_scope->cancel_requested)) {
        if (nova_cancel_mask_load(mco_running()) == 0) {
            nova_throw_cancel_reason(
                nova_str_from_cstr("scope cancelled"),
                cancel_scope->cancel_reason_ptr);
            /* unreachable */
        }
    }
}

/* Plan 83.11 Ф.3: tok.cancel() submits CANCEL_SCOPE job to driver.
 * Called from nova_cancel_token_cancel_reason after legacy cancel paths.
 *
 * Plan 83.11 §12.31: increment scope->pending_driver_jobs BEFORE submit so the
 * scope's stack frame is kept alive (via nova_supervised_run_impl spin-wait)
 * until the driver finishes dereferencing scope fields. ACQ_REL ordering on
 * the inc makes the increment observable before any thread sees the job in
 * the driver's queue (submit happens-after inc on this thread). */
static inline void _nova_cancel_via_driver(NovaFiberQueue* scope) {
    if (!nova_driver_is_started()) return;
    if (!scope) return;

    NovaDriverJob* job = (NovaDriverJob*)malloc(sizeof(NovaDriverJob));
    if (!job) {
        fprintf(stderr, "nova: _nova_cancel_via_driver: malloc job failed\n");
        return;  /* Fall through — legacy cancel paths may still catch */
    }
    job->kind = NOVA_DRV_JOB_CANCEL_SCOPE;
    job->u.cancel_scope.scope = scope;

    nova_aint_inc(&scope->pending_driver_jobs);
    if (nova_driver_submit_job(job) != 0) {
        free(job);
        /* Submit failed → roll back the increment so main doesn't wait
         * for a job that will never be processed. */
        (void)__atomic_fetch_sub(&scope->pending_driver_jobs, 1, __ATOMIC_ACQ_REL);
    }
}

/* Default impl: context-sensitive sleep (D71 + Plan 22 F2 libuv mandatory).
 *  - In fiber: park-on-uv_timer (Plan 22 Ф.4, D93)
 *  - On main inside supervised body → drain queue + bounded uv_run.
 *  - Else (top-level, no scope) → FATAL abort (D92 implicit main-scope
 *    invariant violated).
 *
 * `ms <= 0` → single yield (compatibility with `Time.sleep(0)` idiom). */
static inline nova_unit time_sleep_ms(nova_int ms) {
    /* Plan 110.2.2.a (D188 R3 + D192): cleanup-deadline gate before
     * suspending. Если scope-cleanup shield active и deadline уже
     * exceeded — throw сразу без park'а (иначе fiber бы спал N ms
     * over budget). */
    nv_shield_check_deadline();
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
        /* Plan 83.11 Ф.3: route to centralized driver if started, otherwise
         * fallback to legacy per-worker path (bootstrap/single-thread mode). */
        extern bool nova_driver_is_started(void);  /* forward decl */
        if (nova_driver_is_started()) {
            _nova_sleep_via_driver(_nova_active_scope, _nova_active_slot, ms);
        } else {
            _nova_sleep_via_libuv(_nova_active_scope, _nova_active_slot, ms);
        }
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

/* Plan 175 Ф.2-v3 (снос рукописного Time-dispatch): хенд-written
 * диспатчи `Nova_Time_sleep`/`_now_unix_ms`/`_now_ms`/`_now_ns`/`_now_monotonic_ns`/
 * `_local_offset_sec` (+ `_nova_time_default_now` / `_nova_time_ensure_default`),
 * ранее жившие ЗДЕСЬ, СНЕСЕНЫ — `emit_effect_type` (emit_c.rs, ТОТ ЖЕ
 * общий путь, что у любого пользовательского `type X effect {...}`)
 * теперь генерирует `Nova_Time_<op>()` постранично из схемы
 * std/prelude/effects.nv `Time`, включая lazy `#default_handler` install-once проверку
 * инлайн в теле каждого диспатчера.
 *
 * `time_sleep_ms` выше ОСТАЁТСЯ — это тонкий `extern "C"`
 * sleep-ПРИМИТИВ (scheduler-aware: fiber-park / drain / bootstrap-block),
 * вызываемый из ВСЕГДА-присутствующего `.nv` default handler'a
 * (std/prelude/effects.nv `time_default`) точно так же, как `time_wall_unix_ms`/
 * `time_monotonic_ns`/`time_local_offset_sec` (декларированы как `extern "C" fn`
 * дальше по файлу). С-fallback больше НЕТ — ambient-поведение
 * (Time без `with`/импорта) даёт `time_default` из prelude (auto-import в
 * КАЖДЫЙ CU), не второй хардкод-слой диспатча. */

/* ──────────────────────────────────────────────────────────────────
 * Plan 173 Ф.5 п.6: nova_runtime_reset() — сброс thread-local
 * error/handler-состояния МЕЖДУ panic-тестами в одном процессе.
 * ──────────────────────────────────────────────────────────────────
 *
 * Re-entry hazard (инфра для Ф.6 panics-клаузулы): пойманная через
 * test-frame паника выходит longjmp'ом МИМО эпилогов with-блоков и
 * fail-frame pop'ов — после неё висят: устаревшие `_nova_fail_top`
 * кадры (stack-адреса уже разрушены — следующий throw = segfault),
 * `_nova_interrupt_top`, `_nova_current_handler_iframe`,
 * `_nova_last_error.live`, установленные handler-vtable слоты
 * (string/any Fail, Time, user-effects), finalizer-stack и
 * active-scope маркеры. N паник подряд в одном процессе без сброса =
 * UB со второй.
 *
 * Вызывается ТОЛЬКО codegen'ом test-runner'а между тест-фреймами
 * (Ф.6; D348). Из user-кода НЕ доступен: идентификатор не существует
 * в Nova-неймспейсе (нет decl в std) — ссылка = compile error;
 * см. neg-тест err173/neg/f5_runtime_reset_unavailable.
 *
 * Handler-слоты сбрасываются через per-thread effect-registry (все
 * зарегистрированные TLS-адреса — built-in + user effects): дефолт
 * каждого слота = NULL (fallback-семантика effects.h). */
static inline void nova_runtime_reset(void) {
    _nova_fail_top = NULL;
    _nova_interrupt_top = NULL;
    _nova_current_handler_iframe = NULL;
    _nova_last_error.live = 0;
    nova_throw_trace_reset();       /* [M-173-error-return-trace] */
    _nova_throw_site.file = NULL;   /* стейл throw-site не течёт в следующий тест */
    _nova_handler_Fail = NULL;
    _nova_handler_Fail_any = NULL;
    _nova_handler_Time = NULL;
    _nova_active_finalizer_stack = NULL;
    _nova_active_scope = NULL;
    _nova_active_slot = -1;
    for (int i = 0; i < _nova_effect_registry.count; i++) {
        *_nova_effect_registry.slots[i] = NULL;
    }
}

#endif /* NOVA_RT_FIBERS_H */
