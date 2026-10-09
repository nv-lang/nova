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

/* --- Plan 294 F.1 (D492): spawn with a live handle + piped child stdio -----
 *
 * `os_process_run` above stays as is (D453). The functions below give Nova a
 * persistent child handle and byte streams over its stdin/stdout/stderr.
 * Everything is plain C-ABI (pointer + length, int rc); the rich types
 * (`Child`, `ChildStdin`, ...) and every Result are built in std/os/proc.nv.
 *
 * Stdio modes (`stdio_modes` packs three 2-bit fields: stdin | stdout<<2 |
 * stderr<<4): NOVA_STDIO_NULL / NOVA_STDIO_INHERIT / NOVA_STDIO_PIPED.
 *
 * Errors are returned as NEGATIVE errno-style values (-ENOENT = -2, ...),
 * translated from libuv codes in process.c so the Windows and POSIX
 * projections agree (IoError.from_os reads POSIX errno numbers).
 *
 * Streams PULL: `proc_pipe_read` does uv_read_start, takes exactly one chunk
 * straight into the caller's buffer and does uv_read_stop in the callback, so
 * a child writing faster than the reader is blocked by the OS pipe: back
 * pressure with no queue of our own. `proc_pipe_write` parks until libuv has
 * flushed the whole buffer. Read and write use independent park slots.
 */
#define NOVA_STDIO_NULL    0
#define NOVA_STDIO_INHERIT 1
#define NOVA_STDIO_PIPED   2

/* Spawn; returns the child handle (NULL on failure with *out_err = -errno). */
void*    proc_spawn(const uint8_t* program, nova_int program_len,
                    const uint8_t* argv, nova_int argv_len, nova_int argc,
                    const uint8_t* env, nova_int env_len, nova_int envc,
                    nova_bool use_env,
                    const uint8_t* cwd, nova_int cwd_len,
                    nova_int stdio_modes, nova_int* out_err);
/* The parent end of the child's stdin(0)/stdout(1)/stderr(2) pipe, or NULL.
 * Marks it taken: the caller now owns it and closes it with proc_pipe_close. */
void*    proc_child_pipe(void* child, nova_int which);
nova_int proc_child_pid(void* child);
/* Park until the child exits. 0 = exited (*out_code / *out_signal valid);
 * NOVA_PROCESS_CANCELLED = enclosing scope cancelled (child was killed). */
nova_int proc_child_wait(void* child, nova_int* out_code, nova_int* out_signal);
/* 0 = exited (out valid), 1 = still running. Never parks. */
nova_int proc_child_try_wait(void* child, nova_int* out_code, nova_int* out_signal);
/* Drop the Nova-side ownership: kills a still-running child (SIGKILL /
 * TerminateProcess) and closes every pipe the caller never took. Idempotent. */
void     proc_child_release(void* child);

/* read: n > 0 bytes, 0 = end of stream, < 0 = -errno. */
nova_int proc_pipe_read(void* pipe, uint8_t* buf, nova_int cap);
/* write: whole buffer or < 0 = -errno (-32 = EPIPE: the child closed its end). */
nova_int proc_pipe_write(void* pipe, const uint8_t* buf, nova_int len);
/* Close our end (idempotent). Closing stdin delivers EOF to the child. */
void     proc_pipe_close(void* pipe);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_PROCESS_H */
