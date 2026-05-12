/* Plan 40 Ф.1 Этап 6 — B1 atomics+mutex stress test.
 *
 * Bypass fiber scheduler — direct pthread test of channel runtime layer.
 * Goal: verify that mutex+atomic discipline correctly serializes
 * concurrent send/recv on a single channel from multiple threads.
 *
 * Expected: 8 threads × 100k iterations = 800k operations, all values
 * accounted for (sum check). No data race under TSan. No use-after-free
 * under ASan.
 *
 * Plan 40 R3-1: each pthread MUST GC_register_my_thread() before nova_alloc.
 *
 * Build (inside Linux Docker):
 *   clang -O2 -g -pthread -I/nova/compiler-codegen/nova_rt \
 *     -fsanitize=thread (or address/undefined) \
 *     nova_tests/plan40_sanitizers/b1_mutex_stress.c \
 *     compiler-codegen/nova_rt/alloc_boehm.c \
 *     compiler-codegen/nova_rt/effects.c \
 *     -lgc -lpthread -o b1_mutex_stress
 */

#define _GNU_SOURCE
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <gc.h>

/* Channel runtime требует fiber context, который мы здесь обходим.
 * Stress test работает с raw mutex/atomic primitives через sync.h.
 *
 * Реальный channel send/recv требует scheduler — для bootstrap нашей
 * single-thread runtime'а мы тестируем именно sync.h-level primitives. */

#include "sync.h"

/* Plan 40 minimal MPSC queue: верифицирует mutex + atomic дискиплину
 * без зависимости от fiber scheduler. */
#define QUEUE_CAP 1024

typedef struct {
    nova_mutex_t     mu;
    int64_t          buf[QUEUE_CAP];
    int64_t          head;
    int64_t          count;
    nova_atomic_bool closed;
    nova_atomic_int  total_sent;
    nova_atomic_int  total_received;
} stress_queue_t;

static stress_queue_t g_q;

static void q_init(stress_queue_t* q) {
    nova_mutex_init(&q->mu);
    q->head = 0;
    q->count = 0;
    nova_abool_init(&q->closed, false);
    nova_aint_init(&q->total_sent, 0);
    nova_aint_init(&q->total_received, 0);
}

/* Returns 1 if pushed, 0 if full. */
static int q_try_send(stress_queue_t* q, int64_t v) {
    nova_mutex_lock(&q->mu);
    if (q->count >= QUEUE_CAP) {
        nova_mutex_unlock(&q->mu);
        return 0;
    }
    int64_t tail = (q->head + q->count) % QUEUE_CAP;
    q->buf[tail] = v;
    q->count++;
    nova_mutex_unlock(&q->mu);
    return 1;
}

/* Returns 1 + value if got, 0 if empty. */
static int q_try_recv(stress_queue_t* q, int64_t* out) {
    nova_mutex_lock(&q->mu);
    if (q->count == 0) {
        nova_mutex_unlock(&q->mu);
        return 0;
    }
    *out = q->buf[q->head];
    q->head = (q->head + 1) % QUEUE_CAP;
    q->count--;
    nova_mutex_unlock(&q->mu);
    return 1;
}

#define N_PRODUCERS 4
#define N_CONSUMERS 4
#define OPS_PER_THREAD 100000

static void* producer_thread(void* arg) {
    /* Plan 40 R3-1: register thread with Boehm GC. */
    struct GC_stack_base sb;
    GC_get_stack_base(&sb);
    GC_register_my_thread(&sb);

    int tid = (int)(uintptr_t)arg;
    int64_t expected_sum = 0;
    for (int i = 0; i < OPS_PER_THREAD; i++) {
        int64_t v = (int64_t)tid * 1000000 + i;
        while (!q_try_send(&g_q, v)) {
            /* busy-retry на full queue */
            sched_yield();
        }
        expected_sum += v;
        nova_aint_inc(&g_q.total_sent);
    }
    GC_unregister_my_thread();
    return (void*)(uintptr_t)expected_sum;
}

static void* consumer_thread(void* arg) {
    struct GC_stack_base sb;
    GC_get_stack_base(&sb);
    GC_register_my_thread(&sb);

    (void)arg;
    int64_t actual_sum = 0;
    int got = 0;
    int target = N_PRODUCERS * OPS_PER_THREAD / N_CONSUMERS;
    while (got < target) {
        int64_t v;
        if (q_try_recv(&g_q, &v)) {
            actual_sum += v;
            nova_aint_inc(&g_q.total_received);
            got++;
        } else {
            sched_yield();
        }
    }
    GC_unregister_my_thread();
    return (void*)(uintptr_t)actual_sum;
}

int main(void) {
    GC_INIT();
    q_init(&g_q);

    pthread_t producers[N_PRODUCERS];
    pthread_t consumers[N_CONSUMERS];

    fprintf(stderr, "[b1_mutex_stress] starting: %d producers × %d ops, %d consumers\n",
            N_PRODUCERS, OPS_PER_THREAD, N_CONSUMERS);

    for (int i = 0; i < N_PRODUCERS; i++) {
        pthread_create(&producers[i], NULL, producer_thread, (void*)(uintptr_t)i);
    }
    for (int i = 0; i < N_CONSUMERS; i++) {
        pthread_create(&consumers[i], NULL, consumer_thread, NULL);
    }

    int64_t expected_total = 0;
    for (int i = 0; i < N_PRODUCERS; i++) {
        void* r;
        pthread_join(producers[i], &r);
        expected_total += (int64_t)(uintptr_t)r;
    }
    int64_t actual_total = 0;
    for (int i = 0; i < N_CONSUMERS; i++) {
        void* r;
        pthread_join(consumers[i], &r);
        actual_total += (int64_t)(uintptr_t)r;
    }

    int sent = nova_aint_load(&g_q.total_sent);
    int recv = nova_aint_load(&g_q.total_received);

    fprintf(stderr, "[b1_mutex_stress] sent=%d, received=%d, expected_total=%lld, actual_total=%lld\n",
            sent, recv, (long long)expected_total, (long long)actual_total);

    if (sent != N_PRODUCERS * OPS_PER_THREAD) {
        fprintf(stderr, "[b1_mutex_stress] FAIL: sent counter mismatch\n");
        return 1;
    }
    if (recv != N_PRODUCERS * OPS_PER_THREAD) {
        fprintf(stderr, "[b1_mutex_stress] FAIL: received counter mismatch\n");
        return 1;
    }
    if (expected_total != actual_total) {
        fprintf(stderr, "[b1_mutex_stress] FAIL: sum mismatch — lost or duplicated values\n");
        return 1;
    }
    fprintf(stderr, "[b1_mutex_stress] PASS\n");
    return 0;
}
