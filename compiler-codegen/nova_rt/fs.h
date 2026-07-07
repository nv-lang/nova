#ifndef NOVA_RT_FS_H
#define NOVA_RT_FS_H

/* Plan 176 Ф.2 (D323); M:N-safety redesign [M-fs-tls-mn-race] (2026-07-08):
 * async uv_fs_* + park/wake.
 *
 * fd-based (not opaque-handle like net): an open file is a plain OS `int` fd, a
 * path is a NUL-terminated UTF-8/WTF-8 `const char*` (libuv converts to UTF-16 on
 * Windows internally), data is `(uint8_t*, len)`. Every blocking op parks the
 * calling fiber via the Plan 22 park/wake pattern (identical to net.c): the
 * request runs on the libuv threadpool and its completion callback (on the loop
 * thread) wakes the parked fiber. Cancel is best-effort (Q4): uv_cancel on a
 * still-queued request; an in-flight syscall runs to completion.
 *
 * Return convention (net-precedent): >= 0 on success (fd / byte count / 0), or a
 * NEGATIVE POSIX errno on failure (UV_E* is translated to a stable POSIX errno so
 * the Nova-side kind_from_errno is platform-independent).
 *
 * NO static/TLS result slots (net.c Д2 fix, applied here): a fiber can migrate
 * between OS threads between any two calls (Plan 44.7 preemption) and two fibers
 * can interleave on the same thread, so a result NEVER transits a __thread slot —
 * it lands in memory the CALLER owns:
 *   - stat/lstat/fstat: the caller passes a STAT_IMAGE_BYTES-sized `img` buffer
 *     (own by the Nova record/local, like net's NovaNetAddr image); `fs_*_into`
 *     fills it, and the pointer-taking accessors below read straight from it —
 *     no accessor reads ambient state.
 *   - realpath: the canonical path is materialised into a fresh GC `nova_str` and
 *     returned directly from the SAME call that resolves it — no follow-up
 *     accessor, no cache.
 *   - scandir: handle-based. `fs_scandir_open` returns an opaque handle (the
 *     underlying request pointer boxed as `nova_int` — safe on an address-sized
 *     `nova_int`/intptr_t); `next`/`name`/`kind` take that handle explicitly and
 *     read its own request state, never a global. The handle is owned by the
 *     Nova-side iteration record until `fs_scandir_close`.
 *
 * The non-blocking fs_seek (lseek) and the platform predicate live in
 * io_console.h (always available, no libuv).
 */

#ifndef NOVA_USE_LIBUV
#  error "Plan 176 Ф.2: NOVA_USE_LIBUV required for std/fs."
#endif

#include <uv.h>
#include <stdint.h>
#include "nova_rt.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Blocking uv_fs_* hooks (park the fiber). */
nova_int fs_open(const uint8_t* path, nova_int flags, nova_int mode);
nova_int fs_close(nova_int fd);
nova_int fs_read(nova_int fd, uint8_t* buf, nova_int len);
nova_int fs_write(nova_int fd, const uint8_t* buf, nova_int len);
nova_int fs_read_at(nova_int fd, uint8_t* buf, nova_int len, nova_int offset);
nova_int fs_write_at(nova_int fd, const uint8_t* buf, nova_int len, nova_int offset);
nova_int fs_fsync(nova_int fd);
nova_int fs_fdatasync(nova_int fd);

/* ─── stat image (value-record, mirrors NovaNetAddr / net_addr_size) ────────
 * `fs_stat_image_bytes()` is the sizeof-source the Nova side allocates a []u8
 * buffer with (like ADDR_IMAGE_BYTES = net_addr_size()). `fs_*_into` fill the
 * caller's image in place; 0 or -errno. The accessors read straight off an
 * `img` pointer — no ambient/cached state, so any number of fibers on any
 * number of threads can each hold their own image concurrently. */
nova_int fs_stat_image_bytes(void);
nova_int fs_stat_into(const uint8_t* path, uint8_t* img);
nova_int fs_lstat_into(const uint8_t* path, uint8_t* img);
nova_int fs_fstat_into(nova_int fd, uint8_t* img);
nova_int fs_stat_size(const uint8_t* img);
int64_t  fs_stat_mtime_ns(const uint8_t* img);
int64_t  fs_stat_atime_ns(const uint8_t* img);
int64_t  fs_stat_ctime_ns(const uint8_t* img);
nova_int fs_stat_mode(const uint8_t* img);
nova_int fs_stat_kind(const uint8_t* img);
/* Build a stat image straight from components, no syscall (mirrors
 * net_addr_v4_into's "pure value constructor" shape) — used by std/fs's
 * mock_fs so a mocked stat produces a real_fs-compatible image without an
 * OS-owned cache of any kind. */
void fs_stat_build_into(uint8_t* img, nova_int size, int64_t mtime_ns,
                         int64_t atime_ns, int64_t ctime_ns, nova_int mode, nova_int kind);

nova_int fs_mkdir(const uint8_t* path, nova_int mode);
nova_int fs_unlink(const uint8_t* path);
nova_int fs_rmdir(const uint8_t* path);
nova_int fs_rename(const uint8_t* from, const uint8_t* to);
nova_int fs_symlink(const uint8_t* target, const uint8_t* link);
nova_int fs_chmod(const uint8_t* path, nova_int mode);
nova_int fs_copyfile(const uint8_t* from, const uint8_t* to);

/* realpath: canonical path is returned DIRECTLY as a fresh GC nova_str (empty
 * str + *out_err = -errno on failure; *out_err = 0 on success). No TLS, no
 * follow-up accessor. */
nova_str fs_realpath_into(const uint8_t* path, nova_int* out_err);

/* Best-effort parent-dir fsync (write_atomic step 5); no-op on Windows. */
nova_int fs_fsync_dir(const uint8_t* path);

/* ─── scandir (handle-based iterator, net-precedent opaque handle) ─────────
 * fs_scandir_open: entry stream handle (boxed request pointer, an address-sized
 * nova_int) on success, or a NEGATIVE POSIX errno. fs_scandir_next reads/advances
 * THAT handle's own request (1 = an entry is ready, 0 = done); fs_scandir_name/
 * _kind read the handle's current entry. fs_scandir_close releases the
 * underlying uv_fs_t — idempotent (safe to call again after natural
 * exhaustion, and MUST be called by the Nova side on an early break to avoid
 * leaking the open request). */
nova_int fs_scandir_open(const uint8_t* path);
nova_int fs_scandir_next(nova_int h);
nova_str fs_scandir_name(nova_int h);
nova_int fs_scandir_kind(nova_int h);
void     fs_scandir_close(nova_int h);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_FS_H */
