/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "internal.h"
#include <limits.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* Bounded reader for korrid's existing JSON native frames, not a new schema.
 * It consumes only event metadata and integer evdev fields. Unknown keys are
 * skipped as complete JSON values. No physical input device is opened here. */
typedef struct {
    size_t begin, end;
    int next, type;
} token;
typedef struct {
    const char *s;
    size_t length, at;
    token t[2048];
    int count;
} json;
static void space(json *j) {
    while (j->at < j->length && strchr(" \r\n\t", j->s[j->at]))
        j->at++;
}
static int value(json *j, unsigned depth) {
    space(j);
    if (depth > 16 || j->count == 2048 || j->at == j->length)
        return -1;
    int index = j->count++;
    token *t = &j->t[index];
    t->begin = j->at;
    t->type = j->s[j->at++];
    if (t->type == '{' || t->type == '[') {
        int close = t->type == '{' ? '}' : ']';
        space(j);
        if (j->at < j->length && j->s[j->at] != close)
            for (;;) {
                if (t->type == '{') {
                    int key = value(j, depth + 1);
                    if (key < 0 || j->t[key].type != '"')
                        return -1;
                    space(j);
                    if (j->at == j->length || j->s[j->at++] != ':')
                        return -1;
                }
                if (value(j, depth + 1) < 0)
                    return -1;
                space(j);
                if (j->at == j->length)
                    return -1;
                if (j->s[j->at] == close)
                    break;
                if (j->s[j->at++] != ',')
                    return -1;
            }
        if (j->at == j->length || j->s[j->at++] != close)
            return -1;
    } else if (t->type == '"') {
        int closed = 0;
        while (j->at < j->length) {
            unsigned char c = (unsigned char)j->s[j->at++];
            if (c == '"') {
                closed = 1;
                break;
            }
            if (c < 32)
                return -1;
            if (c == '\\') {
                if (j->at == j->length)
                    return -1;
                char escape = j->s[j->at++];
                if (escape == 'u') {
                    for (int n = 0; n < 4; n++)
                        if (j->at == j->length || !strchr("0123456789abcdefABCDEF", j->s[j->at++]))
                            return -1;
                } else if (!strchr("\"\\/bfnrt", escape))
                    return -1;
            }
        }
        if (!closed)
            return -1;
    } else {
        while (j->at < j->length && !strchr(",]} \r\n\t", j->s[j->at]))
            j->at++;
    }
    t->end = j->at;
    t->next = j->count;
    return index;
}
static int equal(const json *j, int n, const char *s) {
    return n >= 0 && j->t[n].type == '"' && j->t[n].end - j->t[n].begin == strlen(s) + 2 &&
           !memcmp(j->s + j->t[n].begin + 1, s, strlen(s));
}
static int field(const json *j, int object, const char *key) {
    if (object < 0 || j->t[object].type != '{')
        return -1;
    int found = -1;
    for (int n = object + 1; n < j->t[object].next;) {
        int v = n + 1;
        if (equal(j, n, key)) {
            if (found >= 0)
                return -1;
            found = v;
        }
        n = j->t[v].next;
    }
    return found;
}
static int integer(const json *j, int n, int *out) {
    if (n < 0)
        return -1;
    size_t size = j->t[n].end - j->t[n].begin;
    if (!size || size > 63)
        return -1;
    char text[64];
    memcpy(text, j->s + j->t[n].begin, size);
    text[size] = 0;
    char *end;
    double v = strtod(text, &end);
    if (*end || end == text || !(v >= INT_MIN && v <= INT_MAX) || v != (double)(int)v)
        return -1;
    *out = (int)v;
    return 0;
}

typedef struct {
    char id[1025];
    int occupied, baseline, armed;
    int axes[64], minimum[64], maximum[64];
    unsigned buttons;
    int hat_x, hat_y;
} pad;
static pad pads[16];
static kv_controller_send output = LiSendMultiControllerEvent;
void kv_input_sender(kv_controller_send send) { output = send ? send : LiSendMultiControllerEvent; }
static short stick(pad *p, int axis, int invert) {
    int64_t lo = p->minimum[axis], hi = p->maximum[axis], v = p->axes[axis];
    if (hi <= lo)
        return 0;
    if (v < lo)
        v = lo;
    if (v > hi)
        v = hi;
    int64_t scaled = (v - lo) * 65535 / (hi - lo) - 32768;
    if (invert)
        scaled = -scaled;
    if (scaled > 32767)
        scaled = 32767;
    return (short)scaled;
}
static unsigned char trigger(pad *p, int axis) {
    int64_t lo = p->minimum[axis], hi = p->maximum[axis], v = p->axes[axis];
    if (hi <= lo)
        return 0;
    if (v < lo)
        v = lo;
    if (v > hi)
        v = hi;
    return (unsigned char)((v - lo) * 255 / (hi - lo));
}
static int send_pad(int slot, int neutral) {
    pad *p = &pads[slot];
    unsigned mask = 0;
    for (int i = 0; i < 16; i++)
        if (pads[i].occupied)
            mask |= 1U << i;
    unsigned buttons = p->buttons;
    if (p->hat_x < 0)
        buttons |= LEFT_FLAG;
    if (p->hat_x > 0)
        buttons |= RIGHT_FLAG;
    if (p->hat_y < 0)
        buttons |= UP_FLAG;
    if (p->hat_y > 0)
        buttons |= DOWN_FLAG;
    short lx = stick(p, 0, 0), ly = stick(p, 1, 1), rx = stick(p, 3, 0), ry = stick(p, 4, 1);
    unsigned char lt = trigger(p, 2), rt = trigger(p, 5);
    if (!p->armed && p->baseline && !buttons && !lt && !rt && lx >= -4096 && lx <= 4096 &&
        ly >= -4096 && ly <= 4096 && rx >= -4096 && rx <= 4096 && ry >= -4096 && ry <= 4096)
        p->armed = 1;
    neutral |= !p->armed;
    return output((short)slot, (short)mask, neutral ? 0 : (int)buttons, neutral ? 0 : lt,
                  neutral ? 0 : rt, neutral ? 0 : lx, neutral ? 0 : ly, neutral ? 0 : rx,
                  neutral ? 0 : ry);
}
void kv_input_reset(void) {
    for (int i = 0; i < 16; i++)
        if (pads[i].occupied) {
            (void)send_pad(i, 1);
            pads[i].occupied = 0;
            (void)send_pad(i, 1);
        }
    memset(pads, 0, sizeof(pads));
}
static unsigned button(int code) {
    switch (code) {
    case 304:
        return A_FLAG;
    case 305:
        return B_FLAG;
    case 307:
        return Y_FLAG;
    case 308:
        return X_FLAG;
    case 310:
        return LB_FLAG;
    case 311:
        return RB_FLAG;
    case 314:
        return BACK_FLAG;
    case 315:
        return PLAY_FLAG;
    case 317:
        return LS_CLK_FLAG;
    case 318:
        return RS_CLK_FLAG;
    case 544:
        return UP_FLAG;
    case 545:
        return DOWN_FLAG;
    case 546:
        return LEFT_FLAG;
    case 547:
        return RIGHT_FLAG;
    default:
        return 0; /* BTN_MODE/Home remains local, never sent to Sunshine. */
    }
}
int kv_input_frame(const char *text, size_t length) {
    if (!length || length > 16384 || memchr(text, 0, length))
        return -1;
    json j = {.s = text, .length = length};
    if (value(&j, 0) != 0)
        return -1;
    space(&j);
    if (j.at != length)
        return -1;
    int kind = field(&j, 0, "kind"), object = 0;
    int added = equal(&j, kind, "device-added");
    if (added)
        object = field(&j, 0, "device");
    int id = field(&j, object, "deviceId");
    if (id < 0 || j.t[id].type != '"')
        return -1;
    size_t size = j.t[id].end - j.t[id].begin - 2;
    if (!size || size > 1024)
        return -1;
    char name[1025];
    memcpy(name, text + j.t[id].begin + 1, size);
    name[size] = 0;
    if (strchr(name, '\\'))
        return -1; /* actual edge IDs are numeric evdev paths */
    int slot = -1;
    for (int i = 0; i < 16; i++)
        if (pads[i].occupied && !strcmp(pads[i].id, name))
            slot = i;
    if (added) {
        if (!equal(&j, field(&j, object, "class"), "gamepad") || slot >= 0)
            return -1;
        for (int i = 0; i < 16; i++)
            if (!pads[i].occupied) {
                slot = i;
                break;
            }
        if (slot < 0)
            return -1;
        pad *p = &pads[slot];
        memset(p, 0, sizeof(*p));
        strcpy(p->id, name);
        p->occupied = 1;
        int axes = field(&j, object, "axes");
        if (axes >= 0 && j.t[axes].type == '[')
            for (int n = axes + 1; n < j.t[axes].next; n = j.t[n].next) {
                int code, lo, hi;
                if (integer(&j, field(&j, n, "code"), &code) || code < 0 || code > 63 ||
                    integer(&j, field(&j, n, "minimum"), &lo) ||
                    integer(&j, field(&j, n, "maximum"), &hi) || hi < lo)
                    return -1;
                p->minimum[code] = lo;
                p->maximum[code] = hi;
            }
        return 0;
    }
    if (slot < 0)
        return -1;
    pad *p = &pads[slot];
    if (equal(&j, kind, "device-removed")) {
        (void)send_pad(slot, 1);
        memset(p, 0, sizeof(*p));
        return send_pad(slot, 1);
    }
    if (equal(&j, kind, "device-state-complete")) {
        p->baseline = 1;
        return send_pad(slot, 0);
    }
    if (!equal(&j, kind, "input"))
        return -1;
    int type, code, v;
    if (integer(&j, field(&j, 0, "type"), &type) || integer(&j, field(&j, 0, "code"), &code) ||
        integer(&j, field(&j, 0, "value"), &v))
        return -1;
    if (type == 1) {
        unsigned flag = button(code);
        if (v)
            p->buttons |= flag;
        else
            p->buttons &= ~flag;
    }
    if (type == 3 && code >= 0 && code < 64) {
        p->axes[code] = v;
        if (code == 16)
            p->hat_x = v;
        if (code == 17)
            p->hat_y = v;
    }
    if (type == 0 && code == 0)
        return send_pad(slot, 0);
    return 0;
}
