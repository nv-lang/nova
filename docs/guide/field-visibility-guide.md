// SPDX-License-Identifier: MIT OR Apache-2.0
# Field visibility guide (`priv` modifier)

> **Status:** ACTIVE since 2026-06-02 (Plan 124.1-124.5).
> **Spec:** D220 / D221 / D222 (see `spec/decisions/02-types.md`).

This guide covers Nova's per-field privacy system — when to use
`priv`, how it composes with other modifiers, tool support, and
comparison with mainstream languages.

---

## 1. TL;DR

```nova
export type Account {
    ro name str               // public, immutable
    priv mut balance f64      // private, mutable (only Account methods can touch it)
}

export fn Account.new(n str) -> Account =>
    { name: n, balance: 0.0 }

export fn Account @deposit(amount f64) {
    @balance = @balance + amount    // OK — inside Account method
}

// External code:
ro acc = Account.new("alice")
ro n   = acc.name           // ✅ public
ro b   = acc.balance        // ❌ E_PRIV_FIELD_READ
acc.balance = 100.0         // ❌ E_PRIV_FIELD_WRITE
```

Default visibility is **public** (consistent with Go's exported-by-
case-style fields and Kotlin/Swift defaults в 92.4% of API surface,
as measured on kubernetes API types). Opt-in `priv` per field when
you need invariant protection.

---

## 2. When to use `priv`

| Use case | Recommendation |
|---|---|
| **Invariant-bearing internal state** (balance, lock-state, cursor pos) | ✅ Mark `priv` |
| **Mutable internal cache** (`mut last_modified`) | ✅ Mark `priv` |
| **Sensitive data** (auth tokens, crypto keys, raw pointers) | ✅ Mark `priv` |
| **Public API surface** (record DTO fields, config struct values) | ❌ Leave public |
| **Data-bag types** (event payloads, log records) | ❌ Leave public |

Rule of thumb: **if a method must validate or coordinate before
mutating, mark the underlying field `priv`**. Otherwise, public
keeps the surface minimal.

---

## 3. Syntax + composition

### 3.1 Per-field modifier

Visibility modifier first, then mutability, then name, then type:

```nova
priv mut money f64       // private + mutable
priv ro id u64           // private + readonly
priv consume token Token // private + consume (Plan 100.x)
```

`priv` ordered before `mut`/`ro`/`consume` matches Plan 108 D175/D176
+ Plan 114 D184 modifier-ordering precedent.

### 3.2 Mutual exclusion: `priv` vs `pub`

```nova
priv pub x f64    // ❌ E_PRIV_PUB_CONFLICT
pub priv x f64    // ❌ E_PRIV_PUB_CONFLICT (detected at parser)
```

`pub` is reserved для explicit-public-override of a type-level
`priv` default (Plan 124.7 — type-level flip syntax `type X priv {}`).

### 3.3 Named tuples (Plan 124.4 / D222)

Per-field `priv` extends to named-tuple form (Plan 120 D215):

```nova
type Vec3(priv x f64, priv y f64, priv z f64)
type Account(priv balance f64, name str)      // mixed
type Secret(pub key str, priv salt []u8)      // explicit pub
```

### 3.4 Generic types (D220 §G1)

Uniform enforcement:

```nova
type Stack[T] {
    priv mut len int
    ro capacity int
}

export fn Stack[T] @push(x T) {
    @len = @len + 1     // ✅ inside method scope (recv = Stack)
}

// External:
mut s = Stack[int].new(10)
s.len = 0    // ❌ E_PRIV_FIELD_WRITE — uniform для всех T
```

---

## 4. Diagnostic codes (Plan 50 D102 format)

| Code | Site | When |
|---|---|---|
| `E_PRIV_FIELD_READ` | Member access на priv field outside scope | `acc.balance` |
| `E_PRIV_FIELD_WRITE` | Mutating assignment outside scope | `acc.balance = 0` |
| `E_PRIV_FIELD_INIT` | Record literal или named-tuple ctor outside scope | `Account { balance: 0 }` или `Vec3(x: 1.0)` |
| `E_PRIV_FIELD_PATTERN` | Pattern destructure outside scope | `Account { balance } = acc` |
| `E_PRIV_FIELD_INIT_SPREAD` | Record literal spread outside scope | `Account { ...other }` |
| `E_PRIV_PUB_CONFLICT` | Both `priv` and `pub` on same field | `priv pub x f64` |

Each diagnostic includes:
- Spec link (D220 / D221 / D222)
- Hint suggesting public method or factory
- Span on the violating site

---

## 5. Tooling

### 5.1 `nova doc`

Default — priv fields **hidden** from rendered docs:

```bash
$ nova doc src/account.nv
type Account { name str }    # balance hidden
```

Use `--include-private` to show all fields (with `priv` keyword
preserved in rendered signature):

```bash
$ nova doc src/account.nv --include-private
type Account { name str; priv mut balance f64 }
```

JSON output (`--format json`) emits `"priv_field": true` for each
priv field, regardless of `--include-private` — consumed by LSPs
and other tooling.

### 5.2 LSP (forward-ref)

When Plan 104.2 (hover) and Plan 104.3 (completion) land, they
will:
- Hide priv fields from autocomplete outside type-method scope.
- Show `🔒 priv` badge in hover popups.
- Display priv-field code-lens decorations.

The AST `RecordField.priv_field` and `NamedTupleField.priv_field`
flags (already exposed) are the data source. Plan 124.5 V1 wires
the doc layer; LSP integration follows once Plan 104.2/104.3 ship.

### 5.3 No reflection backdoor

Nova has zero reflection API (D6 managed GC + AOT codegen). priv
enforcement is **compile-time, hard guarantee**.

Compare with Java/Kotlin/C#/Swift, which all have reflection APIs
that bypass private (privileges aside) — Nova's guarantee is
stricter than any of them.

---

## 6. Comparison vs other languages

| Capability | Go | Rust | TS | Java | Swift | C# | **Nova** |
|---|---|---|---|---|---|---|---|
| Per-field privacy | ❌ (case-based) | ✅ `pub` | ✅ `private` | ✅ `private` | ✅ `private` | ✅ `private` | ✅ **`priv`** |
| Default visibility | pkg-priv if lowercase | mod-priv | public | package | internal | private | **public, opt-in priv** |
| Strict type-only scope | ❌ (pkg-wide) | ❌ (mod-wide) | ✅ (class) | ✅ (class) | ❌ (file/mod) | ✅ (class) | ✅ **type-method-only** |
| Reflection backdoor | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ **compile-time enforced** |
| Forced factory for priv-init | ❌ | ✅ `pub(...)` | ✅ | ✅ | ✅ | ✅ | ✅ **outside-scope blocked** |
| Tuple field privacy | ❌ | ✅ `struct(pub T)` | ❌ | ❌ | ❌ | ❌ | ✅ **named tuple priv** |

Nova matches or exceeds на 6/6 capabilities + 3 Nova-only superior
guarantees (strictest scope, no reflection, integrated с effect
system D2).

---

## 7. Migration

V1-V5 are **purely additive** — existing code (no `priv` modifier)
unchanged, compiles bit-identical to pre-Plan 124.

Plan 124.6 (test access escape) and 124.7 (type-level flip) add
opt-in features without breaking V1-V5 semantics. Edition flip
considered but rejected in favor of per-type `type X priv {}`
flip (kinder migration story).

---

## 8. Common patterns

### 8.1 Invariant-preserving setter

```nova
export type Account {
    ro id str
    priv mut balance f64
}

export fn Account mut @deposit(amount f64) -> () {
    assert(amount >= 0.0, "deposit must be non-negative")
    @balance = @balance + amount
}

export fn Account @balance_of() -> f64 => @balance
```

### 8.2 Cache via priv mut

```nova
export type ParseCache {
    ro source str
    priv mut last_parse Option[Ast]
}

export fn ParseCache mut @parse() -> Ast {
    if Some(a) = @last_parse {
        return a
    }
    ro a = do_parse(@source)
    @last_parse = Some(a)
    a
}
```

### 8.3 Sensitive data with priv ro

```nova
export type Session {
    ro user_id str
    priv ro token str          // immutable + private
}

export fn Session.from_login(uid str, t str) -> Session =>
    { user_id: uid, token: t }

// Token only used internally:
export fn Session @authorize(target_op str) -> bool =>
    verify_signature(@token, target_op)
```

---

## 9. See also

- `spec/decisions/02-types.md` — D220 / D221 / D222 (semantics)
- `spec/decisions/07-modules.md` — D47 (module-level pub vs per-field priv)
- `docs/plans/124-priv-field-visibility.md` — umbrella plan
- `docs/research/06-field-visibility-go-kubernetes.md` — empirical
  default-visibility study (kubernetes 11099 structs / 35239 fields).
