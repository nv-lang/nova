# Function parameters in Nova

**English** | [Русский](parameters.ru.md)

> User-facing guide to parameter modifiers and their semantics.

## TL;DR

Function parameters are **read-only by default**. Want to mutate — write `mut`.

```nova
fn append(mut b []int, v int) { b.push(v) }   // ✓ mutates
fn count(b []int) -> int => b.len()           // ✓ read-only (default)
fn count(ro b []int) -> int => b.len()  // ✓ ro (synonym default)
fn drain(consume b []int) { ... }             // ✓ ownership transfer
```

## Modifiers

| Modifier | What's allowed in the callee | Passing at the call site |
|---|---|---|
| (none) — default | reading, iteration, non-mut methods | borrow (caller owns) |
| `mut` | + mut methods (`.push`, `.append`, etc.), index-assign | borrow (caller owns) |
| `ro` | same as default — synonym | borrow (caller owns) |
| `consume` | everything (owned), including mut methods | move (caller's binding is dead) |

## Combination rules

- `mut` + `consume` — ✗ `E_PARAM_MOD_CONFLICT` (consume already implies mut)
- `mut` + `ro` — ✗ `E_PARAM_MOD_CONFLICT` (mutually exclusive)
- `ro` + `consume` — ✗ `E_PARAM_MOD_CONFLICT` (`ro` forbids mutation, consume requires ownership)

## When to use what

### `mut` — need to change it and hand the caller the changed value

```nova
fn append_world(mut sb StringBuilder) { sb.append(" world") }

ro sb = StringBuilder.from("hello")
append_world(sb)
ro s = sb.as_str()                  // "hello world" — the mutation is visible
```

### default or `ro` — just read (while producing a result)

```nova
fn sum(b []int) -> int {
    mut total = 0
    for x in b { total = total + x }
    total
}
```

Use `ro` explicitly when you want to underscore the guarantee in the
API (especially for FFI/documentation):

```nova
export fn hash(ro bytes []u8) -> u64 => ...
```

### `consume` — you take ownership

```nova
fn finalize(consume sb StringBuilder) -> str => sb.as_str()

consume sb = StringBuilder.from("x")
ro s = finalize(sb)                  // sb dead after this
```

## Diagnostics

| Code | When |
|---|---|
| `E_PARAM_NOT_MUT` | calling a mut method on a parameter without `mut` |
| `E_PARAM_MOD_CONFLICT` | mutually exclusive modifiers |
| `E_READONLY_COERCE` | passing `ro T` into a `T` parameter (where `T` expects non-read-only) |

All come with machine-applicable suggestions.

## Coercion (subtyping) for parameters

Since `T` in parameter position is **already read-only** (Plan 108.1
default), most combinations are the identity. The one exception:
``ro` → `mut``.

| caller-type → callee-param | OK? |
|---|---|
| `T` → `T` (param default read-only) | ✓ (narrowing) |
| `T` → `ro T` (param explicit `ro`) | ✓ (synonym default) |
| `T` → `mut T` (param explicit mut) | ✓ (caller allows mut) |
| `ro T` → `T` (param default read-only) | ✓ — both read-only |
| `ro T` → `ro T` | ✓ |
| `ro T` → `mut T` (param explicit mut) | ✗ `E_READONLY_COERCE` |
| `mut T` → `T` (param default read-only) | ✓ (narrowing) |
| `mut T` → `mut T` | ✓ |

## Receiver methods

Receiver mutability is set separately from ordinary parameters:

```nova
fn StringBuilder @len() -> int               // read-only receiver
fn StringBuilder mut @append(s str) -> @     // mut receiver
fn StringBuilder consume @as_str() -> str    // consume receiver
```

## Local let-bindings (Plan 108.2)

Inside a function body, local bindings follow the same rule as
parameters: **no `mut` — read-only**.

```nova
ro arr = []
arr.push(1)                       // ✗ E_LOCAL_NOT_MUT
mut arr = []
arr.push(1)                       // ✓
```

`consume X = ...` implicitly implies `mut` (like a `consume` param).

## Loop-var and pattern (Plan 108.3)

### `for mut x in iter`

The loop variable is read-only by default. Opt in with `mut`:

```nova
for x in arrs { x.push(1) }       // ✗ E_LOCAL_NOT_MUT
for mut x in arrs { x.push(1) }   // ✓
```

`for consume x in iter` — implicit mut (ownership transfer).

### Pattern per-name mut

During destructuring, `mut` is placed **on each name separately** (Rust-style):

```nova
ro (a, b) = pair                  // both immutable
ro (mut a, b) = pair              // a mutable, b immutable
ro (a, mut b) = pair              // a immutable, b mutable
ro (mut a, mut b) = pair          // both mutable
```

**Group-mut is forbidden** — `let mut (a, b) = ...` is rejected at the
parser level (`E_PATTERN_GROUP_MUT`): the `mut` keyword applies to one
name, not to the whole pattern.

## See also

- `spec/decisions/02-types.md` D176 — formal spec params.
- `spec/decisions/02-types.md` D36 + amend Plan 108.2/108.3 — formal spec locals + loop-var + pattern.
- `docs/dev/migration/d176-param-readonly-default.md` — params migration guide.
- `docs/dev/migration/d36-let-mut-enforcement.md` — locals migration guide.
- D131 (Plan 73) — consume affine semantics.
- D157 (Plan 100.3) — view-borrow for consume-types.
