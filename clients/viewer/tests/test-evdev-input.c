// SPDX-License-Identifier: GPL-3.0-or-later
// Real libevdev capabilities and Moonlight's mapping/event code. No physical
// input node is opened or grabbed on the build machine.
#include "io/input/evdev.c"
#include <assert.h>
#include <errno.h>
// Allocation fails only when a check asks for it.
static int refuse_calloc, refuse_realloc;
void* __real_calloc(size_t count, size_t size);
void* __wrap_calloc(size_t count, size_t size) {
  if (refuse_calloc) { errno = ENOMEM; return NULL; }
  return __real_calloc(count, size);
}
void* __real_realloc(void* old, size_t size);
void* __wrap_realloc(void* old, size_t size) {
  if (refuse_realloc) { errno = ENOMEM; return NULL; }
  return __real_realloc(old, size);
}
static void quiet(const struct libevdev* dev, enum libevdev_log_priority priority, void* data,
    const char* file, int line, const char* func, const char* format, va_list args) {}
static int lowest_free_descriptor(void) {
  int probe = dup(0);
  assert(probe >= 0);
  close(probe);
  return probe;
}
// #468: each of these used to exit() the process, past Rust's lifecycle
// report and socket removal. Each now returns its failure to the caller.
static void failures_return_errors(void) {
  struct mapping* none = NULL;
  assert(mapping_load("/nonexistent/korri-mapping.txt", false, &none) == -1 && !none);

  char line[] = "030000005e0400008e02000001000000,Microsoft Xbox 360,a:b0,";
  refuse_calloc = 1;
  errno = 0;
  assert(mapping_parse(line) == NULL && errno == ENOMEM);
  char file[4096];
  const char* scratch = getenv("TMPDIR");
  snprintf(file, sizeof(file), "%s/korri-mapping-XXXXXX", scratch ? scratch : "/tmp");
  int written = mkstemp(file);
  assert(written >= 0);
  const char* record = "030000005e0400008e02000001000000,Microsoft Xbox 360,a:b0,\n";
  assert(write(written, record, strlen(record)) == (ssize_t)strlen(record));
  close(written);
  assert(mapping_load(file, false, &none) == -1 && !none);
  refuse_calloc = 0;
  assert(mapping_load(file, false, &none) == 0 && none && !none->next);
  free(none);
  unlink(file);

  // /dev/null is no input device. Its node classifies as neither a gamepad
  // nor an accelerometer, so evdev_create reaches its allocation.
  int descriptor = lowest_free_descriptor();
  refuse_realloc = 1;
  assert(evdev_create("/dev/null", NULL, false, 0) == -1);
  refuse_realloc = 0;
  assert(numDevices == 0 && devices == NULL);
  assert(lowest_free_descriptor() == descriptor);

  // A read error other than EAGAIN or ENODEV ends the Rust-polled loop with
  // an error. An unattached libevdev reports EBADF without reading.
  devices = calloc(1, sizeof(*devices));
  numDevices = 1;
  devices[0].dev = libevdev_new();
  libevdev_set_device_log_function(devices[0].dev, quiet, LIBEVDEV_LOG_ERROR, NULL);
  devices[0].fd = -1;
  devices[0].controllerId = -1;
  assert(evdev_handle(-1) < 0);
  evdev_destroy();
  puts("mapping, allocation, device and read failures return errors");
}
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
  failures_return_errors();
  struct mapping* maps = NULL;
  assert(mapping_load(argv[1], false, &maps) == 0);
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
  // Mouse-emulation producers must stop before Rust stops the network core.
  devices = calloc(1, sizeof(*devices));
  numDevices = 1;
  devices[0].dev = libevdev_new();
  devices[0].fd = -1;
  devices[0].controllerId = -1;
  devices[0].mouseEmulation = true;
  assert(pthread_create(&devices[0].meThread, NULL, HandleMouseEmulation, &devices[0]) == 0);
  evdev_stop();
  assert(!devices[0].mouseEmulation);
  assert(devices[0].meThread == 0);
  evdev_destroy();
  puts("evdev keyboard, mouse, mapped controller, first-press seat, quit ownership and producer shutdown passed");
}
