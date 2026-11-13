// SPDX-License-Identifier: GPL-3.0-or-later
// Actual SDL event loop, SDL virtual controller and carried input handlers.
// Record their network-core boundary, without a live streaming peer.
#include "sdl.c"
#include "io/bridge.h"
#include <assert.h>
#include <string.h>
#include <unistd.h>

static int keys, buttons, positions, controllers;
static unsigned short last_key;
static int last_buttons;
static short last_y;
int __wrap_LiSendKeyboardEvent(short key, char action, char modifiers) {
  keys++; last_key = key; return 0;
}
int __wrap_LiSendMouseButtonEvent(char action, int button) {
  assert(action == BUTTON_ACTION_PRESS && button == BUTTON_LEFT);
  buttons++; return 0;
}
int __wrap_LiSendMousePositionEvent(short x, short y, short width, short height) {
  assert(width == 128 && height == 72);
  if (x == 17 && y == 29) positions++;
  return 0;
}
int __wrap_LiSendMultiControllerEvent(short id, short mask, int flags,
    unsigned char lt, unsigned char rt, short lx, short ly, short rx, short ry) {
  controllers++; last_buttons = flags; last_y = ly; return 0;
}
static void pump(void) {
  for (int i = 0; i < 50; ++i) assert(sdl_poll(2) == 0);
}
static void send_events(void) {
  SDL_Event key = {.type = SDL_KEYDOWN};
  key.key.keysym.scancode = SDL_SCANCODE_A;
  assert(SDL_PushEvent(&key) == 1);
  SDL_Event button = {.type = SDL_MOUSEBUTTONDOWN};
  button.button.button = SDL_BUTTON_LEFT;
  assert(SDL_PushEvent(&button) == 1);
  SDL_Event motion = {.type = SDL_MOUSEMOTION};
  motion.motion.x = 17; motion.motion.y = 29;
  assert(SDL_PushEvent(&motion) == 1);
  pump();
}
int main(int argc, char** argv) {
  sdl_init(128, 72, false);
  if (argc == 2 && strcmp(argv[1], "viewonly") == 0) {
    // Both platform branches must return before even controller discovery.
    for (int vaapi = 0; vaapi <= 1; ++vaapi) {
      assert(viewer_input_init(vaapi, 1, NULL, 1, 0, 0) == 0);
      assert(viewer_input_gamepad_mask() == 0);
      CONNECTION_LISTENER_CALLBACKS cb = {0};
      viewer_input_callbacks(&cb);
      assert(!cb.rumble && !cb.rumbleTriggers && !cb.setControllerLED);
      send_events();
      assert(!keys && !buttons && !positions && !controllers);
      viewer_input_start(); viewer_input_stop(); viewer_input_destroy();
    }
    puts("viewonly: no input forwarding or discovery on either PC platform");
  } else {
    assert(viewer_input_init(0, 0, NULL, 1, 0, 0) == 0);
    send_events();
    assert(keys == 1 && last_key == 0x8041);
    assert(buttons == 1 && positions == 1);
    int index = SDL_JoystickAttachVirtual(SDL_JOYSTICK_TYPE_GAMECONTROLLER,
      SDL_CONTROLLER_AXIS_MAX, SDL_CONTROLLER_BUTTON_MAX, 0);
    assert(index >= 0);
    pump();
    assert(viewer_input_gamepad_mask() == 1);
    SDL_Joystick* joystick = SDL_JoystickOpen(index);
    assert(joystick);
    assert(SDL_JoystickSetVirtualButton(joystick, SDL_CONTROLLER_BUTTON_A, 1) == 0);
    pump();
    assert(controllers > 0 && (last_buttons & A_FLAG));
    assert(SDL_JoystickSetVirtualAxis(joystick, SDL_CONTROLLER_AXIS_LEFTY, -12345) == 0);
    pump();
    assert(last_y == 12345);
    CONNECTION_LISTENER_CALLBACKS cb = {0};
    viewer_input_callbacks(&cb);
    assert(cb.rumble && cb.rumbleTriggers && cb.setControllerLED && cb.setMotionEventState);
    // Patch 0014: Korri owns the gamepad quit chord.
    setenv("KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT", "1", 1);
    int chord[] = {SDL_CONTROLLER_BUTTON_START, SDL_CONTROLLER_BUTTON_BACK,
      SDL_CONTROLLER_BUTTON_LEFTSHOULDER, SDL_CONTROLLER_BUTTON_RIGHTSHOULDER};
    for (int i = 0; i < 4; ++i)
      assert(SDL_JoystickSetVirtualButton(joystick, chord[i], 1) == 0);
    pump();
    assert((last_buttons & (PLAY_FLAG | BACK_FLAG | LB_FLAG | RB_FLAG)) == (PLAY_FLAG | BACK_FLAG | LB_FLAG | RB_FLAG));
    SDL_JoystickClose(joystick);
    assert(SDL_JoystickDetachVirtual(index) == 0);
    pump();
    assert(viewer_input_gamepad_mask() == 0);
    viewer_input_stop(); viewer_input_destroy();
    puts("SDL keyboard, mouse, virtual controller, hotplug and quit ownership passed");
  }
  sdl_destroy();
}
