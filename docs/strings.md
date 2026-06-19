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
   GraphemesIter  (UAX #29 cluster stream)
   next / count / is_empty — O(n);  no [i]
   ── grapheme layer (visible "character", a str slice) ──

        as_words() ▼   (opt-in: import std.unicode)
   WordsIter  (UAX #29 word segments; O(1) to create)
   next / count / is_empty;  no [i]
   ── word layer (words / spaces / punctuation, str slices) ──

        as_sentences() ▼   (opt-in: import std.unicode)
   SentencesIter  (UAX #29 sentence segments; O(1) to create)
   next / count / is_empty;  no [i]
   ── sentence layer (a sentence + its trailing whitespace, str slices) ──
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
- **`as_words()` is the word-segment lens** — UAX #29 word boundaries (`import
  std.unicode`); iterates words, whitespace and punctuation as `str` slices. O(1) to
  create (forward state-machine, lazy). Powers `to_titlecase`.
- **`as_sentences()` is the sentence-segment lens** — UAX #29 sentence boundaries
  (`import std.unicode`); iterates sentences (each with its trailing whitespace and
  terminator) as `str` slices. O(1) to create (forward state-machine, lazy). Note: the default UAX #29
  algorithm has **no abbreviation dictionary**, so `"Mr. Smith"` splits after `Mr.`
  (a capital letter after `.` is a boundary) — this is the spec's documented
  behaviour, not a bug.

## Length

| You want | Use | Cost |
|---|---|---|
| byte length | `s.byte_len()` | O(1) |
| codepoint count | `s.as_chars().count()` | O(n) |
| grapheme count | `s.as_graphemes().count()` (`import std.unicode`) | O(n) |
| word count | `s.as_words().count()` (`import std.unicode`) | O(n) |
| sentence count | `s.as_sentences().count()` (`import std.unicode`) | O(n) |

There is **no bare `s.len()`** — `str` has three diverging lengths (bytes,
codepoints, graphemes), so the unit is always explicit. `s.len()` → `E_STR_NO_LEN`.

## Element access

| You want | Use | Cost |
|---|---|---|
| i-th byte | `s.as_bytes()[i]` → `u8` (panics OOB) | O(1) |
| i-th codepoint | `s.as_chars().nth(i)` → `Option[char]` | O(n) |
| codepoint + byte offset | `s.as_chars().indices()` → `CharIndicesIter` → `Option[(int, char)]` | O(n) per step |

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

`str.@as_graphemes() -> GraphemesIter` is the third lens — iterate over
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

`GraphemesIter` mirrors `CharsIter` (a value-record stream): `next() ->
Option[str]`, `count()`, `is_empty()`, O(n), no positional `[i]`. Implements the
extended grapheme cluster rules GB1–GB13 **plus GB9c** (Indic Conjunct Break,
Unicode 15.1) — verified against the official `GraphemeBreakTest.txt`.

### Case folding & Unicode case mapping

Locale-independent, multi-codepoint. Convention: **bare `to_upper`/`to_lower` = Unicode
full mapping** (under `import std.unicode`; needs tables); **`_ascii_` suffix =
ASCII-only, table-free, always available** (`to_ascii_upper`/`to_ascii_lower` from
prelude). Call `s.to_upper()` without `import std.unicode` → compile error (E7320) —
the compiler will not silently fall back to ASCII.

```nova
import std.unicode

assert(fold_case("MASSE") == fold_case("masse"))   // caseless match
assert(fold_case("ß") == "ss")                      // full fold
assert("straße".to_upper() == "STRASSE")            // ß → SS (multi-cp)
assert("ﬁle".to_upper() == "FILE")                  // ligature ﬁ → FI
assert("ΟΔΟΣ".to_lower() == "οδος")                  // final Σ → ς, others → σ

// ASCII-only variants (always available, no import needed):
assert("hello".to_ascii_upper() == "HELLO")
assert("HELLO".to_ascii_lower() == "hello")
```

- `s.fold_case()` — full case folding (UCD `CaseFolding` C+F) for caseless matching.
  Not normalization: for canonically-equivalent text, normalize first, then fold.
- `s.to_upper()` / `s.to_lower()` — full Unicode case mapping, including the
  **Final_Sigma** context rule (Greek Σ → ς word-finally, σ otherwise). No locale
  tailoring (Turkic/Lithuanian). Require `import std.unicode`.
- `s.to_ascii_upper()` / `s.to_ascii_lower()` — ASCII-only (A–Z/a–z only); no tables;
  always available from prelude.

### Code-point (`char`) classification & case

The `char` type's ASCII methods (`is_ascii_alphabetic`, `to_digit`, `len_utf8`, … —
prelude, table-free) get Unicode-aware peers under `import std.unicode`. They are
**1:1 with the UCD** (not an ASCII approximation) and match Rust `char`:

```nova
import std.unicode

assert('Ω'.is_alphabetic())          // U+03A9 GREEK CAPITAL OMEGA (Lu)
assert('٣'.is_numeric())             // U+0663 ARABIC-INDIC DIGIT THREE (Nd)
assert('½'.is_numeric())             // U+00BD VULGAR FRACTION (No)
assert('\u{A0}'.is_whitespace())     // NO-BREAK SPACE (Zs)
assert('A'.general_category() == Lu) // import std.unicode.{Lu}
assert('ß'.to_uppercase() == "SS")   // multi-code-point → str (not one char)
assert('ﬁ'.to_uppercase() == "FI")   // ligature ﬁ → "FI"
```

- `@is_alphabetic` / `@is_numeric` / `@is_alphanumeric` / `@is_whitespace` /
  `@is_uppercase` / `@is_lowercase` / `@is_control` — binary predicates over the UCD.
- `@general_category() -> GeneralCategory` — the UCD General_Category (TR44, 30 values
  `Lu`…`Cn`); `Cn` (not assigned) for any code point absent from the UCD.
- `@to_uppercase() -> str` / `@to_lowercase() -> str` — full per-code-point case
  mapping. They return **`str`** (not a single `char`) because one code point can map
  to several (ß → `"SS"`, İ → `"i"` + ◌̇). Final_Sigma is a string-level rule, so a lone
  Σ lowercases to σ (the context-free answer).

These delegate to the `std/unicode` code-point tables (`category_data.nv`:
General_Category + Alphabetic + White_Space from UCD 16.0) and to the case maps
(`case_data.nv`). Like the lenses above, they are **opt-in** — without
`import std.unicode` the Unicode classification is not in scope (the ASCII-core `char`
methods stay prelude-available).

> **Method resolution:** `s.to_upper()` and `s.to_lower()` are defined only under
> `import std.unicode`. Without that import the names are unresolved → compile error
> `E7320`. There is no silent ASCII fallback. `s.to_ascii_upper()` /
> `s.to_ascii_lower()` are always available and are the correct choice when Unicode
> tables are not wanted.

### Word segmentation & title-casing (UAX #29)

`str.@as_words() -> WordsIter` is the fourth lens — iterate UAX #29 word segments
(words, whitespace and punctuation — every inter-boundary piece). O(1) to create
(forward state-machine, lazy — no eager boundary materialisation).

```nova
import std.unicode

assert("can't 3.14".as_words().count() == 3)         // "can't" | " " | "3.14"
assert(to_titlecase("hello world") == "Hello World") // first cased char per word
assert(to_titlecase("ﬁle") == "File")                // ﬁ → "Fi" (title mapping)
```

- `as_words()` / `WordsIter` — `next()`/`count()`/`is_empty()`, UAX #29 boundary
  rules WB1–WB16 (handles `can't`, `3.14`, regional-indicator flags, ZWJ-emoji).
- `to_titlecase(s)` — titlecases the first cased char of each word (using the
  **titlecase** mapping, e.g. ǆ → ǅ, not uppercase Ǆ) and lowercases the rest with
  Final_Sigma. Locale-independent.

### Sentence segmentation (UAX #29)

`str.@as_sentences() -> SentencesIter` is the fifth lens — iterate UAX #29 sentence
segments (each sentence together with its trailing whitespace and terminator). O(1) to
create (forward state-machine, lazy; SB8 lookahead is bounded per-segment, O(1)
amortised).

```nova
import std.unicode

assert("3.4".as_sentences().count() == 1)            // ATerm between digits (SB6)
assert("the resp. leaders are".as_sentences().count() == 1) // lowercase after "." (SB8)
{
    mut sv = "Hello! World".as_sentences()
    assert(sv.next() == Some("Hello! "))             // STerm + space + capital → split
    assert(sv.next() == Some("World"))
    assert(sv.next() == None)
}
```

- `as_sentences()` / `SentencesIter` — `next()`/`count()`/`is_empty()`, UAX #29
  boundary rules SB1–SB11 (+ SB998 default-no-break). Default UAX #29 has **no
  abbreviation dictionary**: `"Mr. Smith went home. He slept."` yields three
  segments (`"Mr. "`, `"Smith went home. "`, `"He slept."`), because a capital
  letter after `.` is a boundary. That is the documented spec behaviour.

### Collation (UCA / DUCET ordering)

`str`'s default `compare`/`<` is **byte-lexicographic** (fast, deterministic,
locale-independent). For Unicode-aware ordering, `import std.unicode` gives a UCA
(UTS #10) DUCET collator — `str` never collates silently (D254):

```nova
import std.unicode

assert(collate_compare("apple", "Apple") < 0)   // case is tertiary, not primary
assert(collate_compare("café", "cafe") > 0)      // accent is secondary
ro key = collate_sort_key("naïve")               // Vec[u32] sort key (cache for sorting)
ro r = Collator.order("a", "b")                  // Collator.order/key/same (DUCET namespace)
```

- `collate_compare(a,b) -> int` (-1/0/+1), `collate_sort_key(s) -> Vec[u32]`,
  `collate_eq`, `Collator.order/key/same` (bodyless namespace, no instance). Multi-level (primary/secondary/tertiary +
  quaternary) **Shifted** variable-weighting; NFD-normalizes first; handles
  contractions (incl. UCA S2.1 discontiguous) and implicit weights (CJK etc.).
- Scope: **DUCET (root, non-tailored)**. CLDR locale-tailoring + `eq_ignore_case` are
  roadmap (Plan 152.5b, `[M-152-collation-tailoring]`) — like Rust `unicode-collation`
  DUCET mode / ICU root collator.

> **Why free functions, not str methods?** String transforms (`trim_ascii`,
> `to_ascii_lower`, `to_upper`, etc.) are `str` methods because they fit the
> "transform this string" idiom. Collation (`collate_compare`, `collate_sort_key`,
> `Collator`) is intentionally **not** `str @compare`/`@equal` — collation must
> never silently replace the default byte-`Ord` (D254 design decision). The
> asymmetry is intentional, not an oversight.

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
| grapheme lens | `str.as_graphemes() -> GraphemesIter` | `import std.unicode`; UAX #29 |
| word lens | `str.as_words() -> WordsIter` | `import std.unicode`; UAX #29 |
| normalization | `normalize_nfc/nfd/nfkc/nfkd(s)` | `import std.unicode`; UAX #15 |
| case fold / map / title | `s.fold_case()`/`s.to_upper()`/`s.to_lower()`/`to_titlecase(s)` | `import std.unicode` |
| case (ASCII-only) | `s.to_ascii_upper()`/`s.to_ascii_lower()` | always available, no import |
| char classification (Unicode) | `c.is_alphabetic`/`is_numeric`/`is_whitespace`/`general_category` | `import std.unicode`; 1:1 UCD |
| char case (Unicode) | `c.to_uppercase()`/`to_lowercase() -> str` | `import std.unicode`; multi-cp |
| codepoint + byte-offset | `s.as_chars().indices() -> CharIndicesIter` | `next()->(int,char)` |
| slice | `str[a..b]` / `str.get(a..b)` | byte-range, zero-copy |
| search | `find`/`rfind`/`contains`/`starts_with`/`ends_with` | byte offsets |
| split/trim/replace/pad/repeat/concat | `transform`/`search` | see std/runtime/string/ |
| owned bytes/chars | `to_bytes`/`to_chars` | alloc |
| UTF-16 / code points | `encode_utf16`/`from_utf16`/`to_code_points` | `import std.encoding.utf16` |
| identity | `==` / `compare` / `hash` / clone | content-based (byte-`Ord`) |
| collation (UCA) | `collate_compare`/`collate_sort_key`/`Collator` | `import std.unicode`; DUCET/UTS #10 |

> Normalization (UAX #15) and grapheme segmentation (UAX #29) ship in the opt-in
> `std/unicode` module — see [Unicode operations](#unicode-operations-opt-in-stdunicode).
> The core lenses above are ASCII-complete and byte/codepoint-correct without any
> Unicode tables.

## Error policy

| Situation | Use | Example |
|---|---|---|
| Invariant violation (programmer bug), out-of-bounds | **panic** | `s.as_bytes()[i]` OOB; `s[a..b]` through codepoint boundary |
| Expected absence (not found, empty, index past end) | **`Option`** | `s.find(needle) -> Option[int]`; `iter.next() -> Option[char]` |
| Recoverable, external input error | **`Result`** | `str.try_parse_int() -> Result[int, ParseIntError]`; `str.from_utf16() -> Result[str, _]` |
| Best-effort decode of untrusted bytes | **lossy U+FFFD** | `str.from_bytes_lossy`; `cps_to_str` (invalid cp → `\u{FFFD}`) |

Rules (source: protocols.nv:126-128, D77, D25):
- **`parse_int(s)`** (bare) — throws `ParseIntError`; for explicit handling use `try_parse_int` (Result) or `parse_int_opt` (Option).
- **Never** return an empty string on failure — that is indistinguishable from an empty input. Use `Option`/`Result` instead.
- `*_lossy` functions always return valid UTF-8; they substitute `U+FFFD` for every invalid byte sequence, never silently drop bytes.

## Interpolation & format specs

Interpolation is `${expr}` (Display) / `${expr:?}` (Debug). A Rust-style format
spec follows the colon — `${expr:[[fill]align][sign][#][0][width][.precision][type]}`
(Plan 152.7-B, D258):

```nova
assert("${42:5}" == "   42")        // min width, right-aligned (numbers)
assert("${42:<5}" == "42   ")       // left align
assert("${42:*^7}" == "**42***")    // fill + center
assert("${42:05}" == "00042")       // zero-pad
assert("${255:x}" == "ff")          // hex; X=upper, b=binary, o=octal
assert("${255:#x}" == "0xff")       // # alternate radix prefix (always lowercase)
assert("${3.14159:.2}" == "3.14")   // precision (f64); for str = truncate
```

A malformed spec is a **compile error** (`E_FORMAT_SPEC_UNKNOWN` / `E_BAD_FORMAT_SPEC`),
never a silent pass. (Generalizing the formatter to write into any `Write` sink —
`@display(mut w Write)` — is roadmap, Plan 152.7.1.)
