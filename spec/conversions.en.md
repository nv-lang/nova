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
