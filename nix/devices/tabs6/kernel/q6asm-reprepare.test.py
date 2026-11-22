#!/usr/bin/env python3
"""Compile the shipped q6asm prepare/map/ack functions and check PCM ring reuse.

Usage: python3 q6asm-reprepare.test.py /path/to/linux-source
The transport is in memory. This checks ring accounting, not ADSP firmware.
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path


def function(text: str, name: str) -> str:
    match = re.search(r"(?:static )?int " + name + r"\(.*?\n\}", text, re.DOTALL)
    if match is None:
        raise ValueError(f"Missing driver function: {name}")
    return match[0]


def verify(source: Path) -> None:
    qdsp6 = source / "sound/soc/qcom/qdsp6"
    dai = (qdsp6 / "q6asm-dai.c").read_text()
    asm = (qdsp6 / "q6asm.c").read_text()
    harness = r"""
#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
typedef unsigned long phys_addr_t;
typedef unsigned long snd_pcm_uframes_t;
#define GFP_ATOMIC 0
#define kzalloc_objs(object, count, flags) calloc(count, sizeof(object))
#define spin_lock_irqsave(lock, flags) ((void)(lock), (flags) = 0)
#define spin_unlock_irqrestore(lock, flags) ((void)(lock), (void)(flags))
#define atomic_set(pointer, value) (*(pointer) = (value))
#define dev_err(dev, ...) ((void)(dev))
#define dev_info(dev, ...) ((void)(dev))
#define SNDRV_PCM_STREAM_PLAYBACK 0
#define SNDRV_PCM_STREAM_CAPTURE 1
#define Q6ASM_STREAM_STOPPED 1
#define Q6ASM_STREAM_RUNNING 2
#define CMD_CLOSE 1
#define FORMAT_LINEAR_PCM 0
#define LEGACY_PCM_MODE 0
struct device { int unused; };
struct audio_buffer { phys_addr_t phys; uint32_t size; };
struct audio_port_data {
    struct audio_buffer *buf;
    uint32_t num_periods, dsp_buf, mem_map_handle;
    int hw_ptr;
};
struct audio_client {
    struct audio_port_data port[2];
    struct device *dev;
    int lock;
};
struct control { snd_pcm_uframes_t appl_ptr; };
struct snd_pcm_runtime {
    void *private_data;
    struct control *control;
    unsigned period_size, periods, rate, channels;
};
struct dai_link { int id; };
struct snd_soc_pcm_runtime { struct dai_link *dai_link; };
struct snd_pcm_substream {
    struct snd_pcm_runtime *runtime;
    struct snd_soc_pcm_runtime *soc;
    unsigned stream;
};
struct q6asm_dai_data { int unused; };
struct snd_soc_component { struct device *dev; struct q6asm_dai_data *data; };
struct q6asm_dai_rtd {
    struct audio_client *audio_client;
    phys_addr_t phys;
    unsigned pcm_count, pcm_size, periods, stream_id, session_id;
    unsigned bits_per_sample, state;
    snd_pcm_uframes_t queue_ptr;
};
static unsigned submitted;
static struct snd_soc_pcm_runtime *snd_soc_substream_to_rtd(struct snd_pcm_substream *s) { return s->soc; }
static struct q6asm_dai_data *snd_soc_component_get_drvdata(struct snd_soc_component *c) { return c->data; }
static unsigned snd_pcm_lib_period_bytes(struct snd_pcm_substream *s) { return s->runtime->period_size * 4; }
/* In-memory transport: retain the mapped ring and submitted buffer index. */
static int __q6asm_memory_map_regions(struct audio_client *ac, int dir,
    size_t bytes, unsigned periods, int contiguous) {
    assert(ac->port[dir].buf && bytes == 1920 && periods == 8 && contiguous);
    return 0;
}
static void q6asm_audio_client_free_buf(struct audio_client *ac, struct audio_port_data *p) {
    (void)ac;
    free(p->buf); p->buf = NULL; p->num_periods = 0;
}
static int q6asm_unmap_memory_regions(unsigned dir, struct audio_client *ac) {
    q6asm_audio_client_free_buf(ac, &ac->port[dir]); return 0;
}
static int q6asm_write_async(struct audio_client *ac, unsigned stream,
    unsigned count, unsigned msw, unsigned lsw, unsigned flags) {
    struct audio_port_data *p = &ac->port[0];
    (void)stream; (void)msw; (void)lsw; (void)flags;
    assert(count == 1920 && p->dsp_buf < p->num_periods);
    assert(p->buf[p->dsp_buf].phys == 0x100000UL + p->dsp_buf * count);
    p->dsp_buf = (p->dsp_buf + 1) % p->num_periods;
    submitted++; return 0;
}
static int q6asm_cmd(struct audio_client *a, unsigned s, int cmd) { return 0; }
static void q6routing_stream_close(int id, unsigned dir) { }
static int q6asm_open_write(struct audio_client *a, unsigned s, int fmt, int mode, unsigned bits, int gapless) { return 0; }
static int q6asm_open_read(struct audio_client *a, unsigned s, int fmt, unsigned bits) { return 0; }
static unsigned q6asm_get_session_id(struct audio_client *a) { return 1; }
static int q6routing_stream_open(int id, int mode, unsigned session, unsigned dir) { return 0; }
static int q6asm_media_format_block_multi_ch_pcm(struct audio_client *a, unsigned s, unsigned rate, unsigned channels, void *map, unsigned bits) { return 0; }
static int q6asm_enc_cfg_blk_pcm_format_support(struct audio_client *a, unsigned s, unsigned rate, unsigned channels, unsigned bits) { return 0; }
static int q6asm_read(struct audio_client *a, unsigned s) { return 0; }
"""
    harness += function(asm, "q6asm_map_memory_regions") + "\n"
    harness += function(dai, "q6asm_dai_prepare") + "\n"
    harness += (
        function(dai, "q6asm_dai_ack")
        + r"""
int main(void) {
    struct audio_client client = {0};
    struct q6asm_dai_data data = {0};
    struct snd_soc_component component = {.data = &data};
    struct q6asm_dai_rtd rtd = {
        .audio_client = &client, .phys = 0x100000,
        .pcm_size = 15360, .periods = 8, .stream_id = 1, .bits_per_sample = 16
    };
    struct control control = {0};
    struct snd_pcm_runtime runtime = {
        .private_data = &rtd, .control = &control,
        .period_size = 480, .rate = 48000, .channels = 2
    };
    struct dai_link link = {.id = 1};
    struct snd_soc_pcm_runtime soc = {.dai_link = &link};
    struct snd_pcm_substream stream = {.runtime = &runtime, .soc = &soc};

    assert(q6asm_dai_prepare(&component, &stream) == 0);
    control.appl_ptr = 2016;
    assert(q6asm_dai_ack(&component, &stream) == 0);
    assert(submitted == 4 && rtd.queue_ptr == 1920);
    assert(client.port[0].dsp_buf == 4);
    client.port[0].hw_ptr = 1;

    /* ALSA resets appl_ptr after drop/prepare; all three cursors must follow. */
    control.appl_ptr = 0;
    assert(q6asm_dai_prepare(&component, &stream) == 0);
    control.appl_ptr = 2016;
    assert(q6asm_dai_ack(&component, &stream) == 0);
    /* An accepted second write must actually submit four fresh periods. */
    assert(submitted == 8 && rtd.queue_ptr == 1920);
    assert(client.port[0].dsp_buf == 4);
    assert(client.port[0].hw_ptr == 0);

    /* A further partial period must not submit a duplicate buffer. */
    control.appl_ptr = 2399;
    assert(q6asm_dai_ack(&component, &stream) == 0 && submitted == 8);
    control.appl_ptr = 2400;
    assert(q6asm_dai_ack(&component, &stream) == 0 && submitted == 9);

    /* Reset the selected direction only, including a reused capture ring. */
    client.port[1].dsp_buf = 5; client.port[1].hw_ptr = 3;
    assert(q6asm_map_memory_regions(1, &client, 0x200000, 1920, 8) == 0);
    assert(client.port[1].dsp_buf == 0 && client.port[1].hw_ptr == 0);
    assert(client.port[0].dsp_buf == 5);
    q6asm_unmap_memory_regions(0, &client);
    q6asm_unmap_memory_regions(1, &client);
    puts("PASS: fresh/reprepared PCM, partial periods, direction-local ring reset");
}
"""
    )
    with tempfile.TemporaryDirectory(prefix="tabs6-q6asm-") as directory:
        root = Path(directory)
        _ = (root / "check.c").write_text(harness)
        _ = subprocess.run(
            [
                "cc",
                "-std=gnu11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wno-unused-parameter",
                "-Wno-sign-compare",
                str(root / "check.c"),
                "-o",
                str(root / "check"),
            ],
            check=True,
        )
        _ = subprocess.run([str(root / "check")], check=True, cwd=root)


if __name__ == "__main__":
    verify(Path(sys.argv[1]))
