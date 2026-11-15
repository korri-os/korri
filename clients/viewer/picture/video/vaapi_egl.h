// SPDX-License-Identifier: GPL-3.0-or-later
// Show a VA-API decoded surface in an X11 window through EGL, for drivers
// whose vaPutSurface cannot present there (issue #477).

#include <stdbool.h>
#include <libavutil/frame.h>
#include <va/va.h>
#include <va/va_drmcommon.h>
#include <X11/Xlib.h>

// Draws the exported surface over the whole window and returns after the GPU
// has read it. The first call binds EGL to this display and window; later
// calls must name the same pair and come from the same thread. `frame` gives
// the visible size and the colour description. False: nothing was shown, and
// the reason is on stderr.
bool vaapi_egl_present(Display* x11, Window window, int window_width, int window_height,
    const VADRMPRIMESurfaceDescriptor* picture, const AVFrame* frame);
void vaapi_egl_cleanup(void);
