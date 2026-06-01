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
- Tuple newtype handles `type X(ptr)` ✓ ([M-115-newtype-constructor] closed)
- `[ffi]` build pipeline в nova.toml ✓ ([M-115-ffi-build-pipeline] closed)

## How user FFI works

Plan 115 V1 ships **manifest-driven FFI build pipeline** через `[ffi]`
секцию в `nova.toml`:

```toml
[ffi]
c_shims      = ["src/sqlite3_shim.c", "src/libpng_shim.c"]
include_dirs = ["third_party/sqlite3/", "third_party/libpng/"]
libs         = ["sqlite3", "png"]
```

Paths относительные к `nova.toml`. Test-runner + future `nova build`
автоматически:
- **`.h` файлы** — force-included через clang `-include` (или MSVC `/FI`)
- **`.c` файлы** — добавляются как compilation units
- **`include_dirs`** → `-I<path>`
- **`libs`** → `-l<name>` (или `<name>.lib` для MSVC)

Пример в работе: `nova_tests/nova.toml` объявляет
`c_shims = ["../examples/ffi/sqlite_mini_ffi.h"]` — Nova test runner
автоматически force-включает этот header в каждый тестовый TU, что даёт
доступ к `nova_fn_mini_sqlite_*` функциям из любой test fixture.

Pending followups:
- `[M-115-bindgen-tool]` — `nova bindgen header.h` auto-generated
  bindings (major tooling, separate plan).

## See also

- `docs/ffi-cookbook.md` — complete tutorial with three library examples
- `spec/decisions/02-types.md#d214` — `ptr` primitive type spec
- `nova_tests/plan115/` — fixture tests exercising the FFI primitives
- `compiler-codegen/nova_rt/plan115_ffi_test.h` — Plan 115 Ф.2 test shim
  used by `nova_tests/plan115/t2_external_fn_tuple_ok.nv`
