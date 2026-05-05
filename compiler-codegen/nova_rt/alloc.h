#ifndef NOVA_RT_ALLOC_H
#define NOVA_RT_ALLOC_H

#include <stddef.h>

/* GC interface — единственная точка контакта кодогенератора с памятью.
 * Реализация в alloc.c. Для смены GC — меняется только alloc.c. */

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

#endif /* NOVA_RT_ALLOC_H */
