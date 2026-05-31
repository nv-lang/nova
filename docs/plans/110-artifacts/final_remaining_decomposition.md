// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 110 Final Remaining Work — Decomposition

> **Created 2026-05-31.** Hard blockers для full Plan 110 closure
> decomposed на atomic-merge sub-sub-sub steps. Each ≤ 1 session
> production-grade с tests + acceptance criteria.

## Plan 110.2 Cancel-Shield Runtime (3 remaining sub-sub)

### 110.2.1.a — Fiber cancel-mask counter
- **Scope:** Add `nova_atomic_int cancel_mask_count` field to fiber state.
- **Files:** `nova_rt/fibers.h` (struct field), `nova_rt/effects.h`
  (nv_consume_enter_shield increments, nv_consume_leave_shield decrements).
- **Tests:** `nova_tests/plan110/shield_basic_mask.nv` —
  consume scope с body containing cancel-check; verify cancel deferred
  during cleanup.
- **Acceptance:**
  - A110.2.1.a.a: cancel_mask_count atomic increment/decrement landed.
  - A110.2.1.a.b: cancel-receive code checks mask > 0 → defer.
- **Estimated:** 1 session.

### 110.2.2.a — Deadline check at suspend points
- **Scope:** At suspend entry (Time.sleep, Net I/O), check deadline_ns
  if cancel_mask_count > 0; throw `CleanupTimeoutError` if exceeded.
- **Files:** `nova_rt/effects.h` (deadline_ns field в shield), suspend
  call points.
- **Tests:** `nova_tests/plan110/shield_deadline_exceeded.nv` —
  consume scope с body await long_op exceeding timeout.
- **Acceptance:**
  - A110.2.2.a.a: deadline check fires CleanupTimeoutError.
  - A110.2.2.a.b: error propagates через outer fail-frame.
- **Estimated:** 1 session.

### 110.2.6 — Plan 110.2 close
- Tests + close summary + 110.2 sub-plan documentation.

## Plan 110.4 Effects Runtime (3 remaining sub-sub)

### 110.4.4.a — Cleanup effect codegen on_scope_enter emit
- **Scope:** ConsumeScope codegen emits `perform Cleanup.on_scope_enter`
  при entry if Cleanup handler bound в effect-stack.
- **Files:** `compiler-codegen/src/codegen/emit_c.rs` (ConsumeScope arm).
- **Tests:** `nova_tests/plan110/cleanup_effect_enter_t7_1.nv`.
- **Acceptance:** A110.4.4.a.a: handler called с type name + timeout_ms.
- **Estimated:** 1 session.

### 110.4.4.b — Cleanup effect on_scope_exit emit
- **Scope:** Same для on_scope_exit at ConsumeScope completion.
- **Tests:** `nova_tests/plan110/cleanup_effect_exit_t7_2.nv`.
- **Acceptance:** A110.4.4.b.a: handler called с outcome value.
- **Estimated:** 1 session.

### 110.4.6.a — Application Level-2 runtime integration
- **Scope:** `nv_resolve_exit_timeout_ms()` checks Application effect-stack;
  if handler bound, returns its `default_exit_timeout_ms`.
- **Files:** `nova_rt/effects.h` (effect-stack traversal helper).
- **Tests:** `nova_tests/plan110/timeout_application_level2_t3_8.nv`.
- **Acceptance:** A110.4.6.a.a: 3-level resolution via Application.
- **Estimated:** 1 session.

### 110.4.7 — Cross-fiber Application propagation
- **Scope:** spawn fiber copies parent's Application handler reference.
- **Files:** `nova_rt/fibers.h` (spawn fiber init).
- **Tests:** `nova_tests/plan110/application_cross_fiber_t8_7.nv`.
- **Estimated:** 1 session.

### 110.4.8 — Plan 110.4 close + final fixtures.

## Plan 110.6 LSP + Bench (8 remaining)

### 110.6.1.a — Structured Suggestion for D188-not-consumable
- **Scope:** Add `Suggestion` to D188-not-consumable Diagnostic с
  template impl.
- **Tests:** existing neg_consume_not_consumable.nv verifies suggestion.
- **Estimated:** 1 session.

### 110.6.5 — Racing cancels stress (after 110.2.1.a)
- **Scope:** Multi-fiber spawn + cancel before scope-exit.
- **Estimated:** 1 session after 110.2.1.a.

### 110.6.8 — Bench cancel-shield overhead
- **Scope:** `bench/plan110/shield_overhead.nv` — measure entry/exit.
- **Tests:** baseline + after 110.2.x.
- **Estimated:** 1 session.

### 110.6.9 — Bench exit_timeout enforcement
- **Scope:** `bench/plan110/timeout_check.nv`.
- **Estimated:** 1 session.

### 110.6.10 — Bench MultiError compose
- **Status:** ✅ T11.7 fixture landed (3967dc25dec).

### 110.6.11 — Memory leak suite
- **Status:** ✅ T11.8 fixture landed (fcc7ab495a4).

## Plan 110.7 FFI Implementation (2 remaining)

### 110.7.2 — SQLite Connection example
- **Scope:** `examples/ffi_sqlite_consumable.nv` — full example showing
  pattern.
- **Files:** new example + minimal SQLite FFI wrapper.
- **Tests:** compile-only (no SQLite library в bootstrap).
- **Estimated:** 1 session.

### 110.7.3.a — #cancel_safe attribute parser
- **Scope:** Add `#cancel_safe` attribute recognition в parser; type-check
  W_FFI_CANCEL_UNSAFE warning at FFI call sites без attribute.
- **Files:** `compiler-codegen/src/lexer/token.rs` + parser + lints.
- **Tests:** `nova_tests/plan110/neg_ffi_cancel_unsafe.nv`.
- **Estimated:** 1 session.

## Plan 110.8 Finalize (5 remaining)

### 110.8.3 — Full regression
- **Scope:** Run `nova test` across all directories; document baseline
  ≥ pre-Plan 110.
- **Estimated:** 1 session (mostly time для test execution).

### 110.8.4 — Cross-platform CI
- **Scope:** Windows + Linux × clang + MSVC matrix; document any
  platform-specific issues.
- **Estimated:** 1 session (depends на CI infrastructure access).

### 110.8.5 — Performance baseline comparison
- **Scope:** Pre-Plan 110 bench comparison; document regression < +5%.
- **Estimated:** 1 session.

### 110.8.6 — Complete spec status flip
- **Status:** 🟡 5/13 done. Remaining D-blocks (D188/D191/D192/D193/
  D194/D195/D198) flip after corresponding runtime/impl land.
- **Estimated:** ½ session per flip + corresponding impl.

### 110.8.8 — Umbrella merge в main
- **Scope:** Final `git merge --no-ff plan-110 main` after все sub-sub
  closed.
- **Estimated:** ½ session.

## Production-Grade Acceptance per Sub-sub-sub

Each sub-sub-sub MUST:
1. Compile cleanly (release nova-cli build success).
2. Pass own positive test fixture(s) via release `nova test`.
3. Pass negative test fixture(s) emitting correct error codes.
4. Pass regression test suite (no induced failures).
5. Include acceptance criteria A110.X.Y.z.* with PASS/FAIL.
6. Update relevant spec D-block (or note staged delivery).
7. Update Q-block(s) если applicable.
8. Commit message documents scope + tests + acceptance + plan ref.

## Total Remaining

| Plan | Sub-sub-(sub) remaining | Wall-time estimate |
|---|---|---|
| 110.2 cancel-shield runtime | 3 (110.2.1.a + 110.2.2.a + 110.2.6) | 3 sessions |
| 110.4 effects runtime | 3 (110.4.4.a + 110.4.4.b + 110.4.6.a + 110.4.7 + 110.4.8) | 5 sessions |
| 110.6 LSP + bench | 4 (110.6.1.a + 110.6.5 + 110.6.8 + 110.6.9) | 4 sessions |
| 110.7 FFI impl | 2 (110.7.2 + 110.7.3.a) | 2 sessions |
| 110.8 finalize | 5 (110.8.3 + 110.8.4 + 110.8.5 + 110.8.6 complete + 110.8.8) | 5 sessions |
| **Total** | **17 sub-sub-(sub)** | **~19 sessions** |

Plus 110.3.3-5 stdlib (CancelScope/Channels/TCP/UDP) — depend on stdlib
types not existing yet ([M-110-stdlib-* follow-up plans]).

## Production-Grade Final Mandatory

Per Plan 110 header: финал Plan 110 = ВСЁ без упрощений. Все sub-sub-
(sub) MUST land до закрытия Plan 110 umbrella. Все [M-110-*] markers
либо closed либо extracted в independent plans. Никаких «good enough».
