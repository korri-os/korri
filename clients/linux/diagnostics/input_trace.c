// Test-only passive syscall witness. Never logs buffers or non-input paths.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/stat.h>
#include <unistd.h>

static _Atomic unsigned char watched[65536];
static int input_path(const char *path) {
    if (!path || strncmp(path, "/dev/input/", 11)) return 0;
    const char *n = path + 11;
    if (!strncmp(n, "event", 5)) n += 5;
    else if (!strncmp(n, "js", 2)) n += 2;
    else return 0;
    if (!*n) return 0;
    for (const char *p = n; *p; ++p) if (*p < '0' || *p > '9' || p - n > 8) return 0;
    return 1;
}
static void record(const char *kind, const char *path, int flags, int result, int error) {
    int saved = errno;
    const char *runtime = getenv("XDG_RUNTIME_DIR");
    if (!runtime || runtime[0] != '/') return;
    char output[512], line[192];
    if (snprintf(output, sizeof(output), "%s/rpmini-input-open.log", runtime) >= (int)sizeof(output)) return;
    int fd = (int)syscall(SYS_openat, AT_FDCWD, output, O_WRONLY | O_APPEND | O_CREAT | O_CLOEXEC | O_NOFOLLOW, 0600);
    if (fd >= 0) {
        int n = snprintf(line, sizeof(line), "pid=%ld tid=%ld %s %s flags=%d result=%d errno=%d\n", (long)getpid(), syscall(SYS_gettid), kind, path ? path : "-", flags, result, error);
        struct stat metadata;
        if (fstat(fd, &metadata) == 0 && metadata.st_size < 1024 * 1024 && n > 0 && n < (int)sizeof(line))
            (void)syscall(SYS_write, fd, line, (size_t)n);
        (void)syscall(SYS_close, fd);
    }
    errno = saved;
}
static int note(const char *kind, const char *path, int flags, int result) {
    int saved = errno;
    if (input_path(path)) {
        if (result >= 0 && result < (int)sizeof(watched)) atomic_store(&watched[result], 1);
        record(kind, path, flags, result, result < 0 ? saved : 0);
    }
    errno = saved;
    return result;
}
static int with_mode(int flags) { return (flags & O_CREAT) || ((flags & O_TMPFILE) == O_TMPFILE); }
#define OPEN_WRAPPER(name) \
int name(const char *path, int flags, ...) { \
    mode_t mode = 0; \
    if (with_mode(flags)) { va_list args; va_start(args, flags); mode = (mode_t)va_arg(args, int); va_end(args); } \
    int (*original)(const char *, int, ...) = dlsym(RTLD_NEXT, #name); \
    if (!original) { errno = ENOSYS; return -1; } \
    return note(#name, path, flags, original(path, flags, mode)); \
}
#define OPENAT_WRAPPER(name) \
int name(int dir, const char *path, int flags, ...) { \
    mode_t mode = 0; \
    if (with_mode(flags)) { va_list args; va_start(args, flags); mode = (mode_t)va_arg(args, int); va_end(args); } \
    int (*original)(int, const char *, int, ...) = dlsym(RTLD_NEXT, #name); \
    if (!original) { errno = ENOSYS; return -1; } \
    return note(#name, path, flags, original(dir, path, flags, mode)); \
}
#define FORTIFIED_WRAPPER(name) \
int name(const char *path, int flags) { \
    int (*original)(const char *, int) = dlsym(RTLD_NEXT, #name); \
    if (!original) { errno = ENOSYS; return -1; } \
    return note(#name, path, flags, original(path, flags)); \
}
OPEN_WRAPPER(open)
OPEN_WRAPPER(open64)
OPENAT_WRAPPER(openat)
OPENAT_WRAPPER(openat64)
FORTIFIED_WRAPPER(__open_2)
FORTIFIED_WRAPPER(__open64_2)
int close(int fd) {
    int trace = fd >= 0 && fd < (int)sizeof(watched) && atomic_exchange(&watched[fd], 0);
    int (*original)(int) = dlsym(RTLD_NEXT, "close");
    if (!original) { errno = ENOSYS; return -1; }
    int result = original(fd);
    int saved = errno;
    if (trace) record("close", "-", fd, result, result < 0 ? saved : 0);
    errno = saved;
    return result;
}
#ifndef TRACE_TEST
__attribute__((constructor)) static void loaded(void) { record("loaded", "-", 0, 0, 0); }
#else
int main(void) {
    if (!input_path("/dev/input/js7") || !input_path("/dev/input/event14") ||
        input_path("/dev/input/event14/secret") || input_path("/dev/input/token-value") ||
        input_path("/dev/input/js") || input_path("/tmp/private")) return 1;
    return 0;
}
#endif
