<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# `Vec[T]` / `[]T` — internals & module layout

> **Audience:** Nova stdlib contributors. For the user-facing guide see
> [`vec.md`](vec.md) (Plan 153.1+). **Spec:** [D239](../spec/decisions/02-types.md#d239-t--синтаксический-псевдоним-vect)
> (`[]T ≡ Vec[T]`), [D232](../spec/decisions/02-types.md#d232-vect--nova-native-generic-growable-array)
> (`Vec[T]` on RawMem), [D238](../spec/decisions/03-syntax.md)/[D240](../spec/decisions/03-syntax.md)
> (`Index`/`MutIndex`).

## What `Vec[T]` is

`Vec[T]` is a fully **Nova-implemented** generic growable array — no compiler
magic beyond typed pointers ([D216](../spec/decisions/02-types.md)),
`size_of[T]()` ([D199](../spec/decisions/02-types.md)) and pointer arithmetic
(Plan 131). `[]T` is a **pure syntactic alias** of `Vec[T]` (D239): the compiler
expands `[]T → Vec[T]` at type-resolution, an array literal `[1, 2, 3]` *builds*
a `Vec[int]`, and a slice `v[a..b]` is a zero-copy `[]T`-view of the same type
(Plan 96, cap == len).

### Layout

```
Vec[T] = { mut data *mut T, mut len int, mut cap int }
```

- `data` — heap buffer of `cap` element **slots**, first `len` live. Stored
  **typed** (`Nova_T*`), no int64-slot erasure: `Vec[Option[int]]`,
  `Vec[MyRecord]`, `Vec[Vec[int]]` all use their natural per-element C width.
- `data` is GC-tracked (`RawMem.alloc`); the conservative scan over the buffer
  keeps pointer-valued elements alive.
- Growth is amortised ×2 (initial cap 8); an explicit `with_capacity(n)` honours
  `n` exactly (so the realloc point — and thus slice detach — is predictable).

### Unsafe model (D216 §8)

Every raw-pointer op (`alloc`, `data + i`, deref read/write) is wrapped in
`unsafe { }`. The element API is fully **safe** to use — in-place mutation is the
safe `v[i] = val` (D240 `MutIndex`). The only deliberate escape is the FFI
accessor `@as_ptr()` (recv-mut overload: `*T` on `ro`, `*mut T` on `mut`):
calling it is safe (a pointer-value copy), *dereferencing* the result is the
caller's `unsafe` obligation.

## Module layout — `std/collections/vec/` (folder-module)

`std/collections/vec/` is a **folder-module** (Nova module model: every co-equal
`.nv` file in the folder declares the SAME `module collections.vec` and they
merge into one module — a method in any peer attaches to the `Vec[T]` type from
`core.nv`). The owning Vec type + all its methods live here; the prelude
re-exports `Vec`/`VecIter` from it, so the **type and its methods are
prelude-global** (`v.push(x)` works without an import).

| File | Layer |
|---|---|
| `_module.nv` | folder-wide `#prelude(...)` attribute carrier (no items) |
| `core.nv` | `Vec[T]` type, constructors (`new`/`with_capacity`/`from`/`from_raw_parts`/`into_raw`), `len`/`cap`/`is_empty`, capacity mgmt (`reserve`/`realloc_to`), buffer helpers, module-private `panic` |
| `access.nv` | `@index`/`@get`/`@first`/`@last`/`@as_ptr` (read + `v[i]=val` write) |
| `mutate.nv` | `push`/`pop`/`insert`/`splice`/`remove`/`swap_remove`/`clear`/`truncate`/`reverse` + bulk (`extend`/`append`/`retain`/`copy_from`/`copy_within`/`fill`/`append_zero`) |
| `slice.nv` | `@index(Range)`/`@get(Range)` — zero-copy `[]T` views (Plan 96) |
| `views.nv` | eager named views (Plan 153.4 / D262): `@split_at`/`@split_first`/`@split_last`/`@first_n`/`@last_n`/`@as_slice` (+ recv-mut `mut @as_slice`) |
| `iter.nv` | `VecIter[T]` + `@iter()`/`@next()` (`Iter`/`Next`, D58) |
| `sort.nv` | `@sort*`/`@binary_search*`/`@dedup*`/`@partition`/`@index_of`/`@position` (Plan 153.3) |
| `restructure.nv` | `@concat`/operator `+`/`@rotate_left`/`@rotate_right`/`@drain`/`@insert_slice`/`@flatten` (Plan 153.5) |
| `protocols.nv` | `Equal`/`Compare`/`Clone`/`Hash`/`Display`/`Debug` |
| *(sibling)* `vec_lazy.nv` | Plan 153.2 LAZY iterator adapters — a SEPARATE explicit-import module `collections.vec_lazy`, NOT in this folder (closure-dense, see below) |

Conventions proven for this folder-module (Plan 153.0):
- A **module-private** helper (`external fn panic`, `alloc_buf`, `null_buf`) is
  declared ONCE (in `core.nv`) and is visible to every co-equal peer.
- Each peer repeats `#prelude(core, runtime, collections, protocols)` so it
  resolves correctly when compiled standalone as an entry; `_module.nv` carries
  the same directive for the folder.
- A file `vec.nv` and a folder `vec/` of the same name are **forbidden**
  (`ambiguous module`) — the legacy `vec.nv` was folded in, `vec_owned.nv` (the
  old `collections.vec_owned` module name) was retired.

### Eager combinators are NOT in the folder

`map`/`filter`/`fold`/`any`/`all` live in a **separate** module
[`std/collections/vec_seq.nv`](../std/collections/vec_seq.nv)
(`collections.vec_seq`), reached by an explicit `import std.collections.vec_seq`
— NOT in the prelude-global folder. Reason: a prelude-global method's
identifiers (its method-level generics `[Acc]` and its callback params `f`/`op`)
leak into every unit's merged body, so a unit with a top-level `fn f`/`fn op` or
a `type Acc` would capture/shadow them ([M-codegen-var-types-fn-scope] + D145).
`@retain(pred)` survives only because `pred` is uncommon. Confining the
combinators behind an explicit import keeps them opt-in, exactly as the
pre-153.0 `collections.vec` module did.

Plan 153.2 added the **lazy** iterator layer
[`std/collections/vec_lazy.nv`](../std/collections/vec_lazy.nv)
(`collections.vec_lazy`, `v.lazy().map().filter().collect()`, no intermediate
allocations — see the user guide [`vec-lazy.md`](vec-lazy.md) and
[D260](../spec/decisions/02-types.md#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532)).
It is a **sibling FILE-module**, NOT a peer inside `vec/`, for the very same
scope-leak reason: every lazy adapter takes a closure (`f`/`pred`) and has
method-level generics (`[U]`/`[Acc]`), so it must stay behind an explicit
`import std.collections.vec_lazy`. Whether the lazy layer can ever become
prelude-global is revisited under [M-153-vec-combinators-prelude-global].

## Slices & views (Plan 153.4 / D262)

A **slice** in Nova is a zero-copy `[]T`-**view of the same type** — there is no
separate `Slice[T]`/`&[T]` type (Plan 96 *D-single-type*; D238/D239). A view is
just a `Vec` header `{ data: parent.data + start, len, cap: len }` pointing inside
the parent's GC-tracked buffer, with `cap == len` (*D-cap-len*). Two surfaces
produce views:

- **`v[a..b]`** (`slice.nv`, `@index(Range)` / `@get(Range)`, Plan 96) — the
  operator form.
- **named view methods** (`views.nv`, Plan 153.4):

  | Method | Returns | Bounds |
  |---|---|---|
  | `@split_at(i)` | `([]T, []T)` | `requires 0 <= i <= len` (OOB → panic, NOT clamp) |
  | `@split_first()` | `Option[(T, []T)]` | empty → `None` |
  | `@split_last()` | `Option[(T, []T)]` | empty → `None` |
  | `@first_n(n)` | `[]T` | **clamps** (`n > len` → whole, `n <= 0` → empty) |
  | `@last_n(n)` | `[]T` | **clamps** (same as `first_n`) |
  | `@as_slice()` | `[]T` | read-only whole-view (the `Vec`-side analogue of `str.as_bytes()`) |
  | `mut @as_slice()` | `mut []T` | write-through whole-view (recv-mut overload, like `mut @as_ptr`) |

`@split_at` enforces a contract (a silent clamp would break the
`len(left) + len(right) == len` invariant and hide a caller bug), whereas
`first_n`/`last_n` clamp because "take up to N" should never surprise the caller
(mirrors Rust `[..n.min(len)]`). The writable whole-view is the **receiver-mut
overload** `mut @as_slice` (selected on a `mut`-bound receiver), **not** a
separate `as_mut_slice` name — same accessor convention as `@as_ptr`/`mut @as_ptr`
(D247 / Plan 135).

### Detach-on-resize (Go-model, GC-safe)

Because a view has `cap == len`, the first **reallocating** mutation on it
(`push`/`reserve`/`insert` at `cap == len`) reallocs into a fresh buffer
(`@realloc_to`, core.nv) and the view **silently detaches** — it never overwrites
the parent's backing store. This removes the Go shared-backing footgun without a
borrow-checker. Until that detach point, a `mut`-bound view writes *through* to
the parent (`s[i] = x`, `for mut x in s`). The detach point is predictable
because exact capacity is honoured (`with_capacity`/`@cap(n)`, Plan 153.1 — no
pow2 rounding). The GC keeps the parent buffer alive while any view is reachable
(`GC_all_interior_pointers`, Plan 138 Ф.2). An *owning* copy is `clone()` /
`to_vec()`, never a view.

`@chunks`/`@chunks_exact`/`@rchunks`/`@windows` are **lazy** iterators (Rust-like,
no outer-`Vec` allocation), deferred under `[M-153.4-chunks-windows-lazy]` and
gated on the Plan 153.2 lazy-iterator infrastructure — they are intentionally NOT
implemented eagerly.

## Compare vs Equal

`Vec[T: Compare] @compare` (protocols.nv) is **lexicographic, element-wise** —
it delegates to each element's own `@compare`, like Rust `Vec<T: Ord>`. It is
NOT a raw byte `memcmp`: that prior impl was correct only for `Vec[u8]` (for
`Vec[f64]` IEEE-754 byte order ≠ numeric order; for `Vec[int]` little-endian
byte order ≠ value order; for records it compared addresses). A `u8`-specialised
memcmp fast-path is a perf followup ([M-153-vec-compare-u8-memcmp-fastpath]).
`@equal` is likewise element-wise (via each element's `==`).

Both read `self` and the other operand **raw** (`unsafe { @data[i] }` /
`unsafe { other.data[i] }`) once the index is proven in bounds — no redundant
`@index` bounds check. The deref is extracted to a typed local before the
`.compare`/`!=` so dispatch resolves on element type `T` (a method call kept
*inline* on a generic raw deref mis-resolves in the erased stub —
[M-codegen-erased-stub-method-on-varindex-deref]).

## Restructure ops — concat / operator `+` / rotate / drain / insert_slice

`restructure.nv` (Plan 153.5, [D263](../spec/decisions/10-overloading.md#d263-vec-restructure-ops--оператор---plus--concat))
holds the ops that **build a new vector** from existing data or **move whole runs**
of elements. All are Nova-body over bulk `RawMem.copy`.

### Concat and operator `+` (non-mutating join)

```nova
ro a = Vec[int].from([1, 2, 3])
ro b = Vec[int].from([4, 5])
ro c = a.concat(b)        // c == [1,2,3,4,5];  a, b untouched
ro d = a + b              // d == [1,2,3,4,5];  `+` == @plus == @concat
mut e = Vec[int].from([1, 2])
e += Vec[int].from([3, 4]) // e == [1,2,3,4];  a += b  ==  a = a + b (fresh Vec)
```

- `@concat(other) -> Vec[T]` allocates **exactly** `a.len() + b.len()` (`with_capacity`),
  then two bulk `RawMem.copy` passes — O(a+b), one allocation. Neither operand is
  mutated. This is the body of the `+` operator (`@plus(other) => @concat(other)`).
- **`+` vs `append`.** `a + b` is a **new** Vec (Kotlin/Python/Ruby semantics);
  `a += b` lowers to `a = a + b` (a fresh concat Vec), *not* an in-place grow. To grow
  `a` in place use **`a.append(b)`** (the in-place bulk merge in `mutate.nv`). One layer,
  one semantics — `concat`/`+` build, `append` mutate.
- **Codegen.** `@plus` is a Nova method; the `+`/`+=` *operator-lowering* is wired in
  `emit_c.rs`: `BinOp::Add` on `Vec[T]` routes through `vec_method_call("plus", …)`
  (registering the mono instance first), and `a += b` is desugared to `a = a + b`
  (raw C `a += b` on a struct/pointer operand is illegal).

### Rotate (cyclic shift, in place)

```nova
mut v = Vec[int].from([1, 2, 3, 4, 5])
v.rotate_left(2)          // [3,4,5,1,2]
v.rotate_right(2)         // [1,2,3,4,5]  (right by k == left by len-k)
```

`mut @rotate_left(n) -> @` / `mut @rotate_right(n) -> @` reduce `n` mod `len` (so any
`n >= 0` is valid; a full/multi-turn rotation is identity), then shift in place — O(len)
time, O(min(n, len−n)) scratch, overlap-safe `RawMem.copy`. Empty/single-element vectors
are unchanged. Contract `requires n >= 0`. They return `@` for chaining.

### Drain (cut a range out, return it owned)

```nova
mut v = Vec[int].from([1, 2, 3, 4, 5])
ro cut = v.drain(1..4)    // cut == [2,3,4];  v == [1,5]
```

`mut @drain(range Range) -> Vec[T]` copies the half-open `[start, end)` run into a new
owned `Vec`, shifts the suffix down to close the gap, and shortens `self` by
`range.len()` — O(len). Empty range drains nothing (returns empty `Vec`, `self`
untouched). Contract `requires start>=0 && end>=start && end<=@len` (OOB / reversed → panic).

### insert_slice (slice-flavoured bulk insert)

```nova
mut v = Vec[int].from([1, 2, 5, 6])
v.insert_slice(2, Vec[int].from([3, 4]))   // [1,2,3,4,5,6]
```

`mut @insert_slice(i, sl []T) -> @` inserts every element of `sl` at index `i` (`i == len`
is an append), shifting the existing suffix right. Under D239 a `[]T` *is* a `Vec[T]`, so it
delegates to `@splice` (mutate.nv); the distinct name documents the slice-argument intent
(Rust `Vec::splice` / Go `slices.Insert`). Overlap-safe → a self-insert is correct.
Contract `requires 0 <= i && i <= @len`.

### flatten (concatenate the inner rows)

```nova
ro nested = Vec[Vec[int]].from([[1, 2], [3], [4, 5]])
ro flat = nested.flatten()    // [1, 2, 3, 4, 5]
```

`Vec[Vec[T]] @flatten() -> Vec[T]` (≡ `[][]T @flatten() -> []T` under D239) concatenates
every inner row into one fresh `Vec[T]`. It first sums each `inner.len()` to pre-size
`out = Vec[T].with_capacity(total)`, then bulk-copies each row via `out.append(inner)` (the
`@append(Vec[T])` `RawMem.copy` fast path — copy, not move, so the operands are unchanged).
Empty inner rows and an empty outer vector are handled naturally. O(Σ inner.len()), one
allocation. The production form is the **carrier** receiver `Vec[Vec[T]] @flatten()` — the
same spelling the rest of the stdlib uses.

#### Nested generic receivers (the enabler)

`flatten` is the first stdlib method with a **nested generic receiver**. A correct
`.flatten()` must name the *innermost* element `T` so the result is `Vec[T]`, not
`Vec[Vec[int]]`. This needed structural typevar unification at **arbitrary nesting depth**
([D145 AMEND](../spec/decisions/02-types.md#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101)),
fixed in both the parser and the monomorphizer (Plan 153.5, commit `1c323d0e`):

- Both spellings are accepted and equivalent under D239: `fn[T] Vec[Vec[T]] @m` ≡
  `fn[T] [][]T @m`. The full structured receiver type is carried on `Receiver.receiver_ty`
  (`type_name` flattens to `"[][]T"` and would lose the depth, so a separate slot is needed).
- The receiver typevar binds to the **innermost** element, recursively: `Vec[Vec[T]]` ⇒
  `T = element-of-element`; `Vec[Vec[Vec[T]]]` ⇒ third-level element; and so on
  (depth-agnostic, not one-level-hardcoded).
- Flat `[]T` (depth 1) is **byte-identical** to before — the override is gated to genuinely
  nested receivers, so the whole `[]T`-method-dispatch path that every slice method rides on
  is unchanged for the common case.

Before this fix the parser rejected the carrier form (`Vec[Vec[T]]` → "expected `]`") and
collapsed the slice form (`[][]T` → `"[]T"`), while the monomorphizer bound `T` to the
*immediate* element (`Vec[int]`), producing the wrong return type and a segfault. See
[D263 AMEND](../spec/decisions/10-overloading.md#d263-vec-restructure-ops--оператор---plus--concat).
