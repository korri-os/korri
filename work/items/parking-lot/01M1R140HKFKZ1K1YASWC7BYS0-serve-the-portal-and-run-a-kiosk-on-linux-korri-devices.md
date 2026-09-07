---
id: 01M1R140HKFKZ1K1YASWC7BYS0
slug: serve-the-portal-and-run-a-kiosk-on-linux-korri-devices
title: Serve the portal and run a kiosk on Linux Korri devices
origin: parked
status: To Do
priority: high
labels:
  - portal
  - nixos
  - rg353m
  - kiosk
created: 2026-09-05
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: main
  commit: 1c410d5b
  repo: korri
  invoked_by: se-work
---

# Serve the portal and run a kiosk on Linux Korri devices

## Why it matters

Pico was demonstrated on the RG353M with a reverse SSH tunnel to a static server on a laptop plus a transient systemd-run chromium unit. Nothing about that survives a reboot, and there is no NixOS module that serves the portal or runs a kiosk on a Linux Korri device — the RG353M currently runs the Sway compositor, korrid, inputd and Sunshine with no UI client at all. Every surface improvement lands somewhere only a developer with an SSH session can see, which makes the whole surface layer unshippable on Linux regardless of how good it looks. A second gap sits behind it: the portal only uses the real korrid client when window.KorriNative is present (the Android shell), so even a running kiosk would render in-memory fixtures rather than the device's own library.

## Acceptance Criteria

- [ ] A NixOS module serves the built portal on the device and runs it in a kiosk on the Korri compositor, started by systemd and surviving reboot
- [ ] The kiosk reaches the device's own korrid so the catalog is the device's real library, not in-memory fixtures
- [ ] Which surface the kiosk mounts is configurable without rebuilding the portal bundle
- [ ] Booting a flashed RG353M shows the surface with no SSH session, tunnel, or manual step

## Related

- `clients/portal/src/main.tsx`
- `surfaces/pico/README.md`
- `docs/acceptance/pico-surface-bring-up-2026-09-04.md`
- `nix/tasks.nix`
