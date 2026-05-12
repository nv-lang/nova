// SPDX-License-Identifier: MIT OR Apache-2.0
# План 41: Per-thread fiber stack arena с lazy commit

> **Статус:** план, не начат. **P1 prerequisite для Plan 23 (M:N runtime).**
> Создан 2026-05-12.

**Цель.** Заменить текущий fiber stack allocation (calloc per-fiber +
`_NOVA_GC_DISABLE` workaround) на per-thread arena с lazy commit. Это
снимает архитектурное ограничение Boehm `MAX_ROOT_SETS=128`, открывает
дорогу к concurrent GC (Plan 23), и даёт растущие стеки на Linux/macOS
без `MCO_USE_VMEM_ALLOCATOR` (который Windows-incompatible).

---

## Контекст

### Текущая ситуация (Plan 27 R4, commit `31207daabe`)

Suspended fiber stacks выделяются через `calloc(56KB)` (minicoro
default). Они **не на OS stack**, поэтому Boehm conservative scanner
их не видит как roots → указатели на heap из fiber stacks не учитываются
→ GC может собрать ещё-живые объекты → use-after-free при fiber resume.

**Что пробовали раньше:** `GC_add_roots()` per-fiber на каждый stack.
**Не сработало:** Boehm имеет compile-time константу `MAX_ROOT_SETS=128`,
на 10k fibers упёрлись в неё → краш.

**Текущий workaround:** `_NOVA_GC_DISABLE()` в начале каждого scheduler
tick'а, `_NOVA_GC_ENABLE()` в конце. Работает потому что **single-thread
cooperative** — GC физически не может запуститься между yield/resume.

**Скрытая хрупкость:** любой call path вызывающий `nova_alloc` **вне**
обёрнутого scheduler tick'а — потенциальный UAF. Сейчас спасает только
дисциплина.

### Что выяснили о minicoro VMEM_ALLOCATOR

minicoro предлагает `MCO_USE_VMEM_ALLOCATOR` для растущих стеков:
- **Linux/macOS:** `mmap` lazy commit — работает (2MB virtual, physical
  только под touched pages).
- **Windows:** `VirtualAlloc(MEM_RESERVE | MEM_COMMIT)` — commits all
  upfront. **Не lazy.** Не даёт win.

Каждый стек = отдельный mmap → **разбросаны по виртуальной памяти**.
Совершенно несовместимо с подходом «1 GC root per arena».

### Архитектурное противоречие (без Plan 41)

Хотим одновременно:
1. Растущие стеки (через lazy commit).
2. GC scaling до тысяч fibers (через единый arena root).
3. Concurrent GC (Plan 23 prerequisite).

VMEM_ALLOCATOR даёт (1), но ломает (2) и (3).
`_NOVA_GC_DISABLE` даёт (2) bootstrap'ом, но ломает (3).
**Нужно решение которое даёт всё.**

---

## Решение

**Per-thread arena с lazy commit + slot allocator.**

### Принцип

Каждый worker thread (сейчас main, в Plan 23 — N workers) при первом
fiber-allocation запросе:

1. Резервирует **большой непрерывный** виртуальный диапазон через
   `mmap MAP_NORESERVE` (Linux/macOS) или `VirtualAlloc MEM_RESERVE`
   (Windows).
2. Регистрирует весь диапазон как **один GC root** через `GC_add_roots`.
3. Все fiber stacks этого thread'а выделяются из этого arena как
   fixed-size slots.

Когда fiber завершается — slot помечен как свободный в bitmap, может
быть переиспользован для следующего fiber'а.

### Что это даёт

| Свойство | Текущий | Plan 41 |
|---|---|---|
| GC root entries | 0 (disable) | 1 per worker thread |
| Suspended stacks visible to GC | No (disable hack) | **Yes** (через arena root) |
| Growable stacks (Linux/macOS) | No (fixed 56KB) | **Yes** (lazy mmap commit) |
| Growable stacks (Windows) | No | No (нужен SEH guard pages, отдельно) |
| Concurrent GC compatible | **No** | **Yes** |
| `_NOVA_GC_DISABLE` workaround | Required | **Removed** |
| Max fibers per thread | ~∞ (calloc'd) | bounded by arena size (~2000-8000 на thread) |

### Сравнение с альтернативами

**A. Текущий `_NOVA_GC_DISABLE`:**
- Pro: уже работает, 0 LOC.
- Con: single-thread only, hidden UAF risk, не масштабируется к Plan 23.

**B. `MCO_USE_VMEM_ALLOCATOR` (vendor-provided):**
- Pro: растущие стеки на Linux/macOS, 5 LOC.
- Con: каждый стек = отдельный mmap → много GC roots → упёрлись в 128 limit.
  Не решает GC scaling.

**C. Per-thread arena (этот план):**
- Pro: 1 GC root per thread, lazy commit (Linux/macOS), concurrent GC ready.
- Con: ~250 LOC новой реализации, slot reuse через bitmap, fixed slot size
  per arena.

**D. Recompile Boehm с `LARGE_CONFIG`:**
- Pro: 0 LOC в Nova, поднимает limit до 8192.
- Con: patch dependency (нарушает «не патчить сторонние библиотеки»),
  `GC_add_roots` linear scan всё равно медленный на 8000 entries,
  не даёт растущие стеки.

**Выбираем (C) per-thread arena.**

---

## Дизайн

### Структура памяти

```
Per-thread (TLS):

    arena: 256 MB virtual reserved          ┐
        ├─ slot 0: 2 MB                     │
        │   ├─ page 0 (4KB) — physical      │ Lazy commit on
        │   ├─ page 1 (4KB) — physical      │ first access
        │   ├─ ...                          │ (Linux/macOS)
        │   └─ page 511 — uncommitted       │
        ├─ slot 1: 2 MB                     │
        ├─ ...                              │
        └─ slot 127: 2 MB                   ┘

    slot_used: bitmap[128 bits]
    one GC_add_roots(arena, arena + 256MB)
```

**256MB virtual** per thread × 16 worker threads (M:N) = 4GB virtual.
На x86_64 (256TB virtual address space) тривиально.

**Physical** — только page-faulted pages под реально используемыми
fragments stacks. На Linux/macOS: typical fiber использует 8-32KB
стека → 2-8 pages × 4KB = 8-32KB physical per fiber. 1000 active
fibers = ~16MB physical per thread.

### API (новый файл `nova_rt/fiber_arena.c` + h)

```c
/* Initialize per-thread arena lazily on first fiber alloc.
 * Idempotent — safe to call multiple times. */
void nova_fiber_arena_init(void);

/* mco custom allocator callbacks (registered via mco_desc.alloc_cb). */
void* nova_fiber_alloc(size_t size, void* user);
void  nova_fiber_dealloc(void* ptr, size_t size, void* user);

/* Stats — for diagnostics. */
typedef struct {
    size_t arena_virtual;   /* Total virtual reserved. */
    size_t slots_total;     /* Total slot capacity. */
    size_t slots_used;      /* Currently allocated. */
    size_t physical_estimate; /* Approximation of resident pages. */
} NovaFiberArenaStats;
NovaFiberArenaStats nova_fiber_arena_stats(void);
```

### Wire через minicoro

`fibers.h:nova_fiber_run` сейчас вызывает `mco_desc_init(entry, 0)`
(default stack size). Меняем на:

```c
mco_desc desc = mco_desc_init(entry, NOVA_FIBER_STACK_SIZE);
desc.alloc_cb = nova_fiber_alloc;
desc.dealloc_cb = nova_fiber_dealloc;
desc.allocator_data = NULL;
```

`NOVA_FIBER_STACK_SIZE = 2*1024*1024` (2MB — как mainstream thread
stack size).

### Slot reuse через bitmap

```c
__thread struct {
    char* base;                  /* arena start */
    size_t slot_size;            /* 2MB */
    size_t slot_count;           /* 128 */
    uint64_t free_bits[2];       /* 128 bits — bitmap свободных */
    size_t high_water;           /* для diagnostics */
} _nova_arena;

static void* nova_fiber_alloc(size_t size, void* user) {
    if (!_nova_arena.base) {
        nova_fiber_arena_init();
    }
    /* Find first free slot via bitmap scan. */
    int slot = -1;
    for (int word = 0; word < 2; word++) {
        if (_nova_arena.free_bits[word] != ~0ULL) {
            slot = word * 64 + __builtin_ctzll(~_nova_arena.free_bits[word]);
            _nova_arena.free_bits[word] |= 1ULL << (slot % 64);
            break;
        }
    }
    if (slot < 0) {
        /* Arena exhausted — fallback: malloc, register as separate root.
         * Caller получит warning через debug log. Не fatal — degrades
         * to old behaviour for this fiber. */
        return calloc(1, size);
    }
    return _nova_arena.base + slot * _nova_arena.slot_size;
}

static void nova_fiber_dealloc(void* ptr, size_t size, void* user) {
    /* Check if ptr is from our arena. */
    if ((char*)ptr >= _nova_arena.base &&
        (char*)ptr < _nova_arena.base + _nova_arena.slot_count * _nova_arena.slot_size) {
        size_t slot = ((char*)ptr - _nova_arena.base) / _nova_arena.slot_size;
        int word = slot / 64;
        int bit  = slot % 64;
        _nova_arena.free_bits[word] &= ~(1ULL << bit);
        /* Optional: madvise MADV_DONTNEED to release physical pages. */
        #ifdef __linux__
        madvise(ptr, _nova_arena.slot_size, MADV_DONTNEED);
        #endif
        return;
    }
    /* Fallback path — calloc'd outside arena. */
    free(ptr);
}
```

### `GC_add_roots` registration

В `nova_fiber_arena_init`:

```c
void nova_fiber_arena_init(void) {
    if (_nova_arena.base) return;

    size_t arena_size = NOVA_FIBER_ARENA_SIZE;  /* 256MB */
    #ifdef _WIN32
        /* Windows: reserve only (no commit). VirtualAlloc with MEM_RESERVE
         * leaves pages uncommitted; access faults until commit. We don't
         * commit lazily on Windows — fixed slot allocation, all pages
         * pre-committed по mere allocation. Acceptable trade-off для
         * bootstrap. Windows growable stacks — отдельная задача (SEH
         * guard pages). */
        _nova_arena.base = VirtualAlloc(NULL, arena_size,
                                          MEM_RESERVE | MEM_COMMIT,
                                          PAGE_READWRITE);
    #else
        /* Linux/macOS: MAP_NORESERVE — pages не commit'ятся пока не touched.
         * Этот flag отключает overcommit accounting — критично для
         * arena сценария где мы reserve много, использовать мало. */
        _nova_arena.base = mmap(NULL, arena_size,
                                  PROT_READ | PROT_WRITE,
                                  MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE,
                                  -1, 0);
        if (_nova_arena.base == MAP_FAILED) _nova_arena.base = NULL;
    #endif

    if (!_nova_arena.base) {
        fprintf(stderr, "nova: fiber arena reservation failed\n");
        abort();
    }

    /* Register as single GC root. Boehm scans this entire range
     * on every collection cycle — covers all suspended fiber stacks. */
    #ifdef NOVA_GC_BOEHM
    GC_add_roots(_nova_arena.base, _nova_arena.base + arena_size);
    #endif

    _nova_arena.slot_size = NOVA_FIBER_STACK_SIZE;  /* 2MB */
    _nova_arena.slot_count = arena_size / _nova_arena.slot_size;
    _nova_arena.free_bits[0] = 0;
    _nova_arena.free_bits[1] = 0;
}
```

### Удаление `_NOVA_GC_DISABLE`

После arena установки в `fibers.h`:

```c
#ifdef NOVA_GC_BOEHM
#  include <gc.h>
/* Plan 41: arena registration делает stacks GC-visible — disable больше
 * не нужен. Концептуально безопасно даже на single-thread runtime'е. */
#  define _NOVA_GC_DISABLE()  ((void)0)
#  define _NOVA_GC_ENABLE()   ((void)0)
#else
#  define _NOVA_GC_DISABLE()  ((void)0)
#  define _NOVA_GC_ENABLE()   ((void)0)
#endif
```

(Оставляем макросы для backward compat, но они no-op.)

---

## Фазы

### Этап 1 — Arena allocator implementation

- `compiler-codegen/nova_rt/fiber_arena.c` + `.h`.
- `nova_fiber_alloc` / `dealloc` / `arena_init` / `stats` functions.
- TLS storage для per-thread arena.
- Wire через `mco_desc.alloc_cb` в `fibers.h::nova_fiber_run`.

**Acceptance:**
- Build clean.
- 262/262 regression PASS (transparent change).
- `nova_fiber_arena_stats()` показывает arena allocated, slots used > 0.

### Этап 2 — Удалить `_NOVA_GC_DISABLE`

- Поменять макросы на no-op.
- Запустить полный regression.
- Запустить `memory_growth_check.nv` (stress 1000+ fibers) — verify
  GC корректно сканит arena, нет use-after-free crashes.

**Acceptance:**
- 262/262 PASS даже с GC enabled во время scheduler ticks.
- `gc_pause_bench.nv` не показывает degradation.

### Этап 3 — Stats API + tests

- `nova_fiber_arena_stats()` exposed через std.runtime.gc (или новый
  std.runtime.fibers).
- Тесты:
  - Arena exhaustion fallback (создать > slot_count fibers, проверить
    что fallback'нулся на calloc и не упал).
  - Slot reuse (создать-завершить 10000 fibers, проверить что arena
    physical usage не растёт).
  - GC сканит arena (allocate object, hold pointer в fiber stack,
    yield, force GC, resume, verify object alive).

**Acceptance:**
- 3 новых тестов PASS.
- `madvise MADV_DONTNEED` после dealloc'а — verify physical memory
  возвращается ОС (через `/proc/self/status` RSS).

### Этап 4 — Linux Docker validation

- Прогнать на Linux Docker (уже инфраструктура из Plan 40 Этап 5).
- Verify lazy commit работает — total RSS не равен total virtual.

**Acceptance:**
- Linux Docker: 262/262 PASS (с perf bench skipped по Plan 40 reasons).
- RSS measurement при 1000 fibers ~ 16-32MB (vs 56MB при текущем
  calloc'd 56KB × 1000).

### Этап 5 — Docs

- spec/decisions/06-concurrency.md: новый D-decision про fiber stack
  allocation (или extension существующего D71).
- project-creation.txt + simplifications.md.
- discussion-log.md.

---

## Зависимости

- **Plan 27 (Boehm GC)** — ✅ закрыт; используем `NOVA_GC_BOEHM` define
  + `GC_add_roots` API.
- **Plan 22 (libuv)** — ✅ закрыт; libuv не задействован напрямую.

## Открывает

- **Plan 23 (M:N runtime)** — снимает GC blocker (Boehm root limit +
  `_NOVA_GC_DISABLE` incompatibility).
- **Plan 25 G6 (growable stacks)** — частично closed для Linux/macOS.
  Windows нужен SEH guard pages — отдельная задача.

---

## Acceptance criteria (полный список)

**Correctness:**
- 262/262 nova_tests + 46/46 std type-check PASS на Windows.
- 261/261 на Linux Docker (perf bench excluded по Plan 40 reasons).
- `_NOVA_GC_DISABLE` удалён, regression сохраняется.
- Hidden UAF risk class устранён — explicit verification через test
  «GC во время fiber yield не ломает указатели».

**Performance:**
- Arena allocation overhead < 5% vs текущий calloc.
- Slot reuse работает: 10000 sequential fibers не растят physical RSS.
- На Linux RSS при N fibers (typical 8KB stack usage) ≈ 32KB × N,
  не 56KB × N (текущий fixed calloc).

**Compatibility:**
- Linux x86_64 / aarch64 — full support (lazy commit working).
- Windows x86_64 — fixed 2MB committed (degradation acceptable;
  growable stacks отдельная задача).
- macOS arm64 — full support.

---

## Risks

1. **Arena exhaustion при >128 active fibers per thread** — fallback на
   calloc + warning log. Не fatal, но degrades Boehm scaling. Mitigation:
   увеличить slot count в config (например 512 или 1024). 1024 slots ×
   2MB = 2GB virtual — всё ещё OK.

2. **Lazy commit на Windows не работает** — Windows VirtualAlloc requires
   explicit MEM_COMMIT per page. Без SEH guard pages — committed
   upfront. Trade-off: 256MB virtual = 256MB physical на Windows arena.
   16 threads × 256MB = 4GB physical только под reserved arena. **Это
   плохо для Windows production**. Mitigation: на Windows fallback на
   меньший arena (32MB × 16 slots × 2MB), или вообще skip arena подход
   на Windows для bootstrap.

3. **madvise MADV_DONTNEED race с suspended fibers** — если другой fiber
   work-steal'нулся в этот slot до madvise, страница уже committed.
   Acceptable trade-off для bootstrap (нет M:N сейчас).

4. **Boehm GC scan'ит всё arena** — даже если 1% slots used. Не баг,
   но overhead на mark phase. На 256MB virtual / 4KB page = 65536 pages
   to scan. Modern CPU ~10GB/s sequential read → mark phase +25ms на
   GC. **Это значимо.** Mitigation: использовать `GC_add_roots` только
   на **активные** slots — но это возвращает к проблеме многих entries.
   Альтернатива: оставить full arena scan, документировать
   GC pause expectation как «proportional to arena size, not fiber count».

5. **TLS arena lifetime** — когда thread завершается, нужен cleanup
   через `pthread_key_create` destructor или `__attribute__((destructor))`.
   Иначе arena leaks при frequent thread spawn/join.

---

## Open questions

- **Q1: Slot size** — 2MB (как mainstream thread) или 1MB? 2MB соответствует
  minicoro `MCO_USE_VMEM_ALLOCATOR` default. Делает arena bigger но
  reduces stack overflow risk.

- **Q2: Slot count per arena** — 128 default? 512? Bounded fibers per thread
  feasible. 128 × 16 threads = 2048 total fibers. Достаточно для
  web server fan-out? Возможно нет — backend на 10k connections нужен
  10k+ fibers. Решение: arena resize при exhaustion (alloc вторую arena,
  +1 GC root entry, ничего страшного — 16 threads × 2 arenas = 32 entries
  всё ещё << 128).

- **Q3: Windows fixed-commit vs SEH growable** — для bootstrap fixed
  OK; SEH guard pages — отдельный план (~150 LOC, требует Windows
  exception handler magic).

- **Q4: madvise MADV_DONTNEED стратегия** — на каждый dealloc, или
  batched? Frequent madvise — много syscalls. Batched — delayed
  physical memory return. Bootstrap: на каждый dealloc (простоту > perf).

- **Q5: `MCO_USE_VMEM_ALLOCATOR` interaction** — несовместимо с arena
  (каждый стек — отдельный mmap). Этот план **заменяет** VMEM_ALLOCATOR,
  не использует его. Документировать что `MCO_USE_VMEM_ALLOCATOR` НЕ
  включается с Plan 41.

---

## Файлы

- `compiler-codegen/nova_rt/fiber_arena.c` — new, ~200 LOC.
- `compiler-codegen/nova_rt/fiber_arena.h` — new, ~50 LOC.
- `compiler-codegen/nova_rt/fibers.h` — wire через `mco_desc.alloc_cb`,
  заменить `_NOVA_GC_DISABLE` на no-op. ~30 LOC delta.
- `compiler-codegen/nova_rt/nova_rt.h` — `#include "fiber_arena.h"`.
- `compiler-codegen/src/test_runner.rs` — add `fiber_arena.c` к C sources
  при build. ~5 LOC.
- `std/runtime/fibers.nv` (new или extend) — expose `arena_stats()`. ~30 LOC.
- `nova_tests/concurrency/plan41_arena_*.nv` — 3+ тестов.

**Итого:** ~400-500 LOC, 4-5 commits, 2-3 сессии.

---

## Связь с другими планами

- **Plan 23 (M:N runtime)** — Plan 41 = critical prerequisite. Без него
  M:N невозможен (Boehm root limit + concurrent GC incompatibility).
- **Plan 25 G6 (growable stacks)** — Plan 41 закрывает на Linux/macOS,
  оставляет открытым Windows.
- **Plan 27 (Boehm GC default)** — Plan 41 удаляет R4 workaround
  (`_NOVA_GC_DISABLE`), упрощает Plan 27 maintenance.
- **Plan 40 (channel hardening)** — independent, не пересекается.
