// SPDX-License-Identifier: MIT OR Apache-2.0
# Q-perf-considerations — Performance Trade-offs of `consume{}` Cleanup

> **Plan 110 Ф.14.2 Q-block.** Q-perf-considerations: overhead analysis
> для `consume X = ... { body }` scope-block — cancel-shield, timeout
> resolution, outcome construction, on_exit dispatch costs +
> mitigation strategies.

## Overhead Per ConsumeScope (Plan 110.1 / 110.2 estimated)

| Component | Cost | When applicable |
|---|---|---|
| Init expression eval | varies | always |
| C local declaration | < 1 ns | always |
| `#define` alias setup | 0 ns (preprocessor) | always |
| `NovaFailFrame` setup | ~5 ns | always (until 110.1.7 hot-path) |
| `nova_fail_push` | ~5 ns | always |
| `setjmp` call | ~10 ns | always |
| Body emit | varies | always |
| Defer scope enter/leave (if body has defer) | ~10 ns | conditional |
| `nova_fail_pop` | ~5 ns | always |
| `nv_resolve_exit_timeout` (3-level) | ~20 ns | always (Plan 110.2) |
| `nova_make_ScopeOutcome_*` heap alloc | ~50 ns | always |
| `Nova_<T>_consume_on_exit` dispatch | varies | always |
| `nova_rethrow_with_suppressed` (on Failure) | ~30 ns | conditional |
| `nv_panic` (on Panic) | ~30 ns + abort | conditional |
| Cancel-shield enter/leave (Plan 110.2) | ~10 ns | always |

**Total overhead** (Success path, no body throws): **~100-150 ns per
ConsumeScope** в current Plan 110.1.4 + 110.1.6 implementation
(without 110.1.7 hot-path elision, without 110.2 cancel-shield).

**With 110.1.7 hot-path elision** (Consumable[never] + no
WithExitTimeout): **~15-30 ns** — comparable to raw lock+unlock.

**With 110.2 cancel-shield**: +5-15 ns shield enter/leave +
deadline-check at suspend points.

## Critical Performance Cases

### 1. Tight Lock Loops

```nova
for i in 0..1_000_000 {
    consume g = mu.lock() {
        critical_section()
    }
}
```

**Without 110.1.7 elision:** ~100ns × 1M = ~100ms ConsumeScope overhead.
**With 110.1.7 elision:** ~30ns × 1M = ~30ms (3.3x improvement).

**Recommendation:** wait for 110.1.7 OR use explicit `defer { g.unlock() }`
pattern в hot loops until elision lands.

### 2. High-Frequency Permits

```nova
for req in incoming_requests {
    consume p = sem.acquire() {
        handle(req)
    }
}
```

Same as #1 — 110.1.7 elision critical for 100K+/s permit throughput.

### 3. Nested Resource Acquisition

```nova
consume conn = pool.acquire()? {
    consume tx = conn.begin()? {
        consume stmt = tx.prepare(sql)? {
            stmt.execute(args)?
        }
    }
}
```

Three nested ConsumeScopes — overhead multiplies (~300-450 ns
unconditionally). For OLTP workloads (1K+/s transactions), this is
< 1% overhead — acceptable.

For analytical workloads с long-running scopes, overhead irrelevant
(< 0.001%).

## Cancel-Shield Cost (Plan 110.2)

Cancel-shield prevents cancel storm during cleanup. Cost components:

1. **Enter shield** (`nv_consume_enter_shield`): set
   `fiber->cancel_masked = true` + register deadline. ~5 ns.
2. **Suspend deadline check** (each `await` в cleanup): compare
   `now >= deadline`. ~2 ns per check.
3. **Leave shield** (`nv_consume_leave_shield`): clear flag, check
   pending cancel. ~5 ns.

For sync cleanup (no `await`): ~10 ns total shield overhead.
For async cleanup with N suspend points: ~10 + 2*N ns.

## 3-Level Timeout Resolution Cost (Plan 110.2)

Per `consume X = ... { body }` entry:
1. **Level 1** check: vtable lookup для `T::exit_timeout_ms`. ~3 ns.
2. **Level 2** check (если Level 1 None): scan effect stack for
   `Application` handler. ~10 ns (depends on stack depth).
3. **Level 3** fallback: hardcoded constant `5000`. 0 ns.

Total: ~3-15 ns per scope. Cached в local after entry — body re-uses.

## Hot-Path Elision Effectiveness (Plan 110.1.7)

For `Consumable[never]` + no WithExitTimeout:

| Construct | Without elision | With elision |
|---|---|---|
| `Mutex.lock()` scope | ~100 ns | ~30 ns |
| `Semaphore.acquire()` scope | ~100 ns | ~30 ns |
| `Channel.recv()` scope | ~120 ns | ~50 ns |
| `CancelScope.new()` scope | ~80 ns | ~20 ns |

Order of magnitude: **3-4x speedup** для elidable resources.

## OpenTelemetry Tracing Cost (D185)

`Cleanup` effect handler emit OTel spans for each ConsumeScope:
1. Span open at `on_scope_enter`: ~500-1000 ns (depends on SDK).
2. Span close at `on_scope_exit`: ~500-1000 ns.

**Total OTel overhead:** ~1-2 μs per scope. Critical для:
- Avoid в hot loops (sample rate, e.g., 1/1000 scopes).
- Use only in high-level operations (HTTP requests, DB transactions,
  not lock/permit acquisition).

Default: **no-op handler** (zero overhead если не bound).

## MultiError Cost (D193)

Per error compose call (`nv_compose_suppressed`):
- Identity check + depth iteration: ~5 ns × N (depth).
- Allocation: ~50 ns.
- Insertion: < 1 ns.

For depth < 10: ~100 ns. For depth 100: ~500 ns. Depth 256
(truncation kick-in): ~1.5 μs.

**Recommendation:** rare for normal use (defer-cascade rarely
exceeds 5-10 errors).

## Optimization Strategies

### Strategy 1: Use `defer` для simple cases

If cleanup is single-method instant call:

```nova
ro f = File.open(path)?
defer { f.close() }   // simpler, no scope-block overhead
read_data(f)
```

vs:

```nova
consume f = File.open(path)? { read_data(f) }
// ~100 ns ConsumeScope overhead even for instant close.
```

`defer` is ~10x cheaper for instant cleanup.

### Strategy 2: Implement WithExitTimeout правильно

Don't add `exit_timeout_ms()` impl unless cleanup может realistically
take seconds. Adding it kills hot-path elision (Plan 110.1.7).

### Strategy 3: Batch scoping

Instead of:
```nova
for x in items {
    consume g = mu.lock() {
        do_one(x)
    }
}
```

Use:
```nova
consume g = mu.lock() {
    for x in items {
        do_one(x)
    }
}
```

Single scope-block — single overhead. Lock held longer, но overall
faster для small N items.

### Strategy 4: Profile, don't guess

```bash
nova bench --plan-110-overhead path/to/bench.nv
```

Most cleanup overhead is < 5% of total runtime. Optimize hot-path
based on profile, не speculation.

## Performance Regression Targets (Plan 110.6 Bench Suite)

Per Plan 110.6 Ф.11.5 acceptance:
- Cancel-shield + 3-level resolution overhead: **≤ baseline + 5%**.
- exit_timeout enforcement overhead: **< 50 ns/scope**.
- MultiError compose overhead (depth 100): **< 5 μs/cascade**.

Measured against Plan 100.4 cleanup baseline (defer-family pre-Plan
110). Regression detection via CI bench suite.

## See also

- [D194 hot-path elision](../../spec/decisions/03-syntax.md#d194).
- [D192 timeout taxonomy](../../spec/decisions/03-syntax.md#d192).
- [D193 MultiError](../../spec/decisions/03-syntax.md#d193).
- [D185 Cleanup effect cost](../../spec/decisions/04-effects.md#d185).
- [Q-hot-path-performance](q-hot-path-performance.md).
- [Q-debugging-cleanup-chains](debugging-cleanup-chains.md).
- [cleanup-cookbook.md §7 Performance](../cleanup-cookbook.md).
