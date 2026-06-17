# Plan 91.14 — Debug Protocol + `${expr:?}` Format Spec — Acceptance Criteria

**Status:** CLOSED  
**Date:** 2026-06-17  
**Tests:** 21/21 PASS (17 pos + 4 neg)

---

## Acceptance Criteria

| # | Criterion | Status | Test(s) |
|---|-----------|--------|---------|
| A0 | Production-grade, no shortcuts/stubs in delivered scope | ✅ | All |
| A1 | Debug protocol defined in `std/prelude/protocols.nv` | ✅ | t1_pos_parse_debug_spec_ok |
| A2 | Primitive types (int/bool/str/char/f64) have @debug | ✅ | t4_pos_debug_primitives, t11_pos_debug_byte_and_negative |
| A3 | Option[T] and Result[T,E] have @debug [T Debug] | ✅ | pos_debug_option_result |
| A4 | []T (Vec[T]) has @debug [T Debug] | ✅ | pos_debug_vec |
| A5 | `${x:?}` calls @debug (not Display/@display) | ✅ | pos_debug_interpolation |
| A6 | #impl(Debug) on record type auto-derives @debug | ✅ | pos_debug_record_derive, t5_pos_debug_struct_memberwise |
| A7 | #impl(Debug) on sum type auto-derives @debug (V1: type name) | ✅ | pos_debug_sum_derive |
| A8 | neg test: #impl(Debug) with wrong @debug signature → compile error | ✅ | neg_debug_no_impl |
| A9 | All fixtures pass via C-codegen (NOT interpreter) | ✅ | 21/21 PASS |
| A10 | D229 spec written/updated in spec/decisions/02-types.md | ✅ | See spec/decisions/02-types.md §D229 |
| A11 | ACCEPTANCE.md written | ✅ | This file |

---

## Tested Features

### Primitive @debug (A2)
- `int` debug: same as display (decimal)
- `bool` debug: same as display ("true"/"false")
- `f64` debug: same as display
- `str` debug: quoted + escape sequences (`"hi"` → `"\"hi\""`)
- `char` debug: single-quoted + escapes (`'A'`, `'\n'`)

### Option[T] / Result[T,E] @debug (A3)
- `Some(42)` → `"Some(42)"`
- `None` → `"None"`
- `Some("hi")` → `"Some(\"hi\")"` (inner str quoted)
- `Ok(1)` → `"Ok(1)"`
- `Err("oops")` → `"Err(\"oops\")"`

### Vec[T] @debug (A4)
- `Vec[int].of(1,2,3).debug(w)` → `"Vec[1, 2, 3]"`
- Empty vec: `"Vec[]"`

### `${x:?}` interpolation (A5)
- Verified that `${s:?}` routes to `@debug` while `${s}` routes to `@display`
- str: display="hello", debug=`"\"hello\""`

### #impl(Debug) auto-derive (A6)
- Record type: `Point { x: 1, y: 2 }` → `"Point { x: 1, y: 2 }"`
- str field with escaping: `Named { name: "\"hello\"", score: 42 }`

### Sum type debug (A7, V1)
- V1: output = type name (`"Color"` for any variant of `Color`)
- Full variant-level synthesis tracked in [M-91.14-sum-debug-variants]

### Nested debug (extra)
- `Option[Point]` with `Some(Point { x: 3, y: 4 })` → correct nesting
- `Result[Point, str]` → `Ok(Point { ... })`
- `Option[Option[int]]` → `"Some(Some(42))"`

### Negative tests
- `E_FORMAT_SPEC_EMPTY`: `${x:}` → compile error
- `E_FORMAT_SPEC_UNKNOWN`: `${x:hex}` → compile error (V1 only supports `?`)
- `E_IMPL_WRONG_SIGNATURE`: `#impl(Debug)` + wrong return type → compile error
- `E_PTR_NO_DISPLAY_USE_DEBUG_STR`: bare `${ptr}` preserved error
- trailing tokens after spec → compile error

---

## Known Limitations / Followups

- **[M-91.14-sum-debug-variants]**: Sum type debug V1 outputs type name only, not per-variant data. Full synthesis (e.g., `"Color::Blue(42)"`) deferred.
- **[M-91.14-none-as-user-type]**: `None as Option[UserStruct]` produces a C struct cast error at compile time (CC-FAIL). Workaround: avoid `None as Option[Point]` — use `None as Option[int]` or keep None in a context where the type is inferred from a `Some`.
- **[M-91.14-format-dsl-extensions]**: Extensions to format-spec grammar (`:hex`, `:.3`, `:pad-N`) deferred to future plans.
- **[M-91.14-str-from-debug-walker]**: The `default_body_calls_satisfy_for` walker doesn't check `str.from_debug` (only `str.from`), so `#impl(Debug)` with a non-Debug field type silently passes type-check but produces garbage at runtime. Fix tracked separately.
