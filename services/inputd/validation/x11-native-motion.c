#define _POSIX_C_SOURCE 200809L

#include <X11/Xlib.h>
#include <errno.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static volatile sig_atomic_t running = 1;

static void stop(int signal_number) {
  (void) signal_number;
  running = 0;
}

static int parse_positive(const char *value, const char *name) {
  char *end = NULL;
  long parsed = strtol(value, &end, 10);
  if (!value[0] || !end || *end || parsed < 1 || parsed > 10000) {
    fprintf(stderr, "invalid %s: %s\n", name, value);
    exit(2);
  }
  return (int) parsed;
}

static struct timespec add_nanoseconds(struct timespec value, int64_t nanoseconds) {
  value.tv_nsec += nanoseconds;
  while (value.tv_nsec >= 1000000000L) {
    value.tv_nsec -= 1000000000L;
    value.tv_sec += 1;
  }
  return value;
}

static double elapsed_seconds(struct timespec start, struct timespec end) {
  return (double) (end.tv_sec - start.tv_sec) +
         (double) (end.tv_nsec - start.tv_nsec) / 1000000000.0;
}

static void request_fullscreen(Display *display, Window window, Window root) {
  Atom state = XInternAtom(display, "_NET_WM_STATE", False);
  Atom fullscreen = XInternAtom(display, "_NET_WM_STATE_FULLSCREEN", False);
  XEvent event;
  memset(&event, 0, sizeof(event));
  event.type = ClientMessage;
  event.xclient.window = window;
  event.xclient.message_type = state;
  event.xclient.format = 32;
  event.xclient.data.l[0] = 1;
  event.xclient.data.l[1] = (long) fullscreen;
  event.xclient.data.l[3] = 1;
  XSendEvent(
    display,
    root,
    False,
    SubstructureRedirectMask | SubstructureNotifyMask,
    &event
  );
}

static const unsigned long band_colors[] = {
  0x171926,
  0x202c55,
  0x174f55,
  0x5b3b66,
  0x60471f,
  0x263f25,
};

#define BAND_COUNT ((int) (sizeof(band_colors) / sizeof(band_colors[0])))

static void draw_background(Display *display, Window window, GC gc, int width, int height) {
  int band_width = (width + BAND_COUNT - 1) / BAND_COUNT;
  for (int index = 0; index < BAND_COUNT; ++index) {
    XSetForeground(display, gc, band_colors[index]);
    XFillRectangle(display, window, gc, index * band_width, 0, (unsigned int) band_width, (unsigned int) height);
  }
}

/*
 * Repaint the banded background inside one rectangle. Every moving object
 * erases itself this way, so the window never needs a full repaint and the
 * per-frame cost stays proportional to what actually moved.
 */
static void restore_background(
  Display *display,
  Window window,
  GC gc,
  int width,
  int x,
  int y,
  int w,
  int h
) {
  if (w <= 0 || h <= 0) return;
  int band_width = (width + BAND_COUNT - 1) / BAND_COUNT;
  for (int index = 0; index < BAND_COUNT; ++index) {
    int band_start = index * band_width;
    int band_end = band_start + band_width;
    int start = x > band_start ? x : band_start;
    int end = (x + w) < band_end ? (x + w) : band_end;
    if (end <= start) continue;
    XSetForeground(display, gc, band_colors[index]);
    XFillRectangle(display, window, gc, start, y, (unsigned int) (end - start), (unsigned int) h);
  }
}

/*
 * Seven-segment digits drawn from filled rectangles. A core X font would be
 * easier, but it is neither guaranteed to be present nor large enough to stay
 * legible once the frame has been encoded, scaled and shown on a phone.
 */
static const unsigned char digit_segments[10] = {
  /* 0 */ 0x3F, /* 1 */ 0x06, /* 2 */ 0x5B, /* 3 */ 0x4F, /* 4 */ 0x66,
  /* 5 */ 0x6D, /* 6 */ 0x7D, /* 7 */ 0x07, /* 8 */ 0x7F, /* 9 */ 0x6F,
};

static void draw_digit(
  Display *display,
  Window window,
  GC gc,
  int value,
  int x,
  int y,
  int w,
  int h,
  int thickness
) {
  if (value < 0 || value > 9) return;
  unsigned char mask = digit_segments[value];
  int mid = y + (h - thickness) / 2;
  /* a, b, c, d, e, f, g as x, y, width, height. */
  const int segments[7][4] = {
    {x, y, w, thickness},
    {x + w - thickness, y, thickness, h / 2},
    {x + w - thickness, mid, thickness, h - (mid - y)},
    {x, y + h - thickness, w, thickness},
    {x, mid, thickness, h - (mid - y)},
    {x, y, thickness, h / 2},
    {x, mid, w, thickness},
  };
  for (int index = 0; index < 7; ++index) {
    if (!(mask & (1u << index))) continue;
    XFillRectangle(
      display,
      window,
      gc,
      segments[index][0],
      segments[index][1],
      (unsigned int) segments[index][2],
      (unsigned int) segments[index][3]
    );
  }
}

/* Renders a rate as three digits and one decimal, for example 59.9. */
static void draw_rate(
  Display *display,
  Window window,
  GC gc,
  double rate,
  int x,
  int y,
  int digit_width,
  int digit_height,
  int thickness
) {
  int scaled = (int) (rate * 10.0 + 0.5);
  if (scaled < 0) scaled = 0;
  if (scaled > 9999) scaled = 9999;
  int digits[3] = {(scaled / 1000) % 10, (scaled / 100) % 10, (scaled / 10) % 10};
  int gap = digit_width / 4;
  int cursor = x;
  for (int index = 0; index < 3; ++index) {
    if (index == 0 && digits[0] == 0) {
      cursor += digit_width + gap;
      continue;
    }
    draw_digit(display, window, gc, digits[index], cursor, y, digit_width, digit_height, thickness);
    cursor += digit_width + gap;
  }
  XFillRectangle(
    display,
    window,
    gc,
    cursor,
    y + digit_height - thickness,
    (unsigned int) thickness,
    (unsigned int) thickness
  );
  cursor += thickness + gap;
  draw_digit(display, window, gc, scaled % 10, cursor, y, digit_width, digit_height, thickness);
}

int main(int argc, char **argv) {
  int fullscreen = 0;
  int show_rate = 0;
  int busy = 0;
  for (int index = 4; index < argc; ++index) {
    if (strcmp(argv[index], "--fullscreen") == 0) {
      fullscreen = 1;
    } else if (strcmp(argv[index], "--show-fps") == 0) {
      show_rate = 1;
    } else if (strcmp(argv[index], "--busy") == 0) {
      busy = 1;
    } else {
      fprintf(stderr, "usage: %s WIDTH HEIGHT FPS [--fullscreen] [--show-fps] [--busy]\n", argv[0]);
      return 2;
    }
  }
  if (argc < 4) {
    fprintf(stderr, "usage: %s WIDTH HEIGHT FPS [--fullscreen] [--show-fps] [--busy]\n", argv[0]);
    return 2;
  }

  int requested_width = parse_positive(argv[1], "width");
  int requested_height = parse_positive(argv[2], "height");
  int fps = parse_positive(argv[3], "fps");
  int64_t frame_nanoseconds = 1000000000LL / fps;

  /* Extra movers for --busy. Sized so each frame changes a large area. */
  enum { MOVER_LIMIT = 10 };
  struct mover {
    int x, y, dx, dy, size;
    int previous_x, previous_y;
    unsigned long color;
  } movers[MOVER_LIMIT];
  int mover_count = busy ? MOVER_LIMIT : 0;

  signal(SIGINT, stop);
  signal(SIGTERM, stop);

  Display *display = XOpenDisplay(NULL);
  if (!display) {
    fprintf(stderr, "could not open X display\n");
    return 1;
  }

  int screen = DefaultScreen(display);
  Window root = RootWindow(display, screen);
  Window window = XCreateSimpleWindow(
    display,
    root,
    0,
    0,
    (unsigned int) requested_width,
    (unsigned int) requested_height,
    0,
    BlackPixel(display, screen),
    BlackPixel(display, screen)
  );
  XStoreName(display, window, "Korri streaming gate");
  XSelectInput(display, window, ExposureMask | StructureNotifyMask);
  XMapWindow(display, window);
  if (fullscreen) {
    request_fullscreen(display, window, root);
  }

  GC gc = XCreateGC(display, window, 0, NULL);
  int width = requested_width;
  int height = requested_height;
  int redraw = 1;
  int previous_x = -1;
  uint64_t frame = 0;
  uint64_t interval_frames = 0;
  double measured_rate = 0.0;
  double drawn_rate = -1.0;
  struct timespec next;
  struct timespec interval_start;
  clock_gettime(CLOCK_MONOTONIC, &next);
  interval_start = next;

  static const unsigned long mover_colors[] = {
    0xff5f56, 0xffbd2e, 0x27c93f, 0x3fa7ff, 0xc678dd, 0x56d7d7,
  };
  for (int index = 0; index < mover_count; ++index) {
    int size = requested_height / 14;
    if (size < 24) size = 24;
    movers[index].size = size;
    movers[index].x = (requested_width - size) * (index + 1) / (mover_count + 1);
    movers[index].y = (requested_height - size) * ((index * 7) % 11 + 1) / 12;
    movers[index].dx = (index % 2 ? 1 : -1) * (3 + index % 5);
    movers[index].dy = (index % 3 ? 1 : -1) * (2 + (index * 3) % 6);
    movers[index].previous_x = -1;
    movers[index].previous_y = -1;
    movers[index].color = mover_colors[index % (int) (sizeof(mover_colors) / sizeof(mover_colors[0]))];
  }

  int rate_digit_width = requested_width / 9;
  if (rate_digit_width < 18) rate_digit_width = 18;
  int rate_digit_height = rate_digit_width * 2;
  int rate_thickness = rate_digit_width / 5;
  if (rate_thickness < 3) rate_thickness = 3;
  int rate_margin = rate_digit_width / 2;
  int rate_box_width = rate_digit_width * 4 + rate_thickness + rate_digit_width / 4 * 4 + rate_margin;
  int rate_box_height = rate_digit_height + rate_margin;

  while (running) {
    while (XPending(display)) {
      XEvent event;
      XNextEvent(display, &event);
      if (event.type == ConfigureNotify) {
        width = event.xconfigure.width;
        height = event.xconfigure.height;
        redraw = 1;
        previous_x = -1;
      } else if (event.type == Expose) {
        redraw = 1;
      }
    }

    int lane_height = height / 7;
    if (lane_height < 32) lane_height = 32;
    int lane_y = (height - lane_height) / 2;
    int block_width = width / 24;
    if (block_width < 24) block_width = 24;
    int block_height = lane_height * 3 / 5;
    int block_y = lane_y + (lane_height - block_height) / 2;
    int travel = width - block_width;
    if (travel < 1) travel = 1;
    int period = fps * 4;
    int phase = (int) (frame % (uint64_t) period);
    int half = period / 2;
    int ramp = phase <= half ? phase : period - phase;
    int x = (int) ((int64_t) travel * ramp / half);

    if (redraw) {
      draw_background(display, window, gc, width, height);
      XSetForeground(display, gc, 0x090b12);
      XFillRectangle(display, window, gc, 0, lane_y, (unsigned int) width, (unsigned int) lane_height);
      redraw = 0;
      for (int index = 0; index < mover_count; ++index) {
        movers[index].previous_x = -1;
      }
      drawn_rate = -1.0;
    } else if (previous_x >= 0) {
      XSetForeground(display, gc, 0x090b12);
      XFillRectangle(
        display,
        window,
        gc,
        previous_x,
        block_y,
        (unsigned int) block_width,
        (unsigned int) block_height
      );
    }

    int rate_box_damaged = 0;
    for (int index = 0; index < mover_count; ++index) {
      struct mover *mover = &movers[index];
      /* A mover that crossed the readout repainted the background under it. */
      if (show_rate && mover->previous_x >= 0 &&
          mover->previous_x < rate_box_width && mover->previous_y < rate_box_height) {
        rate_box_damaged = 1;
      }
      if (mover->previous_x >= 0) {
        restore_background(
          display, window, gc, width,
          mover->previous_x, mover->previous_y, mover->size, mover->size
        );
        if (mover->previous_y + mover->size > lane_y && mover->previous_y < lane_y + lane_height) {
          int top = mover->previous_y > lane_y ? mover->previous_y : lane_y;
          int bottom = mover->previous_y + mover->size;
          int lane_bottom = lane_y + lane_height;
          if (bottom > lane_bottom) bottom = lane_bottom;
          XSetForeground(display, gc, 0x090b12);
          XFillRectangle(
            display, window, gc,
            mover->previous_x, top,
            (unsigned int) mover->size, (unsigned int) (bottom - top)
          );
        }
      }
      mover->x += mover->dx;
      mover->y += mover->dy;
      if (mover->x < 0) { mover->x = 0; mover->dx = -mover->dx; }
      if (mover->x > width - mover->size) { mover->x = width - mover->size; mover->dx = -mover->dx; }
      if (mover->y < 0) { mover->y = 0; mover->dy = -mover->dy; }
      if (mover->y > height - mover->size) { mover->y = height - mover->size; mover->dy = -mover->dy; }
      mover->previous_x = mover->x;
      mover->previous_y = mover->y;
    }

    unsigned long color = 0x55d9ff + ((frame / (uint64_t) fps) % 3U) * 0x220900;
    XSetForeground(display, gc, color);
    XFillRectangle(
      display,
      window,
      gc,
      x,
      block_y,
      (unsigned int) block_width,
      (unsigned int) block_height
    );

    for (int index = 0; index < mover_count; ++index) {
      XSetForeground(display, gc, movers[index].color);
      XFillRectangle(
        display, window, gc,
        movers[index].x, movers[index].y,
        (unsigned int) movers[index].size, (unsigned int) movers[index].size
      );
    }

    /*
     * The readout changes once a second. Repainting its box every frame would
     * rewrite a sixth of the window for nothing, which costs more compositing
     * than the moving objects do and depresses the very rate being reported.
     */
    if (show_rate && (measured_rate != drawn_rate || rate_box_damaged)) {
      XSetForeground(display, gc, 0x000000);
      XFillRectangle(display, window, gc, 0, 0, (unsigned int) rate_box_width, (unsigned int) rate_box_height);
      XSetForeground(display, gc, 0x00ff88);
      draw_rate(
        display, window, gc, measured_rate,
        rate_margin / 2, rate_margin / 2,
        rate_digit_width, rate_digit_height, rate_thickness
      );
      drawn_rate = measured_rate;
    }

    XSync(display, False);
    previous_x = x;
    ++frame;
    ++interval_frames;

    next = add_nanoseconds(next, frame_nanoseconds);
    while (clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &next, NULL) == EINTR && running) {}

    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    double interval = elapsed_seconds(interval_start, now);
    if (interval >= 1.0) {
      measured_rate = (double) interval_frames / interval;
      fprintf(stderr, "korri-validation-fps=%.3f\n", measured_rate);
      interval_start = now;
      interval_frames = 0;
      if (elapsed_seconds(next, now) > 1.0) {
        next = now;
      }
    }
  }

  XFreeGC(display, gc);
  XDestroyWindow(display, window);
  XCloseDisplay(display);
  return 0;
}
