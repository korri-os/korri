# H700 hardware video encoder for Sunshine on a mainline kernel

Date: 2026-10-01. Scope: Allwinner H700 (Anbernic RG35XX Pro/SP), ROCKNIX mainline kernel 7.x.

## Answer

No. No hardware video encoder that Sunshine can use exists for the H700 on a mainline kernel today.
The H616/H700 Video Engine (VE) has an H.264 encoder in silicon (4K@25fps per the datasheet).
Mainline `cedrus` is decode-only, and H616 decode support was only posted in 2026 (v3, July 2026); its merge status is unverified.
Bootlin's cedrus H.264 encoder patches cover only V3/V3s/S3, live out of tree, and need a stateless-encode uAPI that is still under review. Even if that uAPI landed, FFmpeg's `h264_v4l2m2m` (the path Sunshine's V4L2 work uses) drives only stateful encoders, so it would not drive a stateless cedrus encoder.
The only realistic path on mainline is software libx264 (Sunshine's built-in `software` encoder). Its speed at 640x480@60 on this CPU is unmeasured in any primary source I found.

## Options

| Option | Kernel requirement | Codecs (encode) | Upstream status | Works on H700 mainline? | Source |
|---|---|---|---|---|---|
| Mainline `cedrus` | Mainline staging media | None (decode only) | Merged since 4.20. H616 variant posted as v3 in Jul 2026; merge unverified | No encode. Decode only if the H616 series is applied | [Bootlin 2018](https://bootlin.com/blog/author/paul/), [H616 v3 series](https://ratatoskr.run/lkml/2026/07/17247254/t) |
| Bootlin cedrus H.264 encoder | Bootlin `h264-encoding` branch on top of mainline cedrus | H.264 | Out of tree. Not merged. Needs new uAPI | No. V3/V3s/S3 only. An H616 user asked about it in Jan 2024; the question has no answer (issue #1) | [Bootlin blog](https://bootlin.com/blog/open-source-linux-kernel-support-for-the-allwinner-v3-v3s-s3-h-264-video-encoder/), [issue #1](https://github.com/bootlin/v4l2-cedrus-enc-test/issues/1) |
| V4L2 H.264 stateless encode uAPI (Kocialkowski, 2026) | Mainline + series | H.264 | Posted May 2026 as `[PATCH 00/14]`. Targets Verisilicon/Hantro (i.MX8MP VC8000E). Merge unverified | No. No cedrus driver in the series | [series](https://ratatoskr.run/lkml/2026/05/17018750/t) |
| Allwinner BSP `cedar_ve` + libcedarc/libvencoder | Allwinner BSP kernel (4.9 on stock H700 firmware, unverified) | H.264 (BSP) | Vendor only | No (BSP kernel only) | unverified for H700 |
| `aodzip/cedar` + `aodzip/libcedarc` | Out-of-tree module on mainline | H.264 via libcedarc | Community, not upstream | Unverified. README shows a V3 DT example only | [cedar](https://github.com/aodzip/cedar), [libcedarc](https://github.com/aodzip/libcedarc) |
| `uboborov/sunxi-cedar-mainline` | Out-of-tree module, "linux-4.11.y and higher" | Exposes `/dev/cedar_dev` only | Community | Unverified, no H616 claim | [repo](https://github.com/uboborov/sunxi-cedar-mainline) |
| `jemk/cedrus` h264enc | Needs a VE device node (old `/dev/cedar_dev`) | H.264 (PoC) | Research code, "not recommended for production use" | No H616 claim. Unverified | [h264enc](https://github.com/jemk/cedrus/tree/master/h264enc) |
| FFmpeg `cedrus264` (uboborov, FFmpeg-Cedrus, patchwork 9094) | `/dev/cedar_dev` | H.264 | Never merged in FFmpeg | No. Targets H3 | [ffmpeg_h264_H3](https://github.com/uboborov/ffmpeg_h264_H3), [patchwork](https://patchwork.ffmpeg.org/patch/9094) |
| Mali-G31 (Panfrost/PanVK) | Mainline | None | n/a | No. A GPU has no video codec block | [Arm Mali-G31](https://www.arm.com/products/silicon-ip-multimedia/gpu/mali-g31) |
| Sunshine `software` (libx264) | None | H.264 (also HEVC/AV1 via FFmpeg sw where built) | Upstream Sunshine | Yes, functionally. Speed unmeasured | [Sunshine config docs](https://docs.lizardbyte.dev/projects/sunshine/master/md_docs_2configuration.html) |

## Detail

### 1. What the H616/H700 VE can encode

- The H616 datasheet lists a "high-performance H.264 video encoder". [H616 Datasheet V1.0](https://linux-sunxi.org/images/b/b9/H616_Datasheet_V1.0_cleaned.pdf)
- Encode specs in the datasheet and user manual: "H.264 encoding capability: 4K@25fps", "JPEG snapshot performance of 1080p@60fps", CBR/VBR from 256 kbit/s to 100 Mbit/s, eight ROIs. [Datasheet](https://linux-sunxi.org/images/b/b9/H616_Datasheet_V1.0_cleaned.pdf), [User Manual (mirror)](https://www.scs.stanford.edu/~zyedidia/docs/allwinner/h616.pdf)
- No HEVC encode is listed in the snippets I saw. Third-party claims of "1080p@30 H.265 encode" are not primary and are unverified.
- The H700 is a package variant of the H616 die. linux-sunxi says it is "yet another package variant, exposing the RGB LCD pins". [linux-sunxi H616](https://linux-sunxi.org/H616)
- I found no public H700 datasheet. That the H700 VE matches the H616 VE is inferred from the shared die.

### 2. Mainline `cedrus`

- Cedrus is a V4L2 M2M **decoder** driver. It has been in mainline staging since 4.20. [Bootlin](https://bootlin.com/blog/author/paul/), [linux-sunxi Sunxi-Cedrus](https://linux-sunxi.org/Sunxi-Cedrus)
- Bootlin wrote: "We don't yet have H616/H618 support for cedrus in mainline". [Bootlin wrap-up](https://bootlin.com/blog/wrapping-up-the-allwinner-vpu-crowdfunded-linux-driver-work/)
- Chen-Yu Tsai posted "arm64: allwinner: h616: Support video engine": v1 on 2026-05-05, v2 and v3 in July 2026 (v3 dated 2026-07-12). It adds the `allwinner,sun50i-h616-video-engine` compatible, a cedrus H616 variant (4 lines in `cedrus.c`), and the H616 DT node (by Jernej Skrabec). v2 says "The series is ready to be merged." [v1](https://ratatoskr.run/linux-arm-kernel/2026/05/3545331/t), [v2](https://ratatoskr.run/linux-sunxi/2026/07/17246256/t), [v3](https://ratatoskr.run/lkml/2026/07/17247254/t) (ratatoskr.run mirrors the lore lists.)
- Armbian carries its own patch `drv-staging-media-sunxi-cedrus-add-H616-variant.patch` for sunxi-6.18. [armbian/build #10704](https://github.com/armbian/build/pull/10704)
- The series is decode only. Nothing in it adds encoding.
- I did not check git.kernel.org directly. Whether the series is in 7.2 or 7.3 is unverified.

### 3. Bootlin stateless H.264 encoder work

- Nov 2023: Bootlin released H.264 encode support for Allwinner V3/V3s/S3 as "a series of patches on top of the mainline Linux cedrus driver" plus the test tool `v4l2-cedrus-enc-test`. They said that a new uAPI is needed before mainline inclusion. [Bootlin blog](https://bootlin.com/blog/open-source-linux-kernel-support-for-the-allwinner-v3-v3s-s3-h-264-video-encoder/), [v4l2-cedrus-enc-test](https://github.com/bootlin/v4l2-cedrus-enc-test)
- FOSDEM 2024 slides (Kocialkowski) list the "Mainline-based attempt: V3/V3s/S3 H.264 encoding (Bootlin)" with kernel-side rate control and bitstream generation. [FOSDEM 2024 slides](https://archive.fosdem.org/2024/events/attachments/fosdem-2024-3090-v4l2-stateless-video-encoding-hardware-support-and-uapi/slides/22199/v4l2-stateless-video-encoding_rGq1vIt.pdf)
- Parallel RFC (Collabora, Andrzej Pietrasiewicz, Nov 2023): H.264 stateless encode uAPI for Hantro H1. [Patchew](https://patchew.org/linux/20231116154816.70959-1-andrzej.p@collabora.com/), [LWN](https://lwn.net/Articles/951733/)
- May 2026: "[PATCH 00/14] media: Add V4L2 H.264 stateless..." by Paul Kocialkowski (12 patches) and Marco Felsch (2). It adds `media: uapi: Add H.264 stateless encode support`, a shared H.264 encode core, rate control, and Verisilicon (i.MX8MP VC8000E) support. No cedrus patch is listed. [series](https://ratatoskr.run/lkml/2026/05/17018750/t)
- An H616/H618 user asked on 2024-01-21 whether the encoder runs on those SoCs. The tool printed "Failed to open encoder media device", which means no encoder device existed. The issue is open with no reply, so it does not prove the hardware fails; it shows no H616 port exists. [issue #1](https://github.com/bootlin/v4l2-cedrus-enc-test/issues/1)
- Conclusion: H616/H700 is not covered. The uAPI merge status is unverified.

### 4. Out-of-tree options

- `aodzip/cedar`: "Allwinner CedarX Compatible Mainline Kernel Driver (Not Official)". Its README DT example targets `allwinner,sun8i-v3-cedar`. Pairs with `aodzip/libcedarc`. No H616 entry seen. [cedar](https://github.com/aodzip/cedar), [libcedarc](https://github.com/aodzip/libcedarc)
- `uboborov/sunxi-cedar-mainline`: creates `/dev/cedar_dev`, targets "linux-4.11.y and higher". [repo](https://github.com/uboborov/sunxi-cedar-mainline)
- `jemk/cedrus/h264enc`: "basic example for using the H264 hardware encoder for sunxi SoCs ... proof of concept and not recommended for production use". [h264enc](https://github.com/jemk/cedrus/tree/master/h264enc). A fork lists A31s, A80, A33, H3, H8. [sailfish009/h264_encoder_H3](https://github.com/sailfish009/h264_encoder_H3)
- FFmpeg `cedrus264`: posted for H2/H3, not merged. [patchwork 9094](https://patchwork.ffmpeg.org/patch/9094). Forks target H3: [uboborov/ffmpeg_h264_H3](https://github.com/uboborov/ffmpeg_h264_H3), [agustinov/FFmpeg-Cedrus](https://github.com/agustinov/FFmpeg-Cedrus).
- I found no primary source that shows any of these encoding on H616/H618/H700 with a mainline kernel. H616 users on the Armbian forum report that encode does not work. That is not primary. [Armbian forum](https://forum.armbian.com/topic/37759-state-of-the-things-related-to-video-decoding-and-encoding-in-opi-zero-23/)
- None of these would plug into Sunshine without new FFmpeg or Sunshine code.

### 5. H700 distros

- ROCKNIX moved H700 to kernel 7.2 in release 20260901. [ROCKNIX 20260901](https://github.com/ROCKNIX/distribution/releases/tag/20260901), [commit 0c77a91](https://github.com/ROCKNIX/distribution/commit/0c77a91dcabab25de800e77359bfa1a4ae09163e)
- I found no ROCKNIX H700 cedrus or encoder patch. Unverified.
- Knulli, muOS, and stock Anbernic firmware: I found no primary source about what they ship for encode. That they use the Allwinner BSP 4.9 kernel with `cedar_ve` is unverified.
- I found no distro or project that runs Sunshine (or another stream host) with hardware encode on H700. Community use is the reverse direction: Moonlight client on the device. [r/RG35XX_Plus guide](https://www.reddit.com/r/RG35XX_Plus/comments/1c5m2mh/guide_to_moonlight_on_35xxh_batocera/) (not primary)

### 6. Sunshine encoder backends

- Sunshine docs list encoder sections for NVENC, Intel QuickSync, AMD AMF, VideoToolbox, VA-API, and software (`sw_preset`, `sw_tune`). [Sunshine configuration](https://docs.lizardbyte.dev/projects/sunshine/master/md_docs_2configuration.html)
- Vulkan Video encode arrived in Sunshine v2026.413.143228. [Phoronix](https://www.phoronix.com/news/Sunshine-v2026.413.143228) (secondary)
- Sunshine falls back to libx264 when no hardware encoder is found ("Found H.264 encoder: libx264 [software]"). [Sunshine #2190](https://github.com/LizardByte/Sunshine/issues/2190)
- Upstream V4L2 M2M work is open as PR "feat(linux): add zero-copy v4l2 encoder" (#5717). It depends on FFmpeg patches in build-deps #781, which describe "V4L2 M2M hardware encoder support (H.264/HEVC/AV1)". Merge status unverified. [Sunshine #5717](https://github.com/LizardByte/Sunshine/pull/5717), [build-deps #781](https://github.com/LizardByte/build-deps/pull/781)
- FFmpeg `h264_v4l2m2m` is a wrapper for **stateful** V4L2 encoders. A stateless encoder (Bootlin cedrus encoder, Hantro) needs userspace to build headers and drive per-frame controls. FFmpeg has no stateless V4L2 encoder today (inferred; I did not read FFmpeg source in this pass).
- So a cedrus stateless encoder could not plug into Sunshine's V4L2 path as-is. It would need a new FFmpeg encoder or a new Sunshine backend. A stateful M2M encoder would plug in; none exists for this VE.
- Neither Mali-G31 driver has video encode, so the Vulkan encode path does not help (point 8).

### 7. Software x264 at 640x480@60

- I found no primary-source measurement of libx264 at 640x480 on a quad Cortex-A53 at about 1.5 GHz.
- OpenBenchmarking hosts the x264 test profile but I found no H616/H700 or A53 result for it. [pts/x264](https://openbenchmarking.org/test/pts/x264-1.7.0)
- Opinion: 640x480 is about 0.3 MP/frame, about 1/7 of 1080p. With `-preset ultrafast -tune zerolatency` on 4 A53 cores, 60 fps is plausible but not proven. The emulator competes for the same cores. Measure on the device before you rely on it.
- Measurement to run on the device (prebuilt ffmpeg, no compile):
  `ffmpeg -f lavfi -i testsrc2=size=640x480:rate=60 -t 20 -c:v libx264 -preset ultrafast -tune zerolatency -f null -`
  Repeat at 480x320 / 30 fps, and with the emulator running.

### 8. Mali-G31

- Arm sells video codecs as separate Mali-V IP (Mali-V52 launched with Mali-G31). The G31 is a GPU only. [Arm Mali-G31](https://www.arm.com/products/silicon-ip-multimedia/gpu/mali-g31), [CdrInfo](https://www.cdrinfo.com/d7/content/arm-announces-new-mali-g52-and-mali-g31-gpus) (secondary)
- No Mali-V block is documented for the H616; the VE is Allwinner's own. [H616 datasheet](https://linux-sunxi.org/images/b/b9/H616_Datasheet_V1.0_cleaned.pdf)
- Confirmed as expected: no hardware video encode from the GPU. That PanVK exposes no `VK_KHR_video_encode_*` is inferred, not checked in Mesa source.

## Unverified / open questions

- Is the H616 cedrus series (v3, 2026-07-12) in mainline 7.2 or 7.3? Check `grep h616 drivers/staging/media/sunxi/cedrus/cedrus.c` in the ROCKNIX 7.2 tree.
- Is the May 2026 H.264 stateless encode uAPI merged? Check `include/uapi/linux/v4l2-controls.h` in 7.x.
- Does the H616 VE encoder share the register layout of V3/H3 (so Bootlin's cedrus encoder could be ported)? No source found.
- Does stock Anbernic / Knulli / muOS ship `cedar_ve` and libvencoder? Could be checked on the stock image (`ls /dev/cedar_dev`, `lsmod`).
- Is Sunshine PR #5717 merged, and does it need a stateful encoder? I did not read the diff.
- libx264 fps at 640x480@60 on H700: unmeasured.
- Lore URLs: I cited ratatoskr.run mirrors. I did not resolve lore.kernel.org message-ids.
- Korri side note (medium importance): the device log "Unable to find display or encoder during startup" may also come from capture (KMS/wlroots) failure, not only the encoder. Enabling the `software` encoder in the Korri Sunshine build is the first test.
