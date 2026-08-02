<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Migration: D249 string coordinate model (Plan 152.1)

The `str` API moved from flat codepoint-indexed methods (school B) to a **lens
model with byte coordinates** (D249/D250). The compiler points you at every site
with a targeted error + fix-it. This table is the full migration map.

## What changed

| Old (retired) | New | Why |
|---|---|---|
| `s.len()` | `s.byte_len()` (byte length, O(1)) **or** `s.as_chars().count()` (codepoints, O(n)) | `str` has three diverging lengths — pick the unit explicitly (`s.len()` was already bytes, so `byte_len()` is the drop-in) |
| `s.char_len()` | `s.as_chars().count()` | codepoint count is an O(n) lens operation |
| `s.char_at(i)` | `s.as_chars().nth(i)` → `Option[char]` | i-th codepoint, O(n) scan |
| `s.byte_at(i)` | `s.as_bytes()[i]` → `u8` | i-th byte, O(1), bounds-checked |
| `s.get(i)` (int) | `s.as_chars().nth(i)` | safe i-th codepoint |
| `s[i]` (int index) | `s.as_bytes()[i]` (byte) / `s.as_chars().nth(i)` (codepoint) / `s[a..b]` (slice) | codepoint-indexing UTF-8 is O(n) hiding behind `[i]` → banned (`E_STR_NO_INT_INDEX`) |
| `s[a..b]` (codepoint range) | `s[a..b]` (**byte** range) | byte offsets compose with `find` at O(1); fixes non-ASCII `split` |

## Kept / new

- `s.byte_len() -> int` — O(1) byte length (the only length method on `str`).
- `s.as_bytes() -> ro []u8` — byte lens (O(1) `[i]`/`len()`/iteration).
- `s.as_chars() -> CharsIter` — codepoint lens (stream): `next`/`count`/`nth`/`is_empty`.
- `s.to_bytes() -> []u8` / `s.to_chars() -> []char` — owned copies (`to_` = alloc).
- `s[a..b]` — byte-range zero-copy slice (panics OOB / on codepoint-boundary split).
- `s.get(a..b) -> Option[str]` — safe byte-range slice (None on OOB / codepoint split).
- `for c in s` — iterates `char` (codepoints), via `s.as_chars()`.

## Recipes

```nova
// length
ro nbytes = s.byte_len()              // was s.len()
ro nchars = s.as_chars().count()      // was s.char_len()

// element access
ro b = s.as_bytes()[i]                // was s.byte_at(i): u8
ro c = s.as_chars().nth(i)            // was s.char_at(i): Option[char]

// iterate codepoints
for c in s { ... }                    // for c in s.as_chars() { ... }

// iterate bytes
for b in s.as_bytes() { ... }

// find + slice (byte offsets compose at O(1))
match s.find("=") {
    Some(k) => ro rest = s[k+1..],    // byte-range slice
    None => ...,
}
```

## Convention: `as_` vs `to_`

`as_<repr>()` is a **lens** — a zero-copy view/iterator that borrows the source
(`as_bytes`, `as_chars`). `to_<repr>()` is an **owned copy** that allocates
(`to_bytes`, `to_chars`). Reach for `as_` unless you need an independent owned value.

## Errors you may hit

- **`E_STR_NO_INT_INDEX`** — `s[i]` with an integer. Use a lens or `s[a..b]`.
- **`E_STR_NO_LEN`** — `s.len()` / `s.char_len()` / `s.char_at()` / `s.byte_at()`.
  Use the fix-it the compiler prints.
