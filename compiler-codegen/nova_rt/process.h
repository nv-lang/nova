/* SPDX-License-Identifier: MIT OR Apache-2.0
 * nova_rt/process.h — std/os subprocess substrate (Plan 265 Ф.1, D453).
 *
 * ONE layer of FFI (net.h/fs.h precedent, D407 rule 2): a plain `os_process_run`
 * C-ABI function — scalars, pointer+length, out-parameter, return code. NO
 * `nova_str`, no persistent handle exposed to Nova. The Nova types
 * (`Command`/`ExitStatus`) and all logic live in `.nv` on top of
 * `extern "C"` (model: std/net on net.c, std/fs on fs.c).
 *
 * SINGLE-SHOT (D453 §Реализация note 1): unlike `TcpStream` (a handle that
 * survives across many separate calls), `os_process_run` spawns AND waits for
 * exit IN ONE C call — same shape as `net_dns_lookup`, not
 * `net_tcp_connect`. That is a deliberate scope cut (owner, 2026-08-10): no
 * stdio redirection this wave, so there is no reason to hand Nova a live
 * `Process` handle at all. A future `spawn()`+`Process` split (streaming)
 * is DEFERRABLE — gated on the stdio-redirection decision, not designed here.
 *
 * Cancellation: `os_process_run` parks the calling fiber (D93 park/wake) and
 * registers a stop_cb (`nova_sched_register_pending`) so an enclosing
 * `supervised(timeout:)`/`(cancel:)` — including a DIRECT body statement, no
 * `spawn` needed (D439 amend/№165) — kills the child (best-effort) and the
 * call reports `NOVA_PROCESS_CANCELLED` once woken, exactly mirroring
 * net.c's own `cancel_requested`-after-park check.
 */
#ifndef NOVA_RT_PROCESS_H
#define NOVA_RT_PROCESS_H

#ifndef NOVA_USE_LIBUV
#  error "Plan 265: NOVA_USE_LIBUV required for std/os process."
#endif

#include <uv.h>
#include <stdint.h>
#include "nova_rt.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Reserved sentinel `rc` value meaning "interrupted by scope
 * cancellation/timeout, not a real spawn error" (D453). Chosen far outside
 * any real POSIX/Windows errno range (at most a few hundred) so it can never
 * collide with a genuine `-errno` returned by a failed spawn. Mirrored on the
 * Nova side (`std/os/os.nv`, `PROCESS_CANCELLED`) — keep the two in sync. */
#define NOVA_PROCESS_CANCELLED ((nova_int)-100000)

/* Spawn `program` with `argc` NUL-separated arguments from `argv` (program
 * itself is NOT included in `argv` — the C side builds args[0] = program),
 * wait for it to exit, and report the outcome via the return code +
 * `*out_exit_code`:
 *
 *   0                    — the process ran to completion; `*out_exit_code`
 *                          is its exit code (0.., or 128+signal if it died
 *                          from a signal we did NOT send via cancellation).
 *   NOVA_PROCESS_CANCELLED — the enclosing supervised(timeout:)/(cancel:)
 *                          interrupted the wait; the child was killed
 *                          best-effort. `*out_exit_code` is meaningless.
 *   <0 (other)           — failed to SPAWN (PATH lookup / ENOENT / EACCES /
 *                          …), `-errno`-compatible (same convention as
 *                          `os_env.h`'s `_os_fail()`). `*out_exit_code` is
 *                          meaningless.
 *
 * `env`/`envc` are read only when `use_env` is true (NUL-separated
 * "KEY=VALUE" entries, envc of them — envc==0 is a valid, explicit EMPTY
 * environment); when `use_env` is false the child inherits the parent's
 * environment (libuv default, `env=NULL`). `cwd_len==0` inherits the
 * parent's current working directory. */
nova_int os_process_run(const uint8_t* program, nova_int program_len,
                      const uint8_t* argv, nova_int argv_len, nova_int argc,
                      const uint8_t* env, nova_int env_len, nova_int envc,
                      nova_bool use_env,
                      const uint8_t* cwd, nova_int cwd_len,
                      nova_int* out_exit_code);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_PROCESS_H */
