/* Plan 176 Ф.2 (D323); M:N-safety redesign [M-fs-tls-mn-race] (2026-07-08):
 * nova_rt/fs.c — async filesystem stdlib via libuv uv_fs_*.
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
 * kind_from_errno is platform-independent) and returned NEGATED.
 *
 * NO __thread/__declspec(thread) result slots (net.c Д2 fix, applied here —
 * see fs.h for the full rationale): stat lands in a caller-owned image buffer,
 * realpath returns its GC string directly from the resolving call, and scandir
 * is handle-based. Invariant: grep -n "thread)" fs.c == 0.
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
    /* scandir-handle-only fields (unused by every other op): the entry the
     * last fs_scandir_next() call landed on, and whether the underlying
     * uv_fs_t has already been uv_fs_req_cleanup'd (idempotent close: set on
     * natural EOF too, so a later fs_scandir_close() is a safe no-op). */
    uv_dirent_t     cur_ent;
    int             scandir_done;
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

static nova_str _fs_cstr(const char* s) {
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

nova_int fs_open(const uint8_t* path, nova_int flags, nova_int mode) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_open(nova_current_loop(), &fr->req, (const char*)path,
                        _open_flags((int)flags), (int)mode, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_close(nova_int fd) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_close(nova_current_loop(), &fr->req, (uv_file)fd, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_read(nova_int fd, uint8_t* buf, nova_int len) {
    _NovaFsReq* fr = _fs_begin();
    uv_buf_t b = uv_buf_init((char*)buf, (unsigned int)len);
    int rc = uv_fs_read(nova_current_loop(), &fr->req, (uv_file)fd, &b, 1, -1, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_write(nova_int fd, const uint8_t* buf, nova_int len) {
    _NovaFsReq* fr = _fs_begin();
    uv_buf_t b = uv_buf_init((char*)(uintptr_t)buf, (unsigned int)len);
    int rc = uv_fs_write(nova_current_loop(), &fr->req, (uv_file)fd, &b, 1, -1, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_read_at(nova_int fd, uint8_t* buf, nova_int len, nova_int offset) {
    _NovaFsReq* fr = _fs_begin();
    uv_buf_t b = uv_buf_init((char*)buf, (unsigned int)len);
    int rc = uv_fs_read(nova_current_loop(), &fr->req, (uv_file)fd, &b, 1, (int64_t)offset, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_write_at(nova_int fd, const uint8_t* buf, nova_int len, nova_int offset) {
    _NovaFsReq* fr = _fs_begin();
    uv_buf_t b = uv_buf_init((char*)(uintptr_t)buf, (unsigned int)len);
    int rc = uv_fs_write(nova_current_loop(), &fr->req, (uv_file)fd, &b, 1, (int64_t)offset, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_fsync(nova_int fd) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_fsync(nova_current_loop(), &fr->req, (uv_file)fd, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_fdatasync(nova_int fd) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_fdatasync(nova_current_loop(), &fr->req, (uv_file)fd, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

/* ─── stat / lstat / fstat → caller-owned image (no TLS) ───────────────
 *
 * NovaFsStat is the C POD the []u8 image wraps (Nova side: STAT_IMAGE_BYTES =
 * fs_stat_image_bytes(), exactly the net_addr_size()/NovaNetAddr pattern).
 * Field order/packing is private to this file — the Nova side never bakes
 * offsets, only calls the pointer-taking accessors below. */
typedef struct {
    int64_t size;
    int64_t mtime_ns;
    int64_t atime_ns;
    int64_t ctime_ns;
    int32_t mode;
    int32_t kind;
} NovaFsStat;

nova_int fs_stat_image_bytes(void) { return (nova_int)sizeof(NovaFsStat); }

static int _kind_from_mode(uint64_t m) {
    unsigned t = (unsigned)(m & (uint64_t)S_IFMT);
    if (t == (unsigned)S_IFDIR) return 2;   /* KIND_DIR */
#ifdef S_IFLNK
    if (t == (unsigned)S_IFLNK) return 3;   /* KIND_SYMLINK */
#endif
    if (t == (unsigned)S_IFREG) return 1;   /* KIND_FILE */
    return 0;                               /* KIND_OTHER */
}

static void _fill_stat_image(uint8_t* img, const uv_stat_t* st) {
    NovaFsStat* s = (NovaFsStat*)img;
    s->size = (int64_t)st->st_size;
    s->mtime_ns = (int64_t)st->st_mtim.tv_sec * 1000000000LL + (int64_t)st->st_mtim.tv_nsec;
    s->atime_ns = (int64_t)st->st_atim.tv_sec * 1000000000LL + (int64_t)st->st_atim.tv_nsec;
    /* Prefer birth time (creation); fall back to 0 → Nova @created() == None. */
    s->ctime_ns = (int64_t)st->st_birthtim.tv_sec * 1000000000LL + (int64_t)st->st_birthtim.tv_nsec;
    s->mode = (int32_t)st->st_mode;
    s->kind = (int32_t)_kind_from_mode(st->st_mode);
}

nova_int fs_stat_into(const uint8_t* path, uint8_t* img) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_stat(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    if (r >= 0) _fill_stat_image(img, &fr->req.statbuf);
    uv_fs_req_cleanup(&fr->req);
    return (r < 0) ? _fs_fail((int)r) : 0;
}

nova_int fs_lstat_into(const uint8_t* path, uint8_t* img) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_lstat(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    if (r >= 0) _fill_stat_image(img, &fr->req.statbuf);
    uv_fs_req_cleanup(&fr->req);
    return (r < 0) ? _fs_fail((int)r) : 0;
}

nova_int fs_fstat_into(nova_int fd, uint8_t* img) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_fstat(nova_current_loop(), &fr->req, (uv_file)fd, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    if (r >= 0) _fill_stat_image(img, &fr->req.statbuf);
    uv_fs_req_cleanup(&fr->req);
    return (r < 0) ? _fs_fail((int)r) : 0;
}

nova_int fs_stat_size(const uint8_t* img)      { return (nova_int)((const NovaFsStat*)img)->size; }
nova_int fs_stat_mode(const uint8_t* img)      { return (nova_int)((const NovaFsStat*)img)->mode; }
nova_int fs_stat_kind(const uint8_t* img)      { return (nova_int)((const NovaFsStat*)img)->kind; }
int64_t  fs_stat_mtime_ns(const uint8_t* img)  { return ((const NovaFsStat*)img)->mtime_ns; }
int64_t  fs_stat_atime_ns(const uint8_t* img)  { return ((const NovaFsStat*)img)->atime_ns; }
int64_t  fs_stat_ctime_ns(const uint8_t* img)  { return ((const NovaFsStat*)img)->ctime_ns; }

void fs_stat_build_into(uint8_t* img, nova_int size, int64_t mtime_ns,
                         int64_t atime_ns, int64_t ctime_ns, nova_int mode, nova_int kind) {
    NovaFsStat* s = (NovaFsStat*)img;
    s->size = (int64_t)size;
    s->mtime_ns = mtime_ns;
    s->atime_ns = atime_ns;
    s->ctime_ns = ctime_ns;
    s->mode = (int32_t)mode;
    s->kind = (int32_t)kind;
}

/* ─── mkdir / unlink / rmdir / rename / symlink / chmod / copyfile ──── */

nova_int fs_mkdir(const uint8_t* path, nova_int mode) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_mkdir(nova_current_loop(), &fr->req, (const char*)path, (int)mode, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_unlink(const uint8_t* path) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_unlink(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_rmdir(const uint8_t* path) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_rmdir(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_rename(const uint8_t* from, const uint8_t* to) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_rename(nova_current_loop(), &fr->req, (const char*)from, (const char*)to, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_symlink(const uint8_t* target, const uint8_t* link) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_symlink(nova_current_loop(), &fr->req, (const char*)target, (const char*)link, 0, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_chmod(const uint8_t* path, nova_int mode) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_chmod(nova_current_loop(), &fr->req, (const char*)path, (int)mode, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    uv_fs_req_cleanup(&fr->req);
    return _fs_ret(r);
}

nova_int fs_copyfile(const uint8_t* from, const uint8_t* to) {
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

/* ─── realpath → GC string, returned directly (no TLS) ─────────────── */

nova_str fs_realpath_into(const uint8_t* path, nova_int* out_err) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_realpath(nova_current_loop(), &fr->req, (const char*)path, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    nova_str out;
    if (r >= 0) {
        out = _fs_cstr((const char*)fr->req.ptr);
        if (out_err) *out_err = 0;
    } else {
        out.ptr = NULL;
        out.len = 0;
        if (out_err) *out_err = _fs_fail((int)r);
    }
    uv_fs_req_cleanup(&fr->req);
    return out;
}

/* ─── parent-dir fsync (write_atomic step 5) ───────────────────────── */

nova_int fs_fsync_dir(const uint8_t* path) {
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

/* ─── scandir — handle-based iterator (no TLS) ──────────────────────────
 * The handle IS the request pointer, boxed as an address-sized nova_int (same
 * trick as net's opaque void* handles, just numeric because fs's own
 * convention returns a negative-errno failure in the SAME channel — no
 * separate out_err param needed). Ownership: the Nova-side iteration record
 * (read_dir's local loop, today) holds the handle and MUST fs_scandir_close it
 * — done unconditionally in fs.nv after the loop, whether it drained to EOF or
 * exits early, since close is idempotent here. */

nova_int fs_scandir_open(const uint8_t* path) {
    _NovaFsReq* fr = _fs_begin();
    int rc = uv_fs_scandir(nova_current_loop(), &fr->req, (const char*)path, 0, _fs_cb);
    ssize_t r = _fs_wait(fr, rc);
    if (r < 0) {
        nova_int e = _fs_fail((int)r);
        uv_fs_req_cleanup(&fr->req);
        return e;
    }
    return (nova_int)(intptr_t)fr;   /* live req kept by the caller-held handle */
}

nova_int fs_scandir_next(nova_int h) {
    _NovaFsReq* fr = (_NovaFsReq*)(intptr_t)h;
    if (!fr || fr->scandir_done) return 0;
    int rc = uv_fs_scandir_next(&fr->req, &fr->cur_ent);
    if (rc != 0) {
        /* UV_EOF or error → done; release now (idempotent — flagged). */
        uv_fs_req_cleanup(&fr->req);
        fr->scandir_done = 1;
        return 0;
    }
    return 1;   /* fr->cur_ent holds this entry */
}

nova_str fs_scandir_name(nova_int h) {
    _NovaFsReq* fr = (_NovaFsReq*)(intptr_t)h;
    if (!fr) { nova_str z; z.ptr = NULL; z.len = 0; return z; }
    return _fs_cstr(fr->cur_ent.name);
}

nova_int fs_scandir_kind(nova_int h) {
    _NovaFsReq* fr = (_NovaFsReq*)(intptr_t)h;
    if (!fr) return 0;
    switch (fr->cur_ent.type) {
        case UV_DIRENT_DIR:  return 2;   /* KIND_DIR */
        case UV_DIRENT_LINK: return 3;   /* KIND_SYMLINK */
        case UV_DIRENT_FILE: return 1;   /* KIND_FILE */
        default:             return 0;   /* KIND_OTHER / UNKNOWN */
    }
}

void fs_scandir_close(nova_int h) {
    _NovaFsReq* fr = (_NovaFsReq*)(intptr_t)h;
    if (!fr || fr->scandir_done) return;   /* idempotent: no-op past EOF/close */
    uv_fs_req_cleanup(&fr->req);
    fr->scandir_done = 1;
}
