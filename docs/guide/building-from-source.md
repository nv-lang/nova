<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Building from source

**English** | [Русский](building-from-source.ru.md)

Build the `nova` CLI, then use it to compile Nova programs:

```sh
# build nova CLI (requires Rust + Cargo)
cd nova-cli && cargo build --release && cd ..

# compile a Nova file to a native binary, then run it
nova-cli/target/release/nova build path/to/hello.nv -o hello
./hello

# type-check only
nova-cli/target/release/nova check path/to/hello.nv
```

The pipeline is two-stage: `nova-codegen` (internal) produces `.c`, a
native C compiler links it with the runtime (`nova_rt/`). `nova build`
orchestrates this automatically.

## Manual pipeline (without `nova` CLI)

```sh
cd compiler-codegen
cargo run -- compile path/to/hello.nv          # Nova → C
gcc path/to/hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
    -I. -o hello                                # C → binary
./hello
```

Full guide, options, known limitations:
[compiler-codegen/README.md](../../compiler-codegen/README.md).
