#!/usr/bin/env python3
"""Exercise the selected kernel's real ADC conversion paths with in-memory IIO/input adapters."""

from pathlib import Path
import shlex
import subprocess
import sys
import tempfile

source = Path(sys.argv[1]).read_text()


def function(signature):
    start = source.index(signature)
    body = source.index("{", start)
    depth = 0
    for end in range(body, len(source)):
        depth += (source[end] == "{") - (source[end] == "}")
        if depth == 0:
            return source[start : end + 1]
    raise ValueError(f"Unclosed kernel function: {signature}")


adapters = r"""
#include <stdbool.h>
#include <stdint.h>
#include <ctype.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>
typedef uint8_t u8;
typedef uint16_t u16;
enum iio_endian { IIO_CPU, IIO_BE, IIO_LE };
struct iio_chan_spec {
    int scan_index;
    struct { int storagebits, realbits, shift; char sign; enum iio_endian endianness; } scan_type;
};
struct iio_channel { int value; struct iio_chan_spec *channel; };
struct input_dev { int min[64], max[64], values[64], syncs; void *driver; };
struct adc_joystick_axis { unsigned int code; bool inverted; };
struct adc_joystick {
    struct input_dev *input;
    struct iio_channel *chans;
    unsigned int num_chans;
    struct adc_joystick_axis axes[4];
};
static int input_abs_get_min(struct input_dev *d, unsigned int code) { return d->min[code]; }
static int input_abs_get_max(struct input_dev *d, unsigned int code) { return d->max[code]; }
static void *input_get_drvdata(struct input_dev *d) { return d->driver; }
static int iio_read_channel_raw(struct iio_channel *c, int *v) { *v = c->value; return 0; }
static void input_report_abs(struct input_dev *d, unsigned int code, int v) { d->values[code] = v; }
static void input_sync(struct input_dev *d) { d->syncs++; }
static int get_unaligned_be16(const u16 *p) { const u8 *b = (const u8 *)p; return (b[0] << 8) | b[1]; }
static int get_unaligned_le16(const u16 *p) { const u8 *b = (const u8 *)p; return b[0] | (b[1] << 8); }
static int sign_extend32(unsigned int value, int bit) { return ((int32_t)(value << (31-bit))) >> (31-bit); }
#define clamp(value, minimum, maximum) ((value) < (minimum) ? (minimum) : ((value) > (maximum) ? (maximum) : (value)))
#define GENMASK(high, low) (((1U << ((high) + 1)) - 1) & ~((1U << (low)) - 1))
"""

exercise = r"""
int main(void) {
    /* Actual RG353M channel-to-code order: X, RX, Y, RY. ABS_Z is absent.
     * Distinct ranges also catch lookups through another configured axis. */
    const unsigned int codes[4] = {0, 3, 1, 4};
    const int minimum[4] = {15, 20, 35, 40};
    const int maximum[4] = {950, 900, 800, 980};
    struct input_dev input = {0};
    struct iio_channel channels[4] = {0};
    struct iio_chan_spec specs[4] = {0};
    struct adc_joystick joy = {.input = &input, .chans = channels, .num_chans = 4};
    input.driver = &joy;
    for (int i = 0; i < 4; i++) {
        joy.axes[i].code = codes[i];
        input.min[codes[i]] = minimum[i];
        input.max[codes[i]] = maximum[i];
        specs[i].scan_index = i;
        specs[i].scan_type.storagebits = 16;
        specs[i].scan_type.realbits = 10;
        specs[i].scan_type.sign = 'u';
        specs[i].scan_type.endianness = IIO_CPU;
        channels[i].channel = &specs[i];
    }
    for (unsigned int inverted = 0; inverted < 16; inverted++) {
        for (int position = 0; position < 5; position++) {
            u16 buffer[4];
            for (int i = 0; i < 4; i++) {
                joy.axes[i].inverted = (inverted & (1U << i)) != 0;
                channels[i].value = position < 3
                    ? minimum[i] + (maximum[i] - minimum[i]) * position / 2
                    : (position == 3 ? minimum[i] - 10 : maximum[i] + 10);
                buffer[i] = channels[i].value;
            }
            for (int path = 0; path < 2; path++) {
                memset(input.values, 0, sizeof(input.values));
                if (path == 0) adc_joystick_poll(&input);
                else if (adc_joystick_handle(buffer, &joy) != 0) return 1;
                for (int i = 0; i < 4; i++) {
                    int expected = channels[i].value;
                    if (joy.axes[i].inverted) expected = minimum[i] + maximum[i] - expected;
                    expected = clamp(expected, minimum[i], maximum[i]);
                    if (input.values[codes[i]] != expected) {
                        fprintf(stderr, "path=%s channel=%d ABS=%u inverted=%d input=%d expected=%d actual=%d\n",
                            path ? "buffered" : "polled", i, codes[i], joy.axes[i].inverted,
                            channels[i].value, expected, input.values[codes[i]]);
                        return 1;
                    }
                }
            }
        }
    }
    if (input.syncs != 160) return 1;
    puts("Both ADC paths preserve centers and endpoints for all axis directions (640 axis checks)");
    return 0;
}
"""

functions = "\n".join(
    function(signature)
    for signature in [
        "static int adc_joystick_invert(",
        "static void adc_joystick_poll(",
        "static int adc_joystick_handle(",
    ]
)
with tempfile.TemporaryDirectory(prefix="adc-joystick-test-") as directory:
    root = Path(directory)
    c = root / "test.c"
    binary = root / "test"
    c.write_text(adapters + functions + exercise)
    subprocess.run(
        # Kbuild permits its existing int/unsigned loop counters.
        shlex.split(sys.argv[2])
        + [
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wno-sign-compare",
            str(c),
            "-o",
            str(binary),
        ],
        check=True,
    )
    subprocess.run([str(binary)], check=True)
