---
id: 01M1YKQ8V3XN7BR2WGDT4HZC5P
slug: pin-the-rg353m-compositor-to-a-stable-kms-device
title: Pin the RG353M compositor to a stable KMS device
origin: parked
status: To Do
priority: high
labels:
  - rg353m
  - compositor
  - boot
created: 2026-09-07
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  repo: korri
---

# Pin the RG353M compositor to a stable KMS device

## Why it matters

`nix/rg353m/sunshine-host.nix` sets `compositor.drmDevice = "/dev/dri/card0"`,
which becomes `WLR_DRM_DEVICES`. The RG353M exposes two DRM cards and the minor
numbers are assigned in probe order, not by role:

- `platform-display-subsystem` — `rockchipdrm`, the display controller, has KMS
- `platform-fde60000.gpu` — `panfrost`, render-only, has no KMS

When `panfrost` probes first it takes `card0`, sway opens a card with no KMS,
and the compositor dies with `Found 0 GPUs, cannot create backend`. It then
restarts forever, so the kiosk, korrid, and Sunshine never start and the device
shows nothing.

Observed on 2026-09-07: three consecutive boots all gave `card0` to panfrost
(`[drm] Initialized panfrost 1.4.0 for fde60000.gpu on minor 0` at 1.99 s,
`rockchipdrm` after it). The compositor reached restart counter 12 and the
device was unusable until `WLR_DRM_DEVICES` was overridden by hand. This is not
the same defect as `01M1WPXCE2QAG0VR4KGKF460RH`, which is a `renderD128`
permission race that costs time but recovers.

The device is currently running with a `/run` drop-in that sets
`WLR_DRM_DEVICES=/dev/dri/by-path/platform-display-subsystem-card`. That drop-in
does not survive a reboot, so the next reboot can strand the device again.

## Acceptance Criteria

- `compositor.drmDevice` for the RG353M names a probe-order-independent path.
  `/dev/dri/by-path/platform-display-subsystem-card` already exists on the
  device and points at the display subsystem.
- The `services.korriLinuxHost` assertion that `drmDevice` starts with
  `/dev/dri/` still holds, or is widened deliberately to accept `by-path`.
- The compositor starts on a boot where panfrost wins `card0`. Prove it by
  forcing that ordering or by booting repeatedly until it occurs.
- A module check covers the chosen device string so the literal `card0` cannot
  come back unnoticed.

## Notes

The same fragility applies to any other Korri device with a split render/display
DRM pair. Consider resolving the KMS card by capability inside the compositor
module rather than naming a path per device.
