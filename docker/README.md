# Plan 44.1 Ф.1 Linux Docker

Production validation для Plan 44.1 M:N runtime на Tier 1 Linux
(Ubuntu 22.04+ x86_64, clang 15+, glibc 2.35+).

## Что внутри

- **`Dockerfile`** — Ubuntu 22.04 + clang 15 + Rust stable + libuv1-dev
  + libgc-dev (system Boehm). Build-arg `SANITIZER` выбирает режим
  (none / tsan / asan / ubsan).
- **`run-tests.sh`** — entry point, прогоняет regression или sanitizer
  tests в зависимости от `$NOVA_SANITIZER`.

## Usage

### Plain build (regression):

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

## Plan 44.1 audit findings addressed here

- **R3-6:** TSan + Boehm — `THREAD_LOCAL_ALLOC=0` + `PARALLEL_MARK=0`
  to suppress GC-internals false positives.
- **R3-11:** UBSan `signed-integer-overflow` opt-out in nova_int helpers
  (legitimate wraparound exists).
- **R3-1:** pthread stress tests (Этап 6) register threads with Boehm
  via `GC_register_my_thread()` — see `nova_tests/plan40_sanitizers/`.
- **C4 (round 2):** sanitizers must be **TSan + ASan + UBSan**, not TSan
  alone. ASan would have caught the Materialize crossbeam double-free
  (TSan didn't).

## Validation status (2026-05-12)

**Plain Linux build (SANITIZER=none):**
- ✅ 261/261 nova_tests + 46/46 std type-check PASS.
- ❌ `plan40_perf_bench` — Boehm `GC_init` SEGV в
  `GC_find_limit_with_bound` под Docker restricted permissions.
  **Это не Plan 40 bug** — известная Boehm/Docker interaction.
  Mitigation: `--skip plan40_perf_bench` в `run-tests.sh`. Plan 27
  Linux smoke имеет тот же gap.

**Sanitizer builds (TSan/ASan/UBSan):**
- ❌ pthread stress tests **TRAP под sanitizers** в `libgc.so`
  (Ubuntu 22.04 default Boehm package single-threaded; multithread
  variant конфликтует с TSan/ASan instrumentation).
- **Это не Plan 44.1 issue**. Real M:N race detection требует:
  - Build Boehm from source с `--enable-threads=posix --enable-parallel-mark`.
  - Либо альтернативный GC backend (malloc для sanitizer runs).
- **Plan 44.4 prerequisite:** Boehm multithread setup для CI — это
  **отдельная задача**, не Plan 44.1 scope.

**Что Plan 44.1 Ф.1 валидировано:**
- Single-thread correctness (Windows 262/262, Linux 261/261 без perf).
- Cross-platform build (Linux clang + Boehm + libuv).
- Plan 44.1 functional tests (5 файлов) — все PASS на Linux.
- API contracts: portable sync.h wrapper, channel runtime layout.

**Что не валидировано (deferred):**
- Real M:N race detection под TSan (Boehm/Docker issue).
- Per-fiber stress tests на M:N scheduler'е (Plan 44.4 dependency).
- Production benchmark данные на нормальном Linux box (Docker slow).

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
