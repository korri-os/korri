#!/usr/bin/env python3
"""Off-device source/packet checks; NOT a firmware or encoded-frame test.

Usage: python3 iris-gen1-encoder.test.py /path/to/pristine/linux-7.2
Needs patch, cc and Linux userspace headers. Copies only Iris to a temporary
folder, applies the existing keyframe patch plus the gen1 header patch, then
compiles the actual setters and complete gen1 property packetizer with a mock
session transport. The supplied kernel tree is never changed.
"""

import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
IRIS = pathlib.Path("drivers/media/platform/qcom/iris")
PATCHES = [
    HERE / "product-patches/9999-media-qcom-iris-add-request-key-frame-support.patch",
    HERE / "product-patches/9999-media-qcom-iris-gen1-repeat-headers.patch",
]


def function(source, name):
    match = re.search(r"(?:static (?:int|u32)\s+|int )" + name + r"\([^;{]+\{.*?\n\}", source, re.S)
    assert match, name
    return match[0]


def enum(source, name):
    match = re.search(r"enum " + name + r" \{.*?\n\};", source, re.S)
    assert match, name
    return match[0]


def run(source):
    with tempfile.TemporaryDirectory(prefix="iris-gen1-test-") as temporary:
        root = pathlib.Path(temporary)
        shutil.copytree(source / IRIS, root / IRIS)
        for patch in PATCHES:
            subprocess.run(["patch", "--batch", "--fuzz=0", "-p1", "-i", str(patch)], cwd=root, check=True)
        iris = root / IRIS
        ctrls = (iris / "iris_ctrls.c").read_text()
        caps = (iris / "iris_hfi_gen1.c").read_text()
        platform = (iris / "iris_platform_common.h").read_text()
        common = (iris / "iris_hfi_common.h").read_text()
        command = (iris / "iris_hfi_gen1_command.c").read_text()
        response = (iris / "iris_hfi_gen1_response.c").read_text()
        prepend = re.search(r"\{\s*\.cap_id = PREPEND_SPSPPS_TO_IDR,.*?\n\t\}", caps, re.S)[0]
        # Same static companion-control shape as gen2; no second setter can
        # overwrite HEADER_MODE's combined property during stream setup.
        for field, value in [("min", 0), ("max", 1), ("step_or_mask", 1), ("value", 0)]:
            assert f".{field} = {value}," in prepend
        assert ".set" not in prepend and ".flags" not in prepend
        assert "pic_type = com_pkt->picture_type;" in response
        picture_switch = re.search(r"\tswitch \(pic_type\) \{.*?\n\t\}", response, re.S)[0]
        harness = r'''
#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <linux/videodev2.h>
typedef uint8_t u8;
typedef uint32_t u32;
typedef int32_t s32;
#define BIT(n) (1U << (n))
#include "iris_hfi_gen1_defines.h"
#include "iris_hfi_gen2_defines.h"
'''
        for name in ["platform_inst_fw_cap_type", "platform_inst_fw_cap_flags"]:
            harness += enum(platform, name) + "\n"
        for name in ["hfi_packet_port_type", "hfi_packet_payload_info", "hfi_packet_host_flags"]:
            harness += enum(common, name) + "\n"
        harness += r'''
/* Only the session transport and containing instance are mocked. */
enum { DECODER, ENCODER };
struct iris_inst;
struct iris_hfi_session_ops {
    int (*session_set_property)(struct iris_inst *, u32, u32, u32, u32, void *, u32);
};
struct iris_inst {
    const struct iris_hfi_session_ops *hfi_session_ops;
    struct { u32 value, hfi_id, flags; } fw_caps[INST_FW_CAP_MAX];
    u32 session_id, domain;
};
'''
        harness += function(command, "iris_hfi_gen1_packet_session_set_property") + "\n"
        harness += function(ctrls, "iris_get_port_info") + "\n"
        harness += function(ctrls, "iris_set_header_mode_gen1") + "\n"
        harness += function(ctrls, "iris_set_req_sync_frame") + "\n"
        harness += "static u32 picture_flags(u32 pic_type) { u32 flags = 0;\n"
        harness += picture_switch + "\nreturn flags; }\n"
        harness += r'''
static u32 storage[128], calls;
static int transport_error;
static int transport(struct iris_inst *inst, u32 property, u32 flag,
                     u32 port, u32 type, void *payload, u32 size)
{
    assert(flag == HFI_HOST_FLAGS_NONE && size == sizeof(u32));
    if (property == HFI_PROPERTY_CONFIG_VENC_SYNC_FRAME_SEQUENCE_HEADER) {
        assert(port == HFI_PORT_BITSTREAM && type == HFI_PAYLOAD_U32);
    } else {
        assert(property == HFI_PROPERTY_CONFIG_VENC_REQUEST_SYNC_FRAME);
        assert(port == HFI_PORT_RAW && type == HFI_PAYLOAD_U32_ENUM);
    }
    calls++;
    memset(storage, 0xa5, sizeof(storage));
    int ret = iris_hfi_gen1_packet_session_set_property(
        (struct hfi_session_set_property_pkt *)storage, inst, property, payload);
    return ret ? ret : transport_error;
}
static void check_packet(u32 property, u32 size)
{
    struct hfi_session_set_property_pkt *packet = (void *)storage;
    assert(packet->shdr.hdr.size == size);
    assert(packet->shdr.hdr.pkt_type == HFI_CMD_SESSION_SET_PROPERTY);
    assert(packet->shdr.session_id == 42);
    assert(packet->num_properties == 1 && packet->data[0] == property);
}
int main(void)
{
    const struct iris_hfi_session_ops ops = { .session_set_property = transport };
    struct iris_inst inst = { .hfi_session_ops = &ops, .domain = ENCODER, .session_id = 42 };
    inst.fw_caps[HEADER_MODE].hfi_id = HFI_PROPERTY_CONFIG_VENC_SYNC_FRAME_SEQUENCE_HEADER;
    inst.fw_caps[HEADER_MODE].flags = CAP_FLAG_OUTPUT_PORT;
    inst.fw_caps[REQUEST_SYNC_FRAME].hfi_id = HFI_PROPERTY_CONFIG_VENC_REQUEST_SYNC_FRAME;
    inst.fw_caps[REQUEST_SYNC_FRAME].flags = CAP_FLAG_INPUT_PORT;
    const u32 modes[] = { V4L2_MPEG_VIDEO_HEADER_MODE_SEPARATE,
                        V4L2_MPEG_VIDEO_HEADER_MODE_JOINED_WITH_1ST_FRAME };
    for (u32 m = 0; m < 2; m++) {
        for (u32 prepend = 0; prepend < 2; prepend++) {
            inst.fw_caps[HEADER_MODE].value = modes[m];
            inst.fw_caps[PREPEND_SPSPPS_TO_IDR].value = prepend;
            assert(iris_set_header_mode_gen1(&inst, HEADER_MODE) == 0);
            check_packet(0x2006008, 24);
            assert(storage[5] == (m || prepend));
            /* Repeated button presses must each emit a payload-free request,
             * irrespective of the gen2 enum produced by the common setter. */
            for (u32 press = 0; press < 2; press++) {
                assert(iris_set_req_sync_frame(&inst, REQUEST_SYNC_FRAME) == 0);
                check_packet(0x2006004, 20);
                assert(storage[5] == 0xa5a5a5a5);
            }
        }
    }
    assert(calls == 12);
    transport_error = -EIO;
    assert(iris_set_header_mode_gen1(&inst, HEADER_MODE) == -EIO);
    assert(iris_set_req_sync_frame(&inst, REQUEST_SYNC_FRAME) == -EIO);
    assert(picture_flags(HFI_GEN1_PICTURE_IDR) == V4L2_BUF_FLAG_KEYFRAME);
    assert(picture_flags(HFI_GEN1_PICTURE_I) == V4L2_BUF_FLAG_KEYFRAME);
    assert(picture_flags(HFI_GEN1_PICTURE_P) == V4L2_BUF_FLAG_PFRAME);
    assert(picture_flags(HFI_GEN1_PICTURE_B) == V4L2_BUF_FLAG_BFRAME);
    assert(picture_flags(HFI_FRAME_NOTCODED) == 0);
    assert(picture_flags(0xffffffff) == 0);
    puts("PASS: 4 header combinations, 8 payload-free keyframe requests, 2 setter errors, 6 picture flags");
}
'''
        (root / "check.c").write_text(harness)
        subprocess.run(["cc", "-std=gnu11", "-Wall", "-Wextra", "-Werror", "-I", str(iris),
                        str(root / "check.c"), "-o", str(root / "check")], check=True)
        subprocess.run([str(root / "check")], check=True)
        print("PASS: patch stack applies without fuzz; static companion control registered")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    run(pathlib.Path(sys.argv[1]).resolve())
