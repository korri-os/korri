// SPDX-License-Identifier: GPL-3.0-or-later
// Real libevdev capabilities and Moonlight's mapping/event code. No physical
// input node is opened or grabbed on the build machine.
#include "io/input/evdev.c"
#include <assert.h>
static int keys, buttons, moves, controllers;
static short player;
static int active, flags;
int __wrap_LiSendKeyboardEvent(short key, char action, char modifiers) {
  assert((unsigned short)key == 0x8041 && action == KEY_ACTION_DOWN);
  keys++; return 0;
}
int __wrap_LiSendMouseButtonEvent(char action, int button) {
  assert(action == BUTTON_ACTION_PRESS && button == BUTTON_LEFT);
  buttons++; return 0;
}
int __wrap_LiSendMouseMoveEvent(short x, short y) {
  assert(x == 7 && y == 11); moves++; return 0;
}
int __wrap_LiSendMultiControllerEvent(short id, short mask, int state,
    unsigned char lt, unsigned char rt, short lx, short ly, short rx, short ry) {
  controllers++; player = id; active = mask; flags = state; return 0;
}
static void event(struct input_device* dev, int type, int code, int value) {
  struct input_event e = {.type = type, .code = code, .value = value};
  assert(evdev_handle_event(&e, dev));
}
int main(int argc, char** argv) {
  assert(argc == 2);
  // The real shipped controller database is the mapping producer.
  struct mapping* maps = mapping_load(argv[1], false);
  struct mapping* map = maps;
  while (map && strcmp(map->guid, "030000005e0400008e02000001000000")) map = map->next;
  assert(map);
  struct input_device dev = {.dev = libevdev_new(), .controllerId = -1, .map = map};
  assert(dev.dev);
  libevdev_set_name(dev.dev, "Korri build check");
  libevdev_enable_event_code(dev.dev, EV_KEY, BTN_A, NULL);
  libevdev_enable_event_code(dev.dev, EV_KEY, KEY_A, NULL);
  dev.key_map[BTN_A] = map->btn_a;
  evdev_init(false);
  event(&dev, EV_KEY, KEY_A, 1);
  event(&dev, EV_KEY, BTN_LEFT, 1);
  event(&dev, EV_REL, REL_X, 7);
  event(&dev, EV_REL, REL_Y, 11);
  event(&dev, EV_SYN, SYN_REPORT, 0);
  event(&dev, EV_KEY, BTN_A, 1);
  event(&dev, EV_SYN, SYN_REPORT, 0);
  assert(keys == 1 && buttons == 1 && moves == 1);
  assert(controllers == 1 && player == 0 && active == 1 && flags == A_FLAG);
  event(&dev, EV_KEY, BTN_A, 0);
  event(&dev, EV_SYN, SYN_REPORT, 0);
  assert(controllers == 2 && flags == 0);
  setenv("KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT", "1", 1);
  dev.buttonFlags = PLAY_FLAG | BACK_FLAG | LB_FLAG | RB_FLAG;
  event(&dev, EV_KEY, BTN_A, 1);
  event(&dev, EV_SYN, SYN_REPORT, 0);
  assert((flags & A_FLAG) && (flags & QUIT_BUTTONS) == QUIT_BUTTONS);
  libevdev_free(dev.dev);
  while (maps) { struct mapping* next = maps->next; free(maps); maps = next; }
  puts("evdev keyboard, mouse, mapped controller, first-press seat and quit ownership passed");
}
