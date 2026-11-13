// SPDX-License-Identifier: GPL-3.0-or-later
// Encode real Opus packets, then run the shipped callback into ALSA's file
// plugin over null. Read PCM back: success means decode AND output happened.
#include "io/bridge.h"
#include <opus_multistream.h>
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char** argv) {
  assert(argc == 2);
  for (int channels = 2; channels <= 8; channels += channels == 2 ? 4 : 2) {
    OPUS_MULTISTREAM_CONFIGURATION config = {
      .sampleRate = 48000, .channelCount = channels, .samplesPerFrame = 960,
      .streams = channels == 2 ? 1 : channels == 6 ? 4 : 5,
      .coupledStreams = channels == 2 ? 1 : channels == 6 ? 2 : 3,
      .mapping = {0, 1, 2, 3, 4, 5, 6, 7},
    };
    int error;
    OpusMSEncoder* encoder = opus_multistream_encoder_create(48000, channels,
      config.streams, config.coupledStreams, config.mapping, OPUS_APPLICATION_AUDIO, &error);
    assert(encoder && error == OPUS_OK);
    short samples[960 * 8];
    for (int i = 0; i < 960; ++i)
      for (int ch = 0; ch < channels; ++ch)
        samples[i * channels + ch] = (short)(12000 * sin(2 * 3.141592653589793 * (440 + ch * 100) * i / 48000));
    PAUDIO_RENDERER_CALLBACKS cb = viewer_audio_callbacks(1, "korri_test");
    assert(cb && cb->init((channels << 8) | 0xCA, &config, "korri_test", 0) == 0);
    for (int packet = 0; packet < 4; ++packet) {
      unsigned char encoded[8192];
      int length = opus_multistream_encode(encoder, samples, 960, encoded, sizeof(encoded));
      assert(length > 0);
      cb->decodeAndPlaySample((char*)encoded, length);
    }
    cb->cleanup();
    opus_multistream_encoder_destroy(encoder);
    FILE* pcm = fopen(argv[1], "rb");
    assert(pcm);
    assert(fseek(pcm, 0, SEEK_END) == 0);
    assert(ftell(pcm) == 4 * 960 * channels * (long)sizeof(short));
    rewind(pcm);
    int nonzero = 0;
    short sample;
    while (fread(&sample, sizeof(sample), 1, pcm)) nonzero += sample != 0;
    fclose(pcm);
    assert(nonzero > 960 * channels);
    printf("Opus -> ALSA null: %d channels, 4 packets, PCM verified\n", channels);
  }
}
