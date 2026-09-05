---
id: 01M1PEXZWJE7KM2HB6NV9TRBVD
slug: enable-rkvdec-for-hevc-and-vp9-decode-on-the-rg353m
title: Enable rkvdec for HEVC and VP9 decode on the RG353M
origin: parked
status: To Do
priority: medium
labels:
  - rg353m
  - kernel
  - media
  - streaming
created: 2026-09-04
source: se-work
---

# Enable rkvdec for HEVC and VP9 decode on the RG353M

## Why it matters

The RG353M's Hantro block only accepts H.264, MPEG-2, and VP8. The RK3566 has a separate rkvdec block that decodes HEVC and VP9, but CONFIG_VIDEO_ROCKCHIP_RKVDEC is absent from the kernel config, so it is never built or probed. For a Korri streaming client this caps quality per bit: H.264 needs materially higher bitrate than HEVC for the same picture over WiFi. This is the single change that raises the device's media ceiling, and it is invisible until someone measures stream quality on a constrained link.

## Acceptance Criteria

- [ ] CONFIG_VIDEO_ROCKCHIP_RKVDEC is enabled in the RG353M kernel config
- [ ] rkvdec probes on boot and registers a /dev/video* node
- [ ] v4l2-ctl --list-formats-out on the rkvdec node reports HEVC and VP9
- [ ] A decode of an HEVC sample through the node succeeds and is verified on device
- [ ] The change does not regress the existing Hantro H.264 path

## Related

- `nix/rg353m/sd-image.nix`
- `nix/rg353m/gpu.nix`

## Notes

Blocked on builder disk. A kernel config change forces a full rebuild needing roughly 30 GB of scratch; fuji sits at 89 percent on a 200 GB root and failed twice at this. Note the out-of-tree module trick used for the ST7703 panel does not apply here, because rkvdec must be built as part of the kernel config rather than patched into one existing driver. Free disk on fuji or find another aarch64 builder first.
