/* SPDX-License-Identifier: GPL-3.0-or-later */
#define _POSIX_C_SOURCE 200809L
#include "audio.h"
#include "viewer.h"
#include <opus_multistream.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define CHECK(c)                                                                                   \
    do {                                                                                           \
        if (!(c)) {                                                                                \
            fprintf(stderr, "audio test line %d: %s (%s)\n", __LINE__, #c, kv_error());            \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)
static int failures;
static void failed(void) { failures++; }
static void quiet(SS4S_LogLevel level, const char *tag, const char *format, ...) {
    (void)level;
    (void)tag;
    (void)format;
}
int main(int argc, char **argv) {
    int pulse = argc == 2 && !strcmp(argv[1], "pulse");
    SS4S_Config media = {
        .videoDriver = "dummy", .audioDriver = pulse ? "pulse" : "dummy", .loggingFunction = quiet};
    CHECK(!SS4S_Init(0, NULL, &media));
    CHECK(!SS4S_PostInit(0, NULL));
    SS4S_Player *player = SS4S_PlayerOpen();
    CHECK(player);
    const int sizes[] = {120, 240, 480, 960};
    for (size_t test = 0; test < (pulse ? 1U : sizeof(sizes) / sizeof(sizes[0])); test++) {
        int frame = pulse ? 240 : sizes[test];
        AUDIO_RENDERER_CALLBACKS callbacks;
        kv_audio_callbacks(player, &callbacks, failed);
        CHECK(callbacks.capabilities == 0); /* Blocking Pulse must not run on AudioRecv. */
        OPUS_MULTISTREAM_CONFIGURATION cfg = {.sampleRate = 48000,
                                              .channelCount = 2,
                                              .streams = 1,
                                              .coupledStreams = 1,
                                              .samplesPerFrame = frame,
                                              .mapping = {0, 1}};
        CHECK(!callbacks.init(AUDIO_CONFIGURATION_STEREO, &cfg, NULL, 0));
        int error;
        OpusMSEncoder *encoder = opus_multistream_encoder_create(
            48000, 2, 1, 1, cfg.mapping, OPUS_APPLICATION_RESTRICTED_LOWDELAY, &error);
        CHECK(encoder && error == OPUS_OK);
        opus_int16 pcm[960 * 2];
        unsigned char packet[4096];
        for (int i = 0; i < frame; i++) {
            pcm[2 * i] = (i % 32 < 16) ? 12000 : -12000;
            pcm[2 * i + 1] = (opus_int16)(-pcm[2 * i] / 2);
        }
        int repeats = pulse ? 500 : 20;
        for (int i = 0; i < repeats; i++) {
            int bytes = opus_multistream_encode(encoder, pcm, frame, packet, sizeof(packet));
            CHECK(bytes > 0);
            callbacks.decodeAndPlaySample((char *)packet, bytes);
            if (pulse) {
                struct timespec delay = {.tv_sec = 0, .tv_nsec = 5000000};
                nanosleep(&delay, NULL);
            }
        }
        kv_audio_stats stats;
        kv_audio_snapshot(&stats);
        CHECK(!failures && stats.calls == (uint64_t)repeats && stats.packets == stats.calls);
        CHECK(stats.decoded_samples == (uint64_t)(repeats * frame));
        CHECK(stats.peak > 0 && stats.feed_ok == stats.calls && !stats.feed_errors);
        CHECK(stats.feed_bytes == stats.decoded_samples * 2 * sizeof(opus_int16));
        CHECK(!stats.in_feed);
        if (!pulse) {
            callbacks.decodeAndPlaySample(NULL, 0); /* Packet-loss concealment remains valid. */
            kv_audio_snapshot(&stats);
            CHECK(stats.plc == 1 && stats.last_decode == frame);
            kv_audio_common_log("Received first audio packet after %d ms\n");
            kv_audio_common_log("Failed to decrypt audio packet (sequence number: %u)\n");
            kv_audio_snapshot(&stats);
            CHECK(stats.first_packet == 1 && stats.decrypt_errors == 1);
        }
        for (unsigned second = 0; second < 100; second++) {
            kv_audio_report(second);
            kv_audio_report(second);
        }
        callbacks.cleanup();
        opus_multistream_encoder_destroy(encoder);
    }
    if (!pulse) {
        AUDIO_RENDERER_CALLBACKS callbacks;
        kv_audio_callbacks(player, &callbacks, failed);
        OPUS_MULTISTREAM_CONFIGURATION cfg = {.sampleRate = 48000,
                                              .channelCount = 2,
                                              .streams = 1,
                                              .coupledStreams = 1,
                                              .samplesPerFrame = 240,
                                              .mapping = {0, 1}};
        CHECK(!callbacks.init(AUDIO_CONFIGURATION_STEREO, &cfg, NULL, 0));
        char invalid[] = {(char)0xff};
        callbacks.decodeAndPlaySample(invalid, sizeof(invalid));
        kv_audio_stats stats;
        kv_audio_snapshot(&stats);
        CHECK(failures == 1 && stats.decode_errors == 1 && !stats.feed_ok);
        CHECK(SS4S_PlayerAudioClose(player));
        callbacks.decodeAndPlaySample(NULL, 0);
        kv_audio_snapshot(&stats);
        CHECK(failures == 2 && stats.feed_not_ready == 1);
        callbacks.cleanup();
    }
    SS4S_PlayerClose(player);
    SS4S_Quit();
    puts("Audio callbacks: real Opus decode and SS4S feed passed");
    return 0;
}
