/* SPDX-License-Identifier: MIT OR Apache-2.0
 * Plan 294 F.0 probe (b)+(c), POSIX.
 *  T1  uv_spawn(UV_PROCESS_DETACHED => setsid) + kill(-pid, SIGKILL): is the grandchild dead too?
 *  T2  same without DETACHED + kill(pid): grandchild survives (documents why Group is explicit).
 *  T3  forkpty child (upper-cases a line); master fd given to libuv via uv_pipe_open;
 *      read with uv_read_start; what error does the loop see when the child is gone (EIO)?
 *  T4  TIOCSWINSZ on the master: child reads its size with ioctl(TIOCGWINSZ) before/after.
 * Build: gcc probe_c_posix.c -luv -lutil
 */
#define _GNU_SOURCE
#include <uv.h>
#include <pty.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <fcntl.h>

static uv_loop_t* loop;
static void exit_cb(uv_process_t* p, int64_t s, int sg) { (void)s; (void)sg; uv_close((uv_handle_t*)p, NULL); }

static int alive(pid_t p) { return kill(p, 0) == 0; }

/* child "tree": sh spawns a sleeping grandchild, prints its pid to the given file */
static void tree(int detached, const char* label) {
    char pidf[64]; snprintf(pidf, sizeof pidf, "/tmp/f0/gc_%s.pid", label);
    unlink(pidf);
    char script[256]; snprintf(script, sizeof script, "sleep 30 & echo $! > %s; wait", pidf);
    char* a[] = { "/bin/sh", "-c", script, NULL };
    uv_process_t p; uv_process_options_t o; memset(&o, 0, sizeof o);
    o.file = "/bin/sh"; o.args = a; o.exit_cb = exit_cb; o.flags = detached ? UV_PROCESS_DETACHED : 0;
    if (uv_spawn(loop, &p, &o)) { printf("spawn fail\n"); return; }
    usleep(300000);
    FILE* f = fopen(pidf, "r"); int gc = 0; if (f) { if (fscanf(f, "%d", &gc) != 1) gc = 0; fclose(f); }
    pid_t pg = getpgid(p.pid);
    printf("%s: child=%d child_pgid=%d (parent pgid=%d) grandchild=%d\n", label, p.pid, (int)pg, (int)getpgrp(), gc);
    if (detached) kill(-p.pid, SIGKILL); else kill(p.pid, SIGKILL);
    uv_run(loop, UV_RUN_DEFAULT);
    usleep(200000);
    int ga = gc > 0 && waitpid(gc, NULL, WNOHANG) == 0 ? 1 : 0; (void)ga;
    printf("%s: after kill: grandchild alive=%s\n", label, alive(gc) ? "YES" : "no");
    if (alive(gc)) kill(gc, SIGKILL);
}

static uv_pipe_t mpipe; static char rbuf[4096]; static char acc[65536]; static int acclen;
static int final_err = 0;
static void alloc_cb(uv_handle_t* h, size_t s, uv_buf_t* b) { (void)h; (void)s; *b = uv_buf_init(rbuf, sizeof rbuf); }
static void read_cb(uv_stream_t* s, ssize_t n, const uv_buf_t* b) {
    if (n > 0) { memcpy(acc + acclen, b->base, n); acclen += (int)n; }
    else if (n < 0) { final_err = (int)n; uv_close((uv_handle_t*)s, NULL); }
}
static void dump(const char* t) {
    printf("%s (%d bytes): ", t, acclen);
    for (int i = 0; i < acclen; i++) { unsigned char c = acc[i]; if (c >= 32 && c < 127) putchar(c); else printf("<%02x>", c); }
    putchar('\n');
}

static void pty_test(void) {
    int mfd; struct winsize ws = { 24, 80, 0, 0 };
    pid_t pid = forkpty(&mfd, NULL, NULL, &ws);
    if (pid == 0) {
        /* child: only async-signal-safe calls would be required in a threaded parent; exec at once. */
        execl("/bin/sh", "sh", "-c",
              "stty size; read l; echo \"$l\" | tr a-z A-Z; printf '\033[31mred\033[0m\n'; sleep 1; stty size", (char*)NULL);
        _exit(127);
    }
    printf("T3 forkpty pid=%d master_fd=%d\n", pid, mfd);
    fcntl(mfd, F_SETFL, fcntl(mfd, F_GETFL) | O_NONBLOCK);
    uv_pipe_init(loop, &mpipe, 0);
    int rc = uv_pipe_open(&mpipe, mfd);
    printf("T3 uv_pipe_open(master) rc=%d (%s)\n", rc, rc ? uv_strerror(rc) : "ok");
    if (rc) return;
    uv_read_start((uv_stream_t*)&mpipe, alloc_cb, read_cb);
    uv_timer_t t; uv_timer_init(loop, &t);
    /* after 300 ms: send a line, later: resize 40x120 */
    usleep(1); 
    const char* line = "abc\n"; if (write(mfd, line, 4) < 0) perror("write");
    struct winsize ws2 = { 40, 120, 0, 0 };
    uv_run(loop, UV_RUN_NOWAIT); usleep(400000); uv_run(loop, UV_RUN_NOWAIT);
    ioctl(mfd, TIOCSWINSZ, &ws2);
    while (!final_err) uv_run(loop, UV_RUN_ONCE);
    int st; waitpid(pid, &st, 0);
    dump("T3/T4 master output");
    printf("T3 read loop ended with err=%d (%s); EIO=%d EOF=%d; child exit=%d\n", final_err, uv_strerror(final_err), UV_EIO, UV_EOF, WIFEXITED(st) ? WEXITSTATUS(st) : -1);
}

int main(void) {
    loop = uv_default_loop();
    system("mkdir -p /tmp/f0");
    tree(1, "T1_detached");
    tree(0, "T2_plain");
    pty_test();
    return 0;
}
