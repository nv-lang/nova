// SPDX-License-Identifier: MIT OR Apache-2.0
# Q-hot-path-performance — Consumable[never] + No WithExitTimeout (D194)

> **Plan 110 Ф.14.2 Q-block.** Q-hot-path-performance: minimizing
> ConsumeScope overhead для tight loops, lock contention, high-frequency
> permits. D194 hot-path elision rationale + measurement guidance.
> Cross-ref [D194](../../spec/decisions/03-syntax.md#d194).

## TL;DR

For `Consumable[never]` resources without `WithExitTimeout` impl:
codegen elidet cancel-shield setup + timeout resolution + outcome
construction (Plan 110.1.7). Critical для:
- Lock acquisition в tight loops.
- High-frequency semaphore permits.
- Pool slot release.
- Atomic CancelScope cancel.

Expected overhead: **`consume X = mu.lock() { body }` compiles to
`body; mu.unlock()`** — zero overhead vs raw lock+unlock pair.

## When Hot-Path Optimization Applies

**Eligibility:**
1. Type implements `Consumable[never]` (on_exit has no `Fail` effect).
2. Type does NOT satisfy `WithExitTimeout` protocol (no
   `exit_timeout_ms()` method).

**What gets elided:**
- Cancel-shield setup/teardown (on_exit guaranteed not throws → no
  MultiError compose path needed).
- Timeout resolution `nv_resolve_exit_timeout` call (not needed —
  release instant).
- `ScopeOutcome` heap allocation (Mutex/Sem не differentiate
  Success/Failure/Panic — always unlock).

**Result:** ConsumeScope codegen reduces to:
```c
Nova_MutexGuard* g = Nova_Mutex_consume_lock(m);
/* body */
Nova_MutexGuard_consume_on_exit(g, NULL /* outcome ignored */);
```

vs full version:
```c
Nova_MutexGuard* g = Nova_Mutex_consume_lock(m);
#define <binding> g
NovaFailFrame _frame;
nova_fail_push(&_frame);
_frame.error_suppressed = NULL;
int _outcome_kind = 0;
if (setjmp(_frame.jmp) == 0) {
    /* body */
    nova_fail_pop();
} else {
    nova_fail_pop();
    _outcome_kind = (_frame.error_kind == NOVA_THROW_PANIC) ? 2 : 1;
}
Nova_ScopeOutcome* outcome_val;
if (_outcome_kind == 0) { outcome_val = nova_make_ScopeOutcome_Success(); }
else if (_outcome_kind == 1) { /* construct Failure */ }
else { /* construct Panic */ }
Nova_MutexGuard_consume_on_exit(g, outcome_val);
if (_outcome_kind == 1) { nova_rethrow_with_suppressed(&_frame); }
else if (_outcome_kind == 2) { nv_panic(_frame.error_msg); }
#undef <binding>
```

Order of magnitude: 1-2 ns elided overhead per scope-block. Critical
for ~1M/s lock acquisition loops.

## Examples of Hot-Path-Eligible Resources

| Type | Consumable[never] | WithExitTimeout | Hot-path? |
|---|---|---|---|
| `MutexGuard` | ✅ | ❌ | ✅ |
| `ReadGuard` | ✅ | ❌ | ✅ |
| `WriteGuard` | ✅ | ❌ | ✅ |
| `Permit` (Semaphore) | ✅ | ❌ | ✅ |
| `CancelScope` | ✅ | ❌ | ✅ |
| `ChanWriter` (no failure) | ✅ | ❌ | ✅ |
| `Transaction` (commit может throw) | ❌ | ✅ (custom timeout) | ❌ |
| `TcpStream` (close может throw) | ❌ | ✅ | ❌ |

## Trade-off: Throw в Body

**Hot-path elision sacrifices throw handling в body.** If body throws,
control flow longjmps к outer fail-frame **without calling on_exit**.

**Implication for Consumable[never] resources:**

```nova
fn use_mutex(mu Mutex) Fail[E] -> () {
    consume g = mu.lock() {
        risky_op()?       // may throw
        @do_work()
    }
    // If risky_op throws:
    //   - g.on_exit NOT called.
    //   - Mutex stays LOCKED! Deadlock potential.
}
```

**Mitigation:** in hot-path-elided mode, body must be **infallible**
OR caller accepts lock-leak on throw.

**Safer pattern:** explicit acquire/release for fallible body:

```nova
fn use_mutex_safe(mu Mutex) Fail[E] -> () {
    ro g = mu.lock()
    defer { g.unlock() }     // explicit defer ensures cleanup
    risky_op()?
    @do_work()
}
```

OR opt into full version by adding `WithExitTimeout` impl:

```nova
fn MutexGuard @exit_timeout_ms() -> int => 1000   // disables hot-path elision
```

## Measurement Guidance

### Benchmark Methodology

```nova
#bench
fn bench_mutex_hot_path() -> () {
    mut mu = Mutex.new()
    for i in 0..1_000_000 {
        consume g = mu.lock() {
            // Empty body — measure overhead only.
        }
    }
}
```

Expected: **< 50 ns/iteration overhead** vs raw lock/unlock pair
(measured via `nova bench`).

### Disassembly Verification

```bash
nova build --release --emit-asm src/lock_path.nv
```

For hot-path-eligible code, expect:
- ❌ NO `nv_resolve_exit_timeout` call.
- ❌ NO `nova_fail_push` / `nova_fail_pop` pair.
- ❌ NO `setjmp` / `longjmp`.
- ❌ NO `nova_make_ScopeOutcome_*` calls.
- ✅ Direct `Nova_<T>_consume_lock` + body + `Nova_<T>_consume_on_exit(g, NULL)`.

### Profiling

```bash
nova test --profile bench_mutex_hot_path
```

Look for:
- < 1% time в `nova_fail_*` runtime functions.
- < 1% time в `Nova_ScopeOutcome_*` constructors.
- Hot path concentrated в `Nova_Mutex_*` + user body.

## Anti-patterns

### Anti 1: Adding WithExitTimeout reflexively

```nova
// ❌ DON'T: kills hot-path optimization for instant cleanup.
fn MutexGuard @exit_timeout_ms() -> int => 5000   // disables elision
```

WithExitTimeout shouldn't be added unless cleanup может realistically
take seconds.

### Anti 2: Fallible body в hot-path eligible scope

```nova
// ❌ DEADLOCK RISK:
consume g = mu.lock() {
    risky_op()?       // body throws → g.on_exit NOT called → Mutex stuck
}
```

For fallible body, either:
- Use explicit `defer` (safest).
- Use raw `consume g = mu.lock()` + `defer { g.unlock() }`.
- Mark MutexGuard with WithExitTimeout to disable elision.

### Anti 3: Optimizing prematurely

```nova
// ❌ Premature:
fn MyGuard @exit_timeout_ms() -> int => i64.MAX  // never timeout
```

Don't disable elision until profile actually shows overhead. Most
mutex/permit usage doesn't bottleneck on ConsumeScope overhead.

## Current Status (Plan 110.1.7)

110.1.7 hot-path elision codegen — **DEFERRED** for careful design:
- Plain elision (skip outcome construction) safe.
- Full elision (skip fail-frame) breaks throw safety per Mitigation
  above.
- Implementation requires careful trade-off documentation per resource.

Until 110.1.7 lands, all ConsumeScope emit full version (with
fail-frame + outcome construction). Performance impact: ~50 ns/scope
overhead unconditionally.

## See also

- [D194 Consumable[never] + hot-path](../../spec/decisions/03-syntax.md#d194).
- [Plan 110.1.7 hot-path elision codegen](../plans/decomposition.md).
- [Q-perf-considerations](q-perf-considerations.md).
- [cleanup-cookbook.md §7 Performance](../cleanup-cookbook.md).
