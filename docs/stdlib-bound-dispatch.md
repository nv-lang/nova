# Stdlib bound dispatch — migration guide

> **Created 2026-05-16 (Plan 56).** Guide для stdlib authors на тему
> когда / как использовать bound generic dispatch.

## TL;DR

```nova
// ✅ Используй bound когда method works через protocol contract:
fn unique[T Hashable](items []T) -> []T { ... }
type HashMap[K Hashable, V] { ... }

// ✅ Bound methods обязаны быть pure (no Io/Fail/Db):
type Hashable protocol {
    hash() -> u64    // pure, OK
    eq(other Self) -> bool    // pure, OK
}

// ❌ Bound methods НЕ должны иметь effects:
type BadBound protocol {
    save() Io -> ()    // ❌ Io в bound — vtable dispatch ломает effect handlers
}
```

## When to use bound

| Use case | Подход |
|---|---|
| Collection требует hash/eq на K | `K Hashable` |
| Sorted collection / sort algorithm | `K Comparable` |
| Display / debug formatting | `T Display` |
| Multiple operations | `T Hashable + Display` |
| Just need polymorphism (no contract) | bare `[T]` (structural) |

## When NOT to use bound

| Anti-pattern | Альтернатива |
|---|---|
| Effects in bound (Io, Fail, Db) | Use регулярный effect param |
| Single-method use | Free function `fn f[T](x T, hasher fn(T) -> u64)` |
| Stateful (mutable Self) | Mutable struct + free fn API |

## Performance characteristics (Plan 56)

| Dispatch path | When | Cost |
|---|---|---|
| Mono (concrete K) | `HashMap[str, int]` use sites | ~0ns (inline'd) |
| Vtable (erased K) | Generic body внутри stdlib | ~1-2ns indirect |

Mono path **predominates** для typical user code (concrete instances).
Vtable — fallback для erased context (Plan 56 Ф.1 vtable infra).

## Bootstrap limitations

1. **Method-level generic на generic type** не работает (Plan 48 partial):
   ```nova
   // ❌ Не работает в bootstrap:
   fn HashMap[K, V] @map_values[U](f fn(V) -> U) -> HashMap[K, U] { ... }
   // (HashMap[K, V].@method[U] — два уровня generic)
   ```
   Workaround: inline (write the body in caller).

2. **Tuples не monomorphized** (Plan 59):
   ```nova
   // ❌ `for (k, v) in coll` не работает для struct K/V
   //    (nova_str, user records) — _NovaTupleN.f* всегда nova_int slots.
   ```
   Workaround: direct field access (Plan 56 array element type propagation
   handles это).

3. **Multi-bound + vtable** — partial (Plan 56 Ф.2):
   Multi-bound `T: A + B` сейчас works через mono path только. Full
   vtable codegen для erased multi-bound — future.

## Examples из stdlib

### HashMap (uses Hashable + structural V)

```nova
type Hashable protocol { hash() -> u64; eq(other Self) -> bool }

type HashMap[K Hashable, V] { ... }

// Methods use bound K methods через mono path (concrete K на use site):
fn HashMap[K, V] @get(key K) -> Option[V] {
    let idx = @find_slot(key)   // @find_slot uses key.hash(), key.eq()
    ...
}

// Через Plan 56 array element type propagation, direct field access
// также работает в methods:
fn HashMap[K, V] @clone() -> HashMap[K, V] {
    let mut copy = HashMap[K, V].with_capacity(@count)
    for i in 0..@buckets.len() {
        match @buckets[i] {
            Occupied { key: k, value: v } => copy.insert_new(k, v)
            _ => {}
        }
    }
    copy
}
```

### Free function (no bound)

```nova
// Compiler делает structural check на use site (не обязан Hashable).
fn first[T](xs []T) -> Option[T] => xs.get(0)
```

## Related

- [D72](../spec/decisions/02-types.md#d72) — generic bounds (type-checker).
- [D110](../spec/decisions/02-types.md#d110) — hybrid dispatch
  (codegen runtime).
- [Plan 56](plans/56-vtable-dispatch-erased-generics.md) — vtable infra.
- [Plan 48](plans/48-closures-in-generics.md) — monomorphization.
- [Plan 59](plans/59-tuple-monomorphization.md) — mono'd tuples (future).
- [docs/perf-conventions.md](perf-conventions.md) — cost table.
