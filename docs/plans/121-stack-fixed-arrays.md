// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 121 — Stack-allocated fixed-size arrays `[N]T`

> **Создан 2026-06-01.** Extracted из Plan 120 `[M-120-stack-arrays]`.
> **Статус:** 📋 PLANNED
> **Приоритет:** P2 — language ergonomics (hot-path math/physics arrays,
>   zero-GC-pressure fixed buffers). Разблокируется после Plan 120 (named
>   tuples) т.к. использует ту же stack/value-type семантику.
> **Зависимости:**
>   - D215 ✅ (Plan 120: named tuple + allocation contract — establishes
>     the `()` = stack semantic foundation)
>   - D52 (type declarations — need extended fixed-array form)
>   - D32 (value/reference contract — `[N]T` is value type like tuple)
> **D-блоки:** **новый D-блок** для fixed-array syntax + semantics +
>   amend D52 (syntax table) + amend D32 (value type taxonomy).

---

## Зачем

После Plan 120 Nova имеет:
- `type Vec3(x f64, y f64, z f64)` — named tuple, stack value type
- `(f64, f64, f64)` — positional tuple, stack value type

Не хватает: `[3]f64` — fixed-size array, stack value type.

**Use cases:**
- `[4][4]f64` — matrix 4×4 для graphics/physics
- `[256]u8` — fixed buffer для I/O без heap alloc
- `[N]Vec3` — array of structs, cache-friendly layout
- FFI: `[8]u8` для UUID, `[6]u8` для MAC address

**Отличие от `[]T`** (dynamic array):
| | `[N]T` (план) | `[]T` (текущий) |
|---|---|---|
| Allocation | Stack (value type) | Heap (GC-tracked) |
| Size | Const at compile time | Dynamic |
| Pass semantics | Copy | Reference |
| GC tracking | None | Yes |

---

## Дизайн (предварительный)

### Тип

```nova
// Fixed-size array type: [N]T where N is compile-time constant
type Matrix4([4][4]f64)           // named tuple wrapping 4×4 matrix
let buf [256]u8                   // local stack array

// Literals (TBD):
let arr = [1, 2, 3] [3]int       // literal with type annotation
let zeros = [0; 256] [256]u8     // repeat-init: 256 zeros
```

### Indexing и length

```nova
arr[0]          // indexing (bounds-check at runtime or static?)
arr.len         // compile-time constant N
```

### Value semantics

```nova
fn process(buf [256]u8) { ... }  // passed by copy (large arrays → consider &[N]T reference)
```

### Bounds checking

- Option A: always runtime bounds-check (safe, perf cost)
- Option B: static analysis where index is const (zero-cost)
- Decision: deferred to implementation phase

---

## Scope

**In scope V1:**
- `[N]T` type syntax в type declarations и local vars
- Integer literal N (not generic const — deferred)
- Indexing `arr[i]` с runtime bounds check
- `.len` property (compile-time constant)
- Codegen: C array `T arr[N]`
- Value semantics (copy on pass)

**Out of scope V1:**
- Generic const N (`[N where N: const]T`)
- Slice references (`&[N]T`)
- Const-folded bounds check (optimization, followup)
- SIMD intrinsics on fixed arrays

---

## Зависимость от других планов

- **Plan 118** (typed pointers) — `*T` и `&[N]T` reference to stack array;
  Plan 121 V1 может landed до Plan 118 если не нужен `&[N]T`.
- **Plan 120** ✅ — foundation для stack value type semantics.
