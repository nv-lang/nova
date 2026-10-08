/* SPDX-License-Identifier: MIT OR Apache-2.0
 * nova_rt/process.c — std/os subprocess substrate (Plan 265 Ф.1, D453).
 * See process.h for the design contract; net.c's header comment documents the
 * general park/wake/cancel pattern this file follows (D93).
 *
 * SINGLE-SHOT (unlike TcpStream): `os_process_run` spawns AND waits for exit in
 * ONE call, so the request struct's lifetime is bounded by that one call —
 * same shape as `net_dns_lookup`. It is therefore backed by a plain
 * `nova_alloc` (GC-collectable, rooted by this function's own C stack frame
 * across the park), NOT `nova_alloc_uncollectable` — there is no long-lived
 * Nova-visible handle here to keep pinned between separate calls.
 */

#ifndef NOVA_USE_LIBUV
#  error "Plan 265: NOVA_USE_LIBUV required."
#endif

#include "process.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>
#include <errno.h>
#ifdef _WIN32
#  include <windows.h>
#  include <tlhelp32.h>
#else
#  include <unistd.h>
#endif

/* ─── Cancel-scope helper (same pattern as net.c's _nn2_cancel_scope) ───── */

static inline NovaFiberQueue* _proc_cancel_scope(NovaFiberQueue* scope) {
    mco_coro* rc = mco_running();
    if (rc) {
        NovaSpawnCtxBase* base = (NovaSpawnCtxBase*)mco_get_user_data(rc);
        if (base && base->_nova_parent_scope) {
            return (NovaFiberQueue*)base->_nova_parent_scope;
        }
    }
    return scope;
}

/* ─── Request struct (park state) ────────────────────────────────────────── */

typedef struct NovaProcessReq {
    uv_process_t    proc;
    NovaFiberQueue* wait_scope;
    int             wait_slot;
    nova_atomic_int done;      /* 0/1 completion latch (set in close_cb) */
    nova_atomic_int killing;   /* 0/1 guard: uv_process_kill issued at most once */
    int64_t         exit_status;
    int             term_signal;
} NovaProcessReq;

static nova_bool _proc_ready(void* ctx) {
    NovaProcessReq* req = (NovaProcessReq*)ctx;
    return nova_aint_load(&req->done) != 0;
}

static void _proc_close_cb(uv_handle_t* h) {
    NovaProcessReq* req = (NovaProcessReq*)h->data;
    nova_aint_store(&req->done, 1);
    NovaFiberQueue* sc = req->wait_scope; int sl = req->wait_slot;
    req->wait_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

/* libuv requires uv_close() after exit_cb fires — do that here, and publish
 * the completion latch + wake only once uv_close's own close_cb confirms
 * libuv is fully done with the handle (mirrors net.c's listener close_cb,
 * which wakes at CLOSE time, not at the earlier "event happened" callback). */
static void _proc_exit_cb(uv_process_t* proc, int64_t exit_status, int term_signal) {
    NovaProcessReq* req = (NovaProcessReq*)proc->data;
    req->exit_status = exit_status;
    req->term_signal = term_signal;
    uv_close((uv_handle_t*)proc, _proc_close_cb);
}

/* Cancel-flow stop_cb (D93 Ф.8 contract): best-effort-kill, then ASYNC — the
 * real wake happens later, once the (now-triggered) exit_cb/close_cb chain
 * actually completes. Guarded so a cancel racing a natural exit never issues
 * a kill against an already-exited/closing handle twice. */
static NovaStopMode _proc_stop_cb(void* handle) {
    NovaProcessReq* req = (NovaProcessReq*)handle;
    int32_t was = __atomic_exchange_n((volatile int32_t*)&req->killing, 1, __ATOMIC_ACQ_REL);
    if (!was) {
        uv_process_kill(&req->proc, SIGKILL);
    }
    return NOVA_STOP_ASYNC;
}

/* ─── argv/env blob helpers ───────────────────────────────────────────────
 * Nova crosses program/args/env as NUL-separated byte blobs (D453 §2,
 * byte-first — same convention os_env.h uses for env keys/values). `count`
 * is explicit (not inferred from a double-NUL terminator). */

static char* _proc_dupz(const uint8_t* s, nova_int len) {
    char* p = (char*)malloc((size_t)len + 1);
    if (len > 0 && s) memcpy(p, s, (size_t)len);
    p[len < 0 ? 0 : len] = '\0';
    return p;
}

/* Split `count` NUL-separated entries out of blob[0..blob_len) into
 * out[0..count), each a freshly malloc'd NUL-terminated C string. */
static void _proc_split_into(const uint8_t* blob, nova_int blob_len,
                              nova_int count, char** out) {
    nova_int pos = 0;
    for (nova_int i = 0; i < count; i++) {
        nova_int start = pos;
        while (pos < blob_len && blob[pos] != 0) pos++;
        out[i] = _proc_dupz(blob + start, pos - start);
        if (pos < blob_len) pos++;  /* skip the separating NUL */
    }
}

/* Free `n` string ELEMENTS of `arr` — NOT `arr` itself. `arr` may be an
 * interior pointer (e.g. `&args[1]`, a sub-view into a larger malloc'd
 * array) — only a pointer `malloc` itself returned may ever be passed to
 * `free()`; freeing an interior pointer is heap corruption (found the hard
 * way: an earlier version of this helper also did `free(arr)`, which
 * crashed with STATUS_HEAP_CORRUPTION on Windows the moment `_proc_split_
 * into` filled more than zero args — see D453 §Реализация). Callers free
 * the true array pointer (`args`, `envp`) separately, exactly once. */
static void _proc_free_str_elems(char** arr, nova_int n) {
    if (!arr) return;
    for (nova_int i = 0; i < n; i++) free(arr[i]);
}

/* ─── os_process_run — spawn, park until exit, report (rc, exit_code) ───────── */

nova_int os_process_run(const uint8_t* program, nova_int program_len,
                      const uint8_t* argv_blob, nova_int argv_blob_len, nova_int argc,
                      const uint8_t* env_blob, nova_int env_blob_len, nova_int envc,
                      nova_bool use_env,
                      const uint8_t* cwd, nova_int cwd_len,
                      nova_int* out_exit_code) {
    if (out_exit_code) *out_exit_code = 0;

    uv_loop_t* loop = nova_current_loop();
    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/os: process run outside scope\n"); abort(); }

    NovaFiberQueue* cancel_sc = _proc_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) return NOVA_PROCESS_CANCELLED;

    /* Build args[] : args[0] = program, args[1..argc] = split(argv_blob),
     * args[argc+1] = NULL (uv_process_options_t.args convention). */
    char* progz = _proc_dupz(program, program_len);
    char** args = (char**)malloc(sizeof(char*) * (size_t)(argc + 2));
    args[0] = progz;
    _proc_split_into(argv_blob, argv_blob_len, argc, &args[1]);
    args[argc + 1] = NULL;

    char** envp = NULL;
    if (use_env) {
        envp = (char**)malloc(sizeof(char*) * (size_t)(envc + 1));
        _proc_split_into(env_blob, env_blob_len, envc, envp);
        envp[envc] = NULL;
    }

    char* cwdz = (cwd_len > 0) ? _proc_dupz(cwd, cwd_len) : NULL;

    NovaProcessReq* req = (NovaProcessReq*)nova_alloc(sizeof(NovaProcessReq));
    memset(req, 0, sizeof(*req));
    req->proc.data = req;
    nova_aint_init(&req->done, 0);
    nova_aint_init(&req->killing, 0);

    uv_process_options_t opts;
    memset(&opts, 0, sizeof(opts));
    opts.exit_cb = _proc_exit_cb;
    opts.file    = progz;
    opts.args    = args;
    opts.env     = envp;   /* NULL = inherit parent's environment (libuv default) */
    opts.cwd     = cwdz;   /* NULL = inherit parent's cwd */
    /* stdio_count left at 0: libuv redirects the child's stdin/stdout/stderr
     * to the OS null device by default (verified against src/unix/process.c —
     * NOT inherited from the parent). Deliberate for this wave (D453 §4): no
     * stdio redirection yet, and this default never blocks on a full pipe or
     * leaks into the parent's own descriptors. */

    req->wait_scope = scope;
    req->wait_slot  = slot;
    nova_sched_register_pending(scope, slot, req, _proc_stop_cb);

    int rc = uv_spawn(loop, &req->proc, &opts);

    /* uv_spawn (posix_spawn/fork+exec on Unix, CreateProcess on Windows) has
     * consumed file/args/env/cwd SYNCHRONOUSLY by the time it returns — libuv
     * does not retain these pointers past the call. Safe to free now. */
    free(progz);
    free(cwdz);
    _proc_free_str_elems(&args[1], argc);
    free(args);
    _proc_free_str_elems(envp, use_env ? envc : 0);
    free(envp);

    if (rc != 0) {
        nova_sched_unregister_pending(scope, slot);
        return (nova_int)rc;  /* -errno-compatible spawn failure */
    }

    nova_sched_park_until(scope, slot, _proc_ready, req);
    nova_sched_unregister_pending(scope, slot);

    /* Cancellation wins regardless of what the (by-now-fired) exit_cb
     * actually reported — same accepted trade-off as net.c's post-park
     * cancel_requested check (a genuine "finished right as we cancelled"
     * race reports Cancelled, not success; benign per that precedent).
     *
     * Checks BOTH signals, same as net.c's net_tcp_connect (cancel_requested
     * AND the handle's own stage==CLOSED) — found the hard way (D453): for a
     * DIRECT `supervised(timeout:)` body statement (no `spawn`), cancel
     * delivery goes through `nova_sched_cancel_pending_slot`, which by
     * documented design does NOT set `scope->cancel_requested` (it belongs
     * to a DIFFERENT scope — the supervised block's own, not the ambient one
     * process_run's `scope` snapshot resolves to); it only fires the
     * REGISTERED stop_cb. `req->killing` is that local signal — set by
     * `_proc_stop_cb` if and only if it actually killed THIS process, so it
     * is authoritative regardless of which scope's flag did or didn't get
     * touched. */
    if (nova_abool_load(&cancel_sc->cancel_requested) || nova_aint_load(&req->killing) != 0) {
        return NOVA_PROCESS_CANCELLED;
    }

    nova_int code = (nova_int)req->exit_status;
    if (req->term_signal != 0) code = 128 + req->term_signal;
    if (out_exit_code) *out_exit_code = code;
    return 0;
}


/* ===========================================================================
 * Plan 294 F.1 (D492): live child handle + piped stdio
 *
 * Lifetime protocol (net.c's intrusive-refcount idiom, [M-boehm-...] variant b):
 *   - NovaProcPipe: refcount starts at 2 = "existence" unit (released by the
 *     uv_close callback) + "Nova owner" unit (released exactly once by
 *     proc_pipe_close / proc_child_release). Every read/write holds one more
 *     unit across park + wake. The struct is freed by whoever drops the last.
 *   - NovaProcChild: same shape; existence unit released by the process close
 *     callback (fired from exit_cb), Nova unit by proc_child_release.
 * Structs are nova_alloc_uncollectable: they live between separate Nova calls
 * and libuv keeps interior pointers to them.
 * =========================================================================== */

#define PP_IDLE    0
#define PP_CLOSING 1
#define PP_CLOSED  2

/* libuv code -> NEGATIVE POSIX errno number, identical on Windows and POSIX
 * (IoError.from_os projects POSIX numbers; raw libuv Windows codes are -4058...). */
static nova_int _proc_neg_errno(int uv) {
    switch (uv) {
        case UV_EPERM:     return -1;
        case UV_ENOENT:    return -2;
        case UV_EINTR:     return -4;
        case UV_ECANCELED: return -4;   /* surfaces as Interrupted, like D453 */
        case UV_EIO:       return -5;
        case UV_EAGAIN:    return -11;
        case UV_ENOMEM:    return -12;
        case UV_EACCES:    return -13;
        case UV_EEXIST:    return -17;
        case UV_ENOTDIR:   return -20;
        case UV_EISDIR:    return -21;
        case UV_EINVAL:    return -22;
        case UV_EPIPE:     return -32;
        case UV_EOF:       return -32;  /* only reached on the WRITE path (Windows ERROR_BROKEN_PIPE) */
        default:           return uv < 0 ? uv : -uv;
    }
}

typedef struct NovaProcPipe {
    uv_pipe_t       handle;        /* must be first (uv_close compat) */
    uv_loop_t*      loop;
    nova_atomic_int stage;         /* PP_IDLE / PP_CLOSING / PP_CLOSED */
    nova_atomic_int refcount;
    nova_atomic_int user_release_done;

    /* read slot */
    NovaFiberQueue* read_scope;
    int             read_slot;
    uint8_t*        read_ptr;
    nova_int        read_cap;
    nova_int        read_n;
    int             read_err;      /* UV code */
    int             read_eof;
    nova_atomic_int read_done;

    /* write slot (independent: stdin writer and stdout reader are different fibers) */
    uv_write_t      write_req;
    NovaFiberQueue* write_scope;
    int             write_slot;
    nova_int        write_n;
    int             write_err;
    nova_atomic_int write_done;
} NovaProcPipe;

static inline void _pp_acquire(NovaProcPipe* p) { (void)nova_aint_inc(&p->refcount); }
static inline void _pp_release(NovaProcPipe* p) {
    if (nova_aint_fetch_sub_release(&p->refcount) == 1) {
        nova_thread_fence_acquire();
        nova_free_uncollectable(p);
    }
}

static nova_bool _pp_read_ready(void* ctx) {
    NovaProcPipe* p = (NovaProcPipe*)ctx;
    return nova_aint_load(&p->read_done) != 0 || nova_aint_load(&p->stage) >= PP_CLOSING;
}
static nova_bool _pp_write_ready(void* ctx) {
    NovaProcPipe* p = (NovaProcPipe*)ctx;
    return nova_aint_load(&p->write_done) != 0 || nova_aint_load(&p->stage) >= PP_CLOSING;
}

static void _pp_close_cb(uv_handle_t* h) {
    NovaProcPipe* p = (NovaProcPipe*)h->data;
    nova_aint_store(&p->stage, PP_CLOSED);
    if (p->read_scope)  { NovaFiberQueue* sc = p->read_scope;  int sl = p->read_slot;  p->read_scope = NULL;  nova_sched_wake(sc, sl); }
    if (p->write_scope) { NovaFiberQueue* sc = p->write_scope; int sl = p->write_slot; p->write_scope = NULL; nova_sched_wake(sc, sl); }
    _pp_release(p);   /* existence unit */
}

/* Scope-cancel for a parked read/write: close the pipe (a cancelled stream is dead,
 * like a cancelled TcpStream); the close callback wakes the parked fiber. */
static NovaStopMode _pp_stop_cb(void* handle) {
    NovaProcPipe* p = (NovaProcPipe*)handle;
    int32_t expected = PP_IDLE;
    if (__atomic_compare_exchange_n((volatile int32_t*)&p->stage, &expected, PP_CLOSING, 0,
                                    __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        nova_loop_defer_close(p->loop, (uv_handle_t*)&p->handle, _pp_close_cb);
    }
    return NOVA_STOP_ASYNC;
}

static NovaProcPipe* _pp_new(uv_loop_t* loop) {
    NovaProcPipe* p = (NovaProcPipe*)nova_alloc_uncollectable(sizeof(NovaProcPipe));
    memset(p, 0, sizeof(*p));
    nova_aint_init(&p->stage, PP_IDLE);
    nova_aint_init(&p->refcount, 2);          /* existence + Nova owner */
    nova_aint_init(&p->user_release_done, 0);
    nova_aint_init(&p->read_done, 0);
    nova_aint_init(&p->write_done, 0);
    p->loop = loop;
    p->handle.data = p;
    p->write_req.data = p;
    return p;
}

/* Drop the Nova owner unit exactly once and close the handle if still open. */
static void _pp_user_close(NovaProcPipe* p) {
    int32_t expected = PP_IDLE;
    if (__atomic_compare_exchange_n((volatile int32_t*)&p->stage, &expected, PP_CLOSING, 0,
                                    __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        nova_loop_defer_close(p->loop, (uv_handle_t*)&p->handle, _pp_close_cb);
    }
    int32_t zero = 0;
    if (nova_aint_cas(&p->user_release_done, &zero, 1)) _pp_release(p);
}

void proc_pipe_close(void* pv) { if (pv) _pp_user_close((NovaProcPipe*)pv); }

static void _pp_alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf) {
    (void)suggested;
    NovaProcPipe* p = (NovaProcPipe*)h->data;
    buf->base = (char*)p->read_ptr;
    buf->len  = (unsigned long)p->read_cap;
}

/* PULL: take exactly one chunk, then stop reading until the next proc_pipe_read.
 * The OS pipe fills and blocks the child: that is the back pressure. */
static void _pp_read_cb(uv_stream_t* stream, ssize_t nread, const uv_buf_t* buf) {
    (void)buf;
    NovaProcPipe* p = (NovaProcPipe*)stream->data;
    if (nread == 0) return;  /* spurious wake-up: stay parked */
    uv_read_stop(stream);
    if (nread == UV_EOF)   { p->read_n = 0; p->read_eof = 1; p->read_err = 0; }
    else if (nread < 0)    { p->read_n = 0; p->read_err = (int)nread; }
    else                   { p->read_n = (nova_int)nread; p->read_err = 0; }
    nova_aint_store(&p->read_done, 1);
    NovaFiberQueue* sc = p->read_scope; int sl = p->read_slot;
    p->read_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

static void _pp_do_read_start_deferred(void* argp) {
    NovaProcPipe* p = (NovaProcPipe*)argp;
    int rc = uv_read_start((uv_stream_t*)&p->handle, _pp_alloc_cb, _pp_read_cb);
    if (rc != 0) {
        p->read_n = 0; p->read_err = rc;
        nova_aint_store(&p->read_done, 1);
        NovaFiberQueue* sc = p->read_scope; int sl = p->read_slot;
        p->read_scope = NULL;
        if (sc) nova_sched_wake(sc, sl);
    }
}

nova_int proc_pipe_read(void* pv, uint8_t* buf, nova_int cap) {
    NovaProcPipe* p = (NovaProcPipe*)pv;
    nova_int result;
    _pp_acquire(p);

    if (nova_aint_load(&p->stage) >= PP_CLOSING) { result = -4; goto out; }
    if (cap <= 0) { result = 0; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/os: pipe read outside scope\n"); abort(); }
    NovaFiberQueue* cancel_sc = _proc_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = -4; goto out; }

    p->read_ptr = buf; p->read_cap = cap; p->read_n = 0; p->read_eof = 0; p->read_err = 0;
    nova_aint_store(&p->read_done, 0);
    p->read_scope = scope; p->read_slot = slot;
    nova_sched_register_pending(scope, slot, p, _pp_stop_cb);

    if (nova_current_loop() == p->loop) {
        int rc = uv_read_start((uv_stream_t*)&p->handle, _pp_alloc_cb, _pp_read_cb);
        if (rc != 0) {
            nova_sched_unregister_pending(scope, slot);
            p->read_scope = NULL;
            result = _proc_neg_errno(rc); goto out;
        }
    } else {
        nova_loop_defer_call(p->loop, _pp_do_read_start_deferred, p);
    }

    nova_sched_park_until(scope, slot, _pp_read_ready, p);
    nova_sched_unregister_pending(scope, slot);
    p->read_ptr = NULL;   /* do not root the caller's buffer through an uncollectable struct */

    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = -4; goto out; }
    if (nova_aint_load(&p->stage) >= PP_CLOSING)       { result = -4; goto out; }
    if (p->read_err != 0) { result = _proc_neg_errno(p->read_err); goto out; }
    if (p->read_eof)      { result = 0; goto out; }
    result = p->read_n;
out:
    _pp_release(p);
    return result;
}

static void _pp_write_cb(uv_write_t* req, int status) {
    NovaProcPipe* p = (NovaProcPipe*)req->data;
    p->write_err = status;
    nova_aint_store(&p->write_done, 1);
    NovaFiberQueue* sc = p->write_scope; int sl = p->write_slot;
    p->write_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
}

typedef struct { NovaProcPipe* p; uv_buf_t ubuf; } ProcWriteIssueCtx;

static void _pp_write_issue(ProcWriteIssueCtx* c, int* out_rc) {
    *out_rc = uv_write(&c->p->write_req, (uv_stream_t*)&c->p->handle, &c->ubuf, 1, _pp_write_cb);
}
static void _pp_do_write_deferred(void* argp) {
    ProcWriteIssueCtx* c = (ProcWriteIssueCtx*)argp;
    NovaProcPipe* p = c->p;
    int rc;
    _pp_write_issue(c, &rc);
    if (rc != 0) {
        p->write_err = rc;
        nova_aint_store(&p->write_done, 1);
        NovaFiberQueue* sc = p->write_scope; int sl = p->write_slot;
        p->write_scope = NULL;
        if (sc) nova_sched_wake(sc, sl);
    }
}

nova_int proc_pipe_write(void* pv, const uint8_t* buf, nova_int len) {
    NovaProcPipe* p = (NovaProcPipe*)pv;
    nova_int result;
    _pp_acquire(p);

    if (nova_aint_load(&p->stage) >= PP_CLOSING) { result = -4; goto out; }
    if (len <= 0) { result = 0; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/os: pipe write outside scope\n"); abort(); }
    NovaFiberQueue* cancel_sc = _proc_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = -4; goto out; }

    uv_buf_t ubuf = uv_buf_init((char*)(uintptr_t)buf, (unsigned int)len);
    ProcWriteIssueCtx wctx = { p, ubuf };
    memset(&p->write_req, 0, sizeof(p->write_req));
    p->write_req.data = p;
    p->write_n = len; p->write_err = 0;
    nova_aint_store(&p->write_done, 0);
    p->write_scope = scope; p->write_slot = slot;
    nova_sched_register_pending(scope, slot, p, _pp_stop_cb);

    if (nova_current_loop() == p->loop) {
        int rc;
        _pp_write_issue(&wctx, &rc);
        if (rc != 0) {
            nova_sched_unregister_pending(scope, slot);
            p->write_scope = NULL;
            result = _proc_neg_errno(rc); goto out;
        }
    } else {
        nova_loop_defer_call(p->loop, _pp_do_write_deferred, &wctx);
    }

    nova_sched_park_until(scope, slot, _pp_write_ready, p);
    nova_sched_unregister_pending(scope, slot);
    memset(&p->write_req, 0, sizeof(p->write_req));   /* drop the pointer to the caller's buffer */

    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = -4; goto out; }
    if (nova_aint_load(&p->stage) >= PP_CLOSING)       { result = -4; goto out; }
    if (p->write_err != 0) { result = _proc_neg_errno(p->write_err); goto out; }
    result = p->write_n;
out:
    _pp_release(p);
    return result;
}

/* --- Child handle ------------------------------------------------------- */

typedef struct NovaProcChild {
    uv_process_t    proc;          /* must be first */
    uv_loop_t*      loop;
    nova_atomic_int refcount;      /* existence (close_cb) + Nova owner + in-flight wait */
    nova_atomic_int exited;        /* set in exit_cb */
    nova_atomic_int killing;       /* a kill was issued on purpose (cancel / release) */
    nova_atomic_int release_done;
    int64_t         exit_status;
    int             term_signal;
    int             pid;
    NovaFiberQueue* wait_scope;
    int             wait_slot;
    NovaProcPipe*   pipes[3];
    nova_atomic_int taken[3];
    int             group;         /* Tree.Group: every kill hits the whole tree */
#ifdef _WIN32
    HANDLE          job;           /* Job Object holding the tree (group only; may be NULL) */
#endif
} NovaProcChild;

static inline void _pc_acquire(NovaProcChild* c) { (void)nova_aint_inc(&c->refcount); }
static inline void _pc_release(NovaProcChild* c) {
    if (nova_aint_fetch_sub_release(&c->refcount) == 1) {
        nova_thread_fence_acquire();
        nova_free_uncollectable(c);
    }
}

static nova_bool _pc_exited(void* ctx) {
    return nova_aint_load(&((NovaProcChild*)ctx)->exited) != 0;
}

static void _pc_close_cb(uv_handle_t* h) {
    NovaProcChild* c = (NovaProcChild*)h->data;
    _pc_release(c);   /* existence unit */
}

static void _pc_exit_cb(uv_process_t* proc, int64_t exit_status, int term_signal) {
    NovaProcChild* c = (NovaProcChild*)proc->data;
    c->exit_status = exit_status;
    c->term_signal = term_signal;
    nova_aint_store(&c->exited, 1);
    NovaFiberQueue* sc = c->wait_scope; int sl = c->wait_slot;
    c->wait_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
    uv_close((uv_handle_t*)proc, _pc_close_cb);
}

/* Deliver `sig` to the child, or to its whole tree with Tree.Group. 0 also when
 * there is nothing left to signal; < 0 = -errno or NOVA_PROC_UNSUPPORTED. */
static nova_int _pc_send(NovaProcChild* c, int sig) {
#ifdef _WIN32
    if (sig != 9 && sig != 15) return NOVA_PROC_UNSUPPORTED;   /* no signals on Windows */
    if (c->job) { TerminateJobObject(c->job, 1); return 0; }
    if (nova_aint_load(&c->exited) != 0) return 0;
    int rc = uv_process_kill(&c->proc, SIGKILL);
    return (rc == 0 || rc == UV_ESRCH) ? 0 : _proc_neg_errno(rc);
#else
    int r;
    if (c->group) {
        r = kill(-(pid_t)c->pid, sig);     /* the child is its group's leader (setsid) */
    } else {
        if (nova_aint_load(&c->exited) != 0) return 0;
        r = kill((pid_t)c->pid, sig);
    }
    if (r == 0 || errno == ESRCH) return 0;
    return -(nova_int)errno;
#endif
}

/* Scope cancellation / release path: SIGKILL, at most once; the `killing` marker is
 * what lets wait() tell "cancelled" from "died by itself". */
static void _pc_kill(NovaProcChild* c) {
    int32_t was = __atomic_exchange_n((volatile int32_t*)&c->killing, 1, __ATOMIC_ACQ_REL);
    if (!was && nova_aint_load(&c->exited) == 0) (void)_pc_send(c, 9);
}

static int64_t _proc_now_ms(void) { return (int64_t)(uv_hrtime() / 1000000ULL); }

#ifdef _WIN32
/* Tree.Group on Windows: put the just-spawned child into its own Job Object.
 * Assigned right after uv_spawn: probe (b) of plan 294 measured no escapes for an
 * immediate assignment (see docs/dev/simplifications.md, "Windows Tree.Group").
 * KILL_ON_JOB_CLOSE: the tree dies with our handle even if the parent crashes. */
static void _pc_assign_job(NovaProcChild* c) {
    HANDLE j = CreateJobObjectW(NULL, NULL);
    if (!j) return;
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION li;
    memset(&li, 0, sizeof li);
    li.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if (!SetInformationJobObject(j, JobObjectExtendedLimitInformation, &li, sizeof li) ||
        !AssignProcessToJobObject(j, c->proc.process_handle)) {
        CloseHandle(j);
        return;     /* degrades to a single-process kill (pre-Windows-8 nested-job refusal) */
    }
    c->job = j;
}
#endif

static NovaStopMode _pc_stop_cb(void* handle) {
    _pc_kill((NovaProcChild*)handle);
    return NOVA_STOP_ASYNC;   /* the wake comes from exit_cb */
}

void* proc_spawn(const uint8_t* program, nova_int program_len,
                 const uint8_t* argv_blob, nova_int argv_blob_len, nova_int argc,
                 const uint8_t* env_blob, nova_int env_blob_len, nova_int envc,
                 nova_bool use_env,
                 const uint8_t* cwd, nova_int cwd_len,
                 nova_int stdio_modes, nova_int* out_err) {
    if (out_err) *out_err = 0;
    uv_loop_t* loop = nova_current_loop();

    char* progz = _proc_dupz(program, program_len);
    char** args = (char**)malloc(sizeof(char*) * (size_t)(argc + 2));
    args[0] = progz;
    _proc_split_into(argv_blob, argv_blob_len, argc, &args[1]);
    args[argc + 1] = NULL;
    char** envp = NULL;
    if (use_env) {
        envp = (char**)malloc(sizeof(char*) * (size_t)(envc + 1));
        _proc_split_into(env_blob, env_blob_len, envc, envp);
        envp[envc] = NULL;
    }
    char* cwdz = (cwd_len > 0) ? _proc_dupz(cwd, cwd_len) : NULL;

    NovaProcChild* c = (NovaProcChild*)nova_alloc_uncollectable(sizeof(NovaProcChild));
    memset(c, 0, sizeof(*c));
    nova_aint_init(&c->refcount, 2);   /* existence + Nova owner */
    nova_aint_init(&c->exited, 0);
    nova_aint_init(&c->killing, 0);
    nova_aint_init(&c->release_done, 0);
    for (int i = 0; i < 3; i++) nova_aint_init(&c->taken[i], 0);
    c->loop = loop;
    c->proc.data = c;
    c->group = (stdio_modes & NOVA_STDIO_TREE_GROUP) != 0;

    uv_stdio_container_t io[3];
    for (int i = 0; i < 3; i++) {
        int mode = (int)((stdio_modes >> (2 * i)) & 3);
        if (mode == NOVA_STDIO_PIPED) {
            NovaProcPipe* p = _pp_new(loop);
            uv_pipe_init(loop, &p->handle, 0);
            c->pipes[i] = p;
            io[i].flags = (uv_stdio_flags)(UV_CREATE_PIPE | (i == 0 ? UV_READABLE_PIPE : UV_WRITABLE_PIPE));
            io[i].data.stream = (uv_stream_t*)&p->handle;
        } else if (mode == NOVA_STDIO_INHERIT) {
            io[i].flags = UV_INHERIT_FD;
            io[i].data.fd = i;
        } else {
            io[i].flags = UV_IGNORE;
        }
    }

    uv_process_options_t opts;
    memset(&opts, 0, sizeof(opts));
    opts.exit_cb = _pc_exit_cb;
    opts.file = progz;
    opts.args = args;
    opts.env = envp;
    opts.cwd = cwdz;
    opts.stdio_count = 3;
    opts.stdio = io;
#ifndef _WIN32
    /* Tree.Group (POSIX): UV_PROCESS_DETACHED = setsid() in the child, so its pid is
     * also its process group id and kill(-pid) reaches every descendant. The child
     * loses the parent's controlling terminal (plan 294, risk R4). */
    if (c->group) opts.flags |= UV_PROCESS_DETACHED;
#endif

    int rc = uv_spawn(loop, &c->proc, &opts);

    free(progz); free(cwdz);
    _proc_free_str_elems(&args[1], argc);
    free(args);
    _proc_free_str_elems(envp, use_env ? envc : 0);
    free(envp);

    if (rc != 0) {
        /* Spawn failed: nothing is running. Close the (initialised, never started)
         * handles; the close callbacks release the existence units. The Nova
         * units are dropped right here since no Nova value will ever exist. */
        for (int i = 0; i < 3; i++) if (c->pipes[i]) {
            NovaProcPipe* p = c->pipes[i];
            nova_aint_store(&p->stage, PP_CLOSING);
            uv_close((uv_handle_t*)&p->handle, _pp_close_cb);
            int32_t zero = 0;
            if (nova_aint_cas(&p->user_release_done, &zero, 1)) _pp_release(p);
        }
        nova_aint_store(&c->exited, 1);
        uv_close((uv_handle_t*)&c->proc, _pc_close_cb);
        _pc_release(c);   /* Nova unit */
        if (out_err) *out_err = _proc_neg_errno(rc);
        return NULL;
    }
    c->pid = c->proc.pid;
#ifdef _WIN32
    if (c->group) _pc_assign_job(c);
#endif
    return c;
}

void* proc_child_pipe(void* cv, nova_int which) {
    NovaProcChild* c = (NovaProcChild*)cv;
    if (which < 0 || which > 2 || !c->pipes[which]) return NULL;
    int32_t zero = 0;
    if (!nova_aint_cas(&c->taken[which], &zero, 1)) return NULL;   /* handed out once */
    return c->pipes[which];
}

nova_int proc_child_pid(void* cv) { return (nova_int)((NovaProcChild*)cv)->pid; }

nova_int proc_child_try_wait(void* cv, nova_int* out_code, nova_int* out_signal) {
    NovaProcChild* c = (NovaProcChild*)cv;
    if (nova_aint_load(&c->exited) == 0) return 1;
    if (out_code) *out_code = (nova_int)c->exit_status;
    if (out_signal) *out_signal = (nova_int)c->term_signal;
    return 0;
}

nova_int proc_child_wait(void* cv, nova_int* out_code, nova_int* out_signal) {
    NovaProcChild* c = (NovaProcChild*)cv;
    nova_int result;
    _pc_acquire(c);
    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/os: child wait outside scope\n"); abort(); }
    NovaFiberQueue* cancel_sc = _proc_cancel_scope(scope);

    if (nova_aint_load(&c->exited) == 0) {
        if (nova_abool_load(&cancel_sc->cancel_requested)) { _pc_kill(c); result = NOVA_PROCESS_CANCELLED; goto out; }
        c->wait_scope = scope; c->wait_slot = slot;
        nova_sched_register_pending(scope, slot, c, _pc_stop_cb);
        nova_sched_park_until(scope, slot, _pc_exited, c);
        nova_sched_unregister_pending(scope, slot);
        c->wait_scope = NULL;
        /* Same two signals as os_process_run: scope flag, or our own kill marker
         * (a direct supervised(timeout:) body statement never sets the flag). */
        if (nova_abool_load(&cancel_sc->cancel_requested) || nova_aint_load(&c->killing) != 0) {
            result = NOVA_PROCESS_CANCELLED; goto out;
        }
    }
    if (out_code) *out_code = (nova_int)c->exit_status;
    if (out_signal) *out_signal = (nova_int)c->term_signal;
    result = 0;
out:
    _pc_release(c);
    return result;
}

nova_int proc_child_kill(void* cv, nova_int sig) {
    return _pc_send((NovaProcChild*)cv, (int)sig);
}

nova_int proc_child_wait_ms(void* cv, nova_int ms, nova_int* out_code, nova_int* out_signal) {
    NovaProcChild* c = (NovaProcChild*)cv;
    int64_t deadline = _proc_now_ms() + (ms > 0 ? (int64_t)ms : 0);
    for (;;) {
        if (nova_aint_load(&c->exited) != 0) {
            if (out_code) *out_code = (nova_int)c->exit_status;
            if (out_signal) *out_signal = (nova_int)c->term_signal;
            return 0;
        }
        int64_t left = deadline - _proc_now_ms();
        if (left <= 0) return 1;
        /* A fiber sleep: parks (never blocks the OS thread) and unwinds on scope cancellation. */
        (void)time_sleep_ms((nova_int)(left < 5 ? left : 5));
    }
}

/* Child.cleanup: Terminate, a short grace, Kill (the tree with Tree.Group), then wait
 * until the OS has really reaped the process — callers (and kill_pid) must find it gone. */
static void _pc_reap(NovaProcChild* c) {
    int alive = nova_aint_load(&c->exited) == 0;
    if (!alive && !c->group) return;
    if (alive) {
        (void)_pc_send(c, 15);
        NovaFiberQueue* scope = _nova_active_scope;
        int64_t deadline = _proc_now_ms() + NOVA_PROC_RELEASE_GRACE_MS;
        while (nova_aint_load(&c->exited) == 0 && _proc_now_ms() < deadline) {
            /* An unshielded, already cancelled scope cannot sleep (the sleep would throw
             * in the middle of a cleanup): skip the grace and go straight to Kill. */
            if (!scope || !mco_running()) break;
            if (nova_cancel_mask_active() == 0 &&
                nova_abool_load(&_proc_cancel_scope(scope)->cancel_requested)) break;
            (void)time_sleep_ms(2);
        }
    }
    (void)_pc_send(c, 9);          /* leader still alive, or grandchildren that outlived it */
    if (alive && nova_aint_load(&c->exited) == 0) {
        NovaFiberQueue* scope = _nova_active_scope;
        int slot = _nova_active_slot;
        if (scope) {
            c->wait_scope = scope; c->wait_slot = slot;
            nova_sched_park_until(scope, slot, _pc_exited, c);   /* no stop_cb: not cancellable */
            c->wait_scope = NULL;
        }
    }
}

void proc_child_release(void* cv) {
    NovaProcChild* c = (NovaProcChild*)cv;
    if (!c) return;
    int32_t zero = 0;
    if (!nova_aint_cas(&c->release_done, &zero, 1)) return;
    _pc_reap(c);                         /* no orphan survives its owner (D492 rule 6) */
    for (int i = 0; i < 3; i++) {
        int32_t z = 0;
        if (c->pipes[i] && nova_aint_cas(&c->taken[i], &z, 1)) _pp_user_close(c->pipes[i]);
    }
#ifdef _WIN32
    if (c->job) { CloseHandle(c->job); c->job = NULL; }
#endif
    _pc_release(c);                      /* Nova unit */
}

/* ─── kill_pid: a process this program holds no Child for ─────────────────── */

#ifdef _WIN32
static nova_int _win_terminate_pid(DWORD pid) {
    HANDLE h = OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
    if (!h) {
        DWORD e = GetLastError();
        if (e == ERROR_INVALID_PARAMETER) return NOVA_PROC_NO_SUCH;
        return e == ERROR_ACCESS_DENIED ? -1 : -5;
    }
    nova_int r = 0;
    if (!TerminateProcess(h, 1)) {
        DWORD code = 0;
        if (GetExitCodeProcess(h, &code) && code != STILL_ACTIVE) r = NOVA_PROC_NO_SUCH;
        else r = -1;
    }
    CloseHandle(h);
    return r;
}
#endif

nova_int proc_kill_pid(nova_int pid, nova_int sig, nova_bool tree) {
    if (pid <= 0) return -22;     /* never "my own group" / "every process" */
#ifdef _WIN32
    if (sig != 9 && sig != 15) return NOVA_PROC_UNSUPPORTED;
    if (!tree) return _win_terminate_pid((DWORD)pid);
    /* One snapshot of the process table, then every descendant of `pid` (parent-id links) and
     * `pid` itself. A parent id can be stale (pid reuse, plan 294 R6): best effort, documented. */
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snap == INVALID_HANDLE_VALUE) return _win_terminate_pid((DWORD)pid);
    size_t cap = 256, n = 0;
    DWORD* pids = (DWORD*)malloc(cap * sizeof(DWORD));
    DWORD* ppids = (DWORD*)malloc(cap * sizeof(DWORD));
    PROCESSENTRY32W e; e.dwSize = sizeof e;
    for (BOOL ok = Process32FirstW(snap, &e); ok; ok = Process32NextW(snap, &e)) {
        if (n == cap) {
            cap *= 2;
            pids = (DWORD*)realloc(pids, cap * sizeof(DWORD));
            ppids = (DWORD*)realloc(ppids, cap * sizeof(DWORD));
        }
        pids[n] = e.th32ProcessID; ppids[n] = e.th32ParentProcessID; n++;
    }
    CloseHandle(snap);
    DWORD* set = (DWORD*)malloc((n + 1) * sizeof(DWORD));
    size_t m = 0;
    set[m++] = (DWORD)pid;
    for (size_t i = 0; i < m; i++)                 /* breadth first; m grows while we walk */
        for (size_t j = 0; j < n; j++)
            if (ppids[j] == set[i] && pids[j] != set[i]) {
                int seen = 0;
                for (size_t k = 0; k < m; k++) if (set[k] == pids[j]) { seen = 1; break; }
                if (!seen && m <= n) set[m++] = pids[j];
            }
    nova_int result = NOVA_PROC_NO_SUCH;
    for (size_t i = m; i-- > 0;) {                 /* descendants first, the root last */
        nova_int r = _win_terminate_pid(set[i]);
        if (r == 0) result = 0;
        else if (r != NOVA_PROC_NO_SUCH && result != 0) result = r;
    }
    free(pids); free(ppids); free(set);
    return result;
#else
    int r = tree ? kill(-(pid_t)pid, (int)sig) : kill((pid_t)pid, (int)sig);
    if (r == 0) return 0;
    return errno == ESRCH ? NOVA_PROC_NO_SUCH : -(nova_int)errno;
#endif
}
