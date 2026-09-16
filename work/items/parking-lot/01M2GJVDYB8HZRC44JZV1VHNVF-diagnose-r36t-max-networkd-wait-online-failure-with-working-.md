---
id: 01M2GJVDYB8HZRC44JZV1VHNVF
slug: diagnose-r36t-max-networkd-wait-online-failure-with-working-
title: Diagnose R36T Max networkd wait-online failure with working Wi-Fi
origin: parked
status: To Do
priority: medium
labels:
  - r36tmax
  - networking
created: 2026-09-14
source: se-debug
---

# Diagnose R36T Max networkd wait-online failure with working Wi-Fi

## Why it matters

The first diagnostic boot and temporary portal activation both report systemd-networkd-wait-online.service failed even though NetworkManager Wi-Fi and SSH work. It adds about two minutes to activation and causes switch-to-configuration to return 4, obscuring otherwise successful portal checks.

## Acceptance Criteria

- [ ] Reproduce the failure on the running R36T Max and identify the interface/readiness condition it awaits.
- [ ] Correct only that readiness policy, preserving USB recovery and NetworkManager ownership.
- [ ] Verify cold boot and test activation without the wait-online failure while both Wi-Fi and optional USB behavior remain correct.

## Related

- `nix/devices/r36tmax/usb-gadget.nix`
- `nix/devices/r36tmax/wifi/default.nix`
- `nix/devices/r36tmax/default.nix`
