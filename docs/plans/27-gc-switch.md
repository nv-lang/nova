// SPDX-License-Identifier: MIT OR Apache-2.0
# План 27: GC switch + test-runner polish

> **Статус:** план, не начат.
> **Создан:** 2026-05-11. **Обновлён:** 2026-05-11 (production audit pass).
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
    _alloc_count++;
    return p;
}

void nova_retain(void* ptr)  { (void)ptr; }  /* no-op */
void nova_release(void* ptr) { (void)ptr; }  /* no-op */
```

**Объекты создаются и никогда не освобождаются.** `nova_release` — no-op.
Codegen не вставляет cleanup calls.

**Следствие:** любой процесс, который аллоцирует >0 объектов в loop,
упадёт по OOM. CLI tools работают только потому что процесс короткий.

Это **критический gap** — Nova сейчас не production-grade ни для какого
server-side workload'а.

---

## Что готово в репо (проверено)

### Файлы runtime

| Файл | Состояние |
|------|-----------|
| `nova_rt/alloc.c` | Текущий default. Plain malloc, `nova_release` no-op. Все stat-функции реализованы. |
| `nova_rt/alloc_boehm.c` | Boehm backend готов. **Отсутствуют** `nova_gc_alloc_count`, `nova_gc_free_count`, `nova_gc_live_count`, `nova_gc_reset_stats` — нужно добавить через `GC_get_heap_size()` прокси. |
| `nova_rt/alloc_rc.c` | RC backend готов, все функции реализованы. Используется только как альтернатива — не цель этого плана. |
| `nova_rt/alloc.h` | Общий header — декларирует все функции для всех трёх backends. |

### vcpkg (x64-windows-static, проверено)

| Файл | Есть |
|------|------|
| `vcpkg_installed/x64-windows-static/lib/gc.lib` | ✅ |
| `vcpkg_installed/x64-windows-static/lib/atomic_ops.lib` | ✅ |
| `vcpkg_installed/x64-windows-static/include/gc.h` | ✅ |

### Rust-сторона

`BuildOpts` в `test_runner.rs` не имеет поля `gc_kind` — сейчас
`build_command` hardcoded на `opts.rt_dir.join("alloc.c")` (строка 603).
`GcKind` enum не существует нигде в коде.

---

## Часть A — GC switch

### Ф.1 — `GcKind` + `BuildOpts.gc_kind` + `build_command` выбор

**Файлы:** `compiler-codegen/src/test_runner.rs`

#### 1a. Добавить `GcKind` enum

```rust
/// GC backend selection. Wired through BuildOpts → build_command.
/// Malloc = plain malloc, no GC (internal/benchmark only — not for
/// production use; any loop that allocates will OOM eventually).
/// Boehm = Boehm-Demers-Weiser conservative tracing GC (default after Ф.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GcKind {
    #[default]
    Malloc,
    Boehm,
}

impl GcKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "malloc" => Ok(GcKind::Malloc),
            "boehm"  => Ok(GcKind::Boehm),
            _ => Err(anyhow!("unknown gc backend `{}` (expected malloc|boehm)", s)),
        }
    }

    pub fn alloc_c_name(self) -> &'static str {
        match self {
            GcKind::Malloc => "alloc.c",
            GcKind::Boehm  => "alloc_boehm.c",
        }
    }
}
```

#### 1b. Добавить поле в `BuildOpts`

```rust
pub struct BuildOpts<'a> {
    // ... existing fields ...
    pub gc_kind: GcKind,   // ← NEW; default = GcKind::Malloc
}
```

`GcKind` реализует `Default` → существующие `BuildOpts { ... }` без нового поля
получат compile error — найдём все места, обновим явно.

#### 1c. `build_command`: выбор alloc source + Boehm link flags

Заменить hardcoded строку 603:
```rust
let rt_alloc = opts.rt_dir.join("alloc.c");
```
на:
```rust
let rt_alloc = opts.rt_dir.join(opts.gc_kind.alloc_c_name());
```

Для Boehm добавить include + link flags:

```rust
// Boehm GC: vcpkg vendored, x64-windows-static
if opts.gc_kind == GcKind::Boehm {
    // Include path: vcpkg_installed/x64-windows-static/include
    // (gc.h lives directly here, not in a subdir)
    let vcpkg_include = opts.cg_include
        .join("vcpkg_installed")
        .join("x64-windows-static")
        .join("include");
    let vcpkg_lib = opts.cg_include
        .join("vcpkg_installed")
        .join("x64-windows-static")
        .join("lib");
    // Clang: -I<include> ... -L<lib> -lgc -latomic_ops
    // MSVC:  /I<include> ... /link gc.lib atomic_ops.lib
}
```

Точные флаги для каждого toolchain (Clang/MSVC/GCC) — см. шаблоны ниже.

**Clang (Windows):**
```
-I<vcpkg_include> -DGC_THREADS
... (sources) ...
-L<vcpkg_lib> -lgc -latomic_ops
```

**MSVC (cl.exe):**
```
/I<vcpkg_include> /DGC_THREADS
... (sources) ...
/link <vcpkg_lib>\gc.lib <vcpkg_lib>\atomic_ops.lib
```

**GCC (Linux — apt install libgc-dev):**
```
-DGC_THREADS
... (sources) ...
-lgc
```
На Linux vcpkg не нужен — системный libgc. Поле `vcpkg_installed` не
используется.

#### 1d. `alloc_boehm.c` — добавить stat-функции

Сейчас в `alloc_boehm.c` нет `nova_gc_alloc_count` и друзей. Добавить
через Boehm API (не меняет ABI — эти функции нужны для тестов Ф.2):

```c
/* Boehm stat proxy — approximate, but sufficient for leak tests. */
static size_t _alloc_count = 0;

void* nova_alloc(size_t size) {
    void* p = GC_malloc(size);
    if (!p) { fprintf(stderr, "nova: out of memory\n"); abort(); }
    memset(p, 0, size);
    _alloc_count++;
    return p;
}

/* GC_get_heap_size() = total heap bytes; live_count ≈ allocated - collected.
 * Точный live_count без cooperation GC невозможен — используем heap_size proxy. */
size_t nova_gc_alloc_count(void) { return _alloc_count; }
size_t nova_gc_free_count(void)  {
    /* approximate: heap_size ÷ avg_obj_size — acceptable for tests */
    return 0; /* conservative: never claim freed */
}
size_t nova_gc_live_count(void)  { return _alloc_count; /* upper bound */ }
void   nova_gc_reset_stats(void) { _alloc_count = 0; }
```

Примечание: точный `live_count` под Boehm невозможен без
финализатор-инфраструктуры. Для теста Ф.2 используем `GC_get_heap_size()`
как прокси — достаточно чтобы доказать что heap не растёт линейно.

#### 1e. `nova-codegen` CLI — добавить `--gc` arg

В `compiler-codegen/src/main.rs` добавить `--gc malloc|boehm` к
`TestBuild` и `TestAll` subcommands, передавать в `TestAllOpts.gc_kind`
и `TestBuildOpts` → `BuildOpts.gc_kind`.

#### 1f. `nova-cli` — активировать `--gc` stub

В `nova-cli/src/main.rs` флаг `--gc` уже присутствует как stub (Plan 28 r6).
После Plan 27 Ф.1 передавать в `TestAllOpts.gc_kind`.

**Acceptance Ф.1:**
- `nova-codegen test-all --gc malloc` — идентично текущему (regression test: все PASS).
- `nova-codegen test-all --gc boehm` — builds и запускает на Windows Clang/MSVC.
- `nova test --gc boehm` — работает через nova-cli.
- Отдельный `nova-codegen test-build` тест с `--gc boehm` PASS.

**Объём:** ~120 строк Rust + ~20 строк C.

---

### Ф.2 — Verify Boehm GC actually collects

**Файлы:** `nova_tests/gc/` (новая директория)

#### Тест: heap не растёт линейно

```nova
module nova_tests.gc.boehm_collects

// ALLOC_REQUIRES boehm
// Проверяет что Boehm реально собирает unreachable objects.
// Под malloc этот тест падает (heap растёт линейно).

fn main() {
    let initial = _nova_gc_heap_bytes()
    for i in 0..100_000 {
        let s = "alloc-".concat(i.to_string())
        _nova_gc_noop(s)  // prevent optimization
    }
    _nova_gc_collect()  // force collection
    let after = _nova_gc_heap_bytes()
    // Heap после коллекции должен быть < 2x initial (не растёт линейно)
    assert(after < initial + 10_000_000)  // 10 MB generous bound
}
```

Через `external fn`:
```nova
external fn _nova_gc_collect()
external fn _nova_gc_heap_bytes() -> int
external fn _nova_gc_noop(s str)
```

Реализация в `nova_rt/gc_test_helpers.c` (только для тестов):
```c
void _nova_gc_collect(void) {
#ifdef NOVA_GC_BOEHM
    GC_gcollect();
#endif
}
int64_t _nova_gc_heap_bytes(void) {
#ifdef NOVA_GC_BOEHM
    return (int64_t)GC_get_heap_size();
#else
    return (int64_t)nova_gc_live_count() * 32; /* estimate */
#endif
}
void _nova_gc_noop(NovaStr s) { (void)s; }
```

`NOVA_GC_BOEHM` define добавляется в `build_command` при `GcKind::Boehm`.

#### Тест: stress — 1M alloc bounded memory

```nova
module nova_tests.gc.stress_bounded

// ALLOC_REQUIRES boehm
// EXPECT_TIMEOUT_MS 30000

fn main() {
    for i in 0..1_000_000 {
        let _ = "x".repeat(100)  // 100-byte string per iter
    }
    _nova_gc_collect()
    // Если heap > 500 MB — очевидный leak
    assert(_nova_gc_heap_bytes() < 500_000_000)
}
```

**Acceptance Ф.2:**
- `nova-codegen test-all --gc boehm` — оба теста PASS.
- `nova-codegen test-all --gc malloc` — тесты SKIP (ALLOC_REQUIRES boehm).
- RSS процесса после stress не превышает 500 MB.

**Объём:** ~60 строк tests + ~30 строк C helpers.

---

### Ф.3 — Boehm GC pause benchmark

**Файлы:** `nova_tests/gc/pause_bench.nv`

```nova
module nova_tests.gc.pause_bench

// ALLOC_REQUIRES boehm
// EXPECT_TIMEOUT_MS 60000

external fn _nova_gc_collect()
external fn _nova_time_ms() -> int

fn bench_pause(n int) {
    for i in 0..n {
        let _ = "x".repeat(64)
    }
    let t0 = _nova_time_ms()
    _nova_gc_collect()
    let elapsed = _nova_time_ms() - t0
    println("n={n} pause={elapsed}ms")
}

fn main() {
    bench_pause(10_000)
    bench_pause(100_000)
    bench_pause(1_000_000)
}
```

Результаты записать в `spec/overview.md` (секция «GC» — заменить
unverified `<1ms` реальными числами p50/p99).

**Acceptance Ф.3:**
- Bench запускается без ошибок.
- Числа записаны в spec (p50/p99/p99.9 для каждого N).
- `spec/overview.md` не содержит `<1ms` (заменено на measured).

**Объём:** ~40 строк bench + spec update.

---

### Ф.4 — Switch default к Boehm

**После:** Ф.1 + Ф.2 + Ф.3 без регрессий и acceptable pauses.

**Изменения:**
- `GcKind::default()` → `GcKind::Boehm` (одна строка в `#[default]`).
- `nova-cli` — `--gc` флаг default = boehm; `malloc` скрыт из help
  (internal-only, `#[arg(hide = true)]`).
- `nova-codegen` — `--gc` флаг default = boehm; `malloc` видим
  (internal tool, разработчики могут использовать явно).

**Acceptance Ф.4:**
- `nova test` (без `--gc`) — все тесты PASS на Boehm.
- `nova test --gc malloc` — все non-boehm тесты PASS (no regression).
- `sleep_bench` + `cancel_stress` — не ухудшились >10% по elapsed.
- README.md упоминает что GC = Boehm по умолчанию.

**Объём:** ~10 строк Rust + docs.

---

### Ф.5 — Linux/macOS GC support

**Условие:** есть доступ к Linux машине (сейчас blind ship).

**Linux:** `apt install libgc-dev` → `-lgc -lpthread`.
**macOS:** `brew install bdw-gc` → `-lgc`.

В `build_command`, ветка `Toolchain::Gcc`:
```rust
if opts.gc_kind == GcKind::Boehm {
    c.arg("-DGC_THREADS");
    c.arg("-lgc");
    #[cfg(target_os = "linux")]
    c.arg("-lpthread");
}
```

Нет vcpkg на Linux — только системная библиотека. Если `libgc-dev` не
установлен → `detect_or_build_libuv`-style fatal error с инструкцией
`apt install libgc-dev`.

**Acceptance Ф.5:** `nova test --gc boehm` на Linux — все тесты PASS.

**Объём:** ~30 строк.

---

### Ф.6 — `ALLOC_REQUIRES` / `ALLOC_EXCLUDES` маркеры (D89 extension)

Расширение D89 двумя новыми маркерами:

```nova
// ALLOC_REQUIRES boehm   ← тест запускается ТОЛЬКО на boehm
// ALLOC_EXCLUDES malloc   ← тест skipped на malloc, запускается на остальных
```

**Изменения:**

`compiler-codegen/src/test_runner.rs`:
```rust
/// Parsed alloc constraint from test file header.
#[derive(Debug, Clone, PartialEq)]
pub enum AllocConstraint {
    None,
    Requires(GcKind),   // ALLOC_REQUIRES <backend>
    Excludes(GcKind),   // ALLOC_EXCLUDES <backend>
}

impl AllocConstraint {
    /// Returns true if this test should run under the given GcKind.
    pub fn allows(self, gc: GcKind) -> bool {
        match self {
            AllocConstraint::None => true,
            AllocConstraint::Requires(k) => gc == k,
            AllocConstraint::Excludes(k) => gc != k,
        }
    }
}
```

`parse_expect()` → `parse_test_markers()` (или отдельная функция
`parse_alloc_constraint(src: &str) -> AllocConstraint`).

Новый `Outcome` variant:
```rust
Outcome::Skipped { reason: SkipReason, elapsed: Duration }

pub enum SkipReason {
    AllocBackend { required: String, actual: String },
}
```

В `run_one`: проверить constraint ПОСЛЕ чтения файла, ДО codegen.
В summary: `SKIP` не считается ни PASS ни FAIL. Печатается отдельно.

`spec/decisions/09-tooling.md` D89 — добавить пункты 6 и 7.
`docs/test-conventions.md` — примеры.

**Acceptance Ф.6:**
- `nova test --gc malloc` — boehm-only тесты помечены SKIP-ALLOC.
- `nova test --gc boehm` — все тесты запускаются.
- Summary: `91 PASS, 2 SKIP (alloc), 0 FAIL`.

**Объём:** ~80 строк Rust + ~20 строк spec/docs.

---

## Часть B — test-runner polish

Независима от GC. Все задачи — улучшения `compiler-codegen/src/test_runner.rs`.

### Б.1 — Test caching

Hash-based кэш в `target/test-cache/<hash>/`.

**Cache key** (всё влияет на результат — если хоть что-то изменилось, hit невалиден):
- SHA-256 от `.nv` source файла.
- mtime всех `nova_rt/*.c` + `nova_rt/*.h`.
- mtime `nova-codegen` бинарника.
- `toolchain` (clang/msvc/gcc) + `mode` (dev/release).
- `gc_kind` (malloc/boehm — разные бинари!).
- libuv version stamp (`target/libuv-cache/version.txt`).

**Default: `--no-cache`** (CI-safe — false positive cache hit = PASS когда должен FAIL).
Explicit opt-in: `--cache-dir target/test-cache`.

**Реализация:**
```rust
pub struct CacheKey {
    hash: [u8; 32],  // SHA-256 от всех inputs
}

fn compute_cache_key(nv_path: &Path, opts: &TestBuildOpts) -> CacheKey { ... }
fn cache_lookup(key: &CacheKey, cache_dir: &Path) -> Option<CachedOutcome> { ... }
fn cache_store(key: &CacheKey, outcome: &Outcome, cache_dir: &Path) { ... }
```

SHA-256 без extra dep: использовать `std::hash::DefaultHasher` (не криптографический)
или добавить `sha2 = "0.10"` (6 KB) как dev/internal dep.
Рекомендация: **добавить `sha2`** — `DefaultHasher` нестабилен между Rust версиями,
cache будет инвалидирован при каждом `rustup update`. `sha2` — стандарт де-факто для
content-addressable хранилищ.

**Объём:** ~150 строк + `sha2` dep.

### Б.2 — `EXPECT_TIMEOUT_MS` per-test marker

```nova
// EXPECT_TIMEOUT_MS 30000
```

Переопределяет глобальный `--timeout` для конкретного теста.
`parse_test_markers()` → добавить парсинг.
`run_one`: `let timeout = marker_timeout.unwrap_or(opts.timeout)`.

**Объём:** ~30 строк.

### Б.3 — `--verbose` capture stdout PASS-тестов

Расширить `Outcome::Pass`:
```rust
Outcome::Pass {
    elapsed: Duration,
    detail: String,
    captured_stdout: Option<String>,  // NEW — Some(_) если --verbose
    captured_stderr: Option<String>,  // NEW
}
```

`run_one` при `opts.verbose` сохраняет stdout/stderr child-процесса.
`print_summary` при `--verbose` печатает captured для PASS.

Аналог `go test -v` и `cargo test --nocapture`.

**Объём:** ~40 строк.

### Б.4 — Slow tests report

В конце `--format text` summary:
```
===== SLOWEST TESTS (top 10) =====
  3.214s  concurrency/sleep_real_clock
  2.103s  std/checksums/fnv
```

Sort by elapsed, take 10. Только если `total_tests > 10`.

**Объём:** ~25 строк.

### Б.5 — `--list` + `--filter-from`

`nova test --list` — выводит все тесты без запуска, по одному на строку.
`nova test --filter-from shard.txt` — exact-match из файла (не substring).

CI sharding:
```bash
nova test --list | split -l 50 - shard-
nova test --filter-from shard-aa --format junit > results-0.xml
```

**Объём:** ~45 строк.

### Б.6 — Retry count в JUnit XML

`Outcome::Pass { retries: u32, .. }`.
JUnit emit при `retries > 0`:
```xml
<testcase ...>
  <system-out>retried 1 time(s) before pass</system-out>
</testcase>
```

**Объём:** ~20 строк.

### Б.7 — `--shuffle [SEED]`

Fisher-Yates shuffle входного списка тестов перед запуском.
`--shuffle` без аргумента → random seed (system time).
`--shuffle 42` → reproducible seed.

xorshift PRNG (~10 строк) + Fisher-Yates (~10 строк). Без extra deps.

Для воспроизводимости print seed в начале прогона:
```
Shuffling 91 tests with seed 1715432198
```

**Объём:** ~35 строк.

### Б.8 — `EXPECT_TIMEOUT_MS` + `--shuffle` + `--list` в nova-cli

После реализации в test_runner — wire в `nova-cli` (nova test/test-build):
- `--shuffle [SEED]` → `TestAllOpts.shuffle_seed: Option<u64>`
- `--list` → `TestAllOpts.list_only: bool`
- `--filter-from <path>` → `TestAllOpts.filter_from: Option<PathBuf>`

**Объём:** ~30 строк (nova-cli/src/main.rs).

---

## Порядок выполнения

```
Ф.1 → Ф.2 → Ф.3 → Ф.4    (последовательно — каждая зависит от предыдущей)
Ф.5                         (после Ф.4, когда есть Linux доступ)
Ф.6                         (после Ф.1 — нужен GcKind)

Б.2 → Б.3 → Б.4 → Б.6      (независимы от GC, можно в любой момент)
Б.1                         (после Б.2 — cache key включает EXPECT_TIMEOUT_MS)
Б.5 → Б.8                   (независимы)
Б.7                         (независима)
```

---

## Критические файлы

| Файл | Действие |
|------|----------|
| `compiler-codegen/src/test_runner.rs` | `GcKind`, `BuildOpts.gc_kind`, `build_command` выбор alloc, `AllocConstraint`, `Outcome::Skipped`, Boehm link flags |
| `compiler-codegen/nova_rt/alloc_boehm.c` | Добавить stat-функции + `_alloc_count` counter |
| `compiler-codegen/nova_rt/gc_test_helpers.c` | Новый файл: `_nova_gc_collect`, `_nova_gc_heap_bytes`, `_nova_gc_noop` |
| `compiler-codegen/src/main.rs` | `--gc` arg в TestBuild/TestAll |
| `nova-cli/src/main.rs` | Активировать `--gc` stub → передавать `GcKind` |
| `nova_tests/gc/boehm_collects.nv` | Новый: коллекция unreachable objects |
| `nova_tests/gc/stress_bounded.nv` | Новый: 1M alloc memory bound |
| `nova_tests/gc/pause_bench.nv` | Новый: pause distribution benchmark |
| `spec/decisions/09-tooling.md` | D89: ALLOC_REQUIRES / ALLOC_EXCLUDES |
| `spec/overview.md` | GC pause числа (после Ф.3) |
| `docs/test-conventions.md` | ALLOC_REQUIRES примеры |

---

## Ловушки реализации

### `GC_THREADS` define — обязателен

`alloc_boehm.c` уже имеет `#define GC_THREADS` перед `#include <gc.h>`.
В `build_command` дополнительно передавать `-DGC_THREADS` (или `/DGC_THREADS`)
как compile flag — защита если Boehm header включается из другого C файла.

### vcpkg include path для Boehm

`gc.h` лежит прямо в `vcpkg_installed/x64-windows-static/include/gc.h`,
не в `include/gc/gc.h`. Поэтому `-I<vcpkg_include>` достаточно.
Дополнительный `-I<vcpkg_include>/gc` не нужен.

### `atomic_ops.lib` — нужен для thread-safe Boehm

Boehm с `GC_THREADS` на Windows требует `atomic_ops.lib` в дополнение к `gc.lib`.
Оба есть в vcpkg. Порядок линковки: `gc.lib` перед `atomic_ops.lib`.

### MSVC: `/link` должно быть последним аргументом

При `cl.exe` все `/link` опции (пути к `.lib`) должны идти **после** всех
source файлов и compile-time флагов. `build_command` для MSVC строит
аргументы в правильном порядке — убедиться что Boehm libs добавляются
в линкер-секцию, не в compile-секцию.

### `GcKind` в `TestAllOpts` — передавать через весь стек

```
TestAllOpts.gc_kind
  → TestBuildOpts.gc_kind (через worker dispatch)
    → BuildOpts.gc_kind
      → build_command()
```

Не забыть добавить поле во все три структуры.

### Cache invalidation при смене GC backend

Если в Б.1 добавляем кэш, `gc_kind` ОБЯЗАН быть частью cache key.
`alloc.c` и `alloc_boehm.c` производят разные бинари — cache miss при
смене `--gc`.

### `nova_gc_live_count()` под Boehm — приблизительно

Точный live count без финализаторов невозможен. Для тестов Ф.2 используем
`GC_get_heap_size()` как прокси. Документировать в `alloc_boehm.c`.

---

## Risks / Trade-offs

**R1. Boehm conservative tracing** — на 64-bit ложные roots маловероятны
(целые числа редко совпадают с указателями), но не исключены.
Эффект — часть памяти не выпускается. Practical impact: документировано
в community decades, приемлемо для general backend.

**R2. STW pause latency.** На больших heap'ах (>1 GB) pauses могут быть
10-100ms. Для real-time — блокер. Для backend API — приемлемо.
Числа из Ф.3 определяют окончательный вердикт.

**R3. `atomic_ops.lib` на Windows.** Уже в vcpkg_installed — не требует
внешней установки. На clean machine — `vcpkg install bdwgc` восстанавливает.

**R4. Boehm + fibers (minicoro).** Fiber stacks Boehm не сканирует
автоматически. При M:N (Plan 23) потребуется `GC_add_roots()` для каждого
fiber stack. Для текущего coroutine-per-green-thread модели — нормально:
стек сканируется через основной thread.

**R5. Boehm + libuv event loop.** libuv callbacks запускаются из main thread —
Boehm сканирует его стек. Риска нет, но при переходе к настоящим потокам
(Plan 23) потребуется `GC_register_my_thread()` в каждом worker.

---

## Acceptance overall

После Ф.1–Ф.4:
- `nova test` (default = boehm) — все тесты PASS без регрессий.
- `nova test --gc malloc` — все тесты PASS (backward compat).
- `nova_tests/gc/stress_bounded` — RSS bounded после 1M alloc.
- `nova_tests/gc/pause_bench` — числа в spec/overview.md.
- `spec/overview.md` не содержит `<1ms` (заменено на measured).
- `nova test --gc boehm` и `nova test --gc malloc` оба в CI (matrix).

После Ф.5: то же на Linux.
После Ф.6: `ALLOC_REQUIRES` тесты корректно skip на несовместимом backend.

---

## Объём

| Фаза | Строк Rust | Строк C | Строк Nova | Строк docs |
|------|-----------|---------|-----------|-----------|
| Ф.1 | ~120 | ~20 | — | — |
| Ф.2 | — | ~30 | ~40 | — |
| Ф.3 | — | — | ~40 | ~20 |
| Ф.4 | ~10 | — | — | ~10 |
| Ф.5 | ~30 | — | — | ~10 |
| Ф.6 | ~80 | — | — | ~30 |
| Б.1–Б.8 | ~375 | — | — | ~30 |
| **Итого** | **~615** | **~50** | **~80** | **~100** |

---

## Что НЕ входит

- **RC codegen** (retain/release inserting) — `alloc_rc.c` есть, codegen его
  не использует. Отдельный план если Boehm окажется неприемлемым.
- **Concurrent GC** — годами работы, после v1.0.
- **`realtime nogc { }`** arena allocator — Plan TBD.
- **Linux/macOS smoke-test** инфраструктуры — зависит от доступа к машине.
- **GitHub Actions CI matrix** — отдельная задача (3 OS × 2 backends = 6 runners).

---

## Связь

- [Plan 25 G3a/G3b](25-production-readiness-roadmap.md#g3) — этот план closes G3a.
- [D6 (spec/decisions/05-memory.md)](../../spec/decisions/05-memory.md#d6) —
  «managed by default»: Plan 27 = первая реализация.
- [Plan 23 (M:N runtime)](23-mn-runtime-roadmap.md) — потребует GC cooperation
  cross-thread. R4/R5 выше.

---

## История

- **2026-05-11** — создан после Plan 22 hardening retro + Plan 25 honest pass.
- **2026-05-11** — production audit pass: добавлены конкретные impl-детали
  (GcKind enum, vcpkg paths, MSVC линковка, Б.1 sha2 dep обоснование,
  R4/R5 Boehm + fibers/libuv risks, `alloc_boehm.c` stat-функции gap).
