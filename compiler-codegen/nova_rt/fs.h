#ifndef NOVA_RT_FS_H
#define NOVA_RT_FS_H

/* Plan 176 Ф.2 (D323): std/fs libuv backend — async uv_fs_* + park/wake.
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
 * the Nova-side kind_from_errno is platform-independent). Stat/scandir/realpath
 * results are cached in thread-local slots read by the *_data/_size/... accessors
 * immediately after (cooperative-safe: no blocking op interleaves).
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
nova_int nova_fs_open(const uint8_t* path, nova_int flags, nova_int mode);
nova_int nova_fs_close(nova_int fd);
nova_int nova_fs_read(nova_int fd, uint8_t* buf, nova_int len);
nova_int nova_fs_write(nova_int fd, const uint8_t* buf, nova_int len);
nova_int nova_fs_read_at(nova_int fd, uint8_t* buf, nova_int len, nova_int offset);
nova_int nova_fs_write_at(nova_int fd, const uint8_t* buf, nova_int len, nova_int offset);
nova_int nova_fs_fsync(nova_int fd);
nova_int nova_fs_fdatasync(nova_int fd);

/* stat/lstat/fstat: 0 or -errno; success caches the uv_stat_t in TLS. */
nova_int nova_fs_stat(const uint8_t* path);
nova_int nova_fs_lstat(const uint8_t* path);
nova_int nova_fs_fstat(nova_int fd);
nova_int nova_fs_stat_size(void);
int64_t  nova_fs_stat_mtime_ns(void);
int64_t  nova_fs_stat_atime_ns(void);
int64_t  nova_fs_stat_ctime_ns(void);
nova_int nova_fs_stat_mode(void);
nova_int nova_fs_stat_kind(void);

nova_int nova_fs_mkdir(const uint8_t* path, nova_int mode);
nova_int nova_fs_unlink(const uint8_t* path);
nova_int nova_fs_rmdir(const uint8_t* path);
nova_int nova_fs_rename(const uint8_t* from, const uint8_t* to);
nova_int nova_fs_symlink(const uint8_t* target, const uint8_t* link);
nova_int nova_fs_chmod(const uint8_t* path, nova_int mode);
nova_int nova_fs_copyfile(const uint8_t* from, const uint8_t* to);

/* realpath: 0 or -errno; success caches the canonical path bytes in TLS. */
nova_int nova_fs_realpath(const uint8_t* path);
nova_str nova_fs_realpath_data(void);

/* Best-effort parent-dir fsync (write_atomic step 5); no-op on Windows. */
nova_int nova_fs_fsync_dir(const uint8_t* path);

/* scandir: entry count (>= 0) or -errno; then iterate with nova_fs_scandir_next(). */
nova_int nova_fs_scandir(const uint8_t* path);
nova_int nova_fs_scandir_next(void);   /* 1 = an entry is cached, 0 = done */
nova_str nova_fs_scandir_name(void);
nova_int nova_fs_scandir_kind(void);

#ifdef __cplusplus
}
#endif

#endif /* NOVA_RT_FS_H */
