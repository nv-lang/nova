/* io_console.h — std/io console byte hooks (Plan 176 Ф.1, D322 §3c).
 *
 * fd-based byte I/O for the `Io` effect's real handler (real_io):
 *   fd 0 = stdin, 1 = stdout, 2 = stderr.
 * Return value: bytes transferred (>= 0); read returns 0 at EOF; on error a
 * NEGATIVE value == -errno (the Nova side maps it via IoError.from_os(-rc)).
 *
 * Implemented over the C stdio FILE* streams (portable across clang/gcc/MSVC);
 * the higher-level fs layer (uv_fs_*, Plan 176 Ф.2) is separate.
 */
#ifndef NOVA_IO_CONSOLE_H
#define NOVA_IO_CONSOLE_H

#include <stdio.h>
#include <errno.h>
#include <stdint.h>

/* Write `len` bytes from `buf` to stdout (fd 1) or stderr (fd 2 → stderr, any
 * other fd → stdout). Returns bytes written, or -errno on a stream error. */
static inline int64_t io_write_fd(int64_t fd, const uint8_t* buf, int64_t len) {
    if (len <= 0) return 0;
    FILE* f = (fd == 2) ? stderr : stdout;
    size_t w = fwrite((const void*)buf, 1, (size_t)len, f);
    if (w < (size_t)len) {
        if (ferror(f)) {
            int e = errno;
            clearerr(f);
            return e > 0 ? -(int64_t)e : -1;
        }
    }
    return (int64_t)w;
}

/* Read up to `len` bytes into `buf` from stdin (fd 0; other fds unsupported →
 * treated as stdin). Returns bytes read, 0 at EOF, or -errno on error. */
static inline int64_t io_read_fd(int64_t fd, uint8_t* buf, int64_t len) {
    (void)fd;
    if (len <= 0) return 0;
    size_t r = fread((void*)buf, 1, (size_t)len, stdin);
    if (r == 0) {
        if (feof(stdin)) return 0;
        int e = errno;
        clearerr(stdin);
        return e > 0 ? -(int64_t)e : -1;
    }
    return (int64_t)r;
}

#endif /* NOVA_IO_CONSOLE_H */
