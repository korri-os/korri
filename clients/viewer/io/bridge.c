// SPDX-License-Identifier: GPL-3.0-or-later
// Platform dispatch from Moonlight's main.c/platform.c. Rust owns the session.
#include "bridge.h"
#include "audio/audio.h"
#include "input/evdev.h"
#include "input/udev.h"
#include "input/mapping.h"
#include "input/sdl.h"
#include "input/x11.h"
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

static int enabled, using_evdev, started;
static struct mapping* mappings;

PAUDIO_RENDERER_CALLBACKS viewer_audio_callbacks(int vaapi, char* device) {
  if (!vaapi) return &audio_callbacks_sdl;
  if (audio_pulse_init(device)) return &audio_callbacks_pulse;
  return &audio_callbacks_alsa;
}

int viewer_input_init(int vaapi, int viewonly, char* mapping, int mouse_emulation, int verbose, int rotate) {
  if (viewonly) return 0;
  using_evdev = vaapi;
  if (using_evdev) {
    const char* env = getenv("SDL_GAMECONTROLLERCONFIG");
    if (!mapping && !env) {
      fprintf(stderr, "Please specify mapping file as default mapping could not be found.\n");
      return -1;
    }
    if (mapping) mappings = mapping_load(mapping, verbose);
    if (env) {
      // mapping_parse tokenizes its argument. Do not alter the environment.
      char* copy = strdup(env);
      if (!copy) return -1;
      struct mapping* map = mapping_parse(copy);
      free(copy);
      if (!map) return -1;
      map->next = mappings;
      mappings = map;
    }
    evdev_init(mouse_emulation);
    udev_init(true, mappings, verbose, rotate);
  } else {
    sdlinput_init(mapping);
  }
  enabled = 1;
  return 0;
}

int viewer_input_gamepad_mask(void) {
  if (!enabled) return 0;
  int count = using_evdev ? evdev_gamepads : sdl_gamepads;
  return count >= 16 ? 0xffff : (1 << count) - 1;
}
void viewer_input_callbacks(PCONNECTION_LISTENER_CALLBACKS callbacks) {
  if (!enabled) return;
  callbacks->rumble = using_evdev ? evdev_rumble : sdlinput_rumble;
  if (!using_evdev) {
    callbacks->rumbleTriggers = sdlinput_rumble_triggers;
    callbacks->setMotionEventState = sdlinput_set_motion_event_state;
    callbacks->setControllerLED = sdlinput_set_controller_led;
  }
}
void viewer_input_start(void) {
  if (enabled && using_evdev) {
    evdev_start();
    started = 1;
  } else if (enabled) {
    // Moonlight's sdl_loop captures once on entry, not on every timed poll.
    SDL_SetRelativeMouseMode(SDL_TRUE);
  }
}
void viewer_input_stop(void) {
  if (started) evdev_stop();
  started = 0;
}
void viewer_input_destroy(void) {
  viewer_input_stop();
  if (enabled) {
    if (using_evdev) {
      udev_destroy();
      evdev_destroy();
    } else {
      sdlinput_destroy();
    }
  }
  while (mappings) {
    struct mapping* next = mappings->next;
    free(mappings);
    mappings = next;
  }
  enabled = 0;
}
void viewer_input_x11(void* display, unsigned long window) {
  if (enabled) x11_input_init(display, window);
}
int viewer_input_sdl(void* window, void* event) {
  return enabled ? sdlinput_handle_event(window, event) : 0;
}
