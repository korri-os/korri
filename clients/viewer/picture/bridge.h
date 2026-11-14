// SPDX-License-Identifier: GPL-3.0-or-later
#include <Limelight.h>
int picture_init(int vaapi, int width, int height, int fullscreen);
// Defined only in builds with the handheld path (HAVE_V4L2M2M, aarch64).
void picture_init_request(int auto_window_resize);
PDECODER_RENDERER_CALLBACKS picture_callbacks(void);
int picture_poll(int timeout);
void picture_destroy(void);
void picture_log(const char* format, ...);
