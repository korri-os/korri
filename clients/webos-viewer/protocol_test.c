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
