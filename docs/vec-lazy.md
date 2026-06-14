<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Lazy iterators over `Vec[T]` / `[]T`

> **Audience:** Nova users. **Spec:** [D260](../spec/decisions/02-types.md#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532)
> (lazy iterator model), [D239](../spec/decisions/02-types.md#d239-t--синтаксический-псевдоним-vect)
> (`[]T ≡ Vec[T]`). **Internals:** [`vec-internals.md`](vec-internals.md). Plan 153.2.

A lazy iterator processes a vector **one element at a time, on demand**, with **no
intermediate allocations**. Building a pipeline does no work; only a *terminator*
pulls elements through it, and it pulls only as many as it needs.

```nova
import std.collections.vec_lazy

let v = Vec[int].of(1, 2, 3, 4, 5, 6)
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
let v = Vec[int].of(1, 2, 3, 4, 5)

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
| `collect` | `mut @collect() -> Vec[T]` | drain into a fresh `Vec` (default collect-target) |
| `collect_set` | `[T Hash] mut @collect_set() -> Set[T]` | drain into a `Set` (dedup) |
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

## FromIterator / collect-target (Plan 153.6, D264)

Materialise a pipeline (or any iterator source) into a chosen collection.

```nova
import std.collections.vec_lazy
import std.collections.set.{Set}
import std.collections.hashmap.{HashMap}

// Default target — Vec
let v = src.lazy().map(|x| x * 2).collect()

// Set target — dedup (Rust `iter.collect::<HashSet<_>>()`)
let s = src.lazy().filter(|x| x > 0).collect_set()

// HashMap target — collect pairs, then `from`
let m = HashMap[int, int].from(src.lazy().map(|x| (x, x * x)).collect())

// Set target (alternative) — collect a Vec, then `from_iter`
let s2 = Set[int].from_iter(src.lazy().collect())

// Build a Vec from ANY Iter source directly (no lazy stage) — `@extend`
let from_range = Vec[int].new().extend(0..5)        // [0, 1, 2, 3, 4]
let from_vec   = Vec[int].new().extend(other_vec)   // copy
```

Nova types iterators **structurally** ([D58]): any `mut @next() -> Option[T]` is
iterable, so FromIterator is a *set* of constructors/terminators rather than one
enforced single-method protocol — `@collect`/`@collect_set` (terminators),
`from`/`from_iter` (constructors from a collected `Vec`), `@extend` (build from a
source). Gated (compiler gaps, not simplifications): a *static* generic
`Vec[T].from_iter[S Iter[T]]` constructor (`[M-153.6-collect-static-generic]` — use
`Vec[T].new().extend(src)`) and a tuple-element `@collect_map()` terminator
(`[M-153.6-collect-map-tuple-receiver]` — use `HashMap.from(pairs.collect())`).

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
  `chunk_by`/`into_iter`, plus mutable iteration (`for mut x` / `mut @iter()`).
  (`FromIterator`/collect-target is done — see the section above, D264.)
- **Cost.** The current model boxes each `step` closure (a heap thunk per adapter).
  A zero-cost, fully-monomorphized generic-over-source variant is a planned perf
  upgrade (`[M-153.2-generic-over-source-zerocost]`); it does not change the API.

## See also

- [`vec-internals.md`](vec-internals.md) — module layout, the boxed-fluent shape,
  Compare/Equal.
- [D260](../spec/decisions/02-types.md#d260-ленивый-итератор-vect--boxed-fluent-адаптеры-plan-1532) — the decision record.
- [D264](../spec/decisions/02-types.md#d264-vec-протоколы-hash--fromiterator--collect-target-plan-1536) — Hash + FromIterator / collect-target.
- [D58]: ../spec/decisions/03-syntax.md — `Iter`/`Next` structural iteration.
- [Q-iterator-laziness](../spec/open-questions.md) — why lazy is the canon.

[D58]: ../spec/decisions/03-syntax.md
