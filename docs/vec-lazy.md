<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Lazy iterators over `Vec[T]` / `[]T`

> **Audience:** Nova users. **Spec:** [D260](../spec/decisions/02-types.md#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532)
> (lazy iterator model), [D277](../spec/decisions/02-types.md#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2)
> (by-value `BoxIter` + zero-cost `vec_iter_zc`), [D239](../spec/decisions/02-types.md#d239-t--синтаксический-псевдоним-vect)
> (`[]T ≡ Vec[T]`). **Internals:** [`vec-internals.md`](vec-internals.md). Plan 153.2.

A lazy iterator processes a vector **one element at a time, on demand**, with **no
intermediate allocations**. Building a pipeline does no work; only a *terminator*
pulls elements through it, and it pulls only as many as it needs.

```nova
import std.collections.vec_lazy

let v = Vec[int].from([1, 2, 3, 4, 5, 6])
let got = v.lazy().map(|x| x * 10).filter(|x| x > 25).collect()
assert(got == [30, 40, 50, 60])
```

## Getting started

The lazy layer is an **opt-in** module — import it explicitly:

```nova
import std.collections.vec_lazy
```

(It is *not* in the prelude: lazy adapters take closures, and a prelude-global
closure-carrying method would leak its generics/params into every unit — see
[`vec-internals.md`](vec-internals.md). The eager `collections.vec_seq`
combinators are confined the same way.)

Every pipeline starts with `v.lazy()`, which turns a `Vec[T]` (or any `[]T`, since
they are the same type — D239) into a `BoxIter[T]`:

```
v.lazy()  →  BoxIter[T]   →  .map(..) .filter(..) ...   →  terminator
  ^entry        cursor          ^adapters (lazy)             ^drives the chain
```

## Lazy vs eager — why it matters

| | Eager (`collections.vec_seq`) | Lazy (`collections.vec_lazy`) |
|---|---|---|
| `v.map(f).filter(p)` | builds a new `Vec` **per step** (O(n) allocations) | builds zero `Vec`s; wraps closures |
| Work done | always processes **all** elements at each step | only what a terminator pulls |
| Short-circuit | no — full materialization | yes — `find`/`any`/`all`/`take`/`nth` stop early |
| Result | a `Vec` after each adapter | a value/`Vec` only at the terminator |

Lazy is the **canonical, allocation-free** path
([Q-iterator-laziness](../spec/open-questions.md)). The eager `vec_seq`
combinators are retained as a transitional surface; reach for `lazy()` when you
chain more than one step or want short-circuiting.

### Laziness, demonstrated

Nothing runs until a terminator drives the chain, and only the pulled elements
are touched:

```nova
let v = Vec[int].from([1, 2, 3, 4, 5])

// No terminator → no work. `map` never runs.
let _pipeline = v.lazy().map(|x| x * 2).filter(|x| x > 0)

// `take(3)` pulls exactly 3 source elements — `map` runs 3 times, not 5.
let first3 = v.lazy().map(|x| x * 10).take(3).collect()   // [10, 20, 30]

// `find` short-circuits at the first match.
let hit = v.lazy().map(|x| x).find(|x| x == 3)            // Some(3), map ran 3×
```

## API — Phase A

### Entry

| Method | Returns | Notes |
|---|---|---|
| `v.lazy()` | `BoxIter[T]` | begin a lazy pipeline over the vector / slice |

### Adapters (lazy — return a new `BoxIter`, no allocation)

| Adapter | Signature | Yields |
|---|---|---|
| `map` | `@map[U](f fn(T) -> U) -> BoxIter[U]` | `f(x)` for each element |
| `filter` | `@filter(pred fn(T) -> bool) -> BoxIter[T]` | elements where `pred` holds |
| `filter_map` | `@filter_map[U](f fn(T) -> Option[U]) -> BoxIter[U]` | `f`'s `Some(u)`; skips `None` |
| `enumerate` | `@enumerate() -> BoxIter[(int, T)]` | `(index, x)` pairs |
| `take` | `@take(n int) -> BoxIter[T]` | at most the first `n` |
| `skip` | `@skip(n int) -> BoxIter[T]` | all but the first `n` |

### Terminators (drive the chain / short-circuit)

| Terminator | Signature | Result |
|---|---|---|
| `collect` | `mut @collect() -> Vec[T]` | drain into a fresh `Vec` |
| `fold` | `mut @fold[Acc](init Acc, f fn(Acc, T) -> Acc) -> Acc` | left fold |
| `reduce` | `mut @reduce(f fn(T, T) -> T) -> Option[T]` | fold from first; `None` if empty |
| `count` | `mut @count() -> int` | number of remaining elements |
| `sum` | `mut @sum(zero T) -> T` | sum starting from the additive identity |
| `any` | `mut @any(pred fn(T) -> bool) -> bool` | `true` on first match (short-circuit) |
| `all` | `mut @all(pred fn(T) -> bool) -> bool` | `false` on first miss; vacuously `true` |
| `find` | `mut @find(pred fn(T) -> bool) -> Option[T]` | first match, or `None` |
| `for_each` | `mut @for_each(f fn(T) -> ()) -> ()` | run `f` for its side effect |
| `min` | `[T Compare] mut @min() -> Option[T]` | smallest by `@compare`, or `None` |
| `max` | `[T Compare] mut @max() -> Option[T]` | largest by `@compare`, or `None` |
| `nth` | `mut @nth(n int) -> Option[T]` | 0-based `n`-th element, or `None` |
| `last` | `mut @last() -> Option[T]` | last element, or `None` |

`sum(zero T)` takes the additive identity (`0` / `0.0`) explicitly instead of
relying on a numeric protocol — it makes the element type and the empty-iterator
result unambiguous.

## Recipes

```nova
import std.collections.vec_lazy

// Transform then collect
let doubled = v.lazy().map(|x| x * 2).collect()

// Filter then sum
let total = v.lazy().filter(|x| x % 2 == 0).sum(0)

// Sum of squares of the odd elements
let s = v.lazy().map(|x| x * x).filter(|x| x % 2 == 1).fold(0, |acc, x| acc + x)

// Window the middle: drop 2, keep 3
let mid = v.lazy().skip(2).take(3).collect()

// filter_map: keep + transform in one pass
let tripled3 = v.lazy()
    .filter_map(|x| if x % 3 == 0 { Some(x * 10) } else { None })
    .collect()

// enumerate: project index + value in the SAME stage (collapse the tuple with map)
let projected = v.lazy().enumerate().map(|p| p.0 * 100 + p.1).collect()

// Short-circuiting search — stops at the first match
let found = v.lazy().find(|x| x > 100)

// Bounded scan — only the first 3 are ever touched
let early = v.lazy().map(|x| x + 1).take(3).any(|x| x == 3)
```

## Known limits (Phase A)

- **`enumerate` then a tuple-preserving adapter.** `enumerate().map(|p| ...)` (the
  `map` consumes the `(int, T)` tuple in the same stage) is supported. Chaining a
  tuple-PRESERVING adapter directly after `enumerate` — `enumerate().filter(..)` /
  `.take(n)` / `.skip(n)`, where the element stays the tuple — is gated on a
  residual closure-typing gap (`[M-153.2-tuple-elem-adapter]`); collapse the tuple
  with `map` first.
- **Phase B adapters not yet present** (roadmap, not a simplification):
  `zip`/`unzip`/`chain`/`flat_map`/`flatten`/`scan`/`inspect`/`step_by`/
  `take_while`/`skip_while`/`peekable`/`min_by[_key]`/`max_by[_key]`/`partition`/
  `chunk_by`/`into_iter`, plus mutable iteration (`for mut x` / `mut @iter()`) and
  `FromIterator`/`collect`-into-arbitrary-target.
- **Cost.** `BoxIter[T]` is now a `value` record (D277 Stage 1), so the **wrapper
  record itself costs zero heap allocations** — a `v.lazy().map().filter().collect()`
  chain went from 5 `BoxIter` heap boxes to **0**, passed by value on the stack.
  What remains boxed in this model is the per-adapter **`step` closure** (a heap
  thunk + a box of the captured source, plus a `step()` pointer call per element).
  For an allocation-free *and* indirection-free chain, use the zero-cost
  generic-over-source sibling — see below.

## Zero-cost sibling — `collections.vec_iter_zc`

`vec_lazy`/`BoxIter` is the **closure-fluent** surface: one erased cursor type,
uniform `BoxIter[T]` at every stage, at the cost of a boxed `step` per adapter.
For hot paths there is an **allocation-free, indirection-free** sibling module:

```nova
import std.collections.vec_iter_zc

let v = Vec[int].from([1, 2, 3, 4, 5, 6])
let got = v.ziter().zmap(|x| x * 10).zfilter(|x| x > 25).zcollect()
assert(got == [30, 40, 50, 60])
```

Each adapter is its **own generic-over-source `value` record** (`MapIter[I,T,U]` /
`FilterIter[I,T]` / `FilterMapIter[I,T,U]`) that holds the upstream iterator
**inline** as a field `src I` — not a boxed `step` closure. `@next()` calls
`(@src).next()` by a **static, monomorphized** dispatch, so a chain
`v.ziter().zmap(f).zfilter(p)` monomorphizes to a *single* nested concrete type
`FilterIter[MapIter[VecIter[int], int, int], int]`, and every `.next()` inlines
down to the base `VecIter.next()` — no per-element function-pointer call.

| | `vec_lazy` (`BoxIter`) | `vec_iter_zc` (Map/Filter) |
|---|---|---|
| wrapper record per adapter | 0 heap (by-value, D277 Stage 1) | 0 heap (by-value) |
| source box (`_box_src`) per adapter | 1 heap | **0** (source held inline) |
| `step` closure thunk per adapter | 1 heap (`NovaClosBase`) | **0** (static dispatch) |
| per-element source indirection | fn-ptr call | **none** (inlined) |
| capture-free `f`/`pred` closure env/box | 0 heap (D277 Stage 3 — static singleton) | 0 heap (static singleton) |
| terminator body (`collect_into`/`fold`/`sum`/…) | — | **0 `nova_alloc`** (D277 Stage 4) |
| residual heap | a *capturing* `f`/`pred`'s env + the `VecIter` source cursor | same |

For the canonical `map().filter().collect()` chain this removes **6 adapter
allocations and 9 source boxes**. As of D277 **Stage 3**, a closure with **no
captures** (the common `|x| x * 3` form) costs **0 heap** too — it is emitted as a
file-scope static singleton instead of a per-call-site env-box + closure-box
(measured: closure allocs `4 → 0` for the `.zmap().zfilter().zcollect()` chain,
`6 → 0` with a `.zfold()`). The only heap left for an all-capture-free chain is the
`VecIter` source cursor; a *capturing* closure still allocates its env per instance
(irreducible without closures-as-mono-types — `[M-153.2-closure-as-mono-type]`), and
the **call itself** is still a fn-ptr indirection (`[M-153.2-Z-closure-devirt]`).

**Allocation summary** (canonical chain over a `Vec[int]`, measured in generated C):

| chain | boxed `vec_lazy` | zero-cost `vec_iter_zc` | + Stage 3 devirt (`vec_iter_zc`) |
|---|---|---|---|
| `.map(f).filter(p).collect()` (capture-free `f`/`p`) | wrapper + source + step + closure heap | source/step **0**; closure env **4** | closure env **0** (singleton); result `Vec` only |
| `.map(f).filter(p).collect_into(out)` | — | terminator body **0 `nova_alloc`** (Stage 4) | **0** + amortized **0** result (reuses `out`) |
| `.map(f).filter(p).fold(0, g)` (capture-free) | closure heap | closure env **6** | closure env **0**; result scalar (**0**) |

The two coexist behind separate explicit imports — `vec_iter_zc` is **not** a
replacement. Reach for it on hot paths; `vec_lazy` stays the ergonomic
single-cursor default. Entry is `v.ziter()`; adapters `zmap`/`zfilter`/
`zfilter_map`; terminators `zcollect`/`zcollect_into`/`zfold`/`zcount`/`zsum`/
`zfor_each`/`zany`/`zall`/`zfind`. `take`/`skip`/`enumerate` (stateful /
tuple-element) remain on boxed `vec_lazy` for now.

`zcollect_into(out)` is the **allocation-free** sink (D277 Stage 4): it drains the
chain by **appending** into a caller-supplied reusable `Vec[T]` instead of
allocating a fresh result. Its monomorphized body is `0 nova_alloc`. Clear the
buffer first to use it as a fresh sink (`out.clear()` keeps the backing store, so a
reused `out` amortizes to **zero** allocations):

```nova
mut out = Vec[int].new()
for batch in batches {
    out.clear()                                       // len=0, buffer kept
    batch.ziter().zmap(|x| x * 2).zfilter(|x| x > 0).zcollect_into(out)
    consume(out)                                      // reuse `out` next iteration
}
```

## See also

- [`vec-internals.md`](vec-internals.md) — module layout, the boxed-fluent shape,
  the zero-cost generic-over-source sibling, Compare/Equal.
- [D260](../spec/decisions/02-types.md#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532) — boxed-fluent decision record.
- [D277](../spec/decisions/02-types.md#d277-by-value-мономорфизация-generic-value-records--generic-over-source-zero-cost-адаптеры-plan-1532-ф2) — by-value `BoxIter` monomorphization + the zero-cost `vec_iter_zc` sibling.
- [Q-iterator-laziness](../spec/open-questions.md) — why lazy is the canon.
