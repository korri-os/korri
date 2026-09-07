---
id: 01M1WVVJRP40CE2W8QP3GHFX42
slug: close-rg353m-raw-joydev-access-and-prove-gameplay-input-isol
title: Close RG353M raw joydev access and prove gameplay input isolation
origin: parked
status: To Do
priority: high
labels:
  - rg353m
  - input
  - security
created: 2026-09-07
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktree/rg353m-gamepad-api
  branch: feat/rg353m-gamepad-api
---

# Close RG353M raw joydev access and prove gameplay input isolation

## Why it matters

Device checks found a pre-existing access gap: gameplay cannot read raw event0/event5/js0, but the unclaimed ADC joystick's /dev/input/js1 is world-readable. Chromium enumerates that nonstandard device. The new portal adapter ignores it, but filtering is not process-level isolation, and focus gating does not revoke already-open device descriptors.

## Acceptance Criteria

- [ ] Apply a reviewed native access policy that denies Chromium the raw ADC joydev source while retaining the normalized controller.
- [ ] Verify real game focus transitions stop portal navigation, returning focus requires neutral, and the game receives controller input without duplicates.
- [ ] Keep Chromium sandboxing and avoid granting gameplay membership in input.

## Related

- `services/inputd/nix/korri-input.nix`
- `services/inputd/src/virtual_target_acl.rs`
- `clients/portal/src/input/gamepad-adapter.ts`
