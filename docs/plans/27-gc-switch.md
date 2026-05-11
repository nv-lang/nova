// SPDX-License-Identifier: MIT OR Apache-2.0
# План 27: GC switch + test-runner polish

> **Статус:** в работе. Ф.1 ✅ (2026-05-11). Ф.1.5 ✅ (2026-05-11). Ф.2 ✅ (2026-05-11).
> **Создан:** 2026-05-11. **Обновлён:** 2026-05-11 (Ф.1.5 + Ф.2 completed).
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
    void* p = malloc(size);   // НЕ zeroed!
    _alloc_count++;
    return p;
}

void nova_retain(void* ptr)  { (void)ptr; }  /* no-op */
void nova_release(void* ptr) { (void)ptr; }  /* no-op */
```

**Два критических gap:**

**1. Объекты создаются и никогда не освобождаются.** `nova_release` — no-op.
Codegen не вставляет cleanup calls. Любой процесс который аллоцирует >0 объектов
в loop упадёт по OOM. CLI tools работают только потому что процесс короткий.

**2. `malloc` не zeroes память.** Codegen в emit_c.rs явно полагается на то,
что `nova_alloc returns zeroed memory` (comment at line 1891). Структуры (records,
closures, spawn contexts, vtables) инициализируются через присвоение конкретных
полей — остальные поля могут остаться мусором. Сейчас это маскируется тем, что
программы короткие и heap-значения редко читаются без инициализации. Boehm
(`GC_malloc`) и RC (`malloc` + `memset(p, 0, size)`) оба zeroed — так что после
переключения на них баги исчезают. **Нужно зафиксировать ожидание в alloc.h и
добавить `calloc` в `alloc.c`.**

Это **критические gap** — Nova сейчас не production-grade ни для какого
server-side workload'а.

---

## Что готово в репо (проверено)

### Файлы runtime (актуальное состояние)

| Файл | Состояние |
|------|-----------|
| `nova_rt/alloc.c` | Текущий default. `malloc` — **НЕ zeroed**. `nova_release` no-op. Все stat-функции реализованы. |
| `nova_rt/alloc_boehm.c` | Boehm backend. Ф.1 добавил `_alloc_count` counter + все stat-функции. Zeroed через `GC_malloc + memset` (redundant — GC_malloc уже zeroed, см. ловушки). |
| `nova_rt/alloc_rc.c` | RC backend. `malloc + memset`. Retain/release с refcount. Все stat-функции. Не цель этого плана. |
| `nova_rt/alloc.h` | Общий header — декларирует все функции для всех трёх backends. |
| `nova_rt/effects.h` | `Nova_Mem_*` inline-wrappers → `nova_gc_*` stat functions. Работает со всеми тремя backends. |

### vcpkg (x64-windows-static, проверено)

| Файл | Есть |
|------|------|
| `vcpkg_installed/x64-windows-static/lib/gc.lib` | ✅ |
| `vcpkg_installed/x64-windows-static/lib/atomic_ops.lib` | ✅ |
| `vcpkg_installed/x64-windows-static/include/gc.h` | ✅ |

### Rust-сторона (состояние после Ф.1)

`GcKind` enum существует в `test_runner.rs`. `BuildOpts`, `TestBuildOpts`,
`TestAllOpts` — все имеют `gc_kind: GcKind`. `build_command` выбирает alloc source
по `gc_kind.alloc_c_name()` и добавляет Boehm include/link flags для всех toolchain.
`--gc malloc|boehm` wired в nova-codegen CLI и nova-cli.

**Не реализовано:** `NOVA_GC_BOEHM` define в build_command (нужен для gc_test_helpers.c
— если он когда-нибудь появится); все фазы начиная с Ф.2.

### Существующие GC-тесты

| Файл | Что тестирует |
|------|--------------|
| `nova_tests/concurrency/deep_gc.nv` | Object survival под alloc pressure, record chains, spawn + alloc. Works на всех backends. |
| `nova_tests/runtime/memory_growth.nv` | `Mem.alloc_count()` growth rate — regression detection. Works на всех backends через `Mem.` effect. |
| `nova_rt/test_gc_deep.c` | C-level: `nova_gc_alloc_count/free_count/live_count/reset_stats` — все три backends. |
| `nova_rt/test_fibers_deep.c` | `test_gc_alloc_inside_fiber()` — alloc tracking across fiber switch. |

---

## Критический баг: alloc.c не zeroes

**Проблема:** `emit_c.rs` line 1891 комментирует "nova_alloc returns zeroed memory",
и генерирует код для spawn contexts, vtables, closures без явного `memset`.
Но `alloc.c` использует `malloc` — не zeroed.

**Текущее состояние:** скрытый UB. Работает случайно потому что:
1. Short-lived processes (поля читаются сразу после записи).
2. OS часто выдаёт fresh pages (zeroed mmap) для больших аллокаций.

**Исправление в Ф.1.5:** заменить `malloc(size)` на `calloc(1, size)` в `alloc.c`.
Это делает все три backends семантически одинаковыми (zeroed) и убирает UB.

---

## Часть A — GC switch

### Ф.1 — ВЫПОЛНЕНО ✅

`GcKind` enum + поля в BuildOpts/TestBuildOpts/TestAllOpts + `build_command`
выбор alloc source + Boehm link flags для Clang/MSVC/GCC + stat-функции в
`alloc_boehm.c` + `--gc` flag в CLI.

### Ф.1.5 — `alloc.c` zero-fill fix

**Файлы:** `compiler-codegen/nova_rt/alloc.c`

Заменить:
```c
void* p = malloc(size);
if (!p) { ... }
_alloc_count++;
return p;
```
на:
```c
void* p = calloc(1, size);    /* zero-fill: matches codegen assumption */
if (!p) { ... }
_alloc_count++;
return p;
```

Добавить `#include <string.h>` (уже не нужен — `calloc` в stdlib.h).

**Также в `alloc_boehm.c`:** убрать `memset(p, 0, size)` — `GC_malloc` уже
возвращает zeroed memory (документировано в Boehm GC API). Двойной проход — 
лишний overhead особенно для больших объектов.

```c
void* nova_alloc(size_t size) {
    void* p = GC_malloc(size);   /* GC_malloc returns zeroed memory */
    if (!p) { fprintf(stderr, "nova: out of memory\n"); abort(); }
    _alloc_count++;
    return p;                    /* no memset needed */
}
```

**Acceptance Ф.1.5:**
- `nova test --gc malloc` — все тесты PASS (regression: не ломаем поведение).
- `nova test --gc boehm` — все тесты PASS.
- `nova_tests/runtime/memory_growth.nv` PASS на обоих backends.
- Код в `alloc.c` использует `calloc`.

**Объём:** ~5 строк.

---

### Ф.2 — Verify Boehm GC actually collects

**Файлы:** `nova_tests/gc/` (новая директория) + `nova_rt/gc_test_helpers.c`

#### Дизайн (исправленный)

**Важно: `external fn` запрещён в nova_tests/ (types/mod.rs D82 gate).**
Тесты Ф.2 не могут вызывать GC intrinsics напрямую. Используем два подхода:

**Подход 1: Nova-level через `Mem.` effect** (уже работает)

`Mem.alloc_count()` / `Mem.live()` / `Mem.reset()` доступны из любого модуля.
Под Boehm:
- `Mem.alloc_count()` = монотонный счётчик (верхняя граница живых объектов).
- `Mem.live()` = то же самое (free_count = 0 под Boehm, см. alloc_boehm.c).
- `Mem.reset()` = сбрасывает `_alloc_count` = 0.

**Ограничение:** `Mem.` effect не может измерить фактически занятую heap memory.
Нельзя вызвать `GC_gcollect()` из Nova. Поэтому проверяем только что runtime
**не падает по OOM** при нагрузке — достаточно для Ф.2.

**Подход 2: C-level тест** в `nova_rt/gc_test_helpers.c`

Для настоящей проверки "GC собирает" нужен C-тест, который:
1. Включает gc.h (через NOVA_GC_BOEHM define).
2. Вызывает `GC_gcollect()` + `GC_get_heap_size()`.
3. Проверяет что heap не растёт линейно.

Этот тест компилируется отдельно (`nova-codegen test-build --gc boehm gc_helpers_test.c`
— не стандартный Nova pipeline, а отдельный C build step).

#### Тест 1: bounded alloc rate (через Mem effect, Nova)

```nova
module nova_tests.gc.bounded_rate

// Проверяет что runtime не падает по OOM при 100k аллокаций.
// Под malloc — heap растёт неограниченно (но за 100k итераций не падает).
// Под boehm — GC должен в какой-то момент собрать unreachable objects.
// Тест не проверяет фактическое освобождение (нет API для этого из Nova),
// но проверяет что процесс завершается без OOM kill.

fn make_str(i int) -> str => "alloc-" + i.to_string()

fn main() {
    let mut i = 0
    while i < 100_000 {
        let s = make_str(i)
        // prevent trivial dead-code elimination: use len
        assert(s.len > 0)
        i += 1
    }
    // Если мы здесь — не было OOM kill. Основная проверка.
    println("ok: 100k allocs completed")
}
```

```nova
module nova_tests.gc.stress_100k_ints

// EXPECT_TIMEOUT_MS 30000
// Чистый int loop — ноль аллокаций. Базовый sanity check что
// Boehm backend не ломает простые программы.

fn main() {
    let mut sum = 0
    let mut i = 0
    while i < 100_000 {
        sum = sum + i
        i = i + 1
    }
    assert(sum == 4999950000)
    println("ok")
}
```

#### Тест 2: C-level heap bound (nova_rt/gc_test_helpers.c)

```c
/* nova_rt/gc_test_helpers.c — standalone GC verification test.
 * Compile: clang nova_rt/alloc_boehm.c nova_rt/gc_test_helpers.c
 *          -I<vcpkg_include> -DGC_THREADS -DNOVA_GC_BOEHM
 *          -L<vcpkg_lib> -lgc -latomic_ops -o gc_helpers_test
 * Run: ./gc_helpers_test
 *
 * Tests that Boehm actually collects unreachable objects.
 * Cannot be a Nova test because: external fn restricted to std.runtime.*,
 * and we need GC_gcollect() / GC_get_heap_size() which have no Nova binding.
 */

#ifdef NOVA_GC_BOEHM

#define GC_THREADS
#include <gc.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>

#define ASSERT(cond, msg) \
    do { if (!(cond)) { fprintf(stderr, "FAIL: %s\n", msg); return 1; } } while(0)

static int test_gc_collects(void) {
    GC_INIT();
    GC_set_all_interior_pointers(1);

    /* Allocate 1M small objects (simulate Nova string allocations) */
    size_t heap_before = GC_get_heap_size();
    for (int i = 0; i < 1000000; i++) {
        char* p = GC_malloc(64);
        if (!p) { fprintf(stderr, "OOM at %d\n", i); return 1; }
        /* do not keep references — all become unreachable */
    }
    GC_gcollect();
    GC_gcollect(); /* two passes for generational effect */
    size_t heap_after = GC_get_heap_size();

    printf("heap before: %zu KB, after 1M alloc + collect: %zu KB\n",
           heap_before / 1024, heap_after / 1024);

    /* After collection, heap should be < 4× initial + 10 MB baseline.
     * If GC doesn't collect at all, heap ≈ 64 MB (1M × 64 bytes).
     * If GC works, heap ≈ heap_before + small overhead. */
    size_t limit = heap_before + 10 * 1024 * 1024; /* 10 MB above initial */
    ASSERT(heap_after < limit,
           "heap grew by > 10 MB after GC_gcollect — GC not collecting");

    printf("PASS: heap_after (%zu KB) < limit (%zu KB)\n",
           heap_after / 1024, limit / 1024);
    return 0;
}

static int test_gc_pause(void) {
    GC_INIT();

    /* Benchmark pause time for different heap sizes */
    struct { int n; const char* label; } cases[] = {
        {10000, "10k allocs"},
        {100000, "100k allocs"},
        {1000000, "1M allocs"},
    };
    for (int c = 0; c < 3; c++) {
        for (int i = 0; i < cases[c].n; i++) {
            char* p = GC_malloc(64);
            if (!p) { return 1; }
            (void)p;
        }
        /* Measure one GC pause */
        struct timespec t0, t1;
        clock_gettime(CLOCK_MONOTONIC, &t0);
        GC_gcollect();
        clock_gettime(CLOCK_MONOTONIC, &t1);
        long elapsed_us = (t1.tv_sec - t0.tv_sec) * 1000000
                        + (t1.tv_nsec - t0.tv_nsec) / 1000;
        printf("pause[%s]: %ld us\n", cases[c].label, elapsed_us);
    }
    return 0;
}

int main(void) {
    if (test_gc_collects() != 0) return 1;
    if (test_gc_pause() != 0) return 1;
    printf("ALL PASS\n");
    return 0;
}

#else
int main(void) {
    printf("SKIP: not built with NOVA_GC_BOEHM\n");
    return 0;
}
#endif
```

**Запуск из test_runner** (nova-codegen test-build --gc boehm) — нет, это C-only.
Запускается вручную или отдельным CI step:
```powershell
clang compiler-codegen/nova_rt/alloc_boehm.c `
      compiler-codegen/nova_rt/gc_test_helpers.c `
      -I compiler-codegen/vcpkg_installed/x64-windows-static/include `
      -DGC_THREADS -DNOVA_GC_BOEHM `
      -L compiler-codegen/vcpkg_installed/x64-windows-static/lib `
      -lgc -latomic_ops -o gc_helpers_test.exe
.\gc_helpers_test.exe
```

**Acceptance Ф.2:**
- `nova test --gc boehm --filter gc/` — все Nova тесты PASS, нет OOM kill.
- `nova test --gc malloc --filter gc/` — те же тесты PASS (backend-agnostic).
- `gc_helpers_test.exe` — PASS: heap_after < heap_before + 10 MB после 1M alloc.
- `nova_tests/runtime/memory_growth.nv` — PASS на Boehm backend (Mem effect работает).

**Объём:** ~50 строк Nova tests + ~80 строк C helpers.

---

### Ф.3 — Boehm GC pause benchmark (из gc_test_helpers.c)

`gc_test_helpers.c` уже включает `test_gc_pause()`. После Ф.2:

1. Запустить `gc_helpers_test.exe` и зафиксировать pause числа.
2. Записать результаты в `spec/overview.md` секцию «GC».

`spec/overview.md` сейчас содержит placeholder `<1ms` — заменить реальными
измерениями p50/p99 для 10k/100k/1M аллокаций на dev-машине.

**Acceptance Ф.3:**
- Числа зафиксированы в `spec/overview.md` (реальные, не placeholder).
- `spec/overview.md` не содержит `<1ms` (заменено на measured).
- Примечание: числа специфичны для dev-машины — это не production benchmark,
  а первичная точка отсчёта.

**Объём:** запуск + ~20 строк docs.

---

### Ф.4 — Switch default к Boehm

**Условие:** Ф.1 + Ф.1.5 + Ф.2 + Ф.3 — все без регрессий и acceptable pauses.

**Изменения:**

1. `GcKind::default()` → `GcKind::Boehm` (одна строка в `#[default]`):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GcKind {
    Malloc,  // ← убрать #[default]
    #[default]
    Boehm,   // ← добавить #[default]
}
```

2. `nova-cli` — `--gc` флаг: default = "boehm"; `malloc` скрыт из help
   (`#[arg(hide = true)]`) — это internal-only режим для разработчиков runtime.

3. `nova-codegen` — `--gc` флаг: default = "boehm"; `malloc` видим
   (internal tool, разработчики могут использовать явно для бенчмарков).

4. README.md: обновить секцию "Building" — упомянуть что default GC = Boehm.

5. `docs/project-creation.txt` и `docs/simplifications.md` — обновить.

**Acceptance Ф.4:**
- `nova test` (без `--gc`) — все тесты PASS на Boehm, включая `deep_gc.nv`.
- `nova test --gc malloc` — все non-boehm-specific тесты PASS (no regression).
- `nova_tests/runtime/memory_growth.nv` PASS на Boehm:
  - `Mem.free_count()` = 0 (Boehm conservative — documented behaviour).
  - `Mem.live()` = `Mem.alloc_count()` (upper bound — документировать в тесте).
- Concurrency тесты (sleep_bench, cancel_stress) — elapsed не ухудшился >10%.
- README.md упоминает что GC = Boehm по умолчанию.

**Важно:** `memory_growth.nv` содержит тест "pure int loop — zero allocations".
Под Boehm `Mem.alloc_count()` считает вызовы `nova_alloc` — не изменится,
ints stack-allocated, тест должен PASS. Но проверить!

**Объём:** ~10 строк Rust + docs.

---

### Ф.5 — Linux/macOS GC support

**Условие:** есть доступ к Linux машине (сейчас blind ship).

**Linux:** `apt install libgc-dev` → `-lgc -lpthread`.
**macOS:** `brew install bdw-gc` → `-lgc`.

Код в `build_command` для `Toolchain::Gcc` уже содержит Boehm flags (Ф.1).
Нужно:
1. Добавить graceful error если `libgc-dev` не установлен — detect через
   попытку compile с `-lgc` и парсинг "library not found" в stderr.
2. Напечатать hint: `apt install libgc-dev` / `brew install bdw-gc`.

```rust
// В build_command после run_with_timeout для Gcc + Boehm:
if opts.gc_kind == GcKind::Boehm && !ok {
    let has_gc_error = stderr.contains("library not found")
        || stderr.contains("cannot find -lgc")
        || stderr.contains("No such file or directory");
    if has_gc_error {
        eprintln!("hint: Boehm GC not found. Install with:");
        eprintln!("  Ubuntu/Debian: sudo apt install libgc-dev");
        eprintln!("  macOS:         brew install bdw-gc");
    }
}
```

Но это в `run_one`, не в `build_command` (который возвращает Command, не результат).
Правильнее: добавить `detect_boehm_linux()` функцию аналогично `detect_or_build_libuv`.

**Acceptance Ф.5:** `nova test --gc boehm` на Linux — все тесты PASS.

**Объём:** ~30 строк.

---

### Ф.6 — `ALLOC_REQUIRES` / `ALLOC_EXCLUDES` маркеры

Расширение D89 двумя новыми маркерами:

```nova
// ALLOC_REQUIRES boehm   ← тест запускается ТОЛЬКО на boehm backend
// ALLOC_EXCLUDES malloc   ← тест SKIP на malloc, запускается на остальных
```

**Изменения в `compiler-codegen/src/test_runner.rs`:**

#### AllocConstraint enum

```rust
/// Parsed alloc constraint from test file header comment.
/// Controls whether a test runs under a given GC backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocConstraint {
    None,
    Requires(GcKind),   // ALLOC_REQUIRES <backend>: run only on this backend
    Excludes(GcKind),   // ALLOC_EXCLUDES <backend>: skip on this backend
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

/// Parse ALLOC_REQUIRES / ALLOC_EXCLUDES from first 30 lines.
/// Returns None if no marker found.
pub fn parse_alloc_constraint(src: &str) -> AllocConstraint {
    for line in src.lines().take(30) {
        let body = line
            .trim_start_matches("//")
            .trim_start_matches('#')
            .trim();
        if let Some(rest) = body.strip_prefix("ALLOC_REQUIRES") {
            match GcKind::parse(rest.trim()) {
                Ok(k) => return AllocConstraint::Requires(k),
                Err(_) => {} // unknown backend — ignore
            }
        }
        if let Some(rest) = body.strip_prefix("ALLOC_EXCLUDES") {
            match GcKind::parse(rest.trim()) {
                Ok(k) => return AllocConstraint::Excludes(k),
                Err(_) => {}
            }
        }
    }
    AllocConstraint::None
}
```

#### Новый Outcome variant

```rust
/// Тест пропущен из-за несовместимости backend.
/// Не считается ни PASS ни FAIL в summary.
Skipped {
    reason: SkipReason,
    elapsed: Duration,
},

pub enum SkipReason {
    AllocBackend {
        required: String,   // "boehm" или "malloc"
        actual: String,     // текущий backend
    },
}
```

#### run_one интеграция

В начале `run_one`, после чтения src файла, перед codegen:
```rust
let alloc_constraint = parse_alloc_constraint(&src);
if !alloc_constraint.allows(opts.gc_kind) {
    return Outcome::Skipped {
        reason: SkipReason::AllocBackend {
            required: match alloc_constraint {
                AllocConstraint::Requires(k) => format!("{:?}", k).to_lowercase(),
                AllocConstraint::Excludes(k) => format!("not-{:?}", k).to_lowercase(),
                AllocConstraint::None => unreachable!(),
            },
            actual: format!("{:?}", opts.gc_kind).to_lowercase(),
        },
        elapsed: start.elapsed(),
    };
}
```

#### Summary изменения

```
PASS: 91  SKIP: 2 (alloc-backend)  FAIL: 0
```

В text format:
```
SKIP-ALLOC     gc/some_boehm_only_test  # requires boehm, running with malloc
```

В JSON/TAP/JUnit: добавить skip support.

**Spec обновления:**
- `spec/decisions/09-tooling.md` D89 — добавить пункты 6 и 7:
  - D89.6: `ALLOC_REQUIRES <backend>` — run only on specified GC backend.
  - D89.7: `ALLOC_EXCLUDES <backend>` — skip on specified backend.
- `docs/test-conventions.md` — примеры маркеров.

**Acceptance Ф.6:**
- `nova test --gc malloc` — boehm-only тесты: `SKIP-ALLOC`.
- `nova test --gc boehm` — все тесты запускаются (constraint = none или boehm).
- Summary корректно разделяет PASS / SKIP / FAIL.
- JUnit XML: `<skipped/>` для skip тестов.

**Объём:** ~100 строк Rust + ~20 строк spec/docs.

---

## Часть B — test-runner polish

Независима от GC. Все задачи — улучшения `compiler-codegen/src/test_runner.rs`.

### Б.1 — Test caching (opt-in)

Hash-based кэш в configurable dir. **Default: DISABLED** (CI-safe — false
positive cache hit = PASS когда должен FAIL).

**Cache key** (всё влияет на результат — если что-то изменилось, cache miss):
- SHA-256 содержимого `.nv` source файла.
- SHA-256 содержимого каждого `nova_rt/*.c` + `nova_rt/*.h` (не mtime — mtime ненадёжен при git checkout).
- mtime `nova-codegen` бинарника (достаточно, content-hash слишком медленно).
- `toolchain.name()` (string: "clang-17.0/msvc-19.x/gcc-13.x").
- `mode` (dev/release).
- `gc_kind` (malloc/boehm — **разные бинари**).
- libuv version stamp из `target/libuv-cache/version.txt`.

**Важно: DefaultHasher НЕЛЬЗЯ** — нестабилен между Rust версиями (rustup update
инвалидирует весь кэш). Использовать `sha2 = "0.10"` (~6 KB dep).

**Реализация:**
```rust
pub struct CacheKey {
    hash: [u8; 32],  // SHA-256
}

fn compute_cache_key(nv_path: &Path, opts: &TestBuildOpts) -> Result<CacheKey> {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    // Source file
    hasher.update(&std::fs::read(nv_path)?);
    // Runtime sources (content, not mtime)
    let mut rt_files: Vec<_> = std::fs::read_dir(&opts.rt_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            s.ends_with(".c") || s.ends_with(".h")
        })
        .collect();
    rt_files.sort_by_key(|e| e.path());
    for f in rt_files {
        hasher.update(&std::fs::read(f.path())?);
    }
    // Toolchain + mode + gc_kind
    hasher.update(opts.toolchain.name().as_bytes());
    hasher.update([opts.mode as u8, opts.gc_kind as u8]);
    // nova-codegen binary mtime
    if let Ok(meta) = std::fs::metadata(&find_nova_codegen_bin()) {
        if let Ok(mtime) = meta.modified() {
            hasher.update(mtime.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_secs().to_le_bytes());
        }
    }
    Ok(CacheKey { hash: hasher.finalize().into() })
}

struct CachedOutcome {
    outcome_json: String,
}

fn cache_lookup(key: &CacheKey, cache_dir: &Path) -> Option<Outcome> {
    let hex = hex::encode(&key.hash[..16]); // 32-char hex prefix enough
    let path = cache_dir.join(&hex);
    let s = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&s).ok()
}

fn cache_store(key: &CacheKey, outcome: &Outcome, cache_dir: &Path) {
    let hex = hex::encode(&key.hash[..16]);
    let path = cache_dir.join(&hex);
    if let Ok(s) = serde_json::to_string(outcome) {
        let _ = std::fs::write(&path, s);
    }
}
```

Deps: `sha2 = "0.10"`, `serde_json` (уже используется для results.json).

**Объём:** ~150 строк + `sha2` dep.

### Б.2 — `EXPECT_TIMEOUT_MS` per-test marker

```nova
// EXPECT_TIMEOUT_MS 30000
```

Переопределяет глобальный `--timeout` для конкретного теста. Полезно для
sleep_bench/stress тестов которые легитимно долгие.

Расширить `parse_expect()` или добавить отдельную функцию:
```rust
pub fn parse_timeout_ms(src: &str) -> Option<u64> {
    for line in src.lines().take(30) {
        let body = line.trim_start_matches("//").trim_start_matches('#').trim();
        if let Some(rest) = body.strip_prefix("EXPECT_TIMEOUT_MS") {
            return rest.trim().parse::<u64>().ok();
        }
    }
    None
}
```

В `run_one`:
```rust
let per_test_timeout = parse_timeout_ms(&src)
    .map(Duration::from_millis)
    .unwrap_or(opts.timeout);
```

**Объём:** ~30 строк.

### Б.3 — `--verbose` capture stdout PASS-тестов

```go
// Go: go test -v — показывает stdout всех тестов
// Cargo: cargo test --nocapture
```

Расширить `Outcome::Pass`:
```rust
Outcome::Pass {
    elapsed: Duration,
    detail: String,
    // Some(_) при --verbose, иначе None (не тратим память на discard)
    captured_stdout: Option<String>,
    captured_stderr: Option<String>,
}
```

**Важно:** изменение структуры требует обновить все `Outcome::Pass { .. }` match arms.
В `run_one` при `opts.verbosity == Verbosity::Verbose` — сохранять captured output.
В `print_summary` при verbose — печатать captured для каждого PASS.

`Outcome::Fail` уже содержит stderr в `Stage::Run`. При verbose дублировать в stdout.

**Объём:** ~40 строк + структурные изменения.

### Б.4 — Slow tests report

В конце `--format text` summary, если `total > 10`:
```
===== SLOWEST TESTS (top 10) =====
  3.214s  concurrency/sleep_real_clock
  2.103s  std/checksums/fnv
```

Sort outcomes by `elapsed` descending, take 10.

**Объём:** ~25 строк.

### Б.5 — `--list` + `--filter-from`

`nova test --list` — перечислить все тесты без запуска, по одному на строку.
`nova test --filter-from shard.txt` — exact-match из файла (не substring).

CI sharding:
```bash
nova test --list | split -l 50 - shard-
nova test --filter-from shard-aa --format junit > results-0.xml
```

`TestAllOpts`:
```rust
pub list_only: bool,
pub filter_from: Option<PathBuf>,
```

В `run_all`: если `list_only` — print each display name + return empty summary.

**Объём:** ~45 строк.

### Б.6 — Retry count в JUnit XML

`Outcome::Pass { retries: u32, .. }`.

JUnit при `retries > 0`:
```xml
<testcase name="..." classname="..." time="3.2">
  <system-out>retried 1 time(s) before pass</system-out>
</testcase>
```

**Объём:** ~20 строк.

### Б.7 — `--shuffle [SEED]`

Fisher-Yates shuffle входного списка тестов перед запуском.
`--shuffle` без аргумента → random seed (system time).
`--shuffle 42` → reproducible.

xorshift PRNG (~10 строк) + Fisher-Yates (~10 строк). Без extra deps.

Print seed в начале прогона:
```
Shuffling 91 tests with seed 1715432198
```

**Объём:** ~35 строк.

### Б.8 — `EXPECT_TIMEOUT_MS` + `--shuffle` + `--list` в nova-cli

Wire в `nova-cli/src/main.rs` после реализации в test_runner:
- `--shuffle [SEED]` → `TestAllOpts.shuffle_seed: Option<u64>`
- `--list` → `TestAllOpts.list_only: bool`
- `--filter-from <path>` → `TestAllOpts.filter_from: Option<PathBuf>`
- `EXPECT_TIMEOUT_MS` — автоматически через parse_timeout_ms, не нужно CLI флаг.

**Объём:** ~30 строк (nova-cli/src/main.rs).

---

## Порядок выполнения

```
Ф.1.5 (calloc fix)           — немедленно, перед чем-либо другим
Ф.2 (GC verify)              — после Ф.1.5
Ф.3 (pause bench)            — после Ф.2 (использует gc_helpers_test.exe)
Ф.4 (switch default)         — после Ф.2 + Ф.3 без регрессий
Ф.5 (Linux/macOS)            — после Ф.4, когда есть Linux доступ
Ф.6 (ALLOC_REQUIRES)         — после Ф.1 — нужен GcKind

Б.2                          — независима, можно в любой момент
Б.3 → обновить Outcome::Pass — перед Б.1 (кэш сериализует Outcome)
Б.1                          — после Б.2 + Б.3 (cache key включает timeout; Outcome должен быть стабилен)
Б.4, Б.5, Б.7               — независимы, в любом порядке
Б.6                          — после Б.3 (extends Outcome::Pass)
Б.8                          — последней (wire всё в nova-cli)
```

---

## Критические файлы

| Файл | Действие |
|------|----------|
| `compiler-codegen/nova_rt/alloc.c` | Ф.1.5: `malloc` → `calloc` (zero-fill fix) |
| `compiler-codegen/nova_rt/alloc_boehm.c` | Ф.1.5: убрать лишний `memset` после `GC_malloc` |
| `compiler-codegen/nova_rt/gc_test_helpers.c` | Ф.2: новый файл — C-level Boehm collect + pause test |
| `nova_tests/gc/bounded_rate.nv` | Ф.2: 100k alloc — no OOM |
| `nova_tests/gc/stress_100k_ints.nv` | Ф.2: pure int loop — no alloc regression |
| `compiler-codegen/src/test_runner.rs` | Ф.6: AllocConstraint, parse_alloc_constraint, Outcome::Skipped, SkipReason |
| `spec/decisions/09-tooling.md` | Ф.6: D89.6 + D89.7 |
| `docs/test-conventions.md` | Ф.6: ALLOC_REQUIRES примеры |
| `spec/overview.md` | Ф.3: GC pause числа (measured, не placeholder) |
| `compiler-codegen/Cargo.toml` | Б.1: добавить sha2 dep |

---

## Ловушки реализации

### `GC_malloc` уже zeroes — убрать `memset` из alloc_boehm.c

Boehm GC API документирует: `GC_malloc(size)` returns zeroed memory.
Текущий `alloc_boehm.c` делает лишний `memset(p, 0, size)` — двойной проход
по аллокированной памяти. Для больших объектов это заметный overhead. Убрать в Ф.1.5.

### `alloc.c` НЕ zeroes — баг, требует `calloc`

`emit_c.rs` line 1891: "nova_alloc returns zeroed memory" — это ожидание
компилятора. `malloc` его не выполняет. Исправить в Ф.1.5 через `calloc(1, size)`.

### `external fn` запрещён вне `std.runtime.*`

types/mod.rs line 74: hard gate. Nova-тесты в `nova_tests/gc/` не могут
использовать `external fn _nova_gc_collect()`. Все GC intrinsics через `Mem.` effect
(уже есть) или через отдельный C-тест (`gc_test_helpers.c`).

### `GC_THREADS` define — обязателен

`alloc_boehm.c` уже имеет `#define GC_THREADS` перед `#include <gc.h>`.
В `build_command` Ф.1 добавлен `-DGC_THREADS` / `/DGC_THREADS` compile flag —
защита если Boehm header включается из другого .c файла.

### `NOVA_GC_BOEHM` define — добавить в build_command

Для `gc_test_helpers.c` нужен `#ifdef NOVA_GC_BOEHM` guard. В Ф.2 добавить:
```rust
if opts.gc_kind == GcKind::Boehm {
    flags.push("-DNOVA_GC_BOEHM".to_string()); // или /DNOVA_GC_BOEHM для MSVC
}
```
Это defensive define — если gc_test_helpers.c не подключён, define бесполезен.

### vcpkg include path для Boehm

`gc.h` лежит прямо в `vcpkg_installed/x64-windows-static/include/gc.h`,
не в `include/gc/gc.h`. Поэтому `-I<vcpkg_include>` достаточно.
Дополнительный `-I<vcpkg_include>/gc` не нужен.

### `atomic_ops.lib` — нужен для thread-safe Boehm на Windows

Boehm с `GC_THREADS` на Windows требует `atomic_ops.lib` в дополнение к `gc.lib`.
Оба есть в vcpkg. Порядок линковки: `gc.lib` перед `atomic_ops.lib`.

### MSVC: `/link` должно быть последним аргументом

При `cl.exe` все `/link` опции (пути к `.lib`) должны идти **после** всех
source файлов и compile-time флагов. `build_command` для MSVC строит
аргументы в правильном порядке (реализовано в Ф.1).

### `GcKind` в `TestAllOpts` — передавать через весь стек

```
TestAllOpts.gc_kind
  → TestBuildOpts.gc_kind (через worker dispatch)
    → BuildOpts.gc_kind
      → build_command()
```

Реализовано в Ф.1.

### `Mem.` effect behaviour под Boehm

- `Mem.alloc_count()` — монотонный счётчик вызовов `nova_alloc`. Работает корректно.
- `Mem.free_count()` — всегда 0 (консервативно, GC не сообщает что освободил).
- `Mem.live()` — равно `alloc_count` (верхняя граница, не точное число).
- `Mem.reset()` — сбрасывает `_alloc_count = 0`.

`memory_growth.nv` тест "pure int loop — zero allocations" должен PASS:
ints stack-allocated, `nova_alloc` не вызывается.

Тест "supervised with N spawns — bounded per-fiber growth": каждый spawn аллоцирует
ctx + fiber stack через `nova_alloc`. Под Boehm count растёт так же. Тест PASS.

Тест "repeated supervised: same per-iter growth": delta1 ≈ delta2. Под Boehm
GC может вклиниться между двумя runs и изменить bookkeeping. Добавить slack если
нужно.

### Cache invalidation при смене GC backend

Если в Б.1 добавляем кэш, `gc_kind` ОБЯЗАН быть частью cache key.
`alloc.c` и `alloc_boehm.c` производят разные бинари — cache miss при смене `--gc`.
Реализовано в Б.1 дизайне выше.

---

## Risks / Trade-offs

**R1. Boehm conservative tracing** — на 64-bit ложные roots маловероятны
(целые числа редко совпадают с указателями), но не исключены. Эффект —
часть памяти не выпускается. Practical impact: задокументировано community
decades, приемлемо для general backend. Если conservative scanning станет
проблемой — `GC_malloc_atomic()` для объектов без указателей (ints, strings,
numeric arrays) снижает false-positive retention.

**R2. STW pause latency.** Boehm — stop-the-world. На больших heap'ах (>1 GB)
pauses могут быть 10-100ms. Для real-time — блокер. Для backend API — приемлемо.
Реальные числа из Ф.3 определяют окончательный вердикт.

**R3. `atomic_ops.lib` на Windows.** Уже в vcpkg_installed — не требует
внешней установки. На clean machine — `vcpkg install bdwgc` восстанавливает.

**R4. Boehm + fibers (minicoro). КРИТИЧЕСКИЙ для Plan 23.**

minicoro создаёт fiber stack через `mco_create` (malloc или mmap, зависит от
платформы). Boehm GC не знает об этом stack — не сканирует его при collection.

**Текущий статус:** В nova runtime каждый fiber ↔ один green thread (один OS thread).
Boehm сканирует stack того thread который делает `GC_gcollect()` — обычно main thread.
Fiber stacks НЕ сканируются. Объекты reachable только через fiber stack могут быть
собраны GC пока fiber suspendend.

**Митигация для Plan 23 (M:N):** `GC_add_roots(stack_low, stack_high)` для каждого
fiber stack при создании. `GC_remove_roots()` при уничтожении.

**Текущий риск (одиночный thread):** Nova сейчас 1:1 scheduler на одном OS thread.
Все fibers живут на одном thread. Когда fiber запущен — его stack часть основного
stack frame — Boehm сканирует. Когда fiber suspended (minicoro context switch) —
его stack НЕ сканируется основным thread. Риск реальный.

**Действие для Ф.2:** Тест `deep_gc.nv` section 4 ("record allocated inside spawn")
должен PASS на Boehm. Если падает — это R4 в действии, нужен `GC_add_roots()`.

**R5. Boehm + libuv event loop.** libuv callbacks запускаются из main thread —
Boehm сканирует его стек. Риска нет при текущей 1-thread архитектуре. При переходе
к настоящим потокам (Plan 23) потребуется `GC_register_my_thread()` в каждом worker.

**R6. `calloc` vs `malloc` производительность.** `calloc` медленнее `malloc` для
больших объектов (OS mmap + zero-fill). Для small objects разница незначительна
(OS выдаёт zeroed pages из pool). Для производственного use case — нормально.
Если профилирование покажет проблему — перейти к `malloc + memset` явно.

---

## Acceptance overall

После Ф.1–Ф.4:
- `nova test` (default = boehm) — все тесты PASS без регрессий.
- `nova test --gc malloc` — все тесты PASS (backward compat).
- `nova_tests/gc/bounded_rate.nv` — PASS: 100k allocs без OOM kill.
- `gc_helpers_test.exe` — PASS: heap bounded после 1M alloc + GC_gcollect.
- `spec/overview.md` — реальные GC pause числа (не placeholder).
- `nova_tests/concurrency/deep_gc.nv` section 4 — PASS на Boehm (R4 check).
- README.md упоминает что GC = Boehm по умолчанию.

После Ф.5: то же на Linux.
После Ф.6: `ALLOC_REQUIRES` тесты корректно skip на несовместимом backend.

---

## Объём

| Фаза | Строк Rust | Строк C | Строк Nova | Строк docs |
|------|-----------|---------|-----------|-----------|
| Ф.1 | ~200 (выполнено) | ~20 (выполнено) | — | — |
| Ф.1.5 | ~5 | ~5 | — | — |
| Ф.2 | — | ~80 | ~50 | — |
| Ф.3 | — | — | — | ~20 |
| Ф.4 | ~10 | — | — | ~15 |
| Ф.5 | ~30 | — | — | ~10 |
| Ф.6 | ~100 | — | — | ~30 |
| Б.1–Б.8 | ~375 | — | — | ~30 |
| **Итого** | **~720** | **~105** | **~50** | **~105** |

---

## Что НЕ входит

- **RC codegen** (retain/release inserting) — `alloc_rc.c` есть, codegen его
  не использует. Отдельный план если Boehm окажется неприемлемым.
- **Concurrent GC** — годами работы, после v1.0.
- **`realtime nogc { }`** arena allocator — Plan TBD.
- **Linux/macOS smoke-test** инфраструктуры — зависит от доступа к машине.
- **GitHub Actions CI matrix** — отдельная задача (3 OS × 2 backends = 6 runners).
- **`GC_malloc_atomic()`** для non-pointer objects — оптимизация для Plan 23+.
- **`GC_add_roots()` для fibers** — в Plan 23 (M:N runtime).

---

## Связь

- [Plan 25 G3a/G3b](25-production-readiness-roadmap.md#g3) — этот план closes G3a.
- [D6 (spec/decisions/05-memory.md)](../../spec/decisions/05-memory.md#d6) —
  «managed by default»: Plan 27 = первая реализация.
- [Plan 23 (M:N runtime)](23-mn-runtime-roadmap.md) — потребует GC cooperation
  cross-thread (R4 Boehm + fibers, R5 Boehm + libuv workers).

---

## История

- **2026-05-11** — создан после Plan 22 hardening retro + Plan 25 honest pass.
- **2026-05-11** — production audit pass 1: добавлены GcKind enum, vcpkg paths,
  MSVC линковка, Б.1 sha2 dep обоснование, R4/R5 risks, alloc_boehm.c stat gap.
- **2026-05-11** — Ф.1 реализована (commit a778887d6).
- **2026-05-11** — production audit pass 2: исправлен дизайн Ф.2 (external fn
  запрещён в nova_tests/ → используем Mem. effect + C-тест); добавлена Ф.1.5
  (calloc fix + GC_malloc memset redundancy); уточнён R4 (Boehm + fibers — реальный
  риск при suspended fibers); документированы Mem. effect semantics под Boehm;
  таблица объёмов обновлена; убраны phantom acceptance criteria.
