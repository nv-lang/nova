// SPDX-License-Identifier: MIT OR Apache-2.0
# План 27: GC switch + test-runner polish

> **Статус:** план, не начат.
> **Создан:** 2026-05-11.
> **Приоритет:** ВЫСОКИЙ — текущий default `alloc.c` (plain malloc)
> делает Nova **непригодным** для long-running workloads.
> **Зависит от:** —
> **Открыт:** Plan 25 G3a; Plan 26 production review.
>
> **Структура:** Часть A — GC switch (D6 compliance, основная цель).
> Часть B — test-runner polish (carry-over из Plan 26 review).

---

## Зачем

Текущий runtime: `compiler-codegen/nova_rt/alloc.c` — Phase-0 implementation:

```c
void* nova_alloc(size_t size) {
    void* p = malloc(size);
    /* ...check OOM... */
    _alloc_count++;
    return p;
}

void nova_retain(void* ptr)  { (void)ptr; }  /* no-op */
void nova_release(void* ptr) { (void)ptr; }  /* no-op */
```

**Объекты создаются и никогда не освобождаются.** `nova_release` —
no-op. Codegen не вставляет cleanup calls.

**Следствие:** любой процесс, который аллоцирует >0 объектов в loop,
упадёт по OOM. CLI tools работают только потому что процесс короткий
(аллоцирует < доступной RAM).

Это **критический gap** — Nova сейчас не production-grade ни для какого
server-side workload'а, даже single-traffic-host.

## Что готово в репо

**Boehm GC backend:** `compiler-codegen/nova_rt/alloc_boehm.c`.

```c
void* nova_alloc(size_t size) {
    void* p = GC_malloc(size);
    /* ...check OOM... */
    memset(p, 0, size);
    return p;
}
```

Использует [bdwgc](https://github.com/ivmai/bdwgc) (Boehm-Demers-Weiser GC) —
**production-grade** mark-and-sweep, conservative tracing GC,
работает с любым C codegen без cooperation.

**vcpkg packages уже установлены** в репо:

- `compiler-codegen/vcpkg_installed/x64-windows-static/lib/gc.lib`
- `compiler-codegen/vcpkg_installed/x64-windows-static/include/gc.h`
- `compiler-codegen/vcpkg_installed/x64-windows-static/include/gc/`

Зависит ещё от `atomic_ops.lib` (тоже в vcpkg) для thread-safe operations.

## Чего НЕ хватает

Test-runner и codegen build chains hardcoded используют **только**
`alloc.c`. Нужно:

1. **Build flag `--gc=malloc|boehm`** в `nova-codegen test-build` и
   `test-all` (default определить после Шага 4).
2. **Conditional source pick** в `test_runner.rs:build_command`: вместо
   hardcoded `rt_alloc = opts.rt_dir.join("alloc.c")` — выбор по
   GC kind.
3. **Conditional link flags** для Boehm:
   - Windows: `gc.lib`, `atomic_ops.lib` + include path.
   - Linux/macOS: `-lgc` (apt install libgc-dev / brew install bdw-gc).
4. **Cross-platform vcpkg** — текущий x64-windows-static.
   Linux/macOS — system package managers либо vendored build.

## Фазы

### Ф.1 — Add `--gc` flag, malloc default

**Что:**
- `--gc=malloc|boehm` в `test-build` и `test-all` CLI args.
- Default: `malloc` (текущее поведение, no regression).
- `build_command` выбирает alloc source по flag.
- Boehm path добавляет gc.h include + gc.lib link для Windows.

**Файлы:**
- `compiler-codegen/src/main.rs` — добавить `--gc` arg в TestBuild/TestAll.
- `compiler-codegen/src/test_runner.rs`:
  - `BuildOpts` field `gc_kind: GcKind { Malloc, Boehm }`.
  - `build_command`: выбор `alloc.c` либо `alloc_boehm.c` по kind.
  - Conditional include path + link для Boehm.
- `run_tests.ps1`/`run_tests.sh`: pass-through `-Gc` parameter.

**Acceptance:**
- `nova-codegen test-all --gc malloc` works как сейчас (regression).
- `nova-codegen test-all --gc boehm` builds успешно на Windows MSVC/Clang.
- Smoke test: simple alloc-loop процесс не растёт unbounded под Boehm.

**Объём:** ~100 строк Rust + path config.

### Ф.2 — Verify Boehm GC actually collects

**Что:** test что Boehm реально освобождает unreachable objects.

**Tests:**

```nova
test "boehm gc collects unreachable objects" {
    // Force-create 100k records, drop reference, force gc.
    let mut last int = 0
    for i in 0..100_000 {
        let p = Point { x: i, y: i }  // unreachable после iter
        last = p.x
    }
    // Под malloc: live_count == ~100_000.
    // Под Boehm: live_count << 100_000 (collected).
    // Probe через extern nova_gc_live_count().
    let live = _nova_gc_live_count()
    assert(live < 50_000)  // generous bound
}
```

Через `external fn _nova_gc_live_count() -> int` для probing.

**Acceptance:**
- Bench under `--gc boehm` shows live_count << alloc_count после workload'а.
- Under `--gc malloc` тест fail'ит (как expected — leak forever).
- Long-running stress (10M alloc/drop iter) bounded в memory.

**Объём:** ~50 строк test + minor codegen extern probe.

### Ф.3 — Boehm GC pause benchmark

**Что:** measure pause times на realistic workloads.

**Bench:**
- 10k objects allocated → force `GC_gcollect()` → measure ms.
- 100k objects allocated → same.
- 1M objects allocated → same.
- 10M objects allocated → same (если RAM позволяет).

**Acceptance:**
- Bench numbers записаны в spec/overview.md (замена unverified «<1ms»).
- Pause distribution (p50/p99/p99.9) per heap size.

**Объём:** ~80 строк bench + spec update.

### Ф.4 — Switch default к Boehm

**Что:** после Ф.1-Ф.3 если Boehm fit (pauses acceptable, no
regressions) — сделать default GC.

**Trade-off:**
- **Боenhm pros:** correct cycle collection, no cooperation needed,
  production-tested. Default = production-ready behavior.
- **Boehm cons:** STW pauses (10-100ms на больших heap'ах), conservative
  tracing (может удержать память дольше, false roots).
- **Альтернатива** — RC через alloc_rc.c + codegen-rewrite. Это
  отдельный план (Plan 28 если потребуется).

**Acceptance:**
- `test-all` без `--gc` использует Boehm.
- 138/138 regression PASS.
- `nova_alloc` overhead не ухудшает sleep_bench / cancel_stress больше
  чем на 10%.

**Объём:** small (флаг default + docs).

### Ф.5 — Linux/macOS GC support

**Что:** Boehm GC на Linux/macOS.

**Linux:** `apt install libgc-dev` либо vendored build. `-lgc` link.
**macOS:** `brew install bdw-gc` либо vendored. `-lgc` link.

**Объём:** ~50 строк build invoker.

### Ф.6 — ALLOC_REQUIRES / ALLOC_EXCLUDES маркеры

**Что:** не все тесты имеют смысл на всех 3 backends:

- `concurrency/deep_gc.nv` — **только boehm** (тестирует cycle collection).
- `concurrency/sleep_leak_check.nv` — **только boehm** (memory leak detection).
- `basics/*.nv` — **все backends** (basic correctness).
- Будущие RC-specific тесты — **только rc**.

Решение — расширить D89 двумя маркерами (per-file аннотация):

```nova
// ALLOC_REQUIRES boehm
test "cycle collection works" { ... }
```

Test-runner skip'ает с status `SKIP-ALLOC` если current `--gc` не
match. Семантика:

- `ALLOC_REQUIRES <backend>` — тест запускается **только** на этом
  backend'е, на других skipped.
- `ALLOC_EXCLUDES <backend>` — тест skipped на указанном backend'е,
  запускается на остальных.

D89 spec обновить с двумя новыми маркерами.

**Файлы:**
- `compiler-codegen/src/test_runner.rs` — `parse_expect` расширяется
  или отдельный `parse_alloc_constraint`; новый `Status::SkippedAlloc`
  variant.
- `spec/decisions/09-tooling.md` D89 — секция «6. ALLOC_REQUIRES /
  7. ALLOC_EXCLUDES».
- `docs/test-conventions.md` — примеры use-case.

**Acceptance:**
- Тест `concurrency/deep_gc` skipped на `--gc malloc`, runs на
  `--gc boehm`.
- Полный прогон `--gc malloc` и `--gc boehm` оба green (учитывая
  skips).

**Объём:** ~60 строк (parser + skip-logic + status variant).

---

## Часть B — test-runner polish (carry-over из Plan 26)

После Plan 26 closure (~99% production-grade) осталось 7 polish-задач
из cargo-nextest / go test parity. Они **независимы от GC**, но
включены в этот же план для одного логического milestone.

### Б.1 — Ф.5 test caching (carry-over Plan 26)

Hash-based test caching в `target/test-cache/<hash>/`. Cache key:

- SHA-256 от `.nv` source.
- mtime всех `nova_rt/*.c` + `nova_rt/*.h`.
- mtime `nova-codegen.exe`.
- Toolchain (clang/msvc/gcc) + mode (dev/release) + alloc backend
  (важно — boehm vs malloc разные binary).
- libuv enabled/disabled.

Cache hit → пропустить codegen + cc, сразу use cached `.exe`.

CLI: `--no-cache` (force rebuild), `--cache-dir <path>` (override).
Default: `--no-cache` (CI safe; explicit opt-in для local TDD).

**Sensitivity high** — false-positive cache hit (когда invariant
нарушен) даёт «PASS» когда должен FAIL. Поэтому disabled-by-default.

Объём: ~150 строк.

### Б.2 — EXPECT_TIMEOUT_MS per-test marker

Сейчас все тесты используют global `--timeout`. Real-world test'ы
имеют разные SLA:

- `basics/literals` — 5s хватит (только codegen+cc overhead).
- `concurrency/sleep_leak_check` — 15s budget на bench-loop.

Решение — D89 8-й маркер `EXPECT_TIMEOUT_MS <N>`:

```nova
// EXPECT_TIMEOUT_MS 30000
test "long bench" { ... }
```

Per-test override `opts.timeout` в `run_one`. CLI `--timeout` остаётся
fallback для тестов без маркера.

Польза: можно ставить `--timeout 5` глобально (real hang'и видны
немедленно), а long-running тесты сами override'ят.

Объём: ~30 строк.

### Б.3 — `--verbose` capture stdout PASS-тестов

Сейчас `Verbosity::Verbose` — маркер без эффекта (см. `TODO` comment
в `test_runner.rs`). Реализация:

- `run_one` сохраняет `stdout`/`stderr` в `Outcome::Pass {
  captured_stdout: Option<String>, captured_stderr: Option<String> }`.
- `print_summary` в `--verbose` показывает captured для PASS-тестов.

Аналог `go test -v` (показывает `t.Logf`) и `cargo test --nocapture`.

Объём: ~30 строк + расширение Outcome enum (backwards-compatible).

### Б.4 — Slow tests report

В конце прогона `--format text` показывать top-10 самых медленных:

```
===== SLOWEST TESTS =====
  3.214s  concurrency/sleep_real_clock
  2.103s  std/checksums/fnv
  ...
```

Аналог `cargo test --report-time` / `go test -v` (per-test elapsed).
Помогает identify candidates для optimization.

Объём: ~20 строк (sort by elapsed, take 10, format).

### Б.5 — `--list` + `--filter-from` для CI sharding

`nova-codegen test-all --list` — выводит все тесты по одному на строку,
без запуска. CI делит между runner'ами:

```bash
./run_tests.sh --list | awk "NR % 4 == 0" > shard-0.txt
./run_tests.sh --filter-from shard-0.txt --format junit
```

`--filter-from <file>` — новый flag, читает имена тестов из файла,
прогоняет только их (exact-match, не substring как `--filter`).

При scale 5000+ тестов (когда будет self-host) — необходимо для
CI parallel runners.

Объём: ~40 строк.

### Б.6 — Retry count в JUnit XML

Если `--retries 2` и тест PASS после retry — это **информация для
CI report**. JUnit support'ит через custom attribute либо
`<system-out>`-комментарий:

```xml
<testcase classname="basics" name="literals" time="0.234">
  <system-out>retried 1 time before pass</system-out>
</testcase>
```

CI dashboards (GitHub Actions, GitLab) показывают это как warning —
сигнал что flaky tests появляются.

`Outcome::Pass { retries: u32 }`. Если `retries > 0` — emit
`<system-out>`.

Объём: ~15 строк.

### Б.7 — TempSubdir Drop pattern

Сейчас `tmp_dir/t-<hash>/` создаётся явно + cleanup явно в конце
`run_one`. Если worker panic'ит до cleanup — directory orphan'ится.
`cargo` использует `tempfile` crate с Drop trait — automatic cleanup.

Но `tempfile` — extra dependency (~50 KB binary). Для bootstrap
compiler'а лишнее. Own `TempSubdir` с Drop ~30 строк:

```rust
struct TempSubdir(PathBuf);
impl Drop for TempSubdir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
```

Plus `--keep-artifacts` через `std::mem::forget` или `into_path()`
escape hatch.

Объём: ~30 строк.

### Б.8 — `--shuffle [SEED]` для flakiness detection

Сейчас `inputs.sort_by(|a, b| a.0.cmp(&b.0))` — детерминированный
alphabetical. Flaky тесты, проявляющиеся только на определённом
ordering (test1 leak'ит state, test2 fail'ит), мы **не найдём**.

`cargo-nextest --seed N` shuffle тесты для flakiness detection.

CLI: `--shuffle [SEED]`. SEED=0 — random (system time), N — explicit
seed для reproducibility.

Implementation: xorshift PRNG (~10 строк) + Fisher-Yates shuffle
(~10 строк). Без extra deps.

Объём: ~30 строк.

### Б.9 — Slow-tests reporting через JSON output

Дополнительно — `--format json` уже включает `elapsed_ms` в каждом
event'е. CI parser'ы могут сами top-N извлечь. В text-mode (Б.4) для
local dev — встроено.

### Acceptance criteria часть B

- ✅ `--cache-dir target/test-cache` — переиспользование между прогонами,
  10× speedup при unchanged sources.
- ✅ `EXPECT_TIMEOUT_MS 30000` в `.nv` overrides `--timeout`.
- ✅ `-v` показывает stdout PASS-тестов.
- ✅ Text-mode summary включает SLOWEST TESTS список.
- ✅ `--list` + `--filter-from` для CI sharding.
- ✅ JUnit `<testcase>` помечает retried tests через `<system-out>`.
- ✅ Tmp directory cleanup при panic worker'а.
- ✅ `--shuffle [SEED]` для flakiness detection.

### Объём части B

~330 строк Rust + ~50 строк tests + ~30 строк docs.

## Risks / Trade-offs

**R1. Boehm conservative tracing** — на 64-bit machine ложные roots
маловероятны, но возможны (integer выглядит как pointer). Эффект —
**memory не выпускается** в некоторых случаях. Practical impact:
generally небольшой, документировано в Boehm community decades.

**R2. STW pause latency.** На больших heap'ах (>1 GB) pauses могут быть
10-100ms. Для real-time это блокер. Для general backend — приемлемо.

**R3. vcpkg dependency на Windows.** vcpkg_installed уже vendored
в репо — это **plus**, не нужно external setup. На clean machine —
требуется restore (vcpkg install bdwgc). Documented in build chain.

**R4. Boehm has thread-stack scanning** — может тормозить fiber-heavy
workloads. Под M:N (Plan 23) потребует tuning.

## Acceptance overall

После Ф.1-Ф.4:

- `memory_growth_check` тест: 10M alloc/release iter, RSS bounded
  (не растёт linearly).
- `sleep_bench` 10k concurrent: no regression больше 10%.
- Spec/overview.md: «GC pause distribution» секция с measured
  numbers (вместо unverified `<1ms`).

После Ф.5: same на Linux.

## Что НЕ входит

- **RC codegen** (alloc_rc.c с retain/release inserting в codegen) —
  Plan 28 если потребуется альтернатива Boehm. (`alloc_rc.c` runtime
  есть, но codegen его не использует.)
- **Concurrent GC custom** — годами работы, после v1.0.
- **realtime nogc { }** arena allocator — Plan TBD, partial
  implementation.
- **Linux/macOS smoke-test** test-runner'а — отложен до access (Plan
  26 known limitation).
- **GitHub Actions CI workflow** — отдельная задача, но Plan 27 готовит
  всё для matrix: 3 OS × 2 alloc backends = 6 runners.

## Связь

- [Plan 25 G3a/G3b](25-production-readiness-roadmap.md#g3) — этот план
  closes G3a, частично G3b.
- [D6 (spec/decisions/05-memory.md)](../../spec/decisions/05-memory.md#d6) —
  «managed by default» дизайн-decision, Plan 27 = первая реализация.
- [Plan 23 (M:N runtime)](23-mn-runtime-roadmap.md) — потребует GC
  cooperation cross-thread. Boehm thread-safe готов, RC потребует
  atomic refcount.

## История

- **2026-05-11** — создан после Plan 22 hardening retro + Plan 25 honest
  pass: обнаружено что default alloc = plain malloc без GC, любой
  long-running процесс leaks. vcpkg gc.lib уже vendored — switch к
  Boehm = ~1 день инженерной работы.
