// SPDX-License-Identifier: GPL-3.0-or-later
#include <Limelight.h>
int picture_init(int vaapi, int width, int height, int fullscreen);
// Defined only in builds with the handheld path (HAVE_V4L2M2M, aarch64).
void picture_init_request(int auto_window_resize);
PDECODER_RENDERER_CALLBACKS picture_callbacks(void);
int picture_poll(int timeout);
// One turn of the descriptor loop alone (`loop.h`), whatever the platform:
// 0 to go on, 1 when a handler asks to quit, -1 on failure.
int picture_poll_descriptors(int timeout);
// X11 VA-API (video/ffmpeg_vaapi.c): a picture that fills its window goes
// through the EGL presenter too (1), or through vaPutSurface (0, the
// default) (#391 step F5). Before each stream's first frame.
void picture_vaapi_egl_for_filled(int on);
void picture_destroy(void);
// The opt-in frame record (frame_times.c). Each descriptor may be -1; both
// belong to C from here on. Open before the connection; close after it stops.
int picture_frame_times_open(int times_fd, int recording_fd);
int picture_frame_times_close(void);
// The size of the last picture that a presenter showed (place.c): 1 and the
// size, or 0 before the first. The stream's thread reads it after each poll
// and reports a new size as the encoded size (#476).
int picture_shown_size(int* width, int* height);
