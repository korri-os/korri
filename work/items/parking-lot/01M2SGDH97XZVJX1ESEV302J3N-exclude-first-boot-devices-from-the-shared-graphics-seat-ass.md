---
id: 01M2SGDH97XZVJX1ESEV302J3N
slug: exclude-first-boot-devices-from-the-shared-graphics-seat-ass
title: Exclude first-boot devices from the shared graphics-seat assertion
origin: parked
status: To Do
priority: medium
labels:
  - nix
  - device
  - testing
created: 2026-09-18
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: perf/rpminiv2-tty-kernel
  commit: 49117800
  repo: korri
---

# Exclude first-boot devices from the shared graphics-seat assertion

## Why it matters

`nix run .#nixos-layout-check` fails on current main because the shared base check assumes every non-R36-recovery and non-Mini-V2 device has graphics, seatd, and polkit. The new RG35XXSP first-boot target does not satisfy that product-session contract, so the whole-layout gate is red before this kernel trim.

## Acceptance Criteria

- [ ] `nix run .#nixos-layout-check` passes on main.
- [ ] The graphics-seat assertion names product-session devices or excludes every documented first-boot/recovery target.
- [ ] A check prevents a new first-boot device from silently entering the product-session set.

## Related

- `nix/base/module-check.nix`
- `nix/devices/rg35xxsp/`
