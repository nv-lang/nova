// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 120 — Named tuple fields + value/reference allocation contract

> **Создан 2026-05-31.**
> **Статус:** 🆕 PLANNED.
> **Приоритет:** P1 — **language ergonomics improvement** для value-types
>   (hot-path code, geometric types, FFI multi-value returns); explicit
>   documentation of stack/heap allocation semantics (currently scattered
>   и implicit в Spec).
> **Оценка:** ~½-1 dev-day (mechanical parser/checker; codegen unchanged
>   from positional tuples; bulk доли — spec amends + docs).
> **Зависимости:**
>   - D52 ✅ (revised type declarations — extends tuple form)
>   - D32 ✅ (default immutable, by-value vs by-reference semantics — amend)
>   - D123 ✅ (tuple monomorphization — amend для named field access)
> **D-блоки:** **новый D215** (named tuple fields); амендменты **D32**
>   (explicit value/reference allocation contract), **D52** (extended tuple
>   syntax), **D123** (named field codegen + access).
> **Worktree convention:** `nova-p120`.
>
> **Recommended model:**
>   - **Sonnet 4.6 HIGH + Thinking ON** — план mechanical (parser/checker
>     extension), precedent Plan 91.8a/b/c (D-block amends + small language
>     additions). Safe для Sonnet 4.6.
>   - **Opus 4.7** допустим, но overkill для размера.
>
> **Workflow требования (для агента):** идентично Plan 114/91.12/108.4/116/115
>   — commit per phase, update logs, tests через release nova, status section
>   в конце plan-файла, без упрощений.
>
> **Production-grade требование:** реализация без упрощений. Named tuple
>   parser + type-checker enforcement + codegen named field access; spec
>   D-blocks finalized; documentation guide stack-vs-heap explicit.

---

## Зачем

### Проблема 1: Plan 59 Ф.7.4 rejection основан на incomplete understanding

[02-types.md:3699-3704](spec/decisions/02-types.md#L3699-L3704):
> **Named tuple fields** (`(x: T1, y: T2)`) — **ОТКЛОНЕНО окончательно (Plan
> 59 Ф.7.4, 2026-05-21).** Именованные поля кортежа **почти идентичны
> record'у**; заводить два почти одинаковых синтаксиса для одной семантики
> в Nova нет причин.

**Reasoning flaw:** «почти идентичны record'у» — **неверно**. Tuple и
record имеют **fundamentally different allocation semantics**:

| | Tuple (`type X(...)`) | Record (`type X { ... }`) |
|---|---|---|
| Allocation | **Stack** (D123: "no heap alloc для tuple value") | **Managed heap** (D32: "Объекты — managed reference") |
| Field access | `.0` / `.1` positional | `.name` named |
| Semantics | Value type (copy on pass) | Reference type (by-ref) |
| GC tracking | None | Yes |
| Lifetime | Scope-bound (deterministic destruction) | GC-collected |
| Sharing | Copy required для cross-fiber | Share by reference |

Plan 59 rejection assumed **tuple ≡ record semantically**, что **неверно**
per существующих D32 + D123 spec text. Different allocation = different
performance + lifetime characteristics → different syntactic forms
**justified**.

### Проблема 2: Stack/heap distinction нигде explicit в spec

Сейчас:
- D32 (`02-types.md:2386-2388`): «Объекты (record / sum-type / массивы) —
  managed reference. Указатель в managed heap, отслеживаемый GC.»
- D123 (`02-types.md:3686-3690`): «Zero-cost — no heap alloc для tuple value.»

**Нигде явно не сравнивается:** tuple vs record по allocation. Reader
должен deduce разницу из текста разных D-блоков. **AI-unfriendly,
beginner-hostile.**

### Решение Plan 120

1. **Reopen named tuples**: `type Point(x f64, y f64)` — stack-allocated
   value type с named field access (`.x` / `.y`). Withdraws Plan 59 Ф.7.4
   rejection с corrected reasoning.

2. **Document stack/heap distinction explicitly**:
   - **Bracket type encodes allocation:**
     - `type X(...)` — **stack** (value type, positional или named fields)
     - `type X { ... }` — **heap** (reference type, always named fields,
       GC-managed)
   - Update D32, D52, D123 с explicit allocation contract
   - New D215 для named tuple syntax + canonical pattern doc

3. **Use cases для named tuples** (hot-path, math types, FFI returns):
   - `type Vec3(x f64, y f64, z f64)` — graphics/physics
   - `type Color(r u8, g u8, b u8, a u8)` — pixel formats
   - `type SocketReadResult(data []u8, addr SocketAddr, err i64)` — FFI
     multi-value return (cleaner than positional `([]u8, SocketAddr, i64)`)
   - `type IterState(idx int, end int, step int)` — iterator state без GC pressure

---

## Дизайн

### Named tuple syntax (extend D52 tuple form)

```nova
// Existing (positional, Plan 59):
type Point(f64, f64)              // .0 / .1 access
type Pair[A, B](A, B)

// NEW (named, Plan 120):
type Point(x f64, y f64)          // .x / .y access
type Vec3(x f64, y f64, z f64)
type Color(r u8, g u8, b u8, a u8)
type Generic[T](value T, count int)
type SocketReadResult(data []u8, addr SocketAddr, err i64)

// Mixed positional+named — НЕ allowed (один стиль на tuple):
type Bad(x f64, f64)              // ❌ E_TUPLE_MIXED_FIELDS
                                  //    «tuple fields must be all named or all positional»
```

### Construction syntax

```nova
// Positional construction (always works для positional tuples):
let p = Point(1.0, 2.0)            // works for type Point(f64, f64) ИЛИ type Point(x f64, y f64)

// Named construction (only для named tuples):
let p = Point(x: 1.0, y: 2.0)      // works ONLY для type Point(x f64, y f64)
let v = Vec3(x: 0, y: 1, z: 2)
let c = Color(r: 255, g: 0, b: 128, a: 255)

// Mix не allowed:
let bad = Point(x: 1.0, 2.0)       // ❌ E_TUPLE_CONSTRUCT_MIXED
```

### Field access

```nova
let p Point = Point(x: 1.0, y: 2.0)

p.x                                 // ✅ named access (named tuple only)
p.y                                 // ✅
p.0                                 // ⚠ also works? — see below

// Positional access на named tuple — открытый вопрос Q120-positional-access-on-named:
//   Option A: allow .0 / .1 as fallback (Rust-style positional access на ANY tuple)
//   Option B: forbid — named tuples named access only
// Default proposal: Option B (forbid) — explicit intent, single API surface
```

### Allocation contract — bracket encodes semantics

```nova
// () = stack-allocated value type
type Point(x f64, y f64)                    // stack
type Vec3(x f64, y f64, z f64)              // stack

// {} = heap-allocated reference type (GC-managed)
type User { id u64, name str }              // heap
type Order { items []Item, total money }    // heap

// Visual rule:
//   parens   → value type (stack, copy semantics)
//   braces   → reference type (heap, share semantics)
//
// Inside parens: positional OR named fields
// Inside braces: always named fields (no positional records — confusing)
```

### Когда use named tuple vs record

| Use case | Choice | Why |
|---|---|---|
| Hot-path math (Vec3, Matrix, Quaternion) | named tuple `type Vec3(x f64, ...)` | zero GC pressure, predictable performance |
| Pixel formats (Color, Pixel) | named tuple | small, copy-cheap |
| FFI multi-value returns | named tuple | stack-allocated return, fits в registers |
| Iterator state | named tuple | local-lifetime, no heap |
| Geometric primitives (Point, Rect, Circle) | named tuple | math semantics |
| Domain entities (User, Order, Account) | record | identity, sharing, mutations across fibers |
| Aggregates с many fields | record | heap manageable, copy expensive |
| Types passed между modules / persisted | record | reference semantics, cleaner |

**General rule:** small + copy-cheap + value-semantics → tuple; large /
identity-bearing / shared → record.

### Type parameters works для named tuples

```nova
type Pair[A, B](first A, second B)                  // generic named tuple

let p = Pair(first: 1, second: "hello")             // Pair[int, str]
p.first                                              // 1
p.second                                             // "hello"
```

### Method declarations работают идентично records

```nova
type Vec3(x f64, y f64, z f64)

fn Vec3 @magnitude() -> f64 =>
    sqrt(@.x * @.x + @.y * @.y + @.z * @.z)

fn Vec3 @add(other Vec3) -> Vec3 =>
    Vec3(x: @.x + other.x, y: @.y + other.y, z: @.z + other.z)

// Usage:
let v1 = Vec3(x: 1, y: 2, z: 3)
let v2 = Vec3(x: 4, y: 5, z: 6)
let sum = v1.add(v2)                                 // sum on stack, zero alloc
let m = sum.magnitude()
```

### Сравнение с mainstream

| Язык | Stack-value type with named fields |
|---|---|
| Rust | `struct Point { x: f64, y: f64 }` — stack by default; heap via `Box<T>` |
| Swift | `struct Point { let x: Double; let y: Double }` — value type |
| C# | `struct Point { public double X; public double Y; }` — value type |
| Go | `type Point struct { X, Y float64 }` — escape analysis determines stack/heap |
| Kotlin | `inline class Wrapper(val inner: Int)` — value type для one-field; full struct = `data class` (heap) |
| TS | `type Point = { x: number; y: number }` — heap object |
| **Nova V2 (Plan 120)** | `type Point(x f64, y f64)` — stack-allocated value type, **explicit syntax** | ✅ |

Nova V2: bracket choice (`()` vs `{}`) **explicitly encodes** stack/heap
allocation. Better than Swift/C# (no obvious syntax difference между
`struct` value-type и hypothetical reference-`class` analog) — Nova
syntax-level clarity.

---

## Грамматика

Existing tuple grammar (D52):
```ebnf
tuple_decl  ::= "type" IDENT type_params? "(" type_list ")"
type_list   ::= type ("," type)*
```

Extended grammar (Plan 120 D215):
```ebnf
tuple_decl    ::= "type" IDENT type_params? "(" tuple_fields ")"
tuple_fields  ::= positional_list | named_list
positional_list ::= type ("," type)*
named_list      ::= named_field ("," named_field)*
named_field     ::= IDENT type

// Constructor (existing tuple call expression — adds named arg support):
tuple_expr    ::= IDENT type_args? "(" arg_list ")"
arg_list      ::= positional_args | named_args
positional_args ::= expr ("," expr)*
named_args      ::= named_arg ("," named_arg)*
named_arg       ::= IDENT ":" expr

// Field access (existing — extends to named on named tuples):
field_access ::= expr "." (INT | IDENT)
//                            ↑      ↑
//                            .0     .x  (named tuple: only IDENT; positional: only INT)
```

**Parser disambiguation для tuple_fields:**
- See `(IDENT type, ...)` → named tuple
- See `(type, ...)` или `(IDENT, IDENT, ...)` if these IDENTs are type names → positional tuple

Edge case: `(x int, y int)` — could be misread. But `int` is reserved type
keyword, `x` / `y` are identifiers — clear: `x` is field name, `int` is
type. Positional tuple `(int, int)` has no IDENT prefix.

---

## Фазы

### Ф.0 — GATE: D215 draft + audit (~⅛ dev-day)

- **Ф.0.1** Draft D215 «Named tuple fields + value/reference allocation
  contract» в `spec/decisions/02-types.md` (рядом с D52/D123).
- **Ф.0.2** Audit existing tuples в codebase (`std/`, `nova_tests/`) —
  кандидаты для migration to named (if any benefit).
- **Ф.0.3** Worktree `nova-p120` create.
- **Ф.0.4** Acceptance A1-A10 финализированы.

### Ф.1 — Parser: named tuple syntax (~¼ dev-day)

- **Ф.1.1** `compiler-codegen/src/parser/mod.rs` — extend `parse_tuple_decl`
  для named fields:
  - Detect `IDENT type` pattern → named
  - Detect `type` pattern → positional
  - Mixed → `E_TUPLE_MIXED_FIELDS` parse error
- **Ф.1.2** Constructor expression — extend `parse_call_expr` для named
  arguments:
  - `Vec3(x: 1, y: 2, z: 3)` — named args
  - `Vec3(1, 2, 3)` — positional (works для both positional и named tuples)
  - Mixed in single call → `E_TUPLE_CONSTRUCT_MIXED`
- **Ф.1.3** AST extension: `TupleDecl { name, type_params, fields:
  TupleFields::Positional(Vec<Type>) | TupleFields::Named(Vec<NamedField>) }`.
- **Ф.1.4** Tests T1 series.

### Ф.2 — Type-checker: named field access + construction (~¼ dev-day)

- **Ф.2.1** Field access resolution:
  - `.IDENT` on named tuple → check field name exists; resolve to typed
    field
  - `.INT` on positional tuple → existing path (D123)
  - `.IDENT` on positional tuple → `E_TUPLE_NAMED_ACCESS_ON_POSITIONAL`
  - `.INT` on named tuple → `E_TUPLE_POSITIONAL_ACCESS_ON_NAMED` (per
    Q120-positional-access-on-named decision: Option B — forbid)
- **Ф.2.2** Constructor type-checking:
  - Named tuple + named args → match field names, check types per-field
  - Named tuple + positional args → check arity + per-position types
  - Positional tuple + named args → `E_TUPLE_CONSTRUCT_NAMED_ON_POSITIONAL`
- **Ф.2.3** Tests T2 series.

### Ф.3 — Codegen: identical to positional tuples (~⅛ dev-day)

- **Ф.3.1** **NO codegen changes** для named tuples — they have identical
  C-side representation as positional tuples:
  - Both → stack-allocated C struct `NovaTuple_Vec3 { double x; double y;
    double z; }` (для named) vs `NovaTuple_f64_f64_f64 { double _0; double
    _1; double _2; }` (для positional).
  - Field access codegen: `.x` → `c_struct.x`; `.0` → `c_struct._0` (or
    similar). Same SSA pattern.
- **Ф.3.2** Mangling: named tuple monomorphization включает field names
  в symbol (`Vec3_x_f64_y_f64_z_f64` или similar) для disambiguation от
  positional `Vec3_f64_f64_f64`.
- **Ф.3.3** Tests T3 series (codegen smoke).

### Ф.4 — D-block spec amends (~⅛ dev-day)

- **Ф.4.1** **D52 amend** — extend tuple grammar table с named option;
  add «Allocation contract» note: `()` = value type stack, `{}` =
  reference type heap.
- **Ф.4.2** **D32 amend** — explicit value vs reference type taxonomy:
  - Primitives (int, bool, f64, char, u8, ()) — value, register
  - Tuples (positional or named) — value, **stack**
  - Records — reference, **managed heap**
  - Sum types — reference, managed heap
  - Arrays — reference, managed heap
- **Ф.4.3** **D123 amend** — clarify named field access codegen identical
  to positional; mangling differentiates.
- **Ф.4.4** **D215 (NEW)** — Named tuple fields + canonical use cases
  pattern doc.
- **Ф.4.5** **Withdraw Plan 59 Ф.7.4 rejection** — add «Reopened by Plan
  120 (2026-05-31): reasoning of original rejection was incomplete —
  see D215».

### Ф.5 — Stdlib opportunities + docs + close (~¼ dev-day)

- **Ф.5.1** Audit stdlib для named-tuple candidates:
  - `Range` / `RangeIter` — already records; не migrate (identity semantics)
  - Geometric (if exists in std) — migrate to named tuples
  - FFI result types (Plan 91.12, Plan 115) — adopt named tuples для FFI
    returns ([]u8, addr, err) → SocketReadResult(data, addr, err)
- **Ф.5.2** New examples в `examples/`:
  - `examples/value_types/` — Vec3, Color, Point use cases
  - Performance comparison: named tuple vs record (allocation profile)
- **Ф.5.3** Update `docs/types-guide.md` (если есть, create если нет) с
  value/reference type guide.
- **Ф.5.4** `docs/project-creation.txt` — sprint section.
- **Ф.5.5** `docs/simplifications.md` — close `[M-120-*]`.
- **Ф.5.6** `nova-private/discussion-log.md` — design decisions.
- **Ф.5.7** Memory `project-plan120-status.md`.
- **Ф.5.8** Status closure summary.
- **Ф.5.9** Full `nova test` ≥ baseline; cross-platform CI.
- **Ф.5.10** Final merge.

---

## D-block changes

### D215 (NEW) — Named tuple fields + canonical use cases

**Локация:** `spec/decisions/02-types.md` (после D123).

**Что.** Extension D52 tuple form: fields могут быть named (parallel с
positional). Construction via positional args (always) или named args
(named tuples only). Field access via name (named) или position (positional).

**Allocation contract (cross-ref D32):** все tuple forms (positional или
named) — stack-allocated value types. Identical к existing D123 для
positional.

**Use cases (recommended patterns):**
- Hot-path value types: Vec3/Color/Point (math/graphics)
- FFI multi-value returns (Plan 115): `(data, addr, err)` — readable
- Iterator state without GC pressure
- Pixel/sample formats (audio, video)
- Geometric primitives

**Anti-patterns** (use record instead):
- Domain entities (User, Order, Account) — identity matters, sharing needed
- Large aggregates — copy expensive
- Types persisted/serialized cross-process — reference semantics natural

**Plan 59 Ф.7.4 reopened:** D215 supersedes the rejection (corrected
reasoning per D32+D123 stack/heap distinction).

### D52 amend — extend tuple syntax, document allocation contract

**Локация:** `spec/decisions/02-types.md`.

Old: tuple grammar `type X(types)` positional only.
New: tuple grammar `type X(fields)` where `fields` = positional list OR
named list (но не mixed).

**Add «Allocation contract» note:**
> **Bracket choice encodes allocation semantics:**
> - `type X(...)` — **stack-allocated** value type. Copy semantics on
>   pass/assign. No GC tracking. Scope-bound lifetime.
> - `type X { ... }` — **heap-allocated** reference type. Reference
>   semantics on pass/assign. GC-managed. Live until no references.
>
> Fields внутри `(...)` могут быть positional `(T1, T2)` или named
> `(name1 T1, name2 T2)`. Fields внутри `{...}` always named.

### D32 amend — explicit value vs reference type taxonomy

**Локация:** `spec/decisions/02-types.md:2340+`.

**Add explicit taxonomy:**

> **Value types** (by-value, stack-allocated, no GC tracking):
> - Primitives: `int`, `bool`, `f64`, `char`, `u8`, `()` (unit)
> - Tuples: `type X(...)` — positional или named fields
> - **Reasoning:** scope-bound lifetime, predictable performance, copy
>   semantics
>
> **Reference types** (by-reference, heap-allocated, GC-tracked):
> - Records: `type X { ... }`
> - Sum types: `type X | A | B`
> - Arrays: `[]T`, `[N]T`
> - Strings: `str` (immutable, but heap-backed для shared content)
> - **Reasoning:** identity semantics, sharing across fibers/scopes,
>   dynamic lifetime
>
> Function parameter passing:
> - Value type param — by-value (copy в caller's stack frame)
> - Reference type param — by-reference (pointer to heap)
> - `mut` qualifier — caller sees mutations (works для both value &
>   reference, semantics differ)

### D123 amend — named field codegen

**Локация:** `spec/decisions/02-types.md:3570+`.

Add note: named tuple fields use named C struct fields (`struct Vec3 {
double x; double y; double z; }`) instead of positional (`struct
Vec3_f64_f64_f64 { double _0; double _1; double _2; }`). Mangling
differentiates: named tuple symbol includes field names.

---

## Tests

### T1 — Parser positive

- **T1.1** `type Point(x f64, y f64)` — parses; AST = TupleDecl with
  Named fields.
- **T1.2** `type Vec3(x f64, y f64, z f64)` — parses.
- **T1.3** `type Color(r u8, g u8, b u8, a u8)` — parses.
- **T1.4** `type Generic[T](value T, count int)` — parses with generic
  params.
- **T1.5** `type Point(f64, f64)` (positional) — still parses (back-compat).

### T2 — Parser negative

- **NEG-T2.1** `type Bad(x f64, f64)` — mixed fields → `E_TUPLE_MIXED_FIELDS`.
- **NEG-T2.2** `type Bad(f64, x f64)` — mixed (positional then named) → same error.

### T3 — Construction positive

- **T3.1** `Point(x: 1.0, y: 2.0)` — named args.
- **T3.2** `Point(1.0, 2.0)` (positional args на named tuple) — works.
- **T3.3** `Vec3(x: 0, y: 1, z: 2)` — named args.
- **T3.4** Generic: `Pair(first: 1, second: "hello")` — works, infers `Pair[int, str]`.

### T4 — Construction negative

- **NEG-T4.1** `Point(x: 1.0, 2.0)` (mixed) → `E_TUPLE_CONSTRUCT_MIXED`.
- **NEG-T4.2** Positional tuple `Point(f64, f64)` constructed с named args
  → `E_TUPLE_CONSTRUCT_NAMED_ON_POSITIONAL`.
- **NEG-T4.3** Named tuple `Vec3(x f64, y f64, z f64)` constructed
  с wrong arity → `E_TUPLE_CONSTRUCT_ARITY_MISMATCH`.
- **NEG-T4.4** Wrong field name `Point(x: 1.0, wrong: 2.0)` →
  `E_TUPLE_UNKNOWN_FIELD`.

### T5 — Field access positive

- **T5.1** `let p = Point(x: 1.0, y: 2.0); p.x` — returns 1.0.
- **T5.2** `let p = Point(x: 1.0, y: 2.0); p.y` — returns 2.0.

### T6 — Field access negative

- **NEG-T6.1** Positional tuple field-named access:
  `let p = Point(1.0, 2.0); p.x` → `E_TUPLE_NAMED_ACCESS_ON_POSITIONAL`.
- **NEG-T6.2** Named tuple positional access:
  `let v = Vec3(x: 0, y: 1, z: 2); v.0` →
  `E_TUPLE_POSITIONAL_ACCESS_ON_NAMED` (per Q120-positional-access-on-named
  Option B).

### T7 — Codegen smoke

- **T7.1** Named tuple → C struct с named fields (verified в emitted C).
- **T7.2** Sizeof identical to positional tuple equivalent.
- **T7.3** Performance bench: named tuple Vec3.add() vs record Vec3.add()
  — named tuple zero allocations (verify через `nova bench --gc-profile`).

### T8 — Method declarations

- **T8.1** `fn Vec3 @magnitude() -> f64 => sqrt(@.x*@.x + @.y*@.y + @.z*@.z)`
  — works on named tuple.
- **T8.2** `fn Vec3 @add(other Vec3) -> Vec3 => Vec3(x: @.x + other.x, ...)`
  — fluent method.

### T9 — Stdlib examples

- **T9.1** `Vec3(x: 1, y: 2, z: 3).magnitude()` — works.
- **T9.2** `Color(r: 255, g: 0, b: 128, a: 255).to_hex()` — works
  (hypothetical method).

### Regression

- **R1** Existing positional tuples — full `nova test` ≥ baseline.
- **R2** Cross-platform CI.

---

## Acceptance criteria

| # | Критерий | Verification |
|---|---|---|
| A1 | Parser accepts named tuple decl `type X(name1 T1, name2 T2)` | T1 series |
| A2 | Parser rejects mixed fields → `E_TUPLE_MIXED_FIELDS` | NEG-T2 |
| A3 | Construction with named args `X(name: val, ...)` works | T3 |
| A4 | Construction with positional args на named tuple works (back-compat) | T3.2 |
| A5 | Construction errors (mixed/wrong-field/wrong-arity) | NEG-T4 series |
| A6 | Field access by name `.name` works на named tuple | T5 |
| A7 | Cross-access errors (`.x` на positional / `.0` на named) | NEG-T6 series |
| A8 | Codegen identical performance к positional (named tuple = zero-cost) | T7.3 bench |
| A9 | D-blocks D52/D32/D123 amended; D215 NEW promoted в active | spec diff |
| A10 | Plan 59 Ф.7.4 rejection withdrawn с corrected reasoning | spec text |
| A11 | Full `nova test` ≥ baseline; cross-platform PASS | R1 + R2 |
| A12 | Docs (`docs/types-guide.md` или equivalent) explicitly document stack/heap distinction | manual review |

---

## Risk register

| # | Риск | Митигация |
|---|---|---|
| R-1 | Parser ambiguity: `type X(x int, y int)` vs `type X(int, int)` | Disambiguation via lookahead — first token after `(` is IDENT not type-name (`x` not `int`) → named. Type names (`int`, `str`, etc) reserved — never field names. Safe |
| R-2 | Migration: existing tuple uses still positional — should we auto-suggest named? | НЕ migrate automatically; positional tuples остаются valid. User choice |
| R-3 | Confusion: when use named tuple vs record? | Document guidelines в D215 + docs/types-guide.md; concrete use case examples |
| R-4 | Positional access на named tuple — Option A vs B (allow .0 fallback или forbid) | V1 = Option B (forbid) per design rationale: single API surface. If practice shows need — followup `[M-120-positional-fallback]` |
| R-5 | Codegen mangling collision: `type Vec3(f64, f64, f64)` vs `type Vec3(x f64, y f64, z f64)` | Mangling includes field names → distinct C symbols; safe |

---

## Out of scope (followups)

| Маркер | Что | Когда |
|---|---|---|
| `[M-120-positional-fallback]` | Allow `.0`/`.1` positional access на named tuples (Rust-style fallback) | If practice shows need |
| `[M-120-named-positional-mix]` | Mixed positional+named in single tuple decl `(int, x f64)` | Out of scope V1; complex grammar, low value |
| `[M-120-record-positional]` | Positional records `type X { f64, f64 }` (no field names в `{}`) | Bracket choice already encodes semantics; no rationale для positional records |
| `[M-120-stack-arrays]` | Stack-allocated fixed-size arrays `[3]Vec3` inline | Beyond Plan 120 scope; separate plan |
| `[M-120-inline-records]` | Force record stack-allocation via attribute `#[stack]` | Bracket choice already provides; не нужен |

---

## Rollback strategy

1. Revert PR atomic.
2. Worktree `nova-p120` preserved.
3. Per-phase rollback (Ф.1-Ф.5 = отдельные commits).
4. Cross-platform CI smoke за ~30 min.

---

## Cross-references

### Связь с уже-закрытыми planами

- **Plan 59 Ф.7.4** (rejected named tuples 2026-05-21) — **REOPENED**
  Plan 120 с corrected reasoning.
- **Plan 91.8a/b/c** — protocols + array methods; precedent для small
  language additions.

### Связь с активными planами

- **Plan 91.12** (std/net) — FFI returns могут benefit от named tuples
  (`type SocketReadResult(data []u8, addr SocketAddr, err i64)` вместо
  positional `([]u8, SocketAddr, i64)`).
- **Plan 115** (foundational FFI) — tuple FFI returns naturally extend
  на named tuples (better readability в external fn signatures).
- **Plan 114** (keyword refresh) — orthogonal, no conflict.

### Spec D-blocks

- **D32** (default by-reference + by-value primitives) — **amend** для
  explicit value/reference taxonomy
- **D52** (revised type declarations) — **amend** для extended tuple
  syntax + allocation contract
- **D123** (tuple monomorphization) — **amend** для named field access
  codegen
- **D215** (NEW) — Named tuple fields + canonical use cases

---

## Status — closure summary

> Заполняется агентом по завершении Plan 120. Поля:
> - Что сделано per phase
> - Stdlib migrations (если any tuples переведены)
> - Examples list в `examples/value_types/`
> - Bench results: named tuple vs record allocation profile
> - Cross-platform PASS
> - Memory `project-plan120-status.md` created
> - Sprint logs updated
