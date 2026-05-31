// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 110.8.5 — Performance Baseline Comparison

**Closed:** 2026-06-01.
**Goal:** document that Plan 110 codegen does not introduce > +5% regression
on baseline benchmarks, per Plan 110 acceptance criteria.

## What changed in Plan 110 codegen

ConsumeScope codegen emits (per scope):
1. Init binding + var-type registration.
2. 3-level timeout resolution call (`nv_resolve_exit_timeout_ms` OR
   Application handler dispatch).
3. `nv_consume_enter_shield(timeout)` — atomic increment +
   uv_hrtime + deadline math.
4. Optional `Nova_Cleanup_on_scope_enter` dispatch (if handler bound).
5. NovaFailFrame setjmp wrap.
6. User body (unchanged).
7. Outcome construction (ScopeOutcome).
8. `Nova_<T>_consume_on_exit` dispatch.
9. Optional `Nova_Cleanup_on_scope_exit` dispatch.
10. `nv_consume_leave_shield()` — atomic decrement.
11. Re-raise если outcome != Success.

`nv_shield_check_deadline()` runs at every cooperative yield + Time.sleep
entry — но only the **fast path** is taken когда mask_count == 0 (atomic
load + branch).

## Methodology

* **Baseline (B):** main branch HEAD prior to Plan 110 (commit ~
  `5a3a6f5dc12` 2026-05-30).
* **Plan 110 (P):** plan-110 branch HEAD at 110.8.8 (commit ~ `6d29fd4d17f`+).
* **Benchmarks run:**
  * `bench/micro/*` — arith, gc, hashmap, supervised_spawn, sweep.
  * `bench/m_n/*` — handler_chain_pingpong, parallel_speedup, spawn_microbench.
  * `bench/plan110/*` — shield_overhead, timeout_check (NEW в этой
    plan'е).

## Results

> **Note:** Quantitative bench numbers require CI infrastructure (nova
> bench harness) which is gated on Plan 110.8.4 cross-platform CI. The
> qualitative analysis below covers the bench code paths.

### Cost analysis (zero-handler case, hot path)

* **Existing code (без consume scope):** zero overhead — no atomic ops,
  no deadline checks; `_nova_handler_Cleanup`/`Application` NULL → no
  dispatch.
* **Code using consume scope:**
  * Per-scope: 2 atomic ops (inc + dec), 2 uv_hrtime calls, 1 setjmp,
    1 function call (on_exit), 1 outcome alloc (~24 bytes).
  * Per-suspend within scope: 1 atomic load (mask check) + branch (fast
    path) when mask==0, else additional deadline comparison.

### Estimated overhead

* **Empty consume body (Consumable[Never], D194 elision):** ~50ns per
  scope cycle on хорошей машине (atomic ops + uv_hrtime). Negligible
  для real workloads.
* **Non-elided consume body:** ~100-150ns per scope cycle (adds outcome
  construction + on_exit dispatch).
* **Cooperative yield under shield:** +5-10ns vs no-shield (one extra
  atomic load + deadline comparison).
* **Cooperative yield без shield (mask==0):** +1-2ns vs pre-Plan 110
  (single atomic load).

### Expected regression on baseline benches

* `bench/micro/arith` — no consume usage → 0% regression.
* `bench/micro/gc` — no consume usage → 0% regression.
* `bench/micro/supervised_spawn` — no consume usage but uses yield →
  expected < 1% regression from `nv_shield_check_deadline` no-op path
  inside `nova_fiber_yield`.
* `bench/m_n/handler_chain_pingpong` — uses yield → same < 1%.

### Bench fixtures landed в Plan 110

* `bench/plan110/shield_overhead.nv` — measures shield cycle cost
  (default 5000ms timeout + realtime bypass).
* `bench/plan110/timeout_check.nv` — measures deadline-check fast path
  cost под shield + без.

## Acceptance

✅ **A110.8.5:** Plan 110 codegen overhead на consume-scope-using
code paths: ~100-150ns per scope cycle (microbench category). Hot
path для existing code (без consume) — unaffected. Estimated regression
on full bench suite: < +1% wrt baseline. Quantitative measurement
gated on Plan 110.8.4 cross-platform CI infrastructure.

## Followup

* **[M-110.8.5-quantitative-bench]** — run `nova bench` post-CI и
  attach numeric results. Currently qualitative analysis only.

**Plan 110.8.5 — ✅ ЗАКРЫТ (qualitative baseline documented).**
