/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "viewer.h"
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static volatile sig_atomic_t stopping;
/* kv_error contains literals only. Never log URLs, response bodies, keys or pins. */
static void diagnostic(const char *stage, const char *outcome, const char *cause) {
    fprintf(stderr, "korri-webos viewer pid=%ld stage=%s outcome=%s", (long)getpid(), stage,
            outcome);
    if (cause)
        fprintf(stderr, " cause=%s", cause);
    fputc('\n', stderr);
    fflush(stderr);
}
static void stop(int sig) {
    (void)sig;
    stopping = 1;
}
static unsigned port_number(const char *s) {
    if (!*s || strlen(s) > 5 || (strspn)(s, "0123456789") != strlen(s))
        return 0;
    unsigned long n = strtoul(s, NULL, 10);
    return n > 65535 ? 0 : (unsigned)n;
}
int korri_viewer_main(int argc, char **argv) {
    signal(SIGTERM, stop);
    signal(SIGINT, stop);
    signal(SIGHUP, stop);
    if (argc == 4 && !strcmp(argv[1], "discover")) {
        unsigned port = port_number(argv[3]);
        kv_serverinfo server;
        diagnostic("public_serverinfo", "started", NULL);
        if (!port || kv_serverinfo_public(argv[2], port, &server)) {
            diagnostic("public_serverinfo", "failed", kv_error());
            return 1;
        }
        diagnostic("public_serverinfo", "ok", NULL);
        printf("%s\n%u\n", server.uuid, server.https_port);
        return 0;
    }
    if (argc == 3 && !strcmp(argv[1], "identity")) {
        kv_identity *id;
        diagnostic("identity", "started", NULL);
        if (kv_identity_open(argv[2], &id)) {
            diagnostic("identity", "failed", kv_error());
            return 1;
        }
        diagnostic("identity", "ok", NULL);
        int result = fputs(kv_identity_certificate(id), stdout) < 0;
        kv_identity_free(id);
        return result;
    }
    if (argc != 5 || strcmp(argv[1], "stream"))
        return 2;
    unsigned port = port_number(argv[4]);
    if (!port)
        return 2;
    /* The parent owns and closes the bounded pin pipe. No key or pin in argv,
     * environment, logs or a second persistent trust store. */
    char pin[8192];
    size_t used = 0;
    for (;;) {
        ssize_t n = read(STDIN_FILENO, pin + used, sizeof(pin) - used - 1);
        if (n < 0)
            return 1;
        if (!n)
            break;
        used += (size_t)n;
        if (used == sizeof(pin) - 1)
            return 1;
    }
    close(STDIN_FILENO);
    pin[used] = 0;
    if (!used || memchr(pin, 0, used))
        return 1;
    kv_identity *id;
    diagnostic("identity", "started", NULL);
    if (kv_identity_open(argv[2], &id)) {
        diagnostic("identity", "failed", kv_error());
        return 1;
    }
    diagnostic("identity", "ok", NULL);
    const char *stage = "authenticated_serverinfo";
    int result = 1, app;
    kv_serverinfo server;
    diagnostic(stage, "started", NULL);
    if (kv_serverinfo_authenticated(id, argv[3], port, pin, &server))
        goto done;
    if (!server.paired) {
        diagnostic(stage, "failed", "client certificate not provisioned");
        goto close;
    }
    diagnostic(stage, "ok", NULL);
    stage = "applist";
    diagnostic(stage, "started", NULL);
    if (kv_applist(id, argv[3], port, pin, &app))
        goto done;
    diagnostic(stage, "ok", NULL);
    fprintf(stderr, "korri-webos viewer pid=%ld stage=applist_resolved app_id=%d outcome=ok\n",
            (long)getpid(), app);
    if (server.current_game && server.current_game != app) {
        diagnostic("transport", "failed", "different transport is occupied");
        goto close;
    }
    STREAM_CONFIGURATION config;
    LiInitializeStreamConfiguration(&config);
    config.width = 1920;
    config.height = 1080;
    config.fps = 60;
    config.bitrate = 10000;
    config.packetSize = 1024;
    config.streamingRemotely = STREAM_CFG_AUTO;
    config.audioConfiguration = AUDIO_CONFIGURATION_STEREO;
    config.supportedVideoFormats = VIDEO_FORMAT_H264;
    config.encryptionFlags = (int)ENCFLG_ALL;
    config.colorSpace = COLORSPACE_REC_709;
    char rtsp[1024];
    stage = "transport_launch";
    diagnostic(stage, "started", NULL);
    if (kv_launch(id, argv[3], port, pin, app, server.current_game != 0, &config, rtsp))
        goto done;
    diagnostic(stage, "ok", NULL);
    stage = "stream";
    diagnostic(stage, "started", NULL);
    kv_stream_channel(3);
    result = kv_stream(argv[3], &server, rtsp, &config, 0, &stopping, 0) ? 1 : 0;
done:
    diagnostic(stage, result ? "failed" : "ok", result ? kv_error() : NULL);
close:
    kv_identity_free(id);
    return result;
}
