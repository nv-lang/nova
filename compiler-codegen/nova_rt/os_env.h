/* os_env.h — std/os native hooks: args / env / cwd / dirs / process
 * (Plan 176 Ф.3, D324).
 *
 * These are non-blocking native syscalls (getenv/getcwd/setenv/getpid/...), NOT
 * libuv-backed, so — exactly like io_console.h's fs_seek/platform-predicate —
 * they live in this always-included header-only unit rather than a libuv-gated
 * .c file. The generated program is a single translation unit that includes this
 * once; `static inline` gives every definition internal linkage (no multiple-
 * definition even if nova_rt.h pulls it into fs.c / net.c too).
 *
 * Return convention (net/fs precedent): string-returning getters return a
 * `nova_str` carrying the raw bytes (empty == unavailable / error); the mutating
 * ops (env_set/env_remove/set_cwd) return 0 on success or a NEGATIVE POSIX errno.
 * Paths and env keys/values cross as NUL-terminated `const uint8_t*` (the Nova
 * real_os handler NUL-terminates via `c_str`, mirroring fs's `c_path`), so a plain
 * cast to `const char*` is a valid C string. Values crossing OUT are wrapped
 * verbatim (byte-transparent) — non-UTF-8 Unix env bytes round-trip losslessly.
 *
 * Program arguments (argv) are captured once at process start: main() calls
 * `nova_os_set_args(argc, argv)` (spliced by emit_c.rs) into the file-scope
 * argv globals, which nova_os_arg_count/nova_os_arg_at then read.
 */
#ifndef NOVA_OS_ENV_H
#define NOVA_OS_ENV_H

#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <stdio.h>
#include <stdint.h>

#if defined(_WIN32)
#  include <direct.h>    /* _getcwd, _chdir */
#  include <process.h>   /* _getpid */
#  define NOVA_ENVIRON _environ
#else
#  include <unistd.h>    /* getcwd, chdir, getpid, gethostname */
extern char **environ;
#  define NOVA_ENVIRON environ
#endif

/* ─── nova_str wrappers (mirror fs.c's _nova_fs_cstr; GC-allocated copy) ─── */

static inline nova_str _nova_os_bytes(const uint8_t* s, nova_int n) {
    nova_str out;
    if (!s || n <= 0) { out.ptr = NULL; out.len = 0; return out; }
    uint8_t* p = (uint8_t*)nova_alloc((size_t)n + 1);
    memcpy(p, s, (size_t)n);
    p[n] = 0;
    out.ptr = (const uint8_t*)p;
    out.len = (nova_int)n;
    return out;
}

static inline nova_str _nova_os_str(const char* s) {
    if (!s) { nova_str z; z.ptr = NULL; z.len = 0; return z; }
    return _nova_os_bytes((const uint8_t*)s, (nova_int)strlen(s));
}

/* map a failed errno to the negated code convention (-errno; -EINVAL fallback) */
static inline nova_int _nova_os_fail(void) {
    int e = errno;
    return e > 0 ? -(nova_int)e : -(nova_int)22;
}

/* ─── Program arguments (argv) ─── */

static nova_int _nova_argc = 0;
static char**   _nova_argv = NULL;

/* Called once from main() (emit_c.rs) with the process argv. */
static inline void nova_os_set_args(int argc, char** argv) {
    _nova_argc = (nova_int)argc;
    _nova_argv = argv;
}

/* Number of program arguments (argv[0] = program path, included). */
static inline nova_int nova_os_arg_count(void) { return _nova_argc; }

/* The i-th argument (empty out of range). */
static inline nova_str nova_os_arg_at(nova_int i) {
    if (i < 0 || i >= _nova_argc || !_nova_argv) return _nova_os_str("");
    return _nova_os_str(_nova_argv[(size_t)i]);
}

/* ─── Environment ─── */

/* Raw value bytes for `key` (empty if absent — disambiguate with nova_os_env_has). */
static inline nova_str nova_os_env_get(const uint8_t* key) {
    const char* v = getenv((const char*)key);
    return _nova_os_str(v ? v : "");
}

/* 1 if `key` is present, 0 otherwise. */
static inline nova_int nova_os_env_has(const uint8_t* key) {
    return getenv((const char*)key) ? 1 : 0;
}

/* Set `key` = `val` (overwrite). 0 or -errno. */
static inline nova_int nova_os_env_set(const uint8_t* key, const uint8_t* val) {
#if defined(_WIN32)
    return _putenv_s((const char*)key, (const char*)val) == 0 ? 0 : _nova_os_fail();
#else
    return setenv((const char*)key, (const char*)val, 1) == 0 ? 0 : _nova_os_fail();
#endif
}

/* Remove `key`. 0 or -errno (removing a missing key is success). */
static inline nova_int nova_os_env_remove(const uint8_t* key) {
#if defined(_WIN32)
    return _putenv_s((const char*)key, "") == 0 ? 0 : _nova_os_fail();
#else
    return unsetenv((const char*)key) == 0 ? 0 : _nova_os_fail();
#endif
}

/* Number of environment entries (snapshot count of `environ`). */
static inline nova_int nova_os_env_len(void) {
    char** e = NOVA_ENVIRON;
    nova_int n = 0;
    if (!e) return 0;
    while (e[n]) n++;
    return n;
}

/* Key of the i-th environment entry (the part before '='). */
static inline nova_str nova_os_env_key_at(nova_int i) {
    char** e = NOVA_ENVIRON;
    if (!e || i < 0) return _nova_os_str("");
    const char* s = e[(size_t)i];
    if (!s) return _nova_os_str("");
    const char* eq = strchr(s, '=');
    size_t klen = eq ? (size_t)(eq - s) : strlen(s);
    return _nova_os_bytes((const uint8_t*)s, (nova_int)klen);
}

/* Value of the i-th environment entry (the part after '='). */
static inline nova_str nova_os_env_val_at(nova_int i) {
    char** e = NOVA_ENVIRON;
    if (!e || i < 0) return _nova_os_str("");
    const char* s = e[(size_t)i];
    if (!s) return _nova_os_str("");
    const char* eq = strchr(s, '=');
    if (!eq) return _nova_os_str("");
    return _nova_os_str(eq + 1);
}

/* ─── Working directory ─── */

/* Absolute current working directory (empty on error). */
static inline nova_str nova_os_cwd(void) {
    char buf[4096];
#if defined(_WIN32)
    if (_getcwd(buf, (int)sizeof buf)) return _nova_os_str(buf);
#else
    if (getcwd(buf, sizeof buf)) return _nova_os_str(buf);
#endif
    return _nova_os_str("");
}

/* Change the current working directory. 0 or -errno. */
static inline nova_int nova_os_set_cwd(const uint8_t* path) {
#if defined(_WIN32)
    return _chdir((const char*)path) == 0 ? 0 : _nova_os_fail();
#else
    return chdir((const char*)path) == 0 ? 0 : _nova_os_fail();
#endif
}

/* ─── Well-known directories ─── */

/* System temp directory (TMPDIR/TEMP/TMP with a portable fallback). */
static inline nova_str nova_os_temp_dir(void) {
#if defined(_WIN32)
    const char* t = getenv("TEMP");
    if (!t || !*t) t = getenv("TMP");
    if (!t || !*t) t = "C:\\Windows\\Temp";
#else
    const char* t = getenv("TMPDIR");
    if (!t || !*t) t = "/tmp";
#endif
    return _nova_os_str(t);
}

/* User home directory (empty == none). */
static inline nova_str nova_os_home_dir(void) {
#if defined(_WIN32)
    const char* h = getenv("USERPROFILE");
#else
    const char* h = getenv("HOME");
#endif
    return _nova_os_str(h ? h : "");
}

/* ─── Process ─── */

/* Flush stdout/stderr and terminate the process (never returns for real; the
 * declared int return keeps the effect-op shape uniform). */
static inline nova_int nova_os_exit(nova_int code) {
    fflush(stdout);
    fflush(stderr);
    exit((int)code);
    return 0; /* unreachable */
}

/* This process's id. */
static inline nova_int nova_os_pid(void) {
#if defined(_WIN32)
    return (nova_int)_getpid();
#else
    return (nova_int)getpid();
#endif
}

/* Host name (empty on error). */
static inline nova_str nova_os_hostname(void) {
#if defined(_WIN32)
    const char* h = getenv("COMPUTERNAME");
    return _nova_os_str(h ? h : "");
#else
    char buf[256];
    if (gethostname(buf, sizeof buf) == 0) {
        buf[sizeof buf - 1] = 0;
        return _nova_os_str(buf);
    }
    return _nova_os_str("");
#endif
}

#endif /* NOVA_OS_ENV_H */
