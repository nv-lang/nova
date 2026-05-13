// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_FIBER_ARENA_H
#define NOVA_RT_FIBER_ARENA_H

/* Plan 41 Etap 1 — per-thread fiber stack arena (Linux/macOS only).
 *
 * Goal: replace minicoro default calloc(56KB) stack allocator с
 * arena-based allocation для:
 *   1. Single GC root per thread (vs N roots per fiber — упирались
 *      в Boehm MAX_ROOT_SETS=128 в Plan 27 R4).
 *   2. Растущие стеки автоматически (mmap MAP_NORESERVE — lazy commit
 *      pages только под touched memory; Linux/macOS).
 *   3. Concurrent GC ready (Plan 23 prerequisite) — arena registered
 *      as GC root, suspended stacks visible to scanner всегда. Plan 41
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
 * Plan 41 P0 items addressed here:
 *   - P41-5: guard pages (PROT_NONE).
 *   - P41-11: active-range roots (lazy commit preserved).
 *   - P41-12: pthread_key cleanup.
 *   - P41-13: requires -fstack-clash-protection (set в test_runner.rs).
 *   - P41-14: madvise MADV_NOHUGEPAGE.
 *
 * Plan 41 P0 items deferred to integration phase:
 *   - P41-2: slot count 256 (boots) → 4096 (production) после validation.
 *   - P41-3: vm.overcommit_memory=2 detection — TODO.
 *   - P41-6: SIGSEGV handler с pretty error — TODO.
 *
 * Plan 41 deferred to Plan 23:
 *   - P41-15: cross-thread dealloc atomic bitmap (single-thread bootstrap OK).
 */

#include <stddef.h>
#include <stdbool.h>

#if defined(__linux__) || defined(__APPLE__)
  #define NOVA_FIBER_ARENA_ENABLED 1
#else
  #define NOVA_FIBER_ARENA_ENABLED 0
#endif

/* Default config — может быть override'нут через NOVA_FIBER_ARENA_* env.
 *
 * Plan 41 audit P41-2: slot_count bumped 256 → 4096 для production.
 * 4096 × 2MB = 8GB virtual per thread. На x86_64 (256TB virtual)
 * тривиально; physical commit lazy через MAP_NORESERVE. Real workloads
 * (web server 10k connections × 4-8 fibers per request) нуждаются в
 * 4k-16k concurrent fibers per process.
 *
 * Slots reused через bitmap free-list — реальный peak ограничен только
 * concurrent (не cumulative) fibers per worker thread. */
#define NOVA_FIBER_STACK_SIZE     (2 * 1024 * 1024)  /* 2MB per slot */
/* Plan 41 audit R8 (2026-05-13): 32-bit address space недостаточен для
 * 8 GB virtual. Downsize до 64 slots × 2MB = 128 MB на 32-bit. На 64-bit
 * остаёмся 4096. */
#if defined(__SIZEOF_POINTER__) && __SIZEOF_POINTER__ < 8
  #define NOVA_FIBER_SLOT_COUNT   64                 /* 64 × 2MB = 128 MB virtual (32-bit) */
#else
  #define NOVA_FIBER_SLOT_COUNT   4096               /* 4096 × 2MB = 8GB virtual per thread (64-bit) */
#endif
/* Plan 41 audit R8 (2026-05-13): 16 KB guard (было 4 KB) для CVE-2017-1000366
 * stack-clash protection. Single 4 KB guard может быть skipped одним
 * SP-subtract если функция аллоцирует >4 KB local array. 16 KB существенно
 * затрудняет clash (нужен >16 KB allocation в одном instruction). Cost:
 * 12 KB × 4096 = 48 MB extra virtual reservation, zero physical
 * (PROT_NONE never commits). */
#define NOVA_FIBER_GUARD_SIZE     (16 * 1024)        /* 16 KB PROT_NONE at slot base */

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

/* minicoro alloc callbacks. Wire через mco_desc.alloc_cb / dealloc_cb.
 *
 * NOTE: minicoro signature (allocator_data, size) → ptr; we ignore
 * allocator_data (TLS instead).
 */
void* nova_fiber_alloc(size_t size, void* allocator_data);
void  nova_fiber_dealloc(void* ptr, size_t size, void* allocator_data);

/* Check whether ptr is inside this thread's arena (для assertions). */
bool nova_fiber_arena_contains(const void* ptr);

#endif /* NOVA_FIBER_ARENA_ENABLED */

#endif /* NOVA_RT_FIBER_ARENA_H */
