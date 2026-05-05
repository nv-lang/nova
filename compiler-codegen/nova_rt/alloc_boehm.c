/* nova_rt/alloc_boehm.c — Boehm GC implementation.
 *
 * Full tracing GC: collects cycles, concurrent mark (on platforms that support it).
 * Matches Nova spec D6: managed heap, programmer never calls free.
 *
 * To use: compile with this file instead of alloc.c or alloc_rc.c, and link gc.lib.
 *   cl.exe ... nova_rt\alloc_boehm.c /I<vcpkg_installed\x64-windows-static\include>
 *             /link <vcpkg_installed\x64-windows-static\lib\gc.lib>
 *
 * nova_retain / nova_release are no-ops — GC handles everything automatically.
 */

#include "alloc.h"

/* Boehm GC requires GC_THREADS on Windows for thread-safety */
#define GC_THREADS
#include <gc.h>

#include <stdio.h>
#include <string.h>

void nova_gc_init(void) {
    GC_INIT();
    /* Allow GC to run finalisers / collect aggressively */
    GC_set_all_interior_pointers(1);
}

void nova_gc_shutdown(void) {
    GC_gcollect();
}

void* nova_alloc(size_t size) {
    void* p = GC_malloc(size);
    if (!p) {
        fprintf(stderr, "nova: out of memory\n");
        abort();
    }
    memset(p, 0, size);
    return p;
}

/* RC ops are no-ops under Boehm — GC traces references automatically */
void nova_retain(void* ptr)  { (void)ptr; }
void nova_release(void* ptr) { (void)ptr; }
