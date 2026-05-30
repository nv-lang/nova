# Consume-types in Nova

> User-facing guide для work with consume-types: bindings, ownership
> transfer, view-borrow, lifecycle.

## TL;DR

```nova
consume x = Token.new(7)    // ✓ ownership binding
let y = x                   // ✗ E_VIEW_BINDING_FORBIDDEN
consume y = x               // ✓ move (x dead, y owns)
let v = x.release()         // ✓ consume via method
```

- **Ownership** перемещается через `consume X = …`.
- **Алиас-binding** запрещён внутри тела функции (`let Y = X`).
- **View-borrow** разрешён ТОЛЬКО как function-параметр.
- Каждый consume-binding обязан быть consumed до scope-exit.

## What is a consume-type?

A *consume-type* is a type whose values represent ownership of a
non-shareable resource — file handle, mutex guard, builder buffer,
network socket.  Values cannot be copied, aliased, or implicitly
dropped; the owner must explicitly consume them.

Declaration:

```nova
type Token consume {
    val int
}
```

The `consume` keyword on the type declaration marks all instances as
consume-obligated.

## Binding rules (D180)

### Rule 1 — `consume X = …` required for consume-RHS

```nova
let t = Token.new(7)        // ✗ E_CONSUME_KEYWORD_MISSING
consume t = Token.new(7)    // ✓
```

The compiler statically detects when a binding receives a consume-type
value and requires the `consume` keyword.

### Rule 2 — `let Y = consume_var` forbidden in function body

```nova
consume sb = StringBuilder.new()
let view = sb                       // ✗ E_VIEW_BINDING_FORBIDDEN
```

Aliasing a consume-obligation would create a dangling reference once
`sb` is consumed; aliasing inside function bodies is forbidden.

### Rule 3 — `consume Y = X` moves ownership

```nova
consume a = Token.new(11)
consume b = a               // move — a dead, b owns
let v = b.release()         // ✓
```

After the move, `a` is consumed; using it triggers a
`use-after-consume` diagnostic.

### Rule 4 — view-borrow via function parameters only

```nova
fn snapshot(t Token) -> int => t.val

consume t = Token.new(7)
let s = snapshot(t)         // ✓ view-borrow (bounded by call)
let v = t.release()         // ✓ caller still owns; consume here
```

Function parameters of consume-type without the `consume` keyword
are *views* — bounded by the callee's scope, never escape.

## Consume operations

A consume-obligation is satisfied by any of:

1. **`consume X = X_old`** — move to new binding (Rule 3).
2. **`X.method(...)`** where method is a `consume @` method.
3. **`return X`** — return from function.
4. **Implicit return** — trailing expression of function body is `X`
   (or a fluent-chain rooted at `X`; see [M-73.1-fluent-return-
   implicit-consume]).
5. **Pass to consume-parameter** — `f(X)` where `f` takes `consume X`.

## Fluent-return chains (`-> @`)

A method declared with `-> @` return type returns the receiver itself
(Plan 77, D132).  Fluent chains compose mutators:

```nova
consume sb = StringBuilder.new()
let s = sb.append("a").append("b").as_str()   // chain + consume
```

When such a chain is the trailing expression of a function body, the
chain root's consume-obligation is satisfied by the implicit return
(M3, Plan 73.1 V3).

## Diagnostics

| Code                              | Trigger                                     |
| --------------------------------- | ------------------------------------------- |
| `E_CONSUME_KEYWORD_MISSING`       | `let X = ctor()` when ctor returns consume |
| `E_VIEW_BINDING_FORBIDDEN`        | `let Y = consume_var` in function body     |
| `W_CONSUME_KEYWORD_UNNECESSARY`   | `consume X = …` when RHS is non-consume    |
| `D133-not-consumed`               | scope-exit with unsatisfied obligation     |
| `D133-use-after-consume`          | use of consumed binding                    |

All diagnostics carry machine-applicable suggestions (Plan 50 D102).

## References

- `spec/decisions/05-memory.md` — D131 (affine semantics), D180 (binding
  syntax)
- `docs/plans/73.1-consume-binding-syntax.md` — Plan 73.1 status
- `docs/migration/d180-binding-syntax.md` — migration guide
