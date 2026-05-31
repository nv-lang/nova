// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 110 — session-sized decomposition

> **Created 2026-05-31** в продолжение Plan 110 split на 110.1-110.8.
> Каждый sub-sub-plan ≈ **одна автономная сессия** (single deliverable,
> atomic merge, ≤ 1500 LOC, clear acceptance).
>
> **Цель:** разбить multi-day sub-plans на session-sized куски чтобы
> каждый можно было закрыть одним атомарным merge'ом без production-grade
> rule violation.

## Принципы декомпозиции

1. **Атомарность.** Каждая sub-sub-задача landing'уется одним merge'ом
   с working build + working tests. Никаких висящих TODO.
2. **Session-sized.** ≤ 1500 LOC net changes, ≤ 1 сессия Opus 4.7.
3. **Acceptance criteria.** Каждая задача имеет 1-3 чётких критерия
   приёма (PASS/FAIL).
4. **Dependencies.** Явно указаны predecessors. Где возможно — parallel.
5. **Tests inside.** Positive + negative тесты внутри той же задачи
   (не в отдельной).
6. **Spec atomic.** Если задача меняет D-блок status → меняет в этой же
   задаче.

## Total decomposition

**Plan 110.1** → 10 sub-sub-plans (110.1.1 — 110.1.10)
**Plan 110.2** → 6 sub-sub-plans
**Plan 110.3** → 6 sub-sub-plans (parallel-friendly)
**Plan 110.4** → 8 sub-sub-plans
**Plan 110.5** → 7 sub-sub-plans
**Plan 110.6** → 11 sub-sub-plans (parallel-friendly)
**Plan 110.7** → 3 sub-sub-plans
**Plan 110.8** → 8 sub-sub-plans

**Total: ≈ 59 session-sized tasks**. Wall-time с parallel splits ≈ 4-5 weeks
(serial = 12-15 weeks if one task/day).

---

## Plan 110.1 — Core compiler pipeline (10 sub-sub-plans)

> Атомарное правило: parser + checker + codegen + runtime + tests landing
> в одном merge — иначе нет working end-to-end. **Но** можно стратифицировать
> по фазам внутри Plan 110.1 — каждая фаза landing'уется когда тесты её
> покрытия PASS.

### 110.1.1 Parser + AST scaffold

**Scope:** lookahead `{` после `consume IDENT = EXPR`, добавить
`Stmt::ConsumeScope { binding, init, body }` AST variant. Старая raw
form `consume X = expr;` сохраняется.

**Files:**
- `compiler-codegen/src/ast/mod.rs` — `Stmt::ConsumeScope`.
- `compiler-codegen/src/parser/mod.rs` — extend `parse_consume_let`.

**Tests:**
- `nova_tests/plan110/parse_consume_scope_basic.nv` (positive).
- `nova_tests/plan110/parse_consume_scope_with_type_annot.nv` (positive).
- `nova_tests/plan110/neg_parse_consume_scope_missing_body.nv` (negative).
- `nova_tests/plan110/neg_parse_consume_scope_double_block.nv` (negative).

**Acceptance:**
- A110.1.1.a: `consume tx = init() { body }` парсится без error в AST.
- A110.1.1.b: `consume tx = init()` (raw) парсится как раньше (no regression).
- A110.1.1.c: 4 fixture'а PASS через release `nova check`.

**Dependencies:** Plan 110 Ф.0 ✅ (already landed).

**Estimated:** 1 session, ≤ 300 LOC Rust + 4 fixture'а.

---

### 110.1.2 Type-checker — D188 R1+R2 + D196 init type constraint

**Scope:** type-checker правила:
- D196: init expression должен resolve к типу implementing `Consumable[E]`
  (structural match). Поддерживаются: прямой Consumable, Result/Option
  unwrap через `?`/`!!`, conditional, method chain.
- D196: detect `Option[Consumable]` без unwrap → emit
  `D196-wrapped-init-needs-unwrap`.
- D196: detect divergent Consumable types в conditional →
  `D196-divergent-consumable`.
- D188 R1: tracked в type-checker — partial-construction safety (init
  errors не triggers `on_exit`).
- D188 R2: exactly-once tracked в codegen (110.1.5), здесь только note.

**Files:**
- `compiler-codegen/src/types/mod.rs` — check_consume_scope rule.

**Tests:**
- `nova_tests/plan110/check_consume_direct.nv` (positive).
- `nova_tests/plan110/check_consume_result_unwrap.nv` (positive).
- `nova_tests/plan110/check_consume_option_bang.nv` (positive).
- `nova_tests/plan110/check_consume_conditional.nv` (positive).
- `nova_tests/plan110/check_consume_method_chain.nv` (positive).
- `nova_tests/plan110/neg_consume_wrapped_no_unwrap.nv` (D196-wrapped).
- `nova_tests/plan110/neg_consume_divergent.nv` (D196-divergent).
- `nova_tests/plan110/neg_consume_not_consumable.nv` (D188-not-consumable).
- `nova_tests/plan110/neg_on_exit_malformed_sig.nv` (D188-malformed-on-exit).

**Acceptance:**
- A110.1.2.a: positive fixtures PASS type-check.
- A110.1.2.b: 4 negative fixtures emit correct error codes.

**Dependencies:** 110.1.1.

**Estimated:** 1 session, ≤ 400 LOC Rust + 9 fixture'ов.

---

### 110.1.3 Type-checker — D194 `Consumable[never]` + generic bound

**Scope:**
- Special-case: если binding имеет тип `Consumable[never]` (E = never bottom),
  caller'у не требуется `Fail[E]` declared.
- Generic constraint `[T Consumable[E]]` через Plan 101 generic bounds.
- Generic + Never special case: `[T Consumable[never]]` снимает требование
  `Fail[E]` у caller'а generic fn.

**Files:**
- `compiler-codegen/src/types/mod.rs` — Never special case + generic bound
  resolution.

**Tests:**
- `nova_tests/plan110/check_consume_never_no_fail.nv` (positive).
- `nova_tests/plan110/check_consume_generic_bound.nv` (positive).
- `nova_tests/plan110/check_consume_generic_never.nv` (positive).
- `nova_tests/plan110/neg_consume_never_missed_fail_decl.nv` (regression check).

**Acceptance:**
- A110.1.3.a: `Consumable[never]` permits caller без `Fail[E]`.
- A110.1.3.b: `[T Consumable[E]]` works для generic fns.
- A110.1.3.c: `[T Consumable[never]]` drops `Fail[E]` requirement.

**Dependencies:** 110.1.2.

**Estimated:** 1 session, ≤ 250 LOC Rust + 4 fixture'а.

---

### 110.1.4 Codegen — basic desugaring (sync, no shield/timeout)

**Декомпозиция на 8 sub-sub-sub-steps** (2026-05-31):

#### 110.1.4.a — Init binding emit + scope-block C structure

Emit C block `{ Nova_<Type> _consume_<binding>_<id> = <init expr>; ... }`.
Binding visible в body как alias. Без on_exit dispatch (заглушка emits
warning).

**Files:** `emit_c.rs` ConsumeScope branch.
**Tests:** `codegen_consume_init_only.nv` — verify init eval happens.
**Acceptance:** A110.1.4.a — init evaluates, binding accessible в body.
**Estimated:** 1 session, ≤ 150 LOC Rust + 1 fixture.

#### 110.1.4.b — Body emit + return value capture

Body block emit'ит C statements; trailing expr → return value of consume
scope. Без try/catch (still no fail handling).

**Tests:** `codegen_consume_body_value.nv` — body returns value через
trailing expr.
**Estimated:** 1 session, ≤ 100 LOC Rust + 1 fixture.

#### 110.1.4.c — Runtime ScopeOutcome sum-type registration

Register `ScopeOutcome` в `sum_schemas` (как Option/Result). Tags:
SUCCESS=0, FAILURE=1, PANIC=2. Payload: nova_str для Failure/Panic.

**Files:** `emit_c.rs` sum_schemas init + `nova_rt/cleanup.h` typedef.
**Tests:** `codegen_scope_outcome_construct.nv` — construct + match.
**Estimated:** 1 session, ≤ 100 LOC C + 80 LOC Rust + 1 fixture.

#### 110.1.4.d — Setjmp fail-frame for body try/catch

Wrap body in `setjmp(_consume_frame.jb) == 0 ? body : capture`. Capture
err_msg в local var. Pop fail-frame после body.

**Files:** `emit_c.rs` emit_consume_scope_body.
**Tests:** `codegen_consume_throw_caught.nv` — body throws → caught,
outcome=Failure.
**Estimated:** 1 session, ≤ 150 LOC Rust + 1 fixture.

#### 110.1.4.e — on_exit method dispatch via vtable

После body, emit call `Nova_<Type>_method_on_exit(&binding, outcome)`.
Method resolved через type_methods registry. Pass receiver pointer +
outcome value.

**Files:** `emit_c.rs` emit_consume_on_exit_call.
**Tests:** `codegen_consume_on_exit_dispatch.nv` — verify on_exit
called with correct outcome.
**Estimated:** 1 session, ≤ 100 LOC Rust + 1 fixture.

#### 110.1.4.f — Throw re-raise after on_exit на Failure

Если outcome был Failure: после `on_exit` — re-emit `nova_throw(err_msg)`.
Если Panic: `nv_panic(msg)`. Если on_exit сам throws — composes через
D193 MultiError (но MultiError refactor в 110.4 — здесь basic).

**Tests:** `codegen_consume_throw_reraise.nv` — body throws → on_exit
called → re-raise propagates.
**Estimated:** 1 session, ≤ 80 LOC Rust + 1 fixture.

#### 110.1.4.g — Panic propagation через on_exit

Panic в body → outcome=Panic → on_exit(Panic) → resume panic. Separate
из throw path.

**Tests:** `codegen_consume_panic.nv` — body panics → on_exit(Panic) →
panic propagates.
**Estimated:** 1 session, ≤ 60 LOC Rust + 1 fixture.

#### 110.1.4.h — Tests T2.1-T2.8 + NEG + close

Complete remaining T2.x positive tests + NEG-2.1-2.2 (exactly-once
double invocation prevention) + close 110.1.4.

**Tests:**
- `codegen_consume_success.nv` (T2 series).
- `codegen_consume_return.nv` (body returns value → success path).
- `neg_consume_double_on_exit.nv` (manual on_exit call inside body).

**Acceptance closure:**
- A110.1.4.a-h ✅ all 8 sub-steps landed.
- 8 fixtures PASS via release nova test.

**Estimated:** 1 session, ≤ 200 LOC Rust + 3 fixtures + closure docs.

---

**Total Plan 110.1.4:** 8 sub-sub-sub-steps × ~1 session each = ~8 sessions.
Each atomic merge. Production-grade staged delivery: каждый step
landing'уется с working tests, последующие добавляют функциональность
на coherent base.

---

### 110.1.5 Runtime — exactly-once + scope-stack

**Scope:** runtime functions:
- `nv_consume_enter(typeid, instance_ptr) -> consume_scope_t*` —
  push scope on per-fiber stack.
- `nv_consume_exit(scope, outcome_kind, error_ptr)` — call on_exit,
  pop scope, exactly-once invariant via counter (panic at ≥ 2).
- `nv_consume_drop_scope(scope)` — memory cleanup.

**Files:**
- `nova_rt/cleanup.h` + `nova_rt/cleanup.c` — runtime impl.
- `compiler-codegen/src/codegen/emit_c.rs` — emit runtime calls.

**Tests:**
- `nova_tests/plan110/runtime_consume_exactly_once.nv` — verify counter.
- `nova_tests/plan110/neg_runtime_consume_double_invocation.nv` — manual
  duplicate on_exit() → runtime panic D188-on-exit-double-invocation.
- `nova_tests/plan110/runtime_consume_partial_construction.nv` —
  init throws → on_exit NOT called (D188 R1).

**Acceptance:**
- A110.1.5.a: exactly-once invariant enforced (runtime counter).
- A110.1.5.b: partial-construction (init throws) → on_exit not called.
- A110.1.5.c: 3 fixture'а PASS.

**Dependencies:** 110.1.4.

**Estimated:** 1 session, ≤ 300 LOC C + 50 LOC Rust + 3 fixture'а.

---

### 110.1.6 LIFO composition + D162 simplified

**Scope:**
- Mixed `consume{}` + `defer{}` LIFO — shared scope-stack per fiber.
- D162 simplified rules: `consume X = ... { body }` exhaustive by
  construction, no D162-check для scope-bindings. Rules сохраняются для
  raw `consume + defer`.

**Files:**
- `compiler-codegen/src/types/mod.rs` — D162 amend.
- `nova_rt/cleanup.c` — extend scope-stack для mixed defer support.

**Tests:**
- `nova_tests/plan110/lifo_nested_consume.nv` — 3-level nesting, exit LIFO.
- `nova_tests/plan110/lifo_mixed_consume_defer.nv` — mixed LIFO.
- `nova_tests/plan110/d162_consume_scope_exhaustive.nv` — no D162-uncovered.
- `nova_tests/plan110/d162_raw_consume_still_checked.nv` — raw still needs cover.

**Acceptance:**
- A110.1.6.a: LIFO order correct для 3+ levels.
- A110.1.6.b: D162 simplified rules applied.

**Dependencies:** 110.1.5.

**Estimated:** 1 session, ≤ 200 LOC Rust + C + 4 fixture'а.

---

### 110.1.7 D194 hot-path elision codegen

**Scope:** codegen detect: `Consumable[never]` + no `WithExitTimeout`
satisfaction → strip shield/timeout/outcome construction. Emit `body;
on_exit_inline_call` напрямую.

**Files:**
- `compiler-codegen/src/codegen/emit_c.rs` — hot-path branch.

**Tests:**
- `nova_tests/plan110/d194_hot_path_mutex_consume.nv` — Consumable[never]
  + no WithExitTimeout → elided.
- `nova_tests/plan110/d194_no_elision_with_timeout.nv` — Consumable[never]
  + WithExitTimeout impl → no elision.
- `nova_tests/plan110/d194_no_elision_fallible.nv` — Consumable[E≠never] → no elision.
- Bench fixture: `bench/plan110/hot_path_overhead.nv` — disasm check (manual).

**Acceptance:**
- A110.1.7.a: Asm dump для elided case — no `nv_consume_enter` /
  `nv_resolve_exit_timeout` calls (codegen detect verified).
- A110.1.7.b: 3 fixture'а PASS.

**Dependencies:** 110.1.6.

**Estimated:** 1 session, ≤ 200 LOC Rust + 3 fixture'а + bench.

---

### 110.1.8 D197 cleanup re-entrance

**Scope:** nested `consume{}` inside `on_exit` body разрешено:
- inner scope inherits outer cancel-shield (Plan 110.2 dependency for
  full shield — здесь только структура, shield mock'нут);
- inner on_exit errors compose в локальный MultiError;
- depth limit 256 (D193) + sentinel `MultiErrorTruncated`.

**Files:**
- `nova_rt/cleanup.c` — re-entrance tracking + depth counter.
- `compiler-codegen/src/types/mod.rs` — allow ConsumeScope inside on_exit body.

**Tests:**
- `nova_tests/plan110/d197_nested_consume_in_on_exit.nv` — works.
- `nova_tests/plan110/d197_reentrance_depth_limit.nv` — 256 levels → truncation sentinel.
- `nova_tests/plan110/neg_d197_same_resource_reentrance.nv` — linear types prevent.

**Acceptance:**
- A110.1.8.a: nested consume{} inside on_exit works correctly.
- A110.1.8.b: depth limit 256 enforced + sentinel emitted.

**Dependencies:** 110.1.7.

**Estimated:** 1 session, ≤ 250 LOC C + Rust + 3 fixture'а.

---

### 110.1.9 Full positive + negative test suite

**Scope:** реализация всех T1.1-T1.8 + T2.1-T2.12 + NEG-1.1-1.5 + NEG-2.1-2.2
+ NEG-15.1 из Plan 110 §Tests. Многие уже покрыты в 110.1.1-8 — здесь
fill gaps + edge cases.

**Files:**
- `nova_tests/plan110/` — completion.

**Acceptance:**
- A110.1.9.a: все T1.x + T2.x PASS.
- A110.1.9.b: все NEG-1.x + NEG-2.x + NEG-15.1 emit correct error codes.

**Dependencies:** 110.1.8.

**Estimated:** 1 session, ≤ 600 LOC fixtures.

---

### 110.1.10 Plan 110.1 close — regression + merge

**Scope:**
- Full `nova test` ≥ baseline (1158/19 минимум).
- Plan 110.1 status table closure summary.
- Memory `project-plan110_1-status.md`.
- Branch merge в `main` (umbrella branch).

**Acceptance:**
- A110.1.10.a: zero regression на full nova test.
- A110.1.10.b: Plan 110.1 ✅ ЗАКРЫТ в status table.

**Dependencies:** 110.1.9.

**Estimated:** 1 session, integration + close.

---

## Plan 110.2 — Cancel-shield + 3-level timeout (6 sub-sub-plans)

### 110.2.1 Runtime — cancel-shield primitives

**Scope:**
- `nv_consume_enter_shield(deadline_ns)` — set `fiber->cancel_masked = true`,
  register deadline.
- `nv_consume_leave_shield()` — clear flag, deliver pending cancel.

**Files:** `nova_rt/cleanup.c`, `nova_rt/fiber.h`.

**Tests:**
- `nova_tests/plan110/shield_basic_mask.nv` — cancel во время on_exit body masked.
- `nova_tests/plan110/shield_deliver_after_exit.nv` — cancel re-raised after.

**Acceptance:**
- A110.2.1.a: cancel maskируется внутри shield, доставляется после leave.

**Dependencies:** Plan 110.1 ✅.

**Estimated:** 1 session.

---

### 110.2.2 Deadline check + CleanupTimeoutError

**Scope:** на каждой suspend-точке cleanup-body — check `now > deadline`
→ throw `CleanupTimeoutError`.

**Files:** `nova_rt/suspend.c` — extend suspend dispatch.

**Tests:**
- `nova_tests/plan110/cleanup_timeout_exceeded.nv` — long cleanup → CleanupTimeoutError.
- `nova_tests/plan110/cleanup_timeout_zero_with_suspend.nv` — D192-zero-timeout-suspend.
- `nova_tests/plan110/cleanup_timeout_negative.nv` — D192-negative-timeout panic.
- `nova_tests/plan110/cleanup_timeout_max_warning.nv` — D192-infinite-timeout-warn.

**Acceptance:**
- A110.2.2.a: 4 fixture'а PASS с correct error codes.

**Dependencies:** 110.2.1.

**Estimated:** 1 session.

---

### 110.2.3 Codegen — 3-level resolution emit

**Scope:** `nv_resolve_exit_timeout(typeid, instance_ptr)` runtime fn:
- Level 1: WithExitTimeout impl via vtable lookup.
- Level 2: Application effect handler call (если активен).
- Level 3: hardcoded 5000ms.

**Files:**
- `compiler-codegen/src/codegen/emit_c.rs` — emit `nv_resolve_exit_timeout`
  call at scope-entry.
- `nova_rt/cleanup.c` — implement `nv_resolve_exit_timeout`.

**Tests:**
- `nova_tests/plan110/timeout_level1_with_exit_timeout.nv`.
- `nova_tests/plan110/timeout_level2_application.nv`.
- `nova_tests/plan110/timeout_level3_hardcoded.nv`.
- `nova_tests/plan110/timeout_library_pattern_db_connect.nv`.

**Acceptance:**
- A110.2.3.a: 3-level fallback resolution correct.

**Dependencies:** 110.2.2; Plan 110.4.5 для Application (можно mock).

**Estimated:** 1 session.

---

### 110.2.4 D198 #realtime bypass codegen

**Scope:** codegen detect: enclosing fn `#realtime` → bypass
`nv_resolve_exit_timeout`, emit hardcoded `Duration.zero`.

**Files:**
- `compiler-codegen/src/codegen/emit_c.rs` — #realtime detection.

**Tests:**
- `nova_tests/plan110/d198_realtime_bypass.nv` — #realtime fn → zero.
- `nova_tests/plan110/neg_d198_realtime_app_override.nv` — D198 warning.
- `nova_tests/plan110/neg_d198_realtime_parking_op.nv` — E_REALTIME_SYNC_PARK.

**Acceptance:**
- A110.2.4.a: #realtime → zero timeout enforced; bypass 3-level.
- A110.2.4.b: D198-warning emitted при detect Application override.

**Dependencies:** 110.2.3.

**Estimated:** 1 session.

---

### 110.2.5 Cross-platform validation

**Scope:** validate cancel-shield на Windows (Plan 82 fiber-arena) + Linux
(libuv). MSVC + clang builds.

**Files:** test harness.

**Tests:**
- Stress fixture: `nova_tests/plan110/cross_platform_shield_stress.nv`
  (run на Win + Linux).

**Acceptance:**
- A110.2.5.a: Win + Linux × MSVC + clang PASS.

**Dependencies:** 110.2.4.

**Estimated:** 1 session.

---

### 110.2.6 Plan 110.2 close — tests T3 + merge

**Scope:** complete T3.1-T3.12 + NEG-3.1-3.4 + close 110.2.

**Acceptance:**
- A110.2.6.a: все T3.x + NEG-3.x PASS.
- A110.2.6.b: 110.2 ✅ ЗАКРЫТ.

**Estimated:** 1 session.

---

## Plan 110.3 — Stdlib migration (6 sub-sub-plans, parallel-friendly)

### 110.3.1 Mutex family — Consumable[never] impls

**Scope:** `MutexGuard`, `ReadGuard`, `WriteGuard`, `ReentrantGuard` impl
`Consumable[never]` (hot-path elision per D194).

**Files:** `std/runtime/sync.nv`.

**Tests:** T4.3 + bench verifying hot-path.

**Dependencies:** Plan 110.1.7 (hot-path codegen).

**Estimated:** 1 session.

### 110.3.2 Semaphore + CancelScope

**Scope:** `Permit` (Sem), `CancelScope` impl `Consumable[never]`.

**Files:** `std/runtime/sync.nv`, `std/runtime/cancel.nv`.

**Tests:** T4.4 + T4.6.

**Estimated:** 1 session.

### 110.3.3 Channels — Consumable impls

**Scope:** `ChanReader`, `ChanWriter`, `Channel` impl `Consumable[never]`.

**Files:** `std/runtime/channel.nv`.

**Tests:** T5.2.

**Estimated:** 1 session.

### 110.3.4 Networking — TCP/UDP Consumable[IoError]

**Scope:** `TcpStream`, `TcpListener`, `UdpSocket` impl
`Consumable[IoError]` с grace-close semantics.

**Files:** `std/runtime/net.nv`.

**Tests:** T5.1.

**Dependencies:** Plan 83.12 ✅.

**Estimated:** 1 session.

### 110.3.5 Connection pools (если applicable)

**Scope:** `ConnPool` + `PooledConn` impl `Consumable[ConnPoolError]`.

**Files:** `std/runtime/pool.nv` (если есть).

**Tests:** T5.5.

**Note:** если ConnPool не существует — extract в `[M-110-stdlib-pool]`.

**Estimated:** 1 session.

### 110.3.6 Plan 110.3 close — T4 + T5 + merge

**Scope:** complete T4.x + T5.x. Skip T4.1/T4.2/T4.5 (File/Transaction/
BufReader — extract в `[M-110-stdlib-fs/db/bufio]`).

**Estimated:** 1 session.

**Parallel-friendly:** 110.3.1-110.3.5 can run в 5 параллельных agent'ах.

---

## Plan 110.4 — Effects + MultiError (8 sub-sub-plans)

### 110.4.1 MultiError API refactor — walk + find_first_panic

**Scope:** добавить `@walk() -> Iter[any]`, `@fmt_chain() -> str`,
`@find_first_panic() -> Option[str]` к `std/prelude/errors.nv` MultiError.

**Files:** `std/prelude/errors.nv` + Rust dispatch для new methods.

**Tests:** T6.1 + T6.2 + T6.3.

**Dependencies:** Plan 110.1 ✅.

**Estimated:** 1 session.

### 110.4.2 Cycle-safety + depth-limit

**Scope:** `nv_compose_error` идентiti-check + depth limit 256 + sentinel
`MultiErrorTruncated` emission.

**Files:** `nova_rt/error.c` + Rust glue.

**Tests:** T6.4 + T6.5.

**Estimated:** 1 session.

### 110.4.3 Cleanup effect declaration + observability emit

**Scope:** declare `Cleanup` effect в `std/prelude/effects.nv`; codegen
emit `perform Cleanup.on_scope_enter/exit` если handler активен.

**Files:** `std/prelude/effects.nv` + `compiler-codegen/src/codegen/emit_c.rs`.

**Tests:** T7.1 + T7.2 + T7.3 (handler throw rejected).

**Estimated:** 1 session.

### 110.4.4 OpenTelemetry handler example

**Scope:** `examples/cleanup_tracing.nv` — reference OTel handler impl.
Spec D185 §otel implemented как library, не language feature.

**Files:** `examples/cleanup_tracing.nv` + `std/observability/otel_cleanup.nv` (new).

**Tests:** T7.4 + T7.5.

**Estimated:** 1 session.

### 110.4.5 Application effect declaration + register_finalizer

**Scope:** declare `effect Application` в stdlib + `ApplicationHandler`
type + `register_finalizer` + `default_exit_timeout_ms` operations.

**Files:** `std/runtime/application.nv` (new).

**Tests:** T8.1 + T8.2 + T8.3 + T8.4.

**Estimated:** 1 session.

### 110.4.6 Application nesting D195 R1-R5

**Scope:** inner handler wins; registry NOT inherited; default_exit_timeout
NOT inherited; test isolation use case.

**Files:** `std/runtime/application.nv` + integration с Plan 110.2.3.

**Tests:** T8.6 + T8.8.

**Dependencies:** 110.4.5; Plan 110.2.3.

**Estimated:** 1 session.

### 110.4.7 Application cross-fiber propagation D195 R6

**Scope:** spawn child fiber видит parent Application через effect-stack
snapshot (D80).

**Files:** `nova_rt/spawn.c` + `std/runtime/application.nv`.

**Tests:** T8.7.

**Dependencies:** 110.4.6.

**Estimated:** 1 session.

### 110.4.8 Plan 110.4 close — T6 + T7 + T8 + merge

**Scope:** complete tests + close.

**Estimated:** 1 session.

---

## Plan 110.5 — Auto-fix migration tool (7 sub-sub-plans)

### 110.5.1 Parser deprecation warnings (transition)

**Scope:** parser emit `W-d189-deprecated-{okdefer,errdefer,defer-result}`
для legacy forms (transitional, до Ф.5.6 hard-removal).

**Files:** `compiler-codegen/src/parser/mod.rs` + diagnostic codes.

**Tests:** 3 NEG-5.1-5.3 (warnings).

**Estimated:** 1 session.

### 110.5.2 Auto-fix Pattern 1 — consume + errdefer + okdefer

**Scope:** `nova fix --simplify-cleanup` Pattern 1 implementation:
`consume X = ...; errdefer { rollback }; okdefer { commit }` →
`consume X = ... { ... }`.

**Files:** `nova-cli/src/bin/nova-fix.rs` (new).

**Tests:** fixture coverage на plan100_4_3/errdefer_okdefer_exhaustive.nv migration.

**Estimated:** 1 session.

### 110.5.3 Auto-fix Pattern 2 — bare errdefer

**Scope:** `errdefer { x }` → `let mut done = false; defer { if !done { x } }`.

**Tests:** syntax/errdefer_basic.nv migration.

**Estimated:** 1 session.

### 110.5.4 Auto-fix Pattern 3 — defer |result|

**Scope:** `defer |r| match { ... }` → `consume X = ... { ... }` или
`with Cleanup = h { ... }`.

**Tests:** plan100_4_3/defer_with_result_logs_value.nv migration.

**Estimated:** 1 session.

### 110.5.5 Tool run on existing fixtures + manual review

**Scope:** `nova fix --simplify-cleanup nova_tests/` — apply to all 42
fixtures. Manual review impossible cases.

**Tests:** before/after diff verifying semantic preserved.

**Estimated:** 1 session.

### 110.5.6 D90 §7 codegen — cancel as Failure(CancelError)

**Scope:** runtime: cancel/interrupt в `consume {}` body → `Failure(CancelError {
reason })` payload в outcome.

**Files:** `nova_rt/cleanup.c` + cancel routing.

**Tests:** T9.5 + new fixture for cancel-as-failure.

**Estimated:** 1 session.

### 110.5.7 Hard removal — parser reject

**Scope:** удалить parser support для `okdefer`/`errdefer`/`defer |r|`.
Emit `D189-removed-*` errors. Remove `DeferWithResult` AST node + `DeferResult`
prelude type.

**Tests:** NEG-5.4.

**Estimated:** 1 session.

---

## Plan 110.6 — Diagnostics + LSP + stress + bench (11 sub-sub-plans, parallel-friendly)

### 110.6.1 Diagnostic codes + suggestions

**Scope:** все D188-/D189-/D192-/D193-/D198- codes с suggestions
(`D188-not-consumable`, `D188-malformed-on-exit`, etc).

**Files:** `compiler-codegen/src/diag.rs`.

**Tests:** T10.1-T10.5.

**Estimated:** 1 session.

### 110.6.2 LSP quick-fix — convert errdefer to consume{}

**Scope:** LSP code-action.

**Files:** `nova-lsp/src/quick_fix.rs`.

**Tests:** T10.6 (quick-fix subtest).

**Dependencies:** Plan 110.5 ✅.

**Estimated:** 1 session.

### 110.6.3 LSP hover info on consume{}

**Scope:** показать Consumable impl при hover.

**Files:** `nova-lsp/src/hover.rs`.

**Estimated:** 1 session.

### 110.6.4 LSP code-action — implement Consumable for type

**Scope:** скаффолд `Consumable[E]` impl при code-action.

**Estimated:** 1 session.

### 110.6.5 Stress — racing cancels

**Scope:** T11.1 (1000 fibers, 30s).

**Estimated:** 1 session.

### 110.6.6 Stress — nested 10 levels

**Scope:** T11.2.

**Estimated:** 1 session.

### 110.6.7 Stress — parallel for + consume{}

**Scope:** T11.3 + T11.4 (concurrent on_exit).

**Estimated:** 1 session.

### 110.6.8 Bench — cancel-shield + 3-level resolution

**Scope:** T11.5 (target ≤ Plan 100.4 baseline + 5%).

**Files:** `bench/plan110/cancel_shield_overhead.nv`.

**Estimated:** 1 session.

### 110.6.9 Bench — exit_timeout enforcement

**Scope:** T11.6 (target < 50ns).

**Estimated:** 1 session.

### 110.6.10 Bench — MultiError composition

**Scope:** T11.7 (depth 1, 10, 100).

**Estimated:** 1 session.

### 110.6.11 Memory leak suite

**Scope:** T11.8 (OOM, partial-construction, panic mid-cleanup).

**Estimated:** 1 session.

**Parallel-friendly:** 110.6.1-110.6.4 (LSP) + 110.6.5-110.6.7 (stress) +
110.6.8-110.6.10 (bench) можно гонять в 3 паралл. agent'ов.

---

## Plan 110.7 — FFI integration (3 sub-sub-plans)

### 110.7.1 Spec C-side cancellation-safety attestation

**Scope:** spec section в Plan 100.5 (cross-ref) — как C-side declares
cancel-safety; runtime check.

**Files:** spec/decisions/08-runtime.md update + Plan 100.5 amend.

**Estimated:** 1 session.

### 110.7.2 Example — SQLite Connection wrapper

**Scope:** `examples/ffi_sqlite_consumable.nv` — full impl.

**Files:** `examples/ffi_sqlite_consumable.nv` + supporting FFI bindings.

**Tests:** T12.1 + T12.3.

**Dependencies:** Plan 100.5 baseline.

**Estimated:** 1 session.

### 110.7.3 Cancel propagation through FFI

**Scope:** runtime: cancel-shield пробрасывается через FFI call только
если C-side `nv_ffi_cancel_safe` attestation.

**Files:** `nova_rt/ffi.c`.

**Tests:** T12.2.

**Estimated:** 1 session.

---

## Plan 110.8 — Finalize (8 sub-sub-plans)

### 110.8.1 7 remaining Q-blocks

**Scope:** Q-cancel-and-cleanup, Q-async-cleanup, Q-application-effect,
Q-hot-path-performance, Q-debugging-cleanup-chains, Q-perf-considerations,
Q-structural-extension-future (stub).

**Files:** `docs/idiom/*.md` (7 new).

**Dependencies:** Plan 110.1-110.4 (импл должна landing'нуться для
содержательного текста).

**Estimated:** 1 session.

### 110.8.2 Tutorial chapter

**Scope:** `docs/tutorial.md` cleanup chapter (если tutorial exists).

**Estimated:** 1 session.

### 110.8.3 Full regression run

**Scope:** `nova test` ≥ 1158/19; document результаты.

**Estimated:** 1 session.

### 110.8.4 Cross-platform CI

**Scope:** Windows + Linux × clang + MSVC matrix run.

**Estimated:** 1 session.

### 110.8.5 Performance baseline comparison

**Scope:** verify cancel-shield + 3-level + MultiError overhead ≤ baseline
+ 5%.

**Estimated:** 1 session.

### 110.8.6 Spec status flip proposed → active

**Scope:** D185 / D188-D198 / D195 status «proposed» → «active»;
D160 → «retracted»; D158/D161/D162/D90 §7 amends finalized.

**Files:** spec/decisions/03-syntax.md + 04-effects.md status fields.

**Estimated:** 1 session.

### 110.8.7 nova consume-analyze CLI update

**Scope:** extend `nova consume-analyze` (Plan 100.8) для Plan 110 dispatch
info: Consumable impl coverage, hot-path elision report, D198 warnings.

**Files:** `nova-cli/src/bin/nova-consume-analyze.rs`.

**Estimated:** 1 session.

### 110.8.8 Umbrella merge

**Scope:** merge всех Plan 110.x branches → main. Plan 110 umbrella ✅
ЗАКРЫТ. Memory + logs final update.

**Estimated:** 1 session.

---

## Dependency graph (high level)

```
Plan 110 Ф.0 ✅
        │
        ▼
Plan 110.1 (10 sub-sub) ──────┐
        │                      │
        ▼                      ▼
Plan 110.2 (6 sub-sub)    Plan 110.4 (8 sub-sub)
        │                      │
        ▼                      ▼
Plan 110.3 (6 sub-sub)    Plan 110.5 (7 sub-sub)
   [parallel]                  │
        │                      ▼
        ▼                  Plan 110.6 (11 sub-sub)
Plan 110.7 (3 sub-sub) ←──────┤   [parallel]
        │                      │
        ▼                      ▼
              Plan 110.8 (8 sub-sub) ──→ main merge
```

## Parallel windows

- После 110.1 ✅: можно запускать 110.2 + 110.3 + 110.4 + 110.5 +
  110.7 одновременно (5 параллельных agent'ов).
- Внутри 110.3: 5 parallel agent'ов (Mutex/Sem/Channels/TCP/Pool).
- Внутри 110.6: 3 parallel windows (LSP/stress/bench).

С полным parallel: wall-time **~3 недели** для всех 59 sub-sub-задач.
Sequential: **~3 месяца**.

## Status tracking

Каждый sub-sub-plan становится отдельным GitHub issue / Linear ticket с:
- Title: `Plan 110.X.Y — <scope>`.
- Body: scope + files + tests + acceptance + dependencies.
- Estimated: 1 session.
- Status: 🆕 / 🔵 in-progress / ✅ closed.
- Linked PR.

## Connection to existing memory

Memory `project-plan110-status.md` обновится при closure каждого Plan
110.X (не каждой sub-sub задачи — чтобы избежать memory churn).
Sub-sub статус — в Plan 110.X status table (каждый sub-plan имеет свой).

---

## Summary

| Sub-plan | sub-sub count | Est. sessions | Parallel-friendly |
|---|---|---|---|
| 110.1 Core compiler | 10 | 10 | partial (тесты можно параллельно) |
| 110.2 Cancel-shield + timeout | 6 | 6 | low |
| 110.3 Stdlib migration | 6 | 6 | **high** (5-way) |
| 110.4 Effects + MultiError | 8 | 8 | medium |
| 110.5 Auto-fix tool | 7 | 7 | low |
| 110.6 LSP + stress + bench | 11 | 11 | **high** (3-way) |
| 110.7 FFI integration | 3 | 3 | low |
| 110.8 Finalize | 8 | 8 | low |
| **Total** | **59** | **59 sessions** | mixed |

Wall-time:
- **Sequential** (1 session/working day): ~12 weeks.
- **Parallel optimal**: ~3 weeks.

Каждая sub-sub-задача production-grade (working build + working tests +
clear acceptance) при atomic merge.
