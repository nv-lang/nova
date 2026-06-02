// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 124 — Private field visibility (`priv` modifier для records + named tuples)

> **Создан 2026-06-01.**
> **Status:** 🆕 PLANNED — roadmap-индекс. Декомпозирован на 7
> sub-plan'ов (124.1-124.7) per §6 «split». Каждый sub-plan
> independently shippable; release-train V1→V7 incremental.
> **Приоритет:** P1 (V1 = 124.1 — foundational; closes major
> OOP-grade gap; current Nova fields all-public — нет API boundary
> control, refactoring-unsafe). V2-V7 — incremental wins + polish.
> **Оценка (umbrella):** ~10-14 dev-day across 7 sub-plans.
> **Зависимости:**
>   - Plan 35 ✅ (cross-file resolve / R26 visibility) — module-
>     level pub/priv infrastructure.
>   - Plan 108 ✅ (D175/D176 readonly field + ro/mut modifiers) —
>     field-level modifier parser/checker patterns.
>   - Plan 114 ✅ (D184 keyword refresh) — `ro`/`mut`/`consume`
>     canonical.
>   - Plan 120 ✅ (D215 named tuples) — named tuple field
>     declarations.
>   - Plan 50 (D102 diagnostic format) ✅ — error code format.
> **D-блоки:** **D220-D222 NEW** (декомпозированы по слою:
>   D220 core priv semantics, D221 constructor/pattern rules,
>   D222 cross-cutting protocols/generics/tuples).
> **Worktree convention:** `nova-p124` (umbrella).
> **Recommended model:** **Opus 4.7 + Thinking ON + Effort High**
>   — type-system change, semantic precision требует depth.
>   Sub-plans 124.5/124.6 (LSP/migration) — Sonnet 4.6 + High
>   acceptable если 124.1-124.4 уже landed (mechanical sweep).

---

## 1. Контекст и мотивация

### 1.1 Текущее состояние Nova field visibility

Nova сейчас имеет ТОЛЬКО **module-level visibility** (D5 pub/priv,
Plan 35 R26):
- `pub type X { ... }` — type exported из module
- `export type X { ... }` — alias для pub (D5)
- Fields ВСЕГДА accessible если type accessible

**Нет per-field visibility.** Если type `Account` exported, **все**
поля (`money`, `balance`, `secret_key`, ...) automatically доступны
извне для read AND write (`acc.money = 100`).

### 1.2 Что хочется

```nova
export type Account {
    priv mut money f64,     // private: только методы Account могут читать/писать
    ro name str             // public readonly: external read ok, write нет
}

export fn Account.new(name str) -> Self =>
    Self { money: 0.0, name }    // OK — внутри type scope

export fn Account mut @deposit(amount f64) {
    @money = @money + amount     // OK — @-access внутри type method
}

// Outside (другой module / другая часть кода):
ro acc = Account.new("alice")
ro n = acc.name              // ✅ ok — pub readonly
ro m = acc.money             // ❌ E_PRIV_FIELD_READ
acc.money = 100              // ❌ E_PRIV_FIELD_WRITE
ro Account { money, name } = acc // ❌ E_PRIV_FIELD_PATTERN
```

### 1.3 Почему это важно (production case)

1. **Invariant enforcement.** `money` mutate'ится ТОЛЬКО через
   `@deposit`/`@withdraw` — гарантия non-negative balance. Без
   priv — external code может `acc.money = -1000` (поломать
   invariant).
2. **Refactoring safety.** Внутренняя структура type'а — implementation
   detail. priv позволяет менять fields без breaking external API.
3. **API surface clarity.** Public fields = stable API; priv =
   internal. Документация / IDE / nova doc показывают только
   public surface.
4. **Encapsulation enables abstraction.** OOP fundamental — без него
   Nova остаётся «pojo with methods», не industrial-grade type system.
5. **Security.** Sensitive fields (auth tokens, crypto keys, raw
   pointers) скрываются на compile-time — no accidental leakage
   через `@field` patterns.

### 1.4 Plan 35 / D5 vs Plan 124

| Уровень | Что | Кем | Введено |
|---|---|---|---|
| Module-level type | `pub type X { ... }` | Plan 35 / D5 | ✅ existing |
| Module-level fn | `pub fn f(...) { ... }` | Plan 35 / D5 | ✅ existing |
| Field mutability | `mut money f64` / `ro name str` | Plan 108 / 114 / D175/D176/D184 | ✅ existing |
| **Field visibility** | `priv mut money f64` | **Plan 124 / D220-D222** | 🆕 **NEW** |

Plan 124 — последний missing piece в Nova type system visibility
matrix.

---

## 2. Comparative analysis vs Go/Rust/TS/Kotlin/Java/Swift/C#

### 2.1 Capability matrix

| Capability | Go | Rust | TS | Kotlin | Java | Swift | C# | **Nova (Plan 124)** |
|---|---|---|---|---|---|---|---|---|
| Per-field visibility | ❌ (case-based) | ✅ `pub` per field | ✅ `private`/`public` | ✅ `private`/`protected`/`internal` | ✅ `private`/`protected`/`public`/package | ✅ `private`/`fileprivate`/`internal`/`public` | ✅ `private`/`protected`/`internal`/`public` | ✅ **`priv` (V1)** |
| Default visibility | `lower` = pkg-priv | private to mod | public | public | package | internal | private | **public** (opt-in priv) |
| Strict «type-internal only» | ❌ (pkg-wide) | ❌ (mod-wide) | ✅ class-only (#field) | ❌ class+subclass | ✅ class-only | ❌ file/module/class | ✅ class-only | ✅ **type-method-only** |
| Reflection backdoor | ❌ | ❌ | ❌ | ✅ (java.lang.reflect) | ✅ (java.lang.reflect) | ✅ (Mirror) | ✅ (System.Reflection) | ✅ **NO reflection — compile-time enforced** |
| Constructor visibility | ❌ (struct lit anywhere) | ✅ `pub(...)` per field | ✅ via accessor | ✅ via constructor visibility | ✅ via constructor visibility | ✅ via init visibility | ✅ via ctor visibility | ✅ **forced factory if any priv field (V1)** |
| Pattern destructure rules | ❌ (no destructure) | ✅ enforced | ✅ enforced | ⚠️ via destructuring decl | n/a | n/a | n/a | ✅ **enforced (V1.2)** |
| Tuple/positional struct | ❌ | ✅ `struct Foo(pub T)` | ❌ (no tuple types) | ❌ | ❌ | ❌ | ❌ | ✅ **named tuples Plan 120 D215 + priv (V1.4)** |
| Generic type field viz | ✅ uniform | ✅ uniform | ✅ uniform | ✅ uniform | ✅ uniform | ✅ uniform | ✅ uniform | ✅ **uniform (V1.3)** |
| Protocol/trait impl access | ✅ (pkg-wide ok) | ✅ via assoc methods | ✅ via class methods | ✅ via class methods | ✅ via class methods | ✅ via methods | ✅ via class methods | ✅ **type methods only (V1.4)** |
| Same-module override | n/a (pkg-wide) | n/a (mod-wide) | ❌ | `internal` | package | `internal` | `internal` | ❌ **strict type-only** (V1); future `#[visible_to(...)]` |
| Test escape hatch | n/a (pkg) | `pub(crate)` | n/a | `internal` | package | `@testable import` | `InternalsVisibleTo` | ✅ **`#test_access` attr (V6)** |
| Doc generator filters | ✅ godoc | ✅ rustdoc | ✅ tsdoc | ✅ KDoc | ✅ Javadoc | ✅ DocC | ✅ DocFX | ✅ **nova doc hides priv (V5)** |
| LSP hover/completion | ✅ gopls | ✅ rust-analyzer | ✅ tsserver | ✅ IDEA | ✅ JDT | ✅ SourceKit | ✅ Roslyn | ✅ **nova-lsp Plan 104.x (V5)** |
| Edition / source compat | n/a | edition | n/a | source-level | source-level | API-level | source-level | ✅ **edition opt-in (V6)** |

### 2.2 Nova edges (почему лучше)

Nova **match-or-exceeds** на 11/14 capabilities + **3 Nova-only**:

1. **Strictest «type-internal only» scope.** Java/C#/Swift class-only
   — best precedent. Go/Rust/Kotlin pkg/mod-wide — leak'ит broader.
   Nova `priv` = **только методы own type'а** (включая static `fn
   TypeX.new(...)` и instance `fn TypeX @method()` — type-bounded).
   No same-module sibling access (стрictнее всех эталонов).
2. **NO reflection backdoor.** Java/Kotlin/C#/Swift все имеют
   reflection API позволяющий читать private (privileges aside).
   Nova — **zero reflection** by design (D6 managed GC + AOT
   codegen). Compile-time enforcement = **hard guarantee**.
3. **Tuple field privacy.** Named tuples (Plan 120 D215) — stack
   value types с named field access. Plan 124 расширяет `priv` на
   tuple form: `type Vec3(priv x f64, priv y f64, priv z f64)`.
   Эталоны (кроме Rust pub(...) on tuple struct) НЕ покрывают
   tuple form.
4. **Effect-system integration.** Priv field access — method-only.
   Methods имеют explicit effect signature. → API boundary **видна
   из effect signature** (Plan 03.4 D140 effect-surface). Доп.
   layer documentation/safety.
5. **`consume` linearity composition.** `priv consume <type>` field
   — exclusive ownership + invisible externally. Combines encapsulation
   + uniqueness. Unique Nova capability.

### 2.3 Где другие лучше нас (gap closure plan)

- **Kotlin `internal`:** module-wide visibility. Nova **strict
  type-only** в V1 — sometimes restrictive (test code, sibling
  types coordinating). V6 (Plan 124.6) добавит `#test_access`
  attribute + optional `#[visible_to(TypeY)]` friend-type
  declaration для controlled relaxation.
- **Java `protected`:** subclass access. Nova не имеет наследования
  (D1 — без классов / inheritance). N/A → не нужно closure.
- **Rust `pub(crate)` / `pub(in path)`:** fine-grained module
  visibility. Nova `priv` = strict type-only V1; V7 evaluates
  whether fine-grained mod-level priv needed.

**Cumulative Nova V7 result:** match-or-exceeds на ВСЕХ 14
capabilities + Nova-only superior на 3 axes. Production-grade
encapsulation **stricter** чем любого эталона.

---

## 3. Design (umbrella overview)

### 3.1 Syntax

```nova
// Record form (per-field modifiers).
export type Account {
    priv mut money f64,
    ro name str,
    priv ro id u64,           // priv + ro composable
}

// Named tuple form (Plan 120 D215 extension).
export type Vec3(
    priv x f64,
    priv y f64,
    priv z f64,
)

// Generic type — modifiers uniform.
export type Stack[T] {
    priv mut items []T,
    ro capacity int,
}
```

**Modifier ordering** (per Plan 108 D175/D176 + Plan 114 D184):
- Visibility first: `priv` (default pub if omitted)
- Mutability second: `mut` / `ro` / `consume` (default ro for fields?
  TBD в DECISION-B)
- Name + type

### 3.2 Semantics summary

**`priv` field is accessible ONLY from:**
1. Instance methods of own type: `fn Account @method() { @money }`.
2. Static methods of own type: `fn Account.new(...) { ... }`.
3. NOT accessible from:
   - Other types' methods
   - Free functions (even в том же module)
   - Test functions (унless test attribute V6)
   - Pattern destructure outside type-methods
   - Record literal init outside type-methods
   - Protocol implementations (которые extern fn'ы)

### 3.3 Default visibility — public (validated empirically)

**Public by default + opt-in `priv` per field.** Confirmed by
**kubernetes statistical audit** ([06-field-visibility-go-kubernetes.md](../research/06-field-visibility-go-kubernetes.md)):

| Layer | Public fields | Private fields |
|---|---|---|
| `pkg/` internal | 47.0% | 53.0% |
| `staging/` library API | 63.7% | 36.3% |
| `cmd/` entry points | 67.8% | 32.2% |
| `core/v1/types.go` (API canonical) | **92.4%** | 7.6% |
| **Aggregate** | **59.4%** | **40.6%** |

В **public API surface** (analog Nova `export type`) — **~92% полей
public**. Opt-in `priv` annotation требуется на ~8% полей — minimum
boilerplate для most common case.

### 3.3.1 Type-level default flip — `type X priv { ... }`

Для invariant-heavy types (Account / Mutex / Connection / Cursor) где
majority of fields should be private — **opt-in default flip per
type** через syntactic marker `priv` после имени type'а:

```nova
export type Account priv {       // default = priv для этого type'а
    pub ro name str              // explicit pub override
    pub mut money f64            // explicit pub override
    cache Cache                  // default = priv (inherits type-level)
    mut last_modified Instant    // default = priv
}
```

**Преимущества над edition flip (V7 alternative rejected):**
- No edition migration tool needed (per-type opt-in)
- No stdlib mass migration
- Backward compat preserved (default-default остаётся public)
- Locally explicit — reader видит `type X priv` сразу понимает что
  это invariant-bearing type

**Syntax position:** между type name и `{` (или `(` для tuples).
Symmetric: `priv` keyword same в field-level AND type-level position.

**Tuple form** (Plan 120 D215 extension):
```nova
export type Secret priv (key str, mut salt []u8)
// все поля priv по умолчанию

export type Secret(priv key str, priv mut salt []u8)
// same semantics — per-field priv, explicit
```

V1 (Plan 124.1) — field-level `priv` modifier.
V2 (Plan 124.2 или 124.X) — type-level `priv` flip.
~~V7 edition default flip~~ — **REJECTED** в пользу type-level
opt-in flip (cleaner, no migration cost).

### 3.4 Architectural foundation

**Parser/AST changes:**
- `compiler-codegen/src/parser/`: per-field modifier parsing
  расширяется на `priv` token (parallel `mut`/`ro`/`consume`).
- `compiler-codegen/src/ast/`: `FieldDecl { visibility: Visibility,
  mutability: Mutability, name, ty, ... }` — `Visibility` enum
  `{ Public, Private }`.
- **Type-level default:** `TypeDecl { default_visibility: Visibility,
  fields: Vec<FieldDecl>, ... }`. Field's effective visibility =
  field-level explicit OR type-level default OR module-level default
  (`Public`).

**Type-checker changes:**
- New diagnostic codes (per Plan 50 D102 format):
  - `E_PRIV_FIELD_READ` — read priv field outside type
  - `E_PRIV_FIELD_WRITE` — write priv field outside type
  - `E_PRIV_FIELD_PATTERN` — destructure priv field outside type
  - `E_PRIV_FIELD_INIT` — init priv field via record/tuple literal

### 3.4 Architectural foundation

**Parser/AST changes:**
- `compiler-codegen/src/parser/`: per-field modifier parsing
  расширяется на `priv` token (parallel `mut`/`ro`/`consume`).
- `compiler-codegen/src/ast/`: `FieldDecl { visibility: Visibility,
  mutability: Mutability, name, ty, ... }` — `Visibility` enum
  `{ Public, Private }`.

**Type-checker changes:**
- New diagnostic codes (per Plan 50 D102 format):
  - `E_PRIV_FIELD_READ` — read priv field outside type
  - `E_PRIV_FIELD_WRITE` — write priv field outside type
  - `E_PRIV_FIELD_PATTERN` — destructure priv field outside type
  - `E_PRIV_FIELD_INIT` — init priv field via record/tuple literal
    outside type
  - `E_PRIV_FIELD_PROTOCOL` — protocol impl ext fn touches priv
  - `E_PRIV_TUPLE_POSITIONAL_ACCESS` — `.N` access на priv tuple
    field outside type

**Codegen changes:**
- C struct field names — unchanged (priv is checker-only)
- nova doc: hide priv fields from documentation
- LSP: filter priv from autocomplete outside type

**Spec changes:**
- D220 NEW (core semantics + visibility rules)
- D221 NEW (constructor + pattern rules)
- D222 NEW (cross-cutting: protocols/generics/tuples)
- D5 amend (note about per-field visibility)
- D175/D176 amend (composition с priv)
- D215 amend (named tuple priv form)

### 3.5 Release train V1-V7

| Version | Sub-plans | Что | Релиз timing |
|---|---|---|---|
| **V1** | **124.1** | Core: parser + AST + checker + record + constructor | 0.1 foundational |
| **V2** | + 124.2 | Pattern destructure + literal init rules | 0.1.1 |
| **V3** | + 124.3 | Generic type uniform handling | 0.1.1 |
| **V4** | + 124.4 | Named tuple priv (Plan 120 D215 ext) | 0.2 |
| **V5** | + 124.5 | nova doc + LSP integration | 0.2 |
| **V6** | + 124.6 | Test access escape + `#[visible_to]` friends | 0.2+ |
| **V7** | + 124.7 | Edition default flip evaluation | 0.3+ |

Каждый Vn ↔ один sub-plan. Independently shippable.

---

## 4. Sub-plans 124.1-124.7

### 4.1 Plan 124.1 — Core: parser/AST/checker/record (V1 foundational)

**Scope:** parser принимает `priv` modifier на record field; AST
extended; type-checker enforce'ит access rules для basic record
case.

**Status:** 🆕 PLANNED. Worktree: `nova-p124` (umbrella shared).
Sub-plan doc: spawn `docs/plans/124.1-core-record.md` при старте.

**Phases (Ф.0-Ф.6):**
- Ф.0 Investigation + DECISION-A (default visibility), DECISION-B
  (priv + ro/mut composition order), DECISION-C (`priv` keyword
  reservation impact на parser).
- Ф.1 Lexer + parser: tokenize `priv` + parse в FieldDecl.
- Ф.2 AST: `Visibility` enum + FieldDecl extension.
- Ф.3 Type-checker: access rule enforcement + 4 error codes
  (READ/WRITE/INIT/PATTERN).
- Ф.4 Positive (≥10) + negative (≥6) tests + property tests.
- Ф.5 Spec D220 NEW + D5 amend + README spec.
- Ф.6 Closure: 3 logs + plan status flip + push.

**Эстимат:** ~2-3 dev-day.

**Acceptance:** см. §6.1 (A1.1-A1.10).

---

### 4.2 Plan 124.2 — Pattern destructure + literal init rules

**Scope:** record literal `Foo { field: value }` outside type → priv
field init blocked; pattern `match x { Foo { field, ... } }` → priv
field bind blocked.

**Status:** 🆕 PLANNED. Gated на 124.1 ✅.

**Phases (Ф.0-Ф.6):**
- Ф.0 Pattern audit: какие existing tests/std использует record
  literal на types с priv potential.
- Ф.1 Type-checker: literal init rule + `E_PRIV_FIELD_INIT`.
- Ф.2 Type-checker: pattern destructure rule + `E_PRIV_FIELD_PATTERN`.
- Ф.3 Edge cases: spread `{ ..rest }` patterns, rest pattern, nested
  destructure через `priv` field type.
- Ф.4 Tests (8+ positive, 5+ negative).
- Ф.5 Spec D221 NEW.
- Ф.6 Closure.

**Эстимат:** ~1.5 dev-day.

**Acceptance:** см. §6.2.

---

### 4.3 Plan 124.3 — Generic type uniform handling

**Scope:** generic types `Stack[T]` с priv fields — rules apply
uniformly per mono'd instance. Plan 59 + 59.1 mono infrastructure
не должна leak priv access через type-param substitution.

**Status:** 🆕 PLANNED. Gated на 124.1 ✅ + Plan 59.1 ✅ (mono'd
tuples).

**Phases (Ф.0-Ф.6):**
- Ф.0 Generic mono path audit: identify где field-access check
  происходит (pre-mono AST или post-mono codegen).
- Ф.1 Ensure priv visibility checked в pre-mono phase
  (consistent rule regardless of T).
- Ф.2 Edge case: generic type method calls another generic with
  same T — priv field passes through.
- Ф.3 Mono'd named tuple priv (Plan 120 D215 + Plan 59.1 D216).
- Ф.4 Tests (8+ positive, 4+ negative).
- Ф.5 Spec D220 amend (generic clause).
- Ф.6 Closure.

**Эстимат:** ~1.5 dev-day.

**Acceptance:** см. §6.3.

---

### 4.4 Plan 124.4 — Named tuple priv + protocol interaction

**Scope:**
1. Named tuples (Plan 120 D215): `type Vec3(priv x f64, priv y f64,
   priv z f64)` — positional `.0`/`.1`/`.2` access blocked outside
   type; named `.x`/`.y`/`.z` same.
2. Protocol impl rules: if `type Account` implements protocol
   `Display`, the impl fn (`fn Account @to_string()`) IS type method
   → priv access OK. External free fn implementing protocol via
   `external fn`-style trick — priv access BLOCKED.

**Status:** 🆕 PLANNED. Gated на 124.1 ✅ + Plan 120 ✅ + Plan 97
protocol infrastructure ✅.

**Phases (Ф.0-Ф.6):**
- Ф.0 Named tuple parser extension + protocol impl audit.
- Ф.1 Named tuple `priv` parser/AST/checker.
- Ф.2 Positional access rule (`.0` на priv tuple field outside).
- Ф.3 Protocol impl distinction: type-method vs external.
- Ф.4 Tests (10+ positive, 6+ negative — split tuple / protocol).
- Ф.5 Spec D222 NEW + D215 amend.
- Ф.6 Closure.

**Эстимат:** ~2 dev-day.

**Acceptance:** см. §6.4.

---

### 4.5 Plan 124.5 — nova doc + LSP integration

**Scope:**
- `nova doc` hides priv fields by default; `--include-private` flag
  для internal docs.
- LSP autocomplete filters priv outside type-method scope.
- LSP hover shows visibility badge (`priv` / `pub`).
- Diagnostic quick-fixes: «add public getter method» suggestion при
  E_PRIV_FIELD_READ.

**Status:** 🆕 PLANNED. Gated на 124.1 ✅ + Plan 104.x LSP ✅
(infrastructure) + Plan 45 nova doc ✅.

**Phases (Ф.0-Ф.6):**
- Ф.0 nova doc + LSP infrastructure review.
- Ф.1 nova doc filter + `--include-private` flag.
- Ф.2 LSP autocomplete filter + hover badge.
- Ф.3 Quick-fix suggestion (Plan 50 D102 format).
- Ф.4 Tests (LSP integration suite).
- Ф.5 Doc — `docs/field-visibility-guide.md`.
- Ф.6 Closure.

**Эстимат:** ~1.5 dev-day.

**Acceptance:** см. §6.5.

---

### 4.6 Plan 124.6 — Test access escape + `#[visible_to]` friends

**Scope:**
- `#[test_access(TypeX)]` attribute на test module — granted priv
  access. Parallel Rust `#[cfg(test)]` + `pub(crate)`.
- `#[visible_to(TypeY)]` attribute на field — explicit friend
  declaration (TypeY methods get access).
- Both rare-use; default behavior remains strict.

**Status:** 🆕 PLANNED. Gated на 124.1 ✅.

**Phases (Ф.0-Ф.6):**
- Ф.0 Use case audit: which existing tests would need access?
- Ф.1 `#[test_access]` parser/checker.
- Ф.2 `#[visible_to]` parser/checker.
- Ф.3 Tests.
- Ф.4 Migration guide.
- Ф.5 Spec D220 amend (escape hatch clause).
- Ф.6 Closure.

**Эстимат:** ~1 dev-day.

**Acceptance:** см. §6.6.

---

### 4.7 Plan 124.7 — Type-level `priv` default flip syntax

**Scope:** `type X priv { ... }` (или `type X priv (...)` для tuples)
— per-type opt-in flip default field visibility = private. Explicit
`pub` modifier overrides field-level. Для invariant-heavy types
(Account / Mutex / Connection / Cursor) где majority of fields
private.

**Rationale rejecting V7 edition flip:** Original Plan 124.7 plan
proposed edition-level default flip (`nova migrate --to-edition=next
--add-public`). Rejected 2026-06-02 because:
1. Kubernetes data: aggregate 59.4% public — flipping default не дает
   clear win.
2. Per-type `priv {}` syntax granular — invariant-heavy types
   opt-in, data-bag types stay default. Better fit для bimodal
   distribution (kubernetes pkg/ 47% public vs core/v1 92%).
3. No edition migration tool needed — purely additive.
4. Backward compat preserved without effort.

**Syntax:**
```nova
// Record form: priv ПОСЛЕ имени type'а, ДО {
export type Account priv {
    pub ro name str          // explicit pub
    mut money f64            // default = priv
    cache Cache              // default = priv
}

// Named tuple form (Plan 120 D215):
export type Secret priv (key str, mut salt []u8)

// Equivalent: per-field priv с public default:
export type Secret(priv key str, priv mut salt []u8)
```

**Status:** 🆕 PLANNED. Gated на 124.1 ✅ + Plan 120 ✅ (named
tuples для tuple form).

**Phases (Ф.0-Ф.6):**
- Ф.0 Parser lookahead audit: `priv` после type-name distinguishable
  от field-level `priv` через context.
- Ф.1 Parser: extend `TypeDecl` parser для optional `priv` после name.
- Ф.2 AST: `TypeDecl { default_visibility: Visibility, ... }`.
- Ф.3 Type-checker: field's effective visibility resolution
  (field-level overrides type-level overrides module-default=Public).
- Ф.4 Tests (10+ positive, 5+ negative — combos type-level priv +
  field-level pub/priv overrides).
- Ф.5 Spec D220 amend (type-level flip clause) — full description.
- Ф.6 Closure.

**Эстимат:** ~1.5 dev-day.

**Acceptance:** см. §6.7.

---

## 5. Декомпозиция (split rationale)

Per project conventions (Plan 33.x, 91.x, 100.x, 103.x, 123) —
umbrella + sub-plans. Каждый sub-plan:
- Self-contained scope
- Independently shippable
- Own Ф.0-Ф.6 cycle
- Own acceptance criteria (A_N.M)
- Own D-block ИЛИ D-block amend
- Own closure logs entry

**Sub-plan dependency graph:**

```
                124.1 (V1 core record)
                    │
        ┌───────────┼───────────┬──────────┐
        │           │           │          │
      124.2       124.3       124.4      124.6
   (pattern)   (generics)   (tuple+prot) (escape)
        │           │           │          │
        └─────┬─────┴─────┬─────┘          │
              │           │                │
            124.5 ←───────┼────────────────┘
          (LSP/doc)       │
              │           │
            124.7         │
        (edition flip)    │
                          │
                          (V7 long-term)
```

124.1 — gate для всех. 124.2/124.3 могут идти параллельно. 124.4
расширяет на tuples — после 124.1. 124.5 gated на 124.1 + Plan
104.x. 124.6 escape hatch — independent от 124.2-124.5. 124.7 final.

---

## 6. Acceptance criteria (per sub-plan)

### 6.1 Plan 124.1 acceptance (A1.1-A1.10)

- **A1.1** Parser принимает `priv` modifier в record field
  declaration: `type X { priv mut f T }` парсится.
- **A1.2** Modifier composition `priv mut` / `priv ro` / `priv
  consume` — все три combinations PASS.
- **A1.3** AST `FieldDecl.visibility = Visibility::Private` корректно
  attribut'ируется.
- **A1.4** Type-checker emit'ит `E_PRIV_FIELD_READ` (Plan 50 D102
  format) на `outside.priv_field` outside type method.
- **A1.5** Type-checker emit'ит `E_PRIV_FIELD_WRITE` на
  `outside.priv_field = X` outside type method.
- **A1.6** Inside type instance method `@priv_field` access PASS
  (read AND write where mutable).
- **A1.7** Inside type static method `fn X.new(...)` priv field
  access PASS.
- **A1.8** Regression — full nova test 0 new FAIL (baseline vs Plan
  124.1 ON).
- **A1.9** plan124_1 fixtures ≥16 (10+ positive, 6+ negative) ALL
  PASS на release nova-cli + clang.
- **A1.10** Spec D220 NEW + D5 amend + README spec entry — landed.

### 6.2 Plan 124.2 acceptance (A2.1-A2.8)

- **A2.1** Record literal `Foo { priv_f: X, ... }` outside type →
  `E_PRIV_FIELD_INIT`.
- **A2.2** Pattern `Foo { priv_f, ... }` outside type → `E_PRIV_FIELD_PATTERN`.
- **A2.3** Spread pattern `{ priv_f, ..rest }` — `rest` НЕ содержит
  priv fields (excluded automatically).
- **A2.4** Nested destructure через priv field type — outer field
  должен быть accessible; inner destructure respects inner type's
  priv rules.
- **A2.5** Inside type method — both literal init AND pattern PASS.
- **A2.6** plan124_2 fixtures ≥13 (8+ positive, 5+ negative) PASS.
- **A2.7** Regression — 0 new FAIL.
- **A2.8** Spec D221 NEW — landed.

### 6.3 Plan 124.3 acceptance (A3.1-A3.8)

- **A3.1** Generic type `Stack[T] { priv mut items []T }` —
  parser/checker PASS.
- **A3.2** Mono'd instance `Stack[int]` enforces priv same as
  non-generic.
- **A3.3** Generic method calling another generic method on same
  T — priv field passes through correctly.
- **A3.4** Plan 59.1 mono'd tuple D216 — priv preserved через
  monomorphization.
- **A3.5** plan124_3 fixtures ≥12 (8+ positive, 4+ negative) PASS.
- **A3.6** Regression — 0 new FAIL.
- **A3.7** Spec D220 amend (generic clause).
- **A3.8** Tuple-in-Option `Option[(priv T, priv U)]` works
  consistently.

### 6.4 Plan 124.4 acceptance (A4.1-A4.10)

- **A4.1** Named tuple `type Vec3(priv x f64, ...)` parser PASS.
- **A4.2** Positional access `vec.0` на priv tuple field outside
  type → `E_PRIV_TUPLE_POSITIONAL_ACCESS`.
- **A4.3** Named access `vec.x` на priv tuple field outside type
  → `E_PRIV_FIELD_READ`.
- **A4.4** Inside type method — both positional AND named work.
- **A4.5** Protocol impl (type-method-based) — priv access OK.
- **A4.6** Protocol impl extern-fn-based — priv access BLOCKED.
- **A4.7** plan124_4 fixtures ≥16 (10+ positive, 6+ negative) PASS.
- **A4.8** Regression — 0 new FAIL.
- **A4.9** Spec D222 NEW + D215 amend.
- **A4.10** Plan 120 named tuple fixtures (existing) — 0 new FAIL.

### 6.5 Plan 124.5 acceptance (A5.1-A5.8)

- **A5.1** `nova doc` hides priv fields by default.
- **A5.2** `nova doc --include-private` shows priv fields с badge.
- **A5.3** LSP autocomplete filters priv outside type-method scope.
- **A5.4** LSP hover показывает visibility badge.
- **A5.5** Quick-fix «add public getter» suggestion срабатывает.
- **A5.6** plan124_5 fixtures (LSP suite) PASS.
- **A5.7** Doc `docs/field-visibility-guide.md` написан.
- **A5.8** Regression — 0 new FAIL.

### 6.6 Plan 124.6 acceptance (A6.1-A6.8)

- **A6.1** `#[test_access(TypeX)]` attribute parser PASS.
- **A6.2** Test fn с attribute получает priv access к TypeX.
- **A6.3** `#[visible_to(TypeY)]` field attribute PASS.
- **A6.4** TypeY's methods get access к marked priv field of TypeX.
- **A6.5** Conservative: только marked fields, не whole type.
- **A6.6** plan124_6 fixtures ≥10 PASS.
- **A6.7** Regression — 0 new FAIL.
- **A6.8** Spec D220 amend (escape hatches clause).

### 6.7 Plan 124.7 acceptance (A7.1-A7.8)

- **A7.1** Parser принимает `type X priv { ... }` syntax (record form).
- **A7.2** Parser принимает `type X priv (...)` syntax (named tuple
  form, Plan 120 D215 ext).
- **A7.3** `pub` modifier на field overrides type-level priv default.
- **A7.4** Field без modifier inherits type-level default (priv).
- **A7.5** Type-level default flip + field-level explicit modifiers
  combine correctly (4 cases tested: default-default, default-explicit,
  flip-default, flip-explicit).
- **A7.6** plan124_7 fixtures ≥15 (10+ positive, 5+ negative) PASS.
- **A7.7** Regression — 0 new FAIL.
- **A7.8** Spec D220 amend (type-level flip clause) — landed.

### 6.8 Umbrella-level acceptance (Plan 124 total)

- **AU.1** Все sub-plans 124.1-124.7 ✅ closed.
- **AU.2** Comparative analysis vs Go/Rust/TS/Kotlin/Java/Swift/C#
  match-or-exceeds на 14 capabilities + 3 Nova-only verified.
- **AU.3** Full regression: 0 new FAIL across all existing nova
  test suites (baseline vs V7).
- **AU.4** Both per-field `priv` AND type-level `priv {...}` flip
  working — kubernetes-validated bimodal coverage (API surface
  default-public; invariant-heavy types use type-level flip).
- **AU.5** Production deployment artifacts landed (LSP, doc,
  spec D220-D222, escape hatches V6, no edition migration tool
  needed thanks to V7 type-level flip approach).
- **AU.6** D47 (07-modules.md) amended — `_prefix` convention deprecated,
  `priv` keyword normative; cross-ref to D220.

---

## 7. Risk register

### 7.1 Per sub-plan risks

**Plan 124.1:**
- **R-1.1: `priv` keyword reservation conflict.** Existing code
  использует `priv` как identifier? Mitigation: pre-Ф.1 grep
  на std/ + nova_tests/; если conflicts — rename strategy ИЛИ
  contextual keyword (only valid в field decl position).
- **R-1.2: type method recognition.** `@field` access inside
  `fn TypeX @method()` body — checker должен distinguish recv
  type. Mitigation: extend existing receiver-type tracking infra
  (Plan 108.x precedent).
- **R-1.3: false-positive friend access.** Same module sibling
  type's method accidentally allowed. Mitigation: explicit
  type-only check (NOT module-level).

**Plan 124.2:**
- **R-2.1: literal init breakage in stdlib.** Stdlib types
  использует record literals with what would be priv fields.
  Mitigation: stdlib audit pre-Ф.4; stdlib types remain all-pub
  during V1-V6 (V7 evaluates flip).
- **R-2.2: pattern spread fallback.** `{ ..rest }` pattern —
  how does rest type interact с priv? Mitigation: rest type
  excludes priv fields (semantically «public view»).

**Plan 124.3:**
- **R-3.1: mono cache visibility info.** Plan 59 mono'd tuple
  schema doesn't carry visibility metadata. Mitigation: visibility
  checked pre-mono (AST level); mono'd C struct doesn't need
  visibility info (compile-time enforcement).
- **R-3.2: protocol bound interaction.** `[T : SomeProtocol]`
  bounds — if protocol method touches priv через T — should be
  forbidden. Mitigation: protocol method is external (not type-
  method) → access denied uniformly.

**Plan 124.4:**
- **R-4.1: tuple positional `.N` ambiguity.** `.0` на priv tuple
  field — error message clear what user did wrong. Mitigation:
  distinct error code `E_PRIV_TUPLE_POSITIONAL_ACCESS` + hint.
- **R-4.2: protocol impl boundary.** What counts as «type-method-
  based impl» vs «external»? Mitigation: clear D222 spec
  definition.

**Plan 124.5:**
- **R-5.1: nova doc regen breaking.** Hiding priv may break
  external doc references. Mitigation: `--include-private` flag.
- **R-5.2: LSP perf regression.** Visibility filter в autocomplete
  hot path. Mitigation: precomputed visibility cache.

**Plan 124.6:**
- **R-6.1: escape hatch overuse.** `#[test_access]` becomes
  catch-all. Mitigation: lint warning if used >N times per
  project.

**Plan 124.7:**
- **R-7.1: edition flip backward compat.** Old-edition code fails
  to compile в new edition without migration. Mitigation:
  automated `nova migrate` + edition pin per package.

### 7.2 Cross-cutting risks

- **R-X.1: spec drift.** D220-D222 multiple D-blocks — risk of
  inconsistency. Mitigation: D220 = umbrella semantics; D221/D222
  cross-ref + amend D220 as needed.
- **R-X.2: error message UX.** 6+ new error codes — risk of
  confusion. Mitigation: each error code Plan 50 D102 format
  + hint + suggestion + example.
- **R-X.3: stdlib migration cost.** Plan 124.7 audit reveals много
  fields needing annotation. Mitigation: V7 deferred until volume
  measured; V1-V6 — additive, no migration required.

---

## 8. Production-grade requirements

### 8.1 Semantic guarantees

**Formal property:** for every Nova program `P` valid pre-Plan 124
(all fields public-by-default), `P` остаётся valid post-Plan 124
(backward compat). New compile-time checks fire ONLY on programs
explicitly using `priv` modifier (additive feature).

**Verification:**
1. Differential testing: full nova test suite baseline vs Plan 124
   ON — 0 new FAIL.
2. Property-based: random AST with random `priv` annotations + verify
   diagnostic invariants (priv access from outside → error;
   from inside → ok).
3. Stdlib regression: all stdlib types compile unchanged (Plan
   124.7 audit gradual).

### 8.2 Diagnostic UX

All 6 new error codes Plan 50 D102 format:
- `E_PRIV_FIELD_READ` — read priv field outside type
- `E_PRIV_FIELD_WRITE` — write priv field outside type
- `E_PRIV_FIELD_INIT` — init priv via literal outside type
- `E_PRIV_FIELD_PATTERN` — destructure priv outside type
- `E_PRIV_FIELD_PROTOCOL` — protocol impl extern fn accesses priv
- `E_PRIV_TUPLE_POSITIONAL_ACCESS` — `.N` на priv tuple field

Каждый error code:
- Span на нарушающее место
- Note: explain what visibility means
- Hint: suggest fix (add method / use factory / pattern alternative)
- Example: minimal reproducing snippet
- Spec link: D220-D222 reference

### 8.3 Cross-platform determinism

Visibility check is pure AST + type-check operation — deterministic
по definition. CI matrix Windows MSVC / Linux clang / macOS clang
для regression suite.

### 8.4 Bounded overhead

| Resource | Budget | Verification |
|---|---|---|
| Type-check time per module | <2% increase | Plan 57 bench |
| Codegen output unchanged | 0 byte diff | binary diff |
| LSP autocomplete latency | <5ms increase | LSP perf suite |
| nova doc generation time | <10% increase | timing measure |

### 8.5 Escape hatches (V6 + V7)

- `#[test_access(Type)]` attribute — test functions get priv
  access. Default OFF; opt-in per test fn.
- `#[visible_to(OtherType)]` attribute — explicit friend.
- `nova check --allow-priv-access` CLI flag — emergency override
  (warning, not silent).
- Edition tie (V7) — old edition keeps public-by-default; new
  edition могут flip.

### 8.6 Migration

V1-V6 — purely additive, no migration. V7 evaluates edition flip;
if ship — automated migration tool:
- `nova migrate --to-edition=next --add-public` adds `pub` к
  currently-public fields.
- Stdlib audit pre-flip (Plan 124.7 Ф.0).

### 8.7 Documentation

- D220-D222 spec — formal semantics.
- `docs/field-visibility-guide.md` (V5) — user guide: examples,
  comparison с Go/Rust/TS/Kotlin/Java, design rationale, escape
  hatches.
- `docs/migration/124-priv-fields.md` (V7) — migration guide.
- `nova doc` integration — priv badge, hide-by-default.
- Code comments в checker / parser — inline explanation each rule.

---

## 9. Testing strategy

### 9.1 Test categories

**Per sub-plan** (`nova_tests/plan124_N/`):
- Positive ≥N: priv field correctly accessed inside / blocked
  outside, etc.
- Negative ≥M: each error code triggered correctly, exact diagnostic
  message verified.
- Edge cases: sub-plan-specific corner patterns.

**Cross-cutting** (`nova_tests/plan124/`):
- Semantic equivalence: existing code (no `priv`) — bit-identical
  codegen output before/after Plan 124.
- Regression: full nova test 0 new FAIL.
- Property-based: random `priv` annotation programs.

**Stdlib audit** (Plan 124.7 Ф.0):
- All stdlib types — inventory + recommended visibility.

### 9.2 Verification: release nova-cli + clang

Все тесты через release nova-cli + clang (per
`feedback_nova_test_one_pass`):

```bash
cd nova-cli && NOVA_GC_LIB_DIR=... NOVA_GC_INCLUDE_DIR=... \
  cargo run --release --bin nova --quiet -- \
  test ../nova_tests/plan124_N/
```

### 9.3 Negative tests — exact diagnostic format

Каждый negative test использует `// EXPECT_COMPILE_ERROR <code>`
с full code match:
```nova
// EXPECT_COMPILE_ERROR E_PRIV_FIELD_READ
//
// Plan 124.1 — negative: read priv field outside type → error.
module plan124_1.read_outside_neg

export type Account { priv mut money f64, ro name str }

fn main() -> () {
    ro acc = Account { money: 100.0, name: "alice" }  // ← also fails E_PRIV_FIELD_INIT
    ro m = acc.money  // ← target error
}
```

Convention per existing patterns (e.g. plan100_2/D156 cases).

---

## 10. Spec D-blocks decomposition

| D-block | Sub-plan | Scope | Status |
|---|---|---|---|
| **D220** | 124.1 | Field visibility `priv` — core semantics, default, scope rules | NEW |
| **D221** | 124.2 | Constructor + pattern destructure rules | NEW |
| **D222** | 124.4 | Cross-cutting: protocols / generics / tuples (D215 ext) | NEW |
| D5 amend | 124.1 | Note: per-field visibility extends module-level pub/priv | AMEND |
| D175 amend | 124.1 | Composition priv + readonly field | AMEND |
| D176 amend | 124.1 | Composition priv + readonly T | AMEND |
| D215 amend | 124.4 | Named tuple priv form | AMEND |
| D220 amend | 124.3 | Generic clause | AMEND |
| D220 amend | 124.6 | Escape hatches clause | AMEND |

Cross-references:
- D2 (effects) — priv methods have effects visible в signature.
- D5 (pub/priv module-level) — per-field extends.
- D29 (modules) — type scope rules.
- D32 (semantics передачи параметров) — receiver semantics.
- D35 (method declaration `fn T.method`) — defines «type method» scope.
- D52 (record declarations) — field decl syntax.
- D131 (consume) — priv + consume composition.
- D175 (readonly field freeze).
- D176 (readonly T modifier).
- D215 (named tuple — Plan 120) — tuple form extension.

### 10.1 D220 outline (core semantics)

- **§1 Statement:** per-field visibility modifier `priv` added.
- **§2 Default:** public if `priv` omitted.
- **§3 Scope of «private»:** ONLY methods on own type (instance +
  static).
- **§4 Access rules** (read / write / init / pattern).
- **§5 Modifier ordering:** visibility → mutability → name → type.
- **§6 Composition** с ro/mut/consume.
- **§7 Diagnostic codes** (full Plan 50 D102 format).
- **§8 No reflection backdoor** — compile-time enforced.
- **§9 Backward compat** — additive feature.
- **§10 Escape hatches** (V6 forward-ref).
- **§11 Cross-refs** (D5/D29/D35/D52/D131/D175/D176).

### 10.2 Q-resolution

Check open-questions.md для существующих Q'ов:
- Q-field-visibility — closed by D220.
- Q-encapsulation-strategy — closed by D220.
- Q-friend-types — partial close (D220 §10 escape hatches; full
  closure V6/V7).

---

## 11. Documentation deliverables

### 11.1 User-facing

- `docs/field-visibility-guide.md` (V5 → 124.5) — user guide:
  what is `priv`, when use, comparison Go/Rust/TS/Kotlin/Java,
  escape hatches, common patterns.
- `docs/migration/124-priv-fields.md` (V7 → 124.7) — migration
  guide for edition flip.

### 11.2 Developer-facing

- D220-D222 spec — formal semantics.
- `compiler-codegen/src/types/visibility.rs` (или extension to
  existing) — inline comments на each rule.
- Error code definitions (Plan 50 D102 format) с examples.

### 11.3 `nova doc` integration

- Hide priv fields by default.
- `--include-private` flag для internal docs.
- Visibility badge в rendered output.

---

## 12. Tools / model settings

- **Opus 4.7 + Thinking ON + Effort High** — sub-plans 124.1-124.4
  + 124.7 (type system change, semantic precision).
- **Sonnet 4.6 + High** — sub-plans 124.5/124.6 acceptable если
  124.1-124.4 уже landed (mechanical sweep tooling work).
- Release nova-cli builds после каждой checker edit.
- Tests via release nova-cli + clang.

---

## 13. Closure status

🆕 PLANNED (umbrella). Каждый sub-plan flip'ает ✅ CLOSED
независимо. Финальный umbrella close после ALL 7 sub-plans ✅.

**Sub-plan status tracker:**

| Sub-plan | Status |
|---|---|
| 124.1 Core record | 🟢 **V1 FULLY CLOSED 2026-06-02** — parser/AST/lexer + 4 error codes enforcement + 9/9 fixtures (4 positive + 5 negative) + D220 spec + acceptance A1.1-A1.10 all ✅ |
| 124.2 Pattern + literal init | 🟢 **CLOSED 2026-06-02** — Match/IfLet/WhileLet/For/ParallelFor sites + nested + spread (D221 NEW); 14/14 plan124_2 PASS |
| 124.3 Generics | 🟢 **CLOSED 2026-06-02** — uniform enforcement on Generic[T] types verified (10/10 plan124_3); D220 §G1 amend |
| 124.4 Tuple + protocol | 🟢 **CLOSED 2026-06-02** — NamedTupleField priv parsing + 3 checker hooks + protocol impl boundary §3 (D222 NEW); 10/10 plan124_4 PASS |
| 124.5 nova doc + LSP | 🟢 **CLOSED 2026-06-02** — nova doc strip_private filter per-field + render priv keyword + JSON priv_field emit + docs/field-visibility-guide.md; LSP hover/completion forward-ref Plan 104.2/104.3 |
| 124.6 Test access + visible_to | 🟢 **CLOSED 2026-06-02** — `#test_access(TypeX...)` fn attr + `#visible_to(TypeY...)` field attr + unified `priv_field_access_allowed` predicate (D223 NEW); 7/7 plan124_6 PASS |
| 124.7 Type-level priv flip (named tuples) | 🆕 PLANNED |
| **Umbrella** | 🟡 PARTIAL — Plan 124.1 V1 fully closed (compile-time enforcement working); 6 sub-plans remaining (124.2-124.7) |

---

## 14. Execution protocol (autonomous agent)

Pattern Plan 59.1 closure + Plan 123 §15 — reference. Все детали
self-contained чтобы prompt был minimal.

### 14.1 Worktree setup (already done для umbrella)

- Worktree: `d:/Sources/nv-lang/nova-p124` ✅ создан.
- Branch: `plan-124-priv-fields` ✅.
- libuv submodule: ✅ scaffolded.
- Env vars для tests:
  ```
  NOVA_GC_LIB_DIR="d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
  NOVA_GC_INCLUDE_DIR="d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include"
  ```

### 14.2 Sub-plan ordering

Start с **Plan 124.1 (Core record)** — gate для всех остальных.
124.2/124.3 могут parallel. 124.4 после 124.1. 124.5 после
124.1 + Plan 104.x. 124.6 independent escape hatch. 124.7 final.

### 14.3 Per sub-plan execution cycle (Plan 59.1 pattern)

Для каждого Plan 124.N:

1. **Spawn sub-plan doc** `docs/plans/124.N-<slug>.md` с детальной
   Ф.0-Ф.6. Template копировать из Plan 59.1 doc.
2. **Commit scaffold** `docs(plan 124.N): scaffold — ...`.
3. **Ф.0 Investigation:** repro / probe / DECISION finalize.
   Artifact в `docs/plans/124.N-artifacts/investigation.md`.
   Commit `docs(plan 124.N Ф.0): investigation + repro`.
4. **Ф.1-Ф.3 Implementation:** код + commit per фаза.
5. **Ф.4 Tests:** positive + negative + property в
   `nova_tests/plan124_N/`. Verify через release nova-cli + clang.
   Commit `feat(plan 124.N Ф.4): tests — N positive + M negative`.
6. **Ф.5 Spec:** D-block NEW/amend в `spec/decisions/02-types.md`
   (или 07-modules.md если visibility-related). Commit
   `docs(plan 124.N Ф.5): D2XX NEW + cross-refs`.
7. **Ф.6 Closure:** 3 логов:
   - `docs/simplifications.md`
   - `docs/project-creation.txt`
   - `d:/Sources/nv-lang/nova-private/discussion-log.md`
   - Plan doc status flip `🆕 PLANNED → ✅ CLOSED <date>`.
   - Commit `docs(plan 124.N Ф.6): closure logs`.

### 14.4 Regression protocol

Перед finalize:
- plan90/plan90_1/plan91_7/plan59_1 — quick (~5 min).
- plan100_x/plan108_x (closest visibility-adjacent) — targeted.
- Full nova test — final gate (long, can background).

Baseline compare: 0 new FAIL обязательно.

### 14.5 Push protocol

- Per-phase commits — стандарт.
- Push на github после Ф.6 closure: `git push 2>&1 | tail -3`.
- **NO merge в main** без user instruction.

### 14.6 Bail-out

Остановиться + поднять user:
- P0 регрессия.
- Architectural blocker (e.g. `priv` keyword conflict requires
  rename strategy decision).
- D-block conflict (D220-D222 collision с другим plan'ом).
- Cumulative time >2× estimate.

### 14.7 Memory feedback refs

Обязательно соблюдать (см. Plan 123 §15.8 — same 11 refs):
- feedback-isolated-worktree
- feedback_worktree_cwd_clarity
- feedback-worktree-auto-register
- feedback_git_add_specific
- feedback-verify-index-before-commit
- feedback-no-claude-coauthor
- feedback-commit-per-task
- feedback-update-logs
- feedback-no-external-memory-for-project-state
- feedback_nova_syntax
- feedback_nova_test_one_pass
