---
source_rev: e5b206e36
source_date: 2026-07-26
---

> **Informative translation; the Russian text is normative.**

# Nova — type conversions

A consolidated page of all conversion rules in one place. Full
D-decisions: [D54](decisions/03-syntax.md#d54) (`as`),
[D52](decisions/02-types.md#d52) (newtype/alias/sum),
[D325](decisions/04-effects.md#d325) (the unified fallible std contract),
[D410](decisions/03-syntax.md#d410) (the `to_str`/`bytes` family),
[D429](decisions/02-types.md#d429) (`#coerce` — zero-cost implicit),
[D430](decisions/04-effects.md#d430) (checked narrowing `try_to_*`).
`From`/`Into`/`TryFrom`/`TryInto` as **protocols** were retracted
2026-07-06 ([D73](decisions/08-runtime.md#d73)/[D77](decisions/08-runtime.md#d77)) —
details in the "`from`/`try_from` naming" section below.

---

## The three mechanisms

| Mechanism | When | Example |
|---|---|---|
| `as` | infallible numeric/newtype/sum cast, compile-time, no runtime code | `42 as f64`, `n as i16` |
| `.to_str()` | universal conversion of a value **to a string** (bare-`T` blanket + specializations) | `42.to_str()`, `bs.to_str()` |
| `T.from(v)` / `T.try_from(v)` | a concrete static constructor — a **naming convention**, NOT a protocol/auto-derive | `Fahrenheit.from(c)`, `u32.try_from(port_str)` |
| `consume @into_ЦЕЛЬ()` | consuming ownership transfer (a concrete name on the source) | `sb.into_str()`, `wb.into_bytes()` |
| `#coerce` | declarative **implicit** zero-cost conversion in a position with a known expected type (view/finalize) | `w.write(s)` — `str` implicitly `.bytes()` |

**Important (2026-07-06 retraction, see below):** `.from(v)` / `.try_from(v)` —
this is a PAIR of concrete static methods on a concrete type, not a generic
`From[T]`/`TryFrom[T,E]` protocol. The compiler does **not** synthesize the
reverse form (`.into()`/`.try_into()`) automatically — the programmer writes
exactly what they declared. There is no "universal" `.into()` in the language
anymore.

---

## Numeric ↔ numeric

### Widening (no precision loss)

| From → To | Via | Semantics |
|---|---|---|
| `i8 → i16/i32/i64/int` | `as` | sign-extend |
| `u8 → u16/u32/u64/int` | `as` | zero-extend |
| `i8/u8 → f64` | `as` | exact (any int64 representable as f64) |
| `f32 → f64` | `as` | exact |

### Narrowing (potential precision loss)

| From → To | Via | Semantics |
|---|---|---|
| `i64 → i32/i16/i8` | `as` | wraparound (modulo 2^N) |
| `u64 → u32/u16/u8/byte` | `as` | wraparound |
| `f64 → f32` | `as` | IEEE rounding (precision loss) |
| **`f64/f32 → iN/uN`** | `as` | **saturation** + NaN→0 + ±∞→bounds |

**Float→int saturation** — defined behavior on any input (unlike C/C++ UB).
Consistent with Rust 1.45+.

```nova
ro n = 1e20 as int             // saturates to INT64_MAX
ro m = (-1.0) as u32           // saturates to 0
ro nan = 0.0 / 0.0 as i16      // 0
```

### Checked narrowing — `try_to_*` ([D430](decisions/04-effects.md#d430), 2026-07-20)

`as` between integer widths is always wraparound (silent loss of high bits).
If you need a **check** instead of silent wrap — a bounded blanket
`@try_to_<T>()` on any type from the `Ints` set, symmetric for all target
widths (`i8`/`i16`/`i32`/`i64`/`int`/`u8`/`u16`/`u32`/`u64`/`uint`):

```nova
ro ok = (100 as u32).try_to_u8()       // Ok(100 as u8)
ro err = (300 as u32).try_to_u8()      // Err(RangeError) — didn't fit
ro neg = (-1 as i32).try_to_u8()       // Err(RangeError) — negative → unsigned
```

`RangeError` — a unit type ("didn't fit", no payload — the fact itself is
exhaustive). `as` remains the fast truncating cast, unchanged — `try_to_*`
does not replace it, but adds a checked alternative alongside.

---

## Numeric ↔ str

### str → numeric (parse, fallible) — a method ON THE SOURCE, not a static on the target

**Canon (Plan 174.1, 2026-07-08, owner decision — superseded the early
static-constructor design `T.parse(s)`/`T.try_from(s)`):** converting a string
to a number is a method **on `str`** (`s.to_int()`), not a static constructor
on the target type. Mirrors the `s.to_str()` family in reverse.

| From → To | Via | Failure |
|---|---|---|
| `str → int` | `s.to_int(radix: int = 10)` | non-digit / overflow / (custom radix) invalid radix |
| `str → i64/u64` | `s.to_i64()` / `s.to_u64()` | no extra range-check (same width as the engine) |
| `str → i8/i16/i32/u8/u16/u32` | `s.to_i8()` / `s.to_i16()` / `s.to_i32()` / `s.to_u8()` / `s.to_u16()` / `s.to_u32()` | + range-check into the target width |
| `str → f64` | `s.to_f64()` | invalid number format |

```nova
fn parse_decimal(s str) -> Result[int, ParseIntError] =>
    Ok(s.to_int()?)             // radix 10 по умолчанию, Ok(42)

fn parse_hex(s str) -> Result[u32, ParseIntError] =>
    Ok(s.to_u32(radix: 16)?)    // hex-парсинг

fn parse_decimal_f64(s str) -> Result[f64, ParseFloatError] =>
    Ok(s.to_f64()?)             // Ok(3.14)
```

Errors — structural enums: `type ParseIntError enum Empty | InvalidDigit
| Overflow | InvalidRadix` and `type ParseFloatError enum Empty | Invalid`
(`std/runtime/string/parse.nv`).

### str → bool (parse, fallible)

**Canon (Plan 232.1 Т1, owner decision "add", 2026-07-26):**
`s.to_bool()` — strictly `"true"`/`"false"`, lowercase-only (the Rust
`str::parse::<bool>` canon; no case-insensitive/`"1"`/`"0"`/`"yes"` aliases).

| From → To | Via | Failure |
|---|---|---|
| `str → bool` | `s.to_bool()` | empty → `Err(Empty)`; anything other than exactly `"true"`/`"false"` → `Err(Invalid)` |

```nova
fn parse_flag(s str) -> Result[bool, ParseBoolError] => s.to_bool()

assert("true".to_bool() == Ok(true))
assert("TRUE".to_bool().is_err())      // регистр не lowercase → Err(Invalid)
```

`type ParseBoolError enum Empty | Invalid` (`std/runtime/string/parse.nv`)
— the same two-variant pattern as `ParseFloatError`.

### numeric → str (format, infallible) — a single entry point `.to_str()`

**Canon (Plan 174.2, 2026-07-14):** `str.from(scalar)` was **retracted**.
The only public entry point "value → string" is the bare-`T` blanket
`fn[T] T @to_str() -> str => "${@}"` ([D410](decisions/03-syntax.md#d410)
amend), specialized by concrete overloads where a different
arity/semantics is needed (e.g. decode for `[]u8`, see below).

| From → To | Via |
|---|---|
| `int/iN/uN → str` | `n.to_str()` |
| `f64/f32 → str` | `f.to_str()` |
| `bool → str` | `b.to_str()` |
| `char → str` | `c.to_str()` |

```nova
ro s = 42.to_str()             // "42"
ro f = 3.14.to_str()           // "3.14"
```

Interpolation (`"${n}"`) lowers into the same path directly (for primitives —
into a Display helper at the C level, without re-calling `.to_str()` — no
recursion).

---

## Char / Byte / []byte / str

### char → str (UTF-8 encode)

| Via | Semantics |
|---|---|
| `c.to_str()` | infallible UTF-8 encode (1-4 bytes) — a specialization of the `to_str()` blanket, byte-identical to the former `str.from(char)` |

### str → char (single codepoint, fallible)

**Canon (Plan 232.1 Т1, owner decision "add", 2026-07-26):**
`s.to_char()` parses EXACTLY one Unicode codepoint (not a byte — `"é".to_char()`
succeeds, even though `é` is 2 UTF-8 bytes). A receiver form on the source,
the same principle as `str @to_int()`.

| Via | Failure |
|---|---|
| `s.to_char() -> Result[char, ParseCharError]` | empty → `Err(Empty)`; >1 codepoint → `Err(TooManyChars)` |

```nova
assert("a".to_char() == Ok('a'))
assert("ab".to_char() == Err(TooManyChars))    // строгий отказ, не first-char silently
```

`type ParseCharError enum Empty | TooManyChars` (`std/runtime/string/parse.nv`)
— does **NOT** reuse `CharFromError` (see the "int → char" section below): that
domain is a codepoint outside the Unicode scalar value range/surrogates,
unreachable for str→char (the bytes of a `str` are already valid UTF-8, R-UTF8).

### int → char (codepoint range-check, fallible)

**Canon (owner, 2026-07-09):** a receiver form on the **source**
(`(cp int).to_char()`), not a static `char.try_from(n)` — the same chaining
principle as `str @to_int()`: `(32 + off).to_char()?`.

| Via | Failure |
|---|---|
| `(cp int).to_char() -> Result[char, CharFromError]` | `cp < 0` / `cp > 0x10FFFF` / surrogate `[0xD800, 0xDFFF]` |

```nova
fn describe(cp int) -> str =>
    match cp.to_char() {
        Ok(c)              => "codepoint ${cp} = '${c}'"
        Err(CharFromError) => "codepoint ${cp} вне диапазона"
    }
```

### char → byte (only if codepoint < 256, fallible)

This pair **stayed a static form** (did not migrate to a receiver) — the only
case where `try_` remained on the target type:

| Via | Failure |
|---|---|
| `u8.try_from(c char) -> Result[u8, TryFromCharError]` | codepoint > 0xFF (not Latin-1) |

**Exception:** `'A' as byte`, `'A' as int`, `'A' as u8` — allowed
for char literals (compile-time-known codepoint), see D54.

### []byte ↔ str — the unified `to_str` family (D325/174.1)

**Canon:** `[]u8` decode also goes through `to_str()` — a concrete
overload (arity/semantics of decode, not format) beats the bare-`T` blanket
by the "concrete beats generic" rule ([D84](decisions/10-overloading.md#d84)).
`str.try_from([]u8)` / the separate `str.from_bytes(...)` — historical
names, **withdrawn**, only the forms below are current:

| Form | Type | Semantics |
|---|---|---|
| `bs.to_str()` | `-> Result[str, Utf8Error]` | checked decode; `Utf8Error{byte_offset}` points at the first invalid byte |
| `bs.to_str_lossy()` | `-> str` | infallible, invalid sequences are replaced with a replacement character |
| `unsafe { bs.to_str_unchecked() }` | `-> str` | unchecked, the caller guarantees valid UTF-8 |
| `unsafe { bs.consume.into_str_unchecked() }` | `-> str` | as above, but a consuming zero-copy move of the buffer |

```nova
fn decode(bytes []u8) -> str =>
    match bytes.to_str() {
        Ok(s)                        => s
        Err(Utf8Error{byte_offset})  => "invalid UTF-8 at ${byte_offset}"
    }
```

**str → []byte** (view, infallible, zero-copy) — a bare view, not a
transformation: `s.bytes() -> ro []u8` ([D410](decisions/03-syntax.md#d410) —
`as_bytes` was renamed to `bytes`; this same name is the first declared
`#coerce` pair, see the "Zero-cost implicit conversions" section below).

---

## Bool ↔ everything

| From → To | Via | Semantics |
|---|---|---|
| `bool → int` | `as` | `true=1`, `false=0` |
| `bool → byte` / `bool → f64` | `as` | the same |
| `bool → str` | `b.to_str()` | `"true"` / `"false"` |
| **`int/byte/f64/etc → bool`** | **forbidden** | use `n != 0` |

```nova
ro s = true.to_str()           // "true"
ro n = 5
ro ok = if n != 0 { true } else { false }   // explicit != 0, не truthy-int
```

str → bool — see the TODO above (not found in std as of this revision).

---

## Newtype ↔ underlying

A newtype (`type X Y`, without `alias`, [D52](decisions/02-types.md#d52)) —
a type **separate** from the source; conversion is an explicit `as` (identity,
same C-repr). This differs from `alias` (`type X alias Y`) — there `X` and `Y`
are interchangeable **without any cast** (not a separate type).

| Via | Semantics |
|---|---|
| `n as MyNewtype` | identity (same C representation) |
| `nt as int` | identity |

```nova
type UserId int
ro u UserId = 42 as UserId
ro n int = u as int            // 42
```

---

## Sum-variant ↔ int (discriminant)

A sum type requires the `enum` marker after the name ([D406](decisions/02-types.md#d406),
2026-07-01 — the old syntax with a leading `|` without `enum` is revoked):

```nova
type ErrorCode enum NotFound = 404 | InternalError = 500
ro code = NotFound as int      // 404
```

`int → Sum` via `as` is **forbidden** (a number may not hit any variant).
Use pattern matching.

---

## Strict if cond:bool / while cond:bool

`if cond`, `while cond`, `cond1 && cond2`, `cond1 || cond2` —
**cond must be `bool`**. Truthy-int (`if a` where `a: int`)
is forbidden.

```nova
ro n int = 5
if n { ... }                    // ❌ compile error
if n != 0 { ... }               // ✅
```

**Precedents:** Rust, Swift, Kotlin — all require bool. Python/C/JS —
truthy, a known bug-class.
