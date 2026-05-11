/* nova_rt/gc_test_helpers.c — standalone Boehm GC verification test.
 *
 * Compile (Windows/Clang):
 *   clang compiler-codegen/nova_rt/alloc_boehm.c \
 *         compiler-codegen/nova_rt/gc_test_helpers.c \
 *         -I compiler-codegen/vcpkg_installed/x64-windows-static/include \
 *         -DGC_THREADS -DNOVA_GC_BOEHM \
 *         -L compiler-codegen/vcpkg_installed/x64-windows-static/lib \
 *         -lgc -latomic_ops -o gc_helpers_test.exe
 *
 * Compile (Linux/GCC):
 *   gcc nova_rt/alloc_boehm.c nova_rt/gc_test_helpers.c \
 *       -DGC_THREADS -DNOVA_GC_BOEHM -lgc -lpthread -o gc_helpers_test
 *
 * Purpose: verify that Boehm actually collects unreachable objects.
 * Cannot be a Nova test: external fn is restricted to std.runtime.*
 * modules, and we need GC_gcollect() / GC_get_heap_size() which have
 * no Nova binding.
 *
 * Results from Ф.3 should be recorded in spec/overview.md (GC section). */

#ifdef NOVA_GC_BOEHM

#define GC_THREADS
#include <gc.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

/* Cross-platform monotonic clock in microseconds. */
#ifdef _WIN32
#  include <windows.h>
static int64_t now_us(void) {
    LARGE_INTEGER freq, cnt;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&cnt);
    return (int64_t)(cnt.QuadPart * 1000000 / freq.QuadPart);
}
#else
#  include <time.h>
static int64_t now_us(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (int64_t)t.tv_sec * 1000000 + t.tv_nsec / 1000;
}
#endif

#define ASSERT(cond, msg) \
    do { \
        if (!(cond)) { \
            fprintf(stderr, "FAIL: %s\n", (msg)); \
            return 1; \
        } \
    } while (0)

/* test_gc_collects: verify heap does not grow linearly after GC_gcollect.
 *
 * Allocates 1M × 64-byte objects (all immediately unreachable).
 * After two GC_gcollect() calls, heap must be < initial + 10 MB.
 * If Boehm does not collect, heap ≈ 64 MB (1M × 64B) → test fails. */
static int test_gc_collects(void) {
    GC_INIT();
    GC_set_all_interior_pointers(1);

    size_t heap_before = GC_get_heap_size();

    for (int i = 0; i < 1000000; i++) {
        char* p = GC_malloc(64);
        if (!p) {
            fprintf(stderr, "OOM at alloc %d\n", i);
            return 1;
        }
        /* Intentionally drop pointer — object becomes unreachable. */
        (void)p;
    }

    GC_gcollect();
    GC_gcollect(); /* second pass catches objects freed by finalizers in first */

    size_t heap_after = GC_get_heap_size();

    printf("heap before: %zu KB, after 1M alloc + 2×collect: %zu KB\n",
           heap_before / 1024, heap_after / 1024);

    /* Conservative bound: 10 MB above initial heap. Without collection,
     * heap would be ~64 MB (1M × 64 bytes). */
    size_t limit = heap_before + 10 * 1024 * 1024;
    ASSERT(heap_after < limit,
           "heap grew > 10 MB after GC_gcollect — GC not collecting unreachable objects");

    printf("PASS test_gc_collects: heap_after=%zu KB < limit=%zu KB\n",
           heap_after / 1024, limit / 1024);
    return 0;
}

/* test_gc_pause: benchmark stop-the-world pause for different heap sizes.
 *
 * Results should be recorded in spec/overview.md (GC section) after Ф.3.
 * Target: < 1 ms pause for 10k allocs, < 10 ms for 100k. */
static int test_gc_pause(void) {
    GC_INIT();
    GC_set_all_interior_pointers(1);

    static const struct { int n; const char* label; } cases[] = {
        {   10000, "10k allocs" },
        {  100000, "100k allocs"},
        { 1000000, "1M allocs"  },
    };
    static const int NCASES = 3;

    for (int c = 0; c < NCASES; c++) {
        for (int i = 0; i < cases[c].n; i++) {
            char* p = GC_malloc(64);
            if (!p) {
                fprintf(stderr, "OOM at case %d alloc %d\n", c, i);
                return 1;
            }
            (void)p;
        }

        int64_t t0 = now_us();
        GC_gcollect();
        int64_t t1 = now_us();
        long elapsed_us = (long)(t1 - t0);

        printf("pause[%s]: %ld us\n", cases[c].label, elapsed_us);
    }

    return 0;
}

int main(void) {
    printf("=== gc_test_helpers ===\n");
    if (test_gc_collects() != 0) return 1;
    printf("\n");
    if (test_gc_pause() != 0) return 1;
    printf("\nALL PASS\n");
    return 0;
}

#else /* !NOVA_GC_BOEHM */

#include <stdio.h>
int main(void) {
    printf("SKIP: not built with -DNOVA_GC_BOEHM\n");
    return 0;
}

#endif /* NOVA_GC_BOEHM */
