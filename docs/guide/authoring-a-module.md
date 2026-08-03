<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# How to author a Nova module

**English** | [Русский](authoring-a-module.ru.md)

> A general guide: from an empty directory to a publishable package. A
> native-backed module (a wrapper over `.c`/a prebuilt `.lib`) is a **special
> case** at the end (§7). **`[ffi.staticlib]` (a cargo/make-staticlib built as
> part of the build) was RETRACTED by the owner (Plan 195, 2026-07-10)** — a
> native module must build WITHOUT Rust/cargo: only `.nv` + optionally `.c`
> (compiled by clang) + optionally a prebuilt `.lib`/`.a` (linked, not built).
> §7.2 below is kept as historical context (what existed and why it was
> removed).
>
> Related documents (not duplicated here — follow the link to read further):
> [module-conventions](../dev/module-conventions.md) (module design: effect
> plumbing, value/must-consume types, error domain),
> [nv-coding-style](../dev/nv-coding-style.md) (`.nv` code style),
> [ffi-cookbook](ffi-cookbook.md) (FFI mechanics: `extern "C"`, pointers,
> `CStr`, `[ffi]`), [spec D78](../../spec/decisions/07-modules.md#d78-package-tooling-novatoml-novalock-registry-chain-workspace)
> (normative rules for `nova.toml` / module-path).

## 0. TL;DR

1. Create a directory; put `nova.toml` with `[package] name` at its root.
2. Write `.nv` files — **file path = module path** (`foo/bar.nv` ⇒ `module foo.bar`).
3. Tests go alongside, in `*_test.nv` files (or `test "…" { }` blocks inside the module).
4. Mark the public surface with `export` + `#stable(since = "X")`.
5. Need C artifacts — declare them in `[ffi]` (a prebuilt `.c` shim +
   optionally a prebuilt `.lib`/`.a`); they'll be built and linked
   automatically when the module is imported (§7).

## 1. Package layout

A package is a directory with `nova.toml` at its root. **Source root =
package root** (there's no separate `src/` — D78, 2026-05-22). Modules live
directly in subdirectories:

```
nova-greet/                 repository: nova-<package> (§8)
├── nova.toml               manifest (required)
├── LICENSE
├── README.md
├── greet.nv                module greet          (the package's root module)
├── greet_test.nv           tests alongside the module
└── format/
    ├── ascii.nv            module format.ascii
    └── ascii_test.nv       tests alongside
```

Service directories (`target/`, `.git/`, hidden `.`-prefixed ones) are
skipped by the resolver. Non-`.nv` directories (`assets/`, `docs/`) are not
treated as modules.

## 2. `nova.toml` — the manifest

The minimum is `[package] name`; `version` is desirable. The full schema —
[D78](../../spec/decisions/07-modules.md#d78-package-tooling-novatoml-novalock-registry-chain-workspace).

```toml
[package]
name = "greet"                     # snake_case (D30); the package name = the modules' prefix
version = "0.1.0"                  # semver
nova-version = "0.5"               # minimum Nova version
description = "Greetings in different languages"
license = "MIT OR Apache-2.0"      # SPDX
repository = "https://github.com/you/nova-greet"

[[bin]]                            # optional: a binary entry point
name = "greet"
path = "bin/greet.nv"

[dependencies]                     # optional: external packages
some-lib = "1.2"                                        # from the registry
internal = { path = "../internal" }                     # local
remote   = { git = "https://github.com/…", tag = "v1" } # git (Plan 03.1/03.2)
```

A package **is a library by default**: its `export` declarations are
importable by other packages with no `[lib]` section at all. `[[bin]]` adds
binary entry points (a package can be both a library and a set of binaries).

## 3. Module path = file path (D78)

The compiler **always** checks the `module …` declaration against the file
path; a mismatch gives `E_D78_MODULE_PATH_MISMATCH` with a hint. The rule
(rev-3):

| File (from package `greet`'s root) | Declaration | Import |
|---|---|---|
| `greet.nv` | `module greet` | `import greet.{hello}` |
| `format/ascii.nv` | `module format.ascii` | `import format.ascii.{…}` |
| `format/ascii/upper.nv` (a folder peer) | `module format.ascii` | — |

A folder = ONE module made of co-equal files (peer files share one
`parent.folder` declaration). A file and a folder of the same name in one
directory are forbidden.

## 4. Public surface and stability

- `export` — what's visible outside the module/package; without `export` an
  item is module-private. Cross-package import goes only through `export`
  (D216-ecosystem).
- `#stable(since = "X")` on every public item — a semver contract. For
  libraries this can be made **mandatory**: `[lib] enforce-stability = true`
  turns a missing marker into an `nova doc --check` error (D127).
- An immature API — `#unstable` / `#experimental` instead of `#stable`.

```nova
module greet

#stable(since = "0.1")
export fn hello(name str) -> str => "Hello, ${name}!"
```

## 5. Tests alongside the module

Tests live **alongside** the module — in `*_test.nv` files (excluded from
the release graph) or as `test "…" { }` blocks inside the module itself.
Don't put tests in a separate tree. Pos/neg classification goes by the
`EXPECT_*` marker, not by directory ([test-conventions](../dev/test-conventions.md)).

```nova
module greet

test "hello inserts the name" {
    assert(hello("Ada") == "Hello, Ada!")
}
```

For effect modules (§6) a **mock-handler test** is mandatory — determinism
without a real resource.

## 6. Module design (brief; the full picture — module-conventions)

For I/O, OS, and resource subsystems, Nova's canon is **effect plumbing + a
type-level facade** ([module-conventions](../dev/module-conventions.md)):

- **The effect** is the internal dispatch point (`type Fs effect { … }`); the
  user doesn't call it directly → mockability (`with Fs = mem_fs() { … }`).
- **The user API** — methods on types + free functions (`File.open(path)`),
  the effect is visible in the signature's effect row, not in an op's name.
- Small values — `value` records; resources — must-consume `@close() -> Result`.
- Errors — one structural `XError { kind, … }` + an OPEN `ErrorKind`.
- byte-first: raw I/O is `[]u8`; `str` only via `from_utf8 -> Result`.

Pure algorithmics (parsing, encodings, calendar) — ordinary `.nv` functions
with no effect.

## 7. Native-backed module (a special case)

A module can sit on top of a C library or a Rust crate. The thin FFI layer
(`extern "C" fn`, handle types, `CStr`, pointers, ABI) is covered entirely in
[ffi-cookbook](ffi-cookbook.md); here — only **how to wire artifacts into the
build** so that `import`ing the module pulls them in automatically.

A native dependency is declared in `nova.toml` through a single section:

### 7.1. Prebuilt `.c` shims and system `.lib`s — `[ffi]`

```toml
[ffi]
c_shims      = ["native/sqlite3_shim.c"]   # compiled and linked
include_dirs = ["native/", "third_party/sqlite3/"]  # clang -I
libs         = ["sqlite3"]                 # system: clang -lsqlite3 / sqlite3.lib
```

If a system `.lib` isn't on the standard search path (a vcpkg triplet, a
vendored copy) — the link is resolved and wired directly into the build
pipeline (`test_runner.rs::build_command`), the same used-if-referenced D337
pattern as brotli/`net.c`; see `std/tls` below.

### 7.2. `[ffi.staticlib]` (a cargo/make-built staticlib) — RETRACTED (Plan 195)

**Existed (Plan 195), retracted by the owner on 2026-07-10.** It let a
module require **building** a native artifact (`cargo build`, `make`) as
part of its own build — the only user was `compiler-codegen/tls_shim/` (a
Rust staticlib over `rustls`). It contradicts the toolchain canon (the Nova
compiler + clang, WITHOUT Rust/cargo) — removed entirely
(`FfiStaticlibConfig`/`resolve_ffi_staticlib`/the section parsing were
removed from `manifest.rs`/`test_runner.rs`). `tls_shim/` was replaced by
`nova_rt/tls_c_shim.c` (mbedTLS) — the ordinary `[ffi]` path (§7.1), with no
cargo/build script at all: mbedTLS is installed AHEAD OF TIME via
`vcpkg install` (a prebuilt `.lib`, not built on the fly), `tls_c_shim.c` is
compiled/linked conditionally like ANY other runtime module
(`net.c`/`brotli_shim.c`), with no manifest declaration whatsoever.

> **The reference pattern** (current as of 2026-07) — `std/tls` in the
> monorepo (`nova_rt/tls_c_shim.c` + vcpkg mbedTLS, WITHOUT `[ffi.staticlib]`,
> WITHOUT any manifest declaration at all). Full mechanics —
> [ffi-cookbook §retracted](ffi-cookbook.md#ffistaticlib--retracted-plan-195).

## 8. Naming and publishing (an external package)

The convention for external (including native-backed) packages —
[D78 amendment, Plan 195](../../spec/decisions/07-modules.md#именование-внешних-пакетов-репозиториев-амендмент-plan-192-2026-07-10):

| Entity | Convention | Example |
|---|---|---|
| Repository | `nova-<package>` | `nova-tls` |
| Package name (`[package] name`) | `<package>` | `tls` |
| Module root | `<package>.*` | `import tls.{TlsStream}` |
| Native artifacts | `native/` | `native/tls_shim/` |

Publishing: commit the package into the `nova-<package>` repository; a
consumer wires it in as a git dependency —
`[dependencies] tls = { git = "https://…/nova-tls", tag = "v0.1.0" }`. The
registry (named `<package> = "1.2"`) is Plan 03.3, separately.

## 9. New-module checklist

1. `nova.toml` with `[package] name` at the root.
2. `.nv` files: `module path = file path`; a folder = one module.
3. Public surface — `export` + `#stable(since)`; for a library —
   `enforce-stability = true`.
4. Tests alongside (`*_test.nv` / `test` blocks); an effect module → a mock
   test.
5. Design per module-conventions (effect plumbing + facade; value/must-consume;
   one `XError`).
6. Native — `[ffi]` (prebuilt `.c` shims + a prebuilt `.lib`/`.a`;
   `[ffi.staticlib]` (cargo/make-built) — RETRACTED, Plan 195).
7. An external package — repo `nova-<package>`, native in `native/`, a git
   dependency.
