/* nova_rt/alloc.c — Phase-0 implementation: plain malloc, no GC.
 * To switch GC: replace this file only. The codegen never calls malloc directly.
 *
 * Contract: nova_alloc MUST return zeroed memory. Codegen assumes zero-init
 * (see emit_c.rs: record/closure/spawn-context fields set by assignment only).
 * Use calloc, not malloc. */

#include "alloc.h"
#include <stdlib.h>
#include <stdio.h>

static size_t _alloc_count = 0;
static size_t _free_count  = 0;

void nova_gc_init(void)     { _alloc_count = 0; _free_count = 0; }
void nova_gc_shutdown(void) {}

void* nova_alloc(size_t size) {
    void* p = calloc(1, size);
    if (!p) {
        fprintf(stderr, "nova: out of memory\n");
        abort();
    }
    _alloc_count++;
    return p;
}

/* RC stubs — no-ops in malloc mode (no free, so free_count stays 0). */
void nova_retain(void* ptr)  { (void)ptr; }
void nova_release(void* ptr) { (void)ptr; }

size_t nova_gc_alloc_count(void) { return _alloc_count; }
size_t nova_gc_free_count(void)  { return _free_count; }
size_t nova_gc_live_count(void)  { return _alloc_count - _free_count; }
void   nova_gc_reset_stats(void) { _alloc_count = 0; _free_count = 0; }
