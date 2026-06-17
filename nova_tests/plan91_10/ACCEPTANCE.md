# Plan 91.10 Acceptance Criteria — D163 `needs <Cap>` retract

Spec: **D163 RETRACTED** in `spec/decisions/02-types.md`
(Plan 91.10, 2026-05-30). Plan doc:
`docs/plans/91.10-d163-retract-capability-syntax.md`.

D163 (Plan 100.5) introduced a `needs <Cap>` clause on `external fn` that
declared a mandatory capability when the FFI took/returned consume-types.
Plan 91.10 retracted it: a capability is structurally an effect-without-operations,
so a parallel `needs <Cap>` tracking system is design redundancy. The parser now
emits a hard error with a migration hint when it encounters the retracted syntax.

## Functional
- [x] Parser rejects `needs <Cap>` after an `extern`/`external fn` signature with a
      clear error message ("`needs <Cap>` clause is retracted ... declare an effect").
- [x] `check_external_fn_needs_caps` (D163 validation) removed from `types/mod.rs`.
- [x] No `needs <Cap>` occurrences remain in `std/` or `nova_tests/`.
- [x] D163 marked RETRACTED in `spec/decisions/02-types.md` + README index.

## Tests (`nova_tests/plan91_10`) — 1/1 PASS
- [x] `needs_cap_retracted_neg` — negative: `extern "nova" fn foo(...) needs Sys`
      → compile error ("is retracted"). Guards Acceptance #1.

Note: the retract also touched `plan100_1` / `plan100_7` fixtures (drop of `needs`
clauses); those suites continue to pass and are the broader regression surface for
the retract (this folder is the dedicated guard for the retracted-syntax error).
