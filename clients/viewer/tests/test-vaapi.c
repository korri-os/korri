// Exercise the actual VA-API initialization and its failure cleanup, not a
// replacement driver. The build sandbox has an Xvfb but no VA-API driver.
#include <assert.h>
#include <libavcodec/avcodec.h>
#include <X11/Xlib.h>
#include "bridge.h"
#include "video/ffmpeg_vaapi.h"

// No presentation may follow a failed VA-API initialization.
void korri_viewer_first_frame(void) { assert(0 && "failed VA-API presented a frame"); }

// Which pictures vaapi_queue gives to EGL first (#476, #391 step F5).
static void routes(void) {
  struct picture_place filled = picture_fit(1920, 1080, 1920, 1080);
  struct picture_place bars = picture_fit(1080, 1920, 1920, 1080);
  // The default: vaPutSurface shows a picture that fills its window, EGL
  // one with bars.
  assert(!vaapi_presents_through_egl(filled));
  assert(vaapi_presents_through_egl(bars));
  // -filledpicture egl: EGL shows both.
  picture_vaapi_egl_for_filled(1);
  assert(vaapi_presents_through_egl(filled));
  assert(vaapi_presents_through_egl(bars));
  // The session's cleanup leaves the viewer's choice.
  vaapi_cleanup_lib();
  assert(vaapi_presents_through_egl(filled));
  picture_vaapi_egl_for_filled(0);
  assert(!vaapi_presents_through_egl(filled));
  assert(vaapi_presents_through_egl(bars));
}

int main(void) {
  routes();
  assert(vaapi_init_lib(NULL) < 0);
  vaapi_cleanup_lib();
  Display* display = XOpenDisplay(NULL);
  assert(display != NULL);
  for (int attempt = 0; attempt < 3; ++attempt) {
    assert(vaapi_init_lib(display) < 0);
    vaapi_cleanup_lib();
    // The VA context does not own the caller's X connection.
    assert(XSync(display, False) == 1);
  }
  XCloseDisplay(display);
  return 0;
}
