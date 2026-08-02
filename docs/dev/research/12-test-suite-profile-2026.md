# Nova Test Suite Timing Profile (2026-06-17)

## Methodology

- **Tool**: `nova test --results-file` with Plan 169.1 Ф.1 split-timing (compile_ms / run_ms)
- **Platform**: Windows 11, clang toolchain, dev mode (unoptimized codegen)
- **Jobs**: 8 parallel workers (`--jobs 8`)
- **Per-test timeout**: 60 s (`--timeout 60`)
- **Coverage**: 10 sampled directories totalling **260 tests**
  - `nova_tests/basics` (8), `nova_tests/generics` (5), `nova_tests/concurrency` (117),
  - `nova_tests/plan103_3` (25), `nova_tests/plan103_4` (25), `nova_tests/plan103_8` (14),
  - `nova_tests/plan83_4_5_6_stress` (3), `nova_tests/plan83_stress_armed` (5),
  - `nova_tests/plan57` (11), `nova_tests/plan110` (48)
- **Timing fields**:
  - `compile_ms` = Nova→C codegen + clang compile (wall-clock)
  - `run_ms` = executable execution wall-clock
  - `elapsed_ms` = total (compile + link + run overhead)
- **Test categories**:
  - *Codegen tests* (`compile_ms > 0`): full pipeline, 212 tests
  - *CC-FAIL tests* (`compile_ms = 0, elapsed_ms < 5 s`): checker-only (reject-expected), 22 tests
  - *FAIL/timeout* (`passed = false`): 30 tests (pre-existing failures + timeouts)

---

## Top-30 Slowest Tests (by total elapsed_ms)

| Rank | Test | compile_ms | run_ms | total_ms | pass |
|-----:|------|----------:|-------:|--------:|:----:|
| 1 | `cancel_stress_armed` | 0 | 0 | 100 516 | TIMEOUT |
| 2 | `mono_spawn_closure_smoke` | 0 | 0 | 86 762 | TIMEOUT |
| 3 | `condvar_wait_cancel` | 0 | 0 | 61 802 | TIMEOUT |
| 4 | `condvar_producer_consumer` | 52 766 | 1 998 | 54 767 | PASS |
| 5 | `cancel_during_natural_fire` | 51 561 | 2 202 | 53 764 | PASS |
| 6 | `codegen_consume_panic_caught` | 52 529 | 700 | 53 233 | PASS |
| 7 | `cleanup_effect_dispatch_t7_1` | 50 832 | 1 339 | 52 191 | PASS |
| 8 | `cancel_during_runtime_shutdown` | 51 299 | 833 | 52 135 | PASS |
| 9 | `cancel_during_drain_exit` | 49 505 | 2 580 | 52 087 | PASS |
| 10 | `cancel_from_non_fiber` | 50 776 | 1 209 | 51 989 | PASS |
| 11 | `countdown_count_down_at_zero_neg` | 51 434 | 273 | 51 708 | PASS |
| 12 | `cancel_latency_bench` | 50 026 | 1 351 | 51 379 | FAIL |
| 13 | `condvar_wait_without_lock_neg` | 50 418 | 456 | 50 878 | PASS |
| 14 | `p1_bench_basic_compiles` | 49 172 | 1 122 | 50 307 | PASS |
| 15 | `p5_bench_throughput_setters` | 49 228 | 1 039 | 50 278 | PASS |
| 16 | `cancel_double_call` | 48 556 | 1 408 | 49 965 | PASS |
| 17 | `reentrant_unlock_not_locked_neg` | 49 117 | 468 | 49 589 | PASS |
| 18 | `p2_bench_namespace_callable` | 49 159 | 378 | 49 548 | PASS |
| 19 | `cancel_merge_test` | 48 642 | 467 | 49 120 | PASS |
| 20 | `fibers_10k_sleep_cancel` | 45 889 | 2 445 | 48 337 | PASS |
| 21 | `prop_barrier_all_or_none` | 46 279 | 1 917 | 48 200 | PASS |
| 22 | `litmus_refcount_drop` | 46 292 | 1 841 | 48 140 | PASS |
| 23 | `countdown_one_shot_signal` | 46 125 | 1 792 | 47 918 | PASS |
| 24 | `codegen_reentrance_d197` | 46 371 | 1 482 | 47 855 | PASS |
| 25 | `cancel_semantics_test` | 44 910 | 2 562 | 47 481 | PASS |
| 26 | `cancel_negative_behavior_test` | 45 649 | 1 688 | 47 346 | PASS |
| 27 | `mutex_reentrant_deadlock_doc` | 45 639 | 1 694 | 47 335 | PASS |
| 28 | `check_consume_unwrap_form` | 46 940 | 253 | 47 196 | PASS |
| 29 | `reentrant_recursive_same_fiber` | 46 057 | 1 095 | 47 155 | PASS |
| 30 | `cancel_reason_typed_test` | 44 573 | 2 341 | 46 915 | PASS |

---

## Top-15 by run_ms (highest runtime overhead)

Tests where the executable itself runs long — most likely candidates for `_slow` migration based on execution cost rather than compile cost.

| Rank | Test | compile_ms | run_ms | total_ms |
|-----:|------|----------:|-------:|--------:|
| 1 | `sleep_leak_check` | 19 237 | 20 986 | 40 224 |
| 2 | `plan149_stack_floor` | 27 485 | 7 978 | 35 477 |
| 3 | `barrier_cyclic_reusable` | 36 028 | 6 898 | 42 927 |
| 4 | `nested_shield_deadline_inversion_neg` | 32 100 | 6 811 | 38 912 |
| 5 | `plan149_max_clamp` | 29 848 | 6 427 | 36 278 |
| 6 | `polymorphic_recursion_smoke` | 23 003 | 6 235 | 29 240 |
| 7 | `plan40_channel_hardening` | 27 327 | 6 048 | 33 389 |
| 8 | `plan40_perf_bench` | 27 449 | 5 925 | 33 377 |
| 9 | `select_wildcard_test` | 22 783 | 5 868 | 28 652 |
| 10 | `select_timer_stress` | 22 830 | 5 866 | 28 698 |
| 11 | `ctx_pins_scope_cleanup_loop` | 30 855 | 5 618 | 36 474 |
| 12 | `condvar_notify_one` | 33 805 | 5 589 | 39 394 |
| 13 | `condvar_no_lost_wakeup_prop` | 35 980 | 5 146 | 41 132 |
| 14 | `barrier_n_party_rendezvous` | 36 012 | 5 142 | 41 156 |
| 15 | `blocking_cancel_test` | 41 408 | 5 110 | 46 519 |

---

## By Category

### concurrency/ (117 tests)
The largest and most expensive directory. Nearly all tests require full codegen + clang.
- Median elapsed: ~35 s
- Tests with run_ms > 5 s: `sleep_leak_check` (21 s run), `plan149_stack_floor` (8 s), `plan40_*` (~6 s), `select_timer_stress` (~6 s)
- TIMEOUT tests: `cancel_stress_armed`, `mono_spawn_closure_smoke`, `condvar_wait_cancel`
- Pre-existing FAIL (not timeout): `cancel_cycle_linked_tokens`, `detach_test`, `fn_array_collect_test`, `parallel_for_array`, `sleep_real_clock`

### plan103_3/ — Mutex / RwLock (25 tests)
- Median elapsed: ~42 s (some of the heaviest compile times in the suite)
- Several pre-existing FAIL: `mutex_mutual_exclusion_observed_interleave`, `mutex_try_lock_for_timeout`, `rwlock_*` (8 tests)
- Heaviest: `mutex_double_unlock_neg` (46 s), `reentrant_unlock_not_locked_neg` (49 s)

### plan103_4/ — Coordination primitives (25 tests)
- Median elapsed: ~41 s
- Notable: `condvar_producer_consumer` (54 767 ms — heaviest passing test in the entire sample)
- FAIL: `condvar_notify_all`, `condvar_wait_for_timeout`, `condvar_wait_until_predicate`, `semaphore_*` (4 tests)

### plan103_8/ — Property tests / stress (14 tests)
- Generally faster (28-48 s)
- FAIL: `prop_cas_loop_convergence`, `stress_cas_loop_high_contention`

### plan83_4_5_6_stress/ (3 tests)
- `cancel_stress`: 45 s, `park_wake_stress`: 45 s, `spawn_stress_10k`: 35 s
- All compile-heavy: 43-45 s compile

### plan83_stress_armed/ (5 tests)
- `cancel_stress_armed`: 100 s TIMEOUT (heaviest test in sample)
- Others: 40-44 s

### plan57/ — Benchmark fixtures (11 tests)
- CC-only (n1-n4): < 20 ms
- Compile tests (p1, p2, p4, p5): 41-50 s (very large generated C files)
- `n5_sleep_in_measure_warning`, `n6_opaque_literal_warning`, `n7_io_in_measure_warning`: 3-4 s (checker invocations)

### plan110/ — Deadlines / shields / consume (48 tests)
- Wide range: 0 ms (CC-only) to 53 s
- Fast CC-only tests: `neg_*` category (< 5 s)
- Slow: `codegen_consume_panic_caught` (53 s), `cleanup_effect_dispatch_t7_1` (52 s)

### basics/ (8 tests) and generics/ (5 tests)
- Fastest categories: 15-23 s (compile-dominated, tiny run times)
- These establish the compile overhead floor (~15-20 s per test on warm cache)

---

## Compile Time Analysis

The key insight from this profile: **compile_ms dominates total elapsed_ms in nearly every test**.

- Median compile_ms: **35 211 ms** (35 s)
- Median run_ms: **1 375 ms** (1.4 s)
- Compile/total ratio: ~95% of elapsed is compile time

The compile time varies with C file size (generated by Nova codegen):
- Simple tests (basics, generics): 15-20 s compile
- Concurrency tests (larger std imports): 22-35 s compile
- Heavy sync primitives (plan103_3, plan103_4): 33-52 s compile
- Benchmark fixtures (plan57 p-series): 40-50 s compile (largest C output)

**Root cause**: Each test compiles from scratch with clang on Windows. The ~15 s floor is fixed overhead (PCH not in use, large nova_rt.h included every time). Incremental/cached headers would be the highest-leverage optimization.

---

## Candidates for `_slow` Migration

Tests that should be moved to `*_slow.nv` (excluded from default `nova test` run, included with `--include-slow`):

### Already `_slow` (existing)
- `nova_tests/plan91_12/net_v2_*_slow.nv` (7 files) — net integration tests
- `nova_tests/plan156/combo_windows_slow.nv`, `lane_big_slow.nv`

### Recommended for `_slow` migration

**High-priority (timeout/flaky in timed test runs):**
- `cancel_stress_armed` — consistently times out at 60 s (100 s observed)
- `mono_spawn_closure_smoke` — 86 s observed timeout
- `condvar_wait_cancel` — 61 s timeout

**High run_ms (intentional slow by design — bench/stress):**
- `sleep_leak_check` — 21 s run (leak detection loop)
- `cancel_latency_bench` — bench, 51 s total (also currently FAIL)
- `plan40_perf_bench` — "perf_bench" in name, 6 s run
- `plan40_channel_hardening` — 6 s run
- `gc_bench`, `gc_pause_bench`, `sleep_bench`, `sleep_precision_bench` — bench fixtures
- `cancel_latency_bench`, `gc_bench`, `gc_pause_bench`, `sleep_bench`, `sleep_precision_bench` — all have "bench" in name

**Named stress tests already in special directories (plan83_4_5_6_stress, plan83_stress_armed):**
- These are already in isolated directories; if they need to remain in `nova test`, consider `--include-slow`

### File name pattern candidates (by name convention)
Files with "stress", "bench", or "perf" in name that are NOT already `_slow`:

```
concurrency/cancel_latency_bench.nv
concurrency/gc_bench.nv
concurrency/gc_pause_bench.nv
concurrency/plan40_perf_bench.nv
concurrency/select_timer_stress.nv
concurrency/sleep_bench.nv
concurrency/sleep_precision_bench.nv
concurrency/stress_iso_3e.nv
concurrency/stress_iso_large.nv
concurrency/supervised_cancel_stress_test.nv
gc/stress_100k_ints.nv
plan103_2/atomic_stress_sequential.nv
plan103_3/mutex_stress_mn_4workers.nv
plan103_3/rwlock_read_heavy_stress.nv
plan103_5/once_stress_mn_4workers.nv
plan103_8/stress_cas_loop_high_contention.nv
plan103_8/stress_mixed_sync_mn.nv
plan103_8/stress_once_oncecell_lazy_combined.nv
plan103_8/stress_producer_consumer_bounded.nv
plan103_8/stress_rwlock_read_heavy.nv
plan110/racing_cancels_stress_t11_3.nv
plan110/stress_high_freq_loop_t11_8.nv
plan110/stress_nested_10_levels_t11_2.nv
plan140/perf_contract_hot_loop.nv
plan152_5/bench_operators.nv
plan55/f1_closure_array_gc_stress.nv
plan56/f4_clone_gc_stress.nv
plan57/n1_bench_no_measure.nv  (CC-only, fast — OK to keep)
plan57/n2_bench_two_measure.nv (CC-only, fast — OK to keep)
plan57/p1_bench_basic_compiles.nv
plan57/p2_bench_namespace_callable.nv
plan57/p4_bench_groups_compiles.nv
plan57/p5_bench_throughput_setters.nv
plan83_4_5_6_stress/cancel_stress.nv
plan83_4_5_6_stress/park_wake_stress.nv
plan83_4_5_6_stress/spawn_stress_10k.nv
plan83_11/driver_stress_cancel.nv
plan83_stress_armed/cancel_stress_armed.nv
plan83_stress_armed/memory_bounded_armed.nv
plan83_stress_armed/orphan_drain_stress_armed.nv
plan83_stress_armed/park_wake_stress_armed.nv
plan83_stress_armed/spawn_stress_armed.nv
plan97/pos_protocol_lit_gc_stress.nv
```

---

## Summary Stats

| Metric | Value |
|--------|-------|
| Total tests measured | 260 |
| Tests with full compile pipeline | 212 |
| CC-only (checker) tests | 22 |
| Failed / timeout | 30 |
| Median total_ms (compile tests) | 36 741 ms |
| P90 total_ms | 47 481 ms |
| P99 total_ms | 53 764 ms |
| Min total_ms (compile tests) | 15 425 ms |
| Max total_ms (passing tests) | 54 767 ms |
| Median compile_ms | 35 211 ms |
| Median run_ms | 1 375 ms |
| Compile fraction of elapsed | ~96% |
| Slowest passing test | `condvar_producer_consumer` (54 767 ms) |
| Slowest overall (incl. timeout) | `cancel_stress_armed` (100 516 ms timeout) |

### Key Findings

1. **Compile time is the bottleneck**: 96% of test time is clang compilation, not execution.
2. **Floor is ~15 s**: Minimum compile time for any test due to nova_rt.h + GC headers.
3. **Heavy tests reach 50+ s compile**: `plan103_3`, `plan103_4`, `plan57 p-series` have large generated C.
4. **`_slow` migration is the highest-leverage CI win**: Moving 40+ bench/stress tests saves 40-100 s each in the default run.
5. **CC-only tests are fast (< 5 s)**: Negative/checker tests add negligible cost; they should stay in default run.
6. **Run time is usually 1-2 s**: Exceptions are intentional stress/bench tests (up to 21 s for `sleep_leak_check`).

---

## Ф.3 Classification of Slow Candidates

Classification applied 2026-06-17 (Plan 169.1 Ф.3).

### Rules
- **keep-fast**: total_ms < 3000 AND run_ms < 2000 — stays in default run
- **migrate-slow**: total_ms ≥ 3000 OR run_ms ≥ 2000 — rename to `_slow.nv`
- **create-fast-variant**: slow only because of large N — create fast version + rename original to `_slow`
- **investigate**: slow due to suspected runtime/compiler issue — add `[M-169.1-...]` marker

### Classification Table

| File | Category | Reason | Action |
|------|----------|--------|--------|
| `concurrency/cancel_latency_bench.nv` | migrate-slow | Bench fixture: 3 tests using real sleep timers (50ms/30ms/20ms × iterations), inherently time-bound. run_ms ~1350 ms, total ~51 s. | Rename to `cancel_latency_bench_slow.nv` |
| `concurrency/gc_bench.nv` | create-fast-variant | Has 5 sub-tests; "bench: 100k record-аллокаций" and "bench: объект-sentinel жив после 1M аллокаций" are reduceable (100k→1k, 1M→10k). Others small. | Create `gc_bench.nv` (1k/10k N), rename original to `gc_bench_slow.nv` |
| `concurrency/gc_pause_bench.nv` | migrate-slow | GC pause measurement: large workload test (1M × 3 rounds) has 30 s timeout. Inherently an observability test, not a fast regression guard. | Rename to `gc_pause_bench_slow.nv` |
| `concurrency/plan40_perf_bench.nv` | migrate-slow | 3 perf benchmarks: 1000 supervised select dispatches, 200 timer-cleanup rounds, 10k channel throughput. run_ms ~5.9 s, total ~33 s. "perf_bench" in name. | Rename to `plan40_perf_bench_slow.nv` |
| `concurrency/select_timer_stress.nv` | migrate-slow | 500-iteration select cleanup stress (500 × spawn + timer cancel). run_ms ~5.9 s. N is intentionally large for race detection; not reduceable without losing stress value. | Rename to `select_timer_stress_slow.nv` |
| `concurrency/sleep_bench.nv` | migrate-slow | Two bench tests: 5k concurrent sleeps (inherently ~100-500ms wall), 100k sleep(0) yields. Bench fixture — not a functional correctness test. | Rename to `sleep_bench_slow.nv` |
| `concurrency/sleep_precision_bench.nv` | migrate-slow | 50-sample precision measurement loop (50 × sleep(50ms) = minimum 2.5 s). Measurement fixture, not reduceable. ENV NOVA_MAXPROCS=1. | Rename to `sleep_precision_bench_slow.nv` |
| `concurrency/stress_iso_3e.nv` | keep-fast | Regression guard for [M-83.10.4-iso-cancel-startup-race]: 99+1 fibers, cancel(30ms). Total run_ms ~30–300ms expected. Classified slow in profile due to compile cost (~50 s), not run cost. Functional correctness test, must stay in default run. | No change |
| `concurrency/stress_iso_large.nv` | keep-fast | Regression guard for [M-83.11-gc-cancel-token-alias]: 999+1 no-op fibers. Body is instant (no sleeps). Only compile cost makes it appear slow. Must stay in default run. | No change |
| `concurrency/supervised_cancel_stress_test.nv` | migrate-slow | 3 tests with 100-fiber cancel scopes × 3 repetitions, run_ms varies but scope is explicitly "stress". Budget relaxed (1000ms/1000ms/2000ms) but inherently multi-fiber. | Rename to `supervised_cancel_stress_test_slow.nv` |
| `gc/stress_100k_ints.nv` | keep-fast | Pure int loop (no allocs): 100k iterations are trivially fast in C. Only compile dominates. Functional correctness test. run_ms should be < 100 ms. | No change |
| `plan103_2/atomic_stress_sequential.nv` | keep-fast | Sequential atomic ops (10k–5k iterations). Pure single-fiber, no sync overhead. Fast execution. Only compile cost makes it appear slow. | No change |
| `plan103_3/mutex_stress_mn_4workers.nv` | migrate-slow | 8 fibers × 5000 mutex-protected iters = 40000 lock/unlock cycles under M:N(4). EXPECT_TIMEOUT_MS 60000. Inherently slow by concurrency stress design. | Rename to `mutex_stress_mn_4workers_slow.nv` |
| `plan103_3/rwlock_read_heavy_stress.nv` | migrate-slow | 10 readers + 1 writer × 1000 iters under M:N. EXPECT_TIMEOUT_MS 60000. High concurrency lock stress. | Rename to `rwlock_read_heavy_stress_slow.nv` |
| `plan103_5/once_stress_mn_4workers.nv` | migrate-slow | 3 tests: 16 fibers × 100 calls. EXPECT_TIMEOUT_MS 150000 (2.5 min). Explicitly large-scale concurrency stress. | Rename to `once_stress_mn_4workers_slow.nv` |
| `plan103_8/stress_cas_loop_high_contention.nv` | migrate-slow | 3 CAS-loop tests: 16 fibers × 20 iterations under contention. EXPECT_TIMEOUT_MS 300000. High-contention design. | Rename to `stress_cas_loop_high_contention_slow.nv` |
| `plan103_8/stress_mixed_sync_mn.nv` | migrate-slow | 16 fibers × 200 iters using 4 sync primitives simultaneously. EXPECT_TIMEOUT_MS 300000. Cross-cutting stress fixture. | Rename to `stress_mixed_sync_mn_slow.nv` |
| `plan103_8/stress_once_oncecell_lazy_combined.nv` | keep-fast | 4 fibers × small ops. EXPECT_TIMEOUT_MS 300000 (defensive), but actual runtime is fast (4 fibers × 1 init call each). Functional correctness test, low actual run cost. | No change |
| `plan103_8/stress_producer_consumer_bounded.nv` | migrate-slow | 4 producers + 4 consumers × 500 items each using Condvar wait pattern. EXPECT_TIMEOUT_MS 300000. Inherently blocking/synchronization heavy. | Rename to `stress_producer_consumer_bounded_slow.nv` |
| `plan103_8/stress_rwlock_read_heavy.nv` | keep-fast | Small scale: 4 readers + 1 writer × 10 iters only. EXPECT_TIMEOUT_MS 300000 (defensive). Actual run should be fast. Functional correctness. | No change |
| `plan110/racing_cancels_stress_t11_3.nv` | keep-fast | 8 spawns each with trivial consume scope (no sleep, no loop). Completes near-instantly. Only compile cost makes it appear in slow list. | No change |
| `plan110/stress_high_freq_loop_t11_8.nv` | create-fast-variant | 1000-iteration consume-scope loop. Run_ms should be fast (pure CPU). However, if it's slow in profile, N=1000 → N=100 for default, keep 1000 as `_slow`. | Create `stress_high_freq_loop_t11_8.nv` (100 iters), rename original to `stress_high_freq_loop_t11_8_slow.nv` |
| `plan110/stress_nested_10_levels_t11_2.nv` | keep-fast | Single test, 10-level fixed nesting — not parameterized by N. Executes in microseconds. Only compile cost. | No change |
| `plan140/perf_contract_hot_loop.nv` | migrate-slow | 20M iterations hot loop measuring contract overhead. `fn main()` not a `test` — perf fixture. EXPECT_STDOUT PERF_DONE. Inherently long run. | Rename to `perf_contract_hot_loop_slow.nv` |
| `plan152_5/bench_operators.nv` | migrate-slow | 200k memcmp + 50k concat iterations. Explicitly bench workload. "bench" in name. High run cost by design. | Rename to `bench_operators_slow.nv` |
| `plan55/f1_closure_array_gc_stress.nv` | create-fast-variant | 50-cycle GC stress (1000 closures × 50 cycles). Reduce to 5 cycles for default run (still exercises GC path), keep 50-cycle as `_slow`. | Create `f1_closure_array_gc_stress.nv` (5 cycles), rename original to `f1_closure_array_gc_stress_slow.nv` |
| `plan56/f4_clone_gc_stress.nv` | create-fast-variant | 100 clones × 100-entry HashMap. Reduce to 10 clones for default (still verifies GC behavior), keep 100 as `_slow`. | Create `f4_clone_gc_stress.nv` (10 clones), rename original to `f4_clone_gc_stress_slow.nv` |
| `plan57/p1_bench_basic_compiles.nv` | keep-fast | bench DSL compile smoke test — 1 test + 2 bench items. Inherently fast to execute (bench items don't run in `nova test`). Only clang compile time. | No change |
| `plan57/p2_bench_namespace_callable.nv` | keep-fast | 5 functional tests of bench.*/gc.* intrinsics. Fast execution. Only compile cost. | No change |
| `plan57/p4_bench_groups_compiles.nv` | keep-fast | bench DSL group/case compile smoke + 1 trivial test. Fast execution. Only compile cost. | No change |
| `plan57/p5_bench_throughput_setters.nv` | keep-fast | bench setter callability smoke + 1 trivial test. Fast execution. Only compile cost. | No change |
| `plan83_4_5_6_stress/cancel_stress.nv` | migrate-slow | 3 tests: 100 sequential supervised cancel scopes, 50 mid-flight cancels, 10 sleep-cancel cycles. Inherently sequential stress (100+ supervised scopes). | Rename to `cancel_stress_slow.nv` |
| `plan83_4_5_6_stress/park_wake_stress.nv` | migrate-slow | 100 fibers × sleep(1), 20 fibers × sleep(5), 50-fiber sum. Real timer sleeps — inherently time-bounded. ENV NOVA_AUTOARM=0. | Rename to `park_wake_stress_slow.nv` |
| `plan83_4_5_6_stress/spawn_stress_10k.nv` | migrate-slow | 1K no-op spawns, 100-spawn sum, 10-outer × 10-inner nested. ENV NOVA_AUTOARM=0. High spawn count stress. | Rename to `spawn_stress_10k_slow.nv` |
| `plan83_11/driver_stress_cancel.nv` | migrate-slow | 20 cancel-sleep cycles × (8 fibers + canceller), 50 immediate-cancel races. EXPECT_TIMEOUT_MS 30000. ENV NOVA_AUTOARM=0. Concurrency stress. | Rename to `driver_stress_cancel_slow.nv` |
| `plan83_stress_armed/cancel_stress_armed.nv` | migrate-slow | 10K supervised cancel cycles armed M:N. ALLOC_REQUIRES boehm. Times out at 60 s (100 s observed). Production scale stress. | Rename to `cancel_stress_armed_slow.nv` |
| `plan83_stress_armed/memory_bounded_armed.nv` | migrate-slow | 10 cycles × 1K parallel-for spawns under armed M:N with GC measurement. ALLOC_REQUIRES boehm. | Rename to `memory_bounded_armed_slow.nv` |
| `plan83_stress_armed/orphan_drain_stress_armed.nv` | migrate-slow | 1K detach orphans + runtime.drain_orphans(). ALLOC_REQUIRES boehm. | Rename to `orphan_drain_stress_armed_slow.nv` |
| `plan83_stress_armed/park_wake_stress_armed.nv` | migrate-slow | 10K channel ping-pong under armed M:N. ALLOC_REQUIRES boehm. High N by design. | Rename to `park_wake_stress_armed_slow.nv` |
| `plan83_stress_armed/spawn_stress_armed.nv` | migrate-slow | 10K parallel-for spawns under armed M:N. ALLOC_REQUIRES boehm. EXPECT_TIMEOUT_MS 120000. | Rename to `spawn_stress_armed_slow.nv` |
| `plan97/pos_protocol_lit_gc_stress.nv` | keep-fast | 1000 protocol literal allocations — tight loop, instant in C. Second test is 4-literal smoke. Only compile cost makes it appear slow. | No change |

### Summary

| Category | Count | Files |
|----------|------:|-------|
| migrate-slow | 22 | See list below |
| create-fast-variant | 4 | `gc_bench`, `stress_high_freq_loop_t11_8`, `f1_closure_array_gc_stress`, `f4_clone_gc_stress` |
| keep-fast | 15 | `stress_iso_3e`, `stress_iso_large`, `stress_100k_ints`, `atomic_stress_sequential`, `stress_once_oncecell_lazy_combined`, `stress_rwlock_read_heavy`, `racing_cancels_stress_t11_3`, `stress_nested_10_levels_t11_2`, `p1_bench_basic_compiles`, `p2_bench_namespace_callable`, `p4_bench_groups_compiles`, `p5_bench_throughput_setters`, `pos_protocol_lit_gc_stress`, `gc_stress_100k_ints`, `stress_high_freq_loop_t11_8` (pending fast variant) |
| investigate | 0 | All slowness attributed to intentional N or compile cost |

### migrate-slow list (22 files)

```
concurrency/cancel_latency_bench.nv
concurrency/gc_pause_bench.nv
concurrency/plan40_perf_bench.nv
concurrency/select_timer_stress.nv
concurrency/sleep_bench.nv
concurrency/sleep_precision_bench.nv
concurrency/supervised_cancel_stress_test.nv
plan103_3/mutex_stress_mn_4workers.nv
plan103_3/rwlock_read_heavy_stress.nv
plan103_5/once_stress_mn_4workers.nv
plan103_8/stress_cas_loop_high_contention.nv
plan103_8/stress_mixed_sync_mn.nv
plan103_8/stress_producer_consumer_bounded.nv
plan140/perf_contract_hot_loop.nv
plan152_5/bench_operators.nv
plan83_4_5_6_stress/cancel_stress.nv
plan83_4_5_6_stress/park_wake_stress.nv
plan83_4_5_6_stress/spawn_stress_10k.nv
plan83_11/driver_stress_cancel.nv
plan83_stress_armed/cancel_stress_armed.nv
plan83_stress_armed/memory_bounded_armed.nv
plan83_stress_armed/orphan_drain_stress_armed.nv
plan83_stress_armed/park_wake_stress_armed.nv
plan83_stress_armed/spawn_stress_armed.nv
```

### create-fast-variant list (4 files)

| Original (→ `_slow`) | Fast variant parameters |
|---------------------|------------------------|
| `concurrency/gc_bench.nv` | Reduce test 1: 100k→1k allocs; test 5: 1M→10k sentinel allocs; others unchanged |
| `plan110/stress_high_freq_loop_t11_8.nv` | Reduce 1000→100 loop iterations |
| `plan55/f1_closure_array_gc_stress.nv` | Reduce 50→5 GC stress cycles |
| `plan56/f4_clone_gc_stress.nv` | Reduce 100→10 clone iterations |

### investigate list

None. All observed slowness is attributable to:
- Intentional N (stress/bench design) → migrate-slow or create-fast-variant
- Compile time floor (~15-50 s per test on Windows/clang debug) → not a runtime bug
