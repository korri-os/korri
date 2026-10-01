---
id: 01M2VNY0XM2WWASX9GBZ9MYGNS
slug: bind-rp-mini-v2-auxiliary-controls-to-portal-actions
title: Bind RP Mini V2 auxiliary controls to portal actions
origin: parked
status: To Do
priority: medium
labels:
  - rpminiv2
  - input
  - portal
created: 2026-09-19
source: se-code-review
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktrees/feat/rpminiv2-korri
  branch: feat/rpminiv2-korri
  commit: 905d648f
  repo: korri
  invoked_by: user
---

# Bind RP Mini V2 auxiliary controls to portal actions

## Why it matters

The first local portal milestone intentionally leaves Select, Guide, and the vendor BTN_BACK control unbound. Choosing their semantic portal actions requires hardware confirmation and must not add an F1 keyboard target that Chromium can consume directly.

## Acceptance Criteria

- [ ] Select, Guide, and BTN_BACK each have an explicit semantic portal action or an explicitly approved no-op.
- [ ] InputPlumber exposes no unowned keyboard shortcut and no duplicate gamepad.
- [ ] Hardware acceptance confirms each auxiliary control on the Retroid Pocket Mini V2.

## Related

- `nix/devices/rpminiv2/inputplumber-check.py`
- `nix/devices/rpminiv2/inputplumber/capability_maps/retroid_pocket_mini_v2.yaml`
- `nix/devices/rpminiv2/README.md`
- `clients/portal/src/input/gamepad-adapter.ts`
