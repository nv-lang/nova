/* Plan 40 Ф.1 Этап 6 — T2 doubly-linked waiter list churn stress test.
 *
 * High-frequency enqueue/unlink на doubly-linked list под concurrent
 * access. Verifies prev/next pointer integrity и absence of use-after-
 * free (ASan), data races (TSan).
 *
 * Setup: N threads, each repeatedly insert+unlink waiter в shared list.
 * Expected: list integrity preserved (no corruption, no leak, no UAF).
 */

#define _GNU_SOURCE
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <gc.h>

#include "sync.h"

#define N_THREADS 4
#define OPS_PER_THREAD 10000

/* Simplified BaseWaiter-like struct для теста. */
typedef struct test_waiter {
    int                 id;
    struct test_waiter* next;
    struct test_waiter* prev;
} test_waiter_t;

typedef struct {
    nova_mutex_t   mu;
    test_waiter_t* head;
    nova_atomic_int total_inserts;
    nova_atomic_int total_unlinks;
} test_list_t;

static test_list_t g_list;

static void list_init(test_list_t* l) {
    nova_mutex_init(&l->mu);
    l->head = NULL;
    nova_aint_init(&l->total_inserts, 0);
    nova_aint_init(&l->total_unlinks, 0);
}

static void list_insert(test_list_t* l, test_waiter_t* w) {
    nova_mutex_lock(&l->mu);
    w->prev = NULL;
    w->next = l->head;
    if (l->head) l->head->prev = w;
    l->head = w;
    nova_mutex_unlock(&l->mu);
    nova_aint_inc(&l->total_inserts);
}

static void list_unlink(test_list_t* l, test_waiter_t* w) {
    nova_mutex_lock(&l->mu);
    if (w->prev) {
        w->prev->next = w->next;
    } else if (l->head == w) {
        l->head = w->next;
    }
    if (w->next) {
        w->next->prev = w->prev;
    }
    w->next = NULL;
    w->prev = NULL;
    nova_mutex_unlock(&l->mu);
    nova_aint_inc(&l->total_unlinks);
}

static void* churn_thread(void* arg) {
    struct GC_stack_base sb;
    GC_get_stack_base(&sb);
    GC_register_my_thread(&sb);

    int tid = (int)(uintptr_t)arg;
    for (int i = 0; i < OPS_PER_THREAD; i++) {
        test_waiter_t* w = (test_waiter_t*)GC_MALLOC(sizeof(test_waiter_t));
        w->id = tid * 1000000 + i;
        list_insert(&g_list, w);
        /* immediately unlink to stress concurrent insert/unlink. */
        list_unlink(&g_list, w);
    }
    GC_unregister_my_thread();
    return NULL;
}

int main(void) {
    GC_INIT();
    list_init(&g_list);

    fprintf(stderr, "[t2_waiter_churn] starting: %d threads × %d ops\n",
            N_THREADS, OPS_PER_THREAD);

    pthread_t threads[N_THREADS];
    for (int i = 0; i < N_THREADS; i++) {
        pthread_create(&threads[i], NULL, churn_thread, (void*)(uintptr_t)i);
    }
    for (int i = 0; i < N_THREADS; i++) {
        pthread_join(threads[i], NULL);
    }

    int inserts = nova_aint_load(&g_list.total_inserts);
    int unlinks = nova_aint_load(&g_list.total_unlinks);
    int expected = N_THREADS * OPS_PER_THREAD;

    fprintf(stderr, "[t2_waiter_churn] inserts=%d, unlinks=%d, expected=%d\n",
            inserts, unlinks, expected);

    if (inserts != expected || unlinks != expected) {
        fprintf(stderr, "[t2_waiter_churn] FAIL: counter mismatch\n");
        return 1;
    }
    /* После всех unlinks list должен быть пустой. */
    if (g_list.head != NULL) {
        fprintf(stderr, "[t2_waiter_churn] FAIL: list head not NULL after all unlinks\n");
        return 1;
    }
    fprintf(stderr, "[t2_waiter_churn] PASS — list integrity preserved\n");
    return 0;
}
