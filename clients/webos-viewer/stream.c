/* SPDX-License-Identifier: GPL-3.0-or-later */
#define _POSIX_C_SOURCE 200809L
#include "internal.h"
#include <errno.h>
#include <opus_multistream.h>
#include <pthread.h>
#include <ss4s.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

/* common-c callbacks have no context on frame/audio/termination. Its API allows
 * one connection per process. Serialize the entire lifetime, never invent a
 * second registry or stop common-c on one of its own callback threads. */
static pthread_mutex_t singleton = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t status_lock = PTHREAD_MUTEX_INITIALIZER;
static SS4S_Player *player;
static OpusMSDecoder *audio;
static int ended, fatal, connected, first_frame, frame_samples;
static int channel = -1;
void kv_stream_channel(int descriptor) { channel = descriptor; }
static unsigned char *video_buffer;
#define FRAME_LIMIT (8 * 1024 * 1024)
static void fail_stream(void) {
    pthread_mutex_lock(&status_lock);
    fatal = 1;
    pthread_mutex_unlock(&status_lock);
}
static void terminated(int code) {
    pthread_mutex_lock(&status_lock);
    ended = 1;
    if (code)
        fatal = 1;
    pthread_mutex_unlock(&status_lock);
}
static void started(void) {
    pthread_mutex_lock(&status_lock);
    connected = 1;
    pthread_mutex_unlock(&status_lock);
}
static int video_setup(int format, int width, int height, int fps, void *ctx, int flags) {
    (void)ctx;
    (void)flags;
    if (format != VIDEO_FORMAT_H264 || width != 1920 || height != 1080 || fps != 60)
        return -1;
    SS4S_VideoInfo info = {.codec = SS4S_VIDEO_H264,
                           .width = width,
                           .height = height,
                           .frameRateNumerator = fps,
                           .frameRateDenominator = 1};
    return SS4S_PlayerVideoOpen(player, &info) == SS4S_VIDEO_OPEN_OK ? 0 : -1;
}
static void video_cleanup(void) {
    if (!SS4S_PlayerVideoClose(player))
        fail_stream();
}
static int submit(PDECODE_UNIT unit) {
    if (unit->fullLength <= 0 || unit->fullLength > FRAME_LIMIT) {
        fail_stream();
        return DR_NEED_IDR;
    }
    size_t used = 0;
    for (PLENTRY p = unit->bufferList; p; p = p->next) {
        if (p->length <= 0 || (size_t)p->length > (size_t)unit->fullLength - used) {
            fail_stream();
            return DR_NEED_IDR;
        }
        memcpy(video_buffer + used, p->data, (size_t)p->length);
        used += (size_t)p->length;
    }
    if (used != (size_t)unit->fullLength) {
        fail_stream();
        return DR_NEED_IDR;
    }
    SS4S_VideoFeedFlags flags = SS4S_VIDEO_FEED_DATA_FRAME_START | SS4S_VIDEO_FEED_DATA_FRAME_END;
    if (unit->frameType == FRAME_TYPE_IDR)
        flags |= SS4S_VIDEO_FEED_DATA_KEYFRAME;
    SS4S_VideoFeedResult result = SS4S_PlayerVideoFeed(player, video_buffer, used, flags);
    if (result == SS4S_VIDEO_FEED_ERROR)
        fail_stream();
    if (result != SS4S_VIDEO_FEED_OK)
        return DR_NEED_IDR;
    pthread_mutex_lock(&status_lock);
    first_frame = 1;
    pthread_mutex_unlock(&status_lock);
    return DR_OK;
}
static int audio_init(int configuration, const POPUS_MULTISTREAM_CONFIGURATION cfg, void *ctx,
                      int flags) {
    (void)ctx;
    (void)flags;
    if (configuration != AUDIO_CONFIGURATION_STEREO || cfg->channelCount != 2 ||
        cfg->sampleRate != 48000 || cfg->samplesPerFrame <= 0 || cfg->samplesPerFrame > 5760)
        return -1;
    int error;
    audio = opus_multistream_decoder_create(cfg->sampleRate, cfg->channelCount, cfg->streams,
                                            cfg->coupledStreams, cfg->mapping, &error);
    if (!audio || error != OPUS_OK)
        return -1;
    frame_samples = cfg->samplesPerFrame;
    SS4S_AudioInfo info = {.codec = SS4S_AUDIO_PCM_S16LE,
                           .sampleRate = cfg->sampleRate,
                           .numOfChannels = 2,
                           .samplesPerFrame = frame_samples,
                           .appName = "Korri",
                           .streamName = "GameStream"};
    if (SS4S_PlayerAudioOpen(player, &info) != SS4S_AUDIO_OPEN_OK) {
        opus_multistream_decoder_destroy(audio);
        audio = NULL;
        return -1;
    }
    return 0;
}
static void audio_cleanup(void) {
    if (!SS4S_PlayerAudioClose(player))
        fail_stream();
    if (audio)
        opus_multistream_decoder_destroy(audio);
    audio = NULL;
}
static void decode(char *data, int length) {
    opus_int16 pcm[5760 * 2];
    int samples =
        opus_multistream_decode(audio, (unsigned char *)data, length, pcm, frame_samples, 0);
    if (samples < 0 || SS4S_PlayerAudioFeed(player, (unsigned char *)pcm, (size_t)samples * 4) ==
                           SS4S_AUDIO_FEED_ERROR)
        fail_stream();
}
static unsigned long long seconds_now(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (unsigned long long)now.tv_sec;
}
/* Deliberately suppress upstream diagnostics: RTSP/HTTP debug text can contain
 * session secrets. Adapter events/readiness must be reviewed at integration. */
static void quiet(const char *format, ...) { (void)format; }
static void quiet_media(SS4S_LogLevel level, const char *tag, const char *format, ...) {
    (void)level;
    (void)tag;
    (void)format;
}
int kv_stream(const char *host, const kv_serverinfo *server, const char *rtsp,
              STREAM_CONFIGURATION *config, int dummy, volatile sig_atomic_t *stop,
              unsigned seconds) {
    if (!stop || pthread_mutex_trylock(&singleton))
        return kv_fail("only one viewer connection is allowed");
    ended = fatal = connected = first_frame = 0;
    audio = NULL;
    player = NULL;
    video_buffer = malloc(FRAME_LIMIT);
    int result = -1, initialized = 0;
    if (!video_buffer) {
        kv_fail("frame allocation failed");
        goto done;
    }
    if (!dummy) {
        if (geteuid() == 0) {
            kv_fail("NDL viewer refuses root; use the unprivileged app jail");
            goto done;
        }
        const char *appid = getenv("APPID");
        if (!appid || strcmp(appid, "org.korri.shell")) {
            kv_fail("NDL requires org.korri.shell app identity");
            goto done;
        }
        /* Keep ss4s' jail guard; do not silently fall back to another backend. */
        if (!(SS4S_ModuleCheck("ndl-webos4", SS4S_MODULE_CHECK_VIDEO) & SS4S_MODULE_CHECK_VIDEO)) {
            kv_fail("NDL backend jail check failed");
            goto done;
        }
    }
    SS4S_Config media = {.videoDriver = dummy ? "dummy" : "ndl-webos4",
                         .audioDriver = dummy ? "dummy" : "pulse",
                         .loggingFunction = quiet_media};
    if (SS4S_Init(0, NULL, &media)) {
        kv_fail("ss4s init failed");
        goto done;
    }
    initialized = 1;
    if (!SS4S_GetVideoModuleName() || !SS4S_GetAudioModuleName() || SS4S_PostInit(0, NULL)) {
        kv_fail("required ss4s media backend unavailable");
        goto done;
    }
    player = SS4S_PlayerOpen();
    if (!player) {
        kv_fail("ss4s player creation failed");
        goto done;
    }
    SERVER_INFORMATION information;
    LiInitializeServerInformation(&information);
    information.address = host;
    information.serverInfoAppVersion = server->app_version;
    information.serverInfoGfeVersion = server->gfe_version[0] ? server->gfe_version : NULL;
    information.serverCodecModeSupport = server->codec_support;
    information.rtspSessionUrl = rtsp;
    DECODER_RENDERER_CALLBACKS video;
    LiInitializeVideoCallbacks(&video);
    video.setup = video_setup;
    video.cleanup = video_cleanup;
    video.submitDecodeUnit = submit;
    video.capabilities = CAPABILITY_DIRECT_SUBMIT;
    AUDIO_RENDERER_CALLBACKS sound;
    LiInitializeAudioCallbacks(&sound);
    sound.init = audio_init;
    sound.cleanup = audio_cleanup;
    sound.decodeAndPlaySample = decode;
    CONNECTION_LISTENER_CALLBACKS listener;
    LiInitializeConnectionCallbacks(&listener);
    listener.connectionStarted = started;
    listener.connectionTerminated = terminated;
    listener.logMessage = quiet;
    if (LiStartConnection(&information, config, &listener, &video, &sound, NULL, 0, NULL, 0)) {
        kv_fail("common-c connection/RTSP/media initialization failed");
        goto done;
    }
    unsigned long long start = seconds_now();
    int notified = 0;
    char input[16385];
    size_t input_used = 0;
    for (;;) {
        pthread_mutex_lock(&status_lock);
        int finished = ended || fatal, ready = connected && first_frame;
        pthread_mutex_unlock(&status_lock);
        if (ready && !notified && channel >= 0) {
            if (send(channel, "R", 1, MSG_NOSIGNAL) != 1) {
                fail_stream();
                break;
            }
            fprintf(stderr, "korri-webos viewer pid=%ld stage=ready outcome=ok\n", (long)getpid());
            notified = 1;
        }
        if (channel >= 0) {
            ssize_t n = recv(channel, input + input_used, sizeof(input) - input_used, MSG_DONTWAIT);
            if (n == 0) {
                kv_input_reset();
                break;
            }
            if (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK && errno != EINTR) {
                fail_stream();
                break;
            }
            if (n > 0) {
                input_used += (size_t)n;
                size_t consumed = 0;
                for (size_t i = 0; i < input_used; i++)
                    if (input[i] == '\n') {
                        if (kv_input_frame(input + consumed, i - consumed)) {
                            fail_stream();
                            break;
                        }
                        consumed = i + 1;
                    }
                memmove(input, input + consumed, input_used - consumed);
                input_used -= consumed;
                if (input_used == sizeof(input)) {
                    fail_stream();
                    break;
                }
            }
        }
        if (*stop || finished || (seconds && seconds_now() - start >= seconds))
            break;
        if (!ready && seconds_now() - start >= 15) {
            fail_stream();
            break;
        }
        struct timespec delay = {.tv_sec = 0, .tv_nsec = 20000000};
        nanosleep(&delay, NULL);
    }
    kv_input_reset();
    LiStopConnection(); /* Only the owner thread, never inside a callback. */
    pthread_mutex_lock(&status_lock);
    result = fatal || !connected || !first_frame ? -1 : 0;
    pthread_mutex_unlock(&status_lock);
    if (result)
        kv_fail("stream failed or no decode unit accepted (not a visible-frame guarantee)");
done:
    if (player)
        SS4S_PlayerClose(player);
    if (initialized)
        SS4S_Quit();
    free(video_buffer);
    video_buffer = NULL;
    player = NULL;
    pthread_mutex_unlock(&singleton);
    return result;
}
