# Plan 91.8c — Direct @[i].method() Dispatch — Acceptance Criteria

Fix: M-91.8c-direct-index-method
Branch: plan-91.8c-direct-idx

## Problem

`compute_array_elem_type_for_obj` had no `SelfAccess` arm. When a generic
`[]T` method body wrote `@[j].compare(key)`, the `@` receiver was a
`SelfAccess` expression; the function fell through to the `NovaArray_` path
and stripped the trailing `*` from pointer element types (`Nova_Point*`
→ `Nova_Point`), causing method dispatch to find no overload for `.compare()`.

The workaround was to introduce an intermediate binding:
```nova
ro v = @[j]
ro cmp = v.compare(key)
```
which is verbose and hides the real intent.

## Fix

Two-part fix in `compiler-codegen/src/codegen/emit_c.rs`:

1. `compute_array_elem_type_for_obj` — added `ExprKind::SelfAccess` arm that
   returns `array_element_types.get("nova_self")`, so `@[j]` in a generic
   method body resolves the element C type via the self receiver.

2. `emit_monomorphized_method` — after registering `var_types["nova_self"]`,
   derive the concrete element C type from `recv_c`:
   - `Nova_Vec____<elem>*` receivers: use `generic_type_instance_info` lookup
     with `current_type_subst` fallback.
   - `NovaArray_<elem>*` receivers: strip prefix and restore `*` for struct
     pointers.
   Register under `array_element_types["nova_self"]` and restore in cleanup.

## Acceptance Criteria

- [x] A1: Direct `@[arr][idx].method()` dispatches correctly in `[T Compare]`
  generic method body — no workaround binding needed.
- [x] A2: No intermediate binding required for array element method calls.
  `ro cmp = @[mid].compare(key)` compiles and runs correctly.
- [x] A3: All 13 existing `plan91_8c` tests still pass (regression guard).
- [x] A4: Production quality — no shortcuts, no TODO, no stubs in delivered scope.

## Tests (plan91_8c_direct/ — 5 fixtures)

### Positive tests

- `pos_direct_compare.nv` — `[T Compare]` generic methods calling
  `@[i].compare(key)` directly. Covers linear search and binary search, both
  `[]int` and `[]Score` (user record). 10 tests.

- `pos_direct_method_chain.nv` — `[]str` methods calling `@[i].byte_len()`
  and `@[i].is_empty()` directly. Also exercises `@[i].compare(@[i+1])`
  element-vs-element in `is_sorted_direct`. 12 tests.

- `pos_direct_operator.nv` — `@[j] > key` and `@[i] < key` synthesized
  operators in generic `[T Compare]` context without intermediate binding.
  Regression guard for operator synthesis path. 10 tests.

- `pos_sort_direct.nv` — bubble-sort variant using `@[j-1].compare(@[j])`
  element-vs-element, plus `max_direct` and `min_idx_direct` helpers. Covers
  user-defined Pair type. 12 tests.

### Negative tests

- `neg_no_compare_bound.nv` — `@[j].compare(key)` on `[T]` without `Compare`
  bound is a compile error. Guards against accidentally widening dispatch to
  unbound generics. 1 test (EXPECT_COMPILE_ERROR).

## Summary

| File | Tests | Status |
|------|-------|--------|
| pos_direct_compare.nv | 10 | PASS |
| pos_direct_method_chain.nv | 12 | PASS |
| pos_direct_operator.nv | 10 | PASS |
| pos_sort_direct.nv | 12 | PASS |
| neg_no_compare_bound.nv | 1 | PASS (compile error) |
| **Total** | **45** | **PASS** |

Regression (plan91_8c): 14/14 PASS (includes neg_direct_baseline.nv).
