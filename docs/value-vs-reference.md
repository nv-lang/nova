# Value Types vs Reference Types in Nova

> Plan 120 (D215, 2026-05-31): explicit stack/heap allocation guide.
> Cross-ref: [spec/decisions/02-types.md → D215](../spec/decisions/02-types.md#d215).

## The bracket rule

Nova uses **bracket syntax to encode allocation semantics**:

| Syntax | Category | Allocation | Semantics |
|---|---|---|---|
| `type X(T1, T2)` | positional tuple | **stack** | value (copy on pass) |
| `type X(name T, ...)` | named tuple | **stack** | value (copy on pass) |
| `type X { field T }` | record | **heap** (GC-managed) | reference (pointer on pass) |
| `type X \| A \| B` | sum type | heap | reference |
| `int`, `bool`, `f64`, `char`, `u8` | primitives | register | value |

The choice of `()` vs `{}` is not arbitrary: it explicitly communicates
performance and lifetime expectations to both humans and AI.

## Value types: stack-allocated tuples

### Positional tuples

```nova
type Point(f64, f64)      // .0 / .1 access

ro p = Point(1.0, 2.0)
assert(p.0 == 1.0)
```

### Named tuples (Plan 120, D215)

```nova
type Vec3(x f64, y f64, z f64)   // .x / .y / .z access
type Color(r u8, g u8, b u8, a u8)

ro v = Vec3(x: 1.0, y: 2.0, z: 3.0)
ro c = Color(r: 255, g: 0, b: 128, a: 255)

assert(v.x == 1.0)
assert(c.r == 255)
```

Named tuples are identical to positional tuples in performance — both
are stack-allocated C structs, no heap allocation, no GC tracking.

### Methods on value types

```nova
fn Vec3 @dot(other Vec3) -> f64 =>
    @.x * other.x + @.y * other.y + @.z * other.z

fn Vec3 @scale(s f64) -> Vec3 =>
    Vec3(x: @.x * s, y: @.y * s, z: @.z * s)

ro v1 = Vec3(x: 1.0, y: 0.0, z: 0.0)
ro v2 = Vec3(x: 0.0, y: 1.0, z: 0.0)
assert(v1.dot(v2) == 0.0)   // perpendicular vectors
```

## Reference types: heap-allocated records

```nova
type User {
    id u64
    name str
    email str
}

ro u = User { id: 1, name: "alice", email: "alice@example.com" }
// u is a pointer to managed heap; GC-tracked
```

## When to use which

| Use case | Type | Why |
|---|---|---|
| Hot-path math (Vec3, Matrix, Quaternion) | named tuple | zero GC, predictable perf |
| Pixel/sample formats (Color, AudioSample) | named tuple | small, copy-cheap |
| FFI multi-value returns | named tuple | fits in registers |
| Iterator state (idx, end, step) | named tuple | local-lifetime, no heap |
| Geometric primitives (Point, Rect, AABB) | named tuple | value semantics |
| Domain entities (User, Order, Account) | record | identity, sharing across fibers |
| Large aggregates with many fields | record | copy expensive |
| Types shared between modules / persisted | record | reference semantics natural |

**General rule:** small + copy-cheap + value-semantics → tuple;
large / identity-bearing / shared → record.

## Cross-access errors

Nova enforces the boundary between named and positional tuples:

```nova
type Named(x f64, y f64)
type Positional(f64, f64)

fn Named @bad() -> f64 => @.0      // E_TUPLE_POSITIONAL_ACCESS_ON_NAMED
fn Positional @bad() -> f64 => @.x  // E_TUPLE_NAMED_ACCESS_ON_POSITIONAL
```

## Comparison with other languages

| Language | Stack value type with named fields |
|---|---|
| Rust | `struct Point { x: f64, y: f64 }` — stack by default |
| Swift | `struct Point { let x: Double; let y: Double }` — value type |
| C# | `struct Point { public double X; public double Y; }` — value type |
| Go | `type Point struct { X, Y float64 }` — escape analysis decides |
| **Nova** | `type Point(x f64, y f64)` — **explicitly stack**, bracket syntax |

Nova's bracket choice (`()` vs `{}`) makes the allocation decision
**explicit at the type declaration site** — no need to read docs or
remember language rules.
