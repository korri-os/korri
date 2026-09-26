/* Test-only persistent Wayland pointer. It validates the event observers;
 * product controller activity never uses this interface or sends input. */
#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <wayland-client.h>
#include "virtual-pointer.h"

static struct zwlr_virtual_pointer_manager_v1 *manager;
static void global(void *data, struct wl_registry *registry, uint32_t name,
        const char *interface, uint32_t version) {
    (void)data;
    (void)version;
    if (strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name) == 0) {
        manager = wl_registry_bind(registry, name,
            &zwlr_virtual_pointer_manager_v1_interface, 1);
    }
}
static void removed(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data; (void)registry; (void)name;
}
int main(void) {
    struct wl_display *display = wl_display_connect(NULL);
    assert(display);
    struct wl_registry *registry = wl_display_get_registry(display);
    const struct wl_registry_listener listener = {global, removed};
    wl_registry_add_listener(registry, &listener, NULL);
    assert(wl_display_roundtrip(display) >= 0);
    assert(manager);
    struct zwlr_virtual_pointer_v1 *pointer =
        zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, NULL);
    assert(wl_display_roundtrip(display) >= 0);
    puts("ready"); fflush(stdout);
    while (getchar() != EOF) {
        zwlr_virtual_pointer_v1_motion_absolute(pointer, 1, 100, 200, 800, 600);
        zwlr_virtual_pointer_v1_frame(pointer);
        assert(wl_display_roundtrip(display) >= 0);
        puts("moved"); fflush(stdout);
    }
    wl_display_disconnect(display);
    return 0;
}
