---
id: 01M3QHRX641EZ8Q2Z22V6ZJNJ9
slug: remove-the-rp-mini-v2-right-edge-green-bar-from-iris-width-alignment
title: Remove the RP Mini V2 right-edge green bar from Iris width alignment
origin: parked
status: To Do
priority: high
labels:
  - rpminiv2
  - sunshine
  - video
  - driver
created: 2026-09-29
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: main
  repo: korri
  invoked_by: user
---

# Remove the RP Mini V2 right-edge green bar from Iris width alignment

## Why it matters

A Moonlight stream from the Mini at 1240x1080 shows a dark-green bar on the
right edge. The signed SPS-crop candidate removed the green rows at the bottom.
It cannot remove the right bar, because that bar is inside the encoded picture.

## Verified cause

- The Iris driver rounds the raw input width up to a multiple of 128.
  `iris_venc_s_fmt_input()` in Linux 7.2 `iris_venc.c` makes 1240 into 1280.
- FFmpeg's V4L2 buffer copy writes only the 1240 real columns. The other 40
  columns stay zero. Zero-filled video decodes as dark green.
- The driver then sets its input crop to the full 1280 columns and treats the
  1240-wide output as a scale request. The encoder squeezes all 1280 columns,
  including the 40 empty ones, into the picture.
- Device check on 2026-09-29: the existing probe
  `/nix/store/lirybnlv9r1fh3vyds1n00wi3ibqichw-sunshine-v4l2m2m-probe-1`
  encoded a full 1240x1080 pattern. The decoded 1248x1088 frame is blank from
  column 1208 to 1247. The owner's screenshot shows 48 green columns at
  2448 px, about 32 stream pixels after the 8-column SPS crop.
- 1920x1080 has no right bar, because 1920 is a multiple of 128.

Inferred, not measured: 1240x1080 content is also squeezed horizontally by
about 2.5 percent.

## Candidate fix (unverified)

After FFmpeg sets the raw format, it sets `V4L2_SEL_TGT_CROP` on the OUTPUT
queue to the visible size. `iris_venc_s_selection()` accepts that crop and
updates the encoded size. Build the change as an FFmpeg patch in the approved
`v4l2m2mPatches` set, off-device.

## Acceptance Criteria

- [ ] An off-device build of the approved FFmpeg patch set sets the visible
      crop for `h264_v4l2m2m` and `hevc_v4l2m2m`.
- [ ] On the Mini, a 1240x1080 probe stream has no blank columns and is not
      squeezed. Check a pattern with known column positions.
- [ ] 1280x720 and 1920x1080 probe output is unchanged.
- [ ] A Moonlight stream at 1240x1080 shows no green on the right or bottom edge.
- [ ] The owner confirms the picture on the device.

## Current device state (2026-09-29, not persistent)

- Signed crop candidate
  `/nix/store/2yyq3ibyrs37yszk9j2bj5hkk1blpfsm-korri-plugin` is selected. The
  upright package `/nix/store/frc9rcm8iw3zlcbhc2dlz9709x30sv4m-korri-plugin`
  is its direct rollback.
- A patched Iris module is loaded in RAM only. The installed Iris module
  rejects `V4L2_CID_MPEG_VIDEO_PREPEND_SPSPPS_TO_IDR` with `EINVAL`, so
  Sunshine cannot open H.264 without it. A reboot restores the installed module.
- `/run/systemd/system/korri-sunshine.service.d/90-audio-runtime.conf` is also
  runtime-only. A reboot removes it.

## Related

- `services/sunshine/rotation-probe.md`
- `services/sunshine/rotation-crop.patch`
- `services/sunshine/patches/ffmpeg/0001-fix-v4l2m2m-buffer-alignment.patch`
- Uncommitted Iris repeat-header patch:
  `.worktree/rpmini-sunshine-encoder/nix/devices/rpminiv2/kernel/patches/9999-media-qcom-iris-gen1-repeat-headers.patch`

## Notes

Workaround to test first: set Moonlight to 1280x1080. The driver needs no
padding at that width. Expect black side bars instead of green. Not yet tested.

The repeat-header module and audio setting need their own persistent fix before
this work survives a reboot.
