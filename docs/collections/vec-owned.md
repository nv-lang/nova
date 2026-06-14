# Vec[T] — Nova-native growable array

`Vec[T]` is a generic growable array implemented entirely in Nova on top of
raw pointer allocation (`RawMem.alloc`). It is available as
`std.collections.vec_owned.Vec`.

## When to use Vec[T]

Use `[]T` (the built-in slice) by default. Switch to `Vec[T]` when:

- The element type `T` is a **value-struct** — `Option[U]`, named tuple,
  or `value` record — whose in-memory representation is wider than 8 bytes
  (the int64-slot used by `[]T`'s internal erasure model).
- You need **typed storage** where every element is laid out at its real C
  type without boxing.

For primitives and heap-record types, `[]T` and `Vec[T]` behave identically
in practice; prefer `[]T` for terseness.

## Quick start

```nova
import std.collections.vec_owned.{Vec}

fn main() -> () {
    // Build a vector from a literal element list (D259: `of`, not `from([…])`)
    mut v = Vec[int].of(10, 20, 30)
    assert(v.len() == 3)

    // Push and pop
    v.push(40)
    assert(v.pop() == Some(40))

    // Index access
    assert(v.get(0) == Some(10))
    assert(v.get(99) == None)

    // Iteration
    for x in v {
        println(x)    // 10, 20, 30
    }

    // Value-struct elements work correctly
    mut opts = Vec[Option[int]].new()
    opts.push(Some(1))
    opts.push(None)
    opts.push(Some(3))
    assert(opts.get(1) == Some(None))
}
```

## Construction

| Call | Result |
|------|--------|
| `Vec[T].new()` | Empty vector, cap = 0, no allocation |
| `Vec[T].with_capacity(n)` | Empty vector, pre-allocated `n` element slots |
| `Vec[T].of(a, b, c)` | Vector from a **literal element list** (variadic) — **one** allocation |
| `Vec[T].from(coll)` | **Convert** an existing collection / `[]T` — a `clone`-like copy |

### `of` vs `from` — when to use which (Plan 153.1 / [D259](../../spec/decisions/02-types.md#d259-конструктор-конвенция-vect--of-для-литерала-from-для-конверсии-plan-1531))

Two constructors, two distinct roles — don't mix them up:

- **Building from a literal element list → `Vec[T].of(a, b, c)`** (variadic). Like
  Rust `vec![a, b, c]`. Takes the elements directly: **one allocation**.
- **Converting an existing collection → `Vec[T].from(coll)`**. Like Rust
  `Vec::from(iter)` — a `clone`-like copy of something you already hold.

```nova
ro a = Vec[int].of(1, 2, 3)        // ✅ literal list → of (1 allocation)
ro b = Vec[int].new()              // ✅ empty
ro c = Vec[int].from(other_vec)    // ✅ convert an existing collection

ro d = Vec[int].from([1, 2, 3])    // ❌ redundant-clone anti-pattern
ro e = Vec[int].from([])           // ❌ → Vec[int].new()
```

**Why `from([literal])` is an anti-pattern.** Under [D239](../../spec/decisions/02-types.md#d239-t--синтаксический-псевдоним-vect)
an array literal `[1, 2, 3]` is *already* a `Vec[int]` — one allocation at the
literal itself. `from` then copies it into a **second** buffer, so
`Vec[int].from([1, 2, 3])` costs **two** allocations to produce exactly what
`Vec[int].of(1, 2, 3)` produces in **one**. Reserve `from` for converting a
collection you already have (`from(some_vec)` / `from(some_slice)`).

> When the element type is already fixed by context, you don't even need a
> constructor — the bare literal `[a, b, c]` *is* the `Vec[T]` (D239). `of`/`from`
> are only for inline type annotation (return position, generic context).

## Method reference

### Size and capacity

| Method | Signature | Description |
|--------|-----------|-------------|
| `len` | `@len() -> int` | Number of live elements |
| `cap` | `@cap() -> int` | Number of allocated slots |
| `is_empty` | `@is_empty() -> bool` | True when `len == 0` |
| `reserve` | `mut @reserve(additional int) -> ()` | Ensure room for `additional` more elements |
| `shrink_to_fit` | `mut @shrink_to_fit() -> ()` | Reduce capacity to exactly `len` |
| `shrink_to` | `mut @shrink_to(min_cap int) -> ()` | Reduce capacity to `max(len, min_cap)` |

### Adding and removing elements

| Method | Signature | Description |
|--------|-----------|-------------|
| `push` | `mut @push(v T) -> ()` | Append to end; grows ×2 if needed |
| `pop` | `mut @pop() -> Option[T]` | Remove and return last element |
| `insert` | `mut @insert(i int, v T) -> ()` | Insert at index `i`, shifting right; panics if `i > len` |
| `remove` | `mut @remove(i int) -> T` | Remove at `i`, shifting left; panics if out of bounds |
| `swap_remove` | `mut @swap_remove(i int) -> T` | O(1) remove: swap with last, then pop; does not preserve order |
| `clear` | `mut @clear() -> ()` | Set `len = 0`, buffer retained |
| `truncate` | `mut @truncate(n int) -> ()` | Shorten to `n` elements; no-op if `n >= len` |

### Access

| Method | Signature | Description |
|--------|-----------|-------------|
| `get` | `@get(i int) -> Option[T]` | Element by index, bounds-checked |
| `get_mut` | `mut @get_mut(i int) -> Option[*mut T]` | Raw pointer to slot; valid until next realloc |
| `first` | `@first() -> Option[T]` | First element |
| `last` | `@last() -> Option[T]` | Last element |

### Bulk operations

| Method | Signature | Description |
|--------|-----------|-------------|
| `extend` | `mut @extend(items []T) -> ()` | Append all elements from `[]T` slice |
| `append` | `mut @append(mut other Vec[T]) -> ()` | Move all from `other` onto end; `other` becomes empty |
| `retain` | `mut @retain(pred fn(T) -> bool) -> ()` | Keep only elements where `pred` returns true; O(n) |
| `reverse` | `mut @reverse() -> ()` | Reverse live elements in place |

### Slices & views (Plan 153.4 / D262)

Zero-copy `[]T`-views of the **same type** sharing the parent buffer (`cap == len`);
no `Slice` type. A reallocating mutation on a view detaches it (Go-model, GC-safe);
an *owning* copy is `clone()`/`to_vec()`. The lazy slice-view iterators
`chunks`/`chunks_exact`/`rchunks`/`windows` live in the explicitly-imported lazy
module (`import std.collections.vec_lazy`) — they yield `[]T` views one at a time
with **no outer `Vec` allocation** (Rust `slice::chunks`/`windows`). See
[`vec-internals.md` → Slices & views](../vec-internals.md#slices--views-plan-1534--d262).

| Method | Signature | Description |
|--------|-----------|-------------|
| `as_slice` | `@as_slice() -> []T` | Read-only zero-copy whole-view (NOT a copy) |
| `as_slice` (mut) | `mut @as_slice() -> mut []T` | Write-through whole-view (recv-mut overload, like `mut @as_ptr`) |
| `split_at` | `@split_at(i int) -> ([]T, []T)` | Two adjacent views at `i`; `requires 0 <= i <= len` (OOB panics) |
| `split_first` | `@split_first() -> Option[(T, []T)]` | First element + tail view; empty → `None` |
| `split_last` | `@split_last() -> Option[(T, []T)]` | Last element + init view; empty → `None` |
| `first_n` | `@first_n(n int) -> []T` | Prefix view; **clamps** (`n > len` → whole, `n <= 0` → empty) |
| `last_n` | `@last_n(n int) -> []T` | Suffix view; **clamps** (same as `first_n`) |

**Lazy slice-view iterators** (`import std.collections.vec_lazy`; each `-> BoxIter[[]T]`, `requires n > 0`):

| Method | Signature | Description |
|--------|-----------|-------------|
| `chunks` | `@chunks(n int) -> BoxIter[[]T]` | Non-overlapping chunks of `n`; last chunk short |
| `chunks_exact` | `@chunks_exact(n int) -> BoxIter[[]T]` | Full chunks of `n` only; short tail dropped |
| `rchunks` | `@rchunks(n int) -> BoxIter[[]T]` | Non-overlapping chunks from the end (yielded back-to-front); leading chunk short |
| `windows` | `@windows(n int) -> BoxIter[[]T]` | Overlapping width-`n` views (`n-1` shared); `n > len` → empty |

### Conversion

| Method | Signature | Description |
|--------|-----------|-------------|
| `iter` | `@iter() -> VecIter[T]` | Index-cursor iterator |

### Protocols

| Protocol | Method | Notes |
|----------|--------|-------|
| `Iterable[T]` | `@iter() / VecIter[T].@next()` | `for x in v` syntax |
| `Equal` | `@equal(other Vec[T]) -> bool` | Element-wise, via `as_slice` comparison |
| `Clone` | `@clone() -> Vec[T]` | Allocates new buffer, copies all elements |
| `Display` | `@display(mut sb StringBuilder) -> ()` | Format: `Vec[e0, e1, ..., eN-1]` |
| `Debug` | `@debug(mut sb StringBuilder) -> ()` | Same format, for `${v:?}` |

## Examples

### Grow and iterate

```nova
mut v = Vec[int].new()
for i in 0..10 { v.push(i) }
assert(v.len() == 10)
for x in v { print("${x} ") }    // 0 1 2 3 ... 9
```

### Insert and remove

```nova
mut v = Vec[int].of(1, 2, 4, 5)
v.insert(2, 3)                     // [1, 2, 3, 4, 5]
assert(v.remove(0) == 1)           // [2, 3, 4, 5]
assert(v.swap_remove(0) == 2)      // [5, 3, 4] (order disrupted)
```

### Filter with retain

```nova
mut v = Vec[int].of(1, 2, 3, 4, 5, 6)
v.retain(|x| x % 2 == 0)
assert(v.as_slice() == [2, 4, 6])
```

### Slices & views (zero-copy)

```nova
let v = Vec[int].of(1, 2, 3, 4, 5)

// split_at: two views of the same buffer (contract 0 <= i <= len)
let (l, r) = v.split_at(2)
assert(l.len() == 2 && r.len() == 3 && l[0] == 1 && r[0] == 3)

// first_n / last_n clamp ("take up to N")
assert(v.first_n(3).len() == 3)
assert(v.first_n(99).len() == 5)        // clamped to len
assert(v.last_n(2).equal(Vec[int].of(4, 5)))

// mut @as_slice writes through to the parent (until detach)
mut w = Vec[int].of(1, 2, 3)
mut s = w.as_slice()
s[0] = 99
assert(w[0] == 99)

// detach-on-resize: pushing onto a cap==len view reallocs; parent untouched
mut head = w.first_n(2)
head.push(7)                            // detaches into a fresh buffer
assert(w.equal(Vec[int].of(99, 2, 3)))  // parent unchanged
```

### Value-struct elements

```nova
// Option[int] is a value-struct. []Option[int] would erase it.
// Vec[Option[int]] stores each NovaOpt_nova_int struct inline.
mut v = Vec[Option[int]].new()
v.push(Some(42))
v.push(None)
assert(v.get(0) == Some(Some(42)))
assert(v.get(1) == Some(None))
```

### Capacity management

```nova
mut v = Vec[int].with_capacity(100)
assert(v.cap() >= 100)
for i in 0..50 { v.push(i) }
v.shrink_to_fit()
assert(v.cap() == 50)
```

### Clone and equality

```nova
let a = Vec[int].of(1, 2, 3)
let mut b = a.clone()
b.push(4)
assert(a.len() == 3)          // original unchanged
assert(b.len() == 4)
assert(a.equals(Vec[int].of(1, 2, 3)))
```

### Unsafe get_mut

`get_mut` returns a raw mutable pointer for in-place update without copying:

```nova
mut v = Vec[int].of(10, 20, 30)
if let Some(p) = v.get_mut(1) {
    unsafe { *p = 99 }
}
assert(v.get(1) == Some(99))
```

Note: the pointer is invalidated by any subsequent `push`, `insert`,
`reserve`, or other realloc-capable call.

## Comparison with []T

| | `[]T` | `Vec[T]` |
|---|-------|----------|
| Default choice | Yes | No |
| Primitive elements | Full typed | Full typed |
| Record elements | Pointer-in-slot | Pointer-in-slot |
| `Option[U]` elements | int64-erasure (broken) | Inline struct (correct) |
| Named tuple elements | int64-erasure (broken) | Inline struct (correct) |
| Value-record elements | int64-erasure (broken) | Inline struct (correct) |
| `for x in` iteration | Built-in | Via VecIter |
| Compiler magic | Yes (NOVA_ARRAY_DECL) | No (pure Nova) |
| Literal syntax `[1,2,3]` | Yes (`[1,2,3]` *is* a `Vec` under D239) | Yes — `[1,2,3]`, or `Vec[T].of(1,2,3)` for inline type (NOT `from([…])`, D259) |

## Performance notes

- `push`: amortised O(1). Initial capacity 8; doubles on each realloc.
- `get` / `get_mut`: O(1) pointer arithmetic.
- `insert` / `remove`: O(n) element shift — prefer `swap_remove` when
  order does not matter.
- `as_slice`: O(n) copy — avoid in hot paths; iterate directly with
  `for x in v`.
- Buffer allocation uses `RawMem.alloc` (Boehm GC-tracked, zeroed,
  8-byte aligned). GC scans the buffer conservatively, so element
  pointers inside the buffer keep their targets alive.

## Spec references

- [D231](../../spec/decisions/02-types.md#d231-rawmem-allocator-api--nova_alloc--nova_alloc_uncollectable--nova_free_uncollectable) — RawMem allocator API.
- [D232](../../spec/decisions/02-types.md#d232-vect--nova-native-generic-growable-array) — Vec[T] formal spec.
- [D216 §6](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — pointer arithmetic codegen.
- [Q-vec-vs-slice](../../spec/open-questions.md#q-vec-vs-slice----vect-vs-t-which-to-use) — decision guide.
