<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Building Nova on Linux (native / WSL2)

**English** | [Русский](linux-build.ru.md)

Last updated 2026-07-21. Verified 2026-07-20 directly on WSL2 Ubuntu 26.04 (kernel
`6.6.87.2-microsoft-standard-WSL2`), outside Docker. See also
[`docker/README.md`](../../docker/README.md) for the earlier (2026-05-12)
Docker-based validation (Plan 40) — this document complements it with a
bare-metal/WSL recipe and a few gotchas Docker's isolation hides.

Closes `[M-nova-linux-build]` (see `docs/plans/backlog-followups.md`
history / `docs/dev/simplifications.md`).

## TL;DR

```sh
# 1. System packages (Debian/Ubuntu; see §Packages for other distros)
sudo apt install clang cmake make libgc-dev build-essential

# 2. Rust toolchain — use rustup, NOT your distro's rustc package (see
#    §Known issue: distro rustc ICE below).
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain 1.85.0 --profile minimal
source "$HOME/.cargo/env"

# 3. Clone + submodule
git clone https://github.com/nv-lang/nova.git && cd nova
git submodule update --init compiler-codegen/nova_rt/libuv

# 4. Build
cd nova-cli && cargo build --release && cd ..
# → nova-cli/target/release/nova

# 5. Smoke test
echo 'fn main() Io -> () => println("hello")' > /tmp/hello.nv
./nova-cli/target/release/nova build /tmp/hello.nv -o /tmp/hello && /tmp/hello

# 6. Test a module
./nova-cli/target/release/nova test std/src/checksums
```

All of the above is verified working end-to-end (checkpoint deleted on wave closure; see git history for the raw session log).

## Packages

| Purpose | Debian/Ubuntu | Fedora/RHEL | Arch |
|---|---|---|---|
| C toolchain | `clang build-essential` | `clang gcc` | `clang base-devel` |
| Boehm GC | `libgc-dev` | `gc-devel` | `gc` |
| (optional, for `std/tls`) | `libmbedtls-dev` | — | — |
| cmake/make | `cmake make` | `cmake make` | `cmake make` |

`libuv` is **not** a system package dependency — it's vendored as a git
submodule (`compiler-codegen/nova_rt/libuv`) and built from source on
first use (see §libuv below). `pkg-config` is not required — nothing in
the build uses it.

`ar` (binutils) is required for the libuv archive step; it ships with
`build-essential` / `base-devel` already.

Nova's own Rust code has no unconditional Windows-only dependency — no
`[target.'cfg(windows)']` crates exist in `compiler-codegen/Cargo.toml` or
`nova-cli/Cargo.toml`. `compiler-codegen/src/test_runner.rs` already has
mature `#[cfg(target_os = "linux")]` branches for toolchain detection,
Boehm detection (`detect_boehm`), and libuv build
(`detect_or_build_libuv` / `build_libuv_lib`) — this was implemented in
Plan 22/27/40/44.1 and validated once already via Docker (2026-05-12).
This doc's job was to verify it still holds on a real (non-container)
Linux host and record what changed since.

## Known issue: distro-packaged `rustc` can ICE on `compiler-codegen`

On Ubuntu 26.04 the pre-installed `rustc`/`cargo` (`1.93.1
(01f6ddf75 2026-02-11), built from a source tarball`) **panics with an
internal compiler error** while compiling
`compiler-codegen/src/codegen/emit_c.rs`:

```
thread 'rustc' panicked at .../library/alloc/src/vec/mod.rs:2796:36:
slice index starts at 52 but ends at 51
error: the compiler unexpectedly panicked. this is a bug.
query stack during panic:
#0 [check_liveness] checking liveness of variables in
   `codegen::emit_c::<impl at src/codegen/emit_c.rs:2026:1: 2026:14>::receiver_c_type`
```

Reproduced twice, byte-identical — not a flake. This is an upstream rustc
bug in the NLL/MIR-borrowck `check_liveness` query, triggered by
`emit_c.rs`'s size/complexity, not a Nova bug — GitHub CI
(`.github/workflows/nova-test-regression.yml`, `runs-on: ubuntu-latest`,
no explicit toolchain step) does **not** hit this because the GH-hosted
runner image ships a different rustc build.

**Workaround (verified):** install a toolchain via `rustup` instead of
relying on the distro package — `rustup` installs into `$HOME`, **no
`sudo` required**, and coexists with the system `rustc` (rustup will warn
about the pre-existing install; that's harmless, just don't put
`~/.cargo/bin` ahead of `/usr/bin` in `PATH` if you want to keep using the
distro one for anything else):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain 1.85.0 --profile minimal
~/.cargo/bin/cargo build --release --manifest-path nova-cli/Cargo.toml
```

`1.85.0` was picked because it's `compiler-codegen`'s declared
`rust-version` (MSRV) — it built cleanly (`Finished release profile
[optimized] target(s) in 6m 47s` for `nova-cli`, `3m 10s` for
`compiler-codegen` alone). Any rustup-distributed stable release close to
that should work; what specifically breaks is the *distro-patched* build,
not the language/edition. This repo does **not** ship a `rust-toolchain.toml`
pin (would affect the Windows workflow too) — if this bites CI or another
contributor, revisit whether to add one.

## Boehm GC

`detect_boehm()` (Linux branch) looks for `gc.h` at
`/usr/include/gc.h`, `/usr/include/gc/gc.h`, `/usr/local/include/gc.h`,
in that order, and otherwise fails with a `sudo apt install libgc-dev`
hint. Confirmed: Ubuntu 26.04's `libgc-dev` (`1:8.2.12-1`) package puts
the header at `/usr/include/gc/gc.h` and the shared lib at the standard
multiarch path — no `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` overrides
needed for a stock apt install. Linking is plain `-lgc` (+ `-lpthread` on
Linux) — no static/vendored Boehm build needed on Linux (unlike Windows,
which uses vcpkg's `x64-windows-static` triplet).

## libuv

`nova build`/`nova test` build libuv **from source** on first use (there
is no system-package path for libuv on Linux in this codebase — Plan 22
decided to vendor it uniformly across platforms rather than mix
`libuv1-dev` on Linux with a from-source build on Windows). The Linux
branch of `build_libuv_lib` (`compiler-codegen/src/test_runner.rs`)
compiles a fixed whitelist of `src/*.c` + `src/unix/{async,core,dl,fs,...
,linux,procfs-exepath,proctitle,random-getrandom,random-sysctl-linux,
no-fsevents}.c` via `cc` (respects `$CC`) and archives them with `ar` into
`<repo>/target/libuv-cache/libuv.a`. Confirmed working as-is:
`nova: libuv.a built (36 files)`, cached afterwards (~instant on
subsequent builds).

## Fiber arena — POSIX implementation already exists

`compiler-codegen/nova_rt/fiber_arena.c` (POSIX: `mman.h`, `pthread.h`,
`ucontext.h`, `signal.h`, `SIGSEGV` handler for stack-overflow diagnostics)
is a complete, separate implementation from `fiber_arena_win.c`
(guarded by `#if defined(_WIN32) && NOVA_FIBER_ARENA_ENABLED`, compiles to
effectively nothing on Linux). **No porting work was needed** — this was
the one part of the task that would have been a hard-stop if missing; it
wasn't.

## Gotcha: WSL2 `/mnt/<drive>` (9p) is fine for point reads, bad for directory
## walks — copy the Nova *workspace* to native fs before running `nova build`/`test`

If your Nova checkout lives on a Windows drive mounted into WSL2 via
`/mnt/c`/`/mnt/d` (9p protocol), two very different perf profiles show up:

- **`cargo build`** (compiling the Rust crates) reads a modest number of
  individual `.rs` files — fine directly on `/mnt/...` (a release build of
  `compiler-codegen` took ~3 min either way). Point `CARGO_TARGET_DIR` at
  native ext4 (e.g. `$HOME/...`) regardless — cargo's incremental
  build writes thousands of small object/metadata files, which *is* slow
  over 9p.
- **`nova build`/`nova test`** resolve the Nova workspace (`nova.toml` +
  `std/`) by walking directories recursively. On `/mnt/...` this call
  chain visibly blocks in the kernel wait channel `p9_client_rpc` (checked
  via `/proc/<pid>/task/*/wchan`) for **minutes** on a trivial
  hello-world, because every subdirectory listing is a 9p round-trip.
  **Fix:** copy `nova.toml` + `std/` (+ `compiler-codegen/nova_rt/`, for
  the runtime C + trimmed libuv `src`+`include`, skip `libuv/test`+
  `libuv/docs` — ~300 MB you don't need) to a native path and run `nova
  build`/`nova test` with that as `$CWD`. Rebuilds after that: single
  digit seconds.

Aside: `du` **through the 9p mount over-reports size wildly** — it showed
`282M` for `std/` while a `rsync` copy of the exact same 282 files (file
count matched exactly) landed at `3.8M` on native ext4. Don't trust `du`
numbers gathered through `/mnt/...`; trust `find -type f | wc -l` (file
count) instead if you need to sanity-check a copy.

This is a WSL2/9p artifact, not a Nova bug — a native Linux box (no
Windows-drive mount in the loop) shouldn't see it at all, and GitHub CI
doesn't either.

## Verified (2026-07-16 baseline, critical issues resolved 2026-07-20)

The initial Linux build validation (2026-07-16) identified three deterministic platform
-specific issues, all of which have since been fixed (2026-07-20, Plan 208–220 followup
wave). The conformance gate (`nova-gate.yml`) now passes cleanly on Linux:

- **link-order regression** (Unix linker archive ordering): Fixed in `test_runner.rs`
  — `libuv.a` now placed after `.o` files and runtime archive in link command.
- **gc-sections dead-code issue** (`nova_bench_*` symbols): Fixed via
  `-ffunction-sections`/`-fdata-sections` on Unix in `build_rt_archive_lib`.
- **cbrt ULP non-portability**: Fixed in `d109_primitive_methods_f64_f32_math.nv` test
  — replaced exact equality assert with epsilon-based comparison.

| Step | Result |
|---|---|
| `cargo build --release` (compiler-codegen) | PASS (rustup 1.85.0), 3m10s |
| `cargo build --release` (nova-cli) | PASS (rustup 1.85.0), 6m47s, binary runs |
| libuv build-from-source | PASS, `libuv.a built (36 files)` |
| Boehm GC detection/link | PASS, system `libgc-dev`, no overrides |
| `nova build` hello-world | PASS, `built: .../hello (12.09s)`, ran, correct stdout |
| `nova test std/src/checksums` | PASS: 3 FAIL: 0 SKIP: 3 |
| Conformance gate (`spec_tests/conformance`) | PASS (as of 2026-07-20 fixes) |
| TSan smoke (spawn+supervised, manual `clang -fsanitize=thread`) | Compiles+links clean, runs to completion, **found 2 real data races** — checkpoint deleted on wave closure, see git history, and the closing task report for Plan 211 |

## Known gaps (out of scope here, found via existing CI)

`.github/workflows/nova-test-regression.yml` documents pre-existing failures in
`nova test std` on Linux (as of 2026-07-16), distinct from the conformance gate
and beyond the scope of this doc:
`std/src/concurrency/retry_test` (C compile error — struct-return type
mismatch, looks like a mono/codegen bug, not obviously Linux-specific),
two `RUN-FAIL` fiber-stack-overflow crashes (`std/src/fs/concurrent_stat_test`,
`std/src/net/addr`), an integer-overflow `RUN-FAIL` in
`std/src/identifiers/ulid_test`, and a plain `.nv`-source compile error in
`std/src/time/civil/civil_arith_test` (retired `str.len()` API, D249 —
looks like a pre-existing source bug, unrelated to platform). Whoever picks
up full `nova test std` on Linux should track these independently.

## Building nova-lsp

The Nova language server is available in `nova-lsp/` within the repository:

```sh
cd nova-lsp
cargo build --release
# → target/release/nova-lsp (executable available in nova-lsp/target/release)
```

No additional system dependencies are required beyond the standard Nova build setup
(Rust toolchain, C compiler, Boehm GC). The LSP binary can be used as a
language server backend for compatible editors (VSCode, Neovim, etc.).

## TSan / sanitizer builds

Not part of the standard build (`test_runner.rs` has no `--sanitizer`
flag); Plan 40's `docker/Dockerfile` drives sanitizer builds by using
`clang` directly with sanitizer flags outside the normal `nova`-CLI
compile path — this doc's TSan smoke did the same (manually recompiled
the CLI-generated `.c` + `nova_rt/*.c` + `libuv.a` with `-fsanitize=thread`,
checkpoint deleted on wave closure, see git history). No suppression file was needed
for a minimal 2-spawn smoke test with stock system `libgc` (no special
Boehm build flags) — heavier stress tests may still hit the
Boehm/TSan interaction documented in `docker/README.md` (`THREAD_LOCAL_ALLOC=0
PARALLEL_MARK=0` mitigation, needed for `--enable-threads=posix` Boehm
builds under sanitizers). `[M-tsan-race-detector]` and
`[M-83.11-f2-arm-tsan]` (both gated on this doc's closure) can now
proceed.
