**English** | [Русский](ffi-cookbook.ru.md)

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova FFI Cookbook

> **Scope.** The mechanics of the `.nv` ↔ native boundary: `extern "C"`,
> opaque/typed pointers, `CStr`, tuple-by-value, C-ABI checking, and **how
> to plug native artifacts into the build** (`[ffi]` / `[ffi.staticlib]`).
> Foundational FFI — Plan 115 D214; the typed-pointer family (`*T`, `*mut
> T`, `Option[*T]`-NPO) is **merged** (Plan 118/138.5/174.x; the sections
> below are no longer "preview").
>
> **How to build a MODULE** (package layout, `nova.toml`, stability, tests) —
> the general guide [authoring-a-module](authoring-a-module.md) (native-backed —
> its §7). Module design conventions (effect plumbing, types, errors) —
> [module-conventions](../dev/module-conventions.md). Naming of external
> packages (`nova-<package>`) —
> [D78 amendment Plan 195](../../spec/decisions/07-modules.md#именование-внешних-пакетов-репозиториев-амендмент-plan-192-2026-07-10).
>
> ⚠️ **Plan 134 (2026-06-09): `ptr` built-in type removed.** Use `*()` (pointer
> to unit type = `void*` in C) everywhere `ptr` appeared. Compiler emits
> `E_TYPE_REMOVED_PTR_USE_UNIT_PTR` for `ptr` in type position.

This cookbook shows how to bind Nova code to third-party C libraries —
sqlite3, libpng, libcurl — using the foundational FFI primitives
introduced in Plan 115.

## Quick reference

| Need | Tool | Spec |
|---|---|---|
| Opaque pointer | `*()` (pointer to unit = `void*`) | [D214](../../spec/decisions/02-types.md#d214) / [Plan 134](../plans/134-remove-ptr-type.md) |
| NULL literal | `0 as *()` | D214 amend Plan 134 |
| Typed handle | `type X { ro value *() }` record | D214 §3 |
| Multi-value return | `(T1, T2)` tuple-by-value | D214 §2 |
| External fn declaration | `external fn name(args) -> ret` | [D82](../../spec/decisions/03-syntax.md#d82) |
| Resource cleanup | `consume close()` method + `defer` | [D90 / D131](../../spec/decisions/03-syntax.md#d90) |

## Pointer modifier rules (FINAL — Plan 138.5)

When writing FFI signatures with pointer/typed wrappers, the pointee
modifier is written **postfix**, right after `*` (`*mut T` / `*ro T` /
`*unsafe T`). **A prefix before `*` is forbidden** (`mut * T` / `ro * T` /
`unsafe * T` → `E_POINTER_PREFIX_MODIFIER`). Whether a pointer can be
re-pointed is a property of the **binding** (`let` / `mut`), not of the
type.

Quick cheat sheet:

- `*T` ≡ `*ro T` — pointer to read-only T (default)
- `*mut T` — pointer to writable T (the caller may change the pointee)
- `*unsafe T` — pointer to possibly-uninit T (MaybeUninit analog); the
  pointer itself is non-null
- `Option[*T]` — a **nullable** pointer (NPO, 8 bytes); this replaces the
  old `unsafe * T`
- `Option[*unsafe T]` — FFI nullable-uninit pointer (None = null, Some =
  non-null ptr to uninit)
- `*mut *ro Acc` — postfix chain (writable-target ptr to read-only-target
  ptr to Acc)
- `mut p *mut T` — mut binding (p is re-pointable) + mut pointee; `let q
  *ro T` — fixed binding + ro pointee

Full rules (arrow→box model, value-T composition §V3.1/§V3.2) — see [`docs/guide/typed-pointers.md`](typed-pointers.md). Spec — [D216 §1 FINAL](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) + [Plan 138.5](../plans/138.5-d216-v2-v3-simplification.md).

## Layered FFI pattern

```
LAYER 1  Nova public API           Database.open(path)
   ↓
LAYER 2  Nova wrapper              construct typed handle from raw return
   ↓
LAYER 3  external fn declaration   typed handle + tuple return
            external fn nova_fn_sqlite3_open(path str) -> (*(), int)
   ↓
LAYER 4  C shim                    ~5-10 lines, adapts out-param → struct
            _NovaTuple_2_8_nova_ptr_8_nova_int
            nova_fn_sqlite3_open(nova_str path) { ... }
   ↓
LAYER 5  Actual C library          sqlite3_open(path, &db_out)
```


## Typed handles (canon 2026-07-09)

Declare every C handle as a `C`-prefixed newtype IN the extern signatures —
never let a bare `int`/`*()` handle flow through Nova code:

```nova
type CBrotliHandle(int)
extern "C" fn brotli_dec_new() -> CBrotliHandle
extern "C" fn brotli_dec_feed(h CBrotliHandle, p *u8, len int) -> int
```

Lowering: newtype-over-int → `typedef nova_int Nova_CBrotliHandle;` — the C shim
ABI is untouched; the Nova side gets nominal typing for free. Null-check via
`(h as int) == 0`. Normative rule: module-conventions §4а. Known gap: methods
on a newtype receiver mis-dispatch by name ([M-newtype-receiver-method-dispatch]) —
call the typed externs directly until fixed.

## Plan 115 V1 setup

Nova V1 has these foundational pieces (commit `<plan-115-merge>`):
- `*()` (pointer to unit) emitted as `void*` in C output (Plan 134; previously `ptr` with `typedef void* nova_ptr`).
- Tuple-by-value returns from external fn — leverages mono'd
  `_NovaTuple_<arity>_<L_i>_<T_i>...` typedefs (Plan 59 mechanism).
- D82 amended (Plan 115): user-level `external fn` permitted in any
  module — no longer restricted to `std.runtime.*`.

Shipped since V1 (not "future" — already merged):
- **Tuple newtype `type X(*())` constructor** ✅ (`[M-115-newtype-constructor]`)
  — the canonical form; a single-field record is no longer needed.
- **User-shim build pipeline** ✅ — configured not via a CLI flag but
  declaratively in `nova.toml`: `[ffi]` (ready-made `.c` shims + system
  `libs`) and `[ffi.staticlib]` (a staticlib built on the fly, Plan 195).
  When the module is `import`ed, the artifacts are compiled/linked
  automatically — no need to recompile the Nova compiler. See
  ["Build pipeline"](#build-pipeline--ffi-and-ffistaticlib-manifest) below.
- **Typed-pointer family** ✅ (`*T`/`*mut T`/`Option[*T]`-NPO/`CStr`) — Plan
  118/138.5; see the sections below.

Still a followup:
- Auto-generated bindings from C headers — `[M-115-bindgen-tool]`
  (`nova bindgen header.h`, a separate tooling plan).

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

// Typed handles — V1 record form (tuple newtype `type X(*())`).
type Db { ro value *() }
type Stmt { ro value *() }

// External declarations matching the C shim.
external fn nova_fn_sqlite3_open(path str) -> (*(), int)
external fn nova_fn_sqlite3_close(db *()) -> int
external fn nova_fn_sqlite3_exec(db *(), sql str) -> int
external fn nova_fn_sqlite3_prepare(db *(), sql str) -> (*(), int)
external fn nova_fn_sqlite3_step(stmt *()) -> int
external fn nova_fn_sqlite3_column_int(stmt *(), col int) -> int
external fn nova_fn_sqlite3_finalize(stmt *()) -> int

// SQLite return codes (extract subset).
const SQLITE_OK   int = 0
const SQLITE_ROW  int = 100
const SQLITE_DONE int = 101

type DbError | OpenFailed(int) | ExecFailed(int) | PrepareFailed(int)

// Open database, wrap raw ptr in a typed Db handle.
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

// Close. consume — after @close, the handle is invalid (D131).
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

- **Typed handle.** `type Db { ro value *() }` makes `Db` nominally
  distinct from raw `*()` — passing wrong handle is compile error.
- **Tuple return.** `nova_fn_sqlite3_open` returns `(*(), int)`. Nova
  destructures: `ro (raw, rc) = nova_fn_sqlite3_open(path)`.
- **Cleanup.** `fn Db consume @close()` — invalidates handle, prevents
  use-after-close via consume bit (D131). Combine with `defer
  db.@close()` for leak resistance.
- **Error mapping.** Wrap C return codes in a Nova sum type for
  type-safe error handling.

## Example 2 — libpng (read PNG into pixel buffer)

```nova
module my_app.png

type PngFile { ro value *() }
type PngInfo { ro value *() }

external fn nova_fn_png_create_read_struct() -> *()
external fn nova_fn_png_create_info_struct(png *()) -> *()
external fn nova_fn_png_init_io(png *(), fp *()) -> int
external fn nova_fn_png_read_info(png *(), info *()) -> int
external fn nova_fn_png_get_image_width(png *(), info *()) -> int
external fn nova_fn_png_get_image_height(png *(), info *()) -> int
external fn nova_fn_png_destroy_read_struct(png *(), info *()) -> ()

fn PngFile.from_handle(p *()) -> PngFile => PngFile { value: p }

fn read_image_dimensions(file_handle *()) -> (int, int) {
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

type CurlHandle { ro value *() }
type CurlResult | Success | Failed(int)

external fn nova_fn_curl_easy_init() -> *()
external fn nova_fn_curl_easy_setopt_url(h *(), url str) -> int
external fn nova_fn_curl_easy_setopt_write_to_buffer(h *()) -> int
external fn nova_fn_curl_easy_perform(h *()) -> int
external fn nova_fn_curl_easy_cleanup(h *()) -> ()
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
| `(*(), i32)` (12 bytes) | registers (`rax:rdx`) | hidden-out-ptr (`rcx`) | `X0:X1` |
| `(*(), int)` (16 bytes) | registers (`rax:rdx`) | hidden-out-ptr | `X0:X1` |
| `(*(), *())` (16 bytes) | registers | hidden-out-ptr | `X0:X1` |
| `(*(), int, int)` (24 bytes) | hidden-out-ptr | hidden-out-ptr | hidden-out-ptr |
| Larger | hidden-out-ptr | hidden-out-ptr | hidden-out-ptr |

Nova does not override calling convention — the C compiler chooses based
on platform ABI. C-side shim and Nova-side declaration must produce
matching struct layout (Plan 115 D214 `#ifndef NOVA_TUPLE_TYPEDEF_<m>`
guard ensures single definition).

## Safety considerations

- **Ownership.** Nova GC does **not** track `*()` values — these are
  FFI domain. Match every `_open()` / `_init()` / `_alloc()` with a
  `_close()` / `_destroy()` / `_free()`. Use `consume` methods and
  `defer` for leak resistance.
- **Lifetime.** A `*()` from a C library is valid only until the
  matching cleanup call. Nova compile-time cannot enforce this; rely on
  pattern (consume + defer).
- **Null check.** Always check return values for null (`0 as *()`) before
  using. Many C libraries return NULL on allocation failure.
- **Thread-safety.** Most C libraries have thread-safety contracts. If
  Nova spawns fibers that touch the handle, ensure handle is either
  thread-safe or pinned to one fiber.

## Typed pointers + unsafe model (Plan 118 — merged)

> **Status:** merged (Plan 118 → 138.5 FINAL D216; unsafe fn keyword — 118.1.7;
> C-ABI checker — 174.6). Reference doc: [`docs/guide/typed-pointers.md`](typed-pointers.md).
> Plan: [`docs/plans/118-typed-pointers-and-unsafe.md`](../plans/118-typed-pointers-and-unsafe.md).
> Below — the evolution of FFI patterns from opaque `*()` to typed `*T`; both
> variants compile today (opaque — legacy-compatible, typed — preferred).

FFI patterns have moved from opaque `*()` to the typed pointer family `*T`
for type-safe FFI with buffers / structs / nullable returns:

```nova
// Plan 115 V1 / Plan 134 (current — works today):
external fn nova_sqlite3_open(path str) -> (*(), int)

ro (h, rc) = nova_sqlite3_open(path)
if rc != 0 { Fail.throw(DbError.OpenFailed(rc)) }

// Plan 118 V2 (typed + nullable + NPO):
external fn sqlite3_open(path str) -> (Option[Sqlite3Handle], i64)
type Sqlite3Handle(*sqlite3)               // tuple newtype, zero-overhead

unsafe {
    match sqlite3_open(path) {
        (Some(h), 0) => use_handle(h),
        (None, rc)   => Fail.throw(DbError.OpenFailed(rc)),
        (Some(_), rc) => Fail.throw(DbError.OpenFailed(rc)),  // C bug
    }
}
```

**Key improvements:**

| | Plan 115 V1 / Plan 134 (`*()`) | Plan 118 V2 (*T family) |
|---|---|---|
| Type safety | ❌ opaque `*()` cast by hand | ✓ compile-time pointee check |
| Mutability | ❌ no distinction | ✓ `*ro T` / `*mut T` |
| Null safety | ❌ `0 as *()` runtime check | ✓ `Option[*T]` + NPO zero-cost |
| FFI buffer | ❌ untyped `*()` + manual offset | ✓ `*ro u8` / `*mut u8` typed |
| Callback registration | ❌ N/A | ✓ `*fn(Args) -> Ret` |

**Migration path:**
- `ptr` → `*()` (Plan 134 — compiler error on bare `ptr` in type position)
- `0 as ptr` → `0 as *()`
- `null ptr` literals (already retracted Plan 118 A23) → `0 as *()`
- Record handle wrappers `type X { ro value *() }` → tuple
  newtype `type X(*)()` or `type X(*T)` for a zero-overhead ABI

See [`docs/guide/typed-pointers.md`](typed-pointers.md) for the full
reference documentation and [`examples/typed_pointers/`](../../examples/typed_pointers/)
for minimal working samples.

## Plan 118.1 — FFI intrinsics (foundation)

### CStr handle (type-safe const char*)

```nova
import std.ffi.cstr.{CStr}

// External fn principal pattern — a typed handle instead of a bare *u8
external fn c_strlen(s CStr) -> i64
external fn c_printf(fmt CStr) -> i32
```

CStr backing type: `*u8` (Plan 118 typed pointer). The ABI marshals to
`const char*` / `uint8_t*`.

**Conversion methods (`@to_cstr`, copy-based, Plan 199 / D418, 2026-07-11):**

```nova
ro s = "hello"
ro c = s.to_cstr()              // GC-allocs a fresh byte_len()+1 NUL-terminated copy (panics on embedded NUL)

// Zero-alloc overload — copy into a caller-provided buffer (hot FFI paths):
ro buf = unsafe { RawMem.alloc(64) }
ro c2 = s.to_cstr(buf, 64)      // copies ≤63 bytes + '\0'; TRUNCATES if longer, no scan

// Direct usage in an FFI call:
ro n = c_strlen(s.to_cstr())
```

Pure-Nova implementation in `std/src/ffi/cstr.nv`: `str` carries no trailing-NUL
guarantee (D418 retracts D26 §Nul-termination), so BOTH `to_cstr` overloads
COPY — there is no zero-copy path. `to_cstr()` allocates a fresh GC-managed
`byte_len()+1`-byte buffer, copies the bytes, and appends `\0` — after an O(n)
embedded-NUL scan ([M-118.1-cstr-nul-check]). `to_cstr(buf, buf_size)` copies
into the caller's buffer, clamping to `buf_size - 1` + terminator (truncating,
no scan — the explicit "I own the buffer" hot path; `as_cstr`/`as_cstr_unchecked`
are RETIRED, `to_` names the copy correctly).

### addr_of / addr_of_mut (Zig-style pointer creation)

```nova
unsafe {
    ro x = 42
    ro p = addr_of(x)         // *T pointer to a local
    assert(p.read() == 42)
}

unsafe {
    mut buf = 0
    ro p = addr_of_mut(buf)   // *T (mut binding required)
    p.write(100)               // codegen TBD
}
```

Equivalent to the `&x` operator (UnOp::AddrOf), rewriter-desugared.
Use when explicit function-call syntax improves FFI readability.
Same enforcement: unsafe context required, #realtime ban, lvalue
validation (E_AMP_LITERAL / E_AMP_RECORD_LITERAL / E_ARRAY_INDEX_PTR_BANNED).

### RawMem intrinsics (bulk memory ops)

```nova
import std.runtime.raw_mem.{RawMem}

unsafe {
    RawMem.copy(src, dst, n_bytes)              // memmove-safe
    RawMem.copy_nonoverlapping(src, dst, n)     // memcpy fast-path
    RawMem.fill(dst, byte_value, n)             // memset
    ro cmp = RawMem.compare(a, b, n)            // memcmp
}
```

### Typed read/write on a primitive `*T`

```nova
unsafe {
    ro p = addr_of(some_int)
    ro v = p.read()                  // typed primitive read
    p.write(100)                     // typed write (on *mut T)
    ro v_vol = p.read_volatile()     // MMIO read
    p.write_volatile(0xDEAD)         // MMIO write
}
```

### Cross-refs

- Spec D216 (typed pointers) + §22 (CStr type) — `spec/decisions/02-types.md`
- Spec D418 §`str` without a NUL terminator; copy-based `CStr`/`to_cstr`
  (retracts D26 §«Nul-termination») — `spec/decisions/08-runtime.md`
- Plan-doc — `docs/plans/118.1-ffi-intrinsics-and-cstring.md`,
  `docs/plans/199-str-drop-nul-termination.md`

## unsafe fn — declaring and calling unsafe functions (Plan 118.1.7)

> Plan 118.1.7 migrates from `#unsafe fn` attribute to `unsafe fn` keyword (type-consistent
> with TypeRef::Unsafe from Plan 118.5 and `*unsafe fn(...)` fn-ptr type from Plan 118.1.6).
> `#unsafe fn` is now a hard error (`E_UNSAFE_ATTR_DEPRECATED`).

### Declaring an unsafe Nova function

```nova
// unsafe fn — body has implicit unsafe context (pointer ops allowed without unsafe {})
export unsafe fn read_first_byte(p *u8) -> u8 {
    // No `unsafe { }` needed here — the body of an unsafe fn is implicitly
    // unsafe, so the raw-pointer read below is permitted directly.
    p.read()
}
```

### Declaring an unsafe external (C) function

```nova
// external unsafe fn — requires unsafe {} at call site
// pointee-mut written postfix: `*mut u8` = writable target (FINAL, Plan 138.5)
external unsafe fn RawMem.copy(src *u8, dst *mut u8, n int) -> ()
external unsafe fn RawMem.fill(dst *mut u8, byte_value u8, n int) -> ()
```

### Calling an unsafe function

```nova
// Caller MUST wrap in unsafe {}  — E_UNSAFE_CALL_REQUIRES_WRAP otherwise
unsafe {
    RawMem.copy(src_ptr, dst_ptr, n)
    ro b = read_first_byte(src_ptr)
}
```

### unsafe fn as function pointer type

```nova
// addr_of(unsafe fn) propagates unsafe to fn-ptr type: *unsafe fn(...)
unsafe fn risky(p *u8) -> () { /* ... */ }
ro fn_ptr = addr_of(risky)   // type: *unsafe fn(p *u8) -> ()

// Calling via unsafe fn pointer also requires unsafe {}
unsafe { fn_ptr(some_ptr) }
```

### Cross-refs

- D216 §9 (unsafe fn keyword syntax) — `spec/decisions/02-types.md`
- D2 (unsafe effect model, Plan 118.1.7 amend) — `spec/decisions/04-effects.md`
- Plan-doc — `docs/plans/118.1.7-unsafe-fn-keyword-syntax.md`

## C-ABI types for `extern "C" fn` (Plan 174.6 / D282 rule 2 + D353)

> **Status:** Plan 174.6 M1/M2 (2026-07-04). The checker validates every
> `extern "C" fn` signature (params **and** return) against a recursive C-ABI
> type-list; non-C-ABI types → `E_FFI_NON_C_ABI_TYPE`. Spec:
> [D282 rule 2](../../spec/decisions/08-runtime.md#d282) + [D353](../../spec/decisions/08-runtime.md#d353).

### What may cross an `extern "C" fn` boundary

The set is defined recursively:

```
C_ABI  ::= Scalar | RawPtr | FnPtr | Option[RawPtr] | Tuple[C_ABI…] | ValueRecord{ C_ABI… }
Scalar ::= int | uint | i8..i64 | u8..u64 | f32 | f64 | bool | char
RawPtr ::= *T | *() | CStr
FnPtr  ::= *extern "C" fn(C_ABI…) -> C_ABI
```

| Category | C-ABI? | Notes |
|---|---|---|
| `int` / `uint` | ✅ | address-sized (`nova_int` = `intptr_t`, `nova_uint` = `uintptr_t`); **≠** `i64`/`u64` |
| `i8`..`i64`, `u8`..`u64`, `f32`, `f64`, `bool` | ✅ | fixed-width scalars |
| `char` | ✅ | `uint32_t` codepoint (validity is a Nova invariant; Rust `improper_ctypes` flags the analogue) |
| `*T`, `*()`, `CStr` | ✅ | any pointee; recursion stops at the address. `*()` = `void*`, `CStr` = `const char*`/`*u8` |
| `str` | ✅ | value-record `{ptr,len}` (D139) — POD struct. **NOT NUL-terminated**; use `s.as_ptr()`/`s.byte_len()` or `s.to_cstr()` for C strings |
| value-record `type X value {…}` | ✅ iff all fields C-ABI | by-value C struct; the `value` keyword is **mandatory** (without it → heap GC-record, by-reference, **not** C-ABI) |
| named-tuple `type X(a T, b U)` / anon tuple `(T, U)` | ✅ iff all elements C-ABI | by-value; multi-value returns |
| cyclic value-record `type Node value {val int, next *Node}` | ✅ | `*Node` is a raw pointer → recursion terminates |
| `Option[*T]` (any pointer) | ✅ | NPO: `None` = 0, `Some(p)` = `p` (zero-cost nullable pointer) |
| `*extern "C" fn(…) -> …` | ✅ | C callback (see below); signature types themselves C-ABI |
| `-> ()` (top-level return) | ✅ | lowers to C `void` |
| `()` as a **param/element** | ❌ | C has no unit type |
| `Vec[T]`, heap-record refs | ❌ | GC-managed, not POD |
| `Result[T,E]`, `Option[non-ptr]`, other sums | ❌ | tag+payload layout is not C-ABI |
| bare `fn(…)` closure, Nova-ABI `*fn(…)` | ❌ | fat/handler-carrying; a real C callback must be tagged `*extern "C" fn` |

**Layout assumption (S8).** A value-record/tuple passed by-value assumes
Nova-layout == C-layout (field order + padding). Nova emits the struct in
declaration order; matching an **external** C library's struct is the author's
contract (a mismatch is not yet compiler-caught — follow-up
`[M-174.6-ffi-struct-layout]`).

### Callbacks — `*extern "C" fn` (qsort comparator, libuv handler)

A Nova function passed to C as a callback must be tagged `*extern "C" fn(...)`.
The coercion `fn → *extern "C" fn` is accepted **iff** the fn is (1) C-ABI in
every arg/return, (2) captureless (no env), and (3) **effect-free** (declares no
effect at all). Reason for (3): C invokes the callback with **no Nova
handler-frame on the stack**, so any effect-operation (`Fail`, `IO`, a custom
algebraic effect) has nowhere to resolve → unsound. Violations →
`E_FFI_NON_C_ABI_TYPE` / `E_CLOSURE_HAS_ENV` / `E_CALLBACK_THROWS_OVER_C_ABI`.

```nova
module my_app.sortdemo

// C: void qsort(void* base, size_t n, size_t sz,
//               int (*cmp)(const void*, const void*));
extern "C" fn qsort(base *(), n uint, size uint,
                    cmp *extern "C" fn(*(), *()) -> i32) -> ()

// A captureless, effect-free free fn — coerces to the C callback type.
fn cmp_i32(a *(), b *()) -> i32 {
    // read both operands via typed pointers (unsafe), compare … (elided)
    0
}

fn sort_it(buf *(), n uint) {
    // `cmp_i32 as *extern "C" fn(...)` — accepted (captureless + effect-free + C-ABI)
    qsort(buf, n, 4 as uint, cmp_i32 as *extern "C" fn(*(), *()) -> i32)
}
```

libuv-style handler stored in a handle record (D353/M2 — the tag is legal as a
value-record **field**, and its signature is still validated as C-ABI):

```nova
module my_app.uvdemo

// A handle that carries a C-ABI callback pointer (validated by the checker).
type UvTimer value {
    handle *()
    on_tick *extern "C" fn(*()) -> ()
}

extern "C" fn uv_timer_start(t *(), cb *extern "C" fn(*()) -> (),
                             timeout u64, repeat u64) -> i32

fn on_tick(h *()) -> () { /* … effect-free … */ }
```

**What is rejected** (each a distinct `extern "C" fn` neg case):

```nova
extern "C" fn bad1(v Vec[int]) -> int              // E_FFI_NON_C_ABI_TYPE (GC)
extern "C" fn bad2(r Result[int, str]) -> int      // E_FFI_NON_C_ABI_TYPE (tagged union)
extern "C" fn bad3(x Option[int]) -> int           // E_FFI_NON_C_ABI_TYPE (no NPO)
extern "C" fn bad4(x ()) -> int                    // E_FFI_NON_C_ABI_TYPE (unit param)
extern "C" fn bad5(cb *fn(i64) -> i64) -> int      // E_FFI_NON_C_ABI_TYPE (Nova-ABI *fn)
// coercions:
//   (fn(x i64) => x*2) as *extern "C" fn(i64)->i64  → E_CLOSURE_HAS_ENV  (env)
//   throwing_fn as *extern "C" fn(...)              → E_CALLBACK_THROWS_OVER_C_ABI (Fail)
//   effectful_fn as *extern "C" fn(...)             → E_CALLBACK_THROWS_OVER_C_ABI (any effect)
```

### Ownership / pinning across the boundary

Nova pointers handed to C are **borrowed for the duration of the call** only.
The Boehm GC does not scan C-`malloc`'d memory, so a Nova pointer **retained**
by C past the call (stored in a C struct, captured by a callback registration)
can be collected → use-after-free. To keep a Nova object alive across calls,
pin it (keep a live Nova reference for the object's C-visible lifetime; a
dedicated pinning API is a follow-up). This mirrors the `str.as_ptr()` lifetime
rule (D294): the pointer is valid only while the `str` is live.

---

## Build pipeline — `[ffi]` and `[ffi.staticlib]` manifest

Everything above is about *writing* the `.nv` ↔ native boundary. This
section is about *plugging* native artifacts into the build so that
`import`ing the module pulls them in **automatically**, with no compiler
changes. The declaration lives in the package's `nova.toml`. Full "how to
build a module" — [authoring-a-module §7](authoring-a-module.md#7-native-backed-module-a-special-case).

### `[ffi]` — ready-made `.c` shims and system `.lib`s (Plan 115 D214)

For a thin C shim and linking an already-built system library:

```toml
[ffi]
c_shims      = ["native/sqlite3_shim.c"]            # compiled and linked
include_dirs = ["native/", "third_party/sqlite3/"]  # → clang -I
libs         = ["sqlite3"]                          # → clang -lsqlite3 / sqlite3.lib
```

Paths are relative to `nova.toml`; they're resolved to absolute paths
before invoking clang. `.h`-only inline shims are pulled in via
force-include (`-include`), `.c` files as a compilation unit. The `[ffi]`
section may be empty (an `FFI-aware` marker).

### `[ffi.staticlib]` — RETRACTED (Plan 195)

**Retracted by the owner on 2026-07-10 (Plan 195).** The section existed
(Plan 195) as a generalization of the `detect_tls`/`tls-cache`/`-lbcrypt
-lntdll` hardcoding into a manifest mechanism that built a native artifact
via cargo/make on the fly. It let a user native module require Rust/cargo
as part of its own build — which contradicts the toolchain canon
(**Nova compiler + clang**, `.nv → .c → binary`, NO Rust/cargo).
`compiler-codegen/tls_shim/` (a Rust staticlib, rustls) — the mechanism's
only user — was replaced with `compiler-codegen/nova_rt/tls_c_shim.c`
(mbedTLS, the ordinary `[ffi]` path). `FfiStaticlibConfig`/
`resolve_ffi_staticlib`/`[ffi.staticlib]` parsing has been removed
entirely from `manifest.rs`/`test_runner.rs`.

**The native-module canon is now `[ffi]` above, and only that**: a `.c`
shim (compiled by clang, which is in the toolchain) + optionally a
ready-made `.lib`/`.a` (linked, not built — vcpkg/system package/vendored
copy; see `detect_brotli`/`detect_boehm`/`detect_mbedtls` in
`test_runner.rs` for the pattern of conditionally linking a library that
is ALREADY built, rather than one built by a build script).

**Reference example.** `std/tls` (`nova_rt/tls_c_shim.c` + vcpkg mbedTLS)
— a real example: mbedTLS is installed via `vcpkg install` (see
`compiler-codegen/vcpkg.json`, gitignored per-checkout), `tls_c_shim.c`
is compiled/linked CONDITIONALLY based on whether `tls_*` symbols are
used (the same D337 mechanism as brotli), with no manifest declaration
in `std/nova.toml` at all — the linking lives entirely in
`test_runner.rs::build_command` (like `net.c`/`brotli_shim.c`).

---

## Followups

| Marker | What | Status |
|---|---|---|
| `[M-115-newtype-constructor]` | tuple newtype `type X(ptr)` constructor + `.0` access | ✅ CLOSED 2026-06-01 (canonical syntax shipped) |
| `[M-115-ffi-build-pipeline]` | user-shim build/link pipeline | ✅ CLOSED — implemented declaratively via `nova.toml` `[ffi]` (ready-made shims/libs, Plan 115). `[ffi.staticlib]` (a staticlib built on the fly, Plan 195) RETRACTED by the owner (Plan 195) — a native module must build WITHOUT Rust/cargo. See ["Build pipeline"](#build-pipeline--ffi-and-ffistaticlib-manifest) |
| `[M-115-bindgen-tool]` | `nova bindgen header.h` auto-generated bindings | 🟡 deferred (major tooling, separate plan) |
| `[M-115-d126-deprecation]` | `external type X` D126 migration audit | ✅ CLOSED: Plan 91.12 V2 hard retract — `external type X` is now a hard error E_EXTERNAL_TYPE_RETRACTED (sequence: newtype-constructor ✓ → Plan 91.12 Pattern B → D126 retract done) |
| `[M-115-tuple-gc-types]` | tuple elements GC-tracked types in external fn returns | 🟢 CLOSED as by-design (extern "C" boundary correctly excludes Nova-typed containers) |
| `[M-115-external-fn-method]` | receiver-method external fn | 🟢 CLOSED as not needed (free fn + Nova-side wrapper sufficient) |
| `[M-115-examples-ffi-real-build]` | real libsqlite3 link via vcpkg | 🟡 deferred (V1 ships embedded mini-sqlite-equivalent in `nova_rt/sqlite_mini_ffi.h` — proves end-to-end FFI mechanism with no external dependency; real link → CI step) |
| `[M-115-null-ptr-to-option-after-npo]` | hard-retract `null ptr` after Plan 118 Option[*T] NPO | ✅ CLOSED Plan 134 (2026-06-09) — `ptr` removed; use `*()` and `0 as *()` |
