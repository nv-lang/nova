// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 126 — Auto-derive протоколов через `#impl(...)` annotation

> **Создан 2026-06-02.** Draft scaffold.
> **Status:** 🆕 PLANNED (черновик).
> **Trigger:** discussion после Plan 124 — value-record нужны auto-equality/hash/clone/comparison.
> **Gated:** Plan 124.8 ✅ (value-record landed).
> **Эстимат:** ~3-4 dev-day.
> **Model:** Opus 4.7 + Thinking ON (codegen-synthesis, protocol semantics).

---

## 1. Контекст

### 1.1 Существующие протоколы Nova

В `std/prelude/protocols.nv` уже определены 6 формальных протоколов:

| Protocol | Метод | Source |
|---|---|---|
| `From[T]` | `@from(v T) -> Self` | std/prelude/protocols.nv |
| `Into[T]` | `@into() -> T` | same |
| `TryFrom[T]` | `@try_from(v T) -> Result[Self, E]` | same |
| `TryInto[T]` | `@try_into() -> Result[T, E]` | same |
| `Hashable` | `@hash() -> u64` | same |
| `Equatable` | `@equal(other Self) -> bool` | same (TBD method name verification) |
| `Comparable` | `@compare(other Self) -> int` | same |
| `Printable` | `@fmt(sb StringBuilder) -> ()` | std/prelude/protocols.nv:313 |

**Не существует:** `Cloneable`.

### 1.2 Текущее поведение auto-implementation

Пока `#impl(P)` (Plan 91.9 D186) **только проверяет** что type предоставляет
methods of P — но **не auto-generates** их. User должен явно писать
`fn T @equal(other Self) -> bool { ... }`.

### 1.3 Что предлагается

**Auto-generate** method bodies для known built-in protocols когда они
указаны в `#impl(...)` annotation. Memberwise recursive synthesis.

```nova
#impl(Equatable + Hashable + Cloneable + Comparable + Printable)
type Vec3 value {
    mut x f64
    mut y f64
    mut z f64
}

// Compiler synthesizes:
// fn Vec3 @equal(other Vec3) -> bool =>
//     @x == other.x && @y == other.y && @z == other.z
// fn Vec3 @hash() -> u64 =>
//     @x.hash() ^ @y.hash().rotate_left(13) ^ @z.hash().rotate_left(26)
// fn Vec3 @clone() -> Self =>
//     Vec3 { x: @x.clone(), y: @y.clone(), z: @z.clone() }
// fn Vec3 @compare(other Vec3) -> int { ... lexicographic ... }
// fn Vec3 @fmt(sb StringBuilder) -> () { ... memberwise format ... }

ro a = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
ro b = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
assert(a == b)             // ✅ memberwise auto-eq
ro c = a.clone()           // ✅ recursive clone
```

---

## 2. Design

### 2.1 NEW protocol — `Cloneable`

```nova
export type Cloneable protocol {
    fn @clone() -> Self
}
```

Single-method protocol. Deep recursive copy.

### 2.2 Auto-derive triggers

```nova
#impl(Eq+Hashable+Cloneable+Comparable+Printable)   // short form via Eq alias
type X value { ... }

#impl(Equatable + Hashable + Cloneable)             // long form
type X { ... }
```

**Когда compiler synthesize'ит:**
- Type lists protocol в `#impl(P)`.
- Type **не предоставляет** explicit method (`fn T @method()`).
- Protocol is **known built-in** (Equatable / Hashable / Cloneable / Comparable / Printable).

**Когда НЕ synthesize:**
- Protocol не built-in (user-defined protocol).
- Type предоставляет explicit method — user wins (override).
- Type содержит field, который не implement'ит requirement protocol → compile error.

### 2.3 Synthesis rules — memberwise recursive

#### `Equatable.@equal(other)`

```
@equal(other Self) -> bool :=
    field1 == other.field1 &&
    field2 == other.field2 &&
    ... &&
    fieldN == other.fieldN
```

- Для каждого поля рекурсивно вызывается `@equal` (через `==` operator).
- Primitive types (`int`, `f64`, `bool`, `char`, `str`) — built-in `==`.
- Sum-type fields — variant tag eq + payload recursive.

#### `Hashable.@hash()`

```
@hash() -> u64 :=
    initial_hash xor 
    (field1.hash() rotate_left(13 * 0)) xor
    (field2.hash() rotate_left(13 * 1)) xor
    ... xor
    (fieldN.hash() rotate_left(13 * (N-1)))
```

- Each field's hash combined via XOR + rotate (FxHash-style).
- Consistency: equal objects produce equal hashes (auto-derived).

#### `Cloneable.@clone()`

```
@clone() -> Self :=
    Self {
        field1: @field1.clone(),
        field2: @field2.clone(),
        ...
        fieldN: @fieldN.clone()
    }
```

- Recursive deep clone каждого поля.
- Primitive types — copy (no `.clone()` needed; built-in inline copy).
- `str` clone = new str с тем же контентом.
- `[]T` clone = new array с recursive clone элементов.
- Reference fields в value-record — clone underlying (`[]T` deep copy).

#### `Comparable.@compare(other)`

```
@compare(other Self) -> int :=
    let c1 = @field1.compare(other.field1)
    if c1 != 0 { return c1 }
    let c2 = @field2.compare(other.field2)
    if c2 != 0 { return c2 }
    ...
    return 0
```

- Lexicographic comparison.
- Поля сравниваются по declaration order.
- Returns -1 / 0 / 1.

#### `Printable.@fmt(sb)`

```
@fmt(sb StringBuilder) -> () :=
    sb.append("TypeName")
    sb.append(" { ")
    sb.append("field1: ")
    @field1.fmt(sb)
    sb.append(", ")
    sb.append("field2: ")
    @field2.fmt(sb)
    ...
    sb.append(" }")
```

- Human-readable representation: `Vec3 { x: 1.0, y: 2.0, z: 3.0 }`.
- Recursive `.fmt(sb)` для каждого поля.

### 2.4 Field eligibility checks

Перед synthesis compiler проверяет:
- Каждое поле type'а implement'ит protocol (либо primitive, либо `#impl`).
- Если нет → `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL` с указанием field+protocol.

Example error:
```
type Outer value {
    inner Inner    // ← Inner НЕ #impl(Equatable)
}
#impl(Equatable) type Outer { ... }   
// ❌ E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL: 
//    field `inner` (type `Inner`) does not implement `Equatable`
```

### 2.5 Heap-record auto-eq (D109 amend)

Currently heap-records `==` is identity comparison. После Plan 126:
- `#impl(Equatable)` на heap-record → auto-derive memberwise eq (overrides identity).
- Без `#impl(Equatable)` → identity default (unchanged).

Это **обратно совместимо** — old code без `#impl(Equatable)` работает identity-eq как раньше.

### 2.6 Value-record + tuple auto-derive

- Value-record: standard `#impl(...)` workflow.
- Tuple: same (Plan 120 D215 supports `#impl`).

Both уже supported в Plan 124 `f5_check_tuple_construct` + Plan 91.9 D186 protocol infrastructure — нужен только codegen-synthesizer.

### 2.7 Recursive depth — unbounded

User decision: рекурсивно неограниченно. Compile error если cycle:
- `type A value { b B }; type B value { a A }` + `#impl(Cloneable)` → 
  `E_AUTO_DERIVE_CYCLE`.

### 2.8 Composability с Plan 124 priv

Auto-derive **respects** priv: если поле `priv`, eq/hash/clone всё ещё
работает (synthesizer = type-method scope, имеет access). External
вызов `a == b` работает through public method `@equal` synthesized.

---

## 3. Phases

### Ф.0 — Investigation + audit
- Verify Equatable method name (`@equal` vs `@eq`).
- Audit existing manual implementations (StringBuilder, etc.) для conflict-check.
- Cycle detection design.
- **Эстимат:** 0.3 dev-day.

### Ф.1 — Cloneable protocol declaration
- Add `Cloneable` к `std/prelude/protocols.nv`.
- Add к prelude re-export `std/prelude/e2026_*.nv`.
- **Эстимат:** 0.2 dev-day.

### Ф.2 — Synthesis infrastructure
- New module `compiler-codegen/src/protocols/auto_derive.rs`.
- `synthesize_method(type, protocol, method_name) -> FnDecl`.
- Field iteration + recursive call generation.
- **Эстимат:** 1 dev-day.

### Ф.3 — Per-protocol synthesizers
- `synthesize_equal` — memberwise && combine.
- `synthesize_hash` — XOR+rotate combine.
- `synthesize_clone` — record literal с recursive `.clone()`.
- `synthesize_compare` — lexicographic if-chain.
- `synthesize_fmt` — sb.append chain.
- **Эстимат:** 0.7 dev-day.

### Ф.4 — Integration с #impl(P)
- При check_impl (Plan 91.9) — если protocol built-in AND no explicit
  method → call synthesizer + register synthesized method.
- Cycle detection: visited set по type + protocol.
- Field eligibility check pre-synthesis.
- **Эстимат:** 0.7 dev-day.

### Ф.5 — D109 amend (heap-record auto-eq)
- Member access `==` resolution: protocol method overrides identity.
- Spec amend.
- **Эстимат:** 0.3 dev-day.

### Ф.6 — Tests
- `nova_tests/plan126/` — ≥15 positive + ≥6 negative.
- Per-protocol synthesis test (5 protocols × value-record + heap-record + tuple).
- Recursive depth test (3 levels).
- Cycle detection test (E_AUTO_DERIVE_CYCLE).
- Field eligibility error test.
- Manual override test (explicit `fn T @method` wins).
- **Эстимат:** 0.5 dev-day.

### Ф.7 — Spec + docs
- D109 amend (auto-derive rules).
- D227 NEW (Cloneable protocol).
- `docs/auto-derive-guide.md` — user guide.
- **Эстимат:** 0.3 dev-day.

### Ф.8 — Closure
- 3 logs + plan status + commit per phase + push.
- **Эстимат:** 0.2 dev-day.

**Total:** ~3.5-4 dev-day.

---

## 4. Acceptance criteria (A126.1-A126.15)

- **A126.1** ✅ `Cloneable` protocol declared в std/prelude/protocols.nv.
- **A126.2** ✅ `#impl(Equatable)` synthesize'ит `@equal` для value-record.
- **A126.3** ✅ `#impl(Hashable)` synthesize'ит `@hash` для value-record.
- **A126.4** ✅ `#impl(Cloneable)` synthesize'ит deep `@clone`.
- **A126.5** ✅ `#impl(Comparable)` synthesize'ит lexicographic `@compare`.
- **A126.6** ✅ `#impl(Printable)` synthesize'ит memberwise `@fmt`.
- **A126.7** ✅ Recursive synthesis работает (3+ levels nested).
- **A126.8** ✅ Cycle detection: cyclic type → `E_AUTO_DERIVE_CYCLE`.
- **A126.9** ✅ Field eligibility: missing protocol → `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`.
- **A126.10** ✅ Explicit method override: user's `fn T @equal()` wins over auto.
- **A126.11** ✅ Heap-record `#impl(Equatable)` overrides identity-eq.
- **A126.12** ✅ Tuple `#impl(Equatable)` works (Plan 120 + 124.8).
- **A126.13** ✅ Value-record + composability: works с priv/consume modifiers.
- **A126.14** ✅ plan126 fixtures ≥21 PASS.
- **A126.15** ✅ Regression: plan124_1..6 + plan124_8 unchanged.

---

## 5. D-blocks impact

- **D109 amend** — auto-derive rules для built-in protocols.
- **D227 NEW** — `Cloneable` protocol.

---

## 6. Cross-references

- Plan 124.8 — value-record core.
- Plan 91.9 D186 — `#impl(P)` infrastructure.
- D109 — built-in protocols hash/eq/ord.
- Industry: Rust `#[derive]`, Java records, Kotlin data class, C# records.

---

## 7. Open questions

- **Q126.1** — `Equatable.@equal` vs `@eq` — verify в Ф.0 что точно в спеке.
- **Q126.2** — Hash combiner — FxHash-style XOR+rotate vs SipHash? Recommend FxHash (быстрее, нет crypto requirements).
- **Q126.3** — Tuple auto-derive by default (no `#impl`)? Recommend: opt-in `#impl` like records — единый mental model.
- **Q126.4** — Sum-type auto-derive? Recommend Yes — variant tag + payload recursive.
