// SPDX-License-Identifier: GPL-3.0-or-later
// The X11 software picture (video/egl.c) on a real Xvfb window with Mesa's
// software EGL: decoded frames whose size changes in the middle of the stream
// (#476) fit the window between black bars, and each frame's padded rows are
// read at their own length. The window's pixels come from the X server.
#include "video/egl.h"
#include "place.h"
#include "bridge.h"
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { WIDTH = 128, HEIGHT = 72 };

// A frame whose left half is bright and right half dark, in every row. Rows
// read at the wrong length would slant that edge.
static AVFrame* halves(int width, int height) {
  AVFrame* frame = av_frame_alloc();
  assert(frame);
  frame->format = AV_PIX_FMT_YUV420P;
  frame->width = width;
  frame->height = height;
  assert(av_frame_get_buffer(frame, 64) == 0);
  for (int y = 0; y < height; ++y)
    for (int x = 0; x < frame->linesize[0]; ++x)
      frame->data[0][y * frame->linesize[0] + x] = x < width / 2 ? 235 : x < width ? 40 : 255;
  for (int plane = 1; plane < 3; ++plane)
    memset(frame->data[plane], 128, (size_t)frame->linesize[plane] * ((height + 1) / 2));
  return frame;
}

static unsigned long pixel(XImage* image, int x, int y) {
  return XGetPixel(image, x, y) & 0xffffff;
}

static int brightness(unsigned long rgb) {
  return (int)((rgb >> 16) & 0xff);
}

static void shows(Display* display, Window window, AVFrame* frame) {
  struct picture_place place = picture_fit(frame->width, frame->height, WIDTH, HEIGHT);
  egl_draw(frame, place);
  XSync(display, False);
  XImage* image = XGetImage(display, window, 0, 0, WIDTH, HEIGHT, AllPlanes, ZPixmap);
  assert(image);
  for (int y = 0; y < HEIGHT; ++y) {
    for (int x = 0; x < WIDTH; ++x) {
      bool inside = x >= place.x && x < place.x + place.width &&
                    y >= place.y && y < place.y + place.height;
      if (!inside) {
        if (pixel(image, x, y) != 0) {
          fprintf(stderr, "%dx%d: (%d, %d) outside the picture is %06lx\n",
              frame->width, frame->height, x, y, pixel(image, x, y));
          assert(0);
        }
        continue;
      }
      // Two pixels of margin at the picture's edges and at its middle, where
      // the filter blends.
      int across = x - place.x;
      if (across < 2 || across >= place.width - 2 || abs(2 * across - place.width) < 4)
        continue;
      int value = brightness(pixel(image, x, y));
      bool left = 2 * across < place.width;
      if (left ? value < 200 : value > 80) {
        fprintf(stderr, "%dx%d: (%d, %d) in the picture's %s half has brightness %d\n",
            frame->width, frame->height, x, y, left ? "left" : "right", value);
        assert(0);
      }
    }
  }
  XDestroyImage(image);
  int width = 0, height = 0;
  assert(picture_shown_size(&width, &height) == 1);
  assert(width == frame->width && height == frame->height);
}

int main(void) {
  Display* display = XOpenDisplay(NULL);
  assert(display);
  Window window = XCreateSimpleWindow(display, DefaultRootWindow(display), 0, 0, WIDTH, HEIGHT, 0, 0, 0);
  XMapWindow(display, window);
  XSync(display, False);
  egl_init(display, window, WIDTH, HEIGHT);

  // The stream's own shape fills the window; then the stream turns portrait,
  // and back; then it takes a size that is not the start's.
  AVFrame* landscape = halves(WIDTH, HEIGHT);
  AVFrame* portrait = halves(72, 128);
  AVFrame* smaller = halves(96, 54);
  // Padded rows, as a decoder gives them.
  assert(portrait->linesize[0] > portrait->width && smaller->linesize[0] > smaller->width);
  shows(display, window, landscape);
  shows(display, window, portrait);
  shows(display, window, landscape);
  shows(display, window, smaller);
  av_frame_free(&landscape);
  av_frame_free(&portrait);
  av_frame_free(&smaller);
  egl_destroy();
  XDestroyWindow(display, window);
  XCloseDisplay(display);
  puts("the X11 software picture fits each new stream size between black bars");
  return 0;
}
