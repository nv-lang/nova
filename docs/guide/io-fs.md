# I/O, filesystem, and OS in Nova

> User-facing guide for `std.io`/`std.fs`/`std.os` (Plan 176). Model, cross-language
> comparison (7 languages), and the `write_atomic` durability recipe.

## TL;DR

```nova
// byte-first protocols, one shared error, mockable effects
type io.Read  protocol { @read(buf mut []u8) -> Result[int, IoError] }   // Ok(0) = EOF, only when buf is non-empty
type io.Write protocol { @write(data []u8) -> Result[int, IoError]; @flush() -> Result[(), IoError] }

with Fs = mem_fs() {                    // deterministic test, no disk
    consume f = File.create("out.txt".to_path())!!
    f.write("hi".bytes())!!
    // f un-closed at scope-exit -> compile error (D133); explicit @close() needed
    // for close-Result to reach a `Result`-flavored happy path
}
```

- **One structural `IoError{kind, raw_os, op}`** for io/fs/os (Rust `io::Error` precedent).
- **`File`/`BufWriter` are must-consume** (D133): forgetting `@close()` is a compile
  error, not a runtime leak — and a close-time error (`ENOSPC`, quota, …) can never be
  silently swallowed.
- **`Fs`/`Os`/`Io` are effects** — `mem_fs()`/`mock_os()`/`mock_io()` give deterministic
  tests without touching a real disk, environment, or console.
- **`str` is never the raw-I/O type.** All I/O is `[]u8`; `str` only enters through the
  fallible `str.from_bytes(bytes) -> Result[str, Utf8Error]` (Rust/Go/Zig model, not
  Node's silent `U+FFFD`).

## Model

### Byte-first

Nova's `str` is UTF-8-validated and immutable — it is not a byte buffer. Every raw I/O
surface (`io.Read`/`io.Write`, `Fs`, `Os` env values, `Net`) is `[]u8`. Text only enters
through an explicit, fallible decode (`str.from_bytes`) or a lossy one
(`str.from_bytes_lossy`) — never implicitly. The bridge from text back to bytes for a
byte sink is the explicit `write_str` (`std.io`), never an implicit `str`→`Write`
coupling (see [Protocols vs the text sink](#protocols-vs-the-text-sink) below).

### One error, not one per domain

```nova
type IoError value { ro kind ErrorKind, ro raw_os int, ro op str }
type ErrorKind enum
    | NotFound | PermissionDenied | AlreadyExists | NotADirectory | IsADirectory | DirectoryNotEmpty
    | WouldBlock | Interrupted | UnexpectedEof | WriteZero | InvalidInput | InvalidData
    | TimedOut | StorageFull | ReadOnlyFilesystem | CrossesDevices | BrokenPipe
    | ConnectionRefused | ConnectionReset | ConnectionAborted | NotConnected | AddrInUse | AddrNotAvailable
    | Unsupported | Other(int)   // OPEN enum -> a `match` needs a wildcard arm
```

`io`, `fs`, and `os` all return `Result[T, IoError]`. `kind` is the categorised,
matchable projection; `raw_os` is the exact errno/`GetLastError` (authoritative —
`kind` is best-effort, `raw_os` never lies); `op` names the failed call for
diagnostics. Rare/obscure errno values fall to `Other(raw_os)` rather than being
silently coerced into the nearest category.

**Why not per-operation error sets (Zig)?** Considered and rejected: an exact error
union per operation needs error-union infrastructure and fragments error handling
by call site. Nova takes the Rust model — one open `ErrorKind` composes through a
`match`, and (fs) a `source` chain carries the cause instead of a bespoke union per
function.

**`net` stays a separate type.** `NetError` (`std.net`) is not merged into `IoError`
— its own `#stable` `@to_str()` strings keep their exact wording. Instead it gets an
*additive* best-effort projection, `NetError.@to_error_kind() -> ErrorKind` /
`@to_io_error(op) -> IoError`, used to give `TcpStream.@read`/`@write` structural
`io.Read`/`io.Write` conformance (so a `TcpStream` and a `File` both satisfy a
generic `[R Read]`/`[W Write]` bound) without disturbing every other `Net`-effect
call site (`write_all`/`read_to_vec`/the split halves keep `NetError` unchanged).

### Must-consume `File`/`BufWriter` (D133)

```nova
type File consume { … }
fn File @close(consume self) -> Result[(), IoError]   // the ONLY explicit discharge
```

An un-closed `File` or `BufWriter` at scope-exit is `D133-not-consumed` — a
**compile** error, not a runtime leak, and double-close is impossible by
construction. `consume f = open(…) { … }` discharges through `@cleanup` on
scope-exit (an error there joins the suppressed-chain, never lost); explicit
`@close()` is the `Result`-path when the close error must reach the happy path.
This is the single biggest differentiator over every peer in the table below —
none of them make "forgot to close" or "swallowed close-error" impossible to write.

### Mockable effects

`Fs`/`Os` are plumbing effects (libuv-backed under `real_fs()`/`real_os()`; the user
never calls them directly, only the `File`/env/etc. methods built on top — same
shape as `std.net`'s `Net`). `mem_fs()`/`mock_os()`/`mock_io()` give an in-memory
handler for deterministic tests: no disk, no environment mutation, no console,
including error injection (`ENOSPC`/`EIO` on `mem_fs()`, for close-error and
torn-write tests).

### EOF / partial / EINTR

- `read()` → `Ok(0)` signals EOF **only when the buffer is non-empty** — never Go's
  `(n > 0, io.EOF)` footgun where a read can carry both data and an EOF signal.
- A short read (`0 < n < len`) is normal, not EOF; a partial write is legal
  (`write_all` loops until done). `Ok(0)` mid-write → `WriteZero`.
- `Interrupted` (EINTR) is retried automatically inside every loop helper
  (`read_exact`/`read_to_end`/`read_to_string`/`write_all`).

### `write_atomic` — actually durable, not just atomic-looking

```
1. create a temp file in the SAME directory (O_EXCL)
2. write_all the data
3. fsync the temp file        (sync_all)
4. atomic rename/replace over the target
5. best-effort fsync of the parent directory (no-op on Windows)
```

A plain `write` returning `Ok` is **not** durable without steps 3 and 5 — the data
can still be lost on power loss even though the rename already happened.

> **Anti-precedent.** Swift's `.atomic` write option and Zig's `AtomicFile` do steps
> 1/2/4 only — temp file + rename, no `fsync`. That is atomic *against readers*
> (nobody ever observes a half-written file) but **not durable against power
> loss**: the rename can be reordered before the data hits disk by the filesystem's
> own journal, and a crash between rename and the eventual flush can leave the
> "new" file empty or truncated. Nova's `write_atomic` always does the full
> 5-step recipe; there is no shortcut variant.

### Protocols vs the text sink

`io.Read`/`io.Write` are **byte** protocols, module-qualified (`import std.io`) and
deliberately a **sibling** of the prelude text-sink `Write` (`@display`/`Debug`
formatting, D258/D374). Merging them would import text semantics into byte I/O —
the exact confusion Java's `Writer` vs `OutputStream` split exists to avoid. The
bridge is the explicit `write_str(w, s)`.

### `ReadFs` — one VFS protocol over the disk and an embedded directory

`ReadFs` (`std.fs`, D323 amendment, Plan 210 Ф.6б) is a read-only virtual
filesystem — `@read_file(path) -> Result[[]u8, IoError]` +
`@path_exists(path) -> Result[bool, IoError]` — conformed by **`DirFs`** (a
root-scoped view over the real disk, `Fs` effect) and by **`EmbeddedDir`**
(the `embed_dir("dir")` result, pure). The classic "dev serves from disk with
live-reload, prod serves the binary-embedded copy" case becomes one generic
function, `fn serve[F ReadFs](assets F, ...)`, mono'd twice — no runtime `dyn`
switch, because Nova has no effectful-vtable dispatch (D122 amendment) to carry
`DirFs`'s `Fs` effect through an existential `ReadFs` value. The branch between
`DirFs`/`EmbeddedDir` lives at the call site (which mono to instantiate), not
in a variable. `EmbeddedDir`'s conformance is an **extension method** (D287,
declared in `std.fs`, not in `EmbeddedDir`'s home module `prelude.embed`) —
structural conformance through a generic `[F ReadFs]` bound sees it exactly
like an inherent one (`std/src/fs/readfs_test.nv`). `list`/directory-index is
deliberately **not** in the protocol (a real-FS scan is effectful, expensive,
and non-deterministic where the embedded side is free and stable) — see
[`docs/plans/210-embed-dir.md`](plans/210-embed-dir.md) §6б for the full design.

## Cross-language comparison (7 languages)

| Aspect | Go | Rust | TS/Node | Kotlin | Java | Zig | Swift |
|---|---|---|---|---|---|---|---|
| io abstraction | `io.Reader/Writer` | `Read/Write/Seek` + `BufReader` | `stream.*` | okio/java | `InputStream`/nio | `std.Io` Reader/Writer (0.14+) | `FileHandle`/swift-system `FileDescriptor` |
| close | `defer Close()` (**err ignored**) | `Drop` (**swallows**) | `await close()` | `use{}` (suppressed) | try-with-resources (suppressed) | `close()->void` (**nowhere to put an err**) | `close() throws` (easy to forget) |
| path | `string` | `Path`/`OsStr` (bytes) | string | nio.Path | nio.Path (**`InvalidPathException`**) | `[]const u8` (bytes) | `FilePath` (bytes, swift-system) |
| error | sentinels | **`io::Error{ErrorKind}`** | `err.code` string | `IOException` | hierarchy | **per-op error sets** | typed `Errno` |
| EOF/partial | `(n>0, io.EOF)` (**footgun**) | `Ok(0)`=EOF, `read_exact` | promise | — | partial-read | `0`=EOF / error sets | data-based |
| atomic write | manual | manual | manual | manual | `ATOMIC_MOVE` | `AtomicFile` (**no fsync**) | `.atomic` (**no fsync**) |
| TOCTOU | — | — | — | — | — | **🏆 dir-scoped ops (openat by design)** | — |
| async | goroutine | sync/`tokio::fs` (pool) | libuv | suspend | NIO | sync/evented | actors |

**Nova takes:** Rust's `ErrorKind`/`OpenOptions`/`create_new`/`Path`/`read_at`-`write_at`/
`Ok(0)=EOF`; Go's `ReadFile`/`WriteFile` ergonomics + portable path joins; Java's
`ATOMIC_MOVE`; Swift-system's byte-backed `FilePath` precedent.

**Nova avoids:** Go's silent-swallow close + `(n>0, EOF)`; Rust's `Drop`-swallow;
Node's silent `U+FFFD` replacement; Java's `InvalidPathException`; Swift/Zig's
atomic-without-fsync; Zig's `close()->void`.

**Followup, not yet shipped:** Zig's dir-scoped `openat`/`unlinkat` model
(anti-TOCTOU by construction) — tracked as `[M-176-dir-scoped-ops]`; current
`remove_dir_all` is plain path-based recursion.

## Where Nova beats every peer

- **Must-consume `File`/`BufWriter` (D133).** `@close()` is the only explicit
  discharge; an un-closed handle is a compile error, and a close-time error can
  never be silently dropped. Beats all 7: Go's `defer Close()` and Rust's `Drop`
  both swallow; Java/Kotlin suppress on the error path; Node's `await using`
  swallows; Zig's `close()` returns `void` (nowhere to put the error) and its
  `Io.Writer` needs a manual `flush()` (discipline, not the compiler); Swift's
  `close() throws` — but forgetting to call it compiles fine.
- **Mockable `Fs`/`Os`/`Io`.** `with Fs = mem_fs() { … }` — deterministic test, no
  disk, no DI framework. Go needs `afero`, Rust a trait abstraction, Java/Node
  monkey-patching, Zig has no story at all (manual DI), Swift needs a
  protocol-viral `FileManager` mock.
- **Byte-first done right.** `str` is UTF-8-validated; `read_to_string` is
  *fallible* (`Result`, not Node's silent `U+FFFD` corruption). Zig-parity on
  byte-slices, but Zig has no validated string type to fall back on.
- **Typed `Timestamp`** (Plan 175) for mtime/atime/ctime, each an
  `Option[Timestamp]` — beats Node's Date/ms/ns triplet, Go's `Sys() any`, Zig's
  bare `i128` nanoseconds.
- **Structural `IoError{kind, raw_os, op}`** with an exhaustive (wildcard-forced)
  `ErrorKind` — beats Go/Node's stringly-typed `err.code`, Java's checked-exception
  noise; parity with Rust's `ErrorKind` and Swift-system's typed `Errno`. (Zig's
  per-op error sets are a real alternative — considered and rejected, see above.)
- **`write_atomic` that is actually durable**, one primitive, 5 steps — a gap in
  *every* peer (Go/Rust/Node/Kotlin/Java hand-roll it; Swift/Zig ship a
  non-durable "atomic" that is not power-loss safe).
- **Byte-backed `Path`.** Carries real non-UTF-8 Unix names / WTF-8 Windows names
  that the JVM cannot even represent (`InvalidPathException`) and TS/Deno cannot
  represent at all; parity with Rust's `Path`/`OsStr` and Swift-system's
  `FilePath` (also byte-backed), Zig's byte `[]const u8`.

## See also

- [`spec/decisions/04-effects.md`](../spec/decisions/04-effects.md) — D322 (io-core),
  D323 (fs), D324 (os), D302 amendment (net projection).
- [`docs/consume-types.md`](consume-types.md) — must-consume mechanics (D133/D180)
  underlying `File`/`BufWriter`.
- [`docs/plans/176-io-fs-os.md`](plans/176-io-fs-os.md) — the umbrella plan (Q1-Q15
  decision table, phase history).
