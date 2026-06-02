// SPDX-License-Identifier: MIT OR Apache-2.0
# Research — Unified record/tuple syntax: `{}` for both, allocation as modifier

> **Создан 2026-06-02.** Design research для potential Plan 125 (или 120
> amend). Не план — research для решения «стоит / не стоит / какая форма».
> **Trigger:** observed Nova parser asymmetry (records 4 multi-line forms;
> named tuples single-line only). User question: «исследуй вариант сделать
> запись тупла идентичной рекорду, отличие только в том, что рекорд
> создается в памяти, а тупл на стеке».
> **Status:** RESEARCH — рекомендация в §10.

---

## 1. Текущее состояние Nova (Plan 120 / D52 / D123 / D215)

```nova
// Record — heap-allocated, GC-managed reference type, named fields
type Account {
    ro id u64,
    mut balance f64
}

// Named tuple — stack-allocated value type, named fields
type Vec3(x f64, y f64, z f64)

// Positional tuple — stack-allocated value type, .0/.1 access
type Pair[A, B](A, B)
```

**Asymmetries (probed empirically):**

| Capability | Record `{}` | Named tuple `()` |
|---|---|---|
| Single-line | ✅ `type X { a int, b int }` | ✅ `type X(a int, b int)` |
| Multi-line с commas | ✅ | ❌ "expected type, got newline" |
| Multi-line без commas (newline-as-sep) | ✅ | ❌ same error |
| Trailing comma | ✅ | ❌ |
| `mut` per-field | ✅ D175/D176 | ❌ all readonly (D215 §«immutable») |
| `priv` per-field | ✅ Plan 124.1 D220 | ✅ Plan 124.4 D222 |
| Methods `fn T @method()` | ✅ | ✅ |
| `.field` access | ✅ named | ✅ named (positional `.0` blocked, D215 Q120 Option B) |
| Construction syntax | `T { a: x, b: y }` | `T(a: x, b: y)` |
| Pattern destructure | `T { a, b }` | `T { a, b }` (Plan 124.4 record-style) |
| Codegen | heap C-struct + GC | stack inline |
| Pass semantics | reference (pointer) | value (copy) |

**Observations:**
1. Pattern destructure уже uses `{}` form for named tuples (D215 record-style).
2. Named tuples conceptually identical к records EXCEPT allocation.
3. Asymmetric parser surface (record multi-line ✅; named tuple ❌) — UX cost.
4. Two syntaxes для one concept (named field access) — cognitive overhead.

---

## 2. Comparative analysis: «value/reference distinction syntax»

### 2.1 Go

```go
type Point struct { X, Y float64 }      // ONE syntax, ONE keyword
var p = Point{1.0, 2.0}                  // stack OR heap — escape analysis decides
var pp = &Point{1.0, 2.0}                // explicit pointer (likely heap)
```

- **One syntax** для structs.
- Allocation decided by escape analysis (compiler) или explicit `&` (pointer to heap).
- **No user-facing keyword** distinguishing value/reference.

### 2.2 Rust

```rust
struct Point { x: f64, y: f64 }          // named record (stack by default)
struct Pair(f64, f64);                    // tuple struct (stack)
struct Unit;                              // unit struct
let p = Box::new(Point { x: 1.0, y: 2.0 }); // explicit heap via Box
```

- **Two syntaxes** (`{}` named, `()` positional) — но **обе stack по default**.
- Heap = `Box<T>` wrapper, не часть declaration.
- `()` form чисто positional access; `{}` чисто named.
- **Difference is access pattern, NOT allocation.**

### 2.3 TypeScript

```typescript
interface Point { x: number; y: number; }      // structural type, no runtime form
class Point { constructor(public x: number, public y: number) {} }
type Point = { x: number; y: number; };        // alias to anonymous object type
```

- **All heap** (JS engine heap).
- No value/reference distinction at language level.
- Three declaration keywords (interface/class/type) — все heap.

### 2.4 Kotlin

```kotlin
data class Point(val x: Double, val y: Double)   // heap, auto eq/hashCode/copy
class Point(val x: Double, val y: Double)         // heap, manual methods
@JvmInline value class Box(val raw: Long)         // single-field inline (stack-like)
```

- **Heap default** (JVM).
- `value class` (Kotlin 1.5+) — **inline value type**, single field, no allocation overhead.
- `data class` — auto-generated equality + copy + destructure.
- Three keywords for three patterns.

### 2.5 Java

```java
record Point(double x, double y) {}              // heap, auto methods, immutable
class Point { ... }                              // heap
value class Point { ... }                        // Project Valhalla (incoming) — stack-like
```

- **Heap default**.
- `record X(args)` — modern shorthand для immutable + auto-equals/hashCode.
- `value class` (Valhalla) — flat value-type, stack/inline allocation.
- Trend: keyword-based allocation discrimination.

### 2.6 Industry pattern summary

| Language | Stack vs Heap distinction | How |
|---|---|---|
| Go | implicit (escape analysis) | compiler decides |
| Rust | explicit Box wrapper | `T` stack / `Box<T>` heap |
| TS | none | all heap |
| Kotlin | keyword | `value class` |
| Java | keyword (incoming) | `record` / `value class` |
| **Nova current** | **syntax `{}` vs `()`** | declaration form |

**Nova is unique — only language using syntax form to encode allocation.**

---

## 3. Critical evaluation: is current Nova design good?

### 3.1 Pros of current `{}` vs `()`

- ✅ Visually distinct — at-a-glance knows allocation contract.
- ✅ No keyword bloat (no extra `value`/`inline` keyword).
- ✅ Encodes semantic difference syntactically (allocation = part of type identity).

### 3.2 Cons of current `{}` vs `()`

1. ❌ **Asymmetric parser surface** — record multi-line ✅, named tuple multi-line ❌.
2. ❌ **Cognitive overhead** — two syntaxes for «type with named fields», differing only in allocation.
3. ❌ **Migration friction** — refactor record ↔ named tuple = full syntax rewrite.
4. ❌ **Pattern asymmetry** — destructure uses `{}` form even для named tuples
   (`Vec3 { x, y, z } = v`), contradicting declaration's `()` form. Documented в Plan 124.4.
5. ❌ **Methods identical between forms** — `fn T @method()` works for both —
   declaration syntax does not affect API.
6. ❌ **Industry deviation** — все эталоны используют keyword, не syntax form,
   для allocation discrimination. Nova outlier.
7. ❌ **`mut` per-field arbitrary restriction** — D215 forbids `mut` для named
   tuples («immutable by design»). But Rust stack structs allow `mut` per-field —
   stack ≠ immutable.

### 3.3 Underlying conceptual issue

D215 conflates **two orthogonal axes**:
1. **Allocation:** stack vs heap.
2. **Mutability:** mutable vs frozen.

`()` form forces both: stack AND immutable. Real-world value types
(Vec3, Matrix, Cursor) могут хотеть **stack AND mutable** (rotate vec
in-place, advance cursor) — currently impossible.

---

## 4. Proposed design: unified `{}` + allocation modifier

### 4.1 Core proposal

**One syntax** для declaration of structured data types: `type X { fields }`.
**Allocation contract** expressed via **type-level modifier**:

```nova
// Heap-allocated (current record default) — unchanged
type Account {
    ro id u64,
    mut balance f64
}

// Stack-allocated value type — NEW unified syntax
type Vec3 value {
    mut x f64,
    mut y f64,
    mut z f64
}

// Composable с другими modifiers
type Secret value priv {
    pub ro id u64,
    key str        // inherits type-level priv (D220 §3.3.1 / D225 для tuples)
}

type Token value consume {
    raw_bytes []u8
}
```

### 4.2 Allocation modifier semantics

`value` keyword после type name (before `{`):
- Declares **value type** — stack-allocated by default.
- Copy-semantics при передаче (D32 amend).
- Auto-promote к heap if escapes (Go-style escape analysis) — covered by
  managed-GC contract (D6).
- Methods `fn T @method()` — same syntax; codegen-receiver = value (not pointer).
- Equality structural (memberwise), aligned с Java records / Kotlin data class.

Default (no modifier) = **reference type / heap** (unchanged D52 §«record»).

### 4.3 Comparison: keyword choice

| Keyword | Precedent | Pro | Con |
|---|---|---|---|
| `value` | Kotlin (1.5+), Java Valhalla | well-known concept | colloquial |
| `inline` | Java Valhalla draft | semantic clarity (inline expansion) | confused with `inline fn` |
| `stack` | none | implementation-specific | leaks via escape analysis |
| `flat` | embedded systems | unambiguous layout | obscure |

**Pick:** `value` — best industry alignment (Kotlin + Java Valhalla), semantically clear.

### 4.4 Migration path для Plan 120 `()` form

**Option A: deprecate immediately.** Plan 120 named tuples → migrate to `type X value { ... }`.
- Pros: clean spec.
- Cons: ~10-20 stdlib + user-code breakage.

**Option B: dual syntax + auto-migration.** `()` form remains as legacy alias.
- Pros: backward compat preserved.
- Cons: parser keeps both forms — original asymmetry remains.

**Option C: edition flip (Plan 124.7 precedent).** Old edition keeps `()`; new edition `value {}`.
- Pros: gradual migration, no flag day.
- Cons: edition infrastructure burden.

**Option D: auto-migrate via `nova migrate`.** Parser still accepts `()` (with multi-line fix), `nova migrate --plan=120-tuple-unification` rewrites to `value {}`.
- Pros: opt-in migration, no edition burden.
- Cons: dual syntax in code-base during transition.

**My pick:** Option D — `nova migrate` + parser-level `()` form preserved with multi-line support (fix M-120-multiline-named-tuple as part of this).

### 4.5 Production-grade requirements

#### 4.5.1 Semantics

- **Allocation contract**: `value` type instances live on stack; **except** when:
  - Captured by closure with longer lifetime → escape, auto-heap.
  - Returned from fn (return-by-value на small types, return-by-pointer на large
    types — ABI-dependent, Plan 57 perf bench infra).
  - Stored в reference container (`[]Vec3` array — array storage strategy decides).
- **Pass semantics**: copy at call site (memcpy of struct).
- **Equality**: memberwise structural (auto-derive Eq for `value` types).
- **Methods**: identical declaration syntax `fn T @method()`; codegen-receiver = `Self` (not `*Self`).
- **Generic `value` types**: `type Stack[T] value { items []T }` works via mono (Plan 59).
- **`consume` composability**: `type Token value consume { ... }` = single-owner value type.
- **`priv` composability**: `type Secret value priv { ... }` = type-level priv flip on value (Plan 124.7 D225 generalizes).

#### 4.5.2 Backward compatibility

- All pre-Plan 125 code compiles unchanged.
- `type X(...)` form supported при наличии `nova migrate` для opt-in transition.
- Existing Plan 120 named tuple fixtures pass identically.
- Plan 124 fixtures pass identically (priv enforcement works on `value` types same as record).

#### 4.5.3 Performance

| Operation | Current `()` | Current `{}` | Proposed `value {}` |
|---|---|---|---|
| Construction | stack alloc | heap alloc + GC track | stack alloc |
| Field access `.x` | inline load | pointer deref + load | inline load |
| Method call `@method` | value receiver | pointer receiver | value receiver |
| Copy across call | memcpy | pointer copy | memcpy |
| Equality | memberwise | identity (pointer) или manual | memberwise auto |

`value {}` form preserves Plan 120 perf characteristics, adds nothing новое.

#### 4.5.4 Diagnostic codes

NEW codes:
- `E_VALUE_TYPE_REQUIRES_BODY` — `type X value` без `{ ... }` body.
- `E_VALUE_TYPE_INCOMPATIBLE_FORM` — `type X value(...)` syntax error
  (positional form not allowed; positional tuples remain bare `()` via D52 §«tuple»).

Existing codes (preserved):
- `E_TUPLE_POSITIONAL_ACCESS_ON_NAMED` — extends к value `{}` forms.
- All Plan 124 priv codes — work identically.
- All Plan 108 D175/D176 codes — work identically.

#### 4.5.5 Documentation

- D52 amend — note `value` modifier addition.
- D215 supersede — value types use unified `{}` form.
- D32 amend — value type pass semantics formalized.
- D123 amend — tuple monomorphization extends к `value {}` forms.
- New D-block (D226 or next free): `type X value { ... }` syntax + semantics.
- `docs/value-types-guide.md` — user guide.
- `docs/migration/120-tuple-unification.md` — migration story.

#### 4.5.6 Tooling

- `nova doc` — render `value` keyword in signature.
- `nova migrate --plan=120-tuple-unification` — auto-rewrite `()` → `value { ... }`.
- LSP — hover shows allocation contract («value type, stack-allocated»).
- nova check — `--explain-allocation` flag — diagnose escape-analysis decisions.

#### 4.5.7 Escape hatches

- `nova check --allow-legacy-tuple` — accept Plan 120 `()` form without warning.
- Edition-based default (V2) — old edition accepts `()`; new edition strict `value {}`.

---

## 5. Verification: production-grade match-or-exceeds эталоны

### 5.1 Capability matrix vs Go/Rust/TS/Kotlin/Java/Swift/C#

| Capability | Go | Rust | TS | Kotlin | Java | Swift | C# | **Nova proposed** |
|---|---|---|---|---|---|---|---|---|
| Uniform declaration syntax | ✅ `struct` | ⚠️ split `{}`/`()` | ✅ `interface`/`class` | ⚠️ split data/value | ⚠️ split record/class/value | ⚠️ split struct/class | ⚠️ split struct/class | ✅ **`type` unified** |
| Explicit allocation control | ⚠️ via `*T` | ✅ `Box<T>` | ❌ all heap | ✅ `value class` | ⚠️ Valhalla incoming | ✅ `struct` vs `class` | ✅ `struct` vs `class` | ✅ **`value` modifier** |
| Stack value mut per-field | ✅ | ✅ | n/a | ❌ (immutable) | ❌ (Valhalla immutable) | ✅ | ✅ | ✅ **fix D215 gap** |
| Heap reference type | ⚠️ `&T`/`*T` | ✅ `Box`/`Rc` | ✅ default | ✅ class default | ✅ class default | ✅ class | ✅ class | ✅ **record default** |
| Auto-equals для value | ❌ | ✅ derive | n/a | ✅ data class | ✅ record | ❌ | ❌ (manual) | ✅ **memberwise auto** |
| Copy semantics value type | ✅ | ✅ | n/a | ⚠️ (data class manual `.copy`) | ⚠️ | ✅ | ✅ | ✅ **automatic** |
| Methods on value types | ✅ pointer/value | ✅ trait impl | n/a | ✅ | ✅ | ✅ | ✅ | ✅ **fn T @method()** |
| Generic value types | ✅ (Go 1.18+) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ **Plan 59 mono** |
| Pattern destructure | ⚠️ (limited) | ✅ both forms | ⚠️ via JS | ✅ destructuring decl | ✅ records | ✅ | ✅ | ✅ **`{}` form unified** |
| Migration tooling | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✅ **`nova migrate`** |

**Result:** Nova proposed matches-or-exceeds на 10/10 capabilities + 1 Nova-only superior (migration tooling).

### 5.2 Nova-specific superior axes

1. **Most consistent declaration syntax** — single keyword `type` для всех data forms.
   Kotlin/Java split data/value/record; Rust splits `{}`/`()`; Swift/C# split struct/class.
   Nova `type X { ... }` + `value` modifier = cleanest.
2. **Effect system integration** — value/reference distinction visible в method
   signatures (Plan 03.4 D140 effect-surface).
3. **Allocation transparency** — no hidden boxing, no implicit conversions.
   User declares contract; compiler honors.

### 5.3 Outstanding criticism / honest acknowledgement

- **Migration cost real**: ~15-30 occurrences `()` form в stdlib + bench + tests.
  Plan 120 D215 fixtures (~10 files) need rewrite. Mitigated by `nova migrate`.
- **`value` keyword reserved**: must check collision (probably new — Plan 124 added
  `priv`/`pub`, no `value`).
- **Performance regression risk**: escape analysis must be reliable; pessimistic
  decisions promote to heap unnecessarily. Plan 57 bench gates required.
- **Mixed-form codebase during transition**: parser supports both `()` and
  `value {}` — confusing для new users. Mitigated by `nova migrate` + edition tie (V2).

---

## 6. Decision

### 6.1 Is the idea worthwhile?

**Yes** — на 4 axes:

1. **UX symmetry** — closes parser asymmetry (multi-line works для both).
2. **Industry alignment** — moves Nova к Kotlin/Java Valhalla pattern (keyword-based
   allocation).
3. **Cognitive simplification** — one declaration syntax, one mental model.
4. **Mutability liberation** — `value {}` form supports `mut` per-field, fixing
   D215 «all immutable» restriction.

### 6.2 Refinements over user's original proposal

User's original: «запись тупла идентичной рекорду, отличие только в том, что
рекорд создается в памяти, а тупл на стеке».

**Refinements I add:**

1. **Use modifier `value`** (Kotlin/Java precedent), not «implicit by name».
   User's wording could mean «syntax same, but parser/compiler chooses» — that's
   Go-style escape analysis. Nova should be explicit (keyword) для clarity.
2. **Liberate `mut`** для value types (fix D215 «all immutable» artifact).
3. **Auto-equals memberwise** для value types (Java record / Kotlin data class
   precedent).
4. **Preserve old `()` form** with parser fix для multi-line (closes the original
   asymmetry that triggered this research).
5. **Migration tooling** — Nova edge: `nova migrate` automation.
6. **Edition pin** — backward compat preserved even if `()` form retracted in future.

### 6.3 Composability matrix (Nova's hallmark)

| Modifier | Compose с `value`? | Notes |
|---|---|---|
| `priv` (Plan 124) | ✅ | `type Secret value priv { ... }` — Plan 124.7 D225 generalizes |
| `consume` (Plan 100) | ✅ | `type Token value consume { ... }` — single-owner stack value |
| `mut` per-field (D175) | ✅ | fixes D215 limitation |
| `ro` per-field (D176) | ✅ | per-field readonly, mixed |
| `pub` per-field (Plan 124) | ✅ | overrides type-level priv |
| `#test_access` (Plan 124.6) | ✅ | works identically на value types |
| `#visible_to` (Plan 124.6) | ✅ | works identically |
| `external` (D82) | ⚠️ | external value types — needs ABI study (V2) |
| `consume` parameter (D131) | ✅ | `fn f(t value consume Token)` |

**Composability check passes.**

---

## 7. Implementation estimate

### 7.1 Phases

- **Ф.0 — Investigation.** Confirm `value` keyword unreserved; audit `()` form
  usage corpus-wide; identify migration scope. ~0.5 dev-day.
- **Ф.1 — Parser.** Accept `type X value { ... }` form. ~0.5 dev-day.
- **Ф.2 — AST.** `TypeDecl.allocation: AllocKind { Heap, Value }` enum. ~0.3 day.
- **Ф.3 — Type-checker.** Value type pass semantics, copy enforcement, auto-eq
  derivation. ~1 day.
- **Ф.4 — Codegen.** Value type C-struct emission as inline (not pointer); method
  receivers как value. ~1 day.
- **Ф.5 — Tests.** Positive (10+) + negative (5+) для value types; cross-fixture
  matrix. ~0.5 day.
- **Ф.6 — Spec.** D-block NEW + D52/D215/D32/D123 amends. ~0.3 day.
- **Ф.7 — Migration.** `nova migrate --plan=120-tuple-unification` tool. ~0.5 day.
- **Ф.8 — Doc.** `docs/value-types-guide.md` + migration guide. ~0.3 day.
- **Ф.9 — Multi-line `()` fix** (orthogonal but bundled). ~0.2 day.

**Total: ~5-6 dev-day** для V1 (parser + checker + codegen + tests + spec + migration).

### 7.2 Risks

- **R1: Migration breakage**. Mitigation: `()` form preserved + auto-migrate tool.
- **R2: Escape analysis pessimism**. Mitigation: Plan 57 bench gates; user can
  force heap via `Box[T]` wrapper (if needed — Plan 03.x consider).
- **R3: `value` keyword collision**. Mitigation: grep audit (probably free).
- **R4: ABI breakage для external types**. Mitigation: value `external type`
  forbidden in V1; reconsider V2.
- **R5: Differential codegen complexity** — value vs reference paths fork в
  codegen. Mitigation: shared code paths где possible, type metadata табле.

---

## 8. Alternative considered: NO change (status quo)

Honest counterargument для recommendation against:

- Current `()` vs `{}` works для majority cases (Plan 120 shipped).
- Multi-line tuple form needed by ~0% of real code (asymmetry rarely hits users).
- Adding `value` keyword adds cognitive load (not removes — users now learn
  both syntaxes during transition).
- Production-grade migration tooling is expensive infra (`nova migrate` framework
  needs investment).

**Counterargument response:**
- Plan 124 added 2 modifiers (`priv`/`pub`); adding `value` is consistent with
  Nova's modifier-based approach.
- Multi-line tuple form is symptom of deeper conceptual issue (allocation as
  syntax form vs modifier). Fix multi-line alone is partial.
- Cognitive load lowered, not raised: ONE syntax `{}` for all named-field types;
  ONE modifier `value` for allocation. Current state has TWO syntaxes для one
  concept.
- `nova migrate` framework needed для future plans anyway (Plan 73.1 D180 / Plan
  108.1 D176 already used edition+migration tooling). Investment amortizes.

---

## 9. Open questions

- **Q1**: Should `value` types support inheritance/embed (`use name Type`)? D39
  embed semantics — пока no constraint check для value types.
- **Q2**: Default field mutability на value types — `mut` или `ro`? Records default
  to `mut` (post Plan 108). Value types should be **same** (consistency) — answer
  `mut` default.
- **Q3**: Pattern matching syntax — keep `T { fields }` для both, или introduce
  `T(fields)` для value type destructure? Pick: **keep `{}` form** для both (Plan
  124.4 precedent).
- **Q4**: Allocation modifier position — `type X value { ... }` или
  `value type X { ... }`? Pick: **after name** (Plan 124.7 D225 precedent для
  type-level modifiers).
- **Q5**: Edition tie required? V1 backward-compat preserves `()` — no edition
  needed. V2 may retract `()` via edition flip.

---

## 10. Recommendation

**Verdict: WORTHWHILE — implement as Plan 125 (или Plan 120 amend V2).**

### Bundle:

1. **D226 NEW** — `value` type modifier — unified `{}` syntax + allocation contract.
2. **D52 amend** — `type X value { ... }` recognized as 7th form.
3. **D215 supersede** — Plan 120 named tuples migrate к value record form via
   `nova migrate`; legacy `()` form preserved + multi-line fix.
4. **D32 amend** — value type pass semantics (copy on call).
5. **D123 amend** — tuple mono extends к value records.
6. **`value` keyword reserved** — add к lexer.
7. **`nova migrate --plan=125-value-types`** — opt-in migration tool.
8. **Plan 57 bench gates** — escape analysis perf regression detection.

**Estimated cost:** ~5-6 dev-day (single autonomous session feasible).

**Estimated win:**
- UX: unified mental model (1 syntax + 1 modifier).
- Industry alignment: matches Kotlin/Java Valhalla pattern.
- Liberates value-type mutability (real production win — vector/matrix in-place
  ops).
- Closes parser asymmetry (multi-line works для both).
- Strengthens Nova claim «production-grade match-or-exceeds эталоны».

**Recommended next step:** spawn Plan 125 (или Plan 120.V2) sub-plan doc with
this research as §1 context; proceed implementation если user OK.

---

## 11. Concrete worked example (full spec)

```nova
// === Reference types (heap, GC-managed) — current record, unchanged ===
type Account {
    ro id u64
    mut balance f64
    priv mut last_modified Instant
}

// === Value types (stack-allocated, NEW unified syntax) ===
type Vec3 value {
    mut x f64        // mutable, copy-on-pass
    mut y f64
    mut z f64
}

fn Vec3 @rotate(angle f64) -> Vec3 =>
    Vec3 {
        x: @x * cos(angle) - @y * sin(angle),
        y: @x * sin(angle) + @y * cos(angle),
        z: @z
    }

// In-place mutation (fixes D215 limitation):
fn Vec3 mut @normalize() {
    ro len = sqrt(@x * @x + @y * @y + @z * @z)
    @x = @x / len    // works on value type — copy semantics still apply
    @y = @y / len
    @z = @z / len
}

// Auto-derive equality (memberwise, structural)
test "value type equality" {
    ro a = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
    ro b = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
    assert(a == b)  // structural eq via auto-derive
}

// === Composability ===

// Value + priv (Plan 124.7 D225 generalized)
type Secret value priv {
    pub ro id u64
    key str       // inherits type-level priv
    mut salt []u8
}

// Value + consume (Plan 100 D131)
type Token value consume {
    raw []u8
}

// Value + generics (Plan 59 mono)
type Box[T] value {
    item T
    count int
}

// === Pattern destructure unified ===
ro v = Vec3 { x: 1.0, y: 2.0, z: 3.0 }
ro Vec3 { x, y, z } = v    // works для value types (Plan 124.4 precedent)

// === Legacy form preserved (backward compat) ===
type LegacyVec3(x f64, y f64, z f64)    // deprecated, auto-migrate via `nova migrate`
//                                       // equivalent to:
type LegacyVec3 value { x f64, y f64, z f64 }
```

---

## 12. See also

- D52 — type declaration forms (current).
- D215 — Plan 120 named tuples (target of unification).
- D32 — parameter pass semantics (will amend).
- D123 — tuple monomorphization (will amend).
- D175/D176 — readonly field/T modifier (Plan 108) — composable.
- D220-D225 — Plan 124 priv visibility — composable.
- D131 — consume types (Plan 100) — composable.
- Kotlin `value class`: https://kotlinlang.org/docs/inline-classes.html
- Java Project Valhalla: https://openjdk.org/projects/valhalla/
- Rust struct vs tuple struct: https://doc.rust-lang.org/book/ch05-01-defining-structs.html
