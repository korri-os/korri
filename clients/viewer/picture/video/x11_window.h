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

// Korri (#391 step F, slice F4), 2026-11-17: the window of x11_setup in
// video/x11.c, moved to its own file so that a build can put the Rust
// window (src/x11_window/) in its place.

#include <X11/Xlib.h>

// Makes, maps and names the stream's window on `display`, asks the window
// manager for fullscreen when `drFlags` says so, and starts its keyboard
// and mouse input (io/input/x11.c).
Window x11_window_open(Display* display, int width, int height, int drFlags);
