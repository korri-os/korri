---
id: 01M2JSCCEH1GKDNBZ876CSF5SQ
slug: fix-the-rk915-firmware-download-that-hangs-on-module-reload
title: Fix the RK915 firmware download that hangs on module reload
origin: parked
status: To Do
priority: high
labels:
  - r36tmax
  - wifi
  - linux-7.2
  - shutdown
created: 2026-09-15
source: se-debug
---

# Fix the RK915 firmware download that hangs on module reload

## Why it matters

Re-loading rk915 after an unload blocks forever inside firmware download, and it holds a lock the netdev layer needs: on the wedged device every ip, nmcli and iw command hangs while the shell still responds. This is the same signature as the shutdown that never completed, where iptables, dbus and systemd-logind each survived SIGKILL, so fixing it very likely fixes remote reboot too. Until then the driver cannot be iterated in place — every change costs a full power cycle — and any shutdown that unloads the module can wedge the handheld until someone physically holds the power button.

## Acceptance Criteria

- [ ] modprobe -r rk915 followed by modprobe rk915 completes, downloads firmware and brings wlan0 back
- [ ] ip, nmcli and iw keep responding throughout a reload
- [ ] A reboot completes with the module loaded and associated
- [ ] The lock held during firmware download is named, with the blocking call identified from a stack trace

## Related

- `nix/devices/r36tmax/wifi/probe-cleanup.patch`
- `nix/devices/r36tmax/wifi/linux-7.2.patch`
- `nix/devices/r36tmax/KERNEL-7.2.md`

## Notes

Evidence: the kernel log stops at "rk915_download: start download firmware size: 48056" and never reaches "download firmware success", which a cold boot does reach with the identical blob. Uptime kept advancing, so the box was live but stuck. Next step is echo w > /proc/sysrq-trigger, or cat /proc/<insmod pid>/stack, to name the blocking call before changing code. Suspect the chip is left powered in a state the reload never resets: check whether fw_tear_down runs on unload and whether the reset GPIO sequence is reapplied. Do not test with in-place reload until fixed; use reboot cycles.
