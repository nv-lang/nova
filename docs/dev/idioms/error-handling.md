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
m.get(key)     -> Option[V]          // missing key is a normal outcome, not an error
```

`Option` signals: "I might not have an answer, and that is okay." For a *fallible* op with
an inspectable cause (like `parse_int`), use `Result` and convert with `.ok()` if the caller
only wants presence — there is no `_opt` twin (D325 R4).

## Result — recoverable error

Use when failure has a cause the caller can inspect and handle.

```nova
str.parse_int()  -> Result[int, ParseIntError]   // Empty / InvalidDigit / Overflow
str.from_utf16() -> Result[str, Utf16Error]       // malformed surrogate pair
```

Convention (D325 / Plan 177 — Result-everywhere):
- **Plain name** (`parse_int`, `open`, `read_u32`) returns `Result[T, XError]` — every
  fallible public operation. No bare-throws twin, no `try_` duplicate, no `_opt`.
- **`try_` prefix** = ONLY to distinguish the fallible variant of a same-named *infallible*
  one (`from`/`try_from`, `into`/`try_into`, D77). Otherwise no prefix.
- **`.ok()`** converts `Result → Option`; `Option` itself is for genuine absence
  (`find`/`get`/`env`), not fallibility.
- **`!!`** throws at the call site, **`?`** propagates, **`match`** branches (D85). The
  `Fail[E]` effect stays in the language for your own code — std just doesn't expose its
  own errors through it.

### Cross-domain composition — `.map_err` or a domain sum-error

Auto-`From` error conversion at `?` is **rejected** (D85 amend 174.2): mixing domains in one
fn (`IoError` + `ParseIntError`) does not silently coerce. Two canonical patterns:

```nova
// (a) map each foreign error to the fn's domain at the call site:
fn load(path str) Fs -> Result[Config, ConfigError] {
    ro raw = Fs.read_text(path).map_err(|e| ConfigError.Io(e))?
    ro n   = raw.parse_int().map_err(|e| ConfigError.Parse(e))?
    Ok(Config.of(n))
}

// (b) or declare an explicit domain sum-error and let each site map into it (same shape).
type ConfigError enum Io(IoError) | Parse(ParseIntError)
```

### Wrapping a `Fail[E]` body into a `Result` — `runCatching`-style

To capture a throw-style body as a value (Kotlin `runCatching` / Swift `Result(catching:)`):

```nova
ro r Result[T, E] = with Fail[E] = |e| interrupt Err(e) { risky_body()!! ; Ok(v) }
```

The handler turns a thrown `E` into `Err(e)`; the normal path yields `Ok`. This is the inverse
of `!!` (which turns `Err` back into a throw).

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
