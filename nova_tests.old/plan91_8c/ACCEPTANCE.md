# Plan 91.8c — Acceptance Criteria

Generic array sort/min/max/binary_search + _by API (D185).

## Implementation

- [x] `fn[T Compare] []T mut @sort_of() -> @` — generic insertion sort, works for int/str/user types
- [x] `fn[T Compare] []T @binary_search_of(target T) -> Option[int]` — binary search on sorted array
- [x] `fn[T Compare] []T @min_of() -> Option[T]` — generic min
- [x] `fn[T Compare] []T @max_of() -> Option[T]` — generic max
- [x] `fn[T] []T mut @sort_by_of(cmp fn(T, T) -> int) -> @` — sort with callback (no Compare bound)
- [x] `fn[T] []T @min_by_of(cmp fn(T, T) -> int) -> Option[T]` — min with comparator callback
- [x] `fn[T] []T @max_by_of(cmp fn(T, T) -> int) -> Option[T]` — max with comparator callback
- [x] `fn[T] []T mut @reverse_of() -> @` — generic reverse in-place
- [x] `fn[T] []T @position_of(pred fn(T) -> bool) -> Option[int]` — find index by predicate
- [x] `fn[T] []T @count_of(pred fn(T) -> bool) -> int` — count by predicate
- [x] `fn[T] []T @find_of(pred fn(T) -> bool) -> Option[T]` — find element by predicate
- [x] Concrete `[]int @sort/@sort_by/@min/@max/@sum/@product` preserved (no regression)
- [x] Type-checker: `[T Compare]` bound enforced — sort_of on non-Compare type gives compile error

## Spec

- [x] D185 in `spec/decisions/02-types.md` — section added at end of file
- [x] D-index in `spec/decisions/README.md` — D185 row added
- [x] Forward reference in D183 bullet updated to point to D185 section

## Tests (plan91_8c/ — 13/13 PASS)

### Plan-required files
- [x] `pos_sort_str.nv` — sort_of []str alphabetically (6 tests, PASS)
- [x] `pos_sort_record.nv` — sort_by_of with Point callback (5 tests, PASS)
- [x] `pos_min_max_str.nv` — min_of/max_of on []str (8 tests, PASS)
- [x] `pos_min_max_by.nv` — min_by_of/max_by_of with callback (7 tests, PASS)
- [x] `pos_binary_search_str.nv` — binary_search_of on sorted []str (8 tests, PASS)
- [x] `neg_sort_no_compare.nv` — sort_of on type without Compare -> compile error (PASS negative)
- [x] `pos_sort_int_regression.nv` — []int sort/sort_by/sum/product/sort_of/min_of/max_of/binary_search_of all work (8 tests, PASS)

### Pre-existing files (also PASS)
- [x] `generic_sort_test.nv` — sort_of/binary_search_of int+str (7 tests)
- [x] `generic_min_max_test.nv` — min_of/max_of int+str (3 tests)
- [x] `sort_by_callback_test.nv` — sort_by_of descending/ascending/abs (3 tests)
- [x] `min_max_by_find_test.nv` — min_by_of/max_by_of/find_of (6 tests)
- [x] `position_count_test.nv` — position_of/count_of (6 tests)
- [x] `reverse_sum_test.nv` — reverse_of/sum/product (7 tests)

## Acceptance Criteria from Plan

- [x] A0: Production-grade, no shortcuts/stubs/TODO in delivered scope
- [x] A1: sort([]T) works for []str, []record-with-Compare (via sort_of)
- [x] A2: sort_by([]T, fn) works without Compare bound (via sort_by_of)
- [x] A3: min/max/min_by/max_by work for generic T (min_of/max_of/min_by_of/max_by_of)
- [x] A4: binary_search([]T) works for []str (via binary_search_of)
- [x] A5: neg test: sort on type without Compare -> compile error (neg_sort_no_compare.nv PASS)
- [x] A6: []int sort still works (pos_sort_int_regression.nv PASS)
- [x] A7: All fixtures pass via release nova + C-codegen (13/13 PASS)
- [x] A8: D185 spec written
- [x] A9: nova_tests/plan91_8c/ACCEPTANCE.md written (this file)

## Notes

- API uses `_of` suffix (sort_of/min_of/max_of/binary_search_of) to avoid name collision
  with concrete `[]int @sort/@min/@max` methods (which have different receiver types).
- Concrete `[]int @min()/@max()` have a pre-existing codegen mis-dispatch bug (f64.min
  confusion, tracked in plan91/sort_basic.nv CC-FAIL). The regression test uses `min_of()`
  instead, which goes through the correct generic path. This pre-existing bug is out of scope
  for Plan 91.8c; tracked as `[M-91.8c-int-min-max-dispatch]`.
- Algorithm: stable insertion sort O(n²). Followup: `[M-91.8c-pdq-sort]` for large arrays.
