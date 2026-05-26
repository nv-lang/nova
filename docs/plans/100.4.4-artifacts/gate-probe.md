// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100.4.4 — Ф.0 GATE: design probe + emit_defer audit

> Дата: 2026-05-26. Worktree: `nova-p100-4-4`. Branch: `plan-100-4-4-multi-defer`.
> HEAD baseline: `c5109c311fd` (post-100.4.1 + 100.2 closure).

## 1. Design D1–D8 locked
Per plan-doc: D1 LIFO continues после partial failure; D2 panic composes
(no abort); D3 defer-stack runtime structure; D4 chain visibility; D5
Time.timeout; D6 memory safety; D7 composition order; D8 caller inspect.

## 2. Current emit_defer audit (compiler-codegen/src/codegen/emit_c.rs)

**Структура (Plan 20 + 100.4.3):**
- `enter_defer_scope` (12399): per-block NovaFailFrame + setjmp; throw-path
  cleanup внутри handler.
- `leave_defer_scope` (12558): normal-exit; iterate `entries.iter().rev()`
  LIFO; inline body без per-defer setjmp.
- `emit_early_exit_cleanup` (12603): return/break/continue; inline LIFO.

**Текущее FAIL behaviour:**
defer body throws → catches scope's `_defer_<N>_ff` → cleanup cascade runs;
если cascade-internal throw происходит → escapes к outer scope (LIFO halted).
= Rust-style first-error-aborts.

## 3. План реализации (Option B — bootstrap MVP)

**Per-defer NovaFailFrame wrap в leave_defer_scope (normal-exit) + 
enter_defer_scope throw-path cleanup.** Compose collected fails;
trigger throw at end если accumulated.

Pseudocode (для leave_defer_scope):
```c
nova_str _comp_msg = {0};
NovaThrowKind _comp_kind = 0;
void* _comp_payload = NULL; NovaTypeId _comp_tid = 0;
NovaErrorChain* _comp_chain = NULL;
int _comp_has_fail = 0;

for entry in entries.iter().rev() {
    if (active) {
        active = 0;
        NovaFailFrame _df;
        nova_fail_push(&_df);
        _df.error_suppressed = NULL;
        if (setjmp(_df.jmp) == 0) {
            { body }
            nova_fail_pop();
        } else {
            nova_fail_pop();
            if (!_comp_has_fail) {
                _comp_has_fail = 1;
                _comp_msg = _df.error_msg;
                _comp_kind = _df.error_kind;
                _comp_payload = _df.error_user_payload;
                _comp_tid = _df.error_user_type_id;
            } else {
                NovaErrorChain* node = nova_alloc(sizeof(NovaErrorChain));
                node->msg = _df.error_msg; node->kind = _df.error_kind;
                node->user_payload = _df.error_user_payload;
                node->user_type_id = _df.error_user_type_id;
                node->next = _comp_chain;
                _comp_chain = node;
            }
            /* LIFO continue */
        }
    }
}
/* scope's own fail-frame + interrupt-frame pop */
if (!ff_popped) { nova_fail_pop(); }
if (!if_popped) { nova_interrupt_pop(); }

if (_comp_has_fail) {
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = _comp_msg;
        _nova_fail_top->error_kind = _comp_kind;
        _nova_fail_top->error_user_payload = _comp_payload;
        _nova_fail_top->error_user_type_id = _comp_tid;
        _nova_fail_top->error_suppressed = _comp_chain;
        longjmp(_nova_fail_top->jmp, 1);
    } else {
        /* no outer handler — abort */
        fflush(stdout);
        fprintf(stderr, "nova: defer cleanup-fail с no outer handler: %.*s\n",
            (int)_comp_msg.len, _comp_msg.ptr);
        abort();
    }
}
```

В **enter_defer_scope throw-path** (где scope's setjmp catches) — same pattern, плюс
изначально primary = scope's fail-frame error_msg (existing propagating).
Defer-fails compose в scope_ff.error_suppressed; финальный re-throw 
preserves both primary + suppressed.

**Panic — falls out automatically:** nv_panic routes через nearest fail-frame
(`_nova_fail_top`), что = `_df.jmp` (per-defer frame). Same composition path.

## 4. Followup'ы (вне MVP, marker'ы)

- `[M-100.4.4-early-exit-cleanup-wrap]` — emit_early_exit_cleanup тоже
  нуждается в per-defer wrap для return/break/continue paths.
- `[M-100.4.4-interrupt-path-wrap]` — interrupt-path cleanup в enter_defer_scope.
- `[M-100.4.4-time-timeout-defer]` — Time.timeout protected cleanup
  (требует Plan 100.4.2 suspend-в-defer).
- `[M-100.4.4-chain-cap]` — memory pressure cap для chain длины (sentinel
  "and N more"); сейчас unlimited (acceptable для bootstrap).

## 5. Acceptance Ф.0
- [x] D1-D8 locked.
- [x] emit_defer audit complete.
- [x] Implementation strategy detailed.
- [x] Bootstrap scope decided (Option B).
- [x] Followup markers enumerated.

**GATE Ф.0: PASS.** → Ф.1 spec.
