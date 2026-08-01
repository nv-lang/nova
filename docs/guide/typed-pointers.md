<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Typed pointers (`*T` family) + `unsafe` model

> **Plan 118 / 118.5** (D216 V1 + V2 + V3 amends, **Plan 138.5 FINAL pointer
> model**, D2 amend, D214 amend, D32 amend, D184 amend). **Status:** ✅ FINAL
> pointer model ACTIVE 2026-06-11; parser/checker enforcement landed
> (`E_POINTER_PREFIX_MODIFIER`). Full NPO codegen + escape analysis —
> follow-on phases.

Production-grade FFI и низкоуровневая работа с памятью требуют типизированных
указателей. Plan 118 вводит `*T` family типов + `unsafe` model + Null
Pointer Optimization (NPO) для `Option[*T]` zero-cost null-safety.

## Pointer-mutability model: "arrow → box" (Plan 138.5 FINAL)

> **Plan 138.5 (2026-06-11) FINAL model — supersedes V2 right-binding +
> V3 propagation/safe-stopper:** the pointer **TYPE** carries pointee-mutability
> **ONLY**, written **POSTFIX** (the modifier sits *after* `*`). The old prefix
> forms `ro * T` / `mut * T` / `unsafe * T`, the `safe` stopper, and the
> `Unsafe(Pointer)` (`unsafe * T` = nullable-raw) form are **RETIRED** — see
> [retired forms](#retired-forms-plan-1385).

Think of a pointer as an **arrow** pointing at a **box** (the pointee):

- **The arrow target — written in the TYPE, postfix on `*`** — says *what you
  can do to the box*: `*mut T` (you may write into the box), `*ro T` ≡ `*T`
  (read-only box), `*unsafe T` (box may be uninitialized).
- **The arrow itself — the binding (`let` / `mut`, D36)** — says *whether you
  can re-point the arrow at another box*: `let p` = arrow is fixed,
  `mut p` = arrow can be re-pointed.

These are two independent axes. They never collide because one lives in the
**type** (postfix on `*`) and the other lives on the **binding** (before the
name):

```nova
mut p *mut T        // arrow re-pointable (mut binding) + box writable (*mut pointee)
let q *ro T         // arrow fixed (let binding)        + box read-only (*ro pointee)
mut p *ro T         // arrow re-pointable               + box read-only
let p *mut T        // arrow fixed                      + box writable
```

> **There is NO `mut *` / `ro *` / `unsafe *` prefix.** A modifier before `*`
> is a hard error `E_POINTER_PREFIX_MODIFIER` (precedent: Rust `*mut T` /
> `*const T` = pointee mutability; `let mut p` = re-pointability).

### Canonical forms (postfix pointee modifier)

```nova
*T                  // pointer to read-only T (default canonical; ≡ *ro T)
*ro T               // pointer to read-only T (explicit; identical to *T)
*mut T              // pointer to mutable T (deref-store `*p = v` allowed)
*unsafe T           // pointer to possibly-uninit T (MaybeUninit pointee)
Option[*T]          // NULLABLE pointer (NPO: None = null, 8 bytes)
Option[*unsafe T]   // FFI nullable-uninit ptr (None = null, Some = non-null
                    //   ptr к possibly-uninit pointee)
```

The modifier is **always postfix** — it attaches to the pointee of the `*` it
follows. The pointer value itself is **always non-null**; for nullable use
`Option[*T]` (zero-cost via NPO).

### Re-pointability is the binding (D36), not the type

```nova
mut p *T = &acc     // mut binding → p may be reassigned later (p = &other)
let q *T = &acc     // let binding → q is fixed (q = &other ⇒ E_REBIND)
```

A pointer variable obeys the **same** `let` / `mut` rule as every other
variable (D36). The type never encodes re-pointability.

### Pointer chains (multi-level) — postfix on each `*`

```nova
*mut *ro Node       // writable-target pointer  →  (read-only-target pointer → Node)
                    //   *p   = other_ptr   OK   (outer pointee mut)
                    //   **p  = new_value   ERR  (inner pointee ro)

*ro *mut Node       // read-only-target pointer →  (writable-target pointer → Node)
                    //   *p   = other_ptr   ERR  (outer pointee ro)
                    //   **p  = new_value   OK   (inner pointee mut)
```

Each modifier sits postfix, right after its `*`, and describes the target of
that `*` level. Read left-to-right.

### Pointer returns — pointee-mut by default (D184 amend)

D184 (return-type mut default) applies to the **pointee** for pointer returns:

```nova
fn alloc_cell() -> *T       // ≡ -> *ro T : returns a ptr to read-only T
fn alloc_mut()  -> *mut T   // returns a ptr to WRITABLE T
```

The re-pointability of the **result** is decided at the bind site, not in the
return type:

```nova
ro p = alloc_mut()          // p fixed (ro binding); *p = v still OK (pointee mut)
mut q = alloc_mut()         // q re-pointable + *q = v OK
```

This removes the old "two mut in return position" ambiguity (there is no outer
pointer-mut to choose).

### FFI out-param / uninit pointee

```nova
external fn os_read(fd int, buf *mut unsafe u8, n usize) -> int
//                              ^^^^^^^^^^^^^^^
//                       pointee writable (*mut) + possibly-uninit (unsafe);
//                       arrow re-pointability is the binding's concern
```

The pointee axes (`mut` / `ro` and `unsafe`) commute on a value-T pointee and
are both written postfix.

## Quick reference

| Need | Canonical FINAL form | Spec |
|---|---|---|
| Typed pointer (default ro target) | `*T` ≡ `*ro T` | [D216 §1](../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) |
| Pointer to writable target | `*mut T` | D216 §1 |
| Pointer to possibly-uninit target | `*unsafe T` | D216 §1 + V2 §V2.3 |
| Re-pointable pointer variable | `mut p *T` (binding) | D216 §2 + D36 |
| Fixed pointer variable | `let p *T` / `ro p *T` (binding) | D216 §2 + D36 |
| Nullable typed pointer | `Option[*T]` (NPO) | D216 §7 + V2 §V2.4 |
| FFI nullable-uninit pointer | `Option[*unsafe T]` | D216 §1 + V2 §V2.4 |
| Pointer return (writable target) | `-> *mut T` | D184 amend (Plan 138.5) |
| Pointer creation | `&value` | D216 §4 |
| Explicit deref | `*p` | D216 §5 |
| Auto-deref field/method | `p.field` / `p.method()` | D216 §5 |
| Pointer arithmetic | `unsafe { p + n }` → `*unsafe T` | D216 §6 |
| Unsafe boundary | `unsafe { ... }` block / `#unsafe fn` | D216 §8-9 |
| Function pointer для FFI | `*fn(Args) -> Ret` | D216 §10 |
| Opaque untyped (legacy) | `ptr` (D214 amend → `Option[*unsafe ()]` newtype) | D214 amend |

## `*T` family типов

**ABI:** все variants — single pointer-width (8 bytes на 64-bit; bootstrap
target 64-bit only). C type emission: `*ro T` → `const T*` (helps clang/MSVC
optimizer), `*mut T` / `*unsafe T` → `T*`.

**Validity:** every pointer value (`*T` / `*ro T` / `*mut T` / `*unsafe T`)
is **always non-null** (compile-time invariant). The nullable variant is
`Option[*T]` via NPO (single pointer, NULL = None; см. §V2.4 в spec).
`*unsafe T` describes a possibly-**uninitialized** pointee — the *pointer* is
still non-null; null is `Option[*unsafe T]` (`None`).

### Retired forms (Plan 138.5)

> ⚠️ **RETIRED (Plan 138.5, hard error — no grace period):** the prefix
> modifier forms `ro * T` / `mut * T` / `unsafe * T`, the `safe` propagation
> stopper, and the `Unsafe(Pointer)` interpretation of `unsafe * T`
> (nullable-raw pointer) are gone. They contradicted the "arrow → box" model
> (pointee-mut belongs in the type postfix; re-pointability belongs to the
> binding).

```nova
// RETIRED form:           FINAL canonical equivalent:
ro * T                  // *ro T            (postfix pointee modifier)
mut * T                 // *mut T
unsafe * T              // *unsafe T  — for a UNINIT pointee;
                        //   for a NULLABLE pointer use Option[*T]
mut * ro * Acc          // *mut *ro Acc     (postfix chain)
unsafe * safe T         // *T              (`safe` stopper removed)
```

- A modifier **before** `*` ⇒ `E_POINTER_PREFIX_MODIFIER`.
- The `safe` type-modifier ⇒ `E_SAFE_RETIRED` (nothing to stop propagating —
  there is no prefix-modifier propagation anymore).
- Re-pointability is expressed by the binding (`let` / `mut`), never `mut *`.

## Binding mut rule (D216 §2)

The leading `mut` / `ro` before the name is the **binding** (re-pointability,
D36). It is orthogonal to the postfix pointee modifier:

```nova
ro p *Acc                   // ro binding (fixed arrow); pointee ro
mut p *Acc                  // mut binding (re-pointable); pointee mut by default
mut p *Acc  ≡  mut p *mut Acc   // mut binding defaults pointee to mut
ro p *mut Acc               // valid edge: arrow fixed, pointee writable

mut q = &acc                // mut binding; pointee mut auto (no &mut acc needed)
ro p = &acc                 // ro binding; pointee ro auto
```

A `mut` binding defaults the pointee to `mut` (`mut p *Acc` ≡ `mut p *mut Acc`);
this reduces noise in hot-path FFI code. Re-pointability still comes from the
binding alone — there is no `mut *` prefix in the type.

## Chain order (D216 §3)

A pointee modifier is written **postfix**, right after each `*`, and applies to
the **target** of that `*` level; read left-to-right:

```nova
*mut *ro Acc        // writable-target pointer → (read-only-target pointer → Acc)
                    // *p  = другой_pointer OK   (outer pointee mut)
                    // **p = новое_значение ERR  (inner pointee ro)

*ro *mut Acc        // read-only-target pointer → (writable-target pointer → Acc)
                    // *p  = ...            ERR  (outer pointee ro)
                    // **p = ...            OK   (inner pointee mut)
```

Re-pointability of the variable holding the chain is, as always, the binding's
concern (`let` / `mut`).

## `&value` + escape analysis (D216 §4)

```nova
ro acc = Account { name: "Piter" }    // acc — heap reference
ro p = &acc                            // ro binding, type *ro Account; GC tracks acc

ro x = 42                              // x — stack primitive
ro p = &x                              // x auto-promoted to heap; type *ro i64
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
    ro p1 = some_ptr + 1            // *unsafe T (degrades — alignment/bounds gone)
    ro diff = p2 - p1               // isize (element count)
    *p1                              // deref of a degraded *unsafe T pointee
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
    // ro v = buf[2]                ← ERROR E_UNSAFE_REQUIRED (ptr[i] ≡ *(ptr+i))

    unsafe {
        ro x = *p                    // ✓ pointer deref
        ro v = buf[2]                // ✓ pointer index (ptr[i] syntax, [M-118-ptr-index-unsafe])
        ro y = malloc(1024)          // ✓ external fn returning pointer
    }
}
```

**Ops required inside `unsafe { }`:**

| Op | Example | Notes |
|---|---|---|
| Pointer deref | `*p` | reads/writes pointee |
| Pointer index | `p[i]` | `≡ *(p + i)` — no bounds check |
| Address-of | `&value` | produces typed pointer |
| Unsafe fn call | `ffi_write(...)` | `unsafe fn` body |
| Order compare | `p < q` | address ordering |

**`ptr[i]` pointer index** (D216 §8, closed `[M-118-ptr-index-unsafe]` 2026-06-09):
`ptr[i]` is syntactic sugar for `*(ptr + i)` — raw pointer arithmetic с offset,
no bounds check, pointer must be valid. Requires `unsafe { }` or `unsafe fn` body.

```nova
unsafe fn read_at(p *u8, i int) -> u8 { p[i] }   // ✓ inside unsafe fn

// Outside unsafe — compile error:
// ro v = buf[0]                 ← E_UNSAFE_REQUIRED
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

## Pointer Debug formatting (D216 §17, Plan 91.14 D229)

Canonical form — `${expr:?}` format-spec (Plan 91.14, D229):

```nova
unsafe {
    ro p *ro Account = &acc
    ro s = "ptr=${&value:?}"                  // V3 canonical (Plan 91.14)
    println("pointer: ${p:?}")                // → "pointer: 0x7f... -> Account"
}
```

- `${p:?}` debug-format interpolation — canonical pointer rendering inside
  `unsafe { ... }` (Plan 91.14 D229).
- `(*T).to_debug_str() -> str` — legacy built-in alias kept for
  backwards-compat; same semantics as `${p:?}`, allowed in unsafe only.
- `"${p}"` direct (Display) interpolation → `E_PTR_NO_DISPLAY_USE_DEBUG_STR`;
  diagnostic hint points to `${p:?}` (updated в Ф.5.3).
- Pointer addresses non-deterministic, leak ASLR info — explicit decision
  forced.

## Forbidden ops (D216 §15)

```nova
unsafe {
    ro arr = [1, 2, 3]
    ro p = &arr[1]               // ❌ E_ARRAY_INDEX_PTR_BANNED
                                  //   (array may realloc / GC compaction)
}

ro p Option[*u8] = null          // ❌ E_NULL_LITERAL_USE_NONE; use None
mut p *mut u8 = undefined        // ❌ E_UNDEFINED_USE_NONE_INIT_PATTERN
```

## Compiler diagnostic codes

### Errors

- `E_UNSAFE_REQUIRED` — pointer op outside unsafe context (`*p`, `p[i]`, `&v`, order-compare)
- `E_UNSAFE_CALL_REQUIRES_WRAP` — calling `#unsafe` fn без unsafe wrap
- `E_UNSAFE_T_READ_REQUIRES_WRAP` — `unsafe T` value read без `unsafe { }` block (V2 §V2.3)
- `E_UNSAFE_ARG_REQUIRES_WRAP` — `unsafe T` argument passed без unsafe wrap (V2 §V2.3b)
- `E_UNSAFE_T_NARROW_REQUIRES_UNSAFE` — `unsafe T → T` narrow cast без unsafe (V2 §V2.3b)
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
- `E_POINTER_PREFIX_MODIFIER` — modifier **before** `*` (`mut * T` / `ro * T` /
  `unsafe * T`); use postfix pointee `*mut T` / `*ro T` / `*unsafe T` or binding
  `mut x *T` (Plan 138.5, extends `E_INVALID_POINTER_MODIFIER`)
- `E_SAFE_RETIRED` — `safe` type-modifier used; the `safe` propagation stopper
  is retired (no prefix-modifier propagation to stop) (Plan 138.5)
- `E_PARSE_POINTER_TYPE_INCOMPLETE` — `*` без type
- `E_REALTIME_POINTER_OP` — pointer op в `#realtime fn` body
- `E_UNSAFE_HANDLER_BUILTIN_ONLY` — user-defined unsafe_handler attempt
- `E_AMP_CONST_BINDING` — `&const_value`
- `E_AMP_LITERAL` — `&42`
- `E_PTR_NO_DISPLAY_USE_DEBUG_STR` — `"${p}"` interpolation; hint suggests
  canonical `${p:?}` (Plan 91.14 D229) or legacy `p.to_debug_str()`
- `E_VARARG_NOT_SUPPORTED` — vararg FFI call
- `E_CAST_RAW_FN_TO_CLOSURE` — `*fn → fn` cast outside unsafe

#### V3 modifier-composition errors (D216 V3 amend, 2026-06-04)

- `E_MUTABILITY_CONFLICT_VALUE_TYPE` — type-position `ro mut T` / `mut ro T`
  на **value-type T** (primitives / value records / named tuples / anonymous
  tuples / Unit). Binding-form `ro x mut T` остаётся allowed (orthogonal
  binding modifiers). Spec §V3.1.
- `E_MODIFIER_ORDER` — safety modifier (`unsafe`) wrapping mutability modifier
  (`ro` / `mut`); reverse order required — **safety-inner / mutability-outer**
  (`ro unsafe T` ✅ / `unsafe ro T` ❌), consistent with `external unsafe fn`.
  Applies to value-T and to postfix **pointee** content (`*ro unsafe T` ✅ /
  `*unsafe ro T` ❌). Spec §V3.2 (FLIPPED in Plan 138.5).
- `E_REDUNDANT_TYPE_MODIFIER` — same-class modifier repetition. **Binding-level**
  (`ro x ro T`) and **postfix pointee chain** (`*ro ro T`) are kept; the old V3
  type-level *prefix*-chain cases (`ro * ro T`, `unsafe * unsafe T`) are moot —
  a prefix before `*` is already `E_POINTER_PREFIX_MODIFIER` (Plan 138.5). The
  `safe` escape hatch is retired. Spec §V3.4.

> **Note:** the V3 `safe` propagation stopper and the `Unsafe(Pointer)` form
> (`unsafe * T` = nullable-raw) are RETIRED (Plan 138.5). `safe` in
> type-position ⇒ `E_SAFE_RETIRED`; nullable pointers use `Option[*T]`.

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
| **Nova FINAL** (Plan 138.5) | **postfix pointee** `*ro T` / `*mut T` / `*unsafe T`; re-pointability = binding (`let`/`mut`) | (как V2) + value-T composition rules (§V3.1-V3.2) | `Option[*T]` (only) + NPO | (как V2) | (как V2) → `*unsafe T` |

## See also

- [`docs/plans/118-typed-pointers-and-unsafe.md`](plans/118-typed-pointers-and-unsafe.md) — Plan 118 core implementation roadmap
- [`docs/plans/118.1-ffi-intrinsics-and-cstring.md`](plans/118.1-ffi-intrinsics-and-cstring.md) — Plan 118.1 sub-plan (FFI intrinsics)
- [`docs/plans/118.2-slice-fat-pointer-and-uninit.md`](plans/118.2-slice-fat-pointer-and-uninit.md) — Plan 118.2 sub-plan (slice + uninit)
- [`docs/plans/118.3-pointer-concurrency-safety.md`](plans/118.3-pointer-concurrency-safety.md) — Plan 118.3 sub-plan (concurrency)
- [`docs/ffi-cookbook.md`](ffi-cookbook.md) — FFI patterns с ptr + tuple FFI (Plan 115 V1)
- [D216 V1](../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — spec foundation (typed-pointer family + unsafe model + NPO)
- [D216 FINAL pointer model (Plan 138.5)](../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — pointer type = pointee-mut postfix only; re-pointability = binding (D36); prefix modifiers ⇒ `E_POINTER_PREFIX_MODIFIER`; nullable = `Option[*T]` only; `safe` + `Unsafe(Pointer)` retired
- [D216 V2 amend](../spec/decisions/02-types.md#d216-v2-amend-2026-06-04--universal-right-binding-rule-для-type-level-modifiers--unsafe-t-first-class) — historical right-binding rule (§V2.1, RETRACTED) + first-class `unsafe T` value-wrapper (§V2.3, KEPT) + NPO recalc (§V2.4)
- [D216 V3 amend](../spec/decisions/02-types.md#d216-v3-amend-plan-1185-v3-2026-06-04--4-modifier-composition-rules) — value-T modifier-composition rules (V3.3/V3.4 superseded by Plan 138.5):
  - §V3.1 — storage-class-aware `ro+mut` adjacency ban (`E_MUTABILITY_CONFLICT_VALUE_TYPE`) — KEPT
  - §V3.2 — modifier ordering safety-inner / mutability-outer (`ro unsafe T`; `E_MODIFIER_ORDER`) — FLIPPED, KEPT
  - §V3.3 — right-binding propagation — SUPERSEDED (no prefix propagation)
  - §V3.4 — `safe` keyword stopper — RETIRED; `E_REDUNDANT_TYPE_MODIFIER` kept at binding/postfix-pointee level
- [D2 amend](../spec/decisions/04-effects.md#d2) — unsafe keyword restoration (effect-handler sugar)
- [D214 amend](../spec/decisions/02-types.md#d214-ptr-opaque-pointer-type--tuple-ffi-returns--opaque-handle-pattern) — ptr redefine
- [D32 amend](../spec/decisions/02-types.md#d32-семантика-передачи-параметров) — `&value` not Rust borrow
- [`examples/typed_pointers/`](../examples/typed_pointers/) — minimal working samples
