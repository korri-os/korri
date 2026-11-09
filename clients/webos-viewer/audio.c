/* SPDX-License-Identifier: GPL-3.0-or-later */
#define _POSIX_C_SOURCE 200809L
#include "audio.h"
#include <inttypes.h>
#include <opus_multistream.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

/* common-c serializes these audio callbacks for its one connection. Only the
 * bounded statistics cross to the owning control thread. Never hold their lock
 * during Opus or a blocking media write. */
static SS4S_Player *player;
static OpusMSDecoder *decoder;
static int frame_samples;
static void (*on_failure)(void);
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static kv_audio_stats stats;
static unsigned reports;
static unsigned long long last_report;
static uint64_t now_us(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint64_t)now.tv_sec * UINT64_C(1000000) + (uint64_t)now.tv_nsec / 1000;
}
static void failed(void) {
    if (on_failure)
        on_failure();
}
static int init(int configuration, const POPUS_MULTISTREAM_CONFIGURATION cfg, void *context,
                int flags) {
    (void)context;
    (void)flags;
    fprintf(stderr,
            "korri-webos viewer pid=%ld stage=audio_init config=%d rate=%d channels=%d streams=%d "
            "coupled=%d mapping=%u,%u samples_per_frame=%d capabilities=0 outcome=started\n",
            (long)getpid(), configuration, cfg->sampleRate, cfg->channelCount, cfg->streams,
            cfg->coupledStreams, (unsigned)cfg->mapping[0], (unsigned)cfg->mapping[1],
            cfg->samplesPerFrame);
    if (configuration != AUDIO_CONFIGURATION_STEREO || cfg->channelCount != 2 ||
        cfg->sampleRate != 48000 || cfg->samplesPerFrame <= 0 || cfg->samplesPerFrame > 5760) {
        fputs("korri-webos viewer stage=audio_init outcome=failed cause=unsupported audio "
              "configuration\n",
              stderr);
        return -1;
    }
    int error;
    decoder = opus_multistream_decoder_create(cfg->sampleRate, cfg->channelCount, cfg->streams,
                                              cfg->coupledStreams, cfg->mapping, &error);
    if (!decoder || error != OPUS_OK) {
        fputs("korri-webos viewer stage=audio_init outcome=failed cause=Opus multistream "
              "initialization failed\n",
              stderr);
        return -1;
    }
    frame_samples = cfg->samplesPerFrame;
    SS4S_AudioInfo info = {.codec = SS4S_AUDIO_PCM_S16LE,
                           .sampleRate = cfg->sampleRate,
                           .numOfChannels = 2,
                           .samplesPerFrame = frame_samples,
                           .appName = "Korri",
                           .streamName = "GameStream"};
    if (SS4S_PlayerAudioOpen(player, &info) != SS4S_AUDIO_OPEN_OK) {
        opus_multistream_decoder_destroy(decoder);
        decoder = NULL;
        fputs("korri-webos viewer stage=audio_init outcome=failed cause=SS4S audio open failed\n",
              stderr);
        return -1;
    }
    return 0;
}
static void cleanup(void) {
    if (!SS4S_PlayerAudioClose(player))
        failed();
    if (decoder)
        opus_multistream_decoder_destroy(decoder);
    decoder = NULL;
}
static void decode(char *data, int length) {
    pthread_mutex_lock(&lock);
    stats.calls++;
    if (!data && !length)
        stats.plc++;
    else
        stats.packets++;
    pthread_mutex_unlock(&lock);
    opus_int16 pcm[5760 * 2];
    int samples =
        opus_multistream_decode(decoder, (unsigned char *)data, length, pcm, frame_samples, 0);
    unsigned peak = 0;
    if (samples > 0)
        for (int i = 0; i < samples * 2; i++) {
            int value = pcm[i];
            unsigned magnitude = (unsigned)(value < 0 ? -value : value);
            if (magnitude > peak)
                peak = magnitude;
        }
    pthread_mutex_lock(&lock);
    stats.last_decode = samples;
    if (samples < 0)
        stats.decode_errors++;
    else
        stats.decoded_samples += (uint64_t)samples;
    if (peak > stats.peak)
        stats.peak = peak;
    if (samples > 0)
        stats.in_feed = 1;
    pthread_mutex_unlock(&lock);
    if (samples < 0) {
        failed();
        return;
    }
    if (!samples)
        return;
    size_t bytes = (size_t)samples * 2 * sizeof(opus_int16);
    uint64_t started = now_us();
    SS4S_AudioFeedResult fed = SS4S_PlayerAudioFeed(player, (unsigned char *)pcm, bytes);
    uint64_t took = now_us() - started;
    pthread_mutex_lock(&lock);
    stats.in_feed = 0;
    stats.last_feed = fed;
    if (took > stats.feed_max_us)
        stats.feed_max_us = took;
    switch (fed) {
    case SS4S_AUDIO_FEED_OK:
        stats.feed_ok++;
        stats.feed_bytes += bytes;
        break;
    case SS4S_AUDIO_FEED_NOT_READY:
        stats.feed_not_ready++;
        break;
    case SS4S_AUDIO_FEED_OVERFLOW:
        stats.feed_overflow++;
        break;
    default:
        stats.feed_errors++;
        break;
    }
    pthread_mutex_unlock(&lock);
    // A sink that silently stopped consuming must not look like successful audio.
    if (fed != SS4S_AUDIO_FEED_OK)
        failed();
}
void kv_audio_callbacks(SS4S_Player *output, AUDIO_RENDERER_CALLBACKS *out, void (*failure)(void)) {
    player = output;
    on_failure = failure;
    pthread_mutex_lock(&lock);
    memset(&stats, 0, sizeof(stats));
    reports = 0;
    last_report = 0;
    pthread_mutex_unlock(&lock);
    LiInitializeAudioCallbacks(out);
    out->init = init;
    out->cleanup = cleanup;
    out->decodeAndPlaySample = decode;
    // Pulse's pa_simple_write blocks. Zero selects common-c's dedicated AudioDec
    // thread; DIRECT_SUBMIT would run this sink on the UDP receive thread.
    out->capabilities = 0;
}
void kv_audio_snapshot(kv_audio_stats *out) {
    pthread_mutex_lock(&lock);
    *out = stats;
    pthread_mutex_unlock(&lock);
}
void kv_audio_common_log(const char *format) {
    pthread_mutex_lock(&lock);
    if (!strcmp(format, "Received first audio packet after %d ms\n"))
        stats.first_packet++;
    else if (!strcmp(format, "Failed to decrypt audio packet (sequence number: %u)\n"))
        stats.decrypt_errors++;
    else if (!strcmp(format, "Audio Receive: recvUdpSocket() failed: %d\n"))
        stats.receive_errors++;
    else if (!strcmp(format, "Audio packet queue overflow\n"))
        stats.queue_overflow++;
    pthread_mutex_unlock(&lock);
}
void kv_audio_report(unsigned long long elapsed) {
    pthread_mutex_lock(&lock);
    if (elapsed <= last_report || reports == 30) {
        pthread_mutex_unlock(&lock);
        return;
    }
    reports++;
    last_report = elapsed;
    kv_audio_stats s = stats;
    stats.peak = 0;
    pthread_mutex_unlock(&lock);
    fprintf(stderr,
            "korri-webos viewer pid=%ld stage=audio second=%llu calls=%" PRIu64 " packets=%" PRIu64
            " plc=%" PRIu64 " samples=%" PRIu64 " peak=%u decode_err=%" PRIu64
            " decode_last=%d feed_ok=%" PRIu64 " feed_wait=%" PRIu64 " feed_overflow=%" PRIu64
            " feed_err=%" PRIu64 " feed_last=%d bytes=%" PRIu64 " feeding=%d feed_max_us=%" PRIu64
            " received=%" PRIu64 " decrypt_err=%" PRIu64 " recv_err=%" PRIu64
            " queue_overflow=%" PRIu64 "\n",
            (long)getpid(), elapsed, s.calls, s.packets, s.plc, s.decoded_samples, s.peak,
            s.decode_errors, s.last_decode, s.feed_ok, s.feed_not_ready, s.feed_overflow,
            s.feed_errors, s.last_feed, s.feed_bytes, s.in_feed, s.feed_max_us, s.first_packet,
            s.decrypt_errors, s.receive_errors, s.queue_overflow);
}
