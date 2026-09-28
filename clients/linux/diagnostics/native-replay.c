// TEST ONLY. No grab, raw-device writes, joydev, or browser injection.
// The caller supplies the composite/target/node from actual InputPlumber discovery.
// We re-read the protected mapping against its unique bus owner before each pulse.
#define _XOPEN_SOURCE 700
#include <errno.h>
#include <fcntl.h>
#include <linux/input.h>
#include <limits.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/sysmacros.h>
#include <systemd/sd-bus.h>
#include <time.h>
#include <unistd.h>

#define PROVIDER "org.shadowblip.InputPlumber"
#define COMPOSITE "org.shadowblip.Input.CompositeDevice"
#define TARGET "org.shadowblip.Input.Target"
#define BITS(n) (((n) + 7) / 8)
static const unsigned keys_expected[] = {0x130,0x131,0x133,0x134,0x136,0x137,0x13a,0x13b,0x13c,0x13d,0x13e,0x2c0,0x2c1,0x2c2,0x2c3};
static const unsigned axes_expected[] = {0,1,2,3,4,5,16,17};
static volatile sig_atomic_t stopped;
static int output = -1, dirty;
static void stop(int sig) { (void)sig; stopped = 1; }
static int bit(const unsigned char *bits, unsigned n) { return (bits[n / 8] >> (n % 8)) & 1; }
static int exact_bits(const unsigned char *bits, unsigned count, const unsigned *expected, size_t length) {
    for (unsigned n = 0; n < count; n++) {
        int wanted = 0;
        for (size_t i = 0; i < length; i++) if (expected[i] == n) wanted = 1;
        if (bit(bits, n) != wanted) return 0;
    }
    return 1;
}
static int numbered(const char *path, const char *prefix) {
    size_t len = strlen(prefix);
    if (strlen(path) > 256 || strncmp(path, prefix, len) || !path[len]) return 0;
    for (const char *p = path + len; *p; ++p) if (*p < '0' || *p > '9') return 0;
    return 1;
}
enum SourceKind { SOURCE_OTHER, SOURCE_RAW, SOURCE_SEAT, SOURCE_NORMALIZED };
static enum SourceKind source_kind(const char *name, const char *phys, const char *uniq,
                                  const char *sysfs, const struct input_id *id, int capabilities) {
    if (!strncmp(name, "Korri Seat P", 12) || !strncmp(phys, "korri/", 6)) return SOURCE_SEAT;
    if (strncmp(sysfs, "/sys/devices/virtual/input/", 27)) return SOURCE_RAW;
    if (!strcmp(name, "Microsoft X-Box 360 pad") && !*phys && !*uniq && capabilities &&
        id->bustype == BUS_USB && id->vendor == 0x045e && id->product == 0x028e && id->version == 1)
        return SOURCE_NORMALIZED;
    return SOURCE_OTHER;
}
static int empty_ioctl(int fd, unsigned long request, char *buffer, size_t size) {
    memset(buffer, 0, size);
    if (ioctl(fd, request, buffer) >= 0) return 1;
    // uinput targets have no phys/uniq: evdev returns ENOENT, not an empty string.
    return errno == ENOENT;
}
static int validate_fd(int fd, const char *node) {
    struct stat st, current;
    struct input_id id;
    char name[256] = {0}, phys[256], uniq[256], link[128], sysfs[PATH_MAX];
    unsigned char keys[BITS(KEY_CNT)] = {0}, axes[BITS(ABS_CNT)] = {0}, events[BITS(EV_CNT)] = {0};
    if (fstat(fd, &st) || lstat(node, &current) || !S_ISCHR(st.st_mode) ||
        !S_ISCHR(current.st_mode) || st.st_rdev != current.st_rdev ||
        st.st_ino != current.st_ino || st.st_dev != current.st_dev || major(st.st_rdev) != 13) return 0;
    snprintf(link, sizeof(link), "/sys/dev/char/%u:%u", major(st.st_rdev), minor(st.st_rdev));
    if (!realpath(link, sysfs) || ioctl(fd, EVIOCGID, &id) < 0 ||
        ioctl(fd, EVIOCGNAME(sizeof(name)), name) < 0 ||
        !empty_ioctl(fd, EVIOCGPHYS(sizeof(phys)), phys, sizeof(phys)) ||
        !empty_ioctl(fd, EVIOCGUNIQ(sizeof(uniq)), uniq, sizeof(uniq)) ||
        ioctl(fd, EVIOCGBIT(EV_KEY, sizeof(keys)), keys) < 0 ||
        ioctl(fd, EVIOCGBIT(EV_ABS, sizeof(axes)), axes) < 0 ||
        ioctl(fd, EVIOCGBIT(0, sizeof(events)), events) < 0) return 0;
    name[sizeof(name) - 1] = 0;
    phys[sizeof(phys) - 1] = 0;
    uniq[sizeof(uniq) - 1] = 0;
    int caps = exact_bits(keys, KEY_CNT, keys_expected, sizeof(keys_expected)/sizeof(*keys_expected)) &&
        exact_bits(axes, ABS_CNT, axes_expected, sizeof(axes_expected)/sizeof(*axes_expected)) && bit(events, EV_FF);
    return source_kind(name, phys, uniq, sysfs, &id, caps) == SOURCE_NORMALIZED;
}
static int neutral_snapshot(const unsigned char *keys, size_t key_bytes, const int *axes, size_t count) {
    for (size_t i = 0; i < key_bytes; ++i) if (keys[i]) return 0;
    for (size_t i = 0; i < count; ++i) if (axes[i] != 0) return 0;
    return 1;
}
static int neutral(int fd) {
    unsigned char keys[BITS(KEY_CNT)] = {0};
    int values[sizeof(axes_expected)/sizeof(*axes_expected)];
    if (ioctl(fd, EVIOCGKEY(sizeof(keys)), keys) < 0) return 0;
    for (size_t i = 0; i < sizeof(axes_expected)/sizeof(*axes_expected); ++i) {
        struct input_absinfo axis;
        if (ioctl(fd, EVIOCGABS(axes_expected[i]), &axis) < 0) return 0;
        values[i] = axis.value;
        if ((axes_expected[i] == ABS_HAT0X || axes_expected[i] == ABS_HAT0Y) &&
            (axis.minimum != -1 || axis.maximum != 1)) return 0;
    }
    return neutral_snapshot(keys, sizeof(keys), values, sizeof(values)/sizeof(*values));
}
static int emit(int fd, int value) {
    struct input_event events[2] = {
        { .type = EV_ABS, .code = ABS_HAT0X, .value = value },
        { .type = EV_SYN, .code = SYN_REPORT },
    };
    size_t done = 0;
    while (done < sizeof(events)) {
        ssize_t n = write(fd, (char *)events + done, sizeof(events) - done);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0 || n % sizeof(struct input_event)) return -1;
        done += (size_t)n;
    }
    return 0;
}
static void cleanup(void) {
    if (output >= 0 && dirty) {
        if (emit(output, 0) < 0) fputs("REPLAY CLEANUP FAILED: restart capture/provider before reuse\n", stderr);
    }
    if (output >= 0) close(output);
    output = -1;
}
static void delay(long ms) {
    struct timespec time = {.tv_sec = ms / 1000, .tv_nsec = (ms % 1000) * 1000000};
    while (!stopped && nanosleep(&time, &time) < 0 && errno == EINTR) {}
}
static long long monotonic_us(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now)) return 0;
    return (long long)now.tv_sec * 1000000 + now.tv_nsec / 1000;
}
static int owner(sd_bus *bus, char result[256]) {
    if (stopped) return 0;
    sd_bus_message *reply = NULL;
    const char *name;
    int ok = sd_bus_call_method(bus, "org.freedesktop.DBus", "/org/freedesktop/DBus",
        "org.freedesktop.DBus", "GetNameOwner", NULL, &reply, "s", PROVIDER) >= 0 &&
        sd_bus_message_read(reply, "s", &name) > 0 && strlen(name) < 256 && name[0] == ':';
    if (ok) strcpy(result, name);
    sd_bus_message_unref(reply);
    return ok;
}
static int property(sd_bus *bus, const char *owner_name, const char *path, const char *interface,
                    const char *name, const char *type, sd_bus_message **reply) {
    if (stopped) return 0;
    return sd_bus_call_method(bus, owner_name, path, "org.freedesktop.DBus.Properties", "Get",
        NULL, reply, "ss", interface, name) >= 0 && sd_bus_message_enter_container(*reply, 'v', type) > 0;
}
static int array_contains(sd_bus *bus, const char *owner_name, const char *path, const char *interface,
                          const char *name, const char *wanted, int exactly_one) {
    sd_bus_message *reply = NULL;
    const char *value;
    int found = 0, count = 0, status = -1;
    if (property(bus, owner_name, path, interface, name, "as", &reply) &&
        sd_bus_message_enter_container(reply, 'a', "s") > 0) {
        while ((status = sd_bus_message_read(reply, "s", &value)) > 0) {
            if (++count > 256 || strlen(value) > 4096) { status = -1; break; }
            if (!strcmp(value, wanted)) ++found;
        }
    }
    sd_bus_message_unref(reply);
    return status == 0 && found == 1 && (!exactly_one || count == 1);
}
static int mapping(sd_bus *bus, const char *unique, const char *composite, const char *target, const char *node) {
    char current[256];
    sd_bus_message *reply = NULL;
    const char *type;
    int ok = owner(bus, current) && !strcmp(unique, current) &&
        array_contains(bus, unique, composite, COMPOSITE, "TargetDevices", target, 0) &&
        array_contains(bus, unique, target, TARGET, "DevicePaths", node, 1) &&
        property(bus, unique, target, TARGET, "DeviceType", "s", &reply) &&
        sd_bus_message_read(reply, "s", &type) > 0 && (!strcmp(type, "xb360") || !strcmp(type, "gamepad"));
    sd_bus_message_unref(reply);
    return ok && owner(bus, current) && !strcmp(unique, current);
}
static int self_test(void) {
    struct input_id id = {.bustype=BUS_USB, .vendor=0x045e, .product=0x028e, .version=1};
    const char *name = "Microsoft X-Box 360 pad", *virtual = "/sys/devices/virtual/input/input9/event12";
    if (source_kind(name, "", "", virtual, &id, 1) != SOURCE_NORMALIZED ||
        source_kind(name, "", "", "/sys/devices/platform/input/input9/event12", &id, 1) != SOURCE_RAW ||
        source_kind("Korri Seat P5", "", "", virtual, &id, 1) != SOURCE_SEAT ||
        source_kind(name, "korri/inputd/portal", "", virtual, &id, 1) == SOURCE_NORMALIZED ||
        source_kind(name, "", "physical", virtual, &id, 1) == SOURCE_NORMALIZED ||
        source_kind(name, "", "", virtual, &id, 0) == SOURCE_NORMALIZED ||
        !numbered("/dev/input/event12", "/dev/input/event") ||
        numbered("/dev/input/event12/../event3", "/dev/input/event") ||
        numbered("/dev/input/js1", "/dev/input/event")) return 1;
    unsigned char keys[BITS(KEY_CNT)] = {0};
    int axes[8] = {0};
    if (!neutral_snapshot(keys, sizeof(keys), axes, 8)) return 1;
    for (size_t i = 0; i < 8; ++i) {
        axes[i] = 1;
        if (neutral_snapshot(keys, sizeof(keys), axes, 8)) return 1;
        axes[i] = 0;
    }
    keys[BTN_SOUTH / 8] = 1 << (BTN_SOUTH % 8);
    if (neutral_snapshot(keys, sizeof(keys), axes, 8)) return 1;
    id.version = 2;
    if (source_kind(name, "", "", virtual, &id, 1) == SOURCE_NORMALIZED) return 1;
    int pipes[2];
    struct input_event events[4];
    if (pipe(pipes) || emit(pipes[1], 1) || emit(pipes[1], 0) ||
        read(pipes[0], events, sizeof(events)) != (ssize_t)sizeof(events)) return 1;
    close(pipes[0]); close(pipes[1]);
    if (events[0].type != EV_ABS || events[0].code != ABS_HAT0X || events[0].value != 1 ||
        events[1].type != EV_SYN || events[1].code != SYN_REPORT || events[2].value != 0 ||
        events[2].type != EV_ABS || events[2].code != ABS_HAT0X || events[3].type != EV_SYN) return 1;
    puts("native replay SourceKind/path/framing self-test passed; no kernel delivery claim");
    return 0;
}
int main(int argc, char **argv) {
    if (argc == 2 && !strcmp(argv[1], "--self-test")) return self_test();
    if (argc != 4 || !numbered(argv[1], "/org/shadowblip/InputPlumber/CompositeDevice") ||
        strncmp(argv[2], "/org/shadowblip/InputPlumber/devices/target/", strlen("/org/shadowblip/InputPlumber/devices/target/")) ||
        !sd_bus_object_path_is_valid(argv[2]) || !numbered(argv[3], "/dev/input/event")) {
        fputs("usage: korri-replay-native-dpad COMPOSITE_OBJECT TARGET_OBJECT EVENT_NODE\n", stderr); return 2;
    }
    struct sigaction action = {.sa_handler = stop};
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGINT, &action, NULL) || sigaction(SIGTERM, &action, NULL) ||
        sigaction(SIGHUP, &action, NULL) || sigaction(SIGALRM, &action, NULL) || atexit(cleanup)) return 1;
    alarm(15);
    sd_bus *bus = NULL;
    char unique[256];
    int ok = sd_bus_open_system(&bus) >= 0 && sd_bus_set_method_call_timeout(bus, 1000000) >= 0 &&
        owner(bus, unique) && mapping(bus, unique, argv[1], argv[2], argv[3]);
    if (ok) output = open(argv[3], O_RDWR | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC);
    if (!ok || output < 0 || !validate_fd(output, argv[3]) || !neutral(output) ||
        !mapping(bus, unique, argv[1], argv[2], argv[3])) {
        fputs("REFUSED: need a live protected normalized target mapping and completely neutral input\n", stderr);
        sd_bus_unref(bus); return 1;
    }
    printf("REPLAY_BEGIN_MONOTONIC_US=%lld\n", monotonic_us()); fflush(stdout);
    delay(600);
    int pulses = 0;
    for (; pulses < 4 && !stopped; ++pulses) {
        if (!mapping(bus, unique, argv[1], argv[2], argv[3]) || !validate_fd(output, argv[3]) || !neutral(output)) { ok = 0; break; }
        if (stopped) break;
        dirty = 1; // Before write: a partial write still requires release.
        if (emit(output, 1)) { ok = 0; break; }
        delay(180);
        if (emit(output, 0)) { ok = 0; break; }
        dirty = 0;
        delay(650);
    }
    if (!neutral(output)) ok = 0;
    sd_bus_unref(bus);
    if (!ok || stopped || pulses != 4) return 1;
    printf("REPLAY_END_MONOTONIC_US=%lld\n", monotonic_us());
    puts("REPLAY WRITES COMPLETE: four Right pulses; kernel/browser delivery NOT asserted");
    return 0;
}
