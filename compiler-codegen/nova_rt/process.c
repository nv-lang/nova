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
