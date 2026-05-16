# Performance conventions — Nova bootstrap

> **Created 2026-05-16.** Documents wall-clock cost expectations
> для common operations. Used as **mental model** для writing
> performance-sensitive Nova code. Plan 57 (perf bench infrastructure)
> когда готов — добавит CI gate.

## Generic dispatch (Plan 56)

### Decision tree

```
┌─────────────────────────────────────────────────────────────┐
│ Generic method call on bound K                              │
│                                                             │
│  Concrete K on call-site?                                   │
│    YES → Mono path (direct call)         ~0ns (inline'd)    │
│    NO  → Vtable path (indirect)          ~1-2ns (L1)        │
│                                                             │
│ Generic method call on un-bound K (no protocol)             │
│  Always mono'd, no dispatch needed.       0ns               │
└─────────────────────────────────────────────────────────────┘
```

### Cost table (x86_64 Linux, Clang -O2, Boehm GC)

| Operation | Direct (mono) | Vtable | Notes |
|---|---|---|---|
| Bound method call (hash) | ~3ns | ~5-7ns | inline'd для primitives |
| Bound method call (eq) | ~2ns | ~4-6ns | branchy для str |
| Tuple destructure | ~0ns | n/a | direct struct field access |
| HashMap.get(k) | ~30-50ns | n/a | hash + probe + cmp |
| HashMap.insert(k, v) | ~40-80ns | n/a | hash + probe + write + grow |
| HashMap.clone() (N=100) | ~5μs | n/a | N inserts + array alloc |
| Closure call (zero-capture) | ~3ns | n/a | indirect via NovaClos_X |
| Channel send/recv | ~100-200ns | n/a | mutex + cond signal |
| Spawn fiber | ~5μs | n/a | mco_create + scheduler enqueue |

**Notes:**
- Costs measured на microbenchmarks; real workload может отличаться.
- Plan 57 (perf bench infra) — automated regression gate ±5%.
- Mono path **preferred** для hot loops. Vtable — fallback для truly
  erased contexts.

## Allocation patterns

| Pattern | Cost | Recommendation |
|---|---|---|
| `let x = [1, 2, 3]` (array literal) | 1 heap alloc | Use если N > 4 |
| `let m = ["a": 1]` (map literal) | 1 heap alloc + buckets | Use freely |
| Closure capture (small) | 1 heap alloc (env) | Fine для once-call |
| Closure capture (large, hot) | 1 alloc + retain | Hoist если possible |
| String concat `a + b` | 1 alloc + copy | Use `StringBuilder` для N+ concats |

## GC pause expectations (Boehm)

| Heap size | Typical pause | Max observed |
|---|---|---|
| < 100k objects | < 1ms | ~10ms |
| 100k - 1M | 5-15ms | ~30ms |
| 1M - 10M | 50-200ms | ~500ms |
| > 10M | seconds | varies |

**Real-time zones:** wrap в `realtime nogc { ... }` для bounded latency
(D64) — allocations forbidden inside.

## Performance-sensitive code guidelines

1. **Mono dispatch preferred** — write `let m HashMap[str, int] = [...]`
   с concrete K, V (not generic-over-K function calling).
2. **Avoid double-hash в hot loops** — `m.get(k).unwrap()` after `m.contains(k)`
   = 2 hashes; use direct `match m.get(k) { Some(v) => ... }`.
3. **Pre-size collections** — `HashMap.with_capacity(N)` saves N resize'ов.
4. **StringBuilder для N+ concats** — O(N) vs O(N²) для `+` chain.
5. **`bench.opaque(expr)`** (Plan 57 когда готов) — предотвращает
   constant-folding в benchmarks.

## Related plans

- [Plan 09](plans/09-clang-migration.md) — Clang default (10-15% perf
  vs MSVC).
- [Plan 10](plans/10-pgo-integration.md) — PGO future (15-30% perf).
- [Plan 27](plans/27-gc-switch.md) — Boehm GC default.
- [Plan 32](plans/32-gc-introspection.md) — GC introspection API.
- [Plan 44.4](plans/44.4-mn-runtime-stage0.md) — M:N runtime.
- [Plan 56](plans/56-vtable-dispatch-erased-generics.md) — hybrid
  dispatch (mono + vtable).
- [Plan 57](plans/57-perf-benchmark-infrastructure.md) — bench infra
  (future).
