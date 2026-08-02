# SPDX-License-Identifier: MIT OR Apache-2.0
#
# docker/release/Dockerfile — Plan 221 A-D1: релизный Docker-образ Nova v0.1.0
# (компилятор + std/ + C-рантайм, готовый к `docker run ... nova build`).
#
# Two-stage:
#   1. builder — Ubuntu 22.04 + rustup (pinned 1.85, MSRV nova-cli/Cargo.toml
#      + compiler-codegen/Cargo.toml) + системные пакеты по верифицированному
#      рецепту docs/guide/linux-build.md / .github/workflows/nova-gate.yml (clang
#      cmake make libgc-dev build-essential) — собирает nova-cli release из
#      исходников build context'а (COPY, НЕ git clone — build context = корень
#      репозитория, см. docker build snippet в README.md рядом).
#   2. runtime — тонкий Ubuntu 22.04 с ТОЛЬКО тем, что нужно пользователю
#      Nova в рантайме: clang (Nova компилирует .nv -> C -> вызывает clang),
#      libgc-dev (заголовки + so для линковки Boehm GC), cmake+make (нужны
#      build_libuv_lib / vendor FFI pipeline — те же пакеты, что gate),
#      собранный бинарь nova, std/ (исходники стандартной библиотеки),
#      nova_rt/ (C-рантайм: eventloop.c, fiber_arena*.c, libuv submodule
#      целиком — build_libuv_lib собирает libuv/src/*.c из исходников на
#      Linux, ему нужен полный libuv/src+include, не урезанный Windows-набор
#      package-release.ps1) — плюс системные Boehm GC headers/lib уже в
#      /usr/include, /usr/lib (libgc-dev), поэтому NOVA_GC_LIB_DIR/
#      NOVA_GC_INCLUDE_DIR тут указывают на системные пути (см.
#      docs/guide/linux-build.md §Boehm GC: apt libgc-dev не требует override —
#      но выставляем явно для детерминизма и параллели с setup-env.ps1).
#
# Env vars (см. scripts/package-release.ps1 setup-env.ps1 — те же 5 путей,
# юникс-варианты; compiler-codegen/src/manifest.rs resolve_std_path,
# nova-cli/src/main.rs resolve_paths, test_runner.rs detect_boehm):
#   NOVA_STD_PATH        — путь к std/ (исходники стандартной библиотеки)
#   NOVA_CG_INCLUDE      — путь к каталогу с nova_rt/ (родитель nova_rt)
#   NOVA_RT_DIR          — путь к nova_rt/ (C runtime: eventloop.c, libuv/)
#   NOVA_GC_LIB_DIR      — путь к каталогу с libgc.so (Boehm GC)
#   NOVA_GC_INCLUDE_DIR  — путь к каталогу с gc.h (Boehm GC headers)
#
# Build (из корня репозитория — build context ОБЯЗАН быть корнем, COPY ниже
# читает nova-cli/, compiler-codegen/, std/ как top-level каталоги контекста):
#   docker build -f docker/release/Dockerfile -t nova:0.1.0 .
#
# Известный gap (унаследован от docker/README.md, Plan 40, 2026-05-12):
# Boehm GC_init может падать под restricted Docker permissions на
# perf-бенчах (GC_find_limit_with_bound) — не относится к обычной
# компиляции/рантайму программ, не блокирует этот образ.

# ──────────────────────────── Stage 1: builder ────────────────────────────
FROM ubuntu:22.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        clang \
        cmake \
        make \
        libgc-dev \
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

# libuv — git submodule; build context COPY переносит .git-состояние
# каталога, но не гарантирует, что субмодуль инициализирован в чекауте,
# из которого собирается образ (та же оговорка, что docs/guide/linux-build.md
# п.3 "git submodule update --init"). Если submodule уже присутствует
# (обычный чекаут разработчика) — команда ниже no-op.
RUN if [ -f .gitmodules ] && [ -d .git ]; then \
        git submodule update --init compiler-codegen/nova_rt/libuv || true; \
    fi

RUN cargo build --release --manifest-path nova-cli/Cargo.toml

# ──────────────────────────── Stage 2: runtime ────────────────────────────
FROM ubuntu:22.04 AS runtime

ENV DEBIAN_FRONTEND=noninteractive

# Nova компилирует .nv -> C -> вызывает системный C-компилятор; libgc-dev
# даёт и заголовки, и разделяемую библиотеку Boehm GC; cmake+make нужны
# vendor FFI pipeline (build_missing_vendor_ffi_libs, mbedTLS/brotli при
# сборке std/tls, std/compress) и build_libuv_lib.
RUN apt-get update && apt-get install -y --no-install-recommends \
        clang \
        cmake \
        make \
        libgc-dev \
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

# Env vars — как setup-env.ps1 (scripts/package-release.ps1), юникс-пути.
# Системный libgc-dev кладёт заголовки/lib в стандартные multiarch-пути
# (docs/guide/linux-build.md §Boehm GC) — override здесь не строго обязателен,
# но выставлен явно для детерминизма контейнерного окружения.
ENV NOVA_STD_PATH=/opt/nova/std \
    NOVA_CG_INCLUDE=/opt/nova \
    NOVA_RT_DIR=/opt/nova/nova_rt \
    NOVA_GC_LIB_DIR=/usr/lib/x86_64-linux-gnu \
    NOVA_GC_INCLUDE_DIR=/usr/include

WORKDIR /work

ENTRYPOINT ["nova"]
CMD ["--version"]
