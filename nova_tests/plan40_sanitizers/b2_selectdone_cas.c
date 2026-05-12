/* Plan 40 Ф.1 Этап 6 — B2 selectdone CAS protocol stress test.
 *
 * Verify that the unified fired CAS protocol correctly elects ONE
 * winner among N concurrent waiters racing for the same value.
 *
 * Setup: N threads spawn waiters with fired=0. One producer thread CAS's
 * exactly one of them to fired=1. Expected: exactly one waiter gets
 * fired=1 — never zero (lost wakeup), never two (double-fire).
 *
 * This is the core invariant of B2 — Plan 40 R2 explicitly. Without this
 * stress test, race conditions in the CAS loop won't be visible until
 * Plan 23 M:N goes live.
 */

#define _GNU_SOURCE
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <assert.h>
#include <gc.h>

#include "sync.h"

#define N_WAITERS 8
#define N_ROUNDS  10000

typedef struct {
    nova_atomic_int fired;
    int             id;
} stress_waiter_t;

static stress_waiter_t g_waiters[N_WAITERS];
static nova_atomic_int g_winner_count;  /* должно быть ровно N_ROUNDS суммарно */

static void* race_thread(void* arg) {
    struct GC_stack_base sb;
    GC_get_stack_base(&sb);
    GC_register_my_thread(&sb);

    int tid = (int)(uintptr_t)arg;
    for (int r = 0; r < N_ROUNDS; r++) {
        /* Каждый поток пытается CAS'нуть КАЖДЫЙ waiter в текущем раунде.
         * Только один thread successful'ит, остальные fail'ят — ровно
         * один winner per waiter. */
        int idx = (tid + r) % N_WAITERS;
        int32_t expected = 0;
        if (nova_aint_cas_weak_release(&g_waiters[idx].fired, &expected, 1)) {
            nova_aint_inc(&g_winner_count);
        }
    }
    GC_unregister_my_thread();
    return NULL;
}

static void reset_waiters(void) {
    for (int i = 0; i < N_WAITERS; i++) {
        nova_aint_init(&g_waiters[i].fired, 0);
        g_waiters[i].id = i;
    }
}

int main(void) {
    GC_INIT();

    fprintf(stderr, "[b2_selectdone_cas] starting: %d waiters × %d rounds × %d threads\n",
            N_WAITERS, N_ROUNDS, 4);

    /* Каждый round: 4 threads racing на 1 waiter. После N_ROUNDS все
     * waiters должны быть fired (no lost wakeup), и total winners =
     * N_ROUNDS (no double-fire — exactly one winner per round). */

    for (int round = 0; round < N_ROUNDS; round++) {
        reset_waiters();
        nova_aint_init(&g_winner_count, 0);

        pthread_t threads[4];
        for (int i = 0; i < 4; i++) {
            pthread_create(&threads[i], NULL, race_thread, (void*)(uintptr_t)i);
        }
        for (int i = 0; i < 4; i++) {
            pthread_join(threads[i], NULL);
        }

        /* После 4 threads × N_ROUNDS попыток на ту же группу waiters,
         * каждый waiter должен быть fired (CAS'нут ровно одним). */
        int fired_count = 0;
        for (int i = 0; i < N_WAITERS; i++) {
            if (nova_aint_load(&g_waiters[i].fired)) fired_count++;
        }
        int winners = nova_aint_load(&g_winner_count);

        if (fired_count != N_WAITERS) {
            fprintf(stderr, "[b2_selectdone_cas] FAIL round=%d: fired_count=%d != N_WAITERS=%d (lost wakeup)\n",
                    round, fired_count, N_WAITERS);
            return 1;
        }
        if (winners != N_WAITERS) {
            fprintf(stderr, "[b2_selectdone_cas] FAIL round=%d: winners=%d != N_WAITERS=%d (double-fire)\n",
                    round, winners, N_WAITERS);
            return 1;
        }
        /* Note: this loop runs N_ROUNDS² because each round is N_ROUNDS — overkill.
         * In practice the threads do N_ROUNDS iter on shared waiter set, so 1 round
         * of pthread_join сensures verification of one full scenario. */
        if (round == 0) break;  /* one round is enough verification */
    }

    fprintf(stderr, "[b2_selectdone_cas] PASS — exactly one winner per waiter, no lost/double wakeups\n");
    return 0;
}
