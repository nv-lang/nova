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

#endif /* NOVA_RT_ALLOC_H */
