/* nova_rt/alloc_boehm.c — Boehm GC implementation.
 *
 * Full tracing GC: collects cycles, concurrent mark (on platforms that support it).
 * Matches Nova spec D6: managed heap, programmer never calls free.
 *
 * To use: compile with this file instead of alloc.c or alloc_rc.c, and link gc.lib.
 *   cl.exe ... nova_rt\alloc_boehm.c /I<vcpkg_installed\x64-windows-static\include>
 *             /link <vcpkg_installed\x64-windows-static\lib\gc.lib>
 *                   <vcpkg_installed\x64-windows-static\lib\atomic_ops.lib>
 *
 * nova_retain / nova_release are no-ops — GC handles everything automatically.
 *
 * Contract: nova_alloc MUST return zeroed memory. GC_malloc already satisfies
 * this (Boehm API guarantee). No memset needed.
 *
 * Stat functions: nova_gc_live_count / nova_gc_free_count are approximations —
 * exact live count requires finalizer cooperation which Boehm does not provide.
 * _alloc_count is an upper bound; GC may have freed some objects since. */

#include "alloc.h"

/* Nova is single-threaded (libuv + cooperative minicoro). No GC_THREADS needed. */
#include <gc.h>

#include <stdio.h>
#include <stdint.h>

/* Monotonic alloc counter — incremented on every nova_alloc call.
 * Used by nova_gc_alloc_count() and nova_gc_reset_stats(). */
static size_t _alloc_count = 0;

void nova_gc_init(void) {
    /* Plan 41 Etap 1 wire-up fix (2026-05-12): Boehm/Docker hardening
     * before GC_INIT().
     *
     * GC_set_no_dls(1) — skip dynamic-libraries data-segment scan.
     * Without this Boehm's GC_init_linux_data_start() walks /proc/self/maps
     * to detect data segment; under Docker restricted permissions /proc walk
     * returns inconsistent results → SEGV в GC_find_limit_with_bound during
     * GC_init.
     *
     * Nova statically links its runtime — dynamic library roots не нужны.
     * Только main binary data segment + Plan 41 fiber arena ranges + heap. */
    GC_set_no_dls(1);

    GC_INIT();
    /* Allow GC to run finalisers / collect aggressively */
    GC_set_all_interior_pointers(1);
}

void nova_gc_shutdown(void) {
    /* Plan 41 Etap 1 (2026-05-12): skip final GC_gcollect.
     *
     * Under Ubuntu 22.04 system libgc (built с PARALLEL_MARK), GC_gcollect
     * triggers parallel marker threads. Под Docker thread stack walks
     * могут fail → SEGV в GC_do_local_mark / GC_do_parallel_mark.
     *
     * Final collect не нужен функционально: процесс завершается, kernel
     * освобождает всю память. Finalizers (если у нас будут) могут не
     * запуститься, но bootstrap Nova не использует Boehm finalizers.
     *
     * Side benefit: faster shutdown (no full mark+sweep). */
    /* GC_gcollect(); — disabled */
}

void* nova_alloc(size_t size) {
    void* p = GC_malloc(size);
    if (!p) {
        fprintf(stderr, "nova: out of memory\n");
        abort();
    }
    _alloc_count++;
    return p;
}

/* RC ops are no-ops under Boehm — GC traces references automatically */
void nova_retain(void* ptr)  { (void)ptr; }
void nova_release(void* ptr) { (void)ptr; }

/* Stat functions required by alloc.h. Boehm does not expose per-object
 * freed/live counts without finalizers; we use heap_size as a proxy.
 * Conservative: nova_gc_free_count returns 0 (never overclaims). */
size_t nova_gc_alloc_count(void) { return _alloc_count; }
size_t nova_gc_free_count(void)  { return 0; /* conservative: GC freed count unavailable */ }
size_t nova_gc_live_count(void)  { return _alloc_count; /* upper bound; GC may have freed some */ }
void   nova_gc_reset_stats(void) { _alloc_count = 0; }

/* Plan 32: introspection — under Boehm full GC support. */
size_t nova_gc_heap_size(void) { return GC_get_heap_size(); }
void   nova_gc_collect(void)   { GC_gcollect(); }
