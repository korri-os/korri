// Exercise the real X11 presentation handler against a real, resizable window.
// The VA-API sink records its destination rectangle; no GPU is required in CI.
#include "video/x11.c"
#include <assert.h>

static struct picture_place presented;
static int presentations;
static int64_t presented_pts;
static AVFrame* arrive_during_presentation;
enum decoders ffmpeg_decoder = VAAPI;

void vaapi_queue(AVFrame* frame, Window target, struct picture_place place) {
  assert(frame);
  assert(target == window);
  presented = place;
  assert(frame->buf[0] != NULL);
  presented_pts = frame->pts;
  presentations++;
  if (arrive_during_presentation) {
    // A blocked presenter must not hold the producer's mailbox lock.
    AVFrame* next = arrive_during_presentation;
    arrive_during_presentation = NULL;
    queue_frame(next);
  }
}

// The picture fits the window: the window's size, and the picture's place in it.
static void present_in(AVFrame* frame, int width, int height, int x, int y, int shown_width, int shown_height) {
  XResizeWindow(display, window, width, height);
  XSync(display, False);
  int before = presentations;
  queue_frame(frame);
  assert(frame_handle(pipefd[0]) == LOOP_OK);
  assert(presentations == before + 1);
  assert(presented.window_width == width && presented.window_height == height);
  assert(presented.x == x && presented.y == y);
  assert(presented.width == shown_width && presented.height == shown_height);
}

// A picture of the window's shape fills it.
static void present_at(AVFrame* frame, int width, int height) {
  present_in(frame, width, height, 0, 0, width, height);
}

int main(void) {
  display = XOpenDisplay(NULL);
  assert(display);
  window = XCreateSimpleWindow(display, DefaultRootWindow(display),
                               0, 0, 1920, 1080, 0, 0, 0);
  XMapWindow(display, window);
  XSync(display, False);
  assert(pipe(pipefd) == 0);
  assert(fcntl(pipefd[0], F_SETFL, O_NONBLOCK) == 0);

  AVFrame frame = {.format = AV_PIX_FMT_YUV420P, .width = 3840, .height = 2160};
  assert(av_frame_get_buffer(&frame, 32) == 0);
  frame.width = 1920;
  frame.height = 1080;
  // The windowed setup starts with stream dimensions. Korri's compositor then
  // expands that same window to the output, without changing decoded frames.
  display_width = frame.width;
  display_height = frame.height;
  present_at(&frame, 3840, 2160);
  assert(frame.width == 1920 && frame.height == 1080);
  present_at(&frame, 1537, 865);

  // A stream larger than the output must scale down, not be clipped.
  frame.width = display_width = 3840;
  frame.height = display_height = 2160;
  present_at(&frame, 1280, 720);
  present_at(&frame, 1920, 1080);

  // The stream's size changes in the middle of the stream (#476): the same
  // window shows a portrait picture between bars at the sides, never
  // stretched, and the landscape picture fills it again afterwards.
  frame.width = 720;
  frame.height = 1280;
  present_in(&frame, 1920, 1080, 656, 0, 608, 1080);
  frame.width = 1280;
  frame.height = 720;
  present_at(&frame, 1920, 1080);
  // A landscape picture in a portrait window: bars above and below.
  present_in(&frame, 1080, 1920, 0, 656, 1080, 608);
  frame.width = display_width = 3840;
  frame.height = display_height = 2160;

  // A burst replaces the single pending frame, rather than filling a pipe
  // or keeping decoder-owned wrappers. The renderer sees only the newest ID.
  int before = presentations;
  for (int id = 0; id < 2000; ++id) {
    frame.pts = id;
    queue_frame(&frame);
  }
  assert(av_buffer_get_ref_count(frame.buf[0]) == 2);
  frame.pts = 2000;
  arrive_during_presentation = &frame;
  assert(frame_handle(pipefd[0]) == LOOP_OK);
  assert(presentations == before + 1);
  assert(presented_pts == 1999);
  assert(pending_frame != NULL);
  assert(av_buffer_get_ref_count(frame.buf[0]) == 2);
  AVBufferRef* retained = av_buffer_ref(frame.buf[0]);
  assert(retained);
  av_frame_unref(&frame);
  assert(frame_handle(pipefd[0]) == LOOP_OK);
  assert(presented_pts == 2000);
  assert(presentations == before + 2);
  assert(pending_frame == NULL);
  assert(frame_handle(pipefd[0]) == LOOP_OK);
  assert(presentations == before + 2);
  assert(av_buffer_get_ref_count(retained) == 1);
  av_buffer_unref(&retained);

  close(pipefd[0]);
  close(pipefd[1]);
  XDestroyWindow(display, window);
  XCloseDisplay(display);
  puts("VA-API presentation follows actual X11 window geometry across resizes");
}
