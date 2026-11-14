/* A test X client with the window shape of Moonlight's X11 platforms.
 *
 * Usage: key-window EVENTS keys|nokeys|two
 *   keys    one top-level window that asks for key presses (video/x11.c)
 *   nokeys  one top-level window that asks for none
 *   two     two windows that ask for key presses
 * Prints "ready WINDOW", then one line for each key event that reaches the
 * window: "key press|release KEYCODE REALTIME_NS SEND_EVENT". Ends after
 * EVENTS key events, or after 20 seconds.
 */
#define _GNU_SOURCE
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <X11/Xlib.h>

static Window make(Display *display, long mask) {
    /* A painted background: a window with no content is no surface that a
     * Wayland compositor maps, and it never gets the focus there. */
    XSetWindowAttributes attributes = {
        .event_mask = mask,
        .background_pixel = BlackPixel(display, DefaultScreen(display)),
    };
    Window window = XCreateWindow(display, DefaultRootWindow(display), 0, 0, 64, 64, 0,
                                  CopyFromParent, InputOutput, CopyFromParent,
                                  CWEventMask | CWBackPixel, &attributes);
    XMapWindow(display, window);
    return window;
}

int main(int argc, char **argv) {
    if (argc != 3)
        return 2;
    long events = strtol(argv[1], NULL, 10);
    Display *display = XOpenDisplay(NULL);
    if (!display)
        return 3;
    long keys = KeyPressMask | KeyReleaseMask | StructureNotifyMask;
    Window window = make(display, strcmp(argv[2], "nokeys") ? keys : StructureNotifyMask);
    if (!strcmp(argv[2], "two"))
        make(display, keys);
    for (;;) {
        XEvent event;
        XNextEvent(display, &event);
        if (event.type == MapNotify && event.xmap.window == window)
            break;
    }
    /* No window manager under Xvfb: the client takes the focus itself, as a
     * window manager gives it to a new window. */
    XSetInputFocus(display, window, RevertToParent, CurrentTime);
    XSync(display, False);
    setvbuf(stdout, NULL, _IOLBF, 0);
    printf("ready %lu\n", window);
    struct timespec start;
    clock_gettime(CLOCK_MONOTONIC, &start);
    long seen = 0;
    while (seen < events) {
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        if (now.tv_sec - start.tv_sec > 20)
            return 4;
        if (!XPending(display)) {
            struct pollfd fd = {.fd = ConnectionNumber(display), .events = POLLIN};
            poll(&fd, 1, 100);
            continue;
        }
        XEvent event;
        XNextEvent(display, &event);
        if (event.type != KeyPress && event.type != KeyRelease)
            continue;
        struct timespec when;
        clock_gettime(CLOCK_REALTIME, &when);
        printf("key %s %u %lld %d\n", event.type == KeyPress ? "press" : "release",
               event.xkey.keycode, (long long)when.tv_sec * 1000000000LL + when.tv_nsec,
               event.xkey.send_event ? 1 : 0);
        seen++;
    }
    XCloseDisplay(display);
    return 0;
}
