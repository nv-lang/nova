<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Running tests

**English** | [Русский](running-tests.ru.md)

Build `nova` CLI, then run the full test suite:

```sh
# build nova CLI (one-time, or after changes)
cd nova-cli && cargo build --release && cd ..

# run all tests
nova-cli/target/release/nova test
```

## Common flags

```sh
nova test --filter syntax/closure        # subset of tests
nova test --mode release                 # -O3 -flto compilation
nova test --toolchain clang              # force toolchain
nova test --timeout 60                   # timeout per test
nova test --format json                  # JSON events (one per line)
nova test --format junit > results.xml   # JUnit XML for CI parsers
nova test --retries 2                    # retry transient AV/race fails
nova test --rerun-failed                 # only failed-last-time
nova test --include-stdlib               # include std/* alongside nova_tests/*
```

## Single-test debugging

No walkdir, no parallel overhead:

```sh
./compiler-codegen/target/debug/nova-codegen test-build nova_tests/basics/literals.nv \
    --toolchain clang --keep-artifacts
```

## Toolchain setup

- **Windows:** `winget install LLVM.LLVM` (Clang, recommended) +
  Visual Studio Build Tools (MSVC SDK + linker, required by Clang too).
- **Linux:** `apt install clang` or `dnf install clang`; GCC usually
  pre-installed.
- **macOS:** `xcode-select --install` (Apple Clang).

Auto-detection picks Clang first, then MSVC (Windows) or GCC (Linux).
Override with `--toolchain clang|msvc|gcc` or via env-vars
(`NOVA_CLANG`, `NOVA_GCC`, `NOVA_VCVARS`).
