---
id: 01M277MJX5DGM0Z40EAGCV76GP
slug: investigate-rg353m-resume-diagnostics-against-the-full-firmw
title: Investigate RG353M resume diagnostics against the full-firmware baseline
origin: parked
status: To Do
priority: medium
labels:
  - rg353m
  - suspend
  - boot-validation
created: 2026-09-11
source: user
---

# Investigate RG353M resume diagnostics against the full-firmware baseline

## Why it matters

The 30-second RTC suspend test returned after about 31 seconds with the same boot ID, live Ethernet and running Korri services. However xHCI logged a resume error and reinitialization, and uptime reported 630 days while /proc/uptime showed roughly 40 minutes. These observations need explanation before calling suspend/resume fully validated; neither is established as a firmware-trim regression.

## Acceptance Criteria

- [ ] Compare a bounded suspend/resume cycle on the phase-1 candidate and the full-firmware recovery baseline.
- [ ] Explain or fix the xHCI resume/reinitialization message and verify required USB accessories recover reliably.
- [x] Explain the disagreement between uptime and /proc/uptime without assuming an actual clock jump.
- [ ] Verify display, controls, audio, storage, and network after repeated bounded wake cycles.

## Related

- `nix/devices/rg353m/sd-image.nix`
- `nix/devices/rg353m/FIRMWARE.md`

## Notes

Kernel log: xhci-hcd xhci-hcd.11.auto: xHC error in resume, USBSTS 0x401, Reinit. systemd-suspend completed successfully. Wi-Fi scans found six networks before suspend and seven afterward. No additional failed system units appeared.

Follow-up: uptime resolves to coreutils-full 9.8. `who -b` reports the UTMP boot record as 2024-12-19 21:27, and systemd-update-utmp.service logged its start/finish under that early date. The kernel reports btime=1789094280 (2026-09-11), with normal elapsed /proc/uptime and a matching RTC/current wall clock. The misleading 630-day report comes from the stale boot record created before clock initialization, not evidence of a 630-day suspend or kernel uptime jump. The xHCI/accessory checks remain open.
