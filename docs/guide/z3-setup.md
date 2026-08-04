<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# SMT verification and Z3 setup

**English** | [Русский](z3-setup.ru.md)

Nova includes a static contract verifier (`requires`/`ensures`/`invariant`).
By default it uses **TrivialBackend** (reflexive tautologies, constant
folding) — works with no external dependencies. Full verification needs
**Z3**.

## Without Z3 (default)

Works right after a plain build. Proves only reflexive contracts and
constant expressions. Z3-only tests are automatically SKIPped.

```bash
cd nova-cli && cargo build --release
nova test nova_tests/contracts/
# PASS: 82  SKIP: 9 (z3-only)
```

## With Z3

**Step 1: install Z3 via vcpkg** (one time)

```bash
# Windows:
cd compiler-codegen
vcpkg install --triplet x64-windows-static --x-manifest-root=.

# Linux:
cd compiler-codegen
vcpkg install --triplet x64-linux --x-manifest-root=.

# macOS:
cd compiler-codegen
vcpkg install --triplet x64-osx --x-manifest-root=.
```

`vcpkg.json` already lists `z3` and `bdwgc` — both dependencies install with
one command. Result: `vcpkg_installed/<triplet>/lib/libz3.a`.

> This step is needed ONLY for Z3. It also installs the Boehm GC (`bdwgc`) —
> if vcpkg is already configured, `nova build`/`nova test` prefer the vcpkg
> build (faster, no rebuild) — but since #269 Ф.2 that is no longer required:
> without vcpkg or `NOVA_GC_LIB_DIR` the compiler builds the Boehm GC once,
> on its own, from the vendored submodule (`compiler-codegen/nova_rt/gc` +
> `compiler-codegen/nova_rt/libatomic_ops`, pulled by `git clone --recursive`
> or `git submodule update --init`) — see
> [Building from source](building-from-source.md).

**Step 2: build with the `z3-backend` feature**

```bash
cd nova-cli
cargo build --release --features z3-backend
```

**Step 3: run with Z3**

```bash
NOVA_SMT_BACKEND=z3 nova test nova_tests/contracts/
# PASS: 91  SKIP: 0
```

> `VCPKG_TRIPLET` overrides the triplet if you need a non-standard one
> (e.g. `arm64-linux`).

Details: docs/plans/33-contracts-implementation.md — the "Z3 dev-setup" section.
