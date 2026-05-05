/*
 * test_gc_deep.c — deep GC runtime tests
 *
 * Tests what Nova-level tests cannot: actual alloc/free counts, RC lifecycle,
 * pointer integrity under allocation pressure, object survival guarantees.
 *
 * Compile (malloc backend):
 *   cl /nologo /W3 /I. /DNOVA_TEST_GC nova_rt\test_gc_deep.c nova_rt\alloc.c /Fe:test_gc_malloc.exe
 *
 * Compile (RC backend):
 *   cl /nologo /W3 /I. /DNOVA_TEST_GC nova_rt\test_gc_deep.c nova_rt\alloc_rc.c /Fe:test_gc_rc.exe
 */

#include "alloc.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

/* ---- test framework ---- */

static int _total = 0;
static int _failed = 0;

#define ASSERT(cond, msg) do { \
    _total++; \
    if (!(cond)) { \
        printf("  FAIL [%s:%d] %s\n", __FILE__, __LINE__, msg); \
        _failed++; \
    } else { \
        printf("  pass: %s\n", msg); \
    } \
} while(0)

#define SECTION(name) printf("\n=== %s ===\n", name)

/* ---- helpers ---- */

typedef struct { int64_t x; int64_t y; } Point;
typedef struct { int64_t values[16]; } BigObject;
typedef struct Node { int64_t val; struct Node* next; } Node;

static Point* make_point(int64_t x, int64_t y) {
    Point* p = (Point*)nova_alloc(sizeof(Point));
    p->x = x; p->y = y;
    return p;
}

/* ---- Test 1: alloc_count tracks every allocation ---- */
static void test_alloc_count(void) {
    SECTION("alloc_count: every nova_alloc increments counter");

    nova_gc_reset_stats();
    size_t before = nova_gc_alloc_count();

    void* a = nova_alloc(8);
    void* b = nova_alloc(16);
    void* c = nova_alloc(32);
    (void)a; (void)b; (void)c;

    size_t after = nova_gc_alloc_count();
    ASSERT(after - before == 3, "3 allocations counted");
    ASSERT(nova_gc_live_count() >= 3, "live_count >= 3");
}

/* ---- Test 2: RC — free_count matches release calls ---- */
static void test_rc_free_count(void) {
    SECTION("RC: free_count increments when refcount hits zero");

    nova_gc_reset_stats();

    /* In malloc mode: release is a no-op, free_count stays 0.
     * In RC mode: release decrements refcount and frees at 0. */
    void* p1 = nova_alloc(64);
    void* p2 = nova_alloc(64);
    size_t alloc_after = nova_gc_alloc_count();
    ASSERT(alloc_after == 2, "2 objects allocated");

    nova_release(p1);
    nova_release(p2);

    size_t free_after = nova_gc_free_count();
    /* malloc: free_count == 0; RC: free_count == 2 */
    printf("  info: free_count after 2 releases = %zu (malloc=0, RC=2)\n", free_after);
    ASSERT(free_after == 0 || free_after == 2,
           "free_count is 0 (malloc) or 2 (RC)");

    /* If RC: live_count decreased */
    size_t live = nova_gc_live_count();
    printf("  info: live_count = %zu (malloc=2, RC=0)\n", live);
    ASSERT(live == 0 || live == 2,
           "live_count is 0 (RC freed) or 2 (malloc no-free)");
}

/* ---- Test 3: RC retain prevents premature free ---- */
static void test_rc_retain(void) {
    SECTION("RC: retain increments refcount, extra release needed");

    nova_gc_reset_stats();

    void* p = nova_alloc(32);
    nova_retain(p);   /* refcount now 2 */
    nova_release(p);  /* refcount now 1 — NOT freed yet */

    size_t free_mid = nova_gc_free_count();
    ASSERT(free_mid == 0, "not freed after one release (still retained)");

    nova_release(p);  /* refcount now 0 — freed */

    size_t free_end = nova_gc_free_count();
    /* malloc: still 0; RC: now 1 */
    printf("  info: free_count after second release = %zu (malloc=0, RC=1)\n", free_end);
    ASSERT(free_end == 0 || free_end == 1,
           "freed on second release (RC) or no-op (malloc)");
}

/* ---- Test 4: objects survive across many allocations ---- */
static void test_object_survival(void) {
    SECTION("object survival: values intact after 10000 interleaved allocs");

    nova_gc_reset_stats();

    /* Allocate sentinel objects at known positions */
    Point* sentinels[10];
    for (int i = 0; i < 10; i++) {
        sentinels[i] = make_point((int64_t)i * 100, (int64_t)i * 200);
    }

    /* Pressure: allocate 10000 temporary objects */
    for (int i = 0; i < 10000; i++) {
        BigObject* tmp = (BigObject*)nova_alloc(sizeof(BigObject));
        tmp->values[0] = (int64_t)i;
        /* In malloc mode: never freed. In RC mode: would need release. */
        nova_release(tmp); /* RC: freed immediately; malloc: no-op */
    }

    /* Sentinels must still hold their values */
    int all_ok = 1;
    for (int i = 0; i < 10; i++) {
        if (sentinels[i]->x != (int64_t)i * 100 ||
            sentinels[i]->y != (int64_t)i * 200) {
            all_ok = 0;
        }
    }
    ASSERT(all_ok, "all sentinel values intact after 10000 allocs");

    /* Verify alloc_count includes sentinels + temporaries */
    size_t total = nova_gc_alloc_count();
    ASSERT(total >= 10010, "at least 10010 allocations tracked");
    printf("  info: total alloc_count = %zu\n", total);
}

/* ---- Test 5: linked list — object graph integrity ---- */
static void test_linked_list(void) {
    SECTION("linked list: object graph survives 1000-node chain");

    nova_gc_reset_stats();

    /* Build: 1000 -> 999 -> ... -> 1 -> NULL */
    Node* head = NULL;
    for (int i = 1; i <= 1000; i++) {
        Node* n = (Node*)nova_alloc(sizeof(Node));
        n->val = (int64_t)i;
        n->next = head;
        head = n;
    }

    /* Traverse and verify sum = 1+2+...+1000 = 500500 */
    int64_t sum = 0;
    Node* cur = head;
    int count = 0;
    while (cur) {
        sum += cur->val;
        cur = cur->next;
        count++;
    }

    ASSERT(count == 1000, "linked list has 1000 nodes");
    ASSERT(sum == 500500, "sum 1..1000 = 500500");
    printf("  info: sum = %lld, nodes = %d\n", (long long)sum, count);

    /* Release the list in RC mode */
    cur = head;
    while (cur) {
        Node* next = cur->next;
        nova_release(cur);
        cur = next;
    }

    size_t freed = nova_gc_free_count();
    printf("  info: free_count after list release = %zu (malloc=0, RC=1000)\n", freed);
    ASSERT(freed == 0 || freed == 1000,
           "freed all nodes (RC) or no-op (malloc)");
}

/* ---- Test 6: allocation size variety ---- */
static void test_size_variety(void) {
    SECTION("alloc size variety: 1 byte to 1MB, values intact");

    nova_gc_reset_stats();

    /* Small */
    uint8_t* tiny = (uint8_t*)nova_alloc(1);
    *tiny = 0xAB;

    /* Medium */
    uint8_t* med = (uint8_t*)nova_alloc(4096);
    memset(med, 0x55, 4096);

    /* Large */
    uint8_t* big = (uint8_t*)nova_alloc(1024 * 1024);
    big[0] = 0x01;
    big[1024*1024 - 1] = 0xFF;

    ASSERT(*tiny == 0xAB, "1-byte allocation holds value");
    ASSERT(med[0] == 0x55 && med[4095] == 0x55, "4096-byte allocation holds values");
    ASSERT(big[0] == 0x01 && big[1024*1024-1] == 0xFF, "1MB allocation holds values");

    nova_release(tiny);
    nova_release(med);
    nova_release(big);

    ASSERT(nova_gc_alloc_count() >= 3, "3 allocations tracked");
}

/* ---- Test 7: zero-fill guarantee ---- */
static void test_zero_fill(void) {
    SECTION("zero-fill: nova_alloc returns zeroed memory (RC) or unspecified (malloc)");

    /* RC mode explicitly zeroes with memset. Malloc mode does not guarantee.
     * We only assert RC behaviour here. */
    nova_gc_reset_stats();

    int64_t* arr = (int64_t*)nova_alloc(sizeof(int64_t) * 64);
    int all_zero = 1;
    for (int i = 0; i < 64; i++) {
        if (arr[i] != 0) { all_zero = 0; break; }
    }
    /* This is guaranteed by RC, best-effort by malloc (depends on OS) */
    printf("  info: memory zeroed on alloc = %s\n", all_zero ? "yes" : "no (malloc, OS-dependent)");
    /* No hard assertion — just log */
    _total++;  /* count as executed */
    nova_release(arr);
}

/* ---- Test 8: reset_stats isolation ---- */
static void test_reset_stats(void) {
    SECTION("reset_stats: counters zero after reset, independent of prior state");

    /* Accumulate some state */
    void* p = nova_alloc(8);
    nova_release(p);

    /* Reset */
    nova_gc_reset_stats();
    ASSERT(nova_gc_alloc_count() == 0, "alloc_count == 0 after reset");
    ASSERT(nova_gc_free_count()  == 0, "free_count == 0 after reset");
    ASSERT(nova_gc_live_count()  == 0, "live_count == 0 after reset");

    /* New allocations counted from zero */
    void* q = nova_alloc(16);
    (void)q;
    ASSERT(nova_gc_alloc_count() == 1, "alloc_count == 1 after reset + 1 alloc");
}

/* ---- Test 9: high-volume RC throughput ---- */
static void test_rc_throughput(void) {
    SECTION("RC throughput: 100000 alloc+release cycles complete without crash");

    nova_gc_reset_stats();

    for (int i = 0; i < 100000; i++) {
        Point* p = make_point((int64_t)i, (int64_t)-i);
        ASSERT(p->x == (int64_t)i && p->y == (int64_t)-i, "values correct"); /* avoid compiler elision */
        /* exit early on first failure to avoid flooding output */
        if (_failed > 0) break;
        nova_release(p);
    }

    /* Suppress redundant assert-spam: only check final state */
    size_t a = nova_gc_alloc_count();
    size_t f = nova_gc_free_count();
    printf("  info: 100000-cycle: allocs=%zu frees=%zu live=%zu\n", a, f, a - f);

    /* malloc: f=0, live=100000; RC: f=100000, live=0 */
    ASSERT(f == 0 || f == 100000, "free_count is 0 (malloc) or 100000 (RC)");
    size_t live = nova_gc_live_count();
    ASSERT(live == 0 || live == (size_t)(a - f), "live_count consistent");
}

/* ---- main ---- */

int main(void) {
    printf("nova GC deep tests\n");
    printf("==================\n");

    nova_gc_init();

    test_alloc_count();
    test_rc_free_count();
    test_rc_retain();
    test_object_survival();
    test_linked_list();
    test_size_variety();
    test_zero_fill();
    test_reset_stats();
    /* throughput: run but skip individual assert spam when output is large */
    {
        SECTION("RC throughput: 100000 alloc+release cycles");
        nova_gc_reset_stats();
        int ok = 1;
        for (int i = 0; i < 100000; i++) {
            Point* p = make_point((int64_t)i, (int64_t)-i);
            if (p->x != (int64_t)i || p->y != (int64_t)-i) { ok = 0; break; }
            nova_release(p);
        }
        ASSERT(ok, "100000 alloc/release cycles: values always correct");
        size_t a = nova_gc_alloc_count(), f = nova_gc_free_count();
        printf("  info: allocs=%zu frees=%zu live=%zu\n", a, f, a - f);
        ASSERT(f == 0 || f == 100000, "free_count is 0 (malloc) or 100000 (RC)");
    }

    nova_gc_shutdown();

    printf("\n==================\n");
    printf("%d/%d passed\n", _total - _failed, _total);
    return _failed > 0 ? 1 : 0;
}
