---
id: 01M2VKMVWMRTB04MFC195E7MZ5
slug: make-rp-mini-v2-oled-idle-tracking-observe-gamepad-activity
title: Make RP Mini V2 OLED idle tracking observe gamepad activity
origin: parked
status: To Do
priority: medium
labels:
  - rpminiv2
  - input
  - display
  - follow-up
created: 2026-09-19
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktrees/feat/rpminiv2-korri
  branch: feat/rpminiv2-korri
  repo: korri
  invoked_by: user
---

# Make RP Mini V2 OLED idle tracking observe gamepad activity

## Why it matters

Swayidle reliably sees compositor input such as power or volume keys, but wlroots may ignore joystick-only activity. Without an explicit bridge, the OLED could blank after five minutes while the user is actively navigating the Korri portal with the built-in controller.

## Acceptance Criteria

- [ ] Built-in Retroid controller activity resets the graphical idle timer without creating duplicate portal actions.
- [ ] Any mapped controller input wakes DSI-1 after idle power-off.
- [ ] The five-minute no-input OLED protection and USB serial recovery behavior remain intact.
- [ ] Hardware acceptance records continuous controller use beyond five minutes without an unexpected blank.

## Related

- `nix/devices/rpminiv2/portal.nix`
- `nix/devices/rpminiv2/README.md`

## Notes

Defer until physical acceptance confirms how Sway/wlroots accounts for the InputPlumber virtual gamepad.
