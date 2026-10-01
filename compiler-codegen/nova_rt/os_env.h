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
 * `os_set_args(argc, argv)` (spliced by emit_c.rs) into the file-scope
 * argv globals, which os_arg_count/os_arg_at then read.
 *
 * WINDOWS: TEXT FROM THE OS IS READ WIDE AND HANDED OUT AS UTF-8 (registry 221.1
 * #1590). Nova's `str` is UTF-8, but the narrow CRT entry points on Windows give
 * the ANSI code page (cp1251 on a Russian system): `main`'s argv, getenv/_environ,
 * _getcwd -- a Cyrillic argument arrived as `c5 e2 e3` instead of `d0 95 d0 b2 d0 b3`,
 * and a program started from a Cyrillic directory read a wrong cwd. Every point
 * where text comes from the OS here goes through the W API and WideCharToMultiByte
 * (CP_UTF8): argv (GetCommandLineW + CommandLineToArgvW), env get/has/set/remove
 * (_wgetenv/_wputenv_s, keys and values converted from UTF-8), the env snapshot
 * (GetEnvironmentStringsW), cwd (_wgetcwd/_wchdir), temp and home (TEMP/TMP/
 * USERPROFILE through _wgetenv), host name (COMPUTERNAME through _wgetenv). File
 * names (std.fs) go through libuv, which already speaks UTF-8; OS error texts come
 * from uv_strerror (ASCII) -- neither is touched here.
 */
#ifndef NOVA_OS_ENV_H
#define NOVA_OS_ENV_H

#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <stdio.h>
#include <stdint.h>

#if defined(_WIN32)
#  include <direct.h>    /* _wgetcwd, _wchdir */
#  include <process.h>   /* _getpid */
#  include <wchar.h>
#  include <windows.h>   /* already in the TU through uv.h; W API + WideCharToMultiByte */
#  include <shellapi.h>  /* CommandLineToArgvW */
#  pragma comment(lib, "shell32.lib")
#  define NOVA_ENVIRON _environ   /* the non-Windows path below; Windows reads the wide block */
#else
#  include <unistd.h>    /* getcwd, chdir, getpid, gethostname */
extern char **environ;
#  define NOVA_ENVIRON environ
#endif

/* ─── nova_str wrappers (mirror fs.c's _fs_cstr; GC-allocated copy) ─── */

static inline nova_str _os_bytes(const uint8_t* s, nova_int n) {
    nova_str out;
    if (!s || n <= 0) { out.ptr = NULL; out.len = 0; return out; }
    uint8_t* p = (uint8_t*)nova_alloc((size_t)n + 1);
    memcpy(p, s, (size_t)n);
    p[n] = 0;
    out.ptr = (const uint8_t*)p;
    out.len = (nova_int)n;
    return out;
}

static inline nova_str _os_str(const char* s) {
    if (!s) { nova_str z; z.ptr = NULL; z.len = 0; return z; }
    return _os_bytes((const uint8_t*)s, (nova_int)strlen(s));
}

/* map a failed errno to the negated code convention (-errno; -EINVAL fallback) */
static inline nova_int _os_fail(void) {
    int e = errno;
    return e > 0 ? -(nova_int)e : -(nova_int)22;
}

#if defined(_WIN32)
/* #1590: wide OS text -> UTF-8 nova_str (`n` < 0: NUL-terminated). */
static inline nova_str _os_from_wide(const wchar_t* w, int n) {
    if (!w) return _os_str("");
    if (n < 0) n = (int)wcslen(w);
    if (n == 0) return _os_str("");
    int m = WideCharToMultiByte(CP_UTF8, 0, w, n, NULL, 0, NULL, NULL);
    if (m <= 0) return _os_str("");
    uint8_t* p = (uint8_t*)nova_alloc((size_t)m + 1);
    WideCharToMultiByte(CP_UTF8, 0, w, n, (char*)p, m, NULL, NULL);
    p[m] = 0;
    nova_str out; out.ptr = (const uint8_t*)p; out.len = (nova_int)m;
    return out;
}

/* #1590: UTF-8 C string from Nova -> malloc'd wide string (free() it; NULL on error). */
static inline wchar_t* _os_to_wide(const char* s) {
    if (!s) return NULL;
    int n = MultiByteToWideChar(CP_UTF8, 0, s, -1, NULL, 0);
    if (n <= 0) return NULL;
    wchar_t* w = (wchar_t*)malloc((size_t)n * sizeof(wchar_t));
    if (!w) return NULL;
    MultiByteToWideChar(CP_UTF8, 0, s, -1, w, n);
    return w;
}

/* #1590: _wgetenv of a UTF-8 key, as UTF-8 (NULL-ness reported through `found`). */
static inline nova_str _os_wgetenv(const char* key, int* found) {
    wchar_t* wk = _os_to_wide(key);
    const wchar_t* v = wk ? _wgetenv(wk) : NULL;
    free(wk);
    if (found) *found = v != NULL;
    return v ? _os_from_wide(v, -1) : _os_str("");
}
#endif

/* ─── Program arguments (argv) ─── */

static nova_int _nova_argc = 0;
static char**   _nova_argv = NULL;
#if defined(_WIN32)
static wchar_t** _nova_wargv = NULL;   /* #1590: CommandLineToArgvW, kept for the process */
#endif

/* Called once from main() (emit_c.rs) with the process argv. On Windows the narrow
 * argv is in the ANSI code page; the wide command line is the source of truth. */
static inline void nova_os_set_args(int argc, char** argv) {
    _nova_argc = (nova_int)argc;
    _nova_argv = argv;
#if defined(_WIN32)
    int wargc = 0;
    _nova_wargv = CommandLineToArgvW(GetCommandLineW(), &wargc);
    if (_nova_wargv) _nova_argc = (nova_int)wargc;
#endif
}

/* Number of program arguments (argv[0] = program path, included). */
static inline nova_int os_arg_count(void) { return _nova_argc; }

/* The i-th argument (empty out of range). */
static inline nova_str os_arg_at(nova_int i) {
#if defined(_WIN32)
    if (_nova_wargv) {
        if (i < 0 || i >= _nova_argc) return _os_str("");
        return _os_from_wide(_nova_wargv[(size_t)i], -1);
    }
#endif
    if (i < 0 || i >= _nova_argc || !_nova_argv) return _os_str("");
    return _os_str(_nova_argv[(size_t)i]);
}

/* ─── Environment ─── */

/* Raw value bytes for `key` (empty if absent — disambiguate with os_env_has). */
static inline nova_str os_env_get(const uint8_t* key) {
#if defined(_WIN32)
    return _os_wgetenv((const char*)key, NULL);
#else
    const char* v = getenv((const char*)key);
    return _os_str(v ? v : "");
#endif
}

/* 1 if `key` is present, 0 otherwise. */
static inline nova_int os_env_has(const uint8_t* key) {
#if defined(_WIN32)
    int found = 0;
    (void)_os_wgetenv((const char*)key, &found);
    return found ? 1 : 0;
#else
    return getenv((const char*)key) ? 1 : 0;
#endif
}

/* Set `key` = `val` (overwrite). 0 or -errno. */
static inline nova_int os_env_set(const uint8_t* key, const uint8_t* val) {
#if defined(_WIN32)
    wchar_t* wk = _os_to_wide((const char*)key);
    wchar_t* wv = _os_to_wide((const char*)val);
    int rc = (wk && wv) ? _wputenv_s(wk, wv) : EINVAL;
    free(wk); free(wv);
    if (rc != 0) { errno = rc; return _os_fail(); }
    return 0;
#else
    return setenv((const char*)key, (const char*)val, 1) == 0 ? 0 : _os_fail();
#endif
}

/* Remove `key`. 0 or -errno (removing a missing key is success). */
static inline nova_int os_env_remove(const uint8_t* key) {
#if defined(_WIN32)
    wchar_t* wk = _os_to_wide((const char*)key);
    int rc = wk ? _wputenv_s(wk, L"") : EINVAL;
    free(wk);
    if (rc != 0) { errno = rc; return _os_fail(); }
    return 0;
#else
    return unsetenv((const char*)key) == 0 ? 0 : _os_fail();
#endif
}

#if defined(_WIN32)
/* #1590: the i-th visible entry of the wide environment block (entries starting
 * with '=' -- the per-drive cwd records -- are not variables and are skipped, as
 * the CRT's _environ skips them). Returns the entry and its length, or NULL. */
static inline const wchar_t* _os_wenv_entry(wchar_t* block, nova_int i, int* len) {
    nova_int k = 0;
    for (wchar_t* p = block; p && *p; p += wcslen(p) + 1) {
        if (*p == L'=') continue;
        if (k++ == i) { if (len) *len = (int)wcslen(p); return p; }
    }
    return NULL;
}
#endif

/* Number of environment entries (snapshot count of `environ`). */
static inline nova_int os_env_len(void) {
#if defined(_WIN32)
    wchar_t* b = GetEnvironmentStringsW();
    nova_int n = 0;
    for (wchar_t* p = b; p && *p; p += wcslen(p) + 1) if (*p != L'=') n++;
    if (b) FreeEnvironmentStringsW(b);
    return n;
#else
    char** e = NOVA_ENVIRON;
    nova_int n = 0;
    if (!e) return 0;
    while (e[n]) n++;
    return n;
#endif
}

/* Key of the i-th environment entry (the part before '='). */
static inline nova_str os_env_key_at(nova_int i) {
#if defined(_WIN32)
    {
        wchar_t* b = GetEnvironmentStringsW();
        int n = 0;
        const wchar_t* s = i >= 0 ? _os_wenv_entry(b, i, &n) : NULL;
        nova_str out = _os_str("");
        if (s) { const wchar_t* eq = wcschr(s, L'='); out = _os_from_wide(s, eq ? (int)(eq - s) : n); }
        if (b) FreeEnvironmentStringsW(b);
        return out;
    }
#endif
    char** e = NOVA_ENVIRON;
    if (!e || i < 0) return _os_str("");
    const char* s = e[(size_t)i];
    if (!s) return _os_str("");
    const char* eq = strchr(s, '=');
    size_t klen = eq ? (size_t)(eq - s) : strlen(s);
    return _os_bytes((const uint8_t*)s, (nova_int)klen);
}

/* Value of the i-th environment entry (the part after '='). */
static inline nova_str os_env_val_at(nova_int i) {
#if defined(_WIN32)
    {
        wchar_t* b = GetEnvironmentStringsW();
        const wchar_t* s = i >= 0 ? _os_wenv_entry(b, i, NULL) : NULL;
        const wchar_t* eq = s ? wcschr(s, L'=') : NULL;
        nova_str out = eq ? _os_from_wide(eq + 1, -1) : _os_str("");
        if (b) FreeEnvironmentStringsW(b);
        return out;
    }
#endif
    char** e = NOVA_ENVIRON;
    if (!e || i < 0) return _os_str("");
    const char* s = e[(size_t)i];
    if (!s) return _os_str("");
    const char* eq = strchr(s, '=');
    if (!eq) return _os_str("");
    return _os_str(eq + 1);
}

/* ─── Working directory ─── */

/* Absolute current working directory (empty on error). */
static inline nova_str os_cwd(void) {
#if defined(_WIN32)
    wchar_t wbuf[4096];
    if (_wgetcwd(wbuf, (int)(sizeof wbuf / sizeof wbuf[0]))) return _os_from_wide(wbuf, -1);
#else
    char buf[4096];
    if (getcwd(buf, sizeof buf)) return _os_str(buf);
#endif
    return _os_str("");
}

/* Change the current working directory. 0 or -errno. */
static inline nova_int os_set_cwd(const uint8_t* path) {
#if defined(_WIN32)
    wchar_t* wp = _os_to_wide((const char*)path);
    if (!wp) { errno = EINVAL; return _os_fail(); }
    int rc = _wchdir(wp);
    free(wp);
    return rc == 0 ? 0 : _os_fail();
#else
    return chdir((const char*)path) == 0 ? 0 : _os_fail();
#endif
}

/* ─── Well-known directories ─── */

/* System temp directory (TMPDIR/TEMP/TMP with a portable fallback). */
static inline nova_str os_temp_dir(void) {
#if defined(_WIN32)
    nova_str w = _os_wgetenv("TEMP", NULL);
    if (w.len == 0) w = _os_wgetenv("TMP", NULL);
    if (w.len == 0) w = _os_str("C:\\Windows\\Temp");
    return w;
#else
    const char* t = getenv("TMPDIR");
    if (!t || !*t) t = "/tmp";
    return _os_str(t);
#endif
}

/* User home directory (empty == none). */
static inline nova_str os_home_dir(void) {
#if defined(_WIN32)
    return _os_wgetenv("USERPROFILE", NULL);
#else
    const char* h = getenv("HOME");
    return _os_str(h ? h : "");
#endif
}

/* ─── Process ─── */

/* Flush stdout/stderr and terminate the process at once — live fibers are not
 * waited for (221.1 №1418: `exit()` ran the atexit orphan drain and hung on a
 * fiber parked in I/O; see `nova_process_exit_now`, effects.h). Never returns
 * for real; the declared int return keeps the effect-op shape uniform. */
static inline nova_int os_exit(nova_int code) {
    nova_process_exit_now((int)code);
    return 0; /* unreachable */
}

/* This process's id. */
static inline nova_int os_pid(void) {
#if defined(_WIN32)
    return (nova_int)_getpid();
#else
    return (nova_int)getpid();
#endif
}

/* Host name (empty on error). */
static inline nova_str os_hostname(void) {
#if defined(_WIN32)
    return _os_wgetenv("COMPUTERNAME", NULL);
#else
    char buf[256];
    if (gethostname(buf, sizeof buf) == 0) {
        buf[sizeof buf - 1] = 0;
        return _os_str(buf);
    }
    return _os_str("");
#endif
}

#endif /* NOVA_OS_ENV_H */
