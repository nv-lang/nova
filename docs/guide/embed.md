**English** | [Русский](embed.ru.md)

# Embedding files and directories into the binary: `embed` / `embed_dir`

> A user guide to the compile-time intrinsics `embed("file")`
> ([D412](../../spec/decisions/03-syntax.md#d412), Plan 186) and `embed_dir("dir")`
> (D412 amendment in the same file, `spec/decisions/03-syntax.md` — search for
> "D412-амендмент", Plan 210). Both are class-C intrinsics (file input at
> compile time, with precedent in Rust `include_bytes!`, Go `//go:embed`,
> Zig `@embedFile`, C23 `#embed`).

## TL;DR

```nova
ro logo  = embed("assets/logo.png")     // []u8 — the content of ONE file
ro site  = embed_dir("../frontend")     // EmbeddedDir — the WHOLE directory, recursively

assert(site.len() == 3)
assert(site.has("index.html"))
ro index = site.get("index.html")       // Option[[]u8]
```

- The argument is **a string literal only** (the path must be known at
  compile time); it resolves relative to the calling `.nv` file, bounded by
  the caller's package root.
- The content becomes part of the binary's `.rodata`: payloads are
  **zero-copy** (a `ro`-binding is a view over static data, not the heap).
- Embedded files are build dependencies: changing/adding/removing any of
  them invalidates the incremental build cache.
- `embed` → `[]u8`. `embed_dir` → an immutable `EmbeddedDir` (a path→bytes
  map, sorted, binary search).

## Contents

- [`embed("path")` — a single file](#embedpath--a-single-file)
- [`embed_dir("dir")` — a whole directory, recursively](#embed_dirdir--a-whole-directory-recursively)
- [`EmbeddedDir` API](#embeddeddir-api)
- [Materialization: zero-copy](#materialization-zero-copy)
- [Determinism and sorting](#determinism-and-sorting)
- [dot-skip and symlink-skip](#dot-skip-and-symlink-skip)
- [Diagnostic codes](#diagnostic-codes)
- [NFC path normalization](#nfc-path-normalization)
- [rodata mine: don't mutate `data`](#rodata-mine-dont-mutate-data)
- [Interaction with multi-file codegen (Plan 209)](#interaction-with-multi-file-codegen-plan-209)
- [CRLF and `.gitattributes`](#crlf-and-gitattributes)
- [`ReadFs` — one code path for dev (disk) and prod (embedded)](#readfs--one-code-path-for-dev-disk-and-prod-embedded)
- [Cross-language comparison](#cross-language-comparison)
- [See also](#see-also)

---

## `embed("path")` — a single file

```nova
test "embed(\"path\") round-trips the fixture bytes exactly" {
    ro data = embed("d412_embed_fixture.bin")   // path — relative to THIS .nv file
    ro want = x"48 69 00 FF 7F"                 // holds a NUL and a byte > 0x7F — these are raw bytes
    assert(data.len() == want.len())
}
```

(from `spec_tests/conformance/d412_hex_blob_embed.nv`).

- The path resolves relative to the source file where the call sits — the
  same model as Rust's `include_bytes!`. Escaping the caller's package root
  (`..` past the root) is a compile error, not a runtime one.
- Pointing at a directory instead of a file gives `E_EMBED_IS_A_DIR` ("use
  `embed_dir(...)`") — symmetric to `embed_dir`'s `E_EMBED_NOT_A_DIR`.
- `embed`'s neighbor in D412 is the hex-blob literal `x"48 69 00 FF"` (same
  materialization; leading zeros are significant, `_`/space/newline
  separators are ignored, an odd digit count is `E_HEX_BLOB_ODD`).
  `embed(...)` is, in essence, "read a file and substitute its bytes as
  `x"…"`" at compile time.

## `embed_dir("dir")` — a whole directory, recursively

```nova
ro assets = embed_dir("d412d_dir")     // recursively: alpha.txt, beta.txt, nested/gamma.txt

assert(assets.len() == 3)                                   // .hidden doesn't count (dot-skip)
assert(assets.paths() == ["alpha.txt", "beta.txt", "nested/gamma.txt"])   // sorted
assert(assets.has("nested/gamma.txt"))                       // recursion — nested paths are also a POSIX key
assert(!assets.has(".hidden"))

ro alpha = assets.get("alpha.txt").unwrap()                  // bytes "ABC", a zero-copy view
assert(assets.get("./alpha.txt") == None)                    // key WITHOUT a leading `./` — exact byte form
```

(adapted from `spec_tests/conformance/d412d_embed_dir.nv` — the fixture
`d412d_dir/` contains `alpha.txt` ("ABC") / `beta.txt` ("XY") /
`nested/gamma.txt` ("WXYZ") / `.hidden`).

- The same argument contract as `embed`: a string literal, resolved
  relative to the calling `.nv` file, bounded by the package root; escaping
  it (the directory itself OR any file walked inside it) gives
  `E_EMBED_OUTSIDE_PROJECT`.
- **Recursive by default.** A glob/filter is out of scope (future); the
  whole subtree gets embedded.
- A path that points at a file, not a directory, gives `E_EMBED_NOT_A_DIR`
  ("use `embed(...)`"). A directory that doesn't exist gives
  `E_EMBED_DIR_NOT_FOUND`.
- **The entry key** is the path relative to the embed root, with a POSIX
  `/` separator (Windows `\` is converted to `/` while walking the disk),
  **case-sensitive**, without a leading `./`. `get`/`has` do not lexically
  normalize the argument: `..` is not simplified, `get("./x")` on an
  existing `x` genuinely returns `None`.
- A `\` (backslash) inside the argument's string literal itself (for both
  `embed` and `embed_dir`) gives `E_EMBED_PATH_BACKSLASH`: paths are
  written POSIX-style (`/`) regardless of the compiling OS (otherwise the
  source wouldn't be portable).
- An empty directory is legal → an empty `EmbeddedDir` (`len() == 0`).

## `EmbeddedDir` API

| Method | Signature | Semantics |
|---|---|---|
| `get` | `(path str) -> Option[[]u8]` | The file's bytes by exact key. O(log N) binary search over the sorted entries. `None` if the path doesn't exist — **not a panic** |
| `has` | `(path str) -> bool` | Whether a file exists at the path (`get(path).is_some()`) |
| `paths` | `() -> []str` | All embedded paths, in sorted deterministic order |
| `len` | `() -> int` | Number of embedded files |
| `entries` | `() -> ro []EmbeddedEntry` | `(path, data)` pairs without a double lookup — a `ro`-return (L2, read-only view, precedent `str @bytes()`): mutating the result is a compile error |

`EmbeddedEntry { path str, data []u8 }` — a single entry; publicly
constructible (carries no invariant of its own — also used in hand-written
tests/mocks).

**`EmbeddedDir` is fully immutable** — there are no mutating methods. The
only public constructor is `EmbeddedDir.new(entries)` (the same one the
compiler synthesizes for `embed_dir(...)`): it requires the entries to be
sorted and unique by `path` (UTF-8 byte order, same as `str.compare`);
violating this is a `panic`, not a silent miss in `get`. It's legal to
construct your own (non-embedded) directory in tests — the invariant is
still guarded by `verify`:

```nova
ro d = EmbeddedDir.new([
    EmbeddedEntry { path: "a.txt", data: a_bytes },
    EmbeddedEntry { path: "b.txt", data: b_bytes },
])
```

(`std/src/prelude/embed_test.nv` — constructs it by hand from before the
resolver could synthesize `embed_dir`; proves the type's contract
independently of the compiler.)

## Materialization: zero-copy

Both intrinsics are emitted in C as `static const uint8_t nova_blob_<hash>[]`
in `.rodata` — the same place as string literals (interned by content: two
identical files → one static, a hash collision → a `_seq` suffix).

- A **`ro`-binding** (`ro img = embed("logo.png")`) is **zero-copy**: `[]u8`
  with `data` pointing straight at the static, `len == cap == N`.
- A **`mut`-binding / consume into a mutation** copies into the GC heap at
  the point of binding (an ordinary `Vec` buffer from then on, `push` grows
  it as usual).
- The Boehm garbage collector ignores pointers outside its own heap — the
  static blob is never collected or moved.

`embed_dir("dir")` is rewritten by the compiler (the `embed_resolve` pass,
BEFORE type-check) into an ordinary Nova call:

```
EmbeddedDir.new([
    EmbeddedEntry { path: "app.js",     data: x"…" },   // sorted by path
    EmbeddedEntry { path: "index.html", data: x"…" },
])
```

— each `data` goes through the SAME `HexBlobLit` materialization as a
standalone `embed`: **zero changes in `emit_c.rs`**, only a directory walk +
AST synthesis in `embed_resolve.rs`. "Zero-copy" in the contract is about
**file payloads**; the `entries` table itself (headers + pointers) is a
small one-time GC allocation when the expression is evaluated, O(N),
negligible next to the file bytes.

**Tip:** bind `embed_dir(...)` **once** (in `main()`, not at module level —
until `[M-codegen-emission-nondeterminism]`(c) static-init topological
order is closed — and not on a hot path): calling it again rebuilds the
table from scratch. The same caveat applies to a standalone `embed` inside
a function body (a cheap re-creatable view, but still re-created).

## Determinism and sorting

`EmbeddedDir` entries are **sorted by key** — UTF-8 byte order, equivalent
to `str.compare` (D178, the precondition for binary search correctness).
Walking the filesystem is itself NOT deterministic across OSes — the
resolver sorts the result explicitly, so two builds (and builds on
different OSes) produce an identical entry order in the generated `.c`.

## dot-skip and symlink-skip

- **Hidden entries** (name starting with `.`) are skipped while walking.
  The rule applies to entries INSIDE the walk, not to the argument itself:
  `embed_dir(".assets")` (the root named explicitly) is embedded whole.
- **Symbolic links** (files and directories) are NOT followed, and are
  skipped with `W_EMBED_DIR_SYMLINK_SKIPPED` (protection against escaping
  through a link and against walk cycles).
- The directory exists, but there's nothing left to embed after the
  dot/symlink skip → `W_EMBED_DIR_EMPTY` — the typical symptom of
  "pointed at the wrong directory," not a hard error (an empty
  `EmbeddedDir` is legal).

## Diagnostic codes

| Code | Class | When |
|---|---|---|
| `E_EMBED_ARG_NOT_STR_LITERAL` | error | argument isn't a string literal / spread / named / arity ≠ 1 |
| `E_EMBED_NOT_FOUND` | error | (`embed`) file not found / not readable |
| `E_EMBED_IS_A_DIR` | error | (`embed`) path points at a directory — use `embed_dir` |
| `E_EMBED_DIR_NOT_FOUND` | error | (`embed_dir`) directory not found |
| `E_EMBED_NOT_A_DIR` | error | (`embed_dir`) path points at a file — use `embed` |
| `E_EMBED_OUTSIDE_PROJECT` | error | directory/file escapes the caller's package root |
| `E_EMBED_PATH_BACKSLASH` | error | `\` in the path's string literal (non-portable source) |
| `E_EMBED_DIR_NFC_COLLISION` | error | two distinct source names normalize to the same NFC key (see below) |
| `W_EMBED_DIR_SYMLINK_SKIPPED` | warning | a symlink was skipped while walking |
| `W_EMBED_DIR_LARGE` | warning | total size > 16 MiB or > 4096 files |
| `W_EMBED_DIR_EMPTY` | warning | directory is empty after the dot/symlink skip |
| `W_EMBED_DIR_NON_ASCII_PATH` | warning | non-ASCII file name (normalized to NFC — see below) |

Most codes are covered by a dedicated neg/standalone fixture in
`spec_tests/conformance/{neg,standalone}/d412d_*` (convention §116: each
file is its own compile unit with `EXPECT_COMPILE_ERROR`/`EXPECT_COMPILE_WARNING`).
The exception is `W_EMBED_DIR_SYMLINK_SKIPPED`: creating symlinks in a
cross-platform fixture is itself non-portable (requires privileges on
Windows), so this code doesn't yet have a dedicated test — the
`walk_embed_dir_rec` path in `compiler-codegen/src/embed_resolve.rs` is
covered only by code review, not by a fixture.

## NFC path normalization

**The problem:** macOS usually stores file names in NFD (decomposed form —
e.g. `é` as `e` plus a separate COMBINING ACUTE ACCENT U+0301 code point),
while Windows/Linux usually give NFC (precomposed form — `é` as a single
U+00E9 code point). The same git checkout on different OSes used to
produce DIFFERENT byte keys in the `embed_dir` table — and, correspondingly,
different generated `.c` for identical repository content.

**The fix (D412 amendment, Ф.6а):** every relative entry path is
normalized to **NFC** while walking. `get("café.txt")` with an ordinary
(precomposed) string literal in the source now finds the file regardless
of which form the filesystem physically stored the name in on disk:

```nova
// Fixture: d412d_dir_nfc_normalize/ contains ONE file whose name on disk is
// NFD ("cafe" + U+0301 COMBINING ACUTE ACCENT + ".txt").
test "embed_dir NFC-normalizes an on-disk NFD file name" {
    ro d = embed_dir("d412d_dir_nfc_normalize")
    assert(d.has("café.txt"))     // the literal here is NFC ("é" = U+00E9, one code point)
}
```

(`spec_tests/conformance/standalone/d412d_embed_dir_nfc_normalize.nv`.)

**A form collision** — if a directory contains TWO DISTINCT files at the
filesystem level (different name bytes — legal to coexist in the same
directory) whose NFC forms coincide (e.g. a precomposed `café.txt`
alongside a decomposed `café.txt`) — this is a **hard compile error**,
`E_EMBED_DIR_NFC_COLLISION`, not a silent overwrite of one entry by another
in the sorted table:

```nova
// EXPECT_COMPILE_ERROR E_EMBED_DIR_NFC_COLLISION
ro d = embed_dir("d412d_dir_nfc_collision")   // two files, one NFC form
```

(`spec_tests/conformance/neg/d412d_dir_nfc_collision_neg.nv`.)

`W_EMBED_DIR_NON_ASCII_PATH` (non-ASCII file name) remains — a non-ASCII
name is still worth the repository author's attention, but the warning
text now says the file is embedded under a NORMALIZED NFC key, not "as
is"; the form collision above is caught by a separate hard error.

**The implementation adds zero new Cargo dependencies.** Nova already
generates full Unicode 16.0 tables for `std.unicode.normalize_nfc`/
`str @to_nfc()` ([Plan 152.4](../plans/152.4-std-unicode.md), file
`std/src/unicode/norm_data.nv`, ~113 KB). The (Rust) compiler cannot call
this Nova function directly — it runs INSIDE the compiled program, and
`embed_resolve` runs BEFORE type-check, and there's no Nova interpreter in
the compiler. Instead of a new Cargo dependency (`unicode-normalization`
would add ~762 KB of source / ~128 KB of compressed `.crate` for
NFD+NFKD+CCC+quick-check+stream-safe data — an order of magnitude more than
needed) — `compiler-codegen/src/nfc.rs` parses the same
`NFD_DATA`/`CCC_DATA`/`COMP_DATA` (NFKD isn't needed for NFC — that's ~45 KB
of the 113 KB file) and repeats the SAME canonical-decompose →
canonical-order → canonical-compose algorithm (UAX #15, including the
algorithmic Hangul composition) as `std/src/unicode/normalize.nv`. One
canonical UCD version for the whole repository, zero added weight in the
compiler binary.

The bytes of a file's CONTENT (`data`) are untouched by normalization — it
only affects the `embed_dir` table's KEY (path); a standalone `embed(...)`
has no path table and so is unaffected altogether.

## rodata mine: don't mutate `data`

`data`/the result of `get(...)` is a **view over `.rodata`**, not a copy.
A `mut`-binding of the result followed by an in-place write (`d[0] = 5`) is
checker/runtime-level undefined behavior (writing to a read-only memory
page = SEGV on most platforms). D412's existing protection (copy on a
`mut`-binding) catches binding a blob **literal** directly
(`mut x = x"01 02"`), but NOT a value returned FROM a function/method
(`mut d = dir.get(p).unwrap()`) — this is a hazard inherited from
standalone `embed`, tracked as `[M-d412-blob-view-mut-write]` (backlog, P2,
home D412; out of scope for Plan 210).

**For mutating the content — an explicit copy:**

```nova
mut d = dir.get("config.json").unwrap().clone()   // now an ordinary GC buffer
d[0] = 0x7B                                        // legal — not .rodata
```

## Interaction with multi-file codegen (Plan 209)

The blob is rendered into `.c` as text — `0x%02X,` per byte
(`render_interned_blob_literals`, `emit_c.rs`) — meaning **≈×5.3 expansion**
relative to the original byte size (the textual representation of a byte is
longer than the byte itself). Two practical rules follow from this:

- **The `W_EMBED_DIR_LARGE` threshold** is 16 MiB total size or 4096 files,
  not the originally discussed 64 MiB: a 64 MiB payload would produce a
  ~340 MB `.c` file, which `clang` chokes on well before the build becomes
  merely inconvenient.
- **Multi-TU (`NOVA_MULTI_TU=1`, Plan 209):** blob statics are emitted into
  the prologue — in multi-TU that means `_common.h`, and EVERY `part` would
  recompile the whole array (the exact opposite of Plan 209's goal — cut
  duplicate compilation across `part`s). Blob definitions must go into ONE
  `part`, with only an `extern` declaration in `common`; a blob is an
  indivisible unit for `split_tu` (never cut across parts).

The future way out of text rendering is C23 `#embed` (Option E, out of
scope for Plan 210): a tiny `.c`, near-instant compilation; requires
`clang ≥19` (available via WSL-clang; native windows-clang/MSVC keeps the
hex fallback).

## CRLF and `.gitattributes`

Bytes are embedded AS-IS from the working copy on disk, with no line-ending
normalization. On a Windows checkout with `autocrlf=true`, text assets
(`.html`/`.css`/`.js`) byte-DIFFER from a Linux checkout of the same
commit → a different `.c` / a different fingerprint between OSes for
identical repository content. For cross-OS reproducibility — an explicit
`-text` (or `eol=lf`) in `.gitattributes` on the asset directories embedded
via `embed`/`embed_dir`.

## `ReadFs` — one code path for dev (disk) and prod (embedded)

A common case: serving a web server's static assets — from disk in dev mode
(live reload on file edits) and baked into the binary in prod. `ReadFs`
([D323 amendment](../../spec/decisions/04-effects.md#d323), `std.fs`, Plan 210
Ф.6б) is a read-only VFS protocol, conformed to by **both** sources:

```nova
import std.fs.{ReadFs, DirFs}

fn serve_assets[F ReadFs](mux mut ServeMux, assets F) -> () {
    mux.get("/{path...}", handler_fn(|req| {
        ro key = req.param("path").unwrap_or("index.html")
        match assets.read_file(key) {
            Ok(bytes) => ServerResponse.bytes(200, mime_of(key), bytes)
            Err(e)    => match e.kind {
                ErrorKind.NotFound => ServerResponse.empty(404)
                _                  => ServerResponse.empty(500)
            }
        }
    }))
}

fn main() {
    with Net = real_net(), Fs = real_fs() {
        mut mux = ServeMux.new()
        if dev_mode() {
            serve_assets(mut mux, DirFs.new("./frontend".to_path()))   // disk, live-reload
        } else {
            serve_assets(mut mux, embed_dir("../frontend"))                // baked into the binary
        }
        serve(mux, ":8080")
    }
}
```

`EmbeddedDir` conforms to `ReadFs` via **extension methods** (`std.fs`,
without touching the native `prelude.embed` API `@get`/`@has`/`@paths`);
`DirFs` is a wrapper over the real filesystem with a root prefix (the same
escape protection as `embed_dir`: a lexical `..` filter + a symlink-hard
`canonicalize`). The protocol is **effect-agnostic** (the `io.Read` model):
the `EmbeddedDir` conformer is pure, the `DirFs` conformer carries `Fs`, and
the effect propagates transitively at mono time. Nova doesn't support
effectful vtable dispatch, so the dev/prod choice is an `if` branch OVER
two mono instances (at the call site), not a runtime variable of one
`dyn`-type. Details, and why `list`/directory-index deliberately isn't part
of the protocol — [`docs/guide/io-fs.md`](io-fs.md#readfs--one-vfs-protocol-over-the-disk-and-an-embedded-directory)
and [Plan 210 §6б](../plans/210-embed-dir.md).

## Cross-language comparison

| Aspect | Nova | Go `//go:embed` | Rust `include_bytes!`/`include_dir!` | Zig `@embedFile` | C23 `#embed` |
|---|---|---|---|---|---|
| Single file | `embed("f")` → `[]u8` | `embed.FS` + `ReadFile` | `include_bytes!` → `&[u8]` | `@embedFile` → `[N]u8` | `#embed` → list of int |
| Whole directory | `embed_dir("d")` → `EmbeddedDir` | `//go:embed dir` + `embed.FS` | `include_dir!` (crate) | none built in | none |
| Recursion | yes, by default | yes | yes (crate) | — | — |
| Sorting/binary search | yes, sorted + O(log N) `get` | yes (`embed.FS`) | linear (crate) | — | — |
| Hidden files | skipped (`.`-prefix) | skipped (`.`/`_`-prefix) | configurable (crate) | — | — |
| Dev mode (reading from disk) | NO intrinsic substitution — explicit `DirFs` via `ReadFs` | no | `rust-embed` debug=disk (crate option) | — | — |
| NFC path normalization | yes (Ф.6а) + `E_EMBED_DIR_NFC_COLLISION` | no (silent) | no (silent) | — | — |
| Materialization | `.rodata`, zero-copy view | `.rodata`-like (Go binary) | `.rodata`, zero-copy | `.rodata` | `.rodata`, no hex-text bloat |

**What Nova takes:** from Go — recursion, sorting, binary search, dot-skip,
POSIX paths, case-sensitivity. From Rust's `rust-embed` — `.get(path) ->
Option`. **What Nova doesn't take:** dev mode (reading from disk in debug)
— a deliberate rejection (see [Plan 210 §2л](../plans/210-embed-dir.md)):
it would introduce an effect into a type that's pure by construction and
contradict the "one self-contained binary" goal; instead there's an
explicit `DirFs`/`ReadFs` opt-in (see above). **Nova goes further than
either reference** on NFC normalization: neither Go nor Rust addresses the
NFD/NFC cross-platform file-name-reproducibility trap at all.

## See also

- [D412](../../spec/decisions/03-syntax.md#d412) —
  the hex-blob literal `x"…"` + `embed("path")` (original decision, Plan 186).
- D412 amendment (`spec/decisions/03-syntax.md`, search for "D412-амендмент") —
  `embed_dir`, `EmbeddedDir`, diagnostic codes, including AMEND Ф.6а (NFC).
- [D323 amendment](../../spec/decisions/04-effects.md#d323) — `ReadFs` (Plan 210 Ф.6б).
- [Plan 210](../plans/210-embed-dir.md) — the full design/decision/risk map
  (exploration, materialization Option R′, review 1/2/3, Ф.6а/Ф.6б).
- [`docs/guide/io-fs.md`](io-fs.md) — `std.io`/`std.fs`/`std.os` overall, including
  `ReadFs`.
- [`std/src/prelude/embed.nv`](../../std/src/prelude/embed.nv) — the source of
  `EmbeddedDir`/`EmbeddedEntry`.
- [`std/src/fs/readfs.nv`](../../std/src/fs/readfs.nv) — the source of `ReadFs`/`DirFs`.
- [`spec_tests/conformance/d412_hex_blob_embed.nv`](../../spec_tests/conformance/d412_hex_blob_embed.nv),
  [`d412d_embed_dir.nv`](../../spec_tests/conformance/d412d_embed_dir.nv) — reference
  fixtures for both intrinsics.
