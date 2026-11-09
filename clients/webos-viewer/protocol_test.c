/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "viewer.h"
#include <ss4s.h>
#include <stdio.h>
#include <string.h>

#define CHECK(condition)                                                                           \
    do {                                                                                           \
        if (!(condition)) {                                                                        \
            fprintf(stderr, "line %d: %s\n", __LINE__, #condition);                                \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)
static int last_buttons, last_mask, last_x, last_y, sends;
static int record(short slot, short mask, int buttons, unsigned char lt, unsigned char rt, short lx,
                  short ly, short rx, short ry) {
    (void)slot;
    (void)lt;
    (void)rt;
    (void)rx;
    (void)ry;
    last_buttons = buttons;
    last_mask = mask;
    last_x = lx;
    last_y = ly;
    sends++;
    return 0;
}
static int input(const char *json) { return kv_input_frame(json, strlen(json)); }
/* The exact linked common-c parser (Misc.c), not a test replacement. */
extern int extractVersionQuadFromString(const char *text, int *quad);
static int version(const char *text, kv_serverinfo *server) {
    char xml[1024];
    int n = snprintf(xml, sizeof(xml),
                     "<root "
                     "status_code=\"200\"><uniqueid>redacted</uniqueid><appversion>%s</"
                     "appversion><state>SUNSHINE_SERVER_FREE</state><PairStatus>0</"
                     "PairStatus><currentgame>0</currentgame></root>",
                     text);
    if (n < 0 || (size_t)n >= sizeof(xml))
        return -1;
    return kv_serverinfo_parse(xml, (size_t)n, server);
}
int main(void) {
    const char *info = "<root "
                       "status_code=\"200\"><uniqueid>uuid</uniqueid><appversion>7.1.431.0</"
                       "appversion><state>SUNSHINE_SERVER_FREE</state><PairStatus>1</"
                       "PairStatus><currentgame>0</currentgame><HttpsPort>47984</"
                       "HttpsPort><ServerCodecModeSupport>1</ServerCodecModeSupport></root>";
    kv_serverinfo server;
    CHECK(kv_serverinfo_parse(info, strlen(info), &server) == 0);
    CHECK(server.paired && !server.current_game && server.https_port == 47984 &&
          !strcmp(server.uuid, "uuid"));
    /* Owner's real aka Sunshine serverinfo, with only uniqueid/mac redacted. */
    const char *aka = "<root status_code=\"200\"><uniqueid>REDACTED</uniqueid><mac>REDACTED</mac>"
                      "<appversion>7.1.431.-1</appversion><GfeVersion>3.23.0.74</GfeVersion>"
                      "<state>SUNSHINE_SERVER_FREE</state><PairStatus>0</PairStatus>"
                      "<currentgame>0</currentgame><HttpsPort>47984</HttpsPort>"
                      "<ServerCodecModeSupport>197377</ServerCodecModeSupport></root>";
    CHECK(kv_serverinfo_parse(aka, strlen(aka), &server) == 0);
    CHECK(!strcmp(server.app_version, "7.1.431.-1"));
    CHECK(!strcmp(server.gfe_version, "3.23.0.74"));
    CHECK(!strcmp(server.state, "SUNSHINE_SERVER_FREE"));
    CHECK(!server.paired && !server.current_game && server.https_port == 47984);
    CHECK(server.codec_support == 197377);
    CHECK((server.codec_support & (SCM_H264 | SCM_HEVC | SCM_AV1_MAIN8)) ==
          (SCM_H264 | SCM_HEVC | SCM_AV1_MAIN8));
    int quad[4];
    CHECK(extractVersionQuadFromString(server.app_version, quad) == 0);
    CHECK(quad[0] == 7 && quad[1] == 1 && quad[2] == 431 && quad[3] == -1);
    const char *valid_versions[] = {"7.1.431.0", "7.1.431.-1", "7.1.431.+1", "7.1.431.-2147483648",
                                    "7.1.431.2147483647"};
    for (size_t i = 0; i < sizeof(valid_versions) / sizeof(valid_versions[0]); i++)
        CHECK(!version(valid_versions[i], &server));
    const char *bad_versions[] = {"6.1.431.-1",         "7.-1.431.-1",
                                  "7.1.-431.-1",        "7.1.431.",
                                  "7.1.431.-",          "7.1.431.--1",
                                  "7.1.431. -1",        "7.1.431.-1junk",
                                  "7.1.431.-1.0",       "7.1.431.-2147483649",
                                  "7.1.431.2147483648", "7.1.431.999999999999999999999999999999",
                                  "7.1.431-1",          "7.1..-1"};
    for (size_t i = 0; i < sizeof(bad_versions) / sizeof(bad_versions[0]); i++)
        CHECK(version(bad_versions[i], &server));
    /* Host HEVC/AV1 capabilities must not broaden the TV's H.264-only guard.
     * Rejection precedes entropy, identity access or any network request. */
    STREAM_CONFIGURATION guarded;
    LiInitializeStreamConfiguration(&guarded);
    guarded.width = 1920;
    guarded.height = 1080;
    guarded.fps = 60;
    guarded.audioConfiguration = AUDIO_CONFIGURATION_STEREO;
    guarded.supportedVideoFormats = VIDEO_FORMAT_H264 | VIDEO_FORMAT_H265 | VIDEO_FORMAT_AV1_MAIN8;
    char unused_url[1024];
    CHECK(kv_launch(NULL, "127.0.0.1", 1, "unused", 1, 0, &guarded, unused_url) != 0);
    const char *session = "<root "
                          "status_code=\"200\"><resume>1</resume><sessionUrl0>rtspenc://"
                          "127.0.0.1:48010</sessionUrl0></root>";
    char url[1024];
    CHECK(kv_session_parse(session, strlen(session), 1, url) == 0);
    CHECK(!strcmp(url, "rtspenc://127.0.0.1:48010"));
    CHECK(kv_session_parse(session, strlen(session), 0, url) != 0);
    const char *bad[] = {
        "<root status_code=\"401\"><resume>1</resume></root>",
        "<root status_code=\"200\"><resume>0</resume></root>",
        "<root "
        "status_code=\"200\"><resume>1</resume><sessionUrl0>https://elsewhere</sessionUrl0></root>",
        "<root status_code=\"200\"><resume>1</resume><resume>1</resume></root>",
        "<root status_code=\"200\"><resume><nested>1</nested></resume></root>",
        "<!DOCTYPE root [<!ENTITY bomb 'x'>]><root "
        "status_code=\"200\"><resume>&bomb;</resume></root>"};
    for (size_t i = 0; i < sizeof(bad) / sizeof(bad[0]); i++)
        CHECK(kv_session_parse(bad[i], strlen(bad[i]), 1, url) != 0);
    int app;
    const char *apps = "<root "
                       "status_code=\"200\"><App><AppTitle>Other</AppTitle><ID>9</ID></"
                       "App><App><ID>123</ID><AppTitle>Korri Stream</AppTitle></App></root>";
    CHECK(!kv_applist_parse(apps, strlen(apps), &app) && app == 123);
    const char *bad_apps[] = {
        "<root status_code=\"200\"/>",
        "<root status_code=\"200\"><App><ID>0</ID><AppTitle>Korri Stream</AppTitle></App></root>",
        "<root status_code=\"200\"><App><ID>1</ID><ID>2</ID><AppTitle>Korri "
        "Stream</AppTitle></App></root>",
        "<root status_code=\"200\"><App><ID>1</ID><AppTitle>Korri "
        "Stream</AppTitle></App><App><ID>2</ID><AppTitle>Korri Stream</AppTitle></App></root>",
        "<!DOCTYPE root [<!ENTITY x 'Korri Stream'>]><root "
        "status_code=\"200\"><App><ID>1</ID><AppTitle>&x;</AppTitle></App></root>"};
    for (size_t i = 0; i < sizeof(bad_apps) / sizeof(bad_apps[0]); i++)
        CHECK(kv_applist_parse(bad_apps[i], strlen(bad_apps[i]), &app));
    kv_input_sender(record);
    CHECK(!input("{\"kind\":\"device-added\",\"device\":{\"deviceId\":\"pad\",\"class\":"
                 "\"gamepad\",\"axes\":[{\"code\":0.0,\"minimum\":-32768.0,\"maximum\":32767.0},{"
                 "\"code\":1.0,\"minimum\":-32768.0,\"maximum\":32767.0}]}}"));
    CHECK(!input(
        "{\"kind\":\"input\",\"deviceId\":\"pad\",\"type\":1.0,\"code\":304.0,\"value\":1.0}"));
    CHECK(!input("{\"kind\":\"device-state-complete\",\"deviceId\":\"pad\"}"));
    CHECK(last_buttons == 0); /* held baseline must not be a new press */
    CHECK(!input(
        "{\"kind\":\"input\",\"deviceId\":\"pad\",\"type\":1.0,\"code\":304.0,\"value\":0.0}"));
    CHECK(!input(
        "{\"kind\":\"input\",\"deviceId\":\"pad\",\"type\":0.0,\"code\":0.0,\"value\":0.0}"));
    CHECK(!input(
        "{\"kind\":\"input\",\"deviceId\":\"pad\",\"type\":1.0,\"code\":304.0,\"value\":1.0}"));
    CHECK(!input(
        "{\"kind\":\"input\",\"deviceId\":\"pad\",\"type\":3.0,\"code\":0.0,\"value\":32767.0}"));
    CHECK(!input(
        "{\"kind\":\"input\",\"deviceId\":\"pad\",\"type\":3.0,\"code\":1.0,\"value\":-32768.0}"));
    CHECK(!input(
        "{\"kind\":\"input\",\"deviceId\":\"pad\",\"type\":0.0,\"code\":0.0,\"value\":0.0}"));
    CHECK(last_buttons == A_FLAG && last_mask == 1 && last_x == 32767 && last_y == 32767);
    CHECK(!input("{\"kind\":\"device-removed\",\"deviceId\":\"pad\"}"));
    CHECK(last_buttons == 0 && last_mask == 0 && sends > 0);
    CHECK(input("{broken") != 0);
    kv_input_reset();
    kv_input_sender(NULL);
    /* Real ss4s module, no SDL, display, sound server or network. Exercise its
     * open/feed/close contract twice, including the vendored close correction. */
    SS4S_Config config = {.videoDriver = "dummy", .audioDriver = "dummy"};
    CHECK(SS4S_Init(0, NULL, &config) == 0);
    CHECK(SS4S_PostInit(0, NULL) == 0);
    CHECK(SS4S_GetVideoModuleName() && SS4S_GetAudioModuleName());
    for (int i = 0; i < 2; i++) {
        SS4S_Player *player = SS4S_PlayerOpen();
        CHECK(player);
        SS4S_VideoInfo video = {.codec = SS4S_VIDEO_H264,
                                .width = 1920,
                                .height = 1080,
                                .frameRateNumerator = 60,
                                .frameRateDenominator = 1};
        CHECK(SS4S_PlayerVideoOpen(player, &video) == SS4S_VIDEO_OPEN_OK);
        unsigned char frame[] = {0, 0, 0, 1, 9, 16};
        CHECK(SS4S_PlayerVideoFeed(player, frame, sizeof(frame),
                                   SS4S_VIDEO_FEED_DATA_FRAME_START |
                                       SS4S_VIDEO_FEED_DATA_FRAME_END) == SS4S_VIDEO_FEED_OK);
        CHECK(SS4S_PlayerVideoClose(player));
        SS4S_PlayerClose(player);
    }
    SS4S_Quit();
    puts("Protocol and real ss4s dummy lifecycle passed");
    return 0;
}
