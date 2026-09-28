// OFF-DEVICE ISOLATED VM ONLY. This is not the handheld replay binary.
// Exercise the exact replay encoder through a separately opened evdev writer
// while the capture reader owns EVIOCGRAB. No existing device is opened.
#define main replay_program_main
#include "native-replay.c"
#undef main
#include <glob.h>
#include <linux/uinput.h>
#include <poll.h>

static int receive_value(int fd, int wanted) {
    int seen = 0;
    long long deadline = monotonic_us() + 1000000;
    while (monotonic_us() < deadline) {
        struct pollfd pfd = {.fd = fd, .events = POLLIN};
        if (poll(&pfd, 1, 50) <= 0) continue;
        struct input_event events[16];
        ssize_t bytes = read(fd, events, sizeof(events));
        if (bytes <= 0) continue;
        for (size_t i = 0; i < (size_t)bytes / sizeof(*events); ++i) {
            if (events[i].type == EV_ABS && events[i].code == ABS_HAT0X && events[i].value == wanted) seen = 1;
            if (seen && events[i].type == EV_SYN && events[i].code == SYN_REPORT) return 1;
        }
    }
    return 0;
}
int main(int argc, char **argv) {
    if (argc != 2 || strcmp(argv[1], "--isolated-vm")) {
        fputs("Run only in an isolated off-device VM: --isolated-vm\n", stderr); return 2;
    }
    int ui = open("/dev/uinput", O_WRONLY | O_NONBLOCK | O_CLOEXEC);
    int capture = -1, writer = -1, other = -1, result = 1;
    if (ui < 0) return 1;
    struct uinput_setup setup = {.id = {.bustype = BUS_VIRTUAL}};
    strcpy(setup.name, "Korri native probe delivery test");
    struct uinput_abs_setup axis = {.code = ABS_HAT0X, .absinfo = {.minimum = -1, .maximum = 1}};
    if (ioctl(ui, UI_SET_EVBIT, EV_ABS) || ioctl(ui, UI_SET_ABSBIT, ABS_HAT0X) ||
        ioctl(ui, UI_ABS_SETUP, &axis) || ioctl(ui, UI_DEV_SETUP, &setup) || ioctl(ui, UI_DEV_CREATE)) goto done;
    char sysname[128] = {0}, pattern[256], node[256];
    if (ioctl(ui, UI_GET_SYSNAME(sizeof(sysname)), sysname) < 0 ||
        !numbered(sysname, "input")) goto done;
    snprintf(pattern, sizeof(pattern), "/sys/devices/virtual/input/%s/event*", sysname);
    for (int i = 0; i < 100 && capture < 0; ++i) {
        glob_t matches = {0};
        if (glob(pattern, 0, NULL, &matches) == 0 && matches.gl_pathc == 1) {
            const char *base = strrchr(matches.gl_pathv[0], '/');
            snprintf(node, sizeof(node), "/dev/input/%s", base + 1);
            capture = open(node, O_RDONLY | O_NONBLOCK | O_CLOEXEC | O_NOFOLLOW);
        }
        globfree(&matches);
        if (capture < 0) delay(10);
    }
    if (capture < 0) goto done;
    writer = open(node, O_WRONLY | O_NONBLOCK | O_CLOEXEC | O_NOFOLLOW);
    other = open(node, O_RDONLY | O_NONBLOCK | O_CLOEXEC | O_NOFOLLOW);
    if (writer < 0 || other < 0 || ioctl(capture, EVIOCGRAB, 1)) goto done;
    for (int i = 0; i < 4; ++i) {
        if (emit(writer, 1) || !receive_value(capture, 1) ||
            emit(writer, 0) || !receive_value(capture, 0)) goto done;
    }
    struct input_event unexpected;
    if (read(other, &unexpected, sizeof(unexpected)) >= 0 || errno != EAGAIN) goto done;
    result = 0;
    puts("GRABBED EVDEV DELIVERY: capture received four Right/release reports; other reader received none");
done:
    if (writer >= 0) { (void)emit(writer, 0); close(writer); }
    if (other >= 0) close(other);
    if (capture >= 0) close(capture);
    (void)ioctl(ui, UI_DEV_DESTROY);
    close(ui);
    return result;
}
