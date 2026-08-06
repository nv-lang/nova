**English** | [Русский](typed-pointers.ru.md)

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Typed pointers (`*T` family) + `unsafe` model

> **Plan 118 / 118.5** (D216 V1 + V2 + V3 amends, **Plan 138.5 FINAL pointer
> model**, D2 amend, D214 amend, D32 amend, D184 amend, **Plan 174.5
> "everything through methods" amend**). **Status:** ✅ FINAL pointer model
> ACTIVE 2026-06-11; operator access to pointees RETIRED 2026-07-09 in favour
> of intrinsic methods (`E_POINTER_OP_USE_METHOD`); since 2026-08-05 the
> retired write forms are rejected by `nova check` too, and a bare
> `*uninit T` is enforced read-only.

Production-grade FFI and low-level memory work require typed pointers.
Plan 118 introduces the `*T` type family + the `unsafe` model + Null
Pointer Optimization (NPO) for `Option[*T]` zero-cost null-safety.

## Pointer-mutability model: "arrow → box" (Plan 138.5 FINAL)

> **Plan 138.5 (2026-06-11) FINAL model — supersedes V2 right-binding +
> V3 propagation/safe-stopper:** the pointer **TYPE** carries pointee-mutability
> **ONLY**, written **POSTFIX** (the modifier sits *after* `*`). The old prefix
> forms `ro * T` / `mut * T` / `unsafe * T`, the `safe` stopper, and the
> `Unsafe(Pointer)` (`unsafe * T` = nullable-raw) form are **RETIRED** — see
> [retired forms](#retired-forms-plan-1385).

Think of a pointer as an **arrow** pointing at a **box** (the pointee):

- **The arrow target — written in the TYPE, postfix on `*`** — says *what you
  can do to the box*: `*mut T` (you may write into the box), a bare `*T`
  (read-only box — the default; writing it out as `*ro T` is redundant and
  rejected, `E_REDUNDANT_POINTER_RO`), `*uninit T` (box may be uninitialized —
  and is still read-only; writing needs the composed `*mut uninit T`).
- **The arrow itself — the binding (`ro` / `mut`, D36)** — says *whether you
  can re-point the arrow at another box*: `ro p` = arrow is fixed,
  `mut p` = arrow can be re-pointed.

These are two independent axes. They never collide because one lives in the
**type** (postfix on `*`) and the other lives on the **binding** (before the
name):

```nova
mut p *mut T        // arrow re-pointable (mut binding) + box writable (*mut pointee)
ro q *T             // arrow fixed (ro binding)         + box read-only (*T pointee)
mut p *T            // arrow re-pointable               + box read-only
ro p *mut T         // arrow fixed                      + box writable
```

> **There is NO `mut *` / `ro *` / `uninit *` prefix.** A modifier before `*`
> is a hard error `E_POINTER_PREFIX_MODIFIER` (precedent: Rust `*mut T` /
> `*const T` = pointee mutability; `let mut p` = re-pointability).

### Canonical forms (postfix pointee modifier)

```nova
*T                  // pointer to read-only T (the ONLY read-only form —
                    //   `*ro T` is redundant, E_REDUNDANT_POINTER_RO)
*mut T              // pointer to mutable T (p.write(v) allowed)
*uninit T           // pointer to possibly-uninit T — READ-ONLY pointee
*mut uninit T       // writable + possibly-uninit pointee (the write opt-in)
Option[*T]          // NULLABLE pointer (NPO: None = null, 8 bytes)
Option[*uninit T]   // FFI nullable-uninit ptr (None = null, Some = non-null
                    //   ptr to a possibly-uninit pointee)
```

The modifier is **always postfix** — it attaches to the pointee of the `*` it
follows, and read-only is the pointee default (no modifier written for it).
The `uninit` axis is orthogonal to the `mut` axis: a bare `*uninit T` marks
the pointee possibly-uninitialized but does **not** grant writes — writing
requires the explicit composed `*mut uninit T` (order fixed by
`E_MODIFIER_ORDER`: mutability-outer / safety-inner).
The pointer value itself is **always non-null**; for nullable use `Option[*T]`
(zero-cost via NPO).

### Re-pointability is the binding (D36), not the type

```nova
mut p *T = &acc     // mut binding → p may be reassigned later (p = &other)
ro q *T = &acc      // ro binding → q is fixed (q = &other ⇒ E_REBIND)
```

A pointer variable obeys the **same** `ro` / `mut` rule as every other
variable (D36). The type never encodes re-pointability.

### Pointer chains (multi-level) — postfix on each `*`

```nova
*mut *Node          // writable-target pointer  →  (read-only-target pointer → Node)
                    //   p.write(other_ptr)   OK   (outer pointee mut)
                    //   p.read().write(v)    ERR  (inner pointee ro)

**mut Node          // read-only-target pointer →  (writable-target pointer → Node)
                    //   p.write(other_ptr)   ERR  (outer pointee ro)
                    //   p.read().write(v)    OK   (inner pointee mut)
```

Each modifier sits postfix, right after its `*`, and describes the target of
that `*` level. Read left-to-right. Access at every level goes through the
intrinsic methods (`.read()` / `.write(v)`) — there is no `*p` operator (see
[access is methods-only](#access-is-methods-only-plan-1745)).

### Pointer returns — pointee-mut by default (D184 amend)

D184 (return-type mut default) applies to the **pointee** for pointer returns:

```nova
fn alloc_cell() -> *T       // returns a ptr to read-only T (the pointee L3 default)
fn alloc_mut()  -> *mut T   // returns a ptr to WRITABLE T
```

The re-pointability of the **result** is decided at the bind site, not in the
return type:

```nova
ro p = alloc_mut()          // p fixed (ro binding); p.write(v) still OK (pointee mut)
mut q = alloc_mut()         // q re-pointable + q.write(v) OK
```

This removes the old "two mut in return position" ambiguity (there is no outer
pointer-mut to choose).

### FFI out-param / uninit pointee

```nova
extern "C" fn os_read(fd int, buf *mut uninit u8, n int) -> int
//                              ^^^^^^^^^^^^^^^
//                       pointee writable (*mut) + possibly-uninit (uninit);
//                       arrow re-pointability is the binding's concern
```

A bare `*uninit T` (no `mut`) is a read-only pointee: Nova-side writes
(`p.write(v)` / `p.write_at(i, v)` and the rest of the write family) are
rejected with `E_POINTER_RO_ASSIGN`. The write opt-in is always the explicit
composed `*mut uninit T` — for an FFI out-param, whose callee fills the
buffer, that is the form to declare.

## Access is methods-only (Plan 174.5)

> **D216 amend "everything through methods" (Plan 174.5, 2026-07-09):**
> value access and address arithmetic on raw pointers go through **intrinsic
> methods only**. The operator forms are RETIRED with a hard error
> `E_POINTER_OP_USE_METHOD` — including the read forms:
> `*p`, `*p = v`, `p[i]`, `p[i] = v`, `p ± i`, `p - q`, `p < q` (all order
> compares). `nova check` rejects both the write forms (since 2026-08-05)
> and the read forms `x = *p` / `y = p[i]` (since 2026-08-06) — previously
> they only failed at the build stage.

| Method | Replaces | Semantics |
|---|---|---|
| `p.read() -> T` | `*p` | plain deref read |
| `p.write(v T)` | `*p = v` | deref store — requires a `*mut` pointee |
| `p.read_at(i) -> T` | `p[i]` | `*(p+i)` read, element units, no bounds check |
| `p.write_at(i, v)` | `p[i] = v` | `*(p+i)` store — requires a `*mut` pointee |
| `p.offset(n) -> *T` | `p ± i` | address arithmetic, element units; the type does **NOT** degrade |
| `p.dist(q) -> int` | `p - q` | signed element count; order = the sign (`p < q` is retired) |
| `p.read_unaligned()` / `p.write_unaligned(v)` | — | memcpy semantics (unaligned access) |
| `p.read_volatile()` / `p.write_volatile(v)` | — | volatile access |
| `p.write(v *T) -> *mut T` | — | copy from a source pointer (no value copy) |
| `p.copy_from(src, n)` / `p.copy_to(dst, n)` | — | memmove; `_nonoverlapping` variants — memcpy |

```nova
mut buf []int = [10, 20, 30, 40]
unsafe {
    ro p = buf.ptr()
    assert(p.read_at(2) == 30)          // was p[2] — retired
    ro p2 = p.offset(2)                 // was p + 2 — retired; type stays *int
    assert(p2.read() == 30)             // was *p2 — retired
    assert(p2.dist(p) == 2)             // was p2 - p — retired
}
unsafe {
    mut q = buf.ptr()                   // mut receiver overload → *mut int
    q.write_at(1, 99)                   // was q[1] = 99 — retired
}
assert(buf[1] == 99)
```

**What remains an operator:** `p == q` / `p != q` (identity), `p as *U`
(cast; unsafe when `U ≠ T`), and one-level auto-deref **field access**
(next section). `[]`-indexing belongs to safe containers only (D138) —
pointers do not have it.

**Write capability** is checked in one place for the whole write family
(`.write` / `.write_at` / `.write_unaligned` / `.write_volatile` /
`.copy_from[_nonoverlapping]`): the pointee must be `*mut …`, otherwise
`E_POINTER_RO_ASSIGN`.

## Auto-deref field access (D216 §5)

One-level field access through a pointer works without an explicit deref:

```nova
type Counter { mut v int }

mut a = Counter { v: 1 }
ro p = &a                   // a is mut → p is *mut Counter
p.v = 5                     // ✓ field store via auto-deref (requires *mut pointee)
ro r = p.v                  // ✓ field read via auto-deref (any *T)
assert(a.v() == 5)
```

| Op | `*T` | `*mut T` |
|---|---|---|
| `p.field` (read) | ✓ | ✓ |
| `p.field = v` (assign) | ❌ E_POINTER_RO_ASSIGN | ✓ |

**One-level only.** For deeper chains read the pointer value first
(`p.read()`), then continue on the value.

> **Method calls through a pointer** (`p.method()`) work for regular
> methods. One narrow exception remains: calling a *field-accessor property
> method* through a pointer (`p.v()` for field `v`) currently fails to
> compile — the defect is reported and tracked. Read the field directly
> (`p.v`) or read the value first (`p.read().v()`).

## Quick reference

| Need | Canonical FINAL form | Spec |
|---|---|---|
| Typed pointer (default ro target) | `*T` (`*ro T` is redundant — `E_REDUNDANT_POINTER_RO`) | [D216 §1](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) |
| Pointer to writable target | `*mut T` | D216 §1 |
| Pointer to possibly-uninit target (read-only) | `*uninit T` | D216 §1 + V2 §V2.3 |
| Writable possibly-uninit target | `*mut uninit T` | D216 §V2.2 (№358) |
| Re-pointable pointer variable | `mut p *T` (binding) | D216 §2 + D36 |
| Fixed pointer variable | `ro p *T` (binding) | D216 §2 + D36 |
| Nullable typed pointer | `Option[*T]` (NPO) | D216 §7 + V2 §V2.4 |
| FFI nullable-uninit pointer | `Option[*uninit T]` | D216 §1 + V2 §V2.4 |
| Pointer return (writable target) | `-> *mut T` | D184 amend (Plan 138.5) |
| Pointer creation (safe, auto-promote) | `&value` | D216 §4 + 118.6 |
| Raw stack address (no promote) | `unsafe { raw &value }` | D216 §4 amend 2 (118.7) |
| Deref read / store | `p.read()` / `p.write(v)` | D216 amend (174.5) |
| Indexed read / store | `p.read_at(i)` / `p.write_at(i, v)` | D216 amend (174.5) |
| Pointer arithmetic | `p.offset(n)` / `p.dist(q)` | D216 amend (174.5) |
| Auto-deref field | `p.field` / `p.field = v` | D216 §5 |
| Unsafe boundary | `unsafe { ... }` block / `unsafe fn` | D216 §8-9 |
| Function pointer for FFI | `*fn(Args) -> Ret` | D216 §10 |
| Opaque untyped (legacy) | `ptr` (D214 amend → `Option[*uninit ()]` newtype) | D214 amend |

## The `*T` type family

**ABI:** all variants are single pointer-width (8 bytes on 64-bit; bootstrap
target 64-bit only). C type emission: `*T` → `const T*` (helps the clang/MSVC
optimizer), `*mut T` / `*uninit T` → `T*`.

**Validity:** every pointer value (`*T` / `*mut T` / `*uninit T`)
is **always non-null** (compile-time invariant). The nullable variant is
`Option[*T]` via NPO (single pointer, NULL = None; see §V2.4 in the spec).
`*uninit T` describes a possibly-**uninitialized** pointee — the *pointer* is
still non-null; null is `Option[*uninit T]` (`None`).

### Retired forms (Plan 138.5)

> ⚠️ **RETIRED (hard errors — no grace period):** three retirement layers.
> **Plan 138.5** removed the prefix modifier forms `ro * T` / `mut * T` /
> `unsafe * T`, the `safe` propagation stopper, and the `Unsafe(Pointer)`
> interpretation of `unsafe * T` (they contradicted the "arrow → box" model).
> **Plan 118.6/118.7** removed `addr_of()` / `addr_of_mut()`
> (`E_ADDR_OF_REMOVED`) in favour of safe `&x` + unsafe `raw &x`, and Plan
> 118.1.7 replaced the `#unsafe` fn attribute with the `unsafe fn` keyword
> (`E_UNSAFE_ATTR_DEPRECATED`). **Plan 174.5** retired the whole pointer
> operator family (`E_POINTER_OP_USE_METHOD` — see
> [access is methods-only](#access-is-methods-only-plan-1745)).

```nova
// RETIRED form:           FINAL canonical equivalent:
ro * T                  // *T               (postfix pointee modifier;
                        //   bare = ro, `*ro T` itself is E_REDUNDANT_POINTER_RO)
mut * T                 // *mut T
unsafe * T              // *uninit T  — for a UNINIT pointee (§10a rename,
                        //   was `*unsafe T`); for a NULLABLE pointer use Option[*T]
mut * ro * Acc          // *mut *Acc        (postfix chain)
unsafe * safe T         // *T              (`safe` stopper removed)
```

- A modifier **before** `*` ⇒ `E_POINTER_PREFIX_MODIFIER`.
- The `safe` type-modifier ⇒ `E_SAFE_RETIRED` (nothing to stop propagating —
  there is no prefix-modifier propagation anymore).
- Re-pointability is expressed by the binding (`ro` / `mut`), never `mut *`.

## Binding mut rule (D216 §2)

The leading `mut` / `ro` before the name is the **binding** (re-pointability,
D36). It is INDEPENDENT of the postfix pointee modifier — a `mut` binding does
NOT make the pointee writable:

```nova
ro p *Acc                   // ro-binding: arrow fixed, pointee read-only
mut p *Acc                  // mut-binding: arrow re-pointable, pointee STILL read-only
mut p *mut Acc              // writable pointee — the ONLY way: explicit *mut
ro p *mut Acc               // valid edge: arrow fixed, pointee writable

p = other_ptr               // allowed only with a mut binding (L1)
p.field = 1                 // allowed only with a *mut pointee (L3)
```

The binding says nothing about the pointee: `mut p *Acc` re-points the arrow,
but writing through it (`p.field = …` / `p.write(v)`) still requires an
explicit `*mut Acc` (D246: `*T ≡ *ro T` universally, the axes L1/L2/L3 are
independent). Re-pointability comes from the binding alone — there is no
`mut *` prefix in the type.

## `&value` + escape analysis (D216 §4, 118.6/118.7)

```nova
ro acc = Account { name: "Piter" }    // acc — heap reference
ro p = &acc                            // safe; type *Account; GC tracks acc

ro x = 42                              // x — stack primitive
ro q = &x                              // safe; x auto-promoted to heap; type *int
```

`&x` is **safe** (Plan 118.6): no `unsafe { }` wrap is needed — escape
analysis auto-promotes stack values to the heap. The pointee-mutability of
the result follows the **source binding** (D216 §4 amend, owner decision
2026-08-06): `&a` from a `mut` variable is `*mut T`, from a `ro` variable —
`*T`. The canonical way to get a writable pointer is simply:

```nova
mut x int = 1
ro p = &x                   // x is mut → p is *mut int, no cast needed
unsafe { p.write(42) }
assert(x == 42)
```

An explicit annotation is an equivalent form (`ro p *mut int = &x`); in
annotations a bare `*T` still always means a read-only pointee. The old
cast opt-in `(&x) as *mut int` is **retired**: re-asserting mutability over
the same pointee type via a cast is rejected (`E_POINTER_OP_USE_METHOD`
family). And the guarantee behind it all: from a `ro` variable a writable
pointer **cannot be obtained by any route**.

For a **raw stack address** without escape analysis or auto-promote there is
a separate operator (Plan 118.7) — it may dangle after scope exit, so it
requires an unsafe context:

```nova
unsafe {
    ro rp = raw &x          // raw stack address; E_UNSAFE_REQUIRED outside unsafe
}
```

`addr_of()` / `addr_of_mut()` are retired (`E_ADDR_OF_REMOVED`).

**Critical:** `&value` is **NOT a Rust borrow** (D32 amend). There is no
lifetime checker, no `'a` parameters, no XOR aliasing. Safety is provided by:
1. Escape analysis + auto-promote (Go-style) for stack values
2. Unsafe gating of raw access — the intrinsic methods and `raw &x`
3. GC honor-system — the user promises no GC trigger inside unsafe (D216 §16)

## Pointer arithmetic (D216 §6, methods form)

```nova
unsafe {
    ro p2 = p.offset(1)             // element-units step; type is preserved (*T)
    ro diff = p2.dist(p)            // int (signed element count) — here 1
    ro v = p2.read()                // deref read
}
```

- `.offset(n)` / `.dist(q)` are the only address arithmetic; the operator
  forms `p ± i`, `p - q` and order compares `p < q` are retired
  (`E_POINTER_OP_USE_METHOD`). Order, when needed, is the sign of `.dist()`.
- Units: sizeof(T)-scaled (C/Rust convention).
- `.offset()` does **not** degrade the type — the result is the same `*T`
  (the old "arithmetic degrades to `*uninit T`" rule is gone).

## Null safety: `Option[*T]` + NPO (D216 §7)

```nova
extern "C" fn malloc(sz int) -> Option[*u8]
// C codegen: uint8_t* malloc(size_t sz);   // single pointer, NULL = None

unsafe {
    match malloc(1024) {
        Some(buf) => use(buf),               // buf: *u8 non-null guaranteed
        None      => Fail.throw(OutOfMemory),
    }
}
```

**NPO applies to:** `Option[*T]`, `Option[*fn(...)]`, `Option[ptr]`,
`Option[Newtype-over-pointer]`.

**Excluded:** `Option[Option[*T]]` — tagged fallback + `W_OPTION_DOUBLE_NESTED`.

## `unsafe { }` block (D216 §8/§21, D2 amend)

What **requires** an `unsafe { }` wrap (checker-enforced, D216 §21 map):

| Op | Example | Diagnostic |
|---|---|---|
| Raw stack address | `raw &x` | `E_UNSAFE_REQUIRED` |
| Calling `unsafe fn` / `external unsafe fn` | `ffi_write(...)` | `E_UNSAFE_CALL_REQUIRES_WRAP` |
| Reading a `uninit T` value-binding | `ro v = u` | `E_UNSAFE_T_READ_REQUIRES_WRAP` |
| Passing a `uninit T` argument | `f(u)` | `E_UNSAFE_ARG_REQUIRES_WRAP` |
| Narrow cast `uninit T → T` | `u as T` | `E_UNSAFE_T_NARROW_REQUIRES_UNSAFE` |

The raw-pointer intrinsic methods (`.read()` / `.write()` / `.read_at()` /
`.write_at()` / `.offset()` / `.dist()` / volatile / unaligned / copy) are
**contractually unsafe**: wrap them in `unsafe { }` — the checker counts them
as legitimate uses of the block, though a missing wrap around them is not yet
mechanically rejected (a known enforcement gap, D216 §21 map item 8).

An `unsafe { }` block that contains **no** operation from the map is itself
an error — `E_UNSAFE_UNUSED` (dead unsafe blocks don't survive refactoring):

```nova
mut buf []int = [1, 2, 3]
unsafe {
    ro p = buf.ptr()
    ro v = p.read_at(2)      // ✓ intrinsic method — the block is "used"
    assert(v == 3)
}
```

Under the hood `unsafe { }` is sugar over a built-in effect handler (D2
spirit: everything is an effect); it does not propagate any effect upward —
the boundary encapsulates per fn (canonical Rust pattern).

## `unsafe fn` (D216 §9, Plan 118.1.7)

The **keyword** form is canonical; the old `#unsafe` attribute is removed
(`E_UNSAFE_ATTR_DEPRECATED`):

```nova
unsafe fn peek(p *u8) -> u8 { p.read() }    // body is implicitly an unsafe context

fn safe_caller(p *u8) -> u8 {
    // peek(p)                  ← ERROR E_UNSAFE_CALL_REQUIRES_WRAP
    unsafe { peek(p) }          // ✓
}
```

- An `unsafe fn` body is implicitly an unsafe context.
- The caller needs an `unsafe { }` wrap (even from another `unsafe fn` —
  a visual marker).
- NO effect propagation up.
- FFI declarations compose the same way: `external unsafe fn ...`.

## `*fn(...)` function pointers (D216 §10)

```nova
extern "C" fn libuv_set_timer_cb(cb *fn(i64) -> ()) -> i64

fn my_callback(timeout i64) -> () { ... }       // no Fail

unsafe {
    libuv_set_timer_cb(my_callback as *fn(i64) -> ())
}
```

- Cast `fn → *fn` — captureless required (`E_CLOSURE_HAS_ENV`)
- Cast `*fn → fn` — unsafe (wraps in a captureless closure)
- **Callback no-throw:** Fn-with-Fail cast → `*fn` — `E_CALLBACK_THROWS_OVER_C_ABI`
- **Extern fn no-Fail:** an extern fn declaring a `Fail` effect —
  `E_EXTERNAL_FN_FAIL_EFFECT`
- Unsafe **function-pointer** composition keeps the `unsafe` spelling:
  `*unsafe fn(...)` (a distinct concept from the possibly-uninit-data
  modifier `uninit`).

The current platform's C ABI (System V on Unix, MS x64 on Windows).

## FFI handle allocation contract (D216 §18)

**Tuple newtype is canonical for opaque handles** (zero-overhead):

```nova
type Sqlite3Handle(*sqlite3)               // stack, single pointer ABI
extern "C" fn open(path str) -> (Option[Sqlite3Handle], i64)
```

vs record form (extra indirection — pointer-to-struct ABI):

```nova
type DbSession {
    ro handle Sqlite3Handle
    ro path str
    ro opened_at Time
}                                           // record — for handles with extra state
```

Migrating Plan 115 V1 cookbook examples (record form) → tuple newtype
(zero-overhead) is tracked in `[M-118-handle-migration]`.

## GC honor-system (D216 §16)

Inside `unsafe { ... }` the user **promises** no GC trigger occurs between
pointer creation and use. A GC trigger = heap allocation, a yield-point
(await/spawn/supervised), string formatting that allocates, calls to
`#parks`/`#wakes` fns.

This is a spec contract, not a mechanical check — the compiler does not yet
emit a warning for violations (the `W_UNSAFE_GC_TRIGGER` diagnostic is
specified in D216 §16 but not implemented in the bootstrap compiler).

V1 GC = Boehm conservative → does not move objects → the honor-system is safe
for V1. A future moving GC will require a formal pin API (`[M-118-pin-api]`
followup).

## Pointer Debug formatting (D216 §17, Plan 91.14 D229)

Canonical form — `${expr:?}` format-spec (Plan 91.14, D229):

```nova
unsafe {
    ro p *Account = &acc
    ro s = "ptr=${&value:?}"                  // V3 canonical (Plan 91.14)
    println("pointer: ${p:?}")                // → "pointer: 0x7f... -> Account"
}
```

- `${p:?}` debug-format interpolation — canonical pointer rendering inside
  `unsafe { ... }` (Plan 91.14 D229).
- `(*T).to_debug_str() -> str` — legacy built-in alias kept for
  backwards-compat; same semantics as `${p:?}`, allowed in unsafe only.
- `"${p}"` direct (Display) interpolation → `E_PTR_NO_DISPLAY_USE_DEBUG_STR`;
  diagnostic hint points to `${p:?}` (per [D229](../../spec/decisions/02-types.md#d229-Debug-protocol--format-spec-expr)).
- Pointer addresses non-deterministic, leak ASLR info — explicit decision
  forced.

## Forbidden ops (D216 §15)

```nova
unsafe {
    ro arr = [1, 2, 3]
    ro p = &arr[1]               // ❌ E_ARRAY_INDEX_PTR_BANNED
                                  //   (array may realloc / GC compaction)
}

ro q = &42                       // ❌ E_AMP_LITERAL (no address of a literal)
```

There is no `null` / `undefined` in the language: an absent pointer is
`Option[*T] = None` (the legacy `null ptr` spelling is rejected with
`E_NULL_PTR_RETRACTED_USE_OPTION`).

## Compiler diagnostic codes

### Errors

- `E_POINTER_OP_USE_METHOD` — a retired pointer operator (`*p`, `*p = v`,
  `p[i]`, `p[i] = v`, `p ± i`, `p - q`, order compares) or the retired
  mutability-re-asserting cast `p as *mut T` over the same pointee; use the
  intrinsic methods (`.read()` / `.write()` / `.read_at()` / `.write_at()` /
  `.offset()` / `.dist()`) and source-binding inference for `&`
- `E_POINTER_RO_ASSIGN` — `p.field = v` or a write-family method call
  (`.write()` / `.write_at()` / …) through a read-only pointee; the writable
  pointee requires the `*mut T` opt-in
- `E_UNSAFE_REQUIRED` — `raw &x` outside an unsafe context
- `E_UNSAFE_CALL_REQUIRES_WRAP` — calling `unsafe fn` without an unsafe wrap
- `E_UNSAFE_T_READ_REQUIRES_WRAP` — `uninit T` value read without an `unsafe { }` block (V2 §V2.3; the code name kept `UNSAFE` even after the §10a `unsafe T` → `uninit T` type-modifier rename)
- `E_UNSAFE_ARG_REQUIRES_WRAP` — `uninit T` argument passed without an unsafe wrap (V2 §V2.3b)
- `E_UNSAFE_T_NARROW_REQUIRES_UNSAFE` — `uninit T → T` narrow cast without unsafe (V2 §V2.3b)
- `E_UNSAFE_UNUSED` — an `unsafe { }` block containing no operation from the
  D216 §21 map
- `E_UNSAFE_ATTR_DEPRECATED` — the removed `#unsafe` fn attribute; use the
  `unsafe fn` keyword (Plan 118.1.7)
- `E_ADDR_OF_REMOVED` — `addr_of()` / `addr_of_mut()`; use `&x` / `raw &x`
- `E_ARRAY_INDEX_PTR_BANNED` — `&arr[i]`
- `E_AMP_LITERAL` — `&42` (address of a literal)
- `E_NULL_PTR_RETRACTED_USE_OPTION` — legacy `null ptr`; use `Option[ptr] = None`
- `E_CLOSURE_HAS_ENV` — fn → *fn cast with a closure env
- `E_CALLBACK_THROWS_OVER_C_ABI` — Fn-with-Fail → *fn cast
- `E_EXTERNAL_FN_FAIL_EFFECT` — extern fn with a Fail effect
- `E_PTR_CAST_INVALID_TARGET` — `p as bool / f64 / ...`
- `E_PTR_ORDER_COMPARE_REQUIRES_UNSAFE` — checker-level gate on pointer order
  compare; the operator form itself is retired (`E_POINTER_OP_USE_METHOD`),
  use the sign of `.dist()`
- `E_INVALID_POINTER_MODIFIER` — `*const T` and others
- `E_POINTER_PREFIX_MODIFIER` — modifier **before** `*` (`mut * T` / `ro * T` /
  `uninit * T`); use postfix pointee `*mut T` / `*T` / `*uninit T` or binding
  `mut x *T` (Plan 138.5, extends `E_INVALID_POINTER_MODIFIER`)
- `E_REDUNDANT_POINTER_RO` — `*ro T` written explicitly; a bare `*T` is
  already read-only (the L3 pointee default, D246 / Plan 147: `*T ≡ *ro T`
  universally) — fix-it drops the `ro` (`*T`)
- `E_UNSAFE_TYPE_MODIFIER_RENAMED` — `unsafe` used as a **type** modifier on
  non-`Func` payload (the old data-uninit spelling); renamed to `uninit`
  (§10a, Plan 174.5) — use `uninit T` / `*uninit T`. Only `*unsafe fn(...)`
  (unsafe **function-pointer** composition, D216 §10) keeps the `unsafe`
  spelling — it is a distinct concept from possibly-uninit data.
- `E_SAFE_RETIRED` — `safe` type-modifier used; the `safe` propagation stopper
  is retired (no prefix-modifier propagation to stop) (Plan 138.5)
- `E_REALTIME_POINTER_OP` — pointer op inside a `#realtime fn` body
- `E_PTR_NO_DISPLAY_USE_DEBUG_STR` — `"${p}"` interpolation; hint suggests
  canonical `${p:?}` (Plan 91.14 D229) or legacy `p.to_debug_str()`

#### V3 modifier-composition errors (D216 V3 amend, 2026-06-04)

- `E_MUTABILITY_CONFLICT_VALUE_TYPE` — type-position `ro mut T` / `mut ro T`
  on **value-type T** (primitives / value records / named tuples / anonymous
  tuples / Unit). Binding-form `ro x mut T` remains allowed (orthogonal
  binding modifiers). Spec §V3.1.
- `E_MODIFIER_ORDER` — safety modifier (`uninit`) wrapping mutability modifier
  (`ro` / `mut`); reverse order required — **safety-inner / mutability-outer**
  (`ro uninit T` ✅ / `uninit ro T` ❌), consistent with `external unsafe fn`.
  Applies to value-T and to postfix **pointee** content (`*mut uninit T` ✅ /
  `*uninit mut T` ❌ — pointee `*ro …` is no longer a writable token at all,
  see `E_REDUNDANT_POINTER_RO`). Spec §V3.2 (FLIPPED in Plan 138.5).
- `E_REDUNDANT_TYPE_MODIFIER` — same-class modifier repetition. **Binding-level**
  (`ro x ro T`) and **postfix pointee chain** (`*mut mut T`) are kept; the old
  V3 type-level *prefix*-chain cases (`ro * ro T`, `unsafe * unsafe T`) are
  moot — a prefix before `*` is already `E_POINTER_PREFIX_MODIFIER` (Plan
  138.5), and a repeated pointee `ro` is now caught earlier by
  `E_REDUNDANT_POINTER_RO` (it errors on the first `*ro`, never reaching a
  repetition). The `safe` escape hatch is retired. Spec §V3.4.

> **Note:** the V3 `safe` propagation stopper and the `Unsafe(Pointer)` form
> (`unsafe * T` = nullable-raw) are RETIRED (Plan 138.5). `safe` in
> type-position ⇒ `E_SAFE_RETIRED`; nullable pointers use `Option[*T]`.

### Warnings

- `W_OPTION_DOUBLE_NESTED` — `Option[Option[*T]]` NPO fallback

## Mainstream comparison

| Language | Typed ptr | Unsafe model | Null safety | Deref access | Pointer arith |
|---|---|---|---|---|---|
| Rust | `*const T`/`*mut T`/`&T`/`&mut T` | `unsafe {}` + `unsafe fn` | `Option<&T>` + NPO | `*p` operator | unsafe only |
| Zig | `*T`/`*const T`/`[*]T` | (cast intrinsics) | `?*T` + NPO | `.*` postfix + `.` | `+` for `[*]T` |
| C# | `T*` / `ref T` / `in T` / `out T` | `unsafe` modifier | `T?` | `p->field` arrow | unsafe only |
| Swift | `UnsafePointer<T>` / `UnsafeMutablePointer<T>` | Type-based prefix | Optional + NPO | `.pointee` | only `.advanced(by:)` |
| D | `T*` / `ref T` / `scope T*` | `@safe`/`@trusted`/`@system` | `Nullable!T` | `p.field` auto | `@system` only |
| Go | `*T` (managed) / `unsafe.Pointer` | `unsafe` package | Nil runtime | `p.field` auto | `unsafe.Pointer` only |
| **Nova V1** (Plan 115) | `ptr` only | (none) | `null ptr` | (none) | banned |
| **Nova V2** (Plan 118) | **`*T` family** + `unsafe` | `unsafe { }` + `unsafe fn` (D2 amend) | `Option[*T]` + NPO | `p.field` one-level + operators | gated unsafe |
| **Nova FINAL** (Plan 138.5 + 174.5) | **postfix pointee** `*T` / `*mut T` / `*uninit T` / `*mut uninit T`; re-pointability = binding (`ro`/`mut`) | (same as V2) + value-T composition rules (§V3.1-V3.2) | `Option[*T]` (only) + NPO | **methods** `.read()`/`.write()` + `p.field` one-level | **methods** `.offset()`/`.dist()` |

## See also

- [`docs/plans/118-typed-pointers-and-unsafe.md`](../plans/118-typed-pointers-and-unsafe.md) — Plan 118 core implementation roadmap
- [`docs/plans/118.1-ffi-intrinsics-and-cstring.md`](../plans/118.1-ffi-intrinsics-and-cstring.md) — Plan 118.1 sub-plan (FFI intrinsics)
- [`docs/plans/118.2-slice-fat-pointer-and-uninit.md`](../plans/118.2-slice-fat-pointer-and-uninit.md) — Plan 118.2 sub-plan (slice + uninit)
- [`docs/plans/118.3-pointer-concurrency-safety.md`](../plans/118.3-pointer-concurrency-safety.md) — Plan 118.3 sub-plan (concurrency)
- [`docs/guide/ffi-cookbook.md`](ffi-cookbook.md) — FFI patterns with ptr + tuple FFI (Plan 115 V1)
- [D216 V1](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — spec foundation (typed-pointer family + unsafe model + NPO)
- [D216 FINAL pointer model (Plan 138.5)](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — pointer type = pointee-mut postfix only; re-pointability = binding (D36); prefix modifiers ⇒ `E_POINTER_PREFIX_MODIFIER`; nullable = `Option[*T]` only; `safe` + `Unsafe(Pointer)` retired
- [D216 amend "everything through methods" (Plan 174.5)](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — pointer operators retired for intrinsic methods; `E_POINTER_OP_USE_METHOD`
- [D216 V2 amend](../../spec/decisions/02-types.md#d216-v2-amend-2026-06-04--universal-right-binding-rule-для-type-level-modifiers--unsafe-t-first-class) — historical right-binding rule (§V2.1, RETRACTED) + first-class `uninit T` value-wrapper (§V2.3, KEPT; renamed from `unsafe T` by §10a, Plan 174.5) + NPO recalc (§V2.4)
- [D216 V3 amend](../../spec/decisions/02-types.md#d216-v3-amend-plan-1185-v3-2026-06-04--4-modifier-composition-rules) — value-T modifier-composition rules (V3.3/V3.4 superseded by Plan 138.5):
  - §V3.1 — storage-class-aware `ro+mut` adjacency ban (`E_MUTABILITY_CONFLICT_VALUE_TYPE`) — KEPT
  - §V3.2 — modifier ordering safety-inner / mutability-outer (`ro uninit T`; `E_MODIFIER_ORDER`) — FLIPPED, KEPT
  - §V3.3 — right-binding propagation — SUPERSEDED (no prefix propagation)
  - §V3.4 — `safe` keyword stopper — RETIRED; `E_REDUNDANT_TYPE_MODIFIER` kept at binding/postfix-pointee level
- [D216 §10a rename](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — `unsafe` type-modifier → `uninit` (Plan 174.5, 2026-07-11): `*unsafe T` → `*uninit T`, bare value-wrapper `unsafe T` → `uninit T`; the `unsafe { }` block, the `unsafe fn` keyword, and `*unsafe fn(...)` fn-pointer composition keep the `unsafe` spelling (different concept)
- [D2 amend](../../spec/decisions/04-effects.md#d2) — unsafe keyword restoration (effect-handler sugar)
- [D214 amend](../../spec/decisions/02-types.md#d214-ptr-opaque-pointer-type--tuple-ffi-returns--opaque-handle-pattern) — ptr redefine
- [D32 amend](../../spec/decisions/02-types.md#d32-семантика-передачи-параметров) — `&value` not Rust borrow
- [`examples/typed_pointers/`](../../examples/typed_pointers/) — minimal working samples
