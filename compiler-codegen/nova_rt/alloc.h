#ifndef NOVA_RT_ALLOC_H
#define NOVA_RT_ALLOC_H

#include <stddef.h>
#include <stdint.h>

/* GC interface — единственная точка контакта кодогенератора с памятью.
 * Реализация в alloc.c. Для смены GC — меняется только alloc.c.
 *
 * CONTRACT: nova_alloc MUST return zeroed memory. Codegen assumes zero-init
 * (record/closure/spawn-context fields are set by named assignment only;
 * unset fields must not contain garbage). All implementations must satisfy
 * this: alloc.c uses calloc, alloc_rc.c uses malloc+memset, alloc_boehm.c
 * relies on GC_malloc (Boehm API guarantee). */

void  nova_gc_init(void);
void  nova_gc_shutdown(void);
void* nova_alloc(size_t size);
void  nova_retain(void* ptr);
void  nova_release(void* ptr);

/* Instrumentation — available in all alloc implementations.
 * nova_gc_alloc_count : total allocations since nova_gc_init
 * nova_gc_free_count  : total frees/releases since nova_gc_init
 * nova_gc_live_count  : alloc_count - free_count (currently live objects)
 * nova_gc_reset_stats : zero all counters (for per-test isolation)
 */
size_t nova_gc_alloc_count(void);
size_t nova_gc_free_count(void);
size_t nova_gc_live_count(void);
void   nova_gc_reset_stats(void);

/* Plan 32: GC introspection API exposed to Nova via std.runtime.gc.
 * nova_gc_heap_size  : текущий размер heap в bytes (0 под malloc — honest sentinel)
 * nova_gc_collect    : принудительный сбор (no-op под malloc)
 * nova_gc_last_pause_ns : Plan 57.C.2 — длительность последнего collect-цикла
 *                          в наносекундах (0 под malloc; under Boehm — измеряется
 *                          через monotonic timer вокруг GC_gcollect). */
size_t   nova_gc_heap_size(void);
void     nova_gc_collect(void);
uint64_t nova_gc_last_pause_ns(void);

/* Plan 44.2 Этап 3: fiber arena introspection — std.runtime.fibers.
 *
 * На Windows (NOVA_FIBER_ARENA_ENABLED=0) все вернут 0 — honest sentinel
 * «arena не активна, fiber stacks идут через calloc-путь».
 * На Linux/macOS отражают per-thread arena stats. */
size_t nova_fibers_virtual_reserved(void);  /* bytes reserved via mmap */
size_t nova_fibers_slot_count(void);        /* total slots */
size_t nova_fibers_slots_active(void);      /* currently allocated */
size_t nova_fibers_high_water(void);        /* peak concurrent slots */
void   nova_fibers_compact(void);           /* P41-3: batch decay flush */

#endif /* NOVA_RT_ALLOC_H */
