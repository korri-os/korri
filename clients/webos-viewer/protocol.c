/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "internal.h"
#include <arpa/inet.h>
#include <ctype.h>
#include <expat.h>
#include <limits.h>
#include <mbedtls/platform_util.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Field names and launch parameters are GameStream's existing wire, from
 * Moonlight Embedded libgamestream/client.c and Android NvHTTP.java. Nothing
 * here extends korrid's wire or persists host/session data. */
static const char *fields[] = {"uniqueid",    "appversion", "GfeVersion",  "state",
                               "HttpsPort",   "PairStatus", "currentgame", "ServerCodecModeSupport",
                               "gamesession", "resume",     "sessionUrl0"};
enum { UUID, VERSION, GFE, STATE, HTTPS, PAIRED, GAME, CODEC, LAUNCH, RESUME, RTSP, FIELD_COUNT };
typedef struct {
    XML_Parser parser;
    unsigned depth, seen;
    int field, bad, root;
    char values[FIELD_COUNT][1024];
    size_t used;
} document;
static void reject(document *d) {
    d->bad = 1;
    XML_StopParser(d->parser, XML_FALSE);
}
static void XMLCALL start(void *context, const char *name, const char **attrs) {
    document *d = context;
    d->depth++;
    if (d->depth == 1) {
        if (strcmp(name, "root") || d->root) {
            reject(d);
            return;
        }
        d->root = 1;
        int status = 0;
        for (int i = 0; attrs[i]; i += 2) {
            if (!strcmp(attrs[i], "status_code"))
                status = !strcmp(attrs[i + 1], "200");
        }
        if (!status)
            reject(d);
    } else if (d->depth == 2) {
        d->field = -1;
        d->used = 0;
        for (int i = 0; i < FIELD_COUNT; i++)
            if (!strcmp(name, fields[i])) {
                if (d->seen & (1U << i)) {
                    reject(d);
                    return;
                }
                d->seen |= 1U << i;
                d->field = i;
            }
    } else if (d->depth > 16 || d->field >= 0)
        reject(d);
}
static void XMLCALL end(void *context, const char *name) {
    (void)name;
    document *d = context;
    if (d->depth == 2)
        d->field = -1;
    d->depth--;
}
static void XMLCALL text(void *context, const char *data, int length) {
    document *d = context;
    if (d->field < 0)
        return;
    if (length < 0 || d->used + (size_t)length >= sizeof(d->values[0])) {
        reject(d);
        return;
    }
    memcpy(d->values[d->field] + d->used, data, (size_t)length);
    d->used += (size_t)length;
    d->values[d->field][d->used] = 0;
}
static void XMLCALL doctype(void *context, const char *a, const char *b, const char *c,
                            int subset) {
    (void)a;
    (void)b;
    (void)c;
    (void)subset;
    reject(context);
}
static int parse(const char *xml, size_t length, document *d) {
    if (!length || length > KV_LIMIT || memchr(xml, 0, length))
        return kv_fail("XML length or encoding rejected");
    memset(d, 0, sizeof(*d));
    d->field = -1;
    d->parser = XML_ParserCreate(NULL);
    if (!d->parser)
        return kv_fail("XML allocation failed");
    XML_SetUserData(d->parser, d);
    XML_SetElementHandler(d->parser, start, end);
    XML_SetCharacterDataHandler(d->parser, text);
    XML_SetStartDoctypeDeclHandler(d->parser, doctype);
    int result =
        XML_Parse(d->parser, xml, (int)length, XML_TRUE) == XML_STATUS_OK && !d->bad && d->root;
    XML_ParserFree(d->parser);
    return result ? 0 : kv_fail("GameStream XML rejected (status, duplicates, depth or syntax)");
}
static int number(const char *value, unsigned max, unsigned *out) {
    if (!*value || (strspn)(value, "0123456789") != strlen(value))
        return -1;
    unsigned long n = strtoul(value, NULL, 10);
    if (n > max)
        return -1;
    *out = (unsigned)n;
    return 0;
}
static int copy_field(char *out, size_t capacity, const char *value, int required) {
    size_t length = strlen(value);
    if (length >= capacity || (required && !length))
        return -1;
    for (size_t i = 0; i < length; i++)
        if ((unsigned char)value[i] < 32 || (unsigned char)value[i] > 126)
            return -1;
    memcpy(out, value, length + 1);
    return 0;
}
int kv_serverinfo_parse(const char *xml, size_t length, kv_serverinfo *out) {
    document d;
    memset(out, 0, sizeof(*out));
    if (parse(xml, length, &d))
        return -1;
    if (copy_field(out->uuid, sizeof(out->uuid), d.values[UUID], 1) ||
        copy_field(out->app_version, sizeof(out->app_version), d.values[VERSION], 1) ||
        copy_field(out->gfe_version, sizeof(out->gfe_version), d.values[GFE], 0) ||
        copy_field(out->state, sizeof(out->state), d.values[STATE], 1))
        return kv_fail("missing or invalid serverinfo fields");
    /* common-c's version parser accepts four decimal components. */
    const char *part = out->app_version;
    for (int i = 0; i < 4; i++) {
        if (*part < '0' || *part > '9')
            return kv_fail("invalid GameStream appversion");
        char *tail;
        unsigned long component = strtoul(part, &tail, 10);
        if (component > INT_MAX || (i == 0 && component < 7) ||
            (i == 3 ? *tail != 0 : *tail != '.'))
            return kv_fail("unsupported GameStream appversion");
        part = tail + (i == 3 ? 0 : 1);
    }
    unsigned value;
    out->https_port = 47984;
    if (d.seen & (1U << HTTPS)) {
        if (number(d.values[HTTPS], 65535, &out->https_port) || !out->https_port)
            return kv_fail("invalid HTTPS port");
    }
    if (number(d.values[PAIRED], 1, &value))
        return kv_fail("invalid pair state");
    out->paired = (int)value;
    if (number(d.values[GAME], INT_MAX, &value))
        return kv_fail("invalid current game");
    out->current_game = (int)value;
    out->codec_support = SCM_H264;
    if (d.seen & (1U << CODEC)) {
        if (number(d.values[CODEC], INT_MAX, &value))
            return kv_fail("invalid codec support");
        out->codec_support = (int)value;
    }
    return 0;
}
int kv_session_parse(const char *xml, size_t length, int resume, char url[1024]) {
    document d;
    url[0] = 0;
    if (parse(xml, length, &d))
        return -1;
    unsigned session;
    if (number(d.values[resume ? RESUME : LAUNCH], INT_MAX, &session) || !session ||
        copy_field(url, 1024, d.values[RTSP], 1))
        return kv_fail("launch/resume rejected or missing RTSP session URL");
    /* Sunshine's URL selects only a port, never authority to contact a different
     * address. common-c gets the separately validated selected route. */
    const char *authority;
    if (!strncmp(url, "rtsp://", 7))
        authority = url + 7;
    else if (!strncmp(url, "rtspenc://", 10))
        authority = url + 10;
    else
        return kv_fail("invalid RTSP session scheme");
    if (!*authority || strchr(authority, '\r') || strchr(authority, '\n') || strchr(authority, '@'))
        return kv_fail("invalid RTSP session URL");
    return 0;
}
static int request_uuid(char out[37]) {
    unsigned char bytes[16];
    if (kv_random(NULL, bytes, sizeof(bytes)))
        return kv_fail("request entropy failed");
    bytes[6] = (unsigned char)((bytes[6] & 15) | 64);
    bytes[8] = (unsigned char)((bytes[8] & 63) | 128);
    size_t offset = 0;
    for (int i = 0; i < 16; i++) {
        if (i == 4 || i == 6 || i == 8 || i == 10)
            out[offset++] = '-';
        snprintf(out + offset, 3, "%02x", bytes[i]);
        offset += 2;
    }
    return 0;
}
int kv_serverinfo_public(const char *host, unsigned port, kv_serverinfo *out) {
    char *response;
    size_t length;
    if (kv_http(NULL, host, port, NULL, "/serverinfo", &response, &length))
        return -1;
    int result = kv_serverinfo_parse(response, length, out);
    free(response);
    return result;
}
int kv_serverinfo_authenticated(kv_identity *id, const char *host, unsigned port, const char *pin,
                                kv_serverinfo *out) {
    char path[128], uuid[37], *response;
    size_t length;
    if (request_uuid(uuid))
        return -1;
    snprintf(path, sizeof(path), "/serverinfo?uniqueid=%s&uuid=%s", id->uniqueid, uuid);
    if (!pin)
        return kv_fail("authenticated serverinfo requires provisioned pin");
    if (kv_http(id, host, port, pin, path, &response, &length))
        return -1;
    int result = kv_serverinfo_parse(response, length, out);
    free(response);
    return result;
}
int kv_launch(kv_identity *id, const char *host, unsigned port, const char *pin, int app_id,
              int resume, STREAM_CONFIGURATION *config, char rtsp_url[1024]) {
    if (!pin || app_id <= 0 || (resume != 0 && resume != 1) || config->width != 1920 ||
        config->height != 1080 || config->fps != 60 ||
        config->audioConfiguration != AUDIO_CONFIGURATION_STEREO ||
        config->supportedVideoFormats != VIDEO_FORMAT_H264)
        return kv_fail("invalid app ID, pin or baseline stream configuration");
    if (kv_random(NULL, (unsigned char *)config->remoteInputAesKey, 16) ||
        kv_random(NULL, (unsigned char *)config->remoteInputAesIv, 4))
        return kv_fail("session entropy failed");
    memset(config->remoteInputAesIv + 4, 0, 12);
    char hex[33];
    for (int i = 0; i < 16; i++)
        snprintf(hex + i * 2, 3, "%02X", (unsigned char)config->remoteInputAesKey[i]);
    uint32_t key_id;
    memcpy(&key_id, config->remoteInputAesIv, 4);
    key_id = ntohl(key_id);
    char path[2048], uuid[37];
    if (request_uuid(uuid))
        return -1;
    int n = snprintf(
        path, sizeof(path),
        "/%s?uniqueid=%s&uuid=%s&appid=%d&mode=1920x1080x60&additionalStates=1&sops=0&rikey=%"
        "s&rikeyid=%u&localAudioPlayMode=0&surroundAudioInfo=196610&"
        "remoteControllersBitmap=0&gcmap=0&gcpersist=0%s",
        resume ? "resume" : "launch", id->uniqueid, uuid, app_id, hex, key_id,
        LiGetLaunchUrlQueryParameters());
    if (n < 0 || (size_t)n >= sizeof(path))
        return kv_fail("launch request exceeds bound");
    char *response;
    size_t length;
    int result = kv_http(id, host, port, pin, path, &response, &length);
    mbedtls_platform_zeroize(path, sizeof(path));
    mbedtls_platform_zeroize(hex, sizeof(hex));
    if (result)
        return -1;
    result = kv_session_parse(response, length, resume, rtsp_url);
    free(response);
    return result;
}
