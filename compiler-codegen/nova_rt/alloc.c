/* nova_rt/alloc.c — Phase-0 implementation: plain malloc, no GC.
 * To switch GC: replace this file only. The codegen never calls malloc directly. */

#include "alloc.h"
#include <stdlib.h>
#include <stdio.h>

void nova_gc_init(void) {}
void nova_gc_shutdown(void) {}

void* nova_alloc(size_t size) {
    void* p = malloc(size);
    if (!p) {
        fprintf(stderr, "nova: out of memory\n");
        abort();
    }
    return p;
}

/* RC stubs — no-ops in malloc mode. */
void nova_retain(void* ptr)  { (void)ptr; }
void nova_release(void* ptr) { (void)ptr; }
