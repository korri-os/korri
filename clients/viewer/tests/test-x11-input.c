// SPDX-License-Identifier: GPL-3.0-or-later
// Send actual X11 events to a real window; exercise Moonlight's X11 handler.
#include "io/input/x11.c"
#include <assert.h>
#include <stdio.h>
static int keys, buttons, moves, scrolls;
int __wrap_LiSendKeyboardEvent(short key, char action, char modifiers) {
  assert((unsigned short)key == 0x8041 && action == KEY_ACTION_DOWN);
  keys++; return 0;
}
int __wrap_LiSendMouseButtonEvent(char action, int button) {
  assert(action == BUTTON_ACTION_PRESS && button == BUTTON_LEFT);
  buttons++; return 0;
}
int __wrap_LiSendMouseMoveEvent(short x, short y) {
  if (x == 7 && y == 11) moves++;
  return 0;
}
int __wrap_LiSendScrollEvent(signed char clicks) {
  assert(clicks == 1); scrolls++; return 0;
}
static void send_event(XEvent* event, long mask) {
  assert(XSendEvent(display, window, False, mask, event));
  XSync(display, False);
  assert(x11_handler(ConnectionNumber(display)) == LOOP_OK);
}
int main(void) {
  Display* d = XOpenDisplay(NULL);
  assert(d);
  XSetWindowAttributes attrs = {.event_mask = KeyPressMask | ButtonPressMask | PointerMotionMask};
  Window w = XCreateWindow(d, DefaultRootWindow(d), 0, 0, 128, 72, 0,
    CopyFromParent, InputOutput, CopyFromParent, CWEventMask, &attrs);
  XMapWindow(d, w); XSync(d, False);
  x11_input_init(d, w);
  XEvent key = {.xkey = {.type = KeyPress, .keycode = 38}};
  send_event(&key, KeyPressMask);
  XEvent button = {.xbutton = {.type = ButtonPress, .button = Button1}};
  send_event(&button, ButtonPressMask);
  button.xbutton.button = Button4;
  send_event(&button, ButtonPressMask);
  grabbed = false; // Also exercise ungrabbed relative mouse coordinates.
  last_x = last_y = -1;
  XEvent motion = {.xmotion = {.type = MotionNotify, .x = 20, .y = 30}};
  send_event(&motion, PointerMotionMask);
  motion.xmotion.x += 7; motion.xmotion.y += 11;
  send_event(&motion, PointerMotionMask);
  assert(keys == 1 && buttons == 1 && moves == 1 && scrolls == 1);
  loop_remove_fd(ConnectionNumber(d));
  XFreeCursor(d, cursor);
  XDestroyWindow(d, w); XCloseDisplay(d);
  puts("X11 keyboard, mouse motion/button and scroll passed");
}
