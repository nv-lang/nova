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

static size_t _alloc_count = 0;
static size_t _free_count  = 0;

void nova_gc_init(void)     { _alloc_count = 0; _free_count = 0; }
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
    _alloc_count++;
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
        _free_count++;
    }
}

/* Plan 83.4.5.8 (2026-05-24): uncollectable allocation. Под RC backend
 * identical to nova_alloc, free через nova_free_uncollectable. */
void* nova_alloc_uncollectable(size_t size) {
    NovaRcHeader* h = (NovaRcHeader*)malloc(HEADER_SIZE + size);
    if (!h) {
        fprintf(stderr, "nova: out of memory (uncollectable)\n");
        abort();
    }
    h->refcount = 1;
    void* p = HEADER_TO_PTR(h);
    memset(p, 0, size);
    _alloc_count++;
    return p;
}

void nova_free_uncollectable(void* ptr) {
    if (!ptr) return;
    NovaRcHeader* h = PTR_TO_HEADER(ptr);
    free(h);
    _free_count++;
}

size_t nova_gc_alloc_count(void) { return _alloc_count; }
size_t nova_gc_free_count(void)  { return _free_count; }
size_t nova_gc_live_count(void)  { return _alloc_count - _free_count; }
void   nova_gc_reset_stats(void) { _alloc_count = 0; _free_count = 0; }

/* Plan 32 + 57.C.2: introspection stubs для RC backend (legacy, unused
 * in default builds). RC не имеет collect-cycle — sentinel zeros. */
size_t   nova_gc_heap_size(void)     { return 0; }
void     nova_gc_collect(void)       { /* no-op: RC is incremental */ }
uint64_t nova_gc_last_pause_ns(void) { return 0; }
