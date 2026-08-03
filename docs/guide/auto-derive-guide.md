# Auto-derive Guide (Plan 126, D109 amend + D230)

**English** | [Русский](auto-derive-guide.ru.md)

> **Status:** ✅ landed 2026-06-05.
> **D-blocks:** [D109 amend](../../spec/decisions/08-runtime.md#d109-amend-plan-126-2026-06-05---auto-derive-для-пользовательских-типов) + [D230 NEW](../../spec/decisions/02-types.md#d230-new--Clone-protocol-plan-126-ф1).

Nova supports **auto-derive** for five built-in protocols via an `#impl(P)`
annotation on a user-defined type. An analog of Rust's `#[derive(...)]`
with no separate keyword — the same `#impl(P)` mechanism is reused (D186).

## TL;DR

```nova
#impl(Equal + Hash + Clone + Compare + Display)
type Vec3 {
    x f64
    y f64
    z f64
}

ro a = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
ro b = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
assert(a == b)             // auto-derived @equal
ro c = a.clone()           // auto-derived @clone
ro h = a.hash()            // auto-derived @hash
ro cmp = a.compare(b)      // auto-derived @compare
```

The compiler synthesizes method bodies **memberwise, recursively**, based on
the type's fields.

## Supported protocols

| Protocol     | Method                          | Synthesis strategy                            |
|--------------|--------------------------------|---------------------------------------------|
| `Equal`  | `@equal(other) -> bool`       | memberwise `&&` chain                       |
| `Hash`   | `@hash() -> u64`               | XOR + rotate FxHash-style combine           |
| `Clone`  | `@clone() -> Self` ([D230](../../spec/decisions/02-types.md#d230-new--Clone-protocol-plan-126-ф1)) | a record literal with `.clone()` per field |
| `Compare` | `@compare(other) -> int`       | lexicographic if-chain (memcmp-style)       |
| `Display`  | `@display(sb) -> ()`               | `sb.append("TypeName { f: v, ... }")` chain |

All 5 are single-method built-in protocols, declared in
`std/prelude/protocols.nv`.

## When the compiler synthesizes

1. The type is marked `#impl(P)` where `P` is one of the 5 built-in protocols.
2. The type does **not** provide an explicit `fn T @method(...)` — otherwise
   the user's version wins.
3. All of the type's fields are **eligible** — primitive OR have `#impl(P)`
   OR have an explicit `fn FieldType @method`.

If even one condition is violated — a diagnostic from the `E_AUTO_DERIVE_*`
family (see below).

## When the compiler does NOT synthesize

- **The protocol isn't built-in** (a user-defined protocol) — auto-derive is
  only for the 5 known built-ins. User-defined protocols → the user writes
  the body by hand.
- **The type provides an explicit method** — `fn T @equal(other) -> bool => ...`
  wins over auto-derive (a manual override).
- **The field type doesn't implement** the required protocol →
  `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`.

## Field eligibility

Every field of the type must be one of:

| Field category | What the synthesizer does |
|---|---|
| Primitive (`int`/`f64`/`bool`/`char`/`byte`/`str`/`u*`/`i*`) | Inline copy/compare/hash via built-in routines |
| `#impl(P)` annotated record/tuple | Recursive call `@field.method(...)` |
| Explicit `fn FieldType @method` | Direct dispatch to the user-provided method |
| `[]T` array | Recursive over `T` |
| Tuple `(A, B, ...)` | Recursive over element types |

What's **not eligible** — `fn(...)` types, pointers `*T`, opaque types,
protocol types (they require an explicit user impl).

## Examples

### A simple record

```nova
#impl(Equal)
type Money {
    cents int
}

ro a = Money { cents: 100 }
ro b = Money { cents: 100 }
assert(a == b)  // → @a.cents == b.cents → true
```

### Recursive auto-derive

```nova
#impl(Clone)
type Inner {
    name str
    code int
}

#impl(Clone)
type Outer {
    inner Inner       // ← Inner has #impl(Clone) — eligible
    count int
}

ro o = Outer { inner: Inner { name: "x", code: 1 }, count: 5 }
ro p = o.clone()
// synthesized:
//   Outer { inner: @inner.clone(), count: @count }
// → Outer { inner: Inner { name: @name, code: @code }, count: 5 }
```

### Manual override (user wins)

```nova
#impl(Equal)
type CaseInsensitive {
    text str
}

// User implements @equal — wins over auto-derive.
fn CaseInsensitive @equal(other CaseInsensitive) -> bool =>
    @text.to_lower() == other.text.to_lower()

ro a = CaseInsensitive { text: "Hello" }
ro b = CaseInsensitive { text: "HELLO" }
assert(a == b)  // → user-defined logic
```

### Named tuple (Plan 120 D215)

```nova
#impl(Equal + Clone)
type Pair(left int, right int)

ro p = Pair(1, 2)
ro q = Pair(1, 2)
assert(p == q)
ro r = p.clone()
```

### Heap-record `==` override

Before Plan 126, on a heap record `a == b` was **identity-eq** (pointer
comparison). After Plan 126:

```nova
// Without #impl(Equal) — identity-eq preserved (backward compat).
type Account {
    id int
    balance f64
}
ro a = Account { id: 1, balance: 100.0 }
ro b = Account { id: 1, balance: 100.0 }
assert(a != b)  // ← different allocations, identity doesn't match

// With #impl(Equal) — structural eq.
#impl(Equal)
type AccountStruct {
    id int
    balance f64
}
ro x = AccountStruct { id: 1, balance: 100.0 }
ro y = AccountStruct { id: 1, balance: 100.0 }
assert(x == y)  // ← memberwise structural eq
```

## Diagnostics (Plan 126 Ф.4)

| Code                                  | When it triggers                                                              |
|---------------------------------------|--------------------------------------------------------------------------------|
| `E_AUTO_DERIVE_CYCLE`                 | Cyclic recursion through fields doesn't terminate                                 |
| `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`  | Field type doesn't implement the required protocol                                     |
| `E_AUTO_DERIVE_UNKNOWN_PROTOCOL`      | Protocol isn't in the built-in list (`Equal`/`Hash`/`Clone`/`Compare`/`Display`) |
| `E_AUTO_DERIVE_UNSUPPORTED_KIND`      | Type kind (Newtype/Alias/Effect/Protocol/Opaque) doesn't support derive        |

### Example: E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL

```nova
type Plain {
    n int
}

#impl(Equal)
type Wrapper {
    inner Plain    // ← Plain doesn't #impl(Equal)
}
// ❌ E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL:
//   type `Wrapper` claims `#impl(Equal)` but field `inner`
//   (type `Plain`) does not implement `Equal`.
//   Either add `#impl(Equal)` to `Plain`, or provide explicit
//   `fn Wrapper @equal(...)`.
```

**Fix**: add `#impl(Equal)` to `Plain`:

```nova
#impl(Equal)   // ← Fix: now Plain eligible
type Plain {
    n int
}

#impl(Equal)
type Wrapper {
    inner Plain
}
```

## Cycle detection

The compiler maintains a **visited set** `(type, protocol)` during
synthesis. If synthesis for type `T` is already underway and a recursive
path back to `T` is encountered — `E_AUTO_DERIVE_CYCLE`:

```nova
#impl(Clone)
type A { b B }

#impl(Clone)
type B { a A }
// ❌ E_AUTO_DERIVE_CYCLE: cyclic recursion through fields doesn't terminate.
//    Provide explicit `fn A @clone(...)` or `fn B @clone(...)`.
```

**Fix**: an explicit impl on one of the types breaks the recursion:

```nova
#impl(Clone)
type A { b B }

fn A @clone() -> A => A { b: @b }   // ← manual; the synthesizer for B will keep working
```

## Composition with Plan 124.x semantics

Auto-derive is **compatible** with:

- **The `priv` field modifier** ([Plan 124.1/D220 §3.3.1](../../spec/decisions/02-types.md#d220)):
  the synthesizer runs in type-method scope — it has access to `priv` fields.
- **The `mut` field modifier** ([D33](../../spec/decisions/02-types.md#d33)):
  `mut` fields are copied like ordinary fields, mutability is preserved in
  the new value.
- **The `ro` binding** ([D33](../../spec/decisions/02-types.md#d33), [D175](../../spec/decisions/02-types.md#d175)):
  synthesized methods receive a `ro Self` receiver — read-only access.
- **Value record `type X value { ... }`** ([Plan 124.8 D228](../../spec/decisions/02-types.md#d228)):
  full support, synthesis works identically to a heap record.
- **Named tuple `type X(a int, b str)`** ([Plan 120 D215](../../spec/decisions/02-types.md#d215)):
  fields are processed through `NamedTupleField` exactly like `RecordField`.

## Sum-type rich synthesis (Plan 180 Ф.1, D345 — ✅ landed)

All six built-in protocols are synthesized for sum types via
`match @ { … }` with one arm per variant (`SumVariantKind::Unit`/`Tuple`/`Record`).
Payload elements are bound in the arm pattern and recurse exactly like
record fields.

| Marker                          | Form                                                          | Status |
|---------------------------------|----------------------------------------------------------------|--------|
| `[M-126-sum-equal-rich]`        | same-variant + payload-wise `==` (nested match, cross-variant → false) | ✅ CLOSED |
| `[M-126-sum-hash-rich]`         | variant-index seed ⊕ payload-hash (rotate-XOR combine)         | ✅ CLOSED |
| `[M-126-sum-clone-rich]`        | match-arm-per-variant reconstruction (payload primitives shallow, composites `.clone()`) | ✅ CLOSED |
| `[M-126-sum-compare-rich]`      | variant-index order, then payload lexicographic                | ✅ CLOSED |
| `[M-126-sum-fmt-rich]`          | variant-aware `@display`/`@debug` (`V` / `V(x, y)` / `V { f: x }`) | ✅ CLOSED |

> **Ergonomics note:** a method call on a bare unit variant (`Nought.hash()`)
> mis-infers to the variant's type; annotate it via a local
> `ro n Colour = Nought` (the same bidirectional-inference boundary as for
> `Empty` collisions, D141).

**Serialize/Deserialize (Plan 180 Ф.2-sum, externally-tagged — ✅ landed).**
`#impl(Serialize + Deserialize)` on a sum → an externally-tagged wire (Q4):
unit → `"V"`; single-payload → `{"V": x}`; tuple → `{"V": [a, b]}`; record →
`{"V": {fields}}`. Deser reads the tag (`is_str` → bare string / single
object key), an unknown tag → `DeError{UnknownVariant}`.
Internal/adjacent/untagged tagging → followup `[M-180-serde-tagging-modes]`
(gated on `#serde` attributes). Example: `nova_tests/serde/sum_autoderive.nv`.

## What's NOT supported in V1 (followup)

| Marker                          | Description                                                       |
|---------------------------------|----------------------------------------------------------------|
| `[M-126-codegen-method-table]`  | V1: the synthesized FnDecl isn't registered in method_table. Codegen wiring for full `a == b` runtime semantics — V2 expansion |

V1 focuses on the type-check level — auto-derive **correctly suppresses**
`E_IMPL_MISSING_METHODS`, which unblocks pattern usage in downstream
type-checked code. Full `==` wiring through method_table is Plan 126 V2
(once it's needed in the production stdlib).

## Method-level `#impl(P)` — opt-in conformance (D268, Plan 154.1)

`#impl(P)` as a leading attribute works not only on a **type** (auto-derive
above), but also on an individual **method declaration** — it's an
**optional** marker meaning "this method implements protocol `P`'s method":

```nova
#impl(Display)
fn int @display(mut sb StringBuilder) -> () { sb.append(@) }
```

- **Opt-in, not required.** Conformance remains **structural** — a type
  with a matching method satisfies the bound `[T Display]` even without
  `#impl`. `#impl` only **adds** a signature check against `P` + explicitly
  binds `P` to the receiver type (`type_impl_protocols`), as if `P` had
  been listed on the `type` declaration.
- **Three error codes** (checker): `E_IMPL_UNKNOWN_PROTOCOL` (`P` isn't a
  protocol), `E_IMPL_NOT_A_PROTOCOL_METHOD` (`@m` isn't declared in `P`),
  `E_IMPL_SIGNATURE_MISMATCH` (the signature/receiver-mut doesn't match).
- **Where it's used in the stdlib:** all 6 primitives
  (`int/f64/bool/char/str/f32`) got concrete `#impl(Display)` +
  `#impl(Debug)` in [protocols.nv](../std/prelude/protocols.nv) — this
  fixes the mis-dispatch of `Vec[T].debug(sb)` on a primitive element
  (Plan 154.1 / D269).

Details — [D268](../../spec/decisions/10-overloading.md#d268-opt-in-конформность-протоколов-impl-на-метод-декларации)
and [Plan 154.1](../plans/154.1-impl-conformance-primitive-format.md).

## See also

- [Plan 126 — Protocol auto-derive](../plans/126-auto-derive-protocols.md) —
  the whole roadmap, design rationale, AC list.
- [D268 / D269 — method-level `#impl` + concrete primitive Display/Debug](../../spec/decisions/10-overloading.md#d268-opt-in-конформность-протоколов-impl-на-метод-декларации)
  (Plan 154.1).
- [D109 amend](../../spec/decisions/08-runtime.md#d109-amend-plan-126-2026-06-05---auto-derive-для-пользовательских-типов)
  — auto-derive rules.
- [D230 NEW](../../spec/decisions/02-types.md#d230-new--Clone-protocol-plan-126-ф1) —
  Clone protocol semantics.
- [D186 — `#impl(P)` annotation](../../spec/decisions/02-types.md#d186) —
  foundation infrastructure.
- [std/prelude/protocols.nv](../std/prelude/protocols.nv) — protocol
  declarations source-of-truth.
