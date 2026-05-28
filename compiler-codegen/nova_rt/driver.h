// SPDX-License-Identifier: MIT OR Apache-2.0
#ifndef NOVA_RT_DRIVER_H
#define NOVA_RT_DRIVER_H

/* Plan 83.11: Centralized I/O driver — architectural pivot (port Tokio 1.x).
 *
 * Single dedicated driver thread owns one uv_loop_t. Workers (NovaWorker[])
 * submit jobs (arm timer, cancel scope, etc) via lock-protected MPSC queue
 * + uv_async_send wake. Driver processes jobs single-threaded — eliminates
 * cross-thread races by construction.
 *
 * See docs/plans/83.11-design.md for full architecture.
 * See docs/plans/83.11-centralized-io-driver.md §3.0 for Tokio source mapping. */

#include "sync.h"
#include "alloc.h"
#include <uv.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Forward decls — circular include with fibers.h. */
struct NovaSleepState;
struct NovaFiberQueue;
struct NovaBlockingState;

typedef enum {
    NOVA_DRV_JOB_ARM_SLEEP        = 1,
    NOVA_DRV_JOB_CANCEL_SCOPE     = 2,
    NOVA_DRV_JOB_CANCEL_TIMER     = 3,
    NOVA_DRV_JOB_ARM_BLOCKING     = 4,
    /* Sentinel — driver shutdown internal signal */
    NOVA_DRV_JOB__SHUTDOWN_SENTINEL = 99,
} NovaDriverJobKind;

typedef struct NovaDriverJob {
    NovaDriverJobKind kind;
    union {
        struct {
            struct NovaSleepState* st;
            uint64_t               ms;
        } arm_sleep;
        struct {
            struct NovaFiberQueue* scope;
        } cancel_scope;
        struct {
            struct NovaSleepState* st;
        } cancel_timer;
        struct {
            struct NovaBlockingState* st;
            void (*work)(void*);
            void* arg;
        } arm_blocking;
    } u;
    struct NovaDriverJob* next;  /* linked-list MPSC */
} NovaDriverJob;

/* MPSC mutex+linked-list queue. V1 simplicity over lock-free crossbeam-style.
 * Profile в Ф.8 — if hot, switch to MPSC ring buffer. */
typedef struct {
    nova_mutex_t   mu;
    NovaDriverJob* head;
    NovaDriverJob* tail;
} NovaDriverJobQueue;

typedef struct {
    uv_loop_t           loop;            /* dedicated driver UV loop */
    uv_thread_t         thread;
    uv_async_t          job_async;       /* worker→driver job submission wake */
    uv_async_t          shutdown_async;  /* shutdown signal handle */
    NovaDriverJobQueue  jobs;
    nova_atomic_bool    stop;            /* shutdown flag */
    nova_atomic_bool    started;         /* init completed flag */
} NovaDriver;

extern NovaDriver _nova_driver;

/* Initialize driver — called from nova_runtime_init AFTER worker pool
 * materialization (workers exist before driver routes wake events to them).
 * Idempotent: second call is no-op. */
void nova_driver_init(void);

/* Stop driver thread — called from nova_runtime_shutdown BEFORE worker join.
 * Drains in-flight jobs, closes UV handles, joins thread. Idempotent. */
void nova_driver_shutdown(void);

/* Submit job to driver. Lock-protected MPSC push + uv_async_send wake.
 * Job memory must be heap-allocated by caller (nova_alloc); driver frees
 * after processing.
 *
 * Returns 0 on success, -1 if driver not started or shutting down (job is
 * NOT freed in that case — caller responsibility либо leak acceptable on
 * shutdown path). */
int nova_driver_submit_job(NovaDriverJob* job);

/* Fast inline check — used by Time.sleep dispatch to route to driver path
 * (post Plan 83.11 Ф.3) vs legacy per-worker UV path (bootstrap). */
bool nova_driver_is_started(void);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_DRIVER_H */
