/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "viewer.h"
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static volatile sig_atomic_t stopping;
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
        if (!port || kv_serverinfo_public(argv[2], port, &server))
            return 1;
        printf("%s\n%u\n", server.uuid, server.https_port);
        return 0;
    }
    if (argc == 3 && !strcmp(argv[1], "identity")) {
        kv_identity *id;
        if (kv_identity_open(argv[2], &id))
            return 1;
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
    if (kv_identity_open(argv[2], &id))
        return 1;
    int result = 1, app;
    kv_serverinfo server;
    if (kv_serverinfo_authenticated(id, argv[3], port, pin, &server) || !server.paired ||
        kv_applist(id, argv[3], port, pin, &app))
        goto done;
    if (server.current_game && server.current_game != app)
        goto done;
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
    if (kv_launch(id, argv[3], port, pin, app, server.current_game != 0, &config, rtsp))
        goto done;
    kv_stream_channel(3);
    result = kv_stream(argv[3], &server, rtsp, &config, 0, &stopping, 0) ? 1 : 0;
done:
    kv_identity_free(id);
    return result;
}
