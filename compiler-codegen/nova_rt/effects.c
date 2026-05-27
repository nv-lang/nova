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
        && !mco_running()) {
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
            _nova_current_handler_iframe = NULL;
            f->value = value;
            longjmp(f->jmp, 1);
        }
        /* else: fall through к default top (defer frames intercept). */
    }
    if (_nova_interrupt_top) {
        /* Case 1 (fiber-local with) or case 3 (main-flow with) — both safe. */
        _nova_interrupt_top->value = value;
        longjmp(_nova_interrupt_top->jmp, 1);
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
        && !mco_running()) {
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
            _nova_current_handler_iframe = NULL;
            f->value_ptr = value;
            longjmp(f->jmp, 1);
        }
    }
    if (_nova_interrupt_top) {
        _nova_interrupt_top->value_ptr = value;
        longjmp(_nova_interrupt_top->jmp, 1);
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
__declspec(thread) NovaInterruptFrame* _nova_interrupt_top = NULL;
/* Plan 61 followup #1: cross-effect throw routing slot. */
__declspec(thread) NovaInterruptFrame* _nova_current_handler_iframe = NULL;
__declspec(thread) NovaTestFrame*      _nova_test_frame    = NULL;
__declspec(thread) NovaVtable_Fail*     _nova_handler_Fail     = NULL;  /* default NULL → Nova_Fail_fail falls back to nova_throw */
__declspec(thread) NovaVtable_Fail_any* _nova_handler_Fail_any = NULL;  /* Plan 61 Ф.2 typed erased slot */
__declspec(thread) NovaVtable_Time*     _nova_handler_Time     = NULL;  /* default NULL → context-sensitive yield (see fibers.h) */
__declspec(thread) NovaFiberQueue*     _nova_active_scope  = NULL;  /* active supervised scope for current thread */
__declspec(thread) int                 _nova_active_slot   = -1;
/* Plan 44.5 Layer 5 deferred-unlock: set by fiber in park_with_unlock before
 * mco_yield; called by worker loop AFTER mco_resume returns (= after fiber is
 * truly MCO_SUSPENDED). Prevents race where cross-thread wake clears parked
 * flag before mco_yield, causing double-push to worker deque. */
__declspec(thread) void (*_nova_park_unlock_fn)(void*) = NULL;
__declspec(thread) void*               _nova_park_unlock_arg = NULL;
__declspec(thread) volatile int*       _nova_preempt_ptr   = NULL;  /* Plan 44.7 */
#else
__thread NovaFailFrame*      _nova_fail_top      = NULL;
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
#endif

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
