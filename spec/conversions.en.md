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
