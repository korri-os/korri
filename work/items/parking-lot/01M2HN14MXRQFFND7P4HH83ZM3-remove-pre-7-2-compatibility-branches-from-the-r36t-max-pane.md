---
id: 01M2HN14MXRQFFND7P4HH83ZM3
slug: remove-pre-7-2-compatibility-branches-from-the-r36t-max-pane
title: Remove pre-7.2 compatibility branches from the R36T Max panel driver
origin: parked
status: To Do
priority: medium
labels:
  - r36tmax
  - cleanup
  - linux-7.2
created: 2026-09-15
source: se-code-review
---

# Remove pre-7.2 compatibility branches from the R36T Max panel driver

## Why it matters

panel-generic-dsi.c now carries version guards for kernels older than 6.13, including a hand-rolled devm_kzalloc and drm_panel_init path beside devm_drm_panel_alloc, and a conditional include of linux/hex.h. The repository forbids compatibility branches on main, and the device targets exactly one kernel. Every extra path is code nobody compiles or tests, so it rots and misleads the next reader.

## Acceptance Criteria

- [ ] panel-generic-dsi.c includes linux/hex.h and calls devm_drm_panel_alloc directly, with no LINUX_VERSION_CODE guards
- [ ] Stale 6.12 wording corrected in dts/drivers/README.md, wifi/check-module.sh and wifi/README.md, while explicitly historical evidence stays
- [ ] The kernel and image build, and the device checks pass
- [ ] The panel still lights at 720x720 on hardware

## Related

- `nix/devices/r36tmax/dts/drivers/panel-generic-dsi.c`
- `nix/devices/r36tmax/dts/drivers/README.md`
- `nix/devices/r36tmax/wifi/check-module.sh`
- `nix/devices/r36tmax/wifi/README.md`

## Notes

Deliberately not folded into the landing commit: the guards were in the exact source that produced the booted card, and removing them needs its own kernel rebuild and a panel check on hardware.
