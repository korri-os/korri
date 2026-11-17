// SPDX-License-Identifier: GPL-3.0-or-later
// Where a decoded picture goes in its window, and the size of the last
// picture that a presenter showed (#476). Korri's own code.
#ifndef PICTURE_PLACE_H
#define PICTURE_PLACE_H

// A rectangle in window pixels, from the window's top left corner, and the
// window's own size.
struct picture_place {
  int window_width, window_height;
  int x, y, width, height;
};

// The largest rectangle with the picture's shape that fits the window,
// centred: bars above and below, or at the sides. The picture is scaled up or
// down; it is never cut or stretched. Android's video layer has the same rule
// (src/android/placement.rs, `letterbox`).
struct picture_place picture_fit(int picture_width, int picture_height,
    int window_width, int window_height);

// A presenter handed a picture of this size to the display. Any thread.
void picture_shown(int width, int height);
// A new stream has shown no picture yet.
void picture_shown_reset(void);
#endif
