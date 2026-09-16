---
id: 01M2GJV1N2FBKP3D0SFQPMY5N5
slug: remove-retired-brain-json-kiosk-consumers-from-remaining-lin
title: Remove retired brain.json kiosk consumers from remaining Linux devices
origin: parked
status: To Do
priority: high
labels:
  - linux
  - portal
created: 2026-09-14
source: se-debug
---

# Remove retired brain.json kiosk consumers from remaining Linux devices

## Why it matters

The R36T Max live boot proved that the private kiosk expects brain.json while current korrid no longer produces it. Other consumers of that integration can repeat the same browser restart loop. Keep the R36T Max fix focused, then remove the incompatible consumers without restoring retired producer behavior.

## Acceptance Criteria

- [ ] Enumerate actual remaining consumers of services/kiosk/nixos-module.nix.
- [ ] Move affected devices to the current credential-backed portal contract or explicitly retire the unused integration.
- [ ] Run a real producer/consumer startup test and authenticated browser RPC check for each affected configuration.

## Related

- `services/kiosk/nixos-module.nix`
- `services/kiosk/src/runtime.rs`
- `services/korrid/src/main.rs`
- `clients/portal/nix/nixos-module.nix`
- `nix/devices/r36tmax/default.nix`
