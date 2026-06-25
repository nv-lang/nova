<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Error handling in Nova — when to use what

> Source of truth: `protocols.nv:126-128`, D77, D25.

Nova has four error-handling tools, each for a distinct situation.

## panic — invariant violation (programmer bug)

Use when a contract the *caller* was required to satisfy has been broken. The program
cannot recover; continuing would give silently wrong results.

- Out-of-bounds access: `s.as_bytes()[i]` when `i >= s.byte_len()`.
- Codepoint-boundary violation: `s[a..b]` through the middle of a multibyte sequence.
- A `requires` contract that the compiler cannot statically eliminate.

**Never** use panic for external-input errors (bad user data, network, files) — those
are recoverable.

## Option — expected absence

Use when the caller routinely asks "did it work / was it there?" and absence is
not an error but a normal outcome.

```nova
s.find(needle) -> Option[int]        // needle may legitimately be absent
iter.next()    -> Option[char]       // exhaustion is expected
s.parse_int_opt() -> Option[int]    // caller only needs the value or nothing
```

`Option` signals: "I might not have an answer, and that is okay."

## Result — recoverable error

Use when failure has a cause the caller can inspect and handle.

```nova
str.parse_int()  -> Result[int, ParseIntError]   // Empty / InvalidDigit / Overflow
str.from_utf16() -> Result[str, Utf16Error]       // malformed surrogate pair
```

Convention (D325 / Plan 181 — Result-everywhere):
- **Plain name** (`parse_int`, `open`, `read_u32`) returns `Result[T, XError]` — every
  fallible public operation. No bare-throws twin, no `try_` duplicate, no `_opt`.
- **`try_` prefix** = ONLY to distinguish the fallible variant of a same-named *infallible*
  one (`from`/`try_from`, `into`/`try_into`, D77). Otherwise no prefix.
- **`.ok()`** converts `Result → Option`; `Option` itself is for genuine absence
  (`find`/`get`/`env`), not fallibility.
- **`!!`** throws at the call site, **`?`** propagates, **`match`** branches (D85). The
  `Fail[E]` effect stays in the language for your own code — std just doesn't expose its
  own errors through it.

## Lossy U+FFFD — best-effort decode

Use only in `*_lossy` functions and in `cps_to_str` for code points that escape the
Unicode scalar value range. Every invalid byte sequence is replaced with `U+FFFD
REPLACEMENT CHARACTER` — the output is always valid UTF-8.

```nova
str.from_bytes_lossy(bytes) -> str   // invalid UTF-8 → U+FFFD per bad sequence
cps_to_str(cps)             -> str   // cp > 0x10FFFF or surrogate → U+FFFD
```

**Never** return an empty string on failure — that is indistinguishable from an empty
input. If lossy substitution is not appropriate, return `Result` instead.
