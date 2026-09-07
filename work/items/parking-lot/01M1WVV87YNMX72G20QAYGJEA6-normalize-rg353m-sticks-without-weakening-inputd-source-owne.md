---
id: 01M1WVV87YNMX72G20QAYGJEA6
slug: normalize-rg353m-sticks-without-weakening-inputd-source-owne
title: Normalize RG353M sticks without weakening inputd source ownership
origin: parked
status: To Do
priority: medium
labels:
  - rg353m
  - input
created: 2026-09-07
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktree/rg353m-gamepad-api
  branch: feat/rg353m-gamepad-api
---

# Normalize RG353M sticks without weakening inputd source ownership

## Why it matters

The RG353M GPIO buttons and ADC sticks are separate evdev sources. The buttons-only Gamepad API slice preserves inputd's current single-source authority rule; adding the ADC source today would make that composite unsuitable for inputd. Physical stick navigation remains unavailable.

## Acceptance Criteria

- [ ] Ground composite ownership in the real RG353M GPIO and ADC devices, then verify inputd reaches Ready with one normalized Xbox target.
- [ ] Verify both physical sticks and D-pad in the browser Gamepad API and in gameplay without duplicate input or raw-device access grants.

## Related

- `nix/rg353m/inputplumber/devices/01-rg353m.yaml`
- `services/inputd/src/dbus.rs`
- `clients/portal/src/input/gamepad-adapter.ts`
