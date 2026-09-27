// Test-only Linux evdev D-pad replay. No grabs, confirmation keys, or raw devices.
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <linux/input.h>
#include <linux/joystick.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <time.h>
#include <unistd.h>

static volatile sig_atomic_t stopped;
static void stop(int signal) { (void)signal; stopped = 1; }
static void delay(long ns) {
    struct timespec duration = { .tv_sec = 0, .tv_nsec = ns };
    while (!stopped && nanosleep(&duration, &duration) < 0 && errno == EINTR) {}
}
static int emit(int fd, int value) {
    struct input_event events[2] = {
        { .type = EV_ABS, .code = ABS_HAT0X, .value = value },
        { .type = EV_SYN, .code = SYN_REPORT, .value = 0 },
    };
    return write(fd, events, sizeof(events)) == (ssize_t)sizeof(events) ? 0 : -1;
}
static int valid_node(const char *path, const char *prefix) {
    if (strncmp(path, prefix, strlen(prefix)) != 0) return 0;
    const char *p = path + strlen(prefix);
    if (!*p) return 0;
    for (; *p; ++p) if (*p < '0' || *p > '9') return 0;
    return 1;
}
static int valid_path(const char *path) { return valid_node(path, "/dev/input/event"); }
static int observed_right, observed_neutral;
static void observe(int fd, int axis) {
    struct js_event event;
    while (read(fd, &event, sizeof(event)) == (ssize_t)sizeof(event)) {
        if (event.type == JS_EVENT_AXIS && event.number == axis) {
            if (event.value > 0) ++observed_right;
            if (event.value == 0) ++observed_neutral;
        }
    }
}
static int neutral(int x, int y, const unsigned char *keys, size_t size) {
    if (x || y) return 0;
    for (size_t i = 0; i < size; ++i) if (keys[i]) return 0;
    return 1;
}
static long long monotonic_us(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) return -1;
    return (long long)now.tv_sec * 1000000 + now.tv_nsec / 1000;
}
int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "--self-test") == 0) {
        if (!valid_path("/dev/input/event14") || valid_path("/dev/input/event") ||
            valid_path("/dev/input/event14/../js0") || valid_path("/dev/input/js7")) return 1;
        int pipes[2];
        if (pipe(pipes) != 0) return 1;
        if (emit(pipes[1], 1) != 0 || emit(pipes[1], 0) != 0) return 1;
        struct input_event events[4];
        if (read(pipes[0], events, sizeof(events)) != (ssize_t)sizeof(events)) return 1;
        if (events[0].type != EV_ABS || events[0].code != ABS_HAT0X || events[0].value != 1 ||
            events[1].type != EV_SYN || events[2].type != EV_ABS || events[2].value != 0 ||
            events[3].type != EV_SYN) return 1;
        close(pipes[0]); close(pipes[1]);
        const unsigned char released[] = {0}, held[] = {1};
        if (!neutral(0, 0, released, 1) || neutral(1, 0, released, 1) ||
            neutral(0, -1, released, 1) || neutral(0, 0, held, 1)) return 1;
        puts("D-pad replay framing, neutral-state and path validation passed");
        return 0;
    }
    if (argc != 3 || !valid_path(argv[1]) || !valid_node(argv[2], "/dev/input/js")) return 2;
    int fd = open(argv[1], O_RDWR | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC);
    if (fd < 0) { perror("open portal target"); return 1; }
    struct stat st;
    struct input_id id;
    char phys[128] = {0}, name[128] = {0};
    struct input_absinfo axis_x, axis_y;
    if (fstat(fd, &st) || !S_ISCHR(st.st_mode) || major(st.st_rdev) != 13 ||
        ioctl(fd, EVIOCGID, &id) < 0 || id.bustype != BUS_USB || id.vendor != 0x045e || id.product != 0x028e ||
        ioctl(fd, EVIOCGPHYS(sizeof(phys)), phys) < 0 || strcmp(phys, "korri/inputd/portal") != 0 ||
        ioctl(fd, EVIOCGNAME(sizeof(name)), name) < 0 || strcmp(name, "Microsoft X-Box 360 pad (Korri portal)") != 0 ||
        ioctl(fd, EVIOCGABS(ABS_HAT0X), &axis_x) < 0 ||
        ioctl(fd, EVIOCGABS(ABS_HAT0Y), &axis_y) < 0) {
        fputs("Refusing anything except a neutral inputd portal target\n", stderr);
        close(fd); return 1;
    }
    unsigned char keys[(KEY_MAX + 8) / 8] = {0};
    if (ioctl(fd, EVIOCGKEY(sizeof(keys)), keys) < 0) { close(fd); return 1; }
    if (!neutral(axis_x.value, axis_y.value, keys, sizeof(keys))) {
        fputs("Refusing replay while a physical button or D-pad direction is held\n", stderr);
        close(fd); return 1;
    }
    int joy = open(argv[2], O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC);
    unsigned char map[ABS_CNT] = {0};
    unsigned char axes = 0;
    char joy_name[128] = {0};
    if (joy < 0 || ioctl(joy, JSIOCGNAME(sizeof(joy_name)), joy_name) < 0 ||
        strcmp(joy_name, name) != 0 || ioctl(joy, JSIOCGAXMAP, map) < 0 || ioctl(joy, JSIOCGAXES, &axes) < 0) {
        fputs("Cannot observe the portal joydev endpoint\n", stderr);
        if (joy >= 0) close(joy);
        close(fd); return 1;
    }
    int hat = -1;
    for (int i = 0; i < axes; ++i) if (map[i] == ABS_HAT0X) hat = i;
    if (hat < 0) { close(joy); close(fd); return 1; }
    observe(joy, hat); // Drain JS_EVENT_INIT, which never counts as replay.
    signal(SIGINT, stop); signal(SIGTERM, stop); signal(SIGHUP, stop);
    printf("REPLAY_BEGIN_MONOTONIC_US=%lld\n", monotonic_us());
    fflush(stdout);
    int result = emit(fd, 0);
    delay(600000000);
    for (int i = 0; i < 4 && !stopped && result == 0; ++i) {
        result = emit(fd, 1);
        delay(180000000);
        observe(joy, hat);
        if (emit(fd, 0) < 0) result = -1;
        delay(650000000);
        observe(joy, hat);
    }
    if (emit(fd, 0) < 0) result = -1;
    close(fd);
    close(joy);
    if (result != 0 || stopped) return 1;
    printf("JOYDEV OBSERVED: right=%d neutral=%d\n", observed_right, observed_neutral);
    if (observed_right != 4 || observed_neutral != 4) {
        fputs("Portal joydev did not receive the complete replay\n", stderr);
        return 1;
    }
    printf("REPLAY_END_MONOTONIC_US=%lld\n", monotonic_us());
    puts("REPLAY COMPLETE: four Right pulses, neutral at exit; no confirmation keys");
    return 0;
}
