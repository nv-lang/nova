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
#  include <winternl.h>
#  include <io.h>
#else
#  include <unistd.h>
#endif

/* ─── Cancel-scope helper (same pattern as net.c's _nn2_cancel_scope) ───── */

/* Windows: every process creation of this file holds this lock — uv_spawn SHARED (they may run
 * side by side), the pseudo terminal's CreateProcess EXCLUSIVE, because it briefly clears this
 * process's "ignore Ctrl-C" flag, which CreateProcess copies into every child (plan 294 F.4,
 * D492). Holding it shared here keeps a concurrent Child.start / Command.run from inheriting the
 * cleared value. */
#ifdef _WIN32
static SRWLOCK _proc_spawn_lock = SRWLOCK_INIT;
#  define PROC_SPAWN_SHARED_BEGIN() AcquireSRWLockShared(&_proc_spawn_lock)
#  define PROC_SPAWN_SHARED_END()   ReleaseSRWLockShared(&_proc_spawn_lock)
#else
#  define PROC_SPAWN_SHARED_BEGIN() ((void)0)
#  define PROC_SPAWN_SHARED_END()   ((void)0)
#endif

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

    PROC_SPAWN_SHARED_BEGIN();
    int rc = uv_spawn(loop, &req->proc, &opts);
    PROC_SPAWN_SHARED_END();

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
        case UV_ECANCELED: return NOVA_PROC_CLOSED;   /* a stream op cut short by close / cancel */
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

    if (nova_aint_load(&p->stage) >= PP_CLOSING) { result = NOVA_PROC_CLOSED; goto out; }
    if (cap <= 0) { result = 0; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/os: pipe read outside scope\n"); abort(); }
    NovaFiberQueue* cancel_sc = _proc_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = NOVA_PROC_CLOSED; goto out; }

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

    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = NOVA_PROC_CLOSED; goto out; }
    if (nova_aint_load(&p->stage) >= PP_CLOSING)       { result = NOVA_PROC_CLOSED; goto out; }
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

    if (nova_aint_load(&p->stage) >= PP_CLOSING) { result = NOVA_PROC_CLOSED; goto out; }
    if (len <= 0) { result = 0; goto out; }

    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/os: pipe write outside scope\n"); abort(); }
    NovaFiberQueue* cancel_sc = _proc_cancel_scope(scope);
    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = NOVA_PROC_CLOSED; goto out; }

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

    if (nova_abool_load(&cancel_sc->cancel_requested)) { result = NOVA_PROC_CLOSED; goto out; }
    if (nova_aint_load(&p->stage) >= PP_CLOSING)       { result = NOVA_PROC_CLOSED; goto out; }
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
#ifdef _WIN32
    /* Windows has no signals: libuv reports its own uv_process_kill as "signal 9", while a
     * TerminateProcess from anywhere else is a plain exit code. Report both the same way —
     * the code TerminateProcess left (1), signal() == None (D492 table 3.4). */
    (void)term_signal;
    c->term_signal = 0;
#else
    c->term_signal = term_signal;
#endif
    /* Tree.Group: the tree does not outlive its leader — stragglers die HERE, the moment
     * the leader's exit is known, and never later. On POSIX this is the only safe moment:
     * the group id stays reserved only while some member lives, so once the group is empty
     * the number is free and the OS may hand it to an unrelated process that leads its own
     * group; a later kill(-pgid) — from Child.kill or cleanup, maybe hours afterwards —
     * would SIGKILL that stranger (registry 1866). Here the leader has just been reaped:
     * a non-empty group still holds the id, an empty one leaves at most the microseconds
     * since waitpid. Windows: the Job Object handle cannot be reused; same rule for the
     * same behaviour on both systems. After this point nothing is ever signalled. */
    if (c->group) {
#ifdef _WIN32
        if (c->job) TerminateJobObject(c->job, 1);
#else
        (void)kill(-(pid_t)c->pid, SIGKILL);
#endif
    }
    nova_aint_store(&c->exited, 1);
    NovaFiberQueue* sc = c->wait_scope; int sl = c->wait_slot;
    c->wait_scope = NULL;
    if (sc) nova_sched_wake(sc, sl);
    uv_close((uv_handle_t*)proc, _pc_close_cb);
}

/* Deliver `sig` to the child, or to its whole tree with Tree.Group. 0 also when
 * there is nothing left to signal; < 0 = -errno or NOVA_PROC_UNSUPPORTED.
 * Once the child has exited NOTHING is sent, single or group: its pid / pgid may
 * already belong to another process (registry 1866; a group's stragglers were
 * swept by _pc_exit_cb). The remaining window — libuv's waitpid has reaped the
 * child but its exit_cb has not stored `exited` yet — is the same one libuv's
 * own uv_process_kill has. */
static nova_int _pc_send(NovaProcChild* c, int sig) {
#ifdef _WIN32
    if (sig != 9 && sig != 15) return NOVA_PROC_UNSUPPORTED;   /* no signals on Windows */
    if (nova_aint_load(&c->exited) != 0) return 0;
    if (c->job) { TerminateJobObject(c->job, 1); return 0; }
    int rc = uv_process_kill(&c->proc, SIGKILL);
    return (rc == 0 || rc == UV_ESRCH) ? 0 : _proc_neg_errno(rc);
#else
    if (nova_aint_load(&c->exited) != 0) return 0;
    int r;
    if (c->group) {
        r = kill(-(pid_t)c->pid, sig);     /* the child is its group's leader (setsid) */
    } else {
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
 * Assigned right after uv_spawn, so there IS a race window: a grandchild started
 * before the Assign stays outside the job (probe (b) of plan 294: 0/30 escapes for an
 * immediate assignment, 30/30 with a 100 ms delay). D492 wants CreateProcess
 * (CREATE_SUSPENDED) + Assign + ResumeThread, which libuv cannot do — deliberate
 * simplification [M-294-win-job-suspended], docs/dev/simplifications.md.
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

    PROC_SPAWN_SHARED_BEGIN();
    int rc = uv_spawn(loop, &c->proc, &opts);
    PROC_SPAWN_SHARED_END();

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
    if (!alive) return;            /* gone: its stragglers were swept at exit (_pc_exit_cb) */
    {
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
    (void)_pc_send(c, 9);          /* still alive after the grace (a no-op once it exited) */
    if (nova_aint_load(&c->exited) == 0) {
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
     * `pid` itself. Between the snapshot and the kill a listed process may die and its number go to
     * a stranger, and a parent id in the snapshot may itself be stale (registry 1866, plan 294 R6).
     * So every kill goes through a HANDLE (a number cannot be reused while a handle is open) and is
     * checked by creation time first: everything must predate the snapshot, and a descendant must be
     * younger than the parent it was listed under. What stays best effort: when `pid` itself is
     * already gone, its listed children cannot be told from children of an earlier holder of the
     * same number — they are taken if they predate the snapshot (documented in D492). */
    FILETIME t0_ft; GetSystemTimeAsFileTime(&t0_ft);
    ULONGLONG t0 = ((ULONGLONG)t0_ft.dwHighDateTime << 32) | t0_ft.dwLowDateTime;
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
    size_t* parent = (size_t*)malloc((n + 1) * sizeof(size_t));
    size_t m = 0;
    set[m] = (DWORD)pid; parent[m] = (size_t)-1; m++;
    for (size_t i = 0; i < m; i++)                 /* breadth first; m grows while we walk */
        for (size_t j = 0; j < n; j++)
            if (ppids[j] == set[i] && pids[j] != set[i]) {
                int seen = 0;
                for (size_t k = 0; k < m; k++) if (set[k] == pids[j]) { seen = 1; break; }
                if (!seen && m <= n) { set[m] = pids[j]; parent[m] = i; m++; }
            }
    /* Open everything first (handles pin the numbers), then judge by creation time. */
    HANDLE* hs = (HANDLE*)calloc(m, sizeof(HANDLE));
    ULONGLONG* ct = (ULONGLONG*)calloc(m, sizeof(ULONGLONG));
    nova_int root_err = NOVA_PROC_NO_SUCH;
    for (size_t i = 0; i < m; i++) {
        hs[i] = OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION, FALSE, set[i]);
        if (!hs[i]) {
            if (i == 0 && GetLastError() == ERROR_ACCESS_DENIED) root_err = -1;
            continue;
        }
        FILETIME c_ft, x_ft, k_ft, u_ft;
        if (GetProcessTimes(hs[i], &c_ft, &x_ft, &k_ft, &u_ft))
            ct[i] = ((ULONGLONG)c_ft.dwHighDateTime << 32) | c_ft.dwLowDateTime;
    }
    nova_int result = root_err;
    for (size_t i = m; i-- > 0;) {                 /* descendants first, the root last */
        if (!hs[i]) continue;
        int ours = ct[i] != 0 && ct[i] < t0;       /* not born after the snapshot */
        if (ours && parent[i] != (size_t)-1 && hs[parent[i]] && ct[parent[i]] != 0)
            ours = ct[i] >= ct[parent[i]];         /* a child is never older than its parent */
        if (ours) {
            if (TerminateProcess(hs[i], 1)) result = 0;
            else {
                DWORD code = 0;
                if (!(GetExitCodeProcess(hs[i], &code) && code != STILL_ACTIVE) && result != 0) result = -1;
            }
        }
        CloseHandle(hs[i]);
    }
    free(pids); free(ppids); free(set); free(parent); free(hs); free(ct);
    return result;
#else
    int r = tree ? kill(-(pid_t)pid, (int)sig) : kill((pid_t)pid, (int)sig);
    if (r == 0) return 0;
    return errno == ESRCH ? NOVA_PROC_NO_SUCH : -(nova_int)errno;
#endif
}

/* ===========================================================================
 * Plan 294 F.4 (D492): pseudo terminal. Windows: ConPTY.
 *
 * Shape (process.h has the contract):
 *   - two uv_pipe pairs; ConPTY gets the blocking ends, we keep the overlapped
 *     ones and open them with uv_pipe_open as two NovaProcPipe (read = the
 *     child's screen, write = its keyboard): the F.1 pull-style read / parked
 *     write code serves the terminal unchanged;
 *   - our own CreateProcessW (libuv cannot attach a pseudo console), started
 *     CREATE_SUSPENDED, put into its Job Object, then resumed: the tree has no
 *     window to escape through (plan 294 probe (b));
 *   - one watcher thread per terminal: waits for the child's exit (or a hang-up
 *     request), kills the stragglers, closes the console. The output pipe ends
 *     only after ClosePseudoConsole (probe (c)), and that call may wait until the
 *     output is drained (R8), so it never runs on a loop thread. The watcher
 *     reports to the loop through a uv_async_t; nothing else crosses threads.
 *
 * Refcount: Nova owner (proc_pty_close) + watcher (released when the async
 * handle is closed, after the watcher thread has ended). Freed on the loop.
 * =========================================================================== */

#ifdef _WIN32

typedef void* NovaHPCON;
typedef HRESULT (WINAPI *NovaPtyCreateFn)(COORD, HANDLE, HANDLE, DWORD, NovaHPCON*);
typedef HRESULT (WINAPI *NovaPtyResizeFn)(NovaHPCON, COORD);
typedef void    (WINAPI *NovaPtyCloseFn)(NovaHPCON);
#ifndef PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE
#  define PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE 0x00020016
#endif

/* Looked up at run time: ConPTY exists from Windows 10 1809; older systems get
 * NOVA_PROC_UNSUPPORTED instead of a loader failure of the whole program. */
static NovaPtyCreateFn _pty_create_fn;
static NovaPtyResizeFn _pty_resize_fn;
static NovaPtyCloseFn  _pty_close_fn;
static INIT_ONCE       _pty_api_once = INIT_ONCE_STATIC_INIT;


static BOOL CALLBACK _pty_api_init(PINIT_ONCE once, PVOID param, PVOID* ctx) {
    (void)once; (void)param; (void)ctx;
    HMODULE k = GetModuleHandleW(L"kernel32.dll");
    if (k) {
        _pty_create_fn = (NovaPtyCreateFn)(void*)GetProcAddress(k, "CreatePseudoConsole");
        _pty_resize_fn = (NovaPtyResizeFn)(void*)GetProcAddress(k, "ResizePseudoConsole");
        _pty_close_fn  = (NovaPtyCloseFn)(void*)GetProcAddress(k, "ClosePseudoConsole");
    }
    return TRUE;
}

static int _pty_api_ready(void) {
    InitOnceExecuteOnce(&_pty_api_once, _pty_api_init, NULL, NULL);
    return _pty_create_fn && _pty_resize_fn && _pty_close_fn;
}

/* "This process ignores CTRL+C": bit 0 of RTL_USER_PROCESS_PARAMETERS.ConsoleFlags, what
 * SetConsoleCtrlHandler(NULL, TRUE) sets and CreateProcess copies into every child. There is
 * no documented getter; winternl.h exposes the field as Reserved2[1]. */
static int _pty_ctrl_c_ignored(void) {
    PEB* peb = (PEB*)NtCurrentTeb()->ProcessEnvironmentBlock;
    if (!peb || !peb->ProcessParameters) return 0;
    return ((ULONG)(ULONG_PTR)peb->ProcessParameters->Reserved2[1] & 1u) != 0;
}

/* --- UTF-16 command line / environment ---------------------------------- */

typedef struct { wchar_t* p; size_t n, cap; } NovaWBuf;

static void _wb_put(NovaWBuf* b, const wchar_t* s, size_t n) {
    if (b->n + n + 1 > b->cap) {
        size_t cap = b->cap ? b->cap : 256;
        while (b->n + n + 1 > cap) cap *= 2;
        b->p = (wchar_t*)realloc(b->p, cap * sizeof(wchar_t));
        b->cap = cap;
    }
    if (n) memcpy(b->p + b->n, s, n * sizeof(wchar_t));
    b->n += n;
    b->p[b->n] = 0;
}
static void _wb_ch(NovaWBuf* b, wchar_t c) { _wb_put(b, &c, 1); }

/* UTF-8 bytes -> malloc'd NUL-terminated UTF-16. */
static wchar_t* _pty_wide(const uint8_t* s, nova_int len) {
    int n = len > 0 ? MultiByteToWideChar(CP_UTF8, 0, (const char*)s, (int)len, NULL, 0) : 0;
    wchar_t* w = (wchar_t*)malloc(((size_t)n + 1) * sizeof(wchar_t));
    if (n > 0) MultiByteToWideChar(CP_UTF8, 0, (const char*)s, (int)len, w, n);
    w[n] = 0;
    return w;
}

/* One argument, quoted the way CommandLineToArgvW / the MSVC CRT split it back
 * (backslashes double only before a quote). */
static void _pty_quote_arg(NovaWBuf* b, const wchar_t* a) {
    if (a[0] && !wcspbrk(a, L" \t\n\v\"")) { _wb_put(b, a, wcslen(a)); return; }
    _wb_ch(b, L'"');
    for (const wchar_t* p = a;; p++) {
        size_t bs = 0;
        while (*p == L'\\') { bs++; p++; }
        if (!*p) { for (size_t i = 0; i < 2 * bs; i++) _wb_ch(b, L'\\'); break; }
        if (*p == L'"') { for (size_t i = 0; i < 2 * bs + 1; i++) _wb_ch(b, L'\\'); }
        else { for (size_t i = 0; i < bs; i++) _wb_ch(b, L'\\'); }
        _wb_ch(b, *p);
    }
    _wb_ch(b, L'"');
}

static wchar_t* _pty_cmdline(const uint8_t* program, nova_int program_len,
                             const uint8_t* blob, nova_int blob_len, nova_int argc) {
    NovaWBuf b = { NULL, 0, 0 };
    wchar_t* w = _pty_wide(program, program_len);
    _pty_quote_arg(&b, w);
    free(w);
    nova_int pos = 0;
    for (nova_int i = 0; i < argc; i++) {
        nova_int start = pos;
        while (pos < blob_len && blob[pos] != 0) pos++;
        w = _pty_wide(blob + start, pos - start);
        _wb_ch(&b, L' ');
        _pty_quote_arg(&b, w);
        free(w);
        if (pos < blob_len) pos++;
    }
    return b.p;
}

static int _pty_env_cmp(const void* x, const void* y) {
    return CompareStringOrdinal(*(const wchar_t* const*)x, -1, *(const wchar_t* const*)y, -1, TRUE) - 2;
}

/* The explicit environment as a sorted UTF-16 block. Like libuv's uv_spawn (so Command.start
 * and start_pty see the same thing), the variables Windows programs cannot start without are
 * copied from our own environment when the caller left them out. */
static wchar_t* _pty_env_block(const uint8_t* blob, nova_int blob_len, nova_int envc) {
    static const wchar_t* required[] = { L"HOMEDRIVE", L"HOMEPATH", L"LOGONSERVER", L"PATH",
        L"SYSTEMDRIVE", L"SYSTEMROOT", L"TEMP", L"USERDOMAIN", L"USERNAME", L"USERPROFILE", L"WINDIR" };
    size_t nreq = sizeof required / sizeof required[0];
    wchar_t** ents = (wchar_t**)malloc(((size_t)envc + nreq + 1) * sizeof(wchar_t*));
    size_t n = 0;
    nova_int pos = 0;
    for (nova_int i = 0; i < envc; i++) {
        nova_int start = pos;
        while (pos < blob_len && blob[pos] != 0) pos++;
        ents[n++] = _pty_wide(blob + start, pos - start);
        if (pos < blob_len) pos++;
    }
    for (size_t r = 0; r < nreq; r++) {
        size_t klen = wcslen(required[r]);
        int have = 0;
        for (size_t i = 0; i < n && !have; i++)
            have = _wcsnicmp(ents[i], required[r], klen) == 0 && ents[i][klen] == L'=';
        if (have) continue;
        DWORD vlen = GetEnvironmentVariableW(required[r], NULL, 0);
        if (vlen == 0) continue;
        wchar_t* e = (wchar_t*)malloc((klen + 1 + vlen) * sizeof(wchar_t));
        memcpy(e, required[r], klen * sizeof(wchar_t));
        e[klen] = L'=';
        GetEnvironmentVariableW(required[r], e + klen + 1, vlen);
        ents[n++] = e;
    }
    qsort(ents, n, sizeof(wchar_t*), _pty_env_cmp);
    NovaWBuf b = { NULL, 0, 0 };
    for (size_t i = 0; i < n; i++) { _wb_put(&b, ents[i], wcslen(ents[i]) + 1); free(ents[i]); }
    _wb_ch(&b, 0);                       /* the block ends with an empty string */
    if (n == 0) _wb_ch(&b, 0);
    free(ents);
    return b.p;
}

/* --- The terminal handle -------------------------------------------------- */

typedef struct NovaPtyChild {
    uv_async_t      notify;          /* must be first: watcher thread -> loop */
    uv_loop_t*      loop;
    nova_atomic_int refcount;        /* Nova owner + watcher */
    nova_atomic_int exit_seen;       /* the watcher saw the exit (any thread may read) */
    nova_atomic_int exited;          /* published on the loop; wait() parks on it */
    nova_atomic_int watch_done;      /* the watcher is about to end */
    nova_atomic_int notify_closing;
    nova_atomic_int killing;         /* a kill on purpose (scope cancellation) */
    nova_atomic_int release_done;
    nova_atomic_int console_closed;
    int64_t         exit_status;
    int             pid;
    NovaFiberQueue* wait_scope;
    int             wait_slot;
    NovaProcPipe*   in;              /* our write end: the child's keyboard */
    NovaProcPipe*   out;             /* our read end: the child's screen */
    HANDLE          process;
    HANDLE          job;
    HANDLE          hangup;          /* event: close the console now */
    HANDLE          watcher;
    NovaHPCON       hpc;
    SRWLOCK         hpc_lock;
} NovaPtyChild;

static inline void _pty_release(NovaPtyChild* c) {
    if (nova_aint_fetch_sub_release(&c->refcount) == 1) {
        nova_thread_fence_acquire();
        nova_free_uncollectable(c);
    }
}

static nova_bool _pty_exited(void* ctx) { return nova_aint_load(&((NovaPtyChild*)ctx)->exited) != 0; }

/* ClosePseudoConsole exactly once. Any thread; may wait until the output is drained. */
static void _pty_close_console(NovaPtyChild* c) {
    int32_t zero = 0;
    if (!nova_aint_cas(&c->console_closed, &zero, 1)) return;
    AcquireSRWLockExclusive(&c->hpc_lock);
    NovaHPCON h = c->hpc;
    c->hpc = NULL;
    ReleaseSRWLockExclusive(&c->hpc_lock);
    if (h) _pty_close_fn(h);
}

static void _pty_notify_close_cb(uv_handle_t* h) {
    NovaPtyChild* c = (NovaPtyChild*)h->data;
    if (c->watcher) { CloseHandle(c->watcher); c->watcher = NULL; }
    if (c->process) { CloseHandle(c->process); c->process = NULL; }
    if (c->hangup)  { CloseHandle(c->hangup); c->hangup = NULL; }
    if (c->job)     { CloseHandle(c->job); c->job = NULL; }   /* the tree is already dead */
    _pty_release(c);                                          /* watcher unit */
}

static void _pty_notify_cb(uv_async_t* a) {
    NovaPtyChild* c = (NovaPtyChild*)a->data;
    if (nova_aint_load(&c->exit_seen) != 0 && nova_aint_load(&c->exited) == 0) {
        nova_aint_store(&c->exited, 1);
        NovaFiberQueue* sc = c->wait_scope; int sl = c->wait_slot;
        c->wait_scope = NULL;
        if (sc) nova_sched_wake(sc, sl);
    }
    int32_t zero = 0;
    if (nova_aint_load(&c->watch_done) != 0 && nova_aint_cas(&c->notify_closing, &zero, 1)) {
        /* The watcher's last act is one more uv_async_send: let it finish, so that send is
         * either delivered or pending (libuv closes a handle with a pending send safely). */
        WaitForSingleObject(c->watcher, INFINITE);
        uv_close((uv_handle_t*)a, _pty_notify_close_cb);
    }
}

static DWORD WINAPI _pty_watch(LPVOID arg) {
    NovaPtyChild* c = (NovaPtyChild*)arg;
    HANDLE hs[2] = { c->process, c->hangup };
    DWORD n = 2;
    for (;;) {
        DWORD r = WaitForMultipleObjects(n, hs, FALSE, INFINITE);
        if (n == 2 && r == WAIT_OBJECT_0 + 1) { _pty_close_console(c); n = 1; continue; }
        break;                                   /* the child exited (or the wait failed) */
    }
    DWORD code = 0;
    GetExitCodeProcess(c->process, &code);
    c->exit_status = (int64_t)code;
    if (c->job) TerminateJobObject(c->job, 1);   /* the tree does not outlive its leader */
    nova_aint_store(&c->exit_seen, 1);
    uv_async_send(&c->notify);                   /* wait() returns now, even if the close below waits */
    _pty_close_console(c);                       /* the reader gets end of stream after the last byte */
    nova_aint_store(&c->watch_done, 1);
    uv_async_send(&c->notify);
    return 0;
}

/* Kill / Terminate: the whole tree (Job Object), or the child alone when the job could not be
 * set up. Nothing is sent once the child has exited. */
static nova_int _pty_send(NovaPtyChild* c, int sig) {
    if (sig != 9 && sig != 15) return NOVA_PROC_UNSUPPORTED;
    if (nova_aint_load(&c->exit_seen) != 0) return 0;
    if (c->job) { TerminateJobObject(c->job, 1); return 0; }
    if (TerminateProcess(c->process, 1)) return 0;
    DWORD code = 0;
    return (GetExitCodeProcess(c->process, &code) && code != STILL_ACTIVE) ? 0 : -1;
}

static void _pty_kill(NovaPtyChild* c) {
    int32_t was = __atomic_exchange_n((volatile int32_t*)&c->killing, 1, __ATOMIC_ACQ_REL);
    if (!was) (void)_pty_send(c, 9);
}

static NovaStopMode _pty_stop_cb(void* handle) {
    _pty_kill((NovaPtyChild*)handle);
    return NOVA_STOP_ASYNC;              /* the wake comes from the watcher's notify */
}

/* A pipe that never reached Nova: close it and drop the Nova unit too. */
static void _pty_drop_pipe(NovaProcPipe* p) {
    if (!p) return;
    nova_aint_store(&p->stage, PP_CLOSING);
    uv_close((uv_handle_t*)&p->handle, _pp_close_cb);
    int32_t zero = 0;
    if (nova_aint_cas(&p->user_release_done, &zero, 1)) _pp_release(p);
}

/* uv_pipe_open owns the fd from here on, except for fds 0..2, which libuv duplicates
 * and leaves to us. */
static int _pty_open_pipe(NovaProcPipe* p, uv_file fd) {
    int rc = uv_pipe_open(&p->handle, fd);
    if (rc != 0 || fd <= 2) _close(fd);
    return rc;
}

void* proc_pty_spawn(const uint8_t* program, nova_int program_len,
                     const uint8_t* argv_blob, nova_int argv_blob_len, nova_int argc,
                     const uint8_t* env_blob, nova_int env_blob_len, nova_int envc,
                     nova_bool use_env,
                     const uint8_t* cwd, nova_int cwd_len,
                     nova_int rows, nova_int cols, nova_int* out_err) {
    if (out_err) *out_err = 0;
    if (rows < 1 || cols < 1 || rows > 32767 || cols > 32767) { if (out_err) *out_err = -22; return NULL; }
    if (!_pty_api_ready()) { if (out_err) *out_err = NOVA_PROC_UNSUPPORTED; return NULL; }
    uv_loop_t* loop = nova_current_loop();

    /* Keyboard: ConPTY reads in_fd[0], we write in_fd[1]. Screen: ConPTY writes out_fd[1],
     * we read out_fd[0]. Only our ends are overlapped (UV_NONBLOCK_PIPE). */
    uv_file in_fd[2] = { -1, -1 }, out_fd[2] = { -1, -1 };
    int rc = uv_pipe(in_fd, 0, UV_NONBLOCK_PIPE);
    if (rc != 0) { if (out_err) *out_err = _proc_neg_errno(rc); return NULL; }
    rc = uv_pipe(out_fd, UV_NONBLOCK_PIPE, 0);
    if (rc != 0) {
        _close(in_fd[0]); _close(in_fd[1]);
        if (out_err) *out_err = _proc_neg_errno(rc);
        return NULL;
    }

    NovaProcPipe* pin = _pp_new(loop);
    NovaProcPipe* pout = _pp_new(loop);
    uv_pipe_init(loop, &pin->handle, 0);
    uv_pipe_init(loop, &pout->handle, 0);
    rc = _pty_open_pipe(pin, in_fd[1]);
    int rc2 = _pty_open_pipe(pout, out_fd[0]);
    if (rc == 0) rc = rc2;

    NovaHPCON hpc = NULL;
    if (rc == 0) {
        COORD sz = { (SHORT)cols, (SHORT)rows };
        HRESULT hr = _pty_create_fn(sz, (HANDLE)uv_get_osfhandle(in_fd[0]),
                                    (HANDLE)uv_get_osfhandle(out_fd[1]), 0, &hpc);
        if (FAILED(hr)) { hpc = NULL; rc = (hr == E_INVALIDARG) ? UV_EINVAL : UV_EIO; }
    }

    PROCESS_INFORMATION pi;
    memset(&pi, 0, sizeof pi);
    if (rc == 0) {
        wchar_t* cmdline = _pty_cmdline(program, program_len, argv_blob, argv_blob_len, argc);
        wchar_t* envw = use_env ? _pty_env_block(env_blob, env_blob_len, envc) : NULL;
        wchar_t* cwdw = cwd_len > 0 ? _pty_wide(cwd, cwd_len) : NULL;
        STARTUPINFOEXW si;
        memset(&si, 0, sizeof si);
        si.StartupInfo.cb = sizeof si;
        /* Without STARTF_USESTDHANDLES a child of a process whose own stdio is redirected
         * inherits those handles and writes PAST the terminal (plan 294 probe (c): "hello"
         * landed in the parent's stdout). Empty handles = the pseudo console's. */
        si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        SIZE_T alen = 0;
        InitializeProcThreadAttributeList(NULL, 1, 0, &alen);
        si.lpAttributeList = (LPPROC_THREAD_ATTRIBUTE_LIST)malloc(alen);
        BOOL ok = InitializeProcThreadAttributeList(si.lpAttributeList, 1, 0, &alen);
        int list_ready = ok;
        if (ok) ok = UpdateProcThreadAttribute(si.lpAttributeList, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                                               hpc, sizeof hpc, NULL, NULL);
        DWORD err = ok ? 0 : GetLastError();
        if (ok) {
            /* A fresh terminal starts with Ctrl-C ON (POSIX: libuv's child resets every signal to
             * SIG_DFL). The "ignore Ctrl-C" flag of THIS process would otherwise be copied into
             * the child, and 0x03 written to the terminal would do nothing (measured: a test run
             * under a shell that ignores Ctrl-C). Cleared just around CreateProcess, then put back. */
            AcquireSRWLockExclusive(&_proc_spawn_lock);
            int ignored = _pty_ctrl_c_ignored();
            if (ignored) SetConsoleCtrlHandler(NULL, FALSE);
            ok = CreateProcessW(NULL, cmdline, NULL, NULL, FALSE,
                                EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
                                envw, cwdw, &si.StartupInfo, &pi);
            err = ok ? 0 : GetLastError();
            if (ignored) SetConsoleCtrlHandler(NULL, TRUE);
            ReleaseSRWLockExclusive(&_proc_spawn_lock);
        }
        if (list_ready) DeleteProcThreadAttributeList(si.lpAttributeList);
        free(si.lpAttributeList);
        free(cmdline); free(envw); free(cwdw);
        if (!ok) rc = uv_translate_sys_error((int)err);
    }
    /* ConPTY holds its own copies of its pipe ends; ours must go, or the screen never ends. */
    _close(in_fd[0]);
    _close(out_fd[1]);

    if (rc != 0) {
        if (hpc) _pty_close_fn(hpc);
        _pty_drop_pipe(pin);
        _pty_drop_pipe(pout);
        if (out_err) *out_err = _proc_neg_errno(rc);
        return NULL;
    }

    NovaPtyChild* c = (NovaPtyChild*)nova_alloc_uncollectable(sizeof(NovaPtyChild));
    memset(c, 0, sizeof(*c));
    nova_aint_init(&c->refcount, 2);
    nova_aint_init(&c->exit_seen, 0);
    nova_aint_init(&c->exited, 0);
    nova_aint_init(&c->watch_done, 0);
    nova_aint_init(&c->notify_closing, 0);
    nova_aint_init(&c->killing, 0);
    nova_aint_init(&c->release_done, 0);
    nova_aint_init(&c->console_closed, 0);
    InitializeSRWLock(&c->hpc_lock);
    c->loop = loop;
    c->in = pin;
    c->out = pout;
    c->hpc = hpc;
    c->process = pi.hProcess;
    c->pid = (int)pi.dwProcessId;

    /* The Job Object is assigned while the child is still suspended: no grandchild can be born
     * outside it (the window [M-294-win-job-suspended] leaves open for Child does not exist here). */
    HANDLE j = CreateJobObjectW(NULL, NULL);
    if (j) {
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION li;
        memset(&li, 0, sizeof li);
        li.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if (SetInformationJobObject(j, JobObjectExtendedLimitInformation, &li, sizeof li) &&
            AssignProcessToJobObject(j, pi.hProcess)) c->job = j;
        else CloseHandle(j);           /* degrades to a single-process kill */
    }

    uv_async_init(loop, &c->notify, _pty_notify_cb);
    c->notify.data = c;
    c->hangup = CreateEventW(NULL, TRUE, FALSE, NULL);
    c->watcher = c->hangup ? CreateThread(NULL, 64 * 1024, _pty_watch, c, 0, NULL) : NULL;
    if (!c->watcher) {
        /* Cannot watch it: do not leave it running. */
        TerminateProcess(pi.hProcess, 1);
        ResumeThread(pi.hThread);
        CloseHandle(pi.hThread);
        WaitForSingleObject(pi.hProcess, 5000);
        _pty_close_console(c);
        if (c->hangup) CloseHandle(c->hangup);
        c->hangup = NULL;
        CloseHandle(c->process);
        c->process = NULL;
        if (c->job) { CloseHandle(c->job); c->job = NULL; }
        _pty_drop_pipe(pin);
        _pty_drop_pipe(pout);
        nova_aint_store(&c->notify_closing, 1);
        uv_close((uv_handle_t*)&c->notify, NULL);
        /* the struct is leaked on purpose: the closing async handle still points into it */
        if (out_err) *out_err = -12;
        return NULL;
    }
    ResumeThread(pi.hThread);
    CloseHandle(pi.hThread);
    return c;
}

nova_int proc_pty_read(void* pv, uint8_t* buf, nova_int cap) {
    return proc_pipe_read(((NovaPtyChild*)pv)->out, buf, cap);
}

nova_int proc_pty_write(void* pv, const uint8_t* buf, nova_int len) {
    return proc_pipe_write(((NovaPtyChild*)pv)->in, buf, len);
}

nova_int proc_pty_resize(void* pv, nova_int rows, nova_int cols) {
    NovaPtyChild* c = (NovaPtyChild*)pv;
    if (rows < 1 || cols < 1 || rows > 32767 || cols > 32767) return -22;
    nova_int r;
    AcquireSRWLockShared(&c->hpc_lock);
    if (!c->hpc) r = NOVA_PROC_CLOSED;
    else {
        COORD sz = { (SHORT)cols, (SHORT)rows };
        r = SUCCEEDED(_pty_resize_fn(c->hpc, sz)) ? 0 : -5;
    }
    ReleaseSRWLockShared(&c->hpc_lock);
    return r;
}

nova_int proc_pty_pid(void* pv) { return (nova_int)((NovaPtyChild*)pv)->pid; }

nova_int proc_pty_try_wait(void* pv, nova_int* out_code, nova_int* out_signal) {
    NovaPtyChild* c = (NovaPtyChild*)pv;
    if (nova_aint_load(&c->exited) == 0) return 1;
    if (out_code) *out_code = (nova_int)c->exit_status;
    if (out_signal) *out_signal = 0;   /* Windows: never a signal (D492 table 3.4) */
    return 0;
}

nova_int proc_pty_wait(void* pv, nova_int* out_code, nova_int* out_signal) {
    NovaPtyChild* c = (NovaPtyChild*)pv;
    NovaFiberQueue* scope = _nova_active_scope;
    int slot = _nova_active_slot;
    if (!scope) { fprintf(stderr, "nova/os: pty wait outside scope\n"); abort(); }
    NovaFiberQueue* cancel_sc = _proc_cancel_scope(scope);
    if (nova_aint_load(&c->exited) == 0) {
        if (nova_abool_load(&cancel_sc->cancel_requested)) { _pty_kill(c); return NOVA_PROCESS_CANCELLED; }
        c->wait_scope = scope; c->wait_slot = slot;
        nova_sched_register_pending(scope, slot, c, _pty_stop_cb);
        nova_sched_park_until(scope, slot, _pty_exited, c);
        nova_sched_unregister_pending(scope, slot);
        c->wait_scope = NULL;
        if (nova_abool_load(&cancel_sc->cancel_requested) || nova_aint_load(&c->killing) != 0)
            return NOVA_PROCESS_CANCELLED;
    }
    if (out_code) *out_code = (nova_int)c->exit_status;
    if (out_signal) *out_signal = 0;
    return 0;
}

nova_int proc_pty_kill(void* pv, nova_int sig) { return _pty_send((NovaPtyChild*)pv, (int)sig); }

void proc_pty_close(void* pv) {
    NovaPtyChild* c = (NovaPtyChild*)pv;
    if (!c) return;
    int32_t zero = 0;
    if (!nova_aint_cas(&c->release_done, &zero, 1)) return;
    /* Our ends first: a console whose output nobody drains would hold the hang-up (R8). */
    _pp_user_close(c->in);
    _pp_user_close(c->out);
    if (nova_aint_load(&c->exited) == 0) {
        SetEvent(c->hangup);                 /* hang up: the console goes, its processes are told */
        NovaFiberQueue* scope = _nova_active_scope;
        int64_t deadline = _proc_now_ms() + NOVA_PROC_RELEASE_GRACE_MS;
        while (nova_aint_load(&c->exited) == 0 && _proc_now_ms() < deadline) {
            if (!scope || !mco_running()) break;
            if (nova_cancel_mask_active() == 0 &&
                nova_abool_load(&_proc_cancel_scope(scope)->cancel_requested)) break;
            (void)time_sleep_ms(2);
        }
        (void)_pty_send(c, 9);               /* still there after the grace */
        if (nova_aint_load(&c->exited) == 0) {
            scope = _nova_active_scope;
            int slot = _nova_active_slot;
            if (scope) {
                c->wait_scope = scope; c->wait_slot = slot;
                nova_sched_park_until(scope, slot, _pty_exited, c);   /* not cancellable */
                c->wait_scope = NULL;
            }
        }
    }
    _pty_release(c);                         /* Nova unit */
}

#else  /* POSIX: forkpty is the next task (plan 294 F.3); the surface exists and says so. */

void* proc_pty_spawn(const uint8_t* program, nova_int program_len,
                     const uint8_t* argv_blob, nova_int argv_blob_len, nova_int argc,
                     const uint8_t* env_blob, nova_int env_blob_len, nova_int envc,
                     nova_bool use_env,
                     const uint8_t* cwd, nova_int cwd_len,
                     nova_int rows, nova_int cols, nova_int* out_err) {
    (void)program; (void)program_len; (void)argv_blob; (void)argv_blob_len; (void)argc;
    (void)env_blob; (void)env_blob_len; (void)envc; (void)use_env; (void)cwd; (void)cwd_len;
    (void)rows; (void)cols;
    if (out_err) *out_err = NOVA_PROC_UNSUPPORTED;
    return NULL;
}
nova_int proc_pty_read(void* pv, uint8_t* buf, nova_int cap) { (void)pv; (void)buf; (void)cap; return NOVA_PROC_CLOSED; }
nova_int proc_pty_write(void* pv, const uint8_t* buf, nova_int len) { (void)pv; (void)buf; (void)len; return NOVA_PROC_CLOSED; }
nova_int proc_pty_resize(void* pv, nova_int rows, nova_int cols) { (void)pv; (void)rows; (void)cols; return NOVA_PROC_UNSUPPORTED; }
nova_int proc_pty_pid(void* pv) { (void)pv; return 0; }
nova_int proc_pty_wait(void* pv, nova_int* out_code, nova_int* out_signal) {
    (void)pv; (void)out_code; (void)out_signal; return NOVA_PROC_UNSUPPORTED;
}
nova_int proc_pty_try_wait(void* pv, nova_int* out_code, nova_int* out_signal) {
    (void)pv; (void)out_code; (void)out_signal; return NOVA_PROC_UNSUPPORTED;
}
nova_int proc_pty_kill(void* pv, nova_int sig) { (void)pv; (void)sig; return NOVA_PROC_UNSUPPORTED; }
void proc_pty_close(void* pv) { (void)pv; }

#endif
