// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 83.11 Ф.2: Driver scaffolding. Lifecycle + job queue + main loop.
 *
 * NO logic yet — jobs are stubbed (logged but not processed). Ф.3 migrates
 * Time.sleep to use ARM_SLEEP/CANCEL_SCOPE jobs. Ф.4 adds blocking. Etc.
 *
 * Tokio reference: tokio/src/runtime/driver.rs */

#include "driver.h"
#include "sync.h"
#include "alloc.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef NOVA_GC_BOEHM
#include <gc.h>
#endif

NovaDriver _nova_driver = {0};

/* ── Forward declarations (file-private) ─────────────────────────── */
static void _nova_driver_main(void* arg);
static void _nova_driver_job_async_cb(uv_async_t* h);
static void _nova_driver_shutdown_async_cb(uv_async_t* h);
static void _nova_driver_drain_jobs(void);
static void _nova_driver_process_job(NovaDriverJob* job);

/* ── Public API ──────────────────────────────────────────────────── */

void nova_driver_init(void) {
    /* Idempotent guard. */
    if (nova_abool_load(&_nova_driver.started)) return;

    /* Init job queue. */
    nova_mutex_init(&_nova_driver.jobs.mu);
    _nova_driver.jobs.head = NULL;
    _nova_driver.jobs.tail = NULL;

    nova_abool_init(&_nova_driver.stop, false);

    /* Init UV loop. */
    int rc = uv_loop_init(&_nova_driver.loop);
    if (rc != 0) {
        fprintf(stderr, "nova: driver uv_loop_init failed: %s\n", uv_strerror(rc));
        abort();
    }

    /* Init async handles BEFORE thread creation — thread will call uv_run
     * which requires handles to exist. */
    rc = uv_async_init(&_nova_driver.loop, &_nova_driver.job_async,
                      _nova_driver_job_async_cb);
    if (rc != 0) {
        fprintf(stderr, "nova: driver job_async init failed: %s\n", uv_strerror(rc));
        abort();
    }
    rc = uv_async_init(&_nova_driver.loop, &_nova_driver.shutdown_async,
                      _nova_driver_shutdown_async_cb);
    if (rc != 0) {
        fprintf(stderr, "nova: driver shutdown_async init failed: %s\n", uv_strerror(rc));
        abort();
    }

    /* Mark started BEFORE thread spawn — thread checks this to bail early
     * if shutdown raced init (shouldn't happen but defensive). */
    nova_abool_store(&_nova_driver.started, true);

    /* Spawn driver thread. */
    rc = uv_thread_create(&_nova_driver.thread, _nova_driver_main, NULL);
    if (rc != 0) {
        fprintf(stderr, "nova: driver uv_thread_create failed: %s\n", uv_strerror(rc));
        abort();
    }
}

void nova_driver_shutdown(void) {
    if (!nova_abool_load(&_nova_driver.started)) return;

    /* Signal stop flag — driver loop checks между iterations. */
    nova_abool_store(&_nova_driver.stop, true);

    /* Wake driver from uv_run via shutdown_async — ensures loop exits ASAP. */
    uv_async_send(&_nova_driver.shutdown_async);

    /* Wait for driver thread to finish. */
    uv_thread_join(&_nova_driver.thread);

    /* Drain any leftover jobs (workers may have submitted after stop signal
     * but before async fired — race; we leak those jobs at shutdown which
     * is acceptable). */
    nova_mutex_lock(&_nova_driver.jobs.mu);
    NovaDriverJob* head = _nova_driver.jobs.head;
    _nova_driver.jobs.head = NULL;
    _nova_driver.jobs.tail = NULL;
    nova_mutex_unlock(&_nova_driver.jobs.mu);
    /* Memory leak on shutdown — jobs allocated via nova_alloc (Boehm GC) или
     * malloc; either way OS reclaims at exit. Not worth careful cleanup. */
    (void)head;

    nova_mutex_destroy(&_nova_driver.jobs.mu);
    /* Loop closure handled in _nova_driver_main shutdown sequence. */

    nova_abool_store(&_nova_driver.started, false);
}

int nova_driver_submit_job(NovaDriverJob* job) {
    if (!nova_abool_load(&_nova_driver.started)) return -1;
    if (nova_abool_load(&_nova_driver.stop))     return -1;
    if (!job) return -1;

    nova_mutex_lock(&_nova_driver.jobs.mu);
    job->next = NULL;
    if (_nova_driver.jobs.tail) {
        _nova_driver.jobs.tail->next = job;
    } else {
        _nova_driver.jobs.head = job;
    }
    _nova_driver.jobs.tail = job;
    nova_mutex_unlock(&_nova_driver.jobs.mu);

    uv_async_send(&_nova_driver.job_async);
    return 0;
}

/* ── Driver thread main ──────────────────────────────────────────── */

static void _nova_driver_main(void* arg) {
    (void)arg;

#ifdef NOVA_GC_BOEHM
    /* Register driver thread с Boehm. Driver code itself doesn't touch GC
     * heap directly, BUT processed jobs may transitively (e.g., wake worker
     * fiber whose context is GC-allocated). Safer to register. */
    struct GC_stack_base sb;
    if (GC_get_stack_base(&sb) == GC_SUCCESS) {
        GC_register_my_thread(&sb);
    }
#endif

    while (!nova_abool_load(&_nova_driver.stop)) {
        /* UV_RUN_ONCE: block until any handle fires (async wake from worker
         * job submission OR shutdown signal OR future timer/io events).
         * Returns when at least one event processed. */
        uv_run(&_nova_driver.loop, UV_RUN_ONCE);

        /* Drain job queue. Also drained от job_async_cb — this is backup
         * for jobs submitted after async fired но before we returned to
         * top of loop. */
        _nova_driver_drain_jobs();
    }

    /* Shutdown phase: close all active handles, run loop until clean. */
    uv_close((uv_handle_t*)&_nova_driver.job_async, NULL);
    uv_close((uv_handle_t*)&_nova_driver.shutdown_async, NULL);

    /* Run loop until no active handles (drains close callbacks). */
    while (uv_loop_alive(&_nova_driver.loop)) {
        uv_run(&_nova_driver.loop, UV_RUN_NOWAIT);
    }

    uv_loop_close(&_nova_driver.loop);

#ifdef NOVA_GC_BOEHM
    GC_unregister_my_thread();
#endif
}

/* ── Async callbacks (run на driver thread inside uv_run) ────────── */

static void _nova_driver_job_async_cb(uv_async_t* h) {
    (void)h;
    _nova_driver_drain_jobs();
}

static void _nova_driver_shutdown_async_cb(uv_async_t* h) {
    (void)h;
    /* No-op — flag check at loop top will exit. This handle just unblocks
     * uv_run. */
}

/* ── Job processing ──────────────────────────────────────────────── */

static void _nova_driver_drain_jobs(void) {
    /* Move entire queue under lock, then process outside lock (allows new
     * submissions during processing — they go to new queue, picked up next
     * iteration). */
    nova_mutex_lock(&_nova_driver.jobs.mu);
    NovaDriverJob* job = _nova_driver.jobs.head;
    _nova_driver.jobs.head = NULL;
    _nova_driver.jobs.tail = NULL;
    nova_mutex_unlock(&_nova_driver.jobs.mu);

    while (job) {
        NovaDriverJob* next = job->next;
        _nova_driver_process_job(job);
        job = next;
        /* Note: job memory NOT freed here — _nova_driver_process_job либо
         * keeps reference (for arm_sleep st pointer chain) либо handles its
         * own cleanup. Job struct itself nova_alloc'd, GC reclaims когда
         * unreachable. */
    }
}

static void _nova_driver_process_job(NovaDriverJob* job) {
    switch (job->kind) {
    case NOVA_DRV_JOB_ARM_SLEEP:
        /* Ф.3 implementation: uv_timer_init(&_nova_driver.loop, &st->timer);
         * uv_timer_start(...); _nova_driver_arm_list_insert(st); CAS state
         * NEW→ARMED. */
        fprintf(stderr, "nova: driver ARM_SLEEP job — Ф.3 NOT YET IMPLEMENTED\n");
        break;
    case NOVA_DRV_JOB_CANCEL_SCOPE:
        /* Ф.3 implementation: walk scope.armed_list; для каждого st CAS
         * ARMED→CANCEL_REQ; if won — uv_close(&st->timer). */
        fprintf(stderr, "nova: driver CANCEL_SCOPE job — Ф.3 NOT YET IMPLEMENTED\n");
        break;
    case NOVA_DRV_JOB_CANCEL_TIMER:
        fprintf(stderr, "nova: driver CANCEL_TIMER job — Ф.3 NOT YET IMPLEMENTED\n");
        break;
    case NOVA_DRV_JOB_ARM_BLOCKING:
        fprintf(stderr, "nova: driver ARM_BLOCKING job — Ф.4 NOT YET IMPLEMENTED\n");
        break;
    default:
        fprintf(stderr, "nova: driver unknown job kind %d\n", (int)job->kind);
        break;
    }
}
