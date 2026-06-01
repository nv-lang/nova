<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova FFI Cookbook

> **Plan 115 D214 (foundational FFI).** Status: 🆕 V1 — covers `ptr` type +
> tuple-by-value returns + opaque handle pattern. Future plans extend
> (`*T` family — Plan 118; `nova bindgen` — `[M-115-bindgen-tool]`).

This cookbook shows how to bind Nova code to third-party C libraries —
sqlite3, libpng, libcurl — using the foundational FFI primitives
introduced in Plan 115.

## Quick reference

| Need | Tool | Spec |
|---|---|---|
| Opaque pointer | `ptr` primitive type | [D214](../spec/decisions/02-types.md#d214) |
| NULL literal | `null ptr` | D214 §1 |
| Typed handle | `type X { ro value ptr }` record (V1) | D214 §3 |
| Multi-value return | `(T1, T2)` tuple-by-value | D214 §2 |
| External fn declaration | `external fn name(args) -> ret` | [D82](../spec/decisions/03-syntax.md#d82) |
| Resource cleanup | `consume close()` method + `defer` | [D90 / D131](../spec/decisions/03-syntax.md#d90) |

## Layered FFI pattern

```
LAYER 1  Nova public API           Database.open(path)
   ↓
LAYER 2  Nova wrapper              construct typed handle from raw return
   ↓
LAYER 3  external fn declaration   typed handle + tuple return
            external fn nova_fn_sqlite3_open(path str) -> (ptr, int)
   ↓
LAYER 4  C shim                    ~5-10 lines, adapts out-param → struct
            _NovaTuple_2_8_nova_ptr_8_nova_int
            nova_fn_sqlite3_open(nova_str path) { ... }
   ↓
LAYER 5  Actual C library          sqlite3_open(path, &db_out)
```

## Plan 115 V1 setup

Nova V1 has these foundational pieces (commit `<plan-115-merge>`):
- `ptr` primitive type emitted as `typedef void* nova_ptr` in C output.
- Tuple-by-value returns from external fn — leverages mono'd
  `_NovaTuple_<arity>_<L_i>_<T_i>...` typedefs (Plan 59 mechanism).
- D82 amended (Plan 115): user-level `external fn` permitted in any
  module — no longer restricted to `std.runtime.*`.

What V1 does NOT yet ship:
- Tuple newtype `type X(ptr)` constructor — followup
  `[M-115-newtype-constructor]`. V1 uses single-field record form.
- `nova build --c-shim path/to/file.c` link-step CLI — followup
  `[M-115-ffi-build-pipeline]`. V1 places shim headers in
  `compiler-codegen/nova_rt/` and rebuilds Nova compiler.
- Auto-generated bindings from C headers — followup
  `[M-115-bindgen-tool]`.

## Example 1 — libsqlite3 binding

A complete example covering open, exec, prepare, step, finalize, close.

### C shim (`compiler-codegen/nova_rt/sqlite3_ffi.h`)

```c
/* sqlite3_ffi.h — Nova binding shim for libsqlite3.
 *
 * Compile + link target: libsqlite3 must be available.
 * Plan 115 V1 ships header-only inline wrappers — link with -lsqlite3.
 */

#ifndef NOVA_SQLITE3_FFI_H
#define NOVA_SQLITE3_FFI_H

#include <sqlite3.h>

/* Forward-declare Nova mono'd tuple types matching what Nova codegen emits. */
#ifndef NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_ptr_8_nova_int
#define NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_ptr_8_nova_int
typedef struct _NovaTuple_2_8_nova_ptr_8_nova_int {
    nova_ptr f0;
    nova_int f1;
} _NovaTuple_2_8_nova_ptr_8_nova_int;
#endif

/* Open a database. Returns (db_handle, return_code).
 * rc == 0 (SQLITE_OK) on success. */
static inline _NovaTuple_2_8_nova_ptr_8_nova_int
nova_fn_sqlite3_open(nova_str path) {
    _NovaTuple_2_8_nova_ptr_8_nova_int r;
    sqlite3* db = NULL;
    /* sqlite3_open expects C-string; Nova str.ptr may not be NUL-terminated. */
    char path_buf[1024];
    if (path.len >= sizeof(path_buf)) { r.f0 = NULL; r.f1 = SQLITE_TOOBIG; return r; }
    memcpy(path_buf, path.ptr, path.len);
    path_buf[path.len] = '\0';
    int rc = sqlite3_open(path_buf, &db);
    r.f0 = (nova_ptr)db;
    r.f1 = (nova_int)rc;
    return r;
}

/* Close a database. Returns sqlite3 rc. */
static inline nova_int nova_fn_sqlite3_close(nova_ptr db) {
    return (nova_int)sqlite3_close((sqlite3*)db);
}

/* Execute SQL (no result set). Returns rc. */
static inline nova_int nova_fn_sqlite3_exec(nova_ptr db, nova_str sql) {
    char buf[4096];
    if (sql.len >= sizeof(buf)) return SQLITE_TOOBIG;
    memcpy(buf, sql.ptr, sql.len);
    buf[sql.len] = '\0';
    char* errmsg = NULL;
    int rc = sqlite3_exec((sqlite3*)db, buf, NULL, NULL, &errmsg);
    sqlite3_free(errmsg);
    return (nova_int)rc;
}

/* Prepare statement. Returns (stmt_handle, rc). */
static inline _NovaTuple_2_8_nova_ptr_8_nova_int
nova_fn_sqlite3_prepare(nova_ptr db, nova_str sql) {
    _NovaTuple_2_8_nova_ptr_8_nova_int r;
    sqlite3_stmt* stmt = NULL;
    int rc = sqlite3_prepare_v2((sqlite3*)db, sql.ptr, (int)sql.len, &stmt, NULL);
    r.f0 = (nova_ptr)stmt;
    r.f1 = (nova_int)rc;
    return r;
}

/* Step. Returns rc (SQLITE_ROW = 100, SQLITE_DONE = 101). */
static inline nova_int nova_fn_sqlite3_step(nova_ptr stmt) {
    return (nova_int)sqlite3_step((sqlite3_stmt*)stmt);
}

/* Column int value. */
static inline nova_int nova_fn_sqlite3_column_int(nova_ptr stmt, nova_int col) {
    return (nova_int)sqlite3_column_int((sqlite3_stmt*)stmt, (int)col);
}

/* Finalize statement. */
static inline nova_int nova_fn_sqlite3_finalize(nova_ptr stmt) {
    return (nova_int)sqlite3_finalize((sqlite3_stmt*)stmt);
}

#endif /* NOVA_SQLITE3_FFI_H */
```

### Nova binding (`my_app/sqlite3.nv`)

```nova
module my_app.sqlite3

// Typed handles — V1 record form (tuple newtype `type X(ptr)`
// в Plan 115.1).
type Db { ro value ptr }
type Stmt { ro value ptr }

// External declarations matching the C shim.
external fn nova_fn_sqlite3_open(path str) -> (ptr, int)
external fn nova_fn_sqlite3_close(db ptr) -> int
external fn nova_fn_sqlite3_exec(db ptr, sql str) -> int
external fn nova_fn_sqlite3_prepare(db ptr, sql str) -> (ptr, int)
external fn nova_fn_sqlite3_step(stmt ptr) -> int
external fn nova_fn_sqlite3_column_int(stmt ptr, col int) -> int
external fn nova_fn_sqlite3_finalize(stmt ptr) -> int

// SQLite return codes (extract subset).
const SQLITE_OK   int = 0
const SQLITE_ROW  int = 100
const SQLITE_DONE int = 101

type DbError | OpenFailed(int) | ExecFailed(int) | PrepareFailed(int)

// Open database, wrap raw ptr в typed Db handle.
fn Db.open(path str) Fail[DbError] -> Db {
    ro (raw, rc) = nova_fn_sqlite3_open(path)
    if rc != SQLITE_OK { Fail.throw(DbError.OpenFailed(rc)) }
    Db { value: raw }
}

// Execute SQL (no result set).
fn Db @exec(sql str) Fail[DbError] -> () {
    ro rc = nova_fn_sqlite3_exec(self.value, sql)
    if rc != SQLITE_OK { Fail.throw(DbError.ExecFailed(rc)) }
}

// Close. consume — после @close handle invalid (D131).
fn Db consume @close() -> () {
    nova_fn_sqlite3_close(self.value)
}

// Example usage:
//
//   ro db = Db.open("/tmp/test.db")!
//   db.@exec("CREATE TABLE users (id INT, name TEXT)")!
//   db.@exec("INSERT INTO users VALUES (1, 'Alice')")!
//   defer db.@close()
//   ...
```

### Key patterns

- **Typed handle.** `type Db { ro value ptr }` makes `Db` nominally
  distinct from raw `ptr` — passing wrong handle is compile error.
- **Tuple return.** `nova_fn_sqlite3_open` returns `(ptr, int)`. Nova
  destructures: `ro (raw, rc) = nova_fn_sqlite3_open(path)`.
- **Cleanup.** `fn Db consume @close()` — invalidates handle, prevents
  use-after-close via consume bit (D131). Combine with `defer
  db.@close()` for leak resistance.
- **Error mapping.** Wrap C return codes in a Nova sum type for
  type-safe error handling.

## Example 2 — libpng (read PNG into pixel buffer)

```nova
module my_app.png

type PngFile { ro value ptr }
type PngInfo { ro value ptr }

external fn nova_fn_png_create_read_struct() -> ptr
external fn nova_fn_png_create_info_struct(png ptr) -> ptr
external fn nova_fn_png_init_io(png ptr, fp ptr) -> int
external fn nova_fn_png_read_info(png ptr, info ptr) -> int
external fn nova_fn_png_get_image_width(png ptr, info ptr) -> int
external fn nova_fn_png_get_image_height(png ptr, info ptr) -> int
external fn nova_fn_png_destroy_read_struct(png ptr, info ptr) -> ()

fn PngFile.from_handle(p ptr) -> PngFile => PngFile { value: p }

fn read_image_dimensions(file_handle ptr) -> (int, int) {
    ro png = nova_fn_png_create_read_struct()
    ro info = nova_fn_png_create_info_struct(png)
    nova_fn_png_init_io(png, file_handle)
    nova_fn_png_read_info(png, info)
    ro w = nova_fn_png_get_image_width(png, info)
    ro h = nova_fn_png_get_image_height(png, info)
    nova_fn_png_destroy_read_struct(png, info)
    (w, h)
}
```

(C shim mirrors sqlite3 pattern — see `sqlite3_ffi.h`.)

## Example 3 — libcurl (synchronous HTTP GET)

```nova
module my_app.curl

type CurlHandle { ro value ptr }
type CurlResult | Success | Failed(int)

external fn nova_fn_curl_easy_init() -> ptr
external fn nova_fn_curl_easy_setopt_url(h ptr, url str) -> int
external fn nova_fn_curl_easy_setopt_write_to_buffer(h ptr) -> int
external fn nova_fn_curl_easy_perform(h ptr) -> int
external fn nova_fn_curl_easy_cleanup(h ptr) -> ()
external fn nova_fn_curl_get_response_body() -> str

fn CurlHandle.new() -> CurlHandle {
    ro raw = nova_fn_curl_easy_init()
    CurlHandle { value: raw }
}

fn CurlHandle @get(url str) -> (CurlResult, str) {
    nova_fn_curl_easy_setopt_url(self.value, url)
    nova_fn_curl_easy_setopt_write_to_buffer(self.value)
    ro rc = nova_fn_curl_easy_perform(self.value)
    ro body = nova_fn_curl_get_response_body()
    if rc == 0 { (CurlResult.Success, body) }
    else       { (CurlResult.Failed(rc), body) }
}

fn CurlHandle consume @close() -> () {
    nova_fn_curl_easy_cleanup(self.value)
}
```

## ABI cheat sheet

For external fn tuple returns, the C ABI is determined by element layout:

| Tuple | Sys V AMD64 | Windows x64 MSVC | macOS ARM64 |
|---|---|---|---|
| `(ptr, i32)` (12 bytes) | registers (`rax:rdx`) | hidden-out-ptr (`rcx`) | `X0:X1` |
| `(ptr, int)` (16 bytes) | registers (`rax:rdx`) | hidden-out-ptr | `X0:X1` |
| `(ptr, ptr)` (16 bytes) | registers | hidden-out-ptr | `X0:X1` |
| `(ptr, int, int)` (24 bytes) | hidden-out-ptr | hidden-out-ptr | hidden-out-ptr |
| Larger | hidden-out-ptr | hidden-out-ptr | hidden-out-ptr |

Nova does not override calling convention — the C compiler chooses based
on platform ABI. C-side shim and Nova-side declaration must produce
matching struct layout (Plan 115 D214 `#ifndef NOVA_TUPLE_TYPEDEF_<m>`
guard ensures single definition).

## Safety considerations

- **Ownership.** Nova GC does **not** track `ptr` values — these are
  FFI domain. Match every `_open()` / `_init()` / `_alloc()` with a
  `_close()` / `_destroy()` / `_free()`. Use `consume` methods and
  `defer` for leak resistance.
- **Lifetime.** A `ptr` from a C library is valid only until the
  matching cleanup call. Nova compile-time cannot enforce this; rely on
  pattern (consume + defer).
- **Null check.** Always check return values for `null ptr` before
  using. Many C libraries return NULL on allocation failure.
- **Thread-safety.** Most C libraries have thread-safety contracts. If
  Nova spawns fibers that touch the handle, ensure handle is either
  thread-safe or pinned to one fiber.

## Followups

| Marker | What | Status |
|---|---|---|
| `[M-115-newtype-constructor]` | tuple newtype `type X(ptr)` constructor + `.0` access | ✅ CLOSED 2026-06-01 (canonical syntax shipped) |
| `[M-115-ffi-build-pipeline]` | `nova build --c-shim path/to/file.c` user-shim link CLI | 🟡 deferred (V1 shims live в `nova_rt/`) |
| `[M-115-bindgen-tool]` | `nova bindgen header.h` auto-generated bindings | 🟡 deferred (major tooling, separate plan) |
| `[M-115-d126-deprecation]` | `external type X` D126 migration audit | 🟡 deferred (sequence: newtype-constructor ✓ → Plan 91.12 Pattern B → D126 retract) |
| `[M-115-tuple-gc-types]` | tuple elements GC-tracked types в external fn returns | 🟢 CLOSED as by-design (extern "C" boundary correctly excludes Nova-typed containers) |
| `[M-115-external-fn-method]` | receiver-method external fn | 🟢 CLOSED as not needed (free fn + Nova-side wrapper sufficient) |
| `[M-115-examples-ffi-real-build]` | real libsqlite3 link через vcpkg | 🟡 deferred (V1 ships embedded mini-sqlite-equivalent в `nova_rt/sqlite_mini_ffi.h` — proves end-to-end FFI mechanism без external dependency; real link → CI step) |
| `[M-115-null-ptr-to-option-after-npo]` | hard-retract `null ptr` после Plan 118 Option[*T] NPO | 🔴 plan ready, gated на Plan 118 V2 |
