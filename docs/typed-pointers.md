<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Typed pointers (`*T` family) + `unsafe` model

> **Plan 118** (D216 + D2 amend + D214 amend + D32 amend). **Status:** 🟡
> V1 scaffolding landed (Ф.0–Ф.6 partial). Full enforcement + NPO codegen
> + escape analysis — Ф.4-Ф.9 follow-on phases.

Production-grade FFI и низкоуровневая работа с памятью требуют типизированных
указателей. Plan 118 вводит `*T` family типов + `unsafe` model + Null
Pointer Optimization (NPO) для `Option[*T]` zero-cost null-safety.

## Quick reference

| Need | Tool | Spec |
|---|---|---|
| Typed pointer (default ro) | `*T` | [D216 §1](../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) |
| Explicit readonly | `*ro T` | D216 §1 |
| Mutable pointer | `*mut T` | D216 §1 |
| Unsafe pointer (after arith) | `*unsafe T` | D216 §1 |
| Pointer creation | `&value` | D216 §4 |
| Explicit deref | `*p` | D216 §5 |
| Auto-deref field/method | `p.field` / `p.method()` | D216 §5 |
| Pointer arithmetic | `unsafe { p + n }` → `*unsafe T` | D216 §6 |
| Nullable typed pointer | `Option[*T]` (NPO) | D216 §7 |
| Unsafe boundary | `unsafe { ... }` block / `#unsafe fn` | D216 §8-9 |
| Function pointer для FFI | `*fn(Args) -> Ret` | D216 §10 |
| Opaque untyped (legacy) | `ptr` (D214 amend → `Option[*unsafe ()]` newtype) | D214 amend |

## `*T` family типов

```nova
*T              // ro pointer (default); short form of *ro T
*ro T           // explicit readonly pointer
*mut T          // mutable pointer (can write через *p / p.field = v)
*unsafe T       // unsafe pointer (после арифметики; deref требует ещё unsafe layer)
```

**ABI:** все variants — single pointer-width (8 bytes на 64-bit; bootstrap
target 64-bit only). C type emission: `*ro T` → `const T*` (helps clang/MSVC
optimizer), `*mut T` / `*unsafe T` → `T*`.

**Validity:** `*T` value — **always non-null** (compile-time invariant).
Nullable variant — `Option[*T]` через NPO (single pointer, NULL = None).

## Binding mut rule (D216 §2)

```nova
ro p *Acc                   // ro binding; pointer ro (cannot *p = ...)
mut p *Acc                  // mut binding; pointer mut auto (can *p = ...)
mut p *Acc == mut p *mut Acc          // эквивалентны
ro p *mut Acc               // valid edge case: binding ro, pointee mut

mut q = &acc                // pointer mut auto (no &mut acc needed)
ro p = &acc                 // pointer ro auto
```

Binding modifier пропагирует на pointer mutability по умолчанию. Reduces
noise в hot-path FFI code.

## Chain order (D216 §3)

```nova
*mut *ro Acc        // mut pointer НА (ro pointer на Acc)
                     // *p = другой_pointer OK
                     // **p = новое_значение ERROR (внутренний ro)

*ro *mut Acc        // ro pointer НА (mut pointer на Acc)
                     // *p = ... ERROR (внешний ro)
                     // **p = ... OK (внутренний mut)
```

Modifier перед `*` относится к ЭТОМУ pointer'у; read left-to-right.

## `&value` + escape analysis (D216 §4)

```nova
ro acc = Account { name: "Piter" }    // acc — heap reference
ro p = &acc                            // *ro Account; GC tracks acc

ro x = 42                              // x — stack primitive
ro p = &x                              // x auto-promoted to heap; *ro i64
```

**Critical:** `&value` это **НЕ Rust borrow** (D32 amend). Нет lifetime
checker, нет `'a` параметров, нет XOR aliasing. Safety обеспечивается:
1. Escape analysis + auto-promote (Go-style) для stack values
2. Unsafe gating — `&` + pointer deref только в unsafe context
3. GC honor-system — user обещает no GC trigger в unsafe (D216 §16)

## Auto-deref (D216 §5)

```nova
unsafe {
    p.field                 // ✓ auto-deref one level (read)
    p.method()              // ✓ auto-deref method call
    p.field = v             // ✓ auto-deref assignment (requires *mut T)
    *p                      // ✓ explicit deref
    (*p).field              // ✓ multi-level chain через explicit *
}
```

| Op | `*ro T` | `*mut T` |
|---|---|---|
| `p.field` (read) | ✓ | ✓ |
| `p.field = v` (assign) | ❌ E_POINTER_RO_ASSIGN | ✓ |
| `p.method()` (ro recv) | ✓ | ✓ |
| `p.method()` (mut recv) | ❌ E_POINTER_RO_MUT_METHOD | ✓ |

**One-level only.** Multi-level requires explicit `(*p).field` chain
(Go/D pattern; auto-deref recursion path-dependent = confusing).

## Pointer arithmetic (D216 §6)

```nova
unsafe {
    ro p1 = some_ptr + 1            // *unsafe T (degrades)
    ro diff = p2 - p1               // isize (element count)
    unsafe { *p1 }                   // *unsafe T deref требует ещё unsafe wrap
}
```

- `+`/`-` only в `unsafe { }` block, result `*unsafe T` для `ptr ± int`,
  `isize` для `ptr - ptr`
- Units: sizeof(T)-scaled (C/Rust convention)
- `*`/`/`/`%` — `E_PTR_ARITHMETIC_INVALID`

## Null safety: `Option[*T]` + NPO (D216 §7)

```nova
external fn malloc(sz usize) -> Option[*u8]
// C codegen: uint8_t* malloc(size_t sz);   // single pointer, NULL = None

unsafe {
    match malloc(1024) {
        Some(buf) => use(buf),               // buf: *u8 non-null guaranteed
        None      => Fail.throw(OutOfMemory),
    }
}
```

**NPO applies к:** `Option[*T]`, `Option[*fn(...)]`, `Option[ptr]`,
`Option[Newtype-over-pointer]`.

**Excluded:** `Option[Option[*T]]` — tagged fallback + `W_OPTION_DOUBLE_NESTED`.

## `unsafe { }` block (D216 §8, D2 amend)

```nova
fn safe_user_code() {
    // ro x = *p                    ← ERROR E_UNSAFE_REQUIRED

    unsafe {
        ro x = *p                    // ✓
        ro y = malloc(1024)          // ✓ external fn returning pointer
    }
}
```

**Implementation:** sugar над built-in `unsafe_handler` effect handler.

```nova
unsafe { expr }
// ≡
with unsafe_handler { perform UnsafeOps.<op>(expr) }
```

D2 spirit (всё — эффекты) preserved через built-in `unsafe_handler`
(not user-overridable). No effect propagation up — encapsulates per fn
(canonical Rust pattern).

## `#unsafe` fn attribute (D216 §9)

```nova
#unsafe
fn ffi_wrapper(p *T) -> T {
    *p                              // ✓ body implicitly unsafe context
}

fn safe_caller() {
    // ffi_wrapper(p)               ← ERROR E_UNSAFE_CALL_REQUIRES_WRAP
    unsafe {
        ro x = ffi_wrapper(p)       // ✓
    }
}
```

- `#unsafe fn` body имплицитно unsafe context
- Каллеру требуется `unsafe { }` wrap (even another `#unsafe` fn — visual marker)
- NO effect propagation up

## `*fn(...)` function pointers (D216 §10)

```nova
external fn libuv_set_timer_cb(cb *fn(i64) -> ()) -> i64

fn my_callback(timeout i64) -> () { ... }       // no Fail

unsafe {
    libuv_set_timer_cb(my_callback as *fn(i64) -> ())
}
```

- Cast `fn → *fn` — captureless required (`E_CLOSURE_HAS_ENV`)
- Cast `*fn → fn` — unsafe (wraps в captureless closure)
- **Callback no-throw:** Fn-with-Fail cast → `*fn` — `E_CALLBACK_THROWS_OVER_C_ABI`
- **external fn no-Fail:** `external fn ... Fail -> ...` — `E_EXTERNAL_FN_FAIL_EFFECT`

C ABI текущей платформы (System V на Unix, MS x64 на Windows). No
explicit `extern "C"` keywords — single ABI V1.

## FFI handle allocation contract (D216 §18)

**Tuple newtype canonical для opaque handles** (zero-overhead):

```nova
type Sqlite3Handle(*sqlite3)               // stack, single pointer ABI
external fn open(path str) -> (Option[Sqlite3Handle], i64)
```

vs record form (extra indirection — pointer-to-struct ABI):

```nova
type DbSession {
    ro handle Sqlite3Handle
    ro path str
    ro opened_at Time
}                                           // record — для handles с extra state
```

Migration Plan 115 V1 cookbook examples (record form) → tuple newtype
(zero-overhead) tracked в `[M-118-handle-migration]`.

## GC honor-system (D216 §16)

Внутри `unsafe { ... }` user **обещает** no GC trigger между pointer
creation и use. GC trigger = heap allocation, yield-point (await/spawn/
supervised), string formatting which allocates, calls to `#parks`/`#wakes`
fns.

Compiler emits `W_UNSAFE_GC_TRIGGER` warning per violation site.
Silence: `// noqa: W_UNSAFE_GC_TRIGGER` line marker.

V1 GC = Boehm conservative → не двигает объекты → V1 безопасно warning'ом.
Future moving GC потребует formal pin API (`[M-118-pin-api]` followup).

## Pointer Debug formatting (D216 §17)

```nova
unsafe {
    ro p *Account = &acc
    println("pointer: ${p.to_debug_str()}")   // → "pointer: 0x7f... -> Account"
}
```

- `(*T).to_debug_str() -> str` built-in method (in unsafe only)
- `"${p}"` direct interpolation → `E_PTR_NO_DISPLAY_USE_DEBUG_STR`
- Pointer addresses non-deterministic, leak ASLR info — explicit decision
  forced

## Forbidden ops (D216 §15)

```nova
unsafe {
    ro arr = [1, 2, 3]
    ro p = &arr[1]               // ❌ E_ARRAY_INDEX_PTR_BANNED
                                  //   (array may realloc / GC compaction)
}

ro p Option[*u8] = null          // ❌ E_NULL_LITERAL_USE_NONE; use None
mut p *u8 = undefined            // ❌ E_UNDEFINED_USE_NONE_INIT_PATTERN
```

## Compiler diagnostic codes

### Errors

- `E_UNSAFE_REQUIRED` — pointer op outside unsafe context
- `E_UNSAFE_CALL_REQUIRES_WRAP` — calling `#unsafe` fn без unsafe wrap
- `E_ARRAY_INDEX_PTR_BANNED` — `&arr[i]`
- `E_NULL_LITERAL_USE_NONE` — `null` literal used (general); use `None`
- `E_NULL_PTR_RETRACTED_USE_OPTION` — `null ptr` retracted; use `Option[ptr] = None`
- `E_UNDEFINED_USE_NONE_INIT_PATTERN` — `undefined` used
- `E_CLOSURE_HAS_ENV` — fn → *fn cast с closure env
- `E_CALLBACK_THROWS_OVER_C_ABI` — Fn-with-Fail → *fn cast
- `E_EXTERNAL_FN_FAIL_EFFECT` — external fn с Fail effect
- `E_PTR_ARITHMETIC_INVALID` — `p * 2`, `p / 4`, etc.
- `E_POINTER_RO_ASSIGN` — `*p = v` / `p.field = v` где p ro
- `E_POINTER_RO_MUT_METHOD` — `p.mut_method()` где p ro
- `E_PTR_CAST_INVALID_TARGET` — `p as bool / f64 / ...`
- `E_INVALID_POINTER_MODIFIER` — `*const T` and др.
- `E_DUPLICATE_POINTER_MODIFIER` — `*ro mut T`
- `E_PARSE_POINTER_TYPE_INCOMPLETE` — `*` без type
- `E_REALTIME_POINTER_OP` — pointer op в `#realtime fn` body
- `E_UNSAFE_HANDLER_BUILTIN_ONLY` — user-defined unsafe_handler attempt
- `E_AMP_CONST_BINDING` — `&const_value`
- `E_AMP_LITERAL` — `&42`
- `E_PTR_NO_DISPLAY_USE_DEBUG_STR` — `"${p}"` interpolation
- `E_VARARG_NOT_SUPPORTED` — vararg FFI call
- `E_CAST_RAW_FN_TO_CLOSURE` — `*fn → fn` cast outside unsafe

### Warnings

- `W_UNSAFE_GC_TRIGGER` — GC trigger внутри unsafe с active pointer in scope
- `W_PTR_AS_USIZE_GC_HASH_HAZARD` — `p as usize` как HashMap key
- `W_OPTION_DOUBLE_NESTED` — `Option[Option[*T]]` NPO fallback

## Mainstream comparison

| Язык | Typed ptr | Unsafe model | Null safety | Auto-deref | Pointer arith |
|---|---|---|---|---|---|
| Rust | `*const T`/`*mut T`/`&T`/`&mut T` | `unsafe {}` + `unsafe fn` | `Option<&T>` + NPO | через ref | unsafe only |
| Zig | `*T`/`*const T`/`[*]T` | (cast intrinsics) | `?*T` + NPO | `.*` postfix + `.` | `+` для `[*]T` |
| C# | `T*` / `ref T` / `in T` / `out T` | `unsafe` modifier | `T?` | `p->field` arrow | unsafe only |
| Swift | `UnsafePointer<T>` / `UnsafeMutablePointer<T>` | Type-based prefix | Optional + NPO | `.pointee` | only `.advanced(by:)` |
| D | `T*` / `ref T` / `scope T*` | `@safe`/`@trusted`/`@system` | `Nullable!T` | `p.field` auto | `@system` only |
| Go | `*T` (managed) / `unsafe.Pointer` | `unsafe` package | Nil runtime | `p.field` auto | `unsafe.Pointer` only |
| **Nova V1** (Plan 115) | `ptr` only | (нет) | `null ptr` | (нет) | banned |
| **Nova V2** (Plan 118) | **`*T` family** + `unsafe` | `unsafe { }` + `#unsafe` (D2 amend) | `Option[*T]` + NPO | `p.field`/`p.method()` one-level | gated unsafe → `*unsafe T` |

## See also

- [`docs/plans/118-typed-pointers-and-unsafe.md`](plans/118-typed-pointers-and-unsafe.md) — Plan 118 core implementation roadmap
- [`docs/plans/118.1-ffi-intrinsics-and-cstring.md`](plans/118.1-ffi-intrinsics-and-cstring.md) — Plan 118.1 sub-plan (FFI intrinsics)
- [`docs/plans/118.2-slice-fat-pointer-and-uninit.md`](plans/118.2-slice-fat-pointer-and-uninit.md) — Plan 118.2 sub-plan (slice + uninit)
- [`docs/plans/118.3-pointer-concurrency-safety.md`](plans/118.3-pointer-concurrency-safety.md) — Plan 118.3 sub-plan (concurrency)
- [`docs/ffi-cookbook.md`](ffi-cookbook.md) — FFI patterns с ptr + tuple FFI (Plan 115 V1)
- [D216](../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — spec foundation
- [D2 amend](../spec/decisions/04-effects.md#d2) — unsafe keyword restoration
- [D214 amend](../spec/decisions/02-types.md#d214-ptr-opaque-pointer-type--tuple-ffi-returns--opaque-handle-pattern) — ptr redefine
- [D32 amend](../spec/decisions/02-types.md#d32-семантика-передачи-параметров) — `&value` not Rust borrow
- [`examples/typed_pointers/`](../examples/typed_pointers/) — minimal working samples
