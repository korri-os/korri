// SPDX-License-Identifier: GPL-3.0-or-later
// The handheld presenter reports its first presented frame (#639). korrid
// reveals a Linux stream at the viewer's FirstFrame (ADR 0098); without it,
// the stream stays hidden until korrid's 30 s deadline. Frames go through the
// v4l2m2m presenter's own draw_frame() with SDL's software renderer. The
// decoder and the V4L2 device take no part.
#include <libavutil/hwcontext.h>
#if !__has_include(<libavutil/hwcontext_v4l2request.h>)
// The PC FFmpeg has no V4L2 Request device type (clients/viewer/ffmpeg). This
// check opens no decoder, so only the presenter's request setup names it.
#define AV_HWDEVICE_TYPE_V4L2REQUEST AV_HWDEVICE_TYPE_NONE
#endif
#include "video/v4l2m2m.c"
#include <assert.h>

static int first_frames;
void korri_viewer_first_frame(void) { ++first_frames; }

static AVFrame* picture(enum AVPixelFormat format, int width, int height) {
  AVFrame* frame = av_frame_alloc();
  assert(frame);
  frame->format = format;
  frame->width = width;
  frame->height = height;
  assert(av_frame_get_buffer(frame, 0) == 0);
  for (int plane = 0; plane < AV_NUM_DATA_POINTERS && frame->buf[plane]; ++plane)
    memset(frame->buf[plane]->data, 0x80, frame->buf[plane]->size);
  return frame;
}

int main(void) {
  assert(SDL_Init(SDL_INIT_VIDEO) == 0);
  window = SDL_CreateWindow("v4l2m2m first frame", 0, 0, 64, 36, SDL_WINDOW_HIDDEN);
  assert(window);
  sdl_renderer = SDL_CreateRenderer(window, -1, SDL_RENDERER_SOFTWARE);
  assert(sdl_renderer);

  // A frame this presenter cannot show reveals nothing.
  AVFrame* unsupported = picture(AV_PIX_FMT_YUV420P, 64, 36);
  draw_frame(unsupported);
  assert(first_frames == 0);

  // The first NV12 frame it presents is reported, once per stream.
  AVFrame* frame = picture(AV_PIX_FMT_NV12, 64, 36);
  draw_frame(frame);
  assert(first_frames == 1);
  draw_frame(frame);
  assert(first_frames == 1);

  av_frame_free(&unsupported);
  av_frame_free(&frame);
  SDL_DestroyTexture(sdl_nv12_texture);
  SDL_DestroyRenderer(sdl_renderer);
  SDL_DestroyWindow(window);
  SDL_Quit();
  puts("v4l2m2m presenter: first presented frame reported once");
  return 0;
}
