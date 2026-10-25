"""Compile the patched init function itself against controlled libinput facts."""
import os
from pathlib import Path
import subprocess
import sys

source = Path(sys.argv[1]).read_text()
start = source.index("void init_device_touch(")
end = source.index("\n}", start) + 2
function = source[start:end]
header = r'''
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#define WLR_ERROR 1
struct libinput_device { const char *name; const char *output; };
struct wlr_touch { char *output_name; double width_mm, height_mm; };
struct wlr_libinput_input_device { struct libinput_device *handle; struct wlr_touch touch; };
static const int libinput_touch_impl = 0;
static const char *get_libinput_device_name(struct libinput_device *d) { return d->name; }
static const char *libinput_device_get_output_name(struct libinput_device *d) { return d->output; }
static void wlr_touch_init(struct wlr_touch *t, const int *impl, const char *name) {
    (void)impl; (void)name; memset(t, 0, sizeof(*t));
}
static void libinput_device_get_size(struct libinput_device *d, double *w, double *h) {
    (void)d; *w = 100; *h = 75;
}
static void wlr_log(int level, const char *message) { (void)level; (void)message; }
'''
main = r'''
int main(void) {
    const char *outputs[] = {NULL, "", "DSI-2", "DSI-1"};
    for (unsigned i = 0; i < sizeof(outputs)/sizeof(outputs[0]); ++i) {
        struct libinput_device input = {"Goodix Capacitive TouchScreen", outputs[i]};
        struct wlr_libinput_input_device dev = {.handle = &input};
        init_device_touch(&dev);
        assert(dev.touch.width_mm == 100 && dev.touch.height_mm == 75);
        if (outputs[i] == NULL || outputs[i][0] == '\0') {
            assert(dev.touch.output_name == NULL);
        } else {
            assert(strcmp(dev.touch.output_name, outputs[i]) == 0);
            assert(dev.touch.output_name != outputs[i]);
        }
        free(dev.touch.output_name);
    }
    return 0;
}
'''
Path("touch-output-test.c").write_text(header + function + main)
subprocess.run([os.environ.get("CC", "cc"), "-Wall", "-Wextra", "-Werror", "touch-output-test.c", "-o", "touch-output-test"], check=True)
subprocess.run(["./touch-output-test"], check=True)
