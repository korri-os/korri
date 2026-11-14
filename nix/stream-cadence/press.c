/* The press of number 5: a key on the viewer's own window, with no device.
 *
 * An X client of the private pair's X server. It finds the one top-level
 * window that the viewer process made and that asks for key events, checks
 * that the server's keyboard focus is on it, and presses and releases one key
 * through the server's XTEST extension. The server queues that key as it
 * queues a key from a keyboard, and delivers it to the focused window. No
 * device node is opened or made: the press is an X request on the client's
 * socket. input_measure.py matches each press to the input packet that the
 * viewer then sends; this file never reads a packet.
 *
 * Usage: stream-cadence-press VIEWER_PID PRESSES HOLD_MS GAP_MS JITTER_MS SEED
 *
 * One line for each fact, on standard output:
 *   target WINDOW OWNER_PID KEYCODE
 *   press INDEX REALTIME_NS MONOTONIC_NS FOCUSED
 *   release INDEX REALTIME_NS MONOTONIC_NS FOCUSED
 *   done
 * INDEX -1 is a warm-up press that is not measured. A new Xwayland delivers
 * no event of the first key that XTEST sends it (Xwayland 24.1 under a
 * headless sway, 2026-11-13); every later key arrives.
 * A press time is read just before the request leaves for the server.
 * CLOCK_REALTIME is the clock of the kernel's packet times; CLOCK_MONOTONIC,
 * the middle of two reads around it, lets the reader see a step of the wall
 * clock. OWNER_PID is the window's owner from X-Resource. FOCUSED is 1 when the focus
 * was on the target just before the press or release was sent.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <X11/extensions/XRes.h>
#include <X11/extensions/XTest.h>

enum {
    EXIT_USAGE = 2,
    EXIT_DISPLAY = 3,
    EXIT_EXTENSION = 4,
    EXIT_TARGET = 5,
    EXIT_FOCUS = 6,
};

/* How long the viewer's window may take to appear and take the focus. */
#define TARGET_WAIT_NS 5000000000LL
#define TARGET_POLL_NS 50000000LL
/* The warm-up press waits this long after the target is found, and the
 * first measured press this long after the warm-up. */
#define SETTLE_NS 500000000LL
/* An X server repeats a held key after its repeat delay (660 ms by default).
 * A hold this long or longer could add keys that no press sent. */
#define HOLD_LIMIT_MS 500
/* The wall clock is read between two monotonic reads, at most this many
 * times, until the two are this close: the reader sees a step of the wall
 * clock against the monotonic one of a few microseconds. */
#define CLOCK_TRIES 5
#define CLOCK_BRACKET_NS 5000LL

static int x_error;

static int on_x_error(Display *display, XErrorEvent *event) {
    (void)display;
    x_error = event->error_code;
    return 0;
}

static void die(int code, const char *message) {
    fprintf(stderr, "PRESS %s\n", message);
    exit(code);
}

static long long now_ns(clockid_t clock) {
    struct timespec time;
    clock_gettime(clock, &time);
    return (long long)time.tv_sec * 1000000000LL + time.tv_nsec;
}

static void sleep_until(long long monotonic_ns) {
    struct timespec until = {
        .tv_sec = monotonic_ns / 1000000000LL,
        .tv_nsec = monotonic_ns % 1000000000LL,
    };
    while (clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &until, NULL) == EINTR) {
    }
}

static long parse(const char *text, long low, long high) {
    char *end;
    errno = 0;
    long value = strtol(text, &end, 10);
    if (errno || *end || end == text || value < low || value > high)
        die(EXIT_USAGE, "arguments: VIEWER_PID PRESSES HOLD_MS GAP_MS JITTER_MS SEED");
    return value;
}

/* The process that made a window, as the server knows its client. */
static long owner(Display *display, Window window) {
    XResClientIdSpec spec = {.client = window, .mask = XRES_CLIENT_ID_PID_MASK};
    long count = 0;
    XResClientIdValue *values = NULL;
    long pid = -1;
    if (XResQueryClientIds(display, 1, &spec, &count, &values) != Success)
        return -1;
    for (long i = 0; i < count; i++) {
        if (XResGetClientIdType(&values[i]) == XRES_CLIENT_ID_PID)
            pid = XResGetClientPid(&values[i]);
    }
    XResClientIdsDestroy(count, values);
    return pid;
}

/* The viewer's window: viewable, made by the viewer, and asking for key
 * presses. 0 when there is none yet; -1 when there is more than one. */
static long find_target(Display *display, long viewer) {
    Window root_return, parent, *children = NULL;
    unsigned int count = 0;
    long found = 0;
    if (!XQueryTree(display, DefaultRootWindow(display), &root_return, &parent, &children, &count))
        return 0;
    for (unsigned int i = 0; i < count; i++) {
        XWindowAttributes attributes;
        x_error = 0;
        if (!XGetWindowAttributes(display, children[i], &attributes) || x_error)
            continue; /* A window that went between the two requests. */
        if (attributes.map_state != IsViewable || attributes.class != InputOutput)
            continue;
        if (!(attributes.all_event_masks & KeyPressMask))
            continue;
        if (owner(display, children[i]) != viewer)
            continue;
        if (found) {
            found = -1;
            break;
        }
        found = (long)children[i];
    }
    if (children)
        XFree(children);
    return found;
}

static int focused(Display *display, Window target) {
    Window focus;
    int revert;
    XGetInputFocus(display, &focus, &revert);
    return focus == target;
}

static void send_key(Display *display, Window target, KeyCode key, int down,
                     const char *name, long index) {
    int on_target = focused(display, target);
    XTestFakeKeyEvent(display, key, down ? True : False, CurrentTime);
    long long realtime = 0, before = 0, after = 0;
    for (int try = 0; try < CLOCK_TRIES; try++) {
        before = now_ns(CLOCK_MONOTONIC);
        realtime = now_ns(CLOCK_REALTIME);
        after = now_ns(CLOCK_MONOTONIC);
        if (after - before < CLOCK_BRACKET_NS)
            break;
    }
    XFlush(display);
    printf("%s %ld %lld %lld %d\n", name, index, realtime, before + (after - before) / 2, on_target);
}

int main(int argc, char **argv) {
    if (argc != 7)
        die(EXIT_USAGE, "arguments: VIEWER_PID PRESSES HOLD_MS GAP_MS JITTER_MS SEED");
    long viewer = parse(argv[1], 1, 0x7fffffff);
    long presses = parse(argv[2], 1, 10000);
    long hold_ms = parse(argv[3], 1, 10000);
    long gap_ms = parse(argv[4], 1, 60000);
    long jitter_ms = parse(argv[5], 0, 60000);
    unsigned int seed = (unsigned int)parse(argv[6], 0, 0x7fffffff);
    if (hold_ms >= gap_ms)
        die(EXIT_USAGE, "the hold must be shorter than the gap between two presses");
    if (hold_ms >= HOLD_LIMIT_MS)
        die(EXIT_USAGE, "a hold of 500 ms or more lets the X server's auto-repeat add keys");
    setvbuf(stdout, NULL, _IOLBF, 0);

    Display *display = XOpenDisplay(NULL);
    if (!display)
        die(EXIT_DISPLAY, "cannot open the X display named by DISPLAY");
    XSetErrorHandler(on_x_error);
    int event, error, major, minor;
    if (!XTestQueryExtension(display, &event, &error, &major, &minor))
        die(EXIT_EXTENSION, "the X server has no XTEST extension");
    if (!XResQueryExtension(display, &event, &error) || !XResQueryVersion(display, &major, &minor) ||
        major < 1 || (major == 1 && minor < 2))
        die(EXIT_EXTENSION, "the X server has no X-Resource 1.2: it cannot name the owner of a window");

    long target = 0;
    long long deadline = now_ns(CLOCK_MONOTONIC) + TARGET_WAIT_NS;
    while ((target = find_target(display, viewer)) == 0 && now_ns(CLOCK_MONOTONIC) < deadline)
        sleep_until(now_ns(CLOCK_MONOTONIC) + TARGET_POLL_NS);
    if (target == 0)
        die(EXIT_TARGET, "the viewer has no viewable window that asks for key presses");
    if (target < 0)
        die(EXIT_TARGET, "the viewer has more than one window that asks for key presses");
    /* The compositor's window manager gives the focus. One request is made
     * here when it has not; the manager can take the focus back. */
    if (!focused(display, (Window)target)) {
        XSetInputFocus(display, (Window)target, RevertToParent, CurrentTime);
        XSync(display, False);
    }
    while (!focused(display, (Window)target) && now_ns(CLOCK_MONOTONIC) < deadline)
        sleep_until(now_ns(CLOCK_MONOTONIC) + TARGET_POLL_NS);
    if (!focused(display, (Window)target))
        die(EXIT_FOCUS, "the keyboard focus is not on the viewer's window");

    KeyCode key = XKeysymToKeycode(display, XK_a);
    if (!key)
        die(EXIT_EXTENSION, "the X server's keymap has no key for the letter a");
    /* The owner as the server names it, not the PID that this program was given. */
    printf("target %ld %ld %d\n", target, owner(display, (Window)target), key);

    long long next = now_ns(CLOCK_MONOTONIC) + SETTLE_NS;
    for (long index = -1; index < presses; index++) {
        sleep_until(next);
        send_key(display, (Window)target, key, 1, "press", index);
        sleep_until(now_ns(CLOCK_MONOTONIC) + hold_ms * 1000000LL);
        send_key(display, (Window)target, key, 0, "release", index);
        if (index < 0) {
            next += SETTLE_NS;
            continue;
        }
        /* A gap that is not a whole number of any frame period. */
        long extra = jitter_ms ? (long)(rand_r(&seed) % (unsigned int)(jitter_ms + 1)) : 0;
        next += (gap_ms + extra) * 1000000LL;
    }
    XSync(display, False);
    printf("done\n");
    XCloseDisplay(display);
    return 0;
}
