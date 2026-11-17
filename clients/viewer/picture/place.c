// SPDX-License-Identifier: GPL-3.0-or-later
#include "place.h"
#include "bridge.h"

#include <stdatomic.h>
#include <stdint.h>

struct picture_place picture_fit(int picture_width, int picture_height,
    int window_width, int window_height) {
  int64_t width = picture_width > 0 ? picture_width : 1;
  int64_t height = picture_height > 0 ? picture_height : 1;
  int64_t across = window_width > 0 ? window_width : 0;
  int64_t down = window_height > 0 ? window_height : 0;
  int64_t shown_width, shown_height;
  // Compare the two shapes without floating point: width/height against
  // across/down. Round to the nearest pixel, as Android's rule does.
  if (across * height <= down * width) {
    shown_width = across;
    shown_height = (across * height + width / 2) / width;
  } else {
    shown_width = (down * width + height / 2) / height;
    shown_height = down;
  }
  // Bars of less than one percent come from the rounding of stream sizes
  // (korrid rounds them to 8 pixels), not from a shape: the picture fills
  // the window, stretched by less than one percent, as it was before #476.
  // A presenter then needs no bars at all (ffmpeg_vaapi.c).
  if ((across - shown_width) * 100 < across && (down - shown_height) * 100 < down) {
    shown_width = across;
    shown_height = down;
  }
  return (struct picture_place){
    .window_width = (int)across,
    .window_height = (int)down,
    .x = (int)((across - shown_width) / 2),
    .y = (int)((down - shown_height) / 2),
    .width = (int)shown_width,
    .height = (int)shown_height,
  };
}

bool picture_fills_window(struct picture_place place) {
  return place.x == 0 && place.y == 0 &&
         place.width == place.window_width && place.height == place.window_height;
}

// Width in the high half, height in the low half; 0 before the first picture.
static _Atomic uint64_t shown;

void picture_shown(int width, int height) {
  if (width > 0 && height > 0)
    atomic_store(&shown, (uint64_t)(uint32_t)width << 32 | (uint32_t)height);
}

int picture_shown_size(int* width, int* height) {
  uint64_t size = atomic_load(&shown);
  if (size == 0)
    return 0;
  *width = (int)(size >> 32);
  *height = (int)(uint32_t)size;
  return 1;
}

void picture_shown_reset(void) {
  atomic_store(&shown, 0);
}
