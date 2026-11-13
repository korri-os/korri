// SPDX-License-Identifier: GPL-3.0-or-later
#include <Limelight.h>
int picture_init(int vaapi, int width, int height, int fullscreen);
PDECODER_RENDERER_CALLBACKS picture_callbacks(void);
int picture_poll(int timeout);
void picture_destroy(void);
void picture_log(const char* format, ...);
