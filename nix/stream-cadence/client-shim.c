/* Harvested from client-content-metrics.c. Keep one record per decode unit.
 * The fake renderer suppresses local rendering only; transport is Moonlight.
 * Convert receiveTimeUs to the harvested receive_ms field, never use server PTS.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "Limelight.h"
#include "receive-clock.h"

static FILE *dump, *times;
static uint64_t count;
static int setup(int format, int width, int height, int rate, void *ctx, int flags) {
    (void)ctx; (void)flags;
    const char *video = getenv("CADENCE_DUMP");
    const char *stamps = getenv("CADENCE_FRAME_TIMES");
    if (!video || !stamps) return -1;
    dump = fopen(video, "wb");
    times = fopen(stamps, "w");
    if (!dump || !times) { perror("cadence recording"); return -1; }
    setvbuf(times, NULL, _IOLBF, 0);
    fprintf(times, "index,frame_number,presentation_ms,receive_ms,submit_ms\n");
    fprintf(stderr, "CADENCE setup format=%d width=%d height=%d fps=%d\n", format, width, height, rate);
    return 0;
}
static uint64_t monotonic_ns(void) {
    struct timespec stamp;
    if (clock_gettime(CLOCK_MONOTONIC, &stamp)) abort();
    return (uint64_t)stamp.tv_sec * UINT64_C(1000000000) + (uint64_t)stamp.tv_nsec;
}
static int submit(PDECODE_UNIT unit) {
    uint64_t before = monotonic_ns();
    uint64_t native_now = LiGetMicroseconds();
    uint64_t after = monotonic_ns(), receive;
    if (!receive_monotonic_ns(before, after, native_now, unit->receiveTimeUs, &receive)) abort();
    for (PLENTRY entry = unit->bufferList; entry; entry = entry->next) {
        if (fwrite(entry->data, 1, entry->length, dump) != (size_t)entry->length) abort();
    }
    if (fprintf(times, "%" PRIu64 ",%d,%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                count++, unit->frameNumber, unit->presentationTimeUs / 1000,
                receive / 1000000, after / 1000000) < 0) abort();
    return DR_OK;
}
static void cleanup(void) {
    if (dump && fclose(dump)) abort();
    if (times && fclose(times)) abort();
    dump = times = NULL;
    fprintf(stderr, "CADENCE final frames=%" PRIu64 "\n", count);
}
int LiStartConnection(PSERVER_INFORMATION server, PSTREAM_CONFIGURATION config, PCONNECTION_LISTENER_CALLBACKS listeners,
    PDECODER_RENDERER_CALLBACKS video, PAUDIO_RENDERER_CALLBACKS audio, void *render, int flags, void *audio_ctx, int audio_flags) {
    typedef int (*start_fn)(PSERVER_INFORMATION, PSTREAM_CONFIGURATION, PCONNECTION_LISTENER_CALLBACKS,
        PDECODER_RENDERER_CALLBACKS, PAUDIO_RENDERER_CALLBACKS, void *, int, void *, int);
    start_fn real_start;
    void *symbol = dlsym(RTLD_NEXT, "LiStartConnection");
    memcpy(&real_start, &symbol, sizeof(real_start));
    if (!real_start || video) { fprintf(stderr, "CADENCE requires -platform fake\n"); return -1; }
    static DECODER_RENDERER_CALLBACKS observed = {.setup=setup, .cleanup=cleanup, .submitDecodeUnit=submit};
    return real_start(server, config, listeners, &observed, audio, render, flags, audio_ctx, audio_flags);
}
