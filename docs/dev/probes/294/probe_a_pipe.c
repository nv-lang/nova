/* SPDX-License-Identifier: MIT OR Apache-2.0
 * Plan 294 F.0 probe (a): uv_spawn + UV_CREATE_PIPE on stdout, pull-style read.
 * Question: does uv_read_stop give back-pressure (child blocks on a full pipe)
 * on Windows and POSIX, and is every byte delivered intact?
 *   probe_a                 -> parent; spawns itself as `probe_a writer N progress-file`
 *   probe_a writer N file   -> child; writes N bytes in 4 KiB blocks, rewrites
 *                              `file` with the running total after every block.
 */
#include <uv.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#  include <windows.h>
#  include <io.h>
#  include <fcntl.h>
#  define SLEEP_MS(n) Sleep(n)
#else
#  include <unistd.h>
#  define SLEEP_MS(n) usleep((n)*1000)
#endif

#define TOTAL (8L*1024*1024)

static int writer(long n, const char* pf) {
    unsigned char blk[4096]; long done = 0;
#ifdef _WIN32
    _setmode(_fileno(stdout), _O_BINARY); /* else 0x0A -> 0x0D0A: bytes are NOT transparent */
#endif
    for (int i = 0; i < 4096; i++) blk[i] = (unsigned char)(i * 7 + 13);
    while (done < n) {
        size_t w = fwrite(blk, 1, 4096, stdout);
        if (w != 4096) return 3;
        fflush(stdout);
        done += 4096;
        FILE* f = fopen(pf, "w"); if (f) { fprintf(f, "%ld", done); fclose(f); }
    }
    return 0;
}
static long progress(const char* pf) {
    FILE* f = fopen(pf, "r"); long v = 0; if (f) { if (fscanf(f, "%ld", &v) != 1) v = 0; fclose(f); } return v;
}

static uv_loop_t* loop; static uv_pipe_t out; static uv_process_t proc;
static long got = 0; static unsigned long sum = 0; static int at_eof = 0;
static char buf[4096]; static int chunks = 0; static const char* pf;
static int exited = 0; static long long exit_code = -1;

static void alloc_cb(uv_handle_t* h, size_t s, uv_buf_t* b) { (void)h; (void)s; *b = uv_buf_init(buf, sizeof buf); }
static void read_cb(uv_stream_t* s, ssize_t n, const uv_buf_t* b) {
    if (n > 0) { got += n; for (ssize_t i = 0; i < n; i++) sum += (unsigned char)b->base[i]; chunks++; uv_read_stop(s); }
    else if (n < 0) { at_eof = 1; uv_close((uv_handle_t*)s, NULL); }
}
static void exit_cb(uv_process_t* p, int64_t st, int sig) { (void)sig; exited = 1; exit_code = st; uv_close((uv_handle_t*)p, NULL); }

int main(int argc, char** argv) {
    if (argc >= 4 && !strcmp(argv[1], "writer")) return writer(atol(argv[2]), argv[3]);
    char pfb[512]; snprintf(pfb, sizeof pfb, "probe_a_progress_%d.txt", (int)uv_os_getpid()); pf = pfb;
    char sz[32]; snprintf(sz, sizeof sz, "%ld", TOTAL);
    char exe[1024]; size_t el = sizeof exe; uv_exepath(exe, &el);
    char* args[] = { exe, "writer", sz, pfb, NULL };
    loop = uv_default_loop();
    uv_pipe_init(loop, &out, 0);
    uv_stdio_container_t io[3];
    io[0].flags = UV_IGNORE; io[1].flags = UV_CREATE_PIPE | UV_WRITABLE_PIPE; io[1].data.stream = (uv_stream_t*)&out;
    io[2].flags = UV_IGNORE;
    uv_process_options_t o; memset(&o, 0, sizeof o);
    o.file = exe; o.args = args; o.exit_cb = exit_cb; o.stdio_count = 3; o.stdio = io;
    int rc = uv_spawn(loop, &proc, &o); if (rc) { printf("spawn failed %s\n", uv_strerror(rc)); return 1; }
    /* Phase 1: not reading at all for 1 s -> child must stall at ~pipe capacity. */
    uv_run(loop, UV_RUN_NOWAIT); SLEEP_MS(1000);
    long p1 = progress(pf);
    printf("PHASE1 unread 1s: child progress=%ld bytes (of %ld)\n", p1, TOTAL);
    /* Phase 2: pull ONE chunk, stop reading, sleep: child must not run away. */
    uv_read_start((uv_stream_t*)&out, alloc_cb, read_cb);
    while (chunks < 1) uv_run(loop, UV_RUN_ONCE);
    SLEEP_MS(500);
    long p2a = progress(pf); SLEEP_MS(500); long p2b = progress(pf);
    printf("PHASE2 after one chunk + uv_read_stop: progress %ld -> %ld (stable=%s), got=%ld\n", p2a, p2b, p2a == p2b ? "yes" : "NO", got);
    /* Phase 3: drain pulling chunk by chunk (start/stop per chunk). */
    while (!at_eof) { if (!uv_is_closing((uv_handle_t*)&out)) uv_read_start((uv_stream_t*)&out, alloc_cb, read_cb); uv_run(loop, UV_RUN_ONCE); }
    while (!exited) uv_run(loop, UV_RUN_ONCE);
    uv_run(loop, UV_RUN_DEFAULT);
    unsigned long want = 0; for (long i = 0; i < TOTAL; i++) want += (unsigned char)((i % 4096) * 7 + 13);
    printf("PHASE3 drained: got=%ld sum_ok=%s exit_code=%lld chunks=%d\n", got, sum == want ? "yes" : "NO", exit_code, chunks);
    remove(pf);
    return !(got == TOTAL && sum == want && exit_code == 0);
}
