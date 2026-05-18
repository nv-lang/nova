# Plan 70 — strict type propagation tests

Test fixtures per-site для PhaseA1-A4 migrations Cat A silent
`nova_int` fallback → strict E7001 error.

## Naming

- `fN_<site>_<scenario>_pos.nv` — positive, доказывает migration
  не сломала valid use case
- `fN_<site>_<scenario>_neg.nv` — `EXPECT_COMPILE_ERROR` with E7001;
  доказывает strict mode ловит конкретный bug

## Reachability tracking

Не для всех 64 Cat A sites negative test возможен. Некоторые
fallback paths **unreachable in practice** — type-checker pre-rejects
broken code до codegen. Migration таких sites = **dead-code hardening**
(не silent → loud, но trigger почти невозможен).

| Site (line) | Containing fn | Reachable neg? | Test |
|---|---|---|---|
| 4939 | `emit_type_decl` | ❌ unreachable (type-checker pre-rejects) | f1 pos only |
| 4819, 4820 | `emit_fn_forward_decl` (fn-returning-fn sig) | ❌ unreachable (type-checker resolves before codegen) | f2 pos covers |
| 4829 | `emit_fn_forward_decl` (free fn params) | ❌ unreachable | f2 pos covers |
| 4838, 4841 | `emit_fn_forward_decl` (HOF inner closure sig) | ❌ unreachable | f2 pos covers |
| 1325, 1329 | `emit_module` (free-fn overload reg) | ❌ unreachable | f3 pos covers |
| 1422, 1435 | `emit_module` (method overload reg, non-generic recv) | ❌ unreachable (generic-recv path = erased_type_ref_c, Cat B intentional) | f3 pos covers |
| 9031, 9034 | `emit_stmt` (let array-of-fn element sig) | ❌ unreachable + array-of-fn dispatch has pre-existing limitation | f4 pos via lambda only |
| 9081, 9084 | `emit_stmt` (let `as fn` annotation) | ❌ unreachable | f4 implicit (lambda tested) |
| 9160, 9165, 9169 | `emit_stmt` (let Lambda annotation) | ❌ unreachable | f4 pos covers |
| 9185, 9188 | `emit_stmt` (let ClosureLight annotation) | ❌ unreachable | f4 pos covers |
| 9204, 9207 | `emit_stmt` (let ClosureFull params/return) | ❌ unreachable | f4 implicit |
