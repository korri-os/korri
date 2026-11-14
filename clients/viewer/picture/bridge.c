// SPDX-License-Identifier: GPL-3.0-or-later
// Only picture-loop glue. Rust owns the connection, signals and its lifetime.
#include "bridge.h"
#include "video/video.h"
#include "sdl.h"
#include "loop.h"
#include <poll.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>

static int using_vaapi;
#ifdef HAVE_V4L2M2M
// The handheld path. Moonlight declares the v4l2request decoder under
// HAVE_V4L2M2M; this build carries only that decoder from v4l2m2m.c.
static int using_request;
#else
enum { using_request = 0 };
#endif
static struct pollfd* fds;
static FdHandler* handlers;
static int count;

void loop_add_fd(int fd, FdHandler handler, int events) {
  fds = realloc(fds, sizeof(*fds) * (count + 1));
  handlers = realloc(handlers, sizeof(*handlers) * (count + 1));
  if (!fds || !handlers) abort();
  fds[count] = (struct pollfd){.fd = fd, .events = events};
  handlers[count++] = handler;
}
void loop_remove_fd(int fd) {
  for (int i = 0; i < count; ++i) {
    if (fds[i].fd == fd) {
      --count;
      fds[i] = fds[count];
      handlers[i] = handlers[count];
      return;
    }
  }
}
int picture_init(int vaapi, int width, int height, int fullscreen) {
  using_vaapi = vaapi && x11_init(false, true) == INIT_VAAPI;
  if (using_vaapi) {
    puts("Platform X Window System (VAAPI)");
  } else {
    if (vaapi) puts("No VA display found");
    puts("Platform SDL");
    sdl_init(width, height, fullscreen);
  }
  return using_vaapi;
}
#ifdef HAVE_V4L2M2M
// Moonlight's main.c with -platform v4l2request: no SDL or X11 setup here,
// because the decoder's display thread makes its own window. main.c sets
// this variable for every platform; only the v4l2request presenter reads it.
void picture_init_request(int auto_window_resize) {
  using_request = 1;
  setenv("MOONLIGHT_AUTO_WINDOW_RESIZE", auto_window_resize ? "1" : "0", 1);
  puts("Platform V4L2 Request (stateless) + DRM PRIME GL");
}
#endif
PDECODER_RENDERER_CALLBACKS picture_callbacks(void) {
#ifdef HAVE_V4L2M2M
  if (using_request) return &decoder_callbacks_v4l2request;
#endif
  return using_vaapi ? &decoder_callbacks_x11_vaapi : &decoder_callbacks_sdl;
}
// X11 VA-API and v4l2request are Moonlight's embedded platforms: they run
// its descriptor loop. Only the SDL platform runs the SDL event loop.
int picture_poll(int timeout) {
  if (!using_vaapi && !using_request) return sdl_poll(timeout);
  int result = poll(fds, count, timeout);
  if (result < 0) return errno == EINTR ? 0 : -1;
  for (int i = 0; i < count; ++i) {
    if (!fds[i].revents) continue;
    int handled = handlers[i](fds[i].fd);
    // A negative result is a handler failure, such as an evdev read error.
    if (handled < 0) return -1;
    if (handled == LOOP_RETURN) return 1;
  }
  return 0;
}
void picture_destroy(void) {
  if (!using_vaapi && !using_request) sdl_destroy();
#ifdef HAVE_V4L2M2M
  using_request = 0;
#endif
  free(fds);
  free(handlers);
  fds = NULL;
  handlers = NULL;
  count = 0;
}
// Rust cannot define a C-variadic function. No session logic belongs here.
void picture_log(const char* format, ...) {
  va_list args;
  va_start(args, format);
  vprintf(format, args);
  va_end(args);
  fflush(stdout);
}
