# Plan 40 Ф.1 Linux Docker

Production validation для Plan 40 channel hardening на Tier 1 Linux
(Ubuntu 22.04+ x86_64, clang 15+, glibc 2.35+).

## Что внутри

- **`Dockerfile`** — Ubuntu 22.04 + clang 15 + Rust stable + libuv1-dev
  + libgc-dev (system Boehm). Build-arg `SANITIZER` выбирает режим
  (none / tsan / asan / ubsan).
- **`run-tests.sh`** — entry point, прогоняет regression или sanitizer
  tests в зависимости от `$NOVA_SANITIZER`.

## Usage

### Plain build (262/262 regression):

```sh
docker build -f docker/Dockerfile -t nova:linux .
docker run --rm nova:linux ./docker/run-tests.sh
```

### Sanitizer builds:

```sh
# ThreadSanitizer (data race detection).
docker build -f docker/Dockerfile -t nova:tsan --build-arg SANITIZER=tsan .
docker run --rm nova:tsan ./docker/run-tests.sh

# AddressSanitizer (use-after-free, buffer overflow).
docker build -f docker/Dockerfile -t nova:asan --build-arg SANITIZER=asan .
docker run --rm nova:asan ./docker/run-tests.sh

# UndefinedBehaviorSanitizer.
docker build -f docker/Dockerfile -t nova:ubsan --build-arg SANITIZER=ubsan .
docker run --rm nova:ubsan ./docker/run-tests.sh
```

## Plan 40 audit findings addressed here

- **R3-6:** TSan + Boehm — `THREAD_LOCAL_ALLOC=0` + `PARALLEL_MARK=0`
  to suppress GC-internals false positives.
- **R3-11:** UBSan `signed-integer-overflow` opt-out in nova_int helpers
  (legitimate wraparound exists).
- **R3-1:** pthread stress tests (Этап 6) register threads with Boehm
  via `GC_register_my_thread()` — see `nova_tests/plan40_sanitizers/`.
- **C4 (round 2):** sanitizers must be **TSan + ASan + UBSan**, not TSan
  alone. ASan would have caught the Materialize crossbeam double-free
  (TSan didn't).

## Tier 1 platforms validated

- Linux x86_64 (Ubuntu 22.04+, glibc 2.35+) — primary CI.
- Linux aarch64 (Tier 2 — build only) — manual `--platform=linux/arm64`.

## Tier 3 NOT supported

- mingw-w64 — нет C11 `<threads.h>`.
- MSVC VS2019 — нет C11 `<stdatomic.h>` (только VS2022 17.5+).
- CentOS 7 / RHEL 7 — glibc 2.17 EOL.
- PA-RISC / Itanium / SPARC.

## Build performance

Первый build (загрузка Ubuntu + apt install + cargo build): ~5-8 min.
Inkremental rebuild (только source изменений): ~30-60 sec через Docker
layer cache.

## Что Этап 5 НЕ покрывает (в Этапе 6)

- pthread stress tests на channel runtime (`nova_tests/plan40_sanitizers/`)
  — bypass fiber scheduler, прямой C-level test для race detection.
- p99 wakeup latency stress (taskset 1 CPU jitter).
- send/recv micro-bench acceptance <50 ns.
