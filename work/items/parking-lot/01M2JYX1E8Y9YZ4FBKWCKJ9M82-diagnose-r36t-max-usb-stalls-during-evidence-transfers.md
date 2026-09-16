---
id: 01M2JYX1E8Y9YZ4FBKWCKJ9M82
slug: diagnose-r36t-max-usb-stalls-during-evidence-transfers
title: Diagnose R36T Max USB stalls during evidence transfers
origin: parked
status: To Do
priority: high
labels:
  - r36tmax
  - usb
  - reliability
created: 2026-09-15
source: se-work
---

# Diagnose R36T Max USB stalls during evidence transfers

## Why it matters

Unthrottled evidence transfers twice coincided with loss of USB SSH and serial access. This blocks remote diagnosis and can force a physical recovery. The decoder was not responsible for the first failed launcher: that service exited before GStreamer started. The USB cause remains unknown.

## Acceptance Criteria

- [ ] Identify whether the fault originates in the host USB path, cable or hub, gadget driver, or another device component using retained logs and a supervised reproduction.
- [ ] Transfer an agreed bounded evidence file without losing SSH or serial access, with a physical recovery window available.
- [ ] Retain before-and-after USB and kernel evidence, exact connection topology, transfer rate and boot identity.
- [ ] Do not attribute the fault to Hantro or RK915 without evidence from the failing code path.

## Related

- `nix/devices/r36tmax/CODEC-PATH.md`
- `nix/devices/r36tmax/usb-gadget.nix`
- `nix/devices/r36tmax/HARDWARE-SURVEY-2026-09-14.md`

## Notes

Observed on 2026-09-15. USB SSH first answered uname -r after recovery, then scp -r of the diagnostic directory stalled. The host serial write of a carriage return returned EAGAIN. One partial transfer contained 783360 bytes of the GStreamer registry plus small text files. After the owner reconnected USB, selective transfers at 256 kbit/s succeeded, including a 921600-byte hardware-decoded file. Reconnection and throttling changed together, so neither is a proven fix. The successful Hantro run had eight request submissions, eight decoded frames matching a host reference, and an unchanged kernel log.
