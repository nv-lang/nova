# SPDX-License-Identifier: MIT OR Apache-2.0
#
# docker/release/Dockerfile — Plan 221 A-D1: релизный Docker-образ Nova v0.1.0
# (компилятор + std/ + C-рантайм, готовый к `docker run ... nova build`).
#
# Build with the repository root as context and all submodules initialized.
# The release uses the pinned Boehm collector with LARGE_CONFIG.
FROM ubuntu:22.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        clang \
        cmake \
        make \
        build-essential \
        curl \
        ca-certificates \
        git \
    && rm -rf /var/lib/apt/lists/*

# rustup, pinned to MSRV 1.85 (nova-cli/Cargo.toml, compiler-codegen/Cargo.toml
# rust-version) — тот же идиом, что nova-gate.yml (dtolnay/rust-toolchain).
# НЕ используем дистрибутивный rustc (см. docs/guide/linux-build.md §Known issue:
# distro-packaged rustc can ICE on compiler-codegen).
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
        sh -s -- -y --default-toolchain 1.85.0 --profile minimal

WORKDIR /src

# Build context = корень репозитория (COPY, не git clone).
COPY . .

RUN test -f compiler-codegen/nova_rt/libuv/CMakeLists.txt \
    && cmake -S compiler-codegen/nova_rt/gc -B /tmp/boehm \
        -DBUILD_SHARED_LIBS=OFF -Dbuild_cord=OFF -Dbuild_tests=OFF \
        -Denable_docs=OFF -Denable_large_config=ON -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX=/opt/nova/gc -DCMAKE_INSTALL_LIBDIR=lib \
    && cmake --build /tmp/boehm --parallel \
    && cmake --install /tmp/boehm \
    && install -m 644 compiler-codegen/nova_rt/gc/README.md /opt/nova/gc/README.md

RUN cargo build --release --manifest-path nova-cli/Cargo.toml

# ──────────────────────────── Stage 2: runtime ────────────────────────────
FROM ubuntu:22.04 AS runtime

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        clang \
        cmake \
        make \
        build-essential \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Собранный компилятор.
COPY --from=builder /src/nova-cli/target/release/nova /usr/local/bin/nova

# std/ — исходники стандартной библиотеки (nova.toml резолвит std
# относительно NOVA_STD_PATH, см. compiler-codegen/src/manifest.rs).
COPY --from=builder /src/std /opt/nova/std

# nova_rt/ — C runtime (eventloop.c, fiber_arena*.c, libuv/ ЦЕЛИКОМ —
# build_libuv_lib на Linux компилирует libuv/src/*.c + libuv/src/unix/*.c
# из исходников submodule'а, нужен полный include+src, не урезанный
# Windows-набор package-release.ps1).
COPY --from=builder /src/compiler-codegen/nova_rt /opt/nova/nova_rt

COPY --from=builder /opt/nova/gc /opt/nova/gc

ENV NOVA_STD_PATH=/opt/nova/std \
    NOVA_CG_INCLUDE=/opt/nova \
    NOVA_RT_DIR=/opt/nova/nova_rt \
    NOVA_GC_LIB_DIR=/opt/nova/gc/lib \
    NOVA_GC_INCLUDE_DIR=/opt/nova/gc/include

WORKDIR /work

ENTRYPOINT ["nova"]
CMD ["--version"]
