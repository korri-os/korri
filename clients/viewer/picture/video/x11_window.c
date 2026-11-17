#include "../../io/bridge.h"
/*
 * This file is part of Moonlight Embedded.
 *
 * Copyright (C) 2017 Iwan Timmer
 *
 * Moonlight is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 3 of the License, or
 * (at your option) any later version.
 *
 * Moonlight is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with Moonlight; if not, see <http://www.gnu.org/licenses/>.
 */

// Korri (#391 step F, slice F4), 2026-11-17: these lines were x11_setup's in
// video/x11.c. They are moved here unchanged, but for `window` and the
// window's size, which are now the function's own.

#include "video.h"
#include "x11_window.h"

Window x11_window_open(Display* display, int display_width, int display_height, int drFlags) {
  Window root = DefaultRootWindow(display);
  XSetWindowAttributes winattr = { .event_mask = PointerMotionMask | ButtonPressMask | ButtonReleaseMask | KeyPressMask | KeyReleaseMask };
  Window window = XCreateWindow(display, root, 0, 0, display_width, display_height, 0, CopyFromParent, InputOutput, CopyFromParent, CWEventMask, &winattr);
  XMapWindow(display, window);
  XStoreName(display, window, "Moonlight");

  if (drFlags & DISPLAY_FULLSCREEN) {
    Atom wm_state = XInternAtom(display, "_NET_WM_STATE", False);
    Atom fullscreen = XInternAtom(display, "_NET_WM_STATE_FULLSCREEN", False);

    XEvent xev = {0};
    xev.type = ClientMessage;
    xev.xclient.window = window;
    xev.xclient.message_type = wm_state;
    xev.xclient.format = 32;
    xev.xclient.data.l[0] = 1;
    xev.xclient.data.l[1] = fullscreen;
    xev.xclient.data.l[2] = 0;

    XSendEvent(display, DefaultRootWindow(display), False, SubstructureRedirectMask | SubstructureNotifyMask, &xev);
  }
  XFlush(display);
  viewer_input_x11(display, window);
  return window;
}
