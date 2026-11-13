// Exercise the actual VA-API initialization and its failure cleanup, not a
// replacement driver. The build sandbox has an Xvfb but no VA-API driver.
#include <assert.h>
#include <libavcodec/avcodec.h>
#include <X11/Xlib.h>
#include "video/ffmpeg_vaapi.h"

int main(void) {
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
