/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "internal.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>

static volatile sig_atomic_t stopping;
static void stop(int signal) {
    (void)signal;
    stopping = 1;
}
static int usage(void) {
    fputs("usage: webos-viewer-test identity ABSOLUTE_DIR\n"
          "       webos-viewer-test info DIR NUMERIC_HOST HTTPS_PORT SERVER_CERT_PEM\n"
          "       webos-viewer-test {launch|resume} DIR NUMERIC_HOST HTTPS_PORT SERVER_CERT_PEM "
          "APP_ID {dummy|ndl} [SECONDS]\n"
          "       webos-viewer-test {launch-check|resume-check} DIR NUMERIC_HOST HTTPS_PORT "
          "SERVER_CERT_PEM APP_ID\n"
          "No PIN/pairing or real-host discovery. launch-check starts a transport app but not "
          "RTSP.\n",
          stderr);
    return 2;
}
static int positive(const char *text, unsigned max, unsigned *out) {
    if (!*text || strlen(text) > 10 || (strspn)(text, "0123456789") != strlen(text))
        return -1;
    unsigned long n = strtoul(text, NULL, 10);
    if (!n || n > max)
        return -1;
    *out = (unsigned)n;
    return 0;
}
int main(int argc, char **argv) {
    struct rlimit core = {.rlim_cur = 0, .rlim_max = 0};
    if (setrlimit(RLIMIT_CORE, &core))
        return 1;
    if (argc < 3)
        return usage();
    int identity = !strcmp(argv[1], "identity"), info = !strcmp(argv[1], "info");
    int resolve = !strcmp(argv[1], "resolve");
    int resume = !strcmp(argv[1], "resume") || !strcmp(argv[1], "resume-check");
    int check = !strcmp(argv[1], "launch-check") || !strcmp(argv[1], "resume-check");
    int launch = !strcmp(argv[1], "launch") || resume || check;
    if ((identity && argc != 3) || ((info || resolve) && argc != 6) ||
        (!identity && !info && !resolve && !launch) ||
        (launch && (check ? argc != 7 : (argc != 8 && argc != 9))))
        return usage();
    unsigned port = 0, app = 0, seconds = 0;
    if (!identity && positive(argv[4], 65535, &port))
        return usage();
    if (launch && positive(argv[6], 2147483647, &app))
        return usage();
    if (launch && !check && strcmp(argv[7], "dummy") && strcmp(argv[7], "ndl"))
        return usage();
    if (argc == 9 && positive(argv[8], 3600, &seconds))
        return usage();
#ifndef KV_HOST_TEST
    if (launch && !check && !strcmp(argv[7], "dummy"))
        return usage();
#endif
    kv_identity *id;
    if (kv_identity_open(argv[2], &id))
        goto failed;
    int result = -1;
    if (identity) {
        result = fputs(kv_identity_certificate(id), stdout) < 0 ? -1 : 0;
    } else {
        char pin[8192];
        kv_serverinfo server;
        if (kv_read_file(argv[5], pin, sizeof(pin)) ||
            kv_serverinfo_authenticated(id, argv[3], port, pin, &server))
            goto done;
        if (info) {
            puts("Authenticated serverinfo verified");
            result = 0;
            goto done;
        }
        if (!server.paired) {
            kv_fail("client certificate has not been provisioned");
            goto done;
        }
        if (resolve) {
            int resolved;
            if (kv_applist(id, argv[3], port, pin, &resolved))
                goto done;
            printf("%d\n", resolved);
            result = 0;
            goto done;
        }
        if ((!resume && server.current_game != 0) || (resume && server.current_game != (int)app)) {
            kv_fail("transport is occupied or resume app ID differs; never cancel another session");
            goto done;
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
        if (kv_launch(id, argv[3], port, pin, (int)app, resume, &config, rtsp))
            goto done;
        if (check) {
            puts("Authenticated transport launch verified; no RTSP attempted");
            result = 0;
            goto done;
        }
        signal(SIGINT, stop);
        signal(SIGTERM, stop);
        result = kv_stream(argv[3], &server, rtsp, &config, !strcmp(argv[7], "dummy"), &stopping,
                           seconds);
    }
done:
    kv_identity_free(id);
    if (!result)
        return 0;
failed:
    fprintf(stderr, "webos-viewer-test: %s\n", kv_error());
    return 1;
}
