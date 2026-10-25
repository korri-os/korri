/* Public compiled-backend test. Real uinput/udev/libinput devices live in a VM. */
#define WLR_USE_UNSTABLE
#include <assert.h>
#include <libinput.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <wayland-server-core.h>
#include <wlr/backend.h>
#include <wlr/backend/libinput.h>
#include <wlr/backend/session.h>
#include <wlr/types/wlr_input_device.h>
#include <wlr/types/wlr_touch.h>

static const char **arguments;
static bool seen[3];

static void new_input(struct wl_listener *listener, void *data) {
    (void)listener;
    struct wlr_input_device *device = data;
    if (device->type != WLR_INPUT_DEVICE_TOUCH) {
        return;
    }
    struct libinput_device *input = wlr_libinput_get_device_handle(device);
    const char *sysname = libinput_device_get_sysname(input);
    for (int i = 0; i < 3; ++i) {
        if (strcmp(sysname, arguments[1 + 2*i]) != 0) {
            continue;
        }
        const char *expected = arguments[2 + 2*i];
        const char *output = wlr_touch_from_input_device(device)->output_name;
        if (strcmp(expected, "-") == 0) {
            assert(output == NULL);
        } else {
            assert(output != NULL && strcmp(output, expected) == 0);
        }
        seen[i] = true;
    }
}

int main(int argc, const char **argv) {
    assert(argc == 7);
    arguments = argv;
    struct wl_event_loop *loop = wl_event_loop_create();
    assert(loop != NULL);
    struct wlr_session *session = wlr_session_create(loop);
    assert(session != NULL);
    struct wlr_backend *backend = wlr_libinput_backend_create(session);
    assert(backend != NULL);
    struct wl_listener input = {.notify = new_input};
    wl_signal_add(&backend->events.new_input, &input);
    assert(wlr_backend_start(backend));
    for (int i = 0; i < 100 && !(seen[0] && seen[1] && seen[2]); ++i) {
        assert(wl_event_loop_dispatch(loop, 100) >= 0);
    }
    assert(seen[0] && seen[1] && seen[2]);
    wl_list_remove(&input.link);
    wlr_backend_destroy(backend);
    wlr_session_destroy(session);
    wl_event_loop_destroy(loop);
    puts("PASS: compiled wlroots libinput backend preserves both WL_OUTPUT names and the unmapped device");
    return 0;
}
