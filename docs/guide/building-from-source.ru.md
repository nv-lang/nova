<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Сборка из исходников

[English](building-from-source.md) | **Русский**

Соберите `nova` CLI, затем используйте его для компиляции Nova-программ:

```sh
# build nova CLI (requires Rust + Cargo)
cd nova-cli && cargo build --release && cd ..

# compile a Nova file to a native binary, then run it
nova-cli/target/release/nova build path/to/hello.nv -o hello
./hello

# type-check only
nova-cli/target/release/nova check path/to/hello.nv
```

Pipeline двухступенчатый: `nova-codegen` (внутренний) производит `.c`,
нативный C-компилятор линкует его с runtime'ом (`nova_rt/`). `nova build`
оркестрирует это автоматически.

## Ручной pipeline (без `nova` CLI)

```sh
cd compiler-codegen
cargo run -- compile path/to/hello.nv          # Nova → C
gcc path/to/hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
    -I. -o hello                                # C → binary
./hello
```

Полный guide, опции, известные ограничения:
[compiler-codegen/README.md](../../compiler-codegen/README.md).
