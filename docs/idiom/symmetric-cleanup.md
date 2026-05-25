# Symmetric Cleanup: `errdefer` + `okdefer`

**Related:** D160 · D90 · [cleanup-on-failure.md](cleanup-on-failure.md)

---

## The Pattern

Pair `errdefer` and `okdefer` to cover both exit paths of a resource:

```nova
consume tx = begin()
errdefer { tx.rollback() }   // error path  → rollback
okdefer  { tx.commit()   }   // success path → commit
do_work()
// Whichever path is taken, tx is consumed exactly once.
```

Without `okdefer` you'd need an explicit call on the success path:

```nova
consume tx = begin()
errdefer { tx.rollback() }
do_work()
tx.commit()                  // explicit — asymmetric
```

`okdefer` makes the pattern **symmetric**: both paths are declared at the top of the function, close to the resource acquisition.

---

## Trigger Rules (D160)

| Exit path              | `defer` | `errdefer` | `okdefer` |
|------------------------|---------|------------|-----------|
| Normal end-of-scope    | ✅      | ❌         | ✅        |
| `return expr`          | ✅      | ❌         | ✅        |
| `throw` / `expr?`      | ✅      | ✅         | ❌        |
| `panic(msg)`           | ✅      | ✅         | ❌        |
| `interrupt v`          | ✅      | ❌         | ❌        |
| `exit(code)`           | ❌      | ❌         | ❌        |

- **`defer`** — orthogonal: fires on every path (except `exit`).
- **`errdefer`** — error-only: throw / panic.
- **`okdefer`** — success-only: normal end / return.
- `errdefer` + `okdefer` together are **exhaustive** for the non-`exit` paths.

---

## LIFO Ordering with Mixed Family

All three composable; LIFO respects declaration order:

```nova
defer    { /* A */ }   // outer
okdefer  { /* B */ }
errdefer { /* C */ }
okdefer  { /* D */ }
defer    { /* E */ }   // inner
```

**Normal exit LIFO:** `E → D → B → A` (defer + okdefer; C skipped)

**Error exit LIFO:** `E → C → A` (defer + errdefer; B, D skipped)

**Interrupt path LIFO:** `E → A` (only plain defer; B, C, D skipped)

---

## Reason-Aware: `defer |result| { ... }`

When cleanup needs to behave differently based on exit reason, use the
reason-aware form:

```nova
defer |_result| {
    // body runs on any exit (plain defer semantics)
    cleanup()
}
```

The binding `_result` will hold the exit reason once `DeferResult[T,E]`
type injection is implemented. Use `_` prefix to ignore until then.

---

## Body Constraints (D90 §4–6)

`okdefer` body has the same constraints as `defer`/`errdefer`:

- **No `throw` / `?` / `!!`** — body must be infallible.
- **No `return`** at top level — would hijack the enclosing function's exit.
- **No `break`** at top level — would hijack the enclosing loop.
- **No `spawn`/`supervised`/`detach`/`blocking`** — body must be fast.

---

## When to Use Which

| Need                                    | Keyword                     |
|-----------------------------------------|-----------------------------|
| Cleanup on any exit (logging, metrics)  | `defer { ... }`             |
| Rollback only on error                  | `errdefer { ... }`          |
| Commit only on success                  | `okdefer { ... }`           |
| Cleanup with success/error distinction  | `defer \|result\| { ... }` |

---

## See Also

- **D160** — spec for okdefer + defer |result|
- **D90** — defer/errdefer foundation
- [cleanup-on-failure.md](cleanup-on-failure.md) — error-specific cleanup
- [consume-types.md](consume-types.md) — consume type system (D133)
