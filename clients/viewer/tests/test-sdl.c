// SPDX-License-Identifier: GPL-3.0-or-later
// Decode a real H.264 stream through the carried SDL callbacks, and present it
// through the actual event loop. No input/audio backend participates.
#include "sdl.c"
#include "video/video.h"
#include "video/ffmpeg.h"
#include <assert.h>

static void submit(unsigned char* data, int length) {
  LENTRY entry = {.data = (char*)data, .length = length};
  DECODE_UNIT unit = {.bufferList = &entry, .fullLength = length};
  assert(decoder_callbacks_sdl.submitDecodeUnit(&unit) == DR_OK);
  for (int attempt = 0; attempt < 100 && sdlCurrentFrame < sdlNextFrame; ++attempt)
    assert(sdl_poll(10) == 0);
  assert(sdlCurrentFrame == sdlNextFrame);
}
int main(int argc, char** argv) {
  assert(argc == 2);
  FILE* input = fopen(argv[1], "rb");
  assert(input);
  unsigned char data[65536 + AV_INPUT_BUFFER_PADDING_SIZE] = {0};
  size_t length = fread(data, 1, 65536, input);
  assert(length > 0 && length < 65536);
  fclose(input);
  sdl_init(128, 72, false);
  assert(decoder_callbacks_sdl.setup(VIDEO_FORMAT_H264, 128, 72, 30, NULL, 0) == 0);
  assert(ffmpeg_decoder == SOFTWARE);
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
  assert(sdlCurrentFrame == 4);
  Uint32 pixels[128 * 72];
  assert(SDL_RenderReadPixels(renderer, NULL, SDL_PIXELFORMAT_ARGB8888,
      pixels, 128 * sizeof(Uint32)) == 0);
  bool varied = false;
  for (int i = 1; i < 128 * 72; ++i) varied |= pixels[i] != pixels[0];
  assert(varied);
  av_parser_close(parser);
  avcodec_free_context(&context);
  decoder_callbacks_sdl.cleanup();
  sdl_destroy();
  puts("SDL software fallback decoded and presented four H.264 frames");
}
