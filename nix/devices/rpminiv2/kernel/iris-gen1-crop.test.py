#!/usr/bin/env python3
"""Compile gen1 resolution/stride setters from Linux 7.2 and check HFI payloads.

Usage: python3 iris-gen1-crop.test.py /path/to/pristine/linux-7.2 [--unpatched]
This checks the real setters, not firmware. Hardware encoding is separate.
"""
import argparse
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
IRIS = Path("drivers/media/platform/qcom/iris")
PATCH = HERE / "patches/9999-media-qcom-iris-gen1-visible-encoder-resolution.patch"


def function(text, name):
    match = re.search(r"(?:static )?(?:int|bool) " + name + r"\(.*?\n\}", text, re.S)
    assert match, name
    return match[0]


def verify(source, patched=True, patch=PATCH):
    with tempfile.TemporaryDirectory(prefix="iris-crop-") as directory:
        root = Path(directory)
        shutil.copytree(source / IRIS, root / IRIS)
        if patched:
            subprocess.run(["patch", "--batch", "--fuzz=0", "-p1", "-i", str(patch)], cwd=root, check=True)
        iris = root / IRIS
        text = (iris / "iris_hfi_gen1_command.c").read_text()
        venc = (iris / "iris_venc.c").read_text()
        harness = r'''
#include <assert.h>
#include <stdbool.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <linux/videodev2.h>
typedef uint32_t u32;
typedef uint8_t u8;
#define BIT(n) (1U << (n))
#define ALIGN(x, a) (((x) + (a) - 1) & ~((a) - 1))
#define ARRAY_SIZE(x) (sizeof(x) / sizeof((x)[0]))
#define V4L2_PIX_FMT_QC08C v4l2_fourcc('Q', '0', '8', 'C')
enum { IRIS_FMT_H264, IRIS_FMT_HEVC };
enum { IRIS_FMT_NV12, IRIS_FMT_QC08C };
enum { BUF_INPUT, BUF_OUTPUT };
#include "iris_hfi_gen1_defines.h"
enum { DECODER, ENCODER, IRIS_INST_SUB_FIRST_IPSC = 1 };
struct iris_inst {
    u32 domain, sub_state;
    struct v4l2_format *fmt_src, *fmt_dst;
    struct v4l2_rect crop;
    u32 enc_raw_width, enc_raw_height, enc_scale_width, enc_scale_height, codec;
    struct { u32 min_count, size; } buffers[2];
};
/* Allocation providers do not affect the geometry under test. */
static u32 iris_get_buffer_size(struct iris_inst *inst, u32 type) {
    (void)inst; (void)type; return 4 * 1024 * 1024;
}
static u32 iris_vpu_buf_count(struct iris_inst *inst, u32 type) {
    (void)inst; (void)type; return 4;
}
static int iris_drc_pending(struct iris_inst *inst) { (void)inst; return 0; }
static unsigned calls, error_call;
static u32 properties[4], payloads[4][64];
static int hfi_gen1_set_property(struct iris_inst *inst, u32 property, void *payload, u32 size) {
    (void)inst;
    assert(calls < 4 && size <= sizeof(payloads[0]));
    properties[calls] = property;
    memcpy(payloads[calls++], payload, size);
    return calls == error_call ? -5 : 0;
}
'''
        for array in ("iris_venc_formats_cap", "iris_venc_formats_out"):
            harness += re.search(r"static const u32 " + array + r"\[\] = \{.*?\n\};", venc, re.S)[0] + "\n"
        for name in ("check_format", "iris_venc_try_fmt", "iris_venc_s_fmt_output", "iris_venc_s_fmt_input", "iris_venc_s_selection"):
            harness += function(venc, name) + "\n"
        harness += function(text, "iris_hfi_gen1_set_resolution") + "\n"
        harness += function(text, "iris_hfi_gen1_set_stride") + r'''
static void reset(void) { calls = 0; error_call = 0; memset(payloads, 0, sizeof(payloads)); }
static void frame(unsigned n, u32 type, u32 width, u32 height) {
    struct hfi_framesize *fs = (void *)payloads[n];
    assert(properties[n] == HFI_PROPERTY_PARAM_FRAME_SIZE);
    assert(fs->buffer_type == type && fs->width == width && fs->height == height);
}
int main(void) {
    struct v4l2_format src = {0}, dst = {0};
    struct iris_inst inst = { .domain = ENCODER, .fmt_src = &src, .fmt_dst = &dst };
    const u32 sizes[][2] = {{1240,1080}, {1280,720}, {1920,1080}};
    for (unsigned i = 0; i < 3; ++i) {
        u32 width = sizes[i][0], height = sizes[i][1];
        struct v4l2_format raw = { .type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE };
        raw.fmt.pix_mp.width = width; raw.fmt.pix_mp.height = height;
        raw.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_NV12;
        dst.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_H264;
        assert(iris_venc_s_fmt_input(&inst, &raw) == 0);
        assert(raw.fmt.pix_mp.width == ALIGN(width, 128));
        assert(raw.fmt.pix_mp.height == ALIGN(height, 32));
        assert(inst.crop.width == width && inst.crop.height == height);
        struct v4l2_format coded = { .type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE };
        coded.fmt.pix_mp.width = width; coded.fmt.pix_mp.height = height;
        coded.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_H264;
        assert(iris_venc_s_fmt_output(&inst, &coded) == 0);
        reset();
        assert(iris_hfi_gen1_set_resolution(&inst, V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE) == 0);
        assert(calls == 2);
        frame(0, HFI_BUFFER_INPUT, width, height);
        frame(1, HFI_BUFFER_OUTPUT, width, height);
        reset();
        assert(iris_hfi_gen1_set_stride(&inst, V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE) == 0);
        assert(calls == 1 && properties[0] == HFI_PROPERTY_PARAM_UNCOMPRESSED_PLANE_ACTUAL_INFO);
        struct hfi_uncompressed_plane_actual_info *p = (void *)payloads[0];
        assert(p->buffer_type == HFI_BUFFER_INPUT && p->num_planes == 2);
        assert(p->plane_format[0].actual_stride == (int)ALIGN(width, 128));
        assert(p->plane_format[1].actual_stride == (int)ALIGN(width, 128));
        assert(p->plane_format[0].actual_plane_buffer_height == ALIGN(height, 32));
        assert(p->plane_format[1].actual_plane_buffer_height == ALIGN(height, 32) / 2);
    }
    /* Explicit selection remains supported independently of default crop. */
    struct v4l2_selection selection = { .type = V4L2_BUF_TYPE_VIDEO_OUTPUT,
        .target = V4L2_SEL_TGT_CROP, .r = { .width = 1280, .height = 720 } };
    assert(iris_venc_s_selection(&inst, &selection) == 0);
    reset();
    assert(iris_hfi_gen1_set_resolution(&inst, 0) == 0);
    frame(0, HFI_BUFFER_INPUT, 1280, 720);
    frame(1, HFI_BUFFER_OUTPUT, 1280, 720);
    inst.crop.width = 1920; inst.crop.height = 1080;
    /* A real scaling request must remain distinct from the source crop. */
    inst.enc_scale_width = 640; inst.enc_scale_height = 360;
    reset();
    assert(iris_hfi_gen1_set_resolution(&inst, 0) == 0);
    frame(0, HFI_BUFFER_INPUT, 1920, 1080);
    frame(1, HFI_BUFFER_OUTPUT, 640, 360);
    /* Decoder resolution and dynamic-resolution paths stay unchanged. */
    inst.domain = DECODER;
    reset();
    assert(iris_hfi_gen1_set_resolution(&inst, 0) == 0);
    frame(0, HFI_BUFFER_INPUT, src.fmt.pix_mp.width, src.fmt.pix_mp.height);
    frame(1, HFI_BUFFER_OUTPUT2, dst.fmt.pix_mp.width, dst.fmt.pix_mp.height);
    inst.sub_state = IRIS_INST_SUB_FIRST_IPSC;
    reset();
    assert(iris_hfi_gen1_set_resolution(&inst, 0) == 0 && calls == 1);
    frame(0, HFI_BUFFER_OUTPUT2, dst.fmt.pix_mp.width, dst.fmt.pix_mp.height);
    inst.sub_state = 0;
    reset(); error_call = 1;
    assert(iris_hfi_gen1_set_resolution(&inst, 0) == -5 && calls == 1);
    reset(); error_call = 2;
    assert(iris_hfi_gen1_set_resolution(&inst, 0) == -5 && calls == 2);
    puts("PASS: visible encoder resolutions, aligned strides, scaling, decoder/DRC preservation, transport errors");
}
'''
        (root / "check.c").write_text(harness)
        subprocess.run(["cc", "-std=gnu11", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter", "-I", str(iris), str(root / "check.c"), "-o", str(root / "check")], check=True)
        subprocess.run([str(root / "check")], check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--unpatched", action="store_true")
    parser.add_argument("--patch", type=Path, default=PATCH)
    args = parser.parse_args()
    verify(args.source.resolve(), not args.unpatched, args.patch)
