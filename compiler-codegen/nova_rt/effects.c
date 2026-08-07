/* nova_rt/effects.c — thread-local state for Fail, effect handlers, and tests */

#include "nova_rt.h"
#include "minicoro.h"

/* Whether the calling code is currently running on a fiber's stack.
 * Used by nova_assert to decide between fail-frame routing (in fiber) and
 * test-frame routing (on main flow). Defined here because effects.h is
 * included before fibers.h / minicoro.h, so it can't see mco_running(). */
int nova_in_fiber(void) {
    return mco_running() != NULL ? 1 : 0;
}

/* [221.1 №431] Late-cancel counter — process-wide, not __thread: the whole
 * point is a single end-of-run tally regardless of which worker observed
 * it. Plain 32-bit int + __atomic builtins (no dependency on sync.h's
 * nova_atomic_int/nova_aint_* — effects.c only needs the two builtins
 * directly, and effects.h itself can't see sync.h at all, see the comment
 * on _nova_cancel_no_handler's declaration). */
static int32_t _nova_late_cancel_count = 0;

static void _nova_late_cancel_atexit_print(void) {
    int32_t n = __atomic_load_n(&_nova_late_cancel_count, __ATOMIC_RELAXED);
    if (n <= 0) return;
    fflush(stdout);
    fprintf(stderr,
        "nova: %d cancellation(s) arrived after their scope had already "
        "unwound (harmless per D75 — supervised(cancel:)'s token was "
        "either unbound or its scope had already ended by the time the "
        "signal was delivered; see docs/dev/mn-coding-conventions.md §11 "
        "and 221.1 defect #431)\n",
        n);
}

/* [221.1 №431] `nova_throw_cancel`/`nova_throw_cancel_reason` (effects.h)
 * land here when `_nova_fail_top == NULL` — no active fail-frame to catch
 * the cancel. D75 promises this is a harmless no-op ("token not bound, or
 * scope already ended"); the OLD behaviour (abort() the whole process,
 * effects.h pre-#431) turned every instance of that harmless race into a
 * hard crash. The rejected alternative — just returning normally so the
 * caller (channels.h recv/send, fibers.h preempt-check, …) continues as if
 * nothing happened — is WORSE than abort(): the fiber would keep executing
 * its body against a scope that has already been torn down (writing into
 * freed/reused structures, corrupting state silently instead of crashing
 * loudly). So this function does neither: it quietly retires the CURRENT
 * fiber — never resumes it with more work, does not report anything
 * upward into parent-scope structures (nobody is waiting for this fiber's
 * result — that is exactly the D75 precondition for this path to be
 * reached at all).
 *
 * A counter + one end-of-run summary line (below, lazily atexit-
 * registered on first occurrence) keeps this from silently swallowing a
 * REAL bug's signal the way the toothless `nova:allow` hatch did (221.1
 * #423) — see the mn-coding-conventions.md §0 "what does the reference
 * have that we don't" write-up in this window's report for the Go
 * comparison (Go's cancellation is a channel nobody has to read — Nova's
 * is a throw with nobody left to catch it; the counter is how the two
 * models end up equally harmless in this one corner). */
void _nova_cancel_no_handler(void) {
    int32_t prev = __atomic_fetch_add(&_nova_late_cancel_count, 1, __ATOMIC_RELAXED);
    if (prev == 0) {
        /* Lazy: only pay for atexit registration on the FIRST occurrence —
         * the overwhelming common case (a whole clean run) never touches
         * this function at all. `prev == 0` is true for EXACTLY ONE
         * caller (the atomic fetch-add's return value is unique per
         * increment), so this is race-free without a separate flag. */
        atexit(_nova_late_cancel_atexit_print);
    }

    /* [221.1 №431 п.3] "No scope reference at all" (not even the orphan/
     * detach pool) should be structurally impossible — D50 guarantees
     * every fiber runs inside SOME scope, real supervised or orphan.
     * Loud + stop in a debug build (genuinely new bug class, worth
     * catching with a live debugger); the SAME quiet retirement as the
     * ordinary case in release (never crash the process over a
     * diagnostic — that is exactly the abort() this window removes). */
    if (_nova_active_scope == NULL) {
#ifdef NOVA_DEBUG
        fflush(stdout);
        fprintf(stderr,
            "nova: FATAL: cancel delivered to a fiber with NO scope "
            "reference at all — not even the orphan/detach pool "
            "(nova_runtime_orphan_scope()). D50 guarantees every fiber "
            "runs inside SOME scope; this is a structural-concurrency "
            "invariant violation, not a late-cancel race. See 221.1 "
            "defect #431 п.3.\n");
        abort();
#endif
    }

    mco_coro* co = mco_running();
    if (!co) {
        /* Should not happen — even main-body runs fiber-hosted post-D92 —
         * but there is genuinely nothing safe left to do without a
         * coroutine to park: no C stack to unwind and no fail-frame to
         * catch on. Return is the least-bad remaining option (matches
         * the pre-#431 fallback exactly for this one unreachable-in-
         * practice edge). */
        return;
    }
    /* [221.1 №431 — hang discriminator, empirically found in THIS window]
     * An UNCONDITIONAL `for(;;) mco_yield(co);` was the first design here
     * and was REJECTED after direct testing (`docs/plans/repro/
     * p431_direct_repro.nv`, see PROGRESS-p431.md): a fiber that is still
     * tracked by a LIVE scope's drain loop (bootstrap `nova_supervised_
     * step` / the armed worker loop) keeps resuming a merely-yielded —
     * not dead — coroutine forever, since neither loop has any OTHER
     * signal that this fiber gave up. That trades the abort() this fix
     * removes for an unbounded, CPU-spinning, silent hang — strictly
     * worse for an operator to diagnose (no crash, no stack trace, just a
     * process that never returns).
     *
     * A BOUNDED number of cooperative yields — long enough to let an
     * in-flight dispatch/TLS-restore step (the transient race §10/§11 of
     * mn-coding-conventions.md describe) settle, short enough to never
     * look like a hang — then a plain, signal-free `exit()` (NOT
     * `abort()`): no SIGABRT, no core dump, atexit hooks still run (the
     * late-cancel summary above still prints). This remains the only
     * place left in this function where the process can still end, and
     * it is reached only after every safer option has been tried. */
    for (int _nv_retry = 0; _nv_retry < 3; _nv_retry++) {
        mco_yield(co);
    }
    fflush(stdout);
    fprintf(stderr,
        "nova: cancel delivered to a fiber with no handler, still none "
        "after 3 cooperative retries — exiting (not aborting) rather than "
        "hang forever waiting for a live scope the evidence says is "
        "already gone. See 221.1 defect #431.\n");
    exit(1);
}

/* Plan 173 Ф.5 п.2 (D192-РЕТРАКТ): `_nova_throw_cleanup_timeout_fn` УДАЛЁН
 * вместе с типом CleanupTimeoutError — force-прерывания cleanup'а не
 * существует. Превышение watchdog-порога = one-shot stderr-варн
 * (nv_shield_check_deadline) + duration_ms/overrun в ResourceTrace
 * exit-событии (D185 amend). */

/* Plan 174 (D349): supervised scope-deadline typed-throw indirection. Set by
 * codegen-emitted impl in the user TU when `TimeoutError` is referenced
 * (constructs Nova_TimeoutError + calls nova_throw_typed). NULL — fallback to
 * plain-string throw in nova_throw_scope_timeout. Process-wide (not __thread). */
void (*_nova_throw_scope_timeout_fn)(int64_t deadline_ns) = NULL;

/* Plan 173.2 (supervision-as-effect): Supervisor decision bridge. Set by
 * generated main() when the CU knows the `Supervisor` effect (prelude
 * present) — the impl reads the ambient `_nova_handler_Supervisor` TLS
 * vtable, boxes the NovaChildError into a Nova `any`, invokes the handler's
 * `on_child_fail(idx, err)` and maps the Decision tag to NOVA_SUPERVISE_*.
 * NULL — every decision defaults to Escalate (= pre-173.2 behaviour).
 * Process-wide (not __thread): the impl itself resolves per-thread TLS. */
nova_int (*_nova_supervisor_decide_fn)(void* scope, nova_int idx,
                                       const void* err) = NULL;

/* D61: `interrupt v` — early-exit from the nearest enclosing with-block.
 *
 * Semantics across mco coroutine boundary:
 *
 * 1. **Inside fiber, with-frame in same fiber**: _nova_interrupt_top points
 *    to a frame on the fiber-stack (pushed by `with`-block within
 *    spawn-body). longjmp safe — stays on fiber-stack.
 *
 * 2. **Inside fiber, NO with-frame in same fiber** (with-block lives on
 *    main-stack, outside `supervised`): direct longjmp would cross mco
 *    boundary → UB. Instead:
 *      a. Record `(interrupt_pending=true, interrupt_value=v)` in the
 *         active scope queue.
 *      b. longjmp to fiber-local fail-frame (pushed by spawn-entry).
 *         Spawn-entry catch sees pending interrupt and skips
 *         nova_fiber_report_error.
 *      c. After all fibers drain, `nova_supervised_run` re-issues
 *         `nova_interrupt(v)` on main-flow where with-frame is reachable.
 *
 * 3. **On main-flow** (no fiber): longjmp directly to with-frame. */
/* Plan 221.1 №108 followup (2026-07-25) — see effects.h `_nova_main_fiber_co`
 * doc comment for the full rationale. `!mco_running()` used to mean
 * "definitely main-flow, no coroutine, same-stack" — main-body becoming a
 * fiber (#108) broke that equivalence for plain top-level code with no
 * spawn/supervised boundary anywhere. This restores the original property:
 * true for genuine main-flow (pre-#108 builds, or any future non-fiber
 * caller) AND for the root main-fiber itself; false for any genuinely
 * spawned/detached child fiber (where the original cross-stack-longjmp UB
 * concern this gate exists for is still real). */
static inline int nova_cross_effect_route_safe(void) {
    mco_coro* co = mco_running();
    return !co || (void*)co == _nova_main_fiber_co;
}

void nova_interrupt(nova_int value) {
    /* Plan 61 followup #1: handler-arm interrupt routing. Если активен
     * handler-arm (set by Nova_Fail_fail / nova_throw_typed dispatchers),
     * И owner НЕ в текущей _nova_interrupt_top chain (cross-effect throw —
     * inner with-block pushed позже owner) → jump'аем напрямую в owner.
     *
     * Если owner IS в chain (single with-block — обычный case, ИЛИ defer
     * frames pushed inside body) → fallback к default _nova_interrupt_top.
     * Это позволяет defer cleanup frames intercept'нуть interrupt и
     * propagate через интерс/re-issue (см. defer codegen pattern). */
    /* Plan 61 followup #1: cross-effect routing — ТОЛЬКО на main-flow
     * (НЕ в fiber context). Внутри coroutine longjmp к owner_iframe (что
     * лежит на main's stack) — UB / STATUS_BAD_STACK. Cross-effect throw
     * в fiber обрабатывается через scope.interrupt_pending mechanism
     * (default path ниже). */
    if (_nova_current_handler_iframe
        && _nova_interrupt_top != _nova_current_handler_iframe
        && nova_cross_effect_route_safe()) {
        /* owner != top — walk chain. Если intermediate DEFER_SCOPE frames
         * есть → fall through к top (defer cleanup → re-issue → propagate).
         * Если только WITHBLOCK frames (nested with) → skip directly к owner. */
        int has_defer_between = 0;
        NovaInterruptFrame* p = _nova_interrupt_top;
        while (p && p != _nova_current_handler_iframe) {
            if (p->kind == NOVA_IFRAME_DEFER_SCOPE) {
                has_defer_between = 1;
                break;
            }
            p = p->prev;
        }
        if (!has_defer_between) {
            while (_nova_interrupt_top && _nova_interrupt_top != _nova_current_handler_iframe) {
                _nova_interrupt_top = _nova_interrupt_top->prev;
            }
            NovaInterruptFrame* f = _nova_current_handler_iframe;
            /* Restore saved handler iframe so the with-block recovery code
             * (after longjmp) sees the correct outer context, not the stale
             * handler-arm pointer that was set when the Fail handler was
             * invoked. Without this, the pointer leaks past the with-block
             * and causes STATUS_BAD_STACK on Windows when a later test runs. */
            _nova_current_handler_iframe = f->saved_handler_iframe;
            f->value = value;
            _nova_last_error.live = 0;  /* Ф.4 #5: with-block consumes the interrupt */
            nova_throw_trace_reset();   /* [M-173-error-return-trace] */
            longjmp(f->jmp, 1);
        }
        /* else: fall through к default top (defer frames intercept). */
    }
    if (_nova_interrupt_top) {
        /* Case 1 (fiber-local with) or case 3 (main-flow with) — both safe.
         * Restore saved handler iframe before longjmp: prevents stale pointer
         * leaking past the with-block when interrupt fires inside a Fail
         * handler body (the dispatch code sets _nova_current_handler_iframe
         * but the restore-after-call is bypassed by longjmp). */
        NovaInterruptFrame* top = _nova_interrupt_top;
        _nova_current_handler_iframe = top->saved_handler_iframe;
        top->value = value;
        /* Ф.4 #5: a real with-block (not a defer intercept) consuming the
         * interrupt ends error propagation — invalidate the stable snapshot so
         * it can't leak into a later unrelated value-`interrupt`. Defers
         * (DEFER_SCOPE) re-issue, so they keep it live until the with-block. */
        if (top->kind == NOVA_IFRAME_WITHBLOCK) {
            _nova_last_error.live = 0;
            nova_throw_trace_reset();  /* [M-173-error-return-trace] */
        }
        longjmp(top->jmp, 1);
        /* unreachable */
    }
    if (mco_running() && _nova_active_scope) {
        /* Case 2: cross-boundary interrupt. Record pending + abort fiber via
         * fail-frame. spawn-entry catch sees q->interrupt_pending and skips
         * report_error so we don't poison `first_error`. Also set
         * cancel_requested so peer fibers in same scope unwind on next
         * yield-point — `interrupt v` is a hard exit from the with-block,
         * peers shouldn't keep running after handler decided to exit. */
        _nova_active_scope->interrupt_pending = true;
        _nova_active_scope->interrupt_value   = value;
        _nova_active_scope->cancel_requested  = true;
        if (_nova_fail_top) {
            /* Use a sentinel error message so spawn-entry can distinguish
             * interrupt-abort from real error. The catch reads
             * scope->interrupt_pending instead. */
            _nova_fail_top->error_msg = (nova_str){
                .ptr = "__nova_interrupt__", .len = 18
            };
            longjmp(_nova_fail_top->jmp, 1);
            /* unreachable */
        }
        /* No fail-frame either — should not happen (spawn-entry always
         * pushes one). Fall through to no-op as last resort. */
    }
    /* No with-block, no fiber: interrupt is a no-op (body already exited). */
}

/* Plan 39 Issue A: pointer-variant of nova_interrupt. Stores via
 * NovaInterruptFrame.value_ptr / NovaFiberQueue.interrupt_value_ptr.
 * Mutually-exclusive с nova_interrupt() per `with`-block instance:
 * codegen emits точно один вариант в зависимости от типа выражения. */
void nova_interrupt_ptr(void* value) {
    /* Plan 61 followup #1: см. nova_interrupt() rationale. */
    /* Plan 61 followup #1: см. nova_interrupt() — skip cross-effect routing
     * в fiber context (UB across coroutine boundary). */
    if (_nova_current_handler_iframe
        && _nova_interrupt_top != _nova_current_handler_iframe
        && nova_cross_effect_route_safe()) {
        int has_defer_between = 0;
        NovaInterruptFrame* p = _nova_interrupt_top;
        while (p && p != _nova_current_handler_iframe) {
            if (p->kind == NOVA_IFRAME_DEFER_SCOPE) {
                has_defer_between = 1;
                break;
            }
            p = p->prev;
        }
        if (!has_defer_between) {
            while (_nova_interrupt_top && _nova_interrupt_top != _nova_current_handler_iframe) {
                _nova_interrupt_top = _nova_interrupt_top->prev;
            }
            NovaInterruptFrame* f = _nova_current_handler_iframe;
            _nova_current_handler_iframe = f->saved_handler_iframe;
            f->value_ptr = value;
            _nova_last_error.live = 0;  /* Ф.4 #5: with-block consumes the interrupt */
            nova_throw_trace_reset();   /* [M-173-error-return-trace] */
            longjmp(f->jmp, 1);
        }
    }
    if (_nova_interrupt_top) {
        NovaInterruptFrame* top = _nova_interrupt_top;
        _nova_current_handler_iframe = top->saved_handler_iframe;
        top->value_ptr = value;
        if (top->kind == NOVA_IFRAME_WITHBLOCK) {
            _nova_last_error.live = 0;  /* Ф.4 #5 */
            nova_throw_trace_reset();    /* [M-173-error-return-trace] */
        }
        longjmp(top->jmp, 1);
        /* unreachable */
    }
    if (mco_running() && _nova_active_scope) {
        _nova_active_scope->interrupt_pending   = true;
        _nova_active_scope->interrupt_via_ptr   = true;
        _nova_active_scope->interrupt_value_ptr = value;
        _nova_active_scope->cancel_requested    = true;
        if (_nova_fail_top) {
            _nova_fail_top->error_msg = (nova_str){
                .ptr = "__nova_interrupt__", .len = 18
            };
            longjmp(_nova_fail_top->jmp, 1);
            /* unreachable */
        }
    }
    /* No with-block, no fiber: no-op. */
}

#ifdef _MSC_VER
__declspec(thread) NovaFailFrame*      _nova_fail_top      = NULL;
/* Plan 201 trace-per-fiber: combined last_error/throw_site/throw_trace
 * bucket (effects.h NovaFiberErrorState) + the ONE active-pointer swapped
 * per-fiber around mco_resume (runtime.c) / nova_supervised_step
 * (fibers.h). `_nova_error_state_native` is the per-OS-thread fallback
 * bucket for "outside any fiber" contexts; `_nova_error_state_p` starts
 * NULL and self-heals to it via `nova_error_state_active()` (effects.h). */
__declspec(thread) NovaFiberErrorState  _nova_error_state_native = {0};
__declspec(thread) NovaFiberErrorState* _nova_error_state_p      = NULL;
__declspec(thread) NovaInterruptFrame* _nova_interrupt_top = NULL;
/* Plan 61 followup #1: cross-effect throw routing slot. */
__declspec(thread) NovaInterruptFrame* _nova_current_handler_iframe = NULL;
__declspec(thread) NovaTestFrame*      _nova_test_frame    = NULL;
__declspec(thread) NovaVtable_Fail*     _nova_handler_Fail     = NULL;  /* default NULL → Nova_Fail_fail falls back to nova_throw */
__declspec(thread) NovaVtable_Fail_any* _nova_handler_Fail_any = NULL;  /* Plan 61 Ф.2 typed erased slot */
/* Plan 175 Ф.2-v3: struct type stays hand-written (effects.h doc comment —
 * channels.h / runtime.c need a stable named type), so the TLS slot
 * definition stays HERE too (dispatch FUNCTIONS moved to generic codegen,
 * not this slot). */
__declspec(thread) NovaVtable_Time*     _nova_handler_Time     = NULL;  /* default NULL → ambient #default_handler(Time) installs lazily (prelude) */
__declspec(thread) NovaFiberQueue*     _nova_active_scope  = NULL;  /* active supervised scope for current thread */
__declspec(thread) int                 _nova_active_slot   = -1;
/* Plan 44.5 Layer 5 deferred-unlock: set by fiber in park_with_unlock before
 * mco_yield; called by worker loop AFTER mco_resume returns (= after fiber is
 * truly MCO_SUSPENDED). Prevents race where cross-thread wake clears parked
 * flag before mco_yield, causing double-push to worker deque. */
__declspec(thread) void (*_nova_park_unlock_fn)(void*) = NULL;
__declspec(thread) void*               _nova_park_unlock_arg = NULL;
__declspec(thread) volatile int*       _nova_preempt_ptr   = NULL;  /* Plan 44.7 */
/* Plan 110.9.3 V1.1 [M-110.9.3-register-finalizer-lifo]: active finalizer
 * stack for Application effect. Saved+initialized в `with Application = ...`
 * block prologue, fired LIFO + restored на exit. NULL outside Application
 * blocks → nova_app_register_finalizer becomes no-op. */
__declspec(thread) NovaFinalizerStack* _nova_active_finalizer_stack = NULL;
#else
__thread NovaFailFrame*      _nova_fail_top      = NULL;
/* Plan 201 trace-per-fiber: see MSVC branch above for rationale. */
__thread NovaFiberErrorState  _nova_error_state_native = {0};
__thread NovaFiberErrorState* _nova_error_state_p      = NULL;
__thread NovaInterruptFrame* _nova_interrupt_top = NULL;
__thread NovaInterruptFrame* _nova_current_handler_iframe = NULL;  /* Plan 61 fu#1 */
__thread NovaTestFrame*      _nova_test_frame    = NULL;
__thread NovaVtable_Fail*     _nova_handler_Fail     = NULL;
__thread NovaVtable_Fail_any* _nova_handler_Fail_any = NULL;  /* Plan 61 Ф.2 */
__thread NovaVtable_Time*     _nova_handler_Time     = NULL;
__thread NovaFiberQueue*     _nova_active_scope  = NULL;
__thread int                 _nova_active_slot   = -1;
__thread void (*_nova_park_unlock_fn)(void*)  = NULL;
__thread void*               _nova_park_unlock_arg = NULL;
__thread volatile int*       _nova_preempt_ptr   = NULL;  /* Plan 44.7 */
/* Plan 110.9.3 V1.1 [M-110.9.3-register-finalizer-lifo]: см. MSVC branch. */
__thread NovaFinalizerStack* _nova_active_finalizer_stack = NULL;
#endif

/* Plan 175 Ф.2-v3: `_nova_time_default_ctor` (Ф.2-v2 special-case hook) is
 * GONE — the generic `#default_handler` mechanism now lives inline inside
 * each `Nova_Time_<op>()` dispatcher body (`emit_effect_type`, same as any
 * other `#default_handler` effect). See effects.h doc comment. */

/* Per-fiber handler scoping: registry of effect-storage addresses.
 * Built-in effects (Fail, Time) auto-registered in nova_runtime_init.
 * User-defined эффекты регистрируются codegen'ом при первом использовании
 * (через `nova_register_effect_storage(&_nova_handler_X)` в startup-code).
 *
 * Plan 83.10.4 Ф.3 [M-83.10.1-per-fiber-handler-tls-race]:
 * Registry ДОЛЖЕН быть per-thread (TLS), а не global. Потому что
 * `_nova_handler_Time`, `_nova_handler_Fail` и др. — __declspec(thread)
 * переменные с РАЗНЫМИ АДРЕСАМИ на разных потоках (Windows TLS: каждый
 * поток имеет свой TEB + offset). Если registry global, он хранит адреса
 * main-thread'а. Когда worker вызывает nova_effect_snapshot_restore,
 * он пишет в память main-thread'а (не в свои TLS переменные) → fiber
 * видит NULL handler (default worker TLS) вместо parent-inherited handler.
 *
 * Fix: __declspec(thread) registry → каждый поток регистрирует свои
 * СОБСТВЕННЫЕ TLS адреса. Snapshot values (скопированные с parent) верно
 * восстанавливаются в worker-thread's TLS copies. */
#ifdef _MSC_VER
__declspec(thread) NovaEffectRegistry _nova_effect_registry;
#else
__thread NovaEffectRegistry _nova_effect_registry;
#endif

/* Plan 83.10.4 Ф.3: function pointer set by generated code (nova_fn_main)
 * to register all program effects (built-ins + user-defined). Called by
 * each worker thread at startup so it has its own TLS-address registry. */
void (*_nova_register_effects_fn)(void) = NULL;

/* Plan 221.1 №108 followup — see effects.h doc comment. */
void* _nova_main_fiber_co = NULL;

/* [221.1 №431] TEMPORARY direct-repro hook — see docs/plans/repro/
 * p431_direct_repro.nv and PROGRESS-p431.md's "почему прямой юнит-тест"
 * section. Not part of the public runtime surface; removed once this
 * window's acceptance is verified (kept only long enough for the
 * before/after abort() proof, which an organic scheduler race could not
 * reliably reproduce within this window's budget — the one historically-
 * crashing fixture, `pos_max_fibers_concurrent.nv`, no longer reproduces
 * either, see PROGRESS-p427.md's 200/200 clean runs).
 *
 * Forces the EXACT documented failure precondition all 8 registry call
 * sites share (`_nova_fail_top == NULL`) and calls `nova_throw_cancel`
 * directly — the real defect location, byte-for-byte, no race required. */
void nova_p431_direct_repro(void) {
    _nova_fail_top = NULL;
    _nova_active_scope = NULL;
    nova_throw_cancel(nova_str_from_cstr("p431 direct repro: no handler, no scope"));
}
