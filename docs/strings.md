<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Strings in Nova — the lens model

> Plan 152.1 (D249/D250). `str` is a thin "piece of text"; you work through
> **representation lenses**, and coordinates are **byte-based**. Cost is always
> visible — there is no hidden O(n) under `[i]` or `len`.

## The model

`str` stores UTF-8 as `(ptr *ro u8, len int)` and is **always valid UTF-8**
(invariant R-UTF8). It is immutable. You don't index or measure `str` directly —
you pick a **lens**:

```
                          str  (thin: identity, slice s[a..b], search→byte-offset)
        as_bytes() ▼                              as_chars() ▼
   ro []u8  (Vec[u8] view)                   CharsIter  (decoding stream)
   O(1) [i] / len() / slice / iterate        next / count / nth / is_empty — O(n)
   ── byte layer (u8) ──                      ── codepoint layer (char) ──

        as_graphemes() ▼   (opt-in: import std.unicode)
   GraphemesView  (UAX #29 cluster stream)
   next / count / is_empty — O(n);  no [i]
   ── grapheme layer (visible "character", a str slice) ──
```

- **`as_bytes()` is a reinterpretation** — the bytes physically lie contiguously,
  so it's a real `ro []u8` with O(1) `[i]`/`len()`. Zero-copy.
- **`as_chars()` is a decoding lens** — codepoints are computed on the fly, so it's
  a *stream* (iterator), not a collection: `count()`/`nth(i)` are O(n), and there is
  deliberately **no positional `at(i)`/`len()`** (that would invite `for i in
  0..len { at(i) }` = O(n²)). Mirrors Rust `str::chars()`.
- **`as_graphemes()` is the user-perceived lens** — extended grapheme clusters
  (UAX #29): what a human sees as one character even when it spans several
  codepoints (`é` = `e`+◌́; `🇺🇸` = 2 regional indicators; `👨‍👩‍👧` = a ZWJ-emoji
  sequence — each is **one** grapheme). Also a *stream* (`next`/`count`/`is_empty`,
  O(n), no `[i]`). It is **opt-in** (`import std.unicode`) because it needs Unicode
  tables — the byte/codepoint layers above stay table-free. See
  [Unicode operations](#unicode-operations-opt-in-stdunicode) below.

## Length

| You want | Use | Cost |
|---|---|---|
| byte length | `s.byte_len()` | O(1) |
| codepoint count | `s.as_chars().count()` | O(n) |
| grapheme count | `s.as_graphemes().count()` (`import std.unicode`) | O(n) |

There is **no bare `s.len()`** — `str` has three diverging lengths (bytes,
codepoints, graphemes), so the unit is always explicit. `s.len()` → `E_STR_NO_LEN`.

## Element access

| You want | Use | Cost |
|---|---|---|
| i-th byte | `s.as_bytes()[i]` → `u8` (panics OOB) | O(1) |
| i-th codepoint | `s.as_chars().nth(i)` → `Option[char]` | O(n) |

There is **no `s[i]`** integer index — codepoint-indexing UTF-8 is O(n) hiding
behind `[i]`. `s[i]` → `E_STR_NO_INT_INDEX`.

## Slicing

`s[a..b]` is a **byte-range** zero-copy view (shares the buffer). The bounds are a
`requires` contract (zero-cost when the compiler can prove them, Plan 140.2);
slicing through a codepoint boundary panics (it would break R-UTF8). The safe,
non-panicking form is `s.get(a..b) -> Option[str]` (None on OOB / codepoint split).

```nova
ro s = "héllo"           // byte_len 6: h(0) é(1,2) l(3) l(4) o(5)
ro head = s[0..1]        // "h"
ro e    = s[1..3]        // "é"  (the 2 bytes of é)
ro tail = s[3..]         // "llo"
// s[1..2] would panic — it cuts é in half
```

Byte offsets compose with search at O(1):

```nova
match s.find("=") {           // find returns a BYTE offset
    Some(k) => ro rest = s[k+1..],
    None => ...,
}
```

## Iteration

```nova
for c in s { ... }            // char (codepoints) — the default unit
for b in s.as_bytes() { ... } // u8 (bytes) — explicit
```

## Owned copies

`as_*` lenses borrow (zero-copy). For an independent owned value use `to_*`:
`s.to_bytes() -> []u8`, `s.to_chars() -> []char` (both allocate).

## Unicode operations (opt-in: `std/unicode`)

The core lenses above are **ASCII-complete and byte/codepoint-correct without any
Unicode tables**. Operations that need the Unicode Character Database live in a
separate `std/unicode` module you import explicitly — so a program that doesn't do
Unicode normalization/segmentation never pays for the tables (they are
range-encoded and lazily initialized, pinned to `UNICODE_VERSION`, generated from
the official UCD by `nova-codegen unicode`; no ICU / OS dependency).

### Normalization (UAX #15)

```nova
import std.unicode

ro a = "e\u{301}"            // "e" + combining acute
ro b = "é"                   // precomposed U+00E9
assert(normalize_nfc(a) == normalize_nfc(b))   // canonically equal
assert(normalize_nfkc("ﬁ") == "fi")            // compatibility fold of the ligature
```

- `normalize_nfc(s) -> str`, `normalize_nfd(s) -> str` — canonical (de)composition.
- `normalize_nfkc(s) -> str`, `normalize_nfkd(s) -> str` — compatibility forms.

Full UAX #15 algorithm (decomposition + canonical ordering by CCC + canonical
composition with the blocking rule + algorithmic Hangul) — verified against the
official `NormalizationTest.txt`.

### Grapheme clusters (UAX #29)

`str.@as_graphemes() -> GraphemesView` is the third lens — iterate over
user-perceived characters:

```nova
import std.unicode

assert("é".as_graphemes().count() == 1)        // e + combining acute → 1
assert("🇺🇸".as_graphemes().count() == 1)        // 2 regional indicators → 1 flag
assert("👨‍👩‍👧".as_graphemes().count() == 1)        // ZWJ-emoji family → 1

for g in "a🇺🇸b".as_graphemes() {              // g is a str slice of one cluster
    // "a", "🇺🇸", "b"
}
```

`GraphemesView` mirrors `CharsIter` (a value-record stream): `next() ->
Option[str]`, `count()`, `is_empty()`, O(n), no positional `[i]`. Implements the
extended grapheme cluster rules GB1–GB13 **plus GB9c** (Indic Conjunct Break,
Unicode 15.1) — verified against the official `GraphemeBreakTest.txt`.

### Case folding & Unicode case mapping

Locale-independent, multi-codepoint — the Unicode upgrade of core's ASCII-only
`str.to_upper()`/`to_lower()` (which stay ASCII so they need no tables):

```nova
import std.unicode

assert(fold_case("MASSE") == fold_case("masse"))   // caseless match
assert(fold_case("ß") == "ss")                      // full fold
assert(to_uppercase("straße") == "STRASSE")         // ß → SS (multi-cp)
assert(to_uppercase("ﬁle") == "FILE")               // ligature ﬁ → FI
assert(to_lowercase("ΟΔΟΣ") == "οδος")               // final Σ → ς, others → σ
```

- `fold_case(s)` — full case folding (UCD `CaseFolding` C+F) for caseless matching.
  Not normalization: for canonically-equivalent text, normalize first, then fold.
- `to_uppercase(s)` / `to_lowercase(s)` — full Unicode case mapping, including the
  **Final_Sigma** context rule (Greek Σ → ς word-finally, σ otherwise). No locale
  tailoring (Turkic/Lithuanian); title-casing needs word boundaries and is roadmap.

Locale collation (`Collator`, UCA/CLDR) remains **roadmap** (Plan 152.5b).

## Encoding interop (UTF-16 / code points)

For FFI / JS-interop / protocols, `import std.encoding.utf16` adds UTF-16 and raw
code-point conversions (not in prelude — these are interop concerns, not everyday
string ops):

- `s.encode_utf16() -> []u16` — UTF-16 code units (supplementary code points become
  surrogate pairs).
- `str.from_utf16(units []u16) -> Result[str, Utf16Error]` — checked decode; a lone or
  truncated surrogate is an `Err`, so the result is always valid UTF-8 (R-UTF8).
- `s.code_points() -> []int` — raw `int` code points (no `char` wrapper), same values as
  `as_chars()` cast to `int`.

`from_utf16(s.encode_utf16()) == Ok(s)` round-trips on ASCII, BMP and supplementary
(e.g. `"😀"`). Surrogate helpers (`is_high_surrogate`/`is_low_surrogate`/
`decode_surrogate_pair`) live in the same module.

## Where each operation lives

| Operation | Method | Notes |
|---|---|---|
| byte length | `str.byte_len()` | O(1), reads the `len` field |
| byte lens | `str.as_bytes() -> ro []u8` | O(1) `[i]`/`len()` |
| codepoint lens | `str.as_chars() -> CharsIter` | `next`/`count`/`nth`/`is_empty` |
| grapheme lens | `str.as_graphemes() -> GraphemesView` | `import std.unicode`; UAX #29 |
| normalization | `normalize_nfc/nfd/nfkc/nfkd(s)` | `import std.unicode`; UAX #15 |
| slice | `str[a..b]` / `str.get(a..b)` | byte-range, zero-copy |
| search | `find`/`rfind`/`contains`/`starts_with`/`ends_with` | byte offsets |
| split/trim/replace/pad/repeat/concat | `transform`/`search` | see std/runtime/string/ |
| owned bytes/chars | `to_bytes`/`to_chars` | alloc |
| UTF-16 / code points | `encode_utf16`/`from_utf16`/`code_points` | `import std.encoding.utf16` |
| identity | `==` / `compare` / `hash` / clone | content-based |

> Normalization (UAX #15) and grapheme segmentation (UAX #29) ship in the opt-in
> `std/unicode` module — see [Unicode operations](#unicode-operations-opt-in-stdunicode).
> Unicode case folding/mapping and locale collation remain Phase B (Plan 152.4.4 /
> 152.5b). The core lenses above are ASCII-complete and byte/codepoint-correct
> without any Unicode tables.
