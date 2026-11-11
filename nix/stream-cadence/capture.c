/* Outside measure for a viewer: read the frame-ID bar from the compositor.
 *
 * A wlr-screencopy client. `probe` writes one whole output frame as a PGM so
 * the host can find the bar. `rows` samples the 36 cells of both bar rows on
 * every output frame and stamps the frame with the compositor's commit time.
 * It also records when it asked for each frame, so a gap in the record can be
 * told apart: the compositor showed no new frame, or this client asked too late.
 * bar.py decodes the samples; this file never interprets an ID.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <inttypes.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include "frame-bar.h"
#include "wlr-screencopy-unstable-v1-client-protocol.h"

#define Y_INVERT 1u

static struct wl_display *display;
static struct wl_shm *shm;
static struct wl_output *output;
static struct zwlr_screencopy_manager_v1 *manager;
static uint32_t manager_version;

static struct wl_buffer *buffer;
static uint8_t *pixels;
static size_t mapped;
static uint32_t format, width, height, stride, flags;
static int have_buffer, done, failed;
static uint64_t ready_ns, copy_ns;
static volatile sig_atomic_t stop;

static void die(const char *message) {
    fprintf(stderr, "CAPTURE %s\n", message);
    exit(1);
}

static void on_signal(int signum) { (void)signum; stop = 1; }

static void frame_buffer(void *data, struct zwlr_screencopy_frame_v1 *frame,
                         uint32_t f, uint32_t w, uint32_t h, uint32_t s) {
    (void)data; (void)frame;
    if (buffer && (f != format || w != width || h != height || s != stride)) {
        wl_buffer_destroy(buffer);
        munmap(pixels, mapped);
        buffer = NULL;
    }
    format = f; width = w; height = h; stride = s;
    have_buffer = 1;
}

static void copy(struct zwlr_screencopy_frame_v1 *frame) {
    if (!have_buffer) die("compositor offered no shm buffer");
    /* Green sits at byte 1 in each of these layouts. The bar is black and white. */
    if (format != WL_SHM_FORMAT_ARGB8888 && format != WL_SHM_FORMAT_XRGB8888 &&
        format != WL_SHM_FORMAT_ABGR8888 && format != WL_SHM_FORMAT_XBGR8888)
        die("unsupported shm pixel format");
    if (!buffer) {
        mapped = (size_t)stride * height;
        int fd = memfd_create("stream-cadence-capture", MFD_CLOEXEC);
        if (fd < 0 || ftruncate(fd, (off_t)mapped)) die("cannot allocate capture buffer");
        pixels = mmap(NULL, mapped, PROT_READ, MAP_SHARED, fd, 0);
        if (pixels == MAP_FAILED) die("cannot map capture buffer");
        struct wl_shm_pool *pool = wl_shm_create_pool(shm, fd, (int32_t)mapped);
        buffer = wl_shm_pool_create_buffer(pool, 0, (int32_t)width, (int32_t)height, (int32_t)stride, format);
        wl_shm_pool_destroy(pool);
        close(fd);
    }
    zwlr_screencopy_frame_v1_copy(frame, buffer);
    /* The compositor copies at its next commit after this request arrives. */
    wl_display_flush(display);
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now)) die("cannot read the monotonic clock");
    copy_ns = (uint64_t)now.tv_sec * UINT64_C(1000000000) + (uint64_t)now.tv_nsec;
}

static void frame_buffer_done(void *data, struct zwlr_screencopy_frame_v1 *frame) {
    (void)data;
    copy(frame);
}

static void frame_flags(void *data, struct zwlr_screencopy_frame_v1 *frame, uint32_t f) {
    (void)data; (void)frame;
    flags = f;
}

static void frame_ready(void *data, struct zwlr_screencopy_frame_v1 *frame,
                        uint32_t sec_hi, uint32_t sec_lo, uint32_t nsec) {
    (void)data; (void)frame;
    ready_ns = (((uint64_t)sec_hi << 32) | sec_lo) * UINT64_C(1000000000) + nsec;
    done = 1;
}

static void frame_failed(void *data, struct zwlr_screencopy_frame_v1 *frame) {
    (void)data; (void)frame;
    failed = 1;
}

static void frame_damage(void *data, struct zwlr_screencopy_frame_v1 *frame,
                         uint32_t x, uint32_t y, uint32_t w, uint32_t h) {
    (void)data; (void)frame; (void)x; (void)y; (void)w; (void)h;
}

static void frame_dmabuf(void *data, struct zwlr_screencopy_frame_v1 *frame,
                         uint32_t f, uint32_t w, uint32_t h) {
    (void)data; (void)frame; (void)f; (void)w; (void)h;
}

static const struct zwlr_screencopy_frame_v1_listener frame_listener = {
    .buffer = frame_buffer,
    .flags = frame_flags,
    .ready = frame_ready,
    .failed = frame_failed,
    .damage = frame_damage,
    .linux_dmabuf = frame_dmabuf,
    .buffer_done = frame_buffer_done,
};

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version) {
    (void)data;
    if (!strcmp(interface, wl_shm_interface.name)) {
        shm = wl_registry_bind(registry, name, &wl_shm_interface, 1);
    } else if (!strcmp(interface, wl_output_interface.name) && !output) {
        output = wl_registry_bind(registry, name, &wl_output_interface, 1);
    } else if (!strcmp(interface, zwlr_screencopy_manager_v1_interface.name)) {
        manager_version = version < 3 ? version : 3;
        manager = wl_registry_bind(registry, name, &zwlr_screencopy_manager_v1_interface, manager_version);
    }
}

static void global_remove(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data; (void)registry; (void)name;
}

static const struct wl_registry_listener registry_listener = {global, global_remove};

/* Returns 0 when a frame was copied, 1 when a stop signal ended the wait. */
static int capture(int region, int y, int w, int h) {
    struct zwlr_screencopy_frame_v1 *frame = region
        ? zwlr_screencopy_manager_v1_capture_output_region(manager, 0, output, 0, y, w, h)
        : zwlr_screencopy_manager_v1_capture_output(manager, 0, output);
    done = failed = have_buffer = 0;
    zwlr_screencopy_frame_v1_add_listener(frame, &frame_listener, NULL);
    while (!done && !failed && !stop) {
        /* Before version 3 no buffer_done event follows the buffer event. */
        if (have_buffer == 1 && manager_version < 3) {
            have_buffer = 2;
            copy(frame);
        }
        if (wl_display_dispatch(display) < 0 && errno != EINTR) die("compositor connection lost");
    }
    zwlr_screencopy_frame_v1_destroy(frame);
    if (failed) die("compositor failed the frame copy");
    return done ? 0 : 1;
}

static const uint8_t *row(uint32_t index) {
    return pixels + (size_t)stride * ((flags & Y_INVERT) ? height - 1 - index : index);
}

static int probe(const char *path) {
    if (capture(0, 0, 0, 0)) return 1;
    FILE *file = fopen(path, "wb");
    if (!file) die("cannot write probe frame");
    fprintf(file, "P5\n%" PRIu32 " %" PRIu32 "\n255\n", width, height);
    for (uint32_t y = 0; y < height; y++)
        for (uint32_t x = 0; x < width; x++)
            if (fputc(row(y)[x * 4 + 1], file) == EOF) die("cannot write probe frame");
    if (fclose(file)) die("cannot write probe frame");
    return 0;
}

static int rows(char **args) {
    int output_width = atoi(args[0]), upper = atoi(args[1]), lower = atoi(args[2]);
    double x = atof(args[3]), cell = atof(args[4]);
    if (output_width <= 0 || upper < 0 || lower <= upper || cell < 2)
        die("invalid bar geometry");
    FILE *times = fopen(args[5], "w"), *samples = fopen(args[6], "wb");
    if (!times || !samples) die("cannot write capture records");
    fprintf(times, "index,display_ns,copy_ns\n");
    uint64_t count = 0;
    while (!stop && !capture(1, upper, output_width, lower - upper + 1)) {
        uint8_t values[2 * BAR_BITS];
        for (int line = 0; line < 2; line++)
            for (int bit = 0; bit < BAR_BITS; bit++) {
                /* The same cell centre that bar.py:read samples. */
                uint32_t column = (uint32_t)(x + (bit + 0.5) * cell);
                if (column >= width) die("bar lies outside the captured rows");
                values[line * BAR_BITS + bit] = row(line ? height - 1 : 0)[column * 4 + 1];
            }
        if (fwrite(values, 1, sizeof values, samples) != sizeof values ||
            fprintf(times, "%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n", count++, ready_ns, copy_ns) < 0 ||
            fflush(samples) || fflush(times))
            die("cannot write capture records");
    }
    if (fclose(times) || fclose(samples)) die("cannot write capture records");
    fprintf(stderr, "CAPTURE final frames=%" PRIu64 "\n", count);
    return 0;
}

int main(int argc, char **argv) {
    int is_probe = argc == 3 && !strcmp(argv[1], "probe");
    int is_rows = argc == 9 && !strcmp(argv[1], "rows");
    if (!is_probe && !is_rows) {
        fprintf(stderr, "usage: %s probe FRAME.pgm\n"
                        "       %s rows OUTPUT_WIDTH UPPER_Y LOWER_Y X CELL TIMES.csv SAMPLES.bin\n",
                argv[0], argv[0]);
        return 2;
    }
    struct sigaction action = {.sa_handler = on_signal};
    sigaction(SIGINT, &action, NULL);
    sigaction(SIGTERM, &action, NULL);
    display = wl_display_connect(NULL);
    if (!display) die("cannot connect to the compositor");
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    if (wl_display_roundtrip(display) < 0) die("compositor connection lost");
    if (!shm || !output || !manager) die("compositor lacks wl_shm, an output or wlr-screencopy");
    return is_probe ? probe(argv[2]) : rows(argv + 2);
}
