---
id: 01M1PEY8TJSCS5TQE92BJ9H9WW
slug: boot-verify-a-clean-rg353m-flash-from-main
title: Boot-verify a clean RG353M flash from main
origin: parked
status: To Do
priority: high
labels:
  - rg353m
  - verification
  - first-boot
created: 2026-09-04
source: se-work
---

# Boot-verify a clean RG353M flash from main

## Why it matters

The working card was built up by deploying four generations over SSH, so nobody has ever flashed main and booted it end to end. The untested path is exactly the one that was fixed blind: the corrected first-boot expand script, which reads the partition number from sysfs. If that is wrong, a fresh flash silently leaves the root filesystem at image size and the Nix database empty, which is how the bug presented the first time. Every future device provisioning depends on this path working.

## Acceptance Criteria

- [ ] A card is flashed from a freshly built main image
- [ ] First boot expands the root partition to fill the card without manual intervention
- [ ] nix-store -q --references /run/current-system succeeds, proving the Nix database registered
- [ ] Display, SSH over WiFi, SSH over the USB cable, and the serial console all work on that first boot
- [ ] Boot time is verified with systemd-analyze and stays near 18 s

## Related

- `nix/rg353m/expand-root.nix`
- `nix/rg353m/sd-image.nix`

## Notes

Roughly 15 minutes of work. Needs the physical card out of the device and into the reader, so it cannot be done autonomously.
