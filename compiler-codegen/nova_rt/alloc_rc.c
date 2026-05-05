/* nova_rt/alloc_rc.c — Phase-3 implementation: reference counting.
 *
 * Layout: every allocation has a header { uint32_t refcount } prepended.
 * nova_alloc  → allocates header + payload, refcount = 1, returns payload ptr.
 * nova_retain → increments refcount.
 * nova_release → decrements refcount, frees if 0.
 *
 * To switch from malloc to RC: compile with this file instead of alloc.c.
 * The codegen never changes — only alloc.c is swapped.
 *
 * Limitation: cycles are not collected (no cycle detector).
 * For cycle-safe GC, replace this file with a tracing/Boehm implementation.
 */

#include "alloc.h"
#include <stdlib.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef struct {
    uint32_t refcount;
} NovaRcHeader;

#define HEADER_SIZE (sizeof(NovaRcHeader))
#define PTR_TO_HEADER(p) ((NovaRcHeader*)((char*)(p) - HEADER_SIZE))
#define HEADER_TO_PTR(h) ((void*)((char*)(h) + HEADER_SIZE))

void nova_gc_init(void) {}
void nova_gc_shutdown(void) {}

void* nova_alloc(size_t size) {
    NovaRcHeader* h = (NovaRcHeader*)malloc(HEADER_SIZE + size);
    if (!h) {
        fprintf(stderr, "nova: out of memory\n");
        abort();
    }
    h->refcount = 1;
    void* p = HEADER_TO_PTR(h);
    memset(p, 0, size);
    return p;
}

void nova_retain(void* ptr) {
    if (!ptr) return;
    NovaRcHeader* h = PTR_TO_HEADER(ptr);
    h->refcount++;
}

void nova_release(void* ptr) {
    if (!ptr) return;
    NovaRcHeader* h = PTR_TO_HEADER(ptr);
    if (--h->refcount == 0) {
        free(h);
    }
}
