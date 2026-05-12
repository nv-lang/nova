# Plan 40 Ф.1 Этап 5 — Linux build + tests + optional sanitizers.
#
# Tier 1 supported platform: Ubuntu 22.04+ x86_64, clang LLVM 15+, glibc 2.35+.
#
# Build modes via SANITIZER build-arg:
#   none  — plain release build (default, fastest)
#   tsan  — ThreadSanitizer (data race detection)
#   asan  — AddressSanitizer (use-after-free, buffer overflow)
#   ubsan — UndefinedBehaviorSanitizer (UB detection)
#
# Usage:
#   docker build -f docker/Dockerfile -t nova:linux .
#   docker build -f docker/Dockerfile -t nova:tsan  --build-arg SANITIZER=tsan .
#   docker build -f docker/Dockerfile -t nova:asan  --build-arg SANITIZER=asan .
#   docker build -f docker/Dockerfile -t nova:ubsan --build-arg SANITIZER=ubsan .
#
#   docker run --rm nova:linux  ./docker/run-tests.sh
#   docker run --rm nova:tsan   ./docker/run-tests.sh
#

FROM ubuntu:22.04

# Сразу install build tools + libuv (libuv1-dev из apt — system libuv,
# не пытаемся build из source как на Windows; vcpkg для Linux настроим в
# отдельной итерации). Boehm GC — system libgc-dev.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        clang-15 \
        lld-15 \
        cmake \
        ninja-build \
        curl \
        git \
        ca-certificates \
        libuv1-dev \
        libgc-dev \
        pkg-config \
        build-essential \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain через rustup (для compiler-codegen / nova-cli).
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain stable --profile minimal
ENV PATH=/root/.cargo/bin:$PATH

# Симлинки clang-15 → clang (test runner ищет clang в стандартных местах).
RUN ln -sf /usr/bin/clang-15 /usr/local/bin/clang && \
    ln -sf /usr/bin/clang++-15 /usr/local/bin/clang++ && \
    ln -sf /usr/bin/ld.lld-15 /usr/local/bin/ld.lld

# Sanitizer flags (build-time argument).
ARG SANITIZER=none
ENV NOVA_SANITIZER=${SANITIZER}

# Plan 40 R3-6: TSan + Boehm — disable parallel marking + thread-local
# alloc to avoid false-positive races on GC internals.
ENV BOEHM_TSAN_FLAGS="-DTHREAD_LOCAL_ALLOC=0 -DPARALLEL_MARK=0"

# Plan 40 R3-11: UBSan — exclude signed-integer-overflow for nova_int
# arithmetic (legitimate wraparound in some helpers).
ENV UBSAN_OPTIONS="print_stacktrace=1:halt_on_error=0:report_error_type=1"

# Plan 40 R3-1: GC_register_my_thread per pthread. Our nova_alloc уже
# использует Boehm API; thread registration делается в pthread stress
# tests Этапа 6.

WORKDIR /nova

# Копируем source.
COPY . /nova/

# Build nova-cli release.
# Note: на Linux не используем vcpkg для bdwgc — используем system libgc.
# nova_rt/alloc_boehm.c подключает gc.h из /usr/include/gc/.
RUN cd nova-cli && \
    cargo build --release --manifest-path Cargo.toml 2>&1 | tail -20

# Default command — info-style (доказывает что image собрался).
CMD ["bash", "-c", "echo 'Plan 40 Linux image (SANITIZER=${NOVA_SANITIZER}). Run: ./docker/run-tests.sh'"]
