// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <Limelight.h>

PAUDIO_RENDERER_CALLBACKS viewer_audio_callbacks(int vaapi, char* device);
int viewer_input_init(int vaapi, int viewonly, char* mapping, int mouse_emulation, int verbose, int rotate);
int viewer_input_gamepad_mask(void);
void viewer_input_callbacks(PCONNECTION_LISTENER_CALLBACKS callbacks);
void viewer_input_start(void);
void viewer_input_stop(void);
void viewer_input_destroy(void);
void viewer_input_x11(void* display, unsigned long window);
int viewer_input_sdl(void* window, void* event);
