---
id: 01M275F3FMG5CW9BZDX4W9YSY4
slug: diagnose-rg353m-recovery-virtual-console-setup-failure
title: Diagnose RG353M recovery virtual-console setup failure
origin: parked
status: To Do
priority: medium
labels:
  - rg353m
  - recovery
  - boot-validation
created: 2026-09-11
source: user
---

# Diagnose RG353M recovery virtual-console setup failure

## Why it matters

The phase-1 firmware candidate boots and its kiosk and SSH services run, but systemd-vconsole-setup fails during keymap/font setup. The failure leaves physical console readiness unverified. Establish whether it also occurs with full firmware before attributing it to the trim or changing configuration.

## Acceptance Criteria

- [ ] Reproduce and compare console initialization on the phase-1 candidate and the verified recovery baseline.
- [ ] Identify the loadkeys/KD_FONT_OP_GET I/O failure and verify a justified fix or documented expected behavior.
- [ ] Verify physical recovery console access without weakening SSH authentication or writing internal storage.

## Related

- `nix/devices/rg353m/sd-image.nix`
- `nix/devices/rg353m/FIRMWARE.md`

## Notes

Candidate commit 09a32d29; system n8rlmz611dw9l4x7al7g26929mgl9fr4-nixos-system-haku-sd-card-26.05.20251221.a653104. Boot logs show kbd-2.9.0 loadkeys exit 1 and KD_FONT_OP_GET Input/output error. No fix attempted during the authorized reboot/verification operation.
