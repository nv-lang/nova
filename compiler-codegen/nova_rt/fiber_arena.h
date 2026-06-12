// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_FIBER_ARENA_H
#define NOVA_RT_FIBER_ARENA_H

/* Plan 44.2 Etap 1 — per-thread fiber stack arena (Linux/macOS only).
 *
 * Goal: replace minicoro default calloc(56KB) stack allocator с
 * arena-based allocation для:
 *   1. Single GC root per thread (vs N roots per fiber — упирались
 *      в Boehm MAX_ROOT_SETS=128 в Plan 27 R4).
 *   2. Растущие стеки автоматически (mmap MAP_NORESERVE — lazy commit
 *      pages только под touched memory; Linux/macOS).
 *   3. Concurrent GC ready (Plan 23 prerequisite) — arena registered
 *      as GC root, suspended stacks visible to scanner всегда. Plan 44.2
 *      Этап 2 удалил _NOVA_GC_DISABLE workaround полностью.
 *
 * Architecture (Linux/macOS):
 *   - One arena per thread (TLS, через __thread).
 *   - 512MB virtual reserved (256 slots × 2MB).
 *   - Guard page (PROT_NONE, 4KB) at bottom of every slot —
 *     stack overflow → SIGSEGV → handler prints diagnostic.
 *   - Active-range GC roots: register только [base, base+high_water*slot_size]
 *     to avoid forcing Boehm scan to commit untouched pages.
 *   - pthread_key cleanup on thread exit.
 *   - madvise(MADV_NOHUGEPAGE) после mmap — keep 4KB granularity.
 *
 * Windows: NOT used. Текущий calloc-path остаётся (single-thread
 * cooperative — GC не вытесняет fiber mid-stack).
 * Windows growable stacks через SEH guard pages — Plan 42+.
 *
 * Plan 44.2 P0 items addressed here:
 *   - P41-5: guard pages (PROT_NONE).
 *   - P41-11: active-range roots (lazy commit preserved).
 *   - P41-12: pthread_key cleanup.
 *   - P41-13: requires -fstack-clash-protection (set в test_runner.rs).
 *   - P41-14: madvise MADV_NOHUGEPAGE.
 *
 * Plan 44.2 P0 items deferred to integration phase:
 *   - P41-2: slot count 256 (boots) → 4096 (production) после validation.
 *   - P41-3: vm.overcommit_memory=2 detection — TODO.
 *   - P41-6: SIGSEGV handler с pretty error — TODO.
 *
 * Plan 44.2 deferred to Plan 23:
 *   - P41-15: cross-thread dealloc atomic bitmap (single-thread bootstrap OK).
 */

#include <stddef.h>
#include <stdbool.h>

/* Plan 82 Ф.1: Windows присоединён к arena-пути. POSIX-реализация —
 * fiber_arena.c (mmap); Windows — fiber_arena_win.c (VirtualAlloc lazy-
 * commit). Оба файла компилируются на всех платформах, каждый — пустой
 * TU вне своей ОС; линкуются всегда. API ниже — общий. */
#if defined(__linux__) || defined(__APPLE__) || defined(_WIN32)
  #define NOVA_FIBER_ARENA_ENABLED 1
#else
  #define NOVA_FIBER_ARENA_ENABLED 0
#endif

/* Plan 149: arena is RUNTIME-configurable (GOMAXPROCS-style knobs of the
 * finished program, NOT compiler params):
 *   - NOVA_FIBER_STACK  env / nova.toml [runtime].fiber_stack — per-fiber
 *     stack slot size (human-friendly "4MB"/"4194304"; [256KB,256MB],
 *     round-UP to page). Default 4MB.
 *   - NOVA_MAX_FIBERS   env / nova.toml [runtime].max_fibers — max concurrent
 *     fibers PER WORKER ([64, SLOT_COUNT_MAX], round-UP to ×64). Default
 *     16384. Total process capacity = slot_count × NOVA_MAXPROCS.
 * Precedence: env > nova.toml(-D) > builtin #define. Read fresh in
 * nova_fiber_arena_init() so every worker arena honors them. Garbage env/toml
 * → stderr warning + builtin default (never crash on config). See D233.
 *
 * The macros below are the BUILTIN compile-time defaults feeding the
 * #ifndef *_DEFAULT chain.
 *
 * Plan 44.2 audit P41-2: slot_count bumped 256 → 4096 для production.
 * 4096 × 2MB = 8GB virtual per thread. На x86_64 (256TB virtual)
 * тривиально; physical commit lazy через MAP_NORESERVE. Real workloads
 * (web server 10k connections × 4-8 fibers per request) нуждаются в
 * 4k-16k concurrent fibers per process.
 *
 * Slots reused через bitmap free-list — реальный peak ограничен только
 * concurrent (не cumulative) fibers per worker thread.
 *
 * Plan 83.4.5.10 Ф.2 (2026-05-24): attempted 8MB → 1MB downsize —
 * REVERTED back to 8MB. cancellation_test (within[T]/race2[T] generic
 * monomorphized nested recursion) сразу overflow'ит на 1MB. Возможно
 * minicoro internal stack overhead + Boehm GC reserves bigger чем
 * expected. V2 followup — выяснить точный stack budget per test и
 * выбрать минимально-достаточный slot size (~2MB?).
 *
 * Plan 149 Ф.2 (2026-06-12): default lowered 8MB → 4MB. 4MB по-прежнему
 * щедро (>2× минимально-достаточного ~2MB из 83.4.5.10), но даёт 2×
 * плотность fiber'ов из коробки. Per-fiber stack size теперь RUNTIME-
 * настраиваемый (NOVA_FIBER_STACK env / nova.toml [runtime].fiber_stack /
 * -DNOVA_FIBER_STACK_DEFAULT). Этот #define — **build-time builtin
 * default**, кормит NOVA_FIBER_STACK_DEFAULT через #ifndef ниже. */
#define NOVA_FIBER_STACK_SIZE     (4 * 1024 * 1024)  /* 4MB per slot builtin default (demand-paged via MAP_NORESERVE) */
/* Plan 149 Ф.1: runtime clamp bounds для NOVA_FIBER_STACK. Любое
 * пользовательское значение округляется ВВЕРХ до page-align затем
 * зажимается в [MIN, MAX]. usable = slot_size − guard (16KB); с floor
 * 256KB usable = 240KB > 0 — гарантия инварианта. */
#define NOVA_FIBER_STACK_MIN      (256 * 1024)        /* 256KB floor */
#define NOVA_FIBER_STACK_MAX      (256 * 1024 * 1024) /* 256MB ceil */
/* Plan 149 Ф.1/Ф.2: split compile-time bitmap MAX from runtime DEFAULT.
 *
 *  - NOVA_FIBER_SLOT_COUNT_DEFAULT — runtime builtin default for
 *    a->slot_count when neither NOVA_MAX_FIBERS env nor
 *    -DNOVA_MAX_FIBERS_DEFAULT (toml) is present. Per-worker.
 *  - NOVA_FIBER_SLOT_COUNT_MAX — COMPILE-TIME ceiling. Sizes the
 *    free_bits/used_bits/dirty_bits bitmaps so env may RAISE slot_count
 *    above the default. 262144 slots ⇒ ceil(262144/64)=4096 uint64_t =
 *    32KB bitmap per arena (× workers — копейки). On 32-bit keep tiny
 *    (address space tight).
 *  - NOVA_FIBER_SLOT_COUNT_MIN — runtime floor (one bitmap word = 64).
 *
 * Plan 149 must_fix #4: the per-platform branch sets BOTH _DEFAULT and
 * _MAX inside the SAME #if/#elif/#else cascade (как старый
 * NOVA_FIBER_SLOT_COUNT), BEFORE any generic #ifndef. Иначе 32-bit
 * target получил бы bitmap на 262144 слов через generic-fallback.
 *
 * Runtime DEFAULT unified to 16384 на всех 64-bit/Windows платформах
 * (раньше POSIX 64-bit был 4096) — реальный потолок «4k-16k concurrent
 * fibers per process» (план §3); 16384 не раздувает bitmap (его размер
 * задаёт _MAX, не _DEFAULT). */
#if defined(__SIZEOF_POINTER__) && __SIZEOF_POINTER__ < 8
  /* 32-bit: address space tight — small default + small MAX. */
  #define NOVA_FIBER_SLOT_COUNT_DEFAULT  16            /* 16 × 4MB = 64 MB virtual (32-bit) */
  #ifndef NOVA_FIBER_SLOT_COUNT_MAX
    #define NOVA_FIBER_SLOT_COUNT_MAX    1024          /* ceil(1024/64)=16 words = 128B bitmap */
  #endif
#elif defined(_WIN32)
  #define NOVA_FIBER_SLOT_COUNT_DEFAULT  16384         /* per-worker runtime default */
  #ifndef NOVA_FIBER_SLOT_COUNT_MAX
    #define NOVA_FIBER_SLOT_COUNT_MAX    262144        /* 4096 words = 32KB bitmap/arena */
  #endif
#else
  #define NOVA_FIBER_SLOT_COUNT_DEFAULT  16384         /* per-worker runtime default (64-bit) */
  #ifndef NOVA_FIBER_SLOT_COUNT_MAX
    #define NOVA_FIBER_SLOT_COUNT_MAX    262144        /* 4096 words = 32KB bitmap/arena */
  #endif
#endif
#define NOVA_FIBER_SLOT_COUNT_MIN      64             /* one bitmap word */
/* Plan 44.2 audit R8 (2026-05-13): 16 KB guard (было 4 KB) для CVE-2017-1000366
 * stack-clash protection. Single 4 KB guard может быть skipped одним
 * SP-subtract если функция аллоцирует >4 KB local array. 16 KB существенно
 * затрудняет clash (нужен >16 KB allocation в одном instruction). Cost:
 * 12 KB × 4096 = 48 MB extra virtual reservation, zero physical
 * (PROT_NONE never commits). */
#define NOVA_FIBER_GUARD_SIZE     (16 * 1024)        /* 16 KB PROT_NONE at slot base */

/* Plan 149 Ф.3: defaults-resolution chain (precedence env > toml(-D) >
 * builtin). nova.toml [runtime] bakes -DNOVA_FIBER_STACK_DEFAULT /
 * -DNOVA_MAX_FIBERS_DEFAULT (raw integers — bytes / count) at build time;
 * absent → these #ifndef fallbacks pick the builtin #define. At runtime the
 * env vars (read in nova_fiber_arena_init) override either of these
 * compile-time defaults. BOTH the env value AND the -D/builtin default go
 * through round-UP + clamp, so a garbage toml -D is also clamped, never
 * trusted blindly. */
#ifndef NOVA_FIBER_STACK_DEFAULT
  #define NOVA_FIBER_STACK_DEFAULT   NOVA_FIBER_STACK_SIZE          /* 4MB builtin */
#endif
#ifndef NOVA_MAX_FIBERS_DEFAULT
  #define NOVA_MAX_FIBERS_DEFAULT    NOVA_FIBER_SLOT_COUNT_DEFAULT  /* 16384 builtin */
#endif

#if NOVA_FIBER_ARENA_ENABLED

/* Per-thread arena state (TLS). Forward-declared; impl в fiber_arena.c. */

typedef struct NovaFiberArena NovaFiberArena;

/* Initialize per-thread arena lazily on first use. Idempotent —
 * safe вызывать multiple times. */
void nova_fiber_arena_init(void);

/* Stats (для diagnostics / std.runtime.fibers later). */
typedef struct {
    size_t virtual_reserved;  /* Bytes reserved via mmap. */
    size_t slot_count;        /* Total slots (== virtual / slot_size). */
    size_t slots_active;      /* Currently allocated slots. */
    size_t high_water;        /* Peak concurrent slots (since init). */
} NovaFiberArenaStats;

NovaFiberArenaStats nova_fiber_arena_stats(void);

/* Plan 44.2 P41-3: explicit decay — flush physical pages of free slots
 * batched single MADV_DONTNEED per contiguous run. For long-running
 * workloads без natural idle (server с постоянно активными fibers).
 * No-op если arena не activated. */
void nova_fiber_arena_compact(void);

/* minicoro alloc callbacks. Wire через mco_desc.alloc_cb / dealloc_cb.
 *
 * NOTE: minicoro signature (allocator_data, size) → ptr; we ignore
 * allocator_data (TLS instead).
 */
void* nova_fiber_alloc(size_t size, void* allocator_data);
void  nova_fiber_dealloc(void* ptr, size_t size, void* allocator_data);

/* Check whether ptr is inside this thread's arena (для assertions). */
bool nova_fiber_arena_contains(const void* ptr);

/* Plan 149 Ф.1 (review must_fix #1/#2): runtime per-fiber stack slot size.
 *
 * Lazily inits this thread's arena (idempotent) and returns the FINAL,
 * resolved a->slot_size (env ∨ -D ∨ builtin, after round-UP + clamp). The
 * minicoro desc-init (fibers.h::_nova_mco_desc_init_arena) MUST derive its
 * stack_size from THIS value, NOT from the compile-time NOVA_FIBER_STACK_SIZE
 * macro — otherwise minicoro's coro_size stays fixed at the compile-time
 * default and (a) a LARGER env stack is wasted reservation (fiber still
 * overflows at compile-time depth) and (b) a SMALLER env stack makes
 * coro_size > usable → nova_fiber_alloc returns NULL → fiber create fails.
 * Deriving stack_size from this runtime value scales coro_size with the
 * arena slot, so AC2 (env stack takes effect) holds and the 256KB floor is
 * usable. */
size_t nova_fiber_arena_slot_size(void);

/* Plan 82 Ф.1 (Windows): committed-low начального окна стека слота,
 * содержащего block_ptr (== указатель из nova_fiber_alloc == mco_coro*).
 * fibers.c пишет это в ctx.stack_limit после mco_create — обязательный
 * патч для lazy-commit (иначе __chkstk-код крашит на MSVC, Ф.0 test a).
 * POSIX-реализация возвращает NULL (патч не нужен — kernel demand-paging). */
void* nova_fiber_committed_low(const void* block_ptr);

/* Plan 82 Ф.3 — M:N lifecycle (Windows arena — heap-структуры в
 * глобальном append-only списке; каждый поток имеет свою арену).
 *
 * nova_fiber_arena_thread_exit — worker-поток зовёт перед
 *   GC_unregister_my_thread: обнуляет TLS-указатель (структура арены
 *   остаётся в списке для GC-обхода до shutdown).
 * nova_fiber_arena_release_retired — nova_runtime_shutdown зовёт ПОСЛЕ
 *   join всех worker'ов: освобождает арены завершившихся worker-потоков
 *   (эксклюзивный момент — гонок с GC-обходом нет).
 * POSIX — no-op (арена в TLS, освобождается pthread_key при выходе потока). */
void nova_fiber_arena_thread_exit(void);
void nova_fiber_arena_release_retired(void);

#endif /* NOVA_FIBER_ARENA_ENABLED */

#endif /* NOVA_RT_FIBER_ARENA_H */
