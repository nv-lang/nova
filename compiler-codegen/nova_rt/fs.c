/* Plan 176 Ф.2 (D323): nova_rt/fs.c — async filesystem stdlib via libuv uv_fs_*.
 *
 * Park/wake pattern (identical to net.c / Plan 22):
 *   1. Allocate a request on the GC heap (kept alive by the parked fiber's
 *      stack; on the cancel path the fiber stays parked until the completion
 *      callback fires, so the request is never freed under an in-flight op).
 *   2. Issue uv_fs_* on nova_current_loop() with _fs_cb.
 *   3. register_pending(stop_cb) + park; the completion callback (loop thread)
 *      wakes us.
 *   4. Resume: unregister, read req.result, uv_fs_req_cleanup.
 *
 * Errors: UV_E* is translated to a stable POSIX errno (so the Nova-side
 * kind_from_errno is platform-independent) and returned NEGATED. Stat / scandir
 * / realpath results are cached in thread-local slots (cooperative-safe — the
 * Nova handler reads them before any other blocking op).
 */

#ifndef NOVA_USE_LIBUV
#  error "Plan 176 Ф.2: NOVA_USE_LIBUV required."
#endif

#include "fs.h"
#include <string.h>
#include <stdlib.h>
#include <stdio.h>
#include <sys/stat.h>
#if !defined(_WIN32)
#  include <fcntl.h>
#endif

/* ─── UV error → POSIX errno (kind_from_errno-stable) ───────────────── */

static int _fs_errno(int uvrc) {
    switch (uvrc) {
        case UV_ENOENT:    return 2;
        case UV_EPERM:     return 1;
        case UV_EINTR:     return 4;
        case UV_EAGAIN:    return 11;
        case UV_EACCES:    return 13;
        case UV_EEXIST:    return 17;
        case UV_EXDEV:     return 18;
        case UV_ENOTDIR:   return 20;
        case UV_EISDIR:    return 21;
        case UV_EINVAL:    return 22;
        case UV_ENOSPC:    return 28;
        case UV_EROFS:     return 30;
        case UV_EPIPE:     return 32;
        case UV_ENOTEMPTY: return 39;
        /* Unmapped: return a positive code that kind_from_errno funnels to
         * Other(raw). Negate the UV code so it stays distinctive. */
        default:           return uvrc < 0 ? -uvrc : uvrc;
    }
}

/* Negative POSIX errno for a failed uv result (what the Nova hooks return). */
static nova_int _fs_fail(int uvrc) {
    return (nova_int)(-_fs_errno(uvrc));
}

/* ─── Request + park/wake plumbing ─────────────────────────────────── */

typedef struct {
    NovaFiberQueue* scope;
    int             slot;
    uv_fs_t         req;
} _NovaFsReq;

static void _fs_cb(uv_fs_t* req) {
    _NovaFsReq* fr = (_NovaFsReq*)req->data;
    NovaFiberQueue* sc = fr->scope;
    int sl = fr->slot;
    fr->scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

static NovaStopMode _fs_stop_cb(void* handle) {
    _NovaFsReq* fr = (_NovaFsReq*)handle;
    /* Best-effort cancel (Q4): uv_cancel only affects a still-queued request; an
     * in-flight syscall runs to completion and _fs_cb still wakes us. Stay parked
     * (ASYNC) so the request outlives the abandoned wait — no UAF, no leak. */
    uv_cancel((uv_req_t*)&fr->req);
    return NOVA_STOP_ASYNC;
}

static _NovaFsReq* _fs_begin(void) {
    _NovaFsReq* fr = (_NovaFsReq*)nova_alloc(sizeof(_NovaFsReq));
    memset(fr, 0, sizeof(*fr));
    fr->req.data = fr;
    fr->scope = _nova_active_scope;
    fr->slot  = _nova_active_slot;
    if (!fr->scope) {
        fprintf(stderr, "nova/fs: filesystem op outside a supervised scope\n");
        abort();
    }
    return fr;
}

/* Park until the issued request completes. Returns req.result (>= 0 or the
 * negative UV code). Caller uv_fs_req_cleanup's AFTER reading statbuf/ptr. */
static ssize_t _fs_wait(_NovaFsReq* fr, int issue_rc) {
    if (issue_rc < 0) return issue_rc;
    NovaFiberQueue* scope = fr->scope;
    int slot = fr->slot;
    nova_sched_register_pending(scope, slot, fr, _fs_stop_cb);
    nova_sched_park(scope, slot);
    nova_sched_unregister_pending(scope, slot);
    return (ssize_t)fr->req.result;
}

/* Result → Nova int (fd / count on success, negated POSIX errno on failure). */
static nova_int _fs_ret(ssize_t r) {
    return (r < 0) ? _fs_fail((int)r) : (nova_int)r;
}

static nova_str _nova_fs_cstr(const char* s) {
    if (!s) { nova_str z; z.ptr = NULL; z.len = 0; return z; }
    size_t n = strlen(s);
    char* p = (char*)nova_alloc(n + 1);
    memcpy(p, s, n + 1);
    nova_str out;
    out.ptr = (const uint8_t*)p;
    out.len = (nova_int)n;
    return out;
}

/* Portable OPEN_* bitset → platform UV_FS_O_* flags (ffi.nv: 1/2/4/8/16/32). */
static int _open_flags(int nf) {
    int rd = nf & 1, wr = nf & 2, ap = nf & 4, tr = nf & 8, cr = nf & 16, ex = nf & 32;
    int f = 0;
    if (rd && (wr || ap))   f |= UV_FS_O_RDWR;
    else if (wr || ap)      f |= UV_FS_O_WRONLY;
    else                    f |= UV_FS_O_RDONLY;
    if (ap) f |= UV_FS_O_APPEND;
    if (tr) f |= UV_FS_O_TRUNC;
    if (cr) f |= UV_FS_O_CREAT;
    if (ex) f |= UV_FS_O_EXCL;
    return f;
}

/* ─── open / close / read / write ──────────────────────────────────── */

nova_int nova_fs_open(const uint8_t* path, nova_int flags, nova_int mode) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_open(nova_current_loop(), &fr->req, (const char*)path,
                        _open_flags((int)flags), (int)mode, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_close(nova_int fd) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_close(nova_current_loop(), &fr->req, (uv_file)fd, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_read(nova_int fd, uint8_t* buf, nova_int len) {
    _NovaFsReq* fr = _fs_begin();
    uv_buf_t b = uv_buf_init((char*)buf, (unsigned int)len);
    int rc = uv_fs_read(nova_current_loop(), &fr->req, (uv_file)fd, &b, 1, -1, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_write(nova_int fd, const uint8_t* buf, nova_int len) {
    _NovaFsReq* fr = _fs_begin();
    uv_buf_t b = uv_buf_init((char*)(uintptr_t)buf, (unsigned int)len);
    int rc = uv_fs_write(nova_current_loop(), &fr->req, (uv_file)fd, &b, 1, -1, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_read_at(nova_int fd, uint8_t* buf, nova_int len, nova_int offset) {
    _NovaFsReq* fr = _fs_begin();
    uv_buf_t b = uv_buf_init((char*)buf, (unsigned int)len);
    int rc = uv_fs_read(nova_current_loop(), &fr->req, (uv_file)fd, &b, 1, (int64_t)offset, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_write_at(nova_int fd, const uint8_t* buf, nova_int len, nova_int offset) {
    _NovaFsReq* fr = _fs_begin();
    uv_buf_t b = uv_buf_init((char*)(uintptr_t)buf, (unsigned int)len);
    int rc = uv_fs_write(nova_current_loop(), &fr->req, (uv_file)fd, &b, 1, (int64_t)offset, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_fsync(nova_int fd) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_fsync(nova_current_loop(), &fr->req, (uv_file)fd, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_fdatasync(nova_int fd) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_fdatasync(nova_current_loop(), &fr->req, (uv_file)fd, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

/* ─── stat / lstat / fstat + TLS cache ─────────────────────────────── */

#if defined(_MSC_VER)
  static __declspec(thread) uv_stat_t _fs_stat_tls;
#else
  static __thread uv_stat_t _fs_stat_tls;
#endif

static int _kind_from_mode(uint64_t m) {
    unsigned t = (unsigned)(m & (uint64_t)S_IFMT);
    if (t == (unsigned)S_IFDIR) return 2;   /* KIND_DIR */
#ifdef S_IFLNK
    if (t == (unsigned)S_IFLNK) return 3;   /* KIND_SYMLINK */
#endif
    if (t == (unsigned)S_IFREG) return 1;   /* KIND_FILE */
    return 0;                               /* KIND_OTHER */
}

nova_int nova_fs_stat(const uint8_t* path) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_stat(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    if (r >= 0) _fs_stat_tls = fr->req.statbuf;
    uv_fs_req_cleanup(&fr->req);
    return (r < 0) ? _fs_fail((int)r) : 0;
}

nova_int nova_fs_lstat(const uint8_t* path) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_lstat(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    if (r >= 0) _fs_stat_tls = fr->req.statbuf;
    uv_fs_req_cleanup(&fr->req);
    return (r < 0) ? _fs_fail((int)r) : 0;
}

nova_int nova_fs_fstat(nova_int fd) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_fstat(nova_current_loop(), &fr->req, (uv_file)fd, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    if (r >= 0) _fs_stat_tls = fr->req.statbuf;
    uv_fs_req_cleanup(&fr->req);
    return (r < 0) ? _fs_fail((int)r) : 0;
}

nova_int nova_fs_stat_size(void) { return (nova_int)_fs_stat_tls.st_size; }
nova_int nova_fs_stat_mode(void) { return (nova_int)_fs_stat_tls.st_mode; }
nova_int nova_fs_stat_kind(void) { return (nova_int)_kind_from_mode(_fs_stat_tls.st_mode); }
int64_t  nova_fs_stat_mtime_ns(void) {
    return (int64_t)_fs_stat_tls.st_mtim.tv_sec * 1000000000LL + (int64_t)_fs_stat_tls.st_mtim.tv_nsec;
}
int64_t  nova_fs_stat_atime_ns(void) {
    return (int64_t)_fs_stat_tls.st_atim.tv_sec * 1000000000LL + (int64_t)_fs_stat_tls.st_atim.tv_nsec;
}
int64_t  nova_fs_stat_ctime_ns(void) {
    /* Prefer birth time (creation); fall back to 0 → Nova @created() == None. */
    return (int64_t)_fs_stat_tls.st_birthtim.tv_sec * 1000000000LL + (int64_t)_fs_stat_tls.st_birthtim.tv_nsec;
}

/* ─── mkdir / unlink / rmdir / rename / symlink / chmod / copyfile ──── */

nova_int nova_fs_mkdir(const uint8_t* path, nova_int mode) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_mkdir(nova_current_loop(), &fr->req, (const char*)path, (int)mode, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_unlink(const uint8_t* path) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_unlink(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_rmdir(const uint8_t* path) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_rmdir(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_rename(const uint8_t* from, const uint8_t* to) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_rename(nova_current_loop(), &fr->req, (const char*)from, (const char*)to, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_symlink(const uint8_t* target, const uint8_t* link) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_symlink(nova_current_loop(), &fr->req, (const char*)target, (const char*)link, 0, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_chmod(const uint8_t* path, nova_int mode) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_chmod(nova_current_loop(), &fr->req, (const char*)path, (int)mode, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int nova_fs_copyfile(const uint8_t* from, const uint8_t* to) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_copyfile(nova_current_loop(), &fr->req, (const char*)from, (const char*)to, 0, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    /* On success uv reports 0; the caller wants the byte count → stat the dest is
     * overkill, so report the source size best-effort via the copyfile result if
     * libuv exposed it. libuv sets result=0 on success; return 0 → the Nova
     * `copy` reports Ok(0). (Byte-accurate copy count → followup.) */
    uv_fs_req_cleanup(&fr->req);
    return (r < 0) ? _fs_fail((int)r) : 0;
}

/* ─── realpath + TLS cache ─────────────────────────────────────────── */

#if defined(_MSC_VER)
  static __declspec(thread) nova_str _fs_realpath_tls;
#else
  static __thread nova_str _fs_realpath_tls;
#endif

nova_int nova_fs_realpath(const uint8_t* path) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_realpath(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    if (r >= 0) _fs_realpath_tls = _nova_fs_cstr((const char*)fr->req.ptr);
    uv_fs_req_cleanup(&fr->req);
    return (r < 0) ? _fs_fail((int)r) : 0;
}

nova_str nova_fs_realpath_data(void) { return _fs_realpath_tls; }

/* ─── parent-dir fsync (write_atomic step 5) ───────────────────────── */

nova_int nova_fs_fsync_dir(const uint8_t* path) {
#if defined(_WIN32)
    /* Directory fsync is not supported by the Win32 API — the atomic MoveFileEx
     * rename is already durable enough for the write_atomic contract (§3c). */
    (void)path;
    return 0;
#else
    _NovaFsReq* fo = _fs_begin();
    int rc = uv_fs_open(nova_current_loop(), &fo->req, (const char*)path, O_RDONLY, 0, _fs_cb);
    ssize_t r = _fs_wait(fo, rc);
    uv_fs_req_cleanup(&fo->req);
    if (r < 0) return 0;   /* best-effort */
    uv_file dfd = (uv_file)r;
    _NovaFsReq* fsn = _fs_begin();
    int rc2 = uv_fs_fsync(nova_current_loop(), &fsn->req, dfd, _fs_cb);
    (void)_fs_wait(fsn, rc2);
    uv_fs_req_cleanup(&fsn->req);
    _NovaFsReq* fc = _fs_begin();
    int rc3 = uv_fs_close(nova_current_loop(), &fc->req, dfd, _fs_cb);
    (void)_fs_wait(fc, rc3);
    uv_fs_req_cleanup(&fc->req);
    return 0;
#endif
}

/* ─── scandir (iterator over TLS-held request) ─────────────────────── */

#if defined(_MSC_VER)
  static __declspec(thread) _NovaFsReq*   _fs_scandir_fr;
  static __declspec(thread) uv_dirent_t   _fs_scandir_ent;
#else
  static __thread _NovaFsReq*   _fs_scandir_fr;
  static __thread uv_dirent_t   _fs_scandir_ent;
#endif

nova_int nova_fs_scandir(const uint8_t* path) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_scandir(nova_current_loop(), &fr->req, (const char*)path, 0, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    if (r < 0) {
        nova_int e = _fs_fail((int)r);
        uv_fs_req_cleanup(&fr->req);
        _fs_scandir_fr = NULL;
        return e;
    }
    _fs_scandir_fr = fr;   /* keep the req live for iteration; cleaned in _next */
    return (nova_int)r;    /* entry count */
}

nova_int nova_fs_scandir_next(void) {
    if (!_fs_scandir_fr) return 0;
    int rc = uv_fs_scandir_next(&_fs_scandir_fr->req, &_fs_scandir_ent);
    if (rc != 0) {
        /* UV_EOF or error → done. */
        uv_fs_req_cleanup(&_fs_scandir_fr->req);
        _fs_scandir_fr = NULL;
        return 0;
    }
    return 1;   /* _fs_scandir_ent holds this entry */
}

nova_str nova_fs_scandir_name(void) {
    return _nova_fs_cstr(_fs_scandir_ent.name);
}

nova_int nova_fs_scandir_kind(void) {
    switch (_fs_scandir_ent.type) {
        case UV_DIRENT_DIR:  return 2;   /* KIND_DIR */
        case UV_DIRENT_LINK: return 3;   /* KIND_SYMLINK */
        case UV_DIRENT_FILE: return 1;   /* KIND_FILE */
        default:             return 0;   /* KIND_OTHER / UNKNOWN */
    }
}
