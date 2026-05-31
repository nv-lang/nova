<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova FFI Examples

> **Plan 115 D214 (foundational FFI).** See `docs/ffi-cookbook.md` for
> complete tutorial.

## Files

| File | What |
|---|---|
| `ptr_basics.nv` | `ptr` primitive type usage — null, casts, typed handles |
| `sqlite_mini.nv` | Minimal libsqlite3 binding sketch — handles + tuple returns |

## Build status

Plan 115 V1 ships:
- `ptr` opaque pointer type ✓
- Tuple-by-value FFI returns ✓
- User-level `external fn` declarations ✓ (D82 amend)
- Record-form typed handles ✓

Pending followups before real-library examples build:
- `[M-115-ffi-build-pipeline]` — `nova build --c-shim` CLI for linking
  user-provided C shim files. Currently shims must live in
  `compiler-codegen/nova_rt/*.h`.
- `[M-115-bindgen-tool]` — `nova bindgen header.h` auto-generated
  bindings.

For `sqlite_mini.nv`: the shim header in `docs/ffi-cookbook.md`
§«Example 1» can be dropped into `compiler-codegen/nova_rt/` and the
example compiles + links if libsqlite3 is on the system. This is the
"manual integration" path — `[M-115-ffi-build-pipeline]` will automate
it for user code.

## See also

- `docs/ffi-cookbook.md` — complete tutorial with three library examples
- `spec/decisions/02-types.md#d214` — `ptr` primitive type spec
- `nova_tests/plan115/` — fixture tests exercising the FFI primitives
- `compiler-codegen/nova_rt/plan115_ffi_test.h` — Plan 115 Ф.2 test shim
  used by `nova_tests/plan115/t2_external_fn_tuple_ok.nv`
