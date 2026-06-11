// SPDX-License-Identifier: MIT OR Apache-2.0
/* Standalone unit test for runq.h (Plan 83-go-cmn Ф.1 fixed ring).
 *
 * Validates the lock-free per-worker ring + global overflow in isolation,
 * WITHOUT the full Nova build, so the load-bearing data structure is proven
 * before it is wired into runtime.c.
 *
 * Build (Windows clang):
 *   clang -O2 -g -o test_runq.exe test_runq.c
 * Run:
 *   ./test_runq.exe
 *
 * Tests:
 *   T1  single-thread FIFO put/get
 *   T2  fill-to-CAP then one more → put_slow spills HALF to global; counts
 *   T3  global_get drains spilled fibers in FIFO order
 *   T4  concurrent: 1 producer (put + occasional get) + K thieves (grab) +
 *       global drain → conservation (every fiber consumed exactly once)
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>

/* Provide a concrete mco_coro BEFORE including runq.h (which forward-declares
 * it). We carry an id + the intrusive schedlink the global queue needs. */
struct mco_coro { int id; struct mco_coro* schedlink; };
typedef struct mco_coro mco_coro;
#define NOVA_MCO_CORO_DEFINED 1

#include "runq.h"

/* schedlink accessor required by runq.h's global overflow queue. */
mco_coro** nova_co_schedlink(mco_coro* co) { return &co->schedlink; }

/* the single diag instance runq.h's counters reference. */
NovaRunqDiag nova_runq_diag;

static int failures = 0;
#define CHECK(cond, msg) do { if (!(cond)) { printf("  FAIL: %s\n", msg); failures++; } } while (0)

/* ── T1: single-thread FIFO ─────────────────────────────────────────── */
static void t1_fifo(void) {
    printf("T1 single-thread FIFO put/get\n");
    NovaRunq q; NovaGlobalRunq g; nova_runq_init(&q); nova_globrunq_init(&g);
    static mco_coro fibers[10];
    for (int i = 0; i < 10; i++) { fibers[i].id = i; nova_runq_put(&q, &g, &fibers[i]); }
    CHECK(nova_runq_len(&q) == 10, "len==10 after 10 puts");
    for (int i = 0; i < 10; i++) {
        mco_coro* c = nova_runq_get(&q);
        CHECK(c && c->id == i, "FIFO order");
    }
    CHECK(nova_runq_get(&q) == NULL, "empty after drain");
}

/* ── T2/T3: overflow spill + global drain ───────────────────────────── */
static void t2_overflow(void) {
    printf("T2/T3 overflow spill-half + global drain\n");
    NovaRunq q; NovaGlobalRunq g; nova_runq_init(&q); nova_globrunq_init(&g);
    int N = (int)NOVA_RUNQ_CAP + 1;            /* force one overflow */
    mco_coro* fibers = (mco_coro*)calloc((size_t)N, sizeof(mco_coro));
    for (int i = 0; i < N; i++) { fibers[i].id = i; nova_runq_put(&q, &g, &fibers[i]); }
    /* After CAP fit, the (CAP+1)-th triggers put_slow: claims CAP/2 + the new
     * fiber → CAP/2+1 go to global; ring keeps CAP/2. */
    CHECK(g.size == (int)(NOVA_RUNQ_CAP / 2 + 1), "global got CAP/2+1");
    CHECK(nova_runq_len(&q) == NOVA_RUNQ_CAP / 2, "ring kept CAP/2");
    CHECK(nova_runq_diag.overflow_spills == 1, "exactly one spill");

    /* T3: drain everything, assert every id seen exactly once. */
    char* seen = (char*)calloc((size_t)N, 1);
    int total = 0; mco_coro* c;
    while ((c = nova_runq_get(&q)) != NULL) { seen[c->id]++; total++; }
    while ((c = nova_globrunq_get_one(&g)) != NULL) { seen[c->id]++; total++; }
    CHECK(total == N, "drained all N");
    int dup = 0, lost = 0;
    for (int i = 0; i < N; i++) { if (seen[i] > 1) dup++; if (seen[i] == 0) lost++; }
    CHECK(dup == 0, "no duplicates");
    CHECK(lost == 0, "no lost fibers");
    free(seen); free(fibers);
}

/* ── T4: concurrent producer + thieves → conservation ───────────────── */
#define T4_N      200000
#define T4_THIEVES 4

static NovaRunq      t4_q;
static NovaGlobalRunq t4_g;
static mco_coro*     t4_fibers;
static volatile int  t4_consumed[T4_N];   /* per-fiber consume count (atomic) */
static volatile int  t4_done_producing = 0;
static volatile long t4_total_consumed = 0;

static void consume_one(mco_coro* c) {
    __atomic_fetch_add(&t4_consumed[c->id], 1, __ATOMIC_RELAXED);
    __atomic_fetch_add(&t4_total_consumed, 1, __ATOMIC_RELAXED);
}

static DWORD WINAPI t4_thief(LPVOID arg) {
    (void)arg;
    mco_coro* batch[NOVA_RUNQ_CAP / 2];
    for (;;) {
        uint32_t n = nova_runq_grab(&t4_q, batch, NOVA_RUNQ_CAP / 2);
        if (n == 0) {
            /* also help drain the global overflow queue */
            mco_coro* c = nova_globrunq_get_one(&t4_g);
            if (c) { consume_one(c); continue; }
            if (__atomic_load_n(&t4_done_producing, __ATOMIC_ACQUIRE) &&
                __atomic_load_n(&t4_total_consumed, __ATOMIC_RELAXED) >= T4_N)
                return 0;
            continue;
        }
        for (uint32_t i = 0; i < n; i++) consume_one(batch[i]);
    }
}

static void t4_concurrent(void) {
    printf("T4 concurrent producer + %d thieves, N=%d (conservation)\n",
           T4_THIEVES, T4_N);
    nova_runq_init(&t4_q); nova_globrunq_init(&t4_g);
    t4_fibers = (mco_coro*)calloc((size_t)T4_N, sizeof(mco_coro));
    for (int i = 0; i < T4_N; i++) t4_fibers[i].id = i;
    memset((void*)t4_consumed, 0, sizeof(t4_consumed));
    t4_done_producing = 0; t4_total_consumed = 0;

    HANDLE th[T4_THIEVES];
    for (int i = 0; i < T4_THIEVES; i++) th[i] = CreateThread(NULL, 0, t4_thief, NULL, 0, NULL);

    /* Producer (this thread = ring owner): put all, occasionally get one. */
    for (int i = 0; i < T4_N; i++) {
        nova_runq_put(&t4_q, &t4_g, &t4_fibers[i]);
        if ((i & 7) == 0) { mco_coro* c = nova_runq_get(&t4_q); if (c) consume_one(c); }
    }
    /* Owner drains its remaining ring + helps with global. */
    mco_coro* c;
    while ((c = nova_runq_get(&t4_q)) != NULL) consume_one(c);
    while ((c = nova_globrunq_get_one(&t4_g)) != NULL) consume_one(c);
    __atomic_store_n(&t4_done_producing, 1, __ATOMIC_RELEASE);

    for (int i = 0; i < T4_THIEVES; i++) { WaitForSingleObject(th[i], INFINITE); CloseHandle(th[i]); }

    /* Final drain in case anything landed after the last owner pass. */
    while ((c = nova_runq_get(&t4_q)) != NULL) consume_one(c);
    while ((c = nova_globrunq_get_one(&t4_g)) != NULL) consume_one(c);

    long total = __atomic_load_n(&t4_total_consumed, __ATOMIC_RELAXED);
    CHECK(total == T4_N, "total consumed == N");
    int dup = 0, lost = 0;
    for (int i = 0; i < T4_N; i++) {
        int cnt = __atomic_load_n(&t4_consumed[i], __ATOMIC_RELAXED);
        if (cnt > 1) dup++; if (cnt == 0) lost++;
    }
    CHECK(dup == 0, "no fiber consumed twice");
    CHECK(lost == 0, "no fiber lost");
    CHECK(nova_runq_empty(&t4_q), "ring empty at end");
    CHECK(t4_g.size == 0, "global empty at end");
    printf("  (spills=%llu grabs=%llu retries=%llu gputs=%llu gpulls=%llu)\n",
           (unsigned long long)nova_runq_diag.overflow_spills,
           (unsigned long long)nova_runq_diag.grab_batches,
           (unsigned long long)nova_runq_diag.grab_retries,
           (unsigned long long)nova_runq_diag.global_puts,
           (unsigned long long)nova_runq_diag.global_pulls);
    free(t4_fibers);
}

/* ── T5: runq_steal moves half victim→self + returns one ────────────── */
static void t5_steal(void) {
    printf("T5 runq_steal half victim->self + conservation\n");
    NovaRunq victim, self; NovaGlobalRunq g;
    nova_runq_init(&victim); nova_runq_init(&self); nova_globrunq_init(&g);
    int K = 40;
    static mco_coro fibers[40];
    for (int i = 0; i < K; i++) { fibers[i].id = i; nova_runq_put(&victim, &g, &fibers[i]); }
    mco_coro* got = nova_runq_steal(&self, &victim);
    CHECK(got != NULL, "steal returned a fiber");
    /* steal-half of 40 = 20: returns 1, puts 19 into self, leaves 20 in victim */
    CHECK(nova_runq_len(&victim) == 20, "victim left with 20");
    CHECK(nova_runq_len(&self) == 19, "self got 19");
    /* drain all three sources, assert each id once */
    char seen[40]; memset(seen, 0, sizeof(seen));
    int total = 0; mco_coro* c;
    seen[got->id]++; total++;
    while ((c = nova_runq_get(&self)) != NULL)   { seen[c->id]++; total++; }
    while ((c = nova_runq_get(&victim)) != NULL) { seen[c->id]++; total++; }
    CHECK(total == K, "all K accounted for");
    int dup = 0, lost = 0;
    for (int i = 0; i < K; i++) { if (seen[i] > 1) dup++; if (seen[i] == 0) lost++; }
    CHECK(dup == 0 && lost == 0, "no dup, no loss after steal");
}

int main(void) {
    printf("=== runq.h unit tests (CAP=%u) ===\n", NOVA_RUNQ_CAP);
    t1_fifo();
    t2_overflow();
    t5_steal();
    t4_concurrent();
    if (failures == 0) { printf("\nALL PASS\n"); return 0; }
    printf("\n%d CHECK(s) FAILED\n", failures);
    return 1;
}
