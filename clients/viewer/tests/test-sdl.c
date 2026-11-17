// SPDX-License-Identifier: GPL-3.0-or-later
// Decode a real H.264 stream through the carried SDL callbacks, and present it
// through the actual event loop. No input/audio backend participates. The
// frame record is on, as the outside measure asks for it. In the middle of
// the stream its size changes from landscape to portrait (#476): the picture
// then fits the same window between black bars at the sides.
#include "sdl.c"
#include "bridge.h"
#include "video/video.h"
#include "video/ffmpeg.h"
#include "place.h"
#include <assert.h>
#include <inttypes.h>
#include <stdlib.h>
#include <unistd.h>

static int frame_number;

static void submit(unsigned char* data, int length) {
  LENTRY entry = {.data = (char*)data, .length = length};
  DECODE_UNIT unit = {.bufferList = &entry, .fullLength = length,
                      .frameNumber = ++frame_number, .enqueueTimeUs = LiGetMicroseconds()};
  assert(decoder_callbacks_sdl.submitDecodeUnit(&unit) == DR_OK);
  for (int attempt = 0; attempt < 100 && sdlCurrentFrame < sdlNextFrame; ++attempt)
    assert(sdl_poll(10) == 0);
  assert(sdlCurrentFrame == sdlNextFrame);
}
// One H.264 file as the network core gives it: one decode unit per frame.
static size_t play(const char* path, unsigned char* data, size_t room) {
  FILE* input = fopen(path, "rb");
  assert(input);
  size_t length = fread(data, 1, room, input);
  assert(length > 0 && length < room);
  fclose(input);
  AVCodecParserContext* parser = av_parser_init(AV_CODEC_ID_H264);
  AVCodecContext* context = avcodec_alloc_context3(NULL);
  assert(parser && context);
  unsigned char* output;
  int output_size;
  for (size_t offset = 0; offset < length;) {
    int used = av_parser_parse2(parser, context, &output, &output_size,
        data + offset, length - offset, AV_NOPTS_VALUE, AV_NOPTS_VALUE, 0);
    assert(used >= 0);
    assert(used || output_size);
    offset += used;
    if (output_size) submit(output, output_size);
  }
  assert(av_parser_parse2(parser, context, &output, &output_size, NULL, 0,
      AV_NOPTS_VALUE, AV_NOPTS_VALUE, 0) >= 0);
  if (output_size) submit(output, output_size);
  av_parser_close(parser);
  avcodec_free_context(&context);
  return length;
}

static Uint32 pixels[128 * 72];

// The window as the renderer last presented it.
static void read_window(void) {
  assert(SDL_RenderReadPixels(renderer, NULL, SDL_PIXELFORMAT_ARGB8888,
      pixels, 128 * sizeof(Uint32)) == 0);
}

// Whether the columns from `first` to `last` hold more than one colour.
static bool varied(int first, int last) {
  for (int y = 0; y < 72; ++y)
    for (int x = first; x <= last; ++x)
      if (pixels[y * 128 + x] != pixels[first]) return true;
  return false;
}

// Whether every pixel of the columns from `first` to `last` is black.
static bool black(int first, int last) {
  for (int y = 0; y < 72; ++y)
    for (int x = first; x <= last; ++x)
      if ((pixels[y * 128 + x] & 0xffffff) != 0) return false;
  return true;
}

int main(int argc, char** argv) {
  assert(argc == 3);
  static unsigned char data[65536 + AV_INPUT_BUFFER_PADDING_SIZE];
  static unsigned char portrait[65536 + AV_INPUT_BUFFER_PADDING_SIZE];
  const char* directory = getenv("TMPDIR") ? getenv("TMPDIR") : "/tmp";
  char times[4096], recording[4096];
  snprintf(times, sizeof times, "%s/sdl-frame-times-XXXXXX", directory);
  snprintf(recording, sizeof recording, "%s/sdl-recording-XXXXXX", directory);
  assert(picture_frame_times_open(mkstemp(times), mkstemp(recording)) == 0);
  sdl_init(128, 72, false);
  assert(decoder_callbacks_sdl.setup(VIDEO_FORMAT_H264, 128, 72, 30, NULL, 0) == 0);
  assert(ffmpeg_decoder == SOFTWARE);
  int width = 0, height = 0;
  assert(picture_shown_size(&width, &height) == 0);

  // The stream's own shape fills the window.
  size_t length = play(argv[1], data, 65536);
  assert(sdlCurrentFrame == 4);
  read_window();
  assert(varied(0, 127));
  assert(!black(0, 3) && !black(124, 127));
  assert(picture_shown_size(&width, &height) == 1 && width == 128 && height == 72);

  // The sender changes the stream to 72x128. The texture takes the new size,
  // and the picture fits the window: 41x72 at x = 43, black at both sides.
  size_t portrait_length = play(argv[2], portrait, 65536);
  assert(sdlCurrentFrame == 8);
  read_window();
  struct picture_place place = picture_fit(72, 128, 128, 72);
  assert(place.x == 43 && place.width == 41 && place.height == 72);
  assert(black(0, place.x - 1));
  assert(black(place.x + place.width, 127));
  assert(varied(place.x + 1, place.x + place.width - 2));
  assert(picture_shown_size(&width, &height) == 1 && width == 72 && height == 128);

  decoder_callbacks_sdl.cleanup();
  sdl_destroy();
  assert(picture_frame_times_close() == 0);
  // Every unit was decoded and handed to the display, in order.
  FILE* rows = fopen(times, "r");
  assert(rows);
  char line[256];
  assert(fgets(line, sizeof line, rows));
  assert(strcmp(line, "index,frame_number,enqueue_ns,decoded_ns,display_ns\n") == 0);
  for (int64_t index = 0; index < 8; ++index) {
    int64_t row;
    int number;
    uint64_t enqueue, decoded, display;
    assert(fgets(line, sizeof line, rows));
    assert(sscanf(line, "%" SCNd64 ",%d,%" SCNu64 ",%" SCNu64 ",%" SCNu64,
                  &row, &number, &enqueue, &decoded, &display) == 5);
    assert(row == index && number == index + 1);
    assert(enqueue <= decoded && decoded <= display);
  }
  assert(!fgets(line, sizeof line, rows));
  fclose(rows);
  // The recording is the stream as it arrived, at both sizes.
  FILE* recorded = fopen(recording, "rb");
  assert(recorded);
  static unsigned char copy[2 * sizeof data];
  assert(fread(copy, 1, sizeof copy, recorded) == length + portrait_length);
  assert(memcmp(copy, data, length) == 0);
  assert(memcmp(copy + length, portrait, portrait_length) == 0);
  fclose(recorded);
  unlink(times);
  unlink(recording);
  puts("SDL software fallback decoded, presented and recorded eight H.264 frames; "
       "a new stream size fits the window between black bars");
}
