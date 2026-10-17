#define _GNU_SOURCE

#include <dlfcn.h>
#include <errno.h>
#include <linux/input.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <unistd.h>

#include <linux/audit.h>
#include <linux/filter.h>
#include <linux/seccomp.h>
#include <sys/prctl.h>
#include <sys/syscall.h>

#if defined(__aarch64__)
#define KORRI_GUARD_AUDIT_ARCH AUDIT_ARCH_AARCH64
#elif defined(__x86_64__)
#define KORRI_GUARD_AUDIT_ARCH AUDIT_ARCH_X86_64
#endif

static ssize_t (*real_read_fn)(int, void *, size_t) = NULL;
static int (*real_ioctl_fn)(int, unsigned long, void *) = NULL;

static bool env_enabled(const char *name, bool default_value) {
  const char *value = getenv(name);
  if (value == NULL || value[0] == '\0') return default_value;
  return !(strcmp(value, "0") == 0 || strcasecmp(value, "false") == 0 ||
           strcasecmp(value, "no") == 0 || strcasecmp(value, "off") == 0);
}

static bool guard_enabled(void) {
  return env_enabled("KORRI_STEAM_INPUT_GUARD", true);
}

static bool debug_enabled(void) {
  return env_enabled("KORRI_STEAM_INPUT_GUARD_DEBUG", false);
}

// Neutralize EVIOCGRAB at the seccomp layer so the reservation also covers x86
// payloads run under FEX emulation. The libc ioctl() interposer below only sees
// calls that route through glibc's ioctl symbol; FEX issues the emulated game's
// syscalls raw, bypassing that hook. An x86 game can therefore take an
// exclusive grab on the shared InputPlumber virtual pad and starve korri-inputd
// of the Home+shoulder shortcut chords. This filter turns ioctl(_, EVIOCGRAB,
// _) into a no-op success (matching the libc hook) and passes everything else
// through untouched.
//
// Installed from a load-time constructor so it is inherited across fork/exec by
// every descendant in the Steam launch tree, including the native FEX host
// processes that issue the guest's syscalls. It is only installed where
// NO_NEW_PRIVS is already set (i.e. inside the pressure-vessel/bwrap sandbox),
// so it never turns NO_NEW_PRIVS on in a pre-sandbox context that could rely on
// a setuid helper. Any failure is fail-safe: behavior falls back to the libc
// hook alone. Disable with KORRI_STEAM_INPUT_GUARD_SECCOMP=0.
static void install_grab_seccomp(void) {
  if (!guard_enabled()) return;
  if (!env_enabled("KORRI_STEAM_INPUT_GUARD_SECCOMP", true)) return;

#ifdef KORRI_GUARD_AUDIT_ARCH
  if (prctl(PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) != 1) return;

  struct sock_filter filter[] = {
      BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, arch)),
      BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, KORRI_GUARD_AUDIT_ARCH, 0, 7),
      BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr)),
      BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, __NR_ioctl, 0, 5),
      // ioctl request is arg[1]; reject unless the high word is zero so we only
      // match the 32-bit EVIOCGRAB constant and never a wider look-alike.
      BPF_STMT(BPF_LD | BPF_W | BPF_ABS,
               offsetof(struct seccomp_data, args[1]) + 4),
      BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, 0, 0, 3),
      BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, args[1])),
      BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, (uint32_t)EVIOCGRAB, 0, 1),
      BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ERRNO | (0 & SECCOMP_RET_DATA)),
      BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
  };
  struct sock_fprog prog = {
      .len = (unsigned short)(sizeof(filter) / sizeof(filter[0])),
      .filter = filter,
  };

  if (prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &prog, 0, 0) != 0) {
    if (debug_enabled()) {
      fprintf(
          stderr,
          "korri-steam-input-guard: seccomp EVIOCGRAB guard not installed (errno %d)\n",
          errno);
    }
    return;
  }

  if (debug_enabled()) {
    fprintf(stderr,
            "korri-steam-input-guard: seccomp EVIOCGRAB no-op installed\n");
  }
#endif
}

__attribute__((constructor)) static void korri_steam_input_guard_init(void) {
  install_grab_seccomp();
}

static bool is_linux_input_event_fd(int fd) {
  struct stat st;
  if (fstat(fd, &st) != 0) return false;
  if (!S_ISCHR(st.st_mode)) return false;

  // Linux input event nodes are character devices on major 13 with event-node
  // minors starting at 64. This intentionally excludes /dev/uinput (major 10)
  // so Steam can still create its virtual pads.
  return major(st.st_rdev) == 13 && minor(st.st_rdev) >= 64;
}

static void resolve_read(void) {
  if (real_read_fn == NULL) {
    real_read_fn = (ssize_t(*)(int, void *, size_t))dlsym(RTLD_NEXT, "read");
  }
}

static void resolve_ioctl(void) {
  if (real_ioctl_fn == NULL) {
    real_ioctl_fn = (int (*)(int, unsigned long, void *))dlsym(RTLD_NEXT, "ioctl");
  }
}

ssize_t read(int fd, void *buf, size_t count) {
  resolve_read();
  if (real_read_fn == NULL) {
    errno = ENOSYS;
    return -1;
  }

  ssize_t bytes = real_read_fn(fd, buf, count);
  if (bytes <= 0 || !guard_enabled()) return bytes;
  if (((size_t)bytes % sizeof(struct input_event)) != 0) return bytes;
  if (!is_linux_input_event_fd(fd)) return bytes;

  struct input_event *events = (struct input_event *)buf;
  size_t event_count = (size_t)bytes / sizeof(struct input_event);
  size_t filtered = 0;

  for (size_t i = 0; i < event_count; i++) {
    if (events[i].type == EV_KEY && events[i].code == BTN_MODE) {
      // Reserve physical Home/Guide for Korri. Keep the read size stable by
      // replacing the button event with a harmless sync marker instead of
      // shortening the stream; evdev readers already tolerate SYN_REPORT.
      events[i].type = EV_SYN;
      events[i].code = SYN_REPORT;
      events[i].value = 0;
      filtered++;
    }
  }

  if (filtered > 0 && debug_enabled()) {
    fprintf(stderr,
            "korri-steam-input-guard: filtered %zu BTN_MODE event(s) on fd %d\n",
            filtered, fd);
  }

  return bytes;
}

int ioctl(int fd, unsigned long request, ...) {
  if (guard_enabled() && request == EVIOCGRAB && is_linux_input_event_fd(fd)) {
    if (debug_enabled()) {
      fprintf(stderr,
              "korri-steam-input-guard: no-op EVIOCGRAB on input fd %d\n",
              fd);
    }
    return 0;
  }

  // Forward the caller-provided third argument unchanged for every
  // non-intercepted ioctl. Some Linux ioctls (including socket control paths
  // Steam uses while initializing networking/WebUI transport) are encoded with
  // _IOC_NONE even though callers still pass a pointer-sized argument. Dropping
  // that argument turns unrelated non-input ioctls into NULL calls.
  void *arg = NULL;
  va_list ap;
  va_start(ap, request);
  arg = va_arg(ap, void *);
  va_end(ap);

  resolve_ioctl();
  if (real_ioctl_fn == NULL) {
    errno = ENOSYS;
    return -1;
  }
  return real_ioctl_fn(fd, request, arg);
}
