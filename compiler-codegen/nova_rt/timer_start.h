// SPDX-License-Identifier: MIT OR Apache-2.0
// One-shot relative timers, armed from the real clock.
//
// WHY. `uv_timer_start(h, cb, ms, 0)` schedules the fire time as
// `h->loop->time + ms`, and `loop->time` is a CACHE: libuv refreshes it once
// per loop iteration (uv_run's uv_update_time) and never inside callbacks or
// between iterations. A timer armed from code that runs on the loop's thread
// but OUTSIDE uv_run's iteration boundary -- a fiber body between two parks,
// the scope-entry code of `supervised(timeout:)`, a worker that ran ready
// fibers for a while -- is therefore counted from a stale instant and fires
// EARLY by exactly the cache's age. Measured (registry 2030): a stalled TCP
// write under `supervised(timeout: 200ms)` was interrupted after 0-2 ms in
// 3 of 5 runs and never later than 108 ms; under `timeout: 3000ms` after
// 2519-2789 ms. One uv_update_time before the arm made both exact
// (206-291 ms and 3011-3058 ms).
//
// WHAT THIS IS NOT. It does not make a timer late: refreshing the cache only
// moves the fire time later, up to the true `now + ms`. With `ms == 0` the
// timer still fires on the next iteration. The refresh is one uv_hrtime call
// and a store; it is not thread-safe in the libuv sense, but neither is the
// uv_timer_start that follows it -- both are done from the arming thread, as
// every site below already did.
#ifndef NOVA_TIMER_START_H
#define NOVA_TIMER_START_H

#include <uv.h>

/* Arm `h` (already uv_timer_init'ed on its loop) to fire once, `ms`
 * milliseconds from the real current time. Returns uv_timer_start's rc. */
static inline int nova_timer_start_rel(uv_timer_t* h, uv_timer_cb cb, uint64_t ms) {
    uv_update_time(h->loop);
    return uv_timer_start(h, cb, ms, 0);
}

#endif /* NOVA_TIMER_START_H */
