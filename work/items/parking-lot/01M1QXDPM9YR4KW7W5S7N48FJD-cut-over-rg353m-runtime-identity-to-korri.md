---
id: 01M1QXDPM9YR4KW7W5S7N48FJD
slug: cut-over-rg353m-runtime-identity-to-korri
title: Cut over RG353M runtime identity to korri
origin: parked
status: To Do
priority: high
labels:
  - rg353m
  - migration
  - runtime-identity
created: 2026-09-05
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktrees/refactor/runtime-identity
  branch: refactor/runtime-identity
  commit: 4f00ff4f
  repo: korri
  invoked_by: user
---

# Cut over RG353M runtime identity to korri

## Why it matters

The public Nix and environment contract now uses runtime terminology, but the deployed RG353M must keep gameplay:games UID/GID 1001 until its Sunshine state, file ownership, old user services, and rollback generation can be migrated together. A direct cut would make the current device gate inspect or restore the wrong user manager and could lose pairing state.

## Acceptance Criteria

- [ ] A read-only device inspection records the real gameplay passwd/group entries, /home/gameplay Sunshine tree modes, and all persistent files owned by UID or GID 1001.
- [ ] The rollout gate or a bounded one-off procedure addresses the rollback and candidate identities separately and restores both identity and ownership on failure.
- [ ] The candidate uses korri:korri UID/GID 1000 with /home/korri and preserves the exact Sunshine private-state digest.
- [ ] The RG353M survives candidate reboot verification before the old gameplay account and home are removed.

## Related

- `nix/rg353m/sunshine-host.nix`
- `services/inputd/deploy/device-check.sh`
- `services/inputd/deploy/README.md`

## Notes

The hostname rg353m did not resolve and no matching Tailscale device was visible during the contract-rename work. Do not invent migration commands before inspecting the real device.
