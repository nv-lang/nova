// SPDX-License-Identifier: MIT OR Apache-2.0
/* Plan 83.11 Ф.2: Driver scaffolding. Lifecycle + job queue + main loop.
 *
 * NO logic yet — jobs are stubbed (logged but not processed). Ф.3 migrates
 * Time.sleep to use ARM_SLEEP/CANCEL_SCOPE jobs. Ф.4 adds blocking. Etc.
 *
 * Tokio reference: tokio/src/runtime/driver.rs */

#include "nova_rt.h"   /* full chain — needs NovaSleepState fields + nova_sched_wake */
#include "driver.h"
#include "runtime.h"   /* nova_runtime_signal_main if needed */
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

bool nova_driver_is_started(void) {
    return nova_abool_load(&_nova_driver.started) &&
           !nova_abool_load(&_nova_driver.stop);
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
        /* Plan 83.11 Ф.2: job allocated via malloc by worker (nova_driver_submit_job
         * caller). Free after processing. Driver retains st pointers (inside job
         * union) which point to fiber-stack-allocated NovaSleepState — those live
         * separately while fiber parked. */
        free(job);
        job = next;
    }
}

/* ── Plan 83.11 Ф.3: sleep state machine — driver side ──────────── */

/* Forward decls of driver-side callbacks. */
static void _nova_driver_sleep_timer_cb(uv_timer_t* h);
static void _nova_driver_sleep_close_cb(uv_handle_t* h);

/* Insert st at head of scope's armed list. Driver-thread only — no lock. */
static void _nova_driver_arm_list_insert(NovaSleepState* st) {
    NovaFiberQueue* scope = st->cancel_scope;
    if (!scope) return;
    st->next_in_scope = scope->armed_sleeps_head;
    st->pprev_in_scope = &scope->armed_sleeps_head;
    if (scope->armed_sleeps_head) {
        scope->armed_sleeps_head->pprev_in_scope = &st->next_in_scope;
    }
    scope->armed_sleeps_head = st;
}

/* O(1) unlink — driver-thread only. */
static void _nova_driver_arm_list_unlink(NovaSleepState* st) {
    if (!st->pprev_in_scope) return;  /* already unlinked or never inserted */
    *(st->pprev_in_scope) = st->next_in_scope;
    if (st->next_in_scope) {
        st->next_in_scope->pprev_in_scope = st->pprev_in_scope;
    }
    st->pprev_in_scope = NULL;
    st->next_in_scope = NULL;
}

/* ARM_SLEEP job handler — driver thread. */
static void _nova_driver_handle_arm_sleep(NovaSleepState* st, uint64_t ms) {
    if (!st) return;

    /* Init timer on driver's loop. */
    int rc = uv_timer_init(&_nova_driver.loop, &st->timer);
    if (rc != 0) {
        fprintf(stderr, "nova: driver uv_timer_init failed: %s\n", uv_strerror(rc));
        /* Move to CLOSED so worker fiber unparks (with error semantics — TBD). */
        nova_aint_store(&st->stage, NOVA_SLEEP_DRV_CLOSED);
        nova_sched_wake(st->scope, st->slot);
        return;
    }
    st->timer.data = st;

    /* Insert into scope's armed list BEFORE starting timer — если timer
     * fires immediately (ms=0), timer_cb might run synchronously? Actually
     * uv_timer_start with 0 fires on next loop iteration, not immediately.
     * But safe to insert first regardless. */
    _nova_driver_arm_list_insert(st);

    /* Transition NEW → ARMED. Single-mutator: no CAS needed for this transition.
     * RELEASE-store so worker's ACQUIRE-load sees it. */
    nova_aint_store(&st->stage, NOVA_SLEEP_DRV_ARMED);

    /* Start timer — может fire prior to return if ms is very small + something
     * weird, но libuv guarantees timer_cb runs only inside uv_run. We're called
     * from uv_run already (job_async_cb path). Timer registered to fire next
     * iteration. */
    rc = uv_timer_start(&st->timer, _nova_driver_sleep_timer_cb, ms, 0);
    if (rc != 0) {
        fprintf(stderr, "nova: driver uv_timer_start failed: %s\n", uv_strerror(rc));
        _nova_driver_arm_list_unlink(st);
        nova_aint_store(&st->stage, NOVA_SLEEP_DRV_CLOSED);
        uv_close((uv_handle_t*)&st->timer, NULL);  /* cleanup */
        nova_sched_wake(st->scope, st->slot);
        return;
    }
}

/* Timer fired naturally (sleep duration elapsed). Driver thread. */
static void _nova_driver_sleep_timer_cb(uv_timer_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    if (!st) return;

    /* CAS ARMED → FIRING. Loser = cancel-job won race; cancel path will
     * uv_close. We just exit. */
    int32_t expected = NOVA_SLEEP_DRV_ARMED;
    if (!nova_aint_cas(&st->stage, &expected, NOVA_SLEEP_DRV_FIRING)) {
        return;
    }

    /* Won — initiate close. close_cb will wake worker fiber. */
    uv_close((uv_handle_t*)&st->timer, _nova_driver_sleep_close_cb);
}

/* CANCEL_SCOPE job handler — driver thread. */
static void _nova_driver_handle_cancel_scope(NovaFiberQueue* scope) {
    if (!scope) return;

    /* Walk armed list. Single-mutator (driver) — no race на list itself.
     * BUT: list modifications (insert/unlink) might happen while we iterate?
     * NO — both insert (ARM_SLEEP) и unlink (close_cb) run on driver thread.
     * We're on driver thread now. No concurrent modification possible.
     *
     * BUT: uv_close inside the loop schedules close_cb to run later. close_cb
     * will unlink st. So we must save next pointer BEFORE calling uv_close. */
    NovaSleepState* st = scope->armed_sleeps_head;
    while (st) {
        NovaSleepState* next = st->next_in_scope;

        /* CAS ARMED → CANCEL_REQ. Loser = timer_cb won (will close itself). */
        int32_t expected = NOVA_SLEEP_DRV_ARMED;
        if (nova_aint_cas(&st->stage, &expected, NOVA_SLEEP_DRV_CANCEL_REQ)) {
            uv_close((uv_handle_t*)&st->timer, _nova_driver_sleep_close_cb);
        }
        /* CAS loser: timer_cb already won, will close. Skip. */

        st = next;
    }
}

/* CANCEL_TIMER job handler — driver thread. Single-timer cancel (для
 * cleanup callbacks of linked tokens etc). */
static void _nova_driver_handle_cancel_timer(NovaSleepState* st) {
    if (!st) return;
    int32_t expected = NOVA_SLEEP_DRV_ARMED;
    if (nova_aint_cas(&st->stage, &expected, NOVA_SLEEP_DRV_CANCEL_REQ)) {
        uv_close((uv_handle_t*)&st->timer, _nova_driver_sleep_close_cb);
    }
}

/* close_cb — final stage. Driver thread. Wakes worker fiber. */
static void _nova_driver_sleep_close_cb(uv_handle_t* h) {
    NovaSleepState* st = (NovaSleepState*)h->data;
    if (!st) return;

    /* Unlink from armed list. */
    _nova_driver_arm_list_unlink(st);

    /* Release-store CLOSED — worker's ACQUIRE in predicate pairs. */
    nova_aint_store(&st->stage, NOVA_SLEEP_DRV_CLOSED);

    /* Cross-thread wake worker fiber. Routed via existing dispatch_ready
     * mechanism (worker's wake_pending + uv_async_send). nova_sched_wake
     * does CAS parked true→false; на CAS-loss path (worker's futex pattern
     * already cleared parked at SEQ_CST recheck), wake is no-op which is
     * correct — worker already returned without yielding. */
    nova_sched_wake(st->scope, st->slot);
}

/* ── Job dispatch ────────────────────────────────────────────────── */

static void _nova_driver_process_job(NovaDriverJob* job) {
    switch (job->kind) {
    case NOVA_DRV_JOB_ARM_SLEEP:
        _nova_driver_handle_arm_sleep(job->u.arm_sleep.st, job->u.arm_sleep.ms);
        break;
    case NOVA_DRV_JOB_CANCEL_SCOPE:
        _nova_driver_handle_cancel_scope(job->u.cancel_scope.scope);
        break;
    case NOVA_DRV_JOB_CANCEL_TIMER:
        _nova_driver_handle_cancel_timer(job->u.cancel_timer.st);
        break;
    case NOVA_DRV_JOB_ARM_BLOCKING:
        fprintf(stderr, "nova: driver ARM_BLOCKING job — Ф.4 NOT YET IMPLEMENTED\n");
        break;
    default:
        fprintf(stderr, "nova: driver unknown job kind %d\n", (int)job->kind);
        break;
    }
}
