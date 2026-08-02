# syntax=docker/dockerfile:1
# SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 187, волна 2 (§9.4 п.7 / Ред.6 п.3) — живое демо flagship-агрегатора
# в Docker: multi-stage (builder компилирует nova-cli + собирает бинарь
# `aggregator` через `nova build`; runtime — только сам бинарь).
#
# Build context = корень репозитория nova (см. docker/README.md — тот же
# приём, что и Plan 40's docker/Dockerfile, но этот образ для ДЕМО, не для
# CI-тестов рантайма).
#
# Перед `docker build` (один раз, submodule не чекаутится в build context
# автоматически):
#   git submodule update --init compiler-codegen/nova_rt/libuv
#
# Usage:
#   docker build -f examples/flagship/aggregator/Dockerfile \
#       --build-context nova-http=../nova-http \
#       -t aggregator-demo:local .
#   docker run --rm -p 8187:8187 aggregator-demo:local
#
# Рецепт верифицирован docs/guide/linux-build.md (2026-07-16, WSL2 Ubuntu) +
# .github/workflows/nova-gate.yml (приём "sibling clone" для path-зависимости
# `http = { path = "../../nova-http" }`, examples/nova.toml — `tls`/
# `compress` резолвятся АВТОМАТИЧЕСКИ по git самим компилятором Nova, sibling
# нужен ТОЛЬКО для `http`).
#
# ВАЖНО (2026-07-17, оркестратор): `nova-http` берётся из ЛОКАЛЬНОГО
# рабочего дерева сиблинг-каталога через именованный build-context Buildx
# (`--build-context nova-http=...`, требует `# syntax=docker/dockerfile:1`
# выше), НЕ `git clone` с GitHub — на момент этой волны в GitHub-репозитории
# nova-http нет свежего коммита `WriteBuffer .into() → .into_bytes()`
# (std-переименование 6dff14d7f); тот коммит существует ТОЛЬКО локально
# (`d:/Sources/nv-lang/nova-http`, ветка `fix-compress-dep`, коммит
# `250f4ab`) — клон с GitHub принёс бы старый `.into()` и упал на сборке.
# `tls`/`compress` остаются git-зависимостями по `nova.lock` (без изменений
# — тот код у них не менялся).

FROM ubuntu:22.04 AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        clang cmake make libgc-dev build-essential git ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain — rustup, пин на MSRV 1.85 (docs/guide/linux-build.md: дистро-rustc
# ICE'ит на emit_c.rs; тот же пин, что nova-gate.yml).
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
        sh -s -- -y --default-toolchain 1.85.0 --profile minimal
ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /build/nova

# Слоёный COPY (не единый `COPY . .`) — агрессивный кэш Docker-слоёв: правки
# в examples/ (main.nv-итерации) не должны инвалидировать 3+ минутный
# `cargo build --release`. nova-cli зависит от `../compiler-codegen` по
# относительному path (nova-cli/Cargo.toml) — оба до RUN-слоя cargo.
# `std/` тоже ДО cargo build (не после, как в первой версии этого Dockerfile
# — RUN-FAIL, найдено этой волной): `compiler-codegen`'s Rust-сборка сама
# читает `.nv`-файлы из `std/src/runtime|net/*.nv` по относительному пути
# `src/codegen/../../../std/...` (embed прелюдии/рантайма в компилятор) —
# это RUST-COMPILE-TIME зависимость, не только `nova build`-time; без
# std/ на месте `cargo build` падает с «couldn't read .../std/...: No such
# file or directory», не с обычной Rust type-error. examples/ — после
# (нужны только `nova build`, не Rust-компилятору) — это оставляет реальный
# caching-выигрыш для итераций на examples/ без std/ изменений.
COPY nova-cli ./nova-cli
COPY compiler-codegen ./compiler-codegen
COPY std ./std

RUN cd nova-cli && cargo build --release --manifest-path Cargo.toml

# Sibling nova-http (path-зависимость examples/nova.toml, Plan 203/204) —
# резолвится относительно /build/nova/examples/nova.toml как ../../nova-http,
# т.е. физически рядом с /build/nova. Именованный build-context (не git
# clone — см. ВАЖНО выше): `--build-context nova-http=../nova-http` кладёт
# ЛОКАЛЬНОЕ рабочее дерево сюда. tls/compress — git-зависимости
# (`{ git = ..., version = "0.1" }`), компилятор фетчит их сам по nova.lock
# при первом обращении — sibling им не нужен (см. nova-gate.yml, который
# клонирует ТОЛЬКО nova-http).
COPY --from=nova-http . /build/nova-http

COPY nova.toml ./nova.toml
COPY examples ./examples

# `nova build` — на Linux system libgc (нет vcpkg, docs/guide/linux-build.md
# §Boehm GC) — NOVA_GC_*_DIR overrides не нужны для apt-стокового libgc-dev.
# mbedTLS (nova-tls, [ffi] vendor) собирается сама (нужен только `cc`, уже
# есть в build-essential/clang).
RUN cd examples && \
    /build/nova/nova-cli/target/release/nova build \
        flagship/aggregator/src/main.nv -o /build/aggregator --strict-effects

FROM ubuntu:22.04 AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        libgc1 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/aggregator /app/aggregator

# Контейнер слушает 0.0.0.0 по умолчанию (иначе -p не достучится снаружи) —
# src/main.nv по умолчанию биндится на 127.0.0.1 (локальная разработка);
# AGGREGATOR_BIND — env-override (см. main.nv, resolve_addr()).
ENV AGGREGATOR_PORT=8187
ENV AGGREGATOR_BIND=0.0.0.0
EXPOSE 8187

WORKDIR /app
ENTRYPOINT ["./aggregator"]
