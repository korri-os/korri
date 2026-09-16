---
id: 01M2K02VTRSZ720YTFFDE3K9CY
slug: preserve-the-initial-nixos-system-profile-during-sd-first-bo
title: Preserve the initial NixOS system profile during SD first boot
origin: parked
status: To Do
priority: medium
labels:
  - nixos
  - first-boot
  - rollback
created: 2026-09-15
source: se-code-review
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktree/retroid-mini-v2
  branch: feat/retroid-mini-v2
  commit: d446491a
  repo: korri-os/korri
---

# Preserve the initial NixOS system profile during SD first boot

## Why it matters

The shared SD expansion hook removes /nix-path-registration before the upstream NixOS hook can create /nix/var/nix/profiles/system. The actual generated local-cmds for the Mini V2 candidate contains this ordering. It affects existing SD consumers too, and can omit the shipped generation from normal rollback history. Fixing shared first-boot behavior needs its own tests across the existing devices; it is outside the Mini V2 preparation slice.

## Acceptance Criteria

- [ ] A real first-boot initialization test leaves the shipped system registered in /nix/var/nix/profiles/system.
- [ ] A subsequent generation update retains the initial generation as a rollback option.
- [ ] The registration marker is removed only after store registration and profile initialization succeed.
- [ ] Existing MBR/GPT root expansion and download-only device policy checks still pass.

## Related

- `nix/formats/expand-root.nix`
- `nix/formats/sd-card.nix`
- `nix/formats/sd-card-check.nix`
- `nix/devices/rpminiv2/README.md`

## Notes

Source-verified, not hardware reproduced. The generated hook at /nix/store/fwcciw3iqc8w2zg6fxmk4zm5kcqhk1j5-local-cmds loads/removes the marker, then the upstream conditional block that would set the system profile sees no marker. Do not silently patch one device with a parallel first-boot initializer.
