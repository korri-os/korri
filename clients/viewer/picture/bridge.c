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
static struct pollfd fds[16];
static FdHandler handlers[16];
static int count;

void loop_add_fd(int fd, FdHandler handler, int events) {
  if (count == 16) abort();
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
PDECODER_RENDERER_CALLBACKS picture_callbacks(void) {
  return using_vaapi ? &decoder_callbacks_x11_vaapi : &decoder_callbacks_sdl;
}
int picture_poll(int timeout) {
  if (!using_vaapi) return sdl_poll(timeout);
  int result = poll(fds, count, timeout);
  if (result < 0) return errno == EINTR ? 0 : -1;
  for (int i = 0; i < count; ++i) {
    if (fds[i].revents && handlers[i](fds[i].fd) == LOOP_RETURN) return 1;
  }
  return 0;
}
void picture_destroy(void) {
  if (!using_vaapi) sdl_destroy();
}
// Rust cannot define a C-variadic function. No session logic belongs here.
void picture_log(const char* format, ...) {
  va_list args;
  va_start(args, format);
  vprintf(format, args);
  va_end(args);
  fflush(stdout);
}
