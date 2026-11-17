// SPDX-License-Identifier: GPL-3.0-or-later
// Where a picture goes in its window (#476): the same rule as Android's video
// layer (src/android/placement.rs, its `letterbox` tests), and the size of
// the last picture shown.
#include "place.h"
#include "bridge.h"
#include <assert.h>
#include <stdio.h>

static void fits(int picture_width, int picture_height, int window_width, int window_height,
    int x, int y, int width, int height) {
  struct picture_place place = picture_fit(picture_width, picture_height, window_width, window_height);
  if (place.x != x || place.y != y || place.width != width || place.height != height ||
      place.window_width != window_width || place.window_height != window_height) {
    fprintf(stderr, "%dx%d in %dx%d: (%d, %d) %dx%d, expected (%d, %d) %dx%d\n",
        picture_width, picture_height, window_width, window_height,
        place.x, place.y, place.width, place.height, x, y, width, height);
    assert(0);
  }
}

int main(void) {
  // The same shape fills the window, at any scale.
  fits(1280, 720, 1280, 720, 0, 0, 1280, 720);
  fits(1280, 720, 3840, 2160, 0, 0, 3840, 2160);
  fits(3840, 2160, 1280, 720, 0, 0, 1280, 720);
  // A portrait stream in a landscape window: bars at the sides.
  fits(720, 1280, 1920, 1080, 656, 0, 608, 1080);
  fits(720, 1280, 1280, 720, 437, 0, 405, 720);
  // A landscape stream in a portrait window: bars above and below.
  fits(1280, 720, 1080, 1920, 0, 656, 1080, 608);
  // Android's cases: a wide window and a tall one.
  fits(1280, 720, 2400, 1080, 240, 0, 1920, 1080);
  fits(1280, 720, 1080, 2400, 0, 896, 1080, 608);
  // A tall stream in a square window rounds to the nearest pixel.
  fits(1080, 2400, 1000, 1000, 275, 0, 450, 1000);
  // Bars of less than one percent come from korrid's rounding of sizes to
  // 8 pixels: the picture fills the window instead.
  fits(1920, 1080, 1537, 865, 0, 0, 1537, 865);
  fits(1368, 768, 1366, 768, 0, 0, 1366, 768);
  fits(1920, 1088, 1920, 1080, 0, 0, 1920, 1080);
  fits(1288, 720, 1280, 720, 0, 0, 1280, 720);
  // One percent or more is a shape: bars, even in a small window.
  fits(16, 9, 10, 10, 0, 2, 10, 6);
  fits(1280, 720, 1280, 736, 0, 8, 1280, 720);
  // No window yet: nothing to show, and no division by zero.
  fits(1280, 720, 0, 0, 0, 0, 0, 0);
  fits(0, 0, 1280, 720, 280, 0, 720, 720);

  // Only a picture with no bars fills its window.
  assert(picture_fills_window(picture_fit(1280, 720, 1920, 1080)));
  assert(picture_fills_window(picture_fit(1920, 1080, 1537, 865)));
  assert(!picture_fills_window(picture_fit(720, 1280, 1920, 1080)));
  assert(!picture_fills_window(picture_fit(1280, 720, 1080, 1920)));
  assert(picture_fills_window(picture_fit(1368, 768, 1366, 768)));
  assert(!picture_fills_window(picture_fit(1280, 720, 1280, 736)));

  int width = 0, height = 0;
  assert(picture_shown_size(&width, &height) == 0);
  picture_shown(1280, 720);
  assert(picture_shown_size(&width, &height) == 1 && width == 1280 && height == 720);
  picture_shown(720, 1280);
  assert(picture_shown_size(&width, &height) == 1 && width == 720 && height == 1280);
  // A size that no picture has is not one.
  picture_shown(0, 1280);
  assert(picture_shown_size(&width, &height) == 1 && width == 720 && height == 1280);
  picture_shown_reset();
  assert(picture_shown_size(&width, &height) == 0);
  puts("a picture fits its window with bars, and the last size shown is kept");
  return 0;
}
