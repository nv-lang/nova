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
