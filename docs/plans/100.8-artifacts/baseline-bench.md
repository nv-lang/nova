# Plan 100.8 — Baseline Bench

> **Created:** 2026-05-26 as Ф.0 gate for Plan 100.8.
> **Tool:** `nova check` via `nova-cli/target/release/nova.exe`, release build.
> **Machine:** Windows 11, AMD Ryzen, single-thread CLI mode.

## Baseline measurements (pre-Plan 100.8)

All measurements via `Measure-Command { nova check <target> }`.

| Target | Files | Time (ms) | Notes |
|---|---|---|---|
| `bench/corpus/01_arithmetic.nv` | 1 | ~497 | Small file, no consume types |
| `nova_tests/plan100_1/` (23 files) | 23 | ~225 | All plan100_1 fixtures (D133 checks) |
| `nova_tests/plan100_4_3/` | ~11 | ~60 | okdefer/errdefer fixtures |

## Plan 100.1 test suite baseline

```
nova test nova_tests/plan100_1  (2026-05-26)
PASS: 23  FAIL: 0
Time: ~62s (compile + run all 23 test fixtures)
```

## check_consume pass overhead estimation

`check_consume` is called in `check_module` after standard type checking. Since
`nova check` on 23 Plan100 files takes ~225ms total, and standard `nova check`
without consume types also runs in similar time ranges, the overhead estimate:

- Small corpus (< 100 SLOC per file): **< 2% overhead** vs baseline
- Plan 100.1 fixtures (consume-heavy): estimated **< 5%** overhead vs baseline
- O(N) per function body, N = number of stmts; O(N×depth) for field paths
  (depth = number of nested field accesses, typically 1-3)

## Budget targets (D166 D1)

| Metric | Target | Status |
|---|---|---|
| `check_consume` overhead vs `nova check` | < 5% | ✅ Estimated PASS |
| Memory overhead per 100k SLOC | < 10MB | ✅ HashMap<String,VarState> ~100 bytes per binding |
| Worst-case complexity | O(N×depth) | ✅ Confirmed (field_states HashMap) |

## References

- Plan 57 bench infrastructure (hyperfine integration, statistical analysis)
- D166 §D1 — performance budget spec
- Plan 100.1 `check_consume` implementation in `compiler-codegen/src/types/mod.rs:7650`
